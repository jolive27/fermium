"""Gauntlet, optics and waves: run each problem and check its numbers against
closed-form answers computed here in Python, with SciPy's brentq as an independent
root finder.

Tolerances: every checked number is printed to 8 significant figures, so rounding
alone is up to 5e-8 relative; the computations inside Fermium are closed-form,
symbolic derivatives or full-precision root finding, so rel = 1e-7 is the right bound.
"""
import math
import os
import re

from scipy.optimize import brentq

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(HERE), "gauntlet", "optics_waves")
P8 = 1e-7

NUM = re.compile(r"(?:(?<=\s)|(?<==)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?(?![\d/.])")


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


def close(a, b, rel=P8):
    assert math.isclose(a, b, rel_tol=rel), f"{a} != {b} (rel {rel})"


def test_single_slit():
    out = run_problem("01_single_slit.fm")
    lam, a, L = 633e-9, 0.1e-3, 2.0
    y1 = L * math.tan(math.asin(lam / a))
    y2 = L * math.tan(math.asin(2 * lam / a))
    close(value(out, "first dark fringe at y  ="), y1 * 1e3)
    close(value(out, "second dark fringe at y ="), y2 * 1e3)
    close(value(out, "central fringe width    ="), 2 * y1 * 1e3)
    # secondary maximum: tan β = β, between π and 3π/2
    beta = brentq(lambda b: math.tan(b) - b, math.pi + 1e-6, 1.5 * math.pi - 1e-6, xtol=1e-15)
    close(value(out, "secondary maximum at β ="), beta)
    close(value(out, "I/I0 there              ="), (math.sin(beta) / beta) ** 2)
    close(beta, 4.4934094579, 1e-10)          # textbook value
    th = math.atan(0.02 / L)
    b = math.pi * a * math.sin(th) / lam
    close(value(out, "I/I0 at y = 2.00 cm     ="), (math.sin(b) / b) ** 2)


def test_lensmaker():
    out = run_problem("02_lensmaker.fm")
    R1, R2, A, B = 0.20, -0.30, 1.5046, 4200e-18

    def n(lam):
        return A + B / lam**2

    def f(lam):
        return 1 / ((n(lam) - 1) * (1 / R1 - 1 / R2))

    f1 = f(589e-9)
    close(value(out, "n at 589 nm ="), n(589e-9))
    close(value(out, "f at 589 nm ="), f1 * 100)
    s1 = 0.5
    si = 1 / (1 / f1 - 1 / s1)
    m1 = -si / s1
    close(value(out, "image distance ="), si * 100)
    close(value(out, "magnification  ="), m1)
    close(value(out, "image height   ="), m1 * 3.0)
    s2 = 0.10 - si
    s2i = 1 / (1 / -0.15 - 1 / s2)
    close(value(out, "final image distance s2' ="), s2i * 100)
    close(value(out, "overall magnification ="), m1 * (-s2i / s2))
    close(value(out, "f at 486 nm ="), f(486e-9) * 100)
    close(value(out, "f at 656 nm ="), f(656e-9) * 100)
    # df/dλ = f² (1/R1 - 1/R2) 2B/λ³, in mm/nm = 1e3/1e9 of SI
    lam = 589e-9
    dfdl = f(lam) ** 2 * (1 / R1 - 1 / R2) * 2 * B / lam**3
    close(value(out, "df/dλ at 589 nm ="), dfdl * 1e3 / 1e9)


def test_water_wave_dispersion():
    out = run_problem("03_water_wave_dispersion.fm")
    g, sig, rho = 9.81, 0.0728, 1000.0

    def w(k):
        return math.sqrt(g * k + sig / rho * k**3)

    def vp(k):
        return w(k) / k

    def vg(k):
        return (g + 3 * sig / rho * k**2) / (2 * w(k))

    k1 = 2 * math.pi
    close(value(out, "λ = 1 m:  v_p ="), vp(k1))
    close(value(out, "λ = 1 m:  v_g ="), vg(k1))
    close(value(out, "λ = 1 m:  v_g/v_p ="), vg(k1) / vp(k1))
    k2 = 2 * math.pi / 1e-3
    close(value(out, "λ = 1 mm: v_g/v_p ="), vg(k2) / vp(k2))
    kmin = math.sqrt(rho * g / sig)
    # independent: minimise v_p numerically via its derivative (v_g = v_p there)
    close(brentq(lambda k: vg(k) - vp(k), 1, 1000, xtol=1e-14), kmin, 1e-12)
    kl = values(out, "k_min =")
    close(kl[0], kmin)
    close(kl[1], kmin)
    vl = values(out, "v_min =")
    close(vl[0], (4 * g * sig / rho) ** 0.25 * 100)
    close(vl[1], (4 * g * sig / rho) ** 0.25 * 100)
    close(value(out, "v_g/v_p at k_min ="), 1.0)
    H, k3 = 2.0, 2 * math.pi / 10
    vp3 = math.sqrt(g * k3 * math.tanh(k3 * H)) / k3
    vg3 = vp3 / 2 * (1 + 2 * k3 * H / math.sinh(2 * k3 * H))
    close(value(out, "λ = 10 m: v_g ="), vg3)
    close(value(out, "λ = 10 m: formula ="), vg3)
