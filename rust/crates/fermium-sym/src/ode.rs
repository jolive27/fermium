//! Solving an equation for its highest derivative (calculus.py `isolate`, `linear_coeffs`), used by ODE solve.
use std::collections::HashMap;

use fermium_syntax::ast as A;

use crate::build::*;
use crate::diff::{d, Plain};
use crate::simplify::simplify;
use crate::source::{key, to_source};
use crate::walk::{depends_on, map_children, subst};

fn replace_keys(e: &A::Expr, keys: &HashMap<String, String>) -> A::Expr {
    if let Some(n) = keys.get(&key(e)) {
        return name(n);
    }
    map_children(e, &mut |c| replace_keys(c, keys))
}

/// Solve lhs = rhs for `target` (an AST node such as x''), assuming it appears linearly: the formula for
/// target, or the error "can't solve this equation for x'': it must appear linearly …".
pub fn isolate(lhs: &A::Expr, rhs: &A::Expr, target: &A::Expr) -> SymResult<A::Expr> {
    isolate_with(lhs, rhs, target, "__H__")
}

/// [`isolate`] with a chosen placeholder name.
pub fn isolate_with(lhs: &A::Expr, rhs: &A::Expr, target: &A::Expr, placeholder: &str) -> SymResult<A::Expr> {
    let mut keys = HashMap::new();
    keys.insert(key(target), placeholder.to_string());
    let dd = simplify(&replace_keys(&sub(lhs.clone(), rhs.clone()), &keys));
    let a = d(&dd, placeholder, &mut Plain).ok().map(|x| simplify(&x));
    let bad = match &a {
        None => true,
        Some(a) => depends_on(a, placeholder) || is_num_v(a, 0.0),
    };
    if bad {
        return Err(ferr(format!("can't solve this equation for {}: it must appear linearly (like m x'' = ...)",
                                to_source(target)), lhs.span, Some("rewrite it as  x'' = <formula>".into())));
    }
    let mut m = HashMap::new();
    m.insert(placeholder.to_string(), num(0.0));
    let b = simplify(&subst(&dd, &m));
    Ok(simplify(&neg(div(b, a.unwrap()))))
}

/// lhs - rhs = Σ a_j · target_j + r0 for equations linear in the targets (Lagrange's mass-matrix form, D47).
/// Returns ([a_j], r0), each simplified, or None if a coefficient still contains a target.
pub fn linear_coeffs(lhs: &A::Expr, rhs: &A::Expr, targets: &[A::Expr]) -> Option<(Vec<A::Expr>, A::Expr)> {
    let names: Vec<String> = (0..targets.len()).map(|j| format!("__T{j}__")).collect();
    let mut keys = HashMap::new();
    for (t, n) in targets.iter().zip(&names) {
        keys.insert(key(t), n.clone());
    }
    let dd = simplify(&replace_keys(&sub(lhs.clone(), rhs.clone()), &keys));
    let mut coeffs = vec![];
    for n in &names {
        let a = simplify(&d(&dd, n, &mut Plain).ok()?);
        if names.iter().any(|m| depends_on(&a, m)) {
            return None;
        }
        coeffs.push(a);
    }
    let m: HashMap<String, A::Expr> = names.iter().map(|n| (n.clone(), num(0.0))).collect();
    let r0 = simplify(&subst(&dd, &m));
    Some((coeffs, r0))
}
