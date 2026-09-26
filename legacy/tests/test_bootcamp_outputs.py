"""The output boxes in the bootcamp must be what Fermium really prints.

Each `<!-- output -->` box after a ```fermium block (or a `<!-- run as name.fm -->` block) is re-run with
bootcamp/update_outputs.py's renderer and compared with the box. When this fails after a deliberate change,
run `python3 bootcamp/update_outputs.py` and review the diff.
"""
import importlib.util
import os
import re

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_spec = importlib.util.spec_from_file_location("update_outputs", os.path.join(ROOT, "bootcamp", "update_outputs.py"))
uo = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(uo)


def _split(text):
    """Warnings (each a 'warning: ...' line plus its indented lines) as a sorted list, and the rest."""
    warnings, rest, cur = [], [], None
    for line in text.split("\n"):
        if line.startswith("warning"):
            cur = [line]
            warnings.append(cur)
        elif cur is not None and line.startswith(" "):
            cur.append(line)
        else:
            cur = None
            rest.append(line)
    return sorted("\n".join(w) for w in warnings), "\n".join(rest)


def cases():
    out = []
    for md in uo.default_files():
        for line, code, name, old in uo.boxes(md):
            out.append(pytest.param(md, code, name, old, id=f"{os.path.relpath(md, ROOT)}:{line}"))
    return out


CASES = cases()


def test_there_are_many_boxes():
    assert len(CASES) > 100


@pytest.mark.parametrize("md,code,name,old", CASES)
def test_output_box_is_current(md, code, name, old):
    new = uo.run(code, name, os.path.dirname(md))
    assert _split(new) == _split(old), (
        f"the output box is out of date; run `python3 bootcamp/update_outputs.py`.\nbox:\n{old}\nactual:\n{new}")


def test_split_ignores_warning_order():
    a = "warning: one\n    x\n  hint: h\nwarning: two\n    y\n5 m\n"
    b = "warning: two\n    y\nwarning: one\n    x\n  hint: h\n5 m\n"
    assert _split(a) == _split(b)
    assert _split(a) != _split(a.replace("5 m", "6 m"))
    assert re.search("one", _split(a)[0][0])
