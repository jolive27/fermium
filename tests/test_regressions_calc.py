"""Regression tests for calculus bugs found by the adversarial pass (notes/bugs-adversarial.md):
A17, A18, A26, A35, A38, A49, A52."""
import math

from conftest import run
from numparse import num


def close(a, b, rel=1e-5):
    return abs(a - b) <= rel * abs(b)


# ---------------------------------------------------------------- A26: real odd roots
def test_cube_root_derivative_at_negative_x():
    assert close(num(run("f(x) = x^(1/3)\ng = f'\nprint g(-8)")), 1 / 12)


def test_cube_root_derivative_prints_fraction_exponent():
    assert run("f(x) = x^(1/3)\ng = f'\nprint g") == "g(x) = 1/(3x^(2/3))"


def test_odd_root_powers_of_negative_numbers():
    assert run("print (-8)^(2/3), (-8)^(1/3), (-8)^(-2/3), (-32)^(3/5)") == "4 -2 0.25 -8"
    assert run("x = -8\nprint x^(2/3), x^(1/5) < 0") == "4 true"
    assert run("f(x) = x^(2/3)\nprint f(-8)") == "4"


def test_even_roots_of_negative_numbers_stay_nan():
    assert run("x = -4\nprint x^(1/2), x^(3/2), x^0.25") == "NaN NaN NaN"


# ---------------------------------------------------------------- A52: ∞/∞ in derivatives
def test_fermi_function_derivative_far_tail_is_zero():
    src = "kT = 0.025 eV\nμ = 5 eV\nf(E) = 1/(exp((E - μ)/kT) + 1)\ng = f'\nh = f''\n"
    for x in ("g(30 eV) in 1/eV", "g(-30 eV) in 1/eV", "h(30 eV) in 1/eV^2", "h(-30 eV) in 1/eV^2"):
        assert num(run(src + f"print {x}")) == 0
    # and the value near μ is unchanged: -1/(4 kT) at E = μ
    assert close(num(run(src + "print g(5 eV) in 1/eV")), -10)


def test_tanh_and_sech_derivatives_at_large_argument():
    assert run("f(x) = tanh(x)\nh = f''\nprint h(1000), h(-1000)") in ("0 0", "-0 0", "0 -0", "-0 -0")
    assert num(run("f(x) = 1/cosh(x)\nprint f'(800)")) == 0
    assert close(num(run("f(x) = tanh(x)\nh = f''\nprint h(0.5)")), -2 * math.tanh(0.5) / math.cosh(0.5) ** 2,
                 1e-2)


def test_planck_derivative_is_finite_everywhere():
    src = "f(x) = x^3/(exp(x) - 1)\ng = f'\n"
    assert num(run(src + "print g(800)")) == 0
    want = (3 * 4 * (math.exp(2) - 1) - 8 * math.exp(2)) / (math.exp(2) - 1) ** 2
    assert close(num(run(src + "print g(2)")), want)


def test_stable_derivative_keeps_printed_form():
    assert run("f(x) = 1/(exp(x) + 1)\ng = f'\nprint g") == "g(x) = -exp(x)/(exp(x) + 1)²"


def test_derivative_of_formula_at_a_value_is_stable():
    assert num(run("x = 800\nprint d/dx (1/(exp(x) + 1))")) == 0
