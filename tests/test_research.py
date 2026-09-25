"""Research reproductions (research/<name>/): each program's numbers against an independent computation."""
import os
import re

import numpy as np
import pytest

from conftest import run
from fermium.errors import FermiumError

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
    rows = [ln for ln in out.splitlines() if ln.startswith("x_c =")]
    for xc, line in zip((0.2, 0.5, 1.0, 2.0), rows, strict=True):
        assert num(line, "x_c =") == pytest.approx(xc)
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


def test_u238_chain_matches_analytic_bateman():
    """The 15-member U-238 series (radau) against the closed-form Bateman solution in 60-digit arithmetic."""
    import mpmath as mp
    out = run_prog("u238_chain", "u238_chain.fm")
    mp.mp.dps = 60
    yr, day, mn = mp.mpf(365.25 * 86400), mp.mpf(86400), mp.mpf(60)
    half = [4.468e9 * yr, 24.10 * day, 1.159 * mn, 2.455e5 * yr, 7.538e4 * yr, 1600 * yr, 3.8235 * day,
            3.098 * mn, 26.8 * mn, 19.9 * mn, mp.mpf("164.3e-6"), 22.20 * yr, 5.012 * day, 138.376 * day]
    lam = [mp.log(2) / h for h in half]

    def activity(n, t, lams=lam):          # λ_n N_n(t) / N_1(0), members 0-based, pure parent at t = 0
        s = sum(mp.exp(-lams[j] * t) / mp.fprod([lams[k] - lams[j] for k in range(n + 1) if k != j])
                for j in range(n + 1))
        return mp.fprod(lams[:n + 1]) * s

    names = {"Th-234": 1, "U-234": 3, "Th-230": 4, "Ra-226": 5, "Rn-222": 6, "Po-214": 10, "Po-210": 13}
    rows = [ln for ln in out.splitlines() if ln.startswith("t = ")]
    for T, line in zip((1, 1000, 100000, 3000000), rows, strict=True):
        for name, i in names.items():
            v = num(line, name)
            exact = float(activity(i, T * yr) / activity(0, T * yr))
            assert v == pytest.approx(exact, rel=2e-4), (T, name)
    worst = max(abs(float(activity(i, 3e6 * yr) / activity(0, 3e6 * yr)) - 1) for i in range(1, 14))
    assert num(out, "largest |A_i/A(U-238) − 1| =") == pytest.approx(worst, rel=0.05)
    assert worst < 1e-3                                                  # secular equilibrium
    assert abs(num(out, "total/N₀ − 1 =")) < 1e-9
    assert num(out, "activity of 1 kg of U-238:") == pytest.approx(12.44e6, rel=1e-3)   # 12.4 kBq/g
    # two-member in-growth: A2/A1 = λ2/(λ2 − λ1) (1 − exp(−(λ2 − λ1) t)) = 0.99
    for text, (l1, l2) in {"Rn-222 reaches": (lam[5], lam[6]), "Th-234 reaches": (lam[0], lam[1])}.items():
        t99 = -mp.log(1 - 0.99 * (l2 - l1) / l2) / (l2 - l1) / day
        assert num(line_of(out, text), "after") == pytest.approx(float(t99), rel=1e-4)
    assert num(line_of(out, "Rn-222 reaches"), "after") == pytest.approx(3.8235 * np.log(100) / np.log(2), rel=5e-4)   # 6.64 half-lives
    t99 = -mp.log(1 - 0.99 * (lam[1] - lam[0]) / lam[1]) / (lam[1] - lam[0])
    assert num(out, "A(Pa-234m)/A(U-238) =") == pytest.approx(float(activity(2, t99) / activity(0, t99)), rel=1e-4)


def test_hydrogen_levels_match_bohr_with_reduced_mass():
    """Shooting eigenvalues n = 1..4, l = 0..n−1 vs −Ry (μ/m_e)/n²; Lyman α vs NIST 121.567 nm."""
    from scipy.constants import c, e, h, m_e, m_p, physical_constants
    out = run_prog("hydrogen_levels", "hydrogen.fm")
    ry = physical_constants["Rydberg constant times hc in eV"][0]
    mu = m_e * m_p / (m_e + m_p)
    levels = re.findall(r"n = ([\d.]+)\s+l = ([\d.]+) : E = (-[\d.]+) eV", out)
    assert sorted((round(float(n)), round(float(l))) for n, l, _ in levels) == [(n, l) for n in range(1, 5) for l in range(n)]
    for n, l, E in levels:
        assert float(E) == pytest.approx(-ry * mu / m_e / round(float(n)) ** 2, rel=1e-7), (n, l)
    assert num(out, "largest relative difference from Bohr:") < 1e-7
    lya = h * c / (0.75 * ry * mu / m_e * e) * 1e9
    assert num(out, "Lyman α (2p → 1s):") == pytest.approx(lya, rel=1e-6)
    assert num(out, "Lyman α (2p → 1s):") == pytest.approx(121.567, rel=2e-5)     # NIST (fine structure ~10⁻⁵)
    assert num(out, "without the reduced mass it would be") == pytest.approx(lya * mu / m_e, rel=1e-6)
    assert num(out, "2p: u² peaks at r =") == pytest.approx(4 * m_e / mu, rel=1e-5)


def test_friedmann_planck2018_matches_quad():
    """Age, epochs and distances for Planck 2018 ΛCDM vs scipy.integrate.quad and brentq."""
    from scipy.constants import G, c, pi, sigma
    from scipy.integrate import quad
    from scipy.optimize import brentq
    out = run_prog("friedmann_planck2018", "friedmann.fm")
    mpc, yr = 3.0856775814913673e22, 365.25 * 86400
    gyr, gly = 1e9 * yr, 1e9 * c * yr
    H0, om = 67.4e3 / mpc, 0.315
    og = 4 * sigma * 2.7255 ** 4 / c ** 3 / (3 * H0 ** 2 / (8 * pi * G))
    orad = og * (1 + 7 / 8 * (4 / 11) ** (4 / 3) * 3.046)
    ol = 1 - om - orad

    def H(a):
        return H0 * np.sqrt(orad / a ** 4 + om / a ** 3 + ol)

    def age(a1):
        return quad(lambda a: 1 / (a * H(a)), 0, a1, epsabs=0, epsrel=1e-12, limit=200)[0]

    assert num(out, "Ω_r =") == pytest.approx(orad, rel=1e-3)
    t0 = age(1) / gyr
    assert num(out, "age of the universe t₀ =") == pytest.approx(t0, rel=1e-4)
    assert num(out, "until a = 1:") == pytest.approx(t0, rel=1e-4)
    assert abs(t0 - 13.787) < 0.020                           # Planck 2018 VI, Table 2: 13.787 ± 0.020 Gyr
    assert num(out, "matter–Λ equality: z =") == pytest.approx((ol / om) ** (1 / 3) - 1, rel=1e-3)
    zq = brentq(lambda z: om * (1 + z) ** 3 + 2 * orad * (1 + z) ** 4 - 2 * ol, 0, 10)
    assert num(out, "(q = 0): z =") == pytest.approx(zq, rel=1e-3)
    assert num(out, "at t =") == pytest.approx(age(1 / (1 + zq)) / gyr, rel=1e-3)
    assert num(out, "matter–radiation equality: z =") == pytest.approx(om / orad - 1, rel=1e-3)
    dc = c * quad(lambda z: 1 / H(1 / (1 + z)), 0, 1100, epsrel=1e-12, limit=200)[0]
    assert num(out, "comoving distance to z = 1100:") == pytest.approx(dc / mpc, rel=1e-4)
    assert num(out, "age of the universe at z = 1100:") == pytest.approx(age(1 / 1101) / yr, rel=1e-3)
    ph = c * quad(lambda a: 1 / (a * a * H(a)), 0, 1, epsrel=1e-12, limit=200)[0]
    assert num(out, "particle horizon today (comoving):") == pytest.approx(ph / gly, rel=1e-3)


def test_rutherford_monte_carlo_statistics():
    """MC histogram vs the exact Rutherford bin contents; rand() has no seed, so the bounds are statistical (5σ)."""
    from scipy.constants import e, epsilon_0, pi
    from scipy.stats import chi2
    out = run_prog("rutherford_mc", "rutherford.fm")
    d = 2 * 79 * e ** 2 / (4 * pi * epsilon_0 * 5e6 * e)
    assert num(out, "distance of closest approach d =") == pytest.approx(d * 1e15, rel=1e-3)
    bmax = d / 2 / np.tan(np.radians(2.5))
    assert num(out, "σ =") == pytest.approx(pi * bmax ** 2 / 1e-28, rel=1e-3)
    assert num(out, "exact bin contents:") < chi2.ppf(1 - 1e-6, 35)            # χ² with 35 dof
    N = 2e7
    p = (d / 2) ** 2 / bmax ** 2                                                 # b(90°)² / b_max²
    frac = num(out, "backward (θ > 90°) fraction:")
    assert abs(frac - p) < 5 * np.sqrt(p / N) and num(out, "exact:") == pytest.approx(p, rel=1e-5)
    gm = {150: 33.1, 135: 43.0, 120: 51.9, 105: 69.5, 75: 211, 60: 477, 45: 1435, 37.5: 3300, 30: 7800,
          22.5: 27300, 15: 132000}
    mean = np.mean([n * np.sin(np.radians(a) / 2) ** 4 for a, n in gm.items()])
    rows = re.findall(r"θ = ([\d.]+) °: MC dσ/dΩ sin⁴\(θ/2\)/\(d/4\)² = ([\d.]+)\s+\( ([\d.]+(?:×10[⁰¹²³⁴⁵⁶⁷⁸⁹]+)?) α; exact bin average ([\d.]+) \)"
                      r"\s+Geiger–Marsden N sin⁴\(θ/2\) / mean = ([\d.]+)", out)
    assert len(rows) == 11
    for a, ratio, n, exact, gmr in rows:
        a, ratio, n, exact, gmr = float(a), float(ratio), num(n, ""), float(exact), float(gmr)
        lo, hi = np.radians(a - 2.5), np.radians(a + 2.5)
        dOmega = 2 * pi * (np.cos(lo) - np.cos(hi))
        ex = pi * (d / 2) ** 2 * (1 / np.tan(lo / 2) ** 2 - 1 / np.tan(hi / 2) ** 2) / dOmega * np.sin(np.radians(a) / 2) ** 4 / (d / 4) ** 2
        assert exact == pytest.approx(ex, abs=0.006), a
        assert abs(ratio - ex) < 5 * ex / np.sqrt(n) + 0.006, a                 # Poisson, 5σ (+ printed rounding)
        assert gmr == pytest.approx(gm[a] * np.sin(np.radians(a) / 2) ** 4 / mean, abs=0.006)
        assert 0.8 < gmr < 1.25                                                 # Geiger–Marsden: N sin⁴ ≈ constant
    assert num(out, "Rutherford dσ/dΩ at 90°:") == pytest.approx((d / 4) ** 2 / np.sin(pi / 4) ** 4 / 1e-28, rel=1e-3)


def test_pp_cno_crossover_matches_brentq():
    """pp/CNO crossover (Carroll & Ostlie and Kippenhahn & Weigert rates) vs brentq; published ≈ 17–18 MK."""
    from scipy.optimize import brentq
    out = run_prog("pp_cno_crossover", "pp_cno.fm")
    X, Xc = 0.70, 0.01

    def pp(t6):
        return 0.241 * X ** 2 * t6 ** (-2 / 3) * np.exp(-33.80 * t6 ** (-1 / 3))

    def cno(t6):
        return 8.67e20 * X * Xc * t6 ** (-2 / 3) * np.exp(-152.28 * t6 ** (-1 / 3))

    t_co = brentq(lambda t: np.log(pp(t) / cno(t)), 5, 50, xtol=1e-12)
    assert num(out, "crossover (Carroll & Ostlie rates): T =") == pytest.approx(t_co, rel=1e-3)
    assert num(out, "closed form:") == pytest.approx(t_co, rel=1e-3)
    assert 17 < t_co < 18.5                                            # the textbook crossover near 18 MK
    for name, f in (("pp", pp), ("CNO", cno)):
        h = 1e-5
        nu = (np.log(f(15 * (1 + h))) - np.log(f(15 * (1 - h)))) / (np.log(1 + h) - np.log(1 - h))
        assert num(line_of(out, "d ln ε/d ln T"), name) == pytest.approx(nu, rel=2e-3), name

    def pp_kw(T):
        T9 = T / 1e3
        g11 = 1 + 3.82 * T9 + 1.51 * T9 ** 2 + 0.144 * T9 ** 3 - 0.0114 * T9 ** 4
        return 2.57e4 * X ** 2 * g11 * T9 ** (-2 / 3) * np.exp(-3.381 * T9 ** (-1 / 3))

    def cno_kw(T):
        T9 = T / 1e3
        g141 = 1 - 2.00 * T9 + 3.41 * T9 ** 2 - 2.43 * T9 ** 3
        return 8.24e25 * X * Xc * g141 * T9 ** (-2 / 3) * np.exp(-15.231 * T9 ** (-1 / 3) - (T9 / 0.8) ** 2)

    t_kw = brentq(lambda t: np.log(pp_kw(t) / cno_kw(t)), 5, 50, xtol=1e-12)
    assert num(out, "crossover (Kippenhahn & Weigert rates): T =") == pytest.approx(t_kw, rel=1e-3)
    assert 17 < t_kw < 18.5


def _bbn_model(weak_scale=1.0):
    """The research/bbn_network model in NumPy: the same equations, constants and rate fits as bbn.fm.

    Quadratures are 400-point Gauss–Legendre (smooth integrands on finite ranges). The state is
    [T/MeV, Y_n, Y_p, Y_d, Y_3H, Y_3He, Y_4He, Y_7Li, Y_7Be]; weak_scale multiplies both weak rates."""
    from scipy.constants import G, N_A, c, e, hbar, k, m_e, m_u, pi
    MeV, hc, me = 1e6 * e, hbar * c, m_e * c ** 2
    zeta3, q = 1.2020569031595942, 1.29333 * MeV / me
    xg, wg = np.polynomial.legendre.leggauss(400)

    def gl(f, a, b):
        return 0.5 * (b - a) * np.dot(wg, f(0.5 * (b - a) * xg + 0.5 * (b + a)))

    def fd(y):
        return 1 / (1 + np.exp(y))

    lam0 = gl(lambda x: x * np.sqrt(x * x - 1) * (q - x) ** 2, 1, q)

    def epm(T):                                     # e± energy density, pressure, dρ/dT
        x = me / T

        def E(u):
            return np.sqrt(u * u + x * x)
        kk = 2 / pi ** 2 / hc ** 3
        return (kk * T ** 4 * gl(lambda u: u * u * E(u) * fd(E(u)), 0, 60 + x),
                kk * T ** 4 * gl(lambda u: u ** 4 / (3 * E(u)) * fd(E(u)), 0, 60 + x),
                kk * T ** 3 * gl(lambda u: u * u * E(u) ** 2 * fd(E(u)) * fd(-E(u)), 0, 60 + x))

    def rho_g(T):
        return pi ** 2 / 15 * T ** 4 / hc ** 3

    def Tnu(T):                                     # entropy conservation, s_γe = (11/4) s_γ(T_ν)
        re, pe, _ = epm(T)
        return ((4 / 3 * rho_g(T) + re + pe) / T / (11 / 4 * 4 * pi ** 2 / 45 / hc ** 3)) ** (1 / 3)

    def H(T):
        return np.sqrt(8 * pi * G * (rho_g(T) + epm(T)[0] + 3 * 7 / 8 * rho_g(Tnu(T))) / (3 * c ** 2))

    def weak(sg, T):                                # λ(n→p) for sg = 1, λ(p→n) for sg = −1
        z, zn = me / T, me / Tnu(T)

        def f(w):
            x = np.cosh(w)
            return x * np.sinh(w) ** 2 * ((x - sg * q) ** 2 * fd(-x * z) * fd((x - sg * q) * zn)
                                          + (x + sg * q) ** 2 * fd(x * z) * fd(-(x + sg * q) * zn))
        return weak_scale * gl(f, 0, np.arccosh(q + 60 / z)) / (878.4 * lam0)

    rhoc100 = 3 * (100e3 / 3.0856775814913673e22) ** 2 / (8 * pi * G)
    eta = 0.0224 * rhoc100 / ((0.753 * 1.007825 + 0.247 * 4.002602 / 4) * m_u * 2 * zeta3 / pi ** 2 * (k * 2.7255 / hc) ** 3)

    def n_b(T):
        return eta * 11 / 4 * 2 * zeta3 / pi ** 2 * (Tnu(T) / hc) ** 3

    def rates(T):                                   # N_A<σv> in cm³/(mol s) → <σv> in m³/s
        t = min(T / (k * 1e9), 10.0)
        t12, t13, t23, t32, t43, t53 = t ** .5, t ** (1 / 3), t ** (2 / 3), t ** 1.5, t ** (4 / 3), t ** (5 / 3)
        a1, a2, a3, a4 = t / (1 + .0495 * t), t / (1 + .1378 * t), t / (1 + 13.076 * t), t / (1 + .759 * t)
        ex = np.exp
        r = [4.742e4 * (1 - .8504 * t12 + .4895 * t - .09623 * t32 + 8.471e-3 * t ** 2 - 2.80e-4 * t ** 2.5),
             2.65e3 / t23 * ex(-3.720 / t13) * (1 + .112 * t13 + 1.99 * t23 + 1.56 * t + .162 * t43 + .324 * t53),
             3.95e8 / t23 * ex(-4.259 / t13) * (1 + .098 * t13 + .765 * t23 + .525 * t + 9.61e-3 * t43 + .0167 * t53),
             4.17e8 / t23 * ex(-4.258 / t13) * (1 + .098 * t13 + .518 * t23 + .355 * t - .010 * t43 - .018 * t53),
             7.21e8 * (1 - .508 * t12 + .228 * t),
             1.063e11 / t23 * ex(-4.559 / t13 - (t / .0754) ** 2)
             * (1 + .092 * t13 - .375 * t23 - .242 * t + 33.82 * t43 + 55.42 * t53) + 8.047e8 / t23 * ex(-.4857 / t),
             5.021e10 / t23 * ex(-7.144 / t13 - (t / .270) ** 2)
             * (1 + .058 * t13 + .603 * t23 + .245 * t + 6.97 * t43 + 7.19 * t53) + 5.212e8 / t12 * ex(-1.762 / t),
             4.817e6 / t23 * ex(-14.964 / t13) * (1 + .0325 * t13 - 1.04e-3 * t23 - 2.37e-4 * t - 8.11e-5 * t43
                                                  - 4.69e-5 * t53) + 5.938e6 * a1 ** (5 / 6) / t32 * ex(-12.859 / a1 ** (1 / 3)),
             3.032e5 / t23 * ex(-8.090 / t13) * (1 + .0516 * t13 + .0229 * t23 + 8.28e-3 * t - 3.28e-4 * t43
                                                 - 3.01e-4 * t53) + 5.109e5 * a2 ** (5 / 6) / t32 * ex(-8.068 / a2 ** (1 / 3)),
             2.675e9 * (1 - .560 * t12 + .179 * t - .0283 * t32 + 2.214e-3 * t ** 2 - 6.851e-5 * t ** 2.5)
             + 9.391e8 * a3 ** 1.5 / t32 + 4.467e7 / t32 * ex(-.07486 / t),
             1.096e9 / t23 * ex(-8.472 / t13) - 4.830e8 * a4 ** (5 / 6) / t32 * ex(-8.472 / a4 ** (1 / 3))
             + 1.06e10 / t32 * ex(-30.442 / t) + 1.56e5 / t23 * ex(-8.472 / t13 - (t / 1.696) ** 2)
             * (1 + .049 * t13 - 2.498 * t23 + .860 * t + 3.518 * t43 + 3.08 * t53) + 1.55e6 / t32 * ex(-4.478 / t)]
        return [v * 1e-6 / N_A for v in r]

    def saha(g, mu, Q, T):                          # photodisintegration factor, 1/m³
        return g * (mu * m_u * c ** 2 * T / (2 * pi * hc ** 2)) ** 1.5 * np.exp(-Q * MeV / T)

    def back(g, mr, Q, T):                          # <σv>_reverse / <σv>
        return g * mr ** 1.5 * np.exp(-Q * MeV / T)

    def dTdt(T):
        re, pe, dre = epm(T)
        return -3 * H(T) * (4 / 3 * rho_g(T) + re + pe) / (4 * rho_g(T) / T + dre)

    def rhs(t, s):
        T = s[0] * MeV
        n, p, d, h3, he3, he4, li, be = s[1:]
        nb = n_b(T)
        r = rates(T)
        F1 = weak(1, T) * n - weak(-1, T) * p
        F2 = r[0] * (nb * n * p - saha(4 / 3, 1 / 2, 2.224573, T) * d)
        F3 = r[1] * (nb * d * p - saha(3, 2 / 3, 5.493485, T) * he3)
        F4 = nb * r[2] * (d * d / 2 - back(9 / 4, 4 / 3, 3.268914, T) / 2 * n * he3)
        F5 = nb * r[3] * (d * d / 2 - back(9 / 4, 4 / 3, 4.032669, T) / 2 * p * h3)
        F6 = nb * r[4] * (he3 * n - back(1, 1, 0.763763, T) * p * h3)
        F7 = nb * r[5] * (h3 * d - back(3, 3 / 2, 17.589293, T) * n * he4)
        F8 = nb * r[6] * (he3 * d - back(3, 3 / 2, 18.353053, T) * p * he4)
        F9 = r[7] * (nb * he3 * he4 - saha(1 / 2, 12 / 7, 1.586627, T) * be)
        F10 = r[8] * (nb * h3 * he4 - saha(1 / 2, 12 / 7, 2.467032, T) * li)
        F11 = nb * r[9] * (be * n - back(1, 1, 1.644243, T) * li * p)
        F12 = nb * r[10] * (li * p - 2 * back(8, 7 / 16, 17.346244, T) * he4 ** 2 / 2)
        return [dTdt(T) / MeV, -F1 - F2 + F4 - F6 + F7 - F11, F1 - F2 - F3 + F5 + F6 + F8 + F11 - F12,
                F2 - F3 - 2 * F4 - 2 * F5 - F7 - F8, F5 + F6 - F7 - F10, F3 + F4 - F6 - F8 - F9,
                F7 + F8 - F9 - F10 + 2 * F12, F10 + F11 - F12, F9 - F11]

    def weak_rhs(t, s):                             # the weak era: T and Y_n only
        T = s[0] * MeV
        return [dTdt(T) / MeV, -(weak(1, T) * s[1] - weak(-1, T) * (1 - s[1]))]

    return dict(rhs=rhs, weak_rhs=weak_rhs, H=H, weak=weak, eta=eta, MeV=MeV, lam0=lam0)


def test_bbn_network_matches_scipy_radau():
    """BBN: Fermium's two-stage radau solve vs SciPy's Radau on the whole network from 10 MeV in one go."""
    from scipy.integrate import solve_ivp
    from scipy.optimize import brentq
    out = run_prog("bbn_network", "bbn.fm")
    M = _bbn_model()
    MeV, Q = M["MeV"], 1.29333
    assert num(out, "η₁₀ =") == pytest.approx(M["eta"] * 1e10, rel=1e-3)
    T0 = 10.0
    t0 = 1 / (2 * M["H"](T0 * MeV))
    yn0 = 1 / (1 + np.exp(Q / T0))
    assert num(line_of(out, "start:"), "t =") == pytest.approx(t0, rel=1e-3)
    # the weak era (T and Y_n only), with the times T crosses 3, 1, 0.7, 0.5, 0.3 and 0.1 MeV
    ev = [lambda t, s, Tq=Tq: s[0] - Tq for Tq in (3, 1, 0.7, 0.5, 0.3, 0.1)]
    w = solve_ivp(M["weak_rhs"], [t0, 1000], [T0, yn0], method="Radau", rtol=1e-11, atol=1e-14,
                  events=ev, dense_output=True)
    t1, yn1 = w.t_events[5][0], w.y_events[5][0][1]
    line = line_of(out, "weak era:")
    assert num(line, "at t =") == pytest.approx(t1, rel=1e-5)
    assert num(line, "n/p =") == pytest.approx(yn1 / (1 - yn1), rel=1e-5)
    rows = [ln for ln in out.splitlines() if re.match(r"T = [\d.]+ MeV :", ln)]
    for i, (Tq, row) in enumerate(zip((3, 1, 0.7, 0.5, 0.3), rows, strict=True)):
        tq = w.t_events[i][0]
        yq = w.sol(tq)[1]
        assert num(row, "t =") == pytest.approx(tq, rel=1e-3), Tq
        assert num(row, "n/p =") == pytest.approx(yq / (1 - yq), rel=1e-4), Tq
        assert num(row, "exp(−Q/T) =") == pytest.approx(np.exp(-Q / Tq), rel=1e-4)
        assert num(row, "λ(n→p)/H =") == pytest.approx(M["weak"](1, Tq * MeV) / M["H"](Tq * MeV), rel=1e-3)
    Tf = brentq(lambda T: M["weak"](1, T * MeV) - M["H"](T * MeV), 0.3, 3, xtol=1e-12)
    assert num(out, "λ(n→p) = H: T =") == pytest.approx(Tf, rel=1e-3)
    # the whole network from 10 MeV in one solve (an absolute tolerance of 10⁻¹⁶ lets SciPy start there)
    y0 = [T0, yn0, 1 - yn0, 0, 0, 0, 0, 0, 0]
    sol = solve_ivp(M["rhs"], [t0, 1e4], y0, method="Radau", rtol=1e-10, atol=1e-16, first_step=1e-12,
                    dense_output=True)
    assert sol.success
    T, n, p, d, h3, he3, he4, li, be = sol.y[:, -1]
    assert num(line_of(out, "Y_p ="), "Y_p =") == pytest.approx(4 * he4, rel=1e-4)
    assert num(line_of(out, "D/H ="), "D/H =") == pytest.approx(d / p, rel=1e-4)
    assert num(line_of(out, "3He/H ="), "3He/H =") == pytest.approx((he3 + h3) / p, rel=1e-4)
    assert num(line_of(out, "7Li/H ="), "7Li/H =") == pytest.approx((li + be) / p, rel=1e-4)
    assert num(out, "Y_n =") == pytest.approx(n, rel=2e-2)
    end = line_of(out, "network:")
    assert num(end, "T =") == pytest.approx(T, rel=1e-3)
    assert num(end, "T/T_ν =") == pytest.approx((11 / 4) ** (1 / 3), rel=1e-5)      # e± entropy went to the photons
    assert abs(num(out, "Σ A Y − 1 =")) < 1e-10                                     # baryon number conserved
    # the deuterium bottleneck: ⁴He half made; the D/H peak (read at the solver's steps, so to a few %)
    th = brentq(lambda t: sol.sol(t)[6] - he4 / 2, 150, 400, xtol=1e-10)
    line = line_of(out, "half of the ⁴He")
    assert num(line, "by t =") == pytest.approx(th, rel=1e-3)
    assert num(line, "T =") == pytest.approx(sol.sol(th)[0], rel=1e-3)
    tt = np.linspace(150, 400, 5001)
    dh = sol.sol(tt)[3] / sol.sol(tt)[2]
    line = line_of(out, "deuterium peaks:")
    assert num(line, "t =") == pytest.approx(tt[np.argmax(dh)], rel=3e-2)
    assert num(line, "D/H =") == pytest.approx(dh.max(), rel=3e-2)
    # physics sanity against the published standard-BBN values (Fields et al. 2020, PDG 2024)
    assert 0.24 < 4 * he4 < 0.26
    assert 2.5e-5 / 2 < d / p < 2.5e-5 * 2
    assert 0.5e-5 < (he3 + h3) / p < 2e-5
    assert 2.5e-10 < (li + be) / p < 1e-9
    assert 0.6 < Tf < 0.8                                                            # freeze-out near 0.7 MeV
    # README: Born rates normalised with the Coulomb-corrected λ₀ = 1.6887 (Dicus et al. 1982) instead of
    # λ₀ = 1.636 are 3 % slower, and freeze out earlier: Y_p goes up by 0.0057 (to 0.2481)
    slow = _bbn_model(weak_scale=M["lam0"] / 1.6887)
    s2 = solve_ivp(slow["rhs"], [t0, 1e4], y0, method="Radau", rtol=1e-8, atol=1e-16, first_step=1e-12)
    assert 4 * s2.y[6, -1] - 4 * he4 == pytest.approx(0.0057, abs=3e-4)


BBN_FULL_NETWORK = """
T0 = 10 MeV
t0 = 1 / (2 H(T0))
Yn0 = 1 / (1 + exp(Q_np / T0))
solve
    T' = dTdt(T)
    Yn' = -F_np(T, Yn, Yp) - F_pn(T, Yn, Yp, Yd) + F_ddn(T, Yd, Yn, Y3He) - F_h3n(T, Y3He, Yn, Yp, Y3H) + F_td(T, Y3H, Yd, Yn, Y4He) - F_b7n(T, Y7Be, Yn, Y7Li, Yp)
    Yp' = F_np(T, Yn, Yp) - F_pn(T, Yn, Yp, Yd) - F_dp(T, Yd, Yp, Y3He) + F_ddp(T, Yd, Yp, Y3H) + F_h3n(T, Y3He, Yn, Yp, Y3H) + F_h3d(T, Y3He, Yd, Yp, Y4He) + F_b7n(T, Y7Be, Yn, Y7Li, Yp) - F_l7p(T, Y7Li, Yp, Y4He)
    Yd' = F_pn(T, Yn, Yp, Yd) - F_dp(T, Yd, Yp, Y3He) - 2 F_ddn(T, Yd, Yn, Y3He) - 2 F_ddp(T, Yd, Yp, Y3H) - F_td(T, Y3H, Yd, Yn, Y4He) - F_h3d(T, Y3He, Yd, Yp, Y4He)
    Y3H' = F_ddp(T, Yd, Yp, Y3H) + F_h3n(T, Y3He, Yn, Yp, Y3H) - F_td(T, Y3H, Yd, Yn, Y4He) - F_ta(T, Y3H, Y4He, Y7Li)
    Y3He' = F_dp(T, Yd, Yp, Y3He) + F_ddn(T, Yd, Yn, Y3He) - F_h3n(T, Y3He, Yn, Yp, Y3H) - F_h3d(T, Y3He, Yd, Yp, Y4He) - F_h3a(T, Y3He, Y4He, Y7Be)
    Y4He' = F_td(T, Y3H, Yd, Yn, Y4He) + F_h3d(T, Y3He, Yd, Yp, Y4He) - F_h3a(T, Y3He, Y4He, Y7Be) - F_ta(T, Y3H, Y4He, Y7Li) + 2 F_l7p(T, Y7Li, Yp, Y4He)
    Y7Li' = F_ta(T, Y3H, Y4He, Y7Li) + F_b7n(T, Y7Be, Yn, Y7Li, Yp) - F_l7p(T, Y7Li, Yp, Y4He)
    Y7Be' = F_h3a(T, Y3He, Y4He, Y7Be) - F_b7n(T, Y7Be, Yn, Y7Li, Yp)
    with T(t0) = T0, Yn(t0) = Yn0, Yp(t0) = 1 - Yn0, Yd(t0) = 0, Y3H(t0) = 0, Y3He(t0) = 0, Y4He(t0) = 0, Y7Li(t0) = 0, Y7Be(t0) = 0
    for t from t0 to 1e4 s using radau {opts}
print "Y_p =", 4 Y4He[end] to 6 digits
print "D/H =", Yd[end] / Yp[end] to 6 digits
print "3He/H =", (Y3He[end] + Y3H[end]) / Yp[end] to 6 digits
print "7Li/H =", (Y7Li[end] + Y7Be[end]) / Yp[end] to 6 digits
print "Y_n =", Yn[end] to 4 digits
print "T =", T[end] in MeV to 6 digits
"""


def test_bbn_whole_network_from_10_mev_with_an_absolute_tolerance():
    """FRICTION #82 (D160): with  absolute 1e-16  the whole network starts at 10 MeV in one radau solve (the
    published program needs two stages), and agrees with SciPy's Radau at the same rtol and atol."""
    from scipy.integrate import solve_ivp
    d = os.path.join(RES, "bbn_network")
    head = open(os.path.join(d, "bbn.fm"), encoding="utf-8").read().split("# ---- 1. the weak era")[0]
    # purely relative control (D42) can't start it; the message now points at the tolerance
    with pytest.raises(FermiumError) as e:
        run(head + BBN_FULL_NETWORK.format(opts="tolerance 1e-10"), base_dir=d)
    assert "step became too small near t = 0.00738 s" in e.value.message
    assert "probably not a blow-up" in e.value.message and "absolute" in e.value.message
    out = run(head + BBN_FULL_NETWORK.format(opts="tolerance 1e-10 absolute 1e-16, 1e-16 MeV"), base_dir=d)
    M = _bbn_model()
    T0, Q = 10.0, 1.29333
    t0 = 1 / (2 * M["H"](T0 * M["MeV"]))
    yn0 = 1 / (1 + np.exp(Q / T0))
    sol = solve_ivp(M["rhs"], [t0, 1e4], [T0, yn0, 1 - yn0, 0, 0, 0, 0, 0, 0], method="Radau", rtol=1e-10,
                    atol=1e-16, first_step=1e-12)
    assert sol.success
    T, n, p, d_, h3, he3, he4, li, be = sol.y[:, -1]
    assert num(out, "Y_p =") == pytest.approx(4 * he4, rel=1e-5)
    assert num(out, "D/H =") == pytest.approx(d_ / p, rel=1e-5)
    assert num(out, "3He/H =") == pytest.approx((he3 + h3) / p, rel=1e-5)
    assert num(out, "7Li/H =") == pytest.approx((li + be) / p, rel=1e-5)
    assert num(out, "Y_n =") == pytest.approx(n, rel=1e-3)
    assert num(out, "T =") == pytest.approx(T, rel=1e-5)
