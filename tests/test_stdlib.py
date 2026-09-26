"""The standard library modules (M7, D103): every function is checked against a closed form or SciPy,
every function is documented by a comment, and every function is listed in docs/stdlib.md."""
import math
import os
import re

import numpy as np
import pytest
import scipy.integrate
import scipy.special
import scipy.stats

from conftest import run, error_of
from numparse import num
from fermium.constants import CONSTANTS
from fermium.modules import stdlib_dir, stdlib_modules
from fermium.parser import parse
from fermium import ast as A
from fermium.lexer import canonical_name
from fermium.units import lookup_unit

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
K = {k: v[0] for k, v in CONSTANTS.items()}
c, h, hbar, e, G = K["c"], K["h"], K["ħ"], K["e"], K["G"]
m_e, m_p, m_u, m_alpha = K["m_e"], K["m_p"], K["m_u"], K["m_α"]
eps0, mu0, k_e, alpha = K["ε_0"], K["μ_0"], K["k_e"], K["α"]
MeV, eV = 1e6 * e, e
yr, day, pc = 365.25 * 86400, 86400.0, lookup_unit("pc").factor
Mpc = 1e6 * pc
M_sun, R_sun, sigma_SB, b_W, r_e = K["M_sun"], K["R_sun"], K["σ"], K["b_W"], K["r_e"]
g_n = K["g_n"]

EXPORTED = {}
for mod in stdlib_modules():
    with open(os.path.join(stdlib_dir(), mod + ".fm"), encoding="utf-8") as fh:
        EXPORTED[mod] = fh.read()


def functions(mod):
    return [s.name for s in parse(EXPORTED[mod]).body if isinstance(s, A.FuncDef) and not s.name.startswith("_")]


def values(mod, exprs, prelude=""):
    """Evaluate `mod.expr / (1 unit)` for each (expr, unit) as plain numbers, in one program."""
    lines = [f"import {mod}", prelude]
    for expr, unit in exprs:
        lines.append(f"print ({mod}.{expr}) / (1 {unit}) to 12 digits" if unit else
                     f"print {mod}.{expr} to 12 digits")
    out = run("\n".join(lines) + "\n")
    return [num(x) for x in out.split("\n") if x.strip()]


def close(cases, mod, prelude="", rtol=1e-9):
    got = values(mod, [(ex, u) for ex, u, _ in cases], prelude)
    for (ex, u, want), g in zip(cases, got):
        assert g == pytest.approx(want, rel=rtol, abs=1e-300), f"{mod}.{ex}: {g} vs {want}"
    return [ex.split("(")[0] for ex, _, _ in cases]


TESTED = {}


def _tested(mod, names):
    TESTED.setdefault(mod, set()).update(canonical_name(n) for n in names)


# ------------------------------------------------------------------ mechanics
def test_mechanics():
    v, th = 20.0, math.radians(35)
    th0 = math.radians(170)
    Ekin_rel = m_e * c**2 * (1 / math.sqrt(1 - 0.6**2) - 1)
    cases = [
        ("projectile_range(20 m/s, 35°, g_n)", "m", v**2 * math.sin(2 * th) / g_n),
        ("projectile_apex(20 m/s, 35°, g_n)", "m", (v * math.sin(th))**2 / (2 * g_n)),
        ("projectile_flight_time(20 m/s, 35°, g_n)", "s", 2 * v * math.sin(th) / g_n),
        ("pendulum_period(1.5 m, 1.62 m/s²)", "s", 2 * math.pi * math.sqrt(1.5 / 1.62)),
        ("pendulum_period_large(1 m, g_n, 170°)", "s",
         4 * math.sqrt(1 / g_n) * scipy.special.ellipk(math.sin(th0 / 2)**2)),
        ("spring_period(0.5 kg, 20 N/m)", "s", 2 * math.pi * math.sqrt(0.5 / 20)),
        ("kinetic_energy(2 kg, 3 m/s)", "J", 9.0),
        ("relativistic_kinetic_energy(m_e, 0.6 c)", "J", Ekin_rel),
        ("relativistic_kinetic_energy(1 kg, 1 m/s)", "J", 0.5 * (1 + 0.75 / c**2)),   # no cancellation
        ("potential_energy(2 kg, g_n, 10 m)", "J", 2 * g_n * 10),
        ("rocket_equation(3 km/s, 100 tonne, 20 tonne)", "m/s", 3000 * math.log(5)),
        ("rocket_mass_ratio(9 km/s, 3 km/s)", "", math.exp(3)),
    ]
    _tested("mechanics", close(cases, "mechanics"))


def test_large_amplitude_pendulum_reduces_to_small():
    got = values("mechanics", [("pendulum_period_large(1 m, g_n, 0.001)", "s"), ("pendulum_period(1 m, g_n)", "s")])
    assert got[0] == pytest.approx(got[1], rel=1e-6)


def test_mechanics_units_are_checked():
    e_ = error_of("import mechanics\nprint mechanics.projectile_range(20 m/s, 35°, 3 s)\n")
    assert e_.line == 2
    e_ = error_of("import mechanics\nx = mechanics.kinetic_energy(2 kg, 3 m/s) + 1 m\n")
    assert "energy" in e_.message and "length" in e_.message


# ------------------------------------------------------------------ em
def test_em():
    cases = [
        ("coulomb_field(1 nC, 2 m)", "V/m", k_e * 1e-9 / 4),
        ("coulomb_force(e, -e, 1 nm)", "N", -k_e * e**2 / 1e-18),
        ("coulomb_potential(1 nC, 2 m)", "V", k_e * 1e-9 / 2),
        ("parallel_plate_capacitance(1 cm^2, 1 mm)", "F", eps0 * 1e-4 / 1e-3),
        ("capacitor_energy(2 μF, 10 V)", "J", 0.5 * 2e-6 * 100),
        ("rc_time_constant(1 kΩ, 3 μF)", "s", 3e-3),
        ("rc_charging_voltage(5 V, 1 ms, 2 ms)", "V", 5 * (1 - math.exp(-2))),
        ("cyclotron_angular_frequency(e, 1.5 T, m_p)", "rad/s", e * 1.5 / m_p),
        ("cyclotron_frequency(-e, 1.5 T, m_e)", "rev/s", e * 1.5 / (2 * math.pi * m_e)),     # turns per second
        ("larmor_radius(m_e, 1e6 m/s, -e, 1 mT)", "m", m_e * 1e6 / (e * 1e-3)),
        ("skin_depth(1.68e-8 ohm m, 60 Hz, 1)", "m", math.sqrt(1.68e-8 / (math.pi * 60 * mu0))),
        ("wire_field(10 A, 1 cm)", "T", mu0 * 10 / (2 * math.pi * 0.01)),
        ("solenoid_field(1000 /m, 2 A)", "T", mu0 * 1000 * 2),
    ]
    _tested("em", close(cases, "em"))


def test_skin_depth_of_copper_textbook():
    # copper at 60 Hz: about 8.4 mm (Griffiths-style estimate)
    got = values("em", [("skin_depth(1.68e-8 ohm m, 60 Hz, 1)", "mm")])[0]
    assert 8.3 < got < 8.5


# ------------------------------------------------------------------ nuclear
def semf(A_, Z):
    aV, aS, aC, aA, aP = 15.75, 17.8, 0.711, 23.7, 11.18
    d = 0 if A_ % 2 else (aP / math.sqrt(A_) if Z % 2 == 0 else -aP / math.sqrt(A_))
    return aV * A_ - aS * A_**(2 / 3) - aC * Z * (Z - 1) / A_**(1 / 3) - aA * (A_ - 2 * Z)**2 / A_ + d


def test_nuclear():
    l1, l2 = math.log(2) / 10, math.log(2) / 1        # per hour
    t = 5.0
    mu = m_p / 2
    E_G = 2 * mu * c**2 * (math.pi * alpha)**2
    cases = [
        ("semf_binding(56, 26)", "MeV", semf(56, 26)),
        ("semf_binding(238, 92)", "MeV", semf(238, 92)),
        ("semf_binding(27, 13)", "MeV", semf(27, 13)),       # odd A: no pairing term
        ("semf_binding(14, 7)", "MeV", semf(14, 7)),         # odd-odd
        ("semf_binding_per_nucleon(56, 26)", "MeV", semf(56, 26) / 56),
        ("semf_pairing(56, 26)", "MeV", 11.18 / math.sqrt(56)),
        ("Q_value(238.0507884 u, 234.0436014 u + 4.00260325 u)", "MeV",
         (238.0507884 - 234.0436014 - 4.00260325) * m_u * c**2 / MeV),
        ("nuclear_radius(208)", "fm", 1.2 * 208**(1 / 3)),
        ("decay_constant(5730 yr)", "1/s", math.log(2) / (5730 * yr)),
        ("mean_lifetime(5730 yr)", "yr", 5730 / math.log(2)),
        ("activity(1e20, 5730 yr)", "Bq", 1e20 * math.log(2) / (5730 * yr)),
        ("remaining(100 g, 10 day, 30 day)", "g", 12.5),
        ("bateman_daughter(1e6, 10 hr, 1 hr, 5 hr)", "", 1e6 * l1 / (l2 - l1) * (math.exp(-l1 * t) - math.exp(-l2 * t))),
        ("sommerfeld_parameter(1, 1, 1e6 m/s)", "", alpha * c / 1e6),
        ("gamow_energy(1, 1, m_p/2)", "MeV", E_G / MeV),
        ("gamow_factor(1, 1, 1 keV, m_p/2)", "", math.exp(-math.sqrt(E_G / (1e3 * eV)))),
    ]
    _tested("nuclear", close(cases, "nuclear"))


def test_semf_is_close_to_measured_iron_56():
    # measured B(⁵⁶Fe) = 492.26 MeV (AME2020); the SEMF is good to about 1 %
    got = values("nuclear", [("semf_binding(56, 26)", "MeV")])[0]
    assert abs(got - 492.26) / 492.26 < 0.01


def test_gamow_factor_equals_exp_minus_two_pi_eta():
    # exp(−√(E_G/E)) is exp(−2πη) with the relative speed v = √(2E/μ)
    mu, E = m_p / 2, 1e3 * eV
    v = math.sqrt(2 * E / mu)
    got = values("nuclear", [("gamow_factor(1, 1, 1 keV, m_p/2)", "")])[0]
    assert got == pytest.approx(math.exp(-2 * math.pi * alpha * c / v), rel=1e-9)


def test_bateman_matches_ode_solution():
    got = values("nuclear", [("bateman_daughter(1e6, 10 hr, 1 hr, 5 hr)", "")])[0]
    l1, l2 = math.log(2) / 10, math.log(2) / 1
    sol = scipy.integrate.solve_ivp(lambda t, y: [-l1 * y[0], l1 * y[0] - l2 * y[1]], (0, 5), [1e6, 0],
                                    rtol=1e-11, atol=1e-6)
    assert got == pytest.approx(sol.y[1, -1], rel=1e-7)


# ------------------------------------------------------------------ astro
def test_astro():
    sigma_T = 8 * math.pi / 3 * r_e**2
    H0 = 70e3 / Mpc
    Dc = c / H0 * scipy.integrate.quad(lambda z: 1 / math.sqrt(0.3 * (1 + z)**3 + 0.7), 0, 1, epsabs=0,
                                       epsrel=1e-13)[0]
    cases = [
        ("σ_T", "m^2", sigma_T),
        ("schwarzschild_radius(1 M☉)", "km", 2 * G * M_sun / c**2 / 1e3),
        ("eddington_luminosity(1 M☉)", "W", 4 * math.pi * G * M_sun * m_p * c / sigma_T),
        ("kepler_period(1 au, 1 M☉)", "day", 2 * math.pi * math.sqrt(K["AU"]**3 / (G * M_sun)) / day),
        ("orbital_velocity(M_earth, R_earth)", "m/s", math.sqrt(G * K["M_earth"] / K["R_earth"])),
        ("escape_velocity(M_earth, R_earth)", "m/s", math.sqrt(2 * G * K["M_earth"] / K["R_earth"])),
        ("wien_peak(5772 K)", "nm", b_W / 5772 * 1e9),
        ("stellar_luminosity(R_sun, 5772 K)", "W", 4 * math.pi * R_sun**2 * sigma_SB * 5772**4),
        ("distance_modulus(250 pc)", "", 5 * math.log10(25)),
        ("hubble_distance(70 km/s/Mpc)", "Mpc", c / H0 / Mpc),
        ("comoving_distance(1, 70 km/s/Mpc, 0.3)", "Mpc", Dc / Mpc),
        ("luminosity_distance(1, 70 km/s/Mpc, 0.3)", "Mpc", 2 * Dc / Mpc),
    ]
    _tested("astro", close(cases, "astro"))


def test_luminosity_distance_textbook_value():
    # flat ΛCDM, H0 = 70, Ωm = 0.3: d_L(z = 1) ≈ 6607 Mpc (e.g. Ned Wright's cosmology calculator)
    got = values("astro", [("luminosity_distance(1, 70 km/s/Mpc, 0.3)", "Mpc")])[0]
    assert got == pytest.approx(6607.7, rel=2e-4)


def test_luminosity_distance_in_einstein_de_sitter_is_closed_form():
    # Ωm = 1: d_L = 2c/H0 (1 + z)(1 − 1/√(1+z))
    H0 = 70e3 / Mpc
    want = 2 * c / H0 * 2 * (1 - 1 / math.sqrt(2)) / Mpc
    got = values("astro", [("luminosity_distance(1, 70 km/s/Mpc, 1)", "Mpc")])[0]
    assert got == pytest.approx(want, rel=1e-9)


def test_eddington_luminosity_textbook():
    got = values("astro", [("eddington_luminosity(1 M☉)", "W")])[0]
    assert got == pytest.approx(1.26e31, rel=0.01)


# ------------------------------------------------------------------ quantum
def test_quantum():
    mu_H = m_e * m_p / (m_e + m_p)
    mu_He = m_e * m_alpha / (m_e + m_alpha)

    def barrier(E, V0, a, m):
        if E < V0:
            k = math.sqrt(2 * m * (V0 - E)) / hbar
            return 1 / (1 + V0**2 * math.sinh(k * a)**2 / (4 * E * (V0 - E)))
        k = math.sqrt(2 * m * (E - V0)) / hbar
        return 1 / (1 + V0**2 * math.sin(k * a)**2 / (4 * E * (E - V0)))
    cases = [
        ("box_energy(2, m_e, 1 nm)", "eV", 4 * math.pi**2 * hbar**2 / (2 * m_e * 1e-18) / eV),
        ("oscillator_energy(3, 1e15 rad/s)", "eV", 3.5 * hbar * 1e15 / eV),
        ("de_broglie_wavelength(m_e * 1e6 m/s)", "nm", h / (m_e * 1e6) * 1e9),
        ("de_broglie_wavelength_from_energy(m_e, 100 eV)", "nm", h / math.sqrt(2 * m_e * 100 * eV) * 1e9),
        ("photon_energy(500 nm)", "eV", h * c / 500e-9 / eV),
        ("reduced_mass(m_e, m_p)", "kg", mu_H),
        ("hydrogen_level(1)", "eV", -mu_H * c**2 * alpha**2 / 2 / eV),
        ("hydrogen_level(3)", "eV", -mu_H * c**2 * alpha**2 / 18 / eV),
        ("hydrogenlike_level(1, 2, m_α)", "eV", -mu_He * c**2 * (2 * alpha)**2 / 2 / eV),
        ("barrier_transmission(1 eV, 2 eV, 0.5 nm, m_e)", "", barrier(eV, 2 * eV, 0.5e-9, m_e)),
        ("barrier_transmission(3 eV, 2 eV, 0.5 nm, m_e)", "", barrier(3 * eV, 2 * eV, 0.5e-9, m_e)),
        ("barrier_transmission(2 eV, 2 eV, 0.5 nm, m_e)", "", 1 / (1 + m_e * 0.25e-18 * 2 * eV / (2 * hbar**2))),
    ]
    _tested("quantum", close(cases, "quantum"))


def test_hydrogen_ground_state_is_13_598_eV():
    got = values("quantum", [("hydrogen_level(1)", "eV")])[0]
    assert got == pytest.approx(-13.598, abs=1e-3)


def test_barrier_transmission_is_continuous_at_the_top():
    got = values("quantum", [("barrier_transmission(2 eV - 1e-9 eV, 2 eV, 0.5 nm, m_e)", ""),
                             ("barrier_transmission(2 eV, 2 eV, 0.5 nm, m_e)", ""),
                             ("barrier_transmission(2 eV + 1e-9 eV, 2 eV, 0.5 nm, m_e)", "")])
    assert got[0] == pytest.approx(got[1], rel=1e-6) and got[2] == pytest.approx(got[1], rel=1e-6)


# ------------------------------------------------------------------ stats
XS = [1, 2, 3, 4, 5, 6]
YS = [2.1, 3.9, 6.2, 7.8, 10.1, 11.7]
SS = [0.1, 0.2, 0.1, 0.3, 0.2, 0.25]
PRELUDE = ("xs = [" + ", ".join(f"{x} s" for x in XS) + "]\nys = [" + ", ".join(f"{y} m" for y in YS) + "]\n"
           "σs = [" + ", ".join(f"{s} m" for s in SS) + "]\nmodel = 2 m/s * xs\n")


def test_stats():
    x, y, s = np.array(XS, float), np.array(YS), np.array(SS)
    w = 1 / s**2
    lr = scipy.stats.linregress(x, y)
    chi2 = float(np.sum(((y - 2 * x) / s)**2))
    cases = [
        ("standard_error(ys)", "m", scipy.stats.sem(y)),
        ("weighted_mean(ys, σs)", "m", float(np.sum(w * y) / np.sum(w))),
        ("weighted_mean_error(σs)", "m", float(1 / np.sqrt(np.sum(w)))),
        ("chi_squared(ys, model, σs)", "", chi2),
        ("reduced_chi_squared(ys, model, σs, 1)", "", chi2 / 5),
        ("linear_slope(xs, ys)", "m/s", lr.slope),
        ("linear_intercept(xs, ys)", "m", lr.intercept),
        ("linear_slope_error(xs, ys)", "m/s", lr.stderr),
        ("linear_intercept_error(xs, ys)", "m", lr.intercept_stderr),
        ("correlation(xs, ys)", "", lr.rvalue),
    ]
    _tested("stats", close(cases, "stats", PRELUDE))


def test_stats_units_are_checked():
    e_ = error_of("import stats\nprint stats.linear_slope([1 s, 2 s, 3 s], [1 m, 2 m, 4 m]) + 1 s\n")
    assert e_.line == 2 and "speed" in e_.message


# ------------------------------------------------------------------ coverage, docs
def test_every_stdlib_module_exists():
    assert set(stdlib_modules()) >= {"mechanics", "em", "nuclear", "astro", "quantum", "stats"}


@pytest.mark.parametrize("mod", sorted(EXPORTED))
def test_every_function_has_a_comment(mod):
    lines = EXPORTED[mod].split("\n")
    for s in parse(EXPORTED[mod]).body:
        if isinstance(s, A.FuncDef):
            above = lines[s.line - 2].strip() if s.line >= 2 else ""
            assert above.startswith("#"), f"{mod}.{s.name} (line {s.line}) needs a comment line above it"


@pytest.mark.parametrize("mod", sorted(EXPORTED))
def test_every_module_starts_with_a_description(mod):
    assert EXPORTED[mod].startswith(f"# {mod}: ")


def test_every_function_is_tested():
    for name in ("test_mechanics", "test_em", "test_nuclear", "test_astro", "test_quantum", "test_stats"):
        if not TESTED.get(name.split("_", 1)[1]):
            globals()[name]()
    for mod in EXPORTED:
        missing = set(functions(mod)) - TESTED.get(mod, set())
        assert not missing, f"stdlib {mod}: no test for {sorted(missing)}"


def test_every_function_is_in_the_stdlib_reference():
    doc = open(os.path.join(ROOT, "docs", "stdlib.md"), encoding="utf-8").read()
    for mod in EXPORTED:
        assert f"## {mod}" in doc, f"docs/stdlib.md has no section for {mod}"
        section = doc.split(f"## {mod}", 1)[1].split("\n## ", 1)[0]
        for s in parse(EXPORTED[mod]).body:
            if isinstance(s, A.FuncDef) and not s.name.startswith("_"):
                raw = EXPORTED[mod].split("\n")[s.line - 1].split("(")[0]
                assert f"`{raw}(" in section, f"docs/stdlib.md: {mod} section doesn't list {raw}"
            if isinstance(s, A.Assign):
                assert f"`{s.name}`" in section, f"docs/stdlib.md: {mod} section doesn't list the constant {s.name}"


def test_stdlib_reference_is_in_sync_with_generator():
    from fermium.stdlib_doc import render
    doc = open(os.path.join(ROOT, "docs", "stdlib.md"), encoding="utf-8").read()
    assert doc == render(), "docs/stdlib.md is out of date: run  python3 -m fermium.stdlib_doc > docs/stdlib.md"


def test_stdlib_ships_with_the_package():
    text = open(os.path.join(ROOT, "pyproject.toml"), encoding="utf-8").read()
    assert re.search(r'"fermium"\s*=\s*\[[^\]]*"stdlib/\*\.fm"', text)
