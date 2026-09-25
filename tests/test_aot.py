"""`fermium build`: standalone executables print exactly what `fermium run` prints."""
import glob
import math
import os
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET

import pytest

from conftest import run
from fermium.aot import build, find_cc
from fermium.errors import FermiumError

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
pytestmark = pytest.mark.skipif(find_cc() is None, reason="no C compiler")


def programs():
    return [pytest.param(open(f, encoding="utf-8").read(), id=os.path.basename(f))
            for f in sorted(glob.glob(os.path.join(ROOT, "examples", "*.fm")))]


def reference_programs():
    progs = []
    for i, m in enumerate(re.findall(r"```fermium\n(.*?)```", open(os.path.join(ROOT, "docs", "reference.md"),
                                                                     encoding="utf-8").read(), re.S)):
        if not re.search(r"\b(plot|fit|load)\b", m):
            progs.append(pytest.param(m, id=f"reference#{i + 1}"))
    return progs


def as_built(jit_out):
    """What the executable prints instead: plots are written as SVG."""
    return re.sub(r"^plot saved to (.*)\.png$", r"plot saved to \1.svg (standalone programs write SVG)", jit_out,
                  flags=re.M)


def build_and_run(src, work, name="prog"):
    """Build src as if it were work/<name>.fm and run it in work."""
    exe = os.path.join(str(work), name)
    build(src, os.path.join(str(work), name + ".fm"), exe)
    return subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(work))


def compare(src, work):
    got = build_and_run(src, work)
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == as_built(run(src, base_dir=str(work)))
    return got.stdout


@pytest.mark.parametrize("src", programs())
def test_built_example_matches_jit(src, tmp_path):
    # every example, including the load/fit/plot ones; run in a copy so plots don't land in examples/
    shutil.copytree(os.path.join(ROOT, "examples", "data"), tmp_path / "data")
    compare(src, tmp_path)


@pytest.mark.parametrize("src", reference_programs())
def test_built_reference_snippet_matches_jit(src, tmp_path):
    exe = str(tmp_path / "prog")
    build(src, os.path.join(ROOT, "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == run(src, base_dir=ROOT)


def test_runtime_error_in_executable(tmp_path):
    exe = str(tmp_path / "bad")
    build("xs = [1, 2]\nprint xs[5]\n", str(tmp_path / "bad.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True)
    assert r.returncode == 1
    assert r.stderr.strip() == "line 2: index 5 is out of range: the list has 2 elements (valid indexes are 1 to 2)"


def test_cli_build(tmp_path, capsys):
    from fermium import cli
    f = tmp_path / "hello.fm"
    f.write_text("print 4π² 1.20 m / (2.21 s)²\n")
    out = str(tmp_path / "hello")
    assert cli.main(["build", str(f), "-o", out]) == 0
    assert subprocess.run([out], capture_output=True, text=True).stdout == "9.70 m/s²\n"
    assert shutil.which(out) or os.path.exists(out)


# ------------------------------------------------------------------ fit: the same numbers as SciPy's least_squares
def _decay_csv(path):
    noise = [3, -2, 1, 0, -1, 2, -3, 1, 0, 1, -1]
    rows = "".join(f"{float(t)}, {1000 * math.exp(-t / 3.0) + n}\n" for t, n in zip(range(11), noise))
    (path / "decay.csv").write_text("t [ms], N\n" + rows)


def _noisy_csv(path):
    import numpy as np
    r = np.random.default_rng(3)
    t = np.linspace(0, 10, 25)
    y = 5.0 * np.exp(-t / 3.0) + r.normal(0, 0.05, t.size)
    rows = "\n".join(f"{a:.6f}, {b:.6f}" for a, b in zip(t, y))
    (path / "d.csv").write_text("t [s], A [Bq]\n" + rows + "\n")


FITS = [
    ("pend_cm_ms", "pend.csv", "L [cm], T [ms]\n10, 634\n20, 897\n40, 1269\n80, 1794\n",
     'data = load "pend.csv"\nfit T = 2π √(L / g) to data\nprint g in m/s^2'),
    ("celsius_linear", "temp.csv", "T [°C], P [kPa]\n-10, 90\n0, 100\n25, 110\n",
     'd = load "temp.csv"\nfit P = a + b T to d\nprint a in Pa'),
    ("decay_rate_no_guess", None, _decay_csv, 'd = load "decay.csv"\nfit N = A exp(-λ t) to d\nprint 1/λ in ms'),
    ("decay_tau_no_guess", None, _decay_csv, 'd = load "decay.csv"\nfit N = A exp(-t/τ) to d\nprint τ in ms'),
    ("with_guesses", None, _decay_csv,
     'd = load "decay.csv"\nfit N = A exp(-t/τ) to d\n  with A = 900, τ = 2 ms\nprint τ, A'),
    ("curve_fit", None, _noisy_csv,
     'data = load "d.csv"\nfit A = A0 exp(-t / τ) to data\nprint A0 / (1 Bq) to 6 digits\nprint τ / (1 s) to 6 digits'),
    ("exp_plus_background", None, _decay_csv, 'd = load "decay.csv"\nfit N = A exp(-t/τ) + C to d'),
    ("gaussian_bad_model", None, _decay_csv, 'd = load "decay.csv"\nfit N = A exp(-(t/τ)^2) to d'),
    ("guess_from_variable", "pend.csv", "L [m], T [s]\n0.2, 0.898\n0.4, 1.272\n0.6, 1.555\n0.8, 1.795\n",
     'data = load "pend.csv"\ng = 9 m/s²\nfit T = 2π √(L / g) to data\nprint g'),
]


@pytest.mark.parametrize("name,fname,content,src", FITS, ids=[f[0] for f in FITS])
def test_built_fit_matches_jit(name, fname, content, src, tmp_path):
    if callable(content):
        content(tmp_path)
    else:
        (tmp_path / fname).write_text(content, encoding="utf-8")
    out = compare(src, tmp_path)
    assert out.startswith("fit ")


def test_built_degenerate_fit_warns(tmp_path):
    # A and B only appear as A B: no standard errors, and the same warning as fermium run
    _decay_csv(tmp_path)
    got = build_and_run('d = load "decay.csv"\nfit N = A B exp(-t/τ) to d', tmp_path)
    assert got.returncode == 0, got.stderr
    assert "warning: the fit may not have converged" in got.stdout


def test_built_fit_too_few_points(tmp_path):
    (tmp_path / "two.csv").write_text("x, y\n1, 2\n2, 3\n")
    got = build_and_run('d = load "two.csv"\nfit y = a0 + a1 x + a2 x^2 to d', tmp_path)
    assert got.returncode == 1
    assert got.stderr.strip() == "line 2: can't fit 3 parameters to only 2 data points"


# ------------------------------------------------------------------ load: the file is read when the program runs
def test_built_load_reads_file_at_run_time(tmp_path):
    (tmp_path / "m.csv").write_text("x [cm], y\n1, 2\n3, 4\n")
    src = 'd = load "m.csv"\nprint d.x, d.y\nprint len(d.x)\n'
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    (tmp_path / "m.csv").write_text("x [cm], y\n1, 2\n3, 4\n\n5, 6\n")      # new data, same header
    got = subprocess.run([exe], capture_output=True, text=True, cwd=str(tmp_path))
    assert got.stdout == "[1, 3, 5] cm [2, 4, 6]\n3\n"


def _load_error(tmp_path, csv_text):
    (tmp_path / "m.csv").write_text("x [cm], y\n1, 2\n")
    exe = str(tmp_path / "prog")
    build('d = load "m.csv"\nprint d.x', str(tmp_path / "prog.fm"), exe)
    run_dir = tmp_path / "run"
    run_dir.mkdir()
    if csv_text is not None:
        (run_dir / "m.csv").write_text(csv_text, encoding="utf-8")
    got = subprocess.run([exe], capture_output=True, text=True, cwd=str(run_dir))
    assert got.returncode == 1
    return got.stderr.strip()


def test_built_load_missing_file(tmp_path):
    err = _load_error(tmp_path, None)
    assert err.startswith("line 1: can't find the file 'm.csv' (looked in ")
    assert str(tmp_path / "run") in err and "the folder you run the program in" in err


def test_built_load_bad_number_same_message_as_jit(tmp_path):
    bad = "x [cm], y\n1, 2\n3, abc\n"
    err = _load_error(tmp_path, bad)
    (tmp_path / "jit").mkdir()
    (tmp_path / "jit" / "m.csv").write_text(bad)
    with pytest.raises(FermiumError) as ei:
        run('d = load "m.csv"\nprint d.x', base_dir=str(tmp_path / "jit"))
    assert ei.value.message == "m.csv, line 3: not a number: ['3', ' abc']"
    assert err == "line 1: m.csv, line 3: not a number: ['3', ' abc']"


def test_built_load_wrong_column_count(tmp_path):
    assert _load_error(tmp_path, "x [cm], y\n1, 2, 3\n") == "line 1: m.csv, line 2: expected 2 values but found 3"


def test_built_load_changed_header(tmp_path):
    err = _load_error(tmp_path, "x [m], y\n1, 2\n")
    assert err == ("line 1: m.csv: the header is 'x [m], y' but the program was built for 'x [cm], y' (the columns' "
                   "units are compiled in; build it again with  fermium build)")


def test_built_load_quoted_fields_and_bom(tmp_path):
    (tmp_path / "q.csv").write_text('﻿"x [s]","y [m]"\r\n"1",2\r\n3,"4.5"\r\n', encoding="utf-8")
    compare('d = load "q.csv"\nprint d.x, d.y', tmp_path)


# ------------------------------------------------------------------ plot: an SVG file
def _svg(path):
    root = ET.parse(str(path)).getroot()
    ns = "{http://www.w3.org/2000/svg}"
    series = [e for e in root.iter() if e.tag in (ns + "polyline", ns + "path")]
    texts = ["".join(e.itertext()) for e in root.iter(ns + "text")]
    return series, texts


def test_built_plot_fit_example_svg(tmp_path):
    shutil.copytree(os.path.join(ROOT, "examples", "data"), tmp_path / "data")
    src = open(os.path.join(ROOT, "examples", "18_fit_decay_data.fm"), encoding="utf-8").read()
    out = compare(src, tmp_path)
    assert "plot saved to gallery/ba137m_fit.svg (standalone programs write SVG)" in out
    series, texts = _svg(tmp_path / "gallery" / "ba137m_fit.svg")
    assert len(series) == 2 and all(s.tag.endswith("path") for s in series)      # measured data: markers
    assert "data.t [s]" in texts and "data.rate [1/s], model(data.t) [1/s]" in texts
    assert "data.rate" in texts and "model(data.t)" in texts                      # the legend


def test_built_plot_lines_legend_and_units(tmp_path):
    src = open(os.path.join(ROOT, "examples", "08_bateman_chain.fm"), encoding="utf-8").read()
    compare(src, tmp_path)
    series, texts = _svg(tmp_path / "gallery" / "bateman_chain.svg")
    assert len(series) == 3 and all(s.tag.endswith("polyline") for s in series)
    assert "N_Mo, N_Tc, N_99" in texts and "t [hr]" in texts
    assert {"N_Mo", "N_Tc", "N_99"} <= set(texts)


def test_built_plot_log_scale_and_title(tmp_path):
    src = open(os.path.join(ROOT, "examples", "27_electrostatics_nabla.fm"), encoding="utf-8").read()
    compare(src, tmp_path)
    series, texts = _svg(tmp_path / "gallery" / "dipole_field.svg")
    assert len(series) == 1
    assert "Dipole field on the axis" in texts and "Ez [V/m]" in texts and "zs [cm]" in texts
    assert {"10", "100", "1000"} <= set(texts)          # decades on the log axis


def test_built_plot_svg_name_and_escaping(tmp_path):
    src = 'xs = [1 m, 2 m, 3 m]\nys = [1 s, 4 s, 9 s]\nplot ys vs xs with title "a < b & c" to "out/sq.svg"\n'
    got = build_and_run(src, tmp_path)
    assert got.stdout == "plot saved to out/sq.svg\n"
    series, texts = _svg(tmp_path / "out" / "sq.svg")
    assert len(series) == 1 and "a < b & c" in texts and "ys [s]" in texts and "xs [m]" in texts


def test_built_plot_formula_default_name(tmp_path):
    src = "f(x) = x^2\nplot f vs x from 0 to 3\n"
    got = build_and_run(src, tmp_path)
    assert got.stdout.strip() == as_built(run(src, base_dir=str(tmp_path)))
    name = re.search(r"plot saved to (\S+\.svg)", got.stdout).group(1)
    series, _ = _svg(tmp_path / name)
    assert len(series) == 1 and len(series[0].get("points").split()) == 400


def test_built_plot_length_mismatch_is_an_error(tmp_path):
    got = build_and_run('xs = [1, 2, 3]\nys = [1, 2]\nplot ys vs xs to "p.png"\n', tmp_path)
    assert got.returncode == 1
    assert got.stderr.strip() == "line 3: plot: the two lists have different lengths (2 and 3 values)"
