//! Fourier transforms (D81, D243): NumPy's conventions (`np.fft.fft`: X_k = Σ x_j e^{−2πi jk/n},
//! unnormalised; `ifft` with 1/n) for any length, computed by [`super::pocketfft`], a port of the
//! pocketfft code numpy.fft runs, so the results match v1 (which calls numpy) to the last bit.
//! [`spectrum`] ports v1's `fermium/runtime/spectral.py`.

use super::dense::C64;
use super::pocketfft::{c2c, Cx};

fn run(x: &[C64], fct: f64, fwd: bool) -> Vec<C64> {
    let mut c: Vec<Cx> = x.iter().map(|v| Cx::new(v.re, v.im)).collect();
    c2c(&mut c, fct, fwd);
    c.into_iter().map(|v| C64::new(v.r, v.i)).collect()
}

/// The unnormalised forward DFT of complex data (X_k = Σ x_j e^{−2πi jk/n}).
pub fn fft(x: &[C64]) -> Vec<C64> {
    run(x, 1.0, true)
}

/// The unnormalised backward DFT (Σ X_k e^{+2πi jk/n}).
pub fn fft_backward(x: &[C64]) -> Vec<C64> {
    run(x, 1.0, false)
}

/// NumPy's `ifft`: the backward transform times 1/n.
pub fn ifft(x: &[C64]) -> Vec<C64> {
    if x.is_empty() {
        return vec![];
    }
    run(x, 1.0 / x.len() as f64, false)
}

/// `np.abs` of a complex128 as numpy's SIMD loop computes it (loops_unary_complex): larger·√(fma(r, r, 1)) with
/// r = smaller/larger, not libm's hypot (numpy on a CPU with FMA, as the oracle's machine).
pub fn numpy_cabs(re: f64, im: f64) -> f64 {
    let (a, b) = (re.abs(), im.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let (larger, smaller) = if a >= b { (a, b) } else { (b, a) };
    if larger == 0.0 {
        return 0.0;
    }
    let r = smaller / larger;
    r.mul_add(r, 1.0).sqrt() * larger
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
                    let mag = numpy_cabs(v.re, v.im);
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
        (0..n)
            .map(|k| {
                x.iter().enumerate().fold(C64::default(), |acc, (j, v)| {
                    let a = -2.0 * std::f64::consts::PI * ((j * k) % n) as f64 / n as f64;
                    acc + *v * C64::new(a.cos(), a.sin())
                })
            })
            .collect()
    }

    #[test]
    fn against_naive_dft() {
        for n in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 15, 16, 30, 49, 97, 101, 128, 210, 211, 2003] {
            let x: Vec<C64> = (0..n).map(|j| C64::new((0.37 * j as f64 + 0.1).sin(), (1.1 * j as f64).cos() * 0.5)).collect();
            let a = fft(&x);
            let b = naive(&x);
            let scale = b.iter().fold(1.0f64, |m, v| m.max(v.abs()));
            for (p, q) in a.iter().zip(&b) {
                assert!((*p - *q).abs() < 1e-11 * scale, "n = {n}: {}", (*p - *q).abs() / scale);
            }
            let back = ifft(&a);
            for (p, q) in back.iter().zip(&x) {
                assert!((*p - *q).abs() < 1e-13, "n = {n} inverse");
            }
        }
    }

    #[test]
    fn exactly_numpy() {
        // numpy 2.4: np.fft.fft([0.3, -1.2, 2.5, 0.7, -0.4, 1.9, -2.2, 0.05]) and ifft of it (real parts)
        let x: Vec<C64> = [0.3, -1.2, 2.5, 0.7, -0.4, 1.9, -2.2, 0.05].iter().map(|&v| C64::new(v, 0.0)).collect();
        let back = ifft(&fft(&x));
        assert_eq!(back[7].re, 0.050000000000000155);
    }
}
