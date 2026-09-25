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
        ["2500 μrad", "0.003 mrad", "1000 rad/s"]


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
        ["<0, 0> 1/s", "<1, 2> 1/s", "<0.25, 0.5> 1/m"]


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
    src = ("a = <1, 0, 0> m\nb = <1, 1, 0> m\nprint angle(a, b) in °\nprint angle(<1, 0> m, <-1, 1e-9> m)\n"
           "F = <1, 1, 0> N\nprint angle(F, a) in °\nprint angle(<1, 0, 0>, <1, 1e-12, 0>)")
    assert both(src).split("\n") == ["45°", "3.14159", "45°", "1×10⁻¹²"]
    assert "2-vectors or two 3-vectors" in error_of("print angle(<1, 0>, <1, 0, 0>)").message


def test_54_elementwise_abs():
    assert both("v = <-1, 2, -3> m/s\nprint abs(v)\nprint abs([[1, -2], [-3, 4]])").split("\n") == \
        ["<1, 2, 3> m/s", "[[1, 2], [3, 4]]"]
