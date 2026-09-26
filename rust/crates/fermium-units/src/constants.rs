//! The constants library (port of fermium/constants.py): CODATA 2022 values, IAU nominal values.

use crate::db::Unit;
use crate::parse::parse_unit_string;
use std::f64::consts::PI;
use std::sync::OnceLock;

const H: f64 = 6.62607015e-34;
const C: f64 = 299792458.0;
const K: f64 = 1.380649e-23;

/// The root of x = 5 (1 - exp(-x)) (Wien's displacement law), by Newton's method to full precision.
fn wien_x() -> f64 {
    let mut x = 5.0f64;
    for _ in 0..50 {
        let g = x - 5.0 * (1.0 - (-x).exp());
        let x_new = x - g / (1.0 - 5.0 * (-x).exp());
        if x_new == x {
            break;
        }
        x = x_new;
    }
    x
}

/// A constant of the library.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstantDef {
    /// Canonical name ("ħ", "k_B").
    pub name: &'static str,
    /// Value in SI units.
    pub value: f64,
    /// Unit string ("J s", "1").
    pub unit: &'static str,
    pub description: &'static str,
    /// Other names for it ("kB").
    pub aliases: &'static [&'static str],
}

/// The constants in the order of `CONSTANTS` in fermium/constants.py.
pub fn constant_defs() -> &'static [ConstantDef] {
    static T: OnceLock<Vec<ConstantDef>> = OnceLock::new();
    T.get_or_init(|| {
        let d = |name, value, unit, description, aliases| ConstantDef { name, value, unit, description, aliases };
        vec![
            d("c", 299792458.0, "m/s", "speed of light in vacuum (exact)", &["c_0"][..]),
            d("h", 6.62607015e-34, "J s", "Planck constant (exact)", &[]),
            d("ħ", 6.62607015e-34 / (2.0 * PI), "J s", "reduced Planck constant ħ = h/2π (exact)", &[]),
            d("e", 1.602176634e-19, "C", "elementary charge (exact)", &[]),
            d("k_B", 1.380649e-23, "J/K", "Boltzmann constant (exact)", &["kB"]),
            d("N_A", 6.02214076e23, "1/mol", "Avogadro constant (exact)", &["NA"]),
            d("R_gas", 6.02214076e23 * 1.380649e-23, "J/(mol K)", "molar gas constant N_A k_B (exact)", &[]),
            d("G", 6.67430e-11, "m³/(kg s²)", "Newtonian constant of gravitation", &[]),
            d("g_n", 9.80665, "m/s²", "standard acceleration of gravity (exact, by definition)", &["g_0"]),
            d("m_e", 9.1093837139e-31, "kg", "electron mass", &[]),
            d("m_p", 1.67262192595e-27, "kg", "proton mass", &[]),
            d("m_n", 1.67492750056e-27, "kg", "neutron mass", &[]),
            d("m_u", 1.66053906892e-27, "kg", "atomic mass constant (1 u)", &[]),
            d("m_α", 6.6446573450e-27, "kg", "alpha particle mass", &[]),
            d("m_d", 3.3435837768e-27, "kg", "deuteron mass", &[]),
            d("m_μ", 1.883531627e-28, "kg", "muon mass", &[]),
            d("ε_0", 8.8541878188e-12, "F/m", "vacuum electric permittivity", &["eps0"]),
            d("μ_0", 1.25663706127e-6, "N/A²", "vacuum magnetic permeability", &["mu0"]),
            d(
                "σ",
                2.0 * PI.powf(5.0) * K.powf(4.0) / (15.0 * H.powf(3.0) * C.powf(2.0)),
                "W/(m² K⁴)",
                "Stefan–Boltzmann constant 2π⁵k⁴/(15h³c²) (exact)",
                &["sigma_SB"],
            ),
            d("α", 7.2973525643e-3, "1", "fine-structure constant", &["alpha_fs"]),
            d("a_0", 5.29177210544e-11, "m", "Bohr radius", &[]),
            d("R_∞", 10973731.568157, "1/m", "Rydberg constant", &["R_inf"]),
            d(
                "b_W",
                H * C / (K * wien_x()),
                "m K",
                "Wien wavelength displacement constant h c/(k x), x = 4.965… (exact)",
                &[],
            ),
            d("r_e", 2.8179403205e-15, "m", "classical electron radius", &[]),
            d("μ_B", 9.2740100657e-24, "J/T", "Bohr magneton", &["mu_B"]),
            d("μ_N", 5.0507837393e-27, "J/T", "nuclear magneton", &["mu_N"]),
            d("k_e", 1.0 / (4.0 * PI * 8.8541878188e-12), "N m²/C²", "Coulomb constant 1/(4π ε₀)", &[]),
            d("M_sun", 1.3271244e20 / 6.67430e-11, "kg", "solar mass (IAU 2015 nominal GM☉/G)", &["M☉"]),
            d("R_sun", 6.957e8, "m", "nominal solar radius (IAU 2015)", &["R☉"]),
            d("L_sun", 3.828e26, "W", "nominal solar luminosity (IAU 2015)", &["L☉"]),
            d("M_earth", 5.9722e24, "kg", "Earth mass", &[]),
            d("GM_sun", 1.3271244e20, "m³/s²", "solar mass parameter GM☉ (IAU 2015 nominal, exact)", &["GM☉"]),
            d("GM_earth", 3.986004418e14, "m³/s²", "geocentric gravitational constant GM⊕ (IERS 2010)", &[]),
            d("R_earth", 6.3781e6, "m", "nominal Earth equatorial radius (IAU 2015)", &[]),
            d("AU", 149597870700.0, "m", "astronomical unit (exact, IAU 2012)", &[]),
            d("π", PI, "1", "pi", &[]),
            d("∞", f64::INFINITY, "1", "infinity", &[]),
        ]
    })
}

/// A constant ready for use: its value in SI, its unit (parsed) and description.
#[derive(Clone, Debug, PartialEq)]
pub struct Constant {
    pub name: String,
    pub value: f64,
    pub unit: Unit,
    pub description: &'static str,
}

/// `all_constants()`: every constant under its canonical name and each alias, in Python's order
/// (a later entry with the same name replaces an earlier one, as in the Python dict).
pub fn constants() -> &'static [Constant] {
    static T: OnceLock<Vec<Constant>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out: Vec<Constant> = Vec::new();
        let mut put = |name: &str, c: &ConstantDef, u: &Unit| {
            let item = Constant { name: name.to_string(), value: c.value, unit: u.clone(), description: c.description };
            if let Some(slot) = out.iter_mut().find(|x| x.name == name) {
                *slot = item;
            } else {
                out.push(item);
            }
        };
        for c in constant_defs() {
            let u = parse_unit_string(c.unit).expect("constant unit");
            put(c.name, c, &u);
            for a in c.aliases {
                put(a, c, &u);
            }
        }
        out
    })
}

/// A constant by name or alias.
pub fn constant(name: &str) -> Option<&'static Constant> {
    constants().iter().find(|c| c.name == name)
}
