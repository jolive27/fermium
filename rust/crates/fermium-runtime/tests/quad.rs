//! Quadrature against v1 (fixtures/quad.txt from rust/tools/numerics_fixtures.py).
mod common;
use common::*;
use fermium_runtime::numerics::quad::{quad, quad_v1};

/// The integrands of QUAD_CASES in numerics_fixtures.py, by name.
fn integrand(name: &str) -> (Box<dyn Fn(f64) -> f64>, f64, f64) {
    use std::f64::INFINITY as INF;
    let b: (Box<dyn Fn(f64) -> f64>, f64, f64) = match name {
        "poly" => (Box::new(|x: f64| x * x), 0.0, 3.0),
        "gauss_inf" => (Box::new(|x: f64| (-x * x).exp()), -INF, INF),
        "inv_sqrt" => (Box::new(|x: f64| 1.0 / x.sqrt()), 0.0, 1.0),
        "planck" => (Box::new(|x: f64| x * x * x / (x.exp() - 1.0)), 0.0, INF),
        "sinc" => (Box::new(|x: f64| x.sin() / x), 0.0, 10.0),
        "decay" => (Box::new(|x: f64| (-x / 2.5).exp()), 0.0, INF),
        "lorentz" => (Box::new(|x: f64| 1.0 / (1.0 + x * x)), -INF, INF),
        "log" => (Box::new(|x: f64| x.ln()), 0.0, 1.0),
        "damped_cos" => (Box::new(|x: f64| (30.0 * x).cos() * (-x).exp()), 0.0, 5.0),
        "reversed" => (Box::new(|x: f64| x * x * x), 2.0, -1.0),
        "left_tail" => (Box::new(|x: f64| x.exp()), -INF, 0.0),
        "power_tail" => (Box::new(|x: f64| 1.0 / ((1.0 + x) * (1.0 + x))), 0.0, INF),
        "kink" => (Box::new(|x: f64| (x - 0.3).abs()), 0.0, 1.0),
        "step" => (Box::new(|x: f64| if x < 0.37 { 1.0 } else { 2.0 }), 0.0, 1.0),
        "sqrt_sing_mid" => (Box::new(|x: f64| 1.0 / (x - 0.5).abs().sqrt()), 0.0, 1.0),
        "atan_tail" => (Box::new(|x: f64| x.atan() / (1.0 + x * x * x)), 0.0, INF),
        "tiny_scale" => (Box::new(|x: f64| (-x / 1e-15).exp()), 0.0, INF),
        "huge_scale" => (Box::new(|x: f64| (-x / 1e20).exp()), 0.0, INF),
        "tanh_wall" => (Box::new(|x: f64| (1000.0 * (x - 0.123)).tanh()), 0.0, 1.0),
        "x_sin_inf" => (Box::new(|x: f64| x * (-x).exp() * x.sin()), 0.0, INF),
        "gauss_offset" => (Box::new(|x: f64| (-(x - 7.3) * (x - 7.3)).exp()), -INF, INF),
        "zero" => (Box::new(|x: f64| 0.0 * x), 0.0, 1.0),
        "x_pow_m09" => (Box::new(|x: f64| x.powf(-0.9)), 0.0, 1.0),
        "nested_scale" => (Box::new(|x: f64| (-x * x / 2e-6).exp()), -1.0, 1.0),
        "rt1_1" => (Box::new(|x: f64| (-((x - 1.0) / 1e-6) * ((x - 1.0) / 1e-6)).exp()), 0.0, INF),
        "bl1" => (Box::new(|x: f64| (-(x - 1000.0) * (x - 1000.0) * 100.0).exp()), 0.0, 2000.0),
        "bl2" => (Box::new(|x: f64| (-x * x).exp()), -1e6, 1e6),
        "bl3_06" => (Box::new(|x: f64| (x - 0.3).abs().powf(-0.6)), 0.0, 1.0),
        "bl3_08" => (Box::new(|x: f64| (x - 0.3).abs().powf(-0.8)), 0.0, 1.0),
        "recip" => (Box::new(|x: f64| 1.0 / x), 0.0, 1.0),
        "sin_inf" => (Box::new(|x: f64| x.sin()), 0.0, INF),
        _ => panic!("no integrand {name}"),
    };
    b
}

/// Cases where the B2 quadrature intentionally differs from v1 (v1 was wrong): it must give the truth.
const B2_FIXED: &[&str] = &["rt1_1", "bl1", "bl2", "bl3_06", "bl3_08"];

#[test]
fn quad_v1_matches_v1_exactly() {
    let rows = load("quad.txt");
    assert!(rows.len() >= 30);
    let mut exact = 0;
    for row in &rows {
        let (f, a, b) = integrand(&row.name);
        let got = quad_v1(|x| f(x), a, b, 1e-10, 0.0, -1.0);
        if row.is_err() {
            let e = got.expect_err(&row.name);
            assert_eq!(e.kind, row.fields[1].parse::<i64>().unwrap(), "{}", row.name);
            close(&row.name, e.a, row.f(2), 1e-12, 0.0);
        } else {
            let g = got.unwrap_or_else(|e| panic!("{}: {e}", row.name));
            close(&row.name, g.value, row.f(0), 1e-14, 0.0);
            assert_eq!(g.all_zero, row.f(1) == 1.0, "{} all_zero", row.name);
            if g.value == row.f(0) {
                exact += 1;
            }
        }
    }
    eprintln!("quad_v1: {exact} of {} values bit-identical to v1", rows.len());
}

#[test]
fn quad_b2_keeps_v1_results_and_fixes_its_failures() {
    for row in load("quad.txt") {
        let (f, a, b) = integrand(&row.name);
        let got = quad(|x| f(x), a, b, 1e-10, 0.0, -1.0);
        let truth = parse(row.fields.last().unwrap());
        if B2_FIXED.contains(&row.name.as_str()) {
            let g = got.unwrap_or_else(|e| panic!("{}: {e}", row.name)).value;
            eprintln!("B2 {}: {g:e} vs truth {truth:e}, rel {:.1e}", row.name, rel(g, truth));
            // bl3_08: the Aitken tail limits the accuracy (NUMERICS.md)
            let tol = if row.name == "bl3_08" { 1e-7 } else { 1e-9 };
            close(&row.name, g, truth, tol, 0.0);
        } else if row.is_err() {
            assert_eq!(got.expect_err(&row.name).kind, row.fields[1].parse::<i64>().unwrap(), "{}", row.name);
        } else {
            let g = got.unwrap_or_else(|e| panic!("{}: {e}", row.name));
            if FIXTURE_LIBM {
                assert_eq!(g.value, row.f(0), "{}: B2 changed a result v1 got right", row.name);
            } else {
                close(&format!("{}: B2 changed a result v1 got right", row.name), g.value, row.f(0), 1e-14, 0.0);
            }
        }
    }
}
