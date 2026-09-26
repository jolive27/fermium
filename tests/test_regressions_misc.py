"""Regression tests for assorted adversarial bugs (dev-notes/notes/bugs-adversarial.md): A23, A24, A54."""
import io
import os
import subprocess
import sys

import pytest

from conftest import run, error_of, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def interp(src, base_dir=None):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=base_dir)
    return out.getvalue().strip()


# ---------------------------------------------------------------- A54: ∫ … du with a unit-named variable
@pytest.mark.parametrize("src, want", [
    ("print ∫ 1/u du from 1 to 2 to 6 digits", "0.693147"),
    ("print ∫ 1/s ds from 1 to 2 to 6 digits", "0.693147"),
    ("print ∫ 2/L dL from 1 m to 2 m to 6 digits", "1.38629"),
    ("f(x) = ∫ 1/u du from 1 to x\nprint f(3) to 6 digits", "1.09861"),
])
def test_integration_variable_named_like_a_unit(src, want):
    assert run(src) == want
    assert interp(src) == want
    assert warnings_of(src) == []


def test_integration_variable_is_local_to_the_integral():
    # after the integral, u is the atomic mass unit again
    assert run("print ∫ 1/u du from 1 to 2 to 6 digits\nprint 1/u") == "0.693147\n1 1/u"


def test_d_unit_differential_still_works():
    assert run("print ∫ 2 dm from 0 kg to 1 kg") == "2 kg"


def test_divergent_integral_with_u_is_an_error():
    e = error_of("print ∫ 1/u du from 0 to 1")
    assert "couldn't compute this integral" in e.message


# ---------------------------------------------------------------- runtime errors inside plots
def test_runtime_error_in_plotted_function_stops_the_program(tmp_path):
    src = 'f(x) = ∫ 1/u du from 0 to x\nplot f(x) vs x from 0 to 2 to "p.png"\nprint "after"'
    for runner in (run, interp):
        with pytest.raises(FermiumError) as ei:
            runner(src, base_dir=str(tmp_path))
        assert "couldn't compute this integral" in ei.value.message
        assert not (tmp_path / "p.png").exists()


# ---------------------------------------------------------------- A23: plots with mixed units
def test_plot_series_need_the_same_x_units():
    e = error_of('xs = [1 m, 2 m]\nys = [1 s, 2 s]\nplot ys vs xs, xs vs ys to "h.png"')
    assert "all series in one plot need the same x units" in e.message


def test_plot_series_need_the_same_y_units():
    e = error_of('xs = [1 m, 2 m]\nys = [1 s, 2 s]\nplot ys vs xs, xs vs xs to "h.png"')
    assert "all series in one plot need the same y units" in e.message


def test_plot_series_with_matching_units_is_fine(tmp_path):
    src = 'xs = [1 m, 2 m]\nys = [1 s, 2 s]\nzs = [3 s, 4 s]\nplot ys vs xs, zs in ms vs xs to "h.png"'
    assert run(src, base_dir=str(tmp_path)) == f"plot saved to {tmp_path / 'h.png'}"


def test_plot_length_mismatch_stops_and_leaves_no_png(tmp_path):
    src = 'xs = [1 m, 2 m]\nys = [1 s]\nplot ys vs xs to "c.png"\nprint "after"'
    for runner in (run, interp):
        out = io.StringIO()
        with pytest.raises(FermiumError) as ei:
            if runner is run:
                from fermium.driver import run_source
                run_source(src, "<test>", out=out, base_dir=str(tmp_path), err=io.StringIO())
            else:
                run_interpreted(src, "<test>", out=out, base_dir=str(tmp_path))
        assert "different lengths" in ei.value.message
        assert ei.value.line == 3
        assert "after" not in out.getvalue()
        assert not (tmp_path / "c.png").exists()


# ---------------------------------------------------------------- A24: bad CSV files
def _run_cli(src, tmp_path):
    f = tmp_path / "p.fm"
    f.write_text(src)
    return subprocess.run([sys.executable, "-m", "fermium.cli", "run", str(f)], capture_output=True, text=True,
                          timeout=60, cwd=ROOT)


@pytest.mark.parametrize("csv", ["x [m], y [m]\n1, 2\n2,\n3, 6\n", "x [m], y [m]\n1, 2\n2\n3, 6\n"])
def test_bad_csv_gives_only_the_proper_error(tmp_path, csv):
    (tmp_path / "gap.csv").write_text(csv)
    r = _run_cli('d = load "gap.csv"\nprint d.y\nprint "after"\n', tmp_path)
    assert r.returncode == 1 and "gap.csv, line 3" in r.stderr
    assert "Exception ignored" not in r.stderr and "Traceback" not in r.stderr
    assert "after" not in r.stdout
