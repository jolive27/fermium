//! Native indefinite integrals, replacing v1's SymPy bridge (spec §B6): rule tables, linear and general
//! substitution, integration by parts, completing the square and partial fractions. Every antiderivative is
//! checked by differentiating it and comparing with the integrand at sample points (like v1's
//! `_antiderivative_ok`). When no formula is found, the error suggests a definite integral.
//! Coverage compared with v1: rust/DIVERGENCES.md.
use std::collections::HashMap;

use fermium_syntax::ast as A;

use crate::build::*;
use crate::diff::{d, Plain};
use crate::simplify::{base_pow, build_product, factors, simplify, sum_terms};
use crate::source::key;
use crate::walk::{depends_on, free_names, map_children, subst1};

type K = A::ExprKind;

const HINT: &str = "give limits (from a to b) to compute it numerically";

/// ∫ integrand d var (without +C). `positive` names the symbols that are safe to assume positive (physical
/// constants, variables only ever given positive values); every other name is just real (D37).
pub fn integrate(integrand: &A::Expr, var: &str, positive: &[String]) -> SymResult<A::Expr> {
    let mut ig = Integrator { x: var.to_string(), positive: positive.to_vec(), depth: 0, fresh: 0, budget: 400, subs: 0 };
    let e = simplify(integrand);
    let Some(r) = ig.integ(&e) else {
        // the term of a sum that has no formula, for the special-function message
        let mut bad = e.clone();
        for (_, t) in sum_terms(&e, 1) {
            let mut ig2 = Integrator { x: var.to_string(), positive: positive.to_vec(), depth: 0, fresh: 0, budget: 400,
                                       subs: 0 };
            if ig2.integ(&simplify(&t)).is_none() {
                bad = simplify(&t);
                break;
            }
        }
        let e = bad;
        if let Some(f) = non_elementary(&e, var) {
            return Err(ferr0(format!("SymPy's formula for this integral uses the function {f}, which Fermium doesn't \
                                      have yet: {f}({})", sympy_str(&ig.nonelem_arg(&e).unwrap_or_else(|| name(var)))),
                             Some(HINT.into())));
        }
        return Err(ferr0("Fermium couldn't find a formula for this integral", Some(HINT.into())));
    };
    // SymPy's canonical form and order, as v1 printed its results; v1 kept the shorter of SymPy's answer and its
    // simplify(), which the tidy candidates (factored, together) stand in for
    let pos: Vec<String> = positive.iter().filter(|p| p.as_str() != var).cloned().collect();
    let heurisch = e.walk().iter().any(|n| matches!(call_parts(n), Some(("exp", [u])) if linear(u, var).is_some()));
    let r = crate::tidy::sympy_best_with(&simplify(&r), &pos, heurisch);
    if !antiderivative_ok(&r, &e, var, positive) {
        return Err(ferr0("Fermium's formula for this integral isn't right for every value of the constants in it, so \
                          Fermium won't use it", Some(HINT.into())));
    }
    Ok(r)
}

/// A formula in SymPy's spelling (for the message about a missing special function).
fn sympy_str(e: &A::Expr) -> String {
    crate::tidy::sympy_text(e)
}

/// The special function a known non-elementary integral needs (Ei, Si, Ci, li, erfi), if it is one.
fn non_elementary(e: &A::Expr, x: &str) -> Option<&'static str> {
    let (_, fs) = factors(e);
    let vf: Vec<&A::Expr> = fs.iter().filter(|f| depends_on(f, x)).collect();
    let lin = |u: &A::Expr| linear(u, x).is_some();
    if vf.len() == 2 {
        let (b0, p0) = base_pow(vf[0]);
        let (b1, p1) = base_pow(vf[1]);
        for ((fb, fp), (gb, gp)) in [((&b0, p0), (&b1, p1)), ((&b1, p1), (&b0, p0))] {
            if fp == 1.0 && gp == -1.0 && lin(gb) {
                for (f, name) in [("exp", "Ei"), ("sin", "Si"), ("cos", "Ci"), ("sinh", "Shi"), ("cosh", "Chi")] {
                    // f(c u)/u for a constant c: Ei(c u), Si(c u), …
                    if is_call(fb, f) {
                        let arg = &call_parts(fb).unwrap().1[0];
                        let ratio = simplify(&div(arg.clone(), gb.clone()));
                        if key(arg) == key(gb) || !depends_on(&ratio, x) {
                            return Some(name);
                        }
                    }
                }
            }
        }
    }
    // ln(u) · exp(c u) (times a power of u): by parts leaves exp(c u)/u
    if vf.iter().any(|f| matches!(call_parts(f), Some(("ln" | "log", [u])) if lin(u)))
        && vf.iter().any(|f| matches!(call_parts(f), Some(("exp", [u])) if lin(u)))
    {
        return Some("Ei");
    }
    if vf.len() == 1 {
        let (b, p) = base_pow(vf[0]);
        if p == -1.0 && is_call(&b, "ln") && lin(&call_parts(&b).unwrap().1[0]) {
            return Some("li");
        }
        if p == 1.0 && is_call(&b, "exp") {
            if let Some(c) = poly_coeffs(&call_parts(&b).unwrap().1[0], x, 2) {
                if c.len() == 3 && is_positive_known(&c[2]) {
                    return Some("erfi");
                }
            }
        }
    }
    None
}

/// A number or a product of positive-looking parts (a literal > 0).
fn is_positive_known(e: &A::Expr) -> bool {
    match &e.kind {
        K::Num { value, .. } => *value > 0.0,
        _ => false,
    }
}

struct Integrator {
    x: String,
    positive: Vec<String>,
    depth: u32,
    fresh: u32,
    /// the most integ calls one integral may make (the search gives up, rather than taking long)
    budget: u32,
    /// nested substitutions in progress
    subs: u32,
}

/// (a, b) when u = a x + b with a, b free of x.
fn linear(u: &A::Expr, x: &str) -> Option<(A::Expr, A::Expr)> {
    if !depends_on(u, x) {
        return None;
    }
    let a = simplify(&d(u, x, &mut Plain).ok()?);
    if depends_on(&a, x) || is_num_v(&a, 0.0) {
        return None;
    }
    let b = simplify(&subst1(u, x, &num(0.0)));
    Some((a, b))
}

/// The coefficients c0, c1, … of u as a polynomial in x of degree ≤ maxdeg, or None.
fn poly_coeffs(u: &A::Expr, x: &str, maxdeg: usize) -> Option<Vec<A::Expr>> {
    if !polyish(u, x) {
        return None;
    }
    let mut out = vec![];
    let mut cur = u.clone();
    let mut fact = 1.0;
    for k in 0..=maxdeg + 1 {
        if is_num_v(&cur, 0.0) {
            // trailing zero coefficients are dropped
            while out.len() > 1 && out.last().is_some_and(|c: &A::Expr| is_num_v(c, 0.0)) {
                out.pop();
            }
            return Some(out);
        }
        if k == maxdeg + 1 {
            return None;
        }
        if k > 0 {
            fact *= k as f64;
        }
        let c = simplify(&div(subst1(&cur, x, &num(0.0)), num(fact)));
        out.push(c);
        cur = simplify(&d(&cur, x, &mut Plain).ok()?);
    }
    None
}

/// Built only from +, −, ×, whole powers and x-free parts (a cheap test before differentiating).
fn polyish(e: &A::Expr, x: &str) -> bool {
    if !depends_on(e, x) {
        return true;
    }
    match &e.kind {
        K::Name { .. } => true,
        K::Neg { operand } => polyish(operand, x),
        K::BinOp { op, left, right, .. } => match op.as_str() {
            "+" | "-" | "*" => polyish(left, x) && polyish(right, x),
            "/" => polyish(left, x) && !depends_on(right, x),
            "^" => polyish(left, x) && right.num_value().is_some_and(|p| p >= 0.0 && p == p.trunc() && p <= 12.0),
            _ => false,
        },
        _ => false,
    }
}

/// (base, exponent) of a factor, unwrapping √ and nested powers: √u → (u, 1/2), (√u)^-1 → (u, -1/2).
fn base_exp(f: &A::Expr) -> (A::Expr, A::Expr) {
    match &f.kind {
        K::Sqrt { operand, root } => {
            let (b, p) = base_exp(operand);
            (b, simplify(&div(p, num(*root as f64))))
        }
        K::BinOp { op, left, right, .. } if op == "^" => {
            let (b, p) = base_exp(left);
            (b, simplify(&mul(p, (**right).clone())))
        }
        _ => (f.clone(), num(1.0)),
    }
}

impl Integrator {
    fn positive(&self, e: &A::Expr) -> bool {
        match &e.kind {
            K::Num { value, .. } => *value > 0.0,
            K::Name { name } => name == "π" || self.positive.iter().any(|p| p == name),
            K::Quantity { value, .. } => match value.num_value() {
                Some(v) => v > 0.0,
                None => self.positive(value),
            },
            K::BinOp { op, left, right, .. } if op == "*" || op == "/" => self.positive(left) && self.positive(right),
            K::BinOp { op, left, right, .. } if op == "^" => {
                self.positive(left) || right.num_value().is_some_and(|p| p % 2.0 == 0.0)
            }
            K::Sqrt { operand, .. } => self.positive(operand),
            K::Call { func, .. } => func.name() == Some("exp"),
            _ => false,
        }
    }

    /// √c for a c ≥ 0: b for c = b² (|b| unless b > 0), √n for a number, else √(c) when c is positive.
    fn sqrt_of(&self, c: &A::Expr) -> Option<A::Expr> {
        if let Some(v) = c.num_value() {
            return (v > 0.0).then(|| num(v.sqrt()));
        }
        let (cc, fs) = factors(c);
        if cc > 0.0 {
            let mut out = vec![];
            for f in &fs {
                let (b, p) = base_pow(f);
                if p % 2.0 == 0.0 {
                    let r = if self.positive(&b) { b.clone() } else { mk(K::Abs { operand: Box::new(b.clone()) }) };
                    out.push(if p == 2.0 { r } else { pwn(r, p / 2.0) });
                } else if self.positive(&b) {
                    out.push(if p == 1.0 { sqrt(b, 2) } else { pwn(b, p / 2.0) });
                } else {
                    return None;
                }
            }
            return Some(simplify(&build_product(cc.sqrt(), &out)));
        }
        None
    }

    /// Is c known to be ≥ 0 (a square, a positive)? Some(true) positive, Some(false) negative, None unknown.
    fn sign_of(&self, c: &A::Expr) -> Option<bool> {
        if let Some(v) = c.num_value() {
            return if v > 0.0 { Some(true) } else if v < 0.0 { Some(false) } else { None };
        }
        let (cc, fs) = factors(c);
        let mut pos = true;
        for f in &fs {
            let (b, p) = base_pow(f);
            if !(p % 2.0 == 0.0 || self.positive(&b)) {
                pos = false;
            }
        }
        if !pos || cc == 0.0 {
            return None;
        }
        Some(cc > 0.0)
    }

    fn integ(&mut self, e: &A::Expr) -> Option<A::Expr> {
        if self.depth > 10 || self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        self.depth += 1;
        let r = self.integ_in(e);
        self.depth -= 1;
        r
    }

    fn integ_in(&mut self, e: &A::Expr) -> Option<A::Expr> {
        let x = self.x.clone();
        if !depends_on(e, &x) {
            return Some(mul(e.clone(), name(&x)));
        }
        match &e.kind {
            K::Neg { operand } => return self.integ(operand).map(neg),
            K::BinOp { op, left, right, .. } if op == "+" || op == "-" => {
                let a = self.integ(left)?;
                let b = self.integ(right)?;
                return Some(if op == "+" { add(a, b) } else { sub(a, b) });
            }
            _ => {}
        }
        let (c, fs) = factors(e);
        let (vf, kf): (Vec<A::Expr>, Vec<A::Expr>) = fs.into_iter().partition(|f| depends_on(f, &x));
        let konst = build_product(c, &kf);
        let vf = merge_powers(vf);
        let vpart = build_product(1.0, &vf);
        let (_, vf) = factors(&vpart);
        let g = self.integ_product(&vf, &vpart)?;
        Some(if is_num_v(&konst, 1.0) { g } else { mul(konst, g) })
    }

    fn integ_product(&mut self, vf: &[A::Expr], whole: &A::Expr) -> Option<A::Expr> {
        let x = self.x.clone();
        if vf.len() == 1 {
            if matches!(op_of(&vf[0]), Some("+") | Some("-")) {
                return self.integ(&vf[0]); // (a + b)/2: term by term
            }
            if let Some(r) = self.integ_atom(&vf[0]) {
                return Some(r);
            }
        }
        if let Some(r) = self.linear_over_quadratic(vf) {
            return Some(r);
        }
        if let Some(r) = self.trig_product(vf) {
            return Some(r);
        }
        if let Some(r) = self.exp_trig(vf) {
            return Some(r);
        }
        if let Some(r) = self.by_parts(vf) {
            return Some(r);
        }
        // a polynomial written as a product (x (x + 1)²): expand and integrate term by term
        if let Some(c) = poly_coeffs(whole, &x, 12) {
            if c.len() > 1 && vf.len() > 1 || vf.iter().any(|f| matches!(op_of(f), Some("+") | Some("-"))) {
                let mut terms = vec![];
                for (k, ck) in c.iter().enumerate() {
                    if !is_num_v(ck, 0.0) {
                        terms.push(mul(div(ck.clone(), num(k as f64 + 1.0)), pwn(name(&x), k as f64 + 1.0)));
                    }
                }
                let mut out = terms.first()?.clone();
                for t in &terms[1..] {
                    out = add(out, t.clone());
                }
                return Some(out);
            }
        }
        // a sum times something: distribute
        if vf.len() > 1 {
            if let Some(i) = vf.iter().position(|f| matches!(op_of(f), Some("+") | Some("-"))) {
                let others: Vec<A::Expr> = vf.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, f)| f.clone()).collect();
                let mut out: Option<A::Expr> = None;
                for (s, t) in crate::simplify::terms(&vf[i]) {
                    let mut fs = others.clone();
                    fs.push(t);
                    let g = self.integ(&simplify(&build_product(s as f64, &fs)))?;
                    out = Some(match out {
                        None => g,
                        Some(o) => add(o, g),
                    });
                }
                return out;
            }
        }
        if let Some(r) = self.substitution(whole) {
            return Some(r);
        }
        self.partial_fractions(whole)
    }

    /// ∫ of one factor f (a function of x).
    fn integ_atom(&mut self, f: &A::Expr) -> Option<A::Expr> {
        let x = self.x.clone();
        let xn = || name(&x);
        if f.name() == Some(&x) {
            return Some(div(pwn(xn(), 2.0), num(2.0)));
        }
        if let K::Abs { operand } = &f.kind {
            return self.abs_rule(operand);
        }
        if let Some(("abs", [u])) = call_parts(f) {
            return self.abs_rule(u);
        }
        let (b, p) = base_exp(f);
        if !is_num_v(&p, 1.0) || matches!(f.kind, K::Sqrt { .. }) {
            return self.power_rule(&b, &p);
        }
        if let Some((fname, [u])) = call_parts(f) {
            if let Some((a, _)) = linear(u, &x) {
                let g = call_rule(fname, u)?;
                return Some(over(g, a));
            }
            if fname == "exp" {
                // exp(A x² + B x + C) with A < 0: an error function
                if let Some(c) = poly_coeffs(u, &x, 2) {
                    if c.len() == 3 && self.sign_of(&c[2]) == Some(false) {
                        let a = simplify(&neg(c[2].clone()));
                        let sa = self.sqrt_of(&a)?;
                        let shift = simplify(&div(c[1].clone(), mul(num(2.0), a.clone())));
                        let rest = simplify(&add(c[0].clone(), div(pwn(c[1].clone(), 2.0), mul(num(4.0), a.clone()))));
                        let arg = simplify(&mul(sa.clone(), sub(xn(), shift)));
                        let k = mul(div(sqrt(name("π"), 2), mul(num(2.0), sa)), call1("exp", rest));
                        return Some(mul(k, call1("erf", arg)));
                    }
                }
            }
        }
        None
    }

    /// ∫ |u| for u = a x + b: if u <= 0 then -u²/(2a) else u²/(2a) (SymPy's Piecewise).
    fn abs_rule(&mut self, u: &A::Expr) -> Option<A::Expr> {
        let (a, _) = linear(u, &self.x)?;
        let f = div(pwn(u.clone(), 2.0), mul(num(2.0), a));
        let cond = mk(K::Compare { op: "<=".into(), left: Box::new(u.clone()), right: Box::new(num(0.0)), tol: None });
        Some(if_expr(cond, neg(f.clone()), f))
    }

    /// ∫ b^p for a base b of x and an exponent p (either may be constant).
    fn power_rule(&mut self, b: &A::Expr, p: &A::Expr) -> Option<A::Expr> {
        let x = self.x.clone();
        let xn = || name(&x);
        if !depends_on(b, &x) {
            // c^u with u linear: c^u / (a ln c)
            let (a, _) = linear(p, &x)?;
            return Some(div(pw(b.clone(), p.clone()), mul(a, call1("ln", b.clone()))));
        }
        if depends_on(p, &x) {
            return None;
        }
        let pv = p.num_value();
        if let Some((a, c0)) = linear(b, &x) {
            // (a x + b)ⁿ with whole n and numbers a, b: SymPy expands it and integrates term by term
            if let (Some(n), Some(_), Some(bv)) = (pv, a.num_value(), c0.num_value()) {
                if n >= 2.0 && n == n.trunc() && n <= 12.0 && bv != 0.0 && b.name() != Some(&x) {
                    if let Some(c) = poly_coeffs(&pw(b.clone(), p.clone()), &x, 12) {
                        let mut out: Option<A::Expr> = None;
                        for (k, ck) in c.iter().enumerate() {
                            if is_num_v(ck, 0.0) {
                                continue;
                            }
                            let t = mul(div(ck.clone(), num(k as f64 + 1.0)), pwn(xn(), k as f64 + 1.0));
                            out = Some(match out {
                                None => t,
                                Some(o) => add(o, t),
                            });
                        }
                        return out;
                    }
                }
            }
            if pv == Some(-1.0) {
                return Some(div(call1("ln", b.clone()), a));
            }
            let p1 = simplify(&add(p.clone(), num(1.0)));
            return Some(div(pw(b.clone(), p1.clone()), mul(a, p1)));
        }
        // 1/(1 ± cos u): tan(u/2), −cot(u/2)
        if pv == Some(-1.0) {
            if let Some((op, l, r)) = bin(b) {
                if (op == "+" || op == "-") && is_num_v(l, 1.0) {
                    if let Some(("cos", [u])) = call_parts(r) {
                        if let Some((a, _)) = linear(u, &x) {
                            let h = div(u.clone(), num(2.0));
                            let g = if op == "+" { call1("tan", h) } else { neg(call1("cot", h)) };
                            return Some(div(g, a));
                        }
                    }
                }
            }
        }
        // sinⁿ, cosⁿ (n ≥ 3) by the reduction formulas; lnⁿ by parts
        if let Some(n) = pv.filter(|n| *n >= 3.0 && *n == n.trunc() && *n <= 12.0) {
            if let Some((fname @ ("sin" | "cos"), [u])) = call_parts(b) {
                if let Some((a, _)) = linear(u, &x).filter(|_| n % 2.0 == 1.0) {
                    // odd n, as SymPy: sinⁿ u = (1 − cos² u)^m sin u, so ∫ = −Σ C(m,j) (−1)^j cos^(2j+1) u/(2j+1)
                    let m = ((n - 1.0) / 2.0) as i64;
                    let (s, c) = (call1("sin", u.clone()), call1("cos", u.clone()));
                    let other = if fname == "sin" { c } else { s };
                    let mut out: Option<A::Expr> = None;
                    let mut binom = 1.0;
                    for j in 0..=m {
                        if j > 0 {
                            binom = binom * (m - j + 1) as f64 / j as f64;
                        }
                        let coef = binom * if j % 2 == 0 { 1.0 } else { -1.0 } / (2 * j + 1) as f64;
                        let coef = if fname == "sin" { -coef } else { coef };
                        let t = mul(num(coef), pwn(other.clone(), (2 * j + 1) as f64));
                        out = Some(match out {
                            None => t,
                            Some(o) => add(o, t),
                        });
                    }
                    return Some(div(out?, a));
                }
                if let Some((a, _)) = linear(u, &x) {
                    // ∫ sinⁿ u = -sinⁿ⁻¹ u cos u / n + (n−1)/n ∫ sinⁿ⁻² u ; cos: +cosⁿ⁻¹ u sin u / n + …
                    let (s, c) = (call1("sin", u.clone()), call1("cos", u.clone()));
                    let head = if fname == "sin" {
                        neg(div(mul(pwn(s, n - 1.0), c), num(n)))
                    } else {
                        div(mul(pwn(c, n - 1.0), s), num(n))
                    };
                    let rest = self.integ(&simplify(&pwn(b.clone(), n - 2.0)))?;
                    return Some(add(div(head, a), mul(num((n - 1.0) / n), rest)));
                }
            }
        }
        if let Some(n) = pv.filter(|n| *n >= 2.0 && *n == n.trunc() && *n <= 12.0) {
            if let Some(("ln" | "log", [u])) = call_parts(b) {
                if let Some((a, _)) = linear(u, &x) {
                    // ∫ lnⁿ u du = u lnⁿ u − n ∫ lnⁿ⁻¹ u du
                    let lower = self.integ(&simplify(&pwn(b.clone(), n - 1.0)))?;
                    return Some(sub(div(mul(u.clone(), pw(b.clone(), num(n))), a), mul(num(n), lower)));
                }
            }
        }
        // trigonometric squares of a linear argument
        if pv == Some(2.0) {
            if let Some((fname, [u])) = call_parts(b) {
                if let Some((a, _)) = linear(u, &x) {
                    let u = u.clone();
                    let r = match fname {
                        "sec" => call1("tan", u),
                        "csc" => neg(call1("cot", u)),
                        "sin" => sub(div(u.clone(), num(2.0)), div(call1("sin", mul(num(2.0), u)), num(4.0))),
                        "cos" => add(div(u.clone(), num(2.0)), div(call1("sin", mul(num(2.0), u)), num(4.0))),
                        "tan" => sub(call1("tan", u.clone()), u),
                        "cot" => sub(neg(call1("cot", u.clone())), u),
                        "sinh" => sub(div(call1("sinh", mul(num(2.0), u.clone())), num(4.0)), div(u, num(2.0))),
                        "cosh" => add(div(call1("sinh", mul(num(2.0), u.clone())), num(4.0)), div(u, num(2.0))),
                        "tanh" => sub(u.clone(), call1("tanh", u)),
                        _ => return None,
                    };
                    return Some(div(r, a));
                }
            }
        }
        if pv == Some(-2.0) {
            if let Some((fname, [u])) = call_parts(b) {
                if let Some((a, _)) = linear(u, &x) {
                    let u = u.clone();
                    let r = match fname {
                        "cos" => call1("tan", u),
                        "sin" => neg(call1("cot", u)),
                        "cosh" => call1("tanh", u),
                        "sinh" => neg(div(num(1.0), call1("tanh", u))),
                        _ => return None,
                    };
                    return Some(div(r, a));
                }
            }
        }
        // quadratic bases: A x² + B x + C with p = -1, ±1/2
        let c = poly_coeffs(b, &x, 2)?;
        if c.len() != 3 {
            return None;
        }
        let (a2, b1, c0) = (&c[2], &c[1], &c[0]);
        // A (u² + r) with u = x + B/(2A), r = value at the vertex / A
        let vertex = simplify(&neg(div(b1.clone(), mul(num(2.0), a2.clone()))));
        let u = simplify(&sub(xn(), vertex.clone()));
        let at_vertex = simplify(&subst1(b, &x, &vertex));
        let _ = c0;
        let r = simplify(&div(at_vertex, a2.clone()));
        let pv = pv?;
        let sa = self.sign_of(a2)?;
        let rs = self.sign_of(&r);
        if pv == -1.0 {
            // 1/(A (u² + r))
            if is_num_v(&r, 0.0) {
                return Some(neg(div(num(1.0), mul(a2.clone(), u))));
            }
            let (pos_r, k) = match rs? {
                true => (true, r.clone()),
                false => (false, simplify(&neg(r.clone()))),
            };
            let sk = self.sqrt_of(&k)?;
            let q = div(u.clone(), sk.clone());
            let _ = sa;
            if pos_r {
                return Some(div(call1("atan", q), mul(a2.clone(), sk)));
            }
            // 1/(u² − k²) = (ln(u − k) − ln(u + k))/(2k): SymPy's form, defined for |u| > k like v1's (atanh
            // would be NaN there)
            let two_k = mul(num(2.0), sk.clone());
            let g = sub(div(call1("ln", sub(u.clone(), sk.clone())), two_k.clone()),
                        div(call1("ln", add(u, sk)), two_k));
            return Some(div(g, a2.clone()));
        }
        if pv == -0.5 || pv == 0.5 {
            // √A √(u² + r) (A > 0) or √(-A) √(k - u²) (A < 0)
            let (s_a, abs_a) = if sa { (true, a2.clone()) } else { (false, simplify(&neg(a2.clone()))) };
            let sqa = self.sqrt_of(&abs_a)?;
            if s_a {
                let rs = rs?;
                let k = if rs { r.clone() } else { simplify(&neg(r.clone())) };
                let sk = self.sqrt_of(&k)?;
                let q = div(u.clone(), sk.clone());
                if pv == -0.5 {
                    // SymPy: asinh(u/k), and ln(u + √(u² − k²)) for the other sign
                    let g = if rs {
                        call1("asinh", q)
                    } else {
                        call1("ln", add(u.clone(), sqrt(sub(pwn(u.clone(), 2.0), k.clone()), 2)))
                    };
                    return Some(div(g, sqa));
                }
                let root = sqrt(add(pwn(u.clone(), 2.0), r.clone()), 2);
                let g = if rs { call1("asinh", q) } else { call1("acosh", q) };
                let inner = add(mul(u, root), mul(r, g));
                return Some(mul(div(sqa, num(2.0)), inner));
            }
            // A < 0: √(-A) √(k - u²) with k = -r > 0
            let k = simplify(&neg(r.clone()));
            if self.sign_of(&k) != Some(true) {
                return None;
            }
            let sk = self.sqrt_of(&k)?;
            let q = div(u.clone(), sk);
            if pv == -0.5 {
                return Some(div(call1("asin", q), sqa));
            }
            let root = sqrt(sub(k.clone(), pwn(u.clone(), 2.0)), 2);
            let inner = add(mul(u, root), mul(k, call1("asin", q)));
            return Some(mul(div(sqa, num(2.0)), inner));
        }
        None
    }

    /// ∫ (m x + n) Q^p for a quadratic Q = A x² + B x + C and p = -1, ±1/2: (m/2A) ∫ Q' Q^p + (n − mB/2A) ∫ Q^p.
    fn linear_over_quadratic(&mut self, vf: &[A::Expr]) -> Option<A::Expr> {
        if vf.len() != 2 {
            return None;
        }
        let x = self.x.clone();
        for (l, q) in [(&vf[0], &vf[1]), (&vf[1], &vf[0])] {
            let Some((m, n)) = linear(l, &x) else { continue };
            let (qb, qp) = base_exp(q);
            let Some(pv) = qp.num_value() else { continue };
            if !(pv == -1.0 || pv == -0.5 || pv == 0.5) {
                continue;
            }
            let Some(c) = poly_coeffs(&qb, &x, 2) else { continue };
            if c.len() != 3 {
                continue;
            }
            let k1 = simplify(&div(m.clone(), mul(num(2.0), c[2].clone())));
            let k2 = simplify(&sub(n, mul(k1.clone(), c[1].clone())));
            // ∫ Q' Q^p = ln Q (p = -1) or Q^(p+1)/(p+1)
            let first = if pv == -1.0 {
                call1("ln", qb.clone())
            } else {
                div(pwn(qb.clone(), pv + 1.0), num(pv + 1.0))
            };
            let mut out = mul(k1, first);
            if !is_num_v(&k2, 0.0) {
                let second = self.power_rule(&qb, &num(pv))?;
                out = add(out, mul(k2, second));
            }
            return Some(out);
        }
        None
    }

    /// ∫ sin A sin B, sin A cos B, cos A cos B (A, B linear): product to sum.
    fn trig_product(&mut self, vf: &[A::Expr]) -> Option<A::Expr> {
        if vf.len() != 2 {
            return None;
        }
        let x = self.x.clone();
        let (Some((f, [u])), Some((g, [v]))) = (call_parts(&vf[0]), call_parts(&vf[1])) else { return None };
        if !matches!(f, "sin" | "cos") || !matches!(g, "sin" | "cos") {
            return None;
        }
        linear(u, &x)?;
        linear(v, &x)?;
        let (dif, sum) = (simplify(&sub(u.clone(), v.clone())), simplify(&add(u.clone(), v.clone())));
        let half = |e: A::Expr| div(e, num(2.0));
        let e = match (f, g) {
            ("sin", "sin") => half(sub(call1("cos", dif), call1("cos", sum))),
            ("cos", "cos") => half(add(call1("cos", dif), call1("cos", sum))),
            ("sin", "cos") => half(add(call1("sin", sum), call1("sin", dif))),
            _ => half(sub(call1("sin", sum), call1("sin", dif))),
        };
        self.integ(&simplify(&e))
    }

    /// ∫ e^(a x + c) sin(b x + d) and cos: e(a sin − b cos)/(a² + b²), e(a cos + b sin)/(a² + b²).
    fn exp_trig(&mut self, vf: &[A::Expr]) -> Option<A::Expr> {
        if vf.len() != 2 {
            return None;
        }
        let x = self.x.clone();
        for (e, t) in [(&vf[0], &vf[1]), (&vf[1], &vf[0])] {
            let (Some(("exp", [u])), Some((tf, [v]))) = (call_parts(e), call_parts(t)) else { continue };
            if tf != "sin" && tf != "cos" {
                continue;
            }
            let (a, _) = linear(u, &x)?;
            let (b, _) = linear(v, &x)?;
            let den = add(pwn(a.clone(), 2.0), pwn(b.clone(), 2.0));
            let (s, c) = (call1("sin", v.clone()), call1("cos", v.clone()));
            let num_ = if tf == "sin" { sub(mul(a, s), mul(b, c)) } else { add(mul(a, c), mul(b, s)) };
            return Some(div(mul(e.clone(), num_), den));
        }
        None
    }

    /// ∫ P(x) g(a x + b) with P a polynomial and g = exp, sin, cos, sinh, cosh (tabular integration by parts);
    /// ∫ P(x) ln(a x + b), P(x) atan(x), asin(x) by parts once.
    fn by_parts(&mut self, vf: &[A::Expr]) -> Option<A::Expr> {
        let x = self.x.clone();
        if vf.len() < 2 {
            // ln(x), atan(x), asin(x) alone are in the call table
            return None;
        }
        for i in 0..vf.len() {
            let g = &vf[i];
            let Some((gname, [u])) = call_parts(g) else { continue };
            let others: Vec<A::Expr> = vf.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, f)| f.clone()).collect();
            let p = simplify(&build_product(1.0, &others));
            let is_poly = poly_coeffs(&p, &x, 12).is_some();
            // x^p (any p ≠ −1) is fine for the logarithm and inverse functions: ∫ x^p ln x = x^(p+1) ln x/(p+1) − …
            let x_power = others.len() == 1 && {
                let (b, e) = base_exp(&others[0]);
                b.name() == Some(x.as_str()) && !depends_on(&e, &x) && !is_num_v(&e, -1.0)
            };
            if !is_poly && !(x_power && matches!(gname, "ln" | "log" | "atan" | "asin" | "acos" | "asinh" | "atanh")) {
                continue;
            }
            if linear(u, &x).is_none() {
                continue;
            }
            match gname {
                "exp" | "sin" | "cos" | "sinh" | "cosh" => {
                    // Σ (-1)^k P^(k) G_(k+1)
                    let mut out: Option<A::Expr> = None;
                    let mut pk = p.clone();
                    let mut gk = g.clone();
                    let mut sign = 1.0;
                    for _ in 0..14 {
                        if is_num_v(&pk, 0.0) {
                            break;
                        }
                        gk = simplify(&self.integ(&gk)?);
                        let t = mul(num(sign), mul(pk.clone(), gk.clone()));
                        out = Some(match out {
                            None => t,
                            Some(o) => add(o, t),
                        });
                        pk = simplify(&d(&pk, &x, &mut Plain).ok()?);
                        sign = -sign;
                    }
                    return out;
                }
                "ln" | "log" | "atan" | "asin" | "acos" | "asinh" | "atanh" => {
                    // ∫ P g = Q g − ∫ Q g'
                    let q = simplify(&self.integ(&p)?);
                    let gd = simplify(&d(g, &x, &mut Plain).ok()?);
                    let rest = self.integ(&simplify(&mul(q.clone(), gd)))?;
                    return Some(sub(mul(q, g.clone()), rest));
                }
                _ => {}
            }
        }
        None
    }

    /// ∫ F(u(x)) u'(x) dx = ∫ F(t) dt at t = u(x), for u the argument of a function, a power's base or a root.
    fn substitution(&mut self, e: &A::Expr) -> Option<A::Expr> {
        if self.subs >= 2 {
            return None;
        }
        let x = self.x.clone();
        let mut cands: Vec<A::Expr> = vec![];
        let mut seen: Vec<String> = vec![];
        collect_candidates(e, &x, &mut cands, &mut seen);
        for u in cands {
            if linear(&u, &x).is_some() || u.name() == Some(&x) {
                continue;
            }
            let Ok(du) = d(&u, &x, &mut Plain) else { continue };
            let du = simplify(&du);
            if is_num_v(&du, 0.0) {
                continue;
            }
            let ratio = simplify(&div(e.clone(), du));
            self.fresh += 1;
            let t = format!("__u{}__", self.fresh);
            let r2 = simplify(&replace_key(&ratio, &key(&u), &t));
            if depends_on(&r2, &x) {
                continue;
            }
            let saved = std::mem::replace(&mut self.x, t.clone());
            self.subs += 1;
            let g = self.integ(&r2);
            self.subs -= 1;
            self.x = saved;
            if let Some(g) = g {
                return Some(subst1(&g, &t, &u));
            }
        }
        None
    }

    /// P(x)/Q(x) with numeric coefficients: polynomial part plus partial fractions over real linear and quadratic
    /// factors of Q.
    fn partial_fractions(&mut self, e: &A::Expr) -> Option<A::Expr> {
        let x = self.x.clone();
        let (c, fs) = factors(e);
        let (mut nums, mut dens) = (vec![build_product(c, &[])], vec![]);
        for f in &fs {
            let (b, p) = base_pow(f);
            if p < 0.0 && p == p.trunc() {
                dens.push(pwn(b, -p));
            } else {
                nums.push(f.clone());
            }
        }
        if dens.is_empty() {
            return None;
        }
        let pnum = simplify(&build_product(1.0, &nums));
        let pden = simplify(&build_product(1.0, &dens));
        let pc = numeric_coeffs(&poly_coeffs(&pnum, &x, 12)?)?;
        let qc = numeric_coeffs(&poly_coeffs(&pden, &x, 12)?)?;
        if qc.len() < 2 {
            return None;
        }
        let (quot, rem) = poly_divmod(&pc, &qc);
        let mut out: Option<A::Expr> = None;
        let mut push = |t: A::Expr| {
            out = Some(match out.take() {
                None => t,
                Some(o) => add(o, t),
            })
        };
        for (k, ck) in quot.iter().enumerate() {
            if *ck != 0.0 {
                push(mul(num(ck / (k as f64 + 1.0)), pwn(name(&x), k as f64 + 1.0)));
            }
        }
        if rem.iter().all(|v| *v == 0.0) {
            return out.or_else(|| Some(num(0.0)));
        }
        // factor Q: real roots (with multiplicity) and conjugate pairs
        let lead = *qc.last().unwrap();
        let roots = poly_roots(&qc)?;
        let mut facs: Vec<(Vec<f64>, usize)> = vec![]; // monic factor coefficients (low first), multiplicity
        let mut used = vec![false; roots.len()];
        for i in 0..roots.len() {
            if used[i] {
                continue;
            }
            let (re, im) = roots[i];
            used[i] = true;
            let tol = 1e-7 * (1.0 + re.abs() + im.abs());
            let f = if im.abs() <= tol {
                vec![-re, 1.0]
            } else {
                // its conjugate
                let j = (0..roots.len()).find(|&j| !used[j] && (roots[j].0 - re).abs() <= tol && (roots[j].1 + im).abs() <= tol)?;
                used[j] = true;
                vec![re * re + im * im, -2.0 * re, 1.0]
            };
            let f = f.iter().map(|v| clean(*v)).collect::<Vec<_>>();
            match facs.iter_mut().find(|(g, _)| g.len() == f.len() && g.iter().zip(&f).all(|(a, b)| (a - b).abs() <= 1e-7 * (1.0 + a.abs()))) {
                Some(g) => g.1 += 1,
                None => facs.push((f, 1)),
            }
        }
        // unknowns: for each factor and each power k = 1..m: A (linear) or B x + C (quadratic)
        let mut cols: Vec<(usize, usize, usize)> = vec![]; // (factor, power, which: 0 const, 1 x)
        for (fi, (f, m)) in facs.iter().enumerate() {
            for k in 1..=*m {
                cols.push((fi, k, 0));
                if f.len() == 3 {
                    cols.push((fi, k, 1));
                }
            }
        }
        let n = cols.len();
        if n != qc.len() - 1 {
            return None;
        }
        // rem(x)/Q(x) = Σ coef·x^w / f^k; multiply by Q/lead and match at sample points
        let qmon: Vec<f64> = qc.iter().map(|v| v / lead).collect();
        let remm: Vec<f64> = rem.iter().map(|v| v / lead).collect();
        let pts: Vec<f64> = (0..n).map(|i| 0.37 + 0.91 * i as f64).collect();
        let mut mat = vec![vec![0.0; n + 1]; n];
        for (r, &xv) in pts.iter().enumerate() {
            let qv = peval(&qmon, xv);
            for (ci, &(fi, k, w)) in cols.iter().enumerate() {
                let fv = peval(&facs[fi].0, xv);
                mat[r][ci] = xv.powi(w as i32) * qv / fv.powi(k as i32);
            }
            mat[r][n] = peval(&remm, xv);
        }
        let sol = solve_dense(mat)?;
        for (ci, &(fi, k, w)) in cols.iter().enumerate() {
            if w == 1 {
                continue;
            }
            let f = &facs[fi].0;
            let a0 = clean(sol[ci]);
            if f.len() == 2 {
                if a0 == 0.0 {
                    continue;
                }
                let base = simplify(&add(name(&x), num(f[0])));
                push(if k == 1 {
                    mul(num(a0), call1("ln", base))
                } else {
                    div(num(-a0), mul(num(k as f64 - 1.0), pwn(base, k as f64 - 1.0)))
                });
            } else {
                // (B x + C)/(x² + p x + q)^k, handled for k = 1 by the quadratic rules
                let b = clean(sol[ci + 1]);
                let term = div(add(mul(num(b), name(&x)), num(a0)),
                               pwn(add(add(pwn(name(&x), 2.0), mul(num(f[1]), name(&x))), num(f[0])), k as f64));
                let g = self.integ(&simplify(&term))?;
                push(g);
            }
        }
        out
    }

    fn nonelem_arg(&self, e: &A::Expr) -> Option<A::Expr> {
        // the argument of the exponential / trigonometric factor (Ei(2*x) for exp(2x)/x)
        let (_, fs) = factors(e);
        for f in &fs {
            if let Some((fname, [u])) = call_parts(f) {
                if matches!(fname, "exp" | "sin" | "cos" | "sinh" | "cosh") && depends_on(u, &self.x) {
                    return Some(u.clone());
                }
            }
        }
        let (_, fs) = factors(e);
        for f in fs {
            let (b, p) = base_pow(&f);
            if p == -1.0 && depends_on(&b, &self.x) {
                return Some(b);
            }
        }
        None
    }
}

/// g / a, with 1/𝑖 written as -𝑖 (SymPy's form: ∫ exp(𝑖 x) dx = -𝑖 exp(𝑖 x)).
fn over(g: A::Expr, a: A::Expr) -> A::Expr {
    let (c, fs) = factors(&a);
    if let Some(i) = fs.iter().position(|f| f.name() == Some("𝑖")) {
        let mut rest = fs.clone();
        rest.remove(i);
        let r = build_product(c, &rest);
        let g = if is_num_v(&r, 1.0) { g } else { div(g, r) };
        return mul(neg(name("𝑖")), g);
    }
    div(g, a)
}

/// Factors with the same base combined, √ and nested powers included: √x · x² → x^(5/2).
fn merge_powers(fs: Vec<A::Expr>) -> Vec<A::Expr> {
    // exp(a) exp(b) = exp(a + b) and exp(a)ⁿ = exp(n a), as SymPy combines them
    let fs: Vec<A::Expr> = fs
        .into_iter()
        .map(|f| {
            let (b, e) = base_exp(&f);
            match call_parts(&b) {
                Some(("exp", [u])) if !is_num_v(&e, 1.0) && e.num_value().is_some() => {
                    call1("exp", simplify(&mul(e, u.clone())))
                }
                _ => f,
            }
        })
        .collect();
    let (exps, others): (Vec<A::Expr>, Vec<A::Expr>) = fs.into_iter().partition(|f| is_call(f, "exp"));
    let mut fs = others;
    if exps.len() > 1 {
        let mut arg = call_parts(&exps[0]).unwrap().1[0].clone();
        for e in &exps[1..] {
            arg = add(arg, call_parts(e).unwrap().1[0].clone());
        }
        fs.push(call1("exp", simplify(&arg)));
    } else {
        fs.extend(exps);
    }
    let mut out: Vec<(String, A::Expr, A::Expr)> = vec![];
    for f in fs {
        let (b, e) = base_exp(&f);
        let k = key(&b);
        match out.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.2 = simplify(&add(x.2.clone(), e)),
            None => out.push((k, b, e)),
        }
    }
    out.into_iter()
        .filter(|(_, _, e)| !is_num_v(e, 0.0))
        .map(|(_, b, e)| if is_num_v(&e, 1.0) { b } else { pw(b, e) })
        .collect()
}

fn clean(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() < 1e-9 * (1.0 + v.abs()) { r } else { v }
}

fn numeric_coeffs(c: &[A::Expr]) -> Option<Vec<f64>> {
    c.iter().map(|e| e.num_value()).collect()
}

fn peval(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, v| acc * x + v)
}

/// (quotient, remainder) of polynomials given low coefficient first.
fn poly_divmod(p: &[f64], q: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut r = p.to_vec();
    let dq = q.len() - 1;
    if p.len() < q.len() {
        return (vec![], r);
    }
    let mut quot = vec![0.0; p.len() - dq];
    for k in (0..quot.len()).rev() {
        let c = r[k + dq] / q[dq];
        quot[k] = c;
        for j in 0..=dq {
            r[k + j] -= c * q[j];
        }
    }
    r.truncate(dq);
    (quot, r.into_iter().map(clean).collect())
}

/// The complex roots of a real polynomial (Durand–Kerner, polished by Newton).
fn poly_roots(c: &[f64]) -> Option<Vec<(f64, f64)>> {
    let n = c.len() - 1;
    let lead = c[n];
    let a: Vec<f64> = c.iter().map(|v| v / lead).collect();
    type C = (f64, f64);
    let mulc = |x: C, y: C| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
    let divc = |x: C, y: C| {
        let d = y.0 * y.0 + y.1 * y.1;
        ((x.0 * y.0 + x.1 * y.1) / d, (x.1 * y.0 - x.0 * y.1) / d)
    };
    let evalc = |z: C| {
        let mut acc = (0.0, 0.0);
        for v in a.iter().rev() {
            acc = mulc(acc, z);
            acc.0 += v;
        }
        acc
    };
    let mut z: Vec<C> = (0..n).map(|k| {
        let t = 0.4 + 0.9 * k as f64;
        (t.cos() * 0.9_f64.powi(k as i32 + 1) + 0.1, t.sin())
    }).collect();
    for _ in 0..500 {
        let mut delta = 0.0f64;
        for i in 0..n {
            let mut den = (1.0, 0.0);
            for j in 0..n {
                if i != j {
                    den = mulc(den, (z[i].0 - z[j].0, z[i].1 - z[j].1));
                }
            }
            let w = divc(evalc(z[i]), den);
            z[i] = (z[i].0 - w.0, z[i].1 - w.1);
            delta = delta.max(w.0.abs() + w.1.abs());
        }
        if delta < 1e-15 {
            break;
        }
    }
    if z.iter().any(|r| !r.0.is_finite() || !r.1.is_finite()) {
        return None;
    }
    Some(z.into_iter().map(|(re, im)| (clean(re), if im.abs() < 1e-9 * (1.0 + re.abs()) { 0.0 } else { im })).collect())
}

/// Gaussian elimination with partial pivoting on an augmented matrix.
fn solve_dense(mut m: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let n = m.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[piv][col].abs() < 1e-300 {
            return None;
        }
        m.swap(col, piv);
        for r in 0..n {
            if r != col {
                let f = m[r][col] / m[col][col];
                for k in col..=n {
                    m[r][k] -= f * m[col][k];
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

fn replace_key(e: &A::Expr, k: &str, t: &str) -> A::Expr {
    if key(e) == k {
        return name(t);
    }
    map_children(e, &mut |c| replace_key(c, k, t))
}

fn collect_candidates(e: &A::Expr, x: &str, out: &mut Vec<A::Expr>, seen: &mut Vec<String>) {
    let mut push = |u: &A::Expr, out: &mut Vec<A::Expr>| {
        if depends_on(u, x) {
            let k = key(u);
            if !seen.contains(&k) {
                seen.push(k);
                out.push(u.clone());
            }
        }
    };
    match &e.kind {
        K::Call { args, .. } => {
            for a in args {
                push(a, out);
            }
            push(e, out);
        }
        K::Sqrt { operand, .. } => push(operand, out),
        K::BinOp { op, left, .. } if op == "^" => push(left, out),
        K::BinOp { op, right, .. } if op == "/" => push(right, out),
        _ => {}
    }
    for c in e.children() {
        collect_candidates(c, x, out, seen);
    }
}

/// ∫ f(u) du for the functions with a table entry (u linear; the caller divides by u').
fn call_rule(f: &str, u: &A::Expr) -> Option<A::Expr> {
    let u = || u.clone();
    Some(match f {
        "sin" => neg(call1("cos", u())),
        "cos" => call1("sin", u()),
        "tan" => neg(call1("ln", call1("cos", u()))),
        "cot" => call1("ln", call1("sin", u())),
        "sec" => call1("ln", add(call1("tan", u()), call1("sec", u()))),
        "csc" => neg(call1("ln", add(call1("cot", u()), call1("csc", u())))),
        "exp" => call1("exp", u()),
        "sinh" => call1("cosh", u()),
        "cosh" => call1("sinh", u()),
        "tanh" => call1("ln", call1("cosh", u())),
        "ln" | "log" => sub(mul(u(), call1("ln", u())), u()),
        "log10" => div(sub(mul(u(), call1("ln", u())), u()), call1("ln", num(10.0))),
        "log2" => div(sub(mul(u(), call1("ln", u())), u()), call1("ln", num(2.0))),
        "sqrt" => div(mul(num(2.0), pwn(u(), 1.5)), num(3.0)),
        "cbrt" => div(mul(num(3.0), pwn(u(), 4.0 / 3.0)), num(4.0)),
        "asin" => add(mul(u(), call1("asin", u())), sqrt(sub(num(1.0), pwn(u(), 2.0)), 2)),
        "acos" => sub(mul(u(), call1("acos", u())), sqrt(sub(num(1.0), pwn(u(), 2.0)), 2)),
        "atan" => sub(mul(u(), call1("atan", u())), div(call1("ln", add(pwn(u(), 2.0), num(1.0))), num(2.0))),
        "asinh" => sub(mul(u(), call1("asinh", u())), sqrt(add(pwn(u(), 2.0), num(1.0)), 2)),
        "atanh" => add(mul(u(), call1("atanh", u())), div(call1("ln", sub(num(1.0), pwn(u(), 2.0))), num(2.0))),
        "erf" => add(mul(u(), call1("erf", u())), div(call1("exp", neg(pwn(u(), 2.0))), sqrt(name("π"), 2))),
        "sign" => mk(K::Abs { operand: Box::new(u()) }),
        _ => return None,
    })
}

/// Check dF/dx = f at pseudo-random real points (positive symbols get positive values), like v1's
/// `_antiderivative_ok`: a guard against a formula that silently assumes a sign.
fn antiderivative_ok(big_f: &A::Expr, f: &A::Expr, x: &str, positive: &[String]) -> bool {
    let Ok(df) = d(big_f, x, &mut Plain) else { return true };
    let mut names: Vec<String> = free_names(big_f).into_iter().chain(free_names(f)).collect();
    names.sort();
    names.dedup();
    names.retain(|n| n != "π" && n != "𝑖");
    let mut seed: u64 = 1234;
    let mut rnd = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let mut checked = 0;
    for _ in 0..36 {
        let mut env = HashMap::new();
        for n in &names {
            let v = 0.3 + 2.7 * rnd();
            let pos = positive.iter().any(|p| p == n) || rnd() < 0.5;
            env.insert(n.clone(), if pos { v } else { -v });
        }
        let (Some(a), Some(b)) = (crate::numeval::eval(&df, &env), crate::numeval::eval(f, &env)) else { continue };
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        if (a - b).abs() > 1e-7 * (a.abs() + b.abs() + 1e-300) {
            return false;
        }
        checked += 1;
        if checked >= 12 {
            break;
        }
    }
    true
}
