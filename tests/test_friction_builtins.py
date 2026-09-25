"""Gauntlet friction fixes: built-ins and small syntax (gauntlet/FRICTION.md #49, #50, #52, #53, #54, #55, #57)."""
import io

from conftest import run, error_of
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
