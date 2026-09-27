//! Native Levenberg–Marquardt fit against v1 (SciPy least_squares 'lm'), fixtures/fit.txt.
mod common;
use common::*;
use fermium_runtime::numerics::fit::least_squares_fit;

fn model(name: &str, p: &[f64], x: f64) -> f64 {
    match name {
        "line" | "exact" | "n_eq_k" => p[0] * x + p[1],
        "decay_scan" | "decay_guess" | "si_scale" => p[0] * (-x / p[1]).exp(),
        "gauss" => p[0] * (-(x - p[1]) * (x - p[1]) / (2.0 * p[2] * p[2])).exp() + p[3],
        "power" => p[0] * x.powf(p[1]),
        "degenerate" => p[0] * p[1] * x,
        "damped" => p[0] * (-x / p[1]).exp() * (p[2] * x).cos(),
        _ => panic!("no model {name}"),
    }
}

#[test]
fn fits_match_v1() {
    let rows = by_name("fit.txt");
    let names: Vec<String> = rows.keys().filter(|k| !k.contains(':')).cloned().collect();
    assert!(names.len() >= 10);
    for name in names {
        let xs = rows[&format!("{name}:x")].floats();
        let ys = rows[&format!("{name}:y")].floats();
        let guess: Vec<Option<f64>> = rows[&format!("{name}:guess")].floats().iter().map(|&g| (!g.is_nan()).then_some(g)).collect();
        let want = rows[&name].floats();
        let k = want[0] as usize;
        let n = xs.len();
        let mut resid = |p: &[f64], o: &mut [f64]| {
            for i in 0..n {
                o[i] = model(&name, p, xs[i]) - ys[i];
            }
        };
        let fit = least_squares_fit(&mut resid, n, &guess).unwrap();
        let mut worst_p = 0.0f64;
        let mut worst_e = 0.0f64;
        for i in 0..k {
            let (wp, we) = (want[1 + i], want[1 + k + i]);
            // parameters: to 1e-8 relative (the optimum is only defined to about that by the
            // tolerances; v1 itself differs by 1e-10 between two starts)
            close(&format!("{name} p{i}"), fit.params[i], wp, 1e-8, 1e-14);
            worst_p = worst_p.max(rel(fit.params[i], wp));
            match fit.errors[i] {
                None => assert!(we.is_nan(), "{name}: error {i} not estimated, v1 has {we}"),
                Some(e) => {
                    assert!(!we.is_nan(), "{name}: error {i} = {e}, v1 could not estimate it");
                    close(&format!("{name} err{i}"), e, we, 1e-6, 1e-14);
                    worst_e = worst_e.max(rel(e, we));
                }
            }
        }
        close(&format!("{name} rms"), fit.rms, want[1 + 2 * k], 1e-9, 1e-15);
        eprintln!("{name:12} params rel {worst_p:.1e}   std errors rel {worst_e:.1e}");
    }
}
