"""Gauntlet, second pass, thermodynamics: run gauntlet/thermodynamics/2*.fm and check the numbers independently.

References: the van der Waals critical point and a Maxwell construction done with NumPy's cubic
roots and the closed-form area ∫P dV = RT ln((V−b)/(V−b)) + a(1/V − 1/V); SciPy `quad` for the
Debye and D₃ integrals and `brentq` for the half-classical temperature; closed forms for the photon
gas (VT³ = const, W = ∮P dV with P ∝ V^(-4/3) on the adiabats) and the Stirling efficiencies.

Tolerances: 6 significant figures are printed, so rel 2e-5 covers rounding plus quadrature
(relative 1e-10) and regula-falsi errors. The Clausius–Clapeyron slope is a central difference
with a ±0.1 K step (truncation ~ (0.1/300)² ~ 1e-7) printed to 5 figures: rel 1e-4.
"""
import math
import os
import re

import numpy as np
import pytest
from scipy.integrate import quad
from scipy.optimize import brentq

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DIR = os.path.join(ROOT, "gauntlet", "thermodynamics")
REL = 2e-5
R = 8.314462618

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
        if line.startswith(label + ":"):
            return _nums(line[len(label) + 1:])
    raise AssertionError(f"no line {label!r} in output of {name}:\n{output(name)}")


def val(name, label, i=0):
    return vals(name, label)[i]


# ---------------------------------------------------------------- 21 van der Waals

N21 = "21_van_der_waals_maxwell"
A, B = 0.3640, 4.267e-5


def _vols(T, P):
    """Smallest and largest real roots of P V³ − (P b + R T) V² + a V − a b = 0."""
    r = np.roots([P, -(P * B + R * T), A, -A * B])
    r = sorted(x.real for x in r if abs(x.imag) < 1e-9 * abs(x))
    return r[0], r[-1]


def _area(T, P):
    vl, vg = _vols(T, P)
    return R * T * math.log((vg - B) / (vl - B)) + A * (1 / vg - 1 / vl) - P * (vg - vl)


def _spinodal_pressures(T):
    # ∂P/∂V = 0  ⇔  2a (V − b)² = R T V³
    r = np.roots([R * T, -2 * A, 4 * A * B, -2 * A * B * B])
    v = sorted(x.real for x in r if abs(x.imag) < 1e-12 and x.real > B)
    return v, [R * T / (x - B) - A / x**2 for x in v]


def _psat(T):
    _, (plo, phi) = _spinodal_pressures(T)
    return brentq(lambda P: _area(T, P), max(plo, 0) * (1 + 1e-9), phi * (1 - 1e-9), xtol=1e-9, rtol=1e-14)


def test_vdw_critical_point_and_spinodals():
    Tc, Pc = 8 * A / (27 * R * B), A / (27 * B * B)
    assert val(N21, "critical temperature") == pytest.approx(Tc, rel=REL)
    assert val(N21, "critical pressure") == pytest.approx(Pc / 1e6, rel=REL)
    assert val(N21, "critical volume") == pytest.approx(3 * B * 1e6, rel=REL)
    v, p = _spinodal_pressures(0.9 * Tc)
    assert vals(N21, "spinodal volumes") == pytest.approx([x * 1e6 for x in v], rel=REL)
    assert vals(N21, "spinodal pressures") == pytest.approx([x / 1e6 for x in p], rel=REL)


def test_vdw_maxwell_construction():
    Tc, Pc = 8 * A / (27 * R * B), A / (27 * B * B)
    T = 0.9 * Tc
    Ps = _psat(T)
    vl, vg = _vols(T, Ps)
    assert val(N21, "saturation pressure") == pytest.approx(Ps / 1e6, rel=REL)
    assert val(N21, "P_s / P_c") == pytest.approx(Ps / Pc, rel=REL)
    assert Ps / Pc == pytest.approx(0.6470, abs=1e-4)      # the universal reduced value at T_r = 0.9
    assert vals(N21, "liquid and gas volumes") == pytest.approx([vl * 1e6, vg * 1e6], rel=REL)
    assert abs(val(N21, "area check (J/mol)")) < 1e-9
    L = T * R * math.log((vg - B) / (vl - B))
    assert val(N21, "latent heat") == pytest.approx(L / 1e3, rel=REL)
    slope = (_psat(T + 0.1) - _psat(T - 0.1)) / 0.2
    assert val(N21, "dP_s/dT, numerical") == pytest.approx(slope / 1e3, rel=1e-4)
    assert val(N21, "dP_s/dT, Clapeyron") == pytest.approx(L / (T * (vg - vl)) / 1e3, rel=1e-4)
    assert slope == pytest.approx(L / (T * (vg - vl)), rel=1e-6)          # Clapeyron is exact


# ---------------------------------------------------------------- 22 Debye and Einstein

N22 = "22_debye_einstein"
TD = 343.0
TE = math.sqrt(0.6) * TD


def _CD(T):
    f = lambda x: x**4 * math.exp(-x) / (-math.expm1(-x))**2      # noqa: E731
    return 9 * R * (T / TD)**3 * quad(f, 0, TD / T, epsabs=0, epsrel=1e-12, limit=200)[0]


def _CE(T):
    y = TE / T
    return 3 * R * y * y * math.exp(-y) / (-math.expm1(-y))**2


def test_debye_einstein_table():
    assert val(N22, "matched Einstein temperature") == pytest.approx(TE, rel=REL)
    lines = [ln for ln in output(N22).split("\n") if ln.startswith("T = ")]
    assert len(lines) == 5
    for T, ln in zip((5, 20, 100, 300, 1000), lines):
        parts = ln.split("|")
        assert _nums(parts[0])[0] == T
        assert _nums(parts[1])[-1] == pytest.approx(_CD(T), rel=REL)
        assert _nums(parts[2])[-1] == pytest.approx(_CE(T), rel=REL)
        assert _nums(parts[3])[-1] == pytest.approx(12 * math.pi**4 / 5 * R * (T / TD)**3, rel=REL)
        assert _nums(parts[4])[-1] == pytest.approx(_CD(T) / (3 * R), rel=REL)
    # the limits: T³ law at 5 K (corrections ~ e^{-θ/T} ≈ 1e-30) and Dulong–Petit at 1000 K
    # (1 − θ²/(20T²) = 0.99412; the next term is O((θ/T)⁴) ≈ 1e-4)
    assert _CD(5) == pytest.approx(12 * math.pi**4 / 5 * R * (5 / TD)**3, rel=1e-9)
    assert _CD(1000) / (3 * R) == pytest.approx(1 - TD**2 / (20 * 1000**2), abs=2e-4)
    assert _CE(1000) / (3 * R) == pytest.approx(1 - TE**2 / (12 * 1000**2), abs=2e-4)


def test_debye_half_and_entropy():
    Th = brentq(lambda T: _CD(T) - 1.5 * R, 1, TD, xtol=1e-12)
    got = vals(N22, "C_D = 3R/2 at")
    assert got[0] == pytest.approx(Th, rel=REL)
    assert got[-1] == pytest.approx(Th / TD, rel=REL)
    S_int = quad(lambda u: _CD(u) / u, 0, 300, epsabs=0, epsrel=1e-11, limit=200)[0]
    y = TD / 300
    D3 = 3 / y**3 * quad(lambda t: t**3 * math.exp(-t) / (-math.expm1(-t)), 0, y, epsrel=1e-13)[0]
    S_closed = R * (4 * D3 - 3 * math.log(-math.expm1(-y)))
    assert S_int == pytest.approx(S_closed, rel=1e-8)
    assert val(N22, "entropy at 300 K, ∫ C/T dT") == pytest.approx(S_closed, rel=REL)
    assert val(N22, "entropy at 300 K, closed form") == pytest.approx(S_closed, rel=REL)


# ---------------------------------------------------------------- 23 photon Carnot, Stirling

N23 = "23_photon_carnot_stirling"
SIGMA, C = 5.670374419e-8, 299792458.0
AR = 4 * SIGMA / C


def test_photon_adiabat():
    assert val(N23, "radiation constant a") == pytest.approx(AR, rel=REL)
    assert val(N23, "adiabat: T at 8 L") == pytest.approx(1500.0, rel=1e-7)
    assert vals(N23, "V T³ at 1 L and 8 L")[-2:] == pytest.approx([2.7e7, 2.7e7], rel=1e-7)


def test_photon_carnot():
    Th, Tc, V1, V2 = 3000.0, 1500.0, 1e-3, 2e-3
    V3, V4 = V2 * (Th / Tc)**3, V1 * (Th / Tc)**3
    assert vals(N23, "corner volumes") == pytest.approx([1, 2, V3 * 1e3, V4 * 1e3], rel=REL)

    def adiabat_work(Va, Vb, V0, T0):
        # P = (a/3) T0⁴ (V0/V)^(4/3)  ⇒  ∫P dV = a T0⁴ V0^(4/3) (Va^(-1/3) − Vb^(-1/3))
        return AR * T0**4 * V0**(4 / 3) * (Va**(-1 / 3) - Vb**(-1 / 3))
    W = [AR * Th**4 / 3 * (V2 - V1), adiabat_work(V2, V3, V2, Th),
         AR * Tc**4 / 3 * (V4 - V3), adiabat_work(V4, V1, V4, Tc)]
    assert vals(N23, "work on each leg (μJ)") == pytest.approx([w * 1e6 for w in W], rel=REL)
    assert val(N23, "net work") == pytest.approx(sum(W) * 1e6, rel=REL)
    Qh = 4 / 3 * AR * Th**4 * (V2 - V1)
    Qc = 4 / 3 * AR * Tc**4 * (V4 - V3)
    assert vals(N23, "heat in, heat out") == pytest.approx([Qh * 1e6, Qc * 1e6], rel=REL)
    assert abs(val(N23, "first law, W − (Q_h + Q_c)")) < 1e-9
    assert abs(val(N23, "Clausius sum")) < 1e-20
    assert vals(N23, "photon Carnot efficiency")[0] == pytest.approx(sum(W) / Qh, rel=REL)
    assert sum(W) / Qh == pytest.approx(1 - Tc / Th, rel=1e-12)


def test_stirling():
    Th, Tc, r = 3000.0, 1500.0, 3.0
    Cv = 1.5 * R
    Wh = R * Th * math.log(r)
    W = R * (Th - Tc) * math.log(r)
    assert val(N23, "Stirling net work") == pytest.approx(W / 1e3, rel=REL)
    eta0 = W / (Wh + Cv * (Th - Tc))
    assert val(N23, "Stirling, no regenerator") == pytest.approx(eta0, rel=REL)
    assert val(N23, "Stirling, closed form") == pytest.approx(eta0, rel=REL)
    assert val(N23, "Stirling, perfect regenerator") == pytest.approx(1 - Tc / Th, rel=REL)
    assert val(N23, "Stirling, 90 % regenerator") == pytest.approx(W / (Wh + 0.1 * Cv * (Th - Tc)), rel=REL)


def test_all_second_pass_thermodynamics_problems_are_tested():
    names = sorted(f[:-3] for f in os.listdir(DIR) if f.endswith(".fm") and f.startswith("2"))
    assert names == [N21, N22, N23]
