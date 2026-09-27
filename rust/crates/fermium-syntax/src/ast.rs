//! The abstract syntax tree: a port of `fermium/ast.py` (Fermium 1.5, the oracle).
//!
//! Every node records where it came from (line, column, length) so errors can point at it. The names of the
//! variants and fields are those of the Python classes and fields, so the two can be read side by side (and
//! `sexpr.rs` prints both the same way). The Python parser attaches extra facts to nodes as attributes
//! (`times_unit`, `imag_literal`, the number's source spelling, ...); here they live in [`Attrs`].

use num_rational::Rational64;

/// Where a node came from: 1-based line and column (in code points), and its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub length: u32,
}

impl Default for Span {
    fn default() -> Self {
        Span { line: 0, col: 0, length: 1 }
    }
}

/// A reference to another node (the Python parser stores the node object itself): its id, and its class and
/// position when the reference was made (used if the node is no longer in the tree).
#[derive(Clone, Debug)]
pub struct NodeRef {
    pub id: u32,
    pub class: &'static str,
    pub span: Span,
}

/// A factor of a denominator `a / b c d` (D8): the node, the index of its first token, space before it.
#[derive(Clone, Debug)]
pub struct DivFactor {
    pub node: NodeRef,
    /// The node itself (a copy), for the solve-unknown check (FRICTION #9).
    pub expr: Box<Expr>,
    pub start: usize,
    pub ws: bool,
}

/// `div_info` of the Python parser: on a division whose denominator is a product of juxtaposed factors
/// (`start`, `end`, `factors`, `warned`, `op`), or on an integral whose upper limit is followed by a spaced `/`
/// (`tok`, `hi_text`, and later `divisor`, `div_text`) (D34, D205).
#[derive(Clone, Debug, Default)]
pub struct DivInfo {
    /// Product form: the '/' token (line, col), first and past-the-end token index of the denominator.
    pub op: Option<(u32, u32)>,
    /// The index of the '/' token (not printed).
    pub op_i: Option<usize>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub factors: Option<Vec<DivFactor>>,
    pub warned: Option<bool>,
    /// Integral form.
    pub tok: Option<(u32, u32)>,
    pub tok_i: Option<usize>,
    pub hi_text: Option<String>,
    pub divisor: Option<NodeRef>,
    pub div_text: Option<String>,
}

/// `sum_info` of an integral: a spaced + or - at the top level of its upper limit (D173, D205).
#[derive(Clone, Debug)]
pub struct SumInfo {
    pub tok: (u32, u32),
    pub limit: String,
    pub rest: String,
    pub head: String,
    pub op: String,
    pub head_node: Option<NodeRef>,
    pub rest_node: Option<NodeRef>,
}

/// Facts the parser records on a node beyond its fields (the Python parser's ad-hoc attributes). `None` means the
/// attribute was never set.
#[derive(Clone, Debug, Default)]
pub struct Attrs {
    /// The number as written (`2.50e19`), for messages; on a `Field`, the name as written (`np.pi`).
    pub raw: Option<String>,
    /// `4i`, `1i`: an imaginary literal (D90).
    pub imag_literal: Option<bool>,
    /// `(…) MeV`, `2 (…) MeV`: multiplied by one unit (D215, D238).
    pub times_unit: Option<bool>,
    /// `(73/24)` made by the fraction-coefficient rule (D236).
    pub coefficient: Option<bool>,
    /// Juxtaposition: was there whitespace before the right operand, and the index of its first token.
    pub juxt_ws: Option<bool>,
    pub juxt_i: Option<usize>,
    /// `A_d u` where u is undefined: the value on the left, for the hint (#55).
    pub unit_left: Option<NodeRef>,
    /// `∫ … to E / (2 P0)`: the integral this node divides (D205).
    pub limit_div_of: Option<NodeRef>,
    /// The exponent of a superscript (`x²` gives 2).
    pub extra_int: Option<i64>,
    pub div_info: Option<Box<DivInfo>>,
    /// `Some(None)`: the attribute is set to None.
    pub sum_info: Option<Option<Box<SumInfo>>>,
}

#[derive(Clone, Debug)]
pub struct UnitFactor {
    /// Raw spelling: `km`, `°C`, `μm`.
    pub name: String,
    pub exp: Rational64,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct UnitExpr {
    pub factors: Vec<UnitFactor>,
    /// Source text, for display hints.
    pub text: String,
    pub span: Span,
    pub juxt_join: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct Expr {
    /// Identity of the node (the Python object): copies of one node share it.
    pub id: u32,
    pub kind: ExprKind,
    pub span: Span,
    pub paren: bool,
    pub attrs: Attrs,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Num { value: f64, sigfigs: Option<u32>, digit: bool },
    /// A number with units: `3 m/s`, `2 [kg]`, `x [m]`.
    Quantity { value: Box<Expr>, unit: UnitExpr, bracket: bool },
    Str { value: String },
    Bool { value: bool },
    Name { name: String },
    /// `+ - * / ^` (and `×` as written); `implicit` for juxtaposition.
    BinOp { op: String, left: Box<Expr>, right: Box<Expr>, implicit: bool },
    Neg { operand: Box<Expr> },
    /// `== != < > <= >= ~=`, with `within tol` for ≈ (D260).
    Compare { op: String, left: Box<Expr>, right: Box<Expr>, tol: Option<Box<Expr>> },
    Logic { op: String, left: Box<Expr>, right: Box<Expr> },
    Not { operand: Box<Expr> },
    Call { func: Box<Expr>, args: Vec<Expr> },
    Index { target: Box<Expr>, index: Option<Box<Expr>> },
    /// `a:b` inside `xs[...]`, both included, 1-based (D114).
    Slice { lo: Option<Box<Expr>>, hi: Option<Box<Expr>> },
    /// `end` inside an index.
    End,
    Field { target: Box<Expr>, name: String },
    Prime { target: Box<Expr>, order: i64 },
    /// `d/dt operand`, `d²/dt² operand`, `∂/∂x operand`.
    Deriv { var: String, order: i64, operand: Box<Expr>, partial: bool },
    Integral { integrand: Box<Expr>, var: String, lo: Option<Box<Expr>>, hi: Option<Box<Expr>> },
    /// `Σ(body for var from lo to hi step st)` (D51).
    Sum { body: Box<Expr>, var: String, lo: Box<Expr>, hi: Box<Expr>, step: Option<Box<Expr>> },
    Sqrt { operand: Box<Expr>, root: i64 },
    Abs { operand: Box<Expr> },
    ListLit { items: Vec<Expr> },
    /// `table(x = xs, y = ys)` (D193).
    Table { names: Vec<String>, items: Vec<Expr> },
    /// `∇f` (grad), `∇·F` (div), `∇×F` (curl), `∇²f` (lap).
    VecCalc { kind: String, func: Box<Expr> },
    VecLit { items: Vec<Expr> },
    IfExpr { cond: Box<Expr>, then: Box<Expr>, other: Box<Expr> },
    /// `expr in unit`.
    Convert { value: Box<Expr>, unit: UnitExpr },
    /// `expr to N digits`.
    Digits { value: Box<Expr>, digits: i64 },
    Load { path: String },
    Where { value: Box<Expr>, bindings: Vec<(String, Expr)> },
    /// `a ± b` (D120).
    Uncertain { value: Box<Expr>, err: Box<Expr> },
}

impl ExprKind {
    /// The Python class name.
    pub fn class(&self) -> &'static str {
        match self {
            ExprKind::Num { .. } => "Num",
            ExprKind::Quantity { .. } => "Quantity",
            ExprKind::Str { .. } => "Str",
            ExprKind::Bool { .. } => "Bool",
            ExprKind::Name { .. } => "Name",
            ExprKind::BinOp { .. } => "BinOp",
            ExprKind::Neg { .. } => "Neg",
            ExprKind::Compare { .. } => "Compare",
            ExprKind::Logic { .. } => "Logic",
            ExprKind::Not { .. } => "Not",
            ExprKind::Call { .. } => "Call",
            ExprKind::Index { .. } => "Index",
            ExprKind::Slice { .. } => "Slice",
            ExprKind::End => "End",
            ExprKind::Field { .. } => "Field",
            ExprKind::Prime { .. } => "Prime",
            ExprKind::Deriv { .. } => "Deriv",
            ExprKind::Integral { .. } => "Integral",
            ExprKind::Sum { .. } => "Sum",
            ExprKind::Sqrt { .. } => "Sqrt",
            ExprKind::Abs { .. } => "Abs",
            ExprKind::ListLit { .. } => "ListLit",
            ExprKind::Table { .. } => "Table",
            ExprKind::VecCalc { .. } => "VecCalc",
            ExprKind::VecLit { .. } => "VecLit",
            ExprKind::IfExpr { .. } => "IfExpr",
            ExprKind::Convert { .. } => "Convert",
            ExprKind::Digits { .. } => "Digits",
            ExprKind::Load { .. } => "Load",
            ExprKind::Where { .. } => "Where",
            ExprKind::Uncertain { .. } => "Uncertain",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub unit: Option<UnitExpr>,
    /// `r: vector [m]`: the kind a version of a function takes (number, vector, list, complex; C5 dispatch)
    pub kind: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Equation {
    pub lhs: Expr,
    pub rhs: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct PlotSeries {
    pub y: Expr,
    pub x: Expr,
    pub lo: Option<Expr>,
    pub hi: Option<Expr>,
    pub span: Span,
}

/// A value in a plot's options.
#[derive(Clone, Debug)]
pub enum PlotOpt {
    Bool(bool),
    Str(String),
    Num(f64),
    Range(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum FuncBody {
    Expr(Expr),
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug)]
pub struct PySig {
    pub name: String,
    /// (name, unit, is_int)
    pub params: Vec<(String, Option<UnitExpr>, bool)>,
    pub ret_shape: Option<String>,
    pub ret_unit: Option<UnitExpr>,
    pub span: Span,
}

/// One parameter of a C or Fortran function (`import c`, D275).
#[derive(Clone, Debug)]
pub enum CParamKind {
    /// a double, in the unit if one is given: `m [kg]`
    Num(Option<UnitExpr>),
    /// a C `int` (Fortran `integer`): `n: int`
    Int,
    /// an array of doubles: `x: list [m]`
    List(Option<UnitExpr>),
    /// the length of a list parameter, passed as an int and filled in by Fermium: `n: len(x)`
    Len(String),
}

#[derive(Clone, Debug)]
pub struct CParam {
    pub name: String,
    pub kind: CParamKind,
    pub span: Span,
}

/// What a C or Fortran function returns.
#[derive(Clone, Debug)]
pub enum CRetDecl {
    /// a double in this unit: `-> [MeV]`
    Unit(UnitExpr),
    /// a plain double: `-> number`
    Number,
    /// a C `int`: `-> int`
    Int,
}

/// `kinetic_energy(m [kg], v [km/s]) -> [J]` in an `import c` block; `bind(C[, name="…"])` for Fortran.
#[derive(Clone, Debug)]
pub struct CSig {
    pub name: String,
    pub params: Vec<CParam>,
    pub ret: CRetDecl,
    /// None: no bind; Some(None): `bind(C)`; Some(Some(n)): `bind(C, name="n")`
    pub bind: Option<Option<String>>,
    /// C++ (C4, D290): the qualified name as written (`phys::Particle::rest_energy`); `name` is then the last
    /// component, or the `as` name after the result
    pub cpp_name: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `propagate montecarlo [N samples]` + a block (D123).
    Propagate { samples: Option<Expr>, body: Vec<Stmt> },
    /// `name = value`, `+=`, `-=`, `*=`, `/=`.
    Assign { name: String, value: Expr, op: String },
    /// `xs[i] = …`, `M[i, j] = …` (D195).
    IndexAssign { target: String, index: Expr, value: Expr, op: String, index2: Option<Expr> },
    FuncDef { name: String, params: Vec<Param>, body: FuncBody, where_: Vec<(String, Expr)> },
    Print { items: Vec<Expr> },
    Plot { series: Vec<PlotSeries>, out: Option<String>, options: Vec<(String, PlotOpt)> },
    Solve(Box<Solve>),
    Fit { model: Equation, data: Expr, guesses: Vec<(String, Expr)> },
    /// `analyze pendulum: T [s] depends on L [m], m [kg], g` (D70); raw: each name's spelling.
    Analyze { title: Option<String>, target: Param, inputs: Vec<Param>, raw: Vec<(String, String)> },
    If { cond: Expr, then: Vec<Stmt>, other: Option<Vec<Stmt>> },
    For { var: String, lo: Expr, hi: Expr, step: Option<Expr>, body: Vec<Stmt>, parallel: bool },
    ForIn { var: String, iterable: Expr, body: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Return { value: Option<Expr> },
    Break,
    Continue,
    ExprStmt { value: Expr },
    Assert { cond: Expr, message: Option<String> },
    /// `units natural(ħ = c = 1)` or `units nuclear:` + a block (D60).
    Units { system: String, consts: Vec<String>, body: Option<Vec<Stmt>> },
    /// `import mechanics`, `import "lib/x.fm" as x`, `from nuclear import a, b as c` (D100).
    Import { module: String, is_path: bool, alias: Option<String>, names: Option<Vec<(String, Option<String>)>> },
    /// `use python numpy as np` (D140).
    UsePython { module: String, alias: Option<String>, sigs: Vec<PySig> },
    /// `import c "libphys.so":` / `import fortran "libnuclear.so":` with signatures (C3, D275);
    /// `import cpp "libphys.so" header "phys.hpp":` (C4, D290: `lang` "cpp", `lib` may be empty, `header` set).
    ImportC { lang: String, lib: String, header: Option<String>, sigs: Vec<CSig> },
}

/// `solve …` (ODEs, eigenvalue problems, PDEs).
#[derive(Clone, Debug)]
pub struct Solve {
    pub equations: Vec<Equation>,
    pub initial: Vec<Equation>,
    pub var: String,
    pub lo: Expr,
    pub hi: Expr,
    pub step: Option<Expr>,
    pub method: Option<String>,
    pub tolerance: Option<Expr>,
    /// The stop condition `until lhs = rhs` (D39).
    pub until: Option<Equation>,
    /// `absolute a[, b …]` (D160).
    pub absolute: Option<Vec<Expr>>,
    /// `lowest N` / `grid N` of an eigenvalue problem or a PDE, and a PDE's second range (D82, D83).
    pub lowest: Option<Expr>,
    pub grid: Option<Expr>,
    pub var2: Option<String>,
    pub lo2: Option<Expr>,
    pub hi2: Option<Expr>,
    pub step2: Option<Expr>,
}

impl StmtKind {
    pub fn class(&self) -> &'static str {
        match self {
            StmtKind::Propagate { .. } => "Propagate",
            StmtKind::Assign { .. } => "Assign",
            StmtKind::IndexAssign { .. } => "IndexAssign",
            StmtKind::FuncDef { .. } => "FuncDef",
            StmtKind::Print { .. } => "Print",
            StmtKind::Plot { .. } => "Plot",
            StmtKind::Solve(_) => "Solve",
            StmtKind::Fit { .. } => "Fit",
            StmtKind::Analyze { .. } => "Analyze",
            StmtKind::If { .. } => "If",
            StmtKind::For { .. } => "For",
            StmtKind::ForIn { .. } => "ForIn",
            StmtKind::While { .. } => "While",
            StmtKind::Return { .. } => "Return",
            StmtKind::Break => "Break",
            StmtKind::Continue => "Continue",
            StmtKind::ExprStmt { .. } => "ExprStmt",
            StmtKind::Assert { .. } => "Assert",
            StmtKind::Units { .. } => "Units",
            StmtKind::Import { .. } => "Import",
            StmtKind::UsePython { .. } => "UsePython",
            StmtKind::ImportC { .. } => "ImportC",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub body: Vec<Stmt>,
}

impl Expr {
    pub fn class(&self) -> &'static str {
        self.kind.class()
    }

    pub fn noderef(&self) -> NodeRef {
        NodeRef { id: self.id, class: self.class(), span: self.span }
    }

    /// Direct child expressions (ast.children).
    pub fn children(&self) -> Vec<&Expr> {
        use ExprKind::*;
        match &self.kind {
            Num { .. } | Str { .. } | Bool { .. } | Name { .. } | End | Load { .. } => vec![],
            Quantity { value, .. } => vec![value],
            Compare { left, right, tol, .. } => {
                let mut v: Vec<&Expr> = vec![left, right];
                if let Some(t) = tol {
                    v.push(t);
                }
                v
            }
            BinOp { left, right, .. } | Logic { left, right, .. } => vec![left, right],
            Neg { operand } | Not { operand } | Sqrt { operand, .. } | Abs { operand } => vec![operand],
            Call { func, args } => {
                let mut v: Vec<&Expr> = vec![func];
                v.extend(args.iter());
                v
            }
            Index { target, index } => {
                let mut v: Vec<&Expr> = vec![target];
                if let Some(i) = index {
                    v.push(i);
                }
                v
            }
            Slice { lo, hi } => lo.iter().chain(hi.iter()).map(|b| &**b).collect(),
            Field { target, .. } | Prime { target, .. } => vec![target],
            Deriv { operand, .. } => vec![operand],
            Integral { integrand, lo, hi, .. } => {
                let mut v: Vec<&Expr> = vec![integrand];
                v.extend(lo.iter().map(|b| &**b));
                v.extend(hi.iter().map(|b| &**b));
                v
            }
            Sum { body, lo, hi, step, .. } => {
                let mut v: Vec<&Expr> = vec![body, lo, hi];
                v.extend(step.iter().map(|b| &**b));
                v
            }
            ListLit { items } | VecLit { items } | Table { items, .. } => items.iter().collect(),
            VecCalc { .. } => vec![],
            IfExpr { cond, then, other } => vec![cond, then, other],
            Convert { value, .. } | Digits { value, .. } => vec![value],
            Where { value, bindings } => {
                let mut v: Vec<&Expr> = vec![value];
                v.extend(bindings.iter().map(|(_, e)| e));
                v
            }
            Uncertain { value, err } => vec![value, err],
        }
    }

    /// Mutable direct children, in the same order as [`Expr::children`].
    pub fn children_mut(&mut self) -> Vec<&mut Expr> {
        use ExprKind::*;
        match &mut self.kind {
            Num { .. } | Str { .. } | Bool { .. } | Name { .. } | End | Load { .. } => vec![],
            Quantity { value, .. } => vec![value],
            Compare { left, right, tol, .. } => {
                let mut v: Vec<&mut Expr> = vec![left, right];
                if let Some(t) = tol {
                    v.push(t);
                }
                v
            }
            BinOp { left, right, .. } | Logic { left, right, .. } => vec![left, right],
            Neg { operand } | Not { operand } | Sqrt { operand, .. } | Abs { operand } => vec![operand],
            Call { func, args } => {
                let mut v: Vec<&mut Expr> = vec![func];
                v.extend(args.iter_mut());
                v
            }
            Index { target, index } => {
                let mut v: Vec<&mut Expr> = vec![target];
                if let Some(i) = index {
                    v.push(i);
                }
                v
            }
            Slice { lo, hi } => lo.iter_mut().chain(hi.iter_mut()).map(|b| &mut **b).collect(),
            Field { target, .. } | Prime { target, .. } => vec![target],
            Deriv { operand, .. } => vec![operand],
            Integral { integrand, lo, hi, .. } => {
                let mut v: Vec<&mut Expr> = vec![integrand];
                v.extend(lo.iter_mut().map(|b| &mut **b));
                v.extend(hi.iter_mut().map(|b| &mut **b));
                v
            }
            Sum { body, lo, hi, step, .. } => {
                let mut v: Vec<&mut Expr> = vec![body, lo, hi];
                v.extend(step.iter_mut().map(|b| &mut **b));
                v
            }
            ListLit { items } | VecLit { items } | Table { items, .. } => items.iter_mut().collect(),
            VecCalc { .. } => vec![],
            IfExpr { cond, then, other } => vec![cond, then, other],
            Convert { value, .. } | Digits { value, .. } => vec![value],
            Where { value, bindings } => {
                let mut v: Vec<&mut Expr> = vec![value];
                v.extend(bindings.iter_mut().map(|(_, e)| e));
                v
            }
            Uncertain { value, err } => vec![value, err],
        }
    }

    /// Pre-order walk (ast.walk).
    pub fn walk(&self) -> Vec<&Expr> {
        let mut out = vec![];
        fn go<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
            out.push(e);
            for c in e.children() {
                go(c, out);
            }
        }
        go(self, &mut out);
        out
    }

    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        f(self);
        for c in self.children_mut() {
            c.walk_mut(f);
        }
    }

    pub fn name(&self) -> Option<&str> {
        if let ExprKind::Name { name } = &self.kind {
            Some(name)
        } else {
            None
        }
    }

    pub fn is_name(&self) -> bool {
        matches!(self.kind, ExprKind::Name { .. })
    }

    pub fn is_num(&self) -> bool {
        matches!(self.kind, ExprKind::Num { .. })
    }

    pub fn num_value(&self) -> Option<f64> {
        if let ExprKind::Num { value, .. } = &self.kind {
            Some(*value)
        } else {
            None
        }
    }
}

/// A number as written in the source (`2.50e19`, not `2.5e+19`) for messages (gauntlet #81); a number the
/// parser made itself has no spelling, so it is formatted like Python's `f"{x:g}"` (ast.num_text).
pub fn num_text(n: &Expr) -> String {
    if let Some(r) = &n.attrs.raw {
        if !r.is_empty() {
            return r.clone();
        }
    }
    crate::pyfmt::fmt_g(n.num_value().unwrap_or(0.0))
}
