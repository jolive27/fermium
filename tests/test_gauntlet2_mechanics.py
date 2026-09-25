"""Gauntlet, second pass, mechanics: run gauntlet/mechanics/2*.fm and check the numbers independently.

References: closed forms (principal moments, linear stability rates, the small-oscillation modes of
the double pendulum, the cycloid and the straight line), SciPy's complete elliptic integral
(`ellipk`, for Landau's exact free-top solution), `solve_ivp` (DOP853 at rtol 1e-12) with event
location, `brentq` and `quad`.

Tolerances: values printed to 6-8 significant figures are compared at rel 2e-5 (rounding of a
6-digit number is up to 5e-6 plus RK45/regula-falsi error at the 1e-9 level). Chaotic separations
are compared at 3 %: the separation grows ~e^{1.1 t}, so both integrators' 1e-12 local errors are
amplified along with it; 3 % is far above that and far below the factor ~100 between 4 s and 8 s.
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq
from scipy.special import ellipk

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIR = os.path.join(ROOT, "gauntlet", "mechanics")
REL = 2e-5

_cache = {}
_SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
_NUM = re.compile(r"(?<![\w.])-?\d+(?:\.\d+)?(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+|e-?\d+)?(?![\d./])")


def output(name):
    if name not in _cache:
        with open(os.path.join(DIR, name + ".fm"), encoding="utf-8") as f:
            _cache[name] = run(f.read(), base_dir=DIR)
    return _cache[name]


def vals(name, label):
    """All numbers printed after `label:` on the line that starts with it."""
    for line in output(name).split("\n"):
        if line.startswith(label + ":"):
            rest = line[len(label) + 1:]
            return [float(re.sub(r"×10(\S+)", lambda m: "e" + m.group(1).translate(_SUP), t))
                    for t in _NUM.findall(rest)]
    raise AssertionError(f"no line {label!r} in output of {name}:\n{output(name)}")


def val(name, label, i=0):
    return vals(name, label)[i]


# ---------------------------------------------------------------- 21 intermediate axis

N21 = "21_intermediate_axis"
_m, _a, _b, _c = 0.2, 0.15, 0.075, 0.008
I1, I2, I3 = _m * (_b**2 + _c**2) / 12, _m * (_a**2 + _c**2) / 12, _m * (_a**2 + _b**2) / 12


def _euler(t, w):
    return [(I2 - I3) / I1 * w[1] * w[2], (I3 - I1) / I2 * w[2] * w[0], (I1 - I2) / I3 * w[0] * w[1]]


def test_intermediate_axis_linear_stability():
    assert vals(N21, "principal moments") == pytest.approx([I1, I2, I3], rel=REL)
    w0 = 10.0
    assert val(N21, "wobble frequency about axis 1") == pytest.approx(
        w0 * math.sqrt((I2 - I1) * (I3 - I1) / (I2 * I3)), rel=REL)
    assert val(N21, "wobble frequency about axis 3") == pytest.approx(
        w0 * math.sqrt((I3 - I1) * (I3 - I2) / (I1 * I2)), rel=REL)
    assert val(N21, "growth rate about axis 2") == pytest.approx(
        w0 * math.sqrt((I2 - I1) * (I3 - I2) / (I1 * I3)), rel=REL)


def test_intermediate_axis_conservation_and_flips():
    w = np.array([0.0, 10.0, 0.1])
    E = 0.5 * (I1 * w[0]**2 + I2 * w[1]**2 + I3 * w[2]**2)
    L2 = (I1 * w[0])**2 + (I2 * w[1])**2 + (I3 * w[2])**2
    e0, _, e20 = vals(N21, "kinetic energy")          # the middle number is the "20" of "after 20 s"
    assert e0 == pytest.approx(E, rel=1e-8) and e20 == pytest.approx(E, rel=1e-8)
    l0, _, l20 = vals(N21, "|L|")
    assert l0 == pytest.approx(math.sqrt(L2), rel=1e-8) and l20 == pytest.approx(math.sqrt(L2), rel=1e-8)

    def cross(t, y):
        return y[1]
    s = solve_ivp(_euler, [0, 20], w, method="DOP853", rtol=1e-12, atol=1e-14, events=cross,
                  dense_output=True)
    # printed to 3 significant figures
    assert vals(N21, "ω at 5 s") == pytest.approx(s.sol(5.0), rel=2e-3)
    # Landau's elliptic-function solution (L² > 2EI₂ here)
    assert L2 > 2 * E * I2
    rate = math.sqrt((I3 - I2) * (L2 - 2 * E * I1) / (I1 * I2 * I3))
    k2 = (I2 - I1) * (2 * E * I3 - L2) / ((I3 - I2) * (L2 - 2 * E * I1))
    K = ellipk(k2)
    assert vals(N21, "k²") == pytest.approx([k2, K], rel=1e-8)
    t1, t2 = s.t_events[0][:2]
    assert vals(N21, "first flip, ODE vs Landau") == pytest.approx([t1, K / rate], rel=REL)
    assert vals(N21, "time between flips, ODE vs Landau") == pytest.approx([t2 - t1, 2 * K / rate], rel=REL)


def test_intermediate_axis_stable_axes():
    ts = np.arange(0, 20.0 + 1e-9, 0.01)
    p = solve_ivp(_euler, [0, 20], [10.0, 0.1, 0.1], method="DOP853", rtol=1e-12, atol=1e-14, t_eval=ts)
    q = solve_ivp(_euler, [0, 20], [0.1, 0.1, 10.0], method="DOP853", rtol=1e-12, atol=1e-14, t_eval=ts)
    big1 = max(np.abs(p.y[1]).max(), np.abs(p.y[2]).max())
    big3 = max(np.abs(q.y[0]).max(), np.abs(q.y[1]).max())
    assert val(N21, "largest wobble, spin about axis 1") == pytest.approx(big1, rel=1e-3)
    assert val(N21, "largest wobble, spin about axis 3") == pytest.approx(big3, rel=1e-3)
    assert big1 < 0.2 and big3 < 0.2      # the wobble stays of the order of the 0.1 rad/s kick


# ---------------------------------------------------------------- 22 double pendulum

N22 = "22_double_pendulum"
G = 9.81


def _dp(t, y):
    a, b, wa, wb = y
    d = a - b
    A = np.array([[2.0, math.cos(d)], [math.cos(d), 1.0]])
    rhs = np.array([-wb**2 * math.sin(d) - 2 * G * math.sin(a), wa**2 * math.sin(d) - G * math.sin(b)])
    acc = np.linalg.solve(A, rhs)
    return [wa, wb, acc[0], acc[1]]


def _dp_run(a0, b0, T):
    return solve_ivp(_dp, [0, T], [a0, b0, 0, 0], method="DOP853", rtol=1e-12, atol=1e-14, dense_output=True)


def _energy(y):
    a, b, wa, wb = y
    return wa**2 + 0.5 * wb**2 + wa * wb * math.cos(a - b) - 2 * G * math.cos(a) - G * math.cos(b)


def test_double_pendulum_normal_modes():
    assert vals(N22, "slow mode") == pytest.approx([math.sqrt(G * (2 - math.sqrt(2)))] * 2, rel=REL)
    assert vals(N22, "fast mode") == pytest.approx([math.sqrt(G * (2 + math.sqrt(2)))] * 2, rel=REL)
    assert vals(N22, "slow mode shape θ₂/θ₁") == pytest.approx([math.sqrt(2), 2, math.sqrt(2)], rel=REL)

    def zero(t, y):
        return y[0]
    zero.direction = -1
    a0 = math.radians(1.0)
    s = solve_ivp(_dp, [0, 3], [a0, math.sqrt(2) * a0, 0, 0], method="DOP853", rtol=1e-12, atol=1e-14,
                  events=zero)
    tq = s.t_events[0][0]
    assert val(N22, "slow mode from the ODE at 1°") == pytest.approx(2 * math.pi / (4 * tq), rel=REL)


def test_double_pendulum_energy_and_trajectory():
    a0 = math.radians(120.0)
    s = _dp_run(a0, a0, 20)
    E0 = _energy([a0, a0, 0, 0])
    assert E0 == pytest.approx(14.715)
    e0, e20 = val(N22, "energy at 0 s"), val(N22, "energy at 20 s")
    assert e0 == pytest.approx(E0, rel=1e-9)
    assert e20 == pytest.approx(E0, rel=1e-8)     # conserved over 20 s of chaotic motion
    y2 = s.sol(2.0)
    assert vals(N22, "θ₁, θ₂ at 2 s") == pytest.approx(np.degrees(y2[:2]), rel=REL)


def test_double_pendulum_chaos():
    a0 = math.radians(120.0)
    p = _dp_run(a0, a0, 12)
    q = _dp_run(a0, a0 + 1e-8, 12)

    def sep(t):
        return math.degrees(abs(p.sol(t)[0] - q.sol(t)[0]) + abs(p.sol(t)[1] - q.sol(t)[1]))
    for t in (4, 8):
        assert val(N22, f"separation at {t} s") == pytest.approx(sep(t), rel=3e-2)
    assert val(N22, "separation at 12 s") > 1.0       # saturated: of order the angles themselves
    lyap = math.log(sep(8) / sep(4)) / 4
    assert val(N22, "Lyapunov estimate (4 s to 8 s)") == pytest.approx(lyap, rel=3e-2)
    assert lyap > 0.5

    g0 = math.radians(10.0)
    r = _dp_run(g0, g0, 12)
    u = _dp_run(g0, g0 + 1e-8, 12)
    gentle = math.degrees(abs(r.sol(12)[0] - u.sol(12)[0]) + abs(r.sol(12)[1] - u.sol(12)[1]))
    assert val(N22, "gentle start, separation at 12 s") == pytest.approx(gentle, rel=3e-2)
    assert gentle < 1e-5        # regular motion: no exponential growth from 5.7e-7 degrees


# ---------------------------------------------------------------- 23 brachistochrone

N23 = "23_brachistochrone"


def test_brachistochrone_cycloid_and_line():
    X, Y = 2.0, 1.0
    th = brentq(lambda t: (t - math.sin(t)) / (1 - math.cos(t)) - X / Y, 0.1, 2 * math.pi - 0.1, xtol=1e-14)
    a = Y / (1 - math.cos(th))
    T = th * math.sqrt(a / G)
    line = [ln for ln in output(N23).split("\n") if ln.startswith("cycloid: θ_f =")][0]
    got = [float(x) for x in re.findall(r"\d+\.\d+", line)]
    assert got == pytest.approx([th, a], rel=1e-6)
    assert val(N23, "cycloid time, closed form") == pytest.approx(T, rel=1e-6)
    assert val(N23, "cycloid time, functional") == pytest.approx(T, rel=1e-6)
    Tl = math.sqrt(2 * (X**2 + Y**2) / (G * Y))
    assert val(N23, "straight line, functional") == pytest.approx(Tl, rel=1e-6)
    assert val(N23, "straight line, closed form") == pytest.approx(Tl, rel=1e-6)


def test_brachistochrone_stationary_and_rolling():
    X, Y = 2.0, 1.0
    th = brentq(lambda t: (t - math.sin(t)) / (1 - math.cos(t)) - X / Y, 0.1, 2 * math.pi - 0.1, xtol=1e-14)
    a = Y / (1 - math.cos(th))

    def T(eps):
        def f(t):
            xp = a * (1 - math.cos(t))
            yp = a * math.sin(t) + eps * a * 2 * math.sin(math.pi * t / th) * math.cos(math.pi * t / th) * math.pi / th
            y = 2 * a * math.sin(t / 2)**2 + eps * a * math.sin(math.pi * t / th)**2
            return math.hypot(xp, yp) / math.sqrt(2 * G * y)
        return quad(f, 0, th, epsabs=1e-14, epsrel=1e-13, limit=200)[0]
    assert abs(val(N23, "dT/dε at ε = 0")) < 1e-10
    h = 1e-3
    d2 = (T(h) - 2 * T(0) + T(-h)) / h**2            # truncation error ~h² T'''' ~ 1e-7 relative
    assert val(N23, "d²T/dε² at ε = 0") == pytest.approx(d2, rel=1e-4)
    for label, eps in (("T(+0.01) − T(0)", 0.01), ("T(−0.01) − T(0)", -0.01), ("T(+0.02) − T(0)", 0.02)):
        assert val(N23, label) == pytest.approx(T(eps) - T(0), rel=1e-4)
        assert val(N23, label) > 0                   # the cycloid is a minimum
    assert val(N23, "ratio for doubling ε") == pytest.approx((T(0.02) - T(0)) / (T(0.01) - T(0)), rel=1e-3)
    assert val(N23, "½ T'' ε² for ε = 0.01") == pytest.approx(0.5 * d2 * 1e-4, rel=1e-4)
    T0 = th * math.sqrt(a / G)
    assert val(N23, "rolling ball on the cycloid") == pytest.approx(math.sqrt(7 / 5) * T0, rel=1e-6)
    assert vals(N23, "ratio to the sliding bead") == pytest.approx([math.sqrt(1.4), 5, math.sqrt(1.4)], rel=1e-6)


def test_all_second_pass_mechanics_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("2"))
    assert names == [N21, N22, N23]
