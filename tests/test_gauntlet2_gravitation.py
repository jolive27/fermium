"""Gauntlet, second pass, gravitation: run gauntlet/gravitation/2*.fm and check the numbers independently.

References: closed forms (Lagrange points' linear stability, Routh's limit, the hyperbolic flyby,
the exactly solvable 1/r³ precession), SciPy `brentq` (L1, aphelia), `quad` (the apsidal-angle
integral, after the substitution r = r_min + (r_max − r_min) sin²χ that removes both 1/√
end-point blow-ups) and `solve_ivp` (DOP853, rtol 1e-12) with event location.

Tolerances: 6-7 significant figures are printed, so rel 2e-5 covers rounding plus solver error.
The L4 libration amplitude is a maximum over a 0.25-day grid, compared at 1e-4. Where the test
compares with a perturbative formula, the tolerance is the next order in the small parameter.
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIR = os.path.join(ROOT, "gauntlet", "gravitation")
REL = 2e-5
DAY = 86400.0

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


# ---------------------------------------------------------------- 21 restricted three-body

N21 = "21_restricted_three_body"
GM1, GM2, D = 3.986004418e14, 4.9028e12, 384400e3
MU = GM2 / (GM1 + GM2)
OM = math.sqrt((GM1 + GM2) / D**3)


def _cr3bp(gm1, gm2, mu):
    x1, x2 = -mu * D, (1 - mu) * D

    def f(t, s):
        x, y, z, vx, vy, vz = s
        d1 = ((x - x1)**2 + y**2 + z**2)**1.5
        d2 = ((x - x2)**2 + y**2 + z**2)**1.5
        ax = 2 * OM * vy + OM**2 * x - gm1 * (x - x1) / d1 - gm2 * (x - x2) / d2
        ay = -2 * OM * vx + OM**2 * y - gm1 * y / d1 - gm2 * y / d2
        az = -gm1 * z / d1 - gm2 * z / d2
        return [vx, vy, vz, ax, ay, az]
    return f


def _L4(mu):
    return np.array([(0.5 - mu) * D, math.sqrt(3) / 2 * D, 0.0])


def test_lagrange_points_and_stability():
    assert val(N21, "mass ratio μ") == pytest.approx(MU, rel=REL)
    assert val(N21, "sidereal month 2π/Ω") == pytest.approx(2 * math.pi / OM / DAY, rel=REL)

    def fx(x):
        return -GM1 / (x + MU * D)**2 + GM2 / ((1 - MU) * D - x)**2 + OM**2 * x
    xL1 = brentq(fx, 0, (1 - MU) * D - 1e6, xtol=1e-6)
    assert val(N21, "L1 from the barycentre") == pytest.approx(xL1 / 1e3, rel=REL)
    assert val(N21, "L1 from the Moon") == pytest.approx(((1 - MU) * D - xL1) / 1e3, rel=REL)
    # Hill-sphere estimate D (μ/3)^(1/3) is good to ~ (μ/3)^(1/3) ≈ 16 %
    assert ((1 - MU) * D - xL1) == pytest.approx(D * (MU / 3)**(1 / 3), rel=0.2)
    assert vals(N21, "L4") == pytest.approx(_L4(MU)[:2] / 1e3, rel=REL)
    mu_r = (1 - math.sqrt(23 / 27)) / 2
    assert val(N21, "Routh limit") == pytest.approx(mu_r, rel=REL)
    assert mu_r == pytest.approx(0.03852, abs=1e-5)
    # the characteristic equation λ⁴ + Ω² λ² + (27/4) μ(1 − μ) Ω⁴ = 0, solved numerically
    lam2 = np.roots([1, OM**2, 27 / 4 * MU * (1 - MU) * OM**4])
    w = sorted(np.sqrt(-lam2.real))
    assert val(N21, "short libration period") == pytest.approx(2 * math.pi / w[1] / DAY, rel=REL)
    assert val(N21, "long libration period") == pytest.approx(2 * math.pi / w[0] / DAY, rel=REL)


def test_l4_libration_and_jacobi():
    L4 = _L4(MU)
    start = L4 + 1e6 * L4 / np.linalg.norm(L4)
    ts = np.arange(0, 360 + 1e-9, 0.25) * DAY
    s = solve_ivp(_cr3bp(GM1, GM2, MU), [0, 360 * DAY], np.concatenate([start, np.zeros(3)]),
                  method="DOP853", rtol=1e-12, atol=1e-6, t_eval=ts, dense_output=True)

    def jac(st):
        x, y, z, vx, vy, vz = st
        r1 = math.sqrt((x + MU * D)**2 + y**2 + z**2)
        r2 = math.sqrt((x - (1 - MU) * D)**2 + y**2 + z**2)
        return OM**2 * (x * x + y * y) + 2 * GM1 / r1 + 2 * GM2 / r2 - (vx * vx + vy * vy + vz * vz)
    C0 = jac(np.concatenate([start, np.zeros(3)]))
    assert val(N21, "Jacobi constant at the start") == pytest.approx(C0, rel=1e-11)
    assert val(N21, "Jacobi constant after 360 days") == pytest.approx(C0, rel=1e-10)
    far = np.max(np.linalg.norm(s.y[:3].T - L4, axis=1))
    assert val(N21, "largest distance from L4") == pytest.approx(far / 1e3, rel=1e-4)
    assert far < 0.1 * D                      # bounded libration: stable, as Routh says
    d100 = s.sol(100 * DAY)[:3] - L4
    # the y component is small (-40 km) and suffers cancellation: compare it absolutely (1 m)
    got = vals(N21, "position after 100 days (from L4)")
    assert got[0] == pytest.approx(d100[0] / 1e3, rel=REL)
    assert got[1] == pytest.approx(d100[1] / 1e3, abs=1e-3)


def test_beyond_routh_runs_away():
    mu = 0.05
    L4 = _L4(mu)
    start = L4 + 1e6 * L4 / np.linalg.norm(L4)
    s = solve_ivp(_cr3bp((1 - mu) * (GM1 + GM2), mu * (GM1 + GM2), mu), [0, 120 * DAY],
                  np.concatenate([start, np.zeros(3)]), method="DOP853", rtol=1e-12, atol=1e-6)
    d = np.linalg.norm(s.y[:3, -1] - L4)
    assert val(N21, "μ = 0.05, distance from L4 after 120 days") == pytest.approx(d / 1e3, rel=1e-4)
    assert d > 1e8                              # grew more than a hundredfold from 1000 km


# ---------------------------------------------------------------- 22 slingshot

N22 = "22_jupiter_slingshot"


def test_flyby_closed_form():
    GM, RJ, vinf, V = 1.26686534e17, 71492e3, 7000.0, 13070.0
    rp = 5 * RJ
    e = 1 + rp * vinf**2 / GM
    delta = 2 * math.asin(1 / e)
    b = rp * math.sqrt(1 + 2 * GM / (rp * vinf**2))
    assert val(N22, "eccentricity") == pytest.approx(e, rel=REL)
    assert val(N22, "turning angle") == pytest.approx(math.degrees(delta), rel=REL)
    assert val(N22, "impact parameter") == pytest.approx(b / 1e3, rel=REL)
    # the gain from vector addition, independently of the |u_out|² formula
    uin = np.array([-V, vinf])
    uout = np.array([-V - vinf * math.sin(delta), vinf * math.cos(delta)])
    assert val(N22, "heliocentric speed before") == pytest.approx(np.linalg.norm(uin) / 1e3, rel=REL)
    assert val(N22, "heliocentric speed after") == pytest.approx(np.linalg.norm(uout) / 1e3, rel=REL)
    assert val(N22, "speed gain") == pytest.approx((np.linalg.norm(uout) - np.linalg.norm(uin)) / 1e3, rel=REL)
    assert vals(N22, "|Δu| = 2 v∞ sin(δ/2)")[-1] == pytest.approx(np.linalg.norm(uout - uin) / 1e3, rel=REL)


def test_flyby_integrated():
    GM, RJ, vinf, V = 1.26686534e17, 71492e3, 7000.0, 13070.0
    rp = 5 * RJ
    e = 1 + rp * vinf**2 / GM
    a = GM / vinf**2
    p = a * (e * e - 1)
    d0 = 1000 * rp
    finf = math.acos(-1 / e)
    f0 = -math.acos((p / d0 - 1) / e)
    pos = p / (1 + e * math.cos(f0)) * np.array([math.cos(f0), math.sin(f0)])
    vel = math.sqrt(GM / p) * np.array([-math.sin(f0), e + math.cos(f0)])
    psi = math.pi / 2 - math.atan2(e - 1 / e, math.sin(finf))
    R = np.array([[math.cos(psi), -math.sin(psi)], [math.sin(psi), math.cos(psi)]])
    assert (R @ np.array([math.sin(finf), e - 1 / e]))[0] == pytest.approx(0, abs=1e-12)   # asymptote along +y
    H0 = math.acosh((1 + d0 / a) / e)
    tend = 2 * math.sqrt(a**3 / GM) * (e * math.sinh(H0) - H0)
    assert val(N22, "time from 1000 r_p to 1000 r_p") == pytest.approx(tend / DAY, rel=REL)
    VJ = np.array([-V, 0.0])

    def f(t, s):
        d = s[:2] - VJ * t
        return np.concatenate([s[2:], -GM * d / np.linalg.norm(d)**3])

    def peri(t, s):
        return (s[:2] - VJ * t) @ (s[2:] - VJ)
    s = solve_ivp(f, [0, tend], np.concatenate([R @ pos, VJ + R @ vel]), method="DOP853",
                  rtol=1e-12, atol=1e-6, events=peri)
    gain = np.linalg.norm(s.y[2:, -1]) - np.linalg.norm(VJ + R @ vel)
    assert val(N22, "heliocentric speed gain, ODE") == pytest.approx(gain / 1e3, rel=REL)
    vout = math.sqrt(GM / p) * np.array([math.sin(f0), e + math.cos(f0)])
    exact = np.linalg.norm(VJ + R @ vout) - np.linalg.norm(VJ + R @ vel)
    assert gain == pytest.approx(exact, rel=1e-7)
    assert val(N22, "speed gain between the mirror points, exact") == pytest.approx(exact / 1e3, rel=REL)
    rin, rout = R @ vel, s.y[2:, -1] - VJ
    turn = math.degrees(math.acos(rin @ rout / (np.linalg.norm(rin) * np.linalg.norm(rout))))
    assert val(N22, "turning of the relative velocity, ODE") == pytest.approx(turn, rel=REL)
    assert val(N22, "turning at 1000 r_p, exact") == pytest.approx(
        math.degrees(2 * math.atan2(e + math.cos(f0), math.sin(f0)) - math.pi), rel=REL)
    tp = s.t_events[0][0]
    assert vals(N22, "periapsis time, ODE vs half the flight") == pytest.approx([tp / DAY, tend / 2 / DAY], rel=REL)
    assert val(N22, "closest approach, ODE") == pytest.approx(5.0, rel=REL)


# ---------------------------------------------------------------- 23 precession

N23 = "23_perturbed_precession"
AU = 1.495978707e11
GMS = 1.3271244e20


def _setup():
    a, e = 0.387 * AU, 0.206
    h = math.sqrt(GMS * a * (1 - e * e))
    p = h * h / GMS
    rp = a * (1 - e)
    return a, e, h, p, rp, h / rp


def _apsidal(U, E, rp, h):
    rmax = brentq(lambda r: U(r) - E, 1.5 * rp, 10 * rp, xtol=1e-3)

    def f(chi):   # r = rp + (rmax − rp) sin²χ
        r = rp + (rmax - rp) * math.sin(chi)**2
        drdchi = 2 * (rmax - rp) * math.sin(chi) * math.cos(chi)
        return h / r**2 / math.sqrt(2 * (E - U(r))) * drdchi
    return rmax, quad(f, 1e-12, math.pi / 2 - 1e-12, epsabs=0, epsrel=1e-13, limit=200)[0]


def test_precession_case_a():
    a, e, h, p, rp, vp = _setup()
    assert val(N23, "orbital period") == pytest.approx(2 * math.pi * math.sqrt(a**3 / GMS) / DAY, rel=REL)
    lam = 0.002 * h * h
    beta = math.sqrt(1 - lam / h**2)
    exact = 2 * math.pi * (1 / beta - 1)
    assert val(N23, "case A, exact advance per orbit") == pytest.approx(exact, rel=2e-6)
    assert val(N23, "case A, first order") == pytest.approx(math.pi * lam / h**2, rel=2e-6)
    assert exact == pytest.approx(math.pi * lam / h**2, rel=2 * lam / h**2)    # next order ~ (λ/h²)

    E = 0.5 * vp**2 - GMS / rp - lam / (2 * rp**2)
    rmax, dth = _apsidal(lambda r: h * h / (2 * r * r) - GMS / r - lam / (2 * r * r), E, rp, h)
    # independently, the Binet solution u = (GM/(β²h²))(1 + e' cos βφ) with u(0) = 1/r_p gives r_max
    u0 = GMS / (beta**2 * h * h)
    assert rmax == pytest.approx(1 / (2 * u0 - 1 / rp), rel=1e-9)
    assert val(N23, "case A, aphelion") == pytest.approx(rmax / AU, rel=2e-6)
    assert 2 * dth - 2 * math.pi == pytest.approx(exact, rel=1e-8)
    assert val(N23, "case A, advance from the integral") == pytest.approx(exact, rel=2e-6)
    assert val(N23, "case A, advance from the ODE") == pytest.approx(exact, rel=2e-6)


def test_precession_case_b():
    a, e, h, p, rp, vp = _setup()
    kap = 0.001 * h * h * p
    E = 0.5 * vp**2 - GMS / rp - kap / (3 * rp**3)
    _, dth = _apsidal(lambda r: h * h / (2 * r * r) - GMS / r - kap / (3 * r**3), E, rp, h)
    adv = 2 * dth - 2 * math.pi
    assert val(N23, "case B, advance from the integral") == pytest.approx(adv, rel=2e-6)

    def f(t, s):
        r = math.hypot(s[0], s[1])
        acc = -GMS / r**3 - kap / r**5
        return [s[2], s[3], acc * s[0], acc * s[1]]

    def peri(t, s):
        return s[0] * s[2] + s[1] * s[3]
    P = 2 * math.pi * math.sqrt(a**3 / GMS)
    sol = solve_ivp(f, [0, 1.25 * P], [rp, 0, 0, vp], method="DOP853", rtol=1e-12, atol=1e-3,
                    events=peri, dense_output=True)
    tb = [t for t in sol.t_events[0] if t > 0.75 * P][0]
    x, y = sol.sol(tb)[:2]
    assert val(N23, "case B, advance from the ODE") == pytest.approx(math.atan2(y, x), rel=2e-6)
    assert math.atan2(y, x) == pytest.approx(adv, rel=1e-7)
    first = 2 * math.pi * kap / (h * h * p)
    assert val(N23, "case B, first order") == pytest.approx(first, rel=2e-6)
    assert adv == pytest.approx(first, rel=5e-3)      # second order in κ/(h²p) = 1e-3, times e-dependent factors


def test_all_second_pass_gravitation_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("2"))
    assert names == [N21, N22, N23]
