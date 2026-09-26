"""solve lhs = rhs for x from a to b: algebraic equations (root finding)."""
import io
import math

import pytest

from conftest import run, error_of
from fermium.interp import run_interpreted


def both(src):
    out = run(src)
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    assert o.getvalue().strip() == out
    return out


@pytest.mark.parametrize("eq,rng,want", [
    ("x² = 2", "0 to 2", math.sqrt(2)),
    ("cos(x) = x", "0 to 10", 0.7390851332151607),
    ("exp(-x) = x", "0 to 1", 0.5671432904097838),          # the omega constant
    ("x³ - 2x - 5 = 0", "2 to 3", 2.0945514815423265),       # Wallis's cubic
    ("sin(x) = 0", "3 to 4", math.pi),
    ("x = 0", "-1 to 1", 0.0),
])
def test_roots_to_full_precision(eq, rng, want):
    got = float(both(f"solve {eq} for x from {rng}\nprint x to 17 digits"))
    assert got == pytest.approx(want, rel=4e-15, abs=1e-300)


def test_first_sign_change_is_found_by_scanning():
    # the ends have the same sign; the first root after 0.1 is ~1.39547 (finite square well, z0 = 8)
    got = float(both("solve tan(z) = √((8/z)² - 1) for z from 0.1 to 1.5\nprint z to 12 digits"))
    from scipy.optimize import brentq
    ref = brentq(lambda z: math.tan(z) - math.sqrt((8 / z) ** 2 - 1), 0.1, 1.5, xtol=1e-15)
    assert got == pytest.approx(ref, rel=1e-11)


def test_units_and_display_unit():
    out = both("L = 1 m\nsolve L tan(θ) = 2 m for θ from 0 to 1.5\nprint θ to 6 digits\n"
               "g = 9.81 m/s²\nsolve g t²/2 = 20 m for t from 0 s to 10 s\nprint t to 6 digits\n"
               "solve E² = (3 MeV)² + (4 MeV)² for E from 0 MeV to 10 MeV\nprint E")
    assert out.split("\n") == ["1.10715", "2.01928 s", "5 MeV"]


def test_inside_a_loop_and_with_functions():
    src = ("f(x, a) = x² - a\nfor a from 1 to 3\n    solve f(r, a) = 0 for r from 0 to 5\n    print r to 10 digits")
    got = [float(v) for v in both(src).split()]
    assert got == pytest.approx([1, math.sqrt(2), math.sqrt(3)], rel=1e-9)


@pytest.mark.parametrize("src,msg", [
    ("solve exp(q) = 0 for q from 0 to 1", "no solution between 0 and 1"),
    ("solve tan(x) = 0.5 for x from 1 to 2", "jump past each other"),
    ("solve x = 2 m for x from 0 to 5", "don't match"),
    ("solve x = 2 for x from 0 s to 5", "both ends need the same units"),
    ("solve x = 2 for x from 0 to 5 step 0.1", "full precision"),
])
def test_errors(src, msg):
    assert msg in str(error_of(src))


def test_error_values_in_units():
    assert "no solution between 0 m and 1 m" in str(error_of("solve x = 5 m for x from 0 m to 1 m"))
