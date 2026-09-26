"""Spec 1.5 §A2 (friction #73, D236): a fraction of pure numbers is a single coefficient."""
import pytest

from conftest import run, error_of, warnings_of


@pytest.mark.parametrize("src,out", [
    ("x = 3\nprint 73/24 x²", "27.4"),                       # (73/24)·x², not 73/(24 x²)
    ("t = 2\nprint π²/12 t² to 4 digits", "3.290"),
    ("t = 2\nprint π⁴/80 t⁴ to 4 digits", "19.48"),
    ("print 1/2 kg", "0.500 kg"),                           # was 0.5 1/kg: a deliberate change (CHANGES_1.5.md)
    ("x = 4\nprint 1/2 x", "2"),
    ("x = 3\nprint 2/3 x^2", "6"),
    ("k = 50 N/m\nm = 0.5 kg\nprint 1/(2π) √(k/m)", "1.6 1/s"),     # a bracketed pure number counts too
    ("print h / m_e 3 m/s", "0.000242 m"),                  # unchanged: the denominator starts with a name (D8)
    ("h_ = 2 m\nprint 2 m / h_ 4", "0.250"),
    ("print 0.04 / 1 s", "0.040 1/s"),                       # per 1 s: dividing by one is never a coefficient
    ("print 1e4 / 1 s", "10000 1/s"),
])
def test_coefficients(src, out):
    assert run(src) == out


def test_no_precedence_warning_for_a_coefficient():
    assert warnings_of("x = 3\ny = 73/24 x²\nz = 1/2 x") == []


def test_one_half_m_v_squared_with_a_mass_asks_and_suggests_one_half():
    e = error_of("m = 2 kg\nv = 3 m/s\nprint 1/2 m v²")
    assert "'2 m' is ambiguous" in e.message and "½ m" in e.hint


@pytest.mark.parametrize("src,shown", [
    ("r = 2 m\nprint 4/3 π r³", "4/3 π"),              # a sphere: (4/3)·π r³ ...
    ("k = 50 N/m\nm = 0.5 kg\nprint 1/2π √(k/m)", "1/2 π"),     # ... and 1/(2π): the same shape
])
def test_a_pure_number_after_the_denominator_asks(src, shown):
    e = error_of(src)
    assert f"'{shown}' is ambiguous" in e.message and "(" in e.hint


def test_the_two_readings_written_out():
    assert run("r = 2 m\nprint (4/3) π r³ to 4 digits") == "33.51 m³"
    assert run("r = 2 m\nprint 4/(3 π) r³ to 4 digits") == "3.395 m³"
