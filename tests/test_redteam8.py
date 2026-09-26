"""Findings of the independent red-team review, round 8 (dev-notes/REDTEAM.md, "Round 8"): the v1.5 unit rule
(D235), fraction coefficients (D236) and messages (D237).  Each test names its finding; fixes are D238."""
import pytest

from conftest import run, error_of, warnings_of
from fermium.fmt import fix_source, fix_source_report


# ---- #1, #2: a number over a number-with-unit is a rate unless both are whole numbers ---------------------------
@pytest.mark.parametrize("src,out", [
    ("print 1 / 0.5 s", "2.0 1/s"),
    ("print 2π / 0.5 s", "13 1/s"),
    ("print 1 / 2.2 μs", "4.5×10⁵ 1/s"),
    ("print 1/2 kg", "0.500 kg"),            # the spec's deliberate change: a written fraction
    ("print 1/2 [kg]", "0.500 kg"),          # brackets are units: the same (#2)
    ("print 3/4 kg", "0.750 kg"),
])
def test_1_rates_and_fractions(src, out):
    assert run(src) == out


def test_1_decay_constant_keeps_its_units():
    assert run("print 0.693 / 5730 yr in 1/yr to 3 digits") == run("print ln(2) / 5730 yr in 1/yr to 3 digits")


def test_1_pi_denominators_are_consistent():
    assert run("x = 2\nprint 1/π x to 3 digits") == "0.637"
    assert run("x = 2\nprint 1/π² x to 3 digits") == "0.203"


# ---- #3, #13: fmt --fix keeps a meaning only where Fermium 1 had one ----------------------------------------------
def test_3_fix_leaves_combined_collisions_for_the_writer():
    new, n, left = fix_source_report("m = 2 kg\nv = 3 m/s\nprint 1/2 m v²\nE = 2 m c²\n")
    assert n == 0 and "1/2 m v²" in new and left is not None and "ambiguous" in left.message


def test_13_fix_brackets_the_whole_unit():
    assert fix_source("m = 2 kg\np = 2 kg m/s\n")[0] == "m = 2 kg\np = 2 [kg m/s]\n"


# ---- #4: a number that is itself a denominator takes no reciprocal unit --------------------------------------------
@pytest.mark.parametrize("src", ["print 3 m / 2 / s", "x = 6 m\nprint x / 2 / s", "print 3 m/2/s"])
def test_4_no_reciprocal_unit_below_the_line(src):
    assert "s isn't defined" in error_of(src).message


def test_4_reciprocal_unit_still_works_after_a_numerator():
    assert run("print 0.5 /s") == "0.5 1/s"


# ---- #5, #6: powers of ten and vectors follow the rule ------------------------------------------------------------
@pytest.mark.parametrize("src", ["m = 2 kg\nprint 10^-2 m", "m = 2 kg\nprint 1.5 × 10⁻² m", "T = 2 s\nB = 10^-4 T",
                                 "m = 2 kg\nr = <1, 0> m", "T = 2 s\nB = <0, 0, 0.25> T"])
def test_5_6_powers_and_vectors_ask(src):
    assert "ambiguous" in error_of(src).message


def test_5_power_without_a_collision():
    assert run("print 10^-2 m") == "0.0100 m"


# ---- #7: a where-binding sees the bindings before it ---------------------------------------------------------------
@pytest.mark.parametrize("src", ["F = w * 1 where g = 9.81 m/s², w = 2 g", "z = B where T = 2 s, B = 0.25 T"])
def test_7_where_bindings(src):
    assert "ambiguous" in error_of(src).message


# ---- #8: a unit continues through spaces, '/' and '·'; an explicit * multiplies ------------------------------------
def test_8_explicit_star_multiplies():
    assert run("m = 2 kg\nprint 5 N*m") == "10 N kg"
    assert "m is a unit" in error_of("print 5 N*m").hint
    assert run("print 2 N·m") == "2 N m"
    assert "ambiguous" in error_of("m = 2 kg\nprint 2 N·m").message


# ---- #9: a unit after a bracket needs brackets ---------------------------------------------------------------------
def test_9_after_a_bracket():
    e = error_of("m1 = 1 kg\nm2 = 2 kg\nF = (m1 + m2) g")
    assert "after a bracket is read as a variable" in e.message and "g_n" in e.hint
    assert run("N = 2\nprint (N + 3) [MeV]") == "5 MeV"
    assert fix_source("N = 2\nx = (N + 3) MeV\n")[0] == "N = 2\nx = (N + 3) [MeV]\n"
    assert run("print (3/4) kg") == "0.750 kg" and run("print ½ kg") == "0.500 kg"      # plain numbers


def test_9_integral_differential_after_a_bracket():
    assert run("print ∫ (s + 1) ds from 0 to 1") == "1.50"


# ---- #12: J/kg K warns --------------------------------------------------------------------------------------------
def test_12_only_the_first_name_is_below_the_line():
    ws = warnings_of("c = 4186 J/kg K")
    assert any("has only kg below the line" in w for w in ws)
    assert warnings_of("c = 4186 J/(kg K)") == []


# ---- #14, #16, #17, #19, #20, #21: messages -----------------------------------------------------------------------
def test_14_geometrized_units_hint():
    e = error_of("units natural(G = c = 1)\nprint 1 kg in 1/MeV")
    assert "a mass, a length and a time" in e.hint


def test_16_named_kind_first():
    assert error_of("x = 3 m/s\nprint x^2 in J").hint == "x² is specific energy; try  in J/kg"


def test_16_plain_numbers_get_no_kind():
    assert "time × frequency" not in (error_of("T = 2 in ft/s^2").hint or "")


def test_17_your_own_h():
    e = error_of("h = 20 m\nprint 36 km/h")
    assert "h is your variable here" in e.message and "Planck" not in e.message


def test_19_typos_of_short_names():
    assert "did you mean temp?" in error_of("temp = 3\nprint tmep").hint
    assert "floor" not in (error_of("print foo(3)").hint or "")


def test_20_negative_literal_with_a_unit():
    e = error_of("print √(-4 m²)")
    assert "√ of a negative number (-4 m²)" in e.message and "0i" not in e.hint


def test_21_hint_keeps_the_exponent():
    e = error_of("w = 1/s²\nsolve x'' = -w x + 9.81 m/s² with x(0) = 0 [m], x'(0) = 0 m/s for s from 0 to 1\n"
                 "print x(1)")
    assert "(9.81 m)/s²" in e.hint


def test_21_list_compound_unit_starting_with_your_variable():
    assert run("m = 2 kg\nprint [1, 2] m/s") == "[1, 2] m/s"


def test_15_integral_hint_is_valid_code():
    e = error_of("print ∫ 3 m/s ds from 0 to 2")
    assert "3 [m/s]" in e.hint and "ds]" not in e.hint
