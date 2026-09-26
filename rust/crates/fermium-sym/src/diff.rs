//! Symbolic differentiation of the AST (calculus.py `diff`, `_d`, `BUILTIN_DERIVS`, `DiffContext`).
use fermium_syntax::ast as A;

use crate::build::*;
use crate::simplify::{factor_common, simplify};
use crate::walk::{depends_on, inline_where, subst1};

type K = A::ExprKind;

/// What the differentiator needs to know about names (Python `DiffContext`; the checker implements it).
pub trait DiffContext {
    /// (parameter names, body) of a one-line user function, or None; an error for a multi-line one.
    fn user_function(&mut self, _fname: &str) -> SymResult<Option<(Vec<String>, A::Expr)>> {
        Ok(None)
    }
    /// The name of the function ∂ᵒʳᵈᵉʳf/∂(param i)ᵒʳᵈᵉʳ, registered where `fname` is visible.
    fn derived_function(&mut self, fname: &str, _param_index: usize, _order: i64) -> SymResult<String> {
        Err(ferr0(format!("can't differentiate {fname}(...) symbolically"), None))
    }
    fn is_solution(&mut self, _fname: &str) -> bool {
        false
    }
    /// For `mod.f(…)`: a name under which the module's function f can be used here, or None.
    fn field_function(&mut self, _field: &A::Expr) -> Option<String> {
        None
    }
}

/// A context that knows no names (Python's `Plain` in isolate / linear_coeffs).
pub struct Plain;
impl DiffContext for Plain {}

/// The built-in functions with a derivative rule (Python BUILTIN_DERIVS keys).
pub const BUILTIN_DERIV_NAMES: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "exp", "ln", "log", "log10", "log2", "sinh", "cosh", "tanh", "asin",
    "acos", "atan", "sqrt", "cbrt", "abs", "erf",
];

/// f'(u) for a built-in f (Python BUILTIN_DERIVS).
pub fn builtin_deriv(f: &str, u: &A::Expr) -> Option<A::Expr> {
    let u = || u.clone();
    Some(match f {
        "sin" => call1("cos", u()),
        "cos" => neg(call1("sin", u())),
        "tan" => div(num(1.0), pwn(call1("cos", u()), 2.0)),
        "cot" => neg(div(num(1.0), pwn(call1("sin", u()), 2.0))),
        "sec" => mul(call1("sec", u()), call1("tan", u())),
        "csc" => neg(mul(call1("csc", u()), call1("cot", u()))),
        "exp" => call1("exp", u()),
        "ln" | "log" => div(num(1.0), u()),
        "log10" => div(num(1.0), mul(u(), call1("ln", num(10.0)))),
        "log2" => div(num(1.0), mul(u(), call1("ln", num(2.0)))),
        "sinh" => call1("cosh", u()),
        "cosh" => call1("sinh", u()),
        "tanh" => div(num(1.0), pwn(call1("cosh", u()), 2.0)),
        "asin" => div(num(1.0), sqrt(sub(num(1.0), pwn(u(), 2.0)), 2)),
        "acos" => neg(div(num(1.0), sqrt(sub(num(1.0), pwn(u(), 2.0)), 2))),
        "atan" => div(num(1.0), add(num(1.0), pwn(u(), 2.0))),
        "sqrt" => div(num(1.0), mul(num(2.0), sqrt(u(), 2))),
        "cbrt" => div(num(1.0), mul(num(3.0), pwn(sqrt(u(), 3), 2.0))),
        "abs" => call1("sign", u()),
        "erf" => mul(div(num(2.0), sqrt(name("π"), 2)), call1("exp", neg(pwn(u(), 2.0)))),
        _ => return None,
    })
}

/// (sign, combination) of the Bessel recurrences: J, Y: (f(n−1) − f(n+1))/2; I: (…+…)/2; K: −(…+…)/2.
fn bessel(f: &str) -> Option<(i32, i32)> {
    match f {
        "besselj" | "bessely" => Some((1, -1)),
        "besseli" => Some((1, 1)),
        "besselk" => Some((-1, 1)),
        _ => None,
    }
}

/// d e / d var, simplified (Python `diff`).
pub fn diff(e: &A::Expr, var: &str, ctx: &mut dyn DiffContext) -> SymResult<A::Expr> {
    Ok(factor_common(&simplify(&d(&inline_where(e), var, ctx)?)))
}

/// A shorter equivalent formula for display (Python `sympy_tidy`); see tidy.rs.
pub fn tidy(e: &A::Expr) -> A::Expr {
    crate::tidy::tidy(e)
}

/// The raw derivative, unsimplified (Python `_d`).
pub fn d(e: &A::Expr, var: &str, ctx: &mut dyn DiffContext) -> SymResult<A::Expr> {
    match &e.kind {
        K::Num { .. } | K::Str { .. } | K::Bool { .. } => return Ok(num(0.0)),
        K::Quantity { value, unit, bracket } => {
            if !depends_on(value, var) {
                return Ok(num(0.0));
            }
            let one = with_kind(e, K::Quantity { value: Box::new(num(1.0)), unit: unit.clone(), bracket: *bracket });
            return Ok(mul(d(value, var, ctx)?, one));
        }
        K::Name { name } => return Ok(num(if name == var { 1.0 } else { 0.0 })),
        K::Convert { value, .. } => return d(value, var, ctx),
        _ => {}
    }
    if !depends_on(e, var) && !matches!(e.kind, K::Deriv { .. }) {
        return Ok(num(0.0));
    }
    match &e.kind {
        K::BinOp { op, left: a, right: b, .. } => {
            let (a, b) = (&**a, &**b);
            match op.as_str() {
                "+" => return Ok(add(d(a, var, ctx)?, d(b, var, ctx)?)),
                "-" => return Ok(sub(d(a, var, ctx)?, d(b, var, ctx)?)),
                "*" => {
                    let da = d(a, var, ctx)?;
                    let db = d(b, var, ctx)?;
                    return Ok(add(mul(da, b.clone()), mul(a.clone(), db)));
                }
                "/" => {
                    if !depends_on(b, var) {
                        return Ok(div(d(a, var, ctx)?, b.clone()));
                    }
                    let da = d(a, var, ctx)?;
                    let db = d(b, var, ctx)?;
                    return Ok(div(sub(mul(da, b.clone()), mul(a.clone(), db)), pwn(b.clone(), 2.0)));
                }
                "^" => {
                    if !depends_on(b, var) {
                        let bm1 = simplify(&sub(b.clone(), num(1.0)));
                        return Ok(mul(mul(b.clone(), pw(a.clone(), bm1)), d(a, var, ctx)?));
                    }
                    if !depends_on(a, var) {
                        return Ok(mul(mul(e.clone(), call1("ln", a.clone())), d(b, var, ctx)?));
                    }
                    let db = d(b, var, ctx)?;
                    let da = d(a, var, ctx)?;
                    return Ok(mul(e.clone(), add(mul(db, call1("ln", a.clone())), div(mul(b.clone(), da), a.clone()))));
                }
                _ => {}
            }
        }
        K::Neg { operand } => return Ok(neg(d(operand, var, ctx)?)),
        K::Sqrt { operand: u, root } => {
            let du = d(u, var, ctx)?;
            if *root == 2 {
                return Ok(div(du, mul(num(2.0), sqrt((**u).clone(), 2))));
            }
            return Ok(div(du, mul(num(*root as f64), pwn(sqrt((**u).clone(), *root), (*root - 1) as f64))));
        }
        K::Abs { operand } => return Ok(mul(call1("sign", (**operand).clone()), d(operand, var, ctx)?)),
        K::IfExpr { cond, then, other } => {
            let t = d(then, var, ctx)?;
            let o = d(other, var, ctx)?;
            return Ok(if_expr((**cond).clone(), t, o));
        }
        K::VecLit { items } => {
            let mut out = vec![];
            for x in items {
                out.push(d(x, var, ctx)?);
            }
            return Ok(veclit(out));
        }
        K::Deriv { var: v2, operand, .. } => {
            if operand.is_name() {
                return Err(ferr("can't differentiate this derivative expression symbolically", e.span, None));
            }
            let inner = diff(operand, v2, ctx)?;
            return d(&inner, var, ctx);
        }
        K::Call { func: f, args } => {
            if let K::Field { .. } = &f.kind {
                if let Some(qn) = ctx.field_function(f) {
                    let mut nm = name(&qn);
                    nm.span = f.span;
                    return d(&with_kind(e, K::Call { func: Box::new(nm), args: args.clone() }), var, ctx);
                }
            }
            if let Some(fname) = f.name() {
                if args.len() == 1 {
                    if let Some(fd) = builtin_deriv(fname, &args[0]) {
                        return Ok(mul(fd, d(&args[0], var, ctx)?));
                    }
                }
                if args.len() == 1 && ctx.is_solution(fname) {
                    let u = &args[0];
                    return Ok(mul(call_e(prime(name(fname), 1), vec![u.clone()]), d(u, var, ctx)?));
                }
                let uf = match ctx.user_function(fname) {
                    Ok(x) => x,
                    Err(mut ex) => {
                        if ex.line.is_none() && e.span.line != 0 {
                            ex.line = Some(e.span.line);
                            ex.col = Some(e.span.col);
                            ex.length = fname.chars().count() as u32;
                        }
                        return Err(ex);
                    }
                };
                if uf.is_some() {
                    let mut out: Option<A::Expr> = None;
                    for (i, arg) in args.iter().enumerate() {
                        let da = d(arg, var, ctx)?;
                        if is_num_v(&simplify(&da), 0.0) {
                            continue;
                        }
                        let dname = ctx.derived_function(fname, i, 1)?;
                        let t = mul(call_e(name(&dname), args.clone()), da);
                        out = Some(match out {
                            None => t,
                            Some(o) => add(o, t),
                        });
                    }
                    return Ok(out.unwrap_or_else(|| num(0.0)));
                }
                if let (Some((sgn, comb)), 2) = (bessel(fname), args.len()) {
                    let (n, x) = (&args[0], &args[1]);
                    if !is_num_v(&simplify(&d(n, var, ctx)?), 0.0) {
                        return Err(ferr(format!("can't differentiate {fname}(n, x) with respect to its order n"),
                                        e.span, None));
                    }
                    let lo = call(fname, vec![sub(n.clone(), num(1.0)), x.clone()]);
                    let hi = call(fname, vec![add(n.clone(), num(1.0)), x.clone()]);
                    let dd = div(if comb > 0 { add(lo, hi) } else { sub(lo, hi) }, num(2.0));
                    return Ok(mul(if sgn < 0 { neg(dd) } else { dd }, d(x, var, ctx)?));
                }
                if matches!(fname, "ellipk" | "ellipe") && args.len() == 1 {
                    let m = &args[0];
                    let kk = call1("ellipk", m.clone());
                    let ee = call1("ellipe", m.clone());
                    let dd = if fname == "ellipk" {
                        div(sub(ee, mul(sub(num(1.0), m.clone()), kk)),
                            mul(mul(num(2.0), m.clone()), sub(num(1.0), m.clone())))
                    } else {
                        div(sub(ee, kk), mul(num(2.0), m.clone()))
                    };
                    return Ok(mul(dd, d(m, var, ctx)?));
                }
                if fname == "hypot" && args.len() == 2 {
                    let (a, b) = (&args[0], &args[1]);
                    let da = d(a, var, ctx)?;
                    let db = d(b, var, ctx)?;
                    return Ok(div(add(mul(a.clone(), da), mul(b.clone(), db)), e.clone()));
                }
                if matches!(fname, "floor" | "ceil" | "round" | "sign") {
                    return Ok(num(0.0));
                }
                return Err(ferr(format!("can't differentiate {fname}(...) symbolically"), e.span,
                                Some("differentiate a formula made of +, -, ×, /, powers and standard functions".into())));
            }
            if let K::Prime { target, order } = &f.kind {
                if let Some(tn) = target.name() {
                    if ctx.is_solution(tn) {
                        let u = &args[0];
                        return Ok(mul(call_e(prime((**target).clone(), order + 1), vec![u.clone()]), d(u, var, ctx)?));
                    }
                    if ctx.user_function(tn)?.is_some() {
                        let dn = ctx.derived_function(tn, 0, *order)?;
                        return d(&call_e(name(&dn), args.clone()), var, ctx);
                    }
                }
            }
        }
        K::Sum { body, var: sv, lo, hi, step } => {
            // d/dx Σ f(k, x) = Σ ∂f/∂x, term by term (D51); the limits may not depend on x
            if sv == var {
                return Ok(num(0.0));
            }
            for b in [Some(lo), Some(hi), step.as_ref()].into_iter().flatten() {
                if depends_on(b, var) {
                    return Err(ferr(format!("can't differentiate this sum with respect to {var}: its limits depend on {var}"),
                                    e.span, Some("the number of terms changes in steps, so it has no derivative".into())));
                }
            }
            let inner = simplify(&d(&inline_where(body), var, ctx)?);
            if is_num_v(&inner, 0.0) {
                return Ok(num(0.0));
            }
            return Ok(with_kind(e, K::Sum { body: Box::new(inner), var: sv.clone(), lo: lo.clone(), hi: hi.clone(),
                                            step: step.clone() }));
        }
        K::Integral { integrand, var: iv, lo: Some(lo), hi } => {
            // Leibniz rule: d/dx ∫ f(x, s) ds from a(x) to b(x) = ∫ ∂f/∂x ds + f(x, b) b'(x) - f(x, a) a'(x) (D36)
            let mut res = num(0.0);
            if iv != var && depends_on(integrand, var) {
                let inner = tidy(&simplify(&d(&inline_where(integrand), var, ctx)?));
                if !is_num_v(&inner, 0.0) {
                    res = with_kind(e, K::Integral { integrand: Box::new(inner), var: iv.clone(), lo: Some(lo.clone()),
                                                     hi: hi.clone() });
                }
            }
            if let Some(h) = hi {
                if depends_on(h, var) {
                    res = add(res, mul(subst1(integrand, iv, h), d(h, var, ctx)?));
                }
            }
            if depends_on(lo, var) {
                res = sub(res, mul(subst1(integrand, iv, lo), d(lo, var, ctx)?));
            }
            return Ok(res);
        }
        _ => {}
    }
    if let K::Call { func, .. } = &e.kind {
        if let K::Field { target, name: fname } = &func.kind {
            if let Some(t) = target.name() {
                let raw = func.attrs.raw.clone().unwrap_or_else(|| fname.clone());
                return Err(ferr(format!("can't differentiate {t}.{raw}(...) symbolically"), e.span,
                                Some("if it is a Python function, it is a black box to Fermium: write the formula in \
                                      Fermium, or take a finite difference, like (f(x + h) - f(x - h)) / (2 h)".into())));
            }
        }
    }
    Err(ferr("can't differentiate this expression symbolically", e.span, None))
}
