"""Findings of the independent red-team review, round 4 (REDTEAM.md, "Round 4 (07:30 UTC)").

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
For a false positive, the test asserts that the correct program runs and prints the right thing.
"""
import io
import re

import pytest

from conftest import run
from fermium.driver import run_source
from fermium.errors import FermiumError
from numparse import num


def run_err(src, base_dir=None):
    """Run with the JIT; return (stdout, stderr) so warnings can be seen."""
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err, base_dir=base_dir)
    return out.getvalue().strip(), err.getvalue()


def rt4(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 4 #{n}")


NEWTON = ("k = 0.1 1/min\n"
          "solve T' = -k (T - 20 °C) with T(0 min) = 90 °C for t from 0 min to 30 min tolerance 1e-10 absolute {tol}\n"
          "print T(30 min) in °C to 6 digits\n")


# ---- #1: `absolute 0.001 °C` is read as an absolute temperature (274.15 K), so the solve is sloppy --------------

def test_1_absolute_tolerance_in_celsius_is_a_temperature_step():
    exact = 20 + 70 * 2.718281828459045 ** -3                    # 23.4851 °C
    assert num(run(NEWTON.format(tol="1e-6 K"))) == pytest.approx(exact, abs=1e-3)      # K: fine today
    try:
        out = run(NEWTON.format(tol="1e-6 °C"))
    except FermiumError:
        return                                                    # an error ("write it in K") is right too
    # today: 26.6536 °C (the tolerance is 274.15 K), with no warning
    assert num(out) == pytest.approx(exact, abs=1e-3)


# ---- #2: Hz <-> rpm conversions at the Python boundary are silent -----------------------------------------------

def test_2_python_function_declared_in_hz_given_rpm_warns():
    src = ("use python numpy as np:\n"
           "    positive(f [Hz]) -> [Hz]\n"
           "print np.positive(60 rpm)\n")
    out, err = run_err(src)
    # today: "6.28 Hz" (Python receives 2π, not 1) and no warning; `60 rpm in Hz` in Fermium warns (round 1 #2)
    assert "warning" in err or num(out) == pytest.approx(1.0)


# ---- #3: `1.5 kT` with your own k and T is 1.5 kilotesla, printed as "1.5 kT" ----------------------------------

def test_3_kT_with_own_k_and_T_is_not_silently_kilotesla():
    src = "k = 1.38e-23 J/K\nT = 300 K\nE = 1.5 kT\nprint E\n"
    try:
        out, err = run_err(src)
    except FermiumError:
        return                                                    # an error is right too
    # today: "1.5 kT" (kilotesla) with no warning at all
    assert "warning" in err


# ---- #4: `60 s / (m c_w)` with your own m is a parse error -------------------------------------------------------

def test_4_divisor_bracket_starting_with_your_variable_parses():
    src = ("P = 1000 W\nm = 1 kg\nc_w = 4186 J/(kg K)\n"
           "print P 60 s / (m c_w) to 6 digits\n")
    # today: "expected ')' but found 'c_w'"
    assert num(run(src)) == pytest.approx(60000 / 4186, rel=1e-3)


# ---- #5: `∫ x * -2 dx` says the dx is missing ------------------------------------------------------------------

def test_5_negative_factor_in_integrand():
    # today: "this integral is missing its 'dx'"
    assert num(run("print ∫ x * -2 dx from 0 to 1")) == pytest.approx(-1.0)


# ---- #6: the D173 warning fires when the other reading is a unit error -------------------------------------------

def test_6_no_limit_warning_when_subtracting_after_is_a_unit_error():
    src = "v = 3 m/s\nT = 10 s\nt0 = 2 s\nprint ∫ v dt from 0 s to T - t0\n"
    out, err = run_err(src)
    assert out == "24 m"
    # (∫ … to T) - t0 would be metres minus seconds, so only the limit reading is possible
    assert "part of the upper limit" not in err


# ---- #7: a spaced `/` after the upper limit: refused even when only the limit reading is well-typed ----------------

def test_7_division_in_limit_when_only_that_reading_has_the_right_units():
    src = "E = 3600 J\nP0 = 2 W\nprint ∫ P0 dt from 0 s to E / (2 P0)\n"
    try:
        out = run(src)
    except FermiumError as e:
        # an error is acceptable only if it points at the limit, not "put the integral in parentheses"
        assert "(E / (2 P0))" in (e.message + str(e.hint)) or "to (" in str(e.hint)
        return
    assert num(out) == pytest.approx(3600)


# ---- #8: PDE step control warns on the textbook step-change problem although the answer is accurate -------------

def test_8_heat_step_change_is_accurate_and_quiet():
    src = ("L = 1 m\nD = 1e-4 m^2/s\n"
           "solve ∂u/∂t = D * ∂²u/∂x²\n"
           "    with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(L, t) = 0 K\n"
           "    for x from 0 m to L, t from 0 s to 1000 s\n"
           "print u(0.1 m, 1000 s) to 6 digits\n")
    out, err = run_err(src)
    assert num(out) == pytest.approx(65.8436, abs=1e-3)         # Fourier series: 65.84355 K; today 65.8435 K
    # today: "could not be made fine enough: with 32000 steps the estimated error is still 0.8%"
    assert "could not be made fine enough" not in err


# ---- #9: the hint for `8.5e28 m^-3` next to your own m suggests `8.5e+28 [m]` --------------------------------------

def test_9_lone_unit_hint_keeps_the_whole_unit_and_the_number_as_written():
    out, err = run_err("m = 9.11e-31 kg\nn = 8.5e28 m^-3\nprint n\n")
    assert out == "8.5×10²⁸ 1/m³"
    # following today's hint (8.5e+28 [m]) would turn the density into a length
    assert "[m]" not in err
    assert "8.5e+28" not in err


# ---- #10: one token gets a warning ("is the unit") and then an error ("is ambiguous") ------------------------------

def test_10_no_contradictory_warning_before_the_ambiguity_error():
    out, err = io.StringIO(), io.StringIO()
    with pytest.raises(FermiumError):
        run_source("m = 1.2 kg\nprint 2.2 * 5000 m\n", "<t>", out=out, err=err)
    # today: "warning: '5000 m' is the unit m, not your variable m", then the error "'5000 m' is ambiguous"
    assert "is the unit m, not your variable" not in err.getvalue()


# ---- #11: an uncertainty that is only rounding noise sets the printed digits --------------------------------------

@pytest.mark.parametrize("src, bad", [
    ("L = 1.000 ± 0.010 m\ng = 9.81 m/s^2\nT = 2 π sqrt(L / g)\nprint T / sqrt(L)", "2.0060666807106475318"),
    ("y = 1.000 ± 0.010 m\nprint (y / 3) * 3 - y", "×10⁻¹⁸"),
])
def test_11_rounding_noise_sigma_doesnt_print_twenty_digits(src, bad):
    out = run(src)
    assert bad not in out
    assert len(re.sub(r"[^0-9]", "", out.split("±")[0])) <= 6


# ---- #12: quadrature and vector rounding noise is printed as a 3-figure result ------------------------------------

@rt4(12)
@pytest.mark.parametrize("src, good", [
    ("print ∫ sin(x) dx from -π to π", "0"),
    ("f(x) = <cos(x), sin(x), 0>\nprint ∫ f(x) dx from 0 to π", "<0, 2, 0>"),
])
def test_12_integral_rounding_noise_prints_as_zero(src, good):
    # today: 3.19×10⁻¹⁶ and <1.67×10⁻¹⁶, 2.00, 0>; exp(1i π) already prints -1 + 0i
    out = run(src)
    assert "10⁻¹⁶" not in out
    assert out.replace("2.00", "2") == good


# ---- #13: `2 kg c²` shows its SI value with 6 significant figures -------------------------------------------------

@rt4(13)
def test_13_unit_with_constant_shows_si_value_with_default_figures():
    out = run("print 2 kg c^2")
    # today: "2 kg c² (= 1.79751×10¹⁷ J)", while `print 2 * 1 kg * c^2` gives 1.80×10¹⁷ J
    assert "1.79751" not in out
    assert "1.80×10¹⁷ J" in out


# ---- #14: lists pad exact integers (and 1-figure literals) to the longest element ---------------------------------

@rt4(14)
@pytest.mark.parametrize("src, good", [
    ("print [1.2345, 2]", "[1.2345, 2]"),
    ("print [5.018245e9, -6934.574, 9e1]", "[5.018245×10⁹, -6934.574, 90]"),
    ("print 10000000", "10000000"),
])
def test_14_list_and_literal_printing(src, good):
    # today: [1.2345, 2.0000], 90.00000 (9e1 has 1 significant figure) and 1×10⁷ (123456789 prints as written)
    assert run(src) == good


# ---- #15: the bootcamp still says `0.5 m v^2` means metres with a warning, and that `1/2 m v^2` warns ------------

def test_15_bootcamp_prose_matches_the_error():
    import os
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    l2 = open(os.path.join(root, "bootcamp", "lesson02_variables_formulas.md"), encoding="utf-8").read()
    ts = open(os.path.join(root, "bootcamp", "TROUBLESHOOTING.md"), encoding="utf-8").read()
    assert "means 0.5 *metres* (Fermium warns you)" not in l2
    assert "`0.5 m v^2` uses *metres*" not in l2
    assert "10. [warning: 'm' after the number means the unit m]" not in ts


# ---- #16: exact ties round half to even (0.125 -> 0.12), undocumented -------------------------------------------

@rt4(16)
def test_16_exact_ties_round_half_up_or_are_documented():
    import os
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ref = open(os.path.join(root, "docs", "reference.md"), encoding="utf-8").read()
    if "half to even" in ref or "half-even" in ref:
        return
    assert run("print 0.125 * 1.0") == "0.13"
    assert run("print 2.5e6 * 1.3") == "3.3×10⁶"


# ---- #17: slicing a data table: the hint talks about vector components --------------------------------------------

def test_17_slicing_a_table_message(tmp_path):
    (tmp_path / "p.csv").write_text("L [m], T [s]\n0.2, 0.9\n0.4, 1.28\n0.6, 1.55\n0.8, 1.79\n")
    src = 'data = load "p.csv"\nfit T = 2 π sqrt(L / g) to data[2:4]\nprint g\n'
    try:
        run(src, base_dir=str(tmp_path))
    except FermiumError as e:
        # today: "only lists can be sliced … this isn't a list", hint "pick single components with v[1], v[2]"
        assert "v[1]" not in str(e.hint)
        assert "table" in e.message or "column" in (e.message + str(e.hint))
        return


# ---- #18: an absolute tolerance larger than the solution is accepted silently -------------------------------------

def test_18_absolute_tolerance_larger_than_the_solution_warns():
    src = ("solve x'' = -x / (1 s)^2 with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 10 s absolute 1 km\n"
           "print x(10 s)\n")
    try:
        out, err = run_err(src)
    except FermiumError:
        return
    # today: 89.2 m (exact -0.839 m) with no warning
    assert "warning" in err or num(out) == pytest.approx(-0.839, abs=0.01)


# ---- companions of the fixes (D200–D209): the other boundary, and the cases that must stay quiet ------------------

def test_2_fermium_compile_hz_parameter_given_rpm_warns():
    import fermium
    m = fermium.compile("f(freq [Hz]) = freq\ng(w [rpm]) = w\n", warnings=False)
    assert num(str(m.f(fermium.Q(60, "rpm")))) == pytest.approx(6.28319, rel=1e-5)
    assert any("Hz" in w and "rpm" in w for w in m.warnings)
    n = len(m.warnings)
    m.f(fermium.Q(60, "Hz"))                                       # Hz for Hz: nothing to say
    assert len(m.warnings) == n
    m.g(fermium.Q(1, "Hz"))
    assert any("9.5493 rpm, not 60 rpm" in w for w in m.warnings[n:])


def test_3_common_prefixed_units_next_to_your_variables_stay_quiet():
    out, err = run_err("n = 1.5\nm = 2\nk = 3 N/m\ng = 9.81 m/s^2\nlam = 500 nm\nM = 2 kg\nprint lam, M\n")
    assert out == "500 nm 2 kg"
    assert "warning" not in err
    out, err = run_err("k = 1.38e-23 J/K\nT = 300 K\nprint 1.5 k T\n")
    assert out == "6.2×10⁻²¹ J" and "warning" not in err


def test_4_bracket_of_units_after_a_spaced_slash_is_still_a_unit():
    assert run("print 9.81 kg / (m s^2)") == "9.81 kg/(m s²)"
    assert run("print 3 J / (kg K)") == "3 J/(kg K)"


def test_6_subtracting_after_the_integral_when_the_limit_is_a_unit_error_is_suggested():
    src = "v = 3 m/s\nT = 10 s\nx0 = 2 m\nprint ∫ v dt from 0 s to T - x0\n"
    with pytest.raises(FermiumError) as ei:
        run(src)
    assert "(… to T) - x0" in str(ei.value.hint)


def test_10_no_contradictory_warning_before_the_error_on_the_next_name():
    out, err = io.StringIO(), io.StringIO()
    with pytest.raises(FermiumError):
        run_source("m = 1 kg\nL = 2 [m]\nħ = 1 J s\nE1 = π^2 ħ^2 / (2 m L^2)\n", "<t>", out=out, err=err)
    assert "reading 'L' as your variable" not in err.getvalue()


def test_18_ordinary_absolute_tolerances_dont_warn():
    src = ("solve x'' = -x / (1 s)^2 with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 10 s absolute 1e-9 m\n"
           "print x(10 s) to 4 digits\n")
    out, err = run_err(src)
    assert num(out) == pytest.approx(-0.8391, abs=2e-4)
    assert "warning" not in err
