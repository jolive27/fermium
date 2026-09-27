//! The unit and constant lookups the checker needs, from fermium-units.
pub use fermium_units::{constant, constants, lookup_unit, parse_unit_string, Constant, Unit, UnitSyntaxError};
use fermium_ir::Dim;

/// The name of a dimension for messages ("length [m]").
pub fn dim_name(dim: &Dim) -> String {
    fermium_units::dim_name(dim)
}

/// Python `format_number(x, sig=6)`: `None` means the default 6 figures.
pub fn format_number(x: f64, sig: Option<i64>) -> String {
    fermium_units::numfmt::format_number(x, sig.unwrap_or(6), true)
}

const ANGLE_WORDS: &[&str] = &["rev", "rpm", "rad", "°", "deg", "arcmin", "arcsec"];

/// The words of a unit name: `J/(kg K)` → {J, kg, K} (Python _unit_words).
pub fn unit_words(name: &str) -> std::collections::BTreeSet<String> {
    name.split(|c: char| c.is_whitespace() || "/·*^()⁰¹²³⁴⁵⁶⁷⁸⁹⁻".contains(c))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

fn prefixed(w: &str, prefixes: &str, base: &str) -> bool {
    if w == base {
        return true;
    }
    match w.strip_suffix(base) {
        Some(p) => p.chars().count() == 1 && prefixes.contains(p),
        None => false,
    }
}

/// A light tag on a unit the user wrote, for mix-ups that share an SI dimension (Python _unit_kind, D95):
/// "cycles" (Hz), "angular" (rad/s, rev/s, rpm, °/s), "Gy", "Sv", "Bq", "energy" (J), "torque" (N m), or "".
pub fn unit_kind(h: &fermium_ir::Hint) -> &'static str {
    let w = unit_words(&h.name);
    if w.is_empty() {
        return "";
    }
    if w.iter().any(|x| ANGLE_WORDS.contains(&x.as_str()) || x.ends_with("rad")) {
        return "angular";
    }
    if w.iter().any(|x| prefixed(x, "kMGTPm", "Hz")) {
        return "cycles";
    }
    for tag in ["Gy", "Sv", "Bq"] {
        let any_letter = |x: &str| match x.strip_suffix(tag) {
            Some(p) => p.is_empty() || (p.chars().count() == 1 && p.chars().all(|c| c.is_ascii_alphabetic() || c == 'μ')),
            None => false,
        };
        if w.len() == 1 && w.iter().any(|x| any_letter(x)) {
            return tag;
        }
    }
    if w.len() == 1 && prefixed(w.iter().next().unwrap(), "kMGTmμn", "J") {
        return "energy";
    }
    if w.len() == 2 && w.contains("m") && w.iter().any(|x| prefixed(x, "kMm", "N")) {
        return "torque";
    }
    ""
}
