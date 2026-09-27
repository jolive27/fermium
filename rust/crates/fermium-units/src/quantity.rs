//! Printing quantities: a number in its display unit, lists, vectors, matrices, complex numbers and
//! uncertain values (port of `format_quantity` and the print callbacks in fermium/runtime/core.py,
//! `format_complex` in fermium/cplx.py, `format_clist` in fermium/clist.py and `format_uncertain` in
//! fermium/uncertain.py).
//!
//! A print format mirrors Fermium 1.5's `fmts` entries: the value's dimension, an optional hint (the unit
//! it was written or converted in), the significant figures the program gives (`to N digits`, or the
//! fewest of a written literal) and the `direct` code:
//! - [`DIRECT_NO`] (0): a computed value; 3 significant figures unless the program says (D11)
//! - [`DIRECT_YES`] (1): a literal as written (`print 2.50 m`)
//! - 3: a list written out whose whole-number items print as written
//! - [`DIRECT_LOOP`] (4) / [`DIRECT_LOOP_EXACT`] (5): a loop variable over a written list (D242)

use crate::db::Unit;
use crate::dim::Dim;
use crate::display::{display_unit, preferred_unit};
use crate::numfmt::*;

pub const DIRECT_NO: u8 = 0;
pub const DIRECT_YES: u8 = 1;
pub const DIRECT_WRITTEN_EXACT: u8 = 3;
pub const DIRECT_LOOP: u8 = 4;
pub const DIRECT_LOOP_EXACT: u8 = 5;

/// A print format (one entry of Fermium 1.5's `tables.fmts`).
#[derive(Clone, Debug, PartialEq)]
pub struct PrintFmt {
    pub rdim: Dim,
    pub hint: Option<Unit>,
    pub sf: Option<i64>,
    pub direct: u8,
    pub echo: bool,
}

impl PrintFmt {
    pub fn new(rdim: Dim) -> PrintFmt {
        PrintFmt { rdim, hint: None, sf: None, direct: DIRECT_NO, echo: true }
    }
}

fn special_suffix(name: &str) -> bool {
    matches!(name, "°" | "%" | "′" | "″")
}

fn plain(name: &str) -> bool {
    name.is_empty() || name == "1"
}

/// Does a token of a unit name match `c([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+|\^-?\d+)?` (a speed-of-light unit, MeV/c²)?
fn is_c_token(tok: &str) -> bool {
    let Some(rest) = tok.strip_prefix('c') else { return false };
    if rest.is_empty() {
        return true;
    }
    if let Some(r) = rest.strip_prefix('^') {
        let r = r.strip_prefix('-').unwrap_or(r);
        return !r.is_empty() && r.chars().all(|c| c.is_ascii_digit());
    }
    rest.chars().all(|c| "⁰¹²³⁴⁵⁶⁷⁸⁹⁻".contains(c))
}

fn has_c_unit(name: &str) -> bool {
    name.split(|c: char| c.is_whitespace() || "/()·*".contains(c)).any(is_c_token)
}

/// `str(int(x))` if |x| < 1e15 and whole, else 15 significant figures (a literal as written).
fn literal_text(x: f64) -> String {
    if x.abs() < 1e15 && x == x.trunc() {
        (x as i64).to_string()
    } else {
        format_number(x, 15, true)
    }
}

/// `format_quantity(v, dim, hint, sf, direct, echo, whole_ok)`: an SI value printed in its display unit.
pub fn format_quantity(v: f64, dim: &Dim, hint: Option<&Unit>, sf: Option<i64>, direct: u8, echo: bool, whole_ok: bool) -> String {
    let u = display_unit(dim, hint);
    let is_hint = hint.is_some_and(|h| h.dim == *dim);
    let x = (v - u.offset) / u.factor;
    let s = match sf {
        None => {
            if direct != 0 {
                literal_text(x)
            } else {
                format_default(x, DEFAULT_SF, whole_ok)
            }
        }
        Some(sf) if direct == DIRECT_LOOP || direct == DIRECT_LOOP_EXACT => format_written(x, sf, direct == DIRECT_LOOP_EXACT),
        Some(sf) => format_number(x, if direct != 0 { sf } else { sf.max(2) }, false),
    };
    let name = &u.name;
    if plain(name) {
        return s;
    }
    if special_suffix(name) {
        return format!("{s}{name}");
    }
    if echo && is_hint && direct != 0 && name != "c" && has_c_unit(name) {
        let si = preferred_unit(dim);
        let sig = match sf {
            Some(n) if n != 0 => {
                if direct != 0 {
                    n
                } else {
                    n.max(2)
                }
            }
            _ => DEFAULT_SF,
        };
        return format!("{s} {name} (= {} {})", format_number(v, sig, false), si.name);
    }
    format!("{s} {name}")
}

/// A number printed with a print format (`print x`, `str(x)`).
pub fn format_value(v: f64, f: &PrintFmt) -> String {
    format_quantity(v, &f.rdim, f.hint.as_ref(), f.sf, f.direct, f.echo, true)
}

fn truthy(sf: Option<i64>) -> Option<i64> {
    sf.filter(|&n| n != 0)
}

/// A list: `[1, 2, 3] m`, `[0.500, 1.00, 1.50] s`; longer than 12 shows the first 5 and last 3 values.
pub fn format_list(vals_si: &[f64], f: &PrintFmt) -> String {
    let u = display_unit(&f.rdim, f.hint.as_ref());
    let n = vals_si.len();
    let mut vals: Vec<f64> = vals_si.iter().map(|&p| (p - u.offset) / u.factor).collect();
    let mut sf = f.sf;
    if f.direct == 0 {
        sf = sf.map(|s| s.max(2));
    }
    if n > 12 {
        let mut v = vals[..5].to_vec();
        v.extend_from_slice(&vals[n - 3..]);
        vals = v;
    }
    let mut shown: Vec<String> = match truthy(sf) {
        Some(sf) if f.direct == DIRECT_YES || f.direct == DIRECT_WRITTEN_EXACT => {
            vals.iter().map(|&x| format_written(x, sf, f.direct == DIRECT_WRITTEN_EXACT)).collect()
        }
        Some(sf) => vals.iter().map(|&x| format_number(x, sf, false)).collect(),
        None => format_default_seq(&vals, DEFAULT_SF),
    };
    if n > 12 {
        shown.insert(5, "…".into());
    }
    let mut s = format!("[{}]", shown.join(", "));
    if !plain(&u.name) {
        s.push(' ');
        s.push_str(&u.name);
    }
    if n > 12 {
        s.push_str(&format!("  ({n} values)"));
    }
    s
}

/// The entries of a vector or matrix in one style (`seq_values`), and the unit suffix.
fn seq_values(f: &PrintFmt, p: &[f64]) -> (Vec<String>, String) {
    let u = display_unit(&f.rdim, f.hint.as_ref());
    let xs: Vec<f64> = p.iter().map(|&v| v / u.factor).collect();
    let vals = match truthy(f.sf) {
        Some(sf) => xs
            .iter()
            .map(|&x| {
                if f.direct == DIRECT_YES || f.direct == DIRECT_WRITTEN_EXACT {
                    format_written(x, sf, f.direct == DIRECT_WRITTEN_EXACT)
                } else if f.direct != 0 {
                    format_number(x, sf, false)
                } else {
                    format_number(x, sf.max(2), false)
                }
            })
            .collect(),
        None => format_default_seq(&xs, DEFAULT_SF),
    };
    let unit = if plain(&u.name) { String::new() } else { format!(" {}", u.name) };
    (vals, unit)
}

/// A vector: `<1, 2, 3> m`.
pub fn format_vec(p: &[f64], f: &PrintFmt) -> String {
    let (vals, unit) = seq_values(f, p);
    format!("<{}>{unit}", vals.join(", "))
}

/// A list of vectors or matrices (spec C1, D281): `[<1, 2>, <3, 4>] m`, `[[[1, 0], [0, 1]], …] N/m`. Every entry
/// in one number style and the unit once, as for a matrix; `k` numbers per element (a vector of k components, or
/// an r×c matrix with `cols` = c). More than 12 elements show the first 5 and the last 3, like a list.
pub fn format_vlist(p: &[f64], k: usize, cols: Option<usize>, f: &PrintFmt) -> String {
    let n = if k == 0 { 0 } else { p.len() / k };
    let (vals, unit) = seq_values(f, p);
    let elem = |i: usize| -> String {
        let v = &vals[i * k..i * k + k];
        match cols {
            Some(c) if c > 0 => {
                let rows: Vec<String> = v.chunks(c).map(|r| format!("[{}]", r.join(", "))).collect();
                format!("[{}]", rows.join(", "))
            }
            _ => format!("<{}>", v.join(", ")),
        }
    };
    let mut shown: Vec<String> = if n > 12 {
        (0..5).chain(n - 3..n).map(elem).collect()
    } else {
        (0..n).map(elem).collect()
    };
    if n > 12 {
        shown.insert(5, "…".into());
    }
    let mut s = format!("[{}]{unit}", shown.join(", "));
    if n > 12 {
        s.push_str(&format!("  ({n} {})", if cols.is_some() { "matrices" } else { "vectors" }));
    }
    s
}

/// A vector with a unit per component: `<1 m, 2 m/s>` (whole numbers bare only if every default one is).
pub fn format_mvec(p: &[f64], fs: &[PrintFmt]) -> String {
    let mut xs = Vec::new();
    for (i, f) in fs.iter().enumerate() {
        if f.sf.is_none() && f.direct == 0 {
            let u = display_unit(&f.rdim, f.hint.as_ref());
            xs.push((p[i] - u.offset) / u.factor);
        }
    }
    let whole = xs.iter().all(|&x| is_whole(x) || !x.is_finite());
    let parts: Vec<String> = fs
        .iter()
        .enumerate()
        .map(|(i, f)| format_quantity(p[i], &f.rdim, f.hint.as_ref(), f.sf, f.direct, true, whole))
        .collect();
    format!("<{}>", parts.join(", "))
}

/// A matrix (row-major values): `[[1, 2], [3, 4]] m`, one number style for the whole matrix.
pub fn format_mat(p: &[f64], r: usize, c: usize, f: &PrintFmt) -> String {
    let (vals, unit) = seq_values(f, &p[..r * c]);
    let rows: Vec<String> = (0..r).map(|i| format!("[{}]", vals[i * c..i * c + c].join(", "))).collect();
    format!("[{}]{unit}", rows.join(", "))
}

fn denoise_pair(x: f64, y: f64) -> (f64, f64) {
    let size = x.hypot(y);
    if size.is_finite() && size > 0.0 {
        (if x.abs() < 1e-14 * size { 0.0 } else { x }, if y.abs() < 1e-14 * size { 0.0 } else { y })
    } else {
        (x, y)
    }
}

fn complex_body(x: f64, y: f64, f: &dyn Fn(f64) -> String) -> String {
    let sign = if y < 0.0 { '-' } else { '+' };
    let im = if y.is_nan() { "NaN".to_string() } else { f(y.abs()) };
    format!("{} {sign} {im}i", f(x))
}

/// A complex number: `3 + 4i`, `(3 + 4i) Ω`; a part below 10⁻¹⁴ |z| prints as 0 (D94).
pub fn format_complex(re: f64, im: f64, dim: &Dim, hint: Option<&Unit>, sf: Option<i64>, direct: u8) -> String {
    let u = display_unit(dim, hint);
    let (x, y) = denoise_pair(re / u.factor, im / u.factor);
    let whole = is_whole(x) && is_whole(y);
    let f = |v: f64| -> String {
        match sf {
            None => {
                if direct != 0 {
                    format_number(v, 15, true)
                } else if whole {
                    round_text(v)
                } else {
                    format_number(v, DEFAULT_SF, false)
                }
            }
            Some(sf) => format_number(v, if direct != 0 { sf } else { sf.max(2) }, false),
        }
    };
    let body = complex_body(x, y, &f);
    let name = &u.name;
    if plain(name) {
        body
    } else if special_suffix(name) {
        format!("({body}){name}")
    } else {
        format!("({body}) {name}")
    }
}

/// A list of complex numbers: `[3 + 0i, -1 + 1i] V`; `n_total` is the full length when `pairs` holds only
/// the shown elements (the first 5 and last 3 of a list longer than 12).
pub fn format_clist(pairs: &[(f64, f64)], dim: &Dim, hint: Option<&Unit>, sf: Option<i64>, direct: u8, n_total: Option<usize>) -> String {
    let u = display_unit(dim, hint);
    let n = n_total.unwrap_or(pairs.len());
    let vals: Vec<(f64, f64)> = pairs.iter().map(|&(a, b)| denoise_pair(a / u.factor, b / u.factor)).collect();
    let shown: Vec<(f64, f64)> = if n > 12 {
        let mut v = vals[..5.min(vals.len())].to_vec();
        v.extend_from_slice(&vals[vals.len().saturating_sub(3)..]);
        v
    } else {
        vals
    };
    let whole = shown.iter().all(|&(a, b)| is_whole(a) && is_whole(b));
    let f = |v: f64| -> String {
        match sf {
            None => {
                if whole {
                    round_text(v)
                } else {
                    format_number(v, DEFAULT_SF, false)
                }
            }
            Some(sf) => format_number(v, if direct != 0 { sf } else { sf.max(2) }, false),
        }
    };
    let mut texts: Vec<String> = shown.iter().map(|&(x, y)| complex_body(x, y, &f)).collect();
    if n > 12 {
        texts.insert(5.min(texts.len()), "…".into());
    }
    let mut s = format!("[{}]", texts.join(", "));
    if !plain(&u.name) {
        s.push(' ');
        s.push_str(&u.name);
    }
    if n > 12 {
        s.push_str(&format!("  ({n} values)"));
    }
    s
}

/// An uncertain quantity (value and standard uncertainty in SI): `9.81 ± 0.12 m/s²`.
pub fn format_uncertain(v: f64, s: f64, dim: &Dim, hint: Option<&Unit>) -> String {
    let un = display_unit(dim, hint);
    let x = (v - un.offset) / un.factor;
    let s = s / un.factor.abs();
    let (text, sci) = format_pm(x, s);
    let name = &un.name;
    if plain(name) {
        text
    } else if special_suffix(name) {
        if sci {
            format!("{text}{name}")
        } else {
            format!("({text}){name}")
        }
    } else {
        format!("{text} {name}")
    }
}

/// A list with uncertain entries (`(value, Some(σ))`) and plain ones (`(value, None)`), in one unit:
/// `[1.00 ± 0.10, 2.00 ± 0.20] m`.
pub fn format_uncertain_list(vals: &[(f64, Option<f64>)], un: &Unit) -> String {
    let parts: Vec<String> = vals
        .iter()
        .map(|&(v, s)| match s {
            Some(s) => format_pm((v - un.offset) / un.factor, s / un.factor.abs()).0,
            None => format_number((v - un.offset) / un.factor, 6, true),
        })
        .collect();
    let s = format!("[{}]", parts.join(", "));
    if plain(&un.name) {
        s
    } else {
        format!("{s} {}", un.name)
    }
}

/// A vector or matrix with uncertain entries (Fermium 2.5, C7): each entry `(value, Some(σ))` or plain
/// `(value, None)` in SI. One format for all entries: `<1.00 ± 0.10, 2.00 ± 0.20> m`, a matrix
/// `[[1.00 ± 0.10, 2], [3, 4.00 ± 0.10]] N/m` (`shape` = rows, columns); one format per component (a state
/// vector): `<1.00 ± 0.10 m, 2.0 ± 0.5 m/s>`. Uncertain entries round as a single uncertain number does (the
/// uncertainty to 2 significant figures); plain entries as in a list of uncertain values (6 figures).
pub fn format_uncertain_seq(vals: &[(f64, Option<f64>)], fmts: &[PrintFmt], shape: Option<(usize, usize)>) -> String {
    // in one unit, an entry whose value and uncertainty are both below 10⁻¹⁴ of the largest entry is rounding
    // noise (D197): 0 ± 0
    let big = vals.iter().map(|(v, _)| v.abs()).fold(0.0, f64::max);
    let denoised: Vec<(f64, Option<f64>)>;
    let vals = if fmts.len() == 1 && big.is_finite() && big > 0.0 {
        denoised = vals
            .iter()
            .map(|&(v, s)| {
                if v.abs() < 1e-14 * big && s.is_none_or(|s| s < 1e-14 * big) { (0.0, s.map(|_| 0.0)) } else { (v, s) }
            })
            .collect();
        &denoised[..]
    } else {
        vals
    };
    if fmts.len() > 1 && fmts.len() == vals.len() {
        let parts: Vec<String> = vals
            .iter()
            .zip(fmts)
            .map(|(&(v, s), f)| match s {
                Some(s) => format_uncertain(v, s, &f.rdim, f.hint.as_ref()),
                None => {
                    let u = display_unit(&f.rdim, f.hint.as_ref());
                    let t = format_number((v - u.offset) / u.factor, 6, true);
                    if plain(&u.name) { t } else { format!("{t} {}", u.name) }
                }
            })
            .collect();
        return format!("<{}>", parts.join(", "));
    }
    let f = &fmts[0];
    let u = display_unit(&f.rdim, f.hint.as_ref());
    let parts: Vec<String> = vals
        .iter()
        .map(|&(v, s)| match s {
            Some(s) => format_pm(v / u.factor, s / u.factor.abs()).0,
            None => format_number(v / u.factor, 6, true),
        })
        .collect();
    let body = match shape {
        Some((r, c)) if r * c == parts.len() => {
            let rows: Vec<String> = (0..r).map(|i| format!("[{}]", parts[i * c..i * c + c].join(", "))).collect();
            format!("[{}]", rows.join(", "))
        }
        _ => format!("<{}>", parts.join(", ")),
    };
    if plain(&u.name) { body } else { format!("{body} {}", u.name) }
}
