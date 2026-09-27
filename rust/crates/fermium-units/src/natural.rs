//! Unit systems: SI (the default), natural units (`units natural(ħ = c = 1)`, `units nuclear`) and the
//! `units astro` display preset (port of fermium/natural.py; DECISIONS D60).
//!
//! Setting a set S of constants to 1 identifies dimensions that differ by powers of them. Every SI
//! dimension D splits uniquely as D = Σ aᵢ dim(Cᵢ) + Σ βⱼ Bⱼ over kept base dimensions Bⱼ; the canonical
//! dimension is Σ βⱼ Bⱼ and a quantity's canonical value is its SI value times Π Cᵢ^(−aᵢ).

use crate::db::{lookup_unit, Unit};
use crate::dim::*;
use crate::display::dim_name;
use crate::parse::parse_unit_string;
use num_rational::Rational64;
use num_traits::{One, Zero};
use std::collections::HashMap;
use std::sync::OnceLock;

type R = Rational64;

const HBAR: f64 = 6.62607015e-34 / (2.0 * std::f64::consts::PI);
const C: f64 = 299792458.0;
const KB: f64 = 1.380649e-23;
const G: f64 = 6.67430e-11;
const EPS0: f64 = 8.8541878188e-12;

/// The dimension of energy (J).
pub fn energy() -> Dim {
    MASS * LENGTH.powi(2) / TIME.powi(2)
}

/// The constants a natural system may set to 1: (name, SI value, dimension), in Fermium's order.
pub fn settable() -> [(&'static str, f64, Dim); 5] {
    let e = energy();
    [
        ("ħ", HBAR, e * TIME),
        ("c", C, LENGTH / TIME),
        ("k_B", KB, e / TEMPERATURE),
        ("G", G, LENGTH.powi(3) / (MASS * TIME.powi(2))),
        ("ε_0", EPS0, (CURRENT * TIME).powi(2) / (e * LENGTH)),
    ]
}

fn settable_get(name: &str) -> Option<(f64, Dim)> {
    settable().into_iter().find(|s| s.0 == name).map(|s| (s.1, s.2))
}

/// ASCII and other spellings of the settable constants.
pub const ALIASES: [(&str, &str); 6] =
    [("hbar", "ħ"), ("kB", "k_B"), ("eps0", "ε_0"), ("ε0", "ε_0"), ("epsilon_0", "ε_0"), ("c_0", "c")];

const ORDER: [&str; 5] = ["ħ", "c", "k_B", "G", "ε_0"];

/// The canonical name of a constant (`hbar` -> `ħ`).
pub fn canonical_const_name(name: &str) -> &str {
    ALIASES.iter().find(|a| a.0 == name).map(|a| a.1).unwrap_or(name)
}

fn alias_of(name: &str) -> Option<&'static str> {
    ALIASES.iter().find(|a| a.0 == name).map(|a| a.1)
}

/// Solve Σ xₖ colsₖ = target exactly (cols: 7 Dims forming a basis).
fn solve(cols: &[Dim], target: &Dim) -> Vec<R> {
    let n = 7;
    let mut a: Vec<Vec<R>> = (0..n)
        .map(|r| {
            let mut row: Vec<R> = (0..n).map(|k| cols[k].0[r]).collect();
            row.push(target.0[r]);
            row
        })
        .collect();
    for c in 0..n {
        let p = (c..n).find(|&r| !a[r][c].is_zero()).expect("basis");
        a.swap(c, p);
        let piv = a[c][c];
        // checked (red team 13): a huge exponent in the target is recorded, not wrapped
        use crate::exact::{div_or_record, mul_or_record, sub_or_record};
        a[c] = a[c].iter().map(|x| div_or_record(*x, piv, None)).collect();
        for r in 0..n {
            if r != c && !a[r][c].is_zero() {
                let f = a[r][c];
                let rc = a[c].clone();
                a[r] = a[r].iter().zip(rc.iter()).map(|(x, y)| sub_or_record(*x, mul_or_record(f, *y, None), None)).collect();
            }
        }
    }
    (0..n).map(|r| a[r][n]).collect()
}

fn rank(dims: &[Dim]) -> usize {
    let mut rows: Vec<Vec<R>> = dims.iter().map(|d| d.0.to_vec()).collect();
    let (mut rank, mut col) = (0, 0);
    while rank < rows.len() && col < 7 {
        let p = (rank..rows.len()).find(|&r| !rows[r][col].is_zero());
        let Some(p) = p else {
            col += 1;
            continue;
        };
        rows.swap(rank, p);
        for r in 0..rows.len() {
            if r != rank && !rows[r][col].is_zero() {
                let f = rows[r][col] / rows[rank][col];
                let rr = rows[rank].clone();
                rows[r] = rows[r].iter().zip(rr.iter()).map(|(x, y)| x - f * y).collect();
            }
        }
        rank += 1;
        col += 1;
    }
    rank
}

fn r_to_f64(p: R) -> f64 {
    *p.numer() as f64 / *p.denom() as f64
}

/// SI, a natural system (some constants set to 1), or a display preset (astro).
#[derive(Clone, Debug, PartialEq)]
pub struct UnitSystem {
    pub name: String,
    /// The constants set to 1, in Fermium's order (ħ, c, k_B, G, ε_0).
    pub consts: Vec<&'static str>,
    /// "SI", "natural", "nuclear" or "astro": how values print.
    pub display: String,
    pub natural: bool,
    /// The kept base dimensions Bⱼ.
    pub kept: Vec<Dim>,
    cols: Vec<Dim>,
}

impl UnitSystem {
    /// A system setting `consts` to 1 (errors if they are not independent).
    pub fn new(name: &str, consts: &[&str], display: Option<&str>) -> Result<UnitSystem, String> {
        let cs: Vec<&'static str> = ORDER.iter().copied().filter(|c| consts.contains(c)).collect();
        let mut sys = UnitSystem {
            name: name.to_string(),
            natural: !cs.is_empty(),
            consts: cs,
            display: display.unwrap_or(name).to_string(),
            kept: vec![],
            cols: vec![],
        };
        if !sys.natural {
            return Ok(sys);
        }
        let vecs: Vec<Dim> = sys.consts.iter().map(|c| settable_get(c).unwrap().1).collect();
        if rank(&vecs) < vecs.len() {
            return Err("these constants can't all be 1 at once (they are not independent)".into());
        }
        let (l, m, t, i, th, n, j, e) = (LENGTH, MASS, TIME, CURRENT, TEMPERATURE, AMOUNT, LUMINOSITY, energy());
        let cand = if sys.consts.contains(&"ħ") { vec![e, i, th, n, j, l, m, t] } else { vec![l, i, th, n, j, m, t, e] };
        let mut kept: Vec<Dim> = vec![];
        for d in cand {
            if vecs.len() + kept.len() == 7 {
                break;
            }
            let mut all = vecs.clone();
            all.extend(kept.iter().copied());
            all.push(d);
            if rank(&all) == vecs.len() + kept.len() + 1 {
                kept.push(d);
            }
        }
        sys.cols = vecs;
        sys.cols.extend(kept.iter().copied());
        sys.kept = kept;
        Ok(sys)
    }

    /// Plain SI.
    pub fn si() -> UnitSystem {
        UnitSystem::new("SI", &[], None).unwrap()
    }

    /// "units SI", "units natural (ħ = c = 1)".
    pub fn label(&self) -> String {
        if !self.natural {
            return format!("units {}", self.name);
        }
        format!("units {} ({} = 1)", self.name, self.consts.join(" = "))
    }

    /// The split D = Σ aᵢ Cᵢ + Σ βⱼ Bⱼ: (a, β).
    pub fn split(&self, d: &Dim) -> (Vec<R>, Vec<R>) {
        let x = solve(&self.cols, d);
        let k = self.consts.len();
        (x[..k].to_vec(), x[k..].to_vec())
    }

    /// The canonical dimension of an SI dimension (itself outside natural units).
    pub fn canon_dim(&self, d: &Dim) -> Dim {
        if !self.natural {
            return *d;
        }
        let (_, beta) = self.split(d);
        let mut out = DIMLESS;
        for (b, bd) in beta.iter().zip(&self.kept) {
            if !b.is_zero() {
                out = out * bd.pow(*b);
            }
        }
        out
    }

    /// Multiply an SI value of dimension d by this to get its canonical value.
    pub fn factor(&self, d: &Dim) -> f64 {
        if !self.natural {
            return 1.0;
        }
        let (a, _) = self.split(d);
        let mut f = 1.0;
        for (ai, c) in a.iter().zip(&self.consts) {
            if !ai.is_zero() {
                let v = settable_get(c).unwrap().0;
                f *= v.powf(r_to_f64(-*ai));
            }
        }
        f
    }

    /// Is d unchanged by this system (no powers of the constants)?
    pub fn invariant(&self, d: &Dim) -> bool {
        !self.natural || self.split(d).0.iter().all(|x| x.is_zero())
    }

    /// A unit mapped into this system (canonical dimension, factor including Π Cᵢ^(−aᵢ)).
    pub fn canon_unit(&self, u: &Unit) -> Unit {
        if !self.natural || self.invariant(&u.dim) {
            return u.clone();
        }
        let f = self.factor(&u.dim);
        Unit { name: u.name.clone(), dim: self.canon_dim(&u.dim), factor: u.factor * f, offset: u.offset * f }
    }

    /// A constant's value in this system (exactly 1 for the constants set to 1).
    pub fn const_value(&self, name: &str, value: f64, dim: &Dim) -> f64 {
        if self.consts.contains(&name) || alias_of(name).is_some_and(|a| self.consts.contains(&a)) {
            return 1.0;
        }
        value * self.factor(dim)
    }

    fn base_display(&self, bd: &Dim, sign: R) -> (Unit, i64) {
        if *bd == energy() {
            if self.display == "nuclear" && sign < R::zero() {
                return (self.canon_unit(&lookup_unit("fm").unwrap()), -1);
            }
            return (lookup_unit("MeV").unwrap(), 1);
        }
        if *bd == LENGTH {
            return (lookup_unit("m").unwrap(), 1);
        }
        for (d, s) in [(MASS, "kg"), (TIME, "s"), (CURRENT, "A"), (TEMPERATURE, "K"), (AMOUNT, "mol"), (LUMINOSITY, "cd")] {
            if *bd == d {
                return (lookup_unit(s).unwrap(), 1);
            }
        }
        unreachable!("kept dimension without a display unit")
    }

    /// The unit a value of canonical dimension d prints in when it carries no unit of its own
    /// (None in SI).
    pub fn display_unit(&self, d: &Dim) -> Option<Unit> {
        if self.display == "astro" {
            return astro_display(d);
        }
        if !self.natural {
            return None;
        }
        let (_, beta) = self.split(d);
        let (mut num, mut den, mut factor) = (Vec::new(), Vec::new(), 1.0f64);
        for (b, bd) in beta.iter().zip(&self.kept) {
            if b.is_zero() {
                continue;
            }
            let (u, s) = self.base_display(bd, *b);
            let p = *b * R::from_integer(s);
            factor *= u.factor.powf(r_to_f64(p));
            let item = format!("{}{}", u.name, fmt_exp(if p < R::zero() { -p } else { p }, true));
            if p > R::zero() { num.push(item) } else { den.push(item) }
        }
        let name = if !num.is_empty() || den.len() != 1 {
            join_units(&num, &den)
        } else {
            let d0: Vec<char> = den[0].chars().collect();
            let last = *d0.last().unwrap();
            if "²³⁴⁵⁶⁷⁸⁹".contains(last) {
                format!("{}⁻{}", d0[..d0.len() - 1].iter().collect::<String>(), last)
            } else {
                format!("{}⁻¹", den[0])
            }
        };
        Some(Unit::new(if name.is_empty() { "1".to_string() } else { name }, *d, factor))
    }

    /// A dimension in words, for error messages inside a natural region.
    pub fn describe(&self, d: &Dim) -> String {
        if !self.natural {
            return dim_name(d);
        }
        if d.is_dimensionless() {
            return "a plain number (no units)".into();
        }
        let uname = self.display_unit(d).unwrap().name;
        if self.consts.contains(&"ħ") {
            let (_, beta) = self.split(d);
            let n = beta[0];
            if beta[1..].iter().all(|b| b.is_zero()) {
                let words = if n == R::one() {
                    Some("energy or mass")
                } else if n == -R::one() {
                    Some("length or time (1/energy)")
                } else if n == R::from_integer(2) {
                    Some("energy²")
                } else if n == R::from_integer(-2) {
                    Some("area (1/energy²)")
                } else if n == R::from_integer(-3) {
                    Some("volume (1/energy³)")
                } else {
                    None
                };
                return match words {
                    Some(w) => format!("{w} [{uname}]"),
                    None => format!("energy^{n} [{uname}]"),
                };
            }
        }
        format!("a quantity with units [{uname}]")
    }
}

/// `units natural(ħ = c = 1)`, `units nuclear`, `units astro`, `units SI` (`consts` as written, canonical
/// names; None or empty when none are given).
pub fn make_system(name: &str, consts: Option<&[&str]>) -> Result<UnitSystem, String> {
    let consts = consts.filter(|c| !c.is_empty());
    match name {
        "SI" => Ok(UnitSystem::si()),
        "astro" => {
            if consts.is_some() {
                return Err("units astro sets no constants to 1 (it only chooses M☉, AU and yr for printing); \
                            for G = c = 1 write  units natural(G = c = 1)"
                    .into());
            }
            UnitSystem::new("astro", &[], Some("astro"))
        }
        "nuclear" => {
            if let Some(cs) = consts {
                let mut set: Vec<&str> = cs.to_vec();
                set.sort();
                set.dedup();
                let mut want = vec!["c", "ħ"];
                want.sort();
                if set != want {
                    return Err("units nuclear always means ħ = c = 1 (with MeV and fm); for other constants \
                                write  units natural(...)"
                        .into());
                }
            }
            UnitSystem::new("nuclear", &["ħ", "c"], Some("nuclear"))
        }
        "natural" => {
            let cs: Vec<&str> = consts.map(|c| c.to_vec()).unwrap_or_else(|| vec!["ħ", "c"]);
            for c in &cs {
                if settable_get(c).is_none() {
                    return Err(format!("{c} can't be set to 1 in natural units (Fermium knows ħ, c, k_B, G and ε_0)"));
                }
            }
            let mut uniq = cs.clone();
            uniq.sort();
            uniq.dedup();
            if uniq.len() != cs.len() {
                return Err("a constant is listed twice".into());
            }
            UnitSystem::new("natural", &cs, Some("natural"))
        }
        _ => Err(format!("unknown unit system '{name}' (Fermium knows natural, nuclear, astro and SI)")),
    }
}

fn astro_named() -> &'static HashMap<Dim, Unit> {
    static A: OnceLock<HashMap<Dim, Unit>> = OnceLock::new();
    A.get_or_init(|| {
        let mut m = HashMap::new();
        for spec in ["M☉", "AU", "yr", "L☉", "km/s", "M☉/yr", "AU³/yr²", "AU³/(M☉ yr²)"] {
            let u = parse_unit_string(spec).unwrap();
            m.entry(u.dim).or_insert_with(|| Unit::new(spec, u.dim, u.factor));
        }
        m
    })
}

/// Astronomy display units: M☉, AU, yr (and L☉, km/s); None (SI's choice) for anything else.
pub fn astro_display(d: &Dim) -> Option<Unit> {
    if let Some(u) = astro_named().get(d) {
        return Some(u.clone());
    }
    let e = &d.0;
    let n_mlt = e[..3].iter().filter(|x| !x.is_zero()).count();
    if e[3..].iter().all(|x| x.is_zero()) && n_mlt > 0 && n_mlt <= 2 {
        let parts = [(e[1], "M☉", 1.98841e30), (e[0], "AU", 149597870700.0), (e[2], "yr", 365.25 * 86400.0)];
        let num: Vec<String> =
            parts.iter().filter(|p| p.0 > R::zero()).map(|p| format!("{}{}", p.1, fmt_exp(p.0, true))).collect();
        let den: Vec<String> =
            parts.iter().filter(|p| p.0 < R::zero()).map(|p| format!("{}{}", p.1, fmt_exp(-p.0, true))).collect();
        let mut f = 1.0f64;
        for (p, _, v) in parts {
            if !p.is_zero() {
                f *= f64::powf(v, r_to_f64(p));
            }
        }
        return Some(Unit::new(join_units(&num, &den), *d, f));
    }
    None
}
