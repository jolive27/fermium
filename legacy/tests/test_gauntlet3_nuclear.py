"""Gauntlet third pass (graduate), nuclear physics: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.integrate import quad
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "nuclear")

MEV = 1e6 * K.e
KEV = 1e3 * K.e
FM = 1e-15


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


# ---------------------------------------------------------------- 31 deuteron

def test_deuteron_square_well():
    out = run_problem("31_deuteron_square_well.fm")
    mu = K.m_p * K.m_n / (K.m_p + K.m_n)
    B, R, hbar = 2.2246 * MEV, 2.10 * FM, K.hbar
    kap = math.sqrt(2 * mu * B) / hbar
    kk = lambda V0: math.sqrt(2 * mu * (V0 - B)) / hbar
    V0 = brentq(lambda V: kk(V) / math.tan(kk(V) * R) + kap, 5 * MEV, 60 * MEV, xtol=1e-30)
    k = kk(V0)
    a = lines_with(out, "(a)")[0]
    assert field(a, "V0 =") == pytest.approx(V0 / MEV, rel=1e-9)
    assert field(a, "kR =") == pytest.approx(k * R, rel=1e-9)
    assert field(a, "kappa =") == pytest.approx(kap * FM, rel=1e-7)
    # Krane §4.2: V0 ≈ 35 MeV for R = 2.1 fm
    assert 30 < V0 / MEV < 40

    # (b) E1 must reproduce the input B (the solver doesn't know it); E2 > 0 is the lowest
    # box state above threshold: inside sin(k1 r), outside sin(k2 (L − r)), matched at R
    L = 40 * FM
    b = lines_with(out, "(b)")[0]
    assert field(b, "E1 =") == pytest.approx(-2.2246, rel=2e-6)

    def match(E):
        k1 = math.sqrt(2 * mu * (E + V0)) / hbar
        k2 = math.sqrt(2 * mu * E) / hbar
        return k1 * math.cos(k1 * R) * math.sin(k2 * (L - R)) + k2 * math.cos(k2 * (L - R)) * math.sin(k1 * R)

    Es = np.linspace(1e-4, 2, 4000) * MEV
    vals = [match(E) for E in Es]
    i0 = next(i for i in range(len(Es) - 1) if vals[i] * vals[i + 1] < 0)
    E2 = brentq(match, Es[i0], Es[i0 + 1], xtol=1e-30)
    assert field(b, "E2 =") == pytest.approx(E2 / MEV, rel=1e-5)

    s2 = math.sin(k * R) ** 2
    Pout = (s2 / (2 * kap)) / (R / 2 - math.sin(2 * k * R) / (4 * k) + s2 / (2 * kap))
    c = lines_with(out, "(c)")[0]
    assert field(c, "closed form =") == pytest.approx(Pout, rel=1e-7)
    assert field(c, "numerical =") == pytest.approx(Pout, rel=1e-5)

    # (d) ⟨r²⟩ of the exact wavefunction, by quadrature (the far wall at 40 fm is negligible)
    A2 = 1 / (R / 2 - math.sin(2 * k * R) / (4 * k) + s2 / (2 * kap))
    r2 = A2 * (quad(lambda x: x * x * math.sin(k * x) ** 2, 0, R, epsrel=1e-13)[0]
               + s2 * quad(lambda x: x * x * math.exp(-2 * kap * (x - R)), R, 40 * FM, epsrel=1e-13)[0])
    d = lines_with(out, "(d)")[0]
    assert field(d, "r_d =") == pytest.approx(0.5 * math.sqrt(r2) / FM, rel=1e-5)


# ---------------------------------------------------------------- 32 tritium

def test_tritium_beta_spectrum():
    out = run_problem("32_tritium_beta_spectrum.fm")
    Q = 18.591  # keV
    mc2 = K.m_e * K.c ** 2 / KEV
    al = K.alpha

    def F(T):
        E = T + mc2
        eta = 2 * al * E / math.sqrt(E * E - mc2 * mc2)
        return 2 * math.pi * eta / (1 - math.exp(-2 * math.pi * eta))

    N = lambda T: F(T) * math.sqrt((T + mc2) ** 2 - mc2 ** 2) * (T + mc2) * (Q - T) ** 2
    N0 = lambda T: math.sqrt((T + mc2) ** 2 - mc2 ** 2) * (T + mc2) * (Q - T) ** 2
    q = lambda f: quad(f, 0, Q, epsabs=0, epsrel=1e-12, limit=200)[0]
    Tm = q(lambda T: T * N(T)) / q(N)
    Tm0 = q(lambda T: T * N0(T)) / q(N0)
    a = lines_with(out, "(a)")[0]
    assert field(a, "<T> =") == pytest.approx(Tm, rel=1e-7)
    assert field(a, "without Coulomb =") == pytest.approx(Tm0, rel=1e-7)
    # measured mean energy 5.69–5.70 keV (e.g. Krane Fig. 9.3 discussion; NNDC)
    assert Tm == pytest.approx(5.69, abs=0.02)

    # (b) the peak by brentq on a central-difference derivative of N
    dN = lambda T, h=1e-5: (N(T + h) - N(T - h)) / (2 * h)
    Tp = brentq(dN, 0.5, 15, xtol=1e-12)
    assert field(lines_with(out, "(b)")[0], "most probable T =") == pytest.approx(Tp, rel=1e-7)

    W0 = 1 + Q / mc2

    def FW(W):
        eta = 2 * al * W / math.sqrt(W * W - 1)
        return 2 * math.pi * eta / (1 - math.exp(-2 * math.pi * eta))

    f = quad(lambda W: FW(W) * math.sqrt(W * W - 1) * W * (W0 - W) ** 2, 1, W0, epsabs=0, epsrel=1e-12)[0]
    c = lines_with(out, "(c)")[0]
    assert field(c, "W0 =") == pytest.approx(W0, rel=1e-9)
    assert field(c, "f =") == pytest.approx(f, rel=1e-7)
    logft = math.log10(f * 12.32 * 365.25 * 86400)
    assert field(c, "log10(f t) =") == pytest.approx(logft, abs=1e-4)
    # tabulated log ft = 3.05 for ³H (superallowed mirror transition; Krane §9.3)
    assert logft == pytest.approx(3.05, abs=0.01)
