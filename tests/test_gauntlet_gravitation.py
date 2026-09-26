"""Gauntlet, gravitation: run each gauntlet/gravitation/*.fm and check its numbers independently.

References: Kepler/vis-viva closed forms evaluated here, and scipy.integrate.solve_ivp with event
location for the orbits and the perihelion of the relativistic Binet equation.

Tolerance: the programs print 6 significant figures -> rel=2e-5 (rounding is at most 5e-6
relative; the rest covers RK45 at rtol 1e-9 over ~1 orbit plus root refinement). Any physics
slip (a missing factor 2 in vis-viva, a wrong a(1 - e²)) is far outside that.
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import solve_ivp

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIR = os.path.join(ROOT, "gauntlet", "gravitation")
REL = 2e-5
AU = 149597870700.0
YR = 365.25 * 86400
GM_SUN = 1.32712440018e20
GM_EARTH = 3.986004418e14

_cache = {}
_SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
_NUM = re.compile(r"(?<![\w.])-?\d+(?:\.\d+)?(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+|e-?\d+)?(?![\d./])")


def output(name):
    if name not in _cache:
        with open(os.path.join(DIR, name + ".fm"), encoding="utf-8") as f:
            _cache[name] = run(f.read(), base_dir=DIR)
    return _cache[name]


def numbers(text):
    return [float(re.sub(r"×10(\S+)", lambda m: "e" + m.group(1).translate(_SUP), t))
            for t in _NUM.findall(text)]


def vals(name, label):
    for line in output(name).split("\n"):
        if line.startswith(label + ":"):
            return numbers(line[len(label) + 1:])
    raise AssertionError(f"no line {label!r} in output of {name}:\n{output(name)}")


def val(name, label, i=0):
    return vals(name, label)[i]


def kepler_ode(GM):
    def f(t, y):
        r3 = math.hypot(y[0], y[1])**3
        return [y[2], y[3], -GM * y[0] / r3, -GM * y[1] / r3]
    return f


def radial(t, y):
    return y[0] * y[2] + y[1] * y[3]


# ---------------------------------------------------------------- 01 orbital elements

def test_orbit_elements_closed_form():
    n = "01_orbit_elements"
    r = np.array([1.5 * AU, 0.0])
    v = np.array([5.0e3, 22.0e3])
    eps = 0.5 * v @ v - GM_SUN / np.linalg.norm(r)
    h = r[0] * v[1] - r[1] * v[0]
    # eccentricity from the Laplace-Runge-Lenz vector (independent of the formula in the program)
    evec = np.array([v[1] * h, -v[0] * h]) / GM_SUN - r / np.linalg.norm(r)
    e = np.linalg.norm(evec)
    a = -GM_SUN / (2 * eps)
    assert val(n, "specific energy") == pytest.approx(eps / 1e6, rel=REL)          # km²/s²
    assert val(n, "angular momentum per kg") == pytest.approx(h / 1e6, rel=REL)    # km²/s
    assert "bound: true" in output(n)
    assert val(n, "semi-major axis") == pytest.approx(a / AU, rel=REL)
    assert val(n, "eccentricity") == pytest.approx(e, rel=REL)
    assert val(n, "perihelion") == pytest.approx(a * (1 - e) / AU, rel=REL)
    assert val(n, "aphelion") == pytest.approx(a * (1 + e) / AU, rel=REL)
    assert val(n, "period") == pytest.approx(2 * math.pi * math.sqrt(a**3 / GM_SUN) / YR, rel=REL)


def test_orbit_elements_ode():
    n = "01_orbit_elements"
    y0 = [1.5 * AU, 0.0, 5.0e3, 22.0e3]
    s = solve_ivp(kepler_ode(GM_SUN), [0, 3 * YR], y0, events=radial, dense_output=True,
                  rtol=1e-12, atol=1e-3)
    t_aph, t_peri = s.t_events[0][0], s.t_events[0][1]
    assert val(n, "aphelion from the ODE") == pytest.approx(
        math.hypot(*s.y_events[0][0][:2]) / AU, rel=REL)
    assert val(n, "perihelion from the ODE") == pytest.approx(
        math.hypot(*s.y_events[0][1][:2]) / AU, rel=REL)
    assert val(n, "period from the ODE") == pytest.approx(2 * (t_peri - t_aph) / YR, rel=REL)
    y1 = s.sol(YR)
    v2 = y1[2]**2 + y1[3]**2
    vv = vals(n, "vis-viva, v² at 1 yr")
    eps = 0.5 * (y0[2]**2 + y0[3]**2) - GM_SUN / y0[0]
    a = -GM_SUN / (2 * eps)
    assert vv == pytest.approx([v2 / 1e6, GM_SUN * (2 / math.hypot(y1[0], y1[1]) - 1 / a) / 1e6],
                               rel=REL)
    assert val(n, "angular momentum at 1 yr") == pytest.approx(
        (y1[0] * y1[3] - y1[1] * y1[2]) / 1e6, rel=REL)


# ---------------------------------------------------------------- 02 Hohmann transfer

def test_hohmann_closed_form():
    n = "02_hohmann_transfer"
    RE, Tsid = 6378.1e3, 86164.1
    r2 = (GM_EARTH * Tsid**2 / (4 * math.pi**2)) ** (1 / 3)
    r1 = RE + 300e3
    at = (r1 + r2) / 2
    vc1, vc2 = math.sqrt(GM_EARTH / r1), math.sqrt(GM_EARTH / r2)
    # transfer-orbit speeds from angular momentum + energy (not vis-viva as in the program):
    # v_p r1 = v_a r2 and v_p²/2 - GM/r1 = v_a²/2 - GM/r2
    vp = math.sqrt(2 * GM_EARTH * r2 / (r1 * (r1 + r2)))
    va = vp * r1 / r2
    assert val(n, "geostationary radius") == pytest.approx(r2 / 1e3, rel=REL)
    assert val(n, "geostationary altitude") == pytest.approx((r2 - RE) / 1e3, rel=REL)
    assert val(n, "LEO speed") == pytest.approx(vc1 / 1e3, rel=REL)
    assert val(n, "first burn") == pytest.approx((vp - vc1) / 1e3, rel=REL)
    assert val(n, "second burn") == pytest.approx((vc2 - va) / 1e3, rel=REL)
    assert val(n, "total") == pytest.approx((vp - vc1 + vc2 - va) / 1e3, rel=REL)
    assert val(n, "transfer time") == pytest.approx(
        math.pi * math.sqrt(at**3 / GM_EARTH) / 3600, rel=REL)
    assert val(n, "escape burn from LEO") == pytest.approx(
        (math.sqrt(2 * GM_EARTH / r1) - vc1) / 1e3, rel=REL)


def test_hohmann_ode():
    n = "02_hohmann_transfer"
    RE, Tsid = 6378.1e3, 86164.1
    r2 = (GM_EARTH * Tsid**2 / (4 * math.pi**2)) ** (1 / 3)
    r1 = RE + 300e3
    vp = math.sqrt(2 * GM_EARTH * r2 / (r1 * (r1 + r2)))

    def apogee(t, y):
        return radial(t, y)
    apogee.direction = -1               # r·v goes from + to - at apogee
    s = solve_ivp(kepler_ode(GM_EARTH), [0, 2e5], [r1, 0, 0, vp], events=apogee,
                  rtol=1e-12, atol=1e-6)
    ya = s.y_events[0][0]
    assert val(n, "ODE apogee radius") == pytest.approx(math.hypot(ya[0], ya[1]) / 1e3, rel=REL)
    assert val(n, "ODE time to apogee") == pytest.approx(s.t_events[0][0] / 3600, rel=REL)
    assert val(n, "ODE speed at apogee") == pytest.approx(math.hypot(ya[2], ya[3]) / 1e3, rel=REL)


# ---------------------------------------------------------------- 03 Mercury precession

def test_mercury_precession():
    n = "03_mercury_precession"
    c = 299792458.0
    a, e, P = 5.7909e10, 0.20563, 87.969 * 86400
    h2 = GM_SUN * a * (1 - e**2)
    per_century = 100 * YR / P
    arcsec = math.pi / (180 * 3600)
    theory = 6 * math.pi * GM_SUN / (c**2 * a * (1 - e**2))
    assert val(n, "predicted shift per orbit") == pytest.approx(theory, rel=REL)
    assert val(n, "predicted shift per century") == pytest.approx(
        theory * per_century / arcsec, rel=REL)

    # Independent: integrate the Binet equation in the scaled variable U = u a (order 1) so that
    # SciPy's absolute tolerance is meaningful, and locate the perihelion with an event.
    k = GM_SUN * a / h2
    eps = 3 * GM_SUN / (c**2 * a)

    def f(phi, y):
        return [y[1], k + eps * y[0]**2 - y[0]]

    def peri(phi, y):
        return y[1]
    peri.direction = -1                  # perihelion = maximum of U: U' goes from + to -
    s = solve_ivp(f, [0, 2.5 * math.pi], [1 / (1 - e), 0.0], events=peri,
                  rtol=1e-13, atol=1e-15, method="DOP853")
    phis = [p for p in s.t_events[0] if p > 1.5 * math.pi]
    shift = phis[0] - 2 * math.pi
    # the printed ODE shift has 6 digits; the SciPy shift is good to ~1e-13 rad absolute,
    # i.e. ~2e-7 relative to 5e-7 rad, well inside rel=2e-5
    assert val(n, "ODE shift per orbit") == pytest.approx(shift, rel=REL)
    assert val(n, "ODE shift per century") == pytest.approx(shift * per_century / arcsec, rel=REL)
    # Newtonian: zero shift; the program's value is its numerical noise floor
    assert abs(val(n, "Newtonian shift per orbit")) < 1e-11


def test_all_gravitation_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("0"))  # pass 1 only
    assert names == ["01_orbit_elements", "02_hohmann_transfer", "03_mercury_precession"]
