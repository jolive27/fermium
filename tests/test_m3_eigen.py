"""M3 part 3: bound-state eigenvalue problems (DECISIONS D82).

    solve -ħ²/(2m) * ψ'' + V(x) ψ = E ψ  with ψ(a) = 0, ψ(b) = 0  for x from a to b  lowest N

checked against the infinite well n²π²ħ²/(2mL²), the harmonic oscillator ħω(n + ½), and a finite well's
transcendental equations (solved here with SciPy), with the matrix method and the shooting method.
"""
import io
import math

import pytest

from conftest import run
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
from numparse import num

HBAR = 6.62607015e-34 / (2 * math.pi)
ME = 9.1093837139e-31
EV = 1.602176634e-19


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a = run(src)
    assert interp(src) == a
    return a


def energies(out):
    return [num(line) for line in out.splitlines()]


WELL = """L = 1 nm
solve -ħ²/(2m_e) * ψ'' = E ψ
    with ψ(0 nm) = 0, ψ(L) = 0
    for x from 0 nm to L
    lowest 4{method}
for n from 1 to 4
    print E[n] in eV to 14 digits
"""


@pytest.mark.parametrize("method", ["", " using matrix", " using shooting"])
def test_infinite_well_n_squared(method):
    out = run(WELL.format(method=method))
    e1 = math.pi ** 2 * HBAR ** 2 / (2 * ME * 1e-9 ** 2) / EV
    for n, e in enumerate(energies(out), start=1):
        assert e == pytest.approx(n * n * e1, rel=1e-9)


def test_infinite_well_same_in_interpreter():
    both(WELL.format(method=""))


def test_infinite_well_wavefunctions_normalised_orthogonal_and_exact():
    src = WELL.format(method="") + """print ∫ ψ₁(x)^2 dx from 0 nm to L to 10 digits
print ∫ ψ₁(x) ψ₂(x) dx from 0 nm to L to 10 digits
print ψ₁(0.3 nm) / sqrt(2 / L) to 10 digits
print ψ₂'(0 nm) / (sqrt(2 / L) 2π / L) to 10 digits
print ψ₁''(0.3 nm) / ψ₁(0.3 nm) * ħ² / (2m_e) / E[1] to 10 digits
"""
    out = run(src).splitlines()[4:]
    assert num(out[0]) == pytest.approx(1, abs=1e-9)
    assert abs(num(out[1])) < 1e-9
    assert num(out[2]) == pytest.approx(math.sin(math.pi * 0.3), rel=1e-6)       # first lobe positive
    assert num(out[3]) == pytest.approx(1, rel=1e-5)
    assert num(out[4]) == pytest.approx(-1, rel=1e-9)                             # ψ'' is exact: (V - E)ψ


OSC = """m = m_e
ħω = 1 eV
ω = ħω / ħ
V(x) = m ω² x² / 2
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 5{method}
for n from 1 to 5
    print E[n] / ħω to 14 digits
"""


@pytest.mark.parametrize("method", ["", " using shooting"])
def test_harmonic_oscillator_n_plus_half(method):
    out = both(OSC.format(method=method))
    for n, e in enumerate(energies(out)):
        assert e == pytest.approx(n + 0.5, abs=5e-9)


def test_harmonic_oscillator_ground_state_is_the_gaussian():
    src = OSC.format(method="") + """x0 = sqrt(ħ / (m ω))
print ψ₁(0 nm) * sqrt(x0) * π^(1/4) to 10 digits
print ψ₁(x0) / ψ₁(0 nm) to 10 digits
print ψ₂(x0) / (sqrt(2) exp(-1/2) / (sqrt(x0) π^(1/4))) to 10 digits
"""
    out = run(src).splitlines()[5:]
    assert num(out[0]) == pytest.approx(1, rel=1e-5)
    assert num(out[1]) == pytest.approx(math.exp(-0.5), rel=1e-5)
    assert num(out[2]) == pytest.approx(-1, rel=1e-5)        # the left lobe (x < 0) is the positive one


FINITE = """V0 = 5 eV
a = 0.5 nm
V(x) = if abs(x) < a then 0 eV else V0
solve -ħ²/(2m_e) * ψ'' + V(x) ψ = E ψ
    with ψ(-2 nm) = 0, ψ(2 nm) = 0
    for x from -2 nm to 2 nm
    lowest 3{method}
for n from 1 to 3
    print E[n] in eV to 14 digits
"""


def finite_well_levels(v0_ev=5.0, a=0.5e-9, count=3):
    """Even states: k tan(ka) = κ; odd: -k cot(ka) = κ (a = half-width), solved with brentq."""
    from scipy.optimize import brentq
    V0 = v0_ev * EV

    def k(E):
        return math.sqrt(2 * ME * E) / HBAR

    def kap(E):
        return math.sqrt(2 * ME * (V0 - E)) / HBAR

    def even(E):
        return k(E) * math.sin(k(E) * a) - kap(E) * math.cos(k(E) * a)

    def odd(E):
        return -k(E) * math.cos(k(E) * a) - kap(E) * math.sin(k(E) * a)
    grid = [V0 * i / 20000 for i in range(1, 20000)]
    roots = []
    for f in (even, odd):
        for p, q in zip(grid, grid[1:]):
            if f(p) * f(q) < 0:
                roots.append(brentq(f, p, q, xtol=1e-40, rtol=1e-15))
    return sorted(roots)[:count]


def test_finite_well_matches_transcendental_equation():
    want = [E / EV for E in finite_well_levels()]
    got = energies(both(FINITE.format(method="")))
    assert len(want) == 3
    for g, w in zip(got, want):
        assert g == pytest.approx(w, rel=2e-6)          # the walls fall between grid points; see D82


def test_finite_well_shooting_cross_check():
    want = [E / EV for E in finite_well_levels()]
    got = energies(run(FINITE.format(method=" using shooting")))
    for g, w in zip(got, want):
        assert g == pytest.approx(w, rel=2e-5)


def test_matrix_and_shooting_agree_on_an_anharmonic_potential():
    src = """V(x) = 2 eV (x / 0.3 nm)^4 - 1 eV (x / 0.3 nm)^2
solve -ħ²/(2m_e) * ψ'' + V(x) ψ = E ψ with ψ(-1.5 nm) = 0, ψ(1.5 nm) = 0 for x from -1.5 nm to 1.5 nm lowest 3{m}
for n from 1 to 3
    print E[n] in eV to 14 digits
"""
    a = energies(run(src.format(m="")))
    b = energies(run(src.format(m=" using shooting")))
    assert a == pytest.approx(b, rel=1e-8)


def test_matches_scipy_dense_eigensolver_for_a_linear_potential():
    """V = F x in a box: compare with numpy.linalg.eigh of a fine finite-difference matrix (independent code)."""
    np = pytest.importorskip("numpy")
    src = """F = 2 eV/nm
solve -ħ²/(2m_e) * ψ'' + F x ψ = E ψ with ψ(0 nm) = 0, ψ(2 nm) = 0 for x from 0 nm to 2 nm lowest 2 grid 3000
print E[1] in eV to 14 digits
print E[2] in eV to 14 digits
"""
    got = energies(both(src))
    n = 1500
    L = 2e-9
    h = L / n
    x = np.arange(1, n) * h
    T = HBAR ** 2 / (2 * ME * h * h)
    H = np.diag(2 * T + 2 * EV / 1e-9 * x) + np.diag(-T * np.ones(n - 2), 1) + np.diag(-T * np.ones(n - 2), -1)
    ev = np.linalg.eigvalsh(H)[:2] / EV
    assert got == pytest.approx(list(ev), rel=2e-6)      # numpy's value has the O(h²) error of its grid


def test_units_and_display():
    out = run(WELL.format(method="") + "print E[1]\nprint ψ₁(0.5 nm) in nm^(-1/2) to 8 digits\nprint len(E)")
    lines = out.splitlines()[4:]
    assert lines[0].endswith(" J")
    assert num(lines[1]) == pytest.approx(math.sqrt(2), rel=1e-6)
    assert lines[2] == "4"
    with pytest.raises(FermiumError, match="energy|units"):
        run(WELL.format(method="") + "print E[1] in m")


def test_plot_of_states(tmp_path):
    src = WELL.format(method="") + 'plot ψ₁ vs x, ψ₂ vs x to "states.png"'
    run(src, base_dir=str(tmp_path))
    assert (tmp_path / "states.png").exists()


# ---------------------------------------------------------------- errors
def test_error_no_unknown_eigenvalue():
    with pytest.raises(FermiumError, match="needs an unknown constant"):
        run("E = 1 eV\nsolve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")


def test_error_two_unknowns():
    with pytest.raises(FermiumError, match="exactly one unknown constant"):
        run("solve -ħ²/(2m_e) * ψ'' + W ψ = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")


def test_error_boundary_conditions():
    with pytest.raises(FermiumError, match="at both ends"):
        run("solve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 0 for x from 0 nm to 1 nm lowest 2")
    with pytest.raises(FermiumError, match="only ψ = 0 at the ends"):
        run("solve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 1, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")
    with pytest.raises(FermiumError, match="ends of the range"):
        run("solve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 0, ψ(0.5 nm) = 0 for x from 0 nm to 1 nm lowest 2")


def test_error_units_of_the_equation():
    with pytest.raises(FermiumError, match="same units"):
        run("solve -ħ²/(2m_e) * ψ'' + 3 m ψ = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")


def test_error_first_derivative_term_and_wrong_sign():
    with pytest.raises(FermiumError, match="ψ' isn't supported"):
        run("solve ψ'' + ψ' / (1 nm) = -E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")
    with pytest.raises(FermiumError, match="no lowest eigenvalues"):
        run("solve ψ'' = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2")


def test_error_bad_lowest_and_method():
    with pytest.raises(FermiumError, match="whole number"):
        run("solve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2.5")
    with pytest.raises(FermiumError, match="matrix or shooting"):
        run("solve -ħ²/(2m_e) * ψ'' = E ψ with ψ(0 nm) = 0, ψ(1 nm) = 0 for x from 0 nm to 1 nm lowest 2 "
            "using rk4")


def test_build_refuses_eigenvalue_problems(tmp_path):
    from fermium.aot import build
    with pytest.raises(FermiumError, match="fermium build can't compile an eigenvalue problem"):
        build(WELL.format(method=""), str(tmp_path / "w.fm"), str(tmp_path / "w"))
