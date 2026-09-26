//! PDE steppers against v1 (fermium/runtime/pde.py over SciPy splu), fixtures/pde.txt.
mod common;
use common::*;
use fermium_runtime::numerics::pde::{pde_solve, PdeMethod, PdeOpts};
use std::f64::consts::PI;

type Probe = Box<dyn FnMut(f64, &[f64; 6]) -> [f64; 6]>;

fn barrier(x: f64) -> f64 {
    if x.abs() < 0.5 { 1.0 } else { 0.0 }
}

/// PDE_CASES of numerics_fixtures.py
fn case(name: &str) -> (Probe, f64, f64, f64, f64, PdeOpts) {
    let d = PdeOpts::default();
    match name {
        "heat_cn" => (
            Box::new(|x, a| [0.1 * a[2], (PI * x).sin() + 0.3 * (3.0 * PI * x).sin(), 0.0, 0.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 0.5, PdeOpts { grid: 100, ..d },
        ),
        "heat_jump" => (Box::new(|_x, a| [a[2], 0.0, 0.0, 0.0, 1.0, 0.0]), 0.0, 1.0, 0.0, 0.2, PdeOpts { grid: 50, ..d }),
        "heat_neumann_step" => (
            Box::new(|x, a| [0.5 * a[2] - 0.2 * a[1], (PI * x).cos(), 0.0, 0.0, 0.0, 0.3]),
            0.0, 1.0, 0.0, 1.0, PdeOpts { grid: 80, step: Some(0.05), bc: (1, 1), ..d },
        ),
        "heat_implicit_src" => (
            Box::new(|x, a| [a[2] - a[0] + (3.0 * a[3]).sin() * x, x * (1.0 - x), 0.0, 0.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 2.0, PdeOpts { grid: 60, method: PdeMethod::Implicit, tdep: true, ..d },
        ),
        "heat_explicit" => (
            Box::new(|x, a| [a[2], (-40.0 * (x - 0.3) * (x - 0.3)).exp(), 0.0, 0.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 0.05, PdeOpts { grid: 40, method: PdeMethod::Explicit, ..d },
        ),
        "tdse_free" => (
            Box::new(|x, a| [a[5] * (0.5 * a[2]), (-(x + 5.0) * (x + 5.0) / 4.0).exp(), 2.0 * x, 0.0, 0.0, 0.0]),
            -20.0, 20.0, 0.0, 2.0, PdeOpts { grid: 400, is_complex: true, ..d },
        ),
        "tdse_barrier" => (
            Box::new(|x, a| [a[5] * (0.5 * a[2] - barrier(x) * a[0]), (-(x + 5.0) * (x + 5.0) / 4.0).exp(), 1.5 * x, 0.0, 0.0, 0.0]),
            -20.0, 20.0, 0.0, 3.0, PdeOpts { grid: 400, is_complex: true, ..d },
        ),
        "wave" => (
            Box::new(|x, a| [4.0 * a[2], (-50.0 * (x - 0.5) * (x - 0.5)).exp(), 0.0, 0.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 1.0, PdeOpts { grid: 200, order: 2, ..d },
        ),
        "wave_damped" => (
            Box::new(|x, a| [a[2] - 0.5 * a[4], (PI * x).sin(), 0.0, 1.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 2.0, PdeOpts { grid: 100, order: 2, step: Some(0.004), ..d },
        ),
        "explicit_unstable" => (
            Box::new(|x, a| [a[2], (PI * x).sin(), 0.0, 0.0, 0.0, 0.0]),
            0.0, 1.0, 0.0, 0.1, PdeOpts { grid: 100, method: PdeMethod::Explicit, step: Some(0.01), ..d },
        ),
        _ => panic!("no PDE case {name}"),
    }
}

#[test]
fn pde_matches_v1() {
    for row in load("pde.txt") {
        let (mut probe, xa, xb, t0, t1, opts) = case(&row.name);
        let got = pde_solve(&mut *probe, xa, xb, t0, t1, opts);
        if row.is_err() {
            let e = got.err().unwrap_or_else(|| panic!("{}: expected an error", row.name));
            assert_eq!(e.0.replace(' ', "_"), row.fields[1], "{}", row.name);
            continue;
        }
        let r = got.unwrap_or_else(|e| panic!("{}: {}", row.name, e.0));
        let want = row.floats();
        let n = r.ts.len();
        assert_eq!(n as f64, want[0], "{}: snapshots", row.name);
        assert_eq!((r.m as f64, r.ncomp as f64), (want[1], want[2]), "{}", row.name);
        assert_eq!(r.warnings.len() as f64, want[3], "{}: warnings", row.name);
        if let Some(&(k, est)) = r.warnings.first() {
            assert_eq!(k as f64, want[4]);
            close(&format!("{} warning estimate", row.name), est, want[5], 1e-6, 0.0);
        }
        let width = (r.m + 1) * r.ncomp;
        let yscale = r.ys.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let dscale = r.dys.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let mut pos = 6;
        let (mut worst_y, mut worst_d) = (0.0f64, 0.0f64);
        for s in [0, 1, n / 3, n / 2, n - 1] {
            close(&format!("{} t[{s}]", row.name), r.ts[s], want[pos], 1e-14, 1e-300);
            pos += 1;
            for f in [0.0, 0.21, 0.5, 0.77, 1.0] {
                let i = (f * (width - 1) as f64).round_ties_even() as usize;
                worst_y = worst_y.max((r.ys[s * width + i] - want[pos]).abs() / yscale);
                worst_d = worst_d.max((r.dys[s * width + i] - want[pos + 1]).abs() / dscale);
                pos += 2;
            }
        }
        eprintln!("{:20} {n:5} snapshots: u worst diff/max {worst_y:.1e}, u_t worst diff/max {worst_d:.1e}", row.name);
        assert!(worst_y < 1e-10 && worst_d < 1e-8, "{}: u {worst_y:e}, u_t {worst_d:e}", row.name);
    }
}
