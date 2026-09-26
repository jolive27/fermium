"""Gauntlet third pass (graduate), oscillations: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq
from scipy.special import ai_zeros

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "oscillations")


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    """The number printed right after `key` on a line."""
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def line_with(out, prefix):
    rows = [ln for ln in out if ln.lstrip().startswith(prefix)]
    assert rows, f"no line starts with {prefix!r}"
    return rows[0]


# ---------------------------------------------------------------- 31 anharmonic frequency shift

def test_anharmonic_frequency_shift():
    out = run_problem("31_anharmonic_frequency_shift.fm")
    w0, al, be = 2.0, 0.3, 0.5
    k_LL = 3 * be / (8 * w0) - 5 * al ** 2 / (12 * w0 ** 3)
    assert field(out[0], "Landau coefficient =") == pytest.approx(k_LL, rel=1e-8)
    U = lambda x: 0.5 * w0 ** 2 * x ** 2 + al * x ** 3 / 3 + be * x ** 4 / 4
    rows = [ln for ln in out if ln.startswith("x0 =")]
    assert len(rows) == 4
    diffs = []
    for x0, line in zip([0.05, 0.10, 0.20, 0.40], rows):
        xm = brentq(lambda x: U(x) - U(x0), -2 * x0, -0.01 * x0, xtol=1e-16)
        # the period by QUADPACK with the square-root end points removed by x = mid + half·sin φ
        mid, half = (x0 + xm) / 2, (x0 - xm) / 2
        T = 2 * quad(lambda p: half * math.cos(p) / math.sqrt(max(2 * (U(x0) - U(mid + half * math.sin(p))), 1e-300)),
                     -math.pi / 2, math.pi / 2, epsabs=0, epsrel=1e-13, limit=200)[0]
        # the ODE period by DOP853 with an event at x' = 0
        ev = lambda t, y: y[1]
        ev.direction = 1
        sol = solve_ivp(lambda t, y: [y[1], -w0 ** 2 * y[0] - al * y[0] ** 2 - be * y[0] ** 3], (0, 10), [x0, 0],
                        method="DOP853", rtol=1e-13, atol=1e-16, events=ev)
        T_ode = 2 * sol.t_events[0][0]
        assert T_ode == pytest.approx(T, rel=1e-10)
        assert field(line, "x_minus =") == pytest.approx(xm, rel=1e-9)
        # 11 printed digits; the Fermium ODE at tolerance 1e-12 and the quadrature at 1e-10
        assert field(line, "T_ode =") == pytest.approx(T, rel=1e-10)
        assert field(line, "T_exact =") == pytest.approx(T, rel=1e-10)
        coef = (2 * math.pi / T - w0) / x0 ** 2
        # (ω − ω0)/x0² loses ~4 digits to the cancellation; 8 are printed
        assert field(line, "coefficient =") == pytest.approx(coef, rel=1e-6)
        diffs.append(abs(coef - k_LL))
    # (c) the difference from Landau's coefficient is O(x0): it roughly doubles with x0
    for d1, d2 in zip(diffs, diffs[1:]):
        assert 1.6 < d2 / d1 < 2.4
    assert diffs[0] / k_LL < 3e-3


# ---------------------------------------------------------------- 32 van der Pol

def vdp_cycle(mu, t_end, method, rtol):
    f = lambda t, y: [y[1], mu * (1 - y[0] ** 2) * y[1] - y[0]]
    ev = lambda t, y: y[0]
    ev.direction = 1
    return solve_ivp(f, (0, t_end), [2.0, 0.0], method=method, rtol=rtol, atol=1e-12, events=ev,
                     dense_output=True)


def test_van_der_pol():
    out = run_problem("32_van_der_pol_relaxation.fm")
    # SciPy's own event location for the upward crossings, and the amplitude on the same grid
    for key, mu, t_end, n, method in (("(a)", 0.1, 400, 20, "DOP853"), ("(b)", 10, 400, 10, "DOP853")):
        sol = vdp_cycle(mu, t_end, method, 1e-12)
        ups = sol.t_events[0]
        T = (ups[-1] - ups[-1 - n]) / n
        line = line_with(out, key)
        assert field(line, "T =") == pytest.approx(T, rel=1e-8)
        grid = t_end / 2 + np.arange(1, 20001) * t_end / 40000
        assert field(line, "amplitude =") == pytest.approx(np.max(sol.sol(grid)[0]), rel=1e-6)
    a = line_with(out, "(a)")
    # Lindstedt: T = 2π(1 + μ²/16 − 5μ⁴/3072 + …); the printed value differs by the μ⁴ term
    T_a = field(a, "T =")
    assert field(a, "2π(1 + μ²/16) =") == pytest.approx(2 * math.pi * (1 + 0.01 / 16), rel=1e-8)
    assert T_a == pytest.approx(2 * math.pi * (1 + 0.01 / 16 - 5e-4 / 3072), rel=2e-8)
    assert field(a, "amplitude =") == pytest.approx(2.0, rel=1e-3)
    # published: the μ = 10 period is 19.0784 (e.g. Strogatz Fig. 7.5.x; Grasman Table 1)
    assert field(line_with(out, "(b)"), "T =") == pytest.approx(19.0784, abs=1e-4)

    c = line_with(out, "(c)")
    sol = vdp_cycle(100, 1000, "Radau", 1e-10)
    ups = sol.t_events[0]
    assert field(c, "crossings =") == len(ups)
    assert field(c, "T =") == pytest.approx((ups[-1] - ups[-4]) / 3, rel=1e-7)
    a0 = -ai_zeros(1)[0][0]
    mu = 100
    T_asym = (3 - 2 * math.log(2)) * mu + 3 * a0 * mu ** (-1 / 3) - 2 / 3 * math.log(mu) / mu
    assert field(c, "asymptotic =") == pytest.approx(T_asym, rel=1e-6)
    # the asymptotic series is good to O(1/μ)
    assert abs(field(c, "T =") - T_asym) < 2.0 / mu
