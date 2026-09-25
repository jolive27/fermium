"""D11: when a result's precision is ambiguous or unspecified it prints with 3 significant figures.
The rule is display only: values keep full double precision in every calculation."""
import io
import subprocess

import pytest

from conftest import run
from fermium.aot import build, find_cc
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a, b = run(src), interp(src)
    assert a == b, (a, b)
    return a


@pytest.mark.parametrize("src,out", [
    ("print 2 / 3", "0.667"),
    ("print 1 / 2", "0.500"),
    ("print 2π", "6.28"),
    ("print c", "3.00×10⁸ m/s"),
    ("print G", "6.67×10⁻¹¹ m³/(kg s²)"),
    ("print 20 m / (3 s)", "6.67 m/s"),
    ("print 4 * 5", "20"),                       # a whole number prints exactly
    ("print 1000 * 1000", "1000000"),
    ("x = 3.14159\nprint x", "3.14159"),         # a literal prints as written
    ("print 12345678", "1.2345678×10⁷"),
    ("print 2.00 m * 3", "6.00 m"),              # an input with a stated precision decides
    ("print 0.5 * 9.81", "4.9"),                 # at least 2
    ("print 2 / 3 to 6 digits", "0.666667"),     # `to N digits` overrides
])
def test_default_is_three_significant_figures(src, out):
    assert both(src) == out


@pytest.mark.parametrize("src,out", [
    ("print [1/3, 2/3, 1]", "[0.333, 0.667, 1.00]"),     # one style for the whole list
    ("print [1, 2, 3]", "[1, 2, 3]"),
    ("print [0, 0.5, 1, 1.5]", "[0, 0.5, 1, 1.5]"),      # a written-out list prints as written
    ("print [0.10, 0.20]", "[0.10, 0.20]"),
    ("print <1/3, 1, 2> m", "<0.333, 1.00, 2.00> m"),
])
def test_lists_and_vectors(src, out):
    assert both(src) == out


def test_display_only_precision_is_kept():
    # 1/3 shows as 0.333, but the value is still 1/3 to double precision
    src = "x = 1 / 3\nprint x\nprint x * 3 - 1\nprint x to 17 digits\nprint x * 3000000"
    assert both(src).split("\n") == ["0.333", "0", "0.33333333333333331", "1000000"]


def test_display_only_through_a_long_calculation():
    # a sum of 1000 terms that each print as 0.001: the total is exact, not 1000 × a rounded 0.001
    src = "s = 0\nfor i from 1 to 1000\n    s = s + 1 / 1000\nprint 1 / 1000\nprint s to 15 digits"
    assert both(src).split("\n") == ["0.00100", "1.00000000000000"]


@pytest.mark.skipif(find_cc() is None, reason="no C compiler")
def test_fermium_build_prints_the_same(tmp_path):
    src = "print 2 / 3\nprint c\nprint [1/3, 2/3, 1]\nprint [0.10, 0.20]\nprint 1000 * 1000\nprint 2 / 3 to 6 digits"
    exe = tmp_path / "p"
    build(src, str(tmp_path / "p.fm"), str(exe))
    got = subprocess.run([str(exe)], capture_output=True, text=True, timeout=60).stdout.strip()
    assert got == run(src)
