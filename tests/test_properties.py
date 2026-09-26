"""Property tests (hypothesis): dimension algebra laws, unit conversion round-trips, and
fmt --pretty/--ascii round-trips on generated programs."""
from fractions import Fraction

from hypothesis import given, settings, strategies as st, HealthCheck

from conftest import run
from numparse import num
from fermium.units import Dim, DIMLESS, parse_unit_string, lookup_unit, format_dim
from fermium.types import DExpr, Unifier
from fermium.fmt import format_source

exps = st.fractions(min_value=-4, max_value=4, max_denominator=3)
dims = st.lists(exps, min_size=7, max_size=7).map(Dim)
powers = st.fractions(min_value=-3, max_value=3, max_denominator=4)


@given(dims, dims, dims)
def test_dimension_product_is_associative_and_commutative(a, b, c):
    assert (a * b) * c == a * (b * c)
    assert a * b == b * a
    assert a * DIMLESS == a


@given(dims, dims)
def test_division_inverts_multiplication(a, b):
    assert (a * b) / b == a
    assert a / a == DIMLESS


@given(dims, powers, powers)
def test_power_laws(a, p, q):
    assert (a ** p) ** q == a ** (p * q)
    assert (a ** p) * (a ** q) == a ** (p + q)


@given(dims)
def test_format_dim_round_trips_through_the_unit_parser(a):
    text = format_dim(a)
    if text:
        assert parse_unit_string(text).dim == a


@given(dims, dims)
def test_unifier_solves_linear_equations(a, b):
    """x * a == b  has the solution x = b / a."""
    U = Unifier()
    x = DExpr.fresh("x")
    assert U.unify(x * DExpr.of(a), DExpr.of(b))
    assert U.resolve(x) == b / a


@given(dims, dims)
def test_unifier_rejects_mismatches(a, b):
    U = Unifier()
    assert U.unify(DExpr.of(a), DExpr.of(b)) == (a == b)


UNITS = ["m", "km", "cm", "mm", "μm", "nm", "fm", "Å", "AU", "ly", "pc", "inch", "ft", "mi",
         "s", "ms", "min", "hr", "day", "yr", "kg", "g", "u", "M☉", "lb", "J", "eV", "MeV", "keV", "erg", "cal",
         "N", "dyn", "Pa", "bar", "atm", "Torr", "W", "hp", "K", "°", "rad", "Hz", "b", "C", "V", "T", "gauss"]
unit_pairs = st.sampled_from(UNITS).flatmap(
    lambda u: st.tuples(st.just(u), st.sampled_from([v for v in UNITS if lookup_unit(v).dim == lookup_unit(u).dim])))


@settings(max_examples=40, deadline=None, suppress_health_check=[HealthCheck.too_slow])
@given(unit_pairs, st.floats(min_value=1e-3, max_value=1e3))
def test_conversion_round_trip(pair, x):
    """x [u] converted to v and back is x again (to 12 digits)."""
    u, v = pair
    out = run(f"a = {x!r} [{u}]\nb = a in {v}\nprint b / (1 [{v}]) to 15 digits\nprint (a in {v}) in {u} to 15 digits")
    via, back = out.split("\n")
    assert abs(num(back) - x) <= 1e-12 * abs(x)
    ratio = lookup_unit(u).factor / lookup_unit(v).factor
    assert abs(num(via) - x * ratio) <= 1e-12 * abs(x * ratio)


# ---- fmt round trips on generated programs
names = st.sampled_from(["x", "theta", "omega_0", "v", "L", "E", "alpha", "k_B T", "hbar"])
ops = st.sampled_from([" + ", " - ", " * ", " / "])
atoms = st.one_of(names, st.sampled_from(["2", "pi", "sqrt(x)", "x^2", "3 m", "sin(theta)", "(x + 1)"]))


@st.composite
def formulas(draw):
    n = draw(st.integers(1, 4))
    parts = [draw(atoms)]
    for _ in range(n):
        parts += [draw(ops), draw(atoms)]
    return "".join(parts)


@settings(max_examples=60, deadline=None)
@given(formulas())
def test_fmt_round_trip_is_identity_on_ascii(f):
    src = f"y = {f}\n"
    pretty = format_source(src, "pretty")
    assert format_source(pretty, "ascii") == format_source(format_source(pretty, "ascii"), "ascii")
    back = format_source(pretty, "ascii")
    assert format_source(back, "pretty") == pretty


def test_fraction_exponent_units():
    assert run("print (8 m³)^(1/3)") == "2 m"
    assert run("A = 64\nprint 1.2 [fm] A^(1/3)") == "4.8 fm"
    assert Fraction(1, 3)
