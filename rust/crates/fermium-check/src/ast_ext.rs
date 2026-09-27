//! Helpers over the fermium-syntax AST: building synthetic nodes, and operators as enums.
use fermium_syntax::ast as A;

/// A node the checker makes itself (Python builds A.* objects the same way); id 0 = synthetic.
pub fn mk(kind: A::ExprKind, span: A::Span) -> A::Expr {
    A::Expr { id: 0, kind, span, paren: false, attrs: A::Attrs::default() }
}

pub fn name(n: &str, span: A::Span) -> A::Expr {
    mk(A::ExprKind::Name { name: n.to_string() }, span)
}

pub fn binop(op: &str, left: A::Expr, right: A::Expr, implicit: bool, span: A::Span) -> A::Expr {
    mk(A::ExprKind::BinOp { op: op.to_string(), left: Box::new(left), right: Box::new(right), implicit }, span)
}
