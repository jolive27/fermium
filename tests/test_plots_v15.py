"""Spec A6.2-A6.4: `plot saved to` prints the absolute path, the pendulum example draws the fitted curve over
the data, and axes are labelled with the column or variable name (`T [s]`, not `data.T [s]`). DECISIONS D251-D253."""
import io
import os
import shutil
import subprocess
import xml.etree.ElementTree as ET

import pytest

from conftest import run
from fermium.interp import run_interpreted

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXAMPLES = os.path.join(ROOT, "examples")
CSV = "L [cm], T [s]\n20, 0.898\n40, 1.272\n60, 1.551\n80, 1.802\n100, 2.004\n"


def interp(src, base_dir):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o, base_dir=base_dir)
    return o.getvalue().strip()


def both(src, base_dir):
    out = run(src, base_dir=base_dir)
    assert interp(src, base_dir) == out          # the compiled path and the interpreter agree
    return out


def axes_of(src, tmp_path, monkeypatch, backend):
    """Run src and return the matplotlib axes of its (last) plot."""
    plt = pytest.importorskip("matplotlib.pyplot")
    figs = []
    monkeypatch.setattr(plt, "close", figs.append)
    backend(src, str(tmp_path))
    return figs[-1].axes[0]


def svg_texts(path):
    root = ET.parse(str(path)).getroot()
    ns = "{http://www.w3.org/2000/svg}"
    series = [e for e in root.iter() if e.tag in (ns + "polyline", ns + "path")]
    return series, ["".join(e.itertext()) for e in root.iter(ns + "text")]


def built(src, work, name="prog"):
    from fermium.aot import build, find_cc
    if not find_cc():
        pytest.skip("no C compiler")
    exe = os.path.join(str(work), name)
    build(src, os.path.join(str(work), name + ".fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(work))
    assert got.returncode == 0, got.stderr
    return got.stdout


# ---- A6.2: the absolute path ------------------------------------------------------------------------------

def test_plot_in_a_subfolder_prints_the_absolute_path(tmp_path):
    # the macOS report: `to "gallery/p.png"` in examples/ printed `gallery/p.png`, the file was in examples/gallery/
    prog = tmp_path / "examples"
    prog.mkdir()
    out = both('xs = [1, 2, 3] [m]\nplot xs vs xs to "gallery/p.png"', str(prog))
    path = prog / "gallery" / "p.png"
    assert out == f"plot saved to {path}" and os.path.isabs(out.split(" to ", 1)[1]) and path.exists()


def test_relative_base_dir_still_prints_an_absolute_path(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    os.mkdir("sub")
    out = run('xs = [1, 2] [s]\nplot xs vs xs to "q.png"', base_dir="sub")
    assert out == f"plot saved to {tmp_path / 'sub' / 'q.png'}"


def test_default_plot_name_is_absolute_too(tmp_path):
    out = both("f(x) = x^2\nplot f vs x from 0 to 3", str(tmp_path))
    saved = out.split("plot saved to ", 1)[1]
    assert os.path.isabs(saved) and os.path.dirname(saved) == str(tmp_path) and os.path.exists(saved)


def test_built_program_prints_the_absolute_path(tmp_path):
    out = built('xs = [1 m, 2 m, 3 m]\nys = [1 s, 4 s, 9 s]\nplot ys vs xs to "out/sq.png"\n', tmp_path)
    assert out == f"plot saved to {tmp_path / 'out' / 'sq.svg'} (standalone programs write SVG)\n"


def test_bootcamp_boxes_show_a_machine_independent_path():
    # the bootcamp's output boxes show the repository as /Users/ada/fermium on every computer
    import importlib.util
    spec = importlib.util.spec_from_file_location("uo", os.path.join(ROOT, "bootcamp", "update_outputs.py"))
    uo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(uo)
    md = os.path.join(ROOT, "bootcamp")
    tmpname = "_a62_check.png"
    try:
        out = uo.run(f'xs = [1, 2] [m]\nplot xs vs xs to "{tmpname}"\n', None, md)
    finally:
        if os.path.exists(os.path.join(md, tmpname)):
            os.remove(os.path.join(md, tmpname))
    assert out == f"plot saved to /Users/ada/fermium/bootcamp/{tmpname}\n"


# ---- A6.3: the fitted curve over the data ----------------------------------------------------------------

@pytest.mark.parametrize("backend", [run, interp])
def test_pendulum_example_draws_the_fit_over_the_data(tmp_path, monkeypatch, backend):
    shutil.copytree(os.path.join(EXAMPLES, "data"), tmp_path / "data")
    src = open(os.path.join(EXAMPLES, "01_pendulum.fm"), encoding="utf-8").read()
    ax = axes_of(src, tmp_path, monkeypatch, lambda s, d: backend(s, base_dir=d))
    lines = ax.get_lines()
    assert len(lines) == 2
    data, curve = lines
    assert data.get_linestyle() == "None" and data.get_marker() == "o"       # the measurements: dots
    assert len(data.get_xdata()) == 11
    assert curve.get_linestyle() == "-" and len(curve.get_xdata()) >= 100   # the fitted curve: a line
    # the curve is T = 2π √(L/g) with the fitted g (9.818 m/s²), drawn in the data's units (cm, s)
    import math
    x, y = curve.get_xdata()[-1], curve.get_ydata()[-1]
    assert x == pytest.approx(120) and y == pytest.approx(2 * math.pi * math.sqrt(1.20 / 9.818), rel=2e-3)
    assert [t.get_text() for t in ax.get_legend().get_texts()] == ["T", "2π √(L/g)"]


def test_pendulum_example_built_svg_has_data_and_curve(tmp_path):
    shutil.copytree(os.path.join(EXAMPLES, "data"), tmp_path / "data")
    src = open(os.path.join(EXAMPLES, "01_pendulum.fm"), encoding="utf-8").read()
    built(src, tmp_path)
    series, texts = svg_texts(tmp_path / "gallery" / "pendulum_data.svg")
    assert len(series) == 2
    assert series[0].tag.endswith("path") and series[1].tag.endswith("polyline")   # dots, then the curve
    assert "T [s]" in texts and "L [cm]" in texts and "2π √(L/g)" in texts


# ---- A6.4: axis labels name the column --------------------------------------------------------------------

@pytest.mark.parametrize("backend", [run, interp])
def test_axis_labels_use_the_column_name(tmp_path, monkeypatch, backend):
    (tmp_path / "p.csv").write_text(CSV, encoding="utf-8")
    ax = axes_of('data = load "p.csv"\nplot data.T vs data.L to "p.png"', tmp_path, monkeypatch,
                 lambda s, d: backend(s, base_dir=d))
    assert ax.get_xlabel() == "L [cm]" and ax.get_ylabel() == "T [s]"


@pytest.mark.parametrize("backend", [run, interp])
def test_a_formula_of_a_column_drops_the_data_name(tmp_path, monkeypatch, backend):
    (tmp_path / "p.csv").write_text(CSV, encoding="utf-8")
    ax = axes_of('data = load "p.csv"\nplot data.T^2 vs data.L to "p.png"', tmp_path, monkeypatch,
                 lambda s, d: backend(s, base_dir=d))
    assert ax.get_xlabel() == "L [cm]" and ax.get_ylabel() == "T² [s²]"


@pytest.mark.parametrize("backend", [run, interp])
def test_two_named_series_both_on_the_axis(tmp_path, monkeypatch, backend):
    src = 'xs = [1 m, 2 m]\nys = [1 s, 2 s]\nzs = [3 s, 4 s]\nplot ys vs xs, zs vs xs to "h.png"'
    ax = axes_of(src, tmp_path, monkeypatch, lambda s, d: backend(s, base_dir=d))
    assert ax.get_ylabel() == "ys [s], zs [s]" and ax.get_xlabel() == "xs [m]"


def test_a_variable_that_is_not_data_keeps_its_dot(tmp_path, monkeypatch):
    src = ("solve r'' = -r/(1 s²) with r(0) = <1, 0> m, r'(0) = <0, 1> m/s for t from 0 s to 1 s\n"
           'plot r.y vs r.x to "o.png"')
    ax = axes_of(src, tmp_path, monkeypatch, lambda s, d: run(s, base_dir=d))
    assert ax.get_xlabel() == "r.x [m]" and ax.get_ylabel() == "r.y [m]"


def test_y_axis_label_rule():
    from fermium.runtime.core import y_axis_label, is_formula_label
    assert is_formula_label("2π √(L/g)") and is_formula_label("model(t)") and is_formula_label("T^2")
    assert not is_formula_label("T") and not is_formula_label("N_Mo") and not is_formula_label("r.x")
    assert y_axis_label([("T [s]", False), ("2π √(L/g) [s]", True)]) == "T [s]"
    assert y_axis_label([("T^2 [s²]", True)]) == "T^2 [s²]"             # only formulas: they stay
    assert y_axis_label([("a [m]", False), ("a [m]", False), ("b [m]", False)]) == "a [m], b [m]"
