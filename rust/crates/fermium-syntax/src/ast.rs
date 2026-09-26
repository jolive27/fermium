//! The abstract syntax tree: a port of `fermium/ast.py` (Fermium 1.5, the oracle).
//!
//! Every node records where it came from (line, column, length) so errors can point at it. The Python parser
//! attaches extra facts to nodes as attributes (`times_unit`, `imag_literal`, the number's source spelling, ...);
//! here they live in [`Attrs`], so the checker can read them exactly as the Python checker does.

use num_rational::Rational64;

/// Where a node came from: 1-based line and column (in code points), and its length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub length: u32,
}

/// Facts the parser records on an expression beyond its kind (the Python parser's ad-hoc attributes).
#[derive(Clone, Debug, Default)]
pub struct Attrs {
    /// The number as written (`2.50e19`), for messages.
    pub raw: Option<String>,
    /// `4i`, `1i`: an imaginary literal (D90).
    pub imag_literal: bool,
    /// `(…) MeV`, `2 (…) MeV`: multiplied by one unit (D215, D238).
    pub times_unit: bool,
    /// `(73/24)` made by the fraction-coefficient rule (D236).
    pub coefficient: bool,
    /// Juxtaposition: was there whitespace before the right operand, and its token index.
    pub juxt_ws: Option<bool>,
    /// `A_d u` where u is undefined: the value on the left, for the hint (#55).
    pub unit_left: Option<Box<Expr>>,
    /// An integral's division info (D34, D205), kept opaque for the checker.
    pub div_info: Option<DivInfo>,
    /// The number of `^`-written exponent from a superscript (`x²` gives 2).
    pub extra_int: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct DivInfo {
    pub text: String,
    pub tight: bool,
    pub warned: bool,
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
    pub juxt_join: bool,
}

#[derive(Clone, Debug)]
pub struct Expr {
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
    Str(String),
    Bool(bool),
    Name(String),
    /// `+ - * / ^`; `implicit` for juxtaposition.
    BinOp { op: BinOpKind, left: Box<Expr>, right: Box<Expr>, implicit: bool },
    Neg(Box<Expr>),
    /// `== != < > <= >= ≈`, with `within tol` for ≈ (D260).
    Compare { op: CmpOp, left: Box<Expr>, right: Box<Expr>, tol: Option<Box<Expr>> },
    Logic { and: bool, left: Box<Expr>, right: Box<Expr> },
    Not(Box<Expr>),
    Call { func: Box<Expr>, args: Vec<Expr> },
    Index { target: Box<Expr>, index: Box<Expr> },
    /// `a:b` inside `xs[...]`, both included, 1-based (D114).
    Slice { lo: Option<Box<Expr>>, hi: Option<Box<Expr>> },
    /// `end` inside an index.
    End,
    Field { target: Box<Expr>, name: String },
    Prime { target: Box<Expr>, order: u32 },
    /// `d/dt operand`, `d²/dt² operand`, `∂/∂x operand`.
    Deriv { var: String, order: u32, operand: Box<Expr>, partial: bool },
    Integral { integrand: Box<Expr>, var: String, lo: Option<Box<Expr>>, hi: Option<Box<Expr>> },
    /// `Σ(body for var from lo to hi step st)` (D51).
    Sum { body: Box<Expr>, var: String, lo: Box<Expr>, hi: Box<Expr>, step: Option<Box<Expr>> },
    Sqrt { operand: Box<Expr>, root: u32 },
    Abs(Box<Expr>),
    ListLit(Vec<Expr>),
    /// `table(x = xs, y = ys)` (D193).
    Table { names: Vec<String>, items: Vec<Expr> },
    /// `∇f`, `∇·F`, `∇×F`, `∇²f`.
    VecCalc { kind: VecCalcKind, func: Box<Expr> },
    VecLit(Vec<Expr>),
    IfExpr { cond: Box<Expr>, then: Box<Expr>, other: Box<Expr> },
    /// `expr in unit`.
    Convert { value: Box<Expr>, unit: UnitExpr },
    /// `expr to N digits`.
    Digits { value: Box<Expr>, digits: u32 },
    Load(String),
    Where { value: Box<Expr>, bindings: Vec<(String, Expr)> },
    /// `a ± b` (D120).
    Uncertain { value: Box<Expr>, err: Box<Expr> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Approx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VecCalcKind {
    Grad,
    Div,
    Curl,
    Lap,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub unit: Option<UnitExpr>,
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

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `name = value`, `+=`, `-=`, `*=`, `/=`.
    Assign { name: String, value: Expr, op: String },
    /// `xs[i] = …`, `M[i, j] = …` (D195).
    IndexAssign { target: String, index: Expr, index2: Option<Expr>, value: Expr, op: String },
    FuncDef { name: String, params: Vec<Param>, body: FuncBody, where_: Vec<(String, Expr)> },
    Print(Vec<Expr>),
    Plot { series: Vec<PlotSeries>, out: Option<String>, options: Vec<(String, Expr)> },
    Solve {
        equations: Vec<Equation>,
        initial: Vec<Equation>,
        var: String,
        lo: Expr,
        hi: Expr,
        step: Option<Expr>,
        method: Option<String>,
        tolerance: Option<Expr>,
        until: Option<Equation>,
        absolute: Option<Vec<Expr>>,
        /// Eigenvalue problems (`lowest N`, `grid N`) and PDEs keep their extra clauses here.
        extra: Vec<(String, Expr)>,
    },
    /// `solve lhs = rhs for x from a to b` (D32).
    SolveAlgebraic { eq: Equation, var: String, lo: Expr, hi: Expr },
    Fit { model: Equation, data: Expr, guesses: Vec<(String, Expr)> },
    Analyze { title: Option<String>, target: Param, inputs: Vec<Param> },
    If { cond: Expr, then: Vec<Stmt>, other: Option<Vec<Stmt>> },
    For { var: String, lo: Expr, hi: Expr, step: Option<Expr>, body: Vec<Stmt>, parallel: bool },
    ForIn { var: String, iterable: Expr, body: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
    Assert { cond: Expr, message: Option<String> },
    /// `units natural(ħ = c = 1)` or `units nuclear:` + a block (D60).
    Units { system: String, consts: Vec<String>, body: Option<Vec<Stmt>> },
    /// `import mechanics`, `import "lib/x.fm" as x`, `from nuclear import a, b as c` (D100).
    Import { module: String, is_path: bool, alias: Option<String>, names: Option<Vec<(String, Option<String>)>> },
    /// `use python numpy as np` (D140).
    UsePython { module: String, alias: Option<String>, sigs: Vec<PySig> },
    /// `propagate montecarlo [N samples]` + a block (D123).
    Propagate { samples: Option<Expr>, body: Vec<Stmt> },
    /// `seed(n)`, `push(xs, v)` and other calls used as statements stay `Expr`.
    Pass,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub body: Vec<Stmt>,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span, paren: false, attrs: Attrs::default() }
    }
}
