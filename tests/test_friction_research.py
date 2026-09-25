"""Frictions found while writing the research reproductions (gauntlet/FRICTION.md #60 onwards, source "research").

Programs run compiled (LLVM) and in the reference interpreter, and the two must agree."""
import io

import pytest

from conftest import run, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    return o.getvalue().strip()


def both(src):
    out = run(src)
    assert interp(src) == out
    return out


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    return native.value


# ---------------------------------------------------------------- #60: tolerance and using in any order
DECAY = "solve x' = -x / (1 s)\n  with x(0) = 1 m\n  for t from 0 s to 1 s {opts}\nprint x(1 s) to 9 digits"


@pytest.mark.parametrize("opts", [
    "tolerance 1e-11 using radau",
    "using radau tolerance 1e-11",
    "tolerance 1e-11 using bdf",
    "using rk45 tolerance 1e-11",
    "method rk45 tolerance 1e-11",
    "tolerance 1e-11 using rk45",
])
def test_60_tolerance_and_using_in_either_order(opts):
    tol = 1e-4 if "bdf" in opts else 1e-9
    v = float(both(DECAY.format(opts=opts)).split()[0])
    assert abs(v - 0.367879441) < tol


def test_60_until_combines_with_tolerance_and_using():
    src = ("solve y'' = -9.81 m/s²\n  with y(0 s) = 0 m, y'(0 s) = 10 m/s\n"
           "  for t from 0 s to 5 s until y = 0 m tolerance 1e-11 using rk45\nprint times(y)[end] to 6 digits")
    src2 = src.replace("until y = 0 m tolerance 1e-11 using rk45", "using rk45 tolerance 1e-11 until y = 0 m")
    a = both(src)
    assert abs(float(a.split()[0]) - 2 * 10 / 9.81) < 1e-5
    assert a == both(src2)


def test_60_tolerance_given_twice_is_an_error():
    e = both_error(DECAY.format(opts="tolerance 1e-11 using radau tolerance 1e-9"))
    assert "given twice" in e.message


def test_60_method_name_error_lists_the_methods():
    e = both_error(DECAY.format(opts="tolerance 1e-11 using 3"))
    assert "radau" in e.message


# ---------------------------------------------------------------- #61: bracketed divisor after an upper limit
@pytest.mark.parametrize("tail", ["1 / (1 + z)", "1 / √(4 z)", "1 / |2 z|"])
def test_61_bracketed_divisor_after_upper_limit_warns(tail):
    src = f"z = 1\nprint ∫ 1 da from 0 to {tail} to 4 digits"
    assert both(src) == "0.5000"          # the whole integral (1) divided by 2
    assert any("divides the whole integral" in w for w in warnings_of(src))


def test_61_plain_divisor_still_warns():
    assert any("divides the whole integral" in w for w in warnings_of("print ∫ x dx from 0 to 1 / 2"))


@pytest.mark.parametrize("src", [
    "z = 1\nprint ∫ 1 da from 0 to 1/(1 + z) to 4 digits",
    "z = 1\nprint ∫ 1 da from 0 to (1 / (1 + z)) to 4 digits",
    "z = 1\nprint (∫ 1 da from 0 to 1) / (1 + z) to 4 digits",
])
def test_61_unambiguous_forms_do_not_warn(src):
    assert both(src) == "0.5000"
    assert not any("divides the whole integral" in w for w in warnings_of(src))


def test_61_no_warning_after_an_infinite_limit():
    src = ("I = 2.0 A\nB_axis(z) = μ₀ I (1 m)² / (2 ((1 m)² + z²)^(3/2))\n"
           "print ∫ B_axis(z) dz from -∞ m to ∞ m / (μ₀ I) to 6 digits")
    assert both(src) == "1.00000"
    assert not any("divides the whole integral" in w for w in warnings_of(src))


def test_61_lower_limit_keeps_its_division():
    src = "z = 1\nprint ∫ 1 da from 1 / (1 + z) to 1 to 4 digits"
    assert both(src) == "0.5000"
    assert not any("divides" in w for w in warnings_of(src))
