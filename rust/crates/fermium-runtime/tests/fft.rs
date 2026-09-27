//! FFT and spectra against v1 (spectral.py over numpy.fft), fixtures/fft.txt: bit-identical.
mod common;
use common::*;
use fermium_runtime::numerics::fft::spectrum;

#[test]
fn spectra_match_numpy() {
    let mut worst = 0.0f64;
    let (mut exact, mut total) = (0usize, 0usize);
    let rows = load("fft.txt");
    assert!(rows.len() >= 180);
    for row in rows {
        let (n, kind) = row.name.split_once(':').unwrap();
        let (n, kind): (usize, i32) = (n.parse().unwrap(), kind.parse().unwrap());
        let x: Vec<f64> = (0..n).map(|j| (0.37 * j as f64 + 0.1).sin() + 0.5 * (0.013 * j as f64 * j as f64).cos()).collect();
        let y: Vec<f64> = (0..n).map(|j| (1.1 * j as f64).cos() * 0.5).collect();
        let z: Vec<f64> = x.iter().zip(&y).flat_map(|(a, b)| [*a, *b]).collect();
        let out = match kind {
            4 => spectrum(4, &x, Some(&y), 1.0),
            6 | 7 => spectrum(kind, &z, None, 1.0),
            _ => spectrum(kind, &x, None, 0.01),
        };
        let want = row.floats();
        let l2 = out.iter().map(|a| a * a).sum::<f64>().sqrt();
        let got: Vec<f64> = if out.len() > 256 {
            let mut v: Vec<f64> = (0..16).map(|i| out[((i * (out.len() - 1)) as f64 / 15.0).round() as usize]).collect();
            v.push(out.iter().map(|a| a * a).sum());
            v
        } else {
            out
        };
        assert_eq!(got.len(), want.len(), "{}", row.name);
        // relative to the output's L2 norm (the natural error scale of an FFT)
        let scale = want.iter().fold(l2, |m, v| m.max(v.abs())).max(1e-300);
        let sampled = want.len() == 17 && n > 128;
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            if sampled && i == 16 {
                close(&format!("{} sum of squares", row.name), *g, *w, 1e-12, 0.0);
                continue;
            }
            total += 1;
            if g == w {
                exact += 1;
            } else if std::env::var("FFT_DEBUG").is_ok() {
                eprintln!("{} [{i}]: {g:e} vs {w:e}", row.name);
            }
            let d = (g - w).abs() / scale;
            worst = worst.max(d);
            assert!(d < 1e-14, "{}: {g} vs {w}", row.name);
        }
    }
    eprintln!("fft: worst difference / L2 norm of the output = {worst:.1e}; {exact} of {total} values bit-identical");
    // pocketfft ported operation for operation: every value is numpy's, to the last bit, where the libm is the
    // fixtures' (glibc; the twiddles and this test's inputs use sin/cos). Elsewhere (Apple's libm) the last bits
    // differ and the 1e-14 check above is the test.
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        assert_eq!(exact, total);
    }
}
