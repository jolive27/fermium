//! The unit database (port of the tables and `lookup_unit` in fermium/units.py). The factors of the non-SI
//! units come from Fermium itself: build.rs evaluates fermium/selfhost/units_db.fm (moonshot M8, spec §B4).

use crate::dim::*;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/units_db.rs"));

/// A unit: `value_in_SI = value * factor + offset` (the offset is non-zero only for °C and °F).
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub name: String,
    pub dim: Dim,
    pub factor: f64,
    pub offset: f64,
}

impl Unit {
    pub fn new(name: impl Into<String>, dim: Dim, factor: f64) -> Unit {
        Unit { name: name.into(), dim, factor, offset: 0.0 }
    }

    /// The plain number unit "1".
    pub fn one() -> Unit {
        Unit::new("1", DIMLESS, 1.0)
    }

    /// An affine unit (°C, °F): its zero is not SI's zero.
    pub fn affine(&self) -> bool {
        self.offset != 0.0
    }

    /// `self * o`, named "a b" (the offset is dropped, as in Python).
    pub fn mul(&self, o: &Unit) -> Unit {
        Unit::new(format!("{} {}", self.name, o.name), self.dim * o.dim, self.factor * o.factor)
    }

    /// `self / o`, named "a/b".
    pub fn div(&self, o: &Unit) -> Unit {
        Unit::new(format!("{}/{}", self.name, o.name), self.dim / o.dim, self.factor / o.factor)
    }

    /// `self ** p`, named "a²" (factor ** float(p), as Python computes it).
    pub fn pow(&self, p: num_rational::Rational64) -> Unit {
        let pf = *p.numer() as f64 / *p.denom() as f64;
        Unit::new(format!("{}{}", self.name, fmt_exp(p, true)), self.dim.pow(p), self.factor.powf(pf))
    }

    /// Convert a value in this unit to SI.
    pub fn to_si(&self, x: f64) -> f64 {
        x * self.factor + self.offset
    }

    /// Convert an SI value to this unit.
    pub fn from_si(&self, v: f64) -> f64 {
        (v - self.offset) / self.factor
    }
}

/// Greek small letter mu (the micro prefix Fermium uses).
pub const MU: &str = "\u{3bc}";
/// The micro sign, accepted as an alias of μ.
pub const MICRO_SIGN: &str = "\u{b5}";

pub(crate) struct Entry {
    pub factor: f64,
    pub dim: Dim,
    pub prefixable: bool,
}

fn dims() -> (Dim, Dim, Dim, Dim, Dim, Dim) {
    let n_ = MASS * LENGTH / TIME.powi(2);
    let j_ = n_ * LENGTH;
    let w_ = j_ / TIME;
    let c_ = CURRENT * TIME;
    let v_ = w_ / CURRENT;
    let pa_ = n_ / LENGTH.powi(2);
    (n_, j_, w_, c_, v_, pa_)
}

const C_LIGHT: f64 = 299792458.0;
const E_CHARGE: f64 = 1.602176634e-19;
const U_MASS: f64 = 1.66053906892e-27;
const AU: f64 = 149597870700.0;

/// name -> (factor, dim, prefixable), in the order of `_UNITS` in fermium/units.py, with the Python
/// literal factors (overridden by the self-hosted table where it has the name, as Python does).
fn unit_list() -> Vec<(&'static str, f64, Dim, bool)> {
    let (n_, j_, w_, c_, v_, pa_) = dims();
    let (l, m, t, i, th, n, j, one) = (LENGTH, MASS, TIME, CURRENT, TEMPERATURE, AMOUNT, LUMINOSITY, DIMLESS);
    let ly = C_LIGHT * 365.25 * 86400.0;
    let pc = AU * 648000.0 / PI;
    let tesla = v_ * t / l.powi(2);
    vec![
        ("m", 1.0, l, true),
        ("g", 1e-3, m, true),
        ("s", 1.0, t, true),
        ("A", 1.0, i, true),
        ("K", 1.0, th, true),
        ("mol", 1.0, n, true),
        ("cd", 1.0, j, true),
        ("rad", 1.0, one, true),
        ("sr", 1.0, one, false),
        ("°", PI / 180.0, one, false),
        ("deg", PI / 180.0, one, false),
        ("arcmin", PI / 10800.0, one, false),
        ("arcsec", PI / 648000.0, one, true),
        ("mas", PI / 648000e3, one, false),
        ("\u{3bc}as", PI / 648000e6, one, false),
        ("uas", PI / 648000e6, one, false),
        ("rev", 2.0 * PI, one, false),
        ("rpm", 2.0 * PI / 60.0, one / t, false),
        ("percent", 0.01, one, false),
        ("%", 0.01, one, false),
        ("Hz", 1.0, one / t, true),
        ("N", 1.0, n_, true),
        ("Pa", 1.0, pa_, true),
        ("J", 1.0, j_, true),
        ("W", 1.0, w_, true),
        ("C", 1.0, c_, true),
        ("V", 1.0, v_, true),
        ("F", 1.0, c_ / v_, true),
        ("Ω", 1.0, v_ / i, true),
        ("ohm", 1.0, v_ / i, true),
        ("S", 1.0, i / v_, true),
        ("Wb", 1.0, v_ * t, true),
        ("T", 1.0, tesla, true),
        ("H", 1.0, v_ * t / i, true),
        ("lm", 1.0, j, true),
        ("lx", 1.0, j / l.powi(2), true),
        ("Bq", 1.0, one / t, true),
        ("Gy", 1.0, j_ / m, true),
        ("Sv", 1.0, j_ / m, true),
        ("kat", 1.0, n / t, true),
        ("min", 60.0, t, false),
        ("hr", 3600.0, t, false),
        ("hour", 3600.0, t, false),
        ("day", 86400.0, t, false),
        ("yr", 365.25 * 86400.0, t, true),
        ("year", 365.25 * 86400.0, t, false),
        ("Å", 1e-10, l, false),
        ("angstrom", 1e-10, l, false),
        ("au", AU, l, false),
        ("AU", AU, l, false),
        ("ly", ly, l, true),
        ("pc", pc, l, true),
        ("inch", 0.0254, l, false),
        ("in_", 0.0254, l, false),
        ("ft", 0.3048, l, false),
        ("yd", 0.9144, l, false),
        ("mi", 1609.344, l, false),
        ("mile", 1609.344, l, false),
        ("R☉", 6.957e8, l, false),
        ("Rsun", 6.957e8, l, false),
        ("R_E", 6.3781e6, l, false),
        ("Rearth", 6.3781e6, l, false),
        ("b", 1e-28, l.powi(2), true),
        ("barn", 1e-28, l.powi(2), false),
        ("ha", 1e4, l.powi(2), false),
        ("L", 1e-3, l.powi(3), true),
        ("l", 1e-3, l.powi(3), false),
        ("u", U_MASS, m, false),
        ("amu", U_MASS, m, false),
        ("Da", U_MASS, m, true),
        ("tonne", 1000.0, m, false),
        ("lb", 0.45359237, m, false),
        ("M☉", 1.98841e30, m, false),
        ("Msun", 1.98841e30, m, false),
        ("M_E", 5.9722e24, m, false),
        ("Mearth", 5.9722e24, m, false),
        ("eV", E_CHARGE, j_, true),
        ("erg", 1e-7, j_, false),
        ("cal", 4.184, j_, true),
        ("Wh", 3600.0, j_, true),
        ("L☉", 3.828e26, w_, false),
        ("Lsun", 3.828e26, w_, false),
        ("hp", 745.69987158227022, w_, false),
        ("dyn", 1e-5, n_, false),
        ("lbf", 4.4482216152605, n_, false),
        ("bar", 1e5, pa_, true),
        ("atm", 101325.0, pa_, false),
        ("Torr", 101325.0 / 760.0, pa_, false),
        ("mmHg", 133.322387415, pa_, false),
        ("psi", 6894.757293168361, pa_, false),
        ("gauss", 1e-4, tesla, false),
        ("Gs", 1e-4, tesla, false),
        ("c", C_LIGHT, l / t, false),
        ("kph", 1000.0 / 3600.0, l / t, false),
        ("mph", 0.44704, l / t, false),
        ("Ci", 3.7e10, one / t, true),
    ]
}

pub(crate) fn units() -> &'static HashMap<&'static str, Entry> {
    static U: OnceLock<HashMap<&'static str, Entry>> = OnceLock::new();
    U.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, factor, dim, prefixable) in unit_list() {
            map.insert(name, Entry { factor, dim, prefixable });
        }
        for &(name, factor) in SELF_HOSTED {
            if let Some(e) = map.get_mut(name) {
                e.factor = factor;
            }
        }
        map
    })
}

/// Affine temperature units: SI value = x * factor + offset.
pub const AFFINE: [(&str, f64, f64); 4] = [
    ("°C", 1.0, 273.15),
    ("degC", 1.0, 273.15),
    ("°F", 5.0 / 9.0, 273.15 - 160.0 / 9.0),
    ("degF", 5.0 / 9.0, 273.15 - 160.0 / 9.0),
];

pub(crate) fn affine(name: &str) -> Option<(f64, f64)> {
    AFFINE.iter().find(|a| a.0 == name).map(|a| (a.1, a.2))
}

/// SI prefixes, in the order of `PREFIXES` in fermium/units.py.
pub const PREFIXES: [(&str, f64); 25] = [
    ("Q", 1e30),
    ("R", 1e27),
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("da", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("\u{3bc}", 1e-6),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
    ("r", 1e-27),
    ("q", 1e-30),
];

/// The prefixes in the order `lookup_unit` tries them: `sorted(PREFIXES, key=len, reverse=True)`.
fn prefix_order() -> &'static [(&'static str, f64)] {
    static P: OnceLock<Vec<(&'static str, f64)>> = OnceLock::new();
    P.get_or_init(|| {
        let mut v = PREFIXES.to_vec();
        v.sort_by_key(|p| std::cmp::Reverse(p.0.chars().count()));
        v
    })
}

const PREFIX_2022: [&str; 4] = ["Q", "R", "r", "q"];
const ALLOWED_2022: [&str; 4] = ["Rg", "Qg", "qg", "Qm"];
const BLOCKED: [&str; 11] = ["ft", "mi", "Pa", "cd", "min", "pc", "ha", "nmi", "Gs", "ms_", "dam"];

/// ASCII -> pretty spellings of unit names (used by `fermium fmt`).
pub const UNIT_PRETTY: [(&str, &str); 8] = [
    ("deg", "°"),
    ("degC", "°C"),
    ("degF", "°F"),
    ("angstrom", "Å"),
    ("ohm", "Ω"),
    ("Msun", "M☉"),
    ("Rsun", "R☉"),
    ("Lsun", "L☉"),
];

/// The pretty spelling of an ASCII unit name, if it has one.
pub fn unit_pretty(name: &str) -> Option<&'static str> {
    UNIT_PRETTY.iter().find(|p| p.0 == name).map(|p| p.1)
}

/// The ASCII spelling of a pretty unit name, if it has one.
pub fn unit_ascii(name: &str) -> Option<&'static str> {
    UNIT_PRETTY.iter().find(|p| p.1 == name).map(|p| p.0)
}

/// Symbols whose unit isn't obvious from its dimension (D211).
pub const UNIT_NAMES_LONG: [(&str, &str); 5] =
    [("u", "atomic mass unit"), ("b", "barn"), ("l", "litre"), ("L", "litre"), ("Da", "dalton")];

/// Unit names written out in words -> the symbol Fermium uses (for suggestions in error messages).
pub const SPELLED_UNITS: &[(&str, &str)] = &[
    ("meter", "m"), ("meters", "m"), ("metre", "m"), ("metres", "m"), ("second", "s"), ("seconds", "s"),
    ("sec", "s"), ("secs", "s"), ("kilogram", "kg"), ("kilograms", "kg"), ("gram", "g"), ("grams", "g"),
    ("kilometer", "km"), ("kilometers", "km"), ("kilometre", "km"), ("kilometres", "km"),
    ("centimeter", "cm"), ("centimeters", "cm"), ("centimetre", "cm"), ("centimetres", "cm"),
    ("millimeter", "mm"), ("millimeters", "mm"), ("foot", "ft"), ("feet", "ft"), ("inches", "inch"),
    ("mile", "mi"), ("miles", "mi"), ("hour", "hr"), ("hours", "hr"), ("minute", "min"), ("minutes", "min"),
    ("day", "day"), ("days", "day"), ("year", "yr"), ("years", "yr"), ("newton", "N"), ("newtons", "N"),
    ("joule", "J"), ("joules", "J"), ("watt", "W"), ("watts", "W"), ("volt", "V"), ("volts", "V"),
    ("amp", "A"), ("amps", "A"), ("ampere", "A"), ("amperes", "A"), ("ohm", "Ω"), ("ohms", "Ω"),
    ("kelvin", "K"), ("kelvins", "K"), ("celsius", "°C"), ("degC", "°C"), ("fahrenheit", "°F"),
    ("pascal", "Pa"), ("pascals", "Pa"), ("coulomb", "C"), ("coulombs", "C"), ("tesla", "T"),
    ("hertz", "Hz"), ("liter", "L"), ("liters", "L"), ("litre", "L"), ("litres", "L"), ("degree", "°"),
    ("degrees", "°"), ("deg", "°"), ("radian", "rad"), ("radians", "rad"), ("electronvolt", "eV"),
    ("electronvolts", "eV"), ("mph", "mi/hr"), ("kph", "km/hr"), ("lb", "lbf or lbm"),
    ("lbs", "lbf or lbm"), ("pound", "lbf or lbm"), ("pounds", "lbf or lbm"),
];

/// The symbol for a unit written out in words ("meters" -> "m"), for error messages.
pub fn spelled_unit(word: &str) -> Option<&'static str> {
    SPELLED_UNITS.iter().find(|p| p.0 == word).map(|p| p.1)
}

/// The long name of a unit symbol whose unit isn't obvious ("u" -> "atomic mass unit").
pub fn unit_name_long(sym: &str) -> Option<&'static str> {
    UNIT_NAMES_LONG.iter().find(|p| p.0 == sym).map(|p| p.1)
}

/// A Unit for a single unit name (possibly prefixed: km, μs, MeV), or None.
pub fn lookup_unit(name: &str) -> Option<Unit> {
    if let Some((f, off)) = affine(name) {
        return Some(Unit { name: name.to_string(), dim: TEMPERATURE, factor: f, offset: off });
    }
    let db = units();
    if let Some(e) = db.get(name) {
        return Some(Unit::new(name, e.dim, e.factor));
    }
    let owned;
    let name = if let Some(rest) = name.strip_prefix(MICRO_SIGN) {
        owned = format!("{MU}{rest}");
        owned.as_str()
    } else {
        name
    };
    for &(p, pf) in prefix_order() {
        if let Some(rest) = name.strip_prefix(p) {
            if rest.is_empty() {
                continue;
            }
            if PREFIX_2022.contains(&p) && !ALLOWED_2022.contains(&name) {
                continue;
            }
            if let Some(e) = db.get(rest) {
                if e.prefixable && !BLOCKED.contains(&name) {
                    return Some(Unit::new(name, e.dim, e.factor * pf));
                }
            }
        }
    }
    None
}

/// Is `name` a unit (possibly prefixed)?
pub fn is_unit_name(name: &str) -> bool {
    lookup_unit(name).is_some()
}

/// Every unprefixed unit name in the database (including the affine ones), sorted.
pub fn unit_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = units().keys().copied().collect();
    v.extend(AFFINE.iter().map(|a| a.0));
    v.sort();
    v
}

/// Does an unprefixed unit of the database take SI prefixes? (`None` for a name that isn't one; the
/// language specification's unit catalogue, docs/spec/units.md §3.1, is generated from this.)
pub fn unit_prefixable(name: &str) -> Option<bool> {
    units().get(name).map(|e| e.prefixable)
}

/// The self-hosted factor table generated from units_db.fm at build time (name, SI factor).
pub fn self_hosted_factors() -> &'static [(&'static str, f64)] {
    SELF_HOSTED
}
