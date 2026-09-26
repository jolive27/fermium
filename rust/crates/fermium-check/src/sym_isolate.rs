//! TEMPORARY: a self-contained port of the parts of `fermium/calculus.py` that `solve` needs — `to_source`,
//! `map_children`, `subst`, `simplify`, `_d` (with the plain context: no user functions, no solutions),
//! `isolate` and `linear_coeffs`. The calculus agent owns calculus.py and ports it into `crates/fermium-sym`;
//! when that lands, solve.rs switches to fermium-sym and this file is deleted (a small swap at merge time).
//! Operation order follows the Python exactly, since the isolated right-hand side is what gets evaluated.
use fermium_syntax::ast as A;
use fermium_syntax::ast::ExprKind as K;

/// An error of the symbolic code: (message, the node's span, hint) — Python FermiumError(msg, line, col).
#[derive(Debug, Clone)]
pub struct SymError {
    pub message: String,
    pub span: A::Span,
    pub hint: Option<String>,
}

fn serr(message: impl Into<String>, span: A::Span) -> SymError {
    SymError { message: message.into(), span, hint: None }
}

// ---------------------------------------------------------------- constructors
const NEW_ID: u32 = 3_000_000_000;

pub fn mk(kind: K) -> A::Expr {
    A::Expr { id: NEW_ID, kind, span: A::Span::default(), paren: false, attrs: A::Attrs::default() }
}

/// Python `.at(e)`: the position of e.
pub fn at(mut n: A::Expr, e: &A::Expr) -> A::Expr {
    n.span = e.span;
    n
}

pub fn num(v: f64) -> A::Expr {
    mk(K::Num { value: v, sigfigs: None, digit: false })
}

pub fn name(n: &str) -> A::Expr {
    mk(K::Name { name: n.to_string() })
}

pub fn call(f: &str, args: Vec<A::Expr>) -> A::Expr {
    mk(K::Call { func: Box::new(name(f)), args })
}

fn bin(op: &str, a: A::Expr, b: A::Expr, implicit: bool) -> A::Expr {
    mk(K::BinOp { op: op.to_string(), left: Box::new(a), right: Box::new(b), implicit })
}

pub fn add(a: A::Expr, b: A::Expr) -> A::Expr {
    bin("+", a, b, false)
}
pub fn sub(a: A::Expr, b: A::Expr) -> A::Expr {
    bin("-", a, b, false)
}
pub fn mul(a: A::Expr, b: A::Expr) -> A::Expr {
    bin("*", a, b, true)
}
pub fn div(a: A::Expr, b: A::Expr) -> A::Expr {
    bin("/", a, b, false)
}
pub fn pw(a: A::Expr, b: A::Expr) -> A::Expr {
    bin("^", a, b, false)
}
pub fn neg(a: A::Expr) -> A::Expr {
    mk(K::Neg { operand: Box::new(a) })
}

pub fn is_num(e: &A::Expr, v: Option<f64>) -> bool {
    match &e.kind {
        K::Num { value, .. } => v.is_none_or(|x| *value == x),
        _ => false,
    }
}

fn numv(e: &A::Expr) -> f64 {
    e.num_value().unwrap_or(f64::NAN)
}

// ---------------------------------------------------------------- substitution
/// Python map_children: a copy of e with f applied to each child (only the kinds Python maps).
pub fn map_children(e: &A::Expr, f: &mut dyn FnMut(&A::Expr) -> A::Expr) -> A::Expr {
    let mut n = e.clone();
    let bx = |f: &mut dyn FnMut(&A::Expr) -> A::Expr, b: &Box<A::Expr>| Box::new(f(b));
    n.kind = match &e.kind {
        K::Num { .. } | K::Str { .. } | K::Bool { .. } | K::Name { .. } | K::End | K::Load { .. } => return n,
        K::Quantity { value, unit, bracket } => K::Quantity { value: bx(f, value), unit: unit.clone(), bracket: *bracket },
        K::Compare { op, left, right, tol } => K::Compare {
            op: op.clone(),
            left: bx(f, left),
            right: bx(f, right),
            tol: tol.as_ref().map(|t| bx(f, t)),
        },
        K::BinOp { op, left, right, implicit } => {
            K::BinOp { op: op.clone(), left: bx(f, left), right: bx(f, right), implicit: *implicit }
        }
        K::Logic { op, left, right } => K::Logic { op: op.clone(), left: bx(f, left), right: bx(f, right) },
        K::Neg { operand } => K::Neg { operand: bx(f, operand) },
        K::Not { operand } => K::Not { operand: bx(f, operand) },
        K::Sqrt { operand, root } => K::Sqrt { operand: bx(f, operand), root: *root },
        K::Abs { operand } => K::Abs { operand: bx(f, operand) },
        K::Call { func, args } => {
            let func = bx(f, func);
            K::Call { func, args: args.iter().map(|a| f(a)).collect() }
        }
        K::Index { target, index } => K::Index { target: bx(f, target), index: index.as_ref().map(|i| bx(f, i)) },
        K::Slice { lo, hi } => K::Slice { lo: lo.as_ref().map(|x| bx(f, x)), hi: hi.as_ref().map(|x| bx(f, x)) },
        K::Field { target, name } => K::Field { target: bx(f, target), name: name.clone() },
        K::Prime { target, order } => K::Prime { target: bx(f, target), order: *order },
        K::Deriv { var, order, operand, partial } => {
            K::Deriv { var: var.clone(), order: *order, operand: bx(f, operand), partial: *partial }
        }
        K::Integral { integrand, var, lo, hi } => {
            let integrand = bx(f, integrand);
            let lo = lo.as_ref().map(|x| bx(f, x));
            let hi = hi.as_ref().map(|x| bx(f, x));
            K::Integral { integrand, var: var.clone(), lo, hi }
        }
        K::Sum { body, var, lo, hi, step } => {
            let body = bx(f, body);
            let lo = bx(f, lo);
            let hi = bx(f, hi);
            let step = step.as_ref().map(|x| bx(f, x));
            K::Sum { body, var: var.clone(), lo, hi, step }
        }
        K::ListLit { items } => K::ListLit { items: items.iter().map(|x| f(x)).collect() },
        K::VecLit { items } => K::VecLit { items: items.iter().map(|x| f(x)).collect() },
        K::Table { names, items } => K::Table { names: names.clone(), items: items.iter().map(|x| f(x)).collect() },
        K::IfExpr { cond, then, other } => K::IfExpr { cond: bx(f, cond), then: bx(f, then), other: bx(f, other) },
        K::Convert { value, unit } => K::Convert { value: bx(f, value), unit: unit.clone() },
        K::Where { value, bindings } => {
            let value = bx(f, value);
            K::Where { value, bindings: bindings.iter().map(|(b, v)| (b.clone(), f(v))).collect() }
        }
        _ => return n,
    };
    n
}

/// Replace free names by expressions.
pub fn subst(e: &A::Expr, m: &[(String, A::Expr)]) -> A::Expr {
    let get = |n: &str| m.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    match &e.kind {
        K::Name { name } => get(name).unwrap_or_else(|| e.clone()),
        K::Integral { integrand, var, lo, hi } => {
            let inner: Vec<(String, A::Expr)> = m.iter().filter(|(k, _)| k != var).cloned().collect();
            let mut n = e.clone();
            n.kind = K::Integral {
                integrand: Box::new(subst(integrand, &inner)),
                var: var.clone(),
                lo: lo.as_ref().map(|x| Box::new(subst(x, m))),
                hi: hi.as_ref().map(|x| Box::new(subst(x, m))),
            };
            n
        }
        K::Sum { body, var, lo, hi, step } => {
            let inner: Vec<(String, A::Expr)> = m.iter().filter(|(k, _)| k != var).cloned().collect();
            let mut n = e.clone();
            n.kind = K::Sum {
                body: Box::new(subst(body, &inner)),
                var: var.clone(),
                lo: Box::new(subst(lo, m)),
                hi: Box::new(subst(hi, m)),
                step: step.as_ref().map(|x| Box::new(subst(x, m))),
            };
            n
        }
        K::Where { value, bindings } => {
            let inner: Vec<(String, A::Expr)> =
                m.iter().filter(|(k, _)| !bindings.iter().any(|(b, _)| b == k)).cloned().collect();
            let mut n = e.clone();
            n.kind = K::Where {
                value: Box::new(subst(value, &inner)),
                bindings: bindings.iter().map(|(b, v)| (b.clone(), subst(v, m))).collect(),
            };
            n
        }
        _ => map_children(e, &mut |c| subst(c, m)),
    }
}

pub fn depends_on(e: &A::Expr, var: &str) -> bool {
    crate::walk::free_names(e).iter().any(|n| n == var)
}

// ---------------------------------------------------------------- printing (Python to_source / _src)
const PREC_SUM: u8 = 1;
const PREC_PROD: u8 = 2;
const PREC_NEG: u8 = 3;
const PREC_JUXT: u8 = 4;
const PREC_POW: u8 = 5;
const PREC_ATOM: u8 = 6;

pub fn to_source(e: &A::Expr, pretty: bool) -> String {
    src(e, pretty).0
}

/// Python calculus.key: the ASCII source, used to compare expressions.
pub fn key(e: &A::Expr) -> String {
    to_source(e, false)
}

fn paren(s: String, p: u8, need: u8) -> (String, u8) {
    if p < need { (format!("({s})"), PREC_ATOM) } else { (s, p) }
}

fn ascii_name(n: &str, pretty: bool) -> String {
    if pretty {
        return n.to_string();
    }
    n.split('_')
        .map(|p| {
            if p == "∞" {
                return "infinity".to_string();
            }
            fermium_syntax::lexer::GREEK
                .iter()
                .find(|(k, v)| *v == p && *k != "inf")
                .map(|(k, _)| k.to_string())
                .unwrap_or_else(|| p.to_string())
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn is_unit_name(n: &str) -> bool {
    fermium_units::db::is_unit_name(n)
}

fn ends_with_number(mut e: &A::Expr) -> bool {
    while let K::BinOp { op, right, .. } = &e.kind {
        if op != "*" || e.paren {
            break;
        }
        e = right;
    }
    e.is_num()
}

fn starts_with_unit_name(mut e: &A::Expr) -> bool {
    while let K::BinOp { op, left, .. } = &e.kind {
        if !(op == "*" || op == "^") || e.paren {
            break;
        }
        e = left;
    }
    matches!(&e.kind, K::Name { name } if is_unit_name(name))
}

fn superscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '-' => '⁻',
            c => c,
        })
        .collect()
}

/// The display spelling of a written unit (Python checker.canonical_unit_name).
fn canonical_unit_name(u: &A::UnitExpr) -> String {
    use fermium_units::lookup_unit;
    let mut order: Vec<String> = vec![];
    let mut exps: Vec<num_rational::Rational64> = vec![];
    for f in &u.factors {
        let mut nm = fermium_units::db::unit_pretty(&f.name).map(str::to_string).unwrap_or_else(|| f.name.clone());
        if (nm.starts_with('u') || nm.starts_with('µ')) && nm.chars().count() > 1 {
            let rest: String = nm.chars().skip(1).collect();
            let mu = format!("μ{rest}");
            if let (Some(a), Some(b)) = (lookup_unit(&mu), lookup_unit(&nm)) {
                if rest != "n" && a.factor == b.factor {
                    nm = mu;
                }
            }
        }
        match order.iter().position(|x| *x == nm) {
            Some(i) => exps[i] += f.exp,
            None => {
                order.push(nm);
                exps.push(f.exp);
            }
        }
    }
    let zero = num_rational::Rational64::from_integer(0);
    let numr: Vec<String> = order
        .iter()
        .zip(&exps)
        .filter(|(_, e)| **e > zero)
        .map(|(n, e)| format!("{n}{}", fermium_units::dim::fmt_exp(*e, true)))
        .collect();
    let den: Vec<String> = order
        .iter()
        .zip(&exps)
        .filter(|(_, e)| **e < zero)
        .map(|(n, e)| format!("{n}{}", fermium_units::dim::fmt_exp(-*e, true)))
        .collect();
    fermium_units::dim::join_units(&numr, &den)
}

/// Python Fraction(x).limit_denominator(maxd) as (numerator, denominator); None if x is out of i128 range.
fn limit_denominator(x: f64, maxd: i128) -> Option<(i128, i128)> {
    if !x.is_finite() {
        return None;
    }
    // the exact binary fraction
    let bits = x.to_bits();
    let sign: i128 = if bits >> 63 == 1 { -1 } else { 1 };
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let mant = (bits & ((1u64 << 52) - 1)) as i128;
    let (m, e) = if exp == 0 { (mant, -1074) } else { (mant | (1i128 << 52), exp - 1075) };
    let (mut n, mut d) = if e >= 0 {
        if e > 60 {
            return None;
        }
        (m << e, 1i128)
    } else {
        if -e > 120 {
            return None;
        }
        (m, 1i128 << (-e))
    };
    while n % 2 == 0 && d % 2 == 0 && d > 1 {
        n /= 2;
        d /= 2;
    }
    n *= sign;
    if d <= maxd {
        return Some((n, d));
    }
    let (sn, sd) = (n, d);
    let (mut p0, mut q0, mut p1, mut q1) = (0i128, 1i128, 1i128, 0i128);
    let (mut nn, mut dd) = (n, d);
    loop {
        let a = nn.div_euclid(dd);
        let q2 = q0 + a * q1;
        if q2 > maxd {
            break;
        }
        let np1 = p0 + a * p1;
        p0 = p1;
        q0 = q1;
        p1 = np1;
        q1 = q2;
        let nd = nn - a * dd;
        nn = dd;
        dd = nd;
    }
    let k = (maxd - q0) / q1;
    let (b1n, b1d) = (p0 + k * p1, q0 + k * q1);
    let (b2n, b2d) = (p1, q1);
    // |b2 - s| <= |b1 - s|, exactly
    let diff = |pn: i128, pd: i128| -> (i128, i128) { ((pn * sd - sn * pd).abs(), pd * sd) };
    let (a2, a2d) = diff(b2n, b2d);
    let (a1, a1d) = diff(b1n, b1d);
    if a2.checked_mul(a1d)? <= a1.checked_mul(a2d)? { Some((b2n, b2d)) } else { Some((b1n, b1d)) }
}

fn src(e: &A::Expr, pretty: bool) -> (String, u8) {
    match &e.kind {
        K::Num { value: v, .. } => {
            let v = *v;
            let s = if v == v.trunc() && v.abs() < 1e15 {
                format!("{}", v as i64)
            } else {
                let s = crate::units::format_number(v, Some(12));
                if pretty {
                    s
                } else {
                    s.replace("×10", "e")
                        .chars()
                        .map(|c| match c {
                            '⁰' => '0',
                            '¹' => '1',
                            '²' => '2',
                            '³' => '3',
                            '⁴' => '4',
                            '⁵' => '5',
                            '⁶' => '6',
                            '⁷' => '7',
                            '⁸' => '8',
                            '⁹' => '9',
                            '⁻' => '-',
                            c => c,
                        })
                        .collect()
                }
            };
            if v >= 0.0 { (s, PREC_ATOM) } else { (s, PREC_NEG) }
        }
        K::Name { name } => (ascii_name(name, pretty), PREC_ATOM),
        K::Str { value } => (format!("\"{value}\""), PREC_ATOM),
        K::Bool { value } => ((if *value { "true" } else { "false" }).to_string(), PREC_ATOM),
        K::Quantity { value, unit, bracket } => {
            let v = src(value, pretty).0;
            let mut text = unit.text.clone();
            if pretty {
                let c = canonical_unit_name(unit);
                if !c.is_empty() {
                    text = c;
                }
            }
            (if *bracket { format!("{v} [{text}]") } else { format!("{v} {text}") }, PREC_JUXT)
        }
        K::Uncertain { value, err } => {
            let (v, vp) = src(value, pretty);
            let (r, rp) = src(err, pretty);
            (format!("{} {} {}", paren(v, vp, PREC_PROD).0, if pretty { "±" } else { "+-" }, paren(r, rp, PREC_PROD).0),
             PREC_SUM)
        }
        K::Neg { operand } => {
            let (s, p) = src(operand, pretty);
            let need = if matches!(&operand.kind, K::BinOp { op, .. } if op == "*" || op == "/") {
                PREC_PROD
            } else {
                PREC_JUXT
            };
            (format!("-{}", paren(s, p, need).0), PREC_NEG)
        }
        K::BinOp { op, left, right, .. } if op == "+" || op == "-" => {
            let (mut l, _) = src(left, pretty);
            if matches!(left.kind, K::Integral { .. }) {
                l = format!("({l})");
            }
            let (r, rp) = src(right, pretty);
            let (mut r, rp) = paren(r, rp, PREC_PROD);
            if op == "-" && rp == PREC_SUM {
                r = format!("({r})");
            }
            (format!("{l} {op} {r}"), PREC_SUM)
        }
        K::BinOp { op, left, right, .. } if op == "*" => {
            let (l, lp) = src(left, pretty);
            let (r, rp) = src(right, pretty);
            if left.is_num() && matches!(&right.kind, K::Name { name } if name == "𝑖") && lp >= PREC_JUXT {
                return (format!("{l}i"), PREC_JUXT);
            }
            if lp >= PREC_JUXT && rp >= PREC_JUXT && !r.starts_with('-') {
                let mut sep = " ";
                if matches!(right.kind, K::Num { .. } | K::Quantity { .. }) {
                    sep = if pretty { "·" } else { "*" };
                } else if left.is_num()
                    && pretty
                    && (matches!(&right.kind, K::Name { name } if !is_unit_name(name))
                        || matches!(&right.kind, K::BinOp { op, left: rl, .. } if op == "^"
                            && matches!(&rl.kind, K::Name { name } if !is_unit_name(name))))
                {
                    sep = "";
                } else if ends_with_number(left) && starts_with_unit_name(right) {
                    sep = if pretty { "·" } else { "*" };
                }
                return (format!("{l}{sep}{r}"), PREC_JUXT);
            }
            let (l, _) = paren(l, lp, PREC_PROD);
            let (r, _) = paren(r, rp, PREC_NEG + 1);
            (format!("{l}{}{r}", if pretty { "·" } else { "*" }), PREC_PROD)
        }
        K::BinOp { op, left, right, .. } if op == "/" => {
            let (l, lp) = src(left, pretty);
            let (mut r, rp) = src(right, pretty);
            let (l, _) = paren(l, lp, PREC_PROD);
            if rp <= PREC_JUXT {
                r = format!("({r})");
            }
            (format!("{l}/{r}"), PREC_PROD)
        }
        K::BinOp { op, left, right, .. } if op == "^" => {
            let (l, lp) = src(left, pretty);
            let (l, _) = paren(l, lp, PREC_POW + 1);
            if let Some(rv) = right.num_value() {
                if rv == rv.trunc() && pretty {
                    return (format!("{l}{}", superscript(&format!("{}", rv as i64))), PREC_POW);
                }
                if rv != rv.trunc() {
                    if let Some((n, d)) = limit_denominator(rv, 99) {
                        if ((n as f64 / d as f64) - rv).abs() < 1e-12 {
                            return (format!("{l}^({n}/{d})"), PREC_POW);
                        }
                    }
                }
            }
            let (r, rp) = src(right, pretty);
            let (r, _) = paren(r, rp, PREC_ATOM);
            (format!("{l}^{r}"), PREC_POW)
        }
        K::BinOp { .. } => (format!("<{}>", e.class()), PREC_ATOM),
        K::Sqrt { operand, root } => {
            let s = src(operand, pretty).0;
            let f = if pretty {
                if *root == 2 { "√" } else { "∛" }
            } else if *root == 2 {
                "sqrt"
            } else {
                "cbrt"
            };
            (format!("{f}({s})"), PREC_ATOM)
        }
        K::Abs { operand } => (format!("|{}|", src(operand, pretty).0), PREC_ATOM),
        K::Call { func, args } => {
            let f = src(func, pretty).0;
            let a: Vec<String> = args.iter().map(|x| src(x, pretty).0).collect();
            (format!("{f}({})", a.join(", ")), PREC_ATOM)
        }
        K::Prime { target, order } => (format!("{}{}", src(target, pretty).0, "'".repeat(*order as usize)), PREC_ATOM),
        K::Index { target, index } => {
            let i = index.as_ref().map(|x| src(x, pretty).0).unwrap_or_else(|| "None".into());
            (format!("{}[{i}]", src(target, pretty).0), PREC_ATOM)
        }
        K::Field { target, name } => (format!("{}.{name}", src(target, pretty).0), PREC_ATOM),
        K::Compare { op, left, right, tol } => {
            let t = tol.as_ref().map(|t| format!(" within {}", src(t, pretty).0)).unwrap_or_default();
            (format!("{} {op} {}{t}", src(left, pretty).0, src(right, pretty).0), 0)
        }
        K::Logic { op, left, right } => (format!("{} {op} {}", src(left, pretty).0, src(right, pretty).0), 0),
        K::Not { operand } => (format!("not {}", src(operand, pretty).0), 0),
        K::IfExpr { cond, then, other } => {
            (format!("if {} then {} else {}", src(cond, pretty).0, src(then, pretty).0, src(other, pretty).0), 0)
        }
        K::ListLit { items } => {
            (format!("[{}]", items.iter().map(|x| src(x, pretty).0).collect::<Vec<_>>().join(", ")), PREC_ATOM)
        }
        K::VecLit { items } => {
            (format!("<{}>", items.iter().map(|x| src(x, pretty).0).collect::<Vec<_>>().join(", ")), PREC_ATOM)
        }
        K::Deriv { var, operand, partial, .. } => {
            let d = if *partial && pretty {
                "∂"
            } else if *partial {
                "partial"
            } else {
                "d"
            };
            (format!("{d}/{d}{var} {}", src(operand, pretty).0), PREC_JUXT)
        }
        K::Integral { integrand, var, lo, hi } => {
            let mut s = format!("{} {} d{var}", if pretty { "∫" } else { "integral" }, src(integrand, pretty).0);
            if let Some(lo) = lo {
                let h = hi.as_ref().map(|h| src(h, pretty).0).unwrap_or_else(|| "None".into());
                s += &format!(" from {} to {h}", src(lo, pretty).0);
            }
            (s, 0)
        }
        K::Sum { body, var, lo, hi, step } => {
            let mut s = format!("{}({} for {var} from {} to {}", if pretty { "Σ" } else { "sum" }, src(body, pretty).0,
                                src(lo, pretty).0, src(hi, pretty).0);
            if let Some(st) = step {
                s += &format!(" step {}", src(st, pretty).0);
            }
            (s + ")", PREC_ATOM)
        }
        K::Convert { value, unit } => (format!("{} in {}", src(value, pretty).0, unit.text), 0),
        K::Where { value, bindings } => {
            let b: Vec<String> = bindings.iter().map(|(n, v)| format!("{n} = {}", src(v, pretty).0)).collect();
            (format!("{} where {}", src(value, pretty).0, b.join(", ")), 0)
        }
        K::End => ("end".into(), PREC_ATOM),
        K::Slice { lo, hi } => {
            let l = lo.as_ref().map(|x| src(x, pretty).0).unwrap_or_default();
            let h = hi.as_ref().map(|x| src(x, pretty).0).unwrap_or_default();
            (format!("{l}:{h}"), PREC_ATOM)
        }
        K::Load { path } => (format!("load \"{path}\""), PREC_ATOM),
        K::Table { names, items } => {
            let c: Vec<String> = names.iter().zip(items).map(|(n, v)| format!("{n} = {}", src(v, pretty).0)).collect();
            (format!("table({})", c.join(", ")), PREC_ATOM)
        }
        _ => (format!("<{}>", e.class()), PREC_ATOM),
    }
}

// ---------------------------------------------------------------- simplification
fn factors(e: &A::Expr) -> (f64, Vec<A::Expr>) {
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
        K::Quantity { value, unit, bracket } if value.is_num() && numv(value) != 1.0 => {
            let mut q = e.clone();
            q.kind = K::Quantity { value: Box::new(num(1.0)), unit: unit.clone(), bracket: *bracket };
            (numv(value), vec![q])
        }
        _ => (1.0, vec![e.clone()]),
    }
}

fn inverse(f: &A::Expr) -> A::Expr {
    if let K::BinOp { op, left, right, .. } = &f.kind {
        if op == "^" && right.is_num() {
            return pw((**left).clone(), num(-numv(right)));
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

fn is_unit_quantity(q: &A::Expr) -> bool {
    matches!(&q.kind, K::Quantity { value, .. } if value.is_num() && numv(value) == 1.0)
}

fn with_value(q: &A::Expr, v: f64) -> A::Expr {
    let mut n = q.clone();
    if let K::Quantity { value, .. } = &mut n.kind {
        *value = Box::new(num(v));
    }
    n
}

fn build_product(c: f64, fs: Vec<A::Expr>) -> A::Expr {
    let mut merged: Vec<String> = vec![];
    let mut powers: Vec<(String, A::Expr, f64)> = vec![];
    for f in fs {
        let (base, p) = match &f.kind {
            K::BinOp { op, left, right, .. } if op == "^" && right.is_num() => ((**left).clone(), numv(right)),
            _ => (f.clone(), 1.0),
        };
        let k = key(&base);
        if let Some(entry) = powers.iter_mut().find(|x| x.0 == k) {
            entry.2 += p;
        } else {
            powers.push((k.clone(), base, p));
            merged.push(k);
        }
    }
    if c == 0.0 {
        return num(0.0);
    }
    let mut top = vec![];
    let mut bottom = vec![];
    for k in &merged {
        let (_, base, p) = powers.iter().find(|x| &x.0 == k).unwrap();
        let p = *p;
        if p > 0.0 {
            top.push(if p == 1.0 { base.clone() } else { pw(base.clone(), num(p)) });
        } else if p < 0.0 {
            bottom.push(if p == -1.0 { base.clone() } else { pw(base.clone(), num(-p)) });
        }
    }
    // stable sort, like Python's list.sort
    top.sort_by_key(rank);
    bottom.sort_by_key(rank);
    let sign = if c < 0.0 { -1 } else { 1 };
    let mut c = c.abs();
    let mut den_c = 1.0;
    if c < 1.0 && c > 0.0 {
        let inv = 1.0 / c;
        if (inv - py_round(inv)).abs() < 1e-12 && py_round(inv) <= 1e6 {
            den_c = py_round(inv);
            c = 1.0;
        }
    }
    let mut out: Option<A::Expr> = None;
    if let Some(i) = top.iter().position(is_unit_quantity).filter(|_| c != 0.0) {
        let q = top.remove(i);
        out = Some(with_value(&q, c));
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

/// Python round() to the nearest integer, halves to even.
fn py_round(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }
}

pub fn simplify(e0: &A::Expr) -> A::Expr {
    let e = map_children(e0, &mut |c| simplify(c));
    match &e.kind {
        K::Neg { operand: o } => {
            if let Some(v) = o.num_value() {
                return num(-v);
            }
            if let K::Neg { operand } = &o.kind {
                return (**operand).clone();
            }
            e
        }
        K::BinOp { op, left: a, right: b, .. } => {
            let (a, b) = (&**a, &**b);
            if let (Some(x), Some(y)) = (a.num_value(), b.num_value()) {
                let v = match op.as_str() {
                    "+" => Some(x + y),
                    "-" => Some(x - y),
                    "*" => Some(x * y),
                    "/" => {
                        if y != 0.0 {
                            Some(x / y)
                        } else {
                            None
                        }
                    }
                    "^" => Some(py_pow(x, y)),
                    _ => None,
                };
                if let Some(v) = v {
                    if v.is_finite() {
                        return num(v);
                    }
                }
            }
            match op.as_str() {
                "+" => {
                    if is_num(a, Some(0.0)) {
                        return b.clone();
                    }
                    if is_num(b, Some(0.0)) {
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
                    e
                }
                "-" => {
                    if is_num(b, Some(0.0)) {
                        return a.clone();
                    }
                    if is_num(a, Some(0.0)) {
                        return simplify(&neg(b.clone()));
                    }
                    if let K::Neg { operand } = &b.kind {
                        return simplify(&add(a.clone(), (**operand).clone()));
                    }
                    if key(a) == key(b) {
                        return num(0.0);
                    }
                    e
                }
                "*" => {
                    let (c, fs) = factors(&e);
                    build_product(c, fs)
                }
                "/" => {
                    if is_num(b, Some(1.0)) {
                        return a.clone();
                    }
                    if is_num(a, Some(0.0)) {
                        return num(0.0);
                    }
                    if key(a) == key(b) {
                        return num(1.0);
                    }
                    let sumlike = |x: &A::Expr| matches!(&x.kind, K::BinOp { op, .. } if op == "+" || op == "-");
                    if !sumlike(a) && !sumlike(b) {
                        let (c, fs) = factors(&e);
                        return build_product(c, fs);
                    }
                    if let K::Neg { operand } = &a.kind {
                        return simplify(&neg(div((**operand).clone(), b.clone())));
                    }
                    if let Some(bv) = b.num_value() {
                        if bv != 0.0 {
                            let (ca, fa) = factors(a);
                            return build_product(ca / bv, fa);
                        }
                    }
                    let (ca, fa) = factors(a);
                    if ca != 1.0 && !fa.is_empty() {
                        let inner = div(build_product(1.0, fa), b.clone());
                        return if ca != -1.0 { build_product(ca, vec![inner]) } else { neg(inner) };
                    }
                    e
                }
                "^" => {
                    if is_num(b, Some(1.0)) {
                        return a.clone();
                    }
                    if is_num(b, Some(0.0)) {
                        return num(1.0);
                    }
                    if is_num(a, Some(1.0)) {
                        return num(1.0);
                    }
                    if let K::BinOp { op: aop, left: al, right: ar, .. } = &a.kind {
                        if aop == "^" && ar.is_num() && b.is_num() {
                            return simplify(&pw((**al).clone(), num(numv(ar) * numv(b))));
                        }
                    }
                    if let K::Sqrt { operand, root: 2 } = &a.kind {
                        if is_num(b, Some(2.0)) {
                            return (**operand).clone();
                        }
                    }
                    e
                }
                _ => e,
            }
        }
        K::Sqrt { operand, root } => {
            if let K::Num { value, digit: false, .. } = operand.kind {
                if value >= 0.0 {
                    return num(value.powf(1.0 / *root as f64));
                }
            }
            e
        }
        K::Call { func, args } if args.len() == 1 && args[0].is_num() => {
            if let K::Name { name: f } = &func.kind {
                let v = numv(&args[0]);
                if matches!(f.as_str(), "sin" | "tan" | "sinh" | "tanh" | "asin" | "atan") && v == 0.0 {
                    return num(0.0);
                }
                if matches!(f.as_str(), "cos" | "cosh" | "exp") && v == 0.0 {
                    return num(1.0);
                }
            }
            e
        }
        _ => e,
    }
}

/// Python float ** float: NaN for a complex result (Python gives a complex, which isn't folded).
fn py_pow(x: f64, y: f64) -> f64 {
    if x == 0.0 && y < 0.0 {
        return f64::NAN; // ZeroDivisionError: not folded
    }
    if x < 0.0 && y != y.trunc() {
        return f64::NAN;
    }
    x.powf(y)
}

// ---------------------------------------------------------------- differentiation (plain context)
fn builtin_deriv(f: &str, u: &A::Expr) -> Option<A::Expr> {
    let u = u.clone();
    Some(match f {
        "sin" => call("cos", vec![u]),
        "cos" => neg(call("sin", vec![u])),
        "tan" => div(num(1.0), pw(call("cos", vec![u]), num(2.0))),
        "cot" => neg(div(num(1.0), pw(call("sin", vec![u]), num(2.0)))),
        "sec" => mul(call("sec", vec![u.clone()]), call("tan", vec![u])),
        "csc" => neg(mul(call("csc", vec![u.clone()]), call("cot", vec![u]))),
        "exp" => call("exp", vec![u]),
        "ln" | "log" => div(num(1.0), u),
        "log10" => div(num(1.0), mul(u, call("ln", vec![num(10.0)]))),
        "log2" => div(num(1.0), mul(u, call("ln", vec![num(2.0)]))),
        "sinh" => call("cosh", vec![u]),
        "cosh" => call("sinh", vec![u]),
        "tanh" => div(num(1.0), pw(call("cosh", vec![u]), num(2.0))),
        "asin" => div(num(1.0), mk(K::Sqrt { operand: Box::new(sub(num(1.0), pw(u, num(2.0)))), root: 2 })),
        "acos" => neg(div(num(1.0), mk(K::Sqrt { operand: Box::new(sub(num(1.0), pw(u, num(2.0)))), root: 2 }))),
        "atan" => div(num(1.0), add(num(1.0), pw(u, num(2.0)))),
        "sqrt" => div(num(1.0), mul(num(2.0), mk(K::Sqrt { operand: Box::new(u), root: 2 }))),
        "cbrt" => div(num(1.0), mul(num(3.0), pw(mk(K::Sqrt { operand: Box::new(u), root: 3 }), num(2.0)))),
        "abs" => call("sign", vec![u]),
        "erf" => mul(div(num(2.0), mk(K::Sqrt { operand: Box::new(name("π")), root: 2 })),
                     call("exp", vec![neg(pw(u, num(2.0)))])),
        _ => return None,
    })
}

fn is_builtin_deriv(f: &str) -> bool {
    builtin_deriv(f, &num(0.0)).is_some()
}

/// Python _d(e, var, Plain()): d e / d var, unsimplified.
pub fn d(e: &A::Expr, var: &str) -> Result<A::Expr, SymError> {
    match &e.kind {
        K::Num { .. } | K::Str { .. } | K::Bool { .. } => return Ok(num(0.0)),
        K::Quantity { value, unit, bracket } => {
            if !depends_on(value, var) {
                return Ok(num(0.0));
            }
            let mut q = e.clone();
            q.kind = K::Quantity { value: Box::new(num(1.0)), unit: unit.clone(), bracket: *bracket };
            return Ok(mul(d(value, var)?, q));
        }
        K::Name { name } => return Ok(num(if name == var { 1.0 } else { 0.0 })),
        K::Convert { value, .. } => return d(value, var),
        _ => {}
    }
    if !depends_on(e, var) && !matches!(e.kind, K::Deriv { .. }) {
        return Ok(num(0.0));
    }
    match &e.kind {
        K::BinOp { op, left: a, right: b, .. } if matches!(op.as_str(), "+" | "-" | "*" | "/" | "^") => {
            let (a, b) = (&**a, &**b);
            match op.as_str() {
                "+" => Ok(add(d(a, var)?, d(b, var)?)),
                "-" => Ok(sub(d(a, var)?, d(b, var)?)),
                "*" => Ok(add(mul(d(a, var)?, b.clone()), mul(a.clone(), d(b, var)?))),
                "/" => {
                    if !depends_on(b, var) {
                        return Ok(div(d(a, var)?, b.clone()));
                    }
                    Ok(div(sub(mul(d(a, var)?, b.clone()), mul(a.clone(), d(b, var)?)), pw(b.clone(), num(2.0))))
                }
                _ => {
                    if !depends_on(b, var) {
                        return Ok(mul(mul(b.clone(), pw(a.clone(), simplify(&sub(b.clone(), num(1.0))))), d(a, var)?));
                    }
                    if !depends_on(a, var) {
                        return Ok(mul(mul(e.clone(), call("ln", vec![a.clone()])), d(b, var)?));
                    }
                    Ok(mul(e.clone(),
                           add(mul(d(b, var)?, call("ln", vec![a.clone()])), div(mul(b.clone(), d(a, var)?), a.clone()))))
                }
            }
        }
        K::Neg { operand } => Ok(neg(d(operand, var)?)),
        K::Sqrt { operand: u, root } => {
            if *root == 2 {
                return Ok(div(d(u, var)?, mul(num(2.0), mk(K::Sqrt { operand: u.clone(), root: 2 }))));
            }
            Ok(div(d(u, var)?, mul(num(*root as f64), pw(mk(K::Sqrt { operand: u.clone(), root: *root }),
                                                        num((*root - 1) as f64)))))
        }
        K::Abs { operand } => Ok(mul(call("sign", vec![(**operand).clone()]), d(operand, var)?)),
        K::IfExpr { cond, then, other } => {
            Ok(mk(K::IfExpr { cond: cond.clone(), then: Box::new(d(then, var)?), other: Box::new(d(other, var)?) }))
        }
        K::VecLit { items } => Ok(mk(K::VecLit { items: items.iter().map(|x| d(x, var)).collect::<Result<_, _>>()? })),
        K::Deriv { .. } => {
            // d/dt (formula) inside the equation: needs the full differentiator (fermium-sym); the plain context
            // of isolate only meets it in rare equations
            Err(serr("can't differentiate this derivative expression symbolically", e.span))
        }
        K::Call { func, args } => {
            if let K::Name { name: fname } = &func.kind {
                if args.len() == 1 && is_builtin_deriv(fname) {
                    let u = &args[0];
                    return Ok(mul(builtin_deriv(fname, u).unwrap(), d(u, var)?));
                }
                if matches!(fname.as_str(), "besselj" | "bessely" | "besseli" | "besselk") && args.len() == 2 {
                    let (n, x) = (&args[0], &args[1]);
                    if !is_num(&simplify(&d(n, var)?), Some(0.0)) {
                        return Err(serr(format!("can't differentiate {fname}(n, x) with respect to its order n"), e.span));
                    }
                    let lo = call(fname, vec![sub(n.clone(), num(1.0)), x.clone()]);
                    let hi = call(fname, vec![add(n.clone(), num(1.0)), x.clone()]);
                    let (sgn, comb) = match fname.as_str() {
                        "besselj" | "bessely" => (1, -1),
                        "besseli" => (1, 1),
                        _ => (-1, 1),
                    };
                    let dd = div(if comb > 0 { add(lo, hi) } else { sub(lo, hi) }, num(2.0));
                    return Ok(mul(if sgn < 0 { neg(dd) } else { dd }, d(x, var)?));
                }
                if matches!(fname.as_str(), "ellipk" | "ellipe") && args.len() == 1 {
                    let m = &args[0];
                    let kk = call("ellipk", vec![m.clone()]);
                    let ee = call("ellipe", vec![m.clone()]);
                    let dd = if fname == "ellipk" {
                        div(sub(ee, mul(sub(num(1.0), m.clone()), kk)), mul(mul(num(2.0), m.clone()), sub(num(1.0), m.clone())))
                    } else {
                        div(sub(ee, kk), mul(num(2.0), m.clone()))
                    };
                    return Ok(mul(dd, d(m, var)?));
                }
                if matches!(fname.as_str(), "min" | "max" | "floor" | "ceil" | "round" | "sign" | "atan2" | "hypot") {
                    if fname == "hypot" && args.len() == 2 {
                        let (a, b) = (&args[0], &args[1]);
                        return Ok(div(add(mul(a.clone(), d(a, var)?), mul(b.clone(), d(b, var)?)), e.clone()));
                    }
                    if matches!(fname.as_str(), "floor" | "ceil" | "round" | "sign") {
                        return Ok(num(0.0));
                    }
                }
                let mut er = serr(format!("can't differentiate {fname}(...) symbolically"), e.span);
                er.hint = Some("differentiate a formula made of +, -, ×, /, powers and standard functions".into());
                return Err(er);
            }
            if let K::Field { target, name } = &func.kind {
                if let K::Name { name: t } = &target.kind {
                    let raw = func.attrs.raw.clone().unwrap_or_else(|| name.clone());
                    let mut er = serr(format!("can't differentiate {t}.{raw}(...) symbolically"), e.span);
                    er.hint = Some("if it is a Python function, it is a black box to Fermium: write the formula in \
                                    Fermium, or take a finite difference, like (f(x + h) - f(x - h)) / (2 h)".into());
                    return Err(er);
                }
            }
            Err(serr("can't differentiate this expression symbolically", e.span))
        }
        K::Sum { body, var: sv, lo, hi, step } => {
            if sv == var {
                return Ok(num(0.0));
            }
            for b in [Some(lo), Some(hi), step.as_ref()].into_iter().flatten() {
                if depends_on(b, var) {
                    let mut er = serr(format!("can't differentiate this sum with respect to {var}: its limits depend on \
                                               {var}"), e.span);
                    er.hint = Some("the number of terms changes in steps, so it has no derivative".into());
                    return Err(er);
                }
            }
            let inner = simplify(&d(&inline_where(body), var)?);
            if is_num(&inner, Some(0.0)) {
                return Ok(num(0.0));
            }
            let mut n = e.clone();
            if let K::Sum { body, .. } = &mut n.kind {
                *body = Box::new(inner);
            }
            Ok(n)
        }
        _ => Err(serr("can't differentiate this expression symbolically", e.span)),
    }
}

/// Replace `a where x = b` by a[x := b] everywhere.
pub fn inline_where(e: &A::Expr) -> A::Expr {
    let e = map_children(e, &mut |c| inline_where(c));
    if let K::Where { value, bindings } = &e.kind {
        let mut val = (**value).clone();
        for (b, v) in bindings.iter().rev() {
            val = subst(&val, &[(b.clone(), v.clone())]);
        }
        return val;
    }
    e
}

// ---------------------------------------------------------------- ODE helpers
fn replace_key(e: &A::Expr, keys: &[(String, String)]) -> A::Expr {
    let k = key(e);
    if let Some((_, n)) = keys.iter().find(|(kk, _)| *kk == k) {
        return name(n);
    }
    map_children(e, &mut |c| replace_key(c, keys))
}

/// Solve lhs = rhs for target (e.g. x''), assuming linearity (Python isolate).
pub fn isolate(lhs: &A::Expr, rhs: &A::Expr, target: &A::Expr) -> Result<A::Expr, SymError> {
    let ph = "__H__";
    let dd = simplify(&replace_key(&sub(lhs.clone(), rhs.clone()), &[(key(target), ph.to_string())]));
    let a = d(&dd, ph).ok().map(|x| simplify(&x));
    let bad = match &a {
        None => true,
        Some(a) => depends_on(a, ph) || is_num(a, Some(0.0)),
    };
    if bad {
        let mut er = serr(format!("can't solve this equation for {}: it must appear linearly (like m x'' = ...)",
                                  to_source(target, true)), lhs.span);
        er.hint = Some("rewrite it as  x'' = <formula>".into());
        return Err(er);
    }
    let b = simplify(&subst(&dd, &[(ph.to_string(), num(0.0))]));
    Ok(simplify(&neg(div(b, a.unwrap()))))
}

/// lhs − rhs = Σ a_j · target_j + r0 (Python linear_coeffs); None if not linear in the targets.
pub fn linear_coeffs(lhs: &A::Expr, rhs: &A::Expr, targets: &[A::Expr]) -> Option<(Vec<A::Expr>, A::Expr)> {
    let names: Vec<String> = (0..targets.len()).map(|j| format!("__T{j}__")).collect();
    let keys: Vec<(String, String)> = targets.iter().zip(&names).map(|(t, n)| (key(t), n.clone())).collect();
    let dd = simplify(&replace_key(&sub(lhs.clone(), rhs.clone()), &keys));
    let mut coeffs = vec![];
    for n in &names {
        let a = simplify(&d(&dd, n).ok()?);
        if names.iter().any(|m| depends_on(&a, m)) {
            return None;
        }
        coeffs.push(a);
    }
    let zeros: Vec<(String, A::Expr)> = names.iter().map(|n| (n.clone(), num(0.0))).collect();
    let r0 = simplify(&subst(&dd, &zeros));
    Some((coeffs, r0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_expr(s: &str) -> A::Expr {
        let (prog, _) = fermium_syntax::parse(&format!("zz = {s}\n"), &[]).unwrap();
        match &prog.body[0].kind {
            A::StmtKind::Assign { value, .. } => value.clone(),
            _ => panic!(),
        }
    }

    #[test]
    fn isolate_like_python() {
        // python3 -c "from fermium import calculus as C, parser; ..." : isolate(m x'' , -k x, x'') = -k x/m
        let lhs = parse_expr("m x''");
        let rhs = parse_expr("-k x");
        let t = mk(K::Prime { target: Box::new(name("x")), order: 2 });
        let r = isolate(&lhs, &rhs, &t).unwrap();
        assert_eq!(to_source(&r, true), "-k x/m");
        let cases = [("x'' + 2 γ x' + ω^2 x", "F/m cos(w t)", "-(2γ x' + x ω² - F/(m cos(w t)))"),
                     ("m x''", "-k x - b x' + 0.5 x^3", "(-k x - b x' + x³/2)/m")];
        for (l, r, want) in cases {
            let got = isolate(&parse_expr(l), &parse_expr(r), &t).unwrap();
            assert_eq!(to_source(&got, true), want);
        }
    }

    #[test]
    fn limit_denominator_like_python() {
        assert_eq!(limit_denominator(2.0 / 3.0, 99), Some((2, 3)));
        assert_eq!(limit_denominator(0.1, 99), Some((1, 10)));
    }
}
