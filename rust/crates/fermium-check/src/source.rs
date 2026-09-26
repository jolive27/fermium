//! Source text of an expression, for messages (Python calculus.to_source).
//! PARTIAL: numbers, names, quantities and operators; the full port (precedence rules, juxtaposition
//! separators, calculus notation) belongs with fermium-sym, which needs it too.
use fermium_syntax::ast as A;

pub fn to_source(e: &A::Expr) -> String {
    src(e).0
}

const ATOM: u8 = 9;
const POW: u8 = 7;
const JUXT: u8 = 5;
const NEG: u8 = 4;
const PROD: u8 = 3;
const SUM: u8 = 2;

fn paren(s: String, p: u8, need: u8) -> (String, u8) {
    if p < need { (format!("({s})"), ATOM) } else { (s, p) }
}

fn src(e: &A::Expr) -> (String, u8) {
    use A::ExprKind as K;
    match &e.kind {
        K::Num { value, .. } => {
            let v = *value;
            let s = if v == v.trunc() && v.abs() < 1e15 {
                format!("{}", v as i64)
            } else {
                crate::units::format_number(v, Some(12))
            };
            (s, if v >= 0.0 { ATOM } else { NEG })
        }
        K::Name { name: n } => (n.clone(), ATOM),
        K::Str { value: s } => (format!("\"{s}\""), ATOM),
        K::Bool { value: b } => ((if *b { "true" } else { "false" }).into(), ATOM),
        K::Quantity { value, unit, bracket } => {
            let v = src(value).0;
            (if *bracket { format!("{v} [{}]", unit.text) } else { format!("{v} {}", unit.text) }, JUXT)
        }
        K::Neg { operand: x } => {
            let (s, p) = src(x);
            let need = if matches!(&x.kind, K::BinOp { op, .. } if op == "*" || op == "/") { PROD } else { JUXT };
            (format!("-{}", paren(s, p, need).0), NEG)
        }
        K::BinOp { op, left, right, .. } => {
            let (l, lp) = src(left);
            let (r, rp) = src(right);
            match op.as_str() {
                "+" | "-" => {
                    let (mut r, rp) = paren(r, rp, PROD);
                    if op == "-" && rp == SUM {
                        r = format!("({r})");
                    }
                    (format!("{l} {} {r}", op), SUM)
                }
                "*" | "×" => {
                    if op == "*" && matches!(left.kind, K::Num { .. })
                        && matches!(&right.kind, K::Name { name } if name == "𝑖") && lp >= JUXT {
                        return (format!("{l}i"), JUXT); // the imaginary literal 4i (D90)
                    }
                    if lp >= JUXT && rp >= JUXT && !r.starts_with('-') {
                        let sep = if matches!(right.kind, K::Num { .. } | K::Quantity { .. }) { "·" } else { " " };
                        return (format!("{l}{sep}{r}"), JUXT);
                    }
                    (format!("{}·{}", paren(l, lp, PROD).0, paren(r, rp, NEG + 1).0), PROD)
                }
                "/" => {
                    let l = paren(l, lp, PROD).0;
                    let r = if rp <= JUXT { format!("({r})") } else { r };
                    (format!("{l}/{r}"), PROD)
                }
                _ => (format!("{}^{}", paren(l, lp, POW + 1).0, paren(r, rp, POW).0), POW),
            }
        }
        K::Call { func, args } => {
            let a: Vec<String> = args.iter().map(to_source).collect();
            (format!("{}({})", src(func).0, a.join(", ")), ATOM)
        }
        K::Compare { op, left, right, .. } => {
            let o = match op.as_str() {
                "!=" => "≠",
                "<=" => "≤",
                ">=" => "≥",
                "~=" => "≈",
                x => x,
            };
            (format!("{} {o} {}", to_source(left), to_source(right)), 1)
        }
        _ => ("…".into(), ATOM),
    }
}
