"""Every ```fermium code block in the docs and the bootcamp must run without errors."""
import glob
import os
import re

import pytest

from conftest import run

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FILES = sorted(glob.glob(os.path.join(ROOT, "docs", "*.md")) + glob.glob(os.path.join(ROOT, "bootcamp", "*.md"))
               + glob.glob(os.path.join(ROOT, "bootcamp", "solutions", "*.md")) + [os.path.join(ROOT, "README.md")])


def blocks():
    out = []
    for f in FILES:
        if not os.path.exists(f):
            continue
        text = open(f, encoding="utf-8").read()
        for i, m in enumerate(re.finditer(r"```fermium\n(.*?)```", text, re.S)):
            out.append(pytest.param(f, m.group(1), id=f"{os.path.relpath(f, ROOT)}#{i + 1}"))
    return out


@pytest.mark.parametrize("path,code", blocks())
def test_doc_block_runs(path, code, tmp_path):
    # blocks run in the file's directory so relative data files resolve
    run(code, base_dir=os.path.dirname(path))
