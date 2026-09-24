"""Tier 1/2 'done when': the spec §3.1 snippet runs from a .fm file and in the REPL."""
import io
import os

from conftest import run
from numparse import num
from fermium.repl import main as repl_main

HERE = os.path.dirname(os.path.abspath(__file__))
PROG = os.path.join(HERE, "programs", "spec_31.fm")


def test_spec_snippet_runs_from_file():
    src = open(PROG, encoding="utf-8").read()
    out = run(src, base_dir=os.path.join(HERE, "programs")).split("\n")
    assert out[0] == "9.70 m/s²"
    assert out[1] == "31.8 ft/s²"
    assert out[2] == "plot saved to x_vs_t.png"
    assert out[3].startswith("fit T = 2π √(L/g)")
    assert out[4].strip().startswith("g = 9.8")
    assert "m/s²" in out[4] and "standard error" in out[4]
    assert "10 1/s" in out
    assert "v(t) = -A ω sin(ω t)   [m/s, for t in s]" in out
    assert "a(t) = -A ω² cos(ω t)   [m/s², for t in s]" in out
    assert "1.0 J" in out
    assert os.path.exists(os.path.join(HERE, "programs", "x_vs_t.png"))


def test_pendulum_in_repl():
    stdin = io.StringIO("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g\nprint g in ft/s²\n")
    out = io.StringIO()
    repl_main(stdin=stdin, stdout=out)
    assert out.getvalue().split("\n")[:2] == ["9.70 m/s²", "31.8 ft/s²"]


def test_whole_snippet_in_repl():
    src = open(PROG, encoding="utf-8").read()
    stdin = io.StringIO(src)
    out = io.StringIO()
    cwd = os.getcwd()
    os.chdir(os.path.join(HERE, "programs"))
    try:
        repl_main(stdin=stdin, stdout=out)
    finally:
        os.chdir(cwd)
    text = out.getvalue()
    assert "9.70 m/s²" in text and "internal error" not in text and "line " not in text.replace("warning: line", "")
    assert "v(t) = -A ω sin(ω t)" in text


def test_damped_spring_value():
    src = open(PROG, encoding="utf-8").read()
    last = run(src, base_dir=os.path.join(HERE, "programs")).split("\n")[-1]
    import math
    g, w0 = 0.2, 10.0
    wd = math.sqrt(w0**2 - g**2)
    ref = 0.1 * math.exp(-g * 5) * (math.cos(wd * 5) + g / wd * math.sin(wd * 5))
    assert abs(num(last) - ref) < 1e-3   # printed with 2 significant figures
