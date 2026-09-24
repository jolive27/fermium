"""Run every program in examples/ and check its key numbers against known physics.

Each example is run once (cached per test session) through fermium.driver.run_source
with base_dir=examples/, so data files and gallery plots resolve as with `fermium run`.

Also checks the Rosetta page (docs/rosetta.md): the Python and Julia versions in
examples/rosetta/ must print the same numbers as the Fermium versions.
"""
import glob
import io
import math
import os
import re
import shutil
import subprocess
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from fermium.driver import run_source  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXAMPLES = os.path.join(ROOT, "examples")
ROSETTA = os.path.join(EXAMPLES, "rosetta")
JULIA = os.path.join(ROOT, ".tools", "julia", "bin", "julia")
JULIA_DEPOT = os.path.join(ROOT, ".tools", "julia-depot")

_cache = {}


def run_example(name):
    """Output of examples/<name>.fm (runs it the first time only)."""
    if name not in _cache:
        path = os.path.join(EXAMPLES, name + ".fm")
        with open(path, encoding="utf-8") as f:
            src = f.read()
        out, err = io.StringIO(), io.StringIO()
        run_source(src, path, out=out, base_dir=EXAMPLES, err=err)
        _cache[name] = out.getvalue()
    return _cache[name]


# --- reading numbers from printed output -----------------------------------------------

_SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
_NUM = re.compile(r"[-−]?\d+(?:\.\d+)?(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+|[eE][-+]?\d+)?")


def numbers(text):
    """All numbers in a piece of text; understands 6.6×10⁻¹¹ and 6.6e-11."""
    vals = []
    for tok in _NUM.findall(text):
        tok = tok.replace("−", "-")
        if "×10" in tok:
            mant, exp = tok.split("×10")
            vals.append(float(mant) * 10.0 ** int(exp.translate(_SUP)))
        else:
            vals.append(float(tok))
    return vals


def line_with(out, label):
    for line in out.splitlines():
        if label in line:
            return line
    raise AssertionError(f"no line containing {label!r} in output:\n{out}")


def num(out, label, i=0):
    """The i-th number printed after `label` on the first line containing it."""
    line = line_with(out, label)
    return numbers(line.split(label, 1)[1])[i]


def close(x, expected, rel=1e-3):
    return math.isclose(x, expected, rel_tol=rel)


# --- every example runs -----------------------------------------------------------------

ALL = sorted(os.path.splitext(os.path.basename(p))[0] for p in glob.glob(os.path.join(EXAMPLES, "*.fm")))


def test_there_are_25_examples():
    assert len(ALL) >= 25


@pytest.mark.parametrize("name", ALL)
def test_example_runs(name):
    out = run_example(name)
    assert out.strip(), f"{name} printed nothing"
    head = open(os.path.join(EXAMPLES, name + ".fm"), encoding="utf-8").read().splitlines()[:4]
    assert all(h.startswith("#") for h in head), "each example starts with a header comment"
    for line in out.splitlines():
        if line.startswith("plot saved to"):
            saved = line.split("plot saved to", 1)[1].strip()
            assert saved.startswith("gallery/"), f"plots go to examples/gallery/, got {saved}"
            assert os.path.exists(os.path.join(EXAMPLES, saved))


# --- physics checks ---------------------------------------------------------------------

def test_pendulum():
    out = run_example("01_pendulum")
    assert num(out, "measured g =") == pytest.approx(9.70, abs=0.005)       # spec §3.1: 9.70 m/s²
    assert num(out, "in imperial:") == pytest.approx(31.8, abs=0.05)
    assert num(out, "small-angle period:") == pytest.approx(2.20, abs=0.005)
    # exact period / small-angle period at 90°: 2K(sin 45°)/π = 1.18034
    assert num(out, "amplitude 90", 1) == pytest.approx(1.18, abs=0.005)
    assert num(out, "g from the fit:") == pytest.approx(9.81, abs=0.05)


def test_projectile():
    out = run_example("02_projectile")
    R = 30.0**2 * math.sin(math.radians(90)) / 9.80665
    assert close(num(out, "range"), R, 2e-3)
    assert num(out, "best angle without air:") == 45
    assert 40 < num(out, "with air: range") < 80


def test_damped_spring():
    out = run_example("03_damped_spring")
    assert num(out, "ω0 =") == pytest.approx(10.0)
    assert num(out, "Q =") == pytest.approx(25.0)
    g, w0 = 0.2, 10.0
    wd = math.sqrt(w0**2 - g**2)
    exact = 10 * math.exp(-g * 5) * (math.cos(wd * 5) + g / wd * math.sin(wd * 5))   # cm
    assert close(num(out, "x(5 s) numerical:"), exact, 1e-5)
    assert close(num(out, "fraction left:"), math.exp(-4.0), 0.1)


def test_kepler_orbit():
    out = run_example("04_kepler_orbit")
    assert num(out, "semi-major axis a =") == pytest.approx(1.000, abs=0.002)
    assert num(out, "period from Kepler") == pytest.approx(365.25, abs=0.5)
    assert num(out, "eccentricity:") == pytest.approx(0.0167, abs=0.0005)
    assert num(out, "relative energy drift:") < 1e-6
    assert num(out, "distance from start after one period:") < 100          # km, out of 1.5×10⁸ km
    assert num(out, "aphelion distance:") == pytest.approx(1.017, abs=0.001)


def test_escape_velocity():
    out = run_example("05_escape_velocity")
    assert num(out, "Earth:") == pytest.approx(11.2, abs=0.05)
    assert num(out, "Moon:") == pytest.approx(2.38, abs=0.01)
    assert num(out, "Sun:") == pytest.approx(617.7, abs=0.5)
    assert num(out, "W/(1 kg):") == pytest.approx(num(out, "Earth:"), rel=1e-6)
    assert num(out, "Schwarzschild radius of the Sun:") == pytest.approx(2.953, abs=0.001)
    assert num(out, "Schwarzschild radius of the Earth:") == pytest.approx(8.87, abs=0.01)


def test_blackbody():
    out = run_example("06_blackbody")
    b = 2.897771955e-3
    assert num(out, "(from dB/dλ = 0):") == pytest.approx(b / 5778 * 1e9, abs=0.05)   # 501.5 nm
    assert num(out, "(from dB/dλ = 0):") == pytest.approx(502, abs=1)
    sigma = 5.670374419e-8
    assert close(num(out, "π ∫ B dλ ="), sigma * 5778**4 / 1e6, 1e-5)
    assert abs(num(out, "relative difference:")) < 1e-6
    assert close(num(out, "solar luminosity:"), 3.84e26, 0.01)


def test_radioactive_decay():
    out = run_example("07_radioactive_decay")
    assert num(out, "left after one half-life:") == pytest.approx(0.5)
    lam = math.log(2) / 5730
    assert num(out, "age of the wood:") == pytest.approx(math.log(0.231 / 0.118) / lam, rel=2e-3)
    # the curie was defined from 1 g of radium: ≈ 1 Ci
    assert num(out, "Ra-226:", 1) == pytest.approx(0.9885, abs=0.001)
    assert num(out, "5 half-lives:") == pytest.approx(1 / 32, abs=0.001)


def test_bateman_chain():
    out = run_example("08_bateman_chain")
    lMo, lTc = math.log(2) / 65.94, math.log(2) / 6.0067
    tmax = math.log(lTc / lMo) / (lTc - lMo)
    assert num(out, "Tc-99m peaks at") == pytest.approx(tmax, abs=0.05)
    assert close(num(out, "solve"), num(out, "Bateman formula"), 0.01)
    assert num(out, "activity ratio at 200 h:") == pytest.approx(lTc / (lTc - lMo), abs=0.002)
    assert num(out, "Mo-99 activity:") == pytest.approx(num(out, "Tc-99m activity:"), rel=2e-3)
    assert num(out, "total at 240 h / N0:") == pytest.approx(1.0, abs=1e-6)


def test_binding_energy():
    out = run_example("09_binding_energy")
    assert num(out, "Fe-56:  B/A =") == pytest.approx(8.8, abs=0.1)     # measured 8.79 MeV
    assert num(out, "U-238:  B/A =") == pytest.approx(7.6, abs=0.1)
    assert 50 <= num(out, "the peak of the curve is at A =") <= 65
    assert num(out, "for A = 208:") == 82
    assert 150 < num(out, "fission of U-235 releases about") < 220     # ~200 MeV


def test_nuclear_radius():
    out = run_example("10_nuclear_radius")
    assert num(out, "A = 56") == pytest.approx(1.20 * 56 ** (1 / 3), abs=0.01)    # 4.59 fm
    assert num(out, "A = 238") == pytest.approx(7.44, abs=0.01)
    assert num(out, "density of Fe-56:") == pytest.approx(2.3e17, rel=0.03)
    assert num(out, "nucleons per fm³:") == pytest.approx(0.138, abs=0.002)
    assert num(out, "Pb-208: r0 from data =") == pytest.approx(1.2, abs=0.01)


def test_q_values():
    out = run_example("11_q_value")
    assert num(out, "1 u c² =") == pytest.approx(931.494, abs=0.001)
    # textbook Q-values (MeV)
    assert num(out, "D + T  -> He-4 + n     Q =") == pytest.approx(17.589, abs=0.002)
    assert num(out, "D + D  -> He-3 + n     Q =") == pytest.approx(3.269, abs=0.002)
    assert num(out, "p + Li-7 -> 2 He-4     Q =") == pytest.approx(17.346, abs=0.002)
    assert num(out, "n + Li-6 -> T + He-4   Q =") == pytest.approx(4.783, abs=0.002)
    assert num(out, "U-238 -> Th-234 + α    Q =") == pytest.approx(4.270, abs=0.002)
    assert num(out, "n -> p + e + ν         Q =") == pytest.approx(0.782, abs=0.001)
    assert num(out, "4 H -> He-4            Q =") == pytest.approx(26.73, abs=0.01)
    assert num(out, "α + N-14 -> O-17 + p   Q =") == pytest.approx(-1.192, abs=0.002)
    assert num(out, "threshold alpha energy:") == pytest.approx(1.53, abs=0.01)


def test_coulomb_barrier():
    out = run_example("12_coulomb_barrier")
    assert num(out, "ħc =") == pytest.approx(197.327, abs=0.001)
    assert num(out, "e²/(4π ε₀) =") == pytest.approx(1.43996, abs=1e-5)
    assert num(out, "α ħc =") == pytest.approx(1.43996, abs=1e-5)

    def vc(z1, a1, z2, a2):
        return z1 * z2 * 1.439964 / (1.20 * (a1 ** (1 / 3) + a2 ** (1 / 3)))
    assert num(out, "p + p:") == pytest.approx(vc(1, 1, 1, 1), rel=2e-3)
    assert num(out, "α + U-238:") == pytest.approx(vc(2, 4, 92, 238), rel=2e-3)
    assert num(out, "O-16 + Pb-208:") == pytest.approx(vc(8, 16, 82, 208), rel=2e-3)
    assert num(out, "k_B T in the solar core:") == pytest.approx(8.617333e-5 * 1.57e7 / 1e3, rel=5e-3)
    assert num(out, "closest approach of a 5 MeV alpha to gold:") == pytest.approx(2 * 79 * 1.439964 / 5, rel=2e-3)


def test_lane_emden():
    out = run_example("13_lane_emden")
    known = {"n = 0 ": math.sqrt(6), "n = 1 ": math.pi, "n = 1.5 ": 3.65375, "n = 2 ": 4.35287,
             "n = 3 ": 6.89685, "n = 4 ": 14.97155}
    for label, xi in known.items():
        assert num(out, label + "  ξ₁ =") == pytest.approx(xi, rel=2e-4), label
    assert num(out, "-ξ₁² θ'(ξ₁) =") == pytest.approx(2.018, abs=0.002)
    assert num(out, "ρ_c / ρ̄ =") == pytest.approx(54.18, abs=0.1)


def test_hydrostatic_equilibrium():
    out = run_example("14_hydrostatic_equilibrium")
    G, M, R = 6.67430e-11, 1.98841e30, 6.957e8
    pc = 3 * G * M**2 / (8 * math.pi * R**4)
    assert num(out, "central pressure (solve):") == pytest.approx(pc, rel=1e-3)
    assert num(out, "central pressure (formula):") == pytest.approx(pc, rel=1e-3)
    assert num(out, "scale height H =") == pytest.approx(8.314462618 * 288 / (0.02897 * 9.80665) / 1e3, rel=1e-3)
    assert num(out, "(8849 m):") == pytest.approx(num(out, "exact:"), rel=1e-4)


def test_relativity():
    out = run_example("15_relativity")
    assert num(out, "electron") == pytest.approx(0.510999, abs=1e-6)
    assert num(out, "proton", 0) == pytest.approx(938.272, abs=0.001)
    g = 1 + 1 / 0.51099895
    assert num(out, "γ =") == pytest.approx(g, abs=0.001)
    assert num(out, "v =") == pytest.approx(math.sqrt(1 - 1 / g**2), abs=1e-4)
    assert num(out, "check E²") == pytest.approx(1.511, abs=0.001)
    assert num(out, "LHC proton: γ =") == pytest.approx(6800e3 / 938.272, rel=0.01)
    assert num(out, "dK/dv at v = 0.5 c:") == pytest.approx(num(out, "γ³ m v ="), rel=1e-3)


def test_rc_circuit():
    out = run_example("16_rc_circuit")
    tau = 4.70e3 * 220e-6
    assert num(out, "τ = R C =") == pytest.approx(tau, abs=0.005)
    assert num(out, "after τ:", 1) == pytest.approx(1 - math.exp(-1), abs=1e-3)
    stored = 0.5 * 220e-6 * 9.0**2 * 1e3
    assert num(out, "energy stored in C:") == pytest.approx(stored, rel=2e-3)
    assert num(out, "energy turned to heat:") == pytest.approx(stored, rel=2e-3)
    assert num(out, "energy from the battery:") == pytest.approx(2 * stored, rel=2e-3)
    assert num(out, "check:") == pytest.approx(4.50, abs=0.005)


def test_heat_equation():
    out = run_example("17_heat_equation")
    assert num(out, "relative error:") < 1e-3
    assert num(out, "bump height: simulation") == pytest.approx(80 * math.exp(-1.11e-4 * math.pi**2 * 200 / 0.25), rel=2e-3)


def test_fit_decay_data():
    out = run_example("18_fit_decay_data")
    assert num(out, "number of measurements:") == 40
    t_half = num(out, "t_half =")
    err = num(out, "t_half =", 1)
    assert abs(t_half - 153.12) < 2 * err                  # simulated with t½ = 153.12 s
    assert num(out, "half-life:") == pytest.approx(2.55, abs=0.05)


def test_rutherford():
    out = run_example("19_rutherford_scattering")
    d = 2 * 79 * 1.439964 / 7.69
    assert num(out, "closest approach d =") == pytest.approx(d, rel=2e-3)
    assert num(out, "by integration:") == pytest.approx(num(out, "π b(90°)² ="), rel=1e-6)
    assert num(out, "by integration:") == pytest.approx(math.pi * (d / 2) ** 2 / 100, rel=3e-3)   # fm² -> b


def test_gravitational_redshift():
    out = run_example("20_gravitational_redshift")
    assert num(out, "c z =") == pytest.approx(636, abs=1)
    assert num(out, "Pound") == pytest.approx(9.80665 * 22.5 / 299792458**2, rel=2e-3)
    assert num(out, "net:") == pytest.approx(38.4, abs=0.2)             # the famous 38 μs/day


def test_compton_debroglie():
    out = run_example("21_compton_debroglie")
    assert num(out, "Compton wavelength of the electron:") == pytest.approx(2.42631, abs=1e-5)
    assert num(out, "Compton edge") == pytest.approx(477.3, abs=0.1)
    assert num(out, "electron, 100 eV:") == pytest.approx(0.1226, abs=1e-4)
    assert num(out, "thermal neutron, 25 meV:") == pytest.approx(0.1809, abs=1e-4)
    assert num(out, "electron, 1 GeV:") == pytest.approx(1.2398, abs=1e-3)   # hc / 1 GeV


def test_doppler():
    out = run_example("22_doppler_redshift")
    assert num(out, "relativistic v =") == pytest.approx(0.8, abs=1e-4)
    assert num(out, "Hubble time 1/H0 =") == pytest.approx(13.97, abs=0.05)
    assert num(out, "siren approaching:") == pytest.approx(700 * 343 / 313, abs=0.5)


def test_alpha_decay_gamow():
    out = run_example("23_alpha_decay_gamow")
    # a crude model: right to within a factor of ~50 over 24 orders of magnitude
    assert 1 / 50 < num(out, "Po-212: G =", 1) / 2.99e-7 < 50
    assert 1 / 50 < num(out, "U-238:  t½ =") / 4.47e9 < 50
    assert 1 / 50 < num(out, "Th-232: t½ =") / 1.40e10 < 50


def test_white_dwarf():
    out = run_example("24_white_dwarf_chandrasekhar")
    assert num(out, "Chandrasekhar mass:") == pytest.approx(1.456, abs=0.01)
    assert num(out, "electrons become relativistic above") == pytest.approx(1.95e9, rel=0.01)
    assert num(out, "2GM/(R c²):") == pytest.approx(2 * 6.6743e-11 * 1.4 * 1.98841e30 / (12e3 * 299792458**2), rel=0.02)


def test_rocket():
    out = run_example("25_rocket_equation")
    dv = 2.58 * math.log(2.97 / (2.97 - 2.16))
    assert num(out, "Tsiolkovsky") == pytest.approx(dv, abs=0.005)
    assert num(out, "speed at burnout with gravity:") == pytest.approx(dv - 9.80665 * 168 / 1000, abs=0.01)


# --- Rosetta page: the same programs in Fermium, Python and Julia ---------------------------

ROSETTA_NAMES = sorted(os.path.splitext(os.path.basename(p))[0] for p in glob.glob(os.path.join(ROSETTA, "*.fm")))


def rosetta_numbers(text):
    """Numbers per output line (ignoring lines without numbers)."""
    return [numbers(line) for line in text.splitlines() if numbers(line)]


def assert_same_numbers(fm_out, other_out, what, rel=2e-3):
    a, b = rosetta_numbers(fm_out), rosetta_numbers(other_out)
    assert len(a) == len(b), f"{what}: {len(a)} numeric lines in Fermium vs {len(b)}\n{fm_out}\n---\n{other_out}"
    for i, (xs, ys) in enumerate(zip(a, b)):
        assert len(xs) == len(ys), f"{what} line {i + 1}: {xs} vs {ys}"
        for x, y in zip(xs, ys):
            assert math.isclose(x, y, rel_tol=rel, abs_tol=1e-12), f"{what} line {i + 1}: {xs} vs {ys}"


def rosetta_fermium(name):
    key = "rosetta/" + name
    if key not in _cache:
        path = os.path.join(ROSETTA, name + ".fm")
        out = io.StringIO()
        run_source(open(path, encoding="utf-8").read(), path, out=out, base_dir=ROSETTA, err=io.StringIO())
        _cache[key] = out.getvalue()
    return _cache[key]


def test_rosetta_has_6_to_8_programs():
    assert 6 <= len(ROSETTA_NAMES) <= 8
    for name in ROSETTA_NAMES:
        for ext in (".py", ".jl"):
            assert os.path.exists(os.path.join(ROSETTA, name + ext)), name + ext


def test_rosetta_page_matches_files():
    """docs/rosetta.md shows exactly the code in examples/rosetta/ (so what the page shows is what's tested)."""
    page = open(os.path.join(ROOT, "docs", "rosetta.md"), encoding="utf-8").read()
    for name in ROSETTA_NAMES:
        for ext, lang in ((".fm", "fermium"), (".py", "python"), (".jl", "julia")):
            code = open(os.path.join(ROSETTA, name + ext), encoding="utf-8").read().strip()
            assert f"```{lang}\n{code}\n```" in page, f"docs/rosetta.md is out of date for {name}{ext}"


def _julia():
    return JULIA if os.path.exists(JULIA) else shutil.which("julia")


def other_outputs(lang):
    """Run all Python (or Julia) Rosetta programs at once, in parallel; returns {name: CompletedProcess-like}."""
    key = "rosetta-" + lang
    if key not in _cache:
        env = dict(os.environ)
        if lang == "julia":
            if os.path.isdir(JULIA_DEPOT):   # QuadGK and Unitful are installed here (see benchmarks/)
                env["JULIA_DEPOT_PATH"] = JULIA_DEPOT
            cmd = [_julia(), "--startup-file=no", f"--project={ROSETTA}"]
            ext = ".jl"
        else:
            cmd, ext = [sys.executable], ".py"
        procs = {n: subprocess.Popen(cmd + [os.path.join(ROSETTA, n + ext)], stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True, cwd=ROSETTA, env=env)
                 for n in ROSETTA_NAMES}
        results = {}
        for n, p in procs.items():
            out, err = p.communicate(timeout=300)
            results[n] = (p.returncode, out, err)
        _cache[key] = results
    return _cache[key]


@pytest.mark.parametrize("name", ROSETTA_NAMES)
def test_rosetta_python(name):
    code, out, err = other_outputs("python")[name]
    assert code == 0, err
    assert_same_numbers(rosetta_fermium(name), out, name + ".py")


@pytest.mark.skipif(not os.path.exists(JULIA) and not shutil.which("julia"), reason="Julia not installed")
@pytest.mark.parametrize("name", ROSETTA_NAMES)
def test_rosetta_julia(name):
    code, out, err = other_outputs("julia")[name]
    assert code == 0, err
    assert_same_numbers(rosetta_fermium(name), out, name + ".jl")
