"""Gauntlet second pass, nuclear physics: run each problem and check it against independent answers."""
import csv
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "nuclear")
MEV = 1e6 * K.e
FM = 1e-15
HBAR = K.hbar
U = K.physical_constants["atomic mass constant"][0]
M_ALPHA = K.physical_constants["alpha particle mass"][0]
KE2 = K.e ** 2 / (4 * math.pi * K.epsilon_0)


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


# ---------------------------------------------------------------- 21 Woods–Saxon

def test_woods_saxon():
    out = run_problem("21_woods_saxon.fm")
    A, V0, r0, a = 40, 50.0 * MEV, 1.25 * FM, 0.65 * FM
    R = r0 * A ** (1 / 3)
    mu = K.m_n * A / (A + 1)
    assert field(out[0], "R =") == pytest.approx(R / FM, rel=1e-5)
    assert field(out[0], "f'(R) =") == pytest.approx(-1 / (4 * a), rel=1e-5)   # 1/m

    def dfdr(r):
        ex = math.exp((r - R) / a)
        return -ex / (a * (1 + ex) ** 2)

    def u_end(E, l_, ls, rtol=1e-12):
        rs = 1e-4 * FM
        def rhs(r, y):
            V = -V0 / (1 + math.exp((r - R) / a)) + 0.44 * V0 * r0 ** 2 * dfdr(r) * ls / r
            return [y[1], (l_ * (l_ + 1) / r ** 2 + 2 * mu / HBAR ** 2 * (V - E)) * y[0]]
        y0 = [(rs / FM) ** (l_ + 1), (l_ + 1) * (rs / FM) ** l_ / FM]
        return solve_ivp(rhs, (rs, 25 * FM), y0, method="DOP853", rtol=rtol, atol=1e-30).y[0, -1]

    rows = [ln for ln in out if " l =" in ln]
    expect = []
    for l_ in range(3):
        for twoj in (2 * l_ + 1, 2 * l_ - 1):
            if twoj <= 0:
                continue
            j = twoj / 2
            ls = (j * (j + 1) - l_ * (l_ + 1) - 0.75) / 2
            Es = np.arange(-V0 / MEV + 0.01, -5, 0.25)
            # a loose tolerance is enough to see the sign changes; the roots use 1e-12
            vals = [u_end(E * MEV, l_, ls, 1e-6) for E in Es]
            for k in range(len(Es) - 1):
                if vals[k] * vals[k + 1] < 0:
                    E = brentq(lambda E: u_end(E * MEV, l_, ls), Es[k], Es[k + 1], xtol=1e-13)
                    expect.append((l_, twoj, ls, E))
    assert len(rows) == len(expect) == 6
    for (l_, twoj, ls, E), line in zip(expect, rows):
        assert field(line, "l =") == l_ and field(line, "2j =") == twoj
        assert field(line, "<l.s> =") == pytest.approx(ls)
        # Fermium's RK45 at rtol 1e-9 against DOP853 at 1e-12; roots to double precision
        assert field(line, "E =") == pytest.approx(E, rel=1e-8)
    by = {(l_, twoj): [] for l_, twoj, _, _ in expect}
    for l_, twoj, _, e in expect:
        by[(l_, twoj)].append(e)
    dp = by[(1, 1)][0] - by[(1, 3)][0]
    dd = by[(2, 3)][0] - by[(2, 5)][0]
    assert dp > 0 and dd > dp
    b = [ln for ln in out if ln.startswith("(b)")][0]
    assert field(b, "splitting 1p =") == pytest.approx(dp, rel=1e-6)
    assert field(b, "1d =") == pytest.approx(dd, rel=1e-6)
    assert field(b, "ratio d/p =") == pytest.approx(dd / dp, rel=1e-5)
    order = [by[(0, 1)][0], by[(1, 3)][0], by[(1, 1)][0], by[(2, 5)][0], by[(0, 1)][1], by[(2, 3)][0]]
    assert order == sorted(order)
    assert out[-1].endswith("true")


# ---------------------------------------------------------------- 22 alpha decay

def emitters():
    with open(os.path.join(DIR, "data", "alpha_emitters.csv"), encoding="utf-8") as f:
        rows = list(csv.reader(f))[1:]
    return [(int(r[0]), int(r[1]), float(r[2]) * MEV, float(r[3])) for r in rows]


def gamow(Z, A, Q):
    Zd, Ad = Z - 2, A - 4
    mu = M_ALPHA * Ad * U / (M_ALPHA + Ad * U)
    R = 1.20 * FM * (Ad ** (1 / 3) + 4 ** (1 / 3))
    b = 2 * Zd * KE2 / Q
    G = quad(lambda r: math.sqrt(max(2 * mu * (2 * Zd * KE2 / r - Q), 0)) / HBAR, R, b,
             epsabs=0, epsrel=1e-13, limit=200)[0]
    # Krane's closed form for the same integral
    x = R / b
    G_closed = math.sqrt(2 * mu / (HBAR ** 2 * Q)) * (Zd * K.e ** 2 / (2 * math.pi * K.epsilon_0)) * \
        (math.acos(math.sqrt(x)) - math.sqrt(x * (1 - x)))
    assert G == pytest.approx(G_closed, rel=1e-11)
    t = math.log(2) * 2 * R / math.sqrt(2 * Q / mu) * math.exp(2 * G)
    return G, t, b


def test_alpha_gamow():
    out = run_problem("22_alpha_gamow.fm")
    rows = [ln for ln in out if ln.startswith("Z =")]
    em = emitters()
    assert len(rows) == len(em) == 9
    for (Z, A, Q, t_meas), line in zip(em, rows):
        G, t, _ = gamow(Z, A, Q)
        assert field(line, "Z =") == Z and field(line, "A =") == A
        # quadrature at 1e-10 relative, 8 digits printed
        assert field(line, "G =") == pytest.approx(G, rel=2e-8)
        # log10 t: an error δG in G gives 2δG/ln10 absolute; printed to 6 digits
        # printed to 6 digits (1e-4 resolution for 17.6654)
        assert field(line, "log10 t_calc/s =") == pytest.approx(math.log10(t), abs=1e-4)
        assert field(line, "log10 t_meas/s =") == pytest.approx(math.log10(t_meas), rel=1e-5, abs=1e-5)
        # the physics claim of part (a): within a factor 100 of experiment
        assert abs(math.log10(t) - math.log10(t_meas)) < 2
    Z, A, Q, _ = em[-1]
    G, _, b = gamow(Z, A, Q)
    bl = [ln for ln in out if ln.startswith("(b)")][0]
    assert field(bl, "b =") == pytest.approx(b / FM, rel=1e-7)
    assert field(bl, "G numeric =") == pytest.approx(G, rel=1e-9)
    assert field(bl, "G closed =") == pytest.approx(G, rel=1e-9)
    # (c) linear least squares on the same data
    X = np.array([(Z - 2) / math.sqrt(Q / MEV) for Z, _, Q, _ in em])
    Y = np.array([math.log10(t) for *_, t in em])
    a1, a2 = np.polyfit(X, Y, 1)
    c = out[-1]
    # nonlinear least squares reaches the linear optimum; 8 digits printed
    assert field(c, "a1 =") == pytest.approx(a1, rel=1e-6)
    assert field(c, "a2 =") == pytest.approx(a2, rel=1e-6)


# ---------------------------------------------------------------- 23 point kinetics

BETAS = np.array([0.000215, 0.001424, 0.001274, 0.002568, 0.000748, 0.000273])
LAMS = np.array([0.0124, 0.0305, 0.111, 0.301, 1.14, 3.01])
BETA = BETAS.sum()
LAMBDA = 5.0e-5


def test_point_kinetics():
    out = run_problem("23_point_kinetics.fm")
    rho = 0.1 * BETA

    def rhs(t, y):
        n, C = y[0], y[1:]
        return np.concatenate([[(rho - BETA) / LAMBDA * n + LAMS @ C], BETAS * n / LAMBDA - LAMS * C])

    y0 = np.concatenate([[1.0], BETAS / (LAMS * LAMBDA)])
    # an implicit solver (the system is mildly stiff: prompt root ≈ −117 /s), tight tolerances
    sol = solve_ivp(rhs, (0, 300), y0, method="Radau", rtol=1e-12, atol=1e-12, dense_output=True)
    n = lambda t: sol.sol(t)[0]
    assert field(out[0], "beta =") == pytest.approx(BETA, rel=1e-6)
    # RK45 at rtol 1e-9 over ~3×10⁴ prompt time constants: 1e-7
    assert field(out[1], "n(0.5 s) =") == pytest.approx(n(0.5), rel=1e-7)
    assert field(out[1], "beta/(beta - rho) =") == pytest.approx(1 / 0.9, rel=1e-7)   # 8 digits
    assert field(out[2], "n(100 s) =") == pytest.approx(n(100), rel=1e-7)
    assert field(out[2], "n(300 s) =") == pytest.approx(n(300), rel=1e-7)

    inhour = lambda w: w * LAMBDA + np.sum(BETAS * w / (w + LAMS))
    w1 = brentq(lambda w: inhour(w) - rho, 1e-6, 10, xtol=1e-18)
    b = out[3]
    assert field(b, "omega_1 =") == pytest.approx(w1, rel=1e-9)
    assert field(b, "period =") == pytest.approx(1 / w1, rel=1e-7)
    w_late = math.log(n(300) / n(250)) / 50
    assert field(b, "late-time growth rate =") == pytest.approx(w_late, rel=1e-6)
    # the physics: the late growth rate is the stable period's, up to the slowest
    # decaying transient (ω₂ ≈ −0.013 /s, amplitude small): 1e-4
    assert w_late == pytest.approx(w1, rel=1e-4)
    s = np.sum(BETAS / LAMS)
    assert field(out[4], "sum beta_i/lambda_i =") == pytest.approx(s, rel=1e-7)
    assert field(out[4], "period estimate =") == pytest.approx(s / rho, rel=1e-7)
    r60 = inhour(1 / 60)
    assert field(out[5], "60 s period =") == pytest.approx(r60, rel=1e-7)
    cents = re.search(r"=\s+(\S+)\s+cents", out[5])
    assert cents and num(cents.group(1)) == pytest.approx(100 * r60 / BETA, rel=1e-5)
