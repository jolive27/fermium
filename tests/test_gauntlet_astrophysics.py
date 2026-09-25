"""Gauntlet, astrophysics: run each problem and check it against independent answers."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "astrophysics")
G, C, H, KB = K.G, K.c, K.h, K.k
MSUN = 1.98841e30          # IAU 2015 nominal GM☉ / G (CODATA 2022 G)
RSUN = 6.957e8             # IAU 2015 nominal
LSUN = 3.828e26            # IAU 2015 nominal
AU = 149597870700.0
PC = AU * 648000 / math.pi
YR = 365.25 * 86400
R_E = K.physical_constants["classical electron radius"][0]


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def test_lane_emden_n1():
    out = run_problem("01_lane_emden_n1.fm")
    # SciPy reference for the same initial-value problem, started at the same ξ₀
    x0 = 1e-4
    sol = solve_ivp(lambda x, y: [y[1], -2 * y[1] / x - y[0]], (x0, 4),
                    [1 - x0 ** 2 / 6 + x0 ** 4 / 120, -x0 / 3 + x0 ** 3 / 30],
                    rtol=1e-12, atol=1e-14, dense_output=True)
    for line, x in zip(out[:3], [1, 2, 3]):
        exact = math.sin(x) / x
        assert sol.sol(x)[0] == pytest.approx(exact, rel=1e-9)
        # Fermium's RK45 at rtol 1e-9; θ(3) ≈ 0.047 is small, so allow 1e-8 relative
        assert field(line, "theta =") == pytest.approx(exact, rel=5e-7)   # the adaptive solver keeps a per-step tolerance of 1e-9; globally ~1e-7
        assert field(line, "sin(xi)/xi =") == pytest.approx(exact, rel=1e-9)
    xi1 = brentq(lambda x: sol.sol(x)[0], 3, 3.3, xtol=1e-14)
    assert xi1 == pytest.approx(math.pi, rel=1e-10)
    assert field(out[3], "xi_1 =") == pytest.approx(math.pi, rel=1e-8)
    assert field(out[3], "theta'(xi_1) =") == pytest.approx(math.pi, rel=1e-8)
    rho_mean = MSUN / (4 / 3 * math.pi * RSUN ** 3)
    rho_c = math.pi ** 2 / 3 * rho_mean
    Kp = 2 * G * RSUN ** 2 / math.pi
    assert field(out[4], "mean density =") == pytest.approx(rho_mean / 1000, rel=1e-7)
    assert field(out[4], "central density =") == pytest.approx(rho_c / 1000, rel=1e-7)
    assert field(out[5], "K =") == pytest.approx(Kp, rel=1e-7)
    assert field(out[5], "P_c =") == pytest.approx(Kp * rho_c ** 2, rel=1e-7)


def test_compact_objects():
    out = run_problem("02_compact_objects.fm")
    rs = lambda m: 2 * G * m / C ** 2  # noqa: E731
    sgr = 4.30e6 * MSUN
    # closed forms, printed to 8 digits: 1e-7 relative
    assert field(out[0], "R_S(Sun) =") == pytest.approx(rs(MSUN) / 1e3, rel=1e-7)
    assert field(out[0], "R_S(10 Msun) =") == pytest.approx(rs(10 * MSUN) / 1e3, rel=1e-7)
    assert field(out[0], "R_S(Sgr A*) =") == pytest.approx(rs(sgr) / AU, rel=1e-7)
    theta = 2 * math.sqrt(27) * G * sgr / C ** 2 / (8.28e3 * PC)
    assert field(out[1], "=") == pytest.approx(theta * 180 / math.pi * 3600e6, rel=1e-7)
    sigma_t = 8 * math.pi / 3 * R_E ** 2
    led = lambda m: 4 * math.pi * G * m * K.m_p * C / sigma_t  # noqa: E731
    assert sigma_t == pytest.approx(K.physical_constants["Thomson cross section"][0], rel=1e-9)
    assert field(out[2], "L_Ed(1 Msun) =") == pytest.approx(led(MSUN) / LSUN, rel=1e-7)
    assert field(out[2], "L_Ed(Sgr A*) =") == pytest.approx(led(sgr), rel=1e-7)
    assert field(out[3], "Mdot =") == pytest.approx(led(sgr) / (0.1 * C ** 2) / MSUN * YR, rel=1e-7)


def test_wien_peak():
    out = run_problem("03_wien_peak.fm")
    # the peaks of x³/(eˣ−1) and x⁵/(eˣ−1) in x = hν/kT: 3(1−e⁻ˣ) = x and 5(1−e⁻ˣ) = x
    xn = brentq(lambda x: 3 * (1 - math.exp(-x)) - x, 1, 5, xtol=1e-15)
    xl = brentq(lambda x: 5 * (1 - math.exp(-x)) - x, 1, 10, xtol=1e-15)
    T = 5772
    nu, lam = xn * KB * T / H, H * C / (xl * KB * T)
    # the root finder refines to double precision; derivatives are symbolic -> 10 printed digits
    tol = 1e-9
    assert field(out[0], "nu_max =") == pytest.approx(nu / 1e12, rel=tol)
    assert field(out[0], "lambda_max =") == pytest.approx(lam * 1e9, rel=tol)
    assert field(out[1], "=") == pytest.approx(lam * nu / C, rel=tol)
    assert field(out[2], "h nu_max / kT =") == pytest.approx(xn, rel=tol)
    assert field(out[2], "hc / (lambda_max kT) =") == pytest.approx(xl, rel=tol)
    T = 2.7255
    assert field(out[3], "nu_max =") == pytest.approx(xn * KB * T / H / 1e9, rel=tol)
    assert field(out[3], "lambda_max =") == pytest.approx(H * C / (xl * KB * T) * 1e3, rel=tol)


def test_mass_function():
    out = run_problem("04_mass_function.fm")
    P, K1, M1, inc = 5.599829 * 86400, 75.6e3, 40.6 * MSUN, math.radians(27.51)
    f = P * K1 ** 3 / (2 * math.pi * G)
    m2 = brentq(lambda m: m ** 3 * math.sin(inc) ** 3 / (M1 + m) ** 2 - f, 0.1 * MSUN, 1000 * MSUN, xtol=1e10)
    a = (G * (M1 + m2) * P ** 2 / (4 * math.pi ** 2)) ** (1 / 3)
    # closed forms and a root refined to double precision, printed to 8 digits
    assert field(out[0], "f(M) =") == pytest.approx(f / MSUN, rel=1e-7)
    assert field(out[1], "a1 sin i =") == pytest.approx(P * K1 / (2 * math.pi) / RSUN, rel=1e-7)
    assert field(out[2], "M2 =") == pytest.approx(m2 / MSUN, rel=1e-7)
    assert field(out[3], "separation =") == pytest.approx(a / RSUN, rel=1e-7)
