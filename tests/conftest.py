import io
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from fermium.driver import run_source  # noqa: E402
from fermium.errors import FermiumError, Diagnostics  # noqa: E402
from fermium.parser import parse  # noqa: E402
from fermium.checker import Checker  # noqa: E402


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


def check_only(src, base_dir="."):
    d = Diagnostics()
    prog = parse(src, d)
    Checker(d, base_dir).check_program(prog)
    return d


def warnings_of(src):
    d = Diagnostics()
    prog = parse(src, d)
    Checker(d, ".").check_program(prog)
    return [w.message for w in d.warnings]


@pytest.fixture
def fm():
    return run
