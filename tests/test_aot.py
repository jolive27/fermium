"""`fermium build`: standalone executables print exactly what `fermium run` prints."""
import glob
import os
import re
import shutil
import subprocess

import pytest

from conftest import run
from fermium.aot import build, find_cc
from fermium.errors import FermiumError

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
pytestmark = pytest.mark.skipif(find_cc() is None, reason="no C compiler")


def programs():
    progs = []
    for f in sorted(glob.glob(os.path.join(ROOT, "examples", "*.fm"))):
        src = open(f, encoding="utf-8").read()
        if not re.search(r"^\s*(plot|fit)\b|load\s+\"", src, re.M):
            progs.append(pytest.param(src, os.path.dirname(f), id=os.path.basename(f)))
    for i, m in enumerate(re.findall(r"```fermium\n(.*?)```", open(os.path.join(ROOT, "docs", "reference.md"),
                                                                     encoding="utf-8").read(), re.S)):
        if not re.search(r"\b(plot|fit|load)\b", m):
            progs.append(pytest.param(m, ROOT, id=f"reference#{i + 1}"))
    return progs


@pytest.mark.parametrize("src,base", programs())
def test_built_executable_matches_jit(src, base, tmp_path):
    exe = str(tmp_path / "prog")
    fake = os.path.join(base, "prog.fm")
    build(src, fake, exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    assert got.returncode == 0, got.stderr
    assert got.stdout.strip() == run(src, base_dir=base)


def test_runtime_error_in_executable(tmp_path):
    exe = str(tmp_path / "bad")
    build("xs = [1, 2]\nprint xs[5]\n", str(tmp_path / "bad.fm"), exe)
    r = subprocess.run([exe], capture_output=True, text=True)
    assert r.returncode == 1
    assert r.stderr.strip() == "line 2: index 5 is out of range: the list has 2 elements (valid indexes are 1 to 2)"


def test_plot_programs_are_refused(tmp_path):
    with pytest.raises(FermiumError, match="uses plot"):
        build('xs = [1, 2]\nplot xs vs xs to "a.png"\n', str(tmp_path / "p.fm"), str(tmp_path / "p"))


def test_cli_build(tmp_path, capsys):
    from fermium import cli
    f = tmp_path / "hello.fm"
    f.write_text("print 4π² 1.20 m / (2.21 s)²\n")
    out = str(tmp_path / "hello")
    assert cli.main(["build", str(f), "-o", out]) == 0
    assert subprocess.run([out], capture_output=True, text=True).stdout == "9.70 m/s²\n"
    assert shutil.which(out) or os.path.exists(out)
