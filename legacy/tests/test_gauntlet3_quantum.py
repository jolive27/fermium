"""Gauntlet third pass (graduate), quantum mechanics: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.integrate import solve_ivp
from scipy.linalg import expm

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "quantum")


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    """The number printed right after `key` on a line."""
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def lines_with(out, prefix):
    return [ln for ln in out if ln.lstrip().startswith(prefix)]


# ---------------------------------------------------------------- 31 hydrogen fine structure

def test_hydrogen_fine_structure():
    out = run_problem("31_hydrogen_fine_structure.fm")
    me, c, hbar, alpha = K.m_e, K.c, K.hbar, K.alpha
    k = K.e ** 2 / (4 * math.pi * K.epsilon_0)
    a0 = hbar ** 2 / (me * k)
    uev = K.e * 1e-6
    E2 = -me * k ** 2 / (2 * hbar ** 2) / 4
    a = lines_with(out, "(a)")[0]
    # the eigenvalues: Numerov's, extrapolated (D190), on the default grid (the problem used grid 32000
    # before the eigenfunctions were fourth order, FRICTION #70)
    assert field(a, "E(2s) =") == pytest.approx(E2 / K.e, rel=1e-9)
    assert field(a, "E(2p) =") == pytest.approx(E2 / K.e, rel=1e-9)
    assert field(a, "Bohr =") == pytest.approx(E2 / K.e, rel=1e-9)

    # (b) Griffiths 6.55, 6.56, 6.64 and |ψ200(0)|² = 1/(8π a0³); the eigenfunctions are O(h⁴) (D190):
    # the printed 8 digits are all right at the default grid
    b = lines_with(out, "(b)")[0]
    assert field(b, "<1/r> a0 =") == pytest.approx(0.25, rel=5e-8)
    assert field(b, "<1/r²> a0² =") == pytest.approx(1 / (0.5 * 8), rel=5e-8)
    assert field(b, "|psi(0)|² a0³ =") == pytest.approx(1 / (8 * math.pi), rel=1e-6)
    b2 = out[2]
    assert field(b2, "<1/r> a0 =") == pytest.approx(0.25, rel=5e-8)
    assert field(b2, "<1/r²> a0² =") == pytest.approx(1 / (1.5 * 8), rel=5e-8)
    assert field(b2, "<1/r³> a0³ =") == pytest.approx(1 / (1 * 1.5 * 2 * 8), rel=5e-8)
    assert a0 == pytest.approx(K.physical_constants["Bohr radius"][0], rel=1e-6)

    # (c) the fine-structure formula, and Dirac's energy (expanded exactly with mpmath-free
    # log1p/expm1 in double precision)
    fs = lambda n, j: -(E2 ** 2 / (2 * me * c ** 2)) * (4 * n / (j + 0.5) - 3)

    def dirac(n, j):
        d = n - (j + 0.5) + math.sqrt((j + 0.5) ** 2 - alpha ** 2)
        return me * c ** 2 * math.expm1(-0.5 * math.log1p((alpha / d) ** 2)) + me * c ** 2 * alpha ** 2 / (2 * n * n)

    c1, c2, c3, c4 = out[3], out[4], out[5], out[6]
    # each first-order total agrees with the formula to 5e-6 (the numerical expectation values)
    assert field(c1, "total =") == pytest.approx(fs(2, 0.5) / uev, rel=5e-6)
    assert field(c2, "2p1/2: total =") == pytest.approx(fs(2, 0.5) / uev, rel=5e-6)
    assert field(c2, "2p3/2: total =") == pytest.approx(fs(2, 1.5) / uev, rel=5e-6)
    # the Darwin term alone: (πħ²k/(2m²c²)) / (8π a0³)
    assert field(c1, "Darwin =") == pytest.approx(math.pi * hbar ** 2 * k / (2 * me ** 2 * c ** 2) / (8 * math.pi * a0 ** 3) / uev,
                                                  rel=5e-6)
    assert field(c3, "j = 1/2:") == pytest.approx(fs(2, 0.5) / uev, rel=1e-7)
    assert field(c3, "j = 3/2:") == pytest.approx(fs(2, 1.5) / uev, rel=1e-7)
    assert field(c4, "j = 1/2:") == pytest.approx(dirac(2, 0.5) / uev, rel=1e-7)
    assert field(c4, "j = 3/2:") == pytest.approx(dirac(2, 1.5) / uev, rel=1e-7)
    # first order differs from Dirac by O(α⁶ mc²): 3e-5 relative here
    assert abs(dirac(2, 0.5) - fs(2, 0.5)) / abs(fs(2, 0.5)) < 1e-4

    d = lines_with(out, "(d)")[0]
    split = (dirac(2, 1.5) - dirac(2, 0.5)) / K.h
    assert field(d, "Dirac:") == pytest.approx(split / 1e9, rel=1e-5)
    assert field(d, "2p3/2 - 2p1/2 =") == pytest.approx(split / 1e9, rel=1e-4)
    # measured 10.969 GHz (reduced mass and QED account for the 0.2 %)
    assert field(d, "2p3/2 - 2p1/2 =") == pytest.approx(10.969, rel=3e-3)


# ---------------------------------------------------------------- 32 Rabi and Ramsey

def test_rabi_ramsey():
    out = run_problem("32_rabi_ramsey.fm")
    w0, Om, D = 1e9, 2e7, 1e7
    WR = math.hypot(Om, D)
    tp = math.pi / WR
    a = lines_with(out, "(a)")[0]
    # the RWA Hamiltonian is constant: exact propagation by a matrix exponential
    H = np.array([[0, Om / 2], [Om / 2, -D]])
    psi = expm(-1j * H * tp) @ np.array([1, 0])
    assert abs(psi[1]) ** 2 == pytest.approx(Om ** 2 / WR ** 2, rel=1e-12)
    assert field(a, "P_e (RWA ODE) =") == pytest.approx(abs(psi[1]) ** 2, rel=1e-9)
    assert field(a, "Rabi =") == pytest.approx(0.8, rel=1e-9)
    assert field(a, "norm =") == pytest.approx(1.0, rel=1e-9)

    # (b) the full Hamiltonian with SciPy (real and imaginary parts), DOP853
    w = w0 + D

    def rhs(t, y):
        ae, ag = y[0] + 1j * y[1], y[2] + 1j * y[3]
        dae = -1j * (w0 / 2 * ae + Om * math.cos(w * t) * ag)
        dag = -1j * (-w0 / 2 * ag + Om * math.cos(w * t) * ae)
        return [dae.real, dae.imag, dag.real, dag.imag]

    sol = solve_ivp(rhs, (0, tp), [0, 0, 1, 0], method="DOP853", rtol=1e-12, atol=1e-14)
    Pf = sol.y[0, -1] ** 2 + sol.y[1, -1] ** 2
    b = lines_with(out, "(b)")[0]
    assert field(b, "P_e (full) =") == pytest.approx(Pf, rel=1e-7)
    assert field(b, "difference from RWA =") == pytest.approx(Pf - 0.8, rel=1e-3)
    assert abs(Pf - 0.8) < 2 * Om / w0

    # (c) Ramsey: three constant-Hamiltonian pieces, propagated by matrix exponentials
    tau, T = math.pi / (2 * Om), 1e-6
    rows = lines_with(out, "(c)")
    assert len(rows) == 2
    for d, line in zip((1e6, 3e6), rows):
        Hp = np.array([[0, Om / 2], [Om / 2, -d]])
        Hf = np.array([[0, 0], [0, -d]])
        psi = expm(-1j * Hp * tau) @ expm(-1j * Hf * T) @ expm(-1j * Hp * tau) @ np.array([1, 0])
        P = abs(psi[1]) ** 2
        W = math.hypot(Om, d)
        foot = (2 * Om / W) ** 2 * math.sin(W * tau / 2) ** 2 * (
            math.cos(W * tau / 2) * math.cos(d * T / 2) - d / W * math.sin(W * tau / 2) * math.sin(d * T / 2)) ** 2
        assert P == pytest.approx(foot, rel=1e-10)
        # the pulses switch on and off at kinks in t, which the solver locates: 1e-9
        assert field(line, "P_e (ODE) =") == pytest.approx(P, rel=1e-8)
        assert field(line, "Foot =") == pytest.approx(foot, rel=1e-9)
        assert field(line, "cos²(ΔT/2) =") == pytest.approx(math.cos(d * T / 2) ** 2, rel=1e-5)
