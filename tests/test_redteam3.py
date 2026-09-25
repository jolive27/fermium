"""Findings of the independent red-team review, round 3 (REDTEAM.md, "Round 3 (06:30 UTC)").

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

@rt3(1)
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

@rt3(2)
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

@rt3(3)
def test_3_percent_uncertainty_on_celsius_is_not_3_percent_of_293_K():
    try:
        out, err = run_err("t = 20.0 °C ± 3%\nprint t")
    except FermiumError:
        return                                  # an error ("write the uncertainty in K") is right too
    # today: 20.0 ± 8.8 °C, silently
    assert "warning" in err or "± 0.60" in out


# ---- #4: Crank–Nicolson step control misses the early transient ------------------------------------------

@rt3(4)
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

@rt3(5)
def test_5_vector_integral_that_is_zero_everywhere_sampled_warns():
    out, err = run_err("print ∫ <exp(-x²), 0> dx from -1e6 to 1e6")
    # exact: <1.77, 0>; today <0, 0> silently (the scalar version warns)
    assert "warning" in err or out.startswith("<1.77")


# ---- #6: fermium.compile returns a complex number as a plain 2-array -------------------------------------

@rt3(6)
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

@rt3(7)
@pytest.mark.parametrize("fn,value,sigma", [("cot", 0.642, 0.1412), ("sec", 1.851, 0.2882), ("csc", 1.188, 0.0763)])
def test_7_cot_sec_csc_propagate_uncertainty(fn, value, sigma):
    out = run(f"x = 1.0 ± 0.1\nprint {fn}(x)")      # today: KeyError: 'cot' (a Python traceback)
    v, s = out.split("±")
    assert float(v) == pytest.approx(value, abs=0.01) and float(s) == pytest.approx(sigma, abs=0.01)


# ---- #8: a negative measurement with a unit can't be written ---------------------------------------------

@rt3(8)
def test_8_negative_value_with_unit_after_uncertainty():
    # today: "the uncertainty after ± is length [m] but the value is a plain number"
    assert run("q = -5.0 ± 0.2 m\nprint q") == "-5.00 ± 0.20 m"


@rt3(8)
def test_8_negative_uncertainty_literal_says_negative():
    e = error_of("x = 5.0 ± -0.2 m\nprint x")
    assert "negative" in e.message              # today: the misleading "value is a plain number" message


# ---- #9: rounded large numbers print with trailing zeros that aren't significant -------------------------

@rt3(9)
@pytest.mark.parametrize("src,bad", [
    ("print 1000000 / 3", "333000"),
    ("x = 123456.7 m\nprint x / 1.0", "120000 m"),
    ("print 1.5 * 12345.0", "19000"),
    ("print 2999.85 * 1 MeV to 1 digits", "3000 MeV"),
])
def test_9_rounded_large_numbers_dont_look_exact(src, bad):
    out = run(src)
    assert out != bad                           # e.g. 3.33×10⁵, 1.2×10⁵ m, 1.9×10⁴, 3×10³ MeV


# ---- #10: the Schrödinger PDE doesn't accept the complex constant 𝑖 -------------------------------------

TDSE_IMAG = """m = m_e
σ = 1 nm
k0 = 2 / (1 nm)
solve 𝑖 ħ ∂ψ/∂t = -ħ²/(2*m) * ∂²ψ/∂x²
    with ψ(x, 0 fs) = (2π σ²)^(-1/4) exp(-x² / (4σ²)) exp(𝑖 k0 x), ψ(-40 nm, t) = 0 nm^(-1/2), ψ(40 nm, t) = 0 nm^(-1/2)
    for x from -40 nm to 40 nm, t from 0 fs to 5 fs
print ∫ |ψ(x, 5 fs)|^2 dx from -40 nm to 40 nm to 4 digits
"""


@rt3(10)
def test_10_tdse_with_the_imaginary_constant():
    # today: "the initial value must be a number, but it is a complex number …"
    assert num(run(TDSE_IMAG)) == pytest.approx(1.0, abs=1e-3)


# ---- #11: a run-time error inside a module gives the module's line as if it were the program's ----------

@rt3(11)
def test_11_runtime_error_in_a_module_names_the_module_or_the_calling_line():
    src = "import stats\nx = 1\ny = 2\nprint stats.standard_error([5 m])"
    with pytest.raises(FermiumError) as ei:
        run(src)
    e = ei.value
    # today: "line 6: std needs at least 2 values …" in a 4-line program (line 6 of stats.fm)
    assert e.line == 4 or "stats" in str(e)


# ---- #12: fermium build repeats the zero-integral warning on every call ---------------------------------

@rt3(12)
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

@rt3(13)
@pytest.mark.parametrize("tol", ["1e-6 m", "0.001 s"])
def test_13_tolerance_with_a_unit_is_an_error(tol):
    src = f"solve x' = -x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s tolerance {tol}\nprint x(1 s)"
    with pytest.raises(FermiumError):
        run(src)                                # today: accepted silently


@rt3(13)
def test_13_tolerance_on_its_own_line():
    src = """solve y'' = -9.81 m/s² with y(0 s) = 10 m, y'(0 s) = 0 m/s
  for t from 0 s to 10 s
  until y = 0 m
  tolerance 1e-12
print times(y)[end] to 8 digits
"""
    # `using bdf` and `until …` work on their own lines; today: "expected '=' in this equation but the line ended"
    assert num(run(src)) == pytest.approx((20 / 9.81) ** 0.5, rel=1e-8)


# ---- #14: complex numbers are called vectors in error messages ------------------------------------------

@rt3(14)
@pytest.mark.parametrize("src", [
    "use python numpy as np\nprint np.abs(1 + 2i)",
    "solve z' = 1i z with z(0) = 1 for t from 0 to 1\nprint z([0, 0.5])",
])
def test_14_complex_values_are_not_called_vectors(src):
    e = error_of(src)
    assert "vector" not in e.message and "v.x" not in str(e.hint)


# ---- #15: nits in messages ------------------------------------------------------------------------------

@rt3(15)
def test_15_python_in_natural_units_message_has_no_doubled_word():
    e = error_of("use python numpy as np\nunits natural(ħ = c = 1)\nE = 1 MeV\nprint np.sin(E / (1 MeV))")
    assert "units units" not in e.message


@rt3(15)
def test_15_montecarlo_list_formula_message():
    try:
        out = run("a = 1.0 ± 0.1\npropagate montecarlo 1000 samples\n    b = [a, 2 a]\nprint b")
    except FermiumError as e:
        # today: "needs at least one formula (name = …) in its block", but b = [a, 2 a] is one
        assert "at least one formula" not in e.message
        return
    assert "±" in out


@rt3(15)
def test_15_compile_with_uncertainties_doesnt_blame_the_repl():
    import fermium
    with pytest.raises(FermiumError) as ei:
        fermium.compile("L = 1.20 ± 0.01 m\nf(x) = 2 x\n")
    assert "REPL" not in ei.value.message       # today: "… but not yet in the REPL or Jupyter"
