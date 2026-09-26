//! Constructors for synthetic AST nodes (calculus.py `num`, `name`, `call`, `add`, …) and the error type.
//!
//! A node made here has no position (line 0 = Python's `line = None`) and id 0, like a node the Python
//! calculus module builds itself.
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

pub type SymResult<T> = Result<T, Diagnostic>;

/// A FermiumError at a span; a span without a line (a synthetic node) gives an error without a position.
pub fn ferr(msg: impl Into<String>, span: A::Span, hint: Option<String>) -> Diagnostic {
    let mut d = Diagnostic::error(msg, span.line, span.col, 1, hint);
    if span.line == 0 {
        d.line = None;
    }
    if span.col == 0 {
        d.col = None;
    }
    d
}

/// An error with no position (FermiumError(msg, hint=…)).
pub fn ferr0(msg: impl Into<String>, hint: Option<String>) -> Diagnostic {
    ferr(msg, A::Span::default(), hint)
}

pub fn mk(kind: A::ExprKind) -> A::Expr {
    A::Expr { id: 0, kind, span: A::Span::default(), paren: false, attrs: A::Attrs::default() }
}

/// A copy of `e` with a new kind, keeping its position and attributes (Python `_copy`).
pub fn with_kind(e: &A::Expr, kind: A::ExprKind) -> A::Expr {
    A::Expr { id: e.id, kind, span: e.span, paren: e.paren, attrs: e.attrs.clone() }
}

/// Copy the position of another node (Python `.at(other)`).
pub fn at(mut e: A::Expr, other: &A::Expr) -> A::Expr {
    e.span = other.span;
    e
}

/// `A.Num(float(v), None, False)`.
pub fn num(v: f64) -> A::Expr {
    mk(A::ExprKind::Num { value: v, sigfigs: None, digit: false })
}

pub fn is_num(e: &A::Expr, v: Option<f64>) -> bool {
    match &e.kind {
        A::ExprKind::Num { value, .. } => v.is_none_or(|x| *value == x),
        _ => false,
    }
}

pub fn is_num_v(e: &A::Expr, v: f64) -> bool {
    is_num(e, Some(v))
}

pub fn name(n: &str) -> A::Expr {
    mk(A::ExprKind::Name { name: n.to_string() })
}

pub fn call(f: &str, args: Vec<A::Expr>) -> A::Expr {
    mk(A::ExprKind::Call { func: Box::new(name(f)), args })
}

pub fn call1(f: &str, a: A::Expr) -> A::Expr {
    call(f, vec![a])
}

pub fn call_e(f: A::Expr, args: Vec<A::Expr>) -> A::Expr {
    mk(A::ExprKind::Call { func: Box::new(f), args })
}

pub fn binop(op: &str, a: A::Expr, b: A::Expr, implicit: bool) -> A::Expr {
    mk(A::ExprKind::BinOp { op: op.to_string(), left: Box::new(a), right: Box::new(b), implicit })
}

pub fn add(a: A::Expr, b: A::Expr) -> A::Expr {
    binop("+", a, b, false)
}

pub fn sub(a: A::Expr, b: A::Expr) -> A::Expr {
    binop("-", a, b, false)
}

/// `A.BinOp("*", a, b, implicit=True)`.
pub fn mul(a: A::Expr, b: A::Expr) -> A::Expr {
    binop("*", a, b, true)
}

pub fn div(a: A::Expr, b: A::Expr) -> A::Expr {
    binop("/", a, b, false)
}

pub fn pw(a: A::Expr, b: A::Expr) -> A::Expr {
    binop("^", a, b, false)
}

pub fn pwn(a: A::Expr, b: f64) -> A::Expr {
    pw(a, num(b))
}

pub fn neg(a: A::Expr) -> A::Expr {
    mk(A::ExprKind::Neg { operand: Box::new(a) })
}

pub fn sqrt(a: A::Expr, root: i64) -> A::Expr {
    mk(A::ExprKind::Sqrt { operand: Box::new(a), root })
}

pub fn prime(t: A::Expr, order: i64) -> A::Expr {
    mk(A::ExprKind::Prime { target: Box::new(t), order })
}

pub fn if_expr(c: A::Expr, t: A::Expr, o: A::Expr) -> A::Expr {
    mk(A::ExprKind::IfExpr { cond: Box::new(c), then: Box::new(t), other: Box::new(o) })
}

pub fn veclit(items: Vec<A::Expr>) -> A::Expr {
    mk(A::ExprKind::VecLit { items })
}

/// The operator of a BinOp, if `e` is one.
pub fn op_of(e: &A::Expr) -> Option<&str> {
    match &e.kind {
        A::ExprKind::BinOp { op, .. } => Some(op.as_str()),
        _ => None,
    }
}

/// (op, left, right) of a BinOp.
pub fn bin(e: &A::Expr) -> Option<(&str, &A::Expr, &A::Expr)> {
    match &e.kind {
        A::ExprKind::BinOp { op, left, right, .. } => Some((op.as_str(), left, right)),
        _ => None,
    }
}

pub fn num_val(e: &A::Expr) -> Option<f64> {
    e.num_value()
}

/// `isinstance(e, A.Call) and isinstance(e.func, A.Name) and e.func.name == fname and len(e.args) == 1`.
pub fn is_call(e: &A::Expr, fname: &str) -> bool {
    matches!(&e.kind, A::ExprKind::Call { func, args } if args.len() == 1 && func.name() == Some(fname))
}

/// The argument list of a call to a plain name: (name, args).
pub fn call_parts(e: &A::Expr) -> Option<(&str, &[A::Expr])> {
    match &e.kind {
        A::ExprKind::Call { func, args } => func.name().map(|n| (n, args.as_slice())),
        _ => None,
    }
}
