"""Gauntlet third pass (graduate), mechanics: run each problem and check it against independent answers."""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "mechanics")


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


# ---------------------------------------------------------------- 31 Kapitza pendulum

def kapitza(a, g, ell, w, t_end):
    rhs = lambda t, y: [y[1], -(g - a * w ** 2 * math.cos(w * t)) * math.sin(y[0]) / ell]
    return solve_ivp(rhs, (0, t_end), [math.pi - 0.1, 0.0], method="DOP853", rtol=1e-12, atol=1e-14,
                     dense_output=True)


def test_kapitza_pendulum():
    out = run_problem("31_kapitza_pendulum.fm")
    g, ell, a, f = 9.81, 0.20, 0.01, 100.0
    w = 2 * math.pi * f
    A = line_with(out, "(a)")
    assert field(A, "a²ω² =") == pytest.approx(a ** 2 * w ** 2, rel=1e-6)
    assert A.endswith("true")
    W0 = math.sqrt(g / ell + a ** 2 * w ** 2 / (2 * ell ** 2))
    Wpi = math.sqrt(a ** 2 * w ** 2 / (2 * ell ** 2) - g / ell)
    assert field(out[1], "Omega_0 =") == pytest.approx(W0, rel=1e-7)
    assert field(out[1], "Omega_pi =") == pytest.approx(Wpi, rel=1e-7)
    assert field(out[1], "slow period about pi =") == pytest.approx(2 * math.pi / Wpi, rel=1e-7)

    # (b) the same strobe-and-interpolate measurement on SciPy's DOP853 solution
    sol = kapitza(a, g, ell, w, 5.0)
    Td = 1 / f
    N = int(math.floor(5.0 / Td + 1e-9))
    s = sol.sol(np.arange(N + 1) * Td)[0] - math.pi
    cr = [(n - 1) * Td + Td * s[n - 1] / (s[n - 1] - s[n]) for n in range(1, N + 1)
          if (s[n - 1] < 0 <= s[n]) or (s[n - 1] > 0 >= s[n])]
    T_slow = 2 * (cr[-1] - cr[0]) / (len(cr) - 1)
    B = line_with(out, "(b)")
    assert field(B, "strobed crossings:") == len(cr)
    # both solvers at ~1e-11: the strobed samples agree to ~1e-9, the period to 1e-7
    assert field(B, "slow period =") == pytest.approx(T_slow, rel=1e-7)
    # Kapitza's averaging is exact to O((Ω/ω)²) ≈ 1e-3 here
    rel = field(out[3], "averaged theory:")
    assert rel == pytest.approx((T_slow - 2 * math.pi / Wpi) / (2 * math.pi / Wpi), rel=1e-3)
    assert abs(rel) < 3e-3
    assert field(out[4], "|theta - pi| =") == pytest.approx(np.max(np.abs(s)), rel=1e-6)

    # (c) the weaker drive falls over
    C = line_with(out, "(c)")
    assert C.endswith("false")
    sol2 = kapitza(0.003, g, ell, w, 1.0)
    fall = np.max(np.abs(sol2.sol(np.arange(101) * Td)[0] - math.pi))
    assert field(out[-1], "in 1 s =") == pytest.approx(fall, rel=1e-6)
    assert fall > 0.4


# ---------------------------------------------------------------- 32 heavy symmetric top

def test_heavy_symmetric_top():
    out = run_problem("32_heavy_symmetric_top.fm")
    M, ell, I1, I3, w3, g = 0.150, 0.04, 1.2e-4, 6e-5, 200.0, 9.81
    a = I3 * w3 / I1
    beta = 2 * M * g * ell / I1
    u0 = math.cos(math.radians(60.0))
    b = a * u0
    assert field(out[0], "a =") == pytest.approx(a, rel=1e-6)
    assert field(out[0], "beta =") == pytest.approx(beta, rel=1e-6)

    # (a) the turning points: the roots of the cubic f(u) by numpy
    # f(u) = (1 − u²)(α − βu) − (b − au)² with α = βu₀
    alpha = beta * u0
    cubic = np.polysub(np.polymul([-1, 0, 1], [-beta, alpha]), np.polymul([-a, b], [-a, b]))
    roots = np.sort(np.real(np.roots(cubic)[np.abs(np.imag(np.roots(cubic))) < 1e-9]))
    u2 = [r for r in roots if -1 < r < u0 - 1e-9][0]
    A = line_with(out, "(a)")
    assert field(A, "u2 =") == pytest.approx(u2, rel=1e-9)
    assert field(A, "theta2 =") == pytest.approx(math.degrees(math.acos(u2)), rel=1e-9)

    # (b) the quadratures by QUADPACK. With u3 = a²/β − u2 the other root of the quadratic,
    # f(u) = β (u0 − u)(u − u2)(u3 − u), and u = u2 + (u0 − u2) sin²χ removes both 1/√ end points:
    # du/√f = 2 dχ / √(β (u3 − u))
    u3 = a * a / beta - u2
    d = u0 - u2

    def smooth(h):
        def integrand(chi):
            u = u2 + d * math.sin(chi) ** 2
            return h(u) * 2 / math.sqrt(beta * (u3 - u))
        return 2 * quad(integrand, 0, math.pi / 2, epsabs=0, epsrel=1e-13)[0]

    T = smooth(lambda u: 1.0)
    dphi = smooth(lambda u: (b - a * u) / (1 - u * u))
    B = line_with(out, "(b)")
    assert field(B, "nutation period T =") == pytest.approx(T, rel=1e-9)
    assert field(out[3], "precession per nutation =") == pytest.approx(dphi, rel=1e-9)
    assert field(out[3], "mean rate =") == pytest.approx(dphi / T, rel=1e-9)
    assert field(out[4], "M g l/(I3 w3) =") == pytest.approx(M * g * ell / (I3 * w3), rel=1e-6)
    # the fast-top value is approached, to O(β/a²) ≈ 10 %
    assert dphi / T == pytest.approx(M * g * ell / (I3 * w3), rel=0.05)

    # (c) the equations of motion by SciPy
    def rhs(t, y):
        th, thd, _ = y
        pd = (b - a * math.cos(th)) / math.sin(th) ** 2
        return [thd, pd ** 2 * math.sin(th) * math.cos(th) - a * pd * math.sin(th) + beta / 2 * math.sin(th), pd]

    sol = solve_ivp(rhs, (0, 1), [math.radians(60.0), 0, 0], method="DOP853", rtol=1e-13, atol=1e-15,
                    dense_output=True)
    t1 = brentq(lambda t: sol.sol(t)[1], 0.25 * T, 0.75 * T, xtol=1e-15)
    C = line_with(out, "(c)")
    assert field(C, "first turning time =") == pytest.approx(t1, rel=1e-8)
    assert field(C, "first turning time =") == pytest.approx(T / 2, rel=1e-8)
    assert field(C, "theta there =") == pytest.approx(math.degrees(math.acos(u2)), rel=1e-9)
    assert field(out[6], "2 t1 =") == pytest.approx(T, rel=1e-8)
    assert field(out[6], "phi(T) =") == pytest.approx(dphi, rel=1e-8)
    assert field(out[7], "10 nutations =") == pytest.approx(60.0, rel=1e-7)
