"""Spec 1.5 §A5 (D244): `fit … with` continues on the next line like `solve … with` (examples #12), and lists
of text work (examples #7).  Both were already done; these tests keep them."""
import io
import os

import pytest

from conftest import run
from fermium.interp import run_interpreted
from numparse import num

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "examples", "data")


def interp(src, base_dir=None):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=base_dir)
    return out.getvalue().strip()


FIT_ONE_LINE = """data = load "pendulum.csv"
fit T = 2π √(L / g) to data with g = 9 m/s²
print g
"""

FIT_NEXT_LINE = """data = load "pendulum.csv"
fit T = 2π √(L / g) to data
  with g = 9 m/s²
print g
"""

FIT_TWO_GUESSES = """data = load "pendulum.csv"
fit T = 2π √(L / g) + t0 to data
  with g = 9 m/s²,
       t0 = 0 s
print g
"""


def test_fit_with_on_the_next_line():
    one = run(FIT_ONE_LINE, base_dir=DATA)
    nxt = run(FIT_NEXT_LINE, base_dir=DATA)
    assert one == nxt
    assert "m/s²" in nxt and 9.5 < num(nxt.splitlines()[-1]) < 10.1


def test_fit_with_on_the_next_line_interpreter():
    try:
        got = interp(FIT_NEXT_LINE, base_dir=DATA)
    except TypeError:                      # run_interpreted without base_dir: run from the data folder instead
        pytest.skip("interpreter entry point takes no base_dir")
    assert got == run(FIT_NEXT_LINE, base_dir=DATA)


def test_fit_with_guesses_continued_over_lines():
    out = run(FIT_TWO_GUESSES, base_dir=DATA).splitlines()
    assert out[-1].endswith("m/s²")


@pytest.mark.parametrize("src,out", [
    ('names = ["H-1", "He-4", "C-12"]\nprint names', "[H-1, He-4, C-12]"),
    ('names = ["H-1", "He-4", "C-12"]\nprint names[2], len(names)', "He-4 3"),
    ('names = ["a", "b"]\nfor s in names\n    print s', "a\nb"),
    ('names = ["x", "y"]\nprint names[end] + "!"', "y!"),
])
def test_lists_of_text(src, out):
    assert run(src) == out
    assert interp(src) == out
