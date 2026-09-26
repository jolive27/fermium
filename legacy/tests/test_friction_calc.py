"""Gauntlet friction items #18, #19, #20 and #22 (gauntlet/FRICTION.md):
differentiating under the integral sign, integrals of vectors, indefinite integrals that SymPy
answers with asinh/Abs, and eigenvalues/eigenvectors of symmetric matrices (D34-D37)."""
import io
import math

import numpy as np
import pytest

from conftest import run, error_of
from numparse import nums
from fermium.interp import run_interpreted


def msg_of(src):
    return error_of(src).message


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    """Run natively and in the reference interpreter; they must agree exactly."""
    a = run(src)
    assert interp(src) == a
    return a


def vec(text):
    body = text[text.index("<") + 1:text.index(">")]
    return np.array([nums(x.strip())[0] for x in body.split(",")])


def mat(text):
    body = text[text.index("[["):text.rindex("]]") + 2]
    rows = body[2:-2].split("], [")
    return np.array([[nums(x)[0] for x in row.split(", ")] for row in rows])


# ------------------------------------------------------------------ #18 Leibniz rule
LINE = """
λ = 2.0 nC/m
L = 0.5 m
V(x, y, z) = ∫ λ / (4π ε₀ √((x - s)^2 + y^2 + z^2)) ds from -L to L
"""
K_E = 1 / (4 * math.pi * 8.8541878188e-12)


def line_field(x, y, lam=2e-9, L=0.5):
    """The closed-form field of a line charge from -L to L on the x axis, at (x, y, 0)."""
    rm, rp = math.hypot(x - L, y), math.hypot(x + L, y)
    return np.array([K_E * lam * (1 / rm - 1 / rp), K_E * lam / y * ((x + L) / rp - (x - L) / rm), 0.0])


@pytest.mark.parametrize("x,y", [(0.3, 0.4), (1.2, 0.05), (-0.7, 0.9)])
def test_gradient_of_integral_matches_closed_form_line_charge(x, y):
    out = both(LINE + f"print -∇V({x} m, {y} m, 0 m) to 10 digits")
    assert out.endswith("V/m")
    got = vec(out)
    want = line_field(x, y)
    assert np.allclose(got, want, rtol=1e-8, atol=1e-9 * np.abs(want).max())


def test_partial_derivative_of_integral_matches_finite_difference():
    src = LINE + """
h = 1 mm
p = (∂/∂x V)(0.3 m, 0.4 m, 0.2 m)
fd = (V(0.3 m + h, 0.4 m, 0.2 m) - V(0.3 m - h, 0.4 m, 0.2 m)) / (2 h)
print p to 10 digits
print abs(p - fd) / abs(p) < 1e-5
q = (∂/∂z V)(0.3 m, 0.4 m, 0.2 m)
fz = (V(0.3 m, 0.4 m, 0.2 m + h) - V(0.3 m, 0.4 m, 0.2 m - h)) / (2 h)
print abs(q - fz) / abs(q) < 1e-5
"""
    out = both(src).splitlines()
    assert out[0].endswith("V/m")
    assert out[1:] == ["true", "true"]


def test_derivative_of_integral_is_an_integral_of_the_derivative():
    out = both(LINE + "print ∂/∂x V")
    assert out.startswith("∂V/∂x(x, y, z) = ∫ ")
    assert "ds from -L to L" in out and "V/m" in out


def test_leibniz_rule_with_variable_limits():
    # G(x) = ∫ x s² ds from 0 to x = x⁴/3, so G'(x) = 4x³/3 (an inner term plus a boundary term)
    out = both("G(x) = ∫ x s^2 ds from 0 to x\nprint G'(2) to 12 digits\nprint G'")
    lines = out.splitlines()
    assert nums(lines[0])[0] == pytest.approx(32 / 3, rel=1e-9)
    assert lines[1] == "G'(x) = (∫ s² ds from 0 to x) + x³"
    # lower limit depending on x, and the integration variable named like the parameter
    assert nums(both("f(s) = ∫ s^2 ds from s to 3\nprint f'(2)"))[0] == pytest.approx(-4)
    assert nums(both("g(t) = ∫ t u ds from 0 to 1 where u = 2\nprint g'(5)"))[0] == pytest.approx(2)


def test_divergence_of_integral_potential_gradient_is_laplacian():
    # outside the rod, ∇²V = 0: the Laplacian of an integral is the integral of the Laplacian
    out = both(LINE + "print abs(∇²V(0.3 m, 0.4 m, 0.1 m)) < 1e-6 * 1 V/m^2")
    assert out == "true"


# ------------------------------------------------------------------ #19 vector integrands
def test_integral_of_vector_literal_is_vector_of_integrals():
    assert both("print ∫ <s, s^2, 1> ds from 0 to 1") == "<0.500, 0.333, 1.00>"
    out = both("print ∫ <cos(φ), sin(φ)> dφ from 0 to π/2 to 12 digits")
    assert np.allclose(vec(out), [1, 1], rtol=1e-12)


def test_vector_integral_units_and_mixed_units():
    assert both("print ∫ <1, 2, 3> N dt from 0 s to 2 s") == "<2, 4, 6> kg m/s"
    assert both("print ∫ <1 m, 2 m/s> dt from 0 s to 2 s") == "<2 m s, 4 m>"


def test_vector_integral_matches_per_component_integrals():
    src = """
f(s) = <exp(-s) s, 1 / (1 + s^2), √s> m
v = ∫ f(s) ds from 0 to 3
print v to 12 digits
print ∫ f(s).x ds from 0 to 3 to 12 digits
print ∫ f(s).y ds from 0 to 3 to 12 digits
print ∫ f(s).z ds from 0 to 3 to 12 digits
"""
    a, *b = both(src).splitlines()
    assert np.allclose(vec(a), nums("\n".join(b)), rtol=1e-12)
    assert np.allclose(vec(a), [1 - 4 * math.exp(-3), math.atan(3), 2 * 3 ** 1.5 / 3], rtol=1e-9)


def test_biot_savart_loop_on_axis():
    src = """
R = 0.10 m
I = 2.0 A
ring(φ) = R * <cos(φ), sin(φ), 0>
dl(φ) = R * <-sin(φ), cos(φ), 0>
B(P) = μ₀ I / (4π) * ∫ dl(φ) × (P - ring(φ)) / |P - ring(φ)|^3 dφ from 0 to 2π
b = B(<0 m, 0 m, 0.05 m>)
print b to 10 digits
print μ₀ I R^2 / (2 (R^2 + (0.05 m)^2)^1.5) to 10 digits
"""
    a, b = both(src).splitlines()
    assert a.endswith("T") and b.endswith("T")
    got, want = vec(a), nums(b)[0]
    assert got[2] == pytest.approx(want, rel=1e-9)
    assert abs(got[0]) < 1e-12 * want and abs(got[1]) < 1e-12 * want


def test_vector_integral_inside_function_and_with_parameter():
    assert both("f(x) = ∫ <x s, s> ds from 0 to 1\nprint f(3)") == "<1.50, 0.500>"


def test_matrix_integrand_still_an_error():
    msg = msg_of("print ∫ [[s, 1], [1, s]] ds from 0 to 1")
    assert "matrix" in msg


# ------------------------------------------------------------------ #20 indefinite integrals
def test_indefinite_asinh_positive_constant():
    out = both("a = 0.5 m\nF = ∫ 1 / √(a^2 + s^2) ds\nprint F\nprint F(-1 m) to 9 digits\n"
               "print F(0 m)\nprint F(1 m) to 9 digits")
    lines = out.splitlines()
    assert lines[0].startswith("F(s) = asinh(s/a)")
    v = nums("\n".join(lines[1:]))
    assert v[0] == pytest.approx(-math.asinh(2), rel=1e-5) and v[1] == 0 and v[2] == pytest.approx(math.asinh(2), rel=1e-5)


def test_indefinite_asinh_negative_constant_is_still_right():
    # only b² enters, so b is replaced by |b|; SymPy's own answer would be wrong for b < 0
    src = "b = -2 m\nK = ∫ 1 / √(b^2 + s^2) ds\nprint K(1 m) - K(-1 m) to 9 digits"
    assert nums(both(src))[0] == pytest.approx(2 * math.asinh(0.5), rel=1e-9)


def test_indefinite_with_quantity_inside():
    src = "x = 2 m\nJ = ∫ 1/√((x - s)^2 + (0.5 m)^2) ds\nprint J(3 m) - J(1 m) to 9 digits\nprint J(2 m)"
    a, b = both(src).splitlines()
    assert nums(a)[0] == pytest.approx(2 * math.asinh(2), rel=1e-9)
    assert nums(b)[0] == 0          # asinh((s - x)/q): no 0/0 at s = x


def test_indefinite_variable_that_may_be_negative_gets_abs():
    # a is changed by a loop, so it isn't assumed positive; the formula must hold for any sign
    src = """
a = 1 m
for k from 1 to 2
    a = -a
F = ∫ 1 / √(a^2 + s^2) ds
print F(2 m) - F(-2 m) to 9 digits
"""
    assert nums(both(src))[0] == pytest.approx(2 * math.asinh(2), rel=1e-9)


def test_indefinite_failure_has_line_number_and_clear_message():
    with pytest.raises(Exception) as ei:
        run("x = 1\n\nF = ∫ exp(s) / s ds\nprint F(1)")
    e = ei.value
    assert e.line == 3
    assert "SymPy" in e.message and "Fermium doesn't have" in e.message
    assert "limits" in (e.hint or "")


def test_indefinite_undefined_name_reported_first():
    with pytest.raises(Exception) as ei:
        run("F = ∫ 1 / √(zz^2 + s^2) ds")
    assert "zz isn't defined" in ei.value.message
    assert ei.value.line == 1


def test_indefinite_positive_names_not_trusted_in_solve():
    from fermium.checker import _positive_names
    from fermium.parser import parse
    prog = parse("a = 1 m\nb = 2 m\nc = -1 m\nd = 3 m\nd += 1 m\nsolve b = 1 m for b from 0 m to 3 m\n"
                 "for k from 1 to 3\n    e2 = 1\ne3 = 2 m * 3\n")
    assert _positive_names(prog) == {"a", "e2", "e3"}      # not k (a loop), b (solve), c, d


# ------------------------------------------------------------------ #22 eigenvalues
SYM3 = [[4.0, 1.0, 0.5], [1.0, 3.0, -0.25], [0.5, -0.25, 2.0]]
SYM4 = [[5.0, 1.0, 0.5, 2.0], [1.0, 4.0, 1.5, 0.0], [0.5, 1.5, 6.0, 1.0], [2.0, 0.0, 1.0, 7.0]]


def lit(m):
    return "[" + ", ".join("[" + ", ".join(repr(x) for x in row) + "]" for row in m) + "]"


@pytest.mark.parametrize("m", [[[2.0, 1.0], [1.0, 3.0]], SYM3, SYM4, [[1.0, 0.0], [0.0, 1.0]],
                               [[1e6, 1e-3, 0.0], [1e-3, 2.0, 5.0], [0.0, 5.0, -3.0]]])
def test_eigenvalues_and_vectors_match_numpy(m):
    src = f"M = {lit(m)} N/m\nprint eigenvalues(M) to 15 digits\nprint eigenvectors(M) to 15 digits"
    a, b = both(src).splitlines()
    assert a.endswith("N/m")
    w, v = np.linalg.eigh(np.array(m))
    got = vec(a)
    assert np.allclose(got, w, rtol=1e-12, atol=1e-12 * np.abs(w).max())
    V = mat(b)
    A = np.array(m)
    assert np.allclose(V.T @ V, np.eye(len(m)), atol=1e-12)
    assert np.allclose(A @ V, V * got, atol=1e-10 * np.abs(w).max())
    if len(set(np.round(w, 9))) == len(w):          # distinct eigenvalues: same vectors up to sign
        for j in range(len(m)):
            assert min(np.abs(V[:, j] - v[:, j]).max(), np.abs(V[:, j] + v[:, j]).max()) < 1e-10


def test_eigenvalues_are_sorted_and_vectors_have_positive_largest_entry():
    out = both("M = [[3, 0, 0], [0, -1, 0], [0, 0, 2]]\nprint eigenvalues(M)\nprint eigenvectors(M)")
    assert out.splitlines() == ["<-1, 2, 3>", "[[0, 0, 1], [1, 0, 0], [0, 1, 0]]"]


def test_eigenvalues_units_and_indexing():
    src = """
k = 80 N/m
kc = 20 N/m
K = [[k + kc, -kc], [-kc, k + kc]]
λ = eigenvalues(K)
print λ
print λ[2] in kN/m
"""
    assert both(src).splitlines() == ["<80, 120> N/m", "0.120 kN/m"]


def test_generalized_eigenvalues_normal_modes_match_scipy():
    scipy_linalg = pytest.importorskip("scipy.linalg")
    K = [[30.0, -10.0, 0.0], [-10.0, 25.0, -15.0], [0.0, -15.0, 15.0]]
    M = [[2.0, 0.0, 0.0], [0.0, 1.5, 0.0], [0.0, 0.0, 1.0]]
    src = f"""
K = {lit(K)} N/m
M = {lit(M)} kg
ω2 = eigenvalues(K, M)
print ω2 to 15 digits
print eigenvectors(K, M) to 15 digits
print √(ω2[1]) to 6 digits
"""
    a, b, c = both(src).splitlines()
    assert a.endswith("1/s²") and c.endswith("1/s")
    w, v = scipy_linalg.eigh(np.array(K), np.array(M))
    assert np.allclose(vec(a), w, rtol=1e-12)
    V = mat(b)
    assert np.allclose(np.linalg.norm(V, axis=0), 1, atol=1e-12)
    assert np.allclose(np.array(K) @ V, np.array(M) @ V * vec(a), atol=1e-9)
    for j in range(3):
        u = v[:, j] / np.linalg.norm(v[:, j])
        assert min(np.abs(V[:, j] - u).max(), np.abs(V[:, j] + u).max()) < 1e-10


def test_generalized_with_full_mass_matrix():
    scipy_linalg = pytest.importorskip("scipy.linalg")
    K = SYM4
    M = [[4.0, 1.0, 0.0, 0.5], [1.0, 3.0, 0.2, 0.0], [0.0, 0.2, 2.0, 0.1], [0.5, 0.0, 0.1, 5.0]]
    out = both(f"print eigenvalues({lit(K)}, {lit(M)}) to 15 digits")
    assert np.allclose(vec(out), scipy_linalg.eigh(np.array(K), np.array(M), eigvals_only=True), rtol=1e-12)


def test_eigen_errors():
    assert "symmetric" in msg_of("print eigenvalues([[1, 2], [3, 4]])")
    assert "symmetric" in msg_of("print eigenvectors([[1, 2], [2, 4]], [[1, 1], [0, 1]])")
    assert "positive definite" in msg_of("print eigenvalues([[1, 0], [0, 1]], [[1, 0], [0, -1]])")
    assert "square" in msg_of("print eigenvalues([[1, 2, 3], [4, 5, 6]])")
    assert "same size" in msg_of("print eigenvalues(identity(2), identity(3))")
    assert "matrix" in msg_of("print eigenvalues(<1, 2>)")
    for src in ("print eigenvalues([[1, 2], [3, 4]])", "print eigenvalues([[1, 0], [0, 1]], [[0, 0], [0, 1]])"):
        with pytest.raises(Exception) as ei:
            interp(src)
        assert ei.value.message == msg_of(src)


def test_inverse_times_matrix_is_rejected_with_a_pointer_to_the_generalized_form():
    msg = msg_of("K = [[2, -1], [-1, 1]]\nM = [[1, 0], [0, 3]]\nprint eigenvalues(inverse(M) K)")
    assert "eigenvalues(K, M)" in msg


def test_eigen_in_standalone_build(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    import subprocess
    src = "K = [[2, -1], [-1, 2]] N/m\nprint eigenvalues(K)\nprint eigenvalues(K, [[1, 0], [0, 2]] kg)\n"
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    out = subprocess.run([exe], capture_output=True, text=True).stdout.strip()
    assert out == run(src)
    build("print eigenvalues([[1, 2], [3, 4]])", str(tmp_path / "bad.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True)
    assert r.returncode != 0 and "symmetric" in r.stderr
