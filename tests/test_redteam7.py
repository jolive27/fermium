"""Findings of the independent red-team review, round 7 (REDTEAM.md, "Round 7 (11:15 UTC)"): silent wrong answers
around the latest fixes (D230-D233), the printing rules (D11) and the research programs.

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
Reference values come from NumPy/SciPy or closed forms (see REDTEAM.md).
"""
import io
import os
import re
import shutil
import subprocess
import sys

import pytest

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def rt7(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 7 #{n}")


def num(text):
    """The first number in a printed value, as a float (handles ×10⁻¹⁵ and the minus sign)."""
    sup = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
    t = text.replace("−", "-")
    m = re.search(r"-?\d+(?:\.\d+)?(?:×10([⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+))?", t)
    assert m, text
    base = float(re.match(r"-?\d+(?:\.\d+)?", m.group(0)).group(0))
    if m.group(1):
        base *= 10 ** int(m.group(1).translate(sup))
    return base


def run_both(src):
    from fermium.driver import run_source
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


# ---- #1: D233 forces parity on a slightly asymmetric double well: the tilt is below the symmetry test's
#          tolerance (10⁻¹⁰ of the largest coefficient, set by the walls) but far above the tunnelling splitting --

def test_1_tilted_double_well_ground_state_is_localised():
    src = """m = m_e
V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8 + 3e-9 eV * x / 1 nm
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 2
print ψ₁(-1 nm) / ψ₁(1 nm) to 6 digits
print ∫ x ψ₁(x)^2 dx from -3 nm to 3 nm in nm
"""
    ratio, mean_x = run(src).split("\n")
    # asymmetry δ ≈ 6×10⁻⁹ eV ≫ tunnelling splitting Δ ≈ 5×10⁻¹¹ eV: the ground state sits in the lower (left)
    # well. SciPy eigh_tridiagonal on the same equation (6000 intervals): ψ₁(-1 nm)/ψ₁(1 nm) = 199,
    # ⟨x⟩₁ = -0.981 nm, E₂ - E₁ = 5.87×10⁻⁹ eV. Fermium prints 1.00000 and ⟨x⟩₁ = 1.6×10⁻¹⁶ nm (forced parity).
    assert num(ratio) > 10, ratio
    assert num(mean_x) < -0.5, mean_x


# ---- #2: the heat equation after a jump: early times are still silently wrong (D230 removed the skip, but the
#          error is spatial, below one grid cell, and nothing warns) ----------------------------------------------

@rt7(2)
def test_2_heat_step_very_early_time_is_accurate_or_warned():
    src = """D = 1e-4 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(1 m, t) = 0 K
    for x from 0 m to 1 m, t from 0 s to 100 s
print u(0.5 mm, 0.002 s) to 5 digits
"""
    out, err = run_both(src)
    # 80 K erfc(x / (2√(D t))) = 34.336 K; Fermium prints 55.00 K (60 % high) with no warning
    assert abs(num(out) - 34.336) < 1.0 or "warning" in err, (out, err)


# ---- #3: research/rutherford_mc/README.md (and research/README.md) quote numbers the program no longer prints --

def test_3_rutherford_readme_matches_the_program(tmp_path):
    d = os.path.join(ROOT, "research", "rutherford_mc")
    shutil.copy(os.path.join(d, "rutherford.fm"), tmp_path / "rutherford.fm")
    env = dict(os.environ, PYTHONPATH=ROOT)
    out = subprocess.run([sys.executable, "-m", "fermium", "run", "rutherford.fm"], cwd=tmp_path, env=env,
                         capture_output=True, text=True, timeout=600).stdout
    chi2_out = re.search(r"exact bin contents: ([\d.]+)", out).group(1)
    readme = open(os.path.join(d, "README.md"), encoding="utf-8").read()
    chi2_readme = re.search(r"exact Rutherford contents: ([\d.]+)", readme).group(1)
    # the README says every run gives 35.6 (and "rand() has no seed function"); the program prints 38.2
    assert chi2_out == chi2_readme, (chi2_out, chi2_readme)


# ---- #4: D11 applies "fewest significant figures" to +: a 1-figure addend wipes out a precise value -------------

def test_4_adding_a_small_correction_keeps_the_precise_value():
    out = run("T = 293.15 K\nprint T + 0.5 K\nm_p = 938.272 MeV\nprint m_p + 2.2 MeV\n").split("\n")
    # 293.65 K and 940.472 MeV (decimal places: 293.7 K, 940.5 MeV); Fermium prints 290 K and 940 MeV
    assert abs(num(out[0]) - 293.65) < 0.1, out
    assert abs(num(out[1]) - 940.472) < 0.1, out


# ---- #5: an integral dominated by rounding error prints 3 figures of which 1 is right, with no warning --------

@rt7(5)
def test_5_rounding_dominated_integral_is_right_to_its_printed_digits_or_warned():
    out, err = run_both("print ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1\n")
    # exact 8×10⁻⁹ (the sine part cancels); Fermium prints 7.93×10⁻⁹ with no warning.
    # SciPy quad gives 7.96×10⁻⁹ ± 1.0×10⁻⁸ and warns about round-off.
    assert abs(num(out) - 8e-9) < 0.006e-9 or "warning" in err, (out, err)
