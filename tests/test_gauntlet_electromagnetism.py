"""Gauntlet, electromagnetism: run each problem and check its numbers against
closed-form answers computed here in Python, with SciPy (quad, solve_ivp) as an
independent check of the numerical parts.

Tolerances: numbers printed to 8 significant figures are off by up to 5e-8 relative
from rounding, and Fermium's integrals use a relative tolerance of 1e-10, so closed-form
comparisons use rel = 1e-7. Values read from an ODE solution (adaptive RK45, relative
tolerance 1e-9 per step, errors accumulating over ~20 oscillation periods' worth of
steps plus interpolation between steps) use rel = 1e-6.
"""
import math
import os
import re

from scipy.integrate import quad, solve_ivp

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(HERE), "gauntlet", "electromagnetism")

EPS0 = 8.8541878188e-12      # CODATA 2022
MU0 = 1.25663706127e-6       # CODATA 2022
P8 = 1e-7                    # 8 printed significant figures
ODE = 1e-6

NUM = re.compile(r"(?:(?<=\s)|(?<==)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?")


def run_problem(name):
    with open(os.path.join(TOPIC, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=TOPIC)


def values(out, label):
    """All numbers printed after `label` on the first line that contains it."""
    for line in out.split("\n"):
        if label in line:
            return [num(t) for t in NUM.findall(line.split(label, 1)[1])]
    raise AssertionError(f"no line with {label!r} in:\n{out}")


def value(out, label):
    return values(out, label)[0]


def close(a, b, rel):
    assert math.isclose(a, b, rel_tol=rel), f"{a} != {b} (rel {rel})"


def test_finite_line_charge():
    out = run_problem("01_finite_line_charge.fm")
    lam, L = 5e-9, 2.0
    k = 1 / (4 * math.pi * EPS0)
    y0 = 0.5
    E_bis = lam * L / (4 * math.pi * EPS0 * y0 * math.sqrt(y0**2 + L**2 / 4))
    close(value(out, "(integral) ="), E_bis, P8)
    close(value(out, "(formula)  ="), E_bis, P8)
    # off-axis point: closed-form components of a finite segment's field.
    # E_x = kλ (1/r_R - 1/r_L), E_y = kλ/y (a_R/r_R - a_L/r_L), with a = distances
    # along the rod measured from the point's foot to the ends.
    x, y = 0.3, 0.5
    rR = math.hypot(x - L / 2, y)
    rL = math.hypot(x + L / 2, y)
    Ex = k * lam * (1 / rR - 1 / rL)
    Ey = k * lam / y * ((x + L / 2) / rL - (x - L / 2) / rR)
    # SciPy agrees with the closed form
    close(quad(lambda s: k * lam * (x - s) / ((x - s) ** 2 + y**2) ** 1.5, -1, 1)[0], Ex, 1e-10)
    close(value(out, "E_x at P (integral) ="), Ex, P8)
    close(value(out, "E_y at P (integral) ="), Ey, P8)
    close(value(out, "E_x at P (-∇V) ="), Ex, P8)
    close(value(out, "E_y at P (-∇V) ="), Ey, P8)
    assert abs(value(out, "E_z at P (-∇V) =")) < 1e-9
    V = k * lam * math.log((x + L / 2 + rL) / (x - L / 2 + rR))
    close(value(out, "V at P ="), V, P8)


def test_loop_biot_savart():
    out = run_problem("02_loop_biot_savart.fm")
    R, I, z = 0.05, 2.0, 0.03
    Bz = MU0 * I * R**2 / (2 * (R**2 + z**2) ** 1.5)
    close(value(out, "B_z (Biot–Savart) ="), Bz * 1e6, P8)
    close(value(out, "B_z (formula)     ="), Bz * 1e6, P8)
    assert value(out, "|B_x| + |B_y| =") < 1e-9 * Bz * 1e6
    close(value(out, "B at the centre ="), MU0 * I / (2 * R) * 1e6, P8)
    close(value(out, "μ₀ I / 2R ="), MU0 * I / (2 * R) * 1e6, P8)
    close(value(out, "∫ B_z dz / μ₀ I ="), 1.0, P8)


def test_rlc_circuit():
    out = run_problem("03_rlc_circuit.fm")
    C, V0, L, R = 20e-6, 10.0, 50e-3, 20.0
    Q0 = C * V0
    w0 = 1 / math.sqrt(L * C)
    a = R / (2 * L)
    wd = math.sqrt(w0**2 - a**2)
    close(value(out, "ω0  ="), w0, P8)
    close(value(out, "α   ="), a, P8)
    close(value(out, "ω_d ="), wd, P8)

    def q(t):
        return Q0 * math.exp(-a * t) * (math.cos(wd * t) + a / wd * math.sin(wd * t))

    def i(t):   # dq/dt of the above
        return -Q0 * math.exp(-a * t) * (w0**2 / wd) * math.sin(wd * t)

    # SciPy's solve_ivp agrees with the closed form
    sol = solve_ivp(lambda t, y: [y[1], -(R * y[1] + y[0] / C) / L], (0, 0.002), [Q0, 0.0],
                    rtol=1e-12, atol=1e-18)
    close(sol.y[0, -1], q(0.002), 1e-8)
    t1 = 2e-3
    close(value(out, "q(2 ms) solve ="), q(t1) * 1e6, ODE)
    close(value(out, "q(2 ms) exact ="), q(t1) * 1e6, P8)
    close(value(out, "i(2 ms) solve ="), i(t1) * 1e3, ODE)
    close(value(out, "i(2 ms) exact ="), i(t1) * 1e3, P8)
    t2 = 10e-3
    heat = quad(lambda t: i(t) ** 2 * R, 0, t2, limit=200, epsabs=0, epsrel=1e-12)[0]
    drop = 0.5 * Q0**2 / C - (0.5 * q(t2) ** 2 / C + 0.5 * L * i(t2) ** 2)
    close(heat, drop, 1e-10)
    close(value(out, "(solve) ="), heat * 1e3, ODE)
    close(value(out, "(exact) ="), heat * 1e3, P8)
    close(value(out, "drop in stored energy"), heat * 1e3, ODE)
