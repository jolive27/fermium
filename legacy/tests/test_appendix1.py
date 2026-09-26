"""Spec 1.5 Appendix 1: John's programs are mandatory regression tests.

The programs are in tests/programs/john/, exactly as John wrote them.  The display may follow D11 (significant
figures from the inputs), but the numbers must match the values in the spec's comments.

One conflict inside the spec (D235): lesson2.fm writes `a = 3 m` after `m = 1500 kg`.  The A1 table requires a
lone unit that is also your variable (`x(0) = 0.1 m` next to a mass m, `B = 2 T`, `8 K`, `2 c`) to be an error,
and `a = 3 m` next to a mass m is the same construction.  So lesson2.fm stops at line 14 with a one-line hint, and
`fermium fmt --fix` (which writes `3 [m]`) gives every value in the spec.
"""
import os
import re
import subprocess
import sys

import pytest

from conftest import run, error_of
from fermium.fmt import fix_source

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
JOHN = os.path.join(ROOT, "legacy", "tests", "programs", "john")


def src(name):
    return open(os.path.join(JOHN, f"appendix1_{name}.fm"), encoding="utf-8").read()


def nums(line):
    """The numbers printed on a line, with ×10ⁿ exponents applied."""
    sup = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
    out = []
    for m in re.finditer(r"(-?\d+(?:\.\d+)?)(?:×10([⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+))?", line):
        v = float(m.group(1))
        if m.group(2):
            v *= 10 ** int(m.group(2).translate(sup))
        out.append(v)
    return out


def close(line, want, unit, rel=0.006):
    got = nums(line)
    assert len(got) == len(want), (line, want)
    for g, w in zip(got, want):
        assert g == pytest.approx(w, rel=rel), (line, want)
    assert line.split()[-1] == unit or unit in line, (line, unit)


def test_hello():
    assert run(src("hello")).split("\n") == ["Hello Physics!", "29.4 m/s"]


def test_ke():
    assert run(src("ke")) == "9 J"


def test_lesson2_stops_at_the_collision_with_a_one_line_fix():
    e = error_of(src("lesson2"))
    assert e.line == 14 and "'3 m' is ambiguous" in e.message and "3 [m]" in e.hint


def test_lesson2_after_fmt_fix_gives_every_value():
    fixed, n = fix_source(src("lesson2"))
    assert n == 2 and "a = 3 [m]" in fixed and "b = 5 [m]" in fixed
    assert fixed.replace("3 [m]", "3 m").replace("5 [m]", "5 m") == src("lesson2")      # nothing else changed
    lines = run(fixed).split("\n")
    close(lines[0], [40.8], "m")
    close(lines[1], [579], "kJ")
    close(lines[2], [11.2], "km/s")
    assert lines[3] == "5 m 3 m"
    close(lines[4], [0.243], "nm", rel=0.02)        # prints 0.24 nm: c's 0.01 has one significant figure


def test_lesson2b():
    lines = run(src("lesson2b")).split("\n")
    assert lines[0] == "10 rad/s"
    close(lines[1], [2.0], "s")
    assert lines[2] == "0.500"                 # theta and θ are the same name


def test_lesson3():
    lines = run(src("lesson3")).split("\n")
    close(lines[0], [686], "N")
    close(lines[1], [2.75, 4.59, 7.44], "fm", rel=0.02)     # r₀ = 1.2 fm has 2 significant figures: 2.7 4.6 7.4
    close(lines[2], [35.3, 40.8, 35.3], "m")
    got = nums(lines[3])
    assert got == pytest.approx([502, 0.502, 9.35e3, 9.35], rel=0.002)
    assert "nm" in lines[3] and ("um" in lines[3] or "μm" in lines[3])
    close(lines[4], [8.20], "s")


@pytest.mark.parametrize("line,want", [
    ("print 2 m + 30 cm", "2.30 m"),
    ("print h c in eV nm", "1240 eV nm"),
    ("print h c in J m", "1.99×10⁻²⁵ J m"),
    ("print 0.5 * 2 kg * 3 m/s^2", "3.0 N"),      # the bootcamp's "spot the bug": the unit shows the bug
])
def test_repl_checks(line, want):
    assert run(line) == want


def test_repl_checks_at_the_prompt():
    lines = "print 2 m + 30 cm\nprint h c in eV nm\nprint h c in J m\nprint 0.5 * 2 kg * 3 m/s^2\n:quit\n"
    p = subprocess.run([sys.executable, "-m", "fermium"], input=lines, capture_output=True, text=True, cwd=ROOT,
                       timeout=120)
    for want in ("2.30 m", "1240 eV nm", "1.99×10⁻²⁵ J m", "3.0 N"):
        assert want in p.stdout, p.stdout


def test_programs_are_unchanged_copies():
    # the Appendix text and the files agree (the files are what the tests run)
    spec = open(os.path.join(ROOT, "dev-notes", "FERMIUM_SPEC_V1.5_V2.md"), encoding="utf-8").read()
    for name in ("hello", "ke", "lesson2", "lesson2b", "lesson3"):
        m = re.search(r"\*\*" + name + r"\.fm\*\*.*?\n```fermium\n(.*?)```", spec, re.S)
        assert m and m.group(1) == src(name), name

