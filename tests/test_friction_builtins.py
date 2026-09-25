"""Gauntlet friction fixes: built-ins and small syntax (gauntlet/FRICTION.md #49, #50, #52, #53, #54, #55, #57)."""
import io

import pytest

from conftest import run, error_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    """Run natively and in the reference interpreter; both must print the same."""
    a, b = run(src), interp(src)
    assert a == b, (a, b)
    return a


# ---------------------------------------------------------------- #52 prefixes on rad
def test_52_mrad_and_urad():
    assert both("θ = 2.5 mrad\nprint θ in μrad\nprint 3 μrad in mrad\nprint 1 krad/s in rad/s").split("\n") == \
        ["2500 μrad", "0.00300 mrad", "1000 rad/s"]


# ---------------------------------------------------------------- #57 chained comparisons
def test_57_chained_comparisons():
    src = ("x = 3 m\nprint 1 m < x < 5 m, 1 m < x < 2 m\nprint 0 < 1 <= 1 < 2\n"
           "f(y) = y^2\nprint 0 < f(x/(1 m)) < 10, 0 <= f(x/(1 m)) < 10 <= 10\n"
           "g(t) = if 0 s < t < 1 s then 1 else 0\nprint g(0.5 s) == 1, g(2 s) == 0\nprint 1 == 1 == 1")
    assert both(src).split("\n") == ["true false", "true", "true true", "true true", "true"]


def test_57_middle_evaluated_once():
    src = "n = 0\ntick(x) = x + 1\nk = 0\nwhile 0 < tick(k) < 4\n    k = k + 1\nprint k"
    assert both(src) == "3"


def test_57_unit_error_names_the_pair():
    e = error_of("x = 3 m\nprint 1 m < x < 2 s")
    assert "can't compare length [m] with time [s]" in e.message


def test_57_mixed_chain_refused():
    assert "chain" in error_of("print 1 < 2 != 3").message


# ---------------------------------------------------------------- #53 if-expression over several lines
def test_53_if_expression_continues_on_indented_else_lines():
    src = ("g = 9.8 m/s^2\nt1 = 2 s\ntc = 3 s\n"
           "u(t) = if t < t1 then g t\n"
           "       else if t < t1 + tc then g t1\n"
           "       else g t1 - g (t - t1 - tc)\n"
           "print u(1 s), u(3 s), u(6 s)\n"
           "x = 2\nif x > 1\n    print \"big\"\nelse\n    print \"small\"\n")
    assert both(src).split("\n") == ["9.8 m/s 20 m/s 9.8 m/s", "big"]


def test_53_unindented_else_gets_a_hint():
    e = error_of("x = 1\nv = if x > 1 then 1\nelse 0\nprint v")
    assert "indent the line that starts with else" in e.hint


# ---------------------------------------------------------------- #55 units after names and vectors
def test_55_unit_after_vector_literal_with_slash():
    assert both("a = <0, 0> /s\nprint a\nb = <1, 2> 1/s\nprint b\nx = 4 m\nprint <1, 2> / x").split("\n") == \
        ["<0, 0> 1/s", "<1, 2> 1/s", "<0.250, 0.500> 1/m"]


def test_55_unit_after_a_name_hint():
    e = error_of("A_d = 4\nm_a = 4 u\nμ = m_a A_d u / (m_a + A_d u)\nprint μ")
    assert "u isn't defined" in e.message
    assert "A_d * 1 u" in e.hint and "[u]" in e.hint


# ---------------------------------------------------------------- #54 trace, angle, abs, row/column, v[i]
def test_54_vector_indexed_by_a_loop_variable():
    src = ("v = <1, 2, 3> m\ns = 0 m\nfor i from 1 to 3\n    s = s + v[i]\nprint s\n"
           "M = [[1, 2], [3, 4]]\nt = 0\nfor i from 1 to 2\n    for j from 1 to 2\n        t = t + M[i, j] * i\n"
           "print t\nfor i from 1 to 2\n    print M[i]\nk = 2\nprint v[k], v[end], v[k + 1]\nf(n) = v[n]\nprint f(3)")
    assert both(src).split("\n") == ["6 m", "17", "<1, 2>", "<3, 4>", "2 m 3 m 3 m", "3 m"]


def test_54_eigenvalues_indexed_in_a_loop():
    src = ("K = [[2, -1], [-1, 2]] N/m\nM = [[1, 0], [0, 1]] kg\nω2 = eigenvalues(K, M)\n"
           "for n from 1 to 2\n    print √(ω2[n]) to 6 digits")
    assert both(src).split("\n") == ["1.00000 1/s", "1.73205 1/s"]


@pytest.mark.parametrize("idx", ["k + 2", "k - 2", "k / 4"])
def test_54_runtime_index_is_checked(idx):
    src = f"v = <1, 2, 3> m\nk = 2\nprint v[{idx}]"
    for runner in (run, interp):
        with pytest.raises(FermiumError) as ei:
            runner(src)
        assert "1 to 3" in str(ei.value) or "whole number" in str(ei.value)


def test_54_runtime_index_of_mixed_vector_refused():
    assert "different units" in error_of("s = <1 m, 2 m/s>\nk = 1\nprint s[k]").message


def test_54_trace_row_column():
    src = ("M = [[1, 2, 3], [4, 5, 6], [7, 8, 10]] N/m\nprint trace(M)\nprint column(M, 2), row(M, 3)\n"
           "for j from 1 to 3\n    print column(M, j)\n"
           "f(θ) = [[cos(θ), -sin(θ)], [sin(θ), cos(θ)]]\nprint ∫ trace(f(θ)) dθ from 0 to 1 to 10 digits")
    assert both(src).split("\n") == ["16 N/m", "<2, 5, 8> N/m <7, 8, 10> N/m", "<1, 4, 7> N/m", "<2, 5, 8> N/m",
                                     "<3, 6, 10> N/m", "1.682941970"]
    assert "square" in error_of("print trace([[1, 2, 3], [4, 5, 6]])").message
    assert "no column 4" in error_of("print column([[1, 2], [3, 4]], 4)").message


def test_54_angle_between_vectors():
    src = ("a = <1, 0, 0> m\nb = <1, 1, 0> m\nprint angle(a, b) in °\nprint angle(<1, 0> m, <-1, 1e-9> m) to 6 digits\n"
           "F = <1, 1, 0> N\nprint angle(F, a) in °\nprint angle(<1, 0, 0>, <1, 1e-12, 0>)")
    assert both(src).split("\n") == ["45°", "3.14159", "45°", "1.00×10⁻¹²"]
    assert "2-vectors or two 3-vectors" in error_of("print angle(<1, 0>, <1, 0, 0>)").message


def test_54_elementwise_abs():
    assert both("v = <-1, 2, -3> m/s\nprint abs(v)\nprint abs([[1, -2], [-3, 4]])").split("\n") == \
        ["<1, 2, 3> m/s", "[[1, 2], [3, 4]]"]


# ---------------------------------------------------------------- #50 Bessel functions, elliptic integrals
BESSEL_CASES = [(n, x) for n in (0, 1, 2, 5, 12) for x in (0.01, 0.5, 1.0, 2.5, 7.0, 30.0, 150.0)]


def _values(out):
    return [float(t.replace("×10", "e").translate(str.maketrans("⁻⁰¹²³⁴⁵⁶⁷⁸⁹", "-0123456789")))
            for t in out.split()]


@pytest.mark.parametrize("fname,ref", [("besselj", "jv"), ("bessely", "yv"), ("besseli", "iv"), ("besselk", "kv")])
def test_50_bessel_against_scipy(fname, ref):
    sc = pytest.importorskip("scipy.special")
    src = "\n".join(f"print {fname}({n}, {x}) to 16 digits" for n, x in BESSEL_CASES)
    want = [float(getattr(sc, ref)(n, x)) for n, x in BESSEL_CASES]
    for runner in (run, interp):
        got = _values(runner(src))
        for (n, x), g, w in zip(BESSEL_CASES, got, want):
            assert g == pytest.approx(w, rel=1e-11, abs=1e-300), (fname, n, x)


def test_50_bessel_negative_arguments_and_orders():
    sc = pytest.importorskip("scipy.special")
    src = "print besseli(3, -2) to 16 digits, besselj(-3, 2) to 16 digits, bessely(-2, 3) to 16 digits"
    want = [sc.iv(3, -2), sc.jv(-3, 2), sc.yv(-2, 3)]
    for runner in (run, interp):
        assert _values(runner(src)) == pytest.approx(want, rel=1e-13)


def test_50_elliptic_against_scipy():
    sc = pytest.importorskip("scipy.special")
    ms = [-5.0, -0.5, 0.0, 0.1, 0.5, 0.9, 0.99, 0.999999, 1 - 1e-12]
    src = "\n".join(f"print ellipk({m!r}) to 16 digits, ellipe({m!r}) to 16 digits" for m in ms)
    want = [v for m in ms for v in (sc.ellipk(m), sc.ellipe(m))]
    for runner in (run, interp):
        assert _values(runner(src)) == pytest.approx(want, rel=1e-14)
    assert both("print ellipe(1), ellipk(1), besselk(0, 0)") == "1 ∞ ∞"


def test_50_derivatives():
    # J1' = (J0 − J2)/2, K1' = −K0 − K1/x, and dK/dm, dE/dm against a central difference of SciPy's values
    sc = pytest.importorskip("scipy.special")
    src = ("J1p(x) = d/dx besselj(1, x)\nprint J1p(2) to 15 digits\n"
           "Kd(x) = d/dx besselk(1, x)\nprint besselk(0, 2) + Kd(2) + besselk(1, 2) / 2\n"
           "Kp(m) = d/dm ellipk(m)\nEp(m) = d/dm ellipe(m)\nprint Kp(0.3) to 15 digits, Ep(0.3) to 15 digits\n"
           "I0p(x) = d/dx besseli(0, x)\nprint I0p(1.5) to 15 digits")
    got = _values(both(src))
    h = 1e-5
    assert got[0] == pytest.approx(sc.jvp(1, 2), rel=1e-13)
    assert abs(got[1]) < 1e-15
    assert got[2] == pytest.approx((sc.ellipk(0.3 + h) - sc.ellipk(0.3 - h)) / (2 * h), rel=1e-9)
    assert got[3] == pytest.approx((sc.ellipe(0.3 + h) - sc.ellipe(0.3 - h)) / (2 * h), rel=1e-9)
    assert got[4] == pytest.approx(sc.iv(1, 1.5), rel=1e-13)


def test_50_errors():
    assert "whole-number order" in error_of("print besselj(1.5, 2)").message
    assert "plain numbers" in error_of("print besselj(1, 2 m)").message
    assert "takes 2 arguments" in error_of("print besselk(2)").message


def test_50_off_axis_ring_by_elliptic_integrals():
    # Jackson's charged ring: the potential off the axis as an integral and with K(m)
    src = ("a = 1 m\nλ = 1 nC/m\nk = 1/(4π ε₀)\n"
           "V(r, z) = ∫ k λ a / √(r² + a² + z² - 2 r a cos(φ)) dφ from 0 to 2π\n"
           "Vk(r, z) = 4 k λ a ellipk(4 a r / ((a + r)² + z²)) / √((a + r)² + z²)\n"
           "print V(0.3 m, 0.2 m) / Vk(0.3 m, 0.2 m) to 12 digits")
    assert both(src) == "1.00000000000"


def test_50_pure_python_fallback_for_j(monkeypatch):
    # without a C library, SciPy or mpmath (a bare browser playground) J_n comes from Bessel's integral
    import sys
    from fermium import special
    sc = pytest.importorskip("scipy.special")
    want = {(n, x): float(sc.jv(n, x)) for n, x in [(0, 1.0), (3, 7.5), (1, 40.0), (2, -3.0)]}
    for mod in ("scipy", "scipy.special", "mpmath"):
        monkeypatch.setitem(sys.modules, mod, None)
    for (n, x), w in want.items():
        assert special._bessel_fallback("jv", n, x) == pytest.approx(w, rel=1e-12, abs=1e-15)


# ---------------------------------------------------------------- #49 one-line sums
def test_49_sums():
    src = ("print Σ(k² for k from 1 to 10)\nprint sum(1/k² for k from 1 to 1000) to 8 digits\n"
           "N = 5\nS(N) = Σ(k for k from 1 to N)\nprint S(4), S(N)\nprint Σ(k for k from 1 to 9 step 2)\n"
           "xs = [1 m, 2 m, 3 m]\nprint Σ(xs), sum(xs)\nprint Σ(r for r from 1 m to 3 m step 1 m)\n"
           "print Σ(<k, k²> m for k from 1 to 3)\nprint Σ(k for k from 5 to 1), Σ(k for k from 3 to 1 step -1)")
    assert both(src).split("\n") == ["385", "1.6439346", "10 15", "25", "6 m 6 m", "6 m", "<6, 14> m", "0 6"]


def test_49_fourier_series_derivative_and_laplacian():
    import math
    src = ("f(x) = Σ(sin(n x)/n for n from 1 to 49 step 2)\ng = f'\nprint f(1) to 12 digits, g(1) to 12 digits\n"
           "print g\n"
           "a = 1 m\nV0 = 10 V\n"
           "term(n, x, y) = 4 V0 / (n π) * sin(n π x / a) sinh(n π y / a) / sinh(n π)\n"
           "φ(x, y) = Σ(term(n, x, y) for n from 1 to 61 step 2)\n"
           "print φ(0.5 m, 0.5 m) to 8 digits\nlap = ∇²φ\nprint lap(0.3 m, 0.7 m)\n"
           "u(x, y) = Σ(sin(n x) sin(n y) / n² for n from 1 to 7)\nL = ∇²u\nprint L(0.4, 1.1) to 12 digits")
    out = both(src).split("\n")
    f1 = sum(math.sin(n) / n for n in range(1, 50, 2))
    g1 = sum(math.cos(n) for n in range(1, 50, 2))
    assert [float(v) for v in out[0].split()] == pytest.approx([f1, g1], rel=1e-11)
    assert out[1] == "g(x) = Σ(cos(n x) for n from 1 to 49 step 2)"
    assert out[2:4] == ["2.5000000 V", "0 V/m²"]
    lap = -2 * sum(math.sin(n * 0.4) * math.sin(n * 1.1) for n in range(1, 8))
    assert float(out[4]) == pytest.approx(lap, rel=1e-11)


def test_49_sum_errors():
    assert "limits depend on N" in error_of("F(x, N) = Σ(sin(k x) for k from 1 to N)\nG = ∂/∂N F\nprint G(1, 2)").message
    assert "needs a step" in error_of("print Σ(r for r from 1 m to 3 m)").message
    assert "finite number of terms" in error_of("print Σ(1/k² for k from 1 to ∞)").message
    assert "expected 'from'" in error_of("print Σ(k² for k in 1)").message


def test_50_airy_pattern_with_besselj():
    # the first bright ring of the Airy pattern: I'(x) = 0 with I = (2 J1(x)/x)², at x = 5.1356223
    src = "I(x) = (2 besselj(1, x) / x)^2\nsolve I'(x) = 0 for x from 4 to 6\nprint x to 8 digits\n" \
          "solve besselj(1, x) = 0 for x from 3 to 4.5\nprint x / π to 6 digits"
    assert both(src).split("\n") == ["5.1356223", "1.21967"]
