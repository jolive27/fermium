"""Findings of the independent red-team review, round 3 (dev-notes/REDTEAM.md, "Round 3 (06:30 UTC)").

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
"""
import io
import os
import subprocess

import pytest

from conftest import run, error_of
from fermium.driver import run_source
from fermium.errors import FermiumError
from numparse import num

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run_err(src, base_dir=None):
    """Run with the JIT; return (stdout, stderr) so run-time warnings can be seen."""
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err, base_dir=base_dir)
    return out.getvalue().strip(), err.getvalue()


def rt3(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 3 #{n}")


# ---- #1: `36 km/h` is 36 km divided by Planck's constant, silently -------------------------------------------

def test_1_km_per_h_is_not_km_per_planck_constant():
    try:
        out, err = run_err("v = 36 km/h\nprint v")
    except FermiumError as e:
        # an error that says h isn't hours (write hr) is right too
        assert "hr" in (e.message + str(e.hint)) or "Planck" in (e.message + str(e.hint))
        return
    # today: 5.43×10³⁷ s/(kg m) and no warning
    assert "warning" in err or out in ("36 km/h", "36 km/hr", "10 m/s")


# ---- #2: m c ΔT with ΔT in °C silently uses the absolute temperature --------------------------------------

@pytest.mark.parametrize("src", [
    "print 1 kg * 4186 J/(kg K) * 10 °C",
    "ΔT = 10 °C\nprint 1 kg * 4186 J/(kg K) * ΔT",
    "heat(m [kg], ΔT [K]) = m 4186 J/(kg K) ΔT\nprint heat(1 kg, 10 °C)",
])
def test_2_heat_with_a_celsius_temperature_change_warns(src):
    try:
        out, err = run_err(src)
    except FermiumError:
        return                                  # an error is right too
    # today: 1.19×10⁶ J (10 °C read as 283.15 K) with no warning; `2 * (20 °C)` does warn
    assert "warning" in err or num(out) == pytest.approx(41860, rel=1e-3)


# ---- #3: a relative uncertainty on a °C reading is a percentage of the kelvin value ------------------------

def test_3_percent_uncertainty_on_celsius_is_not_3_percent_of_293_K():
    try:
        out, err = run_err("t = 20.0 °C ± 3%\nprint t")
    except FermiumError:
        return                                  # an error ("write the uncertainty in K") is right too
    # today: 20.0 ± 8.8 °C, silently
    assert "warning" in err or "± 0.60" in out


# ---- #4: Crank–Nicolson step control misses the early transient ------------------------------------------

def test_4_crank_nicolson_early_transient_is_right_or_warns():
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K * sin(20 π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 100 s
print u(0.525 m, 0.1 s)
"""
    out, err = run_err(src)
    # exact: 1 K exp(-400 π² × 0.1) ≈ 3.5×10⁻¹⁷² K; today -0.0120 K (1.2 % of the peak, wrong sign), no warning
    assert "warning" in err or abs(num(out)) < 1e-3


# ---- #5: the zero-integral warning misses vector integrands ----------------------------------------------

def test_5_vector_integral_that_is_zero_everywhere_sampled_warns():
    out, err = run_err("print ∫ <exp(-x²), 0> dx from -1e6 to 1e6")
    # exact: <1.77, 0>; today <0, 0> silently (the scalar version warns)
    assert "warning" in err or out.startswith("<1.77")


# ---- #6: fermium.compile returns a complex number as a plain 2-array -------------------------------------

def test_6_compiled_complex_result_is_complex_or_refused():
    import numpy as np
    import fermium
    mod = fermium.compile("z = 3 + 4i\nf(x) = x + 1i\n")
    try:
        z = mod["z"]
    except (FermiumError, TypeError, ValueError):
        return                                  # refusing complex results clearly is right too
    # today: QuantityArray([3., 4.]) — the same as the vector <3, 4>
    assert isinstance(z, complex) or np.iscomplexobj(z)


# ---- #7: cot, sec, csc of an uncertain value crash with KeyError ----------------------------------------

@pytest.mark.parametrize("fn,value,sigma", [("cot", 0.642, 0.1412), ("sec", 1.851, 0.2882), ("csc", 1.188, 0.0763)])
def test_7_cot_sec_csc_propagate_uncertainty(fn, value, sigma):
    out = run(f"x = 1.0 ± 0.1\nprint {fn}(x)")      # today: KeyError: 'cot' (a Python traceback)
    v, s = out.split("±")
    assert float(v) == pytest.approx(value, abs=0.01) and float(s) == pytest.approx(sigma, abs=0.01)


# ---- #8: a negative measurement with a unit can't be written ---------------------------------------------

def test_8_negative_value_with_unit_after_uncertainty():
    # today: "the uncertainty after ± is length [m] but the value is a plain number"
    assert run("q = -5.0 ± 0.2 m\nprint q") == "-5.00 ± 0.20 m"


def test_8_negative_uncertainty_literal_says_negative():
    e = error_of("x = 5.0 ± -0.2 m\nprint x")
    assert "negative" in e.message              # today: the misleading "value is a plain number" message


# ---- #9: rounded large numbers print with trailing zeros that aren't significant -------------------------

@pytest.mark.parametrize("src,bad,good", [
    ("print 1000000 / 3", "333000", "3.33×10⁵"),
    ("x = 123456.7 m\nprint x / 1.0", "120000 m", "1.2×10⁵ m"),
    ("print 1.5 * 12345.0", "19000", "1.9×10⁴"),
    ("print 2999.85 * 1 MeV to 1 digits", "3000 MeV", "3×10³ MeV"),
])
def test_9_rounded_large_numbers_dont_look_exact(src, bad, good):
    out = run(src)
    assert out != bad and out == good


# ---- #10: the Schrödinger PDE doesn't accept the complex constant 𝑖 -------------------------------------

TDSE_IMAG = """m = m_e
σ = 1 nm
k0 = 2 / (1 nm)
solve 𝑖 ħ ∂ψ/∂t = -ħ²/(2*m) * ∂²ψ/∂x²
    with ψ(x, 0 fs) = (2π σ²)^(-1/4) exp(-x² / (4σ²)) exp(𝑖 k0 x), ψ(-40 nm, t) = 0 nm^(-1/2), ψ(40 nm, t) = 0 nm^(-1/2)
    for x from -40 nm to 40 nm, t from 0 fs to 5 fs
    grid 800
print ∫ |ψ(x, 5 fs)|^2 dx from -40 nm to 40 nm to 4 digits
"""
# (grid 800 added: on the default grid of 400 the interpolated norm is 0.9989 already at t = 0, with `i` too, so the
# 1e-3 check measured the grid, not 𝑖; CN keeps the grid's norm exactly)


def test_10_tdse_with_the_imaginary_constant():
    # today: "the initial value must be a number, but it is a complex number …"
    assert num(run(TDSE_IMAG)) == pytest.approx(1.0, abs=1e-3)


# ---- #11: a run-time error inside a module gives the module's line as if it were the program's ----------

def test_11_runtime_error_in_a_module_names_the_module_or_the_calling_line():
    src = "import stats\nx = 1\ny = 2\nprint stats.standard_error([5 m])"
    with pytest.raises(FermiumError) as ei:
        run(src)
    e = ei.value
    # today: "line 6: std needs at least 2 values …" in a 4-line program (line 6 of stats.fm)
    assert e.line == 4 or "stats" in str(e)


# ---- #12: fermium build repeats the zero-integral warning on every call ---------------------------------

def test_12_build_shows_the_zero_integral_warning_once_per_line(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    src = "f(a) = ∫ exp(-(x - a)²) dx from -1e6 to 1e6\nfor k from 1 to 3\n    print f(k)\n"
    exe = str(tmp_path / "p")
    build(src, str(tmp_path / "p.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True, timeout=120)
    # the JIT and the interpreter print it once; today the executable prints it 3 times
    assert r.stderr.count("came out as exactly 0") == 1


# ---- #13: the tolerance option accepts units, and can't go on its own line -------------------------------

@pytest.mark.parametrize("tol", ["1e-6 m", "0.001 s"])
def test_13_tolerance_with_a_unit_is_an_error(tol):
    src = f"solve x' = -x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s tolerance {tol}\nprint x(1 s)"
    with pytest.raises(FermiumError):
        run(src)                                # today: accepted silently


def test_13_tolerance_on_its_own_line():
    src = """solve y'' = -9.81 m/s² with y(0 s) = 10 m, y'(0 s) = 0 m/s
  for t from 0 s to 10 s
  until y = 0 m
  tolerance 1e-12
print times(y)[end] to 10 digits
"""
    # (was `to 8 digits`: 8 printed digits can't meet rel=1e-8 whatever the solver does; the check is unchanged)
    # `using bdf` and `until …` work on their own lines; today: "expected '=' in this equation but the line ended"
    assert num(run(src)) == pytest.approx((20 / 9.81) ** 0.5, rel=1e-8)


# ---- #14: complex numbers are called vectors in error messages ------------------------------------------

@pytest.mark.parametrize("src", [
    "use python numpy as np\nprint np.abs(1 + 2i)",
    "solve z' = 1i z with z(0) = 1 for t from 0 to 1\nprint z([0, 0.5])",
])
def test_14_complex_values_are_not_called_vectors(src):
    e = error_of(src)
    assert "vector" not in e.message and "v.x" not in str(e.hint)


# ---- #15: nits in messages ------------------------------------------------------------------------------

def test_15_python_in_natural_units_message_has_no_doubled_word():
    e = error_of("use python numpy as np\nunits natural(ħ = c = 1)\nE = 1 MeV\nprint np.sin(E / (1 MeV))")
    assert "units units" not in e.message


def test_15_montecarlo_list_formula_message():
    try:
        out = run("a = 1.0 ± 0.1\npropagate montecarlo 1000 samples\n    b = [a, 2 a]\nprint b")
    except FermiumError as e:
        # today: "needs at least one formula (name = …) in its block", but b = [a, 2 a] is one
        assert "at least one formula" not in e.message
        return
    assert "±" in out


def test_15_compile_with_uncertainties_doesnt_blame_the_repl():
    import fermium
    with pytest.raises(FermiumError) as ei:
        fermium.compile("L = 1.20 ± 0.01 m\nf(x) = 2 x\n")
    assert "REPL" not in ei.value.message       # today: "… but not yet in the REPL or Jupyter"


# =========================================================================================================
# More tests for the fixes (beyond the original repros)
# =========================================================================================================

def run_interp(src):
    from fermium.interp import run_interpreted
    out, err = io.StringIO(), io.StringIO()
    run_interpreted(src, "<t>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


def build_run(tmp_path, src):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "p")
    build(src, str(tmp_path / "p.fm"), exe)
    return subprocess.run([exe], capture_output=True, text=True, timeout=120)


# ---- #1 (D180) ----
@pytest.mark.parametrize("src", [
    "v = 36 km/h\nprint v",
    "print 100 km/h in m/s",
    "print 2 eV/h",
    "h = 2 m\nprint 36 km/h",
    "f(v [km/h]) = v\nprint f(1 m/s)",
    "v = 36 km/hr\nprint v in km/h",
    "print 3 [h]",
])
def test_1_h_after_a_unit_is_an_error_that_says_hr(src):
    e = error_of(src)
    assert "Planck" in e.message + str(e.hint) and "hr" in e.message + str(e.hint)


def test_1_hint_names_the_number_and_how_to_divide():
    e = error_of("v = 36 km/h\nprint v")
    assert "36 km/hr" in e.hint and "(36 km)/h" in e.hint
    e = error_of("h = 2 m\nprint 36 km/h")
    assert "(36 km)/h" in e.hint and "your h" in e.hint
    e = error_of("print 36 km / h")                  # spaces don't matter (D235)
    assert "36 km/hr" in e.hint


def test_1_hours_and_dividing_by_planck_still_work():
    assert run("v = 36 km/hr\nprint v in m/s") == "10 m/s"
    assert run("f(v [km/hr]) = v in m/s\nprint f(36 km/hr)") == "10 m/s"
    assert num(run("print (2 eV)/h in Hz to 4 digits")) == pytest.approx(4.836e14, rel=1e-3)
    assert run("h = 2 m\nprint (36 km)/h") == "18000"
    assert run("x = [h]\nprint len(x)") == "1"


# ---- #2 (D181) ----
def test_2_literal_celsius_in_a_product_warns_with_both_readings():
    out, err = run_err("print 1 kg * 4186 J/(kg K) * 10 °C")
    assert "absolute temperature" in err and "283.15 K" in err and "10 K" in err


@pytest.mark.parametrize("src", [
    "ΔT = 10 °C\nprint ΔT",
    "T1 = 20 °C\nΔT = T1\nprint ΔT",
    "delta_T = 50 °F\nprint delta_T",
    "heat(m [kg], ΔT [K]) = m 4186 J/(kg K) ΔT\nprint heat(1 kg, 10 °C)",
    "heat(m, ΔT) = m 4186 J/(kg K) ΔT\nprint heat(1 kg, 10 °C)",
])
def test_2_delta_name_holding_celsius_is_an_error(src):
    e = error_of(src)
    assert "temperature change" in e.message and "K" in e.hint


def test_2_correct_forms_stay_quiet():
    # a difference of °C readings is a difference (A47); a °C variable in p V = n R T is absolute and right
    out, err = run_err("T1 = 20 °C\nT2 = 30 °C\nΔT = T2 - T1\nprint 4186 J/(kg K) * 1 kg * ΔT")
    assert num(out) == pytest.approx(41860) and "warning" not in err
    out, err = run_err("T = 25 °C\nprint 1 mol R_gas T / (1 m³) to 4 digits")
    assert num(out) == pytest.approx(8.314462618 * 298.15, rel=1e-3) and "warning" not in err
    out, err = run_err("print 1 kg * 4186 J/(kg K) * 10 K")
    assert num(out) == pytest.approx(41860) and "warning" not in err


def test_2_python_celsius_to_a_delta_parameter_is_refused():
    import fermium
    from fermium import Q
    mod = fermium.compile("heat(m [kg], ΔT [K]) = m 4186 J/(kg K) ΔT\nP(T [K]) = 1 mol R_gas T / (1 m³)\n")
    assert float(mod.heat(1, Q(10, "K"))) == pytest.approx(41860)
    assert float(mod.P(Q(20, "°C"))) == pytest.approx(8.314462618 * 293.15, rel=1e-6)
    with pytest.raises(FermiumError) as ei:
        mod.heat(1, Q(10, "°C"))
    assert "temperature change" in ei.value.message and "283.15 K" in ei.value.message


# ---- #3 (D182) ----
@pytest.mark.parametrize("src,want", [
    ("t = 20.0 °C ± 3%\nprint t", "20.00 ± 0.60 °C"),
    ("t = 68.0 °F ± 1%\nprint t", "68.00 ± 0.68 °F"),
    ("ts = [20.0 °C, 40.0 °C] ± 5%\nprint ts", "[20.0 ± 1.0, 40.0 ± 2.0] °C"),
    ("t = 300.0 K ± 1%\nprint t", "300.0 ± 3.0 K"),
    ("t = -10.0 °C ± 10%\nprint t", "-10.0 ± 1.0 °C"),
])
def test_3_relative_uncertainty_of_a_reading(src, want):
    assert run(src) == want


# ---- #4 (D183) ----
def test_4_two_modes_early_times_are_right():
    import math
    src = """L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K * (sin(π x / L) + sin(20 π x / L)), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 1 s
print u(0.525 m, 0.5 ms) to 6 digits
print u(0.525 m, 1 ms) to 6 digits
print u(0.525 m, 0.5 s) to 6 digits
"""
    out, err = run_err(src)

    def exact(t):
        return math.exp(-math.pi ** 2 * t) * math.sin(0.525 * math.pi) + \
            math.exp(-400 * math.pi ** 2 * t) * math.sin(10.5 * math.pi)
    got = [num(x) for x in out.split("\n")]
    # was 0.864 K and 0.851 K (exact 1.131 K and 1.006 K): now within the controller's 10⁻³ of the peak
    for g, t in zip(got, (0.5e-3, 1e-3, 0.5)):
        assert g == pytest.approx(exact(t), abs=2e-3)
    assert "warning" not in err


def test_4_fast_mode_alone_warns_or_is_right():
    out, err = run_err("""L = 1 m
D = 1 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 1 K * sin(20 π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
    for x from 0 m to L, t from 0 s to 100 s
print u(0.525 m, 0.1 s)
print u(0.5 m, 50 s)
""")
    a, b = (num(x) for x in out.split("\n"))
    assert abs(a) < 1e-3 and abs(b) < 1e-6
    assert "time step could not be made fine enough" in err     # honest: the first steps can't resolve it


# ---- #5 (D110) ----
def test_5_vector_zero_integral_warns_once_in_the_interpreter():
    out, err = run_interp("print ∫ <exp(-x²), 0> dx from -1e6 to 1e6")
    assert out == "<0, 0>" and err.count("came out as exactly 0") == 1


def test_5_vector_zero_integral_warns_once_in_the_jit():
    out, err = run_err("print ∫ <exp(-x²), 0, 0> dx from -1e6 to 1e6")
    assert err.count("came out as exactly 0") == 1


@pytest.mark.parametrize("src", [
    "print ∫ <x, 0> dx from -1 to 1",                   # 0 by symmetry, next to a true zero component
    "print ∫ <exp(-x²), 1> dx from -1 to 1",
    "print ∫ <sin(x), cos(x)> dx from 0 to 2π",
])
def test_5_vector_integrals_with_non_zero_samples_are_quiet(src):
    out, err = run_err(src)
    assert "came out as exactly 0" not in err
    out, err = run_interp(src)
    assert "came out as exactly 0" not in err


def test_5_vector_zero_integral_warns_in_build(tmp_path):
    r = build_run(tmp_path, "print ∫ <exp(-x²), 0> dx from -1e6 to 1e6\nprint ∫ <x, 0> dx from -1 to 1\n")
    assert r.stdout.startswith("<0, 0>") and r.stderr.count("came out as exactly 0") == 1


# ---- #6 ----
def test_6_compiled_complex_results():
    import fermium
    mod = fermium.compile("z = 3 + 4i\nf(x) = x + 1i\nw(x [Ω]) = x + 2i Ω\nv = <3, 4>\n")
    z = mod["z"]
    assert isinstance(z, complex) and z == 3 + 4j and abs(z) == 5.0 and z.unit == ""
    assert mod.f(2) == 2 + 1j
    w = mod.w(3)
    assert w == 3 + 2j and w.unit == "Ω" and str(w) == "(3 + 2i) Ω" and w.to("mΩ") == pytest.approx(3000 + 2000j)
    assert list(mod["v"]) == [3.0, 4.0]                   # vectors are still arrays


# ---- #7 ----
def test_7_every_math_function_propagates_uncertainty():
    from fermium.checker import MATH1
    from fermium.uncertain import DERIV, STEP
    assert MATH1 <= set(DERIV) | STEP


# ---- #8 ----
def test_8_negative_values_in_expressions():
    assert run("q = -5.0 ± 0.2 m\nprint q + 10 m") == "5.00 ± 0.20 m"
    assert run("q = -5.0 ± 0.2 m\nprint q, (-5.0 ± 0.2) m") == "-5.00 ± 0.20 m -5.00 ± 0.20 m"


# ---- #10 (D184) ----
def test_10_bare_i_still_works_without_a_variable_i():
    assert num(run(TDSE_IMAG.replace("𝑖", "i"))) == pytest.approx(1.0, abs=1e-3)


def test_10_bare_i_with_your_own_i_is_an_error():
    e = error_of("i = 5\n" + TDSE_IMAG.replace("𝑖", "i"))
    assert "your own variable i" in e.message and "𝑖" in e.hint
    # 𝑖 still works next to a variable i
    assert num(run("i = 5\n" + TDSE_IMAG)) == pytest.approx(1.0, abs=1e-3)


def test_10_pde_solution_is_complex():
    out = run(TDSE_IMAG + "z = ψ(1 nm, 5 fs)\nprint (|z| - √(z.re² + im(z)²)) / (1 nm^(-1/2))\n"
                          "print arg(ψ(0 nm, 0 fs))\n")
    lines = out.split("\n")
    assert num(lines[1]) == pytest.approx(0, abs=1e-9) and num(lines[2]) == pytest.approx(0, abs=1e-6)


# ---- #11 (D185) ----
def test_11_interpreter_names_the_module_and_the_call():
    from fermium.interp import run_interpreted
    with pytest.raises(FermiumError) as ei:
        run_interpreted("import stats\nx = 1\ny = 2\nprint stats.standard_error([5 m])")
    assert ei.value.line == 4 and "stats.fm, line 6" in ei.value.message


def test_11_jit_names_the_module_and_the_call():
    with pytest.raises(FermiumError) as ei:
        run("import stats\nx = 1\ny = 2\nprint stats.standard_error([1.0, 2.0, 3.0] ± 0.1)")
    assert ei.value.line == 4 and "stats.fm, line 6" in ei.value.message
    with pytest.raises(FermiumError) as ei:
        run("import astro\nz = 1\nprint astro.comoving_distance(-10, 70 km/s/Mpc, 0.3)")
    assert ei.value.line == 3 and "astro.fm, line 35" in ei.value.message


def test_11_program_errors_keep_their_line():
    with pytest.raises(FermiumError) as ei:
        run("import stats\nx = [1.0]\nprint x[3]")
    assert ei.value.line == 3 and "(in " not in ei.value.message


def test_11_build_names_the_module_and_the_call(tmp_path):
    r = build_run(tmp_path, "import stats\nx = 1\ny = 2\nprint stats.standard_error([5 m])\n")
    assert r.returncode != 0 and r.stderr.startswith("line 4: ") and "stats.fm, line 6" in r.stderr


# ---- #12 ----
def test_12_build_complex_zero_integral_warns_once(tmp_path):
    r = build_run(tmp_path, "print ∫ exp(-x²) * (1 + 1i) dx from -1e6 to 1e6\n")
    assert r.stderr.count("came out as exactly 0") == 1


# ---- #13 ----
def test_13_tolerance_messages():
    e = error_of("solve x' = -x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s tolerance 2\nprint x(1 s)")
    assert "between 0 and 1" in e.message
    e = error_of("solve x' = -x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s tolerance 1e-6 m\nprint x(1 s)")
    assert "no units" in e.message and "length" in e.message


def test_13_tolerance_on_its_own_line_before_until_and_twice():
    src = """solve y'' = -9.81 m/s² with y(0 s) = 10 m, y'(0 s) = 0 m/s
  for t from 0 s to 10 s
  tolerance 1e-12
  until y = 0 m
print times(y)[end] to 10 digits
"""
    assert num(run(src)) == pytest.approx((20 / 9.81) ** 0.5, rel=1e-8)
    e = error_of("solve x' = -x / (1 s) with x(0 s) = 1 m\n  for t from 0 s to 1 s tolerance 1e-6\n"
                 "  tolerance 1e-8\nprint x(1 s)")
    assert "twice" in e.message


# ---- #14 ----
def test_14_hints_for_complex_values():
    e = error_of("use python numpy as np\nprint np.abs(1 + 2i)")
    assert "complex number" in e.message and "re(z)" in e.hint
    e = error_of("solve z' = 1i z with z(0) = 1 for t from 0 to 1\nprint z([0, 0.5])")
    assert "complex" in e.message and ".re" in e.hint


# ---- #15 ----
def test_15_montecarlo_list_says_what_is_wrong():
    e = error_of("a = 1.0 ± 0.1\npropagate montecarlo 1000 samples\n    b = [a, 2 a]\nprint b")
    assert "b is a list" in e.message


def test_13_absolute_on_its_own_line():
    src = """solve x' = -x / (1 s) with x(0 s) = 1 m
  for t from 0 s to 1 s
  tolerance 1e-10
  absolute 1e-12 m
print x(1 s) to 8 digits
"""
    assert num(run(src)) == pytest.approx(0.36787944, rel=1e-8)
