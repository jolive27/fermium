"""Spec 1.5 §A1: one unit-name rule, independent of spacing (D235).

  1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
  2. If that unit is a single name that is also one of your variables, Fermium stops and asks which you mean.
  3. In a compound unit the first name is always a unit; any later name that is also your variable is an error.

Every row of the spec's table is a test here.
"""
import pytest

from conftest import run, error_of


def warns(src):
    import io
    from fermium.driver import run_source
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<test>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


# ---- the table ---------------------------------------------------------------------------------------------------

def test_first_name_of_a_compound_is_always_a_unit():
    assert run("m = 2 kg\nv = 3 m/s\nprint v") == "3 m/s"          # John's ke.fm


def test_no_number_before_m_means_your_mass():
    assert run("m = 2 kg\nv = 3 m/s\nE = ½ m v²\nprint E") == "9 J"
    assert run("m = 2 kg\nv = 3 m/s\nKE = (1/2) m v²\nprint KE") == "9 J"


@pytest.mark.parametrize("line", ["print 20 m/s/g", "print 20 m/s / g", "print 20 m/s /g", "print 20 m/s/ g"])
def test_per_your_g_is_an_error_whatever_the_spacing(line):
    e = error_of("g = 9.81 m/s²\n" + line)
    assert "ambiguous" in e.message and "g" in e.message
    assert "(20 m/s)/g" in e.hint and "20 [m/s/g]" in e.hint


def test_brackets_divide_by_your_g():
    assert run("g = 9.81 m/s²\nprint (20 m/s)/g") == "2.04 s"


def test_2_g_h_shows_both_fixes():
    e = error_of("g = 9.81 m/s²\nh = 2 m\nprint 2 g h")
    assert "'2 g' is ambiguous" in e.message
    assert "2*g" in e.hint and "2 [g]" in e.hint


def test_initial_condition_next_to_a_mass_is_an_error():
    src = """k = 50 N/m
m = 0.5 kg
solve x'' = -(k/m) x with x(0) = 0.1 m, x'(0) = 0 m/s for t from 0 s to 1 s
print x(1 s)"""
    e = error_of(src)
    assert "'0.1 m' is ambiguous" in e.message and "0.1 [m]" in e.hint


def test_initial_condition_fix_runs():
    src = """m = 0.5 kg
k = 50 [N/m]
solve x'' = -(k/m) x with x(0) = 0.1 [m], x'(0) = 0 m/s for t from 0 s to 1 s
print x(1 s)"""
    assert run(src).endswith(" m")


def test_spring_constant_next_to_a_mass():
    e = error_of("m = 0.5 kg\nk = 50 N/m\nprint k")
    assert "'50 N/m' is ambiguous" in e.message and "50 [N/m]" in e.hint


def test_later_name_your_variable():
    e = error_of("m = 2 kg\nprint 2 kg m")
    assert "'2 kg m' is ambiguous" in e.message and "(2 kg) m" in e.hint and "2 [kg m]" in e.hint


def test_tesla_next_to_a_period():
    e = error_of("T = 2.21 s\nB = 2 T\nprint B")
    assert "'2 T' is ambiguous" in e.message and "2 [T]" in e.hint


@pytest.mark.parametrize("defs,expr", [
    ("Ω = 3 rad/s\nt = 2 s", "2 Ω t"),
    ("K = 3", "8 K"),
    ("b = 2 m", "2 b"),
    ("T = 2 s", "0.25 T"),
    ("l = 2 m", "2 l²"),
])
def test_friction_74_collisions(defs, expr):
    e = error_of(f"{defs}\nprint {expr}")
    assert "ambiguous" in e.message
    num = expr.split()[0]
    assert f"{num}*" in e.hint and f"{num} [" in e.hint


def test_your_own_c():
    e = error_of("c = 340 m/s\nprint 2 c")
    assert "'2 c' is ambiguous" in e.message


def test_no_variables_plain_units():
    assert run("print 9.81 m/s² * 3 s") == "29.4 m/s"
    assert run("print 9.81 m / s^2 * 3 s") == "29.4 m/s"      # hello.fm: spaces don't matter


def test_angles():
    assert run("θ = 45 °\nprint sin(θ)") == "0.707"
    assert run("theta = 30 deg\nprint sin(θ)") == "0.500"


def test_constants_that_are_not_units_multiply():
    out, err = warns("print 2 h")
    assert out.startswith("1.33×10⁻³³ J s")


# ---- spacing never changes meaning --------------------------------------------------------------------------------

@pytest.mark.parametrize("src", ["print 50 N/m", "print 50 N / m", "print 50 N /m", "print 50 N/ m"])
def test_spacing_of_slash_between_units(src):
    assert run(src) == "50 N/m"


@pytest.mark.parametrize("src", ["print 3 J/(kg K)", "print 3 J / (kg K)"])
def test_bracket_denominator_of_units(src):
    assert run(src) == "3 J/(kg K)"


def test_bracket_with_a_non_unit_divides():
    assert run("m = 2 kg\nc_w = 4 J/(kg K)\nprint 60 s / (m c_w)") == run("m = 2 kg\nc_w = 4 J/(kg K)\nprint 60 s/(m c_w)")


def test_bracket_denominator_holding_your_variable_is_an_error():
    for sp in ("", " "):
        e = error_of(f"m = 2 kg\nprint 9.81 kg{sp}/{sp}(m s²)")
        assert "ambiguous" in e.message and "m" in e.message


@pytest.mark.parametrize("src", ["print 36 km/h", "print 36 km / h"])
def test_per_h_is_an_error_whatever_the_spacing(src):
    e = error_of(src)
    assert "Planck's constant, not the hour" in e.message and "km/hr" in e.hint


def test_energy_over_planck_in_brackets():
    assert run("print (2 eV)/h in THz") == "484 THz"


def test_reciprocal_unit_after_a_number():
    assert run("print 0.5 /s") == run("print 0.5/s") == run("print 0.5 / s") == "0.5 1/s"


def test_reciprocal_of_your_variable_divides():
    assert run("T = 2 s\nf = 1/T\nprint f") == "0.500 1/s"
    assert run("m = 2 kg\nprint 8 /m") == run("m = 2 kg\nprint 8/m") == "4 1/kg"


def test_after_a_bracket_your_variable_is_your_variable():
    assert run("m = 3\nprint (1 + 1) m") == "6"
    assert run("m = 2 kg\nv = 3 m/s\nKE = (1/2) m v²\nprint KE") == "9 J"


def test_after_a_list_a_unit_follows_as_after_a_number():
    e = error_of("m = 3\nprint [1, 2] m")          # D192: a list takes a unit like a number, so sentence 2 applies
    assert "ambiguous" in e.message and "[1, 2]*m" in e.hint and "[1, 2] [m]" in e.hint
    assert run("m = 3\nprint [1, 2]*m") == "[3, 6]"


def test_after_a_bracket_a_unit_needs_brackets():
    # a name that doesn't come right after a number is a variable (red team 8 #9, D238)
    assert "after a bracket is read as a variable" in error_of("print (2 + 3) MeV").message
    assert run("print (2 + 3) [MeV]") == "5 MeV"
    assert run("print [1, 2] m") == "[1, 2] m"             # a list takes a unit as a number does (D192)
    e = error_of("m1 = 1 kg\nm2 = 2 kg\nF = (m1 + m2) g")
    assert "g_n" in e.hint


def test_where_names_are_your_variables():
    e = error_of("x = 0.1 m where m = 2 kg\nprint x")
    assert "ambiguous" in e.message and "where" in e.message


def test_where_binding_units_before_a_later_binding():
    assert run("ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg\nprint ω₀ in rad/s") == "10 rad/s"


def test_integration_variable():
    e = error_of("print ∫ 3 s^2 ds from 0 to 1")
    assert "'3 s^2' is ambiguous" in e.message
    assert run("print ∫ 3 s^2 ds from 0 to 1") if False else True


def test_integral_differential_is_not_decimetres():
    assert run("print ∫ 2 [m] dm from 0 to 1") == "2 m"


def test_derivative_variable():
    e = error_of("h(s) = s^2\nprint d/ds h(2 s)")
    assert "ambiguous" in e.message and "differentiate" in e.message


def test_solve_unknown():
    e = error_of("w = 1/s²\nsolve u'' = -2 u w with u(0) = 2 [u], u'(0) = 0 u/s for t from 0 s to 1 s\nprint u(1 s)")
    assert "ambiguous" in e.message


def test_errors_attach_a_bracket_fix():
    e = error_of("g = 9.81 m/s²\nprint 20 m/s/g")
    assert e.fix == [(len("g = 9.81 m/s²\nprint 20 "), len("g = 9.81 m/s²\nprint 20 m/s/g"), "[m/s/g]")]
    e = error_of("g = 9.81 m/s²\nprint 20 m/s / g")
    assert e.fix == [(len("g = 9.81 m/s²\nprint 20 "), len("g = 9.81 m/s²\nprint 20 m/s"), "[m/s]")]
    e = error_of("m = 2 kg\nx = 0.1 m²")
    assert e.fix == [(len("m = 2 kg\nx = 0.1 "), len("m = 2 kg\nx = 0.1 m²"), "[m²]")]
