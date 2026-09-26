//! A shorter equivalent formula for display (calculus.sympy_tidy, which asked SymPy's `simplify`). Native:
//! the formula is put in a canonical product-of-powers form (like SymPy's automatic Mul/Add/Pow
//! evaluation: equal bases combined, whole powers of products distributed), sums that cancel exactly are found
//! by putting them over a common denominator and expanding, and the sign of each sum is made canonical (like
//! SymPy's `signsimp`). The result is written back in SymPy's argument order (Basic.compare) and, as in v1,
//! used only when it is shorter than the formula it replaces. The value is the same either way.
use std::cmp::Ordering;
use std::collections::BTreeMap;

use fermium_syntax::ast as A;

use crate::build::*;
use crate::simplify::simplify;
use crate::source::to_source;

type K = A::ExprKind;
/// A rational exponent (numerator, denominator > 0), reduced.
type Q = (i64, i64);

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

fn q(n: i64, d: i64) -> Q {
    let g = gcd(n, d).max(1);
    let (n, d) = (n / g, d / g);
    if d < 0 { (-n, -d) } else { (n, d) }
}

fn qadd(a: Q, b: Q) -> Q {
    q(a.0 * b.1 + b.0 * a.1, a.1 * b.1)
}

fn qmul(a: Q, b: Q) -> Q {
    q(a.0 * b.0, a.1 * b.1)
}

fn qf(a: Q) -> f64 {
    a.0 as f64 / a.1 as f64
}

fn is_int(a: Q) -> bool {
    a.1 == 1
}

#[derive(Clone, Debug)]
enum T {
    Num(f64),
    Sym(String),
    Fun(String, Vec<T>),
    /// base (Sym | Fun | Add), exponent ≠ 1
    Pow(Box<T>, Q),
    /// coefficient, factors (Sym | Fun | Pow | Add)
    Mul(f64, Vec<T>),
    /// terms (never Add), at least two
    Add(Vec<T>),
}

/// The functions sympy_tidy could convert (calculus.SYMPY_FUNCS), with the name it converts back to.
fn fun_name(f: &str) -> Option<&'static str> {
    Some(match f {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "sec" => "sec",
        "csc" => "csc",
        "exp" => "exp",
        "ln" | "log" => "ln",
        "sinh" => "sinh",
        "cosh" => "cosh",
        "tanh" => "tanh",
        "asin" => "asin",
        "acos" => "acos",
        "atan" => "atan",
        "abs" => "abs",
        "asinh" => "asinh",
        "acosh" => "acosh",
        "atanh" => "atanh",
        "erf" => "erf",
        "erfc" => "erfc",
        "sign" => "sign",
        _ => return None,
    })
}

fn key(t: &T) -> String {
    match t {
        T::Num(v) => format!("{v:e}"),
        T::Sym(s) => s.clone(),
        T::Fun(f, a) => format!("{f}({})", a.iter().map(key).collect::<Vec<_>>().join(",")),
        T::Pow(b, e) => format!("({})^{}/{}", key(b), e.0, e.1),
        T::Mul(c, fs) => {
            let mut ks: Vec<String> = fs.iter().map(key).collect();
            ks.sort();
            format!("{c:e}*{}", ks.join("*"))
        }
        T::Add(ts) => {
            let mut ks: Vec<String> = ts.iter().map(key).collect();
            ks.sort();
            format!("[{}]", ks.join("+"))
        }
    }
}

/// (coefficient, base, exponent) pieces of a product.
fn mul_all(items: Vec<T>) -> T {
    let mut coef = 1.0;
    let mut facs: Vec<(String, T, Q)> = vec![];
    fn push(facs: &mut Vec<(String, T, Q)>, b: T, e: Q) {
        let k = key(&b);
        match facs.iter_mut().find(|f| f.0 == k) {
            Some(f) => f.2 = qadd(f.2, e),
            None => facs.push((k, b, e)),
        }
    }
    for it in items {
        match it {
            T::Num(v) => coef *= v,
            T::Mul(c, fs) => {
                coef *= c;
                for f in fs {
                    match f {
                        T::Pow(b, e) => push(&mut facs, *b, e),
                        other => push(&mut facs, other, (1, 1)),
                    }
                }
            }
            T::Pow(b, e) => push(&mut facs, *b, e),
            other => push(&mut facs, other, (1, 1)),
        }
    }
    if coef == 0.0 {
        return T::Num(0.0);
    }
    let fs: Vec<T> = facs
        .into_iter()
        .filter(|f| f.2 .0 != 0)
        .map(|(_, b, e)| if e == (1, 1) { b } else { T::Pow(Box::new(b), e) })
        .collect();
    if fs.is_empty() {
        return T::Num(coef);
    }
    if coef == 1.0 && fs.len() == 1 {
        return fs.into_iter().next().unwrap();
    }
    if coef == -1.0 && fs.len() > 1 && fs.iter().filter(|f| matches!(f, T::Add(_))).count() == 1 {
        // -(x² + y²)/σ²: the sign goes into the sum, (-x² - y²)/σ²
        let mut fs = fs;
        let i = fs.iter().position(|f| matches!(f, T::Add(_))).unwrap();
        let T::Add(ts) = fs.remove(i) else { unreachable!() };
        fs.push(add_all(ts.into_iter().map(|t| mul_all(vec![T::Num(-1.0), t])).collect()));
        return T::Mul(1.0, fs);
    }
    if fs.len() == 1 && matches!(fs[0], T::Add(_)) {
        // SymPy's Mul(number, sum) distributes: 2 (x + z) is 2x + 2z
        let T::Add(ts) = fs.into_iter().next().unwrap() else { unreachable!() };
        return add_all(ts.into_iter().map(|t| mul_all(vec![T::Num(coef), t])).collect());
    }
    T::Mul(coef, fs)
}

/// t^e, or None where SymPy wouldn't combine (a non-whole power of a product or of a power).
fn pow_q(t: T, e: Q) -> Option<T> {
    if e == (1, 1) {
        return Some(t);
    }
    if e.0 == 0 {
        return Some(T::Num(1.0));
    }
    Some(match t {
        T::Num(v) => {
            if is_int(e) {
                T::Num(v.powi(e.0 as i32))
            } else {
                return None;
            }
        }
        T::Pow(b, p) => {
            if !is_int(e) {
                return None;
            }
            let ne = qmul(p, e);
            if ne == (1, 1) { *b } else { T::Pow(b, ne) }
        }
        T::Mul(c, fs) => {
            if !is_int(e) {
                return None;
            }
            let mut items = vec![T::Num(c.powi(e.0 as i32))];
            for f in fs {
                items.push(pow_q(f, e)?);
            }
            mul_all(items)
        }
        other => T::Pow(Box::new(other), e),
    })
}

/// (coefficient, monomial) of a term.
fn split_term(t: T) -> (f64, Option<T>) {
    match t {
        T::Num(v) => (v, None),
        T::Mul(c, fs) => (c, Some(if fs.len() == 1 { fs.into_iter().next().unwrap() } else { T::Mul(1.0, fs) })),
        other => (1.0, Some(other)),
    }
}

fn add_all(items: Vec<T>) -> T {
    let mut terms: Vec<(String, f64, Option<T>)> = vec![];
    let mut flat = vec![];
    for it in items {
        match it {
            T::Add(ts) => flat.extend(ts),
            other => flat.push(other),
        }
    }
    for t in flat {
        let (c, m) = split_term(t);
        let k = m.as_ref().map(key).unwrap_or_default();
        match terms.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 += c,
            None => terms.push((k, c, m)),
        }
    }
    let ts: Vec<T> = terms
        .into_iter()
        .filter(|t| t.1 != 0.0)
        .map(|(_, c, m)| match m {
            None => T::Num(c),
            Some(m) => mul_all(vec![T::Num(c), m]),
        })
        .collect();
    match ts.len() {
        0 => T::Num(0.0),
        1 => ts.into_iter().next().unwrap(),
        _ => T::Add(ts),
    }
}

fn rational(v: f64) -> Option<Q> {
    for d in 1..=99i64 {
        let n = (v * d as f64).round();
        if (n / d as f64 - v).abs() < 1e-12 {
            return Some(q(n as i64, d));
        }
    }
    None
}

/// The AST as a canonical term, or None where v1's to_sympy would have failed (quantities, lists, …).
fn to_t(e: &A::Expr) -> Option<T> {
    Some(match &e.kind {
        K::Num { value, .. } => T::Num(*value),
        K::Name { name } => T::Sym(name.clone()),
        K::BinOp { op, left, right, .. } => {
            let (a, b) = (to_t(left)?, to_t(right)?);
            match op.as_str() {
                "+" => add_all(vec![a, b]),
                "-" => add_all(vec![a, mul_all(vec![T::Num(-1.0), b])]),
                "*" => mul_all(vec![a, b]),
                "/" => mul_all(vec![a, pow_q(b, (-1, 1))?]),
                "^" => {
                    let T::Num(p) = b else { return None };
                    pow_q(a, rational(p)?)?
                }
                _ => return None,
            }
        }
        K::Neg { operand } => mul_all(vec![T::Num(-1.0), to_t(operand)?]),
        K::Sqrt { operand, root } => pow_q(to_t(operand)?, (1, *root))?,
        K::Abs { operand } => T::Fun("abs".into(), vec![to_t(operand)?]),
        K::Call { func, args } if args.len() == 1 => {
            let f = fun_name(func.name()?)?;
            if func.name() == Some("sqrt") {
                return pow_q(to_t(&args[0])?, (1, 2));
            }
            T::Fun(f.into(), vec![to_t(&args[0])?])
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------- exact cancellation of sums

/// A polynomial over atoms: monomial (sorted (atom key, exponent)) → coefficient. Powers of sums with a
/// negative or fractional exponent are atoms too (their bases are kept in `sums` for expanding later).
type Poly = BTreeMap<Vec<(String, Q)>, f64>;

fn poly_of(t: &T, sums: &mut Vec<(String, T)>) -> Option<Poly> {
    let mut p = Poly::new();
    match t {
        T::Num(v) => {
            p.insert(vec![], *v);
        }
        T::Add(ts) => {
            for x in ts {
                for (m, c) in poly_of(x, sums)? {
                    *p.entry(m).or_insert(0.0) += c;
                }
            }
        }
        T::Mul(c, fs) => {
            let mut acc = Poly::new();
            acc.insert(vec![], *c);
            for f in fs {
                acc = poly_mul(&acc, &poly_of(f, sums)?);
            }
            p = acc;
        }
        T::Pow(b, e) => {
            if let T::Add(_) = **b {
                if is_int(*e) && e.0 >= 0 && e.0 <= 8 {
                    let base = poly_of(b, sums)?;
                    let mut acc = Poly::new();
                    acc.insert(vec![], 1.0);
                    for _ in 0..e.0 {
                        acc = poly_mul(&acc, &base);
                    }
                    p = acc;
                } else {
                    let k = key(b);
                    if !sums.iter().any(|x| x.0 == k) {
                        sums.push((k.clone(), (**b).clone()));
                    }
                    p.insert(vec![(k, *e)], 1.0);
                }
            } else {
                p.insert(vec![(key(b), *e)], 1.0);
            }
        }
        other => {
            p.insert(vec![(key(other), (1, 1))], 1.0);
        }
    }
    Some(p)
}

fn poly_mul(a: &Poly, b: &Poly) -> Poly {
    let mut out = Poly::new();
    for (ma, ca) in a {
        for (mb, cb) in b {
            let mut m: Vec<(String, Q)> = ma.clone();
            for (k, e) in mb {
                match m.iter_mut().find(|x| &x.0 == k) {
                    Some(x) => x.1 = qadd(x.1, *e),
                    None => m.push((k.clone(), *e)),
                }
            }
            m.retain(|x| x.1 .0 != 0);
            m.sort();
            *out.entry(m).or_insert(0.0) += ca * cb;
        }
    }
    out
}

/// Is a sum exactly 0? Expand it, multiply by the lowest power of each sum that appears as an atom (a common
/// denominator), expand those sums, and look at the coefficients.
fn cancels(t: &T) -> bool {
    if !matches!(t, T::Add(_)) {
        return false;
    }
    let mut sums: Vec<(String, T)> = vec![];
    let Some(mut p) = poly_of(t, &mut sums) else { return false };
    let scale0 = p.values().fold(0.0f64, |a, c| a.max(c.abs()));
    if p.values().all(|c| c.abs() <= 1e-12 * scale0) {
        return true; // it cancels term by term once expanded
    }
    for _round in 0..4 {
        let present: Vec<(String, T)> =
            sums.iter().filter(|(k, _)| p.keys().any(|m| m.iter().any(|x| &x.0 == k))).cloned().collect();
        if present.is_empty() {
            break;
        }
        for (k, base) in present {
            let min = p.keys()
                .map(|m| m.iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or((0, 1)))
                .fold((0, 1), |a, e| if qf(e) < qf(a) { e } else { a });
            let mut next = Poly::new();
            for (m, c) in &p {
                let e = m.iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or((0, 1));
                let e = qadd(e, (-min.0, min.1));
                if !is_int(e) || e.0 > 8 {
                    return false;
                }
                let mut rest: Vec<(String, Q)> = m.iter().filter(|x| x.0 != k).cloned().collect();
                rest.sort();
                let mut term = Poly::new();
                term.insert(rest, *c);
                let Some(bp) = poly_of(&base, &mut sums) else { return false };
                for _ in 0..e.0 {
                    term = poly_mul(&term, &bp);
                }
                for (mm, cc) in term {
                    *next.entry(mm).or_insert(0.0) += cc;
                }
            }
            p = next;
        }
    }
    if sums.iter().any(|(k, _)| p.keys().any(|m| m.iter().any(|x| &x.0 == k))) {
        return false;
    }
    let scale = p.values().fold(scale0, |a, c| a.max(c.abs()));
    scale > 0.0 && p.values().all(|c| c.abs() <= 1e-12 * scale)
}

/// A sum that is c·B^k for one of the sums B it contains, once over a common denominator: GMm·(2√Q − 3x²/√Q −
/// 3y²/√Q) with Q = x² + y² is −GMm·√Q (∇² of a 1/r potential in the plane).
fn proportional_sum(t: &T) -> Option<T> {
    let mut sums: Vec<(String, T)> = vec![];
    let p = poly_of(t, &mut sums)?;
    if sums.is_empty() {
        return None;
    }
    // shift every sum base to its lowest power, expanding what is left
    let mut mins: Vec<(String, T, Q)> = vec![];
    for (k, b) in &sums {
        let min = p.keys()
            .map(|m| m.iter().find(|x| &x.0 == k).map(|x| x.1).unwrap_or((0, 1)))
            .fold((0, 1), |a, e| if qf(e) < qf(a) { e } else { a });
        mins.push((k.clone(), b.clone(), min));
    }
    let mut n = Poly::new();
    for (m, c) in &p {
        let mut term = Poly::new();
        let rest: Vec<(String, Q)> = m.iter().filter(|x| !sums.iter().any(|s| s.0 == x.0)).cloned().collect();
        term.insert(rest, *c);
        for (k, b, min) in &mins {
            let e = qadd(m.iter().find(|x| &x.0 == k).map(|x| x.1).unwrap_or((0, 1)), (-min.0, min.1));
            if !is_int(e) || e.0 < 0 || e.0 > 8 {
                return None;
            }
            let bp = poly_of(b, &mut vec![])?;
            for _ in 0..e.0 {
                term = poly_mul(&term, &bp);
            }
        }
        for (mm, cc) in term {
            *n.entry(mm).or_insert(0.0) += cc;
        }
    }
    let scale = n.values().fold(0.0f64, |a, c| a.max(c.abs()));
    n.retain(|_, c| c.abs() > 1e-12 * scale);
    if n.is_empty() {
        return None;
    }
    for (k, b, _) in &mins {
        let bp = poly_of(b, &mut vec![])?;
        let mut pj = bp.clone();
        for j in 1..=3i64 {
            if pj.len() == n.len() && pj.keys().all(|m| n.contains_key(m)) {
                let (m0, c0) = pj.iter().next().unwrap();
                let ratio = n[m0] / c0;
                if pj.iter().all(|(m, c)| (n[m] - ratio * c).abs() <= 1e-12 * scale) {
                    let mut items = vec![T::Num(ratio)];
                    for (k2, b2, min2) in &mins {
                        let e = if k2 == k { qadd(*min2, (j, 1)) } else { *min2 };
                        items.push(pow_q(b2.clone(), e)?);
                    }
                    return Some(mul_all(items));
                }
            }
            pj = poly_mul(&pj, &bp);
        }
        let _ = k;
    }
    None
}

/// Replace every sum that cancels exactly by 0 (a product containing one is then 0).
fn zero_sums(t: T) -> T {
    match t {
        T::Add(ts) => {
            let t = add_all(ts.into_iter().map(zero_sums).collect());
            if cancels(&t) {
                return T::Num(0.0);
            }
            proportional_sum(&t).unwrap_or(t)
        }
        T::Mul(c, fs) => {
            let mut items = vec![T::Num(c)];
            items.extend(fs.into_iter().map(zero_sums));
            mul_all(items)
        }
        T::Pow(b, e) => {
            let b = zero_sums(*b);
            pow_q(b.clone(), e).unwrap_or(T::Pow(Box::new(b), e))
        }
        T::Fun(f, a) => T::Fun(f, a.into_iter().map(zero_sums).collect()),
        other => other,
    }
}

// ---------------------------------------------------------------- signs (SymPy's signsimp)

fn negative(t: &T) -> bool {
    match t {
        T::Num(v) => *v < 0.0,
        T::Mul(c, _) => *c < 0.0,
        _ => false,
    }
}

fn first_symbol(t: &T) -> Option<String> {
    match t {
        T::Num(_) => None,
        T::Sym(s) => Some(s.clone()),
        T::Fun(_, a) => a.iter().filter_map(first_symbol).min(),
        T::Pow(b, _) => first_symbol(b),
        T::Mul(_, fs) => fs.iter().filter_map(first_symbol).min(),
        T::Add(ts) => ts.iter().filter_map(first_symbol).min(),
    }
}

/// SymPy's Add.could_extract_minus_sign: more negative terms than positive ones; on a tie, the term that
/// comes first (by its first variable; numbers last) is negative.
fn could_extract_minus(ts: &[T]) -> bool {
    let neg = ts.iter().filter(|t| negative(t)).count();
    let pos = ts.len() - neg;
    if pos != neg {
        return neg > pos;
    }
    let first = ts.iter().min_by(|a, b| match (first_symbol(a), first_symbol(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    first.is_some_and(negative)
}

fn neg_t(t: T) -> T {
    mul_all(vec![T::Num(-1.0), t])
}

fn flip_add(ts: Vec<T>) -> T {
    add_all(ts.into_iter().map(neg_t).collect())
}

fn signsimp(t: T) -> T {
    match t {
        T::Add(ts) => add_all(ts.into_iter().map(signsimp).collect()),
        T::Fun(f, a) => T::Fun(f, a.into_iter().map(signsimp).collect()),
        T::Pow(b, e) => {
            let b = signsimp(*b);
            match b {
                T::Add(ts) if is_int(e) && e.0 % 2 == 0 && could_extract_minus(&ts) => {
                    T::Pow(Box::new(flip_add(ts)), e)
                }
                other => T::Pow(Box::new(other), e),
            }
        }
        T::Mul(c, fs) => {
            let mut c = c;
            let mut out = vec![];
            for f in fs {
                // an odd power of a sum is flipped only to take a minus sign off the coefficient
                match signsimp(f) {
                    T::Add(ts) if c < 0.0 && could_extract_minus(&ts) => {
                        c = -c;
                        out.push(flip_add(ts));
                    }
                    T::Pow(b, e) if is_int(e) && (e.0 % 2 == 0 || c < 0.0)
                        && matches!(&*b, T::Add(ts) if could_extract_minus(ts)) => {
                        let T::Add(ts) = *b else { unreachable!() };
                        if e.0 % 2 != 0 {
                            c = -c;
                        }
                        out.push(T::Pow(Box::new(flip_add(ts)), e));
                    }
                    other => out.push(other),
                }
            }
            let mut items = vec![T::Num(c)];
            items.extend(out);
            mul_all(items)
        }
        other => other,
    }
}

/// Put a sum with fractions over a common denominator of plain atoms (SymPy's together, for the simple case):
/// 1 + y²/x² → (x² + y²)/x².
fn together(t: T) -> T {
    match t {
        T::Add(ts) => {
            let ts: Vec<T> = ts.into_iter().map(together).collect();
            // the most negative power of each non-sum atom over the terms
            let mut dens: Vec<(String, T, Q)> = vec![];
            for x in &ts {
                let fs: Vec<T> = match x {
                    T::Mul(_, fs) => fs.clone(),
                    o => vec![o.clone()],
                };
                for f in fs {
                    if let T::Pow(b, e) = f {
                        if qf(e) < 0.0 && !matches!(*b, T::Add(_)) {
                            let k = key(&b);
                            match dens.iter_mut().find(|d| d.0 == k) {
                                Some(d) => {
                                    if qf(e) < qf(d.2) {
                                        d.2 = e;
                                    }
                                }
                                None => dens.push((k, *b, e)),
                            }
                        }
                    }
                }
            }
            if dens.is_empty() || ts.len() < 2 {
                return add_all(ts);
            }
            let mut num_terms = vec![];
            for x in ts {
                let mut items = vec![x];
                for (_, b, e) in &dens {
                    match pow_q(b.clone(), (-e.0, e.1)) {
                        Some(p) => items.push(p),
                        None => return T::Num(f64::NAN),
                    }
                }
                num_terms.push(mul_all(items));
            }
            let mut items = vec![add_all(num_terms)];
            for (_, b, e) in dens {
                items.push(T::Pow(Box::new(b), e));
            }
            mul_all(items)
        }
        T::Mul(c, fs) => {
            let mut items = vec![T::Num(c)];
            items.extend(fs.into_iter().map(together));
            mul_all(items)
        }
        T::Pow(b, e) => pow_q(together(*b), e).unwrap_or(T::Num(f64::NAN)),
        T::Fun(f, a) => T::Fun(f, a.into_iter().map(together).collect()),
        other => other,
    }
}

/// 4x² + 4y² − 4σ² → 4·(x² + y² − σ²): the whole-number content of a sum (kept unevaluated, like SymPy's
/// factor_terms does with _keep_coeff).
fn numeric_content(t: T) -> T {
    let T::Add(ts) = &t else { return t };
    let cs: Vec<f64> = ts.iter().map(|x| split_term(x.clone()).0).collect();
    if cs.iter().any(|c| *c != c.trunc() || c.abs() > 1e15) {
        return t;
    }
    let g = cs.iter().fold(0i64, |g, c| gcd(g, *c as i64));
    if g <= 1 {
        return t;
    }
    let T::Add(ts) = t else { unreachable!() };
    let rest = add_all(ts.into_iter().map(|x| mul_all(vec![T::Num(1.0 / g as f64), x])).collect());
    T::Mul(g as f64, vec![rest])
}

/// Take the factors every term of a sum shares out of it (SymPy's factor_terms): -y² cos(x y) - x² cos(x y)
/// → (-x² - y²)·cos(x y).
fn factor_terms(t: T) -> T {
    match t {
        T::Add(ts) => {
            let ts: Vec<T> = ts.into_iter().map(factor_terms).collect();
            let facs = |x: &T| -> Vec<(String, T, Q)> {
                let fs: Vec<T> = match x {
                    T::Mul(_, fs) => fs.clone(),
                    T::Num(_) => vec![],
                    o => vec![o.clone()],
                };
                fs.into_iter()
                    .map(|f| match f {
                        T::Pow(b, e) => (key(&b), *b, e),
                        o => (key(&o), o, (1, 1)),
                    })
                    .collect()
            };
            let mut common = facs(&ts[0]);
            for x in &ts[1..] {
                let fx = facs(x);
                common = common
                    .into_iter()
                    .filter_map(|(k, b, e)| {
                        let (_, _, e2) = fx.iter().find(|f| f.0 == k)?;
                        let (a, c) = (qf(e), qf(*e2));
                        if a * c <= 0.0 {
                            return None;
                        }
                        let m = if a > 0.0 { if a <= c { e } else { *e2 } } else if a >= c { e } else { *e2 };
                        Some((k, b, m))
                    })
                    .collect();
            }
            if common.is_empty() {
                return numeric_content(add_all(ts));
            }
            let inv: Vec<T> = common.iter().filter_map(|(_, b, e)| pow_q(b.clone(), (-e.0, e.1))).collect();
            if inv.len() != common.len() {
                return add_all(ts);
            }
            let rest: Vec<T> = ts.into_iter().map(|x| {
                let mut items = vec![x];
                items.extend(inv.iter().cloned());
                mul_all(items)
            }).collect();
            let mut items: Vec<T> = common.into_iter().map(|(_, b, e)| pow_q(b, e).unwrap()).collect();
            match numeric_content(add_all(rest)) {
                T::Mul(g, fs) if fs.len() == 1 && matches!(fs[0], T::Add(_)) => {
                    items.push(T::Num(g));
                    items.extend(fs);
                }
                other => items.push(other),
            }
            mul_all(items)
        }
        T::Mul(c, fs) => {
            let mut items = vec![T::Num(c)];
            items.extend(fs.into_iter().map(factor_terms));
            mul_all(items)
        }
        T::Pow(b, e) => pow_q(factor_terms(*b), e).unwrap_or(T::Num(f64::NAN)),
        T::Fun(f, a) => T::Fun(f, a.into_iter().map(factor_terms).collect()),
        other => other,
    }
}

// ---------------------------------------------------------------- back to the AST, in SymPy's order

/// SymPy's ordering_of_classes index (Basic.compare): numbers, π, 𝑖, symbols, Pow, Mul, Add, then functions.
fn class_rank(t: &T) -> (u32, String) {
    match t {
        T::Num(v) => (if *v == v.trunc() { 7 } else { 8 }, String::new()),
        T::Sym(s) if s == "π" => (11, String::new()),
        T::Sym(s) if s == "𝑖" => (12, String::new()),
        T::Sym(_) => (13, String::new()),
        T::Pow(..) => (15, String::new()),
        T::Mul(..) => (16, String::new()),
        T::Add(..) => (17, String::new()),
        T::Fun(f, _) => (100, if f == "abs" { "Abs".into() } else { f.clone() }),
    }
}

/// The args of a term as SymPy stores them (for Mul: the coefficient first when it isn't 1).
fn args(t: &T) -> Vec<T> {
    match t {
        T::Mul(c, fs) => {
            let mut v = vec![];
            if *c != 1.0 {
                v.push(T::Num(*c));
            }
            v.extend(sorted(fs.clone()));
            v
        }
        T::Add(ts) => sorted(ts.clone()),
        T::Fun(_, a) => a.clone(),
        T::Pow(b, e) => vec![(**b).clone(), T::Num(qf(*e))],
        _ => vec![],
    }
}

/// Basic.compare.
fn compare(a: &T, b: &T) -> Ordering {
    let (ra, rb) = (class_rank(a), class_rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (T::Num(x), T::Num(y)) => x.total_cmp(y),
        (T::Sym(x), T::Sym(y)) => x.cmp(y),
        _ => {
            let (aa, ab) = (args(a), args(b));
            if aa.len() != ab.len() {
                return aa.len().cmp(&ab.len());
            }
            for (x, y) in aa.iter().zip(&ab) {
                let c = compare(x, y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            Ordering::Equal
        }
    }
}

fn sorted(mut v: Vec<T>) -> Vec<T> {
    v.sort_by(compare);
    v
}

fn num_ast(v: f64) -> A::Expr {
    num(v)
}

/// calculus.from_sympy.
fn from_t(t: &T) -> A::Expr {
    match t {
        T::Num(v) => num_ast(*v),
        T::Sym(s) => name(s),
        T::Fun(f, a) => call(f, a.iter().map(from_t).collect()),
        T::Pow(b, e) => {
            if *e == (1, 2) {
                return sqrt(from_t(b), 2);
            }
            if e.0 < 0 {
                return div(num(1.0), from_t(&T::Pow(b.clone(), (-e.0, e.1))));
            }
            let ex = if is_int(*e) { num(e.0 as f64) } else { div(num(e.0 as f64), num(e.1 as f64)) };
            pw(from_t(b), ex)
        }
        T::Mul(..) => {
            let a = args(t);
            let mut out = from_t(&a[0]);
            for x in &a[1..] {
                match x {
                    T::Pow(b, e) if e.0 < 0 => out = div(out, from_t(&T::Pow(b.clone(), (-e.0, e.1)))),
                    _ => out = mul(out, from_t(x)),
                }
            }
            out
        }
        T::Add(_) => {
            let a = args(t);
            let mut out = from_t(&a[0]);
            for x in &a[1..] {
                out = add(out, from_t(x));
            }
            out
        }
    }
}

/// calculus.sympy_tidy: a shorter equivalent formula, or `e` itself.
pub fn tidy(e: &A::Expr) -> A::Expr {
    let Some(t) = to_t(e) else { return e.clone() };
    let t = zero_sums(t);
    if matches!(t, T::Num(v) if v == 0.0) {
        return shorter(simplify(&from_t(&T::Num(0.0))), e);
    }
    // the plain canonical form, and the one with fractions put together; the shortest wins (like simplify)
    let mut best = e.clone();
    for cand in [zero_sums(factor_terms(t.clone())), zero_sums(factor_terms(together(t)))] {
        if key(&cand).contains("NaN") {
            continue;
        }
        let out = simplify(&from_t(&signsimp(cand)));
        best = shorter(out, &best);
    }
    best
}

fn shorter(out: A::Expr, e: &A::Expr) -> A::Expr {
    if to_source(&out).chars().count() < to_source(e).chars().count() { out } else { e.clone() }
}

