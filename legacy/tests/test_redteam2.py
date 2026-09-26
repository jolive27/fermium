"""Findings of the independent red-team review, round 2 (dev-notes/REDTEAM.md, "Round 2 (05:00 UTC)").

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
"""
import io
import math
import os
import subprocess
import sys

import pytest

from conftest import run, error_of, warnings_of
from fermium.driver import run_source
from fermium.errors import FermiumError
from numparse import num

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def run_err(src):
    """Run with the JIT; return (stdout, stderr) so run-time warnings can be seen."""
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


def rt2(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 2 #{n}")


# ---- #1: `2 c` is the speed of light even when c is your variable -------------------------------------------

def test_1_two_c_times_t_with_your_own_c_is_not_the_speed_of_light():
    src = "c = 340 m/s\nt = 2 s\nd = 2 c * t\nprint d in m"
    try:
        out = run(src)
    except FermiumError:
        return                                  # an error asking "2*c or 2 [c]?" (D7) is right too
    assert num(out) == pytest.approx(1360)      # today: 1.19917×10⁹ m, silently


def test_1_two_c_alone_with_your_own_c_is_an_error():
    # like `2 N`, `2 V`, `2 u` (it warned before the A1 rule, D235)
    assert "'2 c' is ambiguous" in error_of("c = 340 m/s\nprint 2 c in m/s").message


# ---- #2: Crank–Nicolson with a coarse step: silently wrong ---------------------------------------------------

def test_2_coarse_crank_nicolson_step_is_right_or_warns():
    src = """L = 1 m
D = 0.01 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 2 K * sin(π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 1000 s step 100 s
print u(0.5 m, 1000 s) to 6 digits
"""
    out, err = run_err(src)
    # exact: 2 K exp(-D π² 1000 s / L²) = 2.7×10⁻⁴³ K; today 0.0328237 K and no warning
    assert "warning" in err or abs(num(out)) < 1e-6


# ---- #3: Crank–Nicolson keeps grid-scale wiggles from incompatible initial/boundary data -----------------------

def test_3_neumann_slope_at_the_boundary_is_the_one_imposed():
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 0 K, ∂u/∂x(0 m, t) = -1 K/m, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 5 s
print ∂u/∂x(0 m, 5 s) to 6 digits
"""
    assert num(run(src)) == pytest.approx(-1, rel=1e-2)     # today -0.747629 K/m (-0.0100 K/m with grid 4000)


def test_3_step_initial_data_leaves_no_wiggle_at_the_wall():
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 2 s
print u(0.005 m, 2 s) to 6 digits
"""
    # exact (Fourier series): 5.4×10⁻¹¹ K; the peak at x = L/2 is 3.4×10⁻⁹ K.  Today: -0.00265119 K
    assert abs(num(run(src))) < 1e-6


# ---- #4: std of one value is 0 ------------------------------------------------------------------------------

@pytest.mark.parametrize("src", ["print std([5 m])", "import stats\nprint stats.standard_error([5 m])"])
def test_4_std_of_a_single_value_is_not_zero(src):
    try:
        out = run(src)
    except FermiumError:
        return                                  # "std needs at least 2 values" is right
    assert "NaN" in out                         # the N − 1 sample std of one value is 0/0; today "0 m"


# ---- #5: stdlib em.cyclotron_frequency converts wrongly to rev/s and rpm -------------------------------------

def test_5_cyclotron_frequency_in_rev_per_s():
    src = "import em\nf = em.cyclotron_frequency(e, 1 T, m_p)\nprint f in rev/s to 6 digits"
    # e B / (2π m_p) = 1.52452×10⁷ turns per second; today 2.42635×10⁶ rev/s (with a JIT-only warning)
    assert num(run(src)) == pytest.approx(1.52452e7, rel=1e-5)


# ---- #6: `fermium run --interp` never shows compile-time warnings -------------------------------------------

def test_6_interp_cli_shows_the_same_warnings_as_the_jit(tmp_path):
    p = tmp_path / "w.fm"
    p.write_text("f = 50 Hz\nprint f in rpm\n", encoding="utf-8")
    code = "import sys; from fermium.cli import entry; sys.argv[0] = 'fermium'; entry()"
    runs = [subprocess.run([sys.executable, "-c", code, "run", *flag, str(p)], cwd=ROOT,
                           capture_output=True, text=True, timeout=120) for flag in ([], ["--interp"])]
    assert "1 Hz is 9.5493 rpm, not 60 rpm" in runs[0].stderr
    assert "1 Hz is 9.5493 rpm, not 60 rpm" in runs[1].stderr     # today: nothing on stderr


# ---- #7: a call like mechanics.f(...) can't be differentiated ------------------------------------------------

@pytest.mark.parametrize("src,value", [
    ("import mechanics\nT(L) = mechanics.pendulum_period(L, 9.81 m/s²)\nprint T'(1 m) to 6 digits", 1.00303),
    ("import astro\nf(T) = 2 astro.wien_peak(T)\nprint f'(5000 K) in nm/K to 6 digits", -0.231822),
])
def test_7_qualified_module_calls_can_be_differentiated(src, value):
    # `from mechanics import pendulum_period` + T(L) = pendulum_period(L, g) works and gives 1.00303 s/m;
    # the qualified form stops with "can't differentiate this expression symbolically"
    assert num(run(src)) == pytest.approx(value, rel=1e-5)


# ---- #8: analyze in natural units says "length, mass, time" and silently works modulo ħ and c ----------------

def test_8_analyze_in_natural_units_says_so():
    out = run("units natural(ħ = c = 1)\nanalyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]")
    assert "among length, mass, time" not in out
    assert "ħ" in out or "natural" in out


# ---- #9: nuclear.bateman_daughter is NaN for equal half-lives ------------------------------------------------

def test_9_bateman_daughter_equal_half_lives():
    src = "from nuclear import bateman_daughter\nprint bateman_daughter(1000, 10 s, 10 s, 5 s) to 6 digits"
    # limit λ → λ: N0 λ t e^(−λt) = 245.066; today NaN
    assert num(run(src)) == pytest.approx(245.066, rel=1e-4)


# ---- #10: differentiating through a multi-line function: the error has no line -------------------------------

def test_10_multiline_function_error_has_a_line():
    src = "g(t) =\n    a = 2 s\n    t^2 / a\nf(t) = g(t) + 1 s\nprint f'(5 s)"
    e = error_of(src)
    assert e.line is not None


# ---- #11: `using …` on its own line: "expected '=' in this equation" ----------------------------------------

@pytest.mark.parametrize("src", [
    """solve -ħ²/(2*m_e) * ψ'' = E ψ
    with ψ(0 nm) = 0, ψ(1 nm) = 0
    for x from 0 nm to 1 nm
    lowest 3
    using shooting
print E[1] in eV to 6 digits""",
    """solve ∂u/∂t = (0.01 m²/s) * ∂²u/∂x²
    with u(x, 0 s) = 2 K * sin(π x / 1 m), u(0 m, t) = 0 K, u(1 m, t) = 0 K
    for x from 0 m to 1 m, t from 0 s to 10 s
    using explicit
print u(0.5 m, 10 s) to 6 digits""",
])
def test_11_using_on_its_own_line(src):
    # `grid 1000` works on its own line; `using …` must work too, or say where it goes
    try:
        run(src)
    except FermiumError as e:
        assert "expected '='" not in e.message and "using" in (e.message + (e.hint or ""))


# ---- #12: eigenvalue problem in r: the error talks about x ---------------------------------------------------

def test_12_singular_point_error_names_the_right_variable():
    # the singular point is inside the range: since gauntlet #69 (D172) the ends are never evaluated,
    # so r from 0 nm is the hydrogen problem and works
    src = """V(r) = -e²/(4π ε₀ r)
solve -ħ²/(2*m_e) * u'' + V(r) u = E u
    with u(-5 nm) = 0, u(5 nm) = 0
    for r from -5 nm to 5 nm
    lowest 3"""
    e = error_of(src)
    assert "r = 0" in e.message                 # today: "can't be evaluated at x = 0 (SI units)"


# ---- #13: assigning to a module constant: the hint suggests `solve … for nuclear` ---------------------------

def test_13_assigning_to_a_module_constant_has_a_sensible_hint():
    e = error_of("import nuclear\nnuclear.a_V = 16 MeV")
    assert "solve" not in (e.hint or "")


# ---- #14: sample / randn accept nonsense arguments silently -------------------------------------------------

@pytest.mark.parametrize("src", [
    "print len(sample(rand(), 2.5))",           # today: 2
    "print sample(rand(), -1)",                 # today: []
    "print randn(1 m, -1 m)",                   # today: 1.01896 m
])
def test_14_bad_sample_counts_and_negative_sigma_are_errors(src):
    with pytest.raises(FermiumError):
        run(src)


# ---- further tests added with the fixes ---------------------------------------------------------------------

def test_1_three_c_without_your_own_c_is_still_the_speed_of_light():
    assert num(run("print 3 c in m/s to 6 digits")) == pytest.approx(8.99377e8, rel=1e-6)
    assert not warnings_of("v = 0.5 c\nprint v in m/s")


def test_1_two_c_star_t_with_your_own_c_asks_which_you_mean():
    e = error_of("c = 340 m/s\nt = 2 s\nd = 2 c * t\nprint d in m")
    assert "2*c" in (e.hint or "")


HEAT_1 = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K * sin(π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 0.5 s{step}
print u(0.5 m, 0.5 s) to 6 digits
"""


def test_2_a_step_of_your_own_that_is_too_coarse_warns_in_jit_and_interp():
    from fermium.interp import run_interpreted
    src = HEAT_1.format(step=" step 0.05 s")
    out, err = run_err(src)
    assert "time step is too coarse for this PDE" in err and "line 3" in err
    import contextlib
    buf_out, buf_err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
        run_interpreted(src, "<t>")
    assert buf_out.getvalue().strip() == out
    assert "time step is too coarse for this PDE" in buf_err.getvalue()


def test_2_the_default_step_is_refined_until_it_is_accurate():
    out, err = run_err(HEAT_1.format(step=""))
    # the exact decay of this mode on the 400-interval grid (the time stepping is what's tested): λ_h = 4/h² sin²(πh/2)
    lam = 4 * 400 ** 2 * math.sin(math.pi / 800) ** 2
    assert num(out) == pytest.approx(math.exp(-lam * 0.5), rel=1e-4)
    assert "warning" not in err


def test_2_a_fine_step_of_your_own_does_not_warn():
    out, err = run_err(HEAT_1.format(step=" step 0.001 s"))
    assert num(out) == pytest.approx(math.exp(-math.pi ** 2 * 0.5), rel=1e-4)
    assert "warning" not in err


def test_3_wall_jump_keeps_the_middle_right_and_the_neumann_line_straight():
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 2 s
print u(0.5 m, 2 s) to 6 digits
print u(0.0025 m, 2 s) to 6 digits
solve ∂w/∂t = D * ∂²w/∂x²
    with w(x, 0 s) = 0 K, ∂w/∂x(0 m, t) = -1 K/m, w(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 5 s
print w(0 m, 5 s) to 6 digits
print ∂w/∂x(0.01 m, 5 s) to 6 digits
print ∂w/∂x(0.5 m, 5 s) to 6 digits
"""
    out = [num(v) for v in run(src).splitlines()]
    # Fourier series: u(L/2, 2 s) = (4/π) Σ_odd (±1) e^(−n²π²·2)/n ≈ 3.40445×10⁻⁹ K
    assert out[0] == pytest.approx(4 / math.pi * math.exp(-2 * math.pi ** 2), rel=1e-3)
    assert abs(out[1]) < 1e-6
    # w → 1 K − x·1 K/m; the slowest mode left is e^(−π²/4 · 5 s) ≈ 4×10⁻⁶
    assert out[2] == pytest.approx(1, abs=2e-5)
    assert out[3] == pytest.approx(-1, rel=1e-3)
    assert out[4] == pytest.approx(-1, rel=1e-3)


@pytest.mark.parametrize("args,value", [
    ("1000, 10 s, 10.000001 s, 5 s", 245.0645401137812),      # nearly equal: no cancellation (mpmath, 40 digits)
    ("1000, 10 s, 20 s, 5 s", 267.5792681343340),
    ("1000, 1 s, 1000 s, 800 s", 574.9241016001176),           # δt = −799: no overflow of e^(−δt)
])
def test_9_bateman_daughter_is_accurate_near_and_far_from_equal_half_lives(args, value):
    src = f"from nuclear import bateman_daughter\nprint bateman_daughter({args}) to 12 digits"
    assert num(run(src)) == pytest.approx(value, rel=1e-10)


def test_7_qualified_module_call_in_a_derivative_formula():
    src = "import mechanics\nT(L) = mechanics.pendulum_period(L, 9.81 m/s²)\nprint T''(1 m) to 6 digits\n" \
          "print (d/dL mechanics.pendulum_period(L, 9.81 m/s²))(1 m) to 6 digits"
    out = run(src).splitlines()
    assert num(out[0]) == pytest.approx(-0.501517, rel=1e-5)
    assert num(out[1]) == pytest.approx(1.00303, rel=1e-5)


def test_4_std_of_one_value_says_why_in_jit_interp_and_build(tmp_path):
    from fermium.interp import run_interpreted
    from fermium.aot import build, find_cc
    src = "xs = [5 m]\nprint std(xs)"
    e = error_of(src)
    assert "std needs at least 2 values" in e.message and e.line == 2
    with pytest.raises(FermiumError, match="std needs at least 2 values"):
        run_interpreted(src, "<t>", out=io.StringIO())
    assert num(run("print std([4 m, 6 m]) to 10 digits")) == pytest.approx(math.sqrt(2), rel=1e-9)
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "p")
    build(src, str(tmp_path / "p.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True, timeout=120)
    assert r.returncode != 0 and "std needs at least 2 values" in r.stderr and "N − 1" in r.stderr


@pytest.mark.parametrize("same_line,own_line", [
    ("    lowest 3 using shooting\n", "    lowest 3\n    using shooting\n"),
    ("    lowest 3 using shooting\n", "    using shooting\n    lowest 3\n"),
])
def test_11_using_on_its_own_line_means_the_same(same_line, own_line):
    head = "solve -ħ²/(2*m_e) * ψ'' = E ψ\n    with ψ(0 nm) = 0, ψ(1 nm) = 0\n    for x from 0 nm to 1 nm\n"
    tail = "print E[1] in eV to 6 digits\n"
    assert run(head + own_line + tail) == run(head + same_line + tail)
    assert num(run(head + own_line + tail)) == pytest.approx(0.376030, rel=1e-4)    # π²ħ²/(2 m_e L²)


def test_11_using_twice_is_an_error():
    e = error_of("solve -ħ²/(2*m_e) * ψ'' = E ψ\n    with ψ(0 nm) = 0, ψ(1 nm) = 0\n    for x from 0 nm to 1 nm\n"
                 "    lowest 3 using matrix\n    using shooting\nprint E[1]")
    assert "twice" in e.message


def test_11_pde_using_explicit_on_its_own_line_is_explicit():
    src = """solve ∂u/∂t = (0.01 m²/s) * ∂²u/∂x²
    with u(x, 0 s) = 2 K * sin(π x / 1 m), u(0 m, t) = 0 K, u(1 m, t) = 0 K
    for x from 0 m to 1 m, t from 0 s to 10 s{sep}using explicit
print u(0.5 m, 10 s) to 8 digits
"""
    assert run(src.format(sep="\n    ")) == run(src.format(sep=" "))


def test_12_singular_point_error_is_in_your_units_in_jit_and_interp():
    from fermium.interp import run_interpreted
    src = """V(r) = -e²/(4π ε₀ r)
solve -ħ²/(2*m_e) * u'' + V(r) u = E u
    with u(-5 nm) = 0, u(5 nm) = 0
    for r from -5 nm to 5 nm
    lowest 3"""
    e = error_of(src)
    assert "at r = 0 nm" in e.message and "SI units" not in e.message
    with pytest.raises(FermiumError) as ei:
        run_interpreted(src, "<t>", out=io.StringIO())
    assert ei.value.message == e.message


@pytest.mark.parametrize("src,msg", [
    ("n = 2.5\nprint len(sample(rand(), n))", "whole number, 0 or more, not 2.5"),
    ("n = -1\nprint sample(rand(), n)", "whole number, 0 or more, not -1"),
    ("s = -1 m\nprint randn(1 m, s)", "can't be negative"),
])
def test_14_bad_arguments_say_why_in_jit_interp_and_build(src, msg, tmp_path):
    from fermium.interp import run_interpreted
    from fermium.aot import build, find_cc
    e = error_of(src)
    assert msg in e.message and e.line == 2
    with pytest.raises(FermiumError, match=msg.replace("(", r"\(").replace(".", r"\.")):
        run_interpreted(src, "<t>", out=io.StringIO())
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "p")
    build(src, str(tmp_path / "p.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True, timeout=120)
    assert r.returncode != 0 and msg in r.stderr


def test_14_good_sample_counts_and_sigma_zero_still_work():
    assert run("print len(sample(rand(), 3))") == "3"
    assert run("print len(sample(rand(), 0))") == "0"
    assert run("print randn(1 m, 0 m)") == "1 m"


def test_8_analyze_in_natural_units_is_the_si_analysis_and_its_function_works():
    src = ("units natural(ħ = c = 1)\nanalyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]\n"
           "print pendulum(1 m, 9.81 m/s²) in s to 6 digits\n"
           "analyze planck: l [m] depends on G, ħ, c\nprint planck in m to 4 digits")
    out = run(src).splitlines()
    assert "3 independent dimensions (length, mass, time)" in out[1]
    assert "so T ∝ √(L/g)" in "\n".join(out)
    assert num(out[7]) == pytest.approx(math.sqrt(1 / 9.81), rel=1e-5)
    assert num(out[-1]) == pytest.approx(1.616e-35, rel=1e-3)


def test_8_a_variable_computed_in_natural_units_asks_for_its_unit():
    e = error_of("units natural(ħ = c = 1)\nE = 2 MeV\nanalyze x: t [s] depends on E, ħ")
    assert "computed in natural units" in e.message and e.line == 3
