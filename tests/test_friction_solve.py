"""Gauntlet frictions in `solve` (gauntlet/FRICTION.md #2, #3, #11, #12, #13, #32, #33, #36).

Every program runs twice, compiled (LLVM) and in the reference interpreter, and the two must agree."""
import io
import math
import os
import re

import pytest

from conftest import run
from fermium.driver import run_source
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src, base_dir=None):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o, base_dir=base_dir, )
    return o.getvalue().strip()


def both(src, base_dir=None):
    out = run(src, base_dir)
    assert interp(src, base_dir) == out
    return out


SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")


def numbers(text):
    """The numbers in printed output, skipping units ('2.5 s' -> 2.5; '4×10⁻¹⁵ m' -> 4e-15)."""
    out = []
    for tok in text.split():
        tok = re.sub(r"×10([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)", lambda m: "e" + m.group(1).translate(SUP), tok)
        if re.fullmatch(r"-?\d+(\.\d*)?(e-?\d+)?", tok):
            out.append(float(tok))
    return out


def nums(src):
    return numbers(both(src))


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    assert native.value.line == ref.value.line
    return native.value


def runtime_warnings(src):
    err = io.StringIO()
    p = run_source(src, "<t>", out=io.StringIO(), err=err)
    rt = run_interpreted(src, "<t>", out=io.StringIO())
    assert p.runtime.warnings == rt.warnings
    assert all(w in err.getvalue() for w in p.runtime.warnings)
    return p.runtime.warnings


# ------------------------------------------------------------------ #2: the FIRST root after a
def test_first_root_even_when_the_ends_bracket_a_later_one():
    # sin(1) > 0 > sin(10): the ends bracket 3π, but the first root after 1 is π
    assert nums("solve sin(x) = 0 for x from 1 to 10\nprint x to 16 digits")[0] == pytest.approx(math.pi, rel=1e-15)
    # three roots inside, opposite signs at the ends
    assert nums("solve (x - 1) (x - 2) (x - 3) = 0 for x from 0.5 to 3.5\nprint x to 16 digits") == \
        pytest.approx([1.0], rel=1e-15)


def test_first_level_of_a_shooting_problem():
    # ψ'' = -k² ψ, ψ(0) = 0, ψ'(0) = 1: ψ(L) = sin(kL)/k; the range brackets the 4th level too
    src = """L = 1 m
psi_end(k) =
    solve ψ'' = -k² ψ
      with ψ(0 m) = 0 m, ψ'(0 m) = 1
      for x from 0 m to L
    return ψ(L)
solve psi_end(k) = 0 m for k from 0.5 / L to 10.5 / L
print k * L to 12 digits"""
    assert nums(src)[0] == pytest.approx(math.pi, rel=1e-9)


# ------------------------------------------------------------------ #3: primes of known functions are values
def test_prime_of_a_defined_function_is_algebraic():
    assert nums("f(x) = x³ - 3x\nsolve f'(x) = 0 for x from 0 to 3\nprint x to 14 digits") == \
        pytest.approx([1.0], rel=1e-12)
    # the diffraction maximum of O1: I'(θ) = 0 between the first two zeros of I
    src = """I(θ) = (sin(θ) / θ)²
solve I'(θm) = 0 for θm from 3.2 to 6.2
print θm to 12 digits"""
    from scipy.optimize import brentq
    ref = brentq(lambda z: math.tan(z) - z, 3.2, 4.6, xtol=1e-15)    # the maxima satisfy tan θ = θ
    assert nums(src)[0] == pytest.approx(ref, rel=1e-10)


def test_prime_of_an_ode_solution_is_algebraic():
    src = """solve y'' = -y / (1 s)²
  with y(0 s) = 1 m, y'(0 s) = 0 m/s
  for t from 0 s to 10 s
solve y'(t2) = 0 m/s for t2 from 1 s to 5 s
solve y(t3) · y'(t3) = 0 m²/s for t3 from 0.5 s to 3 s
print t2 to 14 digits, t3 to 14 digits"""
    t2, t3 = nums(src)
    assert t2 == pytest.approx(math.pi, rel=1e-8)
    assert t3 == pytest.approx(math.pi / 2, rel=1e-8)


def test_undefined_primed_names_are_still_odes():
    src = """solve x' = -x / (1 s)
  with x(0 s) = 1 m
  for t from 0 s to 1 s
solve x' = -2 x / (1 s)
  with x(0 s) = 1 m
  for t from 0 s to 1 s
print x(1 s) / (1 m) to 14 digits"""
    assert nums(src)[0] == pytest.approx(math.exp(-2), rel=1e-8)
    with pytest.raises(FermiumError, match="missing initial condition"):
        run("solve q' = -q / (1 s) for t from 0 s to 1 s")


# ------------------------------------------------------------------ #11: an `if` on t in the equation
R = "r(t) = if t < 0.3 s then 1 / (1 s) else 2 / (1 s)\n"


@pytest.mark.parametrize("tol", [None, 1e-6, 1e-12])
def test_piecewise_growth_meets_the_tolerance(tol):
    t = f" tolerance {tol}" if tol else ""
    src = R + f"solve y' = r(t) y\n  with y(0 s) = 1\n  for t from 0 s to 1 s{t}\nprint y(1 s) to 17 digits"
    err = nums(src)[0] / math.exp(1.7) - 1
    assert abs(err) < 3 * (tol or 1e-9)


@pytest.mark.parametrize("tol", [None, 1e-12])
def test_piecewise_oscillator_matches_the_analytic_solution(tol):
    t = f" tolerance {tol}" if tol else ""
    src = R + f"solve z'' = -(r(t))² z\n  with z(0 s) = 1, z'(0 s) = 0 / (1 s)\n  for t from 0 s to 1 s{t}\n" \
              "print z(1 s) to 17 digits"
    # z = cos t up to 0.3 s, then the ω = 2 solution through (cos 0.3, -sin 0.3)
    exact = math.cos(0.3) * math.cos(1.4) - math.sin(0.3) / 2 * math.sin(1.4)
    assert abs(nums(src)[0] / exact - 1) < 5 * (tol or 1e-9)


def test_barrier_with_two_edges():
    # a rectangular "barrier" in t: y(1) = exp(0.2 + 3·0.3 + 0.5)
    src = """k(t) = if t > 0.2 s and t < 0.5 s then 3 / (1 s) else 1 / (1 s)
solve y' = k(t) y
  with y(0 s) = 1
  for t from 0 s to 1 s tolerance 1e-11
print y(1 s) to 17 digits, y(0.35 s) to 17 digits"""
    y1, ymid = nums(src)
    assert y1 / math.exp(1.6) - 1 == pytest.approx(0, abs=5e-11)
    assert ymid / math.exp(0.2 + 0.45) - 1 == pytest.approx(0, abs=1e-8)     # cubic Hermite in between


# ------------------------------------------------------------------ #12: towards smaller t
def test_backwards_rk45_and_rk4():
    src = """solve u' = -u / (1 s)
  with u(5 s) = 1 m
  for t from 5 s to 0 s
print u(0 s) / (1 m) to 12 digits, u(2.5 s) / (1 m) to 12 digits, u[end] / (1 m) to 12 digits
print times(u)[1], times(u)[end]
solve w' = -w / (1 s)
  with w(5 s) = 1 m
  for t from 5 s to 0 s step 0.01 s
print w(0 s) / (1 m) to 12 digits, w[end] / (1 m) to 12 digits"""
    lines = both(src).split("\n")
    u0, umid, uend = map(float, lines[0].split())
    assert u0 == pytest.approx(math.exp(5), rel=3e-9) and uend == u0
    assert umid == pytest.approx(math.exp(2.5), rel=1e-7)
    assert lines[1] == "5 s 0 s"
    w0, wend = map(float, lines[2].split())
    assert w0 == pytest.approx(math.exp(5), rel=1e-9) and wend == w0


def test_backwards_matches_scipy():
    from scipy.integrate import solve_ivp
    src = """solve x'' = -x / (1 s)² - 0.1 x' / (1 s)
  with x(3 s) = 1 m, x'(3 s) = 0 m/s
  for t from 3 s to -2 s tolerance 1e-11
print x(-2 s) to 17 digits, x'(-2 s) to 17 digits, x(0 s) to 17 digits"""
    got = nums(src)
    ref = solve_ivp(lambda t, y: [y[1], -y[0] - 0.1 * y[1]], (3, -2), [1, 0], rtol=1e-12, atol=1e-14,
                    dense_output=True)
    assert got[:2] == pytest.approx(list(ref.y[:, -1]), rel=1e-8)
    assert got[2] == pytest.approx(ref.sol(0)[0], rel=1e-6)


def test_backwards_plot(tmp_path):
    src = """solve u' = -u / (1 s)
  with u(5 s) = 1 m
  for t from 5 s to 0 s
plot u vs t to "back.png"
print "ok\""""
    assert run(src, base_dir=str(tmp_path)).endswith("ok")
    assert os.path.exists(tmp_path / "back.png")
    from fermium.runtime.core import SolStruct, sample_solution
    import ctypes
    t = [5.0, 4.0, 2.0, 0.0]
    y = [1.0, math.e, math.e ** 3, math.e ** 5]
    arr = lambda v: (ctypes.c_double * len(v))(*v)  # noqa: E731
    ts, ys = sample_solution(SolStruct(4, 1, 4, arr(t), arr(y), arr(y)), 0, 0, npts=11)
    assert ts[0] == 5.0 and ts[-1] == 0.0 and ys[-1] == pytest.approx(math.e ** 5)


def test_empty_range_is_a_clear_error():
    e = both_error("solve u' = -u / (1 s)\n  with u(1 s) = 1 m\n  for t from 1 s to 1 s")
    assert "the range of t is empty: it starts and ends at 1 s" in e.message
    assert "step" not in e.message


# ------------------------------------------------------------------ #13: runtime errors report the right line
def test_root_error_after_an_inner_solve_reports_its_own_line_and_units():
    src = """f(k) =
    solve y' = k y
      with y(0 s) = 1 m
      for t from 0 s to 1 s
    return y(1 s)
solve f(k) = 0.5 m for k from 1 / (1 s) to 2 / (1 s)
print k"""
    e = both_error(src)
    assert e.line == 6
    assert "between 1 1/s and 2 1/s" in e.message


def test_integral_error_in_a_one_line_function_has_its_line():
    src = "x = 1\nK(k) = ∫ 1/√(1 - k² sin(φ)²) dφ from 0 to π/2\nprint K(0.5)\nprint K(1)"
    e = both_error(src)
    assert e.line == 2 and "couldn't compute this integral" in e.message


def test_error_inside_a_callback_function_reports_where_it_happened():
    src = """lst = [1, 2, 3]
f(x) =
    v = lst[round(x)]
    return v - 3.5
solve f(x) = 0 for x from 1 to 10"""
    e = both_error(src)
    assert e.line == 3 and "index 4 is out of range" in e.message


# ------------------------------------------------------------------ #32: NaN at the start
def test_nan_at_the_start_names_the_variable():
    src = "solve θ'' = -2 θ'/ξ - θ\n  with θ(0) = 1, θ'(0) = 0\n  for ξ from 0 to 4"
    e = both_error(src)
    assert e.line == 1
    assert "the right side of the equation is NaN or infinite at ξ = 0" in e.message
    assert "too many steps" not in e.message
    e = both_error("solve y' = y / t\n  with y(0 s) = 0 m\n  for t from 0 s to 1 s")
    assert "at t = 0 s" in e.message


# ------------------------------------------------------------------ #33: until
def test_until_stops_at_the_first_crossing():
    src = """g = 9.8 m/s²
solve h'' = -g
  with h(0 s) = 0 m, h'(0 s) = 10 m/s
  for t from 0 s to 100 s
  until h = 0 m
print times(h)[end] to 15 digits, h[end] to 15 digits, h'[end] to 15 digits
print 2 × 10 m/s / g to 15 digits"""
    lines = both(src).split("\n")
    t_end, h_end, v_end = numbers(lines[0])
    assert t_end == pytest.approx(20 / 9.8, rel=1e-13)
    assert abs(h_end) < 1e-12 and v_end == pytest.approx(-10, rel=1e-12)


def test_until_matches_scipy_events_with_drag():
    from scipy.integrate import solve_ivp
    src = """g = 9.81 m/s²
k = 0.02 / (1 m)
θ = 50°
solve r'' = <0 m/s², -g> - k |r'| r'
  with r(0 s) = <0, 0> m, r'(0 s) = 40 m/s * <cos(θ), sin(θ)>
  for t from 0 s to 60 s tolerance 1e-11
  until r.y = 0 m
print times(r)[end] to 15 digits, r.x[end] to 15 digits
solve v' = -g - k v |v|
  with v(0 s) = 30 m/s
  for t from 0 s to 60 s tolerance 1e-11
  until v = 0 m/s
print times(v)[end] to 15 digits"""
    lines = both(src).split("\n")
    t_land, x_land = [float(s.split()[0]) for s in (lines[0].split(" s ")[0] + " s", lines[0].split(" s ")[1])]

    def drag(t, u):
        sp = math.hypot(u[2], u[3])
        return [u[2], u[3], -0.02 * sp * u[2], -9.81 - 0.02 * sp * u[3]]

    def ground(t, u):
        return u[1]
    ground.terminal, ground.direction = True, -1
    th = math.radians(50)
    ref = solve_ivp(drag, (0, 60), [0, 0, 40 * math.cos(th), 40 * math.sin(th)], events=ground,
                    rtol=1e-12, atol=1e-12)
    assert t_land == pytest.approx(ref.t_events[0][0], rel=1e-8)
    assert x_land == pytest.approx(ref.y_events[0][0][0], rel=1e-8)
    t_apex = float(lines[1].split()[0])
    assert t_apex == pytest.approx(math.atan(30 * math.sqrt(0.02 / 9.81)) / math.sqrt(0.02 * 9.81), rel=1e-8)


def test_until_other_forms():
    # the apex (a derivative), rk4 with a fixed step, towards smaller t, and written after `with`
    src = """solve y'' = -9.8 m/s²
  with y(0 s) = 0 m, y'(0 s) = 9.8 m/s
  for t from 0 s to 10 s
  until y' = 0 m/s
print times(y)[end] to 12 digits
solve y2'' = -9.8 m/s²
  with y2(0 s) = 0 m, y2'(0 s) = 9.8 m/s
  for t from 0 s to 10 s step 0.01 s
  until y2 = 1 m
print times(y2)[end] to 12 digits
solve u' = u / (1 s)
  with u(0 s) = 1 m
  for t from 0 s to -10 s
  until u = 0.5 m
print times(u)[end] to 12 digits
solve q' = 1 m/s with q(0 s) = 0 m, until q = 2 m for t from 0 s to 5 s
print times(q)[end]"""
    got = [float(v.split()[0]) for v in both(src).split("\n")]
    assert got[0] == pytest.approx(1.0, rel=1e-12)
    assert got[1] == pytest.approx(1 - math.sqrt(1 - 2 / 9.8), rel=1e-12)     # quadratic: RK4 is exact
    assert got[2] == pytest.approx(-math.log(2), rel=1e-9)
    assert got[3] == pytest.approx(2.0, rel=1e-12)


def test_until_errors():
    e = both_error("solve y' = 1 m/s\n  with y(0 s) = 0 m\n  for t from 0 s to 1 s\n  until y = 5 m")
    assert e.line == 1
    assert "the stop condition (until y = 5 m) never happened up to t = 1 s; make the range longer" in e.message
    e = both_error("solve y' = 1 m/s\n  with y(0 s) = 0 m\n  for t from 0 s to 10 s\n  until y = 5 m\n"
                   "print y(6 s)")
    assert "outside the range" in e.message and "it ends at 5 s" in e.message
    with pytest.raises(FermiumError, match="the stop condition can use y, y'"):
        run("solve y'' = -1 m/s²\n  with y(0 s) = 0 m, y'(0 s) = 1 m/s\n  for t from 0 s to 1 s\n"
            "  until y'' = 0 m/s²")
    with pytest.raises(FermiumError, match="stop condition don't match"):
        run("solve y' = 1 m/s\n  with y(0 s) = 0 m\n  for t from 0 s to 1 s\n  until y = 5 s")
    with pytest.raises(FermiumError, match="for differential equations"):
        run("solve x² = 2\n  for x from 0 to 2\n  until x = 1")


# ------------------------------------------------------------------ #36: a root in rounding noise
def test_warning_for_a_root_in_rounding_noise():
    # (x + 1000)² - x² for x ~ 10¹⁷: the squares cancel to rounding noise (like S2, the GZK threshold)
    w = runtime_warnings("solve (x + 1000)² - x² = 3e20 for x from 1e16 to 1e18\nprint x")
    assert len(w) == 1 and "agree only to rounding error" in w[0] and w[0].startswith("warning: line 1:")
    # the expanded form is fine, and so are ordinary equations (also with an exact root on a scan point)
    assert runtime_warnings("solve 2000 x + 1e6 = 3e20 for x from 1e16 to 1e18\nprint x") == []
    assert runtime_warnings("solve x² = 2 for x from 0 to 2\nsolve sin(x) = 0 for x from -1 to 1\n"
                            "solve x = 0 for x from -1 to 1\nsolve 1e-30 x = 1e-30 for x from 0 to 3") == []


# ------------------------------------------------------------------ the same in `fermium build` executables
def test_built_executables(tmp_path):
    import subprocess
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")

    def built(src, name):
        exe = str(tmp_path / name)
        build(src, exe + ".fm", exe)
        return subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(tmp_path))
    src = """g = 9.8 m/s²
solve h'' = -g
  with h(0 s) = 0 m, h'(0 s) = 10 m/s
  for t from 0 s to 100 s
  until h = 0 m
print times(h)[end]
solve u' = -u / (1 s)
  with u(5 s) = 1 m
  for t from 5 s to 0 s step 0.01 s
print u(0 s), u[end]
solve sin(x) = 0 for x from 1 to 10
print x"""
    got = built(src, "ok")
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == run(src)
    got = built("solve θ'' = -2 θ'/ξ - θ\n  with θ(0) = 1, θ'(0) = 0\n  for ξ from 0 to 4", "nan")
    assert "line 1: the right side of the equation is NaN or infinite at ξ = 0" in got.stderr
    got = built("solve y' = 1 m/s\n  with y(0 s) = 0 m\n  for t from 0 s to 1 s\n  until y = 5 m", "never")
    assert "the stop condition (until y = 5 m) never happened up to t = 1 s; make the range longer" in got.stderr
    got = built("solve (x + 1000)² - x² = 3e20 for x from 1e16 to 1e18\nprint x", "noise")
    assert got.returncode == 0 and "warning: line 1: the two sides of this equation agree only to rounding" in got.stderr


def test_until_right_after_the_range():
    src = ("g = 9.81 m/s²\nsolve y'' = -g with y(0) = 0 m, y'(0) = 10 m/s for t from 0 s to 9 s until y = 0 m\n"
           "print times(y)[end] to 10 digits")
    assert abs(float(run(src).split()[0]) - 20 / 9.81) < 1e-8
