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
                match signsimp(f) {
                    T::Add(ts) if could_extract_minus(&ts) => {
                        c = -c;
                        out.push(flip_add(ts));
                    }
                    T::Pow(b, e) if is_int(e) && matches!(&*b, T::Add(ts) if could_extract_minus(ts)) => {
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
    let t = if cancels(&t) { T::Num(0.0) } else { t };
    let t = signsimp(t);
    let out = simplify(&from_t(&t));
    if to_source(&out).chars().count() < to_source(e).chars().count() { out } else { e.clone() }
}
