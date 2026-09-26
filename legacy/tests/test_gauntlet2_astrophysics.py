"""Gauntlet second pass, astrophysics: run each problem and check it against independent answers."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import quad, solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "astrophysics")
G, C, H, HBAR, KB, ME = K.G, K.c, K.h, K.hbar, K.k, K.m_e
MU = K.physical_constants["atomic mass constant"][0]
MSUN = 1.3271244e20 / 6.67430e-11   # IAU 2015 nominal GM☉ / G (CODATA 2022 G), exactly as Fermium defines M_sun
MPC = 3.0856775814913673e22
YR = 365.25 * 86400
GYR = 1e9 * YR


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def line_with(out, text):
    return [ln for ln in out if text in ln][0]


# ---------------------------------------------------------------- 21 white dwarfs

def lane_emden(n):
    x0 = 1e-4
    sol = solve_ivp(lambda x, y: [y[1], -2 * y[1] / x - max(y[0], 0) ** n], (x0, 8),
                    [1 - x0 ** 2 / 6 + n * x0 ** 4 / 120, -x0 / 3 + n * x0 ** 3 / 30],
                    method="DOP853", rtol=1e-12, atol=1e-14, dense_output=True)
    xi1 = brentq(lambda x: sol.sol(x)[0], 2, 8, xtol=1e-14)
    return xi1, -xi1 ** 2 * sol.sol(xi1)[1]


def test_white_dwarfs():
    out = run_problem("21_white_dwarfs.fm")
    mu_e = 2
    xi15, w15 = lane_emden(1.5)
    xi3, w3 = lane_emden(3)
    # Chandrasekhar's (1939) table, to the digits it gives
    assert (xi15, w15, xi3, w3) == pytest.approx((3.65375, 2.71406, 6.89685, 2.01824), abs=1e-5)
    # Fermium's RK45 at rtol 1e-9 against DOP853 at 1e-12, 9 digits printed; for n = 1.5
    # θ' at the surface comes from the interpolated solution next to the kink of
    # max(θ, 0)^1.5, and ω is off by 3e-8: 1e-7
    assert field(out[0], "xi_1 =") == pytest.approx(xi15, rel=2e-8)
    assert field(out[0], "omega =") == pytest.approx(w15, rel=1e-7)
    assert field(out[1], "xi_1 =") == pytest.approx(xi3, rel=2e-8)
    assert field(out[1], "omega =") == pytest.approx(w3, rel=2e-8)

    # (b) n = 1.5 polytrope in closed form: M ∝ ρc^(1/2), so ρc follows directly
    K1 = HBAR ** 2 / (5 * ME) * (3 * math.pi ** 2) ** (2 / 3) / (mu_e * MU) ** (5 / 3)
    a = lambda rc: math.sqrt(2.5 * K1 * rc ** (-1 / 3) / (4 * math.pi * G))
    mass = lambda rc: 4 * math.pi * a(rc) ** 3 * rc * w15
    rc06 = (0.6 * MSUN / mass(1.0)) ** 2
    assert mass(rc06) == pytest.approx(0.6 * MSUN, rel=1e-12)
    b = line_with(out, "(b)")
    # the root is refined to double precision; ω and ξ₁ carry the ODE's 1e-8
    assert field(b, "rho_c =") == pytest.approx(rc06, rel=3e-7)   # a root through an ODE: ~1e-7 global error
    assert field(b, "R =") == pytest.approx(xi15 * a(rc06) / 1e3, rel=1e-7)
    assert field(out[3], "R(1.0)/R(0.6) =") == pytest.approx(0.6 ** (1 / 3), rel=1e-7)
    assert field(out[3], "(0.6/1.0)^(1/3) =") == pytest.approx(0.6 ** (1 / 3), rel=1e-8)

    # (c)
    K2 = HBAR * C / 4 * (3 * math.pi ** 2) ** (1 / 3) / (mu_e * MU) ** (4 / 3)
    MCh = 4 * math.pi * w3 * (K2 / (math.pi * G)) ** 1.5
    assert MCh / MSUN == pytest.approx(1.4563, rel=1e-4)
    assert field(line_with(out, "(c)"), "M_Ch =") == pytest.approx(MCh / MSUN, rel=1e-7)

    # (d) the same structure equations with SciPy, surface by brentq on y = 1
    n_scale = 8 * math.pi / 3 * (ME * C / H) ** 3
    rho = lambda y: mu_e * MU * n_scale * (y * y - 1) ** 1.5 if y > 1 else 0.0
    rows = [ln for ln in out if ln.startswith("x_c =")]
    xcs = [0.3, 1, 3, 10, 30, 100]
    assert len(rows) == len(xcs)
    Ms = []
    for xc, line in zip(xcs, rows):
        yc = math.sqrt(1 + xc * xc)
        r_in = 1.0

        def rhs(r, s):
            y, M = s
            return [-(mu_e * MU / (ME * C * C)) * G * M / r ** 2, 4 * math.pi * r ** 2 * rho(y)]

        sol = solve_ivp(rhs, (r_in, 3e7), [yc, 4 / 3 * math.pi * r_in ** 3 * rho(yc)],
                        method="DOP853", rtol=1e-12, atol=1e-30, dense_output=True)
        R = brentq(lambda r: sol.sol(r)[0] - 1, r_in, 3e7, xtol=1e-6)
        M = sol.sol(R)[1]
        Ms.append(M)
        assert field(line, "x_c =") == pytest.approx(xc)
        assert field(line, "rho_c =") == pytest.approx(rho(yc), rel=1e-5)
        # RK45 at 1e-9 through the surface kink of ρ(y): 1e-7
        assert field(line, "M =") == pytest.approx(M / MSUN, rel=1e-7)
        assert field(line, "R =") == pytest.approx(R / 1e3, rel=1e-7)
    # the physics: M rises towards M_Ch, and R falls
    assert Ms == sorted(Ms) and Ms[-1] < MCh
    assert field(out[-1], "M_Ch =") == pytest.approx(Ms[-1] / MCh, rel=1e-5)


# ---------------------------------------------------------------- 22 Saha

CHI = 13.6 * K.e


def test_saha():
    out = run_problem("22_saha_ionization.fm")
    Pe = 20.0

    def ratio(T):
        return (2 * KB * T / (2 * Pe)) * (2 * math.pi * ME * KB * T / H ** 2) ** 1.5 * math.exp(-CHI / (KB * T))

    frac = lambda T: ratio(T) / (1 + ratio(T))
    T50 = brentq(lambda T: ratio(T) - 1, 3000, 30000, xtol=1e-12)
    assert 9500 < T50 < 9700                      # C&O: "about 9600 K"
    # root to double precision, 8 digits printed
    assert field(out[0], "T =") == pytest.approx(T50, rel=2e-8)
    assert field(out[1], "x(8300 K) =") == pytest.approx(frac(8300), rel=1e-8)
    assert field(out[1], "x(11300 K) =") == pytest.approx(frac(11300), rel=1e-8)
    # dx/dT analytically: x = r/(1+r), dr/dT = r (5/2 + χ/kT)/T
    r = ratio(T50)
    dxdT = r * (2.5 + CHI / (KB * T50)) / T50 / (1 + r) ** 2
    assert field(out[2], "dx/dT at T_half =") == pytest.approx(dxdT, rel=1e-7)
    assert field(out[2], "width 1/(dx/dT) =") == pytest.approx(1 / dxdT, rel=1e-7)

    T0, nH0 = 2.7255, 0.190

    def ion(z):
        T = T0 * (1 + z)
        q = (2 * math.pi * ME * KB * T / H ** 2) ** 1.5 * math.exp(-CHI / (KB * T)) / (nH0 * (1 + z) ** 3)
        # the stable root of x² + q x − q = 0
        return 2 * q / (q + math.sqrt(q * q + 4 * q))

    z50 = brentq(lambda z: ion(z) - 0.5, 500, 3000, xtol=1e-12)
    assert 1300 < z50 < 1450 and 3600 < T0 * (1 + z50) < 3900     # Ryden: z ≈ 1380, T ≈ 3740 K
    # 8 digits printed (1369.2329: 4e-8 resolution)
    assert field(out[3], "at z =") == pytest.approx(z50, rel=5e-8)
    assert field(out[3], "T =") == pytest.approx(T0 * (1 + z50), rel=5e-8)
    # x ≈ √q at small q, where (−q + √(q² + 4q))/2 is well conditioned: 1e-8
    assert field(out[4], "x(z = 1100) =") == pytest.approx(ion(1100), rel=1e-8)
    assert field(out[4], "x(z = 1500) =") == pytest.approx(ion(1500), rel=1e-8)


# ---------------------------------------------------------------- 23 Friedmann

def test_friedmann_age():
    out = run_problem("23_friedmann_age.fm")
    H0 = 67.66e3 / MPC
    Om, Or = 0.3111, 9.14e-5
    OL0 = 1 - Om
    closed = 2 / (3 * H0 * math.sqrt(OL0)) * math.asinh(math.sqrt(OL0 / Om))
    num_mL = quad(lambda a: 1 / (a * H0 * math.sqrt(Om / a ** 3 + OL0)), 0, 1, epsrel=1e-13)[0]
    assert num_mL == pytest.approx(closed, rel=1e-12)
    # adaptive quadrature at 1e-10, 10 digits printed
    assert field(out[0], "integral =") == pytest.approx(closed / GYR, rel=2e-9)
    assert field(out[0], "closed form =") == pytest.approx(closed / GYR, rel=1e-9)

    OL = 1 - Om - Or
    Hf = lambda a: H0 * math.sqrt(Or / a ** 4 + Om / a ** 3 + OL)
    t = lambda a0, a1: quad(lambda a: 1 / (a * Hf(a)), a0, a1, epsrel=1e-13, epsabs=0, limit=200)[0]
    t0 = t(0, 1)
    assert field(out[1], "age =") == pytest.approx(t0 / GYR, rel=2e-9)
    assert field(out[1], "z = 1100 =") == pytest.approx(t(0, 1 / 1101) / YR, rel=2e-8)
    assert field(out[1], "lookback to z = 1 =") == pytest.approx(t(0.5, 1) / GYR, rel=2e-9)
    DC = C * quad(lambda z: 1 / Hf(1 / (1 + z)), 0, 1, epsrel=1e-13)[0]
    assert field(out[2], "D_C(z = 1) =") == pytest.approx(DC / MPC, rel=2e-9)
    assert field(out[2], "D_L =") == pytest.approx(2 * DC / MPC, rel=2e-9)

    a_acc = brentq(lambda a: -0.5 * (Om / a ** 3 + 2 * Or / a ** 4) + OL, 0.1, 1, xtol=1e-15)
    assert field(out[3], "a_acc =") == pytest.approx(a_acc, rel=1e-9)
    assert field(out[3], "z_acc =") == pytest.approx(1 / a_acc - 1, rel=1e-9)
    # the ODE (RK45 at the default rtol 1e-9, over 20 Gyr) against the quadrature:
    # the global error is ~3e-7 (local tolerance, not global), so 1e-6
    assert field(out[4], "t =") == pytest.approx(t(0, a_acc) / GYR, rel=1e-6)
    assert field(out[4], "ODE age (A = 1) =") == pytest.approx(t0 / GYR, rel=1e-6)
