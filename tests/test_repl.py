"""The REPL, driven non-interactively through fermium.repl.main(stdin=..., stdout=...)."""
import io

import pytest

from fermium.repl import main
from fermium.symbols import complete, expand_all


def repl(text):
    out = io.StringIO()
    code = main(stdin=io.StringIO(text), stdout=out)
    assert code == 0
    return out.getvalue()


def repl_lines(text):
    return [ln for ln in repl(text).split("\n") if ln.strip()]


# ------------------------------------------------------------------ the spec's snippet
def test_pendulum_snippet():
    out = repl_lines("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g\n")
    assert out == ["9.70 m/s²"]


def test_pendulum_snippet_ascii_and_conversion():
    out = repl_lines("L = 1.20 m\nT = 2.21 s\ng = 4 pi^2 L / T^2\nprint g\nprint g in ft/s²\n")
    assert out == ["9.70 m/s²", "31.8 ft/s²"]


def test_variables_persist_between_inputs():
    assert repl_lines("x = 2 m\ny = 3 m\nprint x + y\n") == ["5 m"]


def test_functions_persist():
    assert repl_lines("F(x) = 50 N/m * x\nprint F(0.2 m)\n") == ["10 N"]


def test_calculus_in_repl():
    out = repl_lines("x(t) = 0.1 m cos(10 t / 1 s)\nv = d/dt x\nprint v(0 s)\n")
    assert out[-1] in ("0 m/s", "-0 m/s")


def test_solve_in_repl():
    out = repl_lines("solve x' = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 s\nprint x(1 s)\n")
    assert out == ["0.367879 kg"]


# ------------------------------------------------------------------ multi-line blocks
def test_for_block():
    assert repl_lines("for i from 1 to 3\n    print i\n\nprint \"done\"\n") == ["1", "2", "3", "done"]


def test_if_block_single_line_body():
    assert repl_lines("x = 3 m\nif x > 2 m\n    print \"far\"\n\n") == ["far"]


def test_multiline_function_two_body_lines():
    src = "f(x) =\n    y = 2 x\n    y + 1 m\n\nprint f(3 m)\n"
    assert repl_lines(src) == ["7 m"]


def test_multiline_function_with_return():
    src = "speed(h) =\n    g = 9.81 m/s²\n    return √(2*g*h)\n\nprint speed(10 m)\n"
    assert repl_lines(src) == ["14.0 m/s"]


def test_if_else_block():
    src = "g = 9.81 m/s²\nif g > 1 m/s²\n    print \"big\"\nelse\n    print \"small\"\n\nprint 1\n"
    assert repl_lines(src) == ["big", "1"]


def test_while_block_with_two_lines():
    src = "n = 0\nwhile n < 3\n    n += 1\n    print n\n\n"
    assert repl_lines(src) == ["1", "2", "3"]


def test_block_at_end_of_input_without_blank_line():
    assert repl_lines("for i from 1 to 2\n    print i\n") == ["1", "2"]


# ------------------------------------------------------------------ errors don't end the session
def test_unit_error_then_next_line_works():
    out = repl("x = 3 m + 2 s\nprint 5\n")
    lines = out.strip().split("\n")
    assert lines[0] == "line 1: can't add length [m] to time [s]"
    assert "^" in out
    assert lines[-1] == "5"
    assert "Traceback" not in out


def test_open_bracket_continues_like_python():
    # an unclosed bracket continues onto the next line (as in Python), so this is one input
    out = repl("x = (1 +\n2)\nprint x\n")
    assert out.strip() == "3"


def test_parse_error_then_next_line_works():
    out = repl("x = ) 1\nprint 7\n")
    assert out.strip().split("\n")[-1] == "7"
    assert "Traceback" not in out


def test_undefined_name_then_next_line_works():
    out = repl("print nope\ny = 2\nprint y\n")
    assert "nope isn't defined" in out
    assert out.strip().split("\n")[-1] == "2"


def test_runtime_error_then_next_line_works():
    out = repl("xs = [1, 2]\nprint xs[5]\nprint xs[1]\n")
    assert "out of range" in out
    assert out.strip().split("\n")[-1] == "1"


def test_failed_definition_does_not_break_later_ones():
    out = repl("a = 1 m + 1 s\na = 2 m\nprint a\n")
    assert out.strip().split("\n")[-1] == "2 m"


def test_redefinition_allowed_in_repl():
    # D13: the REPL allows redefining a variable with different units
    assert repl_lines("x = 1 m\nx = 2 s\nprint x\n") == ["2 s"]


def test_no_internal_errors_on_garbage():
    out = repl("$$$\n)\nx = = 2\nprint 3\n")
    assert "internal error" not in out
    assert "Traceback" not in out
    assert out.strip().split("\n")[-1] == "3"


# ------------------------------------------------------------------ commands
def test_quit_stops_reading():
    assert repl_lines("print 1\n:quit\nprint 2\n") == ["1"]


def test_help():
    out = repl(":help\n")
    assert "\\theta" in out or "Examples" in out


def test_vars_lists_names():
    out = repl("speed = 3 m/s\n:vars\n")
    assert "speed" in out


# ------------------------------------------------------------------ \name expansion
def test_backslash_theta_expanded_on_enter():
    assert repl_lines("\\theta = 3\nprint θ\n") == ["3"]


def test_backslash_symbols_in_expression():
    assert repl_lines("print \\sqrt(16) + 2\\pi - 2 pi\n") == ["4"]
    assert repl_lines("x = 3\nprint x\\^2\n") == ["9"]


def test_expand_all():
    assert expand_all("\\omega_0 = \\sqrt(k/m)") == "ω_0 = √(k/m)"
    assert expand_all("x\\^2 + y\\_0") == "x² + y₀"
    assert expand_all("\\notasymbol") == "\\notasymbol"


@pytest.mark.parametrize("frag,sym", [
    ("\\the", "θ"), ("\\theta", "θ"), ("\\hbar", "ħ"), ("\\int", "∫"), ("\\sqrt", "√"),
    ("\\^2", "²"), ("\\omega", "ω"), ("\\pi", "π"), ("\\partial", "∂"), ("\\pm", "±"),
    ("\\infty", "∞"), ("\\le", "≤"), ("\\AA", "Å"), ("\\deg", "°"), ("\\cdot", "·"),
])
def test_complete(frag, sym):
    assert complete(frag)[0] == sym


def test_complete_keeps_prefix():
    assert complete("x = 2\\pi") == ["x = 2π"]


def test_complete_prefix_offers_candidates():
    cands = complete("\\ep")
    assert "ε" in cands


def test_complete_nothing():
    assert complete("theta") == []
    assert complete("\\zzz") == []
