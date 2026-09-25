"""Gauntlet, quantum mechanics: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.linalg import eigh_tridiagonal

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "quantum")

HBAR = K.hbar
ME = K.m_e
EV = K.e


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    """The number printed right after `key` on a line."""
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def fd_levels(V, x_max, n_levels, npts=6000):
    """Lowest eigenvalues (J) of -ħ²/2m ψ'' + V ψ on (-x_max, x_max) with ψ = 0 at the ends,
    by second-order finite differences (independent of the shooting method)."""
    x = np.linspace(-x_max, x_max, npts + 2)[1:-1]
    h = x[1] - x[0]
    t = HBAR ** 2 / (2 * ME * h ** 2)
    w = eigh_tridiagonal(2 * t + V(x), -t * np.ones(npts - 1), select="i",
                         select_range=(0, n_levels - 1), eigvals_only=True)
    return w, h


def test_infinite_well():
    out = run_problem("01_infinite_well.fm")
    L = 1e-9
    levels = [ln for ln in out if ln.startswith("n =")]
    assert len(levels) == 3
    # finite differences on the box (centred on 0): error ~ (π n h / L)² / 12 relative
    fd, h = fd_levels(lambda x: 0 * x, L / 2, 3, npts=4000)
    for n, line in enumerate(levels, start=1):
        exact = n ** 2 * math.pi ** 2 * HBAR ** 2 / (2 * ME * L ** 2) / EV
        got = field(line, "E_shoot =")
        # shooting: RK45 at rtol 1e-9 and a root refined to double precision -> ~1e-9;
        # 10 printed digits.  2e-9 relative leaves room for the last printed digit.
        assert got == pytest.approx(exact, rel=2e-9)
        assert field(line, "E_exact =") == pytest.approx(exact, rel=1e-9)
        # the independent eigen-solver agrees to its own discretisation error (< 1e-5 here)
        assert got == pytest.approx(fd[n - 1] / EV, rel=1e-5)
    assert field(out[-2], "norm =") == pytest.approx(1.0, rel=1e-9)
    assert field(out[-1], "<x> =") == pytest.approx(0.5, rel=1e-9)


def test_harmonic_oscillator():
    out = run_problem("02_harmonic_oscillator.fm")
    hw = 1.000 * EV
    omega = hw / HBAR
    a = math.sqrt(HBAR / (ME * omega))
    assert field(out[0], "a =") == pytest.approx(a * 1e9, rel=1e-3)   # printed to 4 digits
    levels = {int(field(ln, "n =")): field(ln, "E_shoot =") for ln in out if ln.startswith("n =")}
    assert sorted(levels) == [0, 1, 2, 3]
    fd, _ = fd_levels(lambda x: 0.5 * ME * omega ** 2 * x ** 2, 8 * a, 4)
    for n, got in levels.items():
        # truncating at 7a changes E by ~exp(-49) (nothing); RK45 rtol 1e-9 -> 2e-9 is honest
        assert got == pytest.approx(n + 0.5, rel=2e-9)
        assert got == pytest.approx(fd[n] / EV, rel=1e-5)
    # <V> = ħω/4 by the virial theorem; quadrature at 1e-10
    assert field(out[-1], "=") == pytest.approx(0.25, rel=1e-9)


def t_exact(E, V0, a):
    """Textbook transmission through a rectangular barrier (energies in J, a in m)."""
    if E < V0:
        kappa = math.sqrt(2 * ME * (V0 - E)) / HBAR
        return 1 / (1 + V0 ** 2 * math.sinh(kappa * a) ** 2 / (4 * E * (V0 - E)))
    k2 = math.sqrt(2 * ME * (E - V0)) / HBAR
    return 1 / (1 + V0 ** 2 * math.sin(k2 * a) ** 2 / (4 * E * (E - V0)))


def test_barrier_tunneling():
    out = run_problem("03_barrier_tunneling.fm")
    V0, a = 5.00 * EV, 0.500e-9
    energies = [0.5, 1, 2, 3, 4, 4.9, 5.5, 7, 10]
    rows = [ln for ln in out if ln.startswith("E =")]
    assert len(rows) == len(energies)
    for E, line in zip(energies, rows):
        assert field(line, "E =") == pytest.approx(E)
        want = t_exact(E * EV, V0, a)
        # RK45 at rtol 1e-9 over a few decay lengths; T is ~1/|A|² so errors double: 1e-8
        assert field(line, "T_numeric =") == pytest.approx(want, rel=1e-8)
        assert field(line, "T_exact =") == pytest.approx(want, rel=1e-9)
    E_res = V0 + math.pi ** 2 * HBAR ** 2 / (2 * ME * a ** 2)
    assert field(out[-1], "resonance E =") == pytest.approx(E_res / EV, rel=1e-9)
    assert field(out[-1], "T_numeric =") == pytest.approx(1.0, rel=1e-8)
