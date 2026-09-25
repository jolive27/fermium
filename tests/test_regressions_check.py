"""Regression tests for adversarial bugs fixed in the checker, parser and fitting
(A16, A19, A31, A32, A40, A42, A45, A46, A47, A48 in notes/bugs-adversarial.md)."""
import math

from conftest import run, error_of, warnings_of


def num(s):
    return float(s.split()[0].replace("×10", "e").translate(str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789")))


# A16 -- v[i] = ... alone makes v a list parameter
def test_index_assignment_makes_list_parameter():
    assert run("f(v) =\n    v[1] = 42\n    0\nxs = [1, 2, 3]\nprint f(xs), xs") == "0 [42, 2, 3]"


def test_index_compound_assignment_makes_list_parameter():
    assert run("bump(v) =\n    v[end] += 1\n    0\nxs = [1, 2]\nprint bump(xs), xs") == "0 [1, 3]"


def test_scalar_function_still_maps_over_list():
    assert run("f(x) = 2 x\nprint f([1, 2])") == "[2, 4]"


# A31 -- a where-binding hiding a parameter
def test_where_shadowing_parameter_warns():
    w = warnings_of("f(x) = 2 x where x = 5 s\nprint f(1 s)")
    assert any("hides the parameter x" in m for m in w)


def test_where_shadowing_in_multiline_function_warns():
    w = warnings_of("f(x) =\n    y = x + a where a = 1\n    y where x = 2\nprint f(1)")
    assert any("hides the parameter x" in m for m in w)


def test_where_without_shadowing_does_not_warn():
    assert not warnings_of("f(x) = k x where k = 5\nprint f(1)")


# A32 -- non-finite constant index
def test_infinite_constant_index_is_an_error():
    msg = str(error_of("xs = [1, 2, 3]\nprint xs[inf]"))
    assert "line 2" in msg and "whole number" in msg and "∞" in msg


# A40 -- the argument's display unit survives a function call
def test_function_keeps_argument_display_unit():
    assert run("f(E) = E\nprint f(3 MeV)") == "3 MeV"
    assert run("f(x) = 2 x\nprint 2 * (1 km), f(1 km)") == "2 km 2 km"


def test_function_body_unit_wins_over_argument_unit():
    assert run("f(x) = x + 1 m\nprint f(1 km)") == "1001 m"


def test_function_argument_unit_needs_same_dimension():
    assert run("f(m) = m c^2\nprint f(1 u) in MeV") == "931.494 MeV"
    assert run("f(x) = 2 x\nprint f(1 km) in m, f(3 cm) in cm, f(5 m)") == "2000 m 6 cm 10 m"


def test_function_of_celsius_argument_is_not_shown_in_celsius():
    # T - Ta is a temperature difference: shown in K, not in the argument's °C
    assert run("Ta = 20 °C\nf(T) = T - Ta\nprint f(30 °C)") == "10 K"


# A42 -- (∂/∂x f)(a, b) is a call
def test_call_parenthesised_partial_with_two_arguments():
    assert run("f(x, y) = x^2 y\nprint (∂/∂x f)(1, 2), (partial/partial y f)(1, 2)") == "4 1"


def test_parenthesised_products_still_multiply():
    assert run("x = 3\nprint (x + 1)(x - 1)") == "8"
    assert run("print (d/dx x^2)(3)") == "6"


# A47 -- K minus °C is a temperature difference
def test_kelvin_minus_celsius():
    assert run("print 300 K - 20 °C") == "6.85 K"


def test_newton_cooling_celsius_ambient():
    out = run("Ta = 20 °C\nsolve T' = -(T - Ta) / (10 min) with T(0 s) = 90 °C for t from 0 min to 30 min\n"
              "print T(30 min) in °C")
    assert abs(num(out) - (20 + 70 * math.exp(-3))) < 1e-4


def test_celsius_plus_celsius_still_rejected():
    assert "absolute temperatures" in str(error_of("print 20 °C + 10 °C"))


# A48 -- a vector-valued function mapped over a list
def test_map_vector_function_over_list_is_a_fermium_error():
    msg = str(error_of("f(x) = <x, 2x>\nprint f([1, 2])"))
    assert "line 2" in msg and "vector" in msg


# A19 -- vector ODE solutions: r''(t) and r[end]
def test_vector_solution_second_derivative():
    src = "solve r'' = -r with r(0) = <1, 0>, r'(0) = <0, 1> for t from 0 to 3\nprint r''(3)"
    assert run(src) == run("print <-cos(3), -sin(3)>")


def test_vector_solution_first_order_derivative():
    src = "solve r' = <-r.y, r.x> / (1 s) with r(0) = <1, 0> for t from 0 s to 3 s\nprint r'(3 s)"
    assert run(src) == run("print <-sin(3), cos(3)> / (1 s)")


def test_vector_solution_index():
    src = ("solve r'' = -r/(1 s)^2 with r(0) = <1, 0> m, r'(0) = <0, 1> m/s for t from 0 s to 3 s\n"
           "print r[end]\nprint r[1], r'[end]")
    last, rest = run(src).split("\n")
    assert last == run("print <cos(3), sin(3)> m")
    assert rest == "<1, 0> m " + run("print <-sin(3), cos(3)> m/s")


def test_vector_solution_values_error_has_line():
    msg = str(error_of("solve r'' = -r with r(0) = <1, 0>, r'(0) = <0, 1> for t from 0 to 3\nfor v in r\n"
                       "    print v"))
    assert "line 2" in msg and "r.x" in msg


# A45 -- fit A exp(-t/τ) without a starting guess
def _decay_csv(tmp_path):
    noise = [3, -2, 1, 0, -1, 2, -3, 1, 0, 1, -1]
    rows = "".join(f"{float(t)}, {1000 * math.exp(-t / 3.0) + n}\n" for t, n in zip(range(11), noise))
    (tmp_path / "decay.csv").write_text("t [ms], N\n" + rows)


def test_fit_decay_time_constant_without_guess(tmp_path):
    _decay_csv(tmp_path)
    out = run('d = load "decay.csv"\nfit N = A exp(-t/τ) to d\nprint τ in ms', base_dir=str(tmp_path))
    assert "warning" not in out
    assert "A = 1001.7   (standard error 1.5)" in out
    assert out.split("\n")[-1] == "2.99 ms"


def test_fit_that_cannot_estimate_errors_warns(tmp_path):
    # A and B only appear as A B: the fit is degenerate, so no standard errors
    _decay_csv(tmp_path)
    out = run('d = load "decay.csv"\nfit N = A B exp(-t/τ) to d', base_dir=str(tmp_path))
    assert "warning: the fit may not have converged" in out


# A46 -- rev/min in Hz (D27)
def test_rpm_unit():
    assert run("print 60 rpm in rev/s, 1 rev/s in rpm") == "1 rev/s 60 rpm"


def test_rev_per_min_in_hz_warns():
    w = warnings_of("print 1 rev/min in Hz")
    assert any("2π/60" in m for m in w)
    assert run("print 1 rev/min in Hz") == "0.10472 Hz"
    assert not warnings_of("print 3 /s in Hz")
