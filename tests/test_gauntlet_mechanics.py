"""Gauntlet, mechanics: run each gauntlet/mechanics/*.fm and check its numbers independently.

The references are closed-form formulas evaluated here, or SciPy (solve_ivp with events, quad).

Tolerance: the programs print 6 significant figures, so rounding alone gives a relative error up
to 5e-6 (for a leading digit 1). We use rel=2e-5: loose enough for that rounding plus the ODE
solver (RK45 at relative tolerance 1e-9) and bisection to 1e-9 s, tight enough that any
physics mistake (a wrong factor, a missing term, a 0.1 % slip) fails.
"""
import math
import os
import re

import pytest
from scipy.integrate import quad, solve_ivp

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


# ---------------------------------------------------------------- 01 projectile with drag

def _projectile():
    m, r, cd, rho, v0, th, g = 0.145, 0.037, 0.35, 1.20, 40.0, math.radians(35.0), 9.81
    k = 0.5 * rho * cd * math.pi * r * r / m

    def f(t, y):
        v = math.hypot(y[2], y[3])
        return [y[2], y[3], -k * v * y[2], -g - k * v * y[3]]

    def land(t, y):
        return y[1]
    land.terminal, land.direction = True, -1

    def apex(t, y):
        return y[3]
    s = solve_ivp(f, [0, 10], [0, 0, v0 * math.cos(th), v0 * math.sin(th)],
                  events=[land, apex], rtol=1e-12, atol=1e-12)
    return dict(k=k, v0=v0, th=th, g=g, t_land=s.t_events[0][0], land=s.y_events[0][0],
                apex=s.y_events[1][0])


def test_projectile_vacuum():
    p = _projectile()
    v0, th, g = p["v0"], p["th"], p["g"]
    n = "01_projectile_drag"
    assert val(n, "vacuum range") == pytest.approx(v0**2 * math.sin(2 * th) / g, rel=REL)
    assert val(n, "vacuum height") == pytest.approx((v0 * math.sin(th))**2 / (2 * g), rel=REL)
    assert val(n, "vacuum flight time") == pytest.approx(2 * v0 * math.sin(th) / g, rel=REL)
    assert val(n, "drag constant k") == pytest.approx(p["k"], rel=REL)


def test_projectile_drag():
    p = _projectile()
    n = "01_projectile_drag"
    R_vac = p["v0"]**2 * math.sin(2 * p["th"]) / p["g"]
    assert val(n, "drag range") == pytest.approx(p["land"][0], rel=REL)
    assert val(n, "drag height") == pytest.approx(p["apex"][1], rel=REL)
    assert val(n, "drag flight time") == pytest.approx(p["t_land"], rel=REL)
    assert val(n, "impact speed") == pytest.approx(math.hypot(*p["land"][2:]), rel=REL)
    # printed to 4 significant figures, in %
    assert val(n, "range lost") == pytest.approx(100 * (1 - p["land"][0] / R_vac), rel=2e-4)


# ---------------------------------------------------------------- 02 incline + pulley

def test_incline_pulley():
    n = "02_incline_pulley"
    m1, m2, th, M, R, mu, g = 4.0, 3.5, math.radians(30.0), 1.0, 0.10, 0.15, 9.81
    I = 0.5 * M * R**2
    Meff = m1 + m2 + I / R**2
    a = (m2 * g - m1 * g * math.sin(th) - mu * m1 * g * math.cos(th)) / Meff
    T1 = m1 * a + m1 * g * math.sin(th) + mu * m1 * g * math.cos(th)
    T2 = m2 * (g - a)
    assert val(n, "acceleration") == pytest.approx(a, rel=REL)
    assert val(n, "tension, incline side") == pytest.approx(T1, rel=REL)
    assert val(n, "tension, hanging side") == pytest.approx(T2, rel=REL)
    assert "torque check (T2 - T1) R = I a/R: true" in output(n)
    assert val(n, "minimum static coefficient") == pytest.approx(
        (m2 - m1 * math.sin(th)) / (m1 * math.cos(th)), rel=REL)

    beta = 0.4

    def F(x):
        return m2 * g - m1 * g * math.sin(th) - (mu + beta * x) * m1 * g * math.cos(th)

    def speed(s):
        return math.sqrt(2 * quad(F, 0, s, epsabs=1e-14)[0] / Meff)
    c = beta * m1 * g * math.cos(th)
    x_vmax = F(0) / c
    assert val(n, "speed after 0.50 m") == pytest.approx(speed(0.5), rel=REL)
    assert vals(n, "largest speed") == pytest.approx([speed(x_vmax), x_vmax], rel=REL)
    assert val(n, "stops after") == pytest.approx(2 * x_vmax, rel=REL)
    assert abs(val(n, "work done at the stop (should be 0)")) < 1e-9
    assert val(n, "stop from W(s) = 0") == pytest.approx(2 * x_vmax, rel=REL)

    t_half = math.pi * math.sqrt(Meff / c)
    assert val(n, "time to stop") == pytest.approx(t_half, rel=REL)
    assert val(n, "ODE position at that time") == pytest.approx(2 * x_vmax, rel=REL)
    assert val(n, "ODE speed after 0.50 m") == pytest.approx(speed(0.5), rel=REL)


# ---------------------------------------------------------------- 03 sounding rocket

def test_rocket_closed_form():
    n = "03_sounding_rocket"
    m0, mf_, tb, ve, g = 1200.0, 900.0, 60.0, 2200.0, 9.81
    mdot = mf_ / tb
    mf = m0 - mf_
    vb = ve * math.log(m0 / mf) - g * tb
    yb = ve * tb - 0.5 * g * tb**2 - ve * mf / mdot * math.log(m0 / mf)
    assert val(n, "thrust") == pytest.approx(ve * mdot / 1e3, rel=REL)            # kN
    assert val(n, "Tsiolkovsky Δv") == pytest.approx(ve * math.log(m0 / mf) / 1e3, rel=REL)
    assert val(n, "burnout speed (no air)") == pytest.approx(vb / 1e3, rel=REL)   # km/s
    assert val(n, "burnout height (no air)") == pytest.approx(yb / 1e3, rel=REL)  # km
    assert val(n, "apex (no air)") == pytest.approx((yb + vb**2 / (2 * g)) / 1e3, rel=REL)
    assert val(n, "ODE burnout speed (no air)") == pytest.approx(vb, rel=REL)     # m/s
    assert val(n, "ODE burnout height (no air)") == pytest.approx(yb / 1e3, rel=REL)


def test_rocket_with_drag():
    n = "03_sounding_rocket"
    m0, tb, ve, g = 1200.0, 60.0, 2200.0, 9.81
    mdot = 900.0 / tb
    mf = m0 - 900.0
    cdA = 0.5 * math.pi * 0.25**2

    def rho(y):
        return 1.225 * math.exp(-y / 8500.0)

    def powered(t, s):
        M, v, y = s
        return [-mdot, (ve * mdot - M * g - 0.5 * rho(y) * cdA * v * abs(v)) / M, v]
    p = solve_ivp(powered, [0, tb], [m0, 0, 0], rtol=1e-12, atol=1e-9)
    vb, yb = p.y[1, -1], p.y[2, -1]

    def coast(t, s):
        v, y = s
        return [-g - 0.5 * rho(y) * cdA * v * abs(v) / mf, v]

    def top(t, s):
        return s[0]
    top.terminal = True
    c = solve_ivp(coast, [tb, tb + 400], [vb, yb], events=top, rtol=1e-12, atol=1e-9)
    assert val(n, "burnout speed (air)") == pytest.approx(vb, rel=REL)
    assert val(n, "burnout height (air)") == pytest.approx(yb / 1e3, rel=REL)
    assert val(n, "time of apex (air)") == pytest.approx(c.t_events[0][0], rel=REL)
    assert val(n, "apex (air)") == pytest.approx(c.y_events[0][0][1] / 1e3, rel=REL)


def test_all_mechanics_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("0"))  # pass 1 only
    assert names == ["01_projectile_drag", "02_incline_pulley", "03_sounding_rocket"]
