//! Uncertain numbers for `5.0 ± 0.2 m` (D120–D124): a port of `fermium/uncertain.py`.
//!
//! An uncertain value is a nominal value plus a sparse vector of contributions {source id: ∂f/∂xᵢ · σᵢ}, one
//! entry per independent error source it depends on; σ² = Σ contribution² (first-order propagation with exact
//! correlations, so x − x = 0 ± 0). The contributions keep Python's dict order (insertion order), which matters
//! where v1 sums them in that order (Monte Carlo samples).
use std::cell::Cell;

thread_local! {
    static NEXT_SOURCE: Cell<u64> = const { Cell::new(1) };
}

/// A new independent error source (uncertain.new_source).
pub fn new_source() -> u64 {
    NEXT_SOURCE.with(|c| {
        let id = c.get();
        c.set(id + 1);
        id
    })
}

/// A cancellation of two contributions down to this fraction of their sizes is rounding noise (D208).
pub const NOISE: f64 = 1e-13;

#[derive(Clone, Debug, PartialEq)]
pub struct UFloat {
    pub v: f64,
    /// (source, contribution), in insertion order
    pub d: Vec<(u64, f64)>,
}

fn get(d: &[(u64, f64)], k: u64) -> Option<usize> {
    d.iter().position(|(x, _)| *x == k)
}

/// Python's dict update d[k] = f(d.get(k, 0.0)).
fn upsert(d: &mut Vec<(u64, f64)>, k: u64, f: impl FnOnce(f64) -> f64) {
    match get(d, k) {
        Some(i) => d[i].1 = f(d[i].1),
        None => d.push((k, f(0.0))),
    }
}

fn keep(c: f64) -> bool {
    c != 0.0 || c.is_nan()
}

/// Python's a / b made IEEE (uncertain._div).
pub fn div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        if a == 0.0 || a.is_nan() {
            return f64::NAN;
        }
        return f64::INFINITY.copysign(a) * 1f64.copysign(b);
    }
    a / b
}

/// Python's a ** b with overflow as ∞ and a complex result as NaN (uncertain._pow).
pub fn pow(a: f64, b: f64) -> f64 {
    if a == 0.0 && b < 0.0 {
        return f64::INFINITY; // ZeroDivisionError
    }
    if a < 0.0 && b.fract() != 0.0 && b.is_finite() {
        return f64::NAN; // a complex result
    }
    let r = a.powf(b);
    if r.is_infinite() && a.is_finite() && b.is_finite() {
        return f64::INFINITY; // OverflowError
    }
    r
}

/// x + y for two contributions of one source; a cancellation down to rounding noise is exactly 0 (D208).
pub fn sum2(x: f64, y: f64) -> f64 {
    let z = x + y;
    if z != 0.0 && z.abs() <= NOISE * (x.abs() + y.abs()) {
        return 0.0;
    }
    z
}

pub fn scale(d: &[(u64, f64)], a: f64) -> Vec<(u64, f64)> {
    if a == 1.0 {
        return d.to_vec();
    }
    d.iter().filter(|(_, c)| c * a != 0.0 || a.is_nan()).map(|&(k, c)| (k, c * a)).collect()
}

pub fn scale_merge(d1: &[(u64, f64)], a: f64, d2: &[(u64, f64)], b: f64) -> Vec<(u64, f64)> {
    let mut out: Vec<(u64, f64)> = d1.iter().map(|&(k, c)| (k, c * a)).collect();
    for &(k, c) in d2 {
        upsert(&mut out, k, |x| sum2(x, c * b));
    }
    out.retain(|(_, c)| keep(*c));
    out
}

/// Python's math.fsum: the exactly rounded sum (Shewchuk's algorithm, as CPython does it).
pub fn fsum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let mut partials: Vec<f64> = vec![];
    let mut special = 0.0f64;
    let mut inf_sum = 0.0f64;
    for mut x in xs {
        if !x.is_finite() {
            if x.is_infinite() {
                inf_sum += x;
            }
            special += x;
            continue;
        }
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        partials.push(x);
    }
    if special != 0.0 {
        if inf_sum.is_nan() {
            return f64::NAN;
        }
        return special;
    }
    let mut n = partials.len();
    if n == 0 {
        return 0.0;
    }
    n -= 1;
    let mut hi = partials[n];
    let mut lo = 0.0;
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        let yr = hi - x;
        lo = y - yr;
        if lo != 0.0 {
            break;
        }
    }
    if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0)) {
        let y = lo * 2.0;
        let x = hi + y;
        let yr = x - hi;
        if y == yr {
            hi = x;
        }
    }
    hi
}

impl UFloat {
    pub fn new(v: f64, d: Vec<(u64, f64)>) -> UFloat {
        UFloat { v, d }
    }

    /// A new independent measurement v ± sigma.
    pub fn measured(v: f64, sigma: f64) -> UFloat {
        let s = sigma.abs();
        UFloat { v, d: if s != 0.0 { vec![(new_source(), s)] } else { vec![] } }
    }

    /// The standard deviation.
    pub fn s(&self) -> f64 {
        fsum(self.d.iter().map(|(_, c)| c * c)).sqrt()
    }

    /// a·self + b·other as contributions (_lin).
    fn lin(&self, a: f64, o: &UFloat, b: f64) -> Vec<(u64, f64)> {
        let mut d: Vec<(u64, f64)> = if a != 1.0 { self.d.iter().map(|&(k, c)| (k, a * c)).collect() } else { self.d.clone() };
        for &(k, c) in &o.d {
            upsert(&mut d, k, |x| sum2(x, b * c));
        }
        d.retain(|(_, c)| keep(*c));
        d
    }

    pub fn add(&self, o: &UFloat) -> UFloat {
        UFloat::new(self.v + o.v, self.lin(1.0, o, 1.0))
    }
    pub fn sub(&self, o: &UFloat) -> UFloat {
        UFloat::new(self.v - o.v, self.lin(1.0, o, -1.0))
    }
    pub fn add_f(&self, o: f64) -> UFloat {
        UFloat::new(self.v + o, self.d.clone())
    }
    /// self − o
    pub fn sub_f(&self, o: f64) -> UFloat {
        UFloat::new(self.v - o, self.d.clone())
    }
    /// o − self
    pub fn rsub_f(&self, o: f64) -> UFloat {
        UFloat::new(o - self.v, self.d.iter().map(|&(k, c)| (k, -c)).collect())
    }
    pub fn neg(&self) -> UFloat {
        UFloat::new(-self.v, self.d.iter().map(|&(k, c)| (k, -c)).collect())
    }
    pub fn mul(&self, o: &UFloat) -> UFloat {
        UFloat::new(self.v * o.v, scale_merge(&self.d, o.v, &o.d, self.v))
    }
    pub fn mul_f(&self, o: f64) -> UFloat {
        UFloat::new(self.v * o, scale(&self.d, o))
    }
    pub fn div(&self, o: &UFloat) -> UFloat {
        let q = div(self.v, o.v);
        UFloat::new(q, scale_merge(&self.d, div(1.0, o.v), &o.d, -div(q, o.v)))
    }
    /// self / o
    pub fn div_f(&self, o: f64) -> UFloat {
        UFloat::new(div(self.v, o), scale(&self.d, div(1.0, o)))
    }
    /// o / self
    pub fn rdiv_f(&self, o: f64) -> UFloat {
        let q = div(o, self.v);
        UFloat::new(q, scale(&self.d, -div(q, self.v)))
    }
    pub fn pow(&self, o: &UFloat) -> UFloat {
        let r = pow(self.v, o.v);
        let da = if self.v != 0.0 { o.v * pow(self.v, o.v - 1.0) } else if o.v > 1.0 { 0.0 } else { f64::NAN };
        let db = if self.v > 0.0 { r * self.v.ln() } else if self.v == 0.0 { 0.0 } else { f64::NAN };
        UFloat::new(r, scale_merge(&self.d, da, &o.d, db))
    }
    /// o ** self for a plain o
    pub fn rpow_f(&self, o: f64) -> UFloat {
        let r = pow(o, self.v);
        let db = if o > 0.0 { r * o.ln() } else if o == 0.0 && self.v > 0.0 { 0.0 } else { f64::NAN };
        UFloat::new(r, scale(&self.d, db))
    }
    /// self ** p for a plain p; f: the plain power function to use for the value (the interpreter's)
    pub fn powc(&self, p: f64, f: impl Fn(f64, f64) -> f64) -> UFloat {
        let r = f(self.v, p);
        if p == 0.0 {
            return UFloat::new(r, vec![]);
        }
        let dv = if !(self.v == 0.0 && p >= 1.0) { p * f(self.v, p - 1.0) } else if p == 1.0 { 1.0 } else { 0.0 };
        UFloat::new(r, scale(&self.d, dv))
    }
    pub fn abs(&self) -> UFloat {
        UFloat::new(self.v.abs(), scale(&self.d, if self.v < 0.0 { -1.0 } else { 1.0 }))
    }
}

/// Uncertain values with the given covariance matrix (row-major n×n): cov = V Λ Vᵀ, each eigenvector one new
/// source (uncertain.correlated, for the parameters found by fit, D124). None for a covariance that isn't
/// finite. The eigenvectors' signs and order can differ from NumPy's eigh; σ and every correlation don't.
pub fn correlated(values: &[f64], cov: &[f64]) -> Option<Vec<UFloat>> {
    let n = values.len();
    if cov.len() != n * n || !cov.iter().all(|c| c.is_finite()) {
        return None;
    }
    let (lam, vec) = super::linalg::jacobi_eigen(cov, n);
    let ids: Vec<u64> = (0..n).map(|_| new_source()).collect();
    Some((0..n)
        .map(|i| {
            let d = (0..n)
                .filter_map(|k| {
                    let c = vec[i * n + k] * lam[k].max(0.0).sqrt();
                    if c != 0.0 { Some((ids[k], c)) } else { None }
                })
                .collect();
            UFloat::new(values[i], d)
        })
        .collect())
}

/// The derivative of a one-argument function at x (uncertain.DERIV); None for the step functions (a plain
/// result) and for names it doesn't know.
pub fn deriv(name: &str, x: f64, digamma: impl Fn(f64) -> f64, gamma: impl Fn(f64) -> f64) -> Option<f64> {
    let sqrt_pi = std::f64::consts::PI.sqrt();
    Some(match name {
        "sin" => x.cos(),
        "cos" => -x.sin(),
        "tan" => 1.0 + x.tan().powi(2),
        "asin" => if x.abs() <= 1.0 { div(1.0, (1.0 - x * x).sqrt()) } else { f64::NAN },
        "acos" => if x.abs() <= 1.0 { -div(1.0, (1.0 - x * x).sqrt()) } else { f64::NAN },
        "atan" => 1.0 / (1.0 + x * x),
        "sinh" => x.cosh(),
        "cosh" => x.sinh(),
        "tanh" => 1.0 - x.tanh().powi(2),
        "asinh" => 1.0 / (x * x + 1.0).sqrt(),
        "acosh" => if x >= 1.0 { div(1.0, (x * x - 1.0).sqrt()) } else { f64::NAN },
        "atanh" => div(1.0, 1.0 - x * x),
        "exp" | "expm1" => x.exp(),
        "ln" | "log" => div(1.0, x),
        "log10" => div(1.0, x * std::f64::consts::LN_10),
        "log2" => div(1.0, x * std::f64::consts::LN_2),
        "erf" => 2.0 / sqrt_pi * (-x * x).exp(),
        "erfc" => -2.0 / sqrt_pi * (-x * x).exp(),
        "gamma" => gamma(x) * digamma(x),
        "lgamma" => digamma(x),
        "log1p" => div(1.0, 1.0 + x),
        "abs" => if x < 0.0 { -1.0 } else { 1.0 },
        "cot" => -div(1.0, x.sin().powi(2)),
        "sec" => div(x.sin(), x.cos().powi(2)),
        "csc" => -div(x.cos(), x.sin().powi(2)),
        _ => return None,
    })
}

/// The digamma function ψ(x) (SciPy's digamma, which v1 uses for d gamma): the recurrence up to x ≥ 10, then the
/// asymptotic series; the reflection formula for x < 0.
pub fn digamma(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if x == 0.0 {
        return if x.is_sign_negative() { f64::INFINITY } else { f64::NEG_INFINITY };
    }
    if x < 0.0 {
        if x == x.floor() {
            return f64::NAN;
        }
        // ψ(1 − x) − ψ(x) = π cot(π x)
        return digamma(1.0 - x) - std::f64::consts::PI / (std::f64::consts::PI * x).tan();
    }
    let mut x = x;
    let mut r = 0.0;
    while x < 10.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    let t = f * (-1.0 / 12.0 + f * (1.0 / 120.0 + f * (-1.0 / 252.0 + f * (1.0 / 240.0 + f * (-1.0 / 132.0
        + f * (691.0 / 32760.0 + f * (-1.0 / 12.0)))))));
    r + x.ln() - 0.5 / x + t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_minus_x_is_exact() {
        let x = UFloat::measured(5.0, 0.2);
        assert_eq!(x.sub(&x).s(), 0.0);
        let y = x.mul_f(2.0);
        assert_eq!(y.sub(&x).sub(&x).s(), 0.0);
        assert!((x.mul(&x).s() - 2.0).abs() < 1e-15);
    }

    #[test]
    fn fsum_is_exact() {
        assert_eq!(fsum([1e100, 1.0, -1e100]), 1.0);
        assert_eq!(fsum([0.1; 10]), 1.0);
    }

    #[test]
    fn digamma_values() {
        // scipy.special.digamma(1) = -0.5772156649015329, digamma(0.5) = -1.9635100260214235
        assert!((digamma(1.0) + 0.5772156649015329).abs() < 1e-14);
        assert!((digamma(0.5) + 1.9635100260214235).abs() < 1e-14);
        assert!((digamma(10.3) - 2.2828154464391224).abs() < 1e-13);
        assert!((digamma(-2.5) - 1.1031566406452433).abs() < 1e-13);
    }
}
