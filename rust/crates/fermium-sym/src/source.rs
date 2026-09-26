//! The source text of an expression (calculus.py `to_source`, `_src`): how derivatives and antiderivatives
//! print, and the key used to compare expressions (`to_source(e, pretty=False)`).
use fermium_syntax::ast as A;
use num_rational_lite::limit_denominator;

use crate::build::is_num;

type K = A::ExprKind;

const PREC_SUM: u8 = 1;
const PREC_PROD: u8 = 2;
const PREC_NEG: u8 = 3;
const PREC_JUXT: u8 = 4;
const PREC_POW: u8 = 5;
const PREC_ATOM: u8 = 6;

/// The pretty source text (Greek letters, superscripts, √, ·).
pub fn to_source(e: &A::Expr) -> String {
    src(e, true).0
}

/// `to_source(e, pretty)`.
pub fn to_source_p(e: &A::Expr, pretty: bool) -> String {
    src(e, pretty).0
}

/// The comparison key of an expression (calculus.key: its ASCII source text).
pub fn key(e: &A::Expr) -> String {
    src(e, false).0
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

fn unsuperscript(s: &str) -> String {
    s.chars()
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

/// A name in ASCII (Greek parts spelled out) unless pretty (calculus._name).
fn name_text(n: &str, pretty: bool) -> String {
    if pretty {
        return n.to_string();
    }
    n.split('_')
        .map(|p| {
            fermium_syntax::lexer::GREEK
                .iter()
                .find(|(k, v)| *v == p && *k != "inf")
                .map(|(k, _)| k.to_string())
                .unwrap_or_else(|| p.to_string())
        })
        .collect::<Vec<_>>()
        .join("_")
}

/// Python `str(int(v))` for a whole float.
fn int_text(v: f64) -> String {
    format!("{}", v as i64)
}

pub fn is_unit_name(n: &str) -> bool {
    fermium_units::lookup_unit(n).is_some()
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

/// The display spelling of a written unit (checker.canonical_unit_name): `ft/s^2` → `ft/s²`, `N·m` → `N m`.
pub fn canonical_unit_name(u: &A::UnitExpr) -> String {
    use fermium_units::Rational64;
    let mut order: Vec<String> = vec![];
    let mut exps: Vec<Rational64> = vec![];
    for f in &u.factors {
        let mut nm = fermium_units::unit_pretty(&f.name).map(str::to_string).unwrap_or_else(|| f.name.clone());
        let mut cs = nm.chars();
        let first = cs.next();
        let rest: String = cs.collect();
        if matches!(first, Some('u') | Some('µ')) && nm.chars().count() > 1 {
            let mu = format!("μ{rest}");
            if let (Some(a), Some(b)) = (fermium_units::lookup_unit(&mu), fermium_units::lookup_unit(&nm)) {
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
    let zero = Rational64::from_integer(0);
    let num: Vec<String> = order
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
    fermium_units::dim::join_units(&num, &den)
}

fn paren(s: String, p: u8, need: u8) -> (String, u8) {
    if p < need { (format!("({s})"), PREC_ATOM) } else { (s, p) }
}

fn src(e: &A::Expr, pretty: bool) -> (String, u8) {
    let s = |x: &A::Expr| src(x, pretty).0;
    match &e.kind {
        K::Num { value, .. } => {
            let v = *value;
            let t = if v.is_finite() && v == v.trunc() && v.abs() < 1e15 {
                int_text(v)
            } else {
                let mut t = fermium_units::numfmt::format_number(v, 12, true);
                if !pretty {
                    t = unsuperscript(&t.replace("×10", "e"));
                }
                t
            };
            (t, if v >= 0.0 || v.is_nan() { PREC_ATOM } else { PREC_NEG })
        }
        K::Name { name } => (name_text(name, pretty), PREC_ATOM),
        K::Str { value } => (format!("\"{value}\""), PREC_ATOM),
        K::Bool { value } => ((if *value { "true" } else { "false" }).into(), PREC_ATOM),
        K::Quantity { value, unit, bracket } => {
            let v = s(value);
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
            let (t, p) = src(operand, pretty);
            let need = if matches!(&operand.kind, K::BinOp { op, .. } if op == "*" || op == "/") {
                PREC_PROD
            } else {
                PREC_JUXT
            };
            (format!("-{}", paren(t, p, need).0), PREC_NEG)
        }
        K::BinOp { op, left, right, .. } => match op.as_str() {
            "+" | "-" => {
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
            "*" => {
                let (l, lp) = src(left, pretty);
                let (r, rp) = src(right, pretty);
                if left.is_num() && right.name() == Some("𝑖") && lp >= PREC_JUXT {
                    return (format!("{l}i"), PREC_JUXT);
                }
                if lp >= PREC_JUXT && rp >= PREC_JUXT && !r.starts_with('-') {
                    let mut sep = " ";
                    let rname_nonunit = matches!(&right.kind, K::Name { name } if !is_unit_name(name));
                    let rpow_nonunit = matches!(&right.kind, K::BinOp { op, left: rl, .. }
                        if op == "^" && matches!(&rl.kind, K::Name { name } if !is_unit_name(name)));
                    if matches!(right.kind, K::Num { .. } | K::Quantity { .. }) {
                        sep = if pretty { "·" } else { "*" };
                    } else if left.is_num() && pretty && (rname_nonunit || rpow_nonunit) {
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
            "/" => {
                let (l, lp) = src(left, pretty);
                let (r, rp) = src(right, pretty);
                let (l, _) = paren(l, lp, PREC_PROD);
                let r = if rp <= PREC_JUXT { format!("({r})") } else { r };
                (format!("{l}/{r}"), PREC_PROD)
            }
            "^" => {
                let (l, lp) = src(left, pretty);
                let (l, _) = paren(l, lp, PREC_POW + 1);
                if let Some(v) = right.num_value() {
                    if v.is_finite() && v == v.trunc() && pretty {
                        return (format!("{l}{}", superscript(&int_text(v))), PREC_POW);
                    }
                    if v.is_finite() && v != v.trunc() {
                        if let Some((p, q)) = limit_denominator(v, 99) {
                            if ((p as f64) / (q as f64) - v).abs() < 1e-12 {
                                return (format!("{l}^({p}/{q})"), PREC_POW);
                            }
                        }
                    }
                }
                let (r, rp) = src(right, pretty);
                let (r, _) = paren(r, rp, PREC_ATOM);
                (format!("{l}^{r}"), PREC_POW)
            }
            _ => ("<BinOp>".to_string(), PREC_ATOM),
        },
        K::Sqrt { operand, root } => {
            let fname = if pretty {
                if *root == 2 { "√" } else { "∛" }
            } else if *root == 2 {
                "sqrt"
            } else {
                "cbrt"
            };
            (format!("{fname}({})", s(operand)), PREC_ATOM)
        }
        K::Abs { operand } => (format!("|{}|", s(operand)), PREC_ATOM),
        K::Call { func, args } => {
            let a: Vec<String> = args.iter().map(s).collect();
            (format!("{}({})", s(func), a.join(", ")), PREC_ATOM)
        }
        K::Prime { target, order } => (format!("{}{}", s(target), "'".repeat((*order).max(0) as usize)), PREC_ATOM),
        K::Index { target, index } => {
            (format!("{}[{}]", s(target), index.as_ref().map(|i| s(i)).unwrap_or_else(|| "None".into())), PREC_ATOM)
        }
        K::Field { target, name } => (format!("{}.{name}", s(target)), PREC_ATOM),
        K::Compare { op, left, right, tol } => {
            let t = tol.as_ref().map(|t| format!(" within {}", s(t))).unwrap_or_default();
            (format!("{} {op} {}{t}", s(left), s(right)), 0)
        }
        K::Logic { op, left, right } => (format!("{} {op} {}", s(left), s(right)), 0),
        K::Not { operand } => (format!("not {}", s(operand)), 0),
        K::IfExpr { cond, then, other } => (format!("if {} then {} else {}", s(cond), s(then), s(other)), 0),
        K::ListLit { items } => (format!("[{}]", items.iter().map(s).collect::<Vec<_>>().join(", ")), PREC_ATOM),
        K::VecLit { items } => (format!("<{}>", items.iter().map(s).collect::<Vec<_>>().join(", ")), PREC_ATOM),
        K::Deriv { var, operand, partial, .. } => {
            let d = if *partial && pretty {
                "∂"
            } else if *partial {
                "partial"
            } else {
                "d"
            };
            (format!("{d}/{d}{var} {}", s(operand)), PREC_JUXT)
        }
        K::Integral { integrand, var, lo, hi } => {
            let mut t = format!("{} {} d{var}", if pretty { "∫" } else { "integral" }, s(integrand));
            if let Some(lo) = lo {
                t += &format!(" from {} to {}", s(lo), hi.as_ref().map(|h| s(h)).unwrap_or_default());
            }
            (t, 0)
        }
        K::Sum { body, var, lo, hi, step } => {
            let mut t = format!("{}({} for {var} from {} to {}", if pretty { "Σ" } else { "sum" }, s(body), s(lo), s(hi));
            if let Some(st) = step {
                t += &format!(" step {}", s(st));
            }
            (t + ")", PREC_ATOM)
        }
        K::Convert { value, unit } => (format!("{} in {}", s(value), unit.text), 0),
        K::Where { value, bindings } => {
            let b = bindings.iter().map(|(n, v)| format!("{n} = {}", s(v))).collect::<Vec<_>>().join(", ");
            (format!("{} where {b}", s(value)), 0)
        }
        K::End => ("end".into(), PREC_ATOM),
        K::Slice { lo, hi } => {
            let l = lo.as_ref().map(|x| s(x)).unwrap_or_default();
            let h = hi.as_ref().map(|x| s(x)).unwrap_or_default();
            (format!("{l}:{h}"), PREC_ATOM)
        }
        K::Load { path } => (format!("load \"{path}\""), PREC_ATOM),
        K::Table { names, items } => {
            let cols = names.iter().zip(items).map(|(n, v)| format!("{n} = {}", s(v))).collect::<Vec<_>>().join(", ");
            (format!("table({cols})"), PREC_ATOM)
        }
        _ => (format!("<{}>", e.class()), PREC_ATOM),
    }
}

/// Is `e` a number (for callers that only need the test)?
pub fn is_number(e: &A::Expr) -> bool {
    is_num(e, None)
}

mod num_rational_lite {
    /// Python `Fraction(v).limit_denominator(max_den)` when a fraction within 1e-12 exists: the one with the
    /// smallest denominator (then it is the closest, since two fractions with denominators ≤ 99 differ by
    /// more than 1e-4).
    pub fn limit_denominator(v: f64, max_den: i64) -> Option<(i64, i64)> {
        for q in 1..=max_den {
            let p = (v * q as f64).round();
            if (p / q as f64 - v).abs() < 1e-12 {
                return Some((p as i64, q));
            }
        }
        None
    }
}
