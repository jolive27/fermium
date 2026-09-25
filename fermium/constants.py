"""Physical constants with units.

Source: CODATA 2022 recommended values (NIST, https://physics.nist.gov/cuu/Constants/,
published May 2024), which are also the values in scipy.constants >= 1.15.
Values marked "exact" are fixed by the 2019 SI redefinition.
Astronomical values: IAU 2015 Resolution B3 nominal solar values, IAU 2012 B2 (au).

Each entry: canonical name -> (value in SI, unit string, description, aliases)
"""
from __future__ import annotations

import math

from .units import parse_unit_string

CONSTANTS = {
    "c": (299792458.0, "m/s", "speed of light in vacuum (exact)", ["c_0"]),
    "h": (6.62607015e-34, "J s", "Planck constant (exact)", []),
    "ħ": (6.62607015e-34 / (2 * math.pi), "J s", "reduced Planck constant ħ = h/2π (exact)", []),
    "e": (1.602176634e-19, "C", "elementary charge (exact)", []),
    "k_B": (1.380649e-23, "J/K", "Boltzmann constant (exact)", ["kB"]),
    "N_A": (6.02214076e23, "1/mol", "Avogadro constant (exact)", ["NA"]),
    "R_gas": (6.02214076e23 * 1.380649e-23, "J/(mol K)", "molar gas constant N_A k_B (exact)", []),
    "G": (6.67430e-11, "m³/(kg s²)", "Newtonian constant of gravitation", []),
    "g_n": (9.80665, "m/s²", "standard acceleration of gravity (exact, by definition)", ["g_0"]),
    "m_e": (9.1093837139e-31, "kg", "electron mass", []),
    "m_p": (1.67262192595e-27, "kg", "proton mass", []),
    "m_n": (1.67492750056e-27, "kg", "neutron mass", []),
    "m_u": (1.66053906892e-27, "kg", "atomic mass constant (1 u)", []),
    "m_α": (6.6446573450e-27, "kg", "alpha particle mass", []),
    "m_d": (3.3435837768e-27, "kg", "deuteron mass", []),
    "m_μ": (1.883531627e-28, "kg", "muon mass", []),
    "ε_0": (8.8541878188e-12, "F/m", "vacuum electric permittivity", ["eps0"]),
    "μ_0": (1.25663706127e-6, "N/A²", "vacuum magnetic permeability", ["mu0"]),
    "σ": (5.670374419e-8, "W/(m² K⁴)", "Stefan–Boltzmann constant (exact)", ["sigma_SB"]),
    "α": (7.2973525643e-3, "1", "fine-structure constant", ["alpha_fs"]),
    "a_0": (5.29177210544e-11, "m", "Bohr radius", []),
    "R_∞": (10973731.568157, "1/m", "Rydberg constant", ["R_inf"]),
    "b_W": (2.897771955e-3, "m K", "Wien wavelength displacement constant", []),
    "r_e": (2.8179403205e-15, "m", "classical electron radius", []),
    "μ_B": (9.2740100657e-24, "J/T", "Bohr magneton", ["mu_B"]),
    "μ_N": (5.0507837393e-27, "J/T", "nuclear magneton", ["mu_N"]),
    "k_e": (1 / (4 * math.pi * 8.8541878188e-12), "N m²/C²", "Coulomb constant 1/(4π ε₀)", []),
    "M_sun": (1.98841e30, "kg", "solar mass (IAU 2015 nominal GM/G)", ["M☉"]),
    "R_sun": (6.957e8, "m", "nominal solar radius (IAU 2015)", ["R☉"]),
    "L_sun": (3.828e26, "W", "nominal solar luminosity (IAU 2015)", ["L☉"]),
    "M_earth": (5.9722e24, "kg", "Earth mass", []),
    # GM is known far better than G or M separately (IAU 2015 B3 nominal solar value; IERS/WGS84 for Earth)
    "GM_sun": (1.3271244e20, "m³/s²", "solar mass parameter GM☉ (IAU 2015 nominal, exact)", ["GM☉"]),
    "GM_earth": (3.986004418e14, "m³/s²", "geocentric gravitational constant GM⊕ (IERS 2010)", []),
    "R_earth": (6.3781e6, "m", "nominal Earth equatorial radius (IAU 2015)", []),
    "AU": (149597870700.0, "m", "astronomical unit (exact, IAU 2012)", []),
    "π": (math.pi, "1", "pi", []),
    "∞": (math.inf, "1", "infinity", []),
}


def all_constants():
    """name -> (value, Unit, description) including aliases."""
    out = {}
    for name, (val, unit, desc, aliases) in CONSTANTS.items():
        u = parse_unit_string(unit)
        out[name] = (val, u, desc)
        for a in aliases:
            out[a] = (val, u, desc)
    return out
