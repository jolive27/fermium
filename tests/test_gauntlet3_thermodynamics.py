"""Gauntlet third pass (graduate), thermodynamics and statistical mechanics: run each problem and check it."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import quad
from scipy.optimize import brentq
from scipy.special import zeta

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "thermodynamics")

HBAR, KB, ME, EV = K.hbar, K.k, K.m_e, K.e


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


# ---------------------------------------------------------------- 31 Bose–Einstein condensation

def polylog(nu, z, terms=20000):
    """g_ν(z) = Σ z^k / k^ν for 0 < z < 1 (the series converges geometrically)."""
    s, zk = 0.0, 1.0
    for k in range(1, terms + 1):
        zk *= z
        s += zk / k ** nu
        if zk < 1e-18:
            break
    return s


def test_bose_einstein_condensation():
    out = run_problem("31_bose_einstein_condensation.fm")
    m = 86.909 * K.atomic_mass
    n = 2.5e19
    lam = lambda T: K.h / math.sqrt(2 * math.pi * m * KB * T)
    z32, z52 = zeta(1.5), zeta(2.5)
    a = lines_with(out, "(a)")[0]
    # improper integrals with an x^(-1/2) end point and an infinite range: 1e-9
    assert field(a, "zeta(3/2) =") == pytest.approx(z32, rel=1e-9)
    assert field(a, "zeta(5/2) =") == pytest.approx(z52, rel=1e-9)
    Tc = 2 * math.pi * HBAR ** 2 / (m * KB) * (n / z32) ** (2 / 3)
    assert field(out[1], "T_c =") == pytest.approx(Tc * 1e9, rel=1e-7)
    assert field(out[1], "n λ(T_c)³ =") == pytest.approx(z32, rel=1e-9)

    target = n * lam(1.5 * Tc) ** 3
    z = brentq(lambda z: polylog(1.5, z) - target, 0.01, 0.999999, xtol=1e-15)
    b = lines_with(out, "(b)")[0]
    assert field(b, "n λ³ =") == pytest.approx(target, rel=1e-9)
    assert field(b, "z =") == pytest.approx(z, rel=1e-9)
    assert field(b, "series g(3/2, z) =") == pytest.approx(polylog(1.5, z), rel=1e-9)
    # the classical estimate: nλ³ = (1.5)^(-3/2) ζ(3/2)
    assert target == pytest.approx(z32 / 1.5 ** 1.5, rel=1e-12)

    c = lines_with(out, "(c)")[0]
    assert field(c, "0.5 T_c =") == pytest.approx(1 - 0.5 ** 1.5, rel=1e-6)
    # Pathria eq. 7.1.38: C/Nk at T_c = 1.925
    assert field(c, "below T_c =") == pytest.approx(15 / 4 * z52 / z32, rel=1e-7)
    assert field(c, "below T_c =") == pytest.approx(1.9257, abs=1e-4)

    d = lines_with(out, "(d)")[0]
    C = 15 / 4 * polylog(2.5, z) / polylog(1.5, z) - 9 / 4 * polylog(1.5, z) / polylog(0.5, z)
    assert field(d, "1.5 T_c =") == pytest.approx(C, rel=1e-7)
    assert 1.5 < C < 15 / 4 * z52 / z32


# ---------------------------------------------------------------- 32 Fermi gas

def test_fermi_gas_sommerfeld():
    out = run_problem("32_fermi_gas_sommerfeld.fm")
    n = 2.65e28
    EF = HBAR ** 2 / (2 * ME) * (3 * math.pi ** 2 * n) ** (2 / 3)
    TF = EF / KB
    a = lines_with(out, "(a)")[0]
    assert field(a, "E_F =") == pytest.approx(EF / EV, rel=1e-7)
    assert field(a, "T_F =") == pytest.approx(TF, rel=1e-7)
    # Ashcroft & Mermin Table 2.1: Na, E_F = 3.24 eV, T_F = 3.77×10⁴ K
    assert field(a, "E_F =") == pytest.approx(3.24, abs=0.01)

    # dimensionless: ε = E/E_F, n/(g-prefactor E_F^(3/2)) = 2/3 at T = 0
    def mu_of(t):
        dens = lambda m: quad(lambda e: math.sqrt(e) / (math.exp(min((e - m) / t, 700)) + 1), 0, m + 60 * t,
                              epsabs=0, epsrel=1e-13, limit=200)[0]
        return brentq(lambda m: dens(m) - 2 / 3, 0.5, 1.1, xtol=1e-15)

    def heat(t, m):
        w = lambda e: math.sqrt(e) / (4 * math.cosh((e - m) / (2 * t)) ** 2)
        hi = m + 80 * t
        I = [quad(lambda e, p=p: w(e) * (e - m) ** p, 0, hi, epsabs=0, epsrel=1e-13, limit=200)[0] for p in (0, 1, 2)]
        return (I[2] - I[1] ** 2 / I[0]) / t ** 2 / (2 / 3)

    rows_b, rows_c = lines_with(out, "(b)"), lines_with(out, "(c)")
    for t, b, c in zip((0.1, 0.3), rows_b, rows_c):
        mu = mu_of(t)
        assert field(b, "mu/E_F =") == pytest.approx(mu, rel=1e-9)
        som = 1 - math.pi ** 2 / 12 * t ** 2 - math.pi ** 4 / 80 * t ** 4
        assert field(b, "Sommerfeld =") == pytest.approx(som, rel=1e-9)
        # the next Sommerfeld term is −(247π⁶/25920) t⁶: the difference is of that size
        assert abs(mu - som) < 1.5 * 247 * math.pi ** 6 / 25920 * t ** 6
        C = heat(t, mu)
        assert field(c, "C/(n k) =") == pytest.approx(C, rel=1e-7)
        assert field(c, "pi² t/2 =") == pytest.approx(math.pi ** 2 / 2 * t, rel=1e-7)
        # the low-T expansion C = (π²/2) t (1 − (3π²/10) t² + …) holds at t = 0.1 to O(t⁴)
    assert field(rows_c[0], "C/(n k) =") == pytest.approx(math.pi ** 2 / 2 * 0.1 * (1 - 3 * math.pi ** 2 / 10 * 0.01),
                                                          rel=5e-3)

    d = lines_with(out, "(d)")[0]
    t = 300 / TF
    assert field(d, "t =") == pytest.approx(t, rel=1e-5)
    assert field(d, "1 - mu/E_F =") == pytest.approx(math.pi ** 2 / 12 * t ** 2, rel=5e-3)
    assert field(d, "(π²/12) t² =") == pytest.approx(math.pi ** 2 / 12 * t ** 2, rel=1e-5)
