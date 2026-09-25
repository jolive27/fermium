"""The CLI called in-process (so coverage sees it): run, check, fmt, doctor, errors."""
import io
import os

import pytest

from fermium import cli
from fermium.doctor import doctor


def write(tmp_path, name, text):
    p = tmp_path / name
    p.write_text(text, encoding="utf-8")
    return str(p)


def test_run_ok(tmp_path, capsys):
    f = write(tmp_path, "a.fm", "x = 2 m\nprint x in cm\n")
    assert cli.main(["run", f]) == 0
    assert capsys.readouterr().out.strip() == "200 cm"


def test_run_shortcut_and_time(tmp_path, capsys):
    f = write(tmp_path, "a.fm", "print 1 + 1\n")
    assert cli.main([f]) == 0
    assert cli.main(["run", "--time", f]) == 0
    err = capsys.readouterr().err
    assert "LLVM+JIT" in err


def test_emit_llvm(tmp_path, capsys):
    f = write(tmp_path, "a.fm", "print 1 m + 2 m\n")
    assert cli.main(["run", "--emit-llvm", f]) == 0
    assert "define" in capsys.readouterr().out


def test_run_unit_error(tmp_path, capsys):
    f = write(tmp_path, "bad.fm", "x = 2 m + 3 s\n")
    assert cli.main(["run", f]) == 1
    err = capsys.readouterr().err
    assert "bad.fm, line 1: can't add length [m] to time [s]" in err
    assert "^" in err and "Traceback" not in err


def test_run_runtime_error(tmp_path, capsys):
    f = write(tmp_path, "bad.fm", "xs = [1, 2]\nprint xs[3]\n")
    assert cli.main(["run", f]) == 1
    assert "line 2: index 3 is out of range" in capsys.readouterr().err


def test_missing_file(capsys):
    with pytest.raises(SystemExit):
        cli.main(["run", "does_not_exist.fm"])
    assert "can't find the file" in capsys.readouterr().err


def test_check(tmp_path, capsys):
    good = write(tmp_path, "g.fm", "x = 1 m\n")
    bad = write(tmp_path, "b.fm", "x = sin(1 m)\n")
    assert cli.main(["check", good]) == 0
    assert "no problems found" in capsys.readouterr().out
    assert cli.main(["check", bad]) == 1
    assert "sin needs a plain number" in capsys.readouterr().err


def test_fmt_modes(tmp_path, capsys):
    f = write(tmp_path, "f.fm", "theta = pi/2\ny = sqrt(2)*x^2\n")
    assert cli.main(["fmt", f, "--pretty"]) == 0
    assert capsys.readouterr().out == "θ = π/2\ny = √(2)·x²\n"
    assert cli.main(["fmt", f, "--pretty", "-w"]) == 0
    assert open(f, encoding="utf-8").read() == "θ = π/2\ny = √(2)·x²\n"
    assert cli.main(["fmt", f, "--ascii"]) == 0
    assert capsys.readouterr().out.endswith("theta = pi/2\ny = sqrt(2)*x^2\n")


def test_fmt_error(tmp_path, capsys):
    f = write(tmp_path, "f.fm", "y = (1 +\n")
    assert cli.main(["fmt", f]) == 1


def test_fmt_warning_for_unconvertible_name(tmp_path, capsys):
    f = write(tmp_path, "f.fm", "ΔE = 3\n")
    assert cli.main(["fmt", f, "--ascii"]) == 0
    assert "no plain-ASCII spelling" in capsys.readouterr().err


def test_doctor(capsys):
    assert doctor() == 0
    out = capsys.readouterr().out
    assert "Everything looks good" in out and "9.70 m/s²" in out
    assert cli.main(["doctor"]) == 0


def test_doctor_reports_c_compiler(monkeypatch, capsys):
    monkeypatch.setattr("fermium.aot.find_cc", lambda: "/usr/bin/cc")
    assert doctor() == 0
    out = capsys.readouterr().out
    assert "C compiler" in out and "/usr/bin/cc" in out and "fermium build" in out


def test_doctor_missing_c_compiler_is_not_a_problem(monkeypatch, capsys):
    monkeypatch.setattr("fermium.aot.find_cc", lambda: None)
    assert doctor() == 0          # only `fermium build` needs it, so it isn't counted as a problem
    out = capsys.readouterr().out
    assert "no C compiler" in out and "only needed for  fermium build" in out
    assert "xcode-select --install" in out
    assert "Everything looks good" in out


def test_repl_subcommand(monkeypatch, capsys):
    monkeypatch.setattr("sys.stdin", io.StringIO("print 2 m\n"))
    assert cli.main(["repl"]) == 0
    assert "2 m" in capsys.readouterr().out


def test_internal_error_is_friendly(monkeypatch, tmp_path, capsys):
    from fermium import driver

    def boom(*a, **k):
        raise RuntimeError("simulated bug")
    monkeypatch.setattr(driver, "run_source", boom)
    monkeypatch.delenv("FERMIUM_DEBUG", raising=False)
    f = write(tmp_path, "a.fm", "print 1\n")
    assert cli.main(["run", f]) == 3
    err = capsys.readouterr().err
    assert "internal error in Fermium" in err and "Traceback" not in err
    assert os.path.exists(f)
