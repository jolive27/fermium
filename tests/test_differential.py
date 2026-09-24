"""Differential testing: the native code (LLVM) and the reference interpreter must print the same
thing for every example, doc/bootcamp program and spec snippet."""
import glob
import io
import os
import re

import pytest

from conftest import run
from fermium.interp import run_interpreted
from fermium.errors import FermiumError

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def corpus():
    items = []
    for f in sorted(glob.glob(os.path.join(ROOT, "examples", "*.fm"))):
        items.append(pytest.param(open(f, encoding="utf-8").read(), os.path.dirname(f), id=os.path.basename(f)))
    for md in sorted(glob.glob(os.path.join(ROOT, "docs", "*.md")) + glob.glob(os.path.join(ROOT, "bootcamp", "*.md"))):
        for i, m in enumerate(re.findall(r"```fermium\n(.*?)```", open(md, encoding="utf-8").read(), re.S)):
            if "clock()" in m or "rand()" in m:
                continue
            items.append(pytest.param(m, os.path.dirname(md), id=f"{os.path.basename(md)}#{i + 1}"))
    items.append(pytest.param(open(os.path.join(ROOT, "tests", "programs", "spec_31.fm"), encoding="utf-8").read(),
                              os.path.join(ROOT, "tests", "programs"), id="spec_31"))
    return items


def interp(src, base):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=base)
    return out.getvalue().strip()


@pytest.mark.parametrize("src,base", corpus())
def test_native_and_interpreter_agree(src, base):
    try:
        native = run(src, base_dir=base)
    except FermiumError as e:
        with pytest.raises(FermiumError) as ei:
            interp(src, base)
        assert ei.value.message == e.message
        return
    assert interp(src, base) == native
