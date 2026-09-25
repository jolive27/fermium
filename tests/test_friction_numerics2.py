"""Second-pass gauntlet frictions in the numerical kernels (gauntlet/FRICTION.md #42, #44, #45, #46, #48).

Every program runs compiled (LLVM) and in the reference interpreter, and the two must agree; answers are
compared with SciPy (quad, solve_ivp) where that helps."""
import io
import math

import numpy as np
import pytest
from scipy.integrate import quad, solve_ivp
from scipy.special import sici

from conftest import run, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    return o.getvalue().strip()


def both(src):
    out = run(src)
    assert interp(src) == out
    return out


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    assert native.value.line == ref.value.line
    return native.value


def nums(text):
    out = []
    for tok in text.replace("<", " ").replace(">", " ").replace(",", " ").split():
        tok = tok.replace("×10", "e").translate(str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-"))
        try:
            out.append(float(tok))
        except ValueError:
            pass
    return out


# ---------------------------------------------------------------- #46: integrals that are 0 by symmetry
@pytest.mark.parametrize("integrand, lo, hi", [
    ("sin(x)", "-1", "1"),
    ("sin(x)", "0", "2π"),
    ("x^3 exp(-x^2)", "-3", "3"),
    ("x cos(x) exp(-x^2)", "-∞", "∞"),
    ("sin(3 x) cos(5 x)", "0", "2π"),
])
def test_46_zero_by_symmetry_converges(integrand, lo, hi):
    v = nums(both(f"print ∫ {integrand} dx from {lo} to {hi}"))[0]
    assert abs(v) < 1e-13


def test_46_vector_integral_with_a_zero_component_E10():
    # the field of a charged disk off axis: the y component is 0 by symmetry, and the inner integrals
    # leave only rounding noise in it (it used to be "doesn't converge")
    src = ("a = 10.0 cm\nP = <6.00, 0, 4.00> cm\n"
           "print ∫ (∫ s (P - <s cos(φ), s sin(φ), 0 m>) / |P - <s cos(φ), s sin(φ), 0 m>|^3 dφ from 0 to 2π) "
           "ds from 0 m to a to 9 digits")
    x, y, z = nums(run(src))           # (compiled only: the nested vector integral is slow in the interpreter)
    P = np.array([0.06, 0.0, 0.04])

    def comp(k):
        def inner(s):
            f = lambda p: s * (P - [s * math.cos(p), s * math.sin(p), 0])[k] / np.linalg.norm(  # noqa: E731
                P - [s * math.cos(p), s * math.sin(p), 0]) ** 3
            return quad(f, 0, 2 * math.pi, epsabs=0, epsrel=1e-12, limit=200)[0]
        return quad(inner, 0, 0.1, epsabs=0, epsrel=1e-11, limit=200)[0]
    assert x == pytest.approx(comp(0), rel=1e-8)
    assert z == pytest.approx(comp(2), rel=1e-8)
    assert abs(y) < 1e-12 * abs(x)


def test_46_nested_vector_integral_with_a_zero_component():
    # (compiled only: the quiet first try of the noisy component takes ~40 s in the interpreter)
    src = "print ∫ (∫ <s, s sin(φ)> dφ from 0 to 2π) ds from 0 to 1 to 13 digits"
    x, y = nums(run(src))
    assert x == pytest.approx(math.pi, rel=1e-12) and abs(y) < 1e-13


def test_46_cancelling_but_nonzero_integral_keeps_its_relative_accuracy():
    # ∫|f| is 100× the result: the result must still be accurate relative to itself
    v = nums(both("print ∫ sin(x) + 0.01 dx from 0 to 20π to 12 digits"))[0]
    assert v == pytest.approx(0.2 * math.pi, rel=1e-10)


@pytest.mark.parametrize("src", ["print ∫ 1/x dx from 0 to 1", "print ∫ 1/x dx from 1 to ∞",
                                 "print ∫ sin(x) dx from 0 to ∞", "print ∫ 1/x^2 dx from -1 to 1",
                                 "print ∫ 1/(x-0.3) dx from 0 to 1"])
def test_46_divergent_integrals_are_still_reported(src):
    assert "doesn't converge" in both_error(src).message


# ---------------------------------------------------------------- #45: NaN / ∞ in the integrand
def test_45_nan_at_one_node_counts_as_a_point():
    # sin(x - x0)/(x - x0) with x0 exactly on a quadrature node: 0/0 there, finite everywhere else
    x0 = 0.40673828125
    v = nums(both(f"x0 = {x0!r}\nprint ∫ sin(x - x0) / (x - x0) dx from 0 to 1 to 12 digits"))[0]
    assert v == pytest.approx(sici(1 - x0)[0] + sici(x0)[0], rel=1e-11)


def test_45_overflow_to_nan_is_reported_with_the_variable_T10():
    # x⁴ eˣ/(eˣ - 1)² is ∞/∞ above x ≈ 710: say so, on the integral's line, instead of "NaN ± NaN"
    src = "x_D = 800\nprint 1\nD = ∫ x^4 exp(x) / (exp(x) - 1)^2 dx from 0 to x_D\nprint D"
    e = both_error(src)
    assert e.line == 3
    assert e.message.startswith("the integrand is NaN at x = ")
    assert float(e.message.split("x = ")[1].split()[0]) > 709
    assert "exp(-x) / (1 - exp(-x))²" in e.message


def test_45_nan_message_names_the_variable_and_its_units():
    src = "f(T) = ∫ exp(u / (1 K)) / (exp(u / (1 K)) - 1) du from 1 K to T\nprint f(900 K)"
    e = both_error(src)
    assert e.line == 1 and "the integrand is NaN at u = " in e.message and " K (" in e.message


def test_45_the_overflow_safe_form_works():
    v = nums(both("print ∫ x^4 exp(-x) / (1 - exp(-x))^2 dx from 0 to 800 to 10 digits"))[0]
    assert v == pytest.approx(4 * math.pi ** 4 / 15, rel=1e-9)


def test_45_cancellation_near_zero_is_reported_not_silently_wrong_M11():
    # dT/dε of the brachistochrone with y = a(1 - cos θ): 1 - cos θ is 0 below θ ≈ 1e-8, so the integrand
    # is 1/0 there over a range of θ, not at a point.  An error pointing there, not a wrong number.
    src = """X = 2.00 m
Y = 1.00 m
g = 9.81 m/s²
solve (θf - sin(θf)) / (1 - cos(θf)) = X / Y for θf from 0.1 to 2π - 0.1
a = Y / (1 - cos(θf))
xc(θ) = a (θ - sin(θ))
yc(θ) = a (1 - cos(θ))
T(ε) = ∫ √(xc'(θ)² + (yc'(θ) + ε a 2 sin(π θ / θf) cos(π θ / θf) π / θf)²) / √(2*g (yc(θ) + ε a sin(π θ / θf)²)) dθ from 0 to θf
dT = d/dε T
print dT(0)"""
    e = both_error(src)
    assert e.line == 8 and "infinite at θ = " in e.message
    assert float(e.message.split("θ = ")[1].split("×")[0].split()[0]) < 1e-7 or "×10⁻" in e.message
    assert "2 sin(x/2)²" in e.message


def test_45_isolated_nan_away_from_nodes_still_matches_scipy():
    v = nums(both("print ∫ (1 - cos(θ)) / θ^2 dθ from 0 to 3 to 12 digits"))[0]
    assert v == pytest.approx(quad(lambda t: (1 - math.cos(t)) / t ** 2 if t else 0.5, 0, 3,
                                   epsabs=0, epsrel=1e-13)[0], rel=1e-10)


# ---------------------------------------------------------------- #42: x'(t) of a first-order unknown
def test_42_derivative_of_a_first_order_unknown_is_as_accurate_as_the_solution():
    out = both("k = 1.5 / (1 s)\nsolve y' = k y\n  with y(0 s) = 1\n  for t from 0 s to 2 s tolerance 1e-11\n"
               "print y'(1.3 s) to 14 digits")
    assert nums(out)[0] == pytest.approx(1.5 * math.exp(1.5 * 1.3), rel=1e-9)


def test_42_S9_relativistic_velocity_matches_the_right_side():
    # S9: a charge in a uniform field; r'(t) used to be the Hermite interpolant's derivative (1e-6 off)
    src = """m_p = 1.67262192e-27 kg
c0 = 2.99792458e8 m/s
q = 1.602176634e-19 C
E = <0, 1e6, 0> V/m
energy(p) = √(m_p² c0⁴ + |p|² c0²)
solve p' = q E, r' = p c0² / energy(p)
  with p(0 ns) = <4e-19, 0, 0> kg m/s, r(0 ns) = <0, 0, 0> m
  for t from 0 ns to 10.0 ns tolerance 1e-11
v = r'(5 ns)
w = p(5 ns) c0² / energy(p(5 ns))
print |v - w| / |w|"""
    assert nums(both(src))[0] < 1e-10


def test_42_second_derivative_of_a_second_order_unknown_uses_the_equation():
    x1, x2 = nums(both("solve x'' = -x / (1 s²)\n  with x(0 s) = 1 m, x'(0 s) = 0 m/s\n"
                       "  for t from 0 s to 3 s tolerance 1e-11\nprint x''(2 s) to 12 digits, x(2 s) to 12 digits"))
    assert x1 == pytest.approx(-x2, rel=1e-14)
    assert x1 == pytest.approx(-math.cos(2), rel=1e-9)


def test_42_derivative_uses_the_values_at_the_time_of_the_solve():
    # the right side reads k; changing k afterwards must not change the solution's derivative
    out = both("k = 1.5 / (1 s)\nsolve y' = k y\n  with y(0 s) = 1\n  for t from 0 s to 2 s\n"
               "k = 7 / (1 s)\nprint y'(1 s) / y(1 s)")
    assert nums(out)[0] == pytest.approx(1.5, rel=1e-12)


def test_42_derivative_of_a_solution_made_inside_a_function():
    out = both("f(k) =\n    solve y' = -k y\n      with y(0 s) = 2\n      for t from 0 s to 1 s\n"
               "    return y'(0.5 s) / y(0.5 s)\nprint f(3 / (1 s))")
    assert nums(out)[0] == pytest.approx(-3, rel=1e-12)


def test_42_rk4_and_until_solutions_too():
    out = both("solve y' = -y / (1 s)\n  with y(0 s) = 1\n  for t from 0 s to 2 s step 0.01 s\n"
               "print y'(0.505 s) / y(0.505 s)\n"
               "solve h'' = -9.81 m/s²\n  with h(0 s) = 10 m, h'(0 s) = 0 m/s\n  for t from 0 s to 5 s\n"
               "  until h = 0 m\nprint h''(1 s)")
    r, a = nums(out)
    assert r == pytest.approx(-1, rel=1e-12) and a == pytest.approx(-9.81, rel=1e-12)


# ---------------------------------------------------------------- #44: several highest derivatives
def test_44_M9_mass_matrix_form():
    out = both("""solve a'' + 0.5 b'' = -a / (1 s²),
      b'' + 0.5 a'' = -b / (1 s²)
  with a(0 s) = 1 m, b(0 s) = 0 m, a'(0 s) = 0 m/s, b'(0 s) = 0 m/s
  for t from 0 s to 1 s tolerance 1e-11
print a(1 s) to 10 digits, b(1 s) to 10 digits""")
    Mi = np.linalg.inv([[1, 0.5], [0.5, 1]])
    ref = solve_ivp(lambda t, y: [y[2], y[3], *(Mi @ -y[:2])], (0, 1), [1, 0, 0, 0], rtol=1e-12, atol=1e-14)
    assert nums(out) == pytest.approx(ref.y[:2, -1], rel=1e-8)


def test_44_M9_no_barn_warning_for_the_unknown_b():
    src = ("solve a'' + 0.5 b'' = -a / (1 s²), b'' + 0.5 a'' = -b / (1 s²)\n"
           "  with a(0 s) = 1 m, b(0 s) = 0 m, a'(0 s) = 0 m/s, b'(0 s) = 0 m/s\n  for t from 0 s to 1 s\n")
    assert not [w for w in warnings_of(src) if "barn" in w]


DOUBLE_PENDULUM = """m1 = 1.0 kg
m2 = 0.5 kg
l1 = 1.0 m
l2 = 0.7 m
g = 9.81 m/s²
solve (m1 + m2) l1 θ1'' + m2 l2 θ2'' cos(θ1 - θ2) + m2 l2 θ2'² sin(θ1 - θ2) + (m1 + m2) g sin(θ1) = 0 N,
      l2 θ2'' + l1 θ1'' cos(θ1 - θ2) - l1 θ1'² sin(θ1 - θ2) + g sin(θ2) = 0 m/s²
  with θ1(0 s) = 1.2, θ2(0 s) = -0.5, θ1'(0 s) = 0 /s, θ2'(0 s) = 0 /s
  for t from 0 s to 5 s tolerance 1e-11
print θ1(5 s) to 9 digits, θ2(5 s) to 9 digits, θ1''(2 s) to 9 digits
"""


def test_44_double_pendulum_lagrange_equations_match_scipy():
    m1, m2, l1, l2, g = 1, .5, 1, .7, 9.81

    def acc(y):
        a, b, ad, bd = y
        d = a - b
        M = [[(m1 + m2) * l1, m2 * l2 * math.cos(d)], [l1 * math.cos(d), l2]]
        r = [-m2 * l2 * bd ** 2 * math.sin(d) - (m1 + m2) * g * math.sin(a), l1 * ad ** 2 * math.sin(d) - g * math.sin(b)]
        return np.linalg.solve(M, r)
    ref = solve_ivp(lambda t, y: [y[2], y[3], *acc(y)], (0, 5), [1.2, -.5, 0, 0], rtol=1e-12, atol=1e-13,
                    dense_output=True)
    a, b, a2 = nums(both(DOUBLE_PENDULUM))
    assert [a, b] == pytest.approx(ref.y[:2, -1], abs=2e-7)
    assert a2 == pytest.approx(acc(ref.sol(2.0))[0], rel=1e-7)


def test_44_first_order_coupled_and_three_unknowns():
    out = both("""solve x' + y' = -x / (1 s), x' - y' = -y / (1 s), 2 z' + x' = 0 m/s
  with x(0 s) = 1 m, y(0 s) = 0 m, z(0 s) = 0 m
  for t from 0 s to 1 s tolerance 1e-11
print x(1 s) to 10 digits, y(1 s) to 10 digits, z(1 s) to 10 digits""")
    M = np.array([[1, 1, 0], [1, -1, 0], [1, 0, 2]], float)

    def f(t, u):
        return np.linalg.solve(M, [-u[0], -u[1], 0])
    ref = solve_ivp(f, (0, 1), [1, 0, 0], rtol=1e-12, atol=1e-14)
    assert nums(out) == pytest.approx(ref.y[:, -1], rel=1e-8, abs=1e-10)


def test_44_nonlinear_in_the_highest_derivatives_is_a_clear_error():
    e = both_error("solve a''² / (1 m/s²) + b'' = -a / (1 s²), b'' + a'' = 0 m/s²\n"
                   "  with a(0 s) = 1 m, b(0 s) = 0 m, a'(0 s) = 0 m/s, b'(0 s) = 0 m/s\n  for t from 0 s to 1 s")
    assert "a'' and b'' both appear in one equation" in e.message and "linear in a'', b''" in e.message
    assert "prime" not in e.message


def test_44_vector_unknown_in_a_coupled_equation_is_a_clear_error():
    e = both_error("solve r'' + s'' = <0, 0> m/s², s'' = <1, 0> m/s²\n"
                   "  with r(0 s) = <0, 0> m, r'(0 s) = <0, 0> m/s, s(0 s) = <0, 0> m, s'(0 s) = <0, 0> m/s\n"
                   "  for t from 0 s to 1 s")
    assert "r'' and s'' both appear in one equation" in e.message and "r is a vector" in e.message


def test_44_singular_mass_matrix_is_an_ode_error():
    e = both_error("m = 1 kg\nsolve m a'' + m c'' = 1 N, a'' + c'' = 2 m/s²\n"
                   "  with a(0 s) = 0 m, c(0 s) = 0 m, a'(0 s) = 0 m/s, c'(0 s) = 0 m/s\n  for t from 0 s to 1 s")
    assert e.message.startswith("the equations don't determine a'' and c'' at t = 0 s")
    assert "singular" in e.message


# ---------------------------------------------------------------- #48: solutions inside functions
def test_48_Q16_root_of_a_solution_inside_a_function():
    out = both("f(k) =\n    solve y' = k y\n      with y(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
               "    solve y(T) = 2 for T from 0 s to 1 s\n    return T\nprint f(1 / (1 s)) to 9 digits")
    assert nums(out)[0] == pytest.approx(math.log(2), rel=1e-8)


def test_48_integral_of_a_solution_inside_a_function():
    out = both("g(k) =\n    solve u' = -k u\n      with u(0 s) = 1\n      for t from 0 s to 2 s tolerance 1e-12\n"
               "    return ∫ u(t) dt from 0 s to 2 s\nprint g(1 / (1 s)) to 9 digits\n"
               "h(k) =\n    solve u' = -k u\n      with u(0 s) = 1\n      for t from 0 s to 1 s tolerance 1e-12\n"
               "    return ∫ (∫ u(t) u(τ) dτ from 0 s to t) dt from 0 s to 1 s\nprint h(1 / (1 s)) to 9 digits")
    a, b = nums(out)
    assert a == pytest.approx(1 - math.exp(-2), rel=1e-8)
    assert b == pytest.approx((1 - math.exp(-1)) ** 2 / 2, rel=1e-8)


def test_48_Q16_ratio_of_integrals_like_the_quantum_problem():
    out = both("expect(E) =\n    solve u'' = -E u / (1 m²)\n      with u(0 m) = 0, u'(0 m) = 1 / (1 m)\n"
               "      for r from 0 m to 1 m\n"
               "    return ∫ r u(r)² dr from 0 m to 1 m / ∫ u(r)² dr from 0 m to 1 m\nprint expect(π²) to 8 digits")
    assert nums(out)[0] == pytest.approx(0.5, rel=1e-7)


def test_48_returning_a_solution_speaks_in_the_users_names():
    e = both_error("h(k) =\n    solve u' = -k u\n      with u(0 s) = 1\n      for t from 0 s to 1 s\n"
                   "    return u\nprint h(1 / (1 s))")
    assert "can't return the ODE solution u" in e.message and "__sol" not in e.message
