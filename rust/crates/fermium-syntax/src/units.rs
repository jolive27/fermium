//! The unit names the parser needs from `fermium/units.py` (`lookup_unit`, `is_unit_name`) and the names of the
//! built-in constants (`fermium/constants.py`), from the generated tables.

use crate::tables::{AFFINE, ALLOWED_2022, BLOCKED, CONSTANT_NAMES, PREFIXES, PREFIX_2022, UNITS};

/// The prefixes longest first, in the order Python's `sorted(PREFIXES, key=len, reverse=True)` gives (stable).
pub fn prefixes_longest_first() -> Vec<&'static str> {
    let mut p: Vec<&'static str> = PREFIXES.to_vec();
    p.sort_by_key(|x| std::cmp::Reverse(x.chars().count()));
    p
}

/// Is this name one of the unprefixed units (`_UNITS`)?
pub fn is_base_unit(name: &str) -> bool {
    UNITS.iter().any(|(n, _, _)| *n == name)
}

pub fn is_affine(name: &str) -> bool {
    AFFINE.iter().any(|(n, _)| *n == name)
}

/// `lookup_unit(name)`: what the unit measures (`dim_name(u.dim).split(" [")[0]`), or None if it isn't a unit.
pub fn lookup_unit(name: &str) -> Option<&'static str> {
    if let Some((_, what)) = AFFINE.iter().find(|(n, _)| *n == name) {
        return Some(what);
    }
    if let Some((_, _, what)) = UNITS.iter().find(|(n, _, _)| *n == name) {
        return Some(what);
    }
    let owned;
    let name = if let Some(rest) = name.strip_prefix('µ') {
        owned = format!("μ{rest}");
        owned.as_str()
    } else {
        name
    };
    for p in prefixes_longest_first() {
        if name.starts_with(p) && name.chars().count() > p.chars().count() {
            let rest = &name[p.len()..];
            if PREFIX_2022.contains(&p) && !ALLOWED_2022.contains(&name) {
                continue;
            }
            if let Some((_, pref, what)) = UNITS.iter().find(|(n, _, _)| *n == rest) {
                if *pref && !BLOCKED.contains(&name) {
                    return Some(what);
                }
            }
        }
    }
    None
}

pub fn is_unit_name(name: &str) -> bool {
    lookup_unit(name).is_some()
}

/// Is this a built-in constant's name (c, AU, M☉ are units too, with the same value)?
pub fn is_constant(name: &str) -> bool {
    CONSTANT_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        for u in ["m", "km", "kg", "μm", "MeV", "°C", "degC", "Rg", "Qm", "M☉", "%", "hr"] {
            assert!(is_unit_name(u), "{u}");
        }
        for u in ["rg", "Rm", "kmin", "x", "dam", "Pa2", "rs"] {
            assert!(!is_unit_name(u), "{u}");
        }
        assert_eq!(lookup_unit("km"), Some("length"));
        assert_eq!(lookup_unit("keV"), Some("energy"));
    }
}
