//! Special functions against v1 (glibc for the libm ones, fermium/special.py for I, K, K(m), E(m))
//! and SciPy, fixtures/special.txt.
mod common;
use common::*;
use fermium_runtime::numerics::special::*;
use std::collections::BTreeMap;

/// the difference, relative to max(|want|, floor): floor 1 for the oscillating J, Y (absolute near
/// their zeros), else a tiny floor
fn diff(fname: &str, got: f64, want: f64) -> f64 {
    if got == want || (got.is_nan() && want.is_nan()) {
        return 0.0;
    }
    if got.is_infinite() || want.is_infinite() {
        return f64::INFINITY;
    }
    let floor = if fname == "besselj" || fname == "bessely" { 1.0 } else { 1e-300 };
    (got - want).abs() / want.abs().max(floor)
}

#[test]
fn special_functions_match_v1() {
    let mut stats: BTreeMap<String, (usize, usize, f64, f64)> = BTreeMap::new(); // n, identical, worst vs v1, worst vs scipy
    for row in load("special.txt") {
        let parts: Vec<&str> = row.name.split(':').collect();
        let f = parts[0];
        let vals: Vec<(f64, f64, f64)> = match f {
            "ellip" => {
                let m = parse(parts[1]);
                let (k, e) = ellip(m);
                vec![(k, row.f(0), row.f(2)), (e, row.f(1), row.f(3))]
            }
            "erf" | "erfc" | "gamma" | "lgamma" => {
                let x = parse(parts[1]);
                let g = match f {
                    "erf" => erf(x),
                    "erfc" => erfc(x),
                    "gamma" => gamma(x),
                    _ => lgamma(x),
                };
                vec![(g, row.f(0), row.f(1))]
            }
            _ => {
                let n = parse(parts[1]);
                let x = parse(parts[2]);
                let g = match f {
                    "besselj" => besselj(n, x),
                    "bessely" => bessely(n, x),
                    "besseli" => besseli(n, x),
                    _ => besselk(n, x),
                };
                vec![(g, row.f(0), row.f(1))]
            }
        };
        let st = stats.entry(f.to_string()).or_insert((0, 0, 0.0, 0.0));
        for (g, v1, sp) in vals {
            st.0 += 1;
            if g.to_bits() == v1.to_bits() || (g.is_nan() && v1.is_nan()) {
                st.1 += 1;
            }
            let d = diff(f, g, v1);
            st.2 = st.2.max(d);
            // v1's own algorithms (I, K, ellip) must be bit-identical; libm ones within a few ulp
            let tol = match f {
                "besseli" | "besselk" | "ellip" => 0.0,
                "gamma" | "lgamma" => 1e-14,
                "besselj" | "bessely" => 2e-15,
                _ => 4e-16,
            };
            assert!(d <= tol, "{}: got {g:e}, v1 {v1:e} (diff {d:e})", row.name);
            if sp.is_finite() && g.is_finite() {
                st.3 = st.3.max(diff(f, g, sp));
            }
        }
    }
    for (f, (n, same, w1, ws)) in &stats {
        eprintln!("{f:8} {n:4} values, {same:4} bit-identical to v1, worst vs v1 {w1:.1e}, worst vs SciPy {ws:.1e}");
    }
}
