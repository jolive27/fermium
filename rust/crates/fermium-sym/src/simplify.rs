//! Simplification (calculus.py `simplify`, `_factors`, `_build_product`, `factor_common`) and the stable
//! evaluation form of a derivative (`stabilize`, A52).
use fermium_syntax::ast as A;

use crate::build::*;
use crate::source::key;
use crate::walk::map_children;

type K = A::ExprKind;

/// Flatten a product/quotient into (coefficient, [factors]); divisors become x^-1 factors (Python `_factors`).
pub fn factors(e: &A::Expr) -> (f64, Vec<A::Expr>) {
    match &e.kind {
        K::BinOp { op, left, right, .. } if op == "*" => {
            let (c1, mut f1) = factors(left);
            let (c2, f2) = factors(right);
            f1.extend(f2);
            (c1 * c2, f1)
        }
        K::BinOp { op, left, right, .. } if op == "/" => {
            let (c1, mut f1) = factors(left);
            let (c2, f2) = factors(right);
            if c2 == 0.0 {
                return (1.0, vec![e.clone()]);
            }
            f1.extend(f2.iter().map(inverse));
            (c1 / c2, f1)
        }
        K::Neg { operand } => {
            let (c, f) = factors(operand);
            (-c, f)
        }
        K::Num { value, .. } => (*value, vec![]),
        K::Quantity { value, unit, bracket } => match value.num_value() {
            Some(v) if v != 1.0 => {
                (v, vec![with_kind(e, K::Quantity { value: Box::new(num(1.0)), unit: unit.clone(), bracket: *bracket })])
            }
            _ => (1.0, vec![e.clone()]),
        },
        _ => (1.0, vec![e.clone()]),
    }
}

fn inverse(f: &A::Expr) -> A::Expr {
    if let K::BinOp { op, left, right, .. } = &f.kind {
        if op == "^" {
            if let Some(v) = right.num_value() {
                return pw((**left).clone(), num(-v));
            }
        }
    }
    pw(f.clone(), num(-1.0))
}

fn rank(x: &A::Expr) -> u8 {
    match &x.kind {
        K::Name { .. } => 0,
        K::BinOp { op, left, .. } if op == "^" && left.is_name() => 1,
        K::Quantity { .. } => 2,
        _ => 3,
    }
}

/// (base, power) of a factor: x^n with a number n, else (f, 1) (Python `_base_pow`).
pub fn base_pow(f: &A::Expr) -> (A::Expr, f64) {
    if let K::BinOp { op, left, right, .. } = &f.kind {
        if op == "^" {
            if let Some(v) = right.num_value() {
                return ((**left).clone(), v);
            }
        }
    }
    (f.clone(), 1.0)
}

fn is_unit_quantity(q: &A::Expr) -> bool {
    matches!(&q.kind, K::Quantity { value, .. } if value.num_value() == Some(1.0))
}

fn with_value(q: &A::Expr, v: f64) -> A::Expr {
    let K::Quantity { unit, bracket, .. } = &q.kind else { unreachable!() };
    with_kind(q, K::Quantity { value: Box::new(num(v)), unit: unit.clone(), bracket: *bracket })
}

/// c × Π factors, repeated factors combined into powers, divisors under one '/' (Python `_build_product`).
pub fn build_product(c: f64, fs: &[A::Expr]) -> A::Expr {
    let mut merged: Vec<(String, A::Expr, f64)> = vec![];
    for f in fs {
        let (base, p) = base_pow(f);
        let k = key(&base);
        match merged.iter_mut().find(|m| m.0 == k) {
            Some(m) => m.2 += p,
            None => merged.push((k, base, p)),
        }
    }
    if c == 0.0 {
        return num(0.0);
    }
    let mut top = vec![];
    let mut bottom = vec![];
    for (_, base, p) in merged {
        if p > 0.0 {
            top.push(if p == 1.0 { base } else { pw(base, num(p)) });
        } else if p < 0.0 {
            bottom.push(if p == -1.0 { base } else { pw(base, num(-p)) });
        }
    }
    top.sort_by_key(rank);
    bottom.sort_by_key(rank);
    let sign = if c < 0.0 { -1 } else { 1 };
    let mut c = c.abs();
    let mut den_c = 1.0;
    if c < 1.0 && c > 0.0 {
        let inv = 1.0 / c;
        let r = inv.round_ties_even();
        if (inv - r).abs() < 1e-12 && r <= 1e6 {
            den_c = r;
            c = 1.0;
        }
    }
    let mut out: Option<A::Expr> = None;
    if let Some(i) = top.iter().position(is_unit_quantity).filter(|_| c != 0.0) {
        let q = top.remove(i);
        out = Some(with_value(&q, c)); // 9 m/s³ rather than 9·1 m/s³
    } else if c != 1.0 || top.is_empty() {
        out = Some(num(c));
    }
    for it in top {
        out = Some(match out {
            None => it,
            Some(o) => mul(o, it),
        });
    }
    let mut den: Option<A::Expr> = None;
    let qb = bottom.iter().position(is_unit_quantity);
    if den_c != 1.0 && qb.is_some() {
        let q = bottom.remove(qb.unwrap());
        den = Some(with_value(&q, den_c));
    } else if den_c != 1.0 {
        den = Some(num(den_c));
    }
    for it in bottom {
        den = Some(match den {
            None => it,
            Some(d) => mul(d, it),
        });
    }
    let mut out = out.unwrap();
    if let Some(d) = den {
        out = div(out, d);
    }
    if sign < 0 { neg(out) } else { out }
}

/// Python float arithmetic for folding two numbers (None where Python raises or gives a complex result).
fn fold(op: &str, a: f64, b: f64) -> Option<f64> {
    let v = match op {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => {
            if b == 0.0 {
                return None;
            }
            a / b
        }
        "^" => {
            if a == 0.0 && b < 0.0 {
                return None;
            }
            if a < 0.0 && b != b.trunc() && b.is_finite() {
                return None;
            }
            a.powf(b)
        }
        _ => return None,
    };
    v.is_finite().then_some(v)
}

fn is_sum(e: &A::Expr) -> bool {
    matches!(op_of(e), Some("+") | Some("-"))
}

/// Python `simplify`.
pub fn simplify(e: &A::Expr) -> A::Expr {
    let e = map_children(e, &mut simplify);
    match &e.kind {
        K::Neg { operand } => {
            if let Some(v) = operand.num_value() {
                return num(-v);
            }
            if let K::Neg { operand: o } = &operand.kind {
                return (**o).clone();
            }
            e
        }
        K::BinOp { op, left: a, right: b, .. } => {
            let (a, b) = (&**a, &**b);
            if let (Some(x), Some(y)) = (a.num_value(), b.num_value()) {
                if let Some(v) = fold(op, x, y) {
                    return num(v);
                }
            }
            match op.as_str() {
                "+" => {
                    if is_num_v(a, 0.0) {
                        return b.clone();
                    }
                    if is_num_v(b, 0.0) {
                        return a.clone();
                    }
                    if let K::Neg { operand } = &b.kind {
                        return simplify(&sub(a.clone(), (**operand).clone()));
                    }
                    if let Some(v) = b.num_value() {
                        if v < 0.0 {
                            return sub(a.clone(), num(-v));
                        }
                    }
                    if key(a) == key(b) {
                        return simplify(&mul(num(2.0), a.clone()));
                    }
                    e.clone()
                }
                "-" => {
                    if is_num_v(b, 0.0) {
                        return a.clone();
                    }
                    if is_num_v(a, 0.0) {
                        return simplify(&neg(b.clone()));
                    }
                    if let K::Neg { operand } = &b.kind {
                        return simplify(&add(a.clone(), (**operand).clone()));
                    }
                    if key(a) == key(b) {
                        return num(0.0);
                    }
                    e.clone()
                }
                "*" => {
                    let (c, fs) = factors(&e);
                    build_product(c, &fs)
                }
                "/" => {
                    if is_num_v(b, 1.0) {
                        return a.clone();
                    }
                    if is_num_v(a, 0.0) {
                        return num(0.0);
                    }
                    if key(a) == key(b) {
                        return num(1.0);
                    }
                    if !is_sum(a) && !is_sum(b) {
                        let (c, fs) = factors(&e);
                        return build_product(c, &fs);
                    }
                    if let K::Neg { operand } = &a.kind {
                        return simplify(&neg(div((**operand).clone(), b.clone())));
                    }
                    if let Some(bv) = b.num_value() {
                        if bv != 0.0 {
                            let (ca, fa) = factors(a);
                            return build_product(ca / bv, &fa);
                        }
                    }
                    let (ca, fa) = factors(a);
                    if ca != 1.0 && !fa.is_empty() {
                        let inner = div(build_product(1.0, &fa), b.clone());
                        return if ca != -1.0 { build_product(ca, &[inner]) } else { neg(inner) };
                    }
                    e.clone()
                }
                "^" => {
                    if is_num_v(b, 1.0) {
                        return a.clone();
                    }
                    if is_num_v(b, 0.0) {
                        return num(1.0);
                    }
                    if is_num_v(a, 1.0) {
                        return num(1.0);
                    }
                    if let (K::BinOp { op: aop, left: al, right: ar, .. }, Some(bv)) = (&a.kind, b.num_value()) {
                        if aop == "^" {
                            // (u^a)^b = u^(ab) holds for every real u only when b is an integer, or when a is
                            // not (u^a then needs u >= 0); (x²)^(1/2) is |x|, not x (red team 16, D331)
                            if let Some(av) = ar.num_value().filter(|&av| bv == bv.trunc() || av != av.trunc()) {
                                return simplify(&pw((**al).clone(), num(av * bv)));
                            }
                        }
                    }
                    if let K::Sqrt { operand, root: 2 } = &a.kind {
                        if is_num_v(b, 2.0) {
                            return (**operand).clone();
                        }
                    }
                    e.clone()
                }
                _ => e.clone(),
            }
        }
        K::Sqrt { operand, root } => match &operand.kind {
            K::Num { value, digit: false, .. } if *value >= 0.0 => num(value.powf(1.0 / *root as f64)),
            _ => e,
        },
        K::Call { func, args } if args.len() == 1 && func.is_name() => {
            if let Some(v) = args[0].num_value() {
                let f = func.name().unwrap();
                if matches!(f, "sin" | "tan" | "sinh" | "tanh" | "asin" | "atan") && v == 0.0 {
                    return num(0.0);
                }
                if matches!(f, "cos" | "cosh" | "exp") && v == 0.0 {
                    return num(1.0);
                }
            }
            e
        }
        _ => e,
    }
}

/// The terms of a sum written without parentheses, with their signs (Python `_sum_terms`).
pub fn sum_terms(e: &A::Expr, sign: i32) -> Vec<(i32, A::Expr)> {
    if let K::BinOp { op, left, right, .. } = &e.kind {
        if (op == "+" || op == "-") && !e.paren {
            let mut v = sum_terms(left, sign);
            v.extend(sum_terms(right, if op == "+" { sign } else { -sign }));
            return v;
        }
    }
    if let K::Neg { operand } = &e.kind {
        return sum_terms(operand, -sign);
    }
    vec![(sign, e.clone())]
}

/// The terms of a sum, even if it was written in parentheses (Python `_terms`).
pub fn terms(e: &A::Expr) -> Vec<(i32, A::Expr)> {
    if let K::BinOp { op, left, right, .. } = &e.kind {
        if op == "+" || op == "-" {
            let mut v = sum_terms(left, 1);
            v.extend(sum_terms(right, if op == "+" { 1 } else { -1 }));
            return v;
        }
    }
    vec![(1, e.clone())]
}

pub fn from_terms(ts: &[(i32, A::Expr)]) -> A::Expr {
    let mut out: Option<A::Expr> = None;
    for (s, t) in ts {
        out = Some(match out {
            None => {
                if *s < 0 {
                    neg(t.clone())
                } else {
                    t.clone()
                }
            }
            Some(o) => {
                if *s < 0 {
                    sub(o, t.clone())
                } else {
                    add(o, t.clone())
                }
            }
        });
    }
    out.unwrap_or_else(|| num(0.0))
}

/// Pull factors shared by every term of a sum out front: 2 e^u - 4 x² e^u -> (2 - 4x²) e^u.
pub fn factor_common(e: &A::Expr) -> A::Expr {
    let e = match &e.kind {
        K::Neg { operand } if is_sum(operand) => {
            with_kind(e, K::Neg { operand: Box::new(map_children(operand, &mut factor_common)) })
        }
        _ => map_children(e, &mut factor_common),
    };
    let ts = sum_terms(&e, 1);
    if ts.len() < 2 {
        return e;
    }
    struct T {
        c: f64,
        cnt: Vec<(String, usize)>,
        byk: Vec<(String, A::Expr)>,
        fs: Vec<A::Expr>,
    }
    let mut fl: Vec<T> = vec![];
    for (sign, t) in &ts {
        let (c, fs) = factors(t);
        let fs: Vec<A::Expr> = fs.into_iter().filter(|f| !f.is_num()).collect();
        let mut cnt: Vec<(String, usize)> = vec![];
        let mut byk: Vec<(String, A::Expr)> = vec![];
        for f in &fs {
            let k = key(f);
            match cnt.iter_mut().find(|x| x.0 == k) {
                Some(x) => x.1 += 1,
                None => cnt.push((k.clone(), 1)),
            }
            match byk.iter_mut().find(|x| x.0 == k) {
                Some(x) => x.1 = f.clone(),
                None => byk.push((k, f.clone())),
            }
        }
        fl.push(T { c: *sign as f64 * c, cnt, byk, fs });
    }
    let mut common: Vec<(String, usize)> = fl[0].cnt.clone();
    for t in &fl[1..] {
        common = common
            .into_iter()
            .filter_map(|(k, n)| {
                let m = t.cnt.iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or(0);
                let n = n.min(m);
                (n > 0).then_some((k, n))
            })
            .collect();
    }
    if common.is_empty() {
        return e;
    }
    let mut order: Vec<&T> = fl.iter().collect();
    order.sort_by_key(|t| t.c < 0.0);
    let mut rest: Option<A::Expr> = None;
    for t in order {
        let mut drop = common.clone();
        let mut remaining = vec![];
        for f in &t.fs {
            let k = key(f);
            match drop.iter_mut().find(|x| x.0 == k && x.1 > 0) {
                Some(x) => x.1 -= 1,
                None => remaining.push(f.clone()),
            }
        }
        let p = build_product(t.c.abs(), &remaining);
        rest = Some(match rest {
            None => {
                if t.c < 0.0 {
                    neg(p)
                } else {
                    p
                }
            }
            Some(r) => {
                if t.c < 0.0 {
                    sub(r, p)
                } else {
                    add(r, p)
                }
            }
        });
    }
    let mut out = rest.unwrap();
    for (k, n) in &common {
        let f = fl[0].byk.iter().find(|x| &x.0 == k).unwrap().1.clone();
        for _ in 0..*n {
            out = mul(out, f.clone());
        }
    }
    simplify(&out)
}

/// Cancel the growth of exp(u)/(exp(u) + r)^k and sinh(u)/cosh(u)^k (Python `_stable_product`).
fn stable_product(c: f64, factors_: &[A::Expr]) -> Option<A::Expr> {
    let mut fs: Vec<(A::Expr, f64)> = factors_.iter().map(base_pow).collect();
    let mut changed = false;
    let mut i = 0;
    while i < fs.len() {
        let (bi, pi) = fs[i].clone();
        if pi <= 0.0 {
            i += 1;
            continue;
        }
        let mut j = 0;
        while j < fs.len() {
            let (bj, pj) = fs[j].clone();
            if pj >= 0.0 || fs[i].1 <= 0.0 {
                j += 1;
                continue;
            }
            let n = fs[i].1.min(-pj);
            if n != n.trunc() {
                j += 1;
                continue;
            }
            let mut new = None;
            if is_call(&bi, "sinh") && is_call(&bj, "cosh") && key(&call_parts(&bi).unwrap().1[0]) == key(&call_parts(&bj).unwrap().1[0]) {
                new = Some(call1("tanh", call_parts(&bi).unwrap().1[0].clone()));
            } else if is_call(&bi, "exp") {
                let ts = terms(&bj);
                let kb = key(&bi);
                if let Some(h) = ts.iter().position(|(_, t)| key(t) == kb) {
                    if ts.len() > 1 {
                        let s = ts[h].0;
                        let mut others = ts.clone();
                        others.remove(h);
                        let rest = from_terms(&others);
                        let u = call_parts(&bi).unwrap().1[0].clone();
                        new = Some(div(num(1.0), add(num(s as f64), mul(rest, call1("exp", neg(u))))));
                    }
                }
            }
            if let Some(nw) = new {
                fs[i].1 -= n;
                fs[j].1 += n;
                fs.push((nw, n));
                changed = true;
            }
            j += 1;
        }
        i += 1;
    }
    if !changed {
        return None;
    }
    let parts: Vec<A::Expr> =
        fs.into_iter().filter(|(_, p)| *p != 0.0).map(|(b, p)| if p == 1.0 { b } else { pw(b, num(p)) }).collect();
    Some(build_product(c, &parts))
}

/// (a b)^n -> a^n b^n for whole n, so every factor is a single base (Python `_expand_factors`).
fn expand_factors(mut c: f64, fs: Vec<A::Expr>) -> (f64, Vec<A::Expr>) {
    let mut out = vec![];
    for f in fs {
        let (b, p) = base_pow(&f);
        let prodlike = matches!(&b.kind, K::Neg { .. }) || matches!(op_of(&b), Some("*") | Some("/"));
        if p == p.trunc() && p != 1.0 && prodlike {
            let (c0, f0) = factors(&b);
            let (cb, fb) = expand_factors(c0, f0);
            c *= cb.powf(p);
            for g in fb {
                let (gb, gp) = base_pow(&g);
                out.push(pw(gb, num(gp * p)));
            }
        } else {
            out.push(f);
        }
    }
    (c, out)
}

/// Rewrite a derivative for evaluation so it doesn't overflow to ∞/∞ = NaN where the true value is finite
/// (Python `stabilize`; the printed form is unchanged).
pub fn stabilize(e: &A::Expr) -> A::Expr {
    let e = map_children(e, &mut stabilize);
    let ok = matches!(op_of(&e), Some("*") | Some("/")) || matches!(e.kind, K::Neg { .. });
    if !ok {
        return e;
    }
    let (c0, f0) = factors(&e);
    let (c, fs) = expand_factors(c0, f0);
    if !fs.iter().any(|f| base_pow(f).1 < 0.0) {
        return e;
    }
    if let Some(r) = stable_product(c, &fs) {
        return r;
    }
    let sums: Vec<usize> =
        (0..fs.len()).filter(|&k| base_pow(&fs[k]).1 == 1.0 && terms(&fs[k]).len() > 1).collect();
    if sums.len() == 1 {
        let mut others = fs.clone();
        others.remove(sums[0]);
        let mut parts = vec![];
        let mut hit = false;
        for (s, t) in terms(&fs[sums[0]]) {
            let (ct, mut ft) = factors(&t);
            ft.extend(others.iter().cloned());
            let cc = s as f64 * c * ct;
            let p = stable_product(cc, &ft);
            hit = hit || p.is_some();
            parts.push((1, p.unwrap_or_else(|| build_product(cc, &ft))));
        }
        if hit {
            return from_terms(&parts);
        }
    }
    e
}
