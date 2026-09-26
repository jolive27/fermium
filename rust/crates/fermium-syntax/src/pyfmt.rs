//! Python's spellings of values, so messages and the oracle's S-expressions read exactly as Fermium 1.5 wrote
//! them: `repr(float)`, `f"{x:g}"`, `json.dumps(s, ensure_ascii=False)` and `int(s)`.

/// The shortest round-trip digits of a finite, non-zero |x| and its decimal exponent: x = 0.d1d2d3… × 10^exp10
/// (so `1.5` gives ("15", 1)).
fn shortest_digits(x: f64) -> (String, i32) {
    let s = format!("{:e}", x.abs()); // like 1.5e-5 or 1e16
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0').to_string();
    let digits = if digits.is_empty() { "0".to_string() } else { digits };
    (digits, exp + 1)
}

/// `repr(x)` of a Python float.
pub fn repr_float(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let (digits, decpt) = shortest_digits(x);
    // Python: fixed notation when -4 < decpt <= 16, else exponent notation
    let n = digits.len() as i32;
    let body = if decpt > -4 && decpt <= 16 {
        if decpt <= 0 {
            format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
        } else if decpt >= n {
            format!("{}{}.0", digits, "0".repeat((decpt - n) as usize))
        } else {
            format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
        }
    } else {
        let e = decpt - 1;
        let m = if n == 1 { digits.clone() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{}e{}{:02}", m, if e < 0 { "-" } else { "+" }, e.abs())
    };
    format!("{sign}{body}")
}

/// `f"{x:g}"`: 6 significant digits, trailing zeros removed, exponent when exp < -4 or exp >= 6.
pub fn fmt_g(x: f64) -> String {
    fmt_g_prec(x, 6)
}

pub fn fmt_g_prec(x: f64, prec: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let p = prec.max(1);
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let s = format!("{:.*e}", p - 1, x);
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= p as i32 {
        let mut m = mant.to_string();
        if m.contains('.') {
            m = m.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        format!("{}e{}{:02}", m, if exp < 0 { "-" } else { "+" }, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        let mut f = format!("{:.*}", decimals, x);
        if f.contains('.') {
            f = f.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        f
    }
}

/// `json.dumps(s, ensure_ascii=False)`.
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `int(s)` for the strings the lexer builds: an optional sign, then ASCII digits. None where Python raises.
pub fn py_int(s: &str) -> Option<i64> {
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for b in digits.bytes() {
        v = v.saturating_mul(10).saturating_add((b - b'0') as i64);
    }
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr() {
        for (x, s) in [
            (1.0, "1.0"),
            (0.5, "0.5"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (6.67e-11, "6.67e-11"),
            (2.5e19, "2.5e+19"),
            (123.456, "123.456"),
            (-0.0, "-0.0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1.2345678901234567e20, "1.2345678901234567e+20"),
        ] {
            assert_eq!(repr_float(x), s);
        }
    }

    #[test]
    fn g() {
        for (x, s) in [(1.0, "1"), (0.5, "0.5"), (2.5e19, "2.5e+19"), (123456.0, "123456"), (1234567.0, "1.23457e+06"),
                       (0.0001, "0.0001"), (0.00001, "1e-05"), (3.14159265, "3.14159")] {
            assert_eq!(fmt_g(x), s);
        }
    }
}
