"""Spec 1.5 §A4: display units, significant figures of list elements and look-alike characters (D240–D242)."""
import io
import os
import shutil
import subprocess

import pytest

from conftest import run
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    """The compiled result, after checking that the reference interpreter prints the same."""
    got = run(src)
    assert interp(src) == got
    return got


# --- A4.1: preferred display units (D240) ---------------------------------------------------------------------
@pytest.mark.parametrize("src,out", [
    ("print h c", "1.99×10⁻²⁵ J m"),                        # was N m²
    ("print ħ c", "3.16×10⁻²⁶ J m"),
    ("print k_e (1.6e-19 C)²", "2.3×10⁻²⁸ J m"),          # Coulomb: k q² is an energy times a length
    ("print h c in eV nm", "1240 eV nm"),                   # asking for eV nm keeps working
    ("print ħ c in MeV fm", "197 MeV fm"),
    ("x = 197.327 MeV fm\nprint x", "197.327 MeV fm"),       # a written unit is kept
    ("print h", "6.63×10⁻³⁴ J s"),
    ("τ = 5 N m\nprint τ", "5 N m"),                         # torque written as N m stays N m
    ("τ = 5 N m\nprint 2 τ", "10 N m"),
    ("r = 2 m\nF = 3 N\nprint r F", "6 J"),                  # a computed F·r can't be told from work: J
    ("r = 2 m\nF = 3 N\nprint r F in N m", "6 N m"),
])
def test_preferred_display_units(src, out):
    assert both(src) == out


def test_j_m_in_error_texts():
    from conftest import error_of
    e = error_of("print h c in J/m")
    assert "[J m]" in e.message


def test_nuclear_units_show_mev_fm():
    assert both("units nuclear\nprint 197.327 MeV fm") == "197.327 MeV fm"


@pytest.mark.skipif(shutil.which("cc") is None and shutil.which("gcc") is None, reason="no C compiler")
def test_built_executable_agrees(tmp_path):
    from fermium.aot import build
    src = "print h c\nprint h c in eV nm\nτ = 5 N m\nprint τ\n"
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60, cwd=str(tmp_path))
    assert got.stdout.strip() == run(src)
    assert os.path.exists(exe)


# --- A4.3: significant figures of list elements (D242; friction #30, examples #15, BC-B29) --------------------
@pytest.mark.parametrize("src,out", [
    # a loop over a written list prints each element as the list does (was 0.5 eV); display only: values
    # computed from E keep the rule they had (2 E: exact inputs and E's mixed precisions, 3 figures)
    ("for E in [0.50 eV, 0.75 eV, 1 eV]\n    print E, 2 E", "0.50 eV 1 eV\n0.75 eV 1.50 eV\n1 eV 2 eV"),
    ("for f in [0.25, 0.5, 3]\n    print f, 2 f", "0.25 0.500\n0.5 1\n3 6"),        # the gauntlet's multipliers
    ("print [0.50 eV, 0.75 eV, 1 eV]", "[0.50, 0.75, 1] eV"),
    ("for E in [0.50 eV, 0.75 eV, 1.00 eV]\n    print E", "0.50 eV\n0.75 eV\n1.0 eV"),
    ("for n in [0, 1, 1.5]\n    print n", "0\n1\n1.5"),                 # D11's example, unchanged
    ("for v in [0.5, 0.75]\n    print v", "0.5\n0.75"),                   # 0.75 isn't rounded to 1 figure
    ("for x in [1.0, 2.0]\n    print x", "1.0\n2.0"),
    ("for d in [1.20 mm, 1.30 mm]\n    print d", "1.20 mm\n1.30 mm"),   # 1.2000000000000002 after the trip via m
    ("print 1.20 mm", "1.20 mm"),
    ("print [1.20 mm, 1.30 mm]", "[1.20, 1.30] mm"),
    ("ts = [1 hr, 10 min]\nprint ts in day", "[0.0417, 0.00694] day"),    # BC-B29: was 0.0416666666666667 day
    ("print [20 °C, 30 °C] in K", "[293, 303] K"),                          # RT4-n2: as the scalar 20 °C in K
    ("print 20 °C in K", "293 K"),
])
def test_list_element_sigfigs(src, out):
    assert both(src) == out


@pytest.mark.skipif(shutil.which("cc") is None and shutil.which("gcc") is None, reason="no C compiler")
def test_list_element_sigfigs_built(tmp_path):
    from fermium.aot import build
    src = ("for E in [0.50 eV, 0.75 eV, 1 eV]\n    print E, 2 E\nfor d in [1.20 mm, 1.30 mm]\n    print d\n"
           "for v in [0.5, 0.75]\n    print v\nprint [1.20 mm, 1.30 mm]\n")
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60, cwd=str(tmp_path))
    assert got.stdout.strip() == run(src)


# --- A4.4: look-alike characters are normalised in code, never inside strings (examples #14) ------------------
@pytest.mark.parametrize("text", ["Pound–Rebka", "a − b", "“quoted”", "it’s", "x′", "5 ÷ 2"])
def test_lookalikes_inside_strings_are_kept(text):
    assert both(f'print "{text}"') == text


def test_lookalikes_in_code_are_still_normalised():
    assert both("x = 5 − 2\nprint x") == "3"          # U+2212 minus is a minus in code
    assert both("x = 6 ÷ 2\nprint x, \"6 ÷ 2\"") == "3 6 ÷ 2"
