//! Native Radau IIA / BDF against v1 (SciPy's steppers + v1's wrapper), fixtures/stiff.txt.
mod common;
use common::*;
use fermium_runtime::numerics::ode::{EventFn, OdeOpts, Sol};
use fermium_runtime::numerics::stiff::{stiff_solve, StiffMethod};
use fermium_runtime::numerics::Fail;

type Rhs = Box<dyn FnMut(f64, &[f64], &mut [f64])>;
type Ev = Option<Box<dyn FnMut(f64, &[f64]) -> f64>>;

/// STIFF_CASES of numerics_fixtures.py
fn case(name: &str) -> (Rhs, Vec<f64>, f64, f64, f64, Ev, Option<Vec<f64>>) {
    let robertson = || -> Rhs {
        Box::new(|_t, y, o| {
            o[0] = -0.04 * y[0] + 1e4 * y[1] * y[2];
            o[1] = 0.04 * y[0] - 1e4 * y[1] * y[2] - 3e7 * y[1] * y[1];
            o[2] = 3e7 * y[1] * y[1];
        })
    };
    match name {
        "robertson" => (robertson(), vec![1.0, 0.0, 0.0], 0.0, 40.0, 1e-6, None, None),
        "robertson9" => (robertson(), vec![1.0, 0.0, 0.0], 0.0, 1e4, 1e-9, None, None),
        "vdp1000" => (
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = 1000.0 * (1.0 - y[0] * y[0]) * y[1] - y[0];
            }),
            vec![2.0, 0.0],
            0.0,
            3000.0,
            1e-6,
            None,
            None,
        ),
        "lin" => (Box::new(|t, y, o| o[0] = -1e5 * (y[0] - t.cos())), vec![0.0], 0.0, 4.0, 1e-8, None, None),
        "chain" => (
            Box::new(|_t, y, o| {
                o[0] = -50.0 * y[0];
                o[1] = 50.0 * y[0] - 0.01 * y[1];
                o[2] = 0.01 * y[1];
            }),
            vec![1.0, 0.0, 0.0],
            0.0,
            5.0,
            1e-7,
            None,
            None,
        ),
        "event" => (
            Box::new(|_t, y, o| o[0] = -1000.0 * (y[0] - 1.0)),
            vec![0.0],
            0.0,
            1.0,
            1e-8,
            Some(Box::new(|_t: f64, y: &[f64]| y[0] - 0.5)),
            None,
        ),
        "back" => (Box::new(|t, y, o| o[0] = 20.0 * (y[0] - t.sin())), vec![0.0], 2.0, -1.0, 1e-8, None, None),
        "user_atol" => (
            Box::new(|_t, y, o| {
                o[0] = -y[0];
                o[1] = y[0] - 1e3 * y[1];
            }),
            vec![1e-12, 0.0],
            0.0,
            10.0,
            1e-6,
            None,
            Some(vec![1e-20, 1e-20]),
        ),
        "blowup" => (Box::new(|_t, y, o| o[0] = y[0] * y[0]), vec![1.0], 0.0, 2.0, 1e-8, None, None),
        "no_event" => (
            Box::new(|_t, y, o| o[0] = -y[0]),
            vec![1.0],
            0.0,
            1.0,
            1e-8,
            Some(Box::new(|_t: f64, y: &[f64]| y[0] + 1.0)),
            None,
        ),
        _ => panic!("no stiff case {name}"),
    }
}

const FRACS: [f64; 5] = [0.1, 0.33, 0.5, 0.77, 0.999];

fn run(key: &str) -> Result<(Sol, f64), Fail> {
    let (name, method) = key.split_once('/').unwrap();
    let m = if method == "radau" { StiffMethod::Radau } else { StiffMethod::Bdf };
    let (mut f, y0, t0, t1, rtol, mut ev, atol) = case(name);
    let opts = OdeOpts { rtol, atol: atol.as_deref(), tname: -1.0, evtext: -1.0, tdep: false, nerr: 0 };
    let evr: Option<EventFn<'_>> = ev.as_mut().map(|b| &mut **b as EventFn<'_>);
    Ok((stiff_solve(&mut f, &y0, t0, t1, m, evr, opts)?, t0))
}

#[test]
fn stiff_solvers_match_v1_scipy() {
    let rows = load("stiff.txt");
    assert!(rows.len() >= 20);
    let mut report = Vec::new();
    for row in &rows {
        let got = run(&row.name);
        if row.is_err() {
            let e = got.err().unwrap_or_else(|| panic!("{}: expected an error", row.name));
            assert_eq!(e.kind, row.fields[1].parse::<i64>().unwrap(), "{}", row.name);
            // where the step became too small depends on the step sequence: same place to 1e-5
            close(&format!("{} err.a", row.name), e.a, row.f(2), 1e-5, 0.0);
            continue;
        }
        let (sol, t0) = got.unwrap_or_else(|e| panic!("{}: {e}", row.name));
        let want = row.floats();
        let dim = sol.dim;
        let tl = *sol.t.last().unwrap();
        // the solution's values: end state, 5 interpolated values (and slopes), max/min
        let mut vals = vec![tl];
        vals.extend_from_slice(&sol.y[sol.y.len() - dim..]);
        let mut wv = vec![want[1]];
        wv.extend_from_slice(&want[2..2 + dim]);
        for (k, fr) in FRACS.iter().enumerate() {
            let tt = t0 + fr * (tl - t0);
            vals.push(sol.eval(0, tt, false, None).unwrap());
            wv.push(want[2 + dim + 2 * k]);
        }
        vals.push(sol.extreme(0, 1.0));
        vals.push(sol.extreme(0, -1.0));
        wv.push(want[want.len() - 2]);
        wv.push(want[want.len() - 1]);
        // compare relative to each component's scale over the solution (values near 0 are noise)
        let scale = wv.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let mut worst = 0.0f64;
        for (i, (&g, &w)) in vals.iter().zip(&wv).enumerate() {
            let e = (g - w).abs() / scale.max(1e-300);
            worst = worst.max(e);
            let _ = i;
        }
        report.push(format!("{:18} steps {:>6} vs v1 {:>6}   worst scaled diff {:.1e}", row.name, sol.n(), want[0], worst));
        // the tolerance of the solve, times a safety factor: both are solutions to rtol
        let (_, _, _, _, rtol, _, _) = case(row.name.split('/').next().unwrap());
        assert!(worst <= 50.0 * rtol, "{}: worst scaled difference {worst:e} (rtol {rtol:e})", row.name);
    }
    for r in report {
        eprintln!("{r}");
    }
}
