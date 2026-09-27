//! Substitution and tree maps (calculus.py `subst`, `map_children`, `inline_where`, `depends_on`, and
//! ast.py `free_names`).
use std::collections::HashMap;

use fermium_syntax::ast as A;

use crate::build::with_kind;

type K = A::ExprKind;

fn bx(e: A::Expr) -> Box<A::Expr> {
    Box::new(e)
}

/// A copy of `e` with `f` applied to each direct child (Python `map_children`; the kinds Python leaves alone,
/// like `Digits`, `Uncertain` and `VecCalc`, come back unchanged).
pub fn map_children(e: &A::Expr, f: &mut dyn FnMut(&A::Expr) -> A::Expr) -> A::Expr {
    let kind = match &e.kind {
        K::Quantity { value, unit, bracket } => K::Quantity { value: bx(f(value)), unit: unit.clone(), bracket: *bracket },
        K::Compare { op, left, right, tol } => {
            let (l, r) = (f(left), f(right));
            K::Compare { op: op.clone(), left: bx(l), right: bx(r), tol: tol.as_ref().map(|t| bx(f(t))) }
        }
        K::BinOp { op, left, right, implicit } => {
            let (l, r) = (f(left), f(right));
            K::BinOp { op: op.clone(), left: bx(l), right: bx(r), implicit: *implicit }
        }
        K::Logic { op, left, right } => {
            let (l, r) = (f(left), f(right));
            K::Logic { op: op.clone(), left: bx(l), right: bx(r) }
        }
        K::Neg { operand } => K::Neg { operand: bx(f(operand)) },
        K::Not { operand } => K::Not { operand: bx(f(operand)) },
        K::Sqrt { operand, root } => K::Sqrt { operand: bx(f(operand)), root: *root },
        K::Abs { operand } => K::Abs { operand: bx(f(operand)) },
        K::Call { func, args } => {
            let fu = f(func);
            K::Call { func: bx(fu), args: args.iter().map(&mut *f).collect() }
        }
        K::Index { target, index } => {
            let t = f(target);
            K::Index { target: bx(t), index: index.as_ref().map(|i| bx(f(i))) }
        }
        K::Slice { lo, hi } => {
            let l = lo.as_ref().map(|x| bx(f(x)));
            K::Slice { lo: l, hi: hi.as_ref().map(|x| bx(f(x))) }
        }
        K::Field { target, name } => K::Field { target: bx(f(target)), name: name.clone() },
        K::Prime { target, order } => K::Prime { target: bx(f(target)), order: *order },
        K::Deriv { var, order, operand, partial } => {
            K::Deriv { var: var.clone(), order: *order, operand: bx(f(operand)), partial: *partial }
        }
        K::Integral { integrand, var, lo, hi } => {
            let i = f(integrand);
            let l = lo.as_ref().map(|x| bx(f(x)));
            K::Integral { integrand: bx(i), var: var.clone(), lo: l, hi: hi.as_ref().map(|x| bx(f(x))) }
        }
        K::Sum { body, var, lo, hi, step } => {
            let b = f(body);
            let l = f(lo);
            let h = f(hi);
            K::Sum { body: bx(b), var: var.clone(), lo: bx(l), hi: bx(h), step: step.as_ref().map(|x| bx(f(x))) }
        }
        K::ListLit { items } => K::ListLit { items: items.iter().map(&mut *f).collect() },
        K::VecLit { items } => K::VecLit { items: items.iter().map(&mut *f).collect() },
        K::Table { names, items } => K::Table { names: names.clone(), items: items.iter().map(&mut *f).collect() },
        K::IfExpr { cond, then, other } => {
            let c = f(cond);
            let t = f(then);
            K::IfExpr { cond: bx(c), then: bx(t), other: bx(f(other)) }
        }
        K::Convert { value, unit } => K::Convert { value: bx(f(value)), unit: unit.clone() },
        K::Where { value, bindings } => {
            let v = f(value);
            K::Where { value: bx(v), bindings: bindings.iter().map(|(b, x)| (b.clone(), f(x))).collect() }
        }
        _ => return e.clone(),
    };
    with_kind(e, kind)
}

/// Replace free names by expressions (Python `subst`).
pub fn subst(e: &A::Expr, mapping: &HashMap<String, A::Expr>) -> A::Expr {
    match &e.kind {
        K::Name { name } => mapping.get(name).cloned().unwrap_or_else(|| e.clone()),
        K::Integral { integrand, var, lo, hi } => {
            let inner: HashMap<String, A::Expr> =
                mapping.iter().filter(|(k, _)| *k != var).map(|(k, v)| (k.clone(), v.clone())).collect();
            with_kind(e, K::Integral { integrand: bx(subst(integrand, &inner)), var: var.clone(),
                                       lo: lo.as_ref().map(|x| bx(subst(x, mapping))),
                                       hi: hi.as_ref().map(|x| bx(subst(x, mapping))) })
        }
        K::Sum { body, var, lo, hi, step } => {
            let inner: HashMap<String, A::Expr> =
                mapping.iter().filter(|(k, _)| *k != var).map(|(k, v)| (k.clone(), v.clone())).collect();
            with_kind(e, K::Sum { body: bx(subst(body, &inner)), var: var.clone(), lo: bx(subst(lo, mapping)),
                                  hi: bx(subst(hi, mapping)), step: step.as_ref().map(|x| bx(subst(x, mapping))) })
        }
        K::Where { value, bindings } => {
            let inner: HashMap<String, A::Expr> = mapping
                .iter()
                .filter(|(k, _)| !bindings.iter().any(|(b, _)| b == *k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            with_kind(e, K::Where { value: bx(subst(value, &inner)),
                                    bindings: bindings.iter().map(|(b, v)| (b.clone(), subst(v, mapping))).collect() })
        }
        _ => map_children(e, &mut |c| subst(c, mapping)),
    }
}

/// subst with one name.
pub fn subst1(e: &A::Expr, name: &str, v: &A::Expr) -> A::Expr {
    let mut m = HashMap::new();
    m.insert(name.to_string(), v.clone());
    subst(e, &m)
}

/// Replace `a where x = b` by a[x := b] everywhere (Python `inline_where`).
pub fn inline_where(e: &A::Expr) -> A::Expr {
    let e = map_children(e, &mut inline_where);
    if let K::Where { value, bindings } = &e.kind {
        let mut val = (**value).clone();
        for (b, v) in bindings.iter().rev() {
            val = subst1(&val, b, v);
        }
        return val;
    }
    e
}

/// Names used in an expression, not counting variables bound inside (ast.free_names).
pub fn free_names(n: &A::Expr) -> Vec<String> {
    match &n.kind {
        K::Name { name } => vec![name.clone()],
        K::Integral { integrand, var, lo, hi } => {
            let mut out: Vec<String> = free_names(integrand).into_iter().filter(|x| x != var).collect();
            for b in lo.iter().chain(hi.iter()) {
                out.extend(free_names(b));
            }
            out
        }
        K::Sum { body, var, lo, hi, step } => {
            let mut out: Vec<String> = free_names(body).into_iter().filter(|x| x != var).collect();
            out.extend(free_names(lo));
            out.extend(free_names(hi));
            for b in step.iter() {
                out.extend(free_names(b));
            }
            out
        }
        K::Where { value, bindings } => {
            let mut out: Vec<String> =
                free_names(value).into_iter().filter(|x| !bindings.iter().any(|(b, _)| b == x)).collect();
            for (_, v) in bindings {
                out.extend(free_names(v));
            }
            out
        }
        _ => n.children().into_iter().flat_map(free_names).collect(),
    }
}

pub fn depends_on(e: &A::Expr, var: &str) -> bool {
    free_names(e).iter().any(|n| n == var)
}
