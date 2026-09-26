//! Choosing a unit to print a value in, and describing dimensions in words (port of `preferred_unit`,
//! `_composite_unit`, `dim_name`, `suggest_units` in fermium/units.py and `display_unit` in
//! fermium/runtime/core.py).

use crate::db::Unit;
use crate::dim::*;
use crate::parse::parse_unit_string;
use std::collections::HashMap;
use std::sync::OnceLock;

fn u(spec: &str) -> Unit {
    parse_unit_string(spec).unwrap_or_else(|e| panic!("{spec}: {e}"))
}

/// The display units tried first, in order (`_PREFERRED_SPECS`).
pub const PREFERRED_SPECS: &[&str] = &[
    "m", "kg", "s", "A", "K", "mol", "cd",
    "m/s", "m/s²", "m²", "m³", "kg/m³", "kg m/s", "N", "J", "W", "Pa", "C", "V", "Ω", "F", "T", "Wb", "H",
    "N/m", "J/K", "J s", "N m²/kg²", "W/m²", "W/(m² K⁴)", "J/(kg K)", "J/(mol K)", "1/mol", "F/m", "H/m",
    "kg m²", "m²/s", "m³/(kg s²)", "V/m", "A/m", "kg/s", "m²/s²", "Pa s", "C/kg", "W/m³", "C²", "C m",
    "J/T", "C/m²", "C/m³", "1/m", "1/m²", "1/m³", "kg/m²", "J/m³", "N/m²",
    "S/m",
    "J/(m³ K⁴)",
    "J m",
];

fn preferred_map() -> &'static HashMap<Dim, Unit> {
    static P: OnceLock<HashMap<Dim, Unit>> = OnceLock::new();
    P.get_or_init(|| {
        let mut m = HashMap::new();
        for spec in PREFERRED_SPECS {
            let x = u(spec);
            m.entry(x.dim).or_insert_with(|| Unit::new(*spec, x.dim, 1.0));
        }
        m
    })
}

const COMPOSITE_NAMED: [&str; 11] = ["V", "N", "J", "W", "T", "Pa", "C", "Ω", "F", "H", "Wb"];
const COMPOSITE_BASE: [&str; 15] =
    ["m", "m²", "m³", "s", "s²", "kg", "K", "mol", "A", "kg²", "m s", "m² K", "m K", "kg K", "mol K"];

fn composite_map() -> &'static HashMap<Dim, String> {
    static C: OnceLock<HashMap<Dim, String>> = OnceLock::new();
    C.get_or_init(|| {
        let mut m = HashMap::new();
        for n in COMPOSITE_NAMED {
            let un = u(n);
            for bname in COMPOSITE_BASE {
                let ub = u(bname);
                let over = if bname.contains(' ') { format!("{n}/({bname})") } else { format!("{n}/{bname}") };
                for (name, dim) in [(over, un.dim / ub.dim), (format!("{n} {bname}"), un.dim * ub.dim)] {
                    if name != "N/A" {
                        m.entry(dim).or_insert(name);
                    }
                }
            }
        }
        m
    })
}

/// A named unit times or over a simple one (V/m², T m, W/(m² K)) for a dimension of 3+ base quantities
/// with no name of its own.
fn composite_unit(d: &Dim) -> Option<Unit> {
    if d.n_bases() < 3 {
        return None;
    }
    composite_map().get(d).map(|name| Unit::new(name.clone(), *d, 1.0))
}

/// A display Unit for dimension d (SI, with a named derived unit if one fits); factor 1.
pub fn preferred_unit(d: &Dim) -> Unit {
    if let Some(u) = preferred_map().get(d) {
        return u.clone();
    }
    if *d == DIMLESS / TIME {
        return Unit::new("1/s", *d, 1.0);
    }
    if let Some(c) = composite_unit(d) {
        return c;
    }
    Unit::new(format_dim(d), *d, 1.0)
}

/// The unit a value is printed in: the hint (the unit it was written or converted in) if it has the
/// value's dimension, else the preferred SI unit.
pub fn display_unit(d: &Dim, hint: Option<&Unit>) -> Unit {
    match hint {
        Some(h) if h.dim == *d => h.clone(),
        _ => preferred_unit(d),
    }
}

/// Names of dimensions (`_DIM_NAMES`), first match wins.
pub const DIM_NAMES: &[(&str, &str)] = &[
    ("length", "m"), ("mass", "kg"), ("time", "s"), ("current", "A"), ("temperature", "K"),
    ("amount of substance", "mol"), ("luminous intensity", "cd"),
    ("speed", "m/s"), ("acceleration", "m/s²"), ("area", "m²"), ("volume", "m³"), ("density", "kg/m³"),
    ("momentum", "kg m/s"), ("force", "N"), ("energy", "J"), ("power", "W"), ("pressure", "Pa"),
    ("charge", "C"), ("voltage", "V"), ("resistance", "Ω"), ("capacitance", "F"),
    ("magnetic field", "T"), ("magnetic flux", "Wb"), ("inductance", "H"), ("frequency", "1/s"),
    ("spring constant", "N/m"), ("action", "J s"), ("angular momentum", "kg m²/s"),
    ("heat capacity", "J/K"), ("electric field", "V/m"), ("moment of inertia", "kg m²"),
    ("mass flow", "kg/s"), ("specific energy", "J/kg"),
];

fn dim_name_map() -> &'static HashMap<Dim, (&'static str, &'static str)> {
    static M: OnceLock<HashMap<Dim, (&'static str, &'static str)>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::new();
        for &(name, spec) in DIM_NAMES {
            m.entry(u(spec).dim).or_insert((name, spec));
        }
        m
    })
}

/// Human description: "length [m]", "energy [J]", "a quantity with units [kg/s³]",
/// "a plain number (no units)".
pub fn dim_name(d: &Dim) -> String {
    if d.is_dimensionless() {
        return "a plain number (no units)".into();
    }
    if let Some((name, spec)) = dim_name_map().get(d) {
        return format!("{name} [{spec}]");
    }
    format!("a quantity with units [{}]", preferred_unit(d).name)
}

/// The kinds of quantity used for "try `in J m`" suggestions (spec A3.3).
pub const KINDS: &[(&str, &[&str])] = &[
    ("energy", &["J", "eV", "MeV"]), ("length", &["m", "nm", "fm"]), ("time", &["s", "ns"]),
    ("mass", &["kg", "u", "MeV/c²"]), ("force", &["N"]), ("momentum", &["kg m/s", "MeV/c"]),
    ("speed", &["m/s", "km/s"]), ("acceleration", &["m/s²"]), ("density", &["kg/m³", "g/cm³"]),
    ("area", &["m²", "cm²"]), ("volume", &["m³", "L"]), ("charge", &["C"]),
    ("temperature", &["K"]), ("power", &["W"]), ("pressure", &["Pa", "atm"]), ("frequency", &["Hz", "1/s"]),
    ("current", &["A"]), ("voltage", &["V"]), ("magnetic field", &["T"]), ("electric field", &["V/m"]),
    ("amount of substance", &["mol"]),
];

/// (description, 1-3 unit spellings) for a dimension, or None: ("energy", ["J", "eV", "MeV"]),
/// ("energy × length", ["J m", "eV nm"]).
pub fn suggest_units(d: &Dim) -> Option<(String, Vec<String>)> {
    if d.is_dimensionless() {
        return None;
    }
    let kinds: Vec<(&str, Dim, &[&str])> = KINDS.iter().map(|(k, us)| (*k, u(us[0]).dim, *us)).collect();
    for (k, dim, us) in &kinds {
        if dim == d {
            return Some((k.to_string(), us.iter().take(3).map(|s| s.to_string()).collect()));
        }
    }
    let name = dim_name(d);
    if name.contains(" [") && !name.starts_with("a quantity with units") {
        let (k, spec) = name.split_once(" [").unwrap();
        return Some((k.to_string(), vec![spec.trim_end_matches(']').to_string()]));
    }
    for (i, (ka, da, ua)) in kinds.iter().enumerate() {
        for (kb, db, ub) in &kinds[i..] {
            if *da * *db == *d {
                let mut pairs = vec![format!("{} {}", ua[0], ub[0])];
                if ua.len() > 1 && ub.len() > 1 {
                    pairs.push(format!("{} {}", ua[1], ub[1]));
                }
                return Some((format!("{ka} × {kb}"), pairs));
            }
        }
    }
    for (ka, da, ua) in &kinds {
        for (kb, db, ub) in &kinds {
            if ka != kb && *da / *db == *d {
                let mut pairs = vec![format!("{}/{}", ua[0], ub[0])];
                if ua.len() > 1 && ub.len() > 1 {
                    pairs.push(format!("{}/{}", ua[1], ub[1]));
                }
                return Some((format!("{ka} / {kb}"), pairs));
            }
        }
    }
    None
}
