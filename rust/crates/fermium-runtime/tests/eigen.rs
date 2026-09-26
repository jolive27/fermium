//! Eigenvalue problems against v1 (fermium/runtime/eigen.py over SciPy), fixtures/eigen.txt.
mod common;
use common::*;
use fermium_runtime::numerics::eigen::{eigen_solve, EigenMethod};

fn fwell(x: f64) -> f64 {
    if x.abs() < 1.0 { 0.0 } else { 50.0 }
}

/// EIGEN_CASES: (ψ''(x, ψ, E), a, b, N, grid)
fn case(name: &str) -> (Box<dyn Fn(f64, f64, f64) -> f64>, f64, f64, usize, usize) {
    match name {
        "harmonic" => (Box::new(|x, p, e| (x * x - 2.0 * e) * p), -10.0, 10.0, 4, 2000),
        "box" => (Box::new(|_x, p, e| -2.0 * e * p), 0.0, 1.0, 3, 1000),
        "finite_well" => (Box::new(|x, p, e| 2.0 * (fwell(x) - e) * p), -4.0, 4.0, 3, 2000),
        "coulomb" => (Box::new(|x, p, e| (-2.0 / x - 2.0 * e) * p), 0.0, 60.0, 3, 4000),
        "double_well" => (Box::new(|x, p, e| 2.0 * (10.0 * (x * x - 4.0) * (x * x - 4.0) - e) * p), -5.0, 5.0, 2, 2000),
        "tilted_double" => (Box::new(|x, p, e| 2.0 * (10.0 * (x * x - 4.0) * (x * x - 4.0) + 1e-9 * x - e) * p), -5.0, 5.0, 2, 2000),
        "morse" => (
            Box::new(|x, p, e| {
                let u = 1.0 - (-(x - 1.0)).exp();
                2.0 * (8.0 * u * u - e) * p
            }),
            0.0,
            12.0,
            3,
            3000,
        ),
        _ => panic!("no eigen case {name}"),
    }
}

const SAMPLES: [f64; 5] = [0.13, 0.31, 0.5, 0.62, 0.87];

#[test]
fn eigen_matches_v1() {
    for row in load("eigen.txt") {
        let (name, method) = row.name.split_once('/').unwrap();
        let m = if method == "matrix" { EigenMethod::Matrix } else { EigenMethod::Shooting };
        let (f, a, b, n, grid) = case(name);
        let mut rhs = |x: f64, p: f64, _dp: f64, e: f64| f(x, p, e);
        let r = eigen_solve(&mut rhs, a, b, n, grid, m).unwrap_or_else(|e| panic!("{}: {}", row.name, e.message));
        let want = row.floats();
        assert_eq!(want[0] as usize, n);
        let mut worst_e = 0.0f64;
        let escale = r.energies.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        for k in 0..n {
            let d = (r.energies[k] - want[1 + k]).abs() / escale;
            worst_e = worst_e.max(d);
        }
        // warnings (D233)
        let nw = want[1 + n + 10 * n] as usize;
        assert_eq!(r.warnings.len(), nw, "{}: warnings", row.name);
        let mut worst_p = 0.0f64;
        if nw == 0 {
            for k in 0..n {
                let pscale = r.psi[k].iter().fold(0.0f64, |m, v| m.max(v.abs()));
                let dscale = r.dpsi[k].iter().fold(0.0f64, |m, v| m.max(v.abs()));
                for (s, fr) in SAMPLES.iter().enumerate() {
                    let i = (fr * (r.xs.len() - 1) as f64).round_ties_even() as usize;
                    let (wp, wd) = (want[1 + n + 10 * k + 2 * s], want[1 + n + 10 * k + 2 * s + 1]);
                    worst_p = worst_p.max((r.psi[k][i] - wp).abs() / pscale).max((r.dpsi[k][i] - wd).abs() / dscale);
                }
            }
        }
        eprintln!("{:24} energies: worst diff / max|E| {worst_e:.1e}   psi, psi': worst diff / max {worst_p:.1e}", row.name);
        assert!(worst_e < 1e-9, "{}: energies differ by {worst_e:e}", row.name);
        assert!(worst_p < 1e-6, "{}: states differ by {worst_p:e}", row.name);
    }
}
