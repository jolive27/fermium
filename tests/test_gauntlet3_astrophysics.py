"""Gauntlet third pass (graduate), astrophysics: run each problem and check it against independent answers."""
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.integrate import quad, solve_ivp
from scipy.optimize import minimize_scalar

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "astrophysics")

C, G, HBAR = K.c, 6.67430e-11, K.hbar
M_SUN = 1.3271244e20 / G          # Fermium's M_sun = GM☉ (IAU nominal) / G
KEV = 1e3 * K.e
YR = 365.25 * 86400


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


# ---------------------------------------------------------------- 31 Oppenheimer–Volkoff

MN = K.m_n
KN = MN ** 4 * C ** 5 / (8 * math.pi ** 2 * HBAR ** 3)
eps = lambda x: KN * (x * (2 * x * x + 1) * math.sqrt(1 + x * x) - math.asinh(x))
P = lambda x: KN / 3 * (x * (2 * x * x - 3) * math.sqrt(1 + x * x) + 3 * math.asinh(x))
dPdx = lambda x: 8 * KN / 3 * x ** 4 / math.sqrt(1 + x * x)


def tov(xc):
    """(M, R) of the TOV star with central x_c, by SciPy's DOP853 with an event at x = 1e-3."""
    def rhs(r, y):
        x, m = y
        dP = -G * (eps(x) + P(x)) * (m + 4 * math.pi * r ** 3 * P(x) / C ** 2) / (C ** 2 * r * r * (1 - 2 * G * m / (r * C ** 2)))
        return [dP / dPdx(x), 4 * math.pi * r * r * eps(x) / C ** 2]

    ev = lambda r, y: y[0] - 1e-3
    ev.terminal = True
    sol = solve_ivp(rhs, (1.0, 1e6), [xc, 4 * math.pi * eps(xc) / (3 * C ** 2)], method="DOP853", rtol=1e-12,
                    atol=[1e-14, 1e-6], events=ev)
    return sol.y_events[0][0][1], sol.t_events[0][0]


def test_oppenheimer_volkoff():
    out = run_problem("31_oppenheimer_volkoff.fm")
    M1, R1 = tov(1.0)
    a = lines_with(out, "(a)")[0]
    assert field(a, "M =") == pytest.approx(M1 / M_SUN, rel=1e-7)
    assert field(a, "R =") == pytest.approx(R1 / 1e3, rel=1e-7)

    best = minimize_scalar(lambda x: -tov(x)[0], bounds=(0.3, 3), method="bounded", options={"xatol": 1e-6})
    Mmax, Rmax = tov(best.x)
    b = lines_with(out, "(b)")[0]
    # the maximum is flat: x_c to 1e-3, M_max to 1e-8
    assert field(b, "x_c at the maximum =") == pytest.approx(best.x, rel=1e-3)
    assert field(b, "M_max =") == pytest.approx(Mmax / M_SUN, rel=1e-7)
    assert field(b, "R =") == pytest.approx(Rmax / 1e3, rel=1e-4)
    # Oppenheimer & Volkoff (1939): 0.71 M☉; Shapiro & Teukolsky §9.1: 0.7 M☉, R ≈ 9.6 km
    assert Mmax / M_SUN == pytest.approx(0.71, abs=0.005)
    assert 9.0 < Rmax / 1e3 < 9.7
    assert field(out[2], "central mass density =") == pytest.approx(eps(best.x) / C ** 2 * 1e-3, rel=2e-3)
    assert field(out[3], "2GM/(Rc²) =") == pytest.approx(2 * G * Mmax / (Rmax * C ** 2), rel=1e-4)

    # the Newtonian limit: the n = 3/2 polytrope (Lane–Emden constants from Chandrasekhar's table)
    xs = 0.05
    rho = MN * (xs * MN * C / HBAR) ** 3 / (3 * math.pi ** 2)
    K1 = (3 * math.pi ** 2) ** (2 / 3) * HBAR ** 2 / (5 * MN ** (8 / 3))
    a_p = math.sqrt(2.5 * K1 * rho ** (-1 / 3) / (4 * math.pi * G))
    Ms, Rs = tov(xs)
    assert field(out[4], "M =") == pytest.approx(Ms / M_SUN, rel=1e-5)
    assert field(out[4], "polytrope =") == pytest.approx(2.71406 * 4 * math.pi * a_p ** 3 * rho / M_SUN, rel=1e-5)
    assert field(out[5], "R =") == pytest.approx(Rs / 1e3, rel=1e-5)
    assert field(out[5], "polytrope =") == pytest.approx(3.65375 * a_p / 1e3, rel=1e-5)
    # GR and the relativistic EOS correct the polytrope at O(x_c², GM/Rc²): < 1 %
    assert Ms == pytest.approx(2.71406 * 4 * math.pi * a_p ** 3 * rho, rel=0.01)
    assert Rs == pytest.approx(3.65375 * a_p, rel=0.01)


# ---------------------------------------------------------------- 32 Gamow peak

def test_gamow_peak():
    out = run_problem("32_gamow_peak.fm")
    mp, u, al, kB = K.m_p, K.atomic_mass, K.alpha, K.k
    cases = [("pp", mp / 2, 1, 15.7e6), ("C12+alpha", 12 * u * 4.002602 * u / (16.002602 * u), 12, 0.2e9)]
    rows_a, rows_b = lines_with(out, "(a)"), lines_with(out, "(b)")
    taus = []
    for (name, mu, ZZ, T), la, lb in zip(cases, rows_a, rows_b):
        assert name in la and name in lb
        b = math.pi * al * ZZ * math.sqrt(2 * mu * C ** 2)          # J^(1/2)
        kT = kB * T
        E0 = (b * kT / 2) ** (2 / 3)
        D = 4 * math.sqrt(E0 * kT / 3)
        tau = 3 * E0 / kT
        taus.append(tau)
        assert field(la, "b =") == pytest.approx(b / math.sqrt(KEV), rel=1e-7)
        assert field(la, "E0 =") == pytest.approx(E0 / KEV, rel=1e-7)
        assert field(la, "Delta =") == pytest.approx(D / KEV, rel=1e-7)
        assert field(la, "tau =") == pytest.approx(tau, rel=1e-7)
        # the integral in keV, by QUADPACK over a range that holds the peak
        bk, kTk = b / math.sqrt(KEV), kT / KEV
        f = lambda E: math.exp(-E / kTk - bk / math.sqrt(E)) if E > 0 else 0.0
        E0k = E0 / KEV
        I = quad(f, 0, E0k, epsabs=0, epsrel=1e-12, limit=200)[0] + quad(f, E0k, 60 * E0k, epsabs=0, epsrel=1e-12,
                                                                         limit=200)[0]
        gauss = math.sqrt(math.pi) / 2 * D / KEV * math.exp(-tau)
        assert field(lb, "integral =") == pytest.approx(I, rel=1e-9)
        assert field(lb, "Gaussian =") == pytest.approx(gauss, rel=1e-7)
        assert field(lb, "ratio =") == pytest.approx(I / gauss, rel=1e-7)
        # the 5/(12τ) correction brings the Gaussian to O(1/τ²)
        assert abs(I / gauss - (1 + 5 / (12 * tau))) < 1 / tau ** 2

    # (c) ν = −3/2 + T I'(T)/I, with I'(T) by a central difference of QUADPACK integrals
    def nu(mu, ZZ, T):
        b = math.pi * al * ZZ * math.sqrt(2 * mu * C ** 2) / math.sqrt(KEV)

        def I(TT):
            kTk = kB * TT / KEV
            E0 = (b * kTk / 2) ** (2 / 3)
            f = lambda E: math.exp(-E / kTk - b / math.sqrt(E)) if E > 0 else 0.0
            return quad(f, 0, E0, epsabs=0, epsrel=1e-13, limit=200)[0] + quad(f, E0, 60 * E0, epsabs=0, epsrel=1e-13,
                                                                              limit=200)[0]
        h = 1e-4 * T
        return -1.5 + T * (I(T + h) - I(T - h)) / (2 * h) / I(T)

    c = lines_with(out, "(c)")[0]
    assert field(c, "pp: nu =") == pytest.approx(nu(mp / 2, 1, 15.7e6), rel=1e-6)
    assert field(c, "(tau - 2)/3 =") == pytest.approx((taus[0] - 2) / 3, rel=1e-7)
    c2 = out[out.index(c) + 1]
    assert field(c2, "C12+alpha: nu =") == pytest.approx(nu(*cases[1][1:]), rel=1e-6)
    assert field(c2, "(tau - 2)/3 =") == pytest.approx((taus[1] - 2) / 3, rel=1e-7)

    # (d) the pp rate
    mu = mp / 2
    b = math.pi * al * math.sqrt(2 * mu * C ** 2)
    kT = kB * 15.7e6
    E0 = (b * kT / 2) ** (2 / 3)
    f = lambda E: math.exp(-E / kT - b / math.sqrt(E)) if E > 0 else 0.0
    I = quad(f, 0, E0, epsabs=0, epsrel=1e-12)[0] + quad(f, E0, 60 * E0, epsabs=0, epsrel=1e-12, limit=200)[0]
    S = 4.01e-25 * 1e6 * K.e * 1e-28
    sv = math.sqrt(8 / (math.pi * mu)) * kT ** -1.5 * S * I
    npr = 0.34 * 150e3 / mp
    d = lines_with(out, "(d)")[0]
    assert field(d, "<sigma v> =") == pytest.approx(sv * 1e6, rel=1e-5)
    assert field(d, "n_p =") == pytest.approx(npr * 1e-6, rel=1e-3)
    assert field(d, "proton mean life =") == pytest.approx(1 / (npr * sv) / YR, rel=1e-3)
    # of order 10¹⁰ yr (Clayton §5-1): the slowness of pp sets the Sun's lifetime
    assert 3e9 < 1 / (npr * sv) / YR < 3e10
