"""Review priority 3 (D110): an integral that comes out exactly 0 because the integrand was 0 at every
sample warns -- a narrow peak in a wide range can hide between the quadrature nodes."""
import io
import subprocess

import pytest

from fermium.aot import build, find_cc
from fermium.driver import run_source
from fermium.interp import run_interpreted

WARN = "this integral came out as exactly 0 because the integrand was 0 at every point"


def native(src, capsys):
    out = io.StringIO()
    run_source(src, "<test>", out=out)
    return out.getvalue().strip(), capsys.readouterr().err


def interp(src, capsys):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip(), capsys.readouterr().err


@pytest.mark.parametrize("run", [native, interp])
@pytest.mark.parametrize("src", [
    "print ∫ exp(-x²) dx from -1e6 to 1e6",                         # √π, all nodes miss the peak
    "print ∫ exp(-(x - 3e5)²) dx from -1e6 to 1e6",
    "σ = 1 nm\nprint ∫ exp(-x²/(2σ²)) dx from -1 m to 1 m",           # with units
    "print ∫ 0 * x dx from 0 to 1",                                 # a true 0 warns too (the message says 'if')
])
def test_all_zero_samples_warn(run, src, capsys):
    out, err = run(src, capsys)
    assert out.split()[0] == "0" and WARN in err
    assert "line" in err and "narrow" in err


@pytest.mark.parametrize("run", [native, interp])
@pytest.mark.parametrize("src,val", [
    ("print ∫ sin(x) dx from 0 to π to 12 digits", 2.0),
    ("print (∫ x dx from -1 to 1) + 1 to 12 digits", 1.0),            # the odd part cancels, nodes are non-zero
    ("print ∫ exp(-x²) dx from -10 to 10 to 12 digits", 1.7724538509055159),
    ("print ∫ exp(-x²) dx from -∞ to ∞ to 12 digits", 1.7724538509055159),
    ("print ∫ 1 dx from 2 to 2", 0.0),                               # an empty range is exactly 0: no warning
])
def test_normal_integrals_are_quiet(run, src, val, capsys):
    out, err = run(src, capsys)
    assert float(out) == pytest.approx(val, rel=1e-11, abs=1e-12)
    assert WARN not in err


@pytest.mark.parametrize("run", [native, interp])
def test_a_zero_component_of_a_vector_integral_is_quiet(run, capsys):
    out, err = run("print ∫ <t, 0> dt from 0 to 1", capsys)
    assert out == "<0.500, 0>" and WARN not in err


@pytest.mark.parametrize("run", [native, interp])
def test_inner_integral_of_a_nested_one_doesnt_count_for_the_outer(run, capsys):
    # the inner integral is 0 only at y = 0, which is never a node; the outer one is fine
    out, err = run("print ∫ (∫ x y dx from 0 to 1) dy from 0 to 1 to 10 digits", capsys)
    assert float(out) == pytest.approx(0.25, rel=1e-10) and WARN not in err


@pytest.mark.skipif(find_cc() is None, reason="no C compiler")
def test_fermium_build_warns_the_same(tmp_path):
    exe = tmp_path / "p"
    build("print ∫ exp(-x²) dx from -1e6 to 1e6", str(tmp_path / "p.fm"), str(exe))
    got = subprocess.run([str(exe)], capture_output=True, text=True, timeout=60)
    assert got.stdout.strip() == "0" and WARN in got.stderr and "line 1" in got.stderr
