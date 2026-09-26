//! Explicit ODE solvers against v1 (fixtures/ode.txt from rust/tools/numerics_fixtures.py).
mod common;
use common::*;
use fermium_runtime::numerics::ode::{dp45, rk4, EventFn, OdeOpts, Sol};
use fermium_runtime::numerics::Fail;

type Rhs = Box<dyn FnMut(f64, &[f64], &mut [f64])>;
type Ev = Option<Box<dyn FnMut(f64, &[f64]) -> f64>>;

/// ODE_CASES of numerics_fixtures.py: (method, f, y0, t0, t1, rtol or step, event, tdep, atol)
fn case(name: &str) -> (&'static str, Rhs, Vec<f64>, f64, f64, f64, Ev, bool, Option<Vec<f64>>) {
    use std::f64::consts::PI;
    let proj_ev = || -> Ev { Some(Box::new(|_t: f64, y: &[f64]| y[0])) };
    match name {
        "decay6" => ("rk45", Box::new(|_t, y, o| o[0] = -y[0]), vec![1.0], 0.0, 5.0, 1e-6, None, false, None),
        "decay10" => ("rk45", Box::new(|_t, y, o| o[0] = -y[0]), vec![1.0], 0.0, 5.0, 1e-10, None, false, None),
        "harmonic" => (
            "rk45",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = -4.0 * y[0];
            }),
            vec![1.0, 0.0],
            0.0,
            20.0,
            1e-8,
            None,
            false,
            None,
        ),
        "vdp" => (
            "rk45",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = 5.0 * (1.0 - y[0] * y[0]) * y[1] - y[0];
            }),
            vec![2.0, 0.0],
            0.0,
            20.0,
            1e-8,
            None,
            false,
            None,
        ),
        "kepler" => (
            "rk45",
            Box::new(|_t, y, o| {
                let r3 = (y[0] * y[0] + y[1] * y[1]).powf(1.5);
                o[0] = y[2];
                o[1] = y[3];
                o[2] = -y[0] / r3;
                o[3] = -y[1] / r3;
            }),
            vec![0.5, 0.0, 0.0, 3f64.sqrt()],
            0.0,
            2.0 * PI,
            1e-10,
            None,
            false,
            None,
        ),
        "backwards" => ("rk45", Box::new(|t, y, o| o[0] = t * y[0]), vec![1.0], 2.0, 0.0, 1e-8, None, false, None),
        "lorenz" => (
            "rk45",
            Box::new(|_t, y, o| {
                o[0] = 10.0 * (y[1] - y[0]);
                o[1] = y[0] * (28.0 - y[2]) - y[1];
                o[2] = y[0] * y[1] - 8.0 / 3.0 * y[2];
            }),
            vec![1.0, 1.0, 1.0],
            0.0,
            10.0,
            1e-9,
            None,
            false,
            None,
        ),
        "projectile" => (
            "rk45",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = -9.81;
            }),
            vec![0.0, 20.0],
            0.0,
            100.0,
            1e-8,
            proj_ev(),
            false,
            None,
        ),
        "jump" => (
            "rk45",
            Box::new(|t, _y, o| o[0] = if t < 0.3 { 1.0 } else { -2.0 }),
            vec![0.0],
            0.0,
            1.0,
            1e-8,
            None,
            true,
            None,
        ),
        "atol" => (
            "rk45",
            Box::new(|t, y, o| o[0] = -y[0] + 1e-20 * t.cos()),
            vec![1e-20],
            0.0,
            10.0,
            1e-6,
            None,
            false,
            Some(vec![1e-24]),
        ),
        "stiffish" => ("rk45", Box::new(|t, y, o| o[0] = -1e5 * (y[0] - t.cos())), vec![0.0], 0.0, 4.0, 1e-6, None, false, None),
        "blowup" => ("rk45", Box::new(|_t, y, o| o[0] = y[0] * y[0]), vec![1.0], 0.0, 2.0, 1e-8, None, false, None),
        "nan_start" => (
            "rk45",
            Box::new(|t, _y, o| o[0] = if t != 0.0 { 1.0 / t } else { f64::INFINITY }),
            vec![1.0],
            0.0,
            1.0,
            1e-8,
            None,
            false,
            None,
        ),
        "no_event" => (
            "rk45",
            Box::new(|_t, _y, o| o[0] = 1.0),
            vec![0.0],
            0.0,
            1.0,
            1e-8,
            Some(Box::new(|_t: f64, y: &[f64]| y[0] + 1.0)),
            false,
            None,
        ),
        "rk4_harm" => (
            "rk4",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = -y[0];
            }),
            vec![1.0, 0.0],
            0.0,
            10.0,
            0.01,
            None,
            false,
            None,
        ),
        "rk4_coarse" => (
            "rk4",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = -y[0];
            }),
            vec![1.0, 0.0],
            0.0,
            30.0,
            0.7,
            None,
            false,
            None,
        ),
        "rk4_event" => (
            "rk4",
            Box::new(|_t, y, o| {
                o[0] = y[1];
                o[1] = -9.81;
            }),
            vec![0.0, 20.0],
            0.0,
            100.0,
            0.01,
            proj_ev(),
            false,
            None,
        ),
        "rk4_back" => ("rk4", Box::new(|_t, y, o| o[0] = -2.0 * y[0]), vec![1.0], 1.0, -1.0, 0.05, None, false, None),
        _ => panic!("no ODE case {name}"),
    }
}

const FRACS: [f64; 5] = [0.1, 0.33, 0.5, 0.77, 0.999];

fn run(name: &str) -> Result<(Sol, f64), Fail> {
    let (method, mut f, y0, t0, t1, par, mut ev, tdep, atol) = case(name);
    let opts = OdeOpts { rtol: par, atol: atol.as_deref(), tname: -1.0, evtext: -1.0, tdep };
    let evr: Option<EventFn<'_>> = ev.as_mut().map(|b| &mut **b as EventFn<'_>);
    let sol = if method == "rk4" {
        rk4(&mut f, &y0, t0, t1, par, evr, opts)?
    } else {
        dp45(&mut f, &y0, t0, t1, evr, opts)?
    };
    Ok((sol, t0))
}

#[test]
fn explicit_solvers_match_v1() {
    let rows = load("ode.txt");
    assert!(rows.len() >= 18);
    let mut identical = 0usize;
    let mut total = 0usize;
    for row in &rows {
        let got = run(&row.name);
        if row.is_err() {
            let e = got.err().unwrap_or_else(|| panic!("{}: expected an error", row.name));
            assert_eq!(e.kind, row.fields[1].parse::<i64>().unwrap(), "{}", row.name);
            close(&format!("{} err.a", row.name), e.a, row.f(2), 1e-12, 0.0);
            continue;
        }
        let (sol, t0) = got.unwrap_or_else(|e| panic!("{}: {e}", row.name));
        let want = row.floats();
        let mut vals = vec![sol.n() as f64, *sol.t.last().unwrap()];
        vals.extend_from_slice(&sol.y[sol.y.len() - sol.dim..]);
        let tl = *sol.t.last().unwrap();
        for fr in FRACS {
            let tt = t0 + fr * (tl - t0);
            vals.push(sol.eval(0, tt, false, None).unwrap());
            vals.push(sol.eval(0, tt, true, None).unwrap());
        }
        vals.push(sol.extreme(0, 1.0));
        vals.push(sol.extreme(0, -1.0));
        vals.push(sol.warnings.len() as f64);
        let (wk, wa) = sol.warnings.first().map(|&(k, a)| (k as f64, a)).unwrap_or((0.0, 0.0));
        vals.push(wk);
        vals.push(wa);
        assert_eq!(vals.len(), want.len(), "{}: layout", row.name);
        assert_eq!(vals[0], want[0], "{}: step count", row.name);
        for (i, (&g, &w)) in vals.iter().zip(&want).enumerate() {
            // another libm: last-bit differences grow along a trajectory; values of order 1, so an absolute floor
            close(&format!("{}[{i}]", row.name), g, w, 1e-12, if FIXTURE_LIBM { 1e-300 } else { 1e-12 });
            total += 1;
            if g == w || (g.is_nan() && w.is_nan()) {
                identical += 1;
            }
        }
    }
    eprintln!("ode: {identical} of {total} numbers bit-identical to v1");
}
