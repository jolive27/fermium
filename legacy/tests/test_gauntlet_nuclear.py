"""Gauntlet, nuclear physics: run each problem and check it against independent answers."""
import csv
import math
import os
import re

import numpy as np
import pytest
from scipy import constants as K
from scipy.integrate import solve_ivp
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "gauntlet", "nuclear")
UC2 = K.physical_constants["atomic mass constant energy equivalent in MeV"][0]   # u c² in MeV
YR = 365.25 * 86400


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def bateman(n, t, lams, N0):
    """Closed-form Bateman solution for member n (0-based) of a chain starting as pure member 0."""
    pre = N0 * np.prod(lams[:n])
    total = 0.0
    for j in range(n + 1):
        den = np.prod([lams[k] - lams[j] for k in range(n + 1) if k != j])
        total += math.exp(-lams[j] * t) / den
    return pre * total


def test_radon_progeny():
    out = run_problem("01_radon_progeny.fm")
    lams = np.log(2) / np.array([3.098 * 60, 26.8 * 60, 19.9 * 60, 164.3e-6])
    N0 = 1000 / lams[0]
    names = ["A(Po-218) =", "A(Pb-214) =", "A(Bi-214) =", "A(Po-214) ="]
    # a stiff solver as a second, independent reference
    J = np.diag(-lams) + np.diag(lams[:-1], -1)
    sol = solve_ivp(lambda t, y: J @ y, (0, 7200), [N0, 0, 0, 0], method="Radau", jac=J,
                    rtol=1e-12, atol=1e-12, dense_output=True)
    for line, T in zip(out[:3], [600, 1800, 3600]):
        for n, key in enumerate(names):
            want = lams[n] * bateman(n, T, lams, N0)
            assert lams[n] * sol.sol(T)[n] == pytest.approx(want, rel=1e-8)
            # Radau (`using radau`) at rtol 1e-9, printed to 8 digits: 1e-7
            assert field(line, key) == pytest.approx(want, rel=1e-7)
    # (b) peak of the Bi-214 activity: dN3/dt = 0
    dN3 = lambda t: lams[1] * bateman(1, t, lams, N0) - lams[2] * bateman(2, t, lams, N0)  # noqa: E731
    t_peak = brentq(dN3, 60, 7200, xtol=1e-9)
    assert field(out[3], "peaks at") == pytest.approx(t_peak / 60, rel=1e-7)
    assert field(out[3], "with") == pytest.approx(lams[2] * bateman(2, t_peak, lams, N0), rel=1e-7)
    # (c) growing-in Bi-214: the Po-214 activity lags by a relative ~λ3/λ4 ~ 1e-7
    r = lams[3] * bateman(3, 3600, lams, N0) / (lams[2] * bateman(2, 3600, lams, N0))
    assert field(out[4], "=") == pytest.approx(r, rel=1e-7)


def test_q_values_semf():
    out = run_problem("02_q_values_semf.fm")
    n, H1, H2, H3, He4 = 1.008664916, 1.007825032, 2.014101778, 3.016049281, 4.002603254
    Th234, U238, Pb207, Pb208 = 234.043599801, 238.050786936, 206.975896887, 207.976652481
    Q = (H2 + H3 - He4 - n) * UC2
    # exact arithmetic on the same masses; printed to 8 significant figures, so the
    # rounding alone can be 5e-8 relative (14.048937 vs 14.0489365)
    tol = 6e-8
    assert field(out[0], "Q(D+T) =") == pytest.approx(Q, rel=tol)
    assert field(out[1], "T_n =") == pytest.approx(Q * He4 / (He4 + n), rel=tol)
    assert field(out[1], "T_alpha =") == pytest.approx(Q * n / (He4 + n), rel=tol)
    Qa = (U238 - Th234 - He4) * UC2
    assert field(out[2], "Q(U-238 alpha) =") == pytest.approx(Qa, rel=tol)
    assert field(out[2], "T_alpha =") == pytest.approx(Qa * 234 / 238, rel=tol)
    assert field(out[3], "=") == pytest.approx((n - H1) * UC2, rel=tol)
    assert field(out[4], "=") == pytest.approx((Pb207 + n - Pb208) * UC2, rel=tol)
    B = (82 * H1 + 126 * n - Pb208) * UC2
    A, Z = 208, 82
    semf = (15.5 * A - 16.8 * A ** (2 / 3) - 0.72 * Z * (Z - 1) / A ** (1 / 3)
            - 23.0 * (A - 2 * Z) ** 2 / A + 34 * A ** -0.75)
    assert field(out[5], "masses") == pytest.approx(B, rel=tol)
    assert field(out[5], "SEMF") == pytest.approx(semf, rel=tol)
    assert field(out[6], "masses") == pytest.approx(B / 208, rel=tol)
    assert field(out[6], "SEMF") == pytest.approx(semf / 208, rel=tol)


def test_radiometric_dating():
    out = run_problem("03_radiometric_dating.fm")
    lam_c = math.log(2) / (5730 * YR)
    age = math.log(15.3 * 5.00 / 32.0) / lam_c
    # closed form printed to 8 digits
    assert field(out[0], "age =") == pytest.approx(age / YR, rel=1e-7)
    assert field(out[1], "atoms =") == pytest.approx(32.0 / 60 / lam_c, rel=1e-7)
    with open(os.path.join(DIR, "data", "rb_sr_isochron.csv"), encoding="utf-8") as f:
        rows = list(csv.reader(f))[1:]
    x = np.array([float(r[0]) for r in rows])
    y = np.array([float(r[1]) for r in rows])
    slope, icpt = np.polyfit(x, y, 1)            # linear least squares: the exact optimum
    line = next(ln for ln in out if ln.startswith("(b)"))
    # the nonlinear fitter should reach the linear least-squares optimum to ~1e-8
    assert field(line, "slope =") == pytest.approx(slope, rel=1e-7)
    assert field(line, "initial ratio =") == pytest.approx(icpt, rel=1e-7)
    assert field(out[-1], "age =") == pytest.approx(math.log(1 + slope) / 1.3972e-11 / 1e9, rel=1e-7)
