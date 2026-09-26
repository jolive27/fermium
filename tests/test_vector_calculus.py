"""∇f, ∇·F, ∇×F and ∇²f (and their ASCII forms grad, div, curl, laplacian)."""
import math

import pytest

from conftest import run, error_of
from fermium.fmt import format_source


def nums(out):
    return [float(v) for v in out.replace("<", " ").replace(">", " ").replace(",", " ").split()
            if v.replace(".", "").replace("-", "").replace("e", "").isdigit()]


def test_point_charge_field_is_minus_grad_phi():
    src = ("q = 1 nC\nφ(x, y, z) = q / (4π ε₀ √(x² + y² + z²))\n"
           "print -∇φ(1 m, 0 m, 0 m) to 6 digits\nprint -∇φ(0 m, 3 m, 4 m) to 10 digits")
    a, b = run(src).split("\n")
    k = 8.9875517862e9 * 1e-9
    assert a == "<8.98755, 0, 0> V/m"
    got = nums(b)
    assert got == pytest.approx([0, k * 3 / 125, k * 4 / 125], rel=1e-6, abs=1e-12)


def test_grad_prints_a_simplified_formula():
    out = run("q = 1 nC\nφ(x, y, z) = q / (4π ε₀ √(x² + y² + z²))\nprint ∇φ")
    assert out.startswith("∇φ(x, y, z) = <-q x/(4π ε")
    assert "(x² + y² + z²)^(3/2)" in out


@pytest.mark.parametrize("f,sy", [
    ("x^2 y + sin(x)", "x**2*y + sin(x)"),
    ("exp(-x^2 - y^2)", "exp(-x**2 - y**2)"),
    ("x y / (1 + x^2 + y^2)", "x*y/(1 + x**2 + y**2)"),
])
def test_grad_and_laplacian_match_sympy(f, sy):
    sp = pytest.importorskip("sympy")
    x, y = sp.symbols("x y")
    e = sp.sympify(sy)
    p = (0.7, -1.3)
    out = run(f"f(x, y) = {f}\nprint grad(f)({p[0]}, {p[1]}) to 12 digits\nprint ∇²f({p[0]}, {p[1]}) to 12 digits")
    g1, g2, lap = nums(out)
    sub = {x: p[0], y: p[1]}
    assert g1 == pytest.approx(float(sp.diff(e, x).subs(sub)), rel=1e-9, abs=1e-12)
    assert g2 == pytest.approx(float(sp.diff(e, y).subs(sub)), rel=1e-9, abs=1e-12)
    assert lap == pytest.approx(float((sp.diff(e, x, 2) + sp.diff(e, y, 2)).subs(sub)), rel=1e-9, abs=1e-12)


def test_curl_and_div_with_units():
    src = ("B(x, y, z) = <-y, x, 0> T/m\nprint ∇×B\nprint ∇×B(1 m, 2 m, 3 m)\nprint ∇·B(1 m, 2 m, 3 m)\n"
           "F(x, y, z) = <x², x y, z³>\nprint div(F)(1, 2, 3)\nprint curl(F)(1, 2, 3)")
    assert run(src).split("\n") == ["∇×B(x, y, z) = <0, 0, 2 T/m>", "<0, 0, 2> T/m", "0",
                                    "30", "<0, 0, 2>"]


def test_curl_of_gradient_is_zero_and_div_of_curl_is_zero():
    src = ("φ(x, y, z) = x² y z + sin(y z)\nE = ∇φ\nprint curl(E)(0.3, 1.2, -0.7)\n"
           "A(x, y, z) = <y z², x³, sin(x y)>\nC = ∇×A\nprint div(C)(0.3, 1.2, -0.7)")
    a, b = run(src).split("\n")
    assert all(abs(v) < 1e-12 for v in nums(a)) and abs(float(b)) < 1e-12


def test_laplacian_of_1_over_r_is_zero_away_from_origin():
    out = run("f(x, y, z) = 1/√(x² + y² + z²)\nprint ∇²f(0.3, -1.1, 2.0)")
    assert abs(float(out)) < 1e-12


def test_units_of_the_gradient():
    # V over m: the field is V/m; adding it to a length is a unit error
    err = error_of("V0 = 1 V\nφ(x, y, z) = V0 x / (1 m)\nE = ∇φ(1 m, 0 m, 0 m)\nd = E + <1, 0, 0> m")
    assert "V/m" in str(err) or "can't add" in str(err)


@pytest.mark.parametrize("src,msg", [
    ("f(x) = x^2\nprint ∇f", "2 or 3 coordinates"),
    ("F(x, y) = <y, x>\nprint ∇×F", "3 coordinates"),
    ("f(x, y, z) = x + y + z\nprint ∇·f", "vector formula"),
    ("k = 3\nprint ∇k", "isn't a function"),
    ("F(x, y, z) = <x, y>\nprint ∇·F", "2 components but 3 coordinates"),
    ("print ∇ 3", "needs the name of a function"),
])
def test_errors(src, msg):
    assert msg in str(error_of(src))


def test_fmt_ascii_round_trip():
    src = "B(x, y, z) = <-y, x, 0> T/m\nprint ∇×B(1 m, 2 m, 3 m)\nf(x, y) = x² y\nprint ∇²f(1, 2), ∇f(1, 2)\n"
    ascii_src = format_source(src, "ascii")
    assert "curl(B)(1 m, 2 m, 3 m)" in ascii_src and "laplacian(f)(1, 2)" in ascii_src and "grad(f)(1, 2)" in ascii_src
    assert ascii_src.isascii()
    assert run(ascii_src) == run(src)


def test_interpreter_agrees():
    import io
    from fermium.interp import run_interpreted
    src = "q = 1 nC\nφ(x, y, z) = q / (4π ε₀ √(x² + y² + z²))\nprint -∇φ(0 m, 3 m, 4 m)"
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    assert o.getvalue().strip() == run(src)
    assert math.isfinite(nums(run(src))[1])
