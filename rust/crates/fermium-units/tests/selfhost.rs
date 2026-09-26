//! The unit factors build.rs computes from fermium/selfhost/units_db.fm must equal Fermium 1.5's
//! generated fermium/units_selfhosted.py bit for bit.

use std::path::PathBuf;

#[test]
fn self_hosted_factors_equal_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let path = ["fermium/units_selfhosted.py", "legacy/fermium/units_selfhosted.py"]
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.exists())
        .expect("fermium/units_selfhosted.py");
    let text = std::fs::read_to_string(path).unwrap();
    let mut py: Vec<(String, f64)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('\'') {
            let (name, val) = rest.split_once("': ").unwrap();
            py.push((name.to_string(), val.trim_end_matches(',').parse().unwrap()));
        }
    }
    let rs = fermium_units::self_hosted_factors();
    assert_eq!(py.len(), rs.len(), "number of unit factors");
    for ((pn, pv), (rn, rv)) in py.iter().zip(rs) {
        assert_eq!(pn, rn);
        assert_eq!(pv.to_bits(), rv.to_bits(), "{pn}: Python {pv:?}, Rust {rv:?}");
    }
    // and lookup_unit uses them
    assert_eq!(fermium_units::lookup_unit("M☉").unwrap().factor, 1.9884098706980512e30);
    assert_eq!(fermium_units::lookup_unit("psi").unwrap().factor, 6894.75729316836);
}
