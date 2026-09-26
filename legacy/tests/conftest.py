import io
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))   # legacy/: the v1 package `fermium`

from fermium.driver import run_source  # noqa: E402
from fermium.errors import FermiumError, Diagnostics  # noqa: E402
from fermium.parser import parse  # noqa: E402
from fermium.checker import Checker  # noqa: E402


HARVEST = os.environ.get("FERMIUM_HARVEST")      # conformance/harvest.py: record every program the tests run


def _harvest(src, base_dir, out, err, error):
    import json
    rec = {"src": src, "base_dir": os.path.abspath(base_dir) if base_dir else None, "stdout": out,
           "stderr": err, "error": None if error is None else {
               "message": error.message, "line": error.line, "col": error.col, "hint": error.hint,
               "kind": type(error).__name__}}
    with open(os.path.join(HARVEST, f"{os.getpid()}.jsonl"), "a", encoding="utf-8") as fh:
        fh.write(json.dumps(rec, ensure_ascii=False) + "\n")


if HARVEST:
    # wrap run_source itself, before the test modules import it, so every program run in-process is recorded
    import fermium.driver as _drv
    _orig_run_source = _drv.run_source

    def _harvesting_run_source(source, filename="<program>", out=None, base_dir=None, show_warnings=True, err=None):
        o, e = io.StringIO(), io.StringIO()
        try:
            r = _orig_run_source(source, filename, out=o, base_dir=base_dir, show_warnings=show_warnings, err=e)
        except FermiumError as ex:
            _harvest(source, base_dir, o.getvalue(), e.getvalue(), ex)
            raise
        finally:
            for dst, buf in ((out if out is not None else sys.stdout, o), (err if err is not None else sys.stderr, e)):
                if dst is not None:
                    dst.write(buf.getvalue())
        _harvest(source, base_dir, o.getvalue(), e.getvalue(), None)
        return r
    _drv.run_source = _harvesting_run_source
    run_source = _harvesting_run_source    # noqa: F811


def run(src, base_dir=None):
    """Run a program; return its printed output (stripped)."""
    out = io.StringIO()
    err = io.StringIO()
    run_source(src, "<test>", out=out, base_dir=base_dir, err=err)
    return out.getvalue().strip()


def run_lines(src, base_dir=None):
    return run(src, base_dir).split("\n")


def error_of(src, base_dir=None):
    """Compile+run a program that must fail; return the FermiumError."""
    with pytest.raises(FermiumError) as ei:
        run(src, base_dir)
    return ei.value


def _harvest_checked(src, base_dir):
    """Checker-only tests (warnings_of, check_only) are harvested too: the harvest re-runs each program with the
    oracle, so only the source matters here (red team round 9 #4)."""
    if HARVEST:
        _harvest(src, base_dir if base_dir not in (None, ".") else None, "", "", None)


def check_only(src, base_dir="."):
    _harvest_checked(src, base_dir)
    d = Diagnostics()
    prog = parse(src, d)
    Checker(d, base_dir).check_program(prog)
    return d


def warnings_of(src):
    _harvest_checked(src, None)
    d = Diagnostics()
    prog = parse(src, d)
    Checker(d, ".").check_program(prog)
    return [w.message for w in d.warnings]


@pytest.fixture
def fm():
    return run
