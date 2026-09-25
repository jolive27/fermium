"""Natural units: `units natural(ħ = c = 1)`, `units nuclear`, `units astro` (D60).

Values are checked "modulo ħ and c" (a mass is an energy, a length is 1/energy) and converted back to SI
exactly on output with `in`.  Reference numbers: CODATA 2022 (scipy.constants) and hand calculation."""
import io
import math
import os
import re
import subprocess
from fractions import Fraction

import pytest

from conftest import run, error_of
from fermium.interp import run_interpreted
from fermium.natural import make_system, SETTABLE, ENERGY
from fermium.units import Dim, L, M, T, I, TH, lookup_unit

HBARC_MEV_FM = 197.3269804     # ħc in MeV fm (exact SI constants)


def num(s):
    """First number in a printed line (handles ×10ⁿ)."""
    m = re.search(r"(-?[\d.]+)(?:×10([⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+))?", s)
    v = float(m.group(1))
    if m.group(2):
        v *= 10 ** int(m.group(2).translate(str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789")))
    return v


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


# ------------------------------------------------------------------ physics results
def test_bohr_radius():
    out = run("units natural(ħ = c = 1)\na0 = 1/(α m_e)\nprint a0 in fm\nprint a0 in Å\nprint a0").splitlines()
    assert out[0] == "52917.7 fm"
    assert out[1] == "0.529177 Å"
    assert out[2].endswith("MeV⁻¹") and abs(num(out[2]) - 1 / (7.2973525643e-3 * 0.51099895)) < 1e-3


def test_compton_wavelength_of_the_electron():
    # λ_C = h/(m_e c) = 2π/m_e in natural units = 2.42631 pm
    out = run("units natural\nλ = 2π/m_e\nprint λ in pm")
    assert abs(num(out) - 2.42631023538) < 1e-5


def test_reduced_compton_wavelength_of_the_proton():
    out = run("units natural\nlam = 1/m_p\nprint lam in fm")
    assert abs(num(out) - 0.2103089) < 1e-6


def test_pion_exchange_range():
    out = run("units nuclear\nm_π = 139.57 MeV\nr = 1/m_π\nprint r\nprint r in fm").splitlines()
    assert out[0] == "1.4138 fm"          # nuclear units show 1/energy in fm (5 significant figures, as given)
    assert abs(num(out[1]) - HBARC_MEV_FM / 139.57) < 1e-3


def test_schwarzschild_radius_with_G_in_natural_units():
    out = run("units natural\nr_s = 2 G M☉\nprint r_s in km\nprint G in GeV^-2").splitlines()
    assert abs(num(out[0]) - 2 * 6.67430e-11 * 1.98841e30 / 299792458.0**2 / 1e3) < 1e-4
    assert abs(num(out[1]) - 6.70883e-39) < 1e-43       # G = 1/M_Planck² (PDG: 6.70883×10⁻³⁹ GeV⁻²)


def test_schwarzschild_radius_geometrized():
    out = run("units natural(G = c = 1)\nM = M☉\nprint M\nprint 2 M in km").splitlines()
    assert abs(num(out[0]) - 1476.625) < 1e-2 and out[0].endswith(" m")
    assert abs(num(out[1]) - 2.95325) < 1e-5


def test_muon_decay_length():
    src = ("units natural\np = 1 GeV\nτ = 2.1969811 μs\nE = sqrt(p^2 + m_μ^2)\nd = (p/m_μ) τ\n"
           "print d in km\nprint d in m")
    out = run(src).splitlines()
    m_mu = 1.883531627e-28 * 299792458.0**2 / 1.602176634e-13     # MeV
    want = 1000 / m_mu * 2.1969811e-6 * 299792458.0 / 1e3
    assert abs(num(out[0]) - want) / want < 1e-6


def test_planck_units():
    out = run("units natural(ħ = c = G = 1)\nprint m_p\nprint 1 in kg\nprint m_p in kg").splitlines()
    assert abs(num(out[0]) - 1.67262192595e-27 / 2.176434e-8) / num(out[0]) < 1e-5
    assert abs(num(out[1]) - 2.176434e-8) < 1e-13          # the Planck mass
    assert abs(num(out[2]) - 1.67262192595e-27) < 1e-32


def test_temperature_with_boltzmann_constant_set_to_one():
    out = run("units natural(ħ = c = k_B = 1)\nT = 300 K\nprint T in meV\nprint 1 eV in K to 9 digits\n"
              "print 25 °C in K")
    lines = out.splitlines()
    assert abs(num(lines[0]) - 25.852) < 1e-3
    assert abs(num(lines[1]) - 11604.518) < 1e-2
    assert lines[2] == "298.15 K"


def test_natural_print_shows_powers_of_MeV():
    out = run("units natural\nE = 939.6 MeV\nr = 1/(197.3 MeV)\nprint E\nprint r\nprint E^2\nprint m_e").splitlines()
    assert out[0] == "939.6 MeV"
    assert out[1] == "0.005068 MeV⁻¹"      # 4 significant figures, like 197.3
    assert out[2] == "882800 MeV²"
    assert out[3] == "0.510999 MeV"


def test_hbar_and_c_are_exactly_one():
    assert run("units natural\nprint ħ, c, ħ c, ħ == 1, c == 1") == "1 1 1 true true"
    assert run("units natural\nprint ħ c in MeV fm") == "197.327 MeV fm"


def test_ODE_and_derivative_inside_natural_units():
    src = ("units natural\nω = 10 MeV\nsolve x'' = -ω^2 x with x(0) = 1.00000 fm, x'(0) = 0 for t from 0 to 1 fm\n"
           "print x(0.500000 fm) in fm to 6 digits\nprint cos(ω * 0.500000 fm) to 6 digits\n"
           "V(r) = -50 MeV * exp(-r/(1.2 fm))\nF(r) = -d/dr V(r)\nprint F(1 fm) in MeV/fm\nprint F(1 fm) in N")
    out = run(src).splitlines()
    assert out[0] == "0.999679 fm" and out[1] == "0.999679"
    f = -50 / 1.2 * math.exp(-1 / 1.2)
    assert abs(num(out[2]) - f) < 1e-4
    assert abs(num(out[3]) - f * 1.602176634e-13 / 1e-15) < 0.01


def test_integral_inside_natural_units():
    assert run("units natural\nk = 2 fm^-1\nI = ∫ exp(-k x) dx from 0 to ∞\nprint I in fm") == "0.5 fm"


# ------------------------------------------------------------------ units are still checked
@pytest.mark.parametrize("src,msg", [
    ("units natural(ħ = c = 1)\nE = 1 MeV\nr = 1 fm\nprint E + r",
     "can't add energy or mass [MeV] to length or time (1/energy) [MeV⁻¹]"),
    ("units natural\nm = 1 kg\nd = 2 fm\nprint m + d", "can't add energy or mass [MeV] to length or time"),
    ("units natural\nE = 1 MeV\nprint E in fm", "can't show energy or mass [MeV] in fm"),
    ("units natural\nprint 1 MeV^2 + 1 MeV", "can't add"),
    ("units natural\nT = 0 MeV\nfor i from 1 to 3\n    T += 1 fm", "can't add energy or mass"),
    ("units natural\nf(E [MeV]) = 2 E\nprint f(1 fm)", "f expects E in MeV (energy or mass [MeV])"),
    ("units natural\nprint sin(1 MeV)", ""),
    ("units natural\nprint exp(1 fm)", ""),
    ("units natural(ħ = c = 1)\nT = 300 K\nprint T + 1 MeV", "can't add"),     # without k_B = 1, K is K
    ("units nuclear\nsolve x' = x with x(0) = 1 fm for t from 0 to 1 fm", ""),
])
def test_natural_units_still_catch_unit_errors(src, msg):
    e = error_of(src)
    assert msg in e.message


def test_mass_plus_energy_and_length_plus_time_are_fine():
    assert run("units natural\nprint 1 kg + 1 J in kg") == "1 kg"
    assert run("units natural\nprint (1 m + 1 s) in m") == "2.99792×10⁸ m"
    assert run("units natural\nprint 1 eV/c^2 in kg") == "1.78266×10⁻³⁶ kg"


# ------------------------------------------------------------------ regions and boundaries
def test_block_region_and_explicit_export():
    src = ("units nuclear:\n    r = 2 fm\n    E = 10 MeV\nprint r in m\nx = r in fm\nprint x\nprint E in J to 10 digits\n"
           "print E in kg to 10 digits")
    out = run(src).splitlines()
    assert out[0] == "2×10⁻¹⁵ m"
    assert out[1] == "2 fm"
    assert abs(num(out[2]) - 1.602176634e-12) < 1e-20
    assert abs(num(out[3]) - 1.602176634e-12 / 299792458.0**2) < 1e-38


def test_exported_value_has_si_units():
    e = error_of("units nuclear:\n    r = 2 fm\ny = r in fm\nprint y in s")
    assert "can't show length [m] in s" in e.message


def test_natural_value_outside_its_region_needs_a_unit():
    e = error_of("units nuclear:\n    r = 2 fm\nprint r")
    assert "r was computed in nuclear units (ħ = c = 1), so its units here are ambiguous" in e.message
    assert "r in fm" in e.hint


def test_export_with_the_wrong_power_of_energy():
    e = error_of("units natural:\n    a = 5 MeV\nprint a in fm")
    assert "a is energy or mass [MeV] in natural units, so it can't be shown in fm" in e.message


def test_SI_variables_convert_into_natural_units():
    out = run("L = 2 m\nm = 3 kg\nunits natural\nprint L, L in fm\nprint m in MeV to 12 digits\n"
              "print L m in 1 to 12 digits").splitlines()
    assert out[0] == "2 m 2×10¹⁵ fm"
    assert abs(num(out[1]) - 3 * 299792458.0**2 / 1.602176634e-13) / num(out[1]) < 1e-9
    # L m / (ħ/c) is dimensionless in natural units
    assert abs(num(out[2]) - 2 * 3 * 299792458.0 / (6.62607015e-34 / (2 * math.pi))) / num(out[2]) < 1e-9


def test_can_not_change_an_SI_variable_inside_natural_units():
    e = error_of("x = 3 m\nunits natural\nx = 2 MeV")
    assert "x was set outside this units natural (ħ = c = 1) region" in e.message


def test_SI_function_is_checked_again_in_natural_units():
    # f(m) = m c² works in both: in natural units c = 1, and the answer converts back to the same joules
    out = run("f(m) = m c^2\nprint f(1 kg)\nunits natural\nprint f(1 kg) in J").splitlines()
    assert out == ["8.98755×10¹⁶ J", "8.98755×10¹⁶ J"]


def test_natural_function_can_not_be_used_in_SI():
    e = error_of("units natural:\n    g(E) = E + 1 kg\nprint g(1 J)")
    assert "g was defined in natural units (ħ = c = 1), so it can only be used where those units hold" in e.message


def test_units_line_must_be_at_top_level():
    e = error_of("for i from 1 to 2\n    units natural")
    assert "must be at the top level" in e.message


@pytest.mark.parametrize("src,msg", [
    ("units natural(ħ = c = 2)", "natural units set constants to 1"),
    ("units natural(h = 1)", "h can't be set to 1"),
    ("units natural(c = c = 1)", "listed twice"),
    ("units nuclear(G = 1)", "units nuclear always means ħ = c = 1"),
    ("units astro(G = 1)", "units astro sets no constants to 1"),
])
def test_bad_units_lines(src, msg):
    assert msg in error_of(src).message


def test_units_is_still_a_variable_name():
    assert run("units = 3\nprint units") == "3"


def test_loading_data_inside_natural_units_is_refused(tmp_path):
    (tmp_path / "d.csv").write_text("t [s],x [m]\n0,1\n")
    e = error_of('units natural\nd = load "d.csv"', base_dir=str(tmp_path))
    assert "data files can't be loaded inside" in e.message


def test_units_SI_switches_back():
    assert run("units natural\nE = 2 MeV\nunits SI\nprint E in J\nprint 1 m") == "3.20435×10⁻¹³ J\n1 m"


# ------------------------------------------------------------------ units astro
def test_astro_display_preset():
    out = run("units astro\nprint G\nprint M☉\nprint 4π^2 (1 AU)^3/(G * (1 yr)^2)\nprint 3 m\nprint 1 AU/yr in km/s")
    lines = out.splitlines()
    assert lines[0] == "39.4769 AU³/(M☉ yr²)"          # 4π² in solar units (Julian year, nominal M☉)
    assert lines[1] == "1 M☉"
    assert lines[2] == "1.00004 M☉"
    assert lines[3] == "3 m"                             # a unit you write is kept
    assert abs(num(lines[4]) - 4.74047) < 1e-4


def test_astro_is_ordinary_SI_checking():
    assert "can't add" in error_of("units astro\nprint 1 AU + 1 M☉").message


# ------------------------------------------------------------------ exactness of the conversion
@pytest.mark.parametrize("consts", [("ħ", "c"), ("ħ", "c", "k_B"), ("G", "c"), ("ħ", "c", "G"), ("c",),
                                    ("ħ", "c", "ε_0"), ("ħ", "c", "k_B", "G", "ε_0")])
def test_split_is_exact_and_round_trips(consts):
    """canon(D) and the factor come from an exact rational split; converting to canonical and back is the identity,
    the canonical map is a homomorphism, and each constant set to 1 has canonical value 1."""
    ns = make_system("natural", list(consts))
    dims = [L, M, T, I, TH, ENERGY, L**3 / (M * T**2), M * L / T, L**2 / T, M ** Fraction(1, 2) * L ** Fraction(-3, 2)]
    for d in dims:
        a, beta = ns.split(d)
        rebuilt = Dim()
        for ai, c in zip(a, ns.consts):
            rebuilt = rebuilt * SETTABLE[c][1] ** ai
        for b, bd in zip(beta, ns.kept):
            rebuilt = rebuilt * bd ** b
        assert rebuilt == d
        for d2 in dims:
            assert ns.canon_dim(d * d2) == ns.canon_dim(d) * ns.canon_dim(d2)
            assert math.isclose(ns.factor(d * d2), ns.factor(d) * ns.factor(d2), rel_tol=1e-12)
    for c in consts:
        v, d = SETTABLE[c]
        assert ns.canon_dim(d) == Dim()
        assert math.isclose(v * ns.factor(d), 1.0, rel_tol=1e-15)


def test_every_unit_round_trips_through_natural_units():
    units = ("m", "fm", "s", "kg", "J", "MeV", "1/GeV²", "barn", "N", "W", "Pa", "K", "C", "T", "V")
    src = "units natural\n" + "\n".join(f"print 1.23456 {u} in {u}" for u in units)
    assert run(src).splitlines() == [f"1.23456 {u}" for u in units]


def test_hbar_c_conversion_value():
    ns = make_system("nuclear")
    fm = ns.canon_unit(lookup_unit("fm"))
    assert math.isclose(1 / (fm.factor * 1.602176634e-13), HBARC_MEV_FM, rel_tol=1e-9)


# ------------------------------------------------------------------ native vs interpreter, fermium build
PROGRAMS = [
    "units natural(ħ = c = 1)\na0 = 1/(α m_e)\nprint a0\nprint a0 in fm\nprint a0 in Å",
    "units nuclear\nm_π = 139.57 MeV\nr = 1/m_π\nprint r, r in m\nprint (1 MeV)^-2",
    "L = 2 m\nunits natural:\n    x = L + 1 fm\n    print x\nprint x in m",
    "units natural\nω = 10 MeV\nsolve x'' = -ω^2 x with x(0) = 1 fm, x'(0) = 0 for t from 0 to 1 fm\n"
    "print x(0.5 fm) in fm",
    "units astro\nprint G, M☉, 1 yr",
]


@pytest.mark.parametrize("src", PROGRAMS)
def test_native_and_interpreter_agree(src):
    assert interp(src) == run(src)


@pytest.mark.parametrize("src", PROGRAMS[:3] + PROGRAMS[4:])
def test_fermium_build_matches_run(src, tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = os.path.join(str(tmp_path), "prog")
    build(src, os.path.join(str(tmp_path), "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(tmp_path))
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == run(src)


def test_fine_structure_constant_emerges_in_natural_units():
    out = run("units natural\nprint e^2/(4π ε_0) to 9 digits, α to 9 digits\n"
              "print e^2/(4π ε_0 * 1 fm) in MeV").splitlines()
    assert out[0] == "0.00729735256 0.00729735256"
    assert out[1] == "1.43996 MeV"            # e²/(4πε₀) = 1.44 MeV fm


def test_heaviside_lorentz_charge():
    # ħ = c = ε₀ = 1: charge is a plain number and e = √(4πα)
    assert run("units natural(ħ = c = ε_0 = 1)\nprint e to 6 digits, sqrt(4π α) to 6 digits") == "0.302822 0.302822"


def test_celsius_with_boltzmann_constant_set_to_one():
    assert run("units natural(ħ = c = k_B = 1)\nT = 25 °C\nprint T, T in K, T in meV") == "25 °C 298.15 K 25.6926 meV"


def test_SI_function_uses_SI_global_inside_natural_units():
    out = run("L0 = 1 fm\nf(x) = x + L0\nprint f(1 m)\nunits natural\nprint f(1 fm) in fm")
    assert out == "1×10¹⁵ fm\n2 fm"
    assert "can't add energy or mass [MeV] to length or time" in error_of(
        "L0 = 1 fm\nf(x) = x + L0\nunits natural\nprint f(1 MeV)").message


def test_export_of_an_expression_and_mixed_systems():
    assert run("units natural:\n    a = 2 fm\n    b = 3 MeV\nprint 2 a b in 1 to 6 digits") == "0.0608128"
    e = error_of("units natural:\n    a = 2 fm\nunits natural(ħ = c = k_B = 1):\n    b = 1 K\nprint a b in 1")
    assert "mixes values from different unit systems" in e.message
