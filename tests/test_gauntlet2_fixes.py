"""Fixes for bugs found by the second gauntlet pass (IDs from gauntlet/<topic>/FRICTION.md)."""
import io

from conftest import run
from fermium.interp import run_interpreted


def both(src):
    out = run(src)
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    assert o.getvalue().strip() == out
    return out


def test_E9_nested_integral_captures_the_function_parameter():
    assert both("f(z) = ∫ (∫ z dφ from 0 to 1) ds from 0 to 1\nprint f(2)") == "2"
    assert both("f(z, w) = ∫ (∫ z s + w φ dφ from 0 to 1) ds from 0 to 1\nprint f(2, 3)") == "2.5"
    assert both("g(a) = ∫ (∫ (∫ a dx from 0 to 1) dy from 0 to 1) dz from 0 to 1\nprint g(5)") == "5"


def test_A5_first_step_follows_the_solution_scale():
    # w = √(t/t0) w0 grows by 10⁵ from t0 = 1e-10 s; the old first step (10⁻⁴ of the range) gave 6.17
    out = both("solve w' = w/(2t) with w(1e-10 s) = 1e-5 for t from 1e-10 s to 1 s\nprint w(1 s) to 8 digits")
    assert abs(float(out) - 1) < 1e-6
