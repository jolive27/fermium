//! The seeded RNG against v1 (fermium/rng.py), fixtures/rng.txt: the exact same stream.
mod common;
use common::*;
use fermium_runtime::numerics::rng::Rng;

#[test]
fn rng_stream_identical_to_v1() {
    let rows = load("rng.txt");
    assert_eq!(rows.len(), 11);
    for row in rows {
        let s = row.name.strip_prefix("seed:").unwrap();
        let mut r = Rng::default();
        if s != "default" {
            r.seed(parse(s));
        }
        let mut got: Vec<f64> = (0..20).map(|_| r.rand()).collect();
        got.extend((0..20).map(|_| r.randn()));
        for _ in 0..5 {
            got.push(r.rand_range(2.0, 7.0));
            got.push(r.randn_ms(1.0, 0.5).unwrap());
        }
        assert_eq!(got, row.floats(), "seed {s}");
    }
    assert_eq!(Rng::default().randn_ms(0.0, -1.0).unwrap_err().kind, 26);
}
