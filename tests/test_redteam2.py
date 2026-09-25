"""Findings of the independent red-team review, round 2 (REDTEAM.md, "Round 2 (05:00 UTC)").

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
"""
import io
import os
import subprocess
import sys

import pytest

from conftest import run, error_of, warnings_of
from fermium.driver import run_source
from fermium.errors import FermiumError
from numparse import num

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run_err(src):
    """Run with the JIT; return (stdout, stderr) so run-time warnings can be seen."""
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


def rt2(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 2 #{n}")


# ---- #1: `2 c` is the speed of light even when c is your variable -------------------------------------------

@rt2(1)
def test_1_two_c_times_t_with_your_own_c_is_not_the_speed_of_light():
    src = "c = 340 m/s\nt = 2 s\nd = 2 c * t\nprint d in m"
    try:
        out = run(src)
    except FermiumError:
        return                                  # an error asking "2*c or 2 [c]?" (D7) is right too
    assert num(out) == pytest.approx(1360)      # today: 1.19917×10⁹ m, silently


@rt2(1)
def test_1_two_c_alone_with_your_own_c_warns():
    assert warnings_of("c = 340 m/s\nprint 2 c in m/s")     # `2 N`, `2 V`, `2 u` all warn; `2 c` doesn't


# ---- #2: Crank–Nicolson with a coarse step: silently wrong ---------------------------------------------------

@rt2(2)
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

@rt2(3)
def test_3_neumann_slope_at_the_boundary_is_the_one_imposed():
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 0 K, ∂u/∂x(0 m, t) = -1 K/m, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 5 s
print ∂u/∂x(0 m, 5 s) to 6 digits
"""
    assert num(run(src)) == pytest.approx(-1, rel=1e-2)     # today -0.747629 K/m (-0.0100 K/m with grid 4000)


@rt2(3)
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

@rt2(4)
@pytest.mark.parametrize("src", ["print std([5 m])", "import stats\nprint stats.standard_error([5 m])"])
def test_4_std_of_a_single_value_is_not_zero(src):
    try:
        out = run(src)
    except FermiumError:
        return                                  # "std needs at least 2 values" is right
    assert "NaN" in out                         # the N − 1 sample std of one value is 0/0; today "0 m"


# ---- #5: stdlib em.cyclotron_frequency converts wrongly to rev/s and rpm -------------------------------------

@rt2(5)
def test_5_cyclotron_frequency_in_rev_per_s():
    src = "import em\nf = em.cyclotron_frequency(e, 1 T, m_p)\nprint f in rev/s to 6 digits"
    # e B / (2π m_p) = 1.52452×10⁷ turns per second; today 2.42635×10⁶ rev/s (with a JIT-only warning)
    assert num(run(src)) == pytest.approx(1.52452e7, rel=1e-5)


# ---- #6: `fermium run --interp` never shows compile-time warnings -------------------------------------------

@rt2(6)
def test_6_interp_cli_shows_the_same_warnings_as_the_jit(tmp_path):
    p = tmp_path / "w.fm"
    p.write_text("f = 50 Hz\nprint f in rpm\n", encoding="utf-8")
    code = "import sys; from fermium.cli import entry; sys.argv[0] = 'fermium'; entry()"
    runs = [subprocess.run([sys.executable, "-c", code, "run", *flag, str(p)], cwd=ROOT,
                           capture_output=True, text=True, timeout=120) for flag in ([], ["--interp"])]
    assert "1 Hz is 9.5493 rpm, not 60 rpm" in runs[0].stderr
    assert "1 Hz is 9.5493 rpm, not 60 rpm" in runs[1].stderr     # today: nothing on stderr


# ---- #7: a call like mechanics.f(...) can't be differentiated ------------------------------------------------

@rt2(7)
@pytest.mark.parametrize("src,value", [
    ("import mechanics\nT(L) = mechanics.pendulum_period(L, 9.81 m/s²)\nprint T'(1 m) to 6 digits", 1.00303),
    ("import astro\nf(T) = 2 astro.wien_peak(T)\nprint f'(5000 K) in nm/K to 6 digits", -0.231822),
])
def test_7_qualified_module_calls_can_be_differentiated(src, value):
    # `from mechanics import pendulum_period` + T(L) = pendulum_period(L, g) works and gives 1.00303 s/m;
    # the qualified form stops with "can't differentiate this expression symbolically"
    assert num(run(src)) == pytest.approx(value, rel=1e-5)


# ---- #8: analyze in natural units says "length, mass, time" and silently works modulo ħ and c ----------------

@rt2(8)
def test_8_analyze_in_natural_units_says_so():
    out = run("units natural(ħ = c = 1)\nanalyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]")
    assert "among length, mass, time" not in out
    assert "ħ" in out or "natural" in out


# ---- #9: nuclear.bateman_daughter is NaN for equal half-lives ------------------------------------------------

@rt2(9)
def test_9_bateman_daughter_equal_half_lives():
    src = "from nuclear import bateman_daughter\nprint bateman_daughter(1000, 10 s, 10 s, 5 s) to 6 digits"
    # limit λ → λ: N0 λ t e^(−λt) = 245.066; today NaN
    assert num(run(src)) == pytest.approx(245.066, rel=1e-4)


# ---- #10: differentiating through a multi-line function: the error has no line -------------------------------

@rt2(10)
def test_10_multiline_function_error_has_a_line():
    src = "g(t) =\n    a = 2 s\n    t^2 / a\nf(t) = g(t) + 1 s\nprint f'(5 s)"
    e = error_of(src)
    assert e.line is not None


# ---- #11: `using …` on its own line: "expected '=' in this equation" ----------------------------------------

@rt2(11)
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

@rt2(12)
def test_12_singular_point_error_names_the_right_variable():
    src = """V(r) = -e²/(4π ε₀ r)
solve -ħ²/(2*m_e) * u'' + V(r) u = E u
    with u(0 nm) = 0, u(5 nm) = 0
    for r from 0 nm to 5 nm
    lowest 3"""
    e = error_of(src)
    assert "r = 0" in e.message                 # today: "can't be evaluated at x = 0 (SI units)"


# ---- #13: assigning to a module constant: the hint suggests `solve … for nuclear` ---------------------------

@rt2(13)
def test_13_assigning_to_a_module_constant_has_a_sensible_hint():
    e = error_of("import nuclear\nnuclear.a_V = 16 MeV")
    assert "solve" not in (e.hint or "")


# ---- #14: sample / randn accept nonsense arguments silently -------------------------------------------------

@rt2(14)
@pytest.mark.parametrize("src", [
    "print len(sample(rand(), 2.5))",           # today: 2
    "print sample(rand(), -1)",                 # today: []
    "print randn(1 m, -1 m)",                   # today: 1.01896 m
])
def test_14_bad_sample_counts_and_negative_sigma_are_errors(src):
    with pytest.raises(FermiumError):
        run(src)
