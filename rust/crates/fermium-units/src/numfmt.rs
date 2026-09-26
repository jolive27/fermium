//! Number printing: significant figures and pretty exponents (port of `format_number`, `format_default`,
//! `_whole`, `format_default_seq` from fermium/units.py, `format_written` from fermium/runtime/core.py and
//! `format_pm` from fermium/uncertain.py).
//!
//! This module is self-contained (std only): build.rs includes it to print the unit database's factors
//! exactly as Fermium 1.5 does.
//!
//! Rust's `{:.N}` / `{:.Ne}` formatting is exact (correctly rounded from the binary value, ties to even),
//! like Python's `f"{x:.Nf}"` / `f"{x:.Ne}"`, so the decimal rounding is the same as CPython's.

#![allow(dead_code)]

/// Output precision when the program doesn't give one (DECISIONS D11).
pub const DEFAULT_SF: i64 = 3;

fn sup_digits(s: &str) -> String {
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

/// `×10ⁿ` for an integer exponent.
pub fn times_ten(exp: i64) -> String {
    format!("×10{}", sup_digits(&exp.to_string()))
}

fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

/// Python's `f"{x:.{p}e}"` split into (mantissa text, exponent).
fn sci_parts(x: f64, p: usize) -> (String, i64) {
    let s = format!("{:.*e}", p, x);
    let (m, e) = s.split_once('e').expect("exponent");
    (m.to_string(), e.parse().expect("exponent"))
}

/// `str(int(x))` for a finite x whose integer part fits in i64.
fn int_text(x: f64) -> String {
    (x as i64).to_string()
}

/// Python's `round(x)` (ties to even) as an integer text, for |x| < 2⁶³.
pub fn round_text(x: f64) -> String {
    (x.round_ties_even() as i64).to_string()
}

/// `format_number(x, sig, trim)`: `sig` significant figures (clamped to 1..17); `trim` drops trailing zeros
/// and prints whole numbers below 10⁷ exactly when sig ≥ 6. Fixed notation unless rounding to `sig` figures
/// would leave two or more non-significant zeros before the decimal point (D11); then `m×10ⁿ`.
pub fn format_number(x: f64, sig: i64, trim: bool) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    if trim && x == x.trunc() && x.abs() < 1e7 && sig >= 6 {
        return int_text(x);
    }
    let sig = sig.clamp(1, 17);
    let (m, exp) = sci_parts(x, (sig - 1) as usize);
    if (-4..6).contains(&exp) && (trim || exp <= sig) {
        let decimals = (sig - 1 - exp).max(0) as usize;
        let v: f64 = format!("{m}e{exp}").parse().expect("float");
        let s = format!("{:.*}", decimals, v);
        return if trim { strip_trailing_zeros(&s) } else { s };
    }
    let m = if trim { strip_trailing_zeros(&m) } else { m };
    format!("{m}{}", times_ten(exp))
}

/// `format_number(x)` with Python's defaults (6 figures, trimmed).
pub fn format_number6(x: f64) -> String {
    format_number(x, 6, true)
}

/// `_whole`: a whole number below 10⁷, allowing for rounding in its last bits (1e-13 relative).
pub fn is_whole(x: f64) -> bool {
    if x == 0.0 {
        return true;
    }
    if x.is_nan() || x.abs() >= 1e7 {
        return false;
    }
    let r = x.round_ties_even();
    r != 0.0 && (x - r).abs() <= 1e-13 * x.abs()
}

/// `format_default(x, sig, whole_ok)`: a value whose precision the program doesn't give (D11).
pub fn format_default(x: f64, sig: i64, whole_ok: bool) -> String {
    if whole_ok && is_whole(x) {
        return round_text(x);
    }
    format_number(x, sig, false)
}

/// `format_default_seq(xs, sig)`: one style for a list/vector/matrix (whole numbers bare only if all are).
pub fn format_default_seq(xs: &[f64], sig: i64) -> Vec<String> {
    if xs.iter().all(|&x| is_whole(x) || !x.is_finite()) {
        return xs
            .iter()
            .map(|&x| if x.is_finite() { round_text(x) } else { format_number6(x) })
            .collect();
    }
    xs.iter().map(|&x| format_number(x, sig, false)).collect()
}

/// `format_written(x, sf, exact_items)`: an element of a list written out in the program.
pub fn format_written(x: f64, sf: i64, exact_items: bool) -> String {
    if exact_items && !x.is_nan() && x.abs() < 1e7 && x == x.trunc() {
        return int_text(x);
    }
    if !x.is_nan() && x.abs() < f64::INFINITY && x != 0.0 {
        let p = (sf.max(1) - 1) as usize;
        let back: f64 = format!("{:.*e}", p, x).parse().expect("float");
        if (back - x).abs() > 1e-13 * x.abs() {
            return format_number(x, 15, true);
        }
    }
    format_number(x, sf, false)
}

/// Python's `10 ** n` as a float (an int converted with correct rounding when n ≥ 0, `pow` when n < 0).
pub fn py_pow10(n: i64) -> f64 {
    if n >= 0 {
        format!("1e{n}").parse().expect("float")
    } else {
        10f64.powf(n as f64)
    }
}

/// Exact `x >= 10**n` (Python compares a float with an int exactly).
fn ge_pow10(x: f64, n: i64) -> bool {
    if n < 0 {
        return x >= 10f64.powf(n as f64);
    }
    if !(x >= 1.0) {
        return false;
    }
    if x.is_infinite() {
        return true;
    }
    let digits = format!("{:.0}", x.trunc()).len() as i64;
    digits > n
}

/// Python's `round(x, nd)` for a float: the exact value rounded to 10^-nd, ties to even.
pub fn py_round(x: f64, nd: i64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    if nd > 323 {
        return x;
    }
    if nd >= 0 {
        return format!("{:.*}", nd as usize, x).parse().expect("float");
    }
    let k = -nd; // round to a multiple of 10^k
    let a = x.abs();
    if a < 1.0 {
        return 0.0 * x;
    }
    let int_digits = format!("{:.0}", a.trunc());
    let e10 = int_digits.len() as i64 - 1;
    let keep = e10 + 1 - k;
    if keep < 0 {
        return 0.0 * x;
    }
    if keep == 0 {
        // |x| in [10^e10, 10^(e10+1)): 0 or 10^(e10+1), by comparison with 5×10^e10
        let half = format!("5{}", "0".repeat(e10 as usize));
        let up = match int_digits.cmp(&half) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => a.fract() != 0.0,
        };
        return if up { py_pow10(e10 + 1).copysign(x) } else { 0.0 * x };
    }
    format!("{:.*e}", (keep - 1) as usize, x).parse().expect("float")
}

/// `_round_sig`: (σ rounded to `sig` significant figures, its decimal exponent).
fn round_sig(s: f64, sig: i64) -> (f64, i64) {
    let mut e = s.log10().floor() as i64;
    let mut r = py_round(s, sig - 1 - e);
    if ge_pow10(r, e + 1) {
        e += 1;
        r = py_round(s, sig - 1 - e);
    }
    (r, e)
}

/// `format_pm(x, s)`: "5.00 ± 0.20", "(1.234 ± 0.056)×10⁻³"; the uncertainty keeps 2 significant figures
/// and the value is rounded to the same decimal place. Returns (text, scientific).
pub fn format_pm(x: f64, s: f64) -> (String, bool) {
    format_pm_sig(x, s, 2)
}

/// `format_pm` with `sig` significant figures of the uncertainty.
pub fn format_pm_sig(x: f64, s: f64, sig: i64) -> (String, bool) {
    if !(x.is_finite() && s.is_finite()) {
        return (format!("{} ± {}", format_number6(x), format_number6(s)), false);
    }
    if s == 0.0 {
        return (format!("{} ± 0", format_default(x, DEFAULT_SF, true)), false);
    }
    let (r, e) = round_sig(s, sig);
    let last = e - sig + 1;
    let xr = py_round(x, -last);
    let big = xr.abs().max(r);
    let ee = big.log10().floor() as i64;
    if -3 < ee && ee < 5 && last <= 0 {
        let nd = (-last) as usize;
        return (format!("{:.*} ± {:.*}", nd, xr, nd, r), false);
    }
    let nd = (ee - last).max(0) as usize;
    let p = py_pow10(ee);
    (format!("({:.*} ± {:.*}){}", nd, xr / p, nd, r / p, times_ten(ee)), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(format_number(333333.0, 3, false), "3.33×10⁵");
        assert_eq!(format_number(9549.0, 3, false), "9550");
        assert_eq!(format_number(0.5, 3, false), "0.500");
        assert_eq!(format_number(1048576.0, 6, true), "1048576");
        assert_eq!(format_number(3e8, 3, false), "3.00×10⁸");
        assert_eq!(format_default(20.0, 3, true), "20");
        assert_eq!(format_pm(5.0, 0.2).0, "5.00 ± 0.20");
    }
}
