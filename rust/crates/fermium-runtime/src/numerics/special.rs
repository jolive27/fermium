//! Special functions v1 exposes (`fermium/special.py`, and the libm calls of `codegen_llvm.py`):
//!
//! | function | here | v1 |
//! |---|---|---|
//! | `erf(x)`, `erfc(x)` | [`erf`], [`erfc`] (the `libm` crate: musl/fdlibm) | C library `erf`, `erfc` |
//! | `gamma(x)`, `lgamma(x)` | [`gamma`], [`lgamma`] (`libm`: musl tgamma, lgamma) | C `tgamma`, `lgamma` |
//! | `factorial(n)` | [`factorial`] = gamma(n + 1) | C `tgamma(n + 1)` |
//! | `besselj(n, x)`, `bessely(n, x)` | [`besselj`], [`bessely`] (`libm`: fdlibm jn, yn) | C `jn`, `yn` |
//! | `besseli(n, x)` | [`besseli`]: v1's positive power series, same operations | `fm_besseli` |
//! | `besselk(n, x)` | [`besselk`]: v1's trapezoidal rule on ∫ exp(−x cosh t) cosh(nt) dt | `fm_besselk` |
//! | `ellipk(m)`, `ellipe(m)` | [`ellipk`], [`ellipe`]: v1's 12-step AGM, same operations | `ellip_ops` |
//!
//! All are plain `f64 → f64` (orders as `f64`, NaN unless a whole number below 2³¹, as v1).
//! Agreement with v1 is measured in NUMERICS.md (glibc for the libm ones: ~1 ulp).

/// v1's `order_ok`: a whole number with |n| < 2³¹
pub fn order_ok(n: f64) -> bool {
    n.is_finite() && n == n.floor() && n.abs() < 2f64.powi(31)
}

pub fn erf(x: f64) -> f64 {
    libm::erf(x)
}

pub fn erfc(x: f64) -> f64 {
    libm::erfc(x)
}

/// Γ(x)
pub fn gamma(x: f64) -> f64 {
    libm::tgamma(x)
}

/// ln |Γ(x)|
pub fn lgamma(x: f64) -> f64 {
    libm::lgamma(x)
}

/// n! = Γ(n + 1) (v1 compiles `factorial(n)` to tgamma(n + 1))
pub fn factorial(n: f64) -> f64 {
    libm::tgamma(n + 1.0)
}

/// J_n(x) for a whole-number order n
pub fn besselj(n: f64, x: f64) -> f64 {
    if !order_ok(n) {
        return f64::NAN;
    }
    libm::jn(n as i32, x)
}

/// Y_n(x) for a whole-number order n
pub fn bessely(n: f64, x: f64) -> f64 {
    if !order_ok(n) {
        return f64::NAN;
    }
    libm::yn(n as i32, x)
}

const I_MAX_TERMS: f64 = 100000.0;
const K_MAX_STEPS: f64 = 100000.0;

/// I_n(x): v1's series Σ (x/2)^(2k+n) / (k! (k+n)!) (all terms positive), as `fm_besseli`.
pub fn besseli(n: f64, x: f64) -> f64 {
    if !order_ok(n) || x != x {
        return f64::NAN;
    }
    let nn = n.abs();
    let h = 0.5 * x;
    let mut t = 1.0;
    let mut k = 1.0;
    while k <= nn {
        t = t * h / k;
        k += 1.0;
    }
    let mut s = t;
    let q = h * h;
    let mut k = 1.0;
    while k < I_MAX_TERMS {
        t = t * q / (k * (k + nn));
        s += t;
        k += 1.0;
        if t.abs() <= 1e-17 * s.abs() {
            break;
        }
    }
    s
}

/// K_n(x): v1's trapezoidal rule on ∫₀^∞ exp(−x cosh t) cosh(n t) dt, as `fm_besselk`.
pub fn besselk(n: f64, x: f64) -> f64 {
    if !(order_ok(n) && x >= 0.0) {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::INFINITY;
    }
    let nn = n.abs();
    let h = 0.1f64.min(0.5 / (x * x + nn * nn).sqrt().sqrt());
    let peak = (nn / x).asinh();
    let mut s = 0.5 * (-x).exp();
    let mut k = 1.0;
    while k < K_MAX_STEPS {
        let t = k * h;
        let ch = if t < 700.0 { t.cosh() } else { f64::INFINITY };
        let xc = x * ch;
        let nt = nn * t;
        let term = 0.5 * ((nt - xc).exp() + (-nt - xc).exp());
        s += term;
        k += 1.0;
        if (t > peak && term <= 1e-18 * s) || s == f64::INFINITY {
            break;
        }
    }
    s * h
}

/// v1's `ellip_ops`: (K(m), E(m)) by 12 AGM steps, parameter m = k².
pub fn ellip(m: f64) -> (f64, f64) {
    let sqrt = |a: f64| if a >= 0.0 { a.sqrt() } else { f64::NAN };
    let mut a = 1.0f64;
    let mut b = sqrt(1.0 - m);
    let mut s = 0.5 * m;
    let mut w = 0.5f64;
    for _ in 0..12 {
        let c = 0.5 * (a - b);
        let (na, nb) = (0.5 * (a + b), sqrt(a * b));
        a = na;
        b = nb;
        w *= 2.0;
        s += w * (c * c);
    }
    let k = if a == 0.0 { f64::INFINITY } else { std::f64::consts::FRAC_PI_2 / a };
    let e = k * (1.0 - s);
    if m == 1.0 { (f64::INFINITY, 1.0) } else { (k, e) }
}

/// K(m), the complete elliptic integral of the first kind (parameter m = k²)
pub fn ellipk(m: f64) -> f64 {
    ellip(m).0
}

/// E(m), the complete elliptic integral of the second kind (parameter m = k²)
pub fn ellipe(m: f64) -> f64 {
    ellip(m).1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert!((erf(1.0) - 0.8427007929497149).abs() < 1e-16);
        assert!((gamma(5.0) - 24.0).abs() < 1e-13);
        assert!((besselj(0.0, 1.0) - 0.7651976865579666).abs() < 1e-16);
        assert!((besseli(0.0, 1.0) - 1.2660658777520082).abs() < 1e-15);
        assert!((besselk(0.0, 1.0) - 0.42102443824070834).abs() < 1e-13);
        assert!((ellipk(0.5) - 1.8540746773013719).abs() < 1e-15);
        assert!(besselj(0.5, 1.0).is_nan());
        assert_eq!(ellip(1.0), (f64::INFINITY, 1.0));
    }
}
