//! Python's fractions.Fraction helpers the compiler needs, reproduced exactly.

/// Python `Fraction(x).limit_denominator(max_den)` as (numerator, denominator), exactly; None for inf/NaN or
/// huge values.
pub fn limit_denominator(x: f64, max_den: i64) -> Option<(i128, i128)> {
    if !x.is_finite() {
        return None;
    }
    // the exact value of the float as a fraction p/q (q a power of two), then Python's algorithm
    let (mant, exp, sign) = integer_decode(x);
    let (mut p, mut q): (i128, i128);
    if exp >= 0 {
        if exp > 60 {
            return None;
        }
        p = (mant as i128) << exp;
        q = 1;
    } else {
        let e = -exp;
        if e > 120 {
            return Some((0, 1));
        }
        p = mant as i128;
        q = 1i128 << e;
        let g = gcd(p, q);
        p /= g;
        q /= g;
    }
    p *= sign as i128;
    let md = max_den as i128;
    if q <= md {
        return Some((p, q));
    }
    let (n0, d0) = (p, q);
    let (mut p0, mut q0, mut p1, mut q1) = (0i128, 1i128, 1i128, 0i128);
    let (mut n, mut d) = (n0, d0);
    loop {
        let a = n.div_euclid(d);
        let q2 = q0 + a * q1;
        if q2 > md {
            break;
        }
        (p0, q0, p1, q1) = (p1, q1, p0 + a * p1, q2);
        (n, d) = (d, n - a * d);
        if d == 0 {
            break;
        }
    }
    let k = (md - q0) / q1;
    let (b1n, b1d) = (p0 + k * p1, q0 + k * q1);
    let (b2n, b2d) = (p1, q1);
    // pick the closer bound to n0/d0 (ties: the second, as in Python)
    let dist = |an: i128, ad: i128| ((an * d0 - n0 * ad).abs(), ad * d0);
    let (x1, y1) = dist(b2n, b2d);
    let (x2, y2) = dist(b1n, b1d);
    if x1 * y2 <= x2 * y1 {
        Some((b2n, b2d))
    } else {
        Some((b1n, b1d))
    }
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn integer_decode(x: f64) -> (u64, i32, i64) {
    let bits = x.to_bits();
    let sign = if bits >> 63 == 0 { 1 } else { -1 };
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = if exponent == 0 { (bits & 0xfffffffffffff) << 1 } else { (bits & 0xfffffffffffff) | 0x10000000000000 };
    if mantissa == 0 {
        return (0, 0, sign);
    }
    let mut m = mantissa;
    let mut e = exponent - 1075;
    while m & 1 == 0 {
        m >>= 1;
        e += 1;
    }
    (m, e, sign)
}


#[cfg(test)]
mod tests {
    #[test]
    fn like_python() {
        use super::limit_denominator as ld;
        assert_eq!(ld(1.0 / 3.0, 10000), Some((1, 3)));
        assert_eq!(ld(std::f64::consts::PI, 10000), Some((355, 113)));
        assert_eq!(ld(-0.75, 99), Some((-3, 4)));
        assert_eq!(ld(1.0 / 17.0, 99), Some((1, 17)));
    }
}
