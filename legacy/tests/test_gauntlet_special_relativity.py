"""Gauntlet, special relativity: run each problem and check it against independent answers."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                   "gauntlet", "special_relativity")
C = K.c
EV = K.e
GEV = 1e9 * EV
MEV = 1e6 * EV
YR = 365.25 * 86400          # Fermium's yr is the Julian year
LY = C * YR
MP = K.m_p * C ** 2          # proton rest energy, J


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def test_muon_time_dilation():
    out = run_problem("01_muon_time_dilation.fm")
    tau, H, v = 2.197e-6, 1907.0, 0.9952 * C
    g = 1 / math.sqrt(1 - 0.9952 ** 2)
    t_lab = H / v
    want = [g, t_lab * 1e6, t_lab / g * 1e6, 563 * math.exp(-t_lab / tau),
            563 * math.exp(-t_lab / g / tau), t_lab / (tau * math.log(563 / 408)), H / g]
    got = [field(line, "=") for line in out]
    assert len(got) == len(want)
    # plain closed-form arithmetic printed to 6 significant figures: 1e-5 relative
    for a, b in zip(got, want):
        assert a == pytest.approx(b, rel=1e-5)


def test_relativistic_rocket():
    out = run_problem("02_relativistic_rocket.fm")
    g = 9.81

    def rhs(t, y):
        v = y[0]
        return [g * (1 - (v / C) ** 2) ** 1.5, v, math.sqrt(1 - (v / C) ** 2)]

    T = 5 * YR
    sol = solve_ivp(rhs, (0, T), [0, 0, 0], rtol=1e-12, atol=[1e-6, 1e-3, 1e-12], dense_output=True)
    v5, x5, tau5 = sol.sol(T)
    gt = g * T / C
    assert v5 / C == pytest.approx(gt / math.sqrt(1 + gt ** 2), rel=1e-9)   # SciPy agrees with theory
    # Fermium's RK45 runs at rtol 1e-9; values printed to 10 digits -> 1e-8 relative
    assert field(out[0], "v(5 yr) =") == pytest.approx(v5 / C, rel=1e-8)
    assert field(out[1], "x(5 yr) =") == pytest.approx(x5 / LY, rel=1e-8)
    assert field(out[2], "tau(5 yr) =") == pytest.approx(tau5 / YR, rel=1e-8)
    # (b) midpoint of a 4.37 ly trip, found with brentq on SciPy's dense output
    d = 4.37 * LY
    t_mid = brentq(lambda t: sol.sol(t)[1] - d / 2, 0, T, xtol=1e-6)
    tau_exact = 2 * C / g * math.acosh(1 + g * d / (2 * C ** 2))
    assert field(out[3], "Earth time =") == pytest.approx(2 * t_mid / YR, rel=1e-8)
    assert field(out[3], "ship time =") == pytest.approx(tau_exact / YR, rel=1e-8)
    assert field(out[3], "exact ship time") == pytest.approx(tau_exact / YR, rel=1e-9)
    gt = g * t_mid / C
    assert field(out[4], "top speed =") == pytest.approx(gt / math.sqrt(1 + gt ** 2), rel=1e-8)
    # (c) velocity addition, from the closed-form ship speed
    gt = g * YR / C
    v1 = gt / math.sqrt(1 + gt ** 2)
    assert field(out[5], "ship =") == pytest.approx(v1, rel=1e-8)
    assert field(out[5], "probe =") == pytest.approx((v1 + 0.6) / (1 + 0.6 * v1), rel=1e-8)


def test_threshold_energies():
    out = run_problem("03_threshold_energy.fm")
    # (a) K = 6 m_p c², p = √(E² − m²c⁴)/c with E = 7 m_p c²
    assert field(out[0], "beam kinetic energy =") == pytest.approx(6 * MP / GEV, rel=1e-9)
    assert field(out[1], "beam momentum =") == pytest.approx(math.sqrt(48) * MP / GEV, rel=1e-9)
    # (b)
    mpi = 134.9768 * MEV
    assert field(out[2], "photon threshold =") == pytest.approx(((MP + mpi) ** 2 - MP ** 2) / (2 * MP) / MEV,
                                                                 rel=1e-9)
    # (c) exact head-on threshold: m_Δ² = m_p² + 2E_γ(E + pc); for E ≫ m_p this is (m_Δ² − m_p²)/4E_γ
    # to 1 part in 10²² — far below the 6 printed digits.
    mD, Eg = 1232 * MEV, 6.4e-4 * EV
    assert field(out[3], "GZK threshold =") == pytest.approx((mD ** 2 - MP ** 2) / (4 * Eg) / EV, rel=1e-5)
