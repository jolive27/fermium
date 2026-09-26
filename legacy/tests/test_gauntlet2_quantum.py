"""Gauntlet second pass, quantum mechanics: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.integrate import quad
from scipy.linalg import eigh, eigh_tridiagonal
from scipy.optimize import brentq
from scipy.special import eval_hermite, gammainc

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "quantum")

HBAR = K.hbar
ME = K.m_e
EV = K.e
NM = 1e-9


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    """The number printed right after `key` on a line."""
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def lines_with(out, prefix):
    return [ln for ln in out if ln.startswith(prefix)]


# ---------------------------------------------------------------- 21 finite well

def test_finite_well():
    out = run_problem("21_finite_well.fm")
    V0, a = 20.0 * EV, 0.500 * NM
    z0 = a * math.sqrt(2 * ME * V0) / HBAR
    assert field(out[0], "z0 =") == pytest.approx(z0, rel=1e-9)
    N = math.ceil(2 * z0 / math.pi)
    assert field(out[0], "N = ceil(2 z0/π) =") == N == 8

    # independent roots: brentq on the matching conditions written as k tan(ka) - κ
    # (even) and k cot(ka) + κ (odd), in energy, between the poles of tan
    def even(z):
        return z * math.sin(z) - math.sqrt(z0 ** 2 - z ** 2) * math.cos(z)

    def odd(z):
        return z * math.cos(z) + math.sqrt(z0 ** 2 - z ** 2) * math.sin(z)

    roots = []
    for j in range(8):
        for f, lo, hi in ((even, j * math.pi, j * math.pi + math.pi / 2),
                          (odd, j * math.pi + math.pi / 2, (j + 1) * math.pi)):
            hi = min(hi, z0)
            if lo < hi and f(lo + 1e-12) * f(hi - 1e-12) < 0:
                roots.append(brentq(f, lo + 1e-12, hi - 1e-12, xtol=1e-15))
    assert len(roots) == N
    rows = [ln for ln in out if ln.startswith(("even", "odd"))]
    assert len(rows) == N
    for z, line in zip(roots, rows):
        E = (HBAR * z / a) ** 2 / (2 * ME) - V0
        # the root is refined to double precision; 10 digits are printed
        assert field(line, "z =") == pytest.approx(z, rel=1e-9)
        assert field(line, "E =") == pytest.approx(E / EV, rel=1e-9)
    E_inf = math.pi ** 2 * HBAR ** 2 / (2 * ME * (2 * a) ** 2) - V0
    c = lines_with(out, "(c)")[0]
    assert field(c, "states found =") == N
    assert field(c, "infinite-well value") == pytest.approx(E_inf / EV, rel=1e-9)
    assert c.endswith(": true")
    # (d) P(outside) = [cos²z/κa] / [1 + sin 2z/(2z) + cos²z/(κa)] (from the integrals in closed form)
    z = roots[0]
    ka = math.sqrt(z0 ** 2 - z ** 2)
    inside = 1 + math.sin(2 * z) / (2 * z)
    outside = math.cos(z) ** 2 / ka
    # both integrals are adaptive quadratures at 1e-10 relative
    assert field(out[-1], "P(outside) =") == pytest.approx(outside / (inside + outside), rel=1e-8)


# ---------------------------------------------------------------- 22 quartic oscillator

BETA = 1.00 * EV / NM ** 4


def fd_levels(V, x_max, n_levels, npts=8000):
    x = np.linspace(-x_max, x_max, npts + 2)[1:-1]
    h = x[1] - x[0]
    t = HBAR ** 2 / (2 * ME * h ** 2)
    return eigh_tridiagonal(2 * t + V(x), -t * np.ones(npts - 1), select="i",
                            select_range=(0, n_levels - 1), eigvals_only=True)


def test_quartic_oscillator():
    out = run_problem("22_quartic_oscillator.fm")
    ell = (HBAR ** 2 / (2 * ME * BETA)) ** (1 / 6)
    eps = BETA * ell ** 4
    assert field(out[0], "length scale =") == pytest.approx(ell / NM, rel=1e-5)
    assert field(out[0], "energy scale =") == pytest.approx(eps / EV, rel=1e-5)
    rows = lines_with(out, "n =")
    assert len(rows) == 4
    fd = fd_levels(lambda x: BETA * x ** 4, 7 * ell, 4)
    # the dimensionless ground state of p² + x⁴ is 1.0603620905 (Hioe & Montroll 1975)
    assert field(rows[0], "E_shoot =") == pytest.approx(1.0603620905 * eps / EV, rel=1e-8)
    xt = lambda E: (E / BETA) ** 0.25
    for n, line in enumerate(rows):
        E_shoot = field(line, "E_shoot =")
        # finite differences, h ≈ 1e-3 ℓ: error ~ 1e-6 relative for these low states
        assert E_shoot == pytest.approx(fd[n] / EV, rel=2e-6)
        # WKB: the same quantisation condition by quad + brentq
        act = lambda E: 2 * quad(lambda x: math.sqrt(max(2 * ME * (E - BETA * x ** 4), 0)),
                                 -xt(E), xt(E), epsabs=0, epsrel=1e-12, limit=200)[0]
        E_wkb = brentq(lambda E: act(E) - 2 * math.pi * HBAR * (n + 0.5), 1e-3 * EV, 3 * EV, xtol=1e-30)
        # quadrature at 1e-10 and a double-precision root: 10 printed digits
        assert field(line, "E_WKB =") == pytest.approx(E_wkb / EV, rel=1e-9)
        assert field(line, "WKB error =") == pytest.approx((E_wkb / EV - E_shoot) / E_shoot, rel=1e-3)
    # WKB improves with n (the physics point of part (b))
    errs = [abs(field(ln, "WKB error =")) for ln in rows]
    assert errs == sorted(errs, reverse=True)

    # (c) closed form: E(α) = ħ²α/2m + 3β/(16α²), minimum at α³ = 3βm/(4ħ²)
    alpha = (3 * BETA * ME / (4 * HBAR ** 2)) ** (1 / 3)
    E_var = HBAR ** 2 * alpha / (2 * ME) + 3 * BETA / (16 * alpha ** 2)
    c = lines_with(out, "(c)")[0]
    # the minimum is found from dE/dα = 0 of a quadrature (Leibniz rule): 1e-9
    assert field(c, "alpha =") == pytest.approx(alpha * NM ** 2, rel=1e-8)
    assert field(c, "E_var =") == pytest.approx(E_var / EV, rel=1e-9)
    assert c.endswith("true")

    # (d) the same 4×4 matrix, with the matrix elements computed independently by
    # quadrature over Hermite functions (not the ladder-operator formulas)
    omega = 2 * HBAR * alpha / ME
    b = math.sqrt(HBAR / (ME * omega))

    def phi(n, y):
        return eval_hermite(n, y) * np.exp(-y ** 2 / 2) / math.sqrt(2 ** n * math.factorial(n) * math.sqrt(math.pi))

    ns = [0, 2, 4, 6]
    Hm = np.zeros((4, 4))
    for i, m in enumerate(ns):
        for j, n in enumerate(ns):
            # ⟨m|T|n⟩ = (ħω/2)(δ(2n+1) - ⟨m|y²|n⟩) with y = x/b; plus β b⁴ ⟨m|y⁴|n⟩
            y2 = quad(lambda y: phi(m, y) * y ** 2 * phi(n, y), -np.inf, np.inf, epsrel=1e-13)[0]
            y4 = quad(lambda y: phi(m, y) * y ** 4 * phi(n, y), -np.inf, np.inf, epsrel=1e-13)[0]
            T = HBAR * omega / 2 * ((2 * n + 1) * (m == n) - y2)
            Hm[i, j] = T + BETA * b ** 4 * y4
    w = eigh(Hm, eigvals_only=True)
    d = lines_with(out, "(d)")[0]
    assert field(d, "H(0,0) =") == pytest.approx(E_var / EV, rel=1e-9)
    # Jacobi rotations to machine precision; matrix elements by quad at 1e-13
    assert field(d, "lowest eigenvalue =") == pytest.approx(w[0] / EV, rel=1e-9)
    assert out[-2].strip().endswith("true")
    assert field(out[-1], "second even eigenvalue =") == pytest.approx(w[1] / EV, rel=1e-9)
    # the 4-state basis is variational: every eigenvalue lies above the true level
    assert w[0] >= fd[0] * (1 - 1e-6) and w[1] >= fd[2] * (1 - 1e-6)


# ---------------------------------------------------------------- 23 hydrogen

def test_hydrogen_radial():
    out = run_problem("23_hydrogen_radial.fm")
    Ry = ME * EV ** 4 / (32 * math.pi ** 2 * K.epsilon_0 ** 2 * HBAR ** 2) / EV
    assert Ry == pytest.approx(K.physical_constants["Rydberg constant times hc in eV"][0], rel=1e-9)
    assert field(out[0], "Ry =") == pytest.approx(Ry, rel=1e-9)
    rows = lines_with(out, "l =")
    got = [(int(field(ln, "l =")), int(field(ln, "n =")), field(ln, "E_shoot =")) for ln in rows]
    assert [(l_, n) for l_, n, _ in got] == [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3)]
    for (l_, n, E), line in zip(got, rows):
        # cutting at 60 a₀ shifts the n = 3 levels by ~6e-10 (SciPy DOP853 at rtol 1e-12
        # on the same cut problem gives −5.9e-10 and −3.3e-10); RK45 at 1e-9: 2e-9
        assert E == pytest.approx(-Ry / n ** 2, rel=2e-9)
        assert field(line, "-Ry/n² =") == pytest.approx(-Ry / n ** 2, rel=1e-9)
    # (b) ⟨r⟩ over [0, 30 a₀] in closed form (incomplete gamma functions):
    # 1s: u² ∝ r² e^{-2r};  2p: u² ∝ r⁴ e^{-r}  (r in a₀)
    r1s = 1.5 * gammainc(4, 60) / gammainc(3, 60)
    r2p = 5 * gammainc(6, 30) / gammainc(5, 30)
    assert r1s == pytest.approx(1.5, rel=1e-15)
    assert r2p == pytest.approx(5, rel=1e-6) and r2p < 5
    # the integrals ride on the RK45 solution at rtol 1e-9: 1e-8
    assert field(out[-1], "<r>_1s =") == pytest.approx(r1s, rel=1e-8)
    assert field(out[-1], "<r>_2p =") == pytest.approx(r2p, rel=1e-8)
