"""Parser tests: every construct in docs/reference.md and every ambiguity rule in DECISIONS D7/D8.

Behaviour is checked mostly by running programs; the precedence rules are also checked on
the AST shape (see astdump.sx for the s-expression notation).
"""
import pytest

from astdump import expr_of, sx_of
from conftest import run, run_lines, error_of, warnings_of
from fermium import ast as A
from fermium.errors import Diagnostics, FermiumError
from fermium.parser import parse


def stmt(src, i=0):
    return parse(src, Diagnostics()).body[i]


# ====================================================================== D8: implicit multiplication
def test_textbook_fraction_shape():
    assert sx_of("h c / λ k_B T") == "(/ (* h c) (* (* λ k_B) T))"


def test_textbook_fraction_value():
    assert run("h = 2\nc = 3\nλ = 4\nk_B = 5\nT = 6\nprint h c / λ k_B T") == "0.0500"


def test_one_over_2x_is_a_coefficient():
    # A2 (D236): a fraction of pure numbers is one coefficient; D8 read 1/(2x) with a warning
    assert sx_of("1/2x") == "(* (/ 1 2) x)"
    assert run("x = 4\nprint 1/2x") == "2"
    assert warnings_of("x = 4\ny = 1/2x") == []


def test_half_written_with_parentheses_does_not_warn():
    assert warnings_of("x = 4\ny = (1/2) x") == []
    assert run("x = 4\nprint (1/2) x") == "2"


def test_one_half_a_b_squared():
    assert sx_of("1/2 a b²") == "(* (* (/ 1 2) a) (^ b 2))"          # A2 (D236)
    assert run("a = 2\nb = 3\nprint 1/2 a b²") == "9"


def test_one_half_m_v_squared_is_ambiguous():
    # `2 m v²` with a mass m: `2 m` is ambiguous (the A1 rule, D235)
    assert "is ambiguous" in str(error_of("m = 2 kg\nv = 3 m/s\nE = 1/2 m v²"))
    assert run("mass = 2 kg\nv = 3 m/s\nprint 1/2 mass v²") == "9 J"          # A2 (D236)


def test_four_pi_squared_L():
    assert sx_of("4π² L") == "(* (* 4 (^ π 2)) L)"
    assert run("L = 1\nprint 4π² L to 6 digits") == "39.4784"


def test_pi_caret_two_then_name():
    # a number right after ^ never takes a unit: pi^2 L = π²·L
    assert run("L = 1\nprint 4 pi^2 L to 6 digits") == "39.4784"


def test_unary_minus_covers_whole_product():
    assert sx_of("-A ω sin(ω t)") == "(neg (* (* A ω) (call sin (* ω t))))"
    assert run("A = 2\nω = 3\nt = 0.5\nprint -A ω sin(ω t) == -(A*ω*sin(ω*t))") == "true"


def test_power_binds_tighter_than_juxtaposition():
    assert sx_of("x^2y") == "(^ x 2)" or sx_of("x^2y") == "(* (^ x 2) y)"
    assert sx_of("x^2y") == "(* (^ x 2) y)"
    assert run("x = 2\ny = 3\nprint x^2y") == "12"
    assert run("x = 2\ny = 3\nprint x²y") == "12"


def test_power_is_right_associative():
    assert sx_of("2^3^2") == "(^ 2 (^ 3 2))"
    assert run("print 2^3^2") == "512"


def test_negative_exponent():
    assert run("print 2^-1") == "0.500"
    assert run("print 10^-2") == "0.0100"


def test_explicit_star_and_slash_left_to_right():
    assert sx_of("a / b * c") == "(* (/ a b) c)"
    assert run("print 8 / 2 * 4") == "16"


def test_plus_minus_lowest_arithmetic():
    assert sx_of("a + b c") == "(+ a (* b c))"
    assert run("print 1 + 2 * 3") == "7"


def test_2x_multiplies():
    assert run("x = 3\nprint 2x") == "6"


def test_number_times_paren():
    assert run("k = 2\nx = 3\nprint k(x+1)") == "8"


def test_space_before_paren_is_multiplication_for_variables():
    assert run("y = 3\nprint y (2)") == "6"


def test_space_before_paren_is_call_for_functions():
    assert run("f(x) = x + 1\nprint f (2)") == "3"


def test_LT_is_one_identifier():
    assert sx_of("LT") == "LT"
    assert run("LT = 3\nprint LT") == "3"


def test_LT_error_suggests_L_T():
    e = error_of("L = 2\nT = 3\nprint LT")
    assert "LT" in e.message
    assert e.line == 3
    assert "L T" in (e.hint or "")


def test_omega_t_error_suggests_space():
    e = error_of("ω = 2\nt = 3\nprint ωt")
    assert "ω t" in (e.hint or "")


# ====================================================================== D7: units vs variables
def test_unit_after_digit_literal():
    assert run("print 3 m") == "3 m"
    q = expr_of("3 m")
    assert isinstance(q, A.Quantity) and q.unit.factors[0].name == "m"


def test_half_m_v_squared_is_variable_m():
    assert run("m = 2 kg\nv = 3 m/s\nprint ½ m v²") == "9 J"
    # with your m, `½ m` is your m; ½ reads exactly like the bracket (1/2) (D241): without any m in the
    # program, `½ m` would be half a metre, as `(1/2) m` is
    prog = parse("m = 2 kg\nv = 3 m/s\n__y = ½ m v²", Diagnostics())
    assert not any(isinstance(n, A.Quantity) for n in A.walk(prog.body[2].value))
    def has_q(src):
        return any(isinstance(n, A.Quantity) for n in A.walk(expr_of(src)))
    assert has_q("½ m v²") == has_q("(1/2) m v²")


def test_bracket_unit_after_number():
    assert run("print 3 [m/s]") == "3 m/s"
    q = expr_of("3 [m/s]")
    assert isinstance(q, A.Quantity) and q.bracket


def test_bracket_unit_after_variable():
    assert run("x = 3\nprint x [m]") == "3 m"
    q = expr_of("x [m]")
    assert isinstance(q, A.Quantity) and isinstance(q.value, A.Name)


def test_bracket_unit_on_quantity_is_error_with_hint():
    e = error_of("x = 3 m\nprint x [cm]")
    assert "already has units" in e.message
    assert "in cm" in (e.hint or "")


def test_name_not_after_number_is_variable():
    e = error_of("print m")
    assert "m isn't defined" in e.message
    assert "unit" in (e.hint or "")


def test_compound_units():
    assert run("print 9.81 m/s²") == "9.81 m/s²"
    assert run("print 2 N m") == "2 N m"
    assert run("print 2 N·m") in ("2 N m", "2 N·m")   # shown as written
    assert run("print 2 N·m == 2 N m") == "true"
    assert run("print 6.674e-11 N m²/kg²") == "6.674×10⁻¹¹ N m²/kg²"
    assert run("print 2 s⁻¹") in ("2 s⁻¹", "2 1/s")   # displayed in a canonical spelling
    assert run("print 2 s^-1 == 2 s⁻¹") == "true"


def test_unit_collision_is_an_error_even_alone():
    # A1 rule, sentence 2 (D235; it was a warning under D7)
    e = error_of("m = 0.5 kg\ny = 0.2 m")
    assert "'0.2 m' is ambiguous" in e.message and "0.2 [m]" in e.hint and "0.2*m" in e.hint
    assert warnings_of("m = 0.5 kg\ny = 0.2 [m]\nz = 0.3 [m]") == []


def test_unit_collision_still_means_unit():
    assert run("m = 0.5 kg\ny = 0.2 [m]\nprint y") == "0.2 m"


def test_no_collision_warning_without_variable():
    assert warnings_of("y = 0.2 m") == []


def test_space_continued_unit_naming_your_variable_is_an_error():
    # `70 kg g` with your own g: a later name of the unit is your variable (A1 sentence 3, D235; D7.4 read g)
    e = error_of("g = 2\nprint 70 kg g")
    assert "'70 kg g' is ambiguous" in e.message and "(70 kg) g" in e.hint and "70 [kg g]" in e.hint
    assert run("g = 2\nprint 70 [kg] g") == "140 kg"
    assert warnings_of("g = 2\ny = 70 [kg] g") == []


def test_slash_continued_unit_naming_your_variable_is_an_error():
    # D7 rule 4 read the unit here; the A1 rule asks (D235)
    assert "'3 m/s' is ambiguous" in error_of("s = 5 kg\nprint 3 m/s").message
    assert run("s = 5 kg\nprint 3 [m/s]") == "3 m/s"


def test_unit_then_variable():
    assert run("t = 2 s\nprint 3 m/s t") == "6 m"


# ====================================================================== calculus syntax
def test_d_dt_shape():
    d = expr_of("d/dt x")
    assert isinstance(d, A.Deriv) and d.var == "t" and d.order == 1 and not d.partial


def test_d2_dt2_shape():
    d = expr_of("d²/dt² x")
    assert isinstance(d, A.Deriv) and d.order == 2


def test_prime_shape():
    p = expr_of("x''")
    assert isinstance(p, A.Prime) and p.order == 2


def test_partial_shape():
    d = expr_of("∂/∂x f")
    assert isinstance(d, A.Deriv) and d.partial and d.var == "x"
    assert sx_of("partial/partial x f") == sx_of("∂/∂x f")


def test_derivatives_run():
    src = ("A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nv = d/dt x\na = x''\n"
           "print v\nprint a\nprint v(0.1 s)")
    assert run_lines(src) == ["v(t) = -A ω sin(ω t)   [m/s, for t in s]",
                              "a(t) = -A ω² cos(ω t)   [m/s², for t in s]",
                              "-0.84 m/s"]


def test_d_dt_equals_prime():
    src = "f(t) = t^3\ng = d/dt f\nh = f'\nprint g(2) == h(2), g(2)"
    assert run(src) == "true 12"


def test_second_derivative_forms_agree():
    src = "f(t) = sin(t)\na = d²/dt² f\nb = f''\nprint a(1) ~= b(1), a(1) ~= -sin(1)"
    assert run(src) == "true true"


def test_integral_shape():
    i = expr_of("∫ F(x) dx from 0 m to 0.2 m")
    assert isinstance(i, A.Integral) and i.var == "x"
    assert isinstance(i.lo, A.Quantity) and isinstance(i.hi, A.Quantity)
    assert sx_of("integral F(x) dx from 0 m to 0.2 m") == sx_of("∫ F(x) dx from 0 m to 0.2 m")


def test_integral_runs():
    assert run("k = 50 N/m\nF(x) = k x\nW = ∫ F(x) dx from 0 m to 0.2 m\nprint W") == "1.0 J"


def test_integral_infinite_limits():
    assert run("print ∫ exp(-x^2) dx from -inf to inf to 6 digits\nprint √π to 6 digits") == "1.77245\n1.77245"


def test_integral_without_dx_is_error():
    e = error_of("print ∫ x² from 0 to 1")
    assert "dx" in e.message


def test_indefinite_integral_is_symbolic():
    out = run("print ∫ x² dx")
    assert "x³" in out


# ====================================================================== solve
SPRING = ("m = 0.5 kg\nk = 50 [N/m]\nb = 0.2 kg/s\n")


def test_solve_multiline_clauses():
    src = SPRING + ("solve m x'' = -k x - b x'\n  with x(0) = 0.1 [m], x'(0) = 0 m/s\n"
                    "  for t from 0 s to 5 s\nprint x(5 s)\nprint x'(1 s)")
    x5, v1 = run_lines(src)
    assert x5.endswith(" m") and float(x5.split()[0]) == pytest.approx(0.0352006, rel=2e-2)
    assert v1.endswith(" m/s") and float(v1.split()[0]) == pytest.approx(0.444121, rel=2e-2)


def test_solve_single_line():
    src = "a = 2 s⁻¹\nsolve x' = -a x with x(0) = 1 kg for t from 0 s to 1 s\nprint x(1 s) to 6 digits"
    assert run(src) == "0.135335 kg"


def test_solve_shape():
    s = stmt("solve m x'' = -k x - b x'\n  with x(0) = 0.1 m, x'(0) = 0 m/s\n  for t from 0 s to 5 s")
    assert isinstance(s, A.Solve)
    assert len(s.equations) == 1 and len(s.initial) == 2 and s.var == "t" and s.step is None


def test_solve_system_commas_and_step():
    src = ("a = 2 s^-1\nsolve x' = -a x, y' = a x - y/(1 s) with x(0) = 1 kg, y(0) = 0 kg "
           "for t from 0 s to 1 s step 1 ms\nprint x(1 s) to 6 digits")
    assert run(src) == "0.135335 kg"
    s = stmt("solve x' = -a x, y' = a x with x(0) = 1, y(0) = 0 for t from 0 to 1 step 0.1")
    assert len(s.equations) == 2 and s.step is not None


def test_solve_system_with_and():
    src = ("a = 2 s^-1\nsolve x' = -a x and y' = a x with x(0) = 1 kg and y(0) = 0 kg "
           "for t from 0 s to 1 s\nprint y(1 s) to 6 digits")
    assert run(src) == "0.864665 kg"


def test_solve_indented_block():
    src = ("a = 2 s⁻¹\nb = 1 [1/s]\nsolve\n    x' = -a x\n    y' = a x - b y\n"
           "    with x(0) = 1 kg, y(0) = 0 kg\n    for t from 0 s to 1 s step 1 ms\nprint x(1 s) to 6 digits")
    assert run(src) == "0.135335 kg"


def test_solve_d_dt_form():
    assert run("solve d/dt x = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 s\nprint x(1 s) to 6 digits") == "0.367879 kg"


def test_solve_result_end_and_values():
    src = "solve x' = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 s step 0.5 s\nprint x[end]"
    assert run(src).endswith("kg")


def test_solve_without_range_is_error():
    e = error_of("solve x' = -x with x(0) = 1")
    assert "range" in e.message
    assert "for t from" in (e.hint or "")


def test_solve_missing_initial_condition():
    e = error_of("solve x' = -x\n  for t from 0 s to 1 s")
    assert "initial condition" in e.message


def test_solve_outside_range_runtime_error():
    e = error_of("solve x' = -x/(1 s) with x(0) = 1 kg for t from 0 s to 1 s\nprint x(2 s)")
    assert "outside the range" in e.message


# ====================================================================== fit / plot / load
CSV = "L [m], T [s]\n0.5, 1.42\n1.0, 2.01\n1.5, 2.46\n2.0, 2.84\n"


@pytest.fixture
def datadir(tmp_path):
    (tmp_path / "pend.csv").write_text(CSV)
    return str(tmp_path)


def test_load_columns(datadir):
    assert run('data = load "pend.csv"\nprint data.L', base_dir=datadir) == "[0.500, 1.00, 1.50, 2.00] m"


def test_fit_existing_parameter(datadir):
    out = run_lines('data = load "pend.csv"\ng = 9 m/s²\nfit T = 2π √(L / g) to data\nprint g',
                    base_dir=datadir)
    assert out[0].startswith("fit T = 2π √(L/g)")
    assert "m/s²" in out[1] and "standard error" in out[1]
    assert out[-1] == "9.78 m/s²"


def test_fit_new_parameter_with_guess(datadir):
    out = run('data = load "pend.csv"\nfit T = a L to data with a = 1 s/m\nprint a', base_dir=datadir)
    assert out.split("\n")[-1] == "1.61 s/m"


def test_fit_shape():
    s = stmt("fit T = a L to data with a = 2 s/m")
    assert isinstance(s, A.Fit) and s.guesses[0][0] == "a"


def test_plot_lists(datadir):
    out = run('data = load "pend.csv"\nplot data.T vs data.L to "p.png"', base_dir=datadir)
    assert out == "plot saved to p.png"
    import os
    assert os.path.exists(os.path.join(datadir, "p.png"))


def test_plot_shape():
    s = stmt('plot a vs t, b vs t to "f.png"')
    assert isinstance(s, A.Plot) and len(s.series) == 2 and s.out == "f.png"


def test_plot_solution(tmp_path):
    src = "solve x' = -x/(1 s) with x(0) = 1 m for t from 0 s to 1 s\nplot x vs t to \"x.png\""
    assert run(src, base_dir=str(tmp_path)) == "plot saved to x.png"


def test_plot_formula_from_to(tmp_path):
    src = 'k = 2 N/m\nF(x) = k x\nplot F(x) vs x from 0 m to 1 m to "F.png"'
    assert run(src, base_dir=str(tmp_path)) == "plot saved to F.png"


def test_plot_needs_vs():
    e = error_of("xs = [1, 2]\nplot xs")
    assert "vs" in e.message


# ====================================================================== where
def test_where():
    assert run("E = ½ m v² where m = 2 kg, v = 3 m/s\nprint E") == "9 J"


def test_where_shape():
    w = expr_of("x y where x = 2, y = 3")
    assert isinstance(w, A.Where) and [k for k, _ in w.bindings] == ["x", "y"]


def test_where_on_print():
    assert run("print xs[end] where xs = [1, 2, 3]") == "3"


def test_print_label_with_where():
    assert run('print "E =", E where E = 3 J') == "E = 3 J"


# ====================================================================== if / loops
def test_if_else_if_else():
    src = ('x = 3 m\nif x > 5 m\n    print "far"\nelse if x > 2 m\n    print "middle"\n'
           'else\n    print "near"')
    assert run(src) == "middle"
    s = stmt(src, 1)
    assert isinstance(s, A.If) and isinstance(s.other[0], A.If)


def test_elif_and_colon_and_then():
    src = "x = 1\nif x > 2:\n    print 1\nelif x > 0:\n    print 2\nif x > 0 then print 3\nif x > 0: print 4"
    assert run_lines(src) == ["2", "3", "4"]


def test_if_expression():
    assert run("x = -3 m\ny = if x > 0 m then x else -x\nprint y") == "3 m"


def test_if_expression_needs_else():
    e = error_of("y = if 1 > 0 then 1")
    assert "else" in e.message


def test_for_from_to_inclusive():
    assert run_lines("for i from 1 to 3\n    print i") == ["1", "2", "3"]


def test_for_with_units_and_step():
    assert run_lines("for t from 0 s to 1 s step 0.25 s\n    print t") == \
        ["0 s", "0.25 s", "0.5 s", "0.75 s", "1 s"]


def test_for_negative_step():
    assert run_lines("for i from 10 to 1 step -3\n    print i") == ["10", "7", "4", "1"]


def test_for_accumulate():
    assert run("total = 0 m\nfor i from 1 to 10\n    total += i * 1 cm\nprint total") == "0.550 m"   # the first operand's unit (m) is kept (D11)


def test_for_in():
    assert run_lines("for x in [1 m, 2 m]\n    print x") == ["1 m", "2 m"]
    s = stmt("for x in xs\n    print x")
    assert isinstance(s, A.ForIn) and s.var == "x"


def test_while_break_continue():
    src = "i = 0\nwhile i < 10\n    i += 1\n    if i == 2\n        continue\n    if i > 3\n        break\n    print i"
    assert run_lines(src) == ["1", "3"]


def test_nested_blocks():
    src = "for i from 1 to 2\n    for j from 1 to 2\n        print i, j\nprint \"done\""
    assert run_lines(src) == ["1 1", "1 2", "2 1", "2 2", "done"]


def test_missing_block_is_error():
    e = error_of("if 3 > 2\nprint 1")
    assert "indented block" in e.message


def test_assert():
    assert run("assert 1 m == 100 cm, \"units\"\nprint 1") == "1"
    e = error_of('assert 1 m == 2 m, "nope"')
    assert "nope" in str(e)


def test_logic_and_comparisons():
    assert run("print not (1 > 2) and 3 > 2 or false") == "true"
    assert run("print 1 m ≈ 100 cm, 1 m ~= 1.1 m, 2 ≠ 3, 2 ≤ 2, 3 ≥ 4") == "true false true true false"
    assert run("print 2 != 3, 2 <= 1, 3 >= 3") == "true false true"


def test_chained_comparison():
    # a < b < c means a < b and b < c (gauntlet #57); mixing == with < is still refused
    assert run("print 1 < 2 < 3, 1 < 3 < 2") == "true false"
    e = error_of("print 1 < 2 == 2")
    assert "chain" in e.message


def test_single_equals_in_condition_hint():
    e = error_of("x = 1\nif x = 1\n    print x")
    assert "==" in (e.hint or "") or "==" in e.message


# ====================================================================== functions
def test_one_line_functions():
    src = "F(x) = 50 N/m * x\nKE(m, v) = ½ m v²\nprint F(0.1 m), KE(2 kg, 3 m/s)"
    assert run(src) == "5.0 N 9 J"   # 0.1 has 1 significant figure -> at least 2 printed


def test_multi_line_function_with_return():
    src = "speed(h) =\n    g = 9.81 m/s²\n    return √(2*g*h)\nprint speed(10 m)"
    assert run(src) == "14.0 m/s"


def test_multi_line_function_last_line_is_result():
    assert run("sq(x) =\n    y = x * x\n    y\nprint sq(3 m)") == "9 m²"


def test_funcdef_shape():
    s = stmt("f(x [m], y) = x y")
    assert isinstance(s, A.FuncDef) and [p.name for p in s.params] == ["x", "y"]
    assert s.params[0].unit is not None and s.params[1].unit is None


def test_recursive_function():
    assert run("fact(n) = if n <= 1 then 1 else n fact(n - 1)\nprint fact(5)") == "120"


def test_function_applied_to_list():
    assert run("F(x) = 2 x\nxs = [1 m, 2 m]\nprint F(xs)") == "[2, 4] m"


def test_functions_call_each_other():
    assert run("f(x) = 2 x\ng(x) = f(x) + 1 m\nprint g(1 m)") == "3 m"


def test_wrong_arg_count():
    e = error_of("f(x) = 2 x\nprint f(1, 2)")
    assert e.line == 2


# ====================================================================== lists
def test_list_literal_and_indexing():
    assert run("xs = [1 m, 2 m, 3 m]\nprint xs[1], xs[end], len(xs)") == "1 m 3 m 3"


def test_end_arithmetic():
    assert run("xs = [1 m, 2 m, 3 m]\nprint xs[end - 1]") == "2 m"
    assert isinstance(expr_of("xs[end]").index, A.End)


def test_index_assign_and_push():
    src = "xs = [1 s, 2 s, 3 s]\nxs[2] = 5 s\nxs[end] += 1 s\nprint xs\npush(xs, 7 s)\nappend(xs, 8 s)\nprint len(xs), xs[end]"
    assert run_lines(src) == ["[1, 5, 4] s", "5 8 s"]


def test_empty_list_push():
    assert run("zs = []\npush(zs, 5 s)\npush(zs, 7 s)\nprint zs") == "[5, 7] s"


def test_list_arithmetic():
    assert run("xs = [1 m, 2 m, 3 m]\nprint 2 xs + 1 m") == "[3, 5, 7] m"
    assert run("xs = [1 m, 2 m, 3 m]\nprint xs^2") == "[1, 4, 9] m²"


def test_list_functions():
    assert run("ys = [3 m, 5 m, 7 m]\nprint sum(ys), mean(ys), max(ys)") == "15 m 5 m 7 m"
    assert run("print linspace(0 s, 1 s, 5)") == "[0, 0.250, 0.500, 0.750, 1.00] s"


def test_index_out_of_range():
    e = error_of("xs = [1 m, 2 m, 3 m]\nprint xs[4]")
    assert "out of range" in e.message and "1 to 3" in e.message


def test_index_zero_out_of_range():
    e = error_of("xs = [1 m, 2 m]\nprint xs[0]")
    assert "out of range" in e.message


def test_index_out_of_range_has_line():
    e = error_of("xs = [1 m, 2 m, 3 m]\nprint xs[4]")
    assert e.line == 2


def test_fractional_index_is_an_error():
    e = error_of("xs = [1 m, 2 m]\nprint xs[1.5]")
    assert "whole number" in e.message


def test_unclosed_list():
    e = error_of("x = [1, 2")
    assert "]" in e.message


# ====================================================================== in / to
def test_in_conversion():
    assert run("g = 9.81 m/s²\nprint g in ft/s²") == "32.2 ft/s²"
    c = expr_of("g in ft/s²")
    assert isinstance(c, A.Convert)


def test_to_function_same_as_in():
    assert run("g = 9.81 m/s²\nprint to(g, km/hr^2) == g, to(g, km/hr^2)") == "true 1.27×10⁵ km/hr²"   # 3 significant figures (D11)


def test_in_with_label():
    assert run('g = 9.81 m/s²\nprint "g is", g') == "g is 9.81 m/s²"


# ====================================================================== misc statements
def test_update_operators():
    assert run("x = 2 m\nx += 1 m\nx -= 50 cm\nx *= 2\nx /= 5\nprint x") == "1 m"


def test_absolute_value_bars():
    assert run("print |-3 m|") == "3 m"
    assert run("print abs(-2 s)") == "2 s"


def test_roots():
    assert run("print √(9 m²), ∛(27 m³), cbrt(8), sqrt(16)") == "3 m 3 m 2 4"   # ∛27 is 3 up to round-off, which counts as whole (D11)


def test_sqrt_binds_to_atom():
    assert sx_of("√x y") == "(* (sqrt x) y)"
    # `2 g` is two grams (reference §5 warns about exactly this)
    assert sx_of("√(2 g h)") == "(sqrt (* (q 2 [g]) h))"
    assert sx_of("√(2*g*h)") == "(sqrt (* (* 2 g) h))"


def test_semicolon_separates_statements():
    assert run("x = 1; print x") == "1"


def test_assign_to_expression_is_error():
    e = error_of("x = 2\nx'' = 3")
    assert "left side" in e.message


def test_e_caret_is_error_suggesting_exp():
    # e² (a fixed positive power) is the charge squared (DECISIONS D16); e^x is an error
    e = error_of("x = 1\nprint e^x")
    assert "exp" in str(e)


def test_constant_can_be_shadowed():
    assert run("h = 10 m\nprint h") == "10 m"


# ====================================================================== uncertainties (D120; were reserved until M4)
@pytest.mark.parametrize("src,out", [("print 5 +- 1", "5.0 ± 1.0"), ("print 5.0 ± 0.2 m", "5.00 ± 0.20 m"),
                                     ("print (5.0 ± 0.2) m", "5.00 ± 0.20 m"), ("print 1 + 2.0 ± 0.1", "3.00 ± 0.10")])
def test_plus_minus_parses(src, out):
    assert run(src) == out


@pytest.mark.parametrize("src", ["print ± 1", "x = 5 ±", "x = 1 ± 2 ± 3"])
def test_plus_minus_misplaced(src):
    e = error_of(src)
    assert "±" in e.message
    assert e.line == 1


# ====================================================================== clean errors, never crashes
def test_bad_unit_exponent_clean_error():
    with pytest.raises(FermiumError):
        run("print 3 [m^(1/x)]")


def _run_no_recursion_error(src):
    # a RecursionError traceback is huge and slow to render, so turn it into a short failure
    try:
        return run(src)
    except RecursionError:
        pytest.fail("RecursionError (should be a FermiumError or work)", pytrace=False)


def test_long_sum_no_crash():
    assert _run_no_recursion_error("print " + "+".join(["1"] * 1000)) == "1000"


def test_deep_nesting_clean_error():
    src = "print " + "(" * 100 + "1" + ")" * 100
    try:
        out = _run_no_recursion_error(src)
    except FermiumError:
        return
    assert out == "1"


@pytest.mark.parametrize("src", [
    "x = (", "x = )", "print ]", "f(x = 2", "for", "for i", "for i from", "while", "if",
    "solve", "fit", "plot", "print 3 [", "print 3 [m", "x = [1,, 2]", "d/dt", "∫", "x = 1 +",
    "print xs[", "f(", "print 'a", "x ==", "= 3", "print d^x/dt x", "print 3 [1/]",
])
def test_garbage_gives_fermium_error(src):
    with pytest.raises(FermiumError):
        run(src)


# ====================================================================== dx/dt (spec §3.5)
def test_dx_dt_notation():
    assert run("x(t) = 3 m/s * t\nv = dx/dt\nprint v(1 s)") == "3 m/s"


def test_dx_dt_with_variables_is_division():
    assert run("dx = 2 m\ndt = 1 s\nprint dx/dt") == "2 m/s"


@pytest.mark.parametrize("src", ["x(t) = t\nprint d^2/dt^* x", "x(t) = t\nprint d/dt ^ * x"])
def test_malformed_derivative_order(src):
    with pytest.raises(FermiumError):
        run(src)


def test_leibniz_notation_in_solve_and_functions():
    assert run("solve dN/dt = -N / (2 s) with N(0) = 100 for t from 0 s to 1 s\nprint N(1 s) to 6 digits") == "60.6531"
    assert run("f(t) = 3 m t / (1 s)\nprint df/dt(2 s)") == "3 m/s"


def test_fit_with_on_next_line(tmp_path):
    (tmp_path / "x.csv").write_text("t [s], y [m]\n0,1\n1,0.5\n2,0.26\n3,0.12\n")
    out = run('d = load "x.csv"\nfit y = A exp(-t/τ) to d\n  with A = 1 m, τ = 1 s\nprint τ', base_dir=str(tmp_path))
    assert out.split("\n")[-1] == "1.45 s"


def test_strings_keep_their_characters():
    assert run('print "Pound–Rebka µ"') == "Pound–Rebka µ"


def test_leibniz_higher_order_and_initial_conditions():
    out = run("x(t) = t^3\nprint d²x/dt²(2)\nprint d^2x/dt^2(2)\n"
              "solve d²y/dt² = -y with y(0) = 1, dy/dt(0) = 0 for t from 0 to 1\nprint y(1) to 6 digits\n"
              "solve d/dt (dz/dt) = -z with z(0) = 1, z'(0) = 0 for t from 0 to 1\nprint z(1) to 6 digits")
    assert out.split("\n") == ["12", "12", "0.540302", "0.540302"]


def test_vec_call_with_unit():
    assert run("print vec(3, 4) m/s") == "<3, 4> m/s"


def test_solution_range_error_has_units():
    e = error_of("solve x' = -x/(1 s) with x(0) = 1 m for t from 0 hr to 2 hr\nprint x(3 hr)")
    assert "at 3 hr" in e.message and "ends at 2 hr" in e.message


def test_solve_clauses_indented_less_than_equations():
    out = run("solve\n    x' = -x / (2 s)\n    y' = x / (2 s)\n  with x(0) = 1, y(0) = 0\n  for t from 0 s to 10 s\n"
              "print y(10 s) to 6 digits")
    assert out == "0.993262"


def test_plot_options(tmp_path):
    out = run('solve N\' = -N/(1 s) with N(0) = 1000 for t from 0 s to 10 s\n'
              'plot N vs t to "n.png" with log y, title "decay"', base_dir=str(tmp_path))
    assert out == "plot saved to n.png" and (tmp_path / "n.png").exists()
    assert "plot options are" in str(error_of('xs = [1, 2]\nplot xs vs xs with sideways'))


def test_lists_of_text():
    out = run('names = ["H-1", "He-4"]\npush(names, "C-12")\nprint names, len(names)\nfor n in names\n    print n\n'
              'print names[2]')
    assert out.split("\n") == ["[H-1, He-4, C-12] 3", "H-1", "He-4", "C-12", "He-4"]
    assert "numbers or text" in str(error_of('xs = [1, "a"]'))
