"""Spec A8.1 (D260): `≈` is Julia's isapprox, |a − b| ≤ max(atol, rtol·max(|a|, |b|)); `within` gives an
explicit tolerance; comparing with a literal zero and no absolute tolerance is a compile error with the fix.
The compiled code, the interpreter and `fermium build` agree."""
import io
import os
import subprocess

import pytest

from conftest import run, error_of
from fermium.aot import build, find_cc
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    """The JIT's output, after checking that the interpreter prints the same."""
    got = run(src)
    assert interp(src) == got
    return got


CASES = [
    # the old near-zero failure, now with an explicit absolute tolerance
    ("v = 1e-12 m/s\nprint v ≈ 0 m/s within 1e-9 m/s", "true"),
    ("v = 1e-12 m/s\nprint v ≈ 0 m/s within 1e-13 m/s", "false"),
    ("v = 1e-12 m/s\nprint v ~= 0 m/s within 1 nm/s", "true"),              # ASCII spelling; units converted
    ("print 0 m/s ≈ -3e-10 m/s within 1e-9 m/s", "true"),                   # zero on the left
    # the default: 10⁻⁶ relative, no absolute part (D21 unchanged away from zero)
    ("print 1 m ≈ 100 cm, 1 m ≈ 1.1 m", "true false"),
    ("print 1 ≈ 1.0000009, 1 ≈ 1.0000011", "true false"),
    ("print 1e-300 ≈ 1.0000001e-300, 1e-300 ≈ 2e-300", "true false"),     # relative at any scale
    ("print 0 ≈ 0, 0 m ≈ 0 m", "true true"),                               # both sides zero: exactly equal
    # `within` states the whole tolerance: an absolute one drops the relative part (like Julia's rtol = 0)
    ("print 1000 m ≈ 1000.0005 m within 1e-9 m", "false"),
    ("print 1000 m ≈ 1000.0005 m within 1 mm", "true"),
    ("print 2 m ≈ 2.001 m within 1 mm, 2 m ≈ 2.0011 m within 1 mm", "true false"),
    # a percentage is relative
    ("print 100 ≈ 101 within 2%, 100 ≈ 103 within 2%", "true false"),
    ("print 100 m ≈ 101 m within 2 percent, 1 km ≈ 1.03 km within 2%", "true false"),
    # ... unless the values themselves are percentages (then 1 % is one percentage point, as with ±)
    ("η = 50 %\nprint η ≈ 50.5 % within 1 %, η ≈ 52 % within 1 %", "true false"),
    # infinities and NaN (Julia: ∞ ≈ ∞, never ∞ ≈ finite; NaN ≈ nothing)
    ("print ∞ ≈ ∞, ∞ ≈ 1e308, -∞ ≈ ∞", "true false false"),
    ("x = 0/0\nprint x ≈ x, x ≈ 1 within 1e9", "false false"),
    # vectors: norms, like Julia's isapprox on arrays
    ("print <1, 2> m ≈ <1, 2.000001> m, <1, 2> m ≈ <1, 2.1> m", "true false"),
    ("print <1e-12, 0, 0> m ≈ <0, 0, 0> m within 1 nm", "true"),
    ("print <3, 4> m ≈ <3.003, 4> m within 1 cm, <3, 4> m ≈ <3.03, 4> m within 1 cm", "true false"),
    # norm-based: a tiny component next to a large one is fine relative to the whole vector
    ("print <1e6, 1e-9> m ≈ <1e6, 2e-9> m", "true"),
    # complex numbers: moduli
    ("print cis(π) ≈ -1 + 0i, cis(π) ≈ -1 + 0.1i within 0.2, cis(π) ≈ -1 + 0.3i within 0.2", "true true false"),
    ("print 1e-12𝑖 ≈ 0 + 0i within 1e-9, (1 + 1e-12𝑖) ≈ 1", "true true"),
    # in a function, an if and an assert
    ("f(x) = x ≈ 2 m within 1 cm\nprint f(2.001 m), f(2.1 m)", "true false"),
    ("v = 3e-15 m/s\nif v ≈ 0 m/s within 1e-12 m/s then print \"at rest\" else print \"moving\"", "at rest"),
    ("v = 3e-15 m/s\nassert v ≈ 0 m/s within 1e-12 m/s\nprint 1", "1"),
    # the tolerance can be computed
    ("σ = 0.02 m\nx = 1.03 m\nprint x ≈ 1 m within 2σ, x ≈ 1 m within σ", "true false"),
    # `within` ends at and/or, a comma, or a bracket
    ("x = 1 m\nprint x ≈ 1.0005 m within 1 mm and x > 0 m, (x ≈ 2 m within 1 mm), x", "true false 1 m"),
    # `within` is only a word after ≈: your own variable called within still works
    ("within = 2\nprint within + 1, 3 ≈ 3 within", "3 false"),
]


@pytest.mark.parametrize("src,want", CASES)
def test_approx(src, want):
    assert both(src) == want


ZERO_ERRORS = [
    ("x = 0.1\nprint x ~= 0", "'x ~= 0' is true only when x is exactly 0", "x ~= 0 within 1e-9"),
    ("v = 1e-12 m/s\nprint v ≈ 0 m/s", "'v ≈ 0 m/s' is true only when v is exactly 0", "v ≈ 0 m/s within 1e-9 m/s"),
    ("x = 1 m\nprint 0 [m] ≈ x", "'0 [m] ≈ x' is true only when x is exactly 0", "0 [m] ≈ x within 1e-9 [m]"),
    ("x = 1 m\nprint x ≈ 0.0 m", "is true only when x is exactly 0", "x ≈ 0.0 m within 1e-9 m"),
    ("x = 1 J\nprint x ≈ -0 J", "is true only when x is exactly 0", "within 1e-9 J"),
    ("v = <1, 2> m/s\nprint v ≈ <0, 0> m/s", "is true only when v is exactly 0", "v ≈ <0, 0> m/s within 1e-9 m/s"),
    ("v = <1 m, 2 m>\nprint v ≈ <0 m, 0 m>", "is true only when v is exactly 0", "within 1e-9 m"),
    ("x = 1 m\nprint x ≈ 0 m within 1%", "'x ≈ 0 m within 1%' is true only when x is exactly 0",
     "x ≈ 0 m within 1e-9 m"),
    ("f(t) = sin(t)\nassert f(π) ≈ 0", "'f(π) ≈ 0' is true only when f(π) is exactly 0", "within 1e-9"),
]


@pytest.mark.parametrize("src,msg,fix", ZERO_ERRORS)
def test_zero_without_absolute_tolerance_is_an_error(src, msg, fix):
    e = error_of(src)
    assert msg in e.message
    assert fix in (e.hint or "")
    assert e.line == src.count("\n") + 1


def test_zero_error_points_at_the_operator():
    e = error_of("v = 1 m/s\nprint v ≈ 0 m/s")
    assert (e.line, e.col) == (2, 9)


def test_zero_celsius_is_not_zero():
    # 0 °C is 273.15 K: a relative tolerance works there
    assert both("T = 273.15 K\nprint T ≈ 0 °C, T ≈ 1 °C") == "true false"


@pytest.mark.parametrize("src,msg", [
    ("x = 1 m\nprint x ≈ 0 m within 1e-9 s",
     "the tolerance after 'within' must be length [m], like the values it compares, but it is time [s]"),
    ("x = 1 m\nprint x ≈ 1.1 m within 1e-3", "the tolerance after 'within' must be length [m]"),
    ("x = 1\nprint x ≈ 1.1 within 1 mm", "the tolerance after 'within' must be"),
    ("x = 1 m\nprint x ≈ 1 m within -1 mm", "a tolerance can't be negative"),
    ("x = 1\nprint x ≈ 2 within", "'within' needs a tolerance after it"),
    ("print <1, 2> m ≈ 1 m", "≈ compares a vector with a vector of the same length"),
    ("print <1, 2> m ≈ <1, 2, 3> m", "can't compare a 2-vector with a 3-vector"),
    ("print <1, 2> m ≈ <1, 2> s", "can't compare"),
    ("print <1 m, 2 s> ≈ <1 m, 2 s>", "≈ needs all components of the vector in the same units"),
    ("print 1 m ≈ 1 s", "can't compare"),
    ("print [1, 2] ≈ [1, 2]", "must be a number"),
])
def test_errors(src, msg):
    assert msg in error_of(src).message


def test_relative_tolerance_hint():
    e = error_of("x = 1 m\nprint x ≈ 1.1 m within 1e-3")
    assert "within 0.1%" in e.hint


def test_fmt_keeps_within():
    from fermium.fmt import format_source
    assert format_source("x = 1 m\nprint x ~= 0 m within 1e-9 m", "pretty").strip() == "x = 1 m\nprint x ≈ 0 m within 1e-9 m"
    assert format_source("x = 1 m\nprint x ≈ 0 m within 1e-9 m", "ascii").strip() == "x = 1 m\nprint x ~= 0 m within 1e-9 m"


@pytest.mark.skipif(find_cc() is None, reason="no C compiler")
def test_build_agrees(tmp_path):
    src = "\n".join(s for s, _ in CASES if "0/0" not in s and "within =" not in s)
    want = run(src)
    exe = os.path.join(str(tmp_path), "prog")
    build(src, os.path.join(str(tmp_path), "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(tmp_path))
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == want
    assert interp(src) == want
