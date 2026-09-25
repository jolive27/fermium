"""Regression tests for calculus bugs found by the adversarial pass (dev-notes/notes/bugs-adversarial.md):
A17, A18, A26, A35, A38, A49, A52."""
import math

from conftest import run, error_of
from numparse import num


def close(a, b, rel=1e-5):
    return abs(a - b) <= rel * abs(b)


# ---------------------------------------------------------------- A26: real odd roots
def test_cube_root_derivative_at_negative_x():
    assert close(num(run("f(x) = x^(1/3)\ng = f'\nprint g(-8) to 6 digits")), 1 / 12)


def test_cube_root_derivative_prints_fraction_exponent():
    assert run("f(x) = x^(1/3)\ng = f'\nprint g") == "g(x) = 1/(3x^(2/3))"


def test_odd_root_powers_of_negative_numbers():
    assert run("print (-8)^(2/3), (-8)^(1/3), (-8)^(-2/3), (-32)^(3/5)") == "4 -2 0.250 -8"
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
    assert close(num(run(src + "print g(2) to 6 digits")), want)


def test_stable_derivative_keeps_printed_form():
    assert run("f(x) = 1/(exp(x) + 1)\ng = f'\nprint g") == "g(x) = -exp(x)/(exp(x) + 1)²"


def test_derivative_of_formula_at_a_value_is_stable():
    assert num(run("x = 800\nprint d/dx (1/(exp(x) + 1))")) == 0


# ---------------------------------------------------------------- A18: d|r|/dt of a vector
def test_derivative_of_vector_length():
    out = run("r(t) = <t^2, t^3, 1>\ns(t) = |r(t)|\ng = s'\nprint g(1) to 6 digits")
    assert close(num(out), 10 / (2 * math.sqrt(3)))


def test_sign_of_a_vector_is_its_direction():
    assert run("print sign(<3, 4> m)") == "<0.600, 0.800>"


def test_derivative_of_scalar_abs_still_sign():
    assert run("f(x) = |x - 1|\ng = f'\nprint g(0), g(3)") == "-1 1"


# ---------------------------------------------------------------- A35: SymPy Piecewise
def test_indefinite_integral_with_parameter():
    out = run("ω = 2 1/s\nF = ∫ cos(ω t) dt\nprint F(1 s) - F(0 s) to 6 digits")
    assert close(num(out), math.sin(2) / 2)


def test_indefinite_integrals_generic_branch():
    assert close(num(run("k = 3\nF = ∫ exp(-k x) dx\nprint F(1) - F(0) to 6 digits")), (1 - math.exp(-3)) / 3)
    assert close(num(run("a = 2\nF = ∫ x^a dx\nprint F(3) - F(0)")), 9)


def test_indefinite_integral_of_abs():
    assert close(num(run("F = ∫ abs(x) dx\nprint F(2) - F(-1)")), 2.5)
    assert run("F = ∫ |x| dx\nprint F") == "F(x) = if x <= 0 then -x²/2 else x²/2"


def test_indefinite_integral_is_printed_with_its_name():
    assert run("F = ∫ x^2 dx\nprint F") == "F(x) = x³/3"


# ---------------------------------------------------------------- A38: fmt --ascii ∂²
def test_parser_accepts_ascii_second_partial():
    assert run("f(x, y) = x^2 y^2\ng = partial^2/partial x^2 f\nprint g(1, 2)") == "8"


def test_fmt_ascii_second_partial_round_trips():
    from fermium.fmt import format_source
    src = "f(x, y) = x^2 y^2\ng = ∂²/∂x² f\nprint g(1, 2)"
    assert run(format_source(src, "ascii")) == run(src) == "8"
    assert format_source(format_source(src, "ascii"), "pretty") == src.replace("^2", "²")


def test_second_partial_orders_must_match():
    assert "orders don't match" in str(error_of("f(x, y) = x^2 y^2\ng = ∂/∂x² f\nprint g(1, 2)"))


# ---------------------------------------------------------------- A17: `2 dm` as the differential
def test_constant_integrand_with_unit_like_differential():
    assert run("print ∫ 2 dm from 0 kg to 1 kg") == "2 kg"
    assert run("print ∫ 1 dV from 0 m^3 to 2 m^3") == "2 m³"
    assert run("print ∫ 3 dT from 0 K to 2 K") == "6 K"


def test_unit_like_names_still_units_outside_integrals():
    assert run("x = 2 dm\nprint x in m") == "0.200 m"
    assert run("print ∫ 2 dm dx from 0 to 1") == "0.200 m"
    assert run("print ∫ 2 m dm from 0 kg to 1 kg") == "2 kg m"


# ---------------------------------------------------------------- A49: d/dt (...) where ...
def test_derivative_of_formula_with_where():
    assert run("g = d/dt (a t^2) where a = 3\nprint g, g(1)") == "g(t) = 6t 6"
    assert run("g = d/dt (a t^2) where a = 3 m\nprint g(2 s)") == "12 m s"


def test_derivative_with_where_binding_the_variable_is_a_value():
    assert run("print d/dt (a t^2) where a = 3, t = 2") == "12"


def test_where_without_derivative_unchanged():
    assert run("y = a + 1 where a = 3\nprint y") == "4"
    assert "g is a function" in str(error_of("g = d/dt (t^2)\nprint g + 1"))
