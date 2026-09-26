"""Gauntlet third pass (graduate), special relativity: run each problem and check it against independent answers."""
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

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "special_relativity")


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


# ---------------------------------------------------------------- 31 Thomas precession

def boost(b):
    """The pure boost matrix for a 3-velocity b (in units of c), with numpy."""
    b = np.asarray(b, float)
    b2 = b @ b
    g = 1 / math.sqrt(1 - b2)
    L = np.eye(4)
    L[0, 0] = g
    L[0, 1:] = L[1:, 0] = -g * b
    if b2 > 0:
        L[1:, 1:] += (g - 1) * np.outer(b, b) / b2
    return L


def rotation_angle(L):
    b = -L[0, 1:] / L[0, 0]
    R = L @ boost(-b)
    # what's left must be a pure spatial rotation about z
    assert np.allclose(R[0], [1, 0, 0, 0], atol=1e-9) and np.allclose(R[:, 0], [1, 0, 0, 0], atol=1e-9)
    return math.atan2(R[2, 1], R[1, 1])


def thomas(beta, N):
    g = 1 / math.sqrt(1 - beta * beta)
    Lam = boost([0, beta, 0])
    for j in range(1, N + 1):
        p = 2 * math.pi * j / N
        Up = Lam @ np.array([g, -g * beta * math.sin(p), g * beta * math.cos(p), 0])
        Lam = boost(Up[1:] / Up[0]) @ Lam
    return rotation_angle(Lam)


def test_thomas_precession():
    out = run_problem("31_thomas_precession.fm")
    b1, b2 = 0.6, 0.8
    g1, g2 = 1 / math.sqrt(1 - b1 * b1), 1 / math.sqrt(1 - b2 * b2)
    th = rotation_angle(boost([0, b2, 0]) @ boost([b1, 0, 0]))
    wigner = math.acos((g1 + g2) / (1 + g1 * g2))
    assert abs(th) == pytest.approx(wigner, rel=1e-12)
    a = lines_with(out, "(a)")[0]
    assert field(a, "Wigner angle =") == pytest.approx(math.degrees(th), rel=1e-9)
    assert field(a, "1 + γ1 γ2)) =") == pytest.approx(math.degrees(wigner), rel=1e-9)
    assert field(out[1], "composite speed =") == pytest.approx(math.sqrt(1 - 1 / (g1 * g2) ** 2), rel=1e-9)

    t1, t2 = thomas(0.6, 1000), thomas(0.6, 2000)
    b = lines_with(out, "(b)")[0]
    assert field(b, "N = 1000:") == pytest.approx(math.degrees(t1), rel=1e-9)
    assert field(b, "N = 2000:") == pytest.approx(math.degrees(t2), rel=1e-9)
    # Thomas: 2π(γ − 1) per orbit = 90° at β = 0.6; the step error is O(1/N²)
    assert (math.degrees(t1) - 90) / (math.degrees(t2) - 90) == pytest.approx(4, rel=1e-3)
    assert field(out[3], "(4 t2 - t1)/3 =") == pytest.approx(90.0, rel=1e-9)
    assert field(out[3], "2π(γ - 1) =") == pytest.approx(90.0, rel=1e-9)


# ---------------------------------------------------------------- 32 Sommerfeld precession

def test_sommerfeld_precession():
    out = run_problem("32_sommerfeld_precession.fm")
    c, me, hbar = K.c, K.m_e, K.hbar
    k = 79 * K.e ** 2 / (4 * math.pi * K.epsilon_0)
    r0 = 3 * hbar / (me * c)
    p0 = 0.5 * me * c
    L = r0 * p0
    E = math.sqrt(p0 ** 2 * c ** 2 + me ** 2 * c ** 4) - k / r0
    a = lines_with(out, "(a)")[0]
    assert field(a, "L/hbar =") == pytest.approx(1.5, rel=1e-7)
    assert field(a, "Z alpha =") == pytest.approx(79 * K.alpha, rel=1e-7)
    assert field(a, "E/(m c²) =") == pytest.approx(E / (me * c ** 2), rel=1e-9)
    assert a.endswith("true")

    # the same equations in units of r0 and r0/c, by DOP853, with events at r·p = 0 going down
    def rhs(t, y):
        x, yy, px, py = y
        Ek = math.sqrt(px * px + py * py + 1)                # momentum in units of m c
        r3 = (x * x + yy * yy) ** 1.5
        kk = k / (r0 * me * c ** 2)
        return [px / Ek, py / Ek, -kk * x / r3, -kk * yy / r3]

    ev = lambda t, y: y[0] * y[2] + y[1] * y[3]
    ev.direction = -1
    t_end = 3e-19 * c / r0
    sol = solve_ivp(rhs, (0, t_end), [1, 0, 0, 0.5], method="DOP853", rtol=1e-13, atol=1e-15, events=ev)
    te, ye = sol.t_events[0], sol.y_events[0]
    ang = [math.atan2(y[1], y[0]) for y in ye]
    adv = (ang[1] - ang[0]) % (2 * math.pi)
    somm = 2 * math.pi * (1 / math.sqrt(1 - (k / (L * c)) ** 2) - 1)
    assert adv == pytest.approx(somm, rel=1e-9)
    b = lines_with(out, "(b)")[0]
    assert field(b, "turning points found:") == len(te)
    assert field(b, "advance per radial period =") == pytest.approx(math.degrees(somm), rel=1e-7)
    assert field(out[2], "− 1) =") == pytest.approx(math.degrees(somm), rel=1e-7)
    assert field(out[3], "first turning angle =") == pytest.approx(math.degrees(ang[0]), rel=1e-7)
    assert field(out[3], "|r| there =") == pytest.approx(math.hypot(*ye[0][:2]), rel=1e-7)

    pr2 = lambda x: ((E + k / x) ** 2 - me ** 2 * c ** 4) / c ** 2 - L ** 2 / x ** 2
    rmax = brentq(pr2, 1.01 * r0, 20 * r0, xtol=1e-30)
    Tr = (te[1] - te[0]) * r0 / c
    # T_r by QUADPACK with the square-root end points removed by x = mid − half cos χ
    mid, half = (r0 + rmax) / 2, (rmax - r0) / 2
    Tq = 2 * quad(lambda ch: (E + k / (mid - half * math.cos(ch))) * half * math.sin(ch)
                  / (c ** 2 * math.sqrt(max(pr2(mid - half * math.cos(ch)), 1e-300))), 0, math.pi,
                  epsabs=0, epsrel=1e-12, limit=200)[0]
    assert Tr == pytest.approx(Tq, rel=1e-9)
    cl = lines_with(out, "(c)")[0]
    assert field(cl, "r_max/r0 =") == pytest.approx(rmax / r0, rel=1e-9)
    assert field(cl, "T_r (ODE) =") == pytest.approx(Tq, rel=1e-8)
    assert field(out[5], "T_r (quadrature) =") == pytest.approx(Tq, rel=1e-8)
