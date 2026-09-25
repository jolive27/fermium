"""Findings of the independent red-team review (REDTEAM.md, round 1).  Each test names its finding."""
import io
import math
import os
import re
import subprocess

import pytest

from conftest import run, error_of, warnings_of
from fermium.driver import run_source
from fermium.errors import FermiumError
from fermium.interp import run_interpreted

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def both(src):
    """Run with the JIT and the reference interpreter; they must agree.  Returns (output, runtime warnings)."""
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    o = io.StringIO()
    rt = run_interpreted(src, "<t>", out=o)
    assert o.getvalue() == out.getvalue()
    jit_rw = [ln for ln in err.getvalue().splitlines() if ln.startswith("warning: line")]
    return out.getvalue().strip(), rt.warnings, jit_rw


def both_error(src):
    with pytest.raises(FermiumError) as e1:
        run(src)
    rt_err = None
    try:
        run_interpreted(src, "<t>", out=io.StringIO())
    except FermiumError as e:
        rt_err = e
    assert rt_err is not None and rt_err.message == e1.value.message
    return e1.value


# ---- #1: docs on infinite ranges -------------------------------------------------------------------------

def test_1_reference_doesnt_promise_every_scale():
    ref = open(os.path.join(ROOT, "docs", "reference.md"), encoding="utf-8").read()
    assert "whatever the physical scale" not in ref
    assert "A narrow peak far from the start" in ref


# ---- #2: Hz vs rad/s vs rev/rpm --------------------------------------------------------------------------

@pytest.mark.parametrize("src,value,text", [
    ("print 1 Hz in rpm", "9.55 rpm", "1 Hz is 9.5493 rpm, not 60 rpm"),
    ("f = 50 Hz\nprint f in rev/min", "477 rev/min", "1 Hz is 9.5493 rev/min, not 60 rev/min"),
    ("print 1 Hz in rev/s", "0.159 rev/s", "1 Hz is 0.159155 rev/s, not 1 rev/s"),
    ("print 1 Hz in rad/s", "1 rad/s", "1 Hz is 1 rad/s, not 6.28319 rad/s"),
    ("print 1 kHz in rpm", "9550 rpm", "1 kHz is 9549.3 rpm, not 60000 rpm"),
])
def test_2_hz_to_turns_warns_with_the_right_numbers(src, value, text):
    assert run(src) == value
    w = warnings_of(src)
    assert any(text in m and "Hz here means rad/s" in m for m in w), w
    assert not any("2π/60" in m for m in w)          # the old rpm-only text isn't shown when rpm isn't involved


@pytest.mark.parametrize("src,text", [
    ("print 60 rpm in Hz", "1 rpm is 0.10472 Hz here, not 0.0166667 Hz"),
    ("print 1 rev/s in Hz", "1 rev/s is 6.28319 Hz here, not 1 Hz"),
    ("print 1 °/s in Hz", "angular frequency"),
])
def test_2_turns_to_hz_warns_tailored(src, text):
    w = warnings_of(src)
    assert any(text in m for m in w), w


def test_2_rad_per_s_in_hz_keeps_the_omega_warning():
    w = warnings_of("ω = 3 rad/s\nprint ω in Hz")
    assert len(w) == 1 and "ω in Hz shows the angular frequency itself" in w[0]


def test_2_no_warning_for_safe_conversions():
    for src in ("print 60 rpm in rev/s", "print 1 rev/s in rpm", "print 3000 rpm in rad/s",
                "print 1 kHz in Hz", "print 3 /s in Hz", "f = 50 Hz\nprint 2π f in rad/s"):
        assert warnings_of(src) == [], src


def test_2_adding_hz_and_rad_per_s_warns():
    for src in ("print 1 Hz + 1 rad/s", "print 3 rad/s - 1 Hz", "print 1 rpm + 1 Hz"):
        w = warnings_of(src)
        assert any("Hz and rad/s are the same unit" in m for m in w), (src, w)
    assert warnings_of("print 1 Hz + 2 kHz") == []
    assert warnings_of("print 1 rad/s + 2 rpm") == []


# ---- #3: benchmark honesty (README) ----------------------------------------------------------------------

def test_3_readme_adaptive_row_is_not_a_spot_check():
    readme = open(os.path.join(ROOT, "README.md"), encoding="utf-8").read()
    assert "spot check" not in readme
    assert "same accuracy" not in open(os.path.join(ROOT, "benchmarks", "fermium", "spring_adaptive.fm"),
                                       encoding="utf-8").read()
    jl = open(os.path.join(ROOT, "benchmarks", "julia", "spring_adaptive.jl"), encoding="utf-8").read()
    assert jl.replace(" ", "").count("rtol=1e-6,atol=1e-30)") == 2          # warm-up and timed run
    for lang in ("python", "numpy"):
        py = open(os.path.join(ROOT, "benchmarks", lang, "spring_adaptive.py"), encoding="utf-8").read()
        assert "1e-30" in py and "1e-10" not in py
    fm = open(os.path.join(ROOT, "benchmarks", "fermium", "spring_adaptive.fm"), encoding="utf-8").read()
    assert re.search(r"tolerance\s+1e-6", fm)


def test_3_readme_python_multipliers_match_results():
    """The README's pure-Python multipliers are those of benchmarks/RESULTS.md (within rounding); re-measured
    in M5, so the test reads both files instead of fixed numbers."""
    import re
    readme = open(os.path.join(ROOT, "README.md"), encoding="utf-8").read()
    results = open(os.path.join(ROOT, "benchmarks", "RESULTS.md"), encoding="utf-8").read()
    rows = {"nbody": "N-body", "blackbody": "Blackbody", "unit_loop": "Loop with units"}
    for bench, label in rows.items():
        m = re.search(rf"^\| {bench} \| Python \(pure\) \|[^|]*\|[^|]*\|[^|]*\| ([0-9.]+)×", results, re.M)
        assert m, bench
        want = float(m.group(1))
        line = next(ln for ln in readme.splitlines() if ln.startswith(f"| {label}"))
        got = float(re.search(r"~([0-9.]+)× Julia \|$", line).group(1))
        assert abs(got - want) <= 0.06 * want, (bench, got, want)


# ---- #4: roots at a pole ---------------------------------------------------------------------------------

def test_4_scan_point_on_a_pole_is_skipped():
    out, _, _ = both("solve 1/(x - 1.5) = 2 for x from 1 to 2\nprint x")
    assert out == "2"


def test_4_pole_without_root_is_an_error():
    e = both_error("solve 1/(x - 1.5) = 0 for x from 1 to 2\nprint x")
    assert "jump past each other near 1.5" in e.message
    # the scan misses the pole's exact position: the same error as before
    e = both_error("solve 1/(x - 1.5) = 0 for x from 1 to 2.01\nprint x")
    assert "jump past each other" in e.message


def test_4_even_pole_is_no_solution_and_nan_domains_still_work():
    e = both_error("solve 1/(x - 1.5)^2 = 0 for x from 1 to 2\nprint x")
    assert "no solution between 1 and 2" in e.message
    out, _, _ = both("solve √(x - 1) = 0.5 for x from 0 to 2\nprint x")
    assert out == "1.25"


# ---- #5: fixed-step RK4 that is far too coarse -----------------------------------------------------------

COARSE = "solve x'' = -(10/(1 s))² x with x(0) = 1 m, x'(0) = 0 m/s for t from 0 s to 10 s step {h}\nprint x(10 s) to 6 digits"


def test_5_coarse_rk4_step_warns():
    out, rw, jw = both(COARSE.format(h="0.1 s"))
    assert out == "0.251501 m"                         # still printed, but not silently
    assert len(rw) == 1 and "line 1: the step is too coarse for this equation" in rw[0]
    assert "drop  step  to use the adaptive solver" in rw[0]
    assert jw == rw
    pct = float(re.search(r"estimated error is ([\d.]+)%", rw[0]).group(1))
    assert 30 < pct < 300                              # the true error is |0.2515 - 0.8623| = 61% of 1 m


def test_5_fine_rk4_step_is_quiet():
    out, rw, jw = both(COARSE.format(h="0.001 s"))
    assert float(out.split()[0]) == pytest.approx(math.cos(100), rel=1e-6)
    assert rw == [] and jw == []
    # RK4 is exact for polynomials: never a warning
    assert both("solve w' = 2 m/s with w(0) = 0 m for t from 0 s to 1 s step 0.5 s\nprint w(1 s)")[1] == []


def test_5_warns_once_in_a_loop():
    src = ("for k from 1 to 3\n    solve x' = -k x/(1 s) with x(0) = 1 m for t from 0 s to 10 s step 1 s\n"
           "    print x(10 s)")
    _, rw, jw = both(src)
    assert len(rw) == 1 and len(jw) == 1


# ---- #6: °C ----------------------------------------------------------------------------------------------

def test_6_sum_of_celsius_is_an_error():
    e = both_error("print sum([10 °C, 20 °C])")
    assert "can't add absolute temperatures" in e.message
    assert run("print mean([10 °C, 20 °C])") == "15 °C"


def test_6_dividing_celsius_warns_and_scaling_text_uses_the_factor():
    w = warnings_of("T1 = 20 °C\nprint T1 / 2")
    assert any("divided as kelvins" in m and "293.15 K / 2" in m for m in w), w
    assert warnings_of("T1 = 20 °C\nprint T1 * 1") == []
    w = warnings_of("T1 = 20 °C\nprint 3 T1")
    assert any("3 × 20 °C is 3 × 293.15 K = 606.3 °C" in m for m in w), w
    w = warnings_of("T1 = 20 °C\nk = 3\nprint k T1")
    assert any("scales an absolute temperature" in m and "2 × 20" not in m for m in w), w


# ---- #7: significant figures in example 03 ---------------------------------------------------------------

def test_7_damped_spring_exact_value_has_six_digits():
    src = open(os.path.join(ROOT, "examples", "03_damped_spring.fm"), encoding="utf-8").read()
    out = run(src, base_dir=os.path.join(ROOT, "examples"))
    line = next(ln for ln in out.splitlines() if ln.startswith("x(5 s) numerical:"))
    assert re.search(r"exact: 3\.52006 cm", line), line


# ---- #8: wording for integrals that can't be computed ----------------------------------------------------

def test_8_integral_message_doesnt_claim_divergence():
    msg = str(error_of("print ∫ sin(x)/x dx from 0 to ∞"))
    assert "couldn't compute this integral numerically" in msg and "doesn't converge" not in msg
    assert "sin(x)/x up to ∞" in msg


# ---- #10: nits -------------------------------------------------------------------------------------------

def test_10_sigma_and_wien_from_exact_definitions():
    from fermium.constants import CONSTANTS
    h, c, k = 6.62607015e-34, 299792458.0, 1.380649e-23
    assert CONSTANTS["σ"][0] == 2 * math.pi ** 5 * k ** 4 / (15 * h ** 3 * c ** 2)
    from scipy.special import lambertw
    x = 5 + lambertw(-5 * math.exp(-5)).real
    assert CONSTANTS["b_W"][0] == pytest.approx(h * c / (k * x), rel=1e-15)
    assert run("print σ to 12 digits") == "5.67037441918×10⁻⁸ W/(m² K⁴)"


def test_10_kwh():
    assert run("print 1 kWh in J") == "3600000 J"
    assert run("print 7.2 MJ in kWh") == "2.0 kWh"


@pytest.mark.parametrize("src,text", [
    ("print 1 Gy + 1 Sv", "grays measure absorbed dose"),
    ("print 2 mSv - 1 mGy", "grays measure absorbed dose"),
    ("print 1 Bq + 1 Hz", "becquerels count decays"),
    ("print 1 J + 1 N m", "J is an energy and N m a torque"),
])
def test_10_confusable_units_warn_when_added(src, text):
    w = warnings_of(src)
    assert any(text in m for m in w), w


def test_10_same_named_units_dont_warn():
    for src in ("print 1 Gy + 2 mGy", "print 1 Bq + 1 kBq", "print 1 J + 1 kJ", "print 1 N m + 2 N m"):
        assert warnings_of(src) == [], src


# ---- the standalone runtime has the same messages --------------------------------------------------------

def test_aot_runtime_messages(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "p")
    build(COARSE.format(h="0.1 s"), str(tmp_path / "p.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    assert got.stdout.strip() == "0.251501 m"
    assert "line 1: the step is too coarse for this equation: the estimated error is" in got.stderr
    exe2 = str(tmp_path / "q")
    build("print ∫ sin(x)/x dx from 0 to ∞", str(tmp_path / "q.fm"), exe2)
    got = subprocess.run([exe2], capture_output=True, text=True, timeout=60)
    assert "couldn't compute this integral numerically" in got.stderr + got.stdout
