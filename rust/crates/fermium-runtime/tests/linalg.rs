//! Small/big dense linear algebra against v1 (fermium.linalg / linalg_big), fixtures/linalg.txt.
mod common;
use common::*;
use fermium_runtime::numerics::linalg::*;

#[test]
fn linalg_bit_identical_to_v1() {
    let rows = by_name("linalg.txt");
    let names: Vec<String> = rows.keys().filter_map(|k| k.strip_suffix(":A").map(str::to_string)).collect();
    assert_eq!(names.len(), 12);
    let mut count = 0;
    for name in names {
        let get = |s: &str| rows[&format!("{name}:{s}")].floats();
        let a = get("A");
        let n = (a.len() as f64).sqrt() as usize;
        let b = get("B");
        let m = get("M");
        let s: Vec<f64> = (0..n * n).map(|ij| 0.5 * (a[(ij / n) * n + ij % n] + a[(ij % n) * n + ij / n])).collect();
        assert_eq!(det(&a, n), get("det")[0], "{name} det");
        assert_eq!(solve(&a, n, &b, 1).unwrap(), get("solve"), "{name} solve");
        assert_eq!(inverse(&a, n).unwrap(), get("inv"), "{name} inverse");
        let (vals, vecs) = jacobi_eigen(&s, n);
        let want = get("eig");
        assert_eq!([vals, vecs].concat(), want, "{name} eigen");
        let (gv, gw, _) = generalized_eigen(&s, &m, n);
        assert_eq!([gv, gw].concat(), get("geig"), "{name} generalized eigen");
        count += 1;
    }
    eprintln!("linalg: {count} matrices (2x2 .. 16x16), det/solve/inverse/eigen/generalized all bit-identical");
}
