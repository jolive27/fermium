"""Gauntlet, second pass, electromagnetism: run each problem and check its numbers
against closed forms evaluated here (cycloid, elliptic integrals from scipy.special,
Fourier series summed with NumPy) and SciPy quadrature (quad, dblquad).

Tolerances: numbers printed to 8 (10) significant figures carry up to 5e-8 (5e-10)
relative rounding error, and Fermium's integrals use a relative tolerance of 1e-10,
so closed-form comparisons use rel = 1e-7 (1e-9 for 10 digits). The vector ODE runs
with `tolerance 1e-11` over three cyclotron periods; its values are compared at
rel = 1e-6 (the global error of RK45 is a few hundred times the per-step tolerance).
Nested integrals are compared at 1e-7: each level has relative tolerance 1e-10.
"""
import math
import os
import re

import numpy as np
from scipy.integrate import dblquad, quad
from scipy.special import ellipe, ellipk

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(HERE), "gauntlet", "electromagnetism")

EPS0 = 8.8541878188e-12      # CODATA 2022
E_CHG = 1.602176634e-19
M_P = 1.67262192595e-27      # CODATA 2022
P8 = 1e-7
P10 = 1e-9
ODE = 1e-6

NUM = re.compile(r"(?:(?<=\s)|(?<==)|(?<=<)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?(?![\d/.])")


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


def test_crossed_fields():
    out = run_problem("21_crossed_fields.fm")
    E, B, vz = 1.0e3, 0.1, 2.0e3
    w = E_CHG * B / M_P
    T = 2 * math.pi / w
    vd = E / B
    R = E / (B * w)
    close(value(out, "(a) ω ="), w, P8)
    close(value(out, "T ="), T * 1e9, P8)
    close(value(out, "v_d_x ="), vd, P8)
    assert value(out, "v_d_y =") == 0
    close(value(out, "R ="), R * 1e3, P8)

    def cyc(t):
        return (R * (w * t - math.sin(w * t)), R * (1 - math.cos(w * t)), vz * t)

    # the solved path and the printed exact path, in mm
    close(values(out, "x(T/4) =")[0], cyc(T / 4)[0] * 1e3, ODE)
    close(values(out, "x(T/4) =")[1], cyc(T / 4)[0] * 1e3, P8)
    close(values(out, "y(T/2) =")[0], 2 * R * 1e3, ODE)
    close(values(out, "y(T/2) =")[1], 2 * R * 1e3, P8)
    close(values(out, "x(3T) =")[0], 6 * math.pi * R * 1e3, ODE)
    close(values(out, "x(3T) =")[1], 6 * math.pi * R * 1e3, P8)
    close(values(out, "z(3T) =")[0], 3 * vz * T * 1e3, ODE)
    # the vector lines print 3 significant figures; check them loosely
    r = values(out, "t/T = 0.5  r =")
    assert np.allclose(r[:3], np.array(cyc(T / 2)) * 1e3, rtol=5e-3)
    close(value(out, "<v_x> over a period ="), vd, ODE)
    close(value(out, "at T/2 ="), 2.0, ODE)
    t1 = 0.3 * T
    ke = E_CHG * E * cyc(t1)[1] / E_CHG          # q E y, in eV
    close(values(out, "KE gained by 0.3 T =")[0], ke, ODE)
    close(values(out, "KE gained by 0.3 T =")[1], ke, ODE)


def ring_closed_form(a, Q, rho, z):
    """V, E_rho, E_z of a charged ring (Jackson §3.3), SciPy's K(m), E(m)."""
    k = Q / (4 * math.pi * EPS0)
    S = math.sqrt((a + rho) ** 2 + z**2)
    D = (a - rho) ** 2 + z**2
    m = 4 * a * rho / S**2
    V = k * 2 / math.pi * ellipk(m) / S
    Er = k / (math.pi * rho * S) * (ellipk(m) - (a * a - rho * rho + z * z) / D * ellipe(m))
    Ez = k * 2 / math.pi * z * ellipe(m) / (D * S)
    return V, Er, Ez


def test_ring_disk_off_axis():
    out = run_problem("22_ring_disk_off_axis.fm")
    a, Q = 0.10, 5e-9
    rho, z = 0.06, 0.04
    V, Er, Ez = ring_closed_form(a, Q, rho, z)
    # the elliptic closed form agrees with direct quadrature of Coulomb's law
    ke = 1 / (4 * math.pi * EPS0)
    k = ke * Q
    lam = Q / (2 * math.pi * a)

    def dist3(p):
        return ((rho - a * math.cos(p)) ** 2 + (a * math.sin(p)) ** 2 + z**2) ** 1.5

    close(quad(lambda p: ke * lam * a * (rho - a * math.cos(p)) / dist3(p), 0, 2 * math.pi,
               epsabs=0, epsrel=1e-12)[0], Er, 1e-10)
    close(value(out, "(a) E_x ="), Er, P8)
    assert abs(value(out, "E_y =")) < 1e-9 * abs(Ez)
    close(value(out, "    E_z ="), Ez, P8)
    close(value(out, "(b) V(P) ="), V, P8)
    close(value(out, "-∇V: E_x ="), Er, P8)
    close(value(out, "-∇V: E_z ="), Ez, P8)
    z1 = 0.07
    Eax = k * z1 / (a**2 + z1**2) ** 1.5
    close(value(out, "on axis (integral) ="), Eax, P8)
    close(value(out, "on axis (formula)  ="), Eax, P8)
    close(values(out, "strongest at z =")[0], a / math.sqrt(2) * 100, P8)
    close(values(out, "strongest at z =")[1], a / math.sqrt(2) * 100, P8)
    # the disk at P by SciPy's dblquad over (s, φ)
    sig = Q / (math.pi * a**2)

    def dd(p, s):
        return ((rho - s * math.cos(p)) ** 2 + (s * math.sin(p)) ** 2 + z**2) ** 1.5

    Exd = dblquad(lambda p, s: ke * sig * s * (rho - s * math.cos(p)) / dd(p, s),
                  0, a, 0, 2 * math.pi, epsabs=0, epsrel=1e-11)[0]
    Ezd = dblquad(lambda p, s: ke * sig * s * z / dd(p, s),
                  0, a, 0, 2 * math.pi, epsabs=0, epsrel=1e-11)[0]
    close(value(out, "disk E_x ="), Exd, P8)
    close(value(out, "disk E_z ="), Ezd, P8)
    Ed_axis = sig / (2 * EPS0) * (1 - z1 / math.sqrt(z1**2 + a**2))
    close(value(out, "disk on axis (integral) ="), Ed_axis, P8)
    close(value(out, "disk on axis (formula)  ="), Ed_axis, P8)


def test_laplace_box():
    out = run_problem("23_laplace_box.fm")
    a, V0 = 0.02, 100.0
    n = np.arange(1, 200, 2)

    def phi(x, y):
        return float(np.sum(4 * V0 / (n * np.pi) * np.sin(n * np.pi * x / a)
                            * np.sinh(n * np.pi * y / a) / np.sinh(n * np.pi)))

    # superposition of the four rotated problems: exactly V0/4 at the centre
    close(phi(a / 2, a / 2), V0 / 4, 1e-13)
    close(values(out, "(a) φ(centre) =")[0], V0 / 4, P10)
    close(value(out, "φ(a/4, 3a/4) ="), phi(a / 4, 3 * a / 4), P10)
    # symbolic second derivatives are negatives of each other
    assert "-4V0 n π sin(n π x/a) sinh(n π y/a)" in out
    lap = value(out, "Laplacian of term 3 at (a/7, a/5) =")
    d2x = value(out, "vs ∂²/∂x² alone")
    x, y = a / 7, a / 5
    close(d2x, -4 * V0 * 3 * math.pi * math.sin(3 * math.pi * x / a) * math.sinh(3 * math.pi * y / a)
          / (a**2 * math.sinh(3 * math.pi)), 5e-3)            # printed to 3 digits
    assert abs(lap) < 1e-9 * abs(d2x)
    close(value(out, "average of φ on a circle about the centre ="), V0 / 4, P10)
    assert values(out, "left wall, bottom wall =") == [0, 0]
    # on the lid the truncated series is (4/π) Σ (−1)^k/(2k+1): a square wave's partial sum
    lid = 4 * V0 / np.pi * np.sum((-1.0) ** np.arange(100) / (2 * np.arange(100) + 1))
    close(value(out, "middle of the lid (N terms) ="), lid, P10)
    lam = -EPS0 * np.sum(8 * V0 / (n * np.pi * np.sinh(n * np.pi)))
    close(value(out, "λ on the bottom wall ="), lam * 1e12, P10)
    # the cube: symmetry of the six faces gives V0/6 at the centre
    close(values(out, "(e) cube centre =")[0], V0 / 6, P10)
