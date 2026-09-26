//! "x isn't defined" with the best hint: a port of Checker.undefined, _unit_only_ancestor, _loose and the
//! difflib matching it uses (Python's get_close_matches, ported exactly so the suggestions agree).
use std::collections::{BTreeSet, HashMap, HashSet};

use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::units;

pub const KEYWORDS: &[&str] = &[
    "if", "else", "elif", "then", "for", "from", "to", "step", "in", "while", "return", "break", "continue", "print",
    "plot", "vs", "solve", "with", "fit", "load", "and", "or", "not", "where", "true", "false", "integral", "partial",
    "sqrt", "cbrt", "assert", "nabla",
];

const GREEK: &[(&str, &str)] = &[
    ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"), ("epsilon", "ε"), ("zeta", "ζ"), ("eta", "η"),
    ("theta", "θ"), ("iota", "ι"), ("kappa", "κ"), ("lambda", "λ"), ("mu", "μ"), ("nu", "ν"), ("xi", "ξ"),
    ("pi", "π"), ("rho", "ρ"), ("sigma", "σ"), ("tau", "τ"), ("upsilon", "υ"), ("phi", "φ"), ("chi", "χ"),
    ("psi", "ψ"), ("omega", "ω"),
];

/// Canonical spelling of an identifier: Greek names → letters (lexer.canonical_name).
pub fn canonical_name(raw: &str) -> String {
    raw.split('_')
        .map(|p| GREEK.iter().find(|(k, _)| *k == p).map(|(_, v)| *v).unwrap_or(p))
        .collect::<Vec<_>>()
        .join("_")
}

fn greek_to_ascii(c: char) -> String {
    let s = c.to_string();
    GREEK.iter().find(|(_, v)| *v == s).map(|(k, _)| k.to_string()).unwrap_or(s)
}

/// A spelling key that ignores case and underscores, with ☉/⊕ spelled out: M_sun, m_sun, Msun, M☉.
pub fn loose(name: &str) -> String {
    name.to_lowercase().replace('_', "").replace('☉', "sun").replace('⊕', "earth")
}

pub fn undefined(c: &mut Checker, name: &str, e: &A::Expr, ctx: &Ctx) -> Diagnostic {
    let unit_left = e.attrs.unit_left.as_deref();
    if name == "g" && unit_left.is_none() {
        // spec A3.2
        return c.err("g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: g = 9.81 m/s²",
                     e.span, Some("put  g = 9.81 m/s²  near the top of your program".into()));
    }
    let known = c.known_names(ctx.scope);
    let mut hint: Option<String> = None;
    // LT -> L T ?  (also omegat -> ω t)
    let chars: Vec<char> = name.chars().collect();
    for i in 1..chars.len() {
        let a = canonical_name(&chars[..i].iter().collect::<String>());
        let b = canonical_name(&chars[i..].iter().collect::<String>());
        if known.contains(&a) && known.contains(&b) {
            hint = Some(format!("did you mean {a} {b} ({a} times {b})? Fermium reads {name} as one name; put a space \
                                 between"));
            break;
        }
    }
    if hint.is_none() {
        hint = c.module_hint(name, ctx);
    }
    let is_unit = units::lookup_unit(name).is_some();
    if hint.is_none() && is_unit {
        if let Some(left) = unit_left {
            let lt = crate::source::to_source(left);
            hint = Some(if lt.chars().count() <= 24 {
                format!("{name} is a unit, and units go right after a number; to multiply {lt} by {name} write {lt} * 1 \
                         {name} or {lt} [{name}]")
            } else {
                format!("{name} is a unit, and units go right after a number; to multiply by it write * 1 {name} or \
                         [{name}] after the value")
            });
        }
    }
    if hint.is_none() && is_unit {
        if let Some(whole) = unit_only_ancestor(c, e, &known) {
            // rate = cm³/(mol s): a whole unit, used as a value (D163)
            let text = crate::source::to_source(&whole);
            return c.err(format!("{text} is a unit, not a value"), e.span,
                         Some(format!("for the quantity write  1 {text}  (a number, then its unit)")));
        }
        hint = Some(format!("{name} is a unit; units go right after a number, like 1 {name}, or in brackets [{name}]"));
    }
    let spelled = fermium_units::spelled_unit(name);
    if hint.is_none() && c.after_number {
        if let Some(sp) = spelled {
            // `3 sec`: the spelled unit, even though sec is also a built-in function (#63)
            return c.err(format!("'{name}' isn't a unit Fermium knows (or a variable you've defined)"), e.span,
                         Some(format!("Fermium writes units as symbols: {sp}")));
        }
    }
    if hint.is_none() {
        let mut cands: Vec<String> = known.iter().filter(|k| !k.starts_with("__")).cloned().collect();
        cands.sort(); // the Python set order is arbitrary; sorted keeps the choice reproducible
        let mut kw: Vec<&str> = KEYWORDS.to_vec();
        kw.sort();
        cands.extend(kw.iter().map(|s| s.to_string()));
        let mut bi: Vec<&str> = crate::builtins::BUILTINS
            .iter()
            .copied()
            .filter(|b| c.calling || !crate::builtins::M3_FUNCS.contains(b)) // speed ≠ seed (as a value)
            .collect();
        bi.sort();
        cands.extend(bi.iter().map(|s| s.to_string()));
        let lower = name.to_lowercase();
        let mut sorted_name: Vec<char> = name.chars().collect();
        sorted_name.sort();
        let first = name.chars().next();
        // `m_sun` is M_sun, not R_sun: a match up to case and underscores comes first (gauntlet #79);
        // a short name needs a closer match: `foo` is not a typo of floor (bootcamp B9)
        let close: Vec<String> = {
            let a: Vec<String> = cands.iter().filter(|k| k.to_lowercase() == lower).cloned().collect();
            if !a.is_empty() {
                a
            } else {
                let b: Vec<String> = cands.iter().filter(|k| loose(k) == loose(name)).cloned().collect();
                if !b.is_empty() {
                    b
                } else {
                    let cc: Vec<String> = cands
                        .iter()
                        .filter(|k| {
                            let mut s: Vec<char> = k.chars().collect();
                            s.sort();
                            k.chars().count() == chars.len() && s == sorted_name && k.chars().next() == first
                        })
                        .cloned()
                        .collect();
                    if !cc.is_empty() {
                        cc
                    } else {
                        get_close_matches(name, &cands, 3, 0.7)
                            .into_iter()
                            .filter(|k| {
                                chars.len() > 4
                                    || ((k.chars().count() as i64 - chars.len() as i64).abs() <= 1
                                        && k.chars().next() == first)
                            })
                            .take(1)
                            .collect()
                    }
                }
            }
        };
        if let Some(k) = close.first() {
            hint = Some(format!("did you mean {k}?"));
        }
    }
    if hint.is_none() && c.after_number {
        return c.err(format!("'{name}' isn't a unit Fermium knows (or a variable you've defined)"), e.span,
                     Some(match spelled {
                         Some(sp) => format!("Fermium writes units as symbols: {sp}"),
                         None => "see the list of units in docs/reference.md §15".into(),
                     }));
    }
    if hint.is_none() && c.calling {
        hint = Some(format!("define the function first, e.g.  {name}(x) = 2 x"));
    }
    if name == "%" {
        return c.err("% is the percent unit in Fermium (5 % = 0.05)", e.span,
                     Some("for the remainder of a division use mod(n, 2)".into()));
    }
    if crate::builtins::is_builtin(name) {
        return c.err(format!("{name} is a built-in function; call it with arguments like {name}(x)"), e.span, None);
    }
    // the constant g_0 is not a state of an eigenvalue problem (spec A3.2)
    let consts: HashSet<&str> = units::constants().iter().map(|k| k.name.as_str()).collect();
    let prefix = format!("{name}_");
    let mut states: Vec<(u64, String)> = known
        .iter()
        .filter(|k| {
            k.strip_prefix(&prefix).is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()))
                && !consts.contains(k.as_str())
        })
        .map(|k| (k[prefix.len()..].parse().unwrap_or(0), k.clone()))
        .collect();
    states.sort();
    if !states.is_empty() {
        // after an eigenvalue problem the states are ψ₁ … ψ_N (§20, red team 5 #12)
        let sub = |s: &str| -> String {
            s.chars().map(|ch| char::from_u32(0x2080 + ch.to_digit(10).unwrap()).unwrap()).collect()
        };
        let pretty: Vec<String> = states.iter().map(|(_, k)| format!("{name}{}", sub(&k[prefix.len()..]))).collect();
        let ascii1: String = states[0].1.chars().map(greek_to_ascii).collect();
        let list = pretty[..pretty.len().min(3)].join(", ");
        let tail = if pretty.len() > 3 { format!(" … {}", pretty.last().unwrap()) } else { String::new() };
        return c.err(format!("{name} isn't defined: the eigenvalue problem's states are {list}{tail}"), e.span,
                     Some(format!("use {} (ASCII: {ascii1}) for the lowest state, {}(x) for its value at x", pretty[0],
                                  pretty[0])));
    }
    c.err(format!("{name} isn't defined"), e.span,
          Some(hint.unwrap_or_else(|| format!("give it a value on an earlier line, e.g.  {name} = 2.5 m  (with its own \
                                               unit)"))))
}

/// The largest expression around the name e (being checked) made only of units that aren't program names, `*`,
/// `/` and whole-number powers, like cm³/(mol s); None if nothing larger than e.
fn unit_only_ancestor(c: &Checker, e: &A::Expr, known: &HashSet<String>) -> Option<A::Expr> {
    fn unit_only(n: &A::Expr, known: &HashSet<String>) -> bool {
        match &n.kind {
            A::ExprKind::Name(x) => !known.contains(x) && units::lookup_unit(x).is_some(),
            A::ExprKind::BinOp { op: A::BinOpKind::Mul | A::BinOpKind::Div, left, right, .. } => {
                unit_only(left, known) && unit_only(right, known)
            }
            A::ExprKind::BinOp { op: A::BinOpKind::Pow, left, right, .. } => {
                unit_only(left, known)
                    && match &right.kind {
                        A::ExprKind::Num { .. } => true,
                        A::ExprKind::Neg(x) => matches!(x.kind, A::ExprKind::Num { .. }),
                        _ => false,
                    }
            }
            _ => false,
        }
    }
    let st = &c.estack;
    if st.last().copied() != Some(e as *const A::Expr) {
        return None;
    }
    let mut best: Option<*const A::Expr> = None;
    for &n in st[..st.len() - 1].iter().rev() {
        // SAFETY: every pointer on the stack refers to an expression still being checked (see expr_any)
        let node = unsafe { &*n };
        if !unit_only(node, known) {
            break;
        }
        best = Some(n);
    }
    best.map(|p| unsafe { (*p).clone() })
}

// ---------------------------------------------------------------- difflib, as in CPython
struct Matcher<'a> {
    a: &'a [char],
    b: &'a [char],
    b2j: HashMap<char, Vec<usize>>,
}

impl<'a> Matcher<'a> {
    fn new(a: &'a [char], b: &'a [char]) -> Self {
        let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
        for (i, ch) in b.iter().enumerate() {
            b2j.entry(*ch).or_default().push(i);
        }
        // autojunk: popular elements are dropped only when len(b) >= 200
        let n = b.len();
        if n >= 200 {
            let ntest = n / 100 + 1;
            b2j.retain(|_, v| v.len() <= ntest);
        }
        Matcher { a, b, b2j }
    }

    fn find_longest_match(&self, alo: usize, ahi: usize, blo: usize, bhi: usize) -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0usize);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut newj2len: HashMap<usize, usize> = HashMap::new();
            if let Some(js) = self.b2j.get(&self.a[i]) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = j2len.get(&(j.wrapping_sub(1))).copied().unwrap_or(0) + 1;
                    let k = if j == 0 { 1 } else { k };
                    newj2len.insert(j, k);
                    if k > bestsize {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        bestsize = k;
                    }
                }
            }
            j2len = newj2len;
        }
        // no junk, so the extension loops for junk elements do nothing
        (besti, bestj, bestsize)
    }

    fn matching_total(&self) -> usize {
        let mut queue = vec![(0, self.a.len(), 0, self.b.len())];
        let mut total = 0;
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(alo, ahi, blo, bhi);
            if k > 0 {
                total += k;
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        total
    }

    fn ratio(&self) -> f64 {
        ratio_of(self.matching_total(), self.a.len() + self.b.len())
    }

    fn quick_ratio(&self) -> f64 {
        let mut avail: HashMap<char, i64> = HashMap::new();
        for ch in self.b {
            *avail.entry(*ch).or_insert(0) += 1;
        }
        let mut matches = 0;
        for ch in self.a {
            let n = avail.entry(*ch).or_insert(0);
            if *n > 0 {
                matches += 1;
            }
            *n -= 1;
        }
        ratio_of(matches, self.a.len() + self.b.len())
    }

    fn real_quick_ratio(&self) -> f64 {
        let (la, lb) = (self.a.len(), self.b.len());
        ratio_of(la.min(lb), la + lb)
    }
}

fn ratio_of(matches: usize, length: usize) -> f64 {
    if length > 0 { 2.0 * matches as f64 / length as f64 } else { 1.0 }
}

/// difflib.get_close_matches(word, possibilities, n, cutoff): best first; ties by the larger string (nlargest).
pub fn get_close_matches(word: &str, possibilities: &[String], n: usize, cutoff: f64) -> Vec<String> {
    let w: Vec<char> = word.chars().collect();
    let mut result: Vec<(f64, String)> = vec![];
    let mut seen = BTreeSet::new();
    for x in possibilities {
        let xc: Vec<char> = x.chars().collect();
        let m = Matcher::new(&xc, &w);
        if m.real_quick_ratio() >= cutoff && m.quick_ratio() >= cutoff {
            let r = m.ratio();
            if r >= cutoff {
                result.push((r, x.clone()));
                seen.insert(x.clone());
            }
        }
    }
    result.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then_with(|| b.1.cmp(&a.1)));
    result.into_iter().take(n).map(|(_, x)| x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_matches_like_python() {
        // difflib.get_close_matches("appel", ["ape", "apple", "peach", "puppy"]) == ['apple', 'ape']
        let p: Vec<String> = ["ape", "apple", "peach", "puppy"].iter().map(|s| s.to_string()).collect();
        assert_eq!(get_close_matches("appel", &p, 3, 0.6), vec!["apple", "ape"]);
        // get_close_matches("velocty", ["velocity", "vel", "v0"], 3, 0.7) == ['velocity']
        let p: Vec<String> = ["velocity", "vel", "v0"].iter().map(|s| s.to_string()).collect();
        assert_eq!(get_close_matches("velocty", &p, 3, 0.7), vec!["velocity"]);
    }
}
