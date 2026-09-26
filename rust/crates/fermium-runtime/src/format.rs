//! Number formatting shared by the run-time pieces (v1's `units.format_number`).

/// v1's `format_number(x, sig, trim)` (units.py; aot_rt.c `fmt_num`): fixed notation unless
/// rounding would leave non-significant zeros, else `m×10ⁿ`.
pub fn format_number(x: f64, sig: usize, trim: bool) -> String {
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
        return format!("{}", x as i64);
    }
    let sig = sig.clamp(1, 17);
    let s = format!("{:.*e}", sig - 1, x);
    let (m, e) = s.split_once('e').unwrap();
    let exp: i32 = e.parse().unwrap();
    let trim_zeros = |t: &str| -> String {
        if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t.to_string() }
    };
    if (-4..6).contains(&exp) && (trim || exp <= sig as i32) {
        let decimals = (sig as i32 - 1 - exp).max(0) as usize;
        let r: f64 = format!("{m}e{e}").parse().unwrap();
        let s = format!("{:.*}", decimals, r);
        return if trim { trim_zeros(&s) } else { s };
    }
    let m = if trim { trim_zeros(m) } else { m.to_string() };
    format!("{m}×10{}", superscript(exp))
}

/// "−12" as "⁻¹²"
pub fn superscript(n: i32) -> String {
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string().chars().map(|c| if c == '-' { '⁻' } else { SUP[c.to_digit(10).unwrap() as usize] }).collect()
}

