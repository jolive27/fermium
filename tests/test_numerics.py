"""Tier 2 validation: Fermium's derivatives, integrals and ODE solutions against SymPy/SciPy.

Tolerances are stated per test.  Inputs are randomized with a fixed seed so failures reproduce.
"""
import math

import numpy as np
import pytest
import sympy as sp
from scipy import constants as C
from scipy.integrate import quad, solve_ivp

from conftest import run
from numparse import nums

RNG = np.random.default_rng(20260925)

# (fermium formula in x, sympy formula in x, domain for sample points)
FUNCS = [
    ("x^3 - 2x + 1", "x**3 - 2*x + 1", (-3, 3)),
    ("sin(x) cos(x)", "sin(x)*cos(x)", (-3, 3)),
    ("exp(-x^2)", "exp(-x**2)", (-2, 2)),
    ("ln(1 + x^2)", "log(1 + x**2)", (-3, 3)),
    ("√(1 + x^2)", "sqrt(1 + x**2)", (-3, 3)),
    ("x / (1 + x^2)", "x/(1 + x**2)", (-3, 3)),
    ("tan(x/3)", "tan(x/3)", (-3, 3)),
    ("atan(x) + asin(x/4)", "atan(x) + asin(x/4)", (-3, 3)),
    ("sinh(x) - cosh(x/2)", "sinh(x) - cosh(x/2)", (-2, 2)),
    ("tanh(2x)", "tanh(2*x)", (-2, 2)),
    ("x^2 exp(-x) sin(3x)", "x**2*exp(-x)*sin(3*x)", (0, 4)),
    ("(x + 2)^(1/3)", "(x + 2)**(sp.Rational(1, 3))", (0, 5)),
    ("1 / (x^2 + 1)^(3/2)", "1/(x**2 + 1)**(sp.Rational(3, 2))", (-3, 3)),
    ("x^x", "x**x", (0.5, 3)),
    ("cos(x)^2 + 2^x", "cos(x)**2 + 2**x", (-2, 2)),
    ("log10(x) + log2(x)", "log(x, 10) + log(x, 2)", (0.5, 5)),
    ("exp(sin(x))", "exp(sin(x))", (-3, 3)),
    ("acos(x/5) atan(x)", "acos(x/5)*atan(x)", (-3, 3)),
    ("|x| x", "x*x", (0.1, 3)),   # |x| = x on this domain
    ("cbrt(x^2 + 1)", "(x**2 + 1)**(sp.Rational(1, 3))", (-3, 3)),
]


def sympy_expr(text):
    x = sp.Symbol("x", real=True)
    return x, eval(text, {"sp": sp, "x": x, **{k: getattr(sp, k) for k in
                                                ("sin", "cos", "tan", "exp", "log", "sqrt", "atan", "asin",
                                                 "acos", "sinh", "cosh", "tanh", "Abs")}})


@pytest.mark.parametrize("fm,sy,dom", FUNCS, ids=[f[0] for f in FUNCS])
def test_derivatives_match_sympy(fm, sy, dom):
    """First and second symbolic derivatives agree with SymPy to 1e-10 relative (5 random points)."""
    pts = [float(v) for v in RNG.uniform(*dom, size=5)]
    lines = [f"f(x) = {fm}", "d1 = f'", "d2 = f''"]
    for p in pts:
        lines.append(f"print d1({p!r}) to 17 digits")
        lines.append(f"print d2({p!r}) to 17 digits")
    got = nums(run("\n".join(lines)))
    x, e = sympy_expr(sy)
    d1, d2 = sp.lambdify(x, sp.diff(e, x)), sp.lambdify(x, sp.diff(e, x, 2))
    for i, p in enumerate(pts):
        for g, ref in ((got[2 * i], d1(p)), (got[2 * i + 1], d2(p))):
            assert g == pytest.approx(float(ref), rel=1e-10, abs=1e-12), (fm, p)


def test_derivative_units_and_formula():
    out = run("A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nprint d/dt x\nprint x''")
    assert out.split("\n") == ["x'(t) = -A ω sin(ω t)   [m/s, for t in s]",
                               "x''(t) = -A ω² cos(ω t)   [m/s², for t in s]"]


def test_partial_derivative():
    out = run("f(x, y) = x² y + sin(x y)\ng = ∂/∂y f\nprint g(1.3, 0.7) to 15 digits")
    x, y = sp.symbols("x y")
    ref = sp.diff(x**2 * y + sp.sin(x * y), y).subs({x: 1.3, y: 0.7})
    assert nums(out)[0] == pytest.approx(float(ref), rel=1e-12)


INTEGRALS = [
    ("exp(-x^2)", "-inf", "inf", lambda x: math.exp(-x * x), -np.inf, np.inf),
    ("x^2 sin(x)", "0", "π", lambda x: x * x * math.sin(x), 0, math.pi),
    ("1/(1 + x^2)", "0", "inf", lambda x: 1 / (1 + x * x), 0, np.inf),
    ("√x", "0", "1", math.sqrt, 0, 1),
    ("ln(x)", "0.001", "2", math.log, 0.001, 2),
    ("x^3/(exp(x) - 1)", "0.0001", "inf", lambda x: x**3 / math.expm1(x) if x < 700 else 0.0, 0.0001, np.inf),
    ("exp(x)", "-inf", "0", math.exp, -np.inf, 0),
    ("cos(50 x)", "0", "1", lambda x: math.cos(50 * x), 0, 1),
    ("1/√(1 - x^2)", "-0.999", "0.999", lambda x: 1 / math.sqrt(1 - x * x), -0.999, 0.999),
    ("x exp(-x) sin(x)", "3", "0", lambda x: x * math.exp(-x) * math.sin(x), 3, 0),
]


@pytest.mark.parametrize("fm,lo,hi,f,a,b", INTEGRALS, ids=[i[0] for i in INTEGRALS])
def test_integrals_match_scipy(fm, lo, hi, f, a, b):
    """Adaptive Gauss–Kronrod agrees with scipy.integrate.quad to 1e-8 relative."""
    got = nums(run(f"print ∫ {fm} dx from {lo} to {hi} to 17 digits"))[0]
    ref, _ = quad(f, a, b, epsabs=0, epsrel=1e-12, limit=500)
    assert got == pytest.approx(ref, rel=1e-8, abs=1e-12)


@pytest.mark.parametrize("seed", range(5))
def test_random_polynomial_integrals(seed):
    """Random polynomials with units: exact antiderivative check to 1e-10."""
    r = np.random.default_rng(seed)
    c = r.uniform(-3, 3, size=4)
    a, b = sorted(float(v) for v in r.uniform(-2, 2, size=2))
    c = [float(v) for v in c]
    src = (f"p(x) = {c[0]!r} N + {c[1]!r} N/m x + {c[2]!r} N/m² x^2 + {c[3]!r} N/m³ x^3\n"
           f"W = ∫ p(x) dx from {a!r} m to {b!r} m\nprint W / (1 J) to 17 digits")
    got = nums(run(src))[0]
    F = lambda x: c[0] * x + c[1] * x**2 / 2 + c[2] * x**3 / 3 + c[3] * x**4 / 4  # noqa: E731
    assert got == pytest.approx(F(b) - F(a), rel=1e-10, abs=1e-12)


def test_integral_units():
    assert run("k = 50 N/m\nF(x) = k x\nprint ∫ F(x) dx from 0 m to 0.2 m") == "1.0 J"   # 0.2 has 1 s.f.


def test_indefinite_integral_sympy():
    out = run("F = ∫ x^2 dx\nprint F(3) to 12 digits")
    assert nums(out)[0] == pytest.approx(9.0)


def test_ode_exponential_decay_rk45():
    out = run("τ = 2 s\nsolve N' = -N / τ with N(0) = 1000 for t from 0 s to 10 s\n"
              "print N(10 s) to 15 digits\nprint N(3.3 s) to 15 digits")
    a, b = nums(out)
    assert a == pytest.approx(1000 * math.exp(-5), rel=1e-7)
    assert b == pytest.approx(1000 * math.exp(-1.65), rel=1e-6)   # Hermite interpolation between steps


@pytest.mark.parametrize("method", ["", "step 1 ms"])
def test_damped_oscillator_vs_solve_ivp(method):
    """Both RK45 and fixed-step RK4 agree with SciPy (rtol 1e-12) to 1e-6 relative of the amplitude."""
    src = ("m = 0.5 kg\nk = 50 [N/m]\nb = 0.2 kg/s\n"
           "solve m x'' = -k x - b x'\n  with x(0) = 0.1 [m], x'(0) = 0 m/s\n"
           f"  for t from 0 s to 5 s {method}\n"
           "print x(5 s) / (1 [m]) to 15 digits\nprint x'(2.5 s) / (1 m/s) to 15 digits")
    got = nums(run(src))
    sol = solve_ivp(lambda t, y: [y[1], (-50 * y[0] - 0.2 * y[1]) / 0.5], (0, 5), [0.1, 0], rtol=1e-12,
                    atol=1e-14, dense_output=True)
    assert got[0] == pytest.approx(sol.sol(5.0)[0], abs=1e-7)
    assert got[1] == pytest.approx(sol.sol(2.5)[1], abs=1e-6)


def test_bateman_chain_vs_analytic():
    """Two-step decay chain A -> B -> C; compare with the Bateman solution."""
    src = ("λA = 0.3 / (1 s)\nλB = 0.1 / (1 s)\n"
           "solve A' = -λA A, B' = λA A - λB B\n  with A(0) = 1000, B(0) = 0\n  for t from 0 s to 20 s\n"
           "print B(20 s) to 15 digits")
    got = nums(run(src))[0]
    la, lb = 0.3, 0.1
    ref = 1000 * la / (lb - la) * (math.exp(-la * 20) - math.exp(-lb * 20))
    assert got == pytest.approx(ref, rel=1e-7)


def test_kepler_orbit_conserves_energy():
    """Circular orbit at 1 AU: after one year the Earth is back where it started (to 1e-6 AU)."""
    src = ("GM = G M_sun\nr0 = 1 AU\nv0 = √(GM / r0)\n"
           "solve x'' = -GM x / (x² + y²)^(3/2), y'' = -GM y / (x² + y²)^(3/2)\n"
           "  with x(0) = r0, y(0) = 0 m, x'(0) = 0 m/s, y'(0) = v0\n"
           "  for t from 0 s to 2π r0 / v0\n"
           "print x(2π r0 / v0) / (1 AU) to 15 digits\nprint y(2π r0 / v0) / (1 AU) to 15 digits")
    x, y = nums(run(src))
    assert x == pytest.approx(1.0, abs=1e-6)
    assert y == pytest.approx(0.0, abs=1e-6)


def test_fit_matches_curve_fit(tmp_path):
    from scipy.optimize import curve_fit
    r = np.random.default_rng(3)
    t = np.linspace(0, 10, 25)
    y = 5.0 * np.exp(-t / 3.0) + r.normal(0, 0.05, t.size)
    rows = "\n".join(f"{a:.6f}, {b:.6f}" for a, b in zip(t, y))
    (tmp_path / "d.csv").write_text("t [s], A [Bq]\n" + rows + "\n")
    out = run('data = load "d.csv"\nfit A = A0 exp(-t / τ) to data\nprint A0 / (1 Bq) to 12 digits\n'
              'print τ / (1 s) to 12 digits', base_dir=str(tmp_path))
    lines = out.split("\n")
    A0, tau = nums("\n".join(lines[-2:]))
    popt, _ = curve_fit(lambda t, a, b: a * np.exp(-t / b), t, y, p0=[1, 1])
    assert A0 == pytest.approx(popt[0], rel=1e-6)
    assert tau == pytest.approx(popt[1], rel=1e-6)
    assert "τ = " in out and " s" in lines[2]


CONSTS = [("c", C.c), ("h", C.h), ("ħ", C.hbar), ("e", C.e), ("k_B", C.k), ("N_A", C.N_A), ("G", C.G),
          ("m_e", C.m_e), ("m_p", C.m_p), ("m_n", C.m_n), ("ε_0", C.epsilon_0), ("μ_0", C.mu_0),
          ("σ", C.sigma), ("α", C.alpha), ("m_u", C.physical_constants["atomic mass constant"][0]),
          ("a_0", C.physical_constants["Bohr radius"][0]), ("R_∞", C.Rydberg)]


@pytest.mark.parametrize("name,ref", CONSTS, ids=[c[0] for c in CONSTS])
def test_constants_match_scipy_codata(name, ref):
    got = nums(run(f"print {name} to 12 digits"))[0]
    assert got == pytest.approx(ref, rel=1e-9)


def test_hbar_c_in_MeV_fm():
    assert nums(run("print ħ c in MeV fm to 7 digits"))[0] == pytest.approx(197.3269804, rel=1e-7)


@pytest.mark.parametrize("fm,sy,dom", FUNCS[:12], ids=[f[0] for f in FUNCS[:12]])
def test_printed_derivative_is_valid_fermium(fm, sy, dom):
    """The formula Fermium prints for a derivative is itself a valid program with the same value."""
    out = run(f"f(x) = {fm}\nprint f'")
    formula = out.split(" = ", 1)[1].split("   [")[0]
    p = float(RNG.uniform(*dom))
    a = nums(run(f"f(x) = {fm}\nd = f'\nprint d({p!r}) to 15 digits"))[0]
    b = nums(run(f"g(x) = {formula}\nprint g({p!r}) to 15 digits"))[0]
    assert b == pytest.approx(a, rel=1e-12, abs=1e-14)
