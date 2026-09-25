"""Matrices with units ([[1, 2], [3, 4]] N/m), 4-vectors, and vectors with a unit per component
(<1 m, 2 m/s>).  Numbers are compared against NumPy; unit errors are checked too (D29)."""
import io

import numpy as np
import pytest

from conftest import run, error_of
from numparse import nums
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def matrix_numbers(text):
    """[[1, 2], [3, 4]] N/m -> [[1.0, 2.0], [3.0, 4.0]]."""
    body = text[text.index("[["):text.rindex("]]") + 2]
    rows = body[2:-2].split("], [")
    return np.array([[nums(x)[0] for x in row.split(", ")] for row in rows])


def vector_numbers(text):
    body = text[text.index("<") + 1:text.index(">")]
    return np.array([nums(x.strip())[0] for x in body.split(",")])


A3 = [[4.0, -2.0, 1.0], [3.0, 6.0, -4.0], [2.0, 1.0, 8.0]]
A4 = [[5.0, 1.0, 0.5, 2.0], [1.0, 4.0, 1.5, 0.0], [0.5, 1.5, 6.0, 1.0], [2.0, 0.0, 1.0, 7.0]]


def lit(m):
    return "[" + ", ".join("[" + ", ".join(repr(x) for x in row) + "]" for row in m) + "]"


# ------------------------------------------------------------------ literals and printing
def test_matrix_literal_prints_rows_on_one_line():
    assert run("K = [[1, 2], [3, 4]] N/m\nprint K") == "[[1, 2], [3, 4]] N/m"


def test_matrix_entries_with_units():
    assert run("K = [[1 N/m, 0 N/m], [0 N/m, 2 N/m]]\nprint K") == "[[1, 0], [0, 2]] N/m"
    assert run("print [[1, 2], [3, 4]] [kN/m] in N/m") == "[[1000, 2000], [3000, 4000]] N/m"


def test_plain_and_rectangular_matrices():
    assert run("print [[1, 2, 3], [4, 5, 6]]") == "[[1, 2, 3], [4, 5, 6]]"
    assert run("print identity(3)") == "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]"


def test_matrix_convert_and_digits():
    assert run("K = [[1, 2], [3, 4]] kN/m\nprint K in N/m") == "[[1000, 2000], [3000, 4000]] N/m"
    assert run("print [[1, 2], [3, 4]] / 3 to 3 digits") == "[[0.333, 0.667], [1.00, 1.33]]"


# ------------------------------------------------------------------ arithmetic
def test_matrix_sum_difference_scaling():
    out = run("K1 = [[1, 2], [3, 4]] N/m\nK2 = [[10, 20], [30, 40]] N/m\n"
              "print K1 + K2\nprint K2 - K1\nprint 2 K1\nprint K1 * 2 m\nprint -K1\nprint K1 / 2")
    assert out.split("\n") == ["[[11, 22], [33, 44]] N/m", "[[9, 18], [27, 36]] N/m",
                               "[[2, 4], [6, 8]] N/m", "[[2, 4], [6, 8]] N", "[[-1, -2], [-3, -4]] N/m",
                               "[[0.5, 1], [1.5, 2]] N/m"]


def test_matrix_times_vector_multiplies_dimensions():
    out = run("K = [[2, -1], [-1, 2]] N/m\nx = <1, 2> cm\nprint K x in N\nprint K * x in N\nprint K · x in N")
    assert out.split("\n") == ["<0, 0.03> N"] * 3


def test_matrix_times_vector_against_numpy():
    out = run(f"M = {lit(A3)} kg\nv = <1.5, -2, 0.25> m/s\nprint M v to 15 digits")
    assert out.endswith("kg m/s")
    np.testing.assert_allclose(vector_numbers(out), np.array(A3) @ np.array([1.5, -2, 0.25]), rtol=1e-13)


def test_4x4_times_4_vector():
    out = run(f"M = {lit(A4)}\nv = <1, 2, 3, 4>\nprint M v to 15 digits\nprint v[4], |v|")
    lines = out.split("\n")
    np.testing.assert_allclose(vector_numbers(lines[0]), np.array(A4) @ np.array([1, 2, 3, 4]), rtol=1e-13)
    assert lines[1].startswith("4 ")


def test_matrix_product_against_numpy():
    out = run(f"A = {lit(A3)} N/m\nB = {lit(A3)}\nprint A * B to 15 digits\nprint (A B)[3, 1] to 15 digits")
    lines = out.split("\n")
    assert lines[0].endswith("N/m")
    np.testing.assert_allclose(matrix_numbers(lines[0]), np.array(A3) @ np.array(A3), rtol=1e-13)
    assert nums(lines[1])[0] == pytest.approx((np.array(A3) @ np.array(A3))[2, 0])


def test_rectangular_product():
    out = run("A = [[1, 2, 3], [4, 5, 6]]\nB = [[1, 0], [0, 1], [1, 1]]\nprint A B\nprint transpose(A)")
    assert out.split("\n") == ["[[4, 5], [10, 11]]", "[[1, 4], [2, 5], [3, 6]]"]


def test_transpose_and_superscript_T():
    out = run("M = [[1, 2], [3, 4]] s\nprint transpose(M)\nprint Mᵀ\nprint (M Mᵀ)[1, 2]")
    assert out.split("\n") == ["[[1, 3], [2, 4]] s", "[[1, 3], [2, 4]] s", "11 s²"]


# ------------------------------------------------------------------ det, inverse, solve
def test_det_has_dimension_to_the_n():
    out = run("K = [[2, -1], [-1, 2]] N/m\nprint det(K)\nprint det(K) in N²/m²")
    assert out.split("\n") == ["3 N²/m²", "3 N²/m²"]
    # det of a 2×2 in N/m is (N/m)², so it can't be added to N/m
    assert "can't add" in str(error_of("K = [[2, -1], [-1, 2]] N/m\nprint det(K) + 1 N/m"))


@pytest.mark.parametrize("m", [A3, A4, [[0.0, 2.0], [3.0, 1.0]]])
def test_det_against_numpy(m):
    out = run(f"print det({lit(m)} m) to 15 digits")
    assert nums(out)[0] == pytest.approx(np.linalg.det(np.array(m)), rel=1e-12)


@pytest.mark.parametrize("m", [A3, A4, [[0.0, 2.0], [3.0, 1.0]]])
def test_inverse_against_numpy(m):
    out = run(f"K = {lit(m)} N/m\nprint inverse(K) to 15 digits")
    assert out.endswith("m/N")
    np.testing.assert_allclose(matrix_numbers(out), np.linalg.inv(np.array(m)), rtol=1e-12, atol=1e-15)


def test_inverse_times_matrix_is_identity():
    out = run(f"Mk = {lit(A4)} kg\nP = inverse(Mk) Mk\nprint P[1, 1]\nprint P[2, 3]\nprint (Mk inverse(Mk))[4, 4]")
    a, b, c = nums(out)
    assert a == pytest.approx(1, abs=1e-14) and abs(b) < 1e-14 and c == pytest.approx(1, abs=1e-14)


def test_solve_linear_against_numpy_and_units():
    out = run(f"K = {lit(A3)} N/m\nF = <1, 2, 3> N\nx = solve_linear(K, F)\nprint x to 15 digits")
    assert out.endswith(" m")
    np.testing.assert_allclose(vector_numbers(out), np.linalg.solve(np.array(A3), [1, 2, 3]), rtol=1e-12)


def test_solve_linear_needs_pivoting():
    # a zero in the top-left corner: plain elimination would divide by 0
    out = run("x = solve_linear([[0, 1], [1, 0]], <2, 3> m)\nprint x")
    assert out == "<3, 2> m"


def test_singular_matrix_is_a_runtime_error():
    msg = str(error_of("print inverse([[1, 2], [2, 4]] N/m)"))
    assert "singular" in msg
    assert "singular" in str(error_of("print solve_linear([[1, 2], [2, 4]], <1, 1>)"))


# ------------------------------------------------------------------ indexing
def test_indexing_is_one_based_and_rows_are_vectors():
    out = run("M = [[1, 2, 3], [4, 5, 6]] m\nprint M[1, 1], M[2, 3], M[2][1]\nprint M[2]\nprint M[end, end]")
    assert out.split("\n") == ["1 m 6 m 4 m", "<4, 5, 6> m", "6 m"]


def test_matrix_in_functions_and_if():
    out = run("rot(θ) = [[cos(θ), -sin(θ)], [sin(θ), cos(θ)]]\nv = <1, 0> m\n"
              "print rot(π/2) v to 3 digits\nsq(A) = A A\nprint sq([[1, 1], [0, 1]])\n"
              "big = true\nM = if big then 2 identity(2) else identity(2)\nprint M")
    assert out.split("\n") == ["<6.12×10⁻¹⁷, 1.00> m", "[[1, 2], [0, 1]]", "[[2, 0], [0, 2]]"]


def test_matrix_variable_can_be_reassigned():
    assert run("M = identity(2)\nM = M [[1, 2], [3, 4]]\nM = M + M\nprint M") == "[[2, 4], [6, 8]]"


# ------------------------------------------------------------------ errors
@pytest.mark.parametrize("src,phrase", [
    ("print [[1 m, 2 m], [3 m, 4 s]]", "all entries of a matrix need the same units"),
    ("print [[1, 2], [3]]", "every row of a matrix needs the same number of entries"),
    ("print [[1, 2, 3, 4, 5], [1, 2, 3, 4, 5]]", "at most 4"),
    ("print [[1, 2], [3, 4]] m + [[1, 2], [3, 4]] s", "can't add matrices of length [m] and time [s]"),
    ("print [[1, 2], [3, 4]] + [[1, 2, 3], [4, 5, 6]]", "2×2 matrix and a 2×3 matrix"),
    ("print [[1, 2], [3, 4]] + 1", "can't add a matrix and a single number"),
    ("print [[1, 2], [3, 4]] * <1, 2, 3>", "a 2×2 matrix times a 3-vector"),
    ("print [[1, 2], [3, 4]] [[1, 2, 3]]", "a 2×2 matrix times a 1×3 matrix"),
    ("print <1, 2> [[1, 2], [3, 4]]", "vector times a matrix"),
    ("print 1 / [[1, 2], [3, 4]]", "can't divide by a matrix"),
    ("print det([[1, 2, 3], [4, 5, 6]])", "square"),
    ("print inverse([[1, 2, 3], [4, 5, 6]])", "square"),
    ("print identity(5)", "identity(n) needs n = 1 to 4"),
    ("n = 3\nprint identity(n)", "a fixed whole number"),
    ("M = identity(2)\nprint M[3, 1]", "row 3"),
    ("M = identity(2)\nprint M[1, 3]", "column 3"),
    ("M = identity(2)\nk = 1\nprint M[k, 1]", "fixed number"),
    ("M = identity(2)\nM = identity(3)", "can't now hold a 3×3 matrix"),
    ("print |identity(2)|", "must be a number"),
    ("print sin(identity(2))", "must be a number"),
    ("print solve_linear(identity(2) * 1 m, <1, 2, 3> N)", "2×2 matrix and a 3-vector"),
    ("print [[1, 2], [3, 4]] × [[1, 2], [3, 4]]", "×"),
])
def test_matrix_errors(src, phrase):
    assert phrase in str(error_of(src))


# ------------------------------------------------------------------ vectors: per-component units
def test_state_vector_keeps_a_unit_per_component():
    out = run("s = <1 m, 2 m/s>\nprint s\nprint s.x, s.y, s[2]\nprint s * 2\nprint 2 kg * s\nprint s / (2 s)")
    assert out.split("\n") == ["<1 m, 2 m/s>", "1 m 2 m/s 2 m/s", "<2 m, 4 m/s>", "<2 kg m, 4 kg m/s>",
                               "<0.5 m/s, 1 m/s²>"]


def test_state_vectors_add_component_by_component():
    out = run("s = <1 m, 2 m/s, 3 kg>\nq = <10 m, 20 m/s, 30 kg>\nprint s + q\nprint q - s\n"
              "s = s + q\nprint s")
    assert out.split("\n") == ["<11 m, 22 m/s, 33 kg>", "<9 m, 18 m/s, 27 kg>", "<11 m, 22 m/s, 33 kg>"]


def test_state_vector_in_a_function():
    out = run("advance(s, dt) = s + <s.y dt, -ω² s.x dt> where ω = 2 rad/s\n"
              "print advance(<1 m, 0 m/s>, 0.1 s)")
    assert out == "<1.0 m, -0.40 m/s>"      # 0.1 s has 1 significant figure; shown with 2 (D11)


@pytest.mark.parametrize("src,phrase", [
    ("print <1 m, 2 m/s> + <1, 2> m", "component 2 is speed [m/s] on one side and length [m] on the other"),
    ("print <1 m, 2 m/s> + <1 m, 2 s>", "component 2"),
    ("print |<1 m, 2 m/s>|", "|v| needs all components of the vector in the same units"),
    ("print norm(<1 m, 2 m/s>)", "needs all components of the vector in the same units"),
    ("print <1 m, 2 m/s> · <1 m, 2 m/s>", "dot product needs all components"),
    ("print <1 m, 2 m/s, 3 s> × <1, 2, 3> m", "cross product needs all components"),
    ("print unit(<1 m, 2 s>)", "same units"),
    ("print <1 m, 2 m/s> in cm", "can't show a vector with different units per component in cm"),
    ("print identity(2) * <1 m, 2 m/s>", "same units"),
    ("s = <1 m, 2 m/s>\ns = <1 m, 2 m>", "s is"),
    ("print <1 m, 2 m/s> + 1 m", "can't add a vector and a single number"),
    ("solve x'' = -x / (1 s²)\n  with x(0) = <1 m, 2 s>, x'(0) = <0, 0> m/s\n  for t from 0 s to 1 s",
     "same units"),
])
def test_mixed_vector_errors(src, phrase):
    assert phrase in str(error_of(src))


def test_uniform_vectors_unchanged():
    assert run("v = <3, 4> m/s\nprint v, |v|\nprint <1 m, 2 m, 3 m> in cm") == "<3, 4> m/s 5 m/s\n<100, 200, 300> cm"


def test_four_vectors():
    out = run("x = <1, 2, 3, 4> m\nprint x, x[4], x.x, |x| to 4 digits\nprint x · <1, 1, 1, 1> m")
    assert out.split("\n") == ["<1, 2, 3, 4> m 4 m 1 m 5.477 m", "10 m²"]


# ------------------------------------------------------------------ compiled == interpreted
PROGRAM = f"""
K = {lit(A4)} N/m
F = <1, -2, 0.5, 3> N
x = solve_linear(K, F)
print x to 17 digits
print inverse(K) to 17 digits
print det(K) to 17 digits
print K Kᵀ to 17 digits
print K x - F to 17 digits
s = <1 m, 2 m/s>
print s + <0.1 m, 0.2 m/s>, 3 s
M = [[1, 2], [3, 4]] kg
print M[2, 1], M[1], M * <1, 1> m/s
"""


def test_native_and_interpreter_agree_on_matrices():
    assert run(PROGRAM) == interp(PROGRAM)


def test_interpreter_singular_error_matches():
    src = "print inverse([[1, 2], [2, 4]])"
    with pytest.raises(Exception) as a:
        run(src)
    with pytest.raises(Exception) as b:
        interp(src)
    assert a.value.message == b.value.message


def test_repl_holds_vectors_and_matrices():
    """Vectors and matrices in REPL slots at any offset (they used to segfault: misaligned vector loads)."""
    from fermium.driver import ReplSession
    out = io.StringIO()
    s = ReplSession(out=out)
    for line in ["x = 1", "v = <1, 2, 3> m", "M = [[1, 2], [3, 4]] N/m", "print v + v", "print M M"]:
        s.execute(line)
    assert out.getvalue().strip().split("\n")[-2:] == ["<2, 4, 6> m", "[[7, 10], [15, 22]] N²/m²"]


def test_vectors_and_matrices_captured_by_an_integral():
    """A function's local vector/matrix used inside ∫ is passed to the integrand (n slots each)."""
    out = run("f(k) = ∫ (M * <x, 1> m) · <1, 1> m dx from 0 to k where M = [[1, 2], [3, 4]]\nprint f(2)\n"
              "g(a) = ∫ (v · v) x dx from 0 to 1 where v = <a, 2 a, 3 a>\nprint g(1 m)")
    assert out.split("\n") == ["20 m²", "7 m²"]
    assert interp("f(k) = ∫ (M * <x, 1> m) · <1, 1> m dx from 0 to k where M = [[1, 2], [3, 4]]\n"
                  "print f(2)") == "20 m²"


def test_vector_captured_by_an_ode_inside_a_function():
    src = ("f(k) =\n    F = <k, 2 k>\n    solve x'' = F · <1, 1> / (1 kg)\n      with x(0) = 0 m, x'(0) = 0 m/s\n"
           "      for t from 0 s to 1 s\n    return x(1 s)\nprint f(1 N) to 6 digits")
    assert run(src) == interp(src) == "1.50000 m"


def test_fmt_ascii_warns_about_transpose():
    from fermium.fmt import format_source
    from fermium.errors import Diagnostics
    d = Diagnostics()
    out = format_source("M = identity(2)\nprint Mᵀ\n", "ascii", d)
    assert "Mᵀ" in out
    assert any("transpose" in w.message for w in d.warnings)


def test_built_executable_prints_matrices(tmp_path):
    import subprocess
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "prog")
    build(PROGRAM, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == run(PROGRAM)
    build("print inverse([[1, 2], [2, 4]])", str(tmp_path / "bad.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    assert got.returncode != 0 and "singular" in got.stderr
