"""Gauntlet, second pass, special relativity: run each problem and check its numbers
against exact hyperbolic-motion formulas, SciPy (quad over the piecewise trip,
solve_ivp for the charged particle) and NumPy matrix algebra (the Lorentz group).

Tolerances: 9 or 10 printed significant figures carry up to 5e-9 / 5e-10 relative
rounding error, so closed forms and integrals (relative tolerance 1e-10) use
rel = 1e-8 (9 digits) or 1e-9 (10 digits). The charged-particle ODE runs with
`tolerance 1e-11` over ~1 time constant: rel 1e-8. The twin-paradox ODE has an
acceleration that switches with `if` on t, which RK45 steps across without
locating the switch (gauntlet friction #11): rel 2e-6. A root found on |r'(t)|
(the interpolant's derivative, friction S9) is only checked to 1e-4.
"""
import math
import os
import re

import numpy as np
from scipy.integrate import quad, solve_ivp

from conftest import run
from numparse import num

HERE = os.path.dirname(os.path.abspath(__file__))
TOPIC = os.path.join(os.path.dirname(HERE), "gauntlet", "special_relativity")

C = 299792458.0
E_CHG = 1.602176634e-19
ME_C2 = 0.51099895069            # MeV, CODATA 2022
MEV = 1e6 * E_CHG
YR = 365.25 * 86400
LY = C * YR
P9 = 1e-8
P10 = 1e-9

NUM = re.compile(r"(?:(?<=\s)|(?<==)|(?<=<)|^)-?\d+(?:\.\d+)?(?:e-?\d+)?(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)?(?![\d/.])")


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


def test_hyperbolic_motion():
    out = run_problem("21_hyperbolic_motion.fm")
    F = 1.0                       # eE in MeV per metre
    p0 = 1.0                      # MeV/c
    e0 = math.hypot(ME_C2, p0)
    t = 10e-9
    ct = C * t

    def x(ct):
        return (math.sqrt(e0**2 + (F * ct) ** 2) - e0) / F

    def y(ct):
        return p0 / F * math.asinh(F * ct / e0)

    tau = ME_C2 / F * math.asinh(F * ct / e0) / C
    # an independent integration in SciPy, in units of MeV and metres (s = ct)
    sol = solve_ivp(lambda s, u: [F, 0, u[0] / math.sqrt(ME_C2**2 + u[0] ** 2 + u[1] ** 2),
                                  u[1] / math.sqrt(ME_C2**2 + u[0] ** 2 + u[1] ** 2)],
                    (0, ct), [0, p0, 0, 0], rtol=1e-12, atol=1e-14)
    close(sol.y[2, -1], x(ct), 1e-9)
    close(sol.y[3, -1], y(ct), 1e-9)
    close(value(out, "(a) ε0 ="), e0, P10)
    for label, exact in (("x(10 ns) =", x(ct)), ("y(10 ns) =", y(ct)), ("τ(10 ns) =", tau * 1e9)):
        solved, printed = values(out, label)
        close(solved, exact, P9)
        close(printed, exact, P9)
    assert values(out, "p(10 ns) =")[:3] == [3.0, 1.0, 0]
    close(value(out, "catenary x at that y ="), x(ct), P9)
    y1 = y(ct)
    v0 = p0 / e0 * C
    close(value(out, "Newtonian parabola x ="), 0.5 * F * C**2 / ME_C2 * (y1 / v0) ** 2, 1e-5)
    g98 = 1 / math.sqrt(1 - 0.98**2)
    t98 = math.sqrt((g98 * ME_C2) ** 2 - e0**2) / (F * C) * 1e9
    close(values(out, "speed 0.98 c at t =")[0], t98, P9)
    close(values(out, "speed 0.98 c at t =")[1], t98, P9)
    close(value(out, "from |r'(t)| instead:"), t98, 1e-4)
    close(values(out, "energy gained =")[0], x(ct) * F, P9)
    close(values(out, "energy gained =")[1], x(ct) * F, P9)


def test_twin_paradox():
    out = run_problem("22_twin_paradox.fm")
    g, t1, tc = 9.81, 1.5 * YR, 2.0 * YR
    Tl = 2 * t1 + tc
    T = 2 * Tl

    def u(t):
        if t < t1:
            return g * t
        if t < t1 + tc:
            return g * t1
        if t < 3 * t1 + tc:
            return g * (2 * t1 + tc - t)
        if t < 3 * t1 + 2 * tc:
            return -g * t1
        return g * (t - T)

    brk = [t1, t1 + tc, 3 * t1 + tc, 3 * t1 + 2 * tc]
    tau_q = quad(lambda t: 1 / math.sqrt(1 + (u(t) / C) ** 2), 0, T, points=brk,
                 epsabs=0, epsrel=1e-13, limit=200)[0]
    gm = math.sqrt(1 + (g * t1 / C) ** 2)
    tau = 4 * C / g * math.asinh(g * t1 / C) + 2 * tc / gm
    close(tau_q, tau, 1e-12)
    close(value(out, "Earth twin ages"), 10.0, 1e-6)
    close(values(out, "traveller ages")[0], tau / YR, P10)
    close(values(out, "traveller ages")[1], tau / YR, P10)
    vmax = g * t1 / gm
    close(value(out, "top speed ="), vmax / C, P10)
    xf = 2 * C**2 / g * (gm - 1) + vmax * tc
    close(quad(lambda t: u(t) / math.sqrt(1 + (u(t) / C) ** 2), 0, Tl, points=brk[:2],
               epsabs=0, epsrel=1e-13)[0], xf, 1e-12)
    close(values(out, "turning point at")[0], xf / LY, P10)
    close(values(out, "turning point at")[1], xf / LY, P10)
    assert abs(value(out, "back home: x(T) =")) < 1e-9
    ode = values(out, "ODE: traveller ages")
    close(ode[0], tau / YR, 2e-6)
    close(ode[1], xf / LY, 2e-6)
    ta = C / g * math.sinh(10.0 * YR * g / C)
    close(value(out, "each leg lasts"), ta / YR, 1e-7)
    close(value(out, "Earth ages"), 4 * ta / YR, 1e-7)
    close(value(out, "farthest point"), 2 * C**2 / g * (math.sqrt(1 + (g * ta / C) ** 2) - 1) / LY, 1e-7)


def boost(beta, n):
    n = np.asarray(n, float)
    g = 1 / math.sqrt(1 - beta**2)
    L = np.eye(4)
    L[0, 0] = g
    L[0, 1:] = L[1:, 0] = -g * beta * n
    L[1:, 1:] += (g - 1) * np.outer(n, n)
    return L


def test_lorentz_matrices():
    out = run_problem("23_lorentz_matrices.fm")
    eta = np.diag([-1.0, 1, 1, 1])
    L1 = boost(0.6, [1, 0, 0])
    assert np.allclose(L1.T @ eta @ L1, eta, atol=1e-15)
    assert value(out, "size of Λᵀ η Λ − η =") < 1e-12
    close(value(out, "det Λ ="), 1.0, P10)
    ev = values(out, "eigenvalues:")
    assert np.allclose(ev, np.sort(np.linalg.eigvalsh(L1)), rtol=P10)
    close(ev[0], math.exp(-math.atanh(0.6)), P10)
    close(ev[3], math.exp(math.atanh(0.6)), P10)
    L12 = boost(0.8, [1, 0, 0]) @ L1
    b = -L12[0, 1] / L12[0, 0]
    close(b, (0.6 + 0.8) / (1 + 0.48), 1e-14)
    close(value(out, "combined β ="), b, P10)
    close(value(out, "rapidity ="), math.atanh(0.6) + math.atanh(0.8), P10)
    Lp = boost(0.8, [0, 1, 0]) @ L1
    g1, g2 = 1.25, 5 / 3
    close(value(out, "combined γ ="), g1 * g2, P10)
    sym = values(out, "entry [1,2] vs [2,1]:")
    close(sym[0], Lp[0, 1], 1e-5)
    close(sym[1], Lp[1, 0], 1e-5)
    # polar decomposition Λ = B R in NumPy: B from the time column
    gt = Lp[0, 0]
    v = -Lp[1:, 0] / gt
    bt = np.linalg.norm(v)
    R = np.linalg.inv(boost(bt, v / bt)) @ Lp
    assert np.allclose(R[0], [1, 0, 0, 0], atol=1e-12)
    theta = math.degrees(abs(math.atan2(R[2, 1], R[1, 1])))
    close(theta, math.degrees(math.acos((g1 + g2) / (1 + g1 * g2))), 1e-12)
    close(values(out, "R[1,1] =")[0], 1.0, P10)
    close(values(out, "Wigner angle =")[0], theta, P10)
    close(value(out, "acos((γ1 + γ2)/(1 + γ1 γ2)) ="), theta, P10)
    # two-body decay
    mpi, mmu = 139.57039, 105.6583755
    ps = (mpi**2 - mmu**2) / (2 * mpi)
    Es = math.hypot(mmu, ps)
    close(value(out, "p* ="), ps, P10)
    close(value(out, "E*_μ ="), Es, P10)
    gpi = 10000.0 / mpi
    bpi = math.sqrt(1 - 1 / gpi**2)
    P = boost(bpi, [-1, 0, 0]) @ np.array([Es, 0, ps, 0])
    close(value(out, "lab energy ="), P[0], P10)
    close(value(out, "γ E* ="), gpi * Es, P10)
    ang = math.degrees(math.atan2(P[2], P[1]))
    close(value(out, "lab angle ="), ang, P10)
    close(value(out, "atan(p*/(γ β E*/c)) ="), ang, P10)
    close(value(out, "invariant mass ="), mmu, P10)
