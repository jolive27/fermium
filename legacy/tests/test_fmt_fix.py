"""`fermium fmt --fix` (spec A1, D235): unit/variable collisions become bracketed units that keep Fermium 1's
reading, and `fmt` says on stderr that it didn't run the program (spec A3.4)."""
import os
import subprocess
import sys

import pytest

from conftest import run
from fermium.fmt import fix_source

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


@pytest.mark.parametrize("src,fixed", [
    ("m = 0.5 kg\nx = 0.1 m\n", "m = 0.5 kg\nx = 0.1 [m]\n"),
    ("m = 0.5 kg\nk = 50 N/m\n", "m = 0.5 kg\nk = 50 [N/m]\n"),               # v1 read on through a tight /
    ("g = 9.81 m/s²\nv = 20 m/s / g\n", "g = 9.81 m/s²\nv = 20 [m/s] / g\n"),  # v1 divided after a spaced /
    ("g = 9.81 m/s²\nv = 20 m/s/g\n", "g = 9.81 m/s²\nv = 20 [m/s/g]\n"),      # ... and read a tight one as grams
    ("g = 2\ny = 70 kg g\n", "g = 2\ny = 70 [kg] g\n"),
    ("T = 300 K\nλ = 2.898e-3 m K / T\n", "T = 300 K\nλ = 2.898e-3 [m K] / T\n"),
    ("m = 2 kg\nn = 8.5e28 m^-3\n", "m = 2 kg\nn = 8.5e28 [m^-3]\n"),
    ("m = 3\nxs = [1, 2] m\n", "m = 3\nxs = [1, 2]*m\n"),                      # v1 multiplied (D222)
    ("h = 2 m\nv = 36 km / h\n", "h = 2 m\nv = 36 [km] / h\n"),
    ("x = 3 m\n", "x = 3 m\n"),                                               # nothing to fix
])
def test_fix(src, fixed):
    assert fix_source(src)[0] == fixed


def test_fix_is_idempotent_and_runs():
    src = "m = 0.5 kg\nk = 50 N/m\nsolve m x'' = -k x with x(0) = 0.1 m, x'(0) = 0 m/s for t from 0 s to 1 s\n" \
          "print x(1 s) to 3 digits\n"
    once, n = fix_source(src)
    assert n == 2 and fix_source(once) == (once, 0)
    assert run(once).endswith(" m")


def test_cli_fix_and_the_not_run_note(tmp_path):
    f = tmp_path / "ke.fm"
    f.write_text("m = 2 kg\nx = 3 m\nprint x\n")
    p = subprocess.run([sys.executable, "-m", "fermium", "fmt", "--fix", str(f)], capture_output=True, text=True,
                       cwd=ROOT, timeout=120)
    assert p.returncode == 0 and p.stdout == "m = 2 kg\nx = 3 [m]\nprint x\n"
    assert "ke.fm (not run — use fermium run)" in p.stderr
    p = subprocess.run([sys.executable, "-m", "fermium", "fmt", "--fix", "-w", str(f)], capture_output=True,
                       text=True, cwd=ROOT, timeout=120)
    assert f.read_text() == "m = 2 kg\nx = 3 [m]\nprint x\n" and "fixed 1 unit/variable collision" in p.stdout


def test_plain_fmt_says_it_did_not_run(tmp_path):
    f = tmp_path / "ke.fm"
    f.write_text("E = 1/2 * 2 kg * (3 m/s)^2\nprint E\n")
    p = subprocess.run([sys.executable, "-m", "fermium", "fmt", "--pretty", str(f)], capture_output=True, text=True,
                       cwd=ROOT, timeout=120)
    assert p.returncode == 0 and "print E" in p.stdout
    assert p.stderr.strip() == "formatted ke.fm (not run — use fermium run)"
