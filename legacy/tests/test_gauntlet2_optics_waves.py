"""Gauntlet, second pass, optics and waves: run each problem and check its numbers
against scipy.special (Bessel functions and their zeros), closed-form Fresnel
formulas, and the complex characteristic-matrix product redone in NumPy.

Tolerances: 10 printed significant figures carry up to 5e-10 relative rounding error;
the Bessel functions are integrals with relative tolerance 1e-10, and roots of
equations built from them are then accurate to ~1e-9 (the root is found to double
precision, but the function it is found on is only good to ~1e-10 absolute), so
Bessel-based values use rel = 3e-9. Values printed to 8 digits use 1e-7, and
everything else (algebra, matrix products) uses 1e-9.
"""
import math
import os
import re

import numpy as np
from scipy.optimize import brentq
from scipy.special import j0, j1, jn_zeros

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(os.path.dirname(HERE)), "gauntlet", "optics_waves")

P8 = 1e-7
P10 = 1e-9
BES = 3e-9
MAS = 180 / math.pi * 3600e3

NUM = re.compile(r"(?:(?<=\s)|(?<==)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?(?![\d/.])")


def run_problem(name):
    with open(os.path.join(TOPIC, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=TOPIC)


def values(out, label):
    for line in out.split("\n"):
        if label in line:
            return [num(t) for t in NUM.findall(line.split(label, 1)[1])]
    raise AssertionError(f"no line with {label!r} in:\n{out}")


def value(out, label):
    return values(out, label)[0]


def close(a, b, rel):
    assert math.isclose(a, b, rel_tol=rel), f"{a} != {b} (rel {rel})"


def test_airy_disk():
    out = run_problem("21_airy_disk.fm")
    x1 = jn_zeros(1, 1)[0]
    close(value(out, "first dark ring at x ="), x1, BES)
    close(value(out, "sin θ D/λ ="), x1 / math.pi, BES)
    close(x1 / math.pi, 1.2197, 1e-4)                     # the textbook 1.22
    x_half = brentq(lambda x: (2 * j1(x) / x) ** 2 - 0.5, 0.5, 3, xtol=1e-15)
    close(value(out, "half intensity at x ="), x_half, BES)
    # d/dx (J1(x)/x) = -J2(x)/x: the first bright ring is the first zero of J2
    x_ring = jn_zeros(2, 1)[0]
    close(values(out, "first bright ring at x =")[0], x_ring, BES)
    close(values(out, "first bright ring at x =")[1], (2 * j1(x_ring) / x_ring) ** 2, BES)
    enc = 1 - j0(x1) ** 2 - j1(x1) ** 2
    close(values(out, "fraction inside the first dark ring =")[0], enc, BES)
    close(value(out, "1 - J0² - J1² ="), enc, BES)
    D, f, lam = 2.40, 57.6, 550e-9
    th = math.asin(x1 * lam / (math.pi * D))
    close(value(out, "Rayleigh angle ="), th * MAS, P8)
    close(value(out, "FWHM ="), 2 * math.asin(x_half * lam / (math.pi * D)) * MAS, P8)
    close(value(out, "focal plane ="), f * math.tan(th) * 1e6, P8)


def test_fresnel_brewster():
    out = run_problem("22_fresnel_brewster.fm")
    n1, n2 = 1.0, 1.52
    close(values(out, "normal incidence =")[0], ((n2 - n1) / (n2 + n1)) ** 2, P10)
    tb = math.degrees(math.atan(n2 / n1))
    close(values(out, "Brewster angle =")[0], tb, P10)
    close(values(out, "Brewster angle =")[1], tb, P10)
    close(value(out, "reflected and refracted rays ="), 90.0, P10)
    ti = math.radians(60)
    tt = math.asin(n1 * math.sin(ti) / n2)
    ci, ct = math.cos(ti), math.cos(tt)
    rs = (n1 * ci - n2 * ct) / (n1 * ci + n2 * ct)
    rp = (n2 * ci - n1 * ct) / (n2 * ci + n1 * ct)
    ts = 2 * n1 * ci / (n1 * ci + n2 * ct)          # the standard forms of t
    tp = 2 * n1 * ci / (n2 * ci + n1 * ct)
    fac = n2 * ct / (n1 * ci)
    s_line = values(out, "(c) R_s =")
    close(s_line[0], rs**2, P10)
    close(s_line[1], fac * ts**2, P10)
    close(s_line[2], 1.0, P10)
    p_line = values(out, "R_p =")
    close(p_line[0], rp**2, P10)
    close(p_line[1], fac * tp**2, P10)
    close(p_line[2], 1.0, P10)
    assert "T_s = 0.81" in out        # a plain number, not shown in degrees
    close(value(out, "R unpolarised ="), (rs**2 + rp**2) / 2, P10)
    close(value(out, "polarisation ="), (rs**2 - rp**2) / (rs**2 + rp**2), P10)
    close(value(out, "critical angle ="), math.degrees(math.asin(n1 / n2)), P10)
    tg = math.radians(45)
    # the TIR phase from the complex Fresnel coefficient itself
    ctc = np.sqrt(complex(1 - (n2 / n1 * math.sin(tg)) ** 2))
    rs_c = (n2 * math.cos(tg) - n1 * ctc) / (n2 * math.cos(tg) + n1 * ctc)
    close(abs(rs_c), 1.0, 1e-12)
    close(value(out, "TIR phase shift for s at 45° ="), math.degrees(-np.angle(rs_c)), P10)


def layer(n, d, lam):
    de = 2 * np.pi * n * d / lam
    return np.array([[np.cos(de), 1j * np.sin(de) / n], [1j * n * np.sin(de), np.cos(de)]])


def refl(M, n0=1.0, ns=1.52):
    B, C = M @ np.array([1, ns])
    return abs((n0 * B - C) / (n0 * B + C)) ** 2


def mirror(lam, nH=2.35, nL=1.46, lm=1064e-9, pairs=8):
    M = np.eye(2, dtype=complex)
    for _ in range(pairs):
        M = M @ layer(nH, lm / (4 * nH), lam) @ layer(nL, lm / (4 * nL), lam)
    return M


def test_thin_film_stack():
    out = run_problem("23_thin_film_stack.fm")
    n0, ns, nf, l0 = 1.0, 1.52, 1.38, 550e-9
    d = l0 / (4 * nf)
    close(value(out, "MgF2 thickness ="), d * 1e9, P8)
    Ra = ((n0 * ns - nf**2) / (n0 * ns + nf**2)) ** 2
    close(refl(layer(nf, d, l0)), Ra, 1e-12)
    close(values(out, "R(550 nm) =")[0], Ra, P10)
    close(values(out, "R(550 nm) =")[1], Ra, P10)
    close(value(out, "bare glass R ="), ((ns - n0) / (ns + n0)) ** 2, P10)
    close(value(out, "λ = 450 nm  R ="), refl(layer(nf, d, 450e-9)), P10)
    close(value(out, "λ = 700 nm  R ="), refl(layer(nf, d, 700e-9)), P10)
    Y = (2.35 / 1.46) ** 16 * ns
    close(refl(mirror(1064e-9)), ((n0 - Y) / (n0 + Y)) ** 2, 1e-12)
    close(values(out, "R(1064 nm) =")[0], ((n0 - Y) / (n0 + Y)) ** 2, P10)
    close(value(out, "R(900 nm) ="), refl(mirror(900e-9)), P10)
    delta = 2 / math.pi * math.asin((2.35 - 1.46) / (2.35 + 1.46))
    edges = values(out, "band edges:")
    close(edges[0], 1064 / (1 + delta), P8)
    close(edges[1], 1064 / (1 - delta), P8)
    g = values(out, "λ0/λ at the edges =")
    close(g[0], 1 - delta, P8)
    close(g[1], 1 + delta, P8)
    close(g[2], 1 - delta, P8)
    close(g[3], 1 + delta, P8)
    rr = values(out, "just outside:")
    close(rr[1], refl(mirror(1.02 * 1064e-9 / (1 - delta))), 1e-5)
