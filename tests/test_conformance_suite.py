"""The conformance suite (spec §B3) agrees with its oracle: the Python implementation passes John's Appendix 1
cases (the whole suite runs with `conformance/run --impl legacy`; its last result is conformance/LEGACY.md)."""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_legacy_passes_appendix1_cases(tmp_path):
    p = subprocess.run([sys.executable, os.path.join(ROOT, "conformance", "run"), "--impl", "legacy", "--area",
                        "appendix1", "-j", "1", "--out", str(tmp_path / "c.md")], capture_output=True, text=True,
                       timeout=300, cwd=ROOT)
    assert p.returncode == 0, p.stdout + p.stderr
    assert "5 of 5 programs pass" in (tmp_path / "c.md").read_text()


def _runner():
    import importlib.machinery
    import importlib.util
    path = os.path.join(ROOT, "conformance", "run")
    loader = importlib.machinery.SourceFileLoader("conformance_run", path)
    spec = importlib.util.spec_from_loader("conformance_run", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


def test_runner_rejects_wrong_implementations():
    """Red team round 9 #1–#2: fakes that change numbers, precision, notation, brackets or messages must fail."""
    r = _runner()
    case = {"stdout": "x = 42\n[1, 2, 3] m\n1.50 m/s\n1.5×10³ J\n", "stderr": "", "error": None, "exit": 0}
    assert r.judge(case, case["stdout"], "", None, 0)[0]
    assert r.judge(case, "x = 42\n[1, 2, 3] m\n1.51 m/s\n1.5×10³ J\n", "", None, 0)[0]   # last-digit rounding
    fakes = [
        "x = 43\n[1, 2, 3] m\n1.50 m/s\n1.5×10³ J\n",        # a whole number off by one
        "x = 42\n[1, 2, 3] m\n1.5 m/s\n1.5×10³ J\n",         # fewer significant figures
        "x = 42\n[1, 2, 3] m\n1.500 m/s\n1.5×10³ J\n",       # more significant figures
        "x = 42\n[1, 2, 3] m\n1.50 m/s\n1500 J\n",           # different notation
        "x = 42\n[1, 2, 3] m\n1.50 m/s\n1.5e3 J\n",
        "x = 42\n1 2 3 m\n1.50 m/s\n1.5×10³ J\n",            # brackets dropped
        "x =  42\n[1, 2, 3] m\n1.50 m/s\n1.5×10³ J\n",       # spacing
    ]
    for f in fakes:
        assert not r.judge(case, f, "", None, 0)[0], f
    assert not r.judge(case, case["stdout"], "", None, 1)[0]        # wrong exit code
    ecase = {"stdout": "", "stderr": "", "exit": 1,
             "error": {"message": "x is not a length", "line": 3, "hint": "give it a unit"}}
    good = {"message": "x is not a length", "line": 3, "hint": "give it a unit"}
    assert r.judge(ecase, "", "", good, 1)[0]
    for bad in ({"message": "x is a length not", "line": 3, "hint": "give it a unit"},
                {"message": "x is a length", "line": 3, "hint": "give it a unit"},
                {"message": "x is not a length", "line": None, "hint": "give it a unit"},
                {"message": "x is not a length", "line": 3, "hint": None}):
        assert not r.judge(ecase, "", "", bad, 1)[0], bad
