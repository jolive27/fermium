//! Root finding against v1 (interp.root) and SciPy (brentq), fixtures/roots.txt.
mod common;
use common::*;
use fermium_runtime::numerics::roots::{brentq, root};

type F = Box<dyn Fn(f64) -> f64>;

/// ROOT_CASES of numerics_fixtures.py: (f, scale, a, b)
fn case(name: &str) -> (F, Option<F>, f64, f64) {
    match name {
        "cos_x" => (Box::new(|x: f64| x.cos() - x), Some(Box::new(|x: f64| x.cos().abs() + x.abs())), 0.0, 2.0),
        "first_of_many" => (Box::new(|x: f64| x.sin()), None, 0.5, 20.0),
        "cubic" => (Box::new(|x: f64| x * x * x - 2.0 * x - 5.0), None, 0.0, 3.0),
        "kepler_E" => (Box::new(|x: f64| x - 0.9 * x.sin() - 1.3), None, 0.0, 3.0),
        "at_start" => (Box::new(|x: f64| x - 1.0), None, 1.0, 2.0),
        "on_scan_point" => (Box::new(|x: f64| x - 0.5), None, 0.0, 1.0),
        "planck_peak" => (Box::new(|x: f64| 3.0 * (1.0 - (-x).exp()) - x), None, 0.5, 10.0),
        "tiny_scale" => (Box::new(|x: f64| x - 3.3e-19), None, 0.0, 1e-18),
        "noise" => (
            Box::new(|x: f64| (1e16 + x) - 1e16 - 0.5),
            Some(Box::new(|x: f64| (1e16 + x).abs() + 1e16 + 0.5)),
            0.0,
            3.0,
        ),
        "no_root" => (Box::new(|x: f64| x * x + 1.0), None, -1.0, 1.0),
        "pole" => (Box::new(|x: f64| x.tan()), None, 1.0, 2.0),
        "backwards" => (Box::new(|x: f64| x.exp() - 2.0), None, 3.0, 0.0),
        _ => panic!("no root case {name}"),
    }
}

#[test]
fn roots_match_v1_and_scipy() {
    let rows = load("roots.txt");
    assert!(rows.len() >= 12);
    for row in rows {
        let (f, scale, a, b) = case(&row.name);
        let mut sc = scale.map(|s| move |x: f64| s(x));
        let got = root(&f, a, b, 200, sc.as_mut().map(|s| s as &mut dyn FnMut(f64) -> f64));
        if row.is_err() {
            let e = got.unwrap_err();
            assert_eq!(e.kind, row.fields[1].parse::<i64>().unwrap(), "{}", row.name);
            assert_eq!(e.a, row.f(2), "{}", row.name);
            continue;
        }
        let r = got.unwrap_or_else(|e| panic!("{}: {e}", row.name));
        assert_eq!(r.x, row.f(0), "{}: v1's root", row.name);
        assert_eq!(r.noise_warning.unwrap_or(f64::NAN).to_bits(), row.f(1).to_bits(), "{}: warning", row.name);
        let bq = row.f(2);
        if !bq.is_nan() {
            let (lo, hi) = (a.min(b), a.max(b));
            let got = brentq(&f, lo, hi, 2e-12, 4.0 * f64::EPSILON, 100);
            let got = if a < b { got } else { brentq(&f, a, b, 2e-12, 4.0 * f64::EPSILON, 100) };
            assert_eq!(got.unwrap(), bq, "{}: brentq", row.name);
        }
    }
}
