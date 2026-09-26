"""Spec 1.5 §A3: error messages John hit, and similar ones (D237)."""
import subprocess
import sys
import os

import pytest

from conftest import run, error_of

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


# ---- A3.1: ħ = c = 1 is natural units ------------------------------------------------------------------------
@pytest.mark.parametrize("src", ["ħ = c = 1", "hbar = c = 1", "hbar = c = k_B = 1"])
def test_natural_units_intent(src):
    e = error_of(src)
    assert "natural units" in e.message and f"units natural({src})" in e.hint and "==" not in e.hint


def test_chained_assignment_of_variables():
    e = error_of("a = b = 1")
    assert "one variable a value at a time" in e.message and "a = …" in e.hint


def test_the_suggested_fix_runs():
    assert run("units natural(ħ = c = 1)\nprint 1 GeV in kg") == "1.78×10⁻²⁷ kg"


# ---- A3.2: an undefined g, and no eigenvalue leak ----------------------------------------------------------------
@pytest.mark.parametrize("src", ["print g", "R = (20 m/s)^2 / g"])
def test_undefined_g(src):
    e = error_of(src)
    assert e.message == ("g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: "
                         "g = 9.81 m/s²")
    assert "eigenvalue" not in e.message + (e.hint or "")


def test_g_n_and_g_0_are_standard_gravity():
    assert run("print g_n, g_0") == "9.81 m/s² 9.81 m/s²"


def test_no_eigenvalue_hint_without_an_eigenvalue_problem():
    # the constant g_0 once made every undefined g look like an eigenvalue problem's state
    e = error_of("print G2")
    assert "eigenvalue" not in e.message + (e.hint or "")


def test_the_eigenvalue_hint_still_appears_after_an_eigenvalue_problem():
    src = """V(x) = 0 eV
solve -ħ²/(2*m_e) * ψ'' + V(x) ψ = E ψ
    with ψ(0 nm) = 0, ψ(1 nm) = 0
    for x from 0 nm to 1 nm lowest 2
print ψ"""
    e = error_of(src)
    assert "states are ψ₁" in e.message


def test_short_name_typo_does_not_suggest_a_different_function():
    e = error_of("print foo(3)")
    assert "floor" not in (e.hint or "")
    assert "did you mean floor?" in error_of("print flor(3)").hint


def test_default_hint_is_not_always_metres():
    e = error_of("print omegat2")
    assert "(with its own unit)" in e.hint


# ---- A3.3: conversions to the wrong kind of unit -----------------------------------------------------------------
@pytest.mark.parametrize("target", ["J/m", "J", "eV/nm"])
def test_h_c_in_the_wrong_unit(target):
    e = error_of(f"print h c in {target}")
    assert e.hint == "h c is energy × length; try  in J m  or  in eV nm"


def test_the_suggestions_work():
    assert run("print h c in eV nm") == "1240 eV nm"
    assert run("print h c in J m") == "1.99×10⁻²⁵ J m"


def test_a_named_kind_suggests_its_units():
    e = error_of("x = 3 m/s²\nprint x in m/s")
    assert e.hint == "x is acceleration; try  in m/s²"


# ---- A3.5: a terminal command at the Fermium prompt ------------------------------------------------------------
def test_shell_command_at_the_repl():
    p = subprocess.run([sys.executable, "-m", "fermium"], input="fermium run ke.fm\nls = 3 m\nprint ls\n:quit\n",
                       capture_output=True, text=True, cwd=ROOT, timeout=120)
    assert "This is the Fermium prompt; type :quit to go back to the terminal first." in p.stdout
    assert "3 m" in p.stdout                 # a variable called ls is not a terminal command


# ---- A3.6: review items R1–R3, R5–R7 (fixed in v1; kept as regressions) --------------------------------------
def test_R1_leibniz_second_derivative_and_R3_dx_dt_initial_condition():
    src = "k = 4 1/s²\nsolve d²x/dt² = -k x with x(0) = 1 m, dx/dt(0) = 0 m/s for t from 0 s to 1 s\n" \
          "print x(1 s) to 4 digits"
    assert run(src) == "-0.4161 m"


def test_R2_d_dt_of_dx_dt_is_second_order():
    src = "k = 4 1/s²\nsolve d/dt (dx/dt) = -k x with x(0) = 1 m, x'(0) = 0 m/s for t from 0 s to 1 s\n" \
          "print x(1 s) to 4 digits"
    assert run(src) == "-0.4161 m"


def test_R5_outside_the_range_in_physics_units():
    e = error_of("solve x' = -x/(1 s) with x(0) = 1 m for t from 0 s to 1 s\nprint x(2 s)")
    assert "at 2 s" in e.message and "ends at 1 s" in e.message


def test_R6_euler_number_hint():
    assert "for Euler's number write exp(1)" in error_of("print ∫ 1/x dx from 1 to e").hint


def test_R7_vec_with_a_unit():
    assert run("print vec(3, 4) m/s") == "<3, 4> m/s"


# ---- similar: an uncertain value in a vector (was an internal error), √ of a negative literal ------------------
def test_vector_of_uncertain_values_is_a_clear_error():
    e = error_of("L = 1.20 ± 0.01 m\nv = <1, 2> * L\nprint v")
    assert "vectors and matrices of uncertain values (±) aren't supported yet" in e.message


@pytest.mark.parametrize("src,frag", [
    ("print sqrt(-4)", "√ of a negative number (-4)"),
    ("print √(-4)", "√ of a negative number (-4)"),
    ("print log(-2)", "log of a negative number"),
    ("print factorial(-1)", "factorial of a negative whole number (-1)"),
])
def test_negative_literal_domain_errors(src, frag):
    assert frag in error_of(src).message


def test_complex_square_root_suggestion_works():
    assert run("print √(-4 + 0i)") == "0 + 2i"
