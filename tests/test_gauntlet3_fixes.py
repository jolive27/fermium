"""Fixes for gauntlet pass 3 friction rows #66–#79 (gauntlet/FRICTION.md; DECISIONS D170–D174).

Every program runs on both back ends (the LLVM JIT and the reference interpreter), which must agree."""
import io

import pytest

from conftest import run, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a = run(src)
    assert interp(src) == a
    return a


def both_error(src):
    with pytest.raises(FermiumError) as e1:
        run(src)
    with pytest.raises(FermiumError) as e2:
        interp(src)
    assert e1.value.message == e2.value.message
    return e1.value


# ---- #66: `2 m c²` with your own m was 2 × the compound unit m·c² (D170) ------------------------------------

@pytest.mark.parametrize("src,shown", [
    ("m = 2 kg\nprint 1 J / (2 m c²)", "2 m c²"),
    ("m = 2 kg\nprint 1 J / (2 m * c²)", "2 m * c²"),
    ("m = 2 kg\nE = 2 m c² + 1 J", "2 m c²"),
    ("N = 3\nprint 2 N m", "2 N m"),
])
def test_66_compound_unit_starting_with_your_variable_is_an_error(src, shown):
    e = both_error(src)
    assert f"'{shown}' is ambiguous" in e.message
    assert "your variable" in e.message
    assert "*" in e.hint and "[" in e.hint


@pytest.mark.parametrize("src,out", [
    ("m = 2 kg\nprint 1 J / (2*m c²) to 3 digits", "2.78×10⁻¹⁸"),
    ("m = 2 kg\nprint 1 J / (2 [m c²]) to 3 digits", "5.56×10⁻¹⁸ kg/m"),
    ("m = 2 kg\ng = 9.81 m/s²\nprint g", "9.81 m/s²"),                 # a '/' compound stays the unit (D7)
    ("print 2 m c² to 3 digits", "2.00 m c²"),                         # no variable m: the unit
    ("print 2 kg m", "2 kg m"),                                        # kg isn't your variable
])
def test_66_unambiguous_forms_are_unchanged(src, out):
    assert both(src) == out


# ---- #67: a spaced + or - after an integral's upper limit is part of the limit (D173) -----------------------

def test_67_minus_after_the_upper_limit_warns_and_says_it_is_in_the_limit():
    src = "print 2 ∫ x dx from 0 to 1 - π to 3 digits"
    assert both(src) == "4.59"                     # the parse is unchanged: the limit is 1 - π
    ws = warnings_of(src)
    assert any("' - π' is part of the upper limit" in w and "up to 1 - π" in w for w in ws)


def test_67_plus_warns_too():
    assert any("' + 1' is part of the upper limit" in w for w in warnings_of("L = 2\nprint ∫ x dx from 0 to L + 1"))


@pytest.mark.parametrize("src,out", [
    ("print (2 ∫ x dx from 0 to 1) - π to 3 digits", "-2.14"),
    ("print 2 ∫ x dx from 0 to (1 - π) to 3 digits", "4.59"),
    ("print ∫ x dx from 0 to 1-0.5 to 3 digits", "0.125"),              # no spaces: clearly the limit
    ("print ∫ x dx from 0 - 1 to 2 to 3 digits", "1.50"),               # the lower limit ends at 'to'
    ("print ∫ x dx from 0 to 2 * -1 to 3 digits", "2.00"),               # a sign, not a sum
    ("print Σ(k for k from 1 to 5 - 1)", "10"),                         # Σ's limits are in brackets
])
def test_67_no_warning_when_the_limit_is_clear(src, out):
    assert both(src) == out
    assert not any("part of the upper limit" in w for w in warnings_of(src))


# ---- #68 and #72: `/unit` and `/(units)` right after a number (D171) ----------------------------------------

def test_68_slash_unit_after_a_number_colliding_with_your_variable_is_an_error():
    e = both_error("m = 2 kg\nn = 8 /m³\nprint n")
    assert "'8 /m³' is ambiguous" in e.message
    assert "8 [1/m³]" in e.hint and "8/m³" in e.hint


@pytest.mark.parametrize("src,out", [
    ("n = 8 /m³\nprint n", "8 1/m³"),                                 # no variable m: the unit
    ("m = 2 kg\nprint 8/m³", "1 1/kg³"),                              # no spaces: divides by your m
    ("m = 2 kg\nprint 8 / m³", "1 1/kg³"),                            # spaced: divides by your m
    ("m = 2 kg\nprint 8 [1/m³]", "8 1/m³"),
    ("m = 2 kg\nprint 8 m⁻³", "8 1/m³"),
])
def test_68_the_clear_forms(src, out):
    assert both(src) == out


@pytest.mark.parametrize("src,out", [
    ("a = 0.300 /(m s²)\nprint a", "0.300 1/(m s²)"),
    ("a = 0.300/(m s^2)\nprint a", "0.300 1/(m s²)"),
    ("print 2 /(kg m)^2", "2 1/(kg² m²)"),
    ("z = 1\nprint 2 /(1 + z)", "1"),                                 # not a unit: a division
    ("k = 2 J/K\nT = 300 K\nprint 600 /(k T) * 1 J", "1"),              # k isn't a unit
])
def test_72_bracketed_unit_denominator_after_a_number(src, out):
    assert both(src) == out


def test_72_bracketed_denominator_naming_your_variable_is_an_error():
    e = both_error("m = 1 kg\na = 0.300 /(m s²)")
    assert "'0.300 /(m s²)' is ambiguous" in e.message
    assert "0.300 [1/(m s²)]" in e.hint


def test_72_tight_bracketed_denominator_with_your_variable_divides():
    assert both("m = 2 kg\ns = 1 s\nprint 4/(m s)") == "2 1/(kg s)"


# ---- #71: differentiating through a multi-line function: where it was asked for -------------------------------

def test_71_error_through_an_indirect_call_has_both_lines():
    src = "F(x) =\n    a = 2\n    x^2 / a\nN(T) = F(T) + 1\ndN = d/dT N\nprint dN(3)"
    e = both_error(src)
    assert "can't differentiate through F" in e.message
    assert e.line == 4 and e.col is not None
    assert "derivative of N on line 5" in e.hint


# ---- #76: Ωπ = … ----------------------------------------------------------------------------------------------

def test_76_omega_pi_is_read_as_omega_times_pi():
    e = both_error("Ωπ = 2")
    assert "read as Ω × π" in e.message
    assert "Ω_π" in e.hint
    assert "solve" not in e.hint


def test_76_omega_underscore_pi_is_a_name():
    assert both("Ω_π = 2 rad/s\nprint Ω_π") == "2 rad/s"


# ---- #77: the 2022 prefixes (D174) ------------------------------------------------------------------------------

@pytest.mark.parametrize("name", ["rg", "rs", "rm", "RC", "RL", "RT", "Rs", "qV", "qm", "Qs"])
def test_77_2022_prefixes_do_not_turn_short_names_into_units(name):
    from fermium.units import is_unit_name
    assert not is_unit_name(name)


def test_77_ronnagram_and_quettagram_remain():
    assert both("print 5.97 Rg in kg to 3 digits") == "5.97×10²⁴ kg"
    assert both("print 1.90 Qg in kg to 3 digits") == "1.90×10²⁷ kg"


def test_77_a_parameter_named_rg_is_just_a_variable():
    assert both("rg = 1.48 km\nprint 2 * rg / 1 km to 3 digits\nf(rg) = 2 rg\nprint f(3) to 3 digits") == \
        "2.96\n6.00"


# ---- #78: a function named integral --------------------------------------------------------------------------

@pytest.mark.parametrize("src", ["integral(b, T) = b T", "x = integral(1, 2)"])
def test_78_integral_is_the_ascii_spelling_of_the_integral_sign(src):
    e = both_error(src)
    assert "integral is the ASCII spelling of ∫" in e.message
    assert "pick another name" in e.message


def test_78_integral_still_integrates():
    assert both("print integral(x^2) dx from 0 to 1 to 3 digits") == "0.333"


# ---- #79: `in M_sun` ---------------------------------------------------------------------------------------------

def test_79_in_M_sun_suggests_the_unit():
    e = both_error("M = 2e30 kg\nprint M in M_sun")
    assert "'M_sun' is not a unit" in e.message
    assert "M☉" in e.hint and "Msun" in e.hint


def test_79_m_sun_suggests_M_sun_not_R_sun():
    assert "did you mean M_sun?" in both_error("print 2 m_sun").hint
    assert "M☉" in both_error("M = 2e30 kg\nprint M in m_sun").hint
