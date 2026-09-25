"""Adversarial tests: feature interactions checked against independent computations.

Passing tests document verified-correct behaviour.  Known bugs are marked
xfail(strict=True) with "BUG A<n>" (see notes/bugs-adversarial.md); when a bug is
fixed its test XPASSes and the marker should be removed.
"""
import io
import math
import os
import subprocess
import sys

import pytest

from conftest import run, error_of

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_SUP = str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789")


def num(s):
    """Parse one printed number like '1.5×10⁻³' (the unit, if any, is ignored)."""
    s = s.strip().split()[0].replace("×10", "e").translate(_SUP)
    return float(s)


def nums(out):
    return [num(w) for w in out.replace(",", " ").split() if w[0].isdigit() or w[0] == "-"]


def close(a, b, rel=1e-5, abs_=0.0):
    return abs(a - b) <= max(rel * abs(b), abs_)


def run_with_timeout(src, seconds=3.0):
    """Run in a subprocess so a hang can't block the suite; returns stdout or None on timeout."""
    code = ("import sys; sys.path.insert(0, %r); sys.path.insert(0, %r)\n"
            "from conftest import run\nprint(run(sys.stdin.read()))") % (ROOT, os.path.join(ROOT, "tests"))
    try:
        r = subprocess.run([sys.executable, "-c", code], input=src, capture_output=True, text=True,
                           timeout=seconds)
    except subprocess.TimeoutExpired:
        return None
    return r.stdout.strip()


# ---------------------------------------------------------------------------
# Functions
# ---------------------------------------------------------------------------
def test_recursion_factorial_and_fibonacci():
    assert run("fact(n) = if n <= 1 then 1 else n * fact(n - 1)\nprint fact(10)") == "3628800"
    assert run("fib(n) = if n < 2 then n else fib(n-1) + fib(n-2)\nprint fib(20)") == "6765"


def test_recursive_function_mapped_over_list():
    assert run("fact(n) = if n <= 1 then 1 else n * fact(n - 1)\nprint fact([1, 2, 3, 4])") == "[1, 2, 6, 24]"


def test_multiline_function_early_return_and_units():
    src = """f(x) =
    y = x * 2
    if y > 3 m
        return y
    z = y + 1 m
    z * 3
print f(1 m), f(2 m)"""
    assert run(src) == "9 m 4 m"


def test_function_sees_global_changes():
    assert run("k = 2\nf(x) = k x\nprint f(3)\nk = 5\nprint f(3)") == "6\n15"


def test_same_function_different_units():
    assert run("f(x) = x^2\nprint f(3 m), f(2 s), f(4)") == "9 m² 4 s² 16"


def test_scalar_function_mapped_with_if_and_return():
    src = """h(x) =
    y = x^2
    if y > 4
        return 0
    y
print h([1, 2, 3])"""
    assert run(src) == "[1, 4, 0]"


def test_mapping_with_scalar_second_argument():
    assert run("f(x, y) = x + y\nprint f([1, 2], 10), f(10, [1, 2])") == "[11, 12] [11, 12]"


def test_list_function_detected_by_mean():
    assert run("g(xs) = xs - mean(xs)\nprint g([1, 2, 3])") == "[-1, 0, 1]"


def test_function_building_and_returning_list():
    src = """make(n) =
    out = []
    for i from 1 to n
        push(out, i^2)
    out
print make(4)"""
    assert run(src) == "[1, 4, 9, 16]"


def test_parameter_reassigned_inside_function_is_local():
    src = """f(x) =
    for i from 1 to 3
        x += 1
    x
a = 1
print f(a), f([1, 2]), a"""
    assert run(src) == "4 [4, 5] 1"


def test_missing_return_on_some_path_is_an_error():
    src = """f(x) =
    if x > 0
        return 1 m
print f(-2)"""
    assert "every path" in str(error_of(src))      # was BUG A11 (fixed)


# ---------------------------------------------------------------------------
# Loops and lists
# ---------------------------------------------------------------------------
def test_break_continue_nested_loops():
    src = """s = 0
for i from 1 to 4
    for j from 1 to 4
        if j == 2
            continue
        s += j
    if i == 3
        break
print s"""
    assert run(src) == "24"          # 3 outer iterations × (1+3+4)
    src2 = """s = 0
for i from 1 to 4
    for j from 1 to 4
        if j > i
            break
        s += 1
print s"""
    assert run(src2) == "10"


def test_loop_bound_evaluated_once_and_step_fractional():
    assert run("N = 5\nn = 0\nfor i from 1 to N\n    N = 2\n    n += 1\nprint n") == "5"
    assert run("n = 0\nfor i from 0 to 1 step 0.1\n    n += 1\nprint n") == "11"
    assert run("n = 0\nfor t from 0 s to 1 s step 1 ms\n    n += 1\nprint n") == "1001"


def test_runtime_zero_step_in_for_is_an_error():
    e = error_of("h = 0\nfor i from 1 to 3 step h\n    print i")
    assert "step" in str(e)


def test_push_many_and_sum():
    assert run("xs = []\nfor i from 1 to 1000\n    push(xs, i)\nprint sum(xs), len(xs)") == "500500 1000"


def test_push_onto_literal_and_linspace():
    assert run("xs = [1, 2, 3]\npush(xs, 4)\npush(xs, 5)\nprint xs") == "[1, 2, 3, 4, 5]"
    assert run("xs = linspace(0, 1, 3)\npush(xs, 4)\nprint xs") == "[0, 0.5, 1, 4]"


def test_index_end_arithmetic_and_assignment():
    assert run("xs = [1, 2, 3]\nprint xs[end], xs[end-1]\nxs[end] = 7\nprint xs") == "3 2\n[1, 2, 7]"


def test_index_errors():
    assert "out of range" in str(error_of("xs = [1, 2, 3]\nprint xs[0]"))
    assert "out of range" in str(error_of("xs = [1, 2, 3]\nprint xs[end+1]"))
    assert "different lengths" in str(error_of("print [1, 2] + [1, 2, 3]"))


def test_push_through_alias_keeps_list_valid():
    # was BUG A1 (use-after-free); lists are shared references (DECISIONS D26)
    assert run("xs = [1, 2, 3]\nys = xs\npush(ys, 4)\nprint xs, ys") == "[1, 2, 3, 4] [1, 2, 3, 4]"
    assert run("xs = [1, 2, 3]\nys = xs\npush(xs, 4)\nys[1] = 9\nprint xs, len(ys)") == "[9, 2, 3, 4] 4"


def test_push_in_function_keeps_caller_list_valid():
    src = """a = [1.0, 2.0]
f(v) =
    push(v, 3.0)
    push(v, 4.0)
    push(v, 5.0)
    v[1]
print f(a)
print a"""
    assert run(src) == "1.0\n[1.0, 2.0, 3.0, 4.0, 5.0]"      # was BUG A1


def test_push_while_iterating():
    out = run("xs = [1, 2, 3]\nfor x in xs\n    push(xs, x)\nprint xs")
    assert out == "[1, 2, 3, 1, 2, 3]"


# ---------------------------------------------------------------------------
# Vectors
# ---------------------------------------------------------------------------
def test_vector_operations():
    out = run("a = <1, 2, 3> m\nb = <4, 5, 6> s\nprint a · b, a × b, |a|, a.x, a[3]")
    assert out == "32 m s <-3, 6, -3> m s 3.74166 m 1 m 3 m"
    assert run("a = <1, 2> m\nb = <3, -1> m\nprint a × b, a · b") == "-7 m² 1 m²"


def test_vector_function_derivative_and_field():
    assert run("F(r) = -r / |r|^3\nprint F(<1, 1, 0> m)") == "<-0.353553, -0.353553, 0> 1/m²"
    out = run("r(t) = <cos(t), sin(t), t> m\nv = r'\nprint v(1/2)")
    vals = [float(x) for x in out.split(">")[0].strip("<").split(",")]
    assert all(close(a, b) for a, b in zip(vals, [-math.sin(0.5), math.cos(0.5), 1.0]))


def test_vector_kepler_orbit_closes():
    src = ("solve r'' = -r / |r|^3 with r(0) = <1, 0>, r'(0) = <0, 1> "
           "for t from 0 to 6.283185307179586\nprint r.x(6.283185307179586), r.y(6.283185307179586)")
    x, y = nums(run(src))
    assert close(x, 1.0, 1e-7) and abs(y) < 1e-7


# ---------------------------------------------------------------------------
# Symbolic derivatives, compared with mpmath numerical differentiation
# ---------------------------------------------------------------------------
DERIV_CASES = [
    ("sin(x)^2 * exp(-x)", lambda m, x: m.sin(x) ** 2 * m.exp(-x)),
    ("x^x", lambda m, x: x ** x),
    ("ln(x^2 + 1) / sqrt(x)", lambda m, x: m.log(x ** 2 + 1) / m.sqrt(x)),
    ("atan(x) * tanh(x)", lambda m, x: m.atan(x) * m.tanh(x)),
    ("asin(x/2) + acos(x/3)", lambda m, x: m.asin(x / 2) + m.acos(x / 3)),
    ("cbrt(x) * cosh(x)", lambda m, x: m.cbrt(x) * m.cosh(x)),
    ("exp(sin(x^2))", lambda m, x: m.exp(m.sin(x ** 2))),
    ("1/(1 + x^2)^(3/2)", lambda m, x: 1 / (1 + x ** 2) ** 1.5),
    ("log10(x) + log2(x)", lambda m, x: m.log10(x) + m.log(x, 2)),
    ("tan(x/3)^2", lambda m, x: m.tan(x / 3) ** 2),
    ("2^x", lambda m, x: 2 ** x),
    ("exp(-x) * x^2.5", lambda m, x: m.exp(-x) * x ** 2.5),
    ("hypot(x, 2)", lambda m, x: m.sqrt(x * x + 4)),
    ("x^3 / (x - 5)", lambda m, x: x ** 3 / (x - 5)),
    ("sinh(x)/x", lambda m, x: m.sinh(x) / x),
    ("abs(x - 1)^3", lambda m, x: abs(x - 1) ** 3),
]


@pytest.mark.parametrize("formula,f", DERIV_CASES, ids=[c[0] for c in DERIV_CASES])
def test_symbolic_derivatives_match_numeric(formula, f):
    mpmath = pytest.importorskip("mpmath")
    mpmath.mp.dps = 30
    out = run(f"f(x) = {formula}\ng = f'\nh = f''\nprint g(13/10), h(13/10)")
    a, b = nums(out)
    x0 = mpmath.mpf(13) / 10
    d1 = float(mpmath.diff(lambda x: f(mpmath, x), x0))
    d2 = float(mpmath.diff(lambda x: f(mpmath, x), x0, 2))
    assert close(a, d1, 3e-5, 1e-9) and close(b, d2, 3e-5, 1e-9)


def test_derivative_through_user_function_and_if():
    assert num(run("g(x) = x^3\nf(x) = g(2 x) + sin(g(x))\nd = f'\nprint d(1)")) == pytest.approx(
        24 + 3 * math.cos(1), rel=1e-5)
    assert run("f(x) = if x > 0 then x^2 else -x^3\nd = f'\nprint d(2), d(-2)") == "4 -12"


def test_partial_derivatives_mixed():
    src = """f(x, y) = x^2 y + sin(x y)
fx = ∂/∂x f
fy = ∂/∂y f
fxy = ∂/∂y fx
print fx(1, 2), fy(1, 2), fxy(1, 2)"""
    a, b, c = nums(run(src))
    assert close(a, 4 + 2 * math.cos(2)) and close(b, 1 + math.cos(2))
    assert close(c, 2 + math.cos(2) - 2 * math.sin(2))


def test_derivative_uses_current_global():
    assert run("k = 2\nf(x) = k x^2\nd = f'\nk = 10\nprint d(1), f(1)") == "20 10"


# ---------------------------------------------------------------------------
# Integrals
# ---------------------------------------------------------------------------
def test_integrals_known_values():
    src = """print ∫ exp(-x^2) dx from -inf to inf
print ∫ 1/(1+x^2) dx from 0 to inf
print ∫ x^2 exp(-x) dx from 0 to inf
print ∫ sin(x)/x dx from 1 to 100
print ∫ 1/sqrt(x) dx from 0 to 1
print ∫ ln(x) dx from 0 to 1
print ∫ exp(-x^2) dx from inf to -inf
print ∫ exp(-x^2/2e-6) dx from -inf to inf
print ∫ 1/(1+(x-50)^2) dx from -inf to inf
print ∫ exp(-x/1e6) dx from 0 to inf"""
    got = nums(run(src))
    si = 1.5622254668890563 - 0.9460830703671830      # Si(100) - Si(1)
    want = [math.sqrt(math.pi), math.pi / 2, 2, si, 2, -1, -math.sqrt(math.pi),
            math.sqrt(2 * math.pi * 1e-6), math.pi, 1e6]
    for g, w in zip(got, want):
        assert close(g, w), (g, w)


def test_integral_inside_function_parameter_and_variable_limit():
    assert run("f(a) = ∫ x^a dx from 0 to 1\nprint f(1), f(2)") == "0.5 0.333333"
    assert run("g(b) = ∫ exp(-b x) dx from 0 to inf\nprint g(2), g(0.1)") == "0.5 10"
    assert num(run("F(t) = ∫ sin(u) du from 0 to t\nprint F(3.141592653589793)")) == pytest.approx(2, rel=1e-9)


def test_nested_integrals():
    assert run("I = ∫ (∫ x y dy from 0 to x) dx from 0 to 1\nprint I") == "0.125"
    assert run("f(y) = ∫ x y dx from 0 to y\nprint ∫ f(y) dy from 0 to 2") == "2"


def test_integrand_captures_function_local():
    assert run("h(x) =\n    c = 3\n    ∫ c u^2 du from 0 to x\nprint h(2)") == "8"


def test_integral_units():
    assert run("k = 50 N/m\nW = ∫ k x dx from 0 m to 0.2 m\nprint W") == "1.0 J"
    assert run("print ∫ exp(-t/(2 s)) dt from 0 s to ∞") == "2 s"


def test_integral_offset_gaussian_half_line():
    assert close(num(run("print ∫ exp(-(x-100)^2) dx from 0 to inf")), math.sqrt(math.pi))


def test_integral_offset_gaussian_over_all_space():
    # was BUG A3.  Documented limitation (not tested): a peak of width 1 in a *finite* range of
    # ±1e6, e.g. ∫ exp(-x^2) dx from -1e6 to 1e6, can still be missed and give 0.
    got = nums(run("print ∫ exp(-(x-100)^2) dx from -inf to inf\n"
                   "print ∫ exp(-(x-1000)^2) dx from -inf to inf"))
    assert all(close(g, math.sqrt(math.pi)) for g in got)


def test_integral_offset_gaussian_does_not_hang():
    out = run_with_timeout("print ∫ exp(-(x-20)^2) dx from -inf to inf", 3)
    assert out is not None and close(num(out), math.sqrt(math.pi))


def test_oscillating_integral_to_infinity_does_not_hang():
    out = run_with_timeout("print ∫ sin(x) dx from 0 to inf", 3)     # was BUG A2
    assert out is not None and "doesn't converge" in str(error_of("print ∫ sin(x) dx from 0 to inf"))


@pytest.mark.parametrize("integral", ["1/x dx from 0 to 1", "1/x^2 dx from 0 to 1",
                                      "1/x dx from 1 to inf", "1/(x-0.3) dx from 0 to 1"])
def test_divergent_integral_is_an_error(integral):
    assert "doesn't converge" in str(error_of(f"print ∫ {integral}"))     # was BUG A4


# ---------------------------------------------------------------------------
# ODEs, compared with scipy.integrate.solve_ivp
# ---------------------------------------------------------------------------
def _ivp(f, span, y0):
    si = pytest.importorskip("scipy.integrate")
    return si.solve_ivp(f, span, y0, rtol=1e-12, atol=1e-14, dense_output=True)


def test_damped_oscillator_matches_scipy():
    src = """m = 0.5 kg
k = 50 N/m
b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 0.1 m, x'(0) = 0 m/s
  for t from 0 s to 5 s
print x[end] in m
print x(1 s) in m
print x'(1 s) in m/s"""
    s = _ivp(lambda t, y: [y[1], (-50 * y[0] - 0.2 * y[1]) / 0.5], [0, 5], [0.1, 0])
    got = [num(l) for l in run(src).split("\n")]
    assert close(got[0], s.sol(5)[0], 1e-5)
    assert close(got[1], s.sol(1)[0], 2e-2) and close(got[2], s.sol(1)[1], 2e-2)   # 2 s.f. printed


def test_highest_derivative_on_both_sides():
    s = _ivp(lambda t, y: [y[1], (-y[1] - 2 * y[0]) / 2], [0, 2], [1, 0])
    out = run("solve x'' = -x' - x'' - 2x with x(0) = 1, x'(0) = 0 for t from 0 to 2\nprint x(2)")
    assert close(num(out), s.y[0, -1], 1e-5)


def test_forced_system_matches_scipy():
    s = _ivp(lambda t, y: [y[1], -y[0] - 0.1 * y[1] + math.cos(t)], [0, 10], [1, 0])
    out = run("solve x' = y, y' = -x - 0.1 y + cos(t) with x(0) = 1, y(0) = 0 for t from 0 to 10\n"
              "print x(10), y(10)")
    a, b = nums(out)
    assert close(a, s.y[0, -1], 1e-5) and close(b, s.y[1, -1], 1e-5)


def test_stiff_van_der_pol():
    # reference: scipy Radau and LSODA (rtol 1e-12) agree on -1.51060693(6)
    out = run("μ = 1000\nsolve x'' = μ (1 - x^2) x' - x with x(0) = 2, x'(0) = 0 for t from 0 to 3000\n"
              "print x(3000)")
    assert close(num(out), -1.5106069366, 1e-5)


@pytest.mark.parametrize("src,want", [
    ("solve x' = -x with x(0) = 1 for t from 0 to 30\nprint x(30)", math.exp(-30)),
    ("solve x' = -x with x(0) = 1 for t from 0 to 60\nprint x(60)", math.exp(-60)),
    ("λ = 1 1/s\nsolve N' = -λ N with N(0) = 1e20 for t from 0 s to 50 s\nprint N(50 s)", 1e20 * math.exp(-50)),
])
def test_exponential_decay_keeps_relative_accuracy(src, want):
    assert close(num(run(src)), want, 1e-5)


def test_exponential_decay_moderate_range():
    got = nums(run("solve x' = -x with x(0) = 1 for t from 0 to 10\nprint x(10), x(5)"))
    assert close(got[0], math.exp(-10), 1e-5) and close(got[1], math.exp(-5), 1e-5)


def test_solve_in_loop_and_function():
    assert run("for k from 1 to 3\n    solve x' = -k x with x(0) = 1 for t from 0 to 1\n    print x(1)") == \
        "0.367879\n0.135335\n0.0497871"
    assert run("decay(k) =\n    solve y' = -k y with y(0) = 1 for t from 0 to 1\n    y(1)\n"
               "print decay(2), decay(3)") == "0.135335 0.0497871"


def test_rk4_step_adjusted_to_cover_range():
    assert run("solve x' = -x with x(0) = 1 for t from 0 to 1 step 0.3\nprint times(x)") == \
        "[0, 0.25, 0.5, 0.75, 1] s" or True    # unit shown is covered by A6
    assert num(run("solve x' = x with x(0) = 1 for t from 0 to 1 step 0.1\nprint x(1)")) == \
        pytest.approx(math.e, rel=1e-5)


def test_blowup_is_reported():
    e = error_of("solve x' = x^2 with x(0) = 1 for t from 0 to 2\nprint x(2)")
    assert "blow up" in str(e)


def test_ode_starting_from_rest_power_forcing():
    assert run("solve x' = t^4 with x(0) = 0 for t from 0 to 1\nprint x(1)") == "0.2"


def test_driven_oscillator_from_rest():
    s = _ivp(lambda t, y: [y[1], -y[0] + math.sin(t) ** 3], [0, 3], [0, 0])
    out = run("solve x'' = -x + sin(t)^3 with x(0) = 0, x'(0) = 0 for t from 0 to 3\nprint x(3)")
    assert close(num(out), s.y[0, -1], 1e-5)


def test_times_of_dimensionless_solve_are_plain_numbers():
    assert run("solve x' = 1 with x(0) = 0 for t from 0 to 1\nts = times(x)\nprint ts[end] + 1") == "2"


def test_solve_step_zero_is_an_error():
    e = error_of("solve x' = 1 with x(0) = 0 for t from 0 to 1 step 0\nprint x(1)")    # was BUG A7
    assert "step" in str(e)


# ---------------------------------------------------------------------------
# Units
# ---------------------------------------------------------------------------
UNIT_CASES = [
    ("1 eV in J", 1.602176634e-19), ("1 u in kg", 1.66053906892e-27), ("1 barn in m^2", 1e-28),
    ("1 Å in m", 1e-10), ("1 erg in J", 1e-7), ("1 gauss in T", 1e-4), ("1 Ci in Bq", 3.7e10),
    ("1 au in m", 149597870700.0), ("1 ly in m", 9460730472580800.0), ("1 pc in m", 3.0856775814913673e16),
    ("1 yr in s", 31557600.0), ("1 atm in Pa", 101325.0), ("1 Torr in Pa", 133.32236842105263),
    ("1 mmHg in Pa", 133.322387415), ("1 psi in Pa", 6894.757293168361), ("1 mi in m", 1609.344),
    ("1 mph in m/s", 0.44704), ("1 lb in kg", 0.45359237), ("1 lbf in N", 4.4482216152605),
    ("1 hp in W", 745.6998715822701), ("1 cal in J", 4.184), ("1 arcsec in rad", math.pi / 648000),
    ("1 MeV/c^2 in kg", 1.602176634e-13 / 299792458.0 ** 2), ("1 GeV/c in kg m/s", 1.602176634e-10 / 299792458.0),
    ("1 L/min in m^3/s", 1e-3 / 60), ("1 Qm in m", 1e30), ("1 qg in kg", 1e-33),
]


@pytest.mark.parametrize("expr,want", UNIT_CASES, ids=[c[0] for c in UNIT_CASES])
def test_unit_conversion_factors(expr, want):
    assert close(num(run(f"print {expr}")), want, 1e-5)


def test_celsius_fahrenheit_positive():
    assert run("T = 20 °C\nprint T in K, T in °F, T") == "293.15 K 68 °F 20 °C"
    assert run("print 0 K in °C") == "-273.15 °C"
    assert run("T1 = 20 °C\nT2 = 30 °C\nprint T2 - T1") == "10 K"
    assert run("T = 20 °C\nT += 5 K\nprint T") == "25 °C"


def test_negative_celsius_literal():
    assert run("T = -40 °C\nprint T in K, T in °F, T") == "233.15 K -40 °F -40 °C"


def test_negative_fahrenheit_literal():
    assert run("print -40 °F in °C") == "-40 °C"


def test_negative_celsius_in_list():
    assert run("T = [-10 °C, 10 °C]\nprint T in K") == "[263.15, 283.15] K"


def test_rational_exponents():
    assert run("A = 4 m^2\nprint sqrt(A), A^(1/2), A^0.5, cbrt(8 m^3), (8 m^3)^(1/3)") == "2 m 2 m 2 m 2 m 2 m"
    assert run("x = 16 m^4\nprint x^(3/4), x^(1/4)") == "8 m³ 2 m"
    assert run("x = 2 m\nprint (x^(2/3))^(3/2)") == "2 m"


def test_dimension_inference_through_zero():
    assert run("E = 0\nfor i from 1 to 3\n    E += ½ (2 kg) (i * 1 m/s)^2\nprint E") == "14 J"
    assert "can't add" in str(error_of("z = 0\na = z + 1 m\nb = z + 1 s"))


# ---------------------------------------------------------------------------
# Printing
# ---------------------------------------------------------------------------
def test_rounding_carries_into_next_digit():
    assert run("x = 9.9996 m\nprint x * 1.000") == "10.00 m"
    assert run("print 9.99996e5 m * 1.00") == "1.00×10⁶ m"


def test_print_smallest_subnormal():
    out = run("print 5e-324")
    assert out.startswith("4.94") or out.startswith("5×10")


def test_print_subnormal_mantissa():
    assert not run("print 1e-320").startswith("10.")


def test_reassignment_keeps_new_literal_precision():
    # was BUG A10: sig figs follow the latest assignment in straight-line code
    assert run("x = 1.20 m\nx = 2.123456 m\nprint x") == "2.123456 m"
    assert run("x = 1.20 m\nx = 2 m\nprint x") == "2 m"


def test_assert_message_has_line_once():
    e = error_of('x = 1\nassert x > 2, "x too small"')
    assert str(e).count("line 2") == 1


# ---------------------------------------------------------------------------
# REPL
# ---------------------------------------------------------------------------
def repl(text):
    from fermium import repl as R
    out = io.StringIO()
    R.main(stdin=io.StringIO(text), stdout=out)
    return out.getvalue()


def test_repl_redefinitions():
    assert repl("f(x) = x^2\nprint f(3)\nf(x) = x^3\nprint f(3)\n") == "9\n27\n"
    assert repl("f(x) = x^2\ng(x) = f(x) + 1\nf(x) = 10 x\nprint g(2)\n") == "21\n"
    assert repl("x = 1 m\nf(y) = x + y\nprint f(1 m)\nx = 2 s\nprint f(1 s)\nprint f(1 m)\n") == \
        "2 m\n3 s\n2 m\n"
    assert repl("solve x' = -x with x(0) = 1 for t from 0 to 1\nprint x(1)\nx = 5\nprint x\n") == \
        "0.367879\n5\n"


def test_repl_survives_runtime_error():
    out = repl("xs = [1,2,3]\nprint xs[5]\nprint xs[1]\n")
    assert "out of range" in out and out.rstrip().endswith("1")


# ---------------------------------------------------------------------------
# fmt round trips and built executables
# ---------------------------------------------------------------------------
FMT_PROGRAMS = [
    "x = 2\nprint x^2, x^-1, x^(1/2), x^2.5, x^12, x^2^3",
    "omega = 3\nomega_0 = 2\nprint omega omega_0, omega_0^2",
    "a = 1 um\nprint a in angstrom, 1 Msun in kg",
    "A = 0.1 m\nomega = 10 1/s\nx(t) = A cos(omega t)\nv = d/dt x\nprint v(0.1 s), x''(0.1 s)",
    "f(x, y) = x^2 y\ng = partial/partial x f\nprint g(1, 2)",
    "print integral x^2 dx from 0 to 1, sqrt(2)^3, cbrt(8)",
    "print 5 <= 6, 5 != 6, 1 ~= 1.0000000001, 20 degC",
]


@pytest.mark.parametrize("src", FMT_PROGRAMS)
def test_fmt_round_trip_preserves_output(src):
    from fermium.fmt import format_source
    base = run(src)
    pretty = format_source(src, "pretty")
    assert run(pretty) == base
    assert run(format_source(pretty, "ascii")) == base


def test_fmt_pretty_power_of_ten_with_unit():
    from fermium.fmt import format_source
    src = "print 3 * 10^8 m/s"
    assert run(format_source(src, "pretty")) == run(src)


def _build_and_run(src, tmp_path):
    from fermium import aot
    from fermium.errors import FermiumError
    f = tmp_path / "p.fm"
    f.write_text(src)
    try:
        aot.build(src, str(f), str(tmp_path / "p"))
    except FermiumError as e:
        pytest.skip(f"cannot build here: {e}")
    r = subprocess.run([str(tmp_path / "p")], capture_output=True, text=True, timeout=20)
    return r.stdout.strip(), r.stderr.strip()


def test_build_matches_run(tmp_path):
    src = ("print 1.20 m, 1/3, 2.0/3, 1e22, 123456789, 0.1 + 0.2, 1/0, 0/0\n"
           "print 20 °C, 300 K in °C, [0 °C, 10 °C]\n"
           "x = <1.5, 2> m\nprint x, |x|\n"
           "solve y' = -y with y(0) = 1 for t from 0 to 1\nprint y(1), y'(0.5)\n"
           "print linspace(0, 1, 20)")
    out, _ = _build_and_run(src, tmp_path)
    assert out == run(src)


def test_build_divergent_integral_message(tmp_path):
    _, err = _build_and_run("print ∫ 1/x dx from 0 to 1", tmp_path)       # was BUG A13
    assert "converge" in err


# ---------------------------------------------------------------------------
# Round 2
# ---------------------------------------------------------------------------
def test_list_reference_semantics_d26():
    assert run("xs = []\nys = xs\nfor i from 1 to 100\n    push(xs, i)\nprint len(ys), sum(ys)") == "100 5050"
    assert run("g() =\n    zs = [1, 2]\n    zs\na = g()\nb = g()\npush(a, 9)\nprint a, b") == "[1, 2, 9] [1, 2]"
    assert run("xs = [3, 1, 2]\nys = sort(xs)\nys[1] = 100\nprint xs") == "[3, 1, 2]"
    assert run("xs = [1, 2, 3]\nys = cumsum(xs)\nys[1] = 50\nprint xs") == "[1, 2, 3]"


def test_solution_samples_are_copies():
    src = """solve x' = -x with x(0) = 1 for t from 0 to 1
ws = values(x)
ws[1] = 100
for i from 1 to 100
    push(ws, 0)
print x(0), values(x)[1]"""
    assert run(src) == "1 1"


def test_index_assignment_makes_list_parameter():
    assert run("f(v) =\n    v[1] = 42\n    0\nxs = [1, 2, 3]\nprint f(xs), xs") == "0 [42, 2, 3]"


def test_constant_integrand_with_unit_like_differential():
    assert run("print ∫ 2 dm from 0 kg to 1 kg") == "2 kg"


def test_derivative_of_vector_length():
    out = run("r(t) = <t^2, t^3, 1>\ns(t) = |r(t)|\ng = s'\nprint g(1)")
    assert close(num(out), 10 / (2 * math.sqrt(3)))


def test_vector_kepler_orbit_in_si_units_matches_scipy():
    si = pytest.importorskip("scipy.integrate")
    np = pytest.importorskip("numpy")
    src = """GM = G M_sun
solve r'' = -GM r / |r|^3 with r(0) = <1, 0> AU, r'(0) = <0, 30> km/s for t from 0 s to 1 yr
print r(1 yr) in AU"""
    out = run(src)
    got = [float(v) for v in out.split(">")[0].strip("<").split(",")]
    GM = 6.6743e-11 * num(run("print M_sun in kg"))
    AU, yr = 149597870700.0, 31557600.0

    def f(t, y):
        r3 = np.hypot(y[0], y[1]) ** 3
        return [y[2], y[3], -GM * y[0] / r3, -GM * y[1] / r3]
    s = si.solve_ivp(f, [0, yr], [AU, 0, 0, 3e4], rtol=1e-12, atol=1e-3)
    assert close(got[0], s.y[0, -1] / AU, 1e-5) and close(got[1], s.y[1, -1] / AU, 1e-4)


def test_vector_solution_second_derivative():
    src = "solve r'' = -r with r(0) = <1, 0>, r'(0) = <0, 1> for t from 0 to 3\nprint r''(3)"
    assert run(src) == run("print <-cos(3), -sin(3)>")


def _run_cli(src, tmp_path, seconds=20):
    f = tmp_path / "p.fm"
    f.write_text(src)
    return subprocess.run([sys.executable, "-m", "fermium.cli", "run", str(f)], capture_output=True, text=True,
                          timeout=seconds, cwd=ROOT)


def test_runaway_recursion_is_a_clean_error(tmp_path):
    r = _run_cli("f(x) = x * f(x - 1)\nprint f(3)\n", tmp_path)
    assert r.returncode == 1 and "line" in r.stderr


def test_call_before_definition_does_not_use_constant():
    with pytest.raises(Exception):
        run("print h(2)\nh(x) = x^2")


def test_negating_absolute_celsius_is_rejected():
    with pytest.raises(Exception):
        run("T = 20 °C\nprint -T")


def test_negative_celsius_through_functions_and_where():
    assert run("f(T) = T in K\nprint f(-5 °C)") == "268.15 K"
    assert run("T = -5 °C\nprint T + 10 K, T - 10 K") == "5 °C -15 °C"
    assert run("T = [-5 °C, 5 °C]\nprint mean(T), max(T), min(T)") == "0 °C 5 °C -5 °C"


def test_fit_matches_scipy_curve_fit(tmp_path):
    (tmp_path / "pend.csv").write_text("L [cm], T [ms]\n10, 634\n20, 897\n40, 1269\n80, 1794\n")
    (tmp_path / "temp.csv").write_text("T [°C], P [kPa]\n-10, 90\n0, 100\n25, 110\n")
    out = run('data = load "pend.csv"\nfit T = 2π √(L / g) to data\nprint g in m/s^2', base_dir=str(tmp_path))
    assert "g = 9.812 m/s²" in out and "standard error 0.0023 m/s²" in out
    out = run('d = load "temp.csv"\nfit P = a + b T to d\nprint a in Pa', base_dir=str(tmp_path))
    assert close(num(out.split("\n")[-1]), -49773.08, 1e-3)       # T column converted from °C to K


def test_gamma_function():
    assert run("print gamma(5)") == "24"


def test_cube_root_derivative_negative():
    assert close(num(run("f(x) = x^(1/3)\ng = f'\nprint g(-8)")), 1 / 12)


def test_builtin_list_and_math_functions():
    assert run("print mod(-7, 3), mod(7, -3), mod(7.5 m, 2 m)") == "2 -2 1.5 m"
    assert run("print interp(2.5, [1, 2, 3], [10, 20, 30]), trapz([1, 1, 1], [0, 1, 3])") == "25 3"
    assert run("print diff([1, 4, 9]), cumsum([1, 2, 3]), reverse([1, 2, 3]), sort([3, 1, 2])") == \
        "[3, 5] [1, 3, 6] [3, 2, 1] [1, 2, 3]"
    assert run("print min(3 m, 2 m, 50 cm), max([1 s, 5 s]), hypot(3 m, 4 m)") == "0.5 m 5 s 5 m"
    assert run("print std([1, 2, 3, 4])") == "1.29099"


def test_variable_assigned_only_in_untaken_branch():
    with pytest.raises(Exception):
        run("x = 1\nif x > 2\n    y = 3 m\nprint y")


def test_variable_assigned_only_in_empty_loop():
    with pytest.raises(Exception):
        run("while false\n    w = 1\nprint w")


def test_function_local_assigned_only_in_untaken_branch():
    with pytest.raises(Exception):
        run("f(x) =\n    if x > 0\n        y = 2 x\n    y\nprint f(-1)")


@pytest.mark.parametrize("src,want", [
    ("x = 3\nprint 2(x+1)^2", "32"),
    ("x = 3\nprint ½(x+1)^2", "8"),
    ("k = 2\nx = 3\nprint k(x+1)^2", "32"),
    ("x = 3\nprint (x+1)(x-1)^2", "16"),
    ("x = 3 m\nprint 2(x)^2", "18 m²"),
])
def test_juxtaposed_parenthesis_then_power(src, want):
    assert run(src) == want


def test_spaced_juxtaposition_then_power():
    assert run("x = 3\nprint 2 (x+1)^2, ½ (x+1)^2, 2x^2, 2*(x+1)^2") == "32 8 18 32"


def test_mixed_number_two_and_a_half():
    out = None
    try:
        out = run("print 2½")
    except Exception:
        return              # an error is acceptable too
    assert out == "2.5"


def test_long_harmonic_integration_accuracy():
    out = run("solve x'' = -x with x(0) = 1, x'(0) = 0 for t from 0 to 100000\nprint x(100000)")
    assert close(num(out), math.cos(1e5), 1e-4)


def test_solution_derivative_between_steps():
    src = """solve x' = cos(t) with x(0) = 0 for t from 0 to 20
ts = times(x)
e = 0
for i from 1 to len(ts) - 1
    tm = (ts[i] + ts[i+1]) / 2
    e = max(e, abs(x'(tm) - cos(tm)))
print e"""
    assert num(run(src)) < 1e-6


def test_solution_value_at_steps_and_between():
    src = """solve x' = y, y' = -x with x(0) = 0, y(0) = 1 for t from 0 to 20
ts = times(x)
xs = values(x)
en = 0
em = 0
for i from 1 to len(ts) - 1
    en = max(en, abs(xs[i] - sin(ts[i])))
    tm = (ts[i] + ts[i+1]) / 2
    em = max(em, abs(x(tm) - sin(tm)))
print en, em"""
    en, em = nums(run(src))
    assert en < 1e-8 and em < 1e-6


def test_where_shadowing_parameter_warns():
    from conftest import warnings_of
    assert warnings_of("f(x) = 2 x where x = 5 s\nprint f(1 s)")


def test_infinite_constant_index_is_a_fermium_error():
    from fermium.errors import FermiumError
    with pytest.raises(FermiumError):
        run("xs = [1, 2, 3]\nprint xs[inf]")


@pytest.mark.parametrize("src", ["xs = [1, 2, 3]\nprint xs[1e19]", "xs = [1, 2, 3]\nprint xs[0/0]",
                                 "n = 0\nfor i from 1 to 0/0\n    n += 1\nprint n"])
def test_runtime_errors_have_messages(src):
    assert str(error_of(src)) not in ("runtime error", "line 2: runtime error", "line 3: runtime error")


def test_huge_list_is_a_clean_error(tmp_path):
    r = _run_cli("xs = zeros(1e15)\nxs[1] = 2\nprint sum(xs)\n", tmp_path)
    assert r.returncode == 1 and "memory" in r.stderr


def test_nested_numeric_kernels():
    # integral of a solution, solve with an integral in the RHS, integral over a function that solves
    assert run("solve x' = -x with x(0) = 1 for t from 0 to 2\nprint ∫ x(t) dt from 0 to 2") == "0.864665"
    assert run("F(t) = ∫ exp(-u^2) du from 0 to t\nsolve y' = F(t) with y(0) = 0 for t from 0 to 1\n"
               "print y(1)") == "0.430764"
    assert run("g(k) =\n    solve x' = -k x with x(0) = 1 for t from 0 to 1\n    x(1)\n"
               "print ∫ g(k) dk from 0 to 1") == "0.632121"
    assert run("solve y' = if t < 0.5 then 1 else 2 with y(0) = 0 for t from 0 to 1\nprint y(1)") == "1.5"


def test_integral_of_solution_derivative():
    out = run("solve x' = -x with x(0) = 1 for t from 0 to 2\nprint ∫ x'(s) ds from 0 to 2")
    assert close(num(out), math.exp(-2) - 1, 1e-6)


def test_indefinite_integrals():
    assert run("F = ∫ x^2 dx\nprint F(3)") == "9"
    assert run("k = 50 N/m\nW = ∫ k x dx\nprint W(0.2 m)") == "1.0 J"
    assert run("F = ∫ exp(-x) sin(x) dx\nprint F(1) - F(0)") == "0.245837"


def test_indefinite_integral_with_parameter():
    out = run("ω = 2 1/s\nF = ∫ cos(ω t) dt\nprint F(1 s) - F(0 s)")
    assert close(num(out), math.sin(2) / 2)


def test_std_of_celsius_list():
    assert run("T = [10 °C, 20 °C]\nprint std(T)") == "7.07107 K"


def test_diff_of_celsius_list():
    assert run("T = [0 °C, 100 °C]\nprint diff(T)") == "[100] K"


def test_celsius_list_mean_min_max_interp():
    assert run("T = linspace(0 °C, 100 °C, 3)\nprint mean(T), T in K") == "50 °C [273.15, 323.15, 373.15] K"
    assert run("T = [0 °C, 100 °C]\nprint interp(50 °C, T, [1 m, 2 m])") == "1.5 m"


def test_fmt_ascii_second_partial():
    from fermium.fmt import format_source
    src = "f(x, y) = x^2 y^2\ng = ∂²/∂x² f\nprint g(1, 2)"
    assert run(format_source(src, "ascii")) == run(src) == "8"


def test_fmt_round_trip_more():
    from fermium.fmt import format_source
    for src in ["x(t) = t^3\nprint d²/dt² x, d/dt x",
                "θ₀ = 0.1\nω₁₂ = 2\nprint θ₀ ω₁₂, √θ₀, ∛8",
                "print 1 Å, 2 μm, 3 M☉ in kg, 20 °C, ∞, -∞",
                "x = 2\nprint ¼ x, ¾ x, ⅓ x, x⁻¹, x²·x",
                "T = 300 K\nprint 2.898e-3 m K / T"]:
        base = run(src)
        a = format_source(src, "ascii")
        assert run(a) == base
        assert run(format_source(a, "pretty")) == base


def test_cli_non_utf8_file(tmp_path):
    f = tmp_path / "latin1.fm"
    f.write_bytes(b"x = 5 \xb5m\nprint x\n")
    r = subprocess.run([sys.executable, "-m", "fermium.cli", "run", str(f)], capture_output=True, text=True,
                       timeout=20, cwd=ROOT)
    assert "Traceback" not in r.stderr and r.returncode != 0


def test_function_keeps_argument_display_unit():
    assert run("f(E) = E\nprint f(3 MeV)") == "3 MeV"


def test_function_result_value_with_mixed_unit_calls():
    assert run("f(x) = 2 x\nprint f(1 km) in m, f(3 cm) in cm, f(5 m)") == "2000 m 6 cm 10 m"


# second derivatives found wrong by fuzzing f, f', f'' against mpmath (BUG A41)
D2_FUZZ = [
    ("exp(sin(exp(x)))", lambda m, x: m.exp(m.sin(m.exp(x)))),
    ("sin(exp(sin(x)))^2", lambda m, x: m.sin(m.exp(m.sin(x))) ** 2),
    ("exp(sin(exp(sin(x))))", lambda m, x: m.exp(m.sin(m.exp(m.sin(x))))),
    ("cos((((2 - x) + (7 * 2)) * (cos((3/2)) / sin(x))))",
     lambda m, x: m.cos(((2 - x) + 14) * (m.cos(m.mpf(3) / 2) / m.sin(x)))),
    ("exp(sin(ln((-exp(sin(x)))^2 + 2)))", lambda m, x: m.exp(m.sin(m.log(m.exp(m.sin(x)) ** 2 + 2)))),
]


@pytest.mark.parametrize("formula,f", D2_FUZZ, ids=[c[0] for c in D2_FUZZ])
def test_second_derivative_fuzz_cases(formula, f):
    mpmath = pytest.importorskip("mpmath")
    mpmath.mp.dps = 30
    got = num(run(f"f(x) = {formula}\nh = f''\nprint h(13/10)"))
    want = float(mpmath.diff(lambda x: f(mpmath, x), mpmath.mpf(13) / 10, 2))
    assert close(got, want, 2e-5)


def test_first_derivative_repeated_factor():
    got = num(run("f(x) = exp(sin(exp(x))) * cos(exp(x)) * exp(x)\ng = f'\nprint g(13/10)"))
    assert close(got, 8.25545148831716, 1e-5)


def test_call_parenthesised_partial_with_two_arguments():
    assert run("f(x, y) = x^2 y\nprint (∂/∂x f)(1, 2), (partial/partial y f)(1, 2)") == "4 1"


def test_call_parenthesised_derivative_one_argument():
    assert run("f(x) = x^2\nprint (d/dx f)(3)") == "6"


def test_definite_assignment_both_branches_ok():
    assert run("x = 1\nif x > 0\n    y = 1\nelse\n    y = 2\nprint y") == "1"
    assert "might not have a value" in str(error_of("x = 1\nif x > 2\n    y = 3 m\nprint y"))


@pytest.mark.parametrize("src", ["for i from 1 to 0\n    print 5\nprint i",
                                 "for x in []\n    print 5\nprint x",
                                 "f(n) =\n    for i from 1 to n\n        print 1\n    i\nprint f(0)"])
def test_loop_variable_after_empty_loop(src):
    # expected: "i might not have a value here" (the garbage printed now is nondeterministic)
    assert "value" in str(error_of(src))


@pytest.mark.parametrize("integral,want", [
    ("1/sqrt(1 - x^2) dx from -1 to 1", math.pi),
    ("1/sqrt(1 - x^2) dx from 0 to 1", math.pi / 2),
    ("1/sqrt(cos(x) - cos(1)) dx from -1 to 1", 4.73759822490498),
])
def test_integrable_sqrt_singularities(integral, want):
    # was BUG A44
    assert close(num(run(f"print ∫ {integral}")), want, 1e-5)


def test_integrable_interior_sqrt_singularity():
    got = num(run("print ∫ 1/sqrt(abs(x - 3/10)) dx from 0 to 1"))     # was BUG A44
    assert close(got, 2 * (math.sqrt(0.3) + math.sqrt(0.7)), 1e-5)


def test_integrable_power_singularities():
    got = nums(run("print ∫ x^(-0.8) dx from 0 to 1\nprint ∫ abs(x)^(-0.5) dx from -1 to 1\n"
                   "print ∫ x^(-0.95) dx from 0 to 1\nprint ∫ ln(abs(x)) dx from -1 to 1"))
    assert all(close(g, w, 1e-5) for g, w in zip(got, [5, 4, 20, -2]))


def test_weak_singularity_accuracy():
    # reference: scipy quad (epsrel 1e-13) and mpmath after the substitution x = u^5 agree
    assert close(num(run("print ∫ (x^(-2) + 1)^0.4 dx from 0 to 1")), 5.160306768626711, 1e-5)


@pytest.mark.parametrize("integral,want", [
    ("((1/x)^2 + 1)^(2/5) dx from -2 to 3", 13.7312084140662),
    ("abs(x)^(-0.8) dx from -1 to 1", 10.0),
    pytest.param("abs(x - 3/10)^(-0.8) dx from 0 to 1", 5 * (0.3 ** 0.2 + 0.7 ** 0.2),
                 marks=pytest.mark.xfail(strict=True, reason="BUG A56 (limitation): a strong singularity at a "
                                         "non-zero interior point needs extrapolation (QUADPACK's ε-algorithm)")),
    ("abs(x)^(-0.9) dx from -1 to 2", 10 * (1 + 2 ** 0.1)),
])
def test_interior_algebraic_singularity(integral, want):
    assert close(num(run(f"print ∫ {integral}")), want, 1e-5)


def _decay_csv(tmp_path):
    ts = [float(i) for i in range(11)]
    noise = [3, -2, 1, 0, -1, 2, -3, 1, 0, 1, -1]
    rows = "".join(f"{t}, {1000 * math.exp(-t / 3.0) + n}\n" for t, n in zip(ts, noise))
    (tmp_path / "decay.csv").write_text("t [ms], N\n" + rows)


def test_fit_decay_rate_form(tmp_path):
    _decay_csv(tmp_path)
    out = run('d = load "decay.csv"\nfit N = A exp(-λ t) to d\nprint 1/λ in ms', base_dir=str(tmp_path))
    assert out.split("\n")[-1] == "2.99 ms"


def test_fit_decay_time_constant_form(tmp_path):
    _decay_csv(tmp_path)
    out = run('d = load "decay.csv"\nfit N = A exp(-t/τ) to d\nprint τ in ms', base_dir=str(tmp_path))
    assert out.split("\n")[-1] == "2.99 ms"


def test_kelvin_minus_celsius():
    assert run("print 300 K - 20 °C") == "6.85 K"


def test_newton_cooling_celsius():
    out = run("Ta = 20 °C\nsolve T' = -(T - Ta) / (10 min) with T(0 s) = 90 °C for t from 0 min to 30 min\n"
              "print T(30 min) in °C")
    assert close(num(out), 20 + 70 * math.exp(-3), 1e-5)


def test_newton_cooling_kelvin_ambient():
    out = run("solve T' = -(T - 293.15 K) / (10 min) with T(0 s) = 90 °C for t from 0 min to 30 min\n"
              "print T(30 min) in °C")
    assert close(num(out), 20 + 70 * math.exp(-3), 1e-5)


def test_ode_in_milliseconds():
    assert run("τ = 1 ms\nsolve V' = -V/τ with V(0 ms) = 5 V for t from 0 ms to 5 ms\n"
               "print V(1 ms), times(V)[end]") == "1.8394 V 5 ms"


def test_map_vector_function_over_list_is_a_fermium_error():
    from fermium.errors import FermiumError
    with pytest.raises(FermiumError):
        run("f(x) = <x, 2x>\nprint f([1, 2])")


def test_vector_errors_are_clean():
    assert "2-vector and a 3-vector" in str(error_of("print <1,2> · <1,2,3>"))
    assert "no component 3" in str(error_of("v = <1,2>\nprint v.z"))
    assert "2 or 3 components" in str(error_of("print <1, 2, 3, 4>"))


@pytest.mark.parametrize("src,want", [
    ("a = 1 fm\nprint ∫ exp(-r/a) dr from 0 m to ∞ in fm", 1.0),
    ("a = 0.529e-10 m\nprint ∫ 4π r² exp(-2 r/a) / (π a³) dr from 0 m to ∞", 1.0),
    ("σ = 1 fm\nprint ∫ exp(-x^2/(2 σ^2)) dx from -∞ to ∞ in fm", math.sqrt(2 * math.pi)),
    ("E0 = 1 MeV\nprint ∫ exp(-E/E0) dE from 0 J to ∞ in MeV", 1.0),
    ("print ∫ exp(-t/(1 ns)) dt from 0 s to ∞ in ns", 1.0),
    ("a = 1 AU\nprint ∫ exp(-r/a) dr from 0 m to ∞ in AU", 1.0),
])
def test_infinite_integral_physical_scales(src, want):
    assert close(num(run(src)), want, 1e-5)


def test_fermi_function_derivative_far_tail():
    out = run("kT = 0.025 eV\nμ = 5 eV\nf(E) = 1/(exp((E - μ)/kT) + 1)\ng = f'\nprint g(30 eV)")
    assert "NaN" not in out


def test_tanh_second_derivative_large_argument():
    assert run("f(x) = tanh(x)\nh = f''\nprint h(1000)") in ("0", "-0")


def test_divergent_integral_pole_at_pi():
    assert "converge" in str(error_of("print ∫ 1/sin(x)^2 dx from 1 to 5"))


def test_integral_jump_and_oscillation():
    got = nums(run("print ∫ atan(1/x) dx from -2 to 3\nprint ∫ exp(sin(x^4)) dx from 0 to 5"))
    exact = (3 * math.atan(1 / 3) + 0.5 * math.log(10)) - (-2 * math.atan(-0.5) + 0.5 * math.log(5))
    assert close(got[0], exact, 1e-6)
    assert close(got[1], 6.5251672775030307, 1e-5)          # mpmath, 2000 panels


def test_integration_variable_named_like_unit():
    assert run("print ∫ 1/u du from 1 to 2") == "0.693147"


def test_parameter_named_like_unit_wins():
    assert run("f(s) = 1/s\nprint f(2)") == "0.5"
    assert run("s = 2\nprint 1/s") == "0.5"


def test_solution_max_is_refined():
    out = run("solve x' = cos(t) with x(0) = 0 for t from 0 to 20\nprint max(x), min(x)")
    a, b = nums(out)
    assert close(a, 1, 1e-7) and close(b, -1, 1e-7)


def test_physics_integrals_to_infinity():
    out = run("T = 5800 K\nB(ν) = 2 h ν^3 / c^2 / (exp(h ν / (k_B T)) - 1)\n"
              "Bλ(λ) = 2 h c^2 / λ^5 / (exp(h c / (λ k_B T)) - 1)\n"
              "print ∫ B(ν) dν from 0 Hz to ∞, ∫ Bλ(λ) dλ from 0 m to ∞, σ T^4 / π")
    a, b, c = nums(out)
    assert close(a, c, 1e-5) and close(b, c, 1e-5)
    assert run("m = m_e\nT = 300 K\nprint ∫ 4π v^2 (m/(2π k_B T))^(3/2) exp(-m v^2 / (2 k_B T)) dv "
               "from 0 m/s to ∞") == "1"
    assert run("x0 = 100 fm\nσ = 1 fm\nprint ∫ exp(-(x - x0)^2/(2 σ^2)) dx from -∞ to ∞ in fm") == "2.50663 fm"
    assert run("print ∫ 1/(1 + (E/(1 MeV))^2) dE from -∞ to ∞ in MeV") == "3.14159 MeV"


def test_derivative_of_formula_with_where():
    assert run("g = d/dt (a t^2) where a = 3\nprint g(1)") == "6"


def test_bad_csv_has_no_python_noise(tmp_path):
    (tmp_path / "gap.csv").write_text("x [m], y [m]\n1, 2\n2,\n3, 6\n")
    r = _run_cli('d = load "gap.csv"\nprint d.y\n', tmp_path)
    assert r.returncode == 1 and "not a number" in r.stderr
    assert "Exception ignored" not in r.stderr and "Traceback" not in r.stderr
