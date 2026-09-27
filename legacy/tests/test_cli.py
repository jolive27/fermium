"""The `fermium-legacy` command (Fermium 1.5's CLI; `fermium` is the Rust binary since v2.0), run as a real subprocess."""
import os
import shutil
import subprocess
import sys

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

PENDULUM = "L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g\nprint g in ft/s²\n"
BAD = "x = 3 m\nt = 2 s\ny = x + t\nprint y\n"


def fermium(*args, cwd=None, stdin=None):
    exe = shutil.which("fermium-legacy")   # v1's console script (D269); `fermium` is the Rust binary
    cmd = [exe] if exe else [sys.executable, "-m", "fermium.cli"]
    return subprocess.run(cmd + list(args), capture_output=True, text=True, cwd=cwd, input=stdin,
                          timeout=120, encoding="utf-8")


@pytest.fixture
def prog(tmp_path):
    def write(name, text):
        p = tmp_path / name
        p.write_text(text, encoding="utf-8")
        return p
    return write


def test_run_ok(prog):
    r = fermium("run", str(prog("pendulum.fm", PENDULUM)))
    assert r.returncode == 0, r.stderr
    assert r.stdout == "9.70 m/s²\n31.8 ft/s²\n"
    assert "Traceback" not in r.stderr


def test_run_shorthand(prog):
    p = prog("pendulum.fm", PENDULUM)
    r = fermium(str(p))
    assert r.returncode == 0, r.stderr
    assert r.stdout.startswith("9.70 m/s²")


def test_run_unit_error(prog):
    r = fermium("run", str(prog("bad.fm", BAD)))
    assert r.returncode == 1
    assert r.stdout == ""                      # nothing ran: the error is found before running
    assert "Traceback" not in r.stderr
    lines = r.stderr.rstrip("\n").split("\n")
    assert lines[0] == "bad.fm, line 3: can't add length [m] to time [s]"
    assert lines[1] == "    y = x + t"
    assert set(lines[2].strip()) == {"^"}
    assert lines[3].startswith("  hint:")


def test_run_runtime_error(prog):
    r = fermium("run", str(prog("idx.fm", "xs = [1, 2]\nprint xs[1]\nprint xs[3]\n")))
    assert r.returncode == 1
    assert r.stdout == "1\n"
    assert "out of range" in r.stderr
    assert "Traceback" not in r.stderr
    assert len(r.stderr.strip().split("\n")) <= 4


def test_run_syntax_error(prog):
    r = fermium("run", str(prog("syn.fm", "x = 3 $ 4\n")))
    assert r.returncode == 1
    assert "unexpected character" in r.stderr
    assert "Traceback" not in r.stderr


def test_run_warning_goes_to_stderr(prog):
    r = fermium("run", str(prog("w.fm", "x = 4\nprint 2/x (3)\n")))
    assert r.returncode == 0
    assert r.stdout == "0.167\n"
    assert "warning" in r.stderr


def test_run_missing_file(tmp_path):
    r = fermium("run", "nope.fm", cwd=str(tmp_path))
    assert r.returncode != 0
    assert "can't find the file 'nope.fm'" in r.stderr
    assert "Traceback" not in r.stderr


def test_run_relative_data_path(tmp_path):
    (tmp_path / "d.csv").write_text("x [m], y [s]\n1, 2\n3, 4\n")
    (tmp_path / "p.fm").write_text('data = load "d.csv"\nprint data.y\n', encoding="utf-8")
    r = fermium("run", str(tmp_path / "p.fm"), cwd=ROOT)
    assert r.returncode == 0, r.stderr
    assert r.stdout == "[2, 4] s\n"


def test_run_time_flag(prog):
    r = fermium("run", "--time", str(prog("p.fm", "print 1\n")))
    assert r.returncode == 0
    assert r.stdout == "1\n"
    assert "parse" in r.stderr and "run" in r.stderr


def test_check_ok(prog):
    p = prog("pendulum.fm", PENDULUM)
    r = fermium("check", str(p))
    assert r.returncode == 0
    assert "no problems found" in r.stdout
    assert "9.70" not in r.stdout                 # check doesn't run the program


def test_check_error(prog):
    r = fermium("check", str(prog("bad.fm", BAD)))
    assert r.returncode == 1
    assert "line 3: can't add length [m] to time [s]" in r.stderr
    assert "Traceback" not in r.stderr


def test_check_missing_file(tmp_path):
    r = fermium("check", "nope.fm", cwd=str(tmp_path))
    assert r.returncode != 0
    assert "can't find the file" in r.stderr


def test_doctor():
    r = fermium("doctor")
    assert r.returncode == 0, r.stdout + r.stderr
    assert "llvmlite" in r.stdout
    assert "Traceback" not in r.stderr


def test_version():
    from fermium import __version__
    r = fermium("--version")
    assert r.returncode == 0
    assert r.stdout.strip() == f"fermium {__version__}"


def test_repl_from_pipe():
    r = fermium(stdin="L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g\n")
    assert r.returncode == 0, r.stderr
    assert "9.70 m/s²" in r.stdout


def test_fmt_ascii(prog):
    r = fermium("fmt", str(prog("p.fm", "E = ½ m v²\n")), "--ascii")
    assert r.returncode == 0
    assert r.stdout == "E = (1/2) m v^2\n"


def test_fmt_error(prog):
    r = fermium("fmt", str(prog("p.fm", "x = (\n")), "--pretty")
    assert r.returncode == 1
    assert "Traceback" not in r.stderr


def test_unknown_command():
    r = fermium("frobnicate")
    assert r.returncode != 0
    assert "Traceback" not in r.stderr


def test_ctrl_c_stops_an_infinite_loop(tmp_path):
    import signal
    import time
    p = tmp_path / "loop.fm"
    p.write_text("x = 1\nwhile x > 0\n    x += 1\n")
    proc = subprocess.Popen([sys.executable, "-m", "fermium.cli", "run", str(p)], stderr=subprocess.PIPE,
                            stdout=subprocess.PIPE, text=True)
    time.sleep(1.5)
    proc.send_signal(signal.SIGINT)
    _, err = proc.communicate(timeout=10)
    assert "stopped by Ctrl+C" in err
    assert "Traceback" not in err
