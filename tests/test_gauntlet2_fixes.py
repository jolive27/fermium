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


def test_43_unit_collision_note_reaches_later_lines():
    from conftest import error_of
    e = error_of("g = 9.81 m/s²\nh = 2 m\nv = √(2 g h)\nt = v / g\nprint t in s")
    assert "t depends on line 3, where '2 g' was read as a unit" in (e.hint or "")


def test_41_dimensionless_result_isnt_shown_in_degrees():
    assert both("θ = 60°\nf(θ) = 2 cos(θ)\nprint f(θ)") == "1"


def test_47_nabla_holds_non_coordinate_parameters_fixed():
    out = both("term(n, x, y) = sin(n π x) sinh(n π y) / sinh(n π)\nprint abs(∇²term(3, 0.2, 0.7)) < 1e-9")
    assert out == "true"
