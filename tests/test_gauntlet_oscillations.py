"""Gauntlet, oscillations: run each gauntlet/oscillations/*.fm and check its numbers independently.

References: closed-form formulas evaluated here, scipy.special.ellipk, scipy.integrate.solve_ivp
and numpy.linalg.eigh for the normal modes.

Tolerances: 6-digit output -> rel=2e-5 (rounding is at most 5e-6 relative; the rest covers the
RK45 solver at rtol 1e-9 over tens of periods). The pendulum prints 8 digits -> rel=2e-7.
Where a quantity is zero in exact arithmetic we compare absolutely (stated at the assert).
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import solve_ivp
from scipy.special import ellipk

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
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


def numbers(text):
    return [float(re.sub(r"×10(\S+)", lambda m: "e" + m.group(1).translate(_SUP), t))
            for t in _NUM.findall(text)]


def lines(name, prefix):
    found = [numbers(line[len(prefix):]) for line in output(name).split("\n")
             if line.startswith(prefix)]
    assert found, f"no line {prefix!r} in output of {name}:\n{output(name)}"
    return found


def val(name, label, i=0):
    return lines(name, label + ":")[0][i]


# ---------------------------------------------------------------- 01 driven resonance

M, K, B, F0 = 0.200, 80.0, 0.400, 0.500
W0 = math.sqrt(K / M)
GAM = B / M


def amp(w):
    return (F0 / M) / math.sqrt((W0**2 - w**2)**2 + (GAM * w)**2)


def test_resonance_formulas():
    n = "01_driven_resonance"
    assert val(n, "natural frequency") == pytest.approx(W0, rel=REL)
    assert val(n, "damping rate") == pytest.approx(GAM, rel=REL)
    assert val(n, "quality factor") == pytest.approx(W0 / GAM, rel=REL)
    wr = math.sqrt(W0**2 - GAM**2 / 2)
    assert val(n, "resonance frequency") == pytest.approx(wr, rel=REL)
    assert val(n, "peak amplitude") == pytest.approx(100 * amp(wr), rel=REL)       # cm
    assert val(n, "static deflection F0/k") == pytest.approx(100 * F0 / K, rel=REL)
    # half-power points: A(w)^2 = A_max^2/2 is a quadratic in w^2, solved in closed form here
    target = amp(wr)**2 / 2
    # (w^2)^2 + (g^2 - 2 w0^2) w^2 + w0^4 - (F0/M)^2/target = 0
    b = GAM**2 - 2 * W0**2
    c = W0**4 - (F0 / M)**2 / target
    lo2, hi2 = (-b - math.sqrt(b * b - 4 * c)) / 2, (-b + math.sqrt(b * b - 4 * c)) / 2
    assert lines(n, "half-power points:")[0] == pytest.approx(
        [math.sqrt(lo2), math.sqrt(hi2)], rel=REL)
    assert val(n, "full width at half power") == pytest.approx(
        math.sqrt(hi2) - math.sqrt(lo2), rel=REL)


def test_resonance_sweep_matches_formula():
    """The ODE amplitude and phase after 30 s (transient e^-30 ~ 1e-13) equal the formula."""
    rows = lines("01_driven_resonance", "ω/ω0 =")
    assert [r[0] for r in rows] == [0.5, 0.9, 0.99, 1.0, 1.1, 2.0]
    for ratio, A_ode, A_f, ph_ode, ph_f in rows:
        w = ratio * W0
        assert A_ode == pytest.approx(100 * amp(w), rel=REL)
        assert A_f == pytest.approx(100 * amp(w), rel=REL)
        delta = math.degrees(math.atan2(GAM * w, W0**2 - w**2))
        assert ph_ode == pytest.approx(delta, rel=REL)
        assert ph_f == pytest.approx(delta, rel=REL)


def test_resonance_ode_independently():
    """SciPy integration of the same ODE at w = 0.99 w0 gives the same steady-state amplitude."""
    w = 0.99 * W0
    s = solve_ivp(lambda t, y: [y[1], (-K * y[0] - B * y[1] + F0 * math.cos(w * t)) / M],
                  [0, 30], [0, 0], rtol=1e-12, atol=1e-14)
    x, v = s.y[:, -1]
    row = [r for r in lines("01_driven_resonance", "ω/ω0 =") if r[0] == 0.99][0]
    assert row[1] == pytest.approx(100 * math.hypot(x, v / w), rel=REL)


# ---------------------------------------------------------------- 02 large pendulum

def test_pendulum_periods():
    n = "02_large_pendulum"
    L, g = 1.0, 9.81
    T0 = 2 * math.pi * math.sqrt(L / g)
    assert val(n, "small-angle period") == pytest.approx(T0, rel=2e-7)
    rows = lines(n, "amplitude")
    assert [r[0] for r in rows] == [10, 45, 90, 150, 179]
    for deg, T_ell, T_en, T_ode, T_ser in rows:
        th = math.radians(deg)
        exact = 4 * math.sqrt(L / g) * ellipk(math.sin(th / 2)**2)   # scipy: parameter m = k^2
        assert T_ell == pytest.approx(exact, rel=2e-7)
        assert T_en == pytest.approx(exact, rel=2e-7)   # the singular energy integral
        assert T_ode == pytest.approx(exact, rel=2e-7)
        assert T_ser == pytest.approx(T0 * (1 + th**2 / 16 + 11 * th**4 / 3072), rel=2e-7)


# ---------------------------------------------------------------- 03 coupled oscillators

def test_coupled_equal_masses():
    n = "03_coupled_oscillators"
    k, kc, m, A = 20.0, 2.0, 0.5, 5.0          # A in cm
    w1, w2 = math.sqrt(k / m), math.sqrt((k + 2 * kc) / m)
    assert val(n, "in-phase mode") == pytest.approx(w1, rel=REL)
    assert val(n, "out-of-phase mode") == pytest.approx(w2, rel=REL)
    t_swap = math.pi / (w2 - w1)
    assert val(n, "energy transfer time") == pytest.approx(t_swap, rel=REL)

    # independent: integrate with SciPy
    def f(t, y):
        x1, x2, v1, v2 = y
        return [v1, v2, (-k * x1 - kc * (x1 - x2)) / m, (-k * x2 - kc * (x2 - x1)) / m]
    rows = lines(n, "t =")
    assert len(rows) == 3
    for t_exact, (t, x1, x1f, x2, x2f) in zip([1.0, 2.5, t_swap], rows):
        assert t == pytest.approx(t_exact, rel=REL)
        s = solve_ivp(f, [0, t_exact], [A, 0, 0, 0], rtol=1e-12, atol=1e-12)
        # x1 at t_swap is 0 in exact arithmetic: compare absolutely (1e-6 cm = 2e-7 of A)
        assert x1 == pytest.approx(s.y[0, -1], rel=REL, abs=1e-6)
        assert x1f == pytest.approx(s.y[0, -1], rel=REL, abs=1e-6)
        assert x2 == pytest.approx(s.y[1, -1], rel=REL, abs=1e-6)
        assert x2f == pytest.approx(s.y[1, -1], rel=REL, abs=1e-6)
    assert val(n, "ω1 from the ODE") == pytest.approx(w1, rel=REL)


def test_coupled_unequal_masses():
    n = "03_coupled_oscillators"
    k, kc, m1, m2 = 20.0, 2.0, 0.5, 1.0
    Kmat = np.array([[k + kc, -kc], [-kc, k + kc]])
    Minv_sqrt = np.diag([1 / math.sqrt(m1), 1 / math.sqrt(m2)])
    w2, vecs = np.linalg.eigh(Minv_sqrt @ Kmat @ Minv_sqrt)
    modes = Minv_sqrt @ vecs                       # back to x coordinates
    ratios = modes[1] / modes[0]
    slow, fast = lines(n, "slow mode:")[0], lines(n, "fast mode:")[0]
    assert slow == pytest.approx([math.sqrt(w2[0]), ratios[0]], rel=REL)
    assert fast == pytest.approx([math.sqrt(w2[1]), ratios[1]], rel=REL)
    # started in a mode, the motion stays a pure cosine: the departure is solver error only.
    # 1e-5 cm is 2e-6 of the 5 cm amplitude, i.e. 100x above what RK45 at rtol 1e-9 leaves.
    assert abs(val(n, "largest departure from a pure slow mode")) < 1e-5


def test_all_oscillation_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and not f.startswith("2"))  # pass 1 only
    assert names == ["01_driven_resonance", "02_large_pendulum", "03_coupled_oscillators"]
