"""Uncertainties (moonshot M4, DECISIONS D120-D124): `5.0 ± 0.2 m`, linear propagation with correlations,
`propagate montecarlo`, fitted parameters with their covariance, error bars.

Numbers are compared with tolerances (or printed with `to N digits`), so the tests don't depend on the default
number of significant figures.  Uncertain values print by their own rule (σ to 2 significant figures)."""
import io
import math
import os
import re

import numpy as np
import pytest

from conftest import run, error_of
from numparse import num
from fermium.driver import Program, ReplSession
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
from fermium.uncertain import UFloat, format_pm, correlation, correlated

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def parts(src):
    """Run a program whose last line prints `value(x) to 12 digits, uncertainty(x) to 12 digits`."""
    return [num(t) for t in run(src).split("\n")[-1].split(",")]


def evaluate(src):
    """Run a program in the interpreter and return its variables (SI values; UFloat for uncertain ones)."""
    from fermium.checker import Checker
    from fermium.errors import Diagnostics
    from fermium.interp import Interpreter
    from fermium.parser import parse
    from fermium.runtime.core import Runtime
    from fermium.tables import finalize_tables
    d = Diagnostics()
    ck = Checker(d, ".")
    mod = ck.check_program(parse(src, d))
    finalize_tables(mod.tables, ck.U)
    rt = Runtime(io.StringIO(), ".")
    rt.tables = mod.tables
    it = Interpreter(mod, rt)
    it.run()
    return {s.name: it.globals.vars[s.id] for s in mod.main.locals if s.id in it.globals.vars}


def vu(expr, setup=""):
    """(value, σ) of an expression, in SI units."""
    r = evaluate(f"{setup}\nr = {expr}\n")["r"]
    return (r.v, r.s) if isinstance(r, UFloat) else (r, 0.0)


def close(a, b, rel=1e-9, abs_=1e-15):
    return abs(a - b) <= rel * max(abs(a), abs(b)) + abs_


# ---------------------------------------------------------------- literals and printing
@pytest.mark.parametrize("src,out", [
    ("print 5.0 ± 0.2 m", "5.00 ± 0.20 m"),
    ("print (5.0 ± 0.2) m", "5.00 ± 0.20 m"),
    ("print 5.0 +- 0.2 m", "5.00 ± 0.20 m"),
    ("print 1.234e-3 ± 0.056e-3 m", "(1.234 ± 0.056)×10⁻³ m"),
    ("print 9.8061 ± 0.0172", "9.806 ± 0.017"),
    ("print 123456 ± 1234", "(1.235 ± 0.012)×10⁵"),
    ("print 0.01234 ± 0.00056 m", "0.01234 ± 0.00056 m"),
    ("print 5.0 ± 3%", "5.00 ± 0.15"),
    ("print 2.0 m ± 5 %", "2.00 ± 0.10 m"),
    ("print (1.20 ± 0.01 m) in cm", "120.0 ± 1.0 cm"),
    ("x = 1.20 ± 0.01 m\nprint x in mm", "1200 ± 10 mm"),
    ("print 20.0 ± 0.5 °C", "20.00 ± 0.50 °C"),
    ("print [1.0, 2.0] ± 0.1", "[1.00 ± 0.10, 2.00 ± 0.10]"),
    ("print [1.0 s, 2.0 s] ± [0.1 s, 0.25 s]", "[1.00 ± 0.10, 2.00 ± 0.25] s"),
    ("print 47 ± 12 J", "47 ± 12 J"),
])
def test_literals_and_printing(src, out):
    assert run(src) == out


@pytest.mark.parametrize("x,s,text", [
    (5.0, 0.2, "5.00 ± 0.20"), (9.96, 0.05, "9.960 ± 0.050"), (0.0, 0.3, "0.00 ± 0.30"),
    (-0.5, 0.1, "-0.50 ± 0.10"), (1.0, 0.0996, "1.00 ± 0.10"), (6.674e-11, 1.5e-13, "(6.674 ± 0.015)×10⁻¹¹"),
    (2.0, 0.0, "2 ± 0"), (1234.5, 150.0, "(1.23 ± 0.15)×10³"),
])
def test_format_rule(x, s, text):
    assert format_pm(x, s)[0] == text


def test_unit_is_given_to_both_numbers():
    v, s = vu("2.0 ± 0.3 cm")
    assert close(v, 0.02) and close(s, 0.003)


def test_temperature_uncertainty_is_a_difference():
    v, s = vu("20.0 ± 0.5 °C")
    assert close(v, 293.15) and close(s, 0.5)


def test_parts_and_relative():
    out = run("x = 4.0 ± 0.2 m\nprint value(x) to 2 digits, uncertainty(x), rel(x) to 3 digits, err(x)")
    assert out == "4.0 m 0.20 m 0.0500 0.20 m"


# ---------------------------------------------------------------- linear propagation
def test_against_hand_formulas():
    # g = 4π² L / T²:  (σg/g)² = (σL/L)² + (2 σT/T)²
    L, sL, T, sT = 1.20, 0.01, 2.21, 0.02
    v, s = vu("4π² L / T²", "L = 1.20 ± 0.01 m\nT = 2.21 ± 0.02 s")
    g = 4 * math.pi ** 2 * L / T ** 2
    assert close(v, g)
    assert close(s, g * math.hypot(sL / L, 2 * sT / T))
    # sin: σ = |cos x| σx; ln: σ/x; power 1.5: 1.5 x^0.5 σ
    assert close(vu("sin(0.7 ± 0.02)")[1], math.cos(0.7) * 0.02)
    assert close(vu("ln(3.0 ± 0.1)")[1], 0.1 / 3.0)
    assert close(vu("(2.0 ± 0.1)^1.5")[1], 1.5 * math.sqrt(2.0) * 0.1)


EXPRS = [
    ("x y + z / x", lambda x, y, z: x * y + z / x),
    ("sin(x) exp(-y) + √z", None),
    ("x^y", None),
    ("atan2(y, x) + hypot(x, z)", None),
    ("ln(x) - log10(z) + tanh(y)", None),
    ("(x - y)² / (x + z)", None),
    ("2^x + asin(y / 3)", None),
    ("cosh(y) / (1 + x²) - erf(y)", None),
]


@pytest.mark.parametrize("expr,_", EXPRS)
def test_against_uncertainties_package(expr, _):
    unc = pytest.importorskip("uncertainties")
    from uncertainties import umath
    x, y, z = unc.ufloat(1.3, 0.05), unc.ufloat(0.8, 0.03), unc.ufloat(2.5, 0.2)
    env = {"x": x, "y": y, "z": z, "sin": umath.sin, "exp": umath.exp, "atan2": umath.atan2,
           "hypot": umath.hypot, "ln": umath.log, "log10": umath.log10, "tanh": umath.tanh, "asin": umath.asin,
           "cosh": umath.cosh, "erf": umath.erf, "sqrt": umath.sqrt}
    py = expr.replace("^", "**").replace("√z", "sqrt(z)").replace("²", "**2").replace("x y", "x*y") \
        .replace("sin(x) exp", "sin(x)*exp").replace(") + ", ") + ").replace("cosh(y) / (1 + x**2)", "cosh(y)/(1+x**2)")
    ref = eval(py, env)
    v, s = vu(expr, "x = 1.3 ± 0.05\ny = 0.8 ± 0.03\nz = 2.5 ± 0.2")
    assert close(v, ref.nominal_value, 1e-9)
    assert close(s, ref.std_dev, 1e-7)


def test_user_functions_and_derivatives_propagate():
    unc = pytest.importorskip("uncertainties")
    x = unc.ufloat(2.0, 0.1)
    v, s = vu("f(2.0 ± 0.1)", "f(a) = a³ - 2 a")
    ref = x ** 3 - 2 * x
    assert close(v, ref.nominal_value) and close(s, ref.std_dev)
    v, s = vu("f'(2.0 ± 0.1)", "f(a) = a³ - 2 a")          # f' = 3a² - 2: σ = 6a σ
    assert close(v, 10.0) and close(s, 6 * 2.0 * 0.1)
    v, s = vu("Σ(k x for k from 1 to 4)", "x = 1.0 ± 0.1")    # 10 x
    assert close(v, 10.0) and close(s, 1.0)


# ---------------------------------------------------------------- correlations
def test_x_minus_x_is_exactly_zero():
    assert run("x = 5.0 ± 0.2 m\nprint x - x") == "0 ± 0 m"
    assert run("x = 5.0 ± 0.2 m\nprint x / x") == "1 ± 0"
    assert run("x = 5.0 ± 0.2 m\ny = 2 x\nprint y - x - x") == "0 ± 0 m"


def test_square_versus_product_of_independent_values():
    _, s_sq = vu("x x", "x = 3.0 ± 0.1")
    _, s_xy = vu("x y", "x = 3.0 ± 0.1\ny = 3.0 ± 0.1")
    assert close(s_sq, 2 * 3.0 * 0.1)                      # fully correlated
    assert close(s_xy, math.sqrt(2) * 3.0 * 0.1)           # independent


def test_each_literal_is_a_new_source():
    # the same number written twice is two measurements
    _, s = vu("(2.0 ± 0.1) - (2.0 ± 0.1)")
    assert close(s, math.sqrt(2) * 0.1)
    # a second ± adds an independent uncertainty (statistical ± systematic)
    _, s = vu("(2.0 ± 0.3) ± 0.4")
    assert close(s, 0.5)


def test_correlation_through_functions():
    unc = pytest.importorskip("uncertainties")
    a = unc.ufloat(1.1, 0.07)
    ref = unc.umath.sin(a) / unc.umath.cos(a) - unc.umath.tan(a)
    v, s = vu("sin(a) / cos(a) - tan(a)", "a = 1.1 ± 0.07")
    assert abs(v - ref.nominal_value) < 1e-12 and s < 1e-12


def test_lists_are_independent_measurements():
    _, s = vu("sum(xs)", "xs = [1.0, 2.0, 3.0, 4.0] ± 0.1")
    assert close(s, 0.2)
    _, s = vu("mean(xs)", "xs = [1.0, 2.0, 3.0, 4.0] ± 0.1")
    assert close(s, 0.05)


def test_comparisons_use_the_value():
    assert run("x = 2.0 ± 5.0\nif x > 1 then print \"big\"\nprint min(x, 3)") == "big\n2.0 ± 5.0"


# ---------------------------------------------------------------- units stay checked
@pytest.mark.parametrize("src,msg", [
    ("x = 1.0 m ± 0.1 s", "the uncertainty after ± is time [s] but the value is length [m]"),
    ("x = 1.0 m ± 0.1", "both need the same units"),
    ("x = 1.0 ± 0.1 m\ny = 2.0 ± 0.1 s\nprint x + y", "can't add"),
    ("print sin(1.0 ± 0.1 m)", "sin needs a plain number"),
    ("print 1.0 m ± 5 cm ± 3 kg", "±"),
])
def test_unit_errors(src, msg):
    e = error_of(src)
    assert msg in e.message, e.message


def test_units_of_propagated_results():
    assert run("L = 1.20 ± 0.01 m\nT = 2.21 ± 0.02 s\nprint 4π² L / T²") == "9.70 ± 0.19 m/s²"
    e = error_of("L = 1.20 ± 0.01 m\nT = 2.21 ± 0.02 s\ng = 4π² L / T²\nx = g + 1 m")
    assert "can't add" in e.message


def test_negative_uncertainty_is_an_error():
    e = error_of("s = -0.1\nx = 1.0 ± s")
    assert "can't be negative" in e.message


# ---------------------------------------------------------------- Monte Carlo
PENDULUM = "L = 1.20 ± 0.01 m\nT = 2.21 ± 0.02 s\n"


def test_montecarlo_matches_linear_for_a_nearly_linear_formula():
    out = run(PENDULUM + "propagate montecarlo 100000 samples\n    g = 4π² L / T²\n"
              "print value(g) to 8 digits\nprint uncertainty(g) to 8 digits")
    v, s = (num(t) for t in out.split("\n"))
    g = 4 * math.pi ** 2 * 1.20 / 2.21 ** 2
    s_lin = g * math.hypot(0.01 / 1.20, 2 * 0.02 / 2.21)
    n = 100000
    # the mean: within 5 standard errors (plus the small second-order bias 3 g (σT/T)² ≈ 0.0024)
    assert abs(v - g) < 5 * s_lin / math.sqrt(n) + 3 * g * (0.02 / 2.21) ** 2
    assert abs(s - s_lin) / s_lin < 5 / math.sqrt(2 * n) + 0.01


def test_montecarlo_shows_where_linear_fails():
    # y = exp(3x), x = 1.0 ± 0.5: lognormal, mean exp(3 + 9/8), std mean·√(exp(9/4) − 1)
    out = run("x = 1.0 ± 0.5\npropagate montecarlo 200000 samples\n    y = exp(3 x)\n"
              "print value(y) to 8 digits\nprint uncertainty(y) to 8 digits\n"
              "print value(exp(3 x)) to 8 digits\nprint uncertainty(exp(3 x)) to 8 digits")
    mv, ms, lv, ls = (num(t) for t in out.split("\n"))
    mean = math.exp(3 + 9 / 8)
    std = mean * math.sqrt(math.exp(9 / 4) - 1)
    assert abs(mv - mean) / mean < 0.03
    assert abs(ms - std) / std < 0.10              # heavy tail: the sample std converges slowly
    assert close(lv, math.exp(3), 1e-7) and close(ls, 1.5 * math.exp(3), 1e-7)
    assert ms > 5 * ls and mv > 3 * lv            # linear propagation badly underestimates both


def test_montecarlo_is_reproducible_and_seedable():
    prog = PENDULUM + "{seed}propagate montecarlo 5000 samples\n    g = 4π² L / T²\nprint value(g) to 10 digits"
    a = run(prog.format(seed=""))
    assert run(prog.format(seed="")) == a
    b = run(prog.format(seed="seed(7)\n"))
    assert run(prog.format(seed="seed(7)\n")) == b and b != a


def test_montecarlo_one_sample_at_a_time_agrees():
    # an if inside the block can't run on arrays: the block runs once per sample, with the same inputs
    vec = run(PENDULUM + "propagate montecarlo 3000 samples\n    g = 4π² L / T²\nprint value(g) to 9 digits")
    one = run(PENDULUM + "propagate montecarlo 3000 samples\n    g = 4π² L / T²\n    if g < 0 m/s² then g = 0 m/s²\n"
              "print value(g) to 9 digits")
    assert close(num(vec), num(one), 1e-12)


def test_montecarlo_integral_with_uncertain_parameter():
    out = run("k = 0.0 ± 0.1\npropagate montecarlo 1000 samples\n    I = ∫ exp(-k t) dt from 0 to 1\n"
              "print uncertainty(I) to 4 digits")
    assert abs(num(out) - 0.05) < 0.006           # I ≈ 1 - k/2


def test_montecarlo_keeps_correlations():
    # g depends on L; g/L - 4π²/T² uses the same sources, so its uncertainty is small
    out = run(PENDULUM + "propagate montecarlo 100000 samples\n    g = 4π² L / T²\n"
              "print uncertainty(g - g) to 3 digits\nprint uncertainty(g / L) to 6 digits\n"
              "print uncertainty(4π² / T²) to 6 digits")
    zero, a, b = (num(t) for t in out.split("\n"))
    assert zero == 0
    assert abs(a - b) / b < 0.02


def test_montecarlo_block_rules():
    assert "only formulas" in error_of("x = 1 ± 0.1\npropagate montecarlo\n    print x").message
    e = error_of("x = -1.0 ± 0.5\npropagate montecarlo 1000 samples\n    y = √x")
    assert "aren't finite" in e.message


# ---------------------------------------------------------------- fit
DECAY = os.path.join(ROOT, "examples", "data")
FIT = ('data = load "ba137m_decay.csv"\n'
       'fit rate = A exp(-t / τ) + B to data with A = 80 s⁻¹, τ = 150 s, B = 1 s⁻¹\n')


def _scipy_fit():
    from scipy.optimize import curve_fit
    d = np.loadtxt(os.path.join(DECAY, "ba137m_decay.csv"), delimiter=",", skiprows=1)
    p, cov = curve_fit(lambda t, A, tau, B: A * np.exp(-t / tau) + B, d[:, 0], d[:, 1], p0=[80, 150, 1])
    return p, cov


def test_fit_parameters_are_uncertain_with_covariance():
    p, cov = _scipy_fit()
    out = run(FIT + "print value(τ) to 10 digits\nprint uncertainty(τ) to 10 digits\n"
              "print uncertainty(A + B) to 10 digits\nprint uncertainty(τ ln(2)) to 10 digits\nprint err(τ) to 10 digits",
              base_dir=DECAY)
    lines = out.split("\n")[-5:]
    tau, s_tau, s_ab, s_half, err_tau = (num(t) for t in lines)
    assert close(tau, p[1], 1e-6)
    assert close(s_tau, math.sqrt(cov[1, 1]), 1e-3)
    assert close(err_tau, s_tau, 1e-9)
    s_ab_ref = math.sqrt(cov[0, 0] + cov[2, 2] + 2 * cov[0, 2])     # needs the correlation of A and B
    assert close(s_ab, s_ab_ref, 1e-3)
    assert abs(s_ab - math.hypot(math.sqrt(cov[0, 0]), math.sqrt(cov[2, 2]))) > 1e-3 * s_ab
    assert close(s_half, math.log(2) * s_tau, 1e-9)


def test_fit_parameters_print_with_uncertainty():
    # in a program that uses uncertainties (here rel), fitted parameters are uncertain values (D124)
    out = run(FIT + 'print "τ =", τ, rel(τ) to 2 digits', base_dir=DECAY)
    assert re.search(r"τ = 219\.6 ± 2\.9 s 0\.013$", out)


def test_programs_without_uncertainties_keep_plain_fits():
    # no ± anywhere: the parameters stay plain numbers and the program still compiles to native code
    src = FIT + "print τ to 6 digits"
    p = Program(src, os.path.join(DECAY, "x.fm"), out=io.StringIO())
    assert not p.interpreted
    assert "±" not in run(src, base_dir=DECAY).split("\n")[-1]


def test_correlated_helper_reproduces_covariance():
    cov = [[4.0, 1.2], [1.2, 1.0]]
    a, b = correlated([1.0, 2.0], cov)
    assert close(a.s, 2.0) and close(b.s, 1.0) and close(correlation(a, b), 0.6)


# ---------------------------------------------------------------- plots
def test_plot_draws_error_bars(tmp_path, monkeypatch):
    import matplotlib.axes
    calls = []
    orig = matplotlib.axes.Axes.errorbar

    def spy(self, *a, **k):
        calls.append(k)
        return orig(self, *a, **k)
    monkeypatch.setattr(matplotlib.axes.Axes, "errorbar", spy)
    out = run("L = [0.2 m, 0.4 m, 0.6 m]\nT = [0.90 s, 1.27 s, 1.55 s] ± 0.02 s\nplot T vs L to \"eb.png\"",
              base_dir=str(tmp_path))
    assert "plot saved" in out and (tmp_path / "eb.png").exists()
    assert len(calls) == 1 and calls[0]["yerr"] == pytest.approx([0.02] * 3) and calls[0]["xerr"] is None


def test_plot_of_uncertain_curve_draws_a_band(tmp_path, monkeypatch):
    import matplotlib.axes
    calls = []
    orig = matplotlib.axes.Axes.fill_between

    def spy(self, x, y1, y2, **k):
        calls.append((list(y1), list(y2)))
        return orig(self, x, y1, y2, **k)
    monkeypatch.setattr(matplotlib.axes.Axes, "fill_between", spy)
    run("k = 2.0 ± 0.1 N/m\nF(x) = k x\nplot F vs x from 0 m to 1 m to \"band.png\"", base_dir=str(tmp_path))
    lo, hi = calls[0]
    assert hi[-1] - lo[-1] == pytest.approx(0.2)


# ---------------------------------------------------------------- honest limits: clear refusals
@pytest.mark.parametrize("src,msg", [
    ("k = 1.0 ± 0.1\nprint ∫ exp(-k x) dx from 0 to 1", "an integral can't use uncertain values"),
    ("x0 = 1.0 ± 0.1 m\nsolve x' = -x / (1 s) with x(0 s) = x0 for t from 0 s to 1 s\nprint x(1 s)",
     "starting value of solve can't be uncertain"),
    ("k = 1.0 ± 0.1\nsolve x' = -k x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s\nprint x(1 s)",
     "differential equation (solve) can't use uncertain values"),
    ("a = 1.0 ± 0.1 m\nv = <a, 2 m>\nprint v", "needs a plain number, but got an uncertain value"),
])
def test_refusals(src, msg):
    e = error_of(src)
    assert msg in e.message, e.message
    assert "propagate montecarlo" in e.message or "value(x)" in e.message


def test_build_refuses(tmp_path):
    from fermium.aot import build
    src = tmp_path / "u.fm"
    src.write_text("x = 1.0 ± 0.1\nprint x\n")
    with pytest.raises(FermiumError) as ei:
        build(src.read_text(), str(src), str(tmp_path / "u"))
    assert "fermium build doesn't support uncertainties" in ei.value.message


def test_repl_refuses():
    s = ReplSession(out=io.StringIO())
    with pytest.raises(FermiumError) as ei:
        s.execute("x = 1.0 ± 0.1")
    assert "not yet in the REPL" in ei.value.message


# ---------------------------------------------------------------- both back ends print the same
LAB = PENDULUM + """g = 4π² L / T²
print g, g in ft/s², rel(g) to 3 digits
propagate montecarlo 20000 samples
    g2 = 4π² L / T²
print g2
"""


def test_run_and_interpreter_agree():
    out = io.StringIO()
    run_interpreted(LAB, "<test>", out=out)
    assert run(LAB) == out.getvalue().strip()
    assert Program(LAB, out=io.StringIO()).interpreted     # ± programs run in the interpreter (D122)


def test_ufloat_arithmetic_basics():
    x = UFloat.measured(2.0, 0.1)
    y = UFloat.measured(3.0, 0.2)
    assert (x - x).s == 0 and (x / x).s == 0
    assert close((x * y).s, math.hypot(3 * 0.1, 2 * 0.2))
    assert close((x ** 2).s, 0.4) and close((2 ** x).s, math.log(2) * 4 * 0.1)
    with pytest.raises(Exception):
        float(x)
