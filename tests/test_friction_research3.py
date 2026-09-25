"""Frictions from the shell-model and recombination research reproductions (gauntlet/FRICTION.md #87-#96,
source "research"; D210-D216).

Programs run compiled (LLVM) and in the reference interpreter, and the two must agree; `fermium build`
is checked where it applies."""
import io
import math
import subprocess

import numparse
import pytest

from conftest import run, warnings_of
from fermium.errors import FermiumError, FermiumRuntimeError
from fermium.interp import run_interpreted


def interp(src, base_dir=None):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o, base_dir=base_dir)
    return o.getvalue().strip()


def both(src, base_dir=None):
    out = run(src, base_dir)
    assert interp(src, base_dir) == out
    return out


def both_error(src, kind=FermiumError):
    with pytest.raises(kind) as native:
        run(src)
    with pytest.raises(kind) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    return native.value


def built(src, tmp_path, name="prog"):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / name)
    build(src, str(tmp_path / (name + ".fm")), exe)
    return subprocess.run([exe], capture_output=True, text=True, timeout=300, cwd=str(tmp_path))


def nums(out):
    return [float(w.strip("[],")) for w in out.split() if w.strip("[],").lstrip("-").replace(".", "", 1)
            .replace("e-", "", 1).replace("e+", "", 1).isdigit()]


# ---------------------------------------------------------------- #87: V'(x) of a known function (D210)
WS = "f(x) = 1 / (1 + exp((x - 5 fm) / (0.6 fm)))\n"
EIG = ("solve -ħ²/(2*m_n) * ψ'' + (-50 MeV) f(x) ψ + (1 MeV fm) {d} ψ = E ψ with ψ(0 fm) = 0, ψ(15 fm) = 0 "
       "for x from 0 fm to 15 fm lowest 2\nprint E in MeV to 9 digits\n")


def test_87_a_known_functions_derivative_in_an_eigenvalue_equation():
    out = both(WS + EIG.format(d="f'(x)"))
    by_hand = both(WS + "g(x) = -f(x) * (1 - f(x)) / (0.6 fm)\n" + EIG.format(d="g(x)"))
    assert out == by_hand
    e1, e2 = nums(out)
    assert -50 < e1 < e2 < 0


def test_87_d_dx_of_a_known_function_too():
    assert both(WS + EIG.format(d="d/dx f(x)")) == both(WS + EIG.format(d="f'(x)"))


def test_87_in_an_ode_too():
    src = ("V(x) = 0.5 N/m * x²\nsolve m_e * x'' = -V'(x) / (1 kg) * m_e with x(0 s) = 1 m, x'(0 s) = 0 m/s "
           "for t from 0 s to 1 s\nprint x(1 s) in m to 8 digits")
    assert float(both(src).split()[0]) == pytest.approx(math.cos(1), rel=1e-6)


def test_87_two_real_unknowns_still_need_two_equations():
    src = ("solve ψ'' + φ'' = -ψ / (1 m²) with ψ(0 m) = 0, ψ(1 m) = 0 for x from 0 m to 1 m lowest 1")
    with pytest.raises(FermiumError, match="one unknown function"):
        run(src)


def test_87_an_undefined_function_is_still_an_unknown():
    with pytest.raises(FermiumError, match="1 equation for 2 unknown functions"):
        run("solve x'' = -y'(t) / (1 s) with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 1 s")


# ---------------------------------------------------------------- #88: the unknown u is not the unit u (D211)
HO = ("solve -0.0380998 eV nm² * u'' + 0.5 * (1 eV/nm²) x² u = E u with u(-3 nm) = 0, u(3 nm) = 0 "
      "for x from -3 nm to 3 nm lowest 2\nprint E in eV to 6 digits")


def test_88_the_unknown_u_after_a_unit_is_the_unknown():
    assert both(HO) == "[0.138021, 0.414064] eV"
    assert both(HO.replace("u''", "ψ''").replace("x² u", "x² ψ").replace("E u", "E ψ")
                .replace("u(-3", "ψ(-3").replace("u(3", "ψ(3")) == "[0.138021, 0.414064] eV"


def test_88_a_number_before_the_unknowns_derivative():
    src = ("solve 2 u'' = -u / (1 s²) with u(0 s) = 1 m, u'(0 s) = 0 m/s for t from 0 s to 1 s\n"
           "print u(1 s) to 6 digits")
    assert both(src) == f"{math.cos(1 / math.sqrt(2)):.6g} m"


def test_88_2_u_combined_is_ambiguous_and_names_the_unknown():
    err = both_error("solve -ħ²/(2*m_e) * u'' + 2 u * 0.1 eV = E u with u(0 nm) = 0, u(3 nm) = 0 "
                     "for x from 0 nm to 3 nm lowest 2")
    assert "'2 u' is ambiguous: right after a number, u is a unit (atomic mass units), but u is also the " \
           "unknown u of this solve" in err.message
    assert "2*u" in err.hint


def test_88_2_u_alone_says_it_is_read_as_the_atomic_mass_unit():
    err = both_error("solve -ħ²/(2*m_e) * u'' = E u - 2 u with u(0 nm) = 0, u(3 nm) = 0 "
                     "for x from 0 nm to 3 nm lowest 2")
    assert err.message.startswith("u here is read as the unit u (atomic mass unit), not the unknown u of this "
                                  "solve: '2 u' is a unit right after a number; write 2 * u")


def test_88_other_units_in_the_solve_are_unchanged():
    # s is seconds in the range and the equation, though x is the unknown; an unknown named like a unit only
    # changes that name
    src = ("solve x' = -x / (2 s) with x(0 s) = 1 m for t from 0 s to 2 s\nprint x(2 s) to 6 digits")
    assert both(src) == f"{math.exp(-1):.6g} m"


# ---------------------------------------------------------------- #89: d/dr f(r, R) (D212)
FR = "f(r, R) = 1 / (1 + exp((r - R) / (0.6 fm)))\n"


def test_89_d_dr_of_a_two_argument_call_keeps_R_as_a_parameter():
    out = both(FR + "df = d/dr f(r, R)\nprint df(5 fm, 5 fm) in 1/fm to 6 digits\n"
               "print df(4 fm, 5 fm) in 1/fm to 6 digits\ng = ∂/∂r f\nprint g(4 fm, 5 fm) in 1/fm to 6 digits")
    a, b, c = out.splitlines()
    assert a == "-0.416667 1/fm"
    x = math.exp(-1 / 0.6)
    assert float(b.split()[0]) == pytest.approx(-x / (1 + x) ** 2 / 0.6, rel=1e-5)
    assert b == c


def test_89_the_other_variable_too():
    out = both(FR + "dR = d/dR f(r, R)\nprint dR(4 fm, 5 fm) in 1/fm to 6 digits")
    x = math.exp(-1 / 0.6)
    assert float(out.split()[0]) == pytest.approx(x / (1 + x) ** 2 / 0.6, rel=1e-5)


def test_89_a_defined_argument_stays_a_value():
    out = both(FR + "R = 5 fm\ndf = d/dr f(r, R)\nprint df(5 fm) in 1/fm to 6 digits")
    assert out == "-0.416667 1/fm"


def test_89_an_undefined_name_inside_an_argument_suggests_the_partial():
    err = both_error(FR + "df = d/dr f(r, 2 R)")
    assert "R isn't defined, so this can't be a function of r alone" in err.message
    assert "∂/∂r f" in err.hint


def test_89_built_executable_matches(tmp_path):
    src = FR + "df = d/dr f(r, R)\nprint df(4 fm, 5 fm) in 1/fm to 6 digits"
    p = built(src, tmp_path)
    assert p.returncode == 0, p.stderr
    assert p.stdout.strip() == both(src)


# ---------------------------------------------------------------- #90: overriding a well-known constant (D213)
@pytest.mark.parametrize("src,name,what", [
    ("h = 0.6736", "h", "Planck's constant"),
    ("e = 0.0167", "e", "the elementary charge"),
    ("G = 1", "G", "the gravitational constant"),
    ("c = 3.0e8 m/s", "c", "the speed of light"),
    ("k_B = 1.38e-23 J/K", "k_B", "Boltzmann's constant"),
])
def test_90_overriding_a_well_known_constant_warns_at_the_assignment(src, name, what):
    ws = warnings_of(src + "\nprint 1")
    assert any(f"{name} ({what}) is now your variable: from here on, {name} means your value" in w for w in ws)


def test_90_once_per_name():
    ws = warnings_of("h = 0.6736\nh = 0.7\nprint h")
    assert sum("is now your variable" in w for w in ws) == 1


def test_90_a_height_h_or_a_step_h_is_not_warned():
    # D13 and the bootcamp: `h = 10 m` for a height is common and meant; its units differ from Planck's,
    # so any later use as the constant is a unit error anyway
    assert not any("is now your variable" in w for w in warnings_of("h = 10 m\ng = 9.81 m/s²\nprint h"))
    assert not any("is now your variable" in w for w in warnings_of("h = 1e-6 s\nprint h"))


def test_90_the_program_still_runs_with_the_users_value():
    assert both("h = 0.6736\nH0 = 100 * h * 1 km/s/Mpc\nprint H0 in km/s/Mpc to 4 digits") == "67.36 km/(s Mpc)"


# ---------------------------------------------------------------- #91: too many steps says where (D214)
STIFF = "solve x' = (x - 1 m) / (1e-8 s) with x(2500 s) = 2 m for t from 2500 s to 0 s\nprint x(0 s)"


def test_91_too_many_steps_reports_the_place_reached_compiled():
    with pytest.raises(FermiumRuntimeError) as ex:
        run(STIFF)
    msg = ex.value.message
    assert "too many steps (20 million: it got from t = 2500 s only to t = 2499 s)" in msg
    assert "using radau" in msg


def test_91_interpreter_same_message(monkeypatch):
    import fermium.interp as fi
    monkeypatch.setattr(fi, "ODE_MAX_STEPS", 200_000)
    with pytest.raises(FermiumRuntimeError) as ex:
        interp(STIFF.replace("1e-8 s", "1e-6 s"))
    assert "it got from t = 2500 s only to t = 2499 s)" in ex.value.message


def test_91_message_digits():
    from fermium.runtime.core import Runtime
    rt = Runtime.__new__(Runtime)
    rt.tables = None
    msg = rt.describe_error(1_000_000, 2499.99994, 2500.0)
    assert "from t = 2500 (SI units) only to t = 2499.9999 (SI units)" in msg
    msg = rt.describe_error(2_000_000, 60.5, 0.0)
    assert msg.startswith("the stiff ODE solver needed too many steps (it got from t = 0 (SI units) only to "
                          "t = 60.5 (SI units))")


def test_91_built_executable_says_where(tmp_path):
    p = built(STIFF, tmp_path)
    assert p.returncode != 0
    assert "it got from t = 2500 s only to t = 2499 s)" in p.stderr + p.stdout


# ---------------------------------------------------------------- #92: a unit after a bracket (D215)
SEMF = "N = 20\nZ = 16\nA = N + Z\n"


def test_92_a_unit_after_a_bracketed_expression():
    out = both(SEMF + "a = (51 - 33 (N - Z)/A) MeV\nprint a in MeV to 6 digits\n"
               "b = 2 (N - Z + 3) MeV\nprint b\nprint (1 + 1) MeV / (2 fm)\nv = (3 + 4) m/s\nprint v")
    a, b, c, d = out.splitlines()
    assert a == f"{51 - 33 * 4 / 36:.6g} MeV"
    assert b == "14 MeV"
    assert c == "160 N"          # 1 MeV/fm
    assert d == "7 m/s"


def test_92_a_bracket_with_units_times_a_unit():
    assert both("x = 3 m\nprint (x + 1 m) s in m s to 3 digits") == "4.00 m s"


def test_92_after_a_product_that_starts_with_a_number():
    assert both("h = 0.6736\nH0 = 100 h km/s/Mpc\nprint H0 in km/s/Mpc to 4 digits") == "67.36 km/(s Mpc)"


def test_92_a_bracket_then_your_variable_is_your_variable():
    # `(v1 - v2) m`, `(4/3) T`: tested programs mean their own m, T (D215 keeps that reading)
    assert both("m = 2 kg\nv = 3 m/s\nprint (v + v) m") == "12 kg m/s"
    assert both("T = 2 K\nprint (4/3) T to 3 digits") == "2.67 K"


def test_92_a_variable_set_later_is_never_read_as_a_unit():
    # g is assigned further down (in a loop): after the bracket it is still the name g, not grams
    with pytest.raises(FermiumError, match="g isn't defined"):
        run("x = 2\nprint (x + 1) g\nfor g in [1, 2]\n    print g")
    with pytest.raises(FermiumError, match="m isn't defined"):
        run("f(x) = (x + 1) m\nprint f(2)\nm = 3 kg")


def test_92_after_a_number_and_pi_a_name_is_still_not_a_unit():
    # D7: `4π² L` is 4π² times L, and `2 a b²` never reads b as barns
    with pytest.raises(FermiumError, match="L isn't defined"):
        run("print 4π² L")
    with pytest.raises(FermiumError, match="b² is a unit, not a value"):     # as before (D163)
        run("a = 2\nprint 2 a b²")


def test_92_constants_that_are_units_keep_their_meaning():
    # `2 h c²` is Planck's law's 2 h c², with c the constant (the unit c has the same value)
    out = both("print 2 h c^2 in W m² to 6 digits\nprint (1 + 1) c in m/s to 6 digits")
    a, b = numparse.nums(out)
    assert a == pytest.approx(2 * 6.62607015e-34 * 299792458.0 ** 2, rel=1e-6)
    assert b == pytest.approx(2 * 299792458.0, rel=1e-6)


def test_92_calls_are_not_brackets():
    with pytest.raises(FermiumError, match="MeV isn't defined"):
        run("f(x) = 2 x\nprint f(3) MeV")


def test_92_built_executable_matches(tmp_path):
    src = SEMF + "a = (51 - 33 (N - Z)/A) MeV\nprint a in MeV to 6 digits\nprint 100 * 0.6736 km/s/Mpc"
    p = built(src, tmp_path)
    assert p.returncode == 0, p.stderr
    assert p.stdout.strip() == both(src)
