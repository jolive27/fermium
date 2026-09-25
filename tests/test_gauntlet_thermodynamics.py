"""Gauntlet, thermodynamics: run each problem and check its numbers against
closed-form answers computed here in Python (and SciPy's quad as a second opinion).

Tolerances: Fermium's integrals use a relative tolerance of 1e-10 and its ODE solver
1e-9, so the numerical error is far below the printing precision. The tolerance is set by
the rounding of the printed value: a number printed to 6 significant figures is off by up
to 5e-6 relative (for a leading digit 1), so P6 = 1e-5; to 8 figures, 1e-7.
"""
import math
import os
import re

from scipy.integrate import quad

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(HERE), "gauntlet", "thermodynamics")

# exact SI values (2019 redefinition)
K_B = 1.380649e-23
N_A = 6.02214076e23
R = K_B * N_A
ATM = 101325.0
P6 = 1e-5   # 6 printed significant figures


def run_problem(name):
    with open(os.path.join(TOPIC, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=TOPIC)


NUM = re.compile(r"(?:(?<=\s)|(?<==)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?(?![\d/.])")


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


def test_otto_cycle():
    out = run_problem("01_otto_cycle.fm")
    g, n, p1, T1, r, T3 = 1.4, 0.02, ATM, 300.15, 8.0, 1600.0
    V1 = n * R * T1 / p1
    V2 = V1 / r
    T2 = T1 * r ** (g - 1)
    p3 = n * R * T3 / V2
    # closed-form work on an adiabat: (p_a V_a - p_b V_b)/(γ - 1)
    W_on = (p1 * r ** g * V2 - p1 * V1) / (g - 1)   # p2 = p1 r^γ
    W_by = (p3 * V2 - p3 * (V2 / V1) ** g * V1) / (g - 1)
    Q_in = n * R / (g - 1) * (T3 - T2)
    close(value(out, "V1 ="), V1 * 1e3, P6)
    close(value(out, "T2 ="), T2, P6)
    close(value(out, "compression ="), W_on, P6)
    close(value(out, "expansion   ="), W_by, P6)
    close(value(out, "Q_in ="), Q_in, P6)
    eta = 1 - r ** (1 - g)
    close(value(out, "efficiency from the integrals ="), eta, 1e-7)
    close(value(out, "efficiency 1 - r^(1-γ)"), eta, 1e-7)


def test_newton_cooling():
    out = run_problem("02_newton_cooling.fm")
    Tr, T0, T5 = 20.0, 90.0, 70.0
    k = math.log((T0 - Tr) / (T5 - Tr)) / 5.0          # per minute
    T = lambda t: Tr + (T0 - Tr) * math.exp(-k * t)   # noqa: E731
    close(value(out, "k ="), k, P6)
    close(value(out, "T(5 min)"), T(5), P6)
    close(value(out, "T(10 min)"), T(10), P6)
    close(value(out, "rate at t = 0:"), -k * (T0 - Tr), P6)
    t_drink = math.log((T0 - Tr) / (50.0 - Tr)) / k
    close(value(out, "drink after"), t_drink, P6)
    close(value(out, "/k ="), t_drink, P6)


def test_maxwell_boltzmann():
    out = run_problem("03_maxwell_boltzmann.fm")
    m = 28.0e-3 / N_A
    T = 298.15
    a = m / (2 * K_B * T)
    f = lambda v: 4 * math.pi * (a / math.pi) ** 1.5 * v * v * math.exp(-a * v * v)  # noqa: E731
    close(value(out, "∫ f dv ="), 1.0, 1e-9)
    v_mean = math.sqrt(8 * K_B * T / (math.pi * m))
    v_rms = math.sqrt(3 * K_B * T / m)
    # SciPy as an independent check of the closed forms
    close(quad(lambda v: v * f(v), 0, math.inf)[0], v_mean, 1e-9)
    mean_line = values(out, "<v>   =")
    close(mean_line[0], v_mean, 1e-7)
    close(mean_line[1], v_mean, 1e-7)
    rms_line = values(out, "v_rms =")
    close(rms_line[0], v_rms, 1e-7)
    close(rms_line[1], v_rms, 1e-7)
    close(value(out, "v_p   ="), math.sqrt(2 * K_B * T / m), 1e-7)
    # f'(v_p) = 0 up to rounding
    assert abs(value(out, "f(v_p) =")) < 1e-12
    assert "f'(0.9 v_p) > 0: true" in out and "f'(1.1 v_p) < 0: true" in out
    # fraction above v0: erfc(x) + 2x e^{-x²}/√π with x = v0 √a
    x = 1000.0 * math.sqrt(a)
    tail = math.erfc(x) + 2 * x * math.exp(-x * x) / math.sqrt(math.pi)
    close(quad(f, 1000.0, math.inf)[0], tail, 1e-8)
    close(value(out, "faster than 1000 m/s ="), tail, 1e-7)
