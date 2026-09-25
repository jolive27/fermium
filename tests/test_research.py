"""Research reproductions (research/<name>/): each program's numbers against an independent computation."""
import os
import re

import numpy as np
import pytest

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RES = os.path.join(ROOT, "research")


def run_prog(name, prog):
    d = os.path.join(RES, name)
    return run(open(os.path.join(d, prog), encoding="utf-8").read(), base_dir=d)


def num(text, label):
    m = re.search(re.escape(label) + r"\s*=?\s*(-?[\d.]+(?:×10[⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+)?)", text)
    s = m.group(1)
    if "×10" in s:
        mant, ex = s.split("×10")
        ex = ex.translate(str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789"))
        return float(mant) * 10 ** int(ex)
    return float(s)


def test_semf_fit_matches_numpy_least_squares():
    out = run_prog("semf_ame2020", "semf.fm")
    data = np.genfromtxt(os.path.join(RES, "semf_ame2020", "ame2020_binding.csv"), delimiter=",", skip_header=1)
    Z, N, A, P, B = data.T
    X = np.column_stack([A, -A ** (2 / 3), -Z * (Z - 1) / A ** (1 / 3), -(A - 2 * Z) ** 2 / A, P / np.sqrt(A)])
    coef, *_ = np.linalg.lstsq(X, B, rcond=None)     # the model is linear in the coefficients
    for name, c in zip(["a_V", "a_S", "a_C", "a_A", "a_P"], coef):
        assert num(out, f"  {name} =") == pytest.approx(c, rel=2e-4), name
    rms = np.sqrt(np.mean((B - X @ coef) ** 2))
    assert num(out, "rms residual:") == pytest.approx(np.std(B - X @ coef, ddof=1), rel=1e-4)   # sample std
    assert rms < 3.5
    assert "at Z = 50 N = 82" in out          # doubly magic ¹³²Sn has the largest shell correction
    for n0 in (50, 82, 126):
        line = next(ln for ln in out.splitlines() if ln.startswith(f"N = {n0} "))
        vals = [float(v) for v in re.findall(r"(-?[\d.]+) MeV", line)]
        assert vals[0] > vals[1] and vals[0] > vals[2]     # magic isotones are more bound than N ± 8


def line_of(out, start):
    return next(ln for ln in out.splitlines() if ln.startswith(start))


def floats(text):
    return [float(v) for v in re.findall(r"-?\d+\.?\d*(?:e-?\d+)?", text)]


def test_tov_ideal_neutron_gas_matches_scipy():
    """Oppenheimer–Volkoff 1939: the ideal neutron Fermi gas has M_max ≈ 0.71 M☉."""
    from scipy.constants import G, c, hbar, m_n, pi
    from scipy.integrate import solve_ivp
    from scipy.optimize import minimize_scalar
    out = run_prog("neutron_star_tov", "tov.fm")
    msun = 1.98841e30
    K = m_n ** 4 * c ** 5 / (8 * pi ** 2 * hbar ** 3)

    def eps(x):
        return K * ((2 * x ** 3 + x) * np.sqrt(1 + x * x) - np.arcsinh(x))

    def P(x):
        return K / 3 * ((2 * x ** 3 - 3 * x) * np.sqrt(1 + x * x) + 3 * np.arcsinh(x))

    def rhs(r, s):
        y, m = s
        x = np.sqrt(abs(y))
        dPdr = -G * (eps(x) + P(x)) * (m + 4 * pi * r ** 3 * P(x) / c ** 2) / (c ** 2 * r ** 2 * (1 - 2 * G * m / (r * c ** 2)))
        return [2 * x * dPdr / (8 * K * x ** 4 / (3 * np.sqrt(1 + x * x))), 4 * pi * r ** 2 * eps(x) / c ** 2]

    def surface(r, s):
        return s[0]
    surface.terminal = True

    def star(xc):
        sol = solve_ivp(rhs, [1.0, 2e5], [xc ** 2, 0.0], events=surface, rtol=1e-10, atol=[1e-14, 1e10])
        return sol.y_events[0][0][1] / msun, sol.t_events[0][0] / 1e3

    best = minimize_scalar(lambda x: -star(x)[0], bounds=(0.5, 1.5), method="bounded", options={"xatol": 1e-6})
    mmax, rmax = star(best.x)
    assert num(out, "maximum mass:") == pytest.approx(mmax, rel=2e-4)
    assert num(out, "radius at maximum mass:") == pytest.approx(rmax, rel=2e-3)
    assert num(out, "central Fermi momentum x_c =") == pytest.approx(best.x, rel=2e-2)   # the maximum is flat
    assert abs(mmax - 0.71) < 0.005                                    # Oppenheimer & Volkoff (1939)
    for xc in (0.2, 0.5, 1.0, 2.0):
        line = line_of(out, f"x_c = {xc:g} :")
        m, r = floats(line.split(":")[1])
        ms, rs = star(xc)
        assert m == pytest.approx(ms, rel=1e-3) and r == pytest.approx(rs, rel=1e-3), xc
    # Newtonian limit: n = 3/2 polytrope, K = (3π²)^(2/3) ħ²/(5 m_n^(8/3)); M R³ = 4π ω₁ ξ₁³ (5K/(8πG))³
    Kp = (3 * pi ** 2) ** (2 / 3) * hbar ** 2 / (5 * m_n ** (8 / 3))
    mr3 = 4 * pi * 2.71406 * 3.65375 ** 3 * (2.5 * Kp / (4 * pi * G)) ** 3 / (msun * 1e9)
    lo = floats(line_of(out, "M R³").split(":")[1])
    assert lo[0] == pytest.approx(mr3, rel=0.03) and lo[1] == pytest.approx(mr3, rel=0.03)


def test_lane_emden_and_chandrasekhar_mass():
    """Lane–Emden constants vs SciPy and Chandrasekhar's 1939 table; M_Ch = 4π ω₃ (K/πG)^(3/2)."""
    from scipy.constants import G, c, hbar, m_e, m_p, m_u, pi
    from scipy.integrate import solve_ivp
    out = run_prog("lane_emden_chandrasekhar", "lane_emden.fm")
    table = {"1": (np.pi, np.pi), "1.5": (3.65375, 2.71406), "3": (6.89685, 2.01824)}   # Chandrasekhar (1939)
    omega = {}
    for n, (xi_tab, om_tab) in table.items():
        nn = float(n)

        def rhs(x, s):
            return [s[1], -2 * s[1] / x - np.sign(s[0]) * abs(s[0]) ** nn]

        def zero(x, s):
            return s[0]
        zero.terminal = True
        x0 = 1e-5
        sol = solve_ivp(rhs, [x0, 10], [1 - x0 ** 2 / 6, -x0 / 3], events=zero, rtol=1e-12, atol=1e-14)
        xi1, dth = sol.t_events[0][0], sol.y_events[0][0][1]
        xi_fm, om_fm, ratio = floats(line_of(out, f"n = {n}:").split(":", 1)[1].replace("²", ""))[-3:]
        assert xi_fm == pytest.approx(xi1, rel=1e-6) and om_fm == pytest.approx(-xi1 ** 2 * dth, rel=1e-6), n
        assert xi_fm == pytest.approx(xi_tab, abs=2e-5) and om_fm == pytest.approx(om_tab, abs=2e-5), n
        assert ratio == pytest.approx(xi1 ** 3 / (3 * -xi1 ** 2 * dth), rel=1e-4)
        omega[n] = -xi1 ** 2 * dth
    msun = 1.98841e30
    mch1 = 4 * pi * omega["3"] * ((hbar * c / 4) * (3 * pi ** 2) ** (1 / 3) / (m_u ** (4 / 3) * pi * G)) ** 1.5 / msun
    assert num(out, "M_Ch μ_e² =") == pytest.approx(mch1, rel=1e-3)
    assert mch1 == pytest.approx(np.sqrt(3 * pi) / 2 * omega["3"] * (hbar * c / G) ** 1.5 / m_u ** 2 / msun, rel=1e-9)
    assert abs(mch1 - 5.83) < 0.01                                          # the textbook 5.83/μ_e² M☉
    assert num(out, "white dwarf) =") == pytest.approx(mch1 / 4, rel=1e-3)
    mch_h = mch1 / 4 * (m_u / (m_p + m_e)) ** 2
    assert floats(line_of(out, "with m_H").split("μ_e = 2:")[1])[0] == pytest.approx(mch_h, rel=1e-3)
    assert abs(mch_h - 1.44) < 0.01                                         # Chandrasekhar's 1.44 M☉
    # n = 3/2 white dwarf, closed form
    K = (3 * pi ** 2) ** (2 / 3) * hbar ** 2 / (5 * m_e * (2 * m_u) ** (5 / 3))
    b = 5 * K / (8 * pi * G)
    rho_c = (0.6 * msun / (4 * pi * b ** 1.5 * omega["1.5"])) ** 2
    R = table["1.5"][0] * np.sqrt(b) * rho_c ** (-1 / 6)
    assert floats(line_of(out, "n = 3/2 white dwarf").split("R =")[1])[0] == pytest.approx(R / 1e3, rel=5e-3)
