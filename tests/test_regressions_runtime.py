"""Regressions for audit bugs in quadrature, loops, list sizes and runtime errors (A3, A33, A34, A43, A44, A51)."""
import math
import os

import pytest

from conftest import run, error_of
from fermium.interp import run_interpreted
import io


def close(a, b, rel):
    return abs(a - b) <= rel * abs(b)


def both(src):
    """Output of the compiled program and of the reference interpreter (they must agree)."""
    out = run(src)
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    assert o.getvalue().strip() == out
    return out


@pytest.mark.parametrize("src,want", [
    ("∫ 1/sqrt(1 - x^2) dx from -1 to 1", math.pi),
    ("∫ 1/sqrt(1 - x^2) dx from 0 to 1", math.pi / 2),
    ("∫ 1/sqrt(0.25 - x^2) dx from -0.5 to 0.5", math.pi),
    ("∫ 1/sqrt(cos(x) - cos(1)) dx from -1 to 1", 4.73759822606119),
    ("∫ 1/sqrt(abs(x - 0.3)) dx from 0 to 1", 2 * (math.sqrt(0.3) + math.sqrt(0.7))),
    ("∫ (x^(-2) + 1)^0.4 dx from 0 to 1", 5.160306768626711),
    ("∫ exp(-(x-100)^2) dx from -inf to inf", math.sqrt(math.pi)),
    ("∫ exp(x) dx from -inf to 0", 1.0),
    ("∫ abs(x)^(-0.8) dx from -1 to 1", 10.0),
    ("∫ abs(x)^(-0.9) dx from -1 to 2", 10 + 10 * 2 ** 0.1),
    ("∫ ((1/x)^2 + 1)^(2/5) dx from -2 to 3", 13.731208414065762),
    ("∫ 1/(1 + x^2) dx from -inf to inf", math.pi),
])
def test_singular_and_offset_integrals(src, want):
    got = float(both(f"print {src} to 12 digits"))
    assert close(got, want, 1e-7)    # interior singularities stop at ~1e-8


@pytest.mark.parametrize("src,want", [
    ("a = 1 fm\nprint ∫ exp(-r/a) dr from 0 m to ∞ in fm to 10 digits", 1.0),
    ("a = 0.529e-10 m\nprint ∫ 4π r² exp(-2 r/a) / (π a³) dr from 0 m to ∞ to 10 digits", 1.0),
    ("σ = 1 fm\nprint ∫ exp(-x^2/(2 σ^2)) dx from -∞ to ∞ in fm to 10 digits", math.sqrt(2 * math.pi)),
    ("print ∫ exp(-t/(1 Gyr)) dt from 0 s to ∞ in Gyr to 10 digits", 1.0),
    ("a = 1 AU\nprint ∫ exp(-r/a) dr from 0 m to ∞ in AU to 10 digits", 1.0),
])
def test_infinite_integrals_at_physical_scales(src, want):
    assert close(float(both(src).split()[0]), want, 1e-8)


@pytest.mark.parametrize("src", ["print ∫ 1/x dx from 0 to 1", "print ∫ 1/x dx from 1 to inf",
                                 "print ∫ sin(x) dx from 0 to inf", "print ∫ 1/x^2 dx from -1 to 1"])
def test_divergent_integrals_still_rejected(src):
    assert "couldn't compute this integral" in str(error_of(src))


@pytest.mark.parametrize("src,msg", [
    ("xs = [1, 2, 3]\nprint xs[1e19]", "out of range"),
    ("xs = [1, 2, 3]\nprint xs[0/0]", "not NaN"),
    ("xs = [1, 2]\nxs[1e19] = 5", "out of range"),
    ("n = 0\nfor i from 1 to 0/0\n    n += 1", "no definite number of steps"),
    ("print len(zeros(inf))", "not enough memory"),
    ("xs = zeros(1e15)\nxs[1] = 2", "not enough memory"),
    ("print len(linspace(0, 1, 0/0))", "not NaN"),
])
def test_runtime_errors_have_messages(src, msg):
    assert msg in str(error_of(src))


def test_for_to_infinity_runs_until_break():
    assert both("n = 0\nfor i from 1 to inf\n    n += 1\n    if n > 5\n        break\nprint n") == "6"


def test_sort_puts_nan_last():
    assert both("print sort([3, 0/0, 1, 2])") == "[1, 2, 3, NaN]"


@pytest.mark.parametrize("src", ["for i from 1 to 0\n    print 5\nprint i",
                                 "for x in []\n    print 5\nprint x",
                                 "f(n) =\n    for i from 1 to n\n        print 1\n    i\nprint f(0)"])
def test_loop_variable_after_loop_that_may_not_run(src):
    assert "might not have a value" in str(error_of(src))


def test_loop_variable_reused_and_predeclared():
    assert run("for i from 1 to 2\n    print i\nfor i from 1 to 2\n    print i") == "1\n2\n1\n2"
    assert run("i = 0\nfor i from 1 to 3\n    n = 1\nprint i") == "3"


def test_solution_max_min_refined_between_steps():
    out = both("solve x' = cos(t) with x(0) = 0 for t from 0 to 20\nprint max(x) to 12 digits, min(x) to 12 digits")
    a, b = (float(v) for v in out.split())
    assert abs(a - 1) < 1e-8 and abs(b + 1) < 1e-8


def test_solution_max_of_time_list_is_plain():
    assert both("solve x' = cos(t) with x(0) = 0 for t from 0 to 20\nprint max(times(x))") == "20"


def test_decay_into_subnormals():
    out = both("solve x' = -x with x(0) = 1 for t from 0 to 700\nprint x(700) to 6 digits\n"
               "solve y' = -y with y(0) = 1e-300 for t from 0 to 30\nprint y(30) to 6 digits")
    assert out.split("\n") == ["9.85968×10⁻³⁰⁵", "9.35762×10⁻³¹⁴"]


def test_unit_lookalike_constant_warns():
    from conftest import warnings_of
    assert any("for hours write 2 hr" in w for w in warnings_of("print 2 h"))
    assert any("gauss" in w for w in warnings_of("x = 3 G\nprint x"))
    # (h = 2 itself warns that Planck's constant is now your variable, D213; no lookalike warning on 3 h)
    assert not [w for w in warnings_of("h = 2\nprint 3 h") if "is now your variable" not in w]
    assert not warnings_of("E = h * 1 Hz\nprint E")
    assert not warnings_of("T = 5800 K\nB(ν) = 2 h ν^3 / c^2 / (exp(h ν / (k_B T)) - 1)\nprint B(1 THz)")
    assert not warnings_of("ν = 1 Hz\nprint 2 h ν")


def test_anonymous_calculus_results_print_readably():
    assert run("print ∫ x dx") == "∫ x dx = x²/2"
    assert run("print d/dt (3 t^2)") == "d/dt (3t²) = 6t"
    assert run("F = ∫ x^2 dx\nprint F") == "F(x) = x³/3"


def test_push_on_a_loaded_column(tmp_path):
    import shutil
    shutil.copy(os.path.join(os.path.dirname(__file__), "..", "examples", "data", "pendulum.csv"), tmp_path)
    src = 'd = load "pendulum.csv"\nLs = d.L\nfor k from 1 to 1000\n    push(Ls, 1 m)\nprint len(Ls), len(d.L)'
    assert run(src, base_dir=str(tmp_path)) == "1011 11"      # A50: the column is copied, so push can grow it
