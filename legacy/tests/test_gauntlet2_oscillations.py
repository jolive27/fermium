"""Gauntlet, second pass, oscillations: run gauntlet/oscillations/2*.fm and check the numbers independently.

References: closed forms (the fixed-fixed chain, the symmetric-mode quadratic, Landau's first- and
second-order parametric-resonance band edges, Lindstedt–Poincaré), `scipy.linalg.eigh` for the
generalized eigenproblem, `solve_ivp` (DOP853, rtol 1e-12) for Floquet traces and the driven
Duffing steady state, `brentq` for the band edges and `quad` for the exact Duffing period.

Tolerances: 6-7 significant figures are printed, so rel 2e-5 covers rounding plus solver error
(RK45 at 1e-12 inside the Floquet trace; regula falsi to double precision). Perturbation-theory
comparisons use tolerances set by the next order in the small parameter, stated at each assert.
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.linalg import eigh
from scipy.optimize import brentq

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DIR = os.path.join(ROOT, "gauntlet", "oscillations")
REL = 2e-5

_cache = {}
_SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
_NUM = re.compile(r"(?<![\w.])-?\d+(?:\.\d+)?(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+|e-?\d+)?(?![\d./])")


def output(name):
    if name not in _cache:
        with open(os.path.join(DIR, name + ".fm"), encoding="utf-8") as f:
            _cache[name] = run(f.read(), base_dir=DIR)
    return _cache[name]


def _nums(text):
    return [float(re.sub(r"×10(\S+)", lambda m: "e" + m.group(1).translate(_SUP), t)) for t in _NUM.findall(text)]


def vals(name, label):
    """All numbers printed after `label:` on the line that starts with it."""
    for line in output(name).split("\n"):
        if line.strip().startswith(label + ":"):
            return _nums(line.strip()[len(label) + 1:])
    raise AssertionError(f"no line {label!r} in output of {name}:\n{output(name)}")


def val(name, label, i=0):
    return vals(name, label)[i]


# ---------------------------------------------------------------- 21 three-mass chain

N21 = "21_three_mass_chain"
k, m, m2 = 40.0, 0.25, 0.5
K = np.array([[2 * k, -k, 0], [-k, 2 * k, -k], [0, -k, 2 * k]])


def test_chain_equal_masses():
    w2, V = eigh(K, m * np.eye(3))
    for n in (1, 2, 3):
        exact = 2 * math.sqrt(k / m) * math.sin(n * math.pi / 8)
        assert math.sqrt(w2[n - 1]) == pytest.approx(exact, rel=1e-12)
        assert vals(N21, f"equal masses, mode {n}") == pytest.approx([exact, exact], rel=REL)
    # mode shapes: unit columns ∝ sin(j n π/4), sign chosen so the largest entry is positive
    line = [ln for ln in output(N21).split("\n") if ln.startswith("mode shapes (columns):")][0]
    got = np.array(_nums(line.split(":", 1)[1])).reshape(3, 3)
    for n in range(3):
        v = np.sin(np.arange(1, 4) * (n + 1) * math.pi / 4)
        v /= np.linalg.norm(v)
        if v[np.argmax(np.abs(v))] < 0:
            v = -v
        assert got[:, n] == pytest.approx(v, abs=1e-3)      # printed to 3 figures


def test_chain_heavy_middle():
    M = np.diag([m, m2, m])
    w2, V = eigh(K, M)
    w = np.sqrt(w2)
    assert val(N21, "heavy middle, slow mode") == pytest.approx(w[0], rel=REL)
    assert val(N21, "heavy middle, middle mode") == pytest.approx(w[1], rel=REL)
    assert w[1] == pytest.approx(math.sqrt(2 * k / m), rel=1e-12)          # antisymmetric mode
    assert val(N21, "heavy middle, middle mode", 2) == pytest.approx(math.sqrt(2 * k / m), rel=REL)
    assert val(N21, "heavy middle, fast mode") == pytest.approx(w[2], rel=REL)
    assert vals(N21, "quadratic for the symmetric modes") == pytest.approx([w[0], w[2]], rel=REL)
    ortho = vals(N21, "M-orthogonality v1·Mv2, v1·Mv3, v2·Mv3")
    assert max(abs(x) for x in ortho[-3:]) < 1e-12


def test_chain_release_and_energy():
    M = np.diag([m, m2, m])
    w2, V = eigh(K, M)          # eigh normalizes Vᵀ M V = I; Fermium uses unit vectors: q_n v_n is the same
    x0 = np.array([0.02, 0, 0])
    q = V.T @ M @ x0            # modal coordinates with M-normalized vectors
    # Fermium's q_n are for unit-length v_n: q_n(unit) = q_n(M-norm) × |v_n(M-norm)|, up to sign
    qu = np.abs(q * np.linalg.norm(V, axis=0))
    assert np.abs(vals(N21, "modal amplitudes")) == pytest.approx(qu, rel=REL)

    def f(t, y):
        return np.concatenate([y[3:], -np.linalg.solve(M, K @ y[:3])])
    s = solve_ivp(f, [0, 5], np.concatenate([x0, np.zeros(3)]), method="DOP853", rtol=1e-12, atol=1e-15)
    modal = sum(q[n] * V[0, n] * math.cos(math.sqrt(w2[n]) * 5) for n in range(3))
    assert modal == pytest.approx(s.y[0, -1], rel=1e-8)
    assert vals(N21, "x1(5 s), ODE vs modal sum") == pytest.approx([100 * s.y[0, -1]] * 2, rel=REL)
    assert val(N21, "x3(5 s), ODE") == pytest.approx(100 * s.y[2, -1], rel=REL)
    E = 0.5 * w2 * q**2 * 1e3                       # mJ
    assert vals(N21, "mode energies") == pytest.approx(E, rel=REL)
    assert vals(N21, "sum vs ½ x₀ᵀ K x₀") == pytest.approx([0.5 * x0 @ K @ x0 * 1e3] * 2, rel=REL)
    assert 0.5 * x0 @ K @ x0 == pytest.approx(k * 0.02**2)


# ---------------------------------------------------------------- 22 parametric resonance

N22 = "22_parametric_resonance"
W0, H = 10.0, 0.1


def _trace(g):
    T = 2 * math.pi / g

    def f(t, y):
        return [y[1], -W0**2 * (1 + H * math.cos(g * t)) * y[0]]
    a = solve_ivp(f, [0, T], [1, 0], method="DOP853", rtol=1e-12, atol=1e-14).y[:, -1]
    b = solve_ivp(f, [0, T], [0, 1], method="DOP853", rtol=1e-12, atol=1e-14).y[:, -1]
    return a[0] + b[1]


def test_mathieu_first_order():
    assert val(N22, "first-order band, lower edge") == pytest.approx(2 * W0 - H * W0 / 2, rel=REL)
    assert val(N22, "first-order band, upper edge") == pytest.approx(2 * W0 + H * W0 / 2, rel=REL)
    assert val(N22, "first-order largest growth rate") == pytest.approx(H * W0 / 4, rel=REL)


def test_mathieu_floquet():
    D = _trace(2 * W0)
    assert val(N22, "trace at 2ω₀") == pytest.approx(D, rel=1e-7)
    s = math.log((abs(D) + math.sqrt(D * D - 4)) / 2) * 2 * W0 / (2 * math.pi)
    assert val(N22, "Floquet growth rate at 2ω₀") == pytest.approx(s, rel=REL)
    assert s == pytest.approx(H * W0 / 4, rel=H**2)      # first order is right to O(h²)
    assert val(N22, "trace off resonance (γ = 1.5 ω₀)") == pytest.approx(_trace(1.5 * W0), rel=REL)
    lo = brentq(lambda g: _trace(g) + 2, 2 * W0 * (1 - H), 2 * W0, xtol=1e-13)
    hi = brentq(lambda g: _trace(g) + 2, 2 * W0, 2 * W0 * (1 + H), xtol=1e-13)
    assert val(N22, "exact band, lower edge") == pytest.approx(lo, rel=REL)
    assert val(N22, "exact band, upper edge") == pytest.approx(hi, rel=REL)
    assert val(N22, "band width, exact vs first order") == pytest.approx(hi - lo, rel=REL)
    # second order: both edges move down by h²ω₀/32 (error O(h³ω₀) ~ 1e-3 × 0.1 = 1e-4 here)
    shifts = vals(N22, "edge shifts, exact")
    assert shifts[0] == pytest.approx(lo - (2 * W0 - H * W0 / 2), abs=2e-6)
    assert shifts[1] == pytest.approx(hi - (2 * W0 + H * W0 / 2), abs=2e-6)
    assert shifts[2] == pytest.approx(-H**2 * W0 / 32, rel=1e-3)
    for x in shifts[:2]:
        assert x == pytest.approx(-H**2 * W0 / 32, abs=H**3 * W0)


def test_mathieu_growth():
    g = 2 * W0
    T = 2 * math.pi / g

    def f(t, y):
        return [y[1], -W0**2 * (1 + H * math.cos(g * t)) * y[0]]
    s = solve_ivp(f, [0, 60 * T], [1e-3, 0], method="DOP853", rtol=1e-12, atol=1e-16, dense_output=True)

    def amp(t):
        y = s.sol(t)
        return math.hypot(y[0], y[1] / W0)
    assert val(N22, "amplitude after 60 periods") == pytest.approx(1e3 * amp(60 * T), rel=REL)
    rate = math.log(amp(60 * T) / amp(40 * T)) / (20 * T)
    assert val(N22, "growth rate from the amplitude") == pytest.approx(rate, rel=REL)
    D = _trace(g)
    floq = math.log((abs(D) + math.sqrt(D * D - 4)) / 2) / T
    assert rate == pytest.approx(floq, rel=1e-4)     # the decaying Floquet solution is e^{-2 s 40T} ≈ 2e-3 smaller


# ---------------------------------------------------------------- 23 Duffing

N23 = "23_duffing_oscillator"
w0, beta = 5.0, 40.0


def _T_exact(A):
    # substitute x = A sin φ to remove the endpoint blow-up
    f = lambda p: A * math.cos(p) / math.sqrt(w0**2 * A**2 * math.cos(p)**2 + 0.5 * beta * (A**4 - (A * math.sin(p))**4))  # noqa: E731
    return 4 * quad(f, 0, math.pi / 2, epsabs=0, epsrel=1e-13)[0]


def _duffing_line(A, key):
    for ln in output(N23).split("\n"):
        if ln.startswith(f"A = {A:.3f} m") and key in ln:
            return ln
    raise AssertionError((A, key, output(N23)))


def test_duffing_periods():
    for A in (0.1, 0.2, 0.4):
        T = _T_exact(A)
        p1 = w0 + 3 * beta * A**2 / (8 * w0)
        p2 = p1 - 21 * beta**2 * A**4 / (256 * w0**3)
        parts = _duffing_line(A, "exact period").split("|")
        assert _nums(parts[1])[-1] == pytest.approx(T, rel=2e-6)
        assert _nums(parts[2])[-1] == pytest.approx(2 * math.pi / T, rel=2e-6)
        assert _nums(parts[3])[-1] == pytest.approx(p1, rel=2e-6)
        assert _nums(parts[4])[-1] == pytest.approx(p2, rel=2e-6)
        ode = _nums(_duffing_line(A, "ODE period").split("|")[1])[0]
        assert ode == pytest.approx(T, rel=2e-6)
        # Lindstedt–Poincaré to 2nd order is right to O(ε³), ε = βA²/ω₀² (0.064 at A = 0.4 m)
        eps = beta * A**2 / w0**2
        assert 2 * math.pi / T == pytest.approx(p2, rel=eps**3)


def test_duffing_error_scaling():
    def err1(A):
        return abs(2 * math.pi / _T_exact(A) - (w0 + 3 * beta * A**2 / (8 * w0)))
    assert val(N23, "first-order error ratio, A = 0.2 m vs 0.1 m") == pytest.approx(err1(0.2) / err1(0.1), rel=5e-4)
    assert val(N23, "first-order error ratio, A = 0.1 m vs 0.05 m") == pytest.approx(err1(0.1) / err1(0.05), rel=5e-4)
    assert err1(0.1) / err1(0.05) == pytest.approx(16, rel=0.02)


def test_duffing_driven():
    c, F, w = 0.5, 1.0, 5.5

    def f(t, y):
        return [y[1], F * math.cos(w * t) - c * y[1] - w0**2 * y[0] - beta * y[0]**3]
    s = solve_ivp(f, [0, 60], [0, 0], method="DOP853", rtol=1e-12, atol=1e-14, dense_output=True)
    ts = np.arange(55, 60 + 1e-9, 0.001)
    A_ode = np.abs(s.sol(ts)[0]).max()
    assert val(N23, "steady-state amplitude, ODE") == pytest.approx(100 * A_ode, rel=1e-4)
    a = w0**2 - w**2
    roots = np.roots([(0.75 * beta)**2, 1.5 * a * beta, a * a + (c * w)**2, -F * F])
    real = [r.real for r in roots if abs(r.imag) < 1e-12 and r.real > 0]
    assert len(real) == 1                                    # single-valued response here
    A_hb = math.sqrt(real[0])
    assert val(N23, "steady-state amplitude, harmonic balance") == pytest.approx(100 * A_hb, rel=1e-4)
    # harmonic balance drops the 3rd harmonic, of relative size ~ βA²/(32 ω²) ≈ 0.2 %; allow 1 %
    assert A_ode == pytest.approx(A_hb, rel=1e-2)


def test_all_second_pass_oscillation_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("2"))
    assert names == [N21, N22, N23]
