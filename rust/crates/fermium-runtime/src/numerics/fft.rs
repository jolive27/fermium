//! Fourier transforms (D81, D243): NumPy's conventions (`np.fft.fft`: X_k = Σ x_j e^{−2πi jk/n},
//! unnormalised; `ifft` with 1/n) for any length: mixed-radix Cooley–Tukey (radix 4, 2, 3, 5 and
//! generic odd primes up to 97) and Bluestein's chirp-z for lengths with a larger prime factor.
//! Twiddle factors are computed with octant reduction, so the results agree with NumPy's pocketfft
//! to rounding (~1e-16 of the largest |X_k|). [`spectrum`] ports v1's `fermium/runtime/spectral.py`.

use super::dense::C64;

/// e^{−2πi k/n}, accurate to rounding for every k (octant reduction with exact integer arithmetic)
fn twiddle(k: usize, n: usize) -> C64 {
    let k = (k % n) as u128;
    let n = n as u128;
    let q = (4 * k) / n; // quadrant
    let r = 4 * k - q * n; // 0 <= r < n: angle within the quadrant = (π/2)·r/n
    let (c, s) = if 2 * r <= n {
        let (c, s) = dd_cos_sin_half_pi(r as f64, n as f64);
        (c, s)
    } else {
        let (c, s) = dd_cos_sin_half_pi((n - r) as f64, n as f64);
        (s, c)
    };
    // angle θ = qπ/2 + φ with (cos φ, sin φ) = (c, s); return (cos θ, −sin θ)
    let (ct, st) = match q {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    };
    C64::new(ct, -st)
}

// ---- double-double arithmetic, so the twiddle factors are correctly rounded like pocketfft's constants
type DD = (f64, f64);

fn fast_two_sum(a: f64, b: f64) -> DD {
    let s = a + b;
    (s, b - (s - a))
}

fn dd_add(a: DD, b: DD) -> DD {
    let s = a.0 + b.0;
    let bb = s - a.0;
    let e = (a.0 - (s - bb)) + (b.0 - bb);
    fast_two_sum(s, e + a.1 + b.1)
}

fn dd_mul(a: DD, b: DD) -> DD {
    let p = a.0 * b.0;
    let e = a.0.mul_add(b.0, -p) + (a.0 * b.1 + a.1 * b.0);
    fast_two_sum(p, e)
}

fn dd_div_f(a: DD, k: f64) -> DD {
    let q1 = a.0 / k;
    let r = (-q1).mul_add(k, a.0);
    fast_two_sum(q1, (r + a.1) / k)
}

/// (cos, sin) of (π/2)·r/n for 0 ≤ r/n ≤ 1/2, correctly rounded (Taylor series in double-double).
fn dd_cos_sin_half_pi(r: f64, n: f64) -> (f64, f64) {
    const HALF_PI: DD = (1.5707963267948966, 6.123233995736766e-17);
    let q1 = r / n;
    let q2 = (-q1).mul_add(n, r) / n;
    let x = dd_mul(HALF_PI, fast_two_sum(q1, q2));
    let (mut c, mut s): (DD, DD) = ((1.0, 0.0), (0.0, 0.0));
    let mut term: DD = (1.0, 0.0);
    for k in 1..40 {
        term = dd_div_f(dd_mul(term, x), k as f64);
        let t = if (k / 2) % 2 == 1 { (-term.0, -term.1) } else { term };
        if k % 2 == 1 { s = dd_add(s, t) } else { c = dd_add(c, t) }
        if term.0.abs() < 1e-40 {
            break;
        }
    }
    (c.0 + c.1, s.0 + s.1)
}

fn factorize(mut n: usize) -> Vec<usize> {
    let mut f = Vec::new();
    while n % 4 == 0 {
        f.push(4);
        n /= 4;
    }
    while n % 2 == 0 {
        f.push(2);
        n /= 2;
    }
    let mut p = 3;
    while p * p <= n {
        while n % p == 0 {
            f.push(p);
            n /= p;
        }
        p += 2;
    }
    if n > 1 {
        f.push(n);
    }
    f
}

struct Plan {
    n: usize,
    factors: Vec<usize>,
    tw: Vec<C64>,
}

impl Plan {
    fn new(n: usize) -> Self {
        Plan { n, factors: factorize(n), tw: (0..n).map(|k| twiddle(k, n)).collect() }
    }

    fn w(&self, j: usize, len: usize) -> C64 {
        // e^{−2πi j/len} for len dividing n
        self.tw[(j % len) * (self.n / len)]
    }

    fn rec(&self, src: &[C64], off: usize, stride: usize, n: usize, dst: &mut [C64], fi: usize) {
        if n == 1 {
            dst[0] = src[off];
            return;
        }
        let p = self.factors[fi];
        let m = n / p;
        for q in 0..p {
            self.rec(src, off + q * stride, stride * p, m, &mut dst[q * m..(q + 1) * m], fi + 1);
        }
        let mut t = vec![C64::default(); p];
        let mut o = vec![C64::default(); p];
        for k in 0..m {
            for q in 0..p {
                let v = dst[q * m + k];
                t[q] = if q == 0 || k == 0 { v } else { v * self.w(q * k, n) };
            }
            match p {
                2 => {
                    o[0] = t[0] + t[1];
                    o[1] = t[0] - t[1];
                }
                4 => {
                    let (a0, a1) = (t[0] + t[2], t[0] - t[2]);
                    let (b0, b1) = (t[1] + t[3], t[1] - t[3]);
                    let b1j = C64::new(b1.im, -b1.re); // −i·b1
                    o[0] = a0 + b0;
                    o[1] = a1 + b1j;
                    o[2] = a0 - b0;
                    o[3] = a1 - b1j;
                }
                3 => {
                    let s = t[1] + t[2];
                    let d = t[1] - t[2];
                    let c1 = -0.5;
                    let s1 = -(3f64.sqrt()) * 0.5; // sin(−2π/3)
                    let m1 = t[0] + s.scale(c1);
                    let jd = C64::new(-d.im * s1, d.re * s1); // i·s1·d
                    o[0] = t[0] + s;
                    o[1] = m1 + jd;
                    o[2] = m1 - jd;
                }
                _ => {
                    // odd p, pocketfft's structure: sums and differences of the pairs (q, p − q), so a real input
                    // gives an exactly real X_0 and exactly conjugate X_r, X_{p−r} (numpy prints 0, not 1e-17)
                    let h = (p - 1) / 2;
                    let s: Vec<C64> = (1..=h).map(|q| t[q] + t[p - q]).collect();
                    let d: Vec<C64> = (1..=h).map(|q| t[q] - t[p - q]).collect();
                    let mut o0 = t[0];
                    for v in &s {
                        o0 = o0 + *v;
                    }
                    o[0] = o0;
                    for r in 1..=h {
                        let mut ca = t[0];
                        let mut cb = C64::default();
                        for q in 1..=h {
                            let w = self.w(q * r, p);
                            ca = ca + s[q - 1].scale(w.re);
                            cb = cb + d[q - 1].scale(w.im);
                        }
                        let icb = C64::new(-cb.im, cb.re); // i·cb
                        o[r] = ca + icb;
                        o[p - r] = ca - icb;
                    }
                }
            }
            for r in 0..p {
                dst[k + r * m] = o[r];
            }
        }
    }

    fn forward(&self, x: &[C64]) -> Vec<C64> {
        let mut out = vec![C64::default(); self.n];
        self.rec(x, 0, 1, self.n, &mut out, 0);
        out
    }
}

/// The largest prime factor handled directly (generic O(p²) butterfly); larger → Bluestein.
const MAX_DIRECT_PRIME: usize = 97;

/// The unnormalised forward DFT of complex data (X_k = Σ x_j e^{−2πi jk/n}).
pub fn fft(x: &[C64]) -> Vec<C64> {
    let n = x.len();
    if n <= 1 {
        return x.to_vec();
    }
    let big = factorize(n).into_iter().max().unwrap_or(1);
    if big <= MAX_DIRECT_PRIME {
        return Plan::new(n).forward(x);
    }
    bluestein(x)
}

/// The unnormalised backward DFT (Σ X_k e^{+2πi jk/n}).
pub fn fft_backward(x: &[C64]) -> Vec<C64> {
    let c: Vec<C64> = x.iter().map(|v| v.conj()).collect();
    fft(&c).into_iter().map(|v| v.conj()).collect()
}

/// NumPy's `ifft`: the backward transform times 1/n.
pub fn ifft(x: &[C64]) -> Vec<C64> {
    let n = x.len();
    let f = 1.0 / n as f64;
    fft_backward(x).into_iter().map(|v| v.scale(f)).collect()
}

fn bluestein(x: &[C64]) -> Vec<C64> {
    let n = x.len();
    let mut m = 1;
    while m < 2 * n - 1 {
        m <<= 1;
    }
    // chirp w_k = e^{−iπ k²/n}, k² reduced mod 2n
    let w: Vec<C64> = (0..n).map(|k| twiddle(((k as u128 * k as u128) % (2 * n as u128)) as usize, 2 * n)).collect();
    let mut a = vec![C64::default(); m];
    for k in 0..n {
        a[k] = x[k] * w[k];
    }
    let mut b = vec![C64::default(); m];
    b[0] = w[0].conj();
    for k in 1..n {
        b[k] = w[k].conj();
        b[m - k] = w[k].conj();
    }
    let plan = Plan::new(m);
    let fa = plan.forward(&a);
    let fb = plan.forward(&b);
    let prod: Vec<C64> = fa.iter().zip(&fb).map(|(p, q)| *p * *q).collect();
    let conv: Vec<C64> = plan.forward(&prod.iter().map(|v| v.conj()).collect::<Vec<_>>()).into_iter().map(|v| v.conj()).collect();
    let f = 1.0 / m as f64;
    (0..n).map(|k| conv[k].scale(f) * w[k]).collect()
}

/// v1's `spectrum(kind, a, b, dt)` (fermium/runtime/spectral.py): kind 0 fft_re, 1 fft_im,
/// 2 amplitude_spectrum, 3 power_spectrum, 4 ifft (real part, from re a and im b), and the
/// complex versions 5 fft(real), 6 ifft(complex), 7 fft(complex), 8 ifft(real) with (re, im)
/// interleaved.
pub fn spectrum(kind: i32, a: &[f64], b: Option<&[f64]>, dt: f64) -> Vec<f64> {
    let interleave = |z: Vec<C64>| -> Vec<f64> { z.iter().flat_map(|v| [v.re, v.im]).collect() };
    if kind >= 5 {
        let z: Vec<C64> = if kind == 6 || kind == 7 {
            a.chunks(2).map(|p| C64::new(p[0], p[1])).collect()
        } else {
            a.iter().map(|&v| C64::new(v, 0.0)).collect()
        };
        return interleave(if kind == 5 || kind == 7 { fft(&z) } else { ifft(&z) });
    }
    if kind == 4 {
        let bb = b.unwrap_or(&[]);
        let z: Vec<C64> = a.iter().enumerate().map(|(i, &v)| C64::new(v, bb.get(i).copied().unwrap_or(0.0))).collect();
        return ifft(&z).iter().map(|v| v.re).collect();
    }
    let n = a.len();
    let x: Vec<C64> = a.iter().map(|&v| C64::new(v, 0.0)).collect();
    let big = fft(&x);
    match kind {
        0 => big.iter().map(|v| v.re).collect(),
        1 => big.iter().map(|v| v.im).collect(),
        _ => {
            let half = &big[..n / 2 + 1];
            let len = half.len();
            half.iter()
                .enumerate()
                .map(|(k, v)| {
                    let wgt = if k == 0 || (n % 2 == 0 && k == len - 1) { 1.0 } else { 2.0 };
                    let mag = v.abs();
                    if kind == 2 { wgt * mag / n as f64 } else { wgt * mag.powf(2.0) * dt / n as f64 }
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(x: &[C64]) -> Vec<C64> {
        let n = x.len();
        (0..n).map(|k| (0..n).fold(C64::default(), |acc, j| acc + x[j] * twiddle(j * k, n))).collect()
    }

    #[test]
    fn against_naive_dft() {
        for n in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 12, 15, 16, 30, 49, 97, 101, 128, 210, 211, 2003] {
            let x: Vec<C64> = (0..n).map(|j| C64::new((0.37 * j as f64 + 0.1).sin(), (1.1 * j as f64).cos() * 0.5)).collect();
            let a = fft(&x);
            let b = naive(&x);
            let scale = b.iter().fold(1.0f64, |m, v| m.max(v.abs()));
            for (p, q) in a.iter().zip(&b) {
                assert!((*p - *q).abs() < 1e-14 * scale, "n = {n}: {}", (*p - *q).abs() / scale);
            }
            let back = ifft(&a);
            for (p, q) in back.iter().zip(&x) {
                assert!((*p - *q).abs() < 1e-14, "n = {n} inverse");
            }
        }
    }
}
