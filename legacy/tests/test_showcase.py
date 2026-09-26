"""Every snippet in SHOWCASE.md runs and prints what the page says."""
import os
import re

import pytest

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BLOCKS = re.findall(r"```fermium\n(.*?)```", open(os.path.join(ROOT, "SHOWCASE.md"), encoding="utf-8").read(), re.S)

EXPECT = [
    ["9.70 m/s²", "31.8 ft/s²"],
    ["∇φ(x, y, z) = <-q x/(4π ε", "V/m", "3.52 cm"],
    ["g = 9.801 ± 0.053 m/s²", "x - x = 0 ± 0 m", "range: 40.2 ± 2.2 m"],
    ["[0.2718, 1.077, 2.379] eV", "ground state: 0.0083"],
    ["MeV"],
]


def test_there_are_five_showcase_snippets():
    assert len(BLOCKS) == 5


@pytest.mark.parametrize("i", range(5))
def test_showcase_snippet(i):
    out = run(BLOCKS[i], base_dir=ROOT)
    for s in EXPECT[i]:
        assert s in out, (i, s, out)
