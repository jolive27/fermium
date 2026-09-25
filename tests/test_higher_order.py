"""Functions passed to functions (gauntlet friction #24, quantum Q5; D43): each call with a function
argument makes its own compile-time instance, so units are checked per passed function and nothing
about the function is left for run time."""
import io
import math

import pytest

from conftest import run, error_of, warnings_of
from fermium.interp import run_interpreted

SIMPSON = """\
simpson(f, a, b, n) =
    h = (b - a) / n
    s = f(a) + f(b)
    for i from 1 to n - 1
        x = a + i h
        if mod(i, 2) == 1
            s += 4 f(x)
        else
            s += 2 f(x)
    s h / 3
"""

SHOOT = """\
me = 9.1093837e-31 kg
L = 1 nm
shoot(V, E) =
    solve ψ'' = 2 me (V(x) - E) ψ / ħ²
      with ψ(0 nm) = 0, ψ'(0 nm) = 1
      for x from 0 nm to L
    ψ(L)
flat(x) = 0 eV
ramp(x) = 0.5 eV x / L
"""


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=".")
    return out.getvalue().strip()


def val(line):
    return float(line.split()[0])


def test_simpson_on_three_functions_matches_exact_integrals():
    src = SIMPSON + (
        "sq(x) = x²\n"
        "g(t) = 9.8 m/s² * t\n"
        "bump(x) = exp(-x²)\n"
        "print simpson(sq, 0, 3, 100) to 12 digits\n"
        "print simpson(g, 0 s, 2 s, 10) to 12 digits\n"
        "print simpson(bump, 0, 2, 200) to 12 digits\n"
        "print simpson(sin, 0, π, 200) to 12 digits\n")
    a, b, c, d = run(src).split("\n")
    assert val(a) == pytest.approx(9, rel=1e-12)
    assert b.endswith(" m") and val(b) == pytest.approx(19.6, rel=1e-12)       # ½ g t², in m
    assert val(c) == pytest.approx(math.sqrt(math.pi) / 2 * math.erf(2), rel=1e-8)
    assert val(d) == pytest.approx(2, rel=1e-8)                                 # a built-in passed in


def test_shooting_function_takes_the_potential():
    src = SHOOT + (
        "solve shoot(flat, E) = 0 for E from 0.2 eV to 0.5 eV\n"
        "print E in eV to 8 digits\n"
        "print π² ħ² / (2 me L²) in eV to 8 digits\n"
        "solve shoot(ramp, E2) = 0 for E2 from 0.4 eV to 1 eV\n"
        "print E2 in eV to 8 digits\n")
    e1, exact, e2 = run(src).split("\n")
    assert val(e1) == pytest.approx(val(exact), rel=1e-6)
    # first-order perturbation theory: E ≈ E₁ + <V> = E₁ + 0.25 eV (the ramp averages to 0.25 eV)
    assert val(e2) == pytest.approx(val(exact) + 0.25, abs=0.01)
    assert val(e2) != pytest.approx(val(e1), abs=0.1)


def test_units_are_checked_per_passed_function():
    src = ("energy(V, x) = V(x) + 1 J\n"
           "V1(x) = 2 N x\n"
           "V2(x) = 2 N\n"
           "print energy(V1, 1 m)\n"
           "print energy(V2, 1 m)\n")
    e = error_of(src)
    assert e.message == "can't add force [N] to energy [J]"
    assert e.line == 1
    assert "calling energy on line 5 (with V = the function V2, x = length [m])" in e.hint
    assert run(src.rsplit("print", 1)[0]) == "3 J"


def test_derivative_of_a_passed_function():
    src = ("force(V, x) = -V'(x)\n"
           "stiffness(V, x) = d²/dx² V(x)\n"
           "spring(x) = ½ (4 N/m) x²\n"
           "gravity(x) = 2 kg * 9.8 m/s² * x\n"
           "print force(spring, 3 m)\n"
           "print force(gravity, 3 m)\n"
           "print stiffness(spring, 1 m)\n"
           "print force(spring', 3 m)\n")
    assert run(src).split("\n") == ["-12 N", "-19.6 N", "4 N/m", "-4 N/m"]


def test_passed_function_in_integral_and_passed_on():
    src = ("area(ψ, L) = ∫ ψ(x)² dx from 0 m to L\n"
           "normalize(ψ, L) = √(1 / area(ψ, L))\n"
           "box(x) = sin(π x / (1 m))\n"
           "print normalize(box, 1 m) to 12 digits\n")
    assert val(run(src)) == pytest.approx(math.sqrt(2), rel=1e-8)


def test_derivatives_and_gradients_can_be_passed():
    src = ("at(f, t) = f(t)\n"
           "print at(d/dt (3 t²), 2 s)\n"
           "φ(x, y) = x² y\n"
           "field(G, x, y) = -G(x, y)\n"
           "print field(∇φ, 1 m, 2 m)\n"
           "p(x) = x³\n"
           "print at(p', 2)\n")
    assert run(src).split("\n") == ["12 s", "<-4, -1> m²", "12"]


def test_recursion_with_function_arguments():
    src = ("iterate(f, x, n) = if n <= 0 then x else iterate(f, f(x), n - 1)\n"
           "half(x) = x / 2\n"
           "cosine(x) = cos(x)\n"
           "print iterate(half, 8 m, 3)\n"
           "print iterate(cosine, 1, 100) to 6 digits\n"
           "swap(f, g, n) = if n <= 0 then f(1) else swap(g, f, n - 1)\n"
           "two(x) = 2 x\n"
           "three(x) = 3 x\n"
           "print swap(two, three, 5)\n")
    assert run(src).split("\n") == ["1 m", "0.739085", "3"]


def test_passed_function_used_by_solve_and_plot(tmp_path):
    src = ("decay(rate, y0, T) =\n"
           "    solve y' = -rate(t) y with y(0 s) = y0 for t from 0 s to T\n"
           "    y(T)\n"
           "k1(t) = 0.5 / 1 s\n"
           "k2(t) = t / 1 s²\n"
           "show(V, L) =\n"
           f"    plot V vs x from 0 m to L to \"{tmp_path / 'v.svg'}\"\n"
           "    V(L)\n"
           "V1(x) = 2 N x\n"
           "print decay(k1, 1 kg, 2 s) to 6 digits\n"
           "print decay(k2, 1 kg, 2 s) to 6 digits\n"
           "print show(V1, 3 m)\n")
    lines = run(src).split("\n")
    assert val(lines[0]) == pytest.approx(math.exp(-1), rel=1e-5)
    assert val(lines[1]) == pytest.approx(math.exp(-2), rel=1e-5)
    assert lines[-1] == "6 J"
    assert (tmp_path / "v.svg").exists()


def test_native_and_interpreter_agree():
    src = SIMPSON + SHOOT + (
        "sq(x) = x²\n"
        "print simpson(sq, 0, 3, 100)\n"
        "print simpson(sin, 0, π, 50) to 12 digits\n"
        "force(V, x) = -V'(x)\n"
        "print force(ramp, 3 nm) in eV/nm\n"
        "solve shoot(ramp, E) = 0 for E from 0.4 eV to 1 eV\n"
        "print E in eV to 10 digits\n"
        "iterate(f, x, n) = if n <= 0 then x else iterate(f, f(x), n - 1)\n"
        "print iterate(cos, 1, 30) to 10 digits\n")
    native = run(src)
    assert native.count("\n") == 4
    assert interp(src) == native


def test_function_where_a_number_is_expected():
    e = error_of("f(g, x) = g + x\nsq(x) = x²\nprint f(sq, 2)\n")
    assert e.message == "g is a function here; call it like g(x)"
    assert "with g = the function sq" in e.hint


def test_number_where_a_function_is_expected():
    e = error_of("force(V, x) = -V'(x)\nprint force(3 N, 2 m)\n")
    assert e.message == "force uses V as a function (V'), but was given force [N]"
    assert e.line == 2
    # V(x) with a number V is the product V × x (as everywhere in Fermium), with a warning
    w = warnings_of("area(V, L) = ∫ V(x) dx from 0 m to L\nprint area(3 J, 2 m)\n")
    assert w == ["V is a number here, so V(x) in area means V × (...)"]
    assert warnings_of("area(V, L) = ∫ V(x) dx from 0 m to L\nf(x) = 3 J\nprint area(f, 2 m)\n") == []


def test_wrong_number_of_arguments_of_the_passed_function():
    e = error_of("ap(f, x) = f(x)\nh(a, b) = a + b\nprint ap(h, 1)\n")
    assert e.message == "h takes 2 arguments but was given 1"
    assert "calling ap on line 3 (with f = the function h" in e.hint


def test_function_argument_with_a_unit_annotation_is_an_error():
    e = error_of("ap(f [m], x) = f(x)\nh(a) = a\nprint ap(h, 1)\n")
    assert e.message == "ap expects f in m, but got the function h"


def test_uncalled_function_taking_a_function_is_not_an_error():
    assert run("ap(f, x) = f'(x) + f(x)\nprint 1\n") == "1"


def test_ode_solution_cant_be_passed_yet():
    e = error_of("solve y' = -y / (1 s) with y(0 s) = 1 for t from 0 s to 1 s\nap(f, x) = f(x)\n"
                 "print ap(y, 1 s)\n")
    assert e.message == "y is the solution of an ODE; it can't be passed to a function yet"


def test_one_instance_per_passed_function():
    from fermium.checker import Checker
    from fermium.errors import Diagnostics
    from fermium.parser import parse
    src = SIMPSON + "sq(x) = x²\ncube(x) = x³\nprint simpson(sq, 1, 2, 10)\nprint simpson(cube, 1, 2, 10)\n" \
                    "print simpson(sq, 1, 3, 10)\n"
    d = Diagnostics()
    mod = Checker(d, ".").check_program(parse(src, d))
    assert sum(1 for f in mod.funcs if f.name.startswith("simpson.")) == 2
    # the passed function is bound at compile time: simpson's instances take only a, b, n
    assert all(len(f.params) == 3 for f in mod.funcs if f.name.startswith("simpson."))
