"""Gauntlet third pass (graduate), gravitation: run each problem and check it against independent answers."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import quad, solve_ivp

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "gravitation")

C = K.c
G = 6.67430e-11
GM_SUN = 1.3271244e20          # IAU 2015 nominal, the value of Fermium's GM_sun
R_SUN = 6.957e8                # IAU nominal
M_SUN = 1.98840987e30          # GM_sun / G, as Fermium derives M_sun
YR = 365.25 * 86400
ARCSEC = math.pi / (180 * 3600)


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


# ---------------------------------------------------------------- 31 light bending

def deflection_ode(m, r0):
    ev = lambda p, y: y[0]
    ev.terminal = True
    sol = solve_ivp(lambda p, y: [y[1], 3 * m * y[0] ** 2 - y[0]], (0, 6), [1 / r0, 0], method="DOP853",
                    rtol=3e-14, atol=1e-30, events=ev)
    return 2 * sol.t_events[0][0] - math.pi


def deflection_quad(m, r0):
    # u = u0 sin χ removes the end-point singularity:
    # u0² − u² − 2m(u0³ − u³) = u0² cos²χ (1 − 2m u0 (1 + sin χ + sin²χ)/(1 + sin χ))
    u0 = 1 / r0
    f = lambda chi: 1 / math.sqrt(1 - 2 * m * u0 * (1 + math.sin(chi) + math.sin(chi) ** 2) / (1 + math.sin(chi)))
    return 2 * quad(f, 0, math.pi / 2, epsabs=0, epsrel=1e-13)[0] - math.pi


def test_light_bending():
    out = run_problem("31_light_bending.fm")
    m = GM_SUN / C ** 2
    d_ode = deflection_ode(m, R_SUN)
    d_q = deflection_quad(m, R_SUN)
    assert d_ode == pytest.approx(d_q, rel=1e-8)
    a = line_with(out, "(a)")
    assert field(a, "phi_inf =") == pytest.approx((d_q + math.pi) / 2, rel=1e-11)
    assert field(a, "deflection =") == pytest.approx(d_q / ARCSEC, rel=1e-6)
    assert field(out[1], "Einstein 4m/r0 =") == pytest.approx(4 * m / R_SUN / ARCSEC, rel=1e-6)
    second = 4 * m / R_SUN + (15 * math.pi / 4 - 4) * (m / R_SUN) ** 2
    assert field(out[1], "second order =") == pytest.approx(second / ARCSEC, rel=1e-6)
    # the second-order formula agrees with the exact deflection to O((m/r0)³)
    assert second == pytest.approx(d_q, rel=1e-10)
    # the classic number: 1.75″ (Dyson, Eddington & Davidson 1920; 1.7512″ with r0 = R☉)
    assert field(a, "deflection =") == pytest.approx(1.7512, abs=1e-4)
    assert field(line_with(out, "(b)"), "Sun =") == pytest.approx(d_q / ARCSEC, rel=1e-6)

    c = line_with(out, "(c)")
    m_ns = 1.40 * GM_SUN / C ** 2
    d_ns = deflection_quad(m_ns, 4 * m_ns)
    assert deflection_ode(m_ns, 4 * m_ns) == pytest.approx(d_ns, rel=1e-10)
    assert field(c, "deflection =") == pytest.approx(math.degrees(d_ns), rel=1e-7)
    assert field(c, "quadrature =") == pytest.approx(math.degrees(d_ns), rel=1e-7)
    assert field(c, "weak field =") == pytest.approx(math.degrees(1.0), rel=1e-5)


# ---------------------------------------------------------------- 32 Hulse–Taylor

def test_hulse_taylor():
    out = run_problem("32_hulse_taylor_decay.fm")
    Pb = 0.322997448918 * 86400
    e0 = 0.6171340
    mp, mc = 1.438 * M_SUN, 1.390 * M_SUN
    M = mp + mc
    a0 = (G * M * (Pb / (2 * math.pi)) ** 2) ** (1 / 3)
    assert field(out[0], "a0 =") == pytest.approx(a0 / 1e3, rel=1e-7)

    wdot = 3 * (2 * math.pi / Pb) ** (5 / 3) * (G * M) ** (2 / 3) / (C ** 2 * (1 - e0 ** 2))
    a = line_with(out, "(a)")
    assert field(a, "periastron advance =") == pytest.approx(math.degrees(wdot) * YR, rel=1e-6)
    # measured 4.226585 °/yr (Weisberg & Huang 2016); the rounded masses give 1e-4
    assert field(a, "periastron advance =") == pytest.approx(4.226585, rel=2e-4)

    fe = (1 + 73 / 24 * e0 ** 2 + 37 / 96 * e0 ** 4) / (1 - e0 ** 2) ** 3.5
    pdot = -192 * math.pi * G ** (5 / 3) / (5 * C ** 5) * (Pb / (2 * math.pi)) ** (-5 / 3) * fe * mp * mc / M ** (1 / 3)
    b = line_with(out, "(b)")
    assert field(b, "f(e) =") == pytest.approx(fe, rel=1e-7)
    assert field(b, "dP_b/dt =") == pytest.approx(pdot, rel=1e-6)
    # Weisberg & Huang (2016), Table 3: the GR prediction is −2.40263(5)×10⁻¹²
    assert field(b, "dP_b/dt =") == pytest.approx(-2.40263e-12, rel=5e-4)

    # (c) Peters' equations by SciPy (LSODA) to a = 1e5 km
    b0 = G ** 3 * mp * mc * M / C ** 5

    def rhs(t, y):
        aa, e = y
        return [-64 / 5 * b0 * (1 + 73 / 24 * e * e + 37 / 96 * e ** 4) / (aa ** 3 * (1 - e * e) ** 3.5),
                -304 / 15 * b0 * e * (1 + 121 / 304 * e * e) / (aa ** 4 * (1 - e * e) ** 2.5)]

    ev = lambda t, y: y[0] - 1e8
    ev.terminal = True
    sol = solve_ivp(rhs, (0, 1e9 * YR), [a0, e0], method="LSODA", rtol=1e-12, atol=[1e-3, 1e-15], events=ev)
    t1 = sol.t_events[0][0]
    e1 = sol.y_events[0][0][1]
    c = line_with(out, "(c)")
    assert field(c, "time to a = 1e5 km =") == pytest.approx(t1 / YR, rel=1e-8)
    assert field(c, "e there =") == pytest.approx(e1, rel=1e-6)

    beta = 64 / 5 * b0
    c0 = a0 * (1 - e0 ** 2) / e0 ** (12 / 19) / (1 + 121 / 304 * e0 ** 2) ** (870 / 2299)
    T_from = lambda e: 12 / 19 * c0 ** 4 / beta * quad(
        lambda x: x ** (29 / 19) * (1 + 121 / 304 * x * x) ** (1181 / 2299) / (1 - x * x) ** 1.5, e, e0,
        epsabs=0, epsrel=1e-13)[0]
    assert field(out[4], "time from e0 to e1 =") == pytest.approx(T_from(e1) / YR, rel=1e-8)
    assert field(out[5], "merger time =") == pytest.approx(T_from(0) / YR, rel=1e-7)
    # ~301 Myr (Weisberg & Huang 2016 quote ~300 Myr)
    assert 2.9e8 < field(out[5], "merger time =") < 3.1e8
    assert field(out[5], "a0⁴/(4β) =") == pytest.approx(a0 ** 4 / (4 * beta) / YR, rel=1e-5)
