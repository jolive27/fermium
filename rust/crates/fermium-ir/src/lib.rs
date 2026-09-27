//! fermium-ir: the typed intermediate representation, the boundary between the checker and the back ends
//! (spec §B4). A port of `fermium/ir.py` and `fermium/types.py` from Fermium 1.5.
//!
//! The checker turns the AST into this IR. Every expression has a type, plus display-only facts: significant
//! figures (`sf`, None = exact), the unit the user wrote (`hint`, used when printing) and whether the value came
//! straight from a literal (`direct`). Units are gone here: every number is in SI base units, so a back end never
//! sees a unit. Variables, functions and lambdas are indices into the module's tables, which suits both the
//! evaluator and the LLVM back end.
pub mod dim;
pub mod pyfrac;
pub mod types;

pub use dim::{Dim, DIMLESS};
pub use types::{DExpr, DimVar, Ty, Unifier};

/// A unit remembered for display: its name, SI factor and offset (°C), and dimension.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub name: String,
    pub factor: f64,
    pub offset: f64,
    pub dim: Dim,
}

pub type SymId = usize;
pub type FuncId = usize;
pub type LambdaId = usize;

/// Where a variable lives: a function's local, module-level, or a REPL arena slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    Local,
    Global,
    Arena,
}

#[derive(Clone, Debug)]
pub struct Sym {
    pub name: String,
    pub ty: Ty,
    pub storage: Storage,
    pub func: Option<FuncId>,
    pub sf: Option<u32>,
    pub hint: Option<Hint>,
    pub direct: u8,
    pub slot: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub sf: Option<u32>,
    pub hint: Option<Hint>,
    /// 0: computed; 1..: came from a literal (the Python checker's `direct` codes, D11/D242).
    pub direct: u8,
    pub line: u32,
    /// Checker-only facts the Python checker sets as attributes on IR nodes (display and warnings).
    pub x: Option<Box<ExprExtra>>,
}

/// Rarely-set facts about an expression (Python: attributes set on IR nodes by the checker).
#[derive(Clone, Debug, Default)]
pub struct ExprExtra {
    /// a loop variable over a written list prints with the list's fewest figures (D242)
    pub list_sf: Option<u32>,
    /// `10 °C` written out: (value as written, the unit), and where (line, col) — for warnings (#9)
    pub abs_literal: Option<(f64, Hint)>,
    pub abs_at: Option<(u32, u32)>,
    /// false: don't echo the unit as written (natural units, D60)
    pub no_echo: bool,
    /// holds a temperature difference (D181)
    pub tdelta: bool,
    /// text-table id of a string literal
    pub text_id: Option<usize>,
    /// the display units of a vector whose components have different units, one each (Python MixedHint, D29)
    pub mixed: Option<Vec<Option<Hint>>>,
    /// a variable holding a fitted parameter or its standard error (or set from one): its figures don't come from
    /// a written value, so a printed sum with it keeps v1's figures rule, not the decimal-place rule (B-F1)
    pub fit_sf: bool,
}

/// Does e use a variable whose figures come from a fit (ExprExtra::fit_sf)? Lambdas are not entered.
pub fn uses_fit_sf(e: &Expr) -> bool {
    e.get_extra().is_some_and(|x| x.fit_sf) || expr_children(e).into_iter().any(uses_fit_sf)
}

impl Expr {
    pub fn extra(&mut self) -> &mut ExprExtra {
        self.x.get_or_insert_with(Default::default)
    }
    pub fn get_extra(&self) -> Option<&ExprExtra> {
        self.x.as_deref()
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Const(f64),
    Bool(bool),
    Str(String),
    Var(SymId),
    /// + - * / on numbers, or element by element on lists (broadcasting a scalar).
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// a ** p with p a compile-time constant.
    PowC(Box<Expr>, f64),
    Pow(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    /// `a ≈ b` (D260): rtol, atol (absolute tolerance or None).
    Approx { a: Box<Expr>, b: Box<Expr>, rtol: f64, atol: Option<Box<Expr>> },
    Logic { and: bool, a: Box<Expr>, b: Box<Expr> },
    Not(Box<Expr>),
    Call(FuncId, Vec<Expr>),
    /// A scalar function applied element by element over list arguments (D191).
    Map { func: FuncId, args: Vec<Expr>, list_pos: Vec<usize> },
    Builtin(String, Vec<Expr>),
    List(Vec<Expr>),
    Vec(Vec<Expr>),
    VecElem(Box<Expr>, usize),
    /// A copy of a vector or matrix with one entry replaced (D195).
    VecSet { v: Box<Expr>, idxs: Vec<(Expr, usize, usize)>, value: Box<Expr> },
    /// Entries picked by run-time indexes: idxs (index, size, stride), offsets of the result.
    VecIndex { v: Box<Expr>, idxs: Vec<(Expr, usize, usize)>, offs: Vec<usize> },
    Index(Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `value where name = e, …`.
    Let(Vec<(SymId, Expr)>, Box<Expr>),
    /// xname: text id of the variable's name and xfmt: print format of a value of it, for the integrand's NaN
    /// message (D45); soft: the quiet first try of a vector component, atol: its second try (D44).
    Integral { lam: LambdaId, lo: Box<Expr>, hi: Box<Expr>, xname: Option<usize>, xfmt: Option<usize>, soft: bool,
               atol: Option<Box<Expr>> },
    /// Σ(body for k from lo to hi step st) (D51).
    Sum { lam: LambdaId, lo: Box<Expr>, hi: Box<Expr>, step: Option<Box<Expr>> },
    /// The x in [lo, hi] where lam(x) = 0 (D32).
    /// scale: |lhs| + |rhs| for the rounding-noise warning; tfmt: print format of the unknown (for messages).
    Root { lam: LambdaId, lo: Box<Expr>, hi: Box<Expr>, scale: Option<LambdaId>, tfmt: Option<usize> },
    /// A solution component (or its derivative) at time t.
    SolEval { sol: SymId, comp: usize, t: Box<Expr>, use_dy: bool, tfmt: usize },
    /// All samples of a solution component (what = 0) or its times (what = 1) as a list.
    SolList { sol: SymId, comp: usize, what: u8 },
    Load(usize),
    Table(Vec<Expr>),
    Column(Box<Expr>, usize),
    /// u(x, t) of a PDE solution (D83).
    /// (xa, xb are unused: the ends of the grid are read from the solution; xfmt, tfmt: formats for errors)
    PdeEval { sol: SymId, xa: f64, xb: f64, m: usize, comp0: usize, x: Box<Expr>, t: Box<Expr>, which: u8,
              xfmt: usize, tfmt: usize },
    /// The highest derivatives of a coupled ODE (D47): the n×n system m·x = b solved by Gaussian elimination;
    /// a singular matrix is an ODE error "<text><t>: the matrix of their coefficients is singular".
    OdeLinSolve { m: Vec<Expr>, b: Vec<Expr>, t: Box<Expr>, text: usize, fmt: usize },
    /// ± and the parts of an uncertain value (D120–D124).
    Uncertain(Box<Expr>, Box<Expr>),
    /// sample(expr, n): a list of n values of the lambda `expr`, evaluated afresh each time (its parameter is the
    /// 1-based sample number; D80).
    Sample { lam: LambdaId, n: Box<Expr> },
}

/// One item of a print statement.
#[derive(Clone, Debug)]
pub enum PrintItem {
    Num(Expr, usize),
    List(Expr, usize),
    Complex(Expr, usize),
    Vec(Expr, usize),
    /// A vector whose components have different units: one format per component.
    MixedVec(Expr, Vec<usize>),
    Mat(Expr, usize),
    ComplexList(Expr, usize),
    /// A list of vectors or matrices (the element format).
    VList(Expr, usize),
    TextList(Expr),
    Bool(Expr),
    /// A constant text (index into the text table).
    Text(usize),
    TextVar(Expr),
    Data(Expr, usize),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Assign(SymId, Expr),
    IndexAssign(SymId, Expr, Expr),
    Push(SymId, Expr),
    Clear(SymId),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    /// for sym from lo to hi step st (inclusive, lo + i·st); parallel: blocks as in par_blocks (D152).
    For { sym: SymId, lo: Expr, hi: Expr, step: Option<Expr>, body: Vec<Stmt>, parallel: bool,
          par: Option<Box<ParInfo>> },
    ForIn(SymId, Expr, Vec<Stmt>),
    Print(Vec<PrintItem>),
    Plot(usize, Vec<Expr>),
    Solve { sol: SymId, rhs: LambdaId, y0: Vec<Expr>, t0: Expr, t1: Expr, step: Option<Expr>, method: String,
            rtol: Option<Expr>, x: Box<SolveExtra> },
    /// errs: the hidden variables that receive the standard errors, for err(x)
    Fit { fit_id: usize, data: Expr, params: Vec<SymId>, guesses: Vec<Expr>, model: LambdaId, errs: Vec<SymId> },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
    Assert(Expr, usize),
    Animate { anim_id: usize, sol: SymId, xa: f64, xb: f64 },
    Propagate { n: Option<Expr>, body: Vec<Stmt>, outs: Vec<SymId> },
}

/// The rest of a solve statement (Python sets them as attributes on SSolve).
#[derive(Clone, Debug, Default)]
pub struct SolveExtra {
    /// the relative tolerance (1e-9 unless given)
    pub rtol: f64,
    /// absolute tolerance per state slot: (value in SI, power of 1/|t1 − t0|) (D160)
    pub atol: Option<Vec<(f64, u32)>>,
    /// the stop condition's g = lhs − rhs (D39), and the text id of its "never happened" message (−1: none)
    pub event: Option<LambdaId>,
    pub evtext: i64,
    /// text id of the independent variable's name, and the print format of its values (for errors)
    pub tname: usize,
    pub tfmt: usize,
    /// a PDE's space variable name (text id), for the grid check's warning
    pub xname: usize,
    /// the right side reads t itself (D40)
    pub tdep: bool,
    /// eigenvalue problems (method "eigen", D82): states, grid, 0 = matrix / 1 = shooting
    pub nstates: usize,
    pub grid: usize,
    pub eig_method: u8,
    /// PDEs (method "pde", D83): the x range, order in t, 0 CN / 1 implicit / 2 explicit, boundary kinds
    pub xa: Option<Expr>,
    pub xb: Option<Expr>,
    pub order: u8,
    pub pmethod: u8,
    pub bc: (u8, u8),
    pub is_complex: bool,
    /// the source line (PDE warnings)
    pub line: u32,
    /// some unknowns are lists (spec C1, D282): their sizes come from the initial values when the solve runs
    /// (the list unknowns' slots are last in the layout; `atol` then has one value per list slot)
    pub lists: bool,
}

/// What the back ends need to run a parallel for (D152): the sums (added up block by block, in order), the lists
/// written as xs[i], the other lists read, and the pairs checked at run time not to be the same list (with the
/// text id naming them, for ERR_PAR_ALIAS).
#[derive(Clone, Debug, Default)]
pub struct ParInfo {
    pub reductions: Vec<SymId>,
    pub written: Vec<SymId>,
    pub lists: Vec<SymId>,
    pub alias: Vec<(SymId, SymId, usize)>,
}

/// A monomorphic instance of a user function.
#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub params: Vec<SymId>,
    pub ret_ty: Ty,
    pub body: Vec<Stmt>,
    pub locals: Vec<SymId>,
    pub sf: Option<u32>,
    /// the function's name as the user wrote it, and the line of its definition (for run-time errors: runaway
    /// recursion is reported there, as the compiled path's stack check does)
    pub display: String,
    pub def_line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LambdaKind {
    /// f(x): integrands, sums, roots, plot samplers.
    Scalar,
    /// f(t, y) → dy for an ODE.
    Ode,
    /// f(params, columns) → residuals for a fit.
    Model,
}

/// A nested function; `captures` are locals of the enclosing function passed through its environment.
#[derive(Clone, Debug)]
pub struct Lambda {
    pub kind: LambdaKind,
    pub name: String,
    pub params: Vec<SymId>,
    pub captures: Vec<SymId>,
    /// variables made inside it (a where-binding in an integrand, …)
    pub locals: Vec<SymId>,
    pub body: Vec<Expr>,
    pub state: Vec<SymId>,
    pub col_syms: Vec<SymId>,
    pub param_syms: Vec<SymId>,
}

/// How a printed number is shown (the Python checker's `tables.fmts` entries).
#[derive(Clone, Debug)]
pub struct Fmt {
    pub dim: Dim,
    pub hint: Option<Hint>,
    pub sf: Option<u32>,
    pub direct: u8,
    pub echo: bool,
    /// The natural-unit system the value was computed in, if not SI (D60).
    pub nat: Option<String>,
}

/// Runtime tables shared by the back ends (printing, plots, data, fits).
#[derive(Clone, Debug, Default)]
pub struct Tables {
    pub fmts: Vec<Fmt>,
    pub texts: Vec<String>,
    pub plots: Vec<serde_like::Json>,
    pub loads: Vec<serde_like::Json>,
    pub fits: Vec<serde_like::Json>,
    /// calls into Python (`use python`, D140): one entry per call site, indexed by the first argument of the
    /// `pycall` built-in
    pub pycalls: Vec<PyCallSite>,
    /// the program's folder, put on Python's sys.path when a module is imported
    pub py_base_dir: String,
}

/// A call site of a Python function (v1's `tables.pycalls` entry): module and function names, the unit factor
/// (value passed = SI / factor) and int flag of each argument, and the result's shape and unit factor.
#[derive(Clone, Debug, Default)]
pub struct PyCallSite {
    pub module: String,
    pub func: String,
    pub display: String,
    pub facs: Vec<f64>,
    pub ints: Vec<bool>,
    pub pnames: Vec<String>,
    pub rlist: bool,
    pub rfac: f64,
    /// the result's shape was declared in the use line
    pub declared: bool,
}

/// A checked program: main, the function instances, lambdas, symbols and tables.
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub main: Vec<Stmt>,
    pub funcs: Vec<Func>,
    pub lambdas: Vec<Lambda>,
    pub syms: Vec<Sym>,
    pub tables: Tables,
    pub uses_uncertainty: bool,
}

/// The blocks [start, end) of a parallel for with n iterations: the same for any number of threads, so sums
/// are added in the same order on every machine (D152).
pub fn par_blocks(n: usize) -> Vec<(usize, usize)> {
    const PAR_BLOCKS: usize = 256;
    let nb = n.min(PAR_BLOCKS);
    if nb == 0 {
        return vec![];
    }
    let (q, r) = (n / nb, n % nb);
    let start: Vec<usize> = (0..=nb).map(|k| k * q + k.min(r)).collect();
    (0..nb).map(|k| (start[k], start[k + 1])).collect()
}

/// A small JSON-like value for the runtime tables (plots, loads, fits) without a serde dependency.
pub mod serde_like {
    #[derive(Clone, Debug, PartialEq)]
    pub enum Json {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        List(Vec<Json>),
        Obj(Vec<(String, Json)>),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parallel_blocks_match_python() {
        // ir.par_blocks(10) == [(0,1),(1,2),…]; par_blocks(1000) has 256 blocks of 3 or 4
        assert_eq!(super::par_blocks(3), vec![(0, 1), (1, 2), (2, 3)]);
        let b = super::par_blocks(1000);
        assert_eq!(b.len(), 256);
        assert_eq!(b[0], (0, 4));
        assert_eq!(b.last().unwrap().1, 1000);
    }
}

/// The direct sub-expressions of an IR expression (lambda bodies are reached through `lambda_of`).
pub fn expr_children(e: &Expr) -> Vec<&Expr> {
    use ExprKind as K;
    match &e.kind {
        K::Const(_) | K::Bool(_) | K::Str(_) | K::Var(_) | K::Load(_) | K::SolList { .. } => vec![],
        K::Bin(_, a, b) | K::Pow(a, b) | K::Cmp(_, a, b) | K::Logic { a, b, .. } | K::Index(a, b)
        | K::Uncertain(a, b) => vec![a, b],
        K::PowC(a, _) | K::Neg(a) | K::Not(a) | K::VecElem(a, _) | K::Column(a, _) => vec![a],
        K::Approx { a, b, atol, .. } => {
            let mut v: Vec<&Expr> = vec![a, b];
            v.extend(atol.iter().map(|x| &**x));
            v
        }
        K::Call(_, args) | K::Map { args, .. } | K::Builtin(_, args) | K::List(args) | K::Vec(args) | K::Table(args) => {
            args.iter().collect()
        }
        K::VecSet { v, idxs, value } => {
            let mut out: Vec<&Expr> = vec![v];
            out.extend(idxs.iter().map(|(i, _, _)| i));
            out.push(value);
            out
        }
        K::VecIndex { v, idxs, .. } => {
            let mut out: Vec<&Expr> = vec![v];
            out.extend(idxs.iter().map(|(i, _, _)| i));
            out
        }
        K::If(c, a, b) => vec![c, a, b],
        K::Let(binds, v) => {
            let mut out: Vec<&Expr> = binds.iter().map(|(_, x)| x).collect();
            out.push(v);
            out
        }
        K::Integral { lo, hi, .. } | K::Root { lo, hi, .. } => vec![lo, hi],
        K::Sum { lo, hi, step, .. } => {
            let mut v: Vec<&Expr> = vec![lo, hi];
            v.extend(step.iter().map(|x| &**x));
            v
        }
        K::SolEval { t, .. } | K::Sample { n: t, .. } => vec![t],
        K::PdeEval { x, t, .. } => vec![x, t],
        K::OdeLinSolve { m, b, t, .. } => {
            let mut v: Vec<&Expr> = m.iter().chain(b.iter()).collect();
            v.push(t);
            v
        }
    }
}

/// The lambda an expression uses (integrand, summand, root function), if any.
pub fn lambda_of(e: &Expr) -> Option<LambdaId> {
    match &e.kind {
        ExprKind::Integral { lam, .. } | ExprKind::Sum { lam, .. } | ExprKind::Root { lam, .. }
        | ExprKind::Sample { lam, .. } => Some(*lam),
        _ => None,
    }
}

/// The expressions directly inside a statement, and its sub-blocks.
pub fn stmt_parts(s: &Stmt) -> (Vec<&Expr>, Vec<&Vec<Stmt>>) {
    use StmtKind as K;
    match &s.kind {
        K::Assign(_, e) | K::Push(_, e) | K::Expr(e) | K::Assert(e, _) => (vec![e], vec![]),
        K::IndexAssign(_, i, v) => (vec![i, v], vec![]),
        K::Clear(_) | K::Break | K::Continue | K::Animate { .. } => (vec![], vec![]),
        K::If(c, a, b) => (vec![c], vec![a, b]),
        K::While(c, b) => (vec![c], vec![b]),
        K::For { lo, hi, step, body, .. } => {
            let mut v: Vec<&Expr> = vec![lo, hi];
            v.extend(step.iter());
            (v, vec![body])
        }
        K::ForIn(_, l, b) => (vec![l], vec![b]),
        K::Print(items) => (items.iter().filter_map(print_item_expr).collect(), vec![]),
        K::Plot(_, es) => (es.iter().collect(), vec![]),
        K::Solve { y0, t0, t1, step, rtol, x, .. } => {
            let mut v: Vec<&Expr> = y0.iter().collect();
            v.push(t0);
            v.push(t1);
            v.extend(step.iter());
            v.extend(rtol.iter());
            v.extend(x.xa.iter().chain(x.xb.iter()));
            (v, vec![])
        }
        K::Fit { data, guesses, .. } => {
            let mut v: Vec<&Expr> = vec![data];
            v.extend(guesses.iter());
            (v, vec![])
        }
        K::Return(e) => (e.iter().collect(), vec![]),
        K::Propagate { n, body, .. } => (n.iter().collect(), vec![body]),
    }
}

pub fn print_item_expr(it: &PrintItem) -> Option<&Expr> {
    match it {
        PrintItem::Num(e, _) | PrintItem::List(e, _) | PrintItem::Complex(e, _) | PrintItem::Vec(e, _)
        | PrintItem::MixedVec(e, _) | PrintItem::Mat(e, _) | PrintItem::ComplexList(e, _) | PrintItem::VList(e, _)
        | PrintItem::TextList(e)
        | PrintItem::Bool(e) | PrintItem::TextVar(e) | PrintItem::Data(e, _) => Some(e),
        PrintItem::Text(_) => None,
    }
}
