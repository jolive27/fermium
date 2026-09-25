"""Regression tests for parser frictions from the textbook gauntlet (gauntlet/FRICTION.md #7, #8, #9, #10,
#14, #15, #28)."""
import pytest

from conftest import run, error_of, warnings_of
from fermium.errors import Diagnostics
from fermium.parser import parse


def warns(src, text):
    return any(text in w for w in warnings_of(src))


PRECEDENCE = "implicit multiplication binds tighter than '/'"


# ---------------------------------------------------------------- #7: R <cos φ, sin φ, 0>
def test_scalar_times_vector_literal_by_juxtaposition():
    src = "R = 2.0 m\nring(φ) = R <cos(φ), sin(φ), 0>\nprint ring(0.0)"
    assert run(src) == "<2.0, 0, 0> m"


def test_name_times_2d_vector_literal():
    src = "v0 = 10 m/s\nθ = 0\nprint v0 <cos(θ), sin(θ)>"
    assert run(src) == "<10, 0> m/s"


def test_number_times_vector_literal_with_unit():
    assert run("print 2 <1, 0> m") == "<2, 0> m"


def test_vector_initial_condition_by_juxtaposition():
    src = ("v0 = 10 m/s\nθ = 0\n"
           "solve r'' = <0, 0> m/s² with r(0 s) = <0, 0> m, r'(0 s) = v0 <cos(θ), sin(θ)> for t from 0 s to 1 s\n"
           "print r(1 s)")
    assert run(src) == "<10, 0> m"


@pytest.mark.parametrize("src, want", [
    ("a = 1\nb = 2\nprint a < b", "true"),
    ("x = 3\ny = 4\nprint x <y", "true"),
    ("x = 3\ny = 4\nif x <y\n    print \"less\"", "less"),
    ("x = 3\ny = 4\nprint x < y and y > x", "true"),
    ("x = 3\nprint x <4 and x > 1", "true"),
    ("x = 3\nprint x <4 or x >2", "true"),
    ("x = 3\nprint x <4, x >2", "true true"),
])
def test_comparisons_with_less_than_still_work(src, want):
    assert run(src) == want


# ---------------------------------------------------------------- #8: integral upper limits
def test_spaced_division_after_upper_limit_divides_the_integral():
    src = ("I = 2.0 A\nB_axis(z) = μ₀ I (1 m)² / (2 ((1 m)² + z²)^(3/2))\n"
           "print ∫ B_axis(z) dz from -∞ m to ∞ m / (μ₀ I)")
    assert run(src) == "1.0"


def test_tight_division_stays_in_the_upper_limit():
    assert run("print ∫ x dx from 0 to 1/2") == "0.125"


def test_bracketed_division_stays_in_the_upper_limit():
    assert run("print ∫ x dx from 0 to (1 / 2)") == "0.125"
    assert run("print ∫ x dx from 0 to |1 / 2|") == "0.125"


def test_spaced_division_by_a_number_after_a_limit_warns():
    src = "print ∫ x dx from 0 to 1 / 2"
    assert run(src) == "0.25"
    assert warns(src, "divides the whole integral")
    assert warnings_of("print ∫ x dx from 0 to 1/2") == []


# ---------------------------------------------------------------- #9: a/b (c) warnings
@pytest.mark.parametrize("src", [
    "g = 9.81 m/s²\nT = 1 yr\nprint c²/g (√(1 + (g T / c)²) - 1) in ly",
    "g = 9.81 m/s²\nT = 1 yr\nprint c² / g (√(1 + (g T / c)²) - 1) in ly",
    "a = 1.0\nb = 2.0\nx = 3.0\nprint a/b x",
])
def test_ambiguous_juxtaposed_denominator_warns(src):
    assert warns(src, PRECEDENCE)


@pytest.mark.parametrize("src", [
    "n = 1.0 mol\nγ = 1.4\nT3 = 300 K\nT2 = 200 K\nprint n R_gas/(γ - 1) (T3 - T2)",
    "I = 1.0 A\ndl = 1 m\nprint μ₀ I/(4π) dl",
    "a = 1.0\nb = 2.0\nx = 3.0\nprint a / (b) x",
    "H = 2\nΩ = 0.7\nprint 2/(3 H √Ω) asinh(√(Ω/0.3))",
])
def test_bracketed_denominator_then_a_factor_is_an_error(src):
    # gauntlet #58: the brackets say the denominator ends, the precedence rule says it doesn't
    assert "is ambiguous" in str(error_of(src))


def test_divide_by_ode_unknown_warns():
    src = ("E = 1 eV\n"
           "solve ψ'' = -2 m_e E / ħ² ψ with ψ(0 nm) = 0 m^(-1/2), ψ'(0 nm) = 1 m^(-3/2) "
           "for x from 0 nm to 1 nm")
    d = Diagnostics()
    parse(src, d)       # (the program itself then fails its unit check, which is the point)
    w = [x.message for x in d.warnings if PRECEDENCE in x.message]
    assert len(w) == 1 and "unknown ψ" in w[0]


def test_ambiguous_denominator_hint_suggests_both_readings():
    e = error_of("I = 1.0 A\ndl = 1 m\nprint μ₀ I/(4π) dl")
    assert "/(4π) * dl" in e.hint and "/((4π) dl)" in e.hint


@pytest.mark.parametrize("src", [
    "a = 1.0\nb = 2.0\nx = 3.0\nprint a/(b x)",
    "M = 1 kg\nm2 = 1 kg\nr = 1 m\nprint G M m2 / r²",
    "λ = 500 nm\nT = 5000 K\nprint h c / λ k_B T",
    "x = 2.0\nprint x/2π",
])
def test_unambiguous_divisions_do_not_warn(src):
    assert not warns(src, PRECEDENCE)


def test_existing_one_half_warning_is_not_doubled():
    w = [m for m in warnings_of("x = 2.0\nprint 1/2 x") if "binds tighter" in m]
    assert len(w) == 1


# ---------------------------------------------------------------- #10: 2 L is 2 litres: say so in unit errors
def test_unit_error_after_collision_names_the_cause():
    e = error_of("T = 2.0 s\nfor t from 0 s to 1.2 T\n    print t")
    assert "'1.2 T' here is 1.2 T" in e.hint and "1.2*T" in e.hint


def test_unit_error_in_function_body_after_collision():
    e = error_of("L = 2 m\nf(x) = x + 2 L\nprint f(1 m)")
    assert "can't add" in e.message
    assert "'2 L' here is 2 L" in e.hint and "write 2*L" in e.hint


def test_no_collision_note_without_a_collision():
    e = error_of("x = 2 m\ny = 3 s\nprint x + y")
    assert "right after a number" not in (e.hint or "")


def test_collision_reading_itself_is_unchanged():
    # spec §3.4.2: a unit right after a digit literal is a unit, even when a variable has its name
    assert run("L = 3 m\nprint 2 L in L") == "2 L"


# ---------------------------------------------------------------- #14: m_π
def test_greek_symbol_subscript_in_a_name():
    assert run("m_π = 134.9768 MeV/c²\nprint m_π in MeV/c²") == "134.9768 MeV/c²"


def test_pi_subscript_is_the_same_name_as_ascii():
    assert run("m_π = 2 kg\nprint m_pi") == "2 kg"


def test_pi_alone_is_still_the_constant():
    assert run("x = 2π\nprint x") == "6.28319"


# ---------------------------------------------------------------- #15: 15.3 / min / g
def test_per_minute_per_gram_with_spaces():
    assert run("A = 15.3 / min / g\nprint A") == "15.3 1/(min g)"
    assert run("print 60 / min in Hz") == "1 Hz"


def test_min_function_still_works_after_a_number():
    assert run("print 2 / min(4, 8)") == "0.5"
    assert run("print 3/min(4, 8)") == "0.75"


def test_user_variable_named_min_is_divided_by():
    assert run("min = 4.0\nprint 8 / min") == "2.0"


# ---------------------------------------------------------------- #28: h² = …
def test_assign_to_power_suggests_solve():
    e = error_of("GM = 1.0 m³/s²\na = 1 m\nh² = GM a")
    assert "h²" in e.message
    assert "solve h² = … for h" in e.hint
    assert "==" not in e.hint


def test_assign_to_expression_mentions_compare_too():
    e = error_of("a = 1\na + b = 3")
    assert "only assign to a name" in e.hint and "==" in e.hint


def test_assign_to_derivative_suggests_solve_ode():
    e = error_of("x = 2\nx'' = 3")
    assert "differential equation" in e.hint
