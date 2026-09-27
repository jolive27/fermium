"""Generate docs/stdlib.md from the standard library's source (M7, D103).

    python3 -m fermium.stdlib_doc > docs/stdlib.md

Each function's documentation is the comment right above it; its signature (with the parameters' units)
comes from the source, and the units of its result are found by the checker, calling it with arguments
in those units.  legacy/tests/test_stdlib.py checks that docs/stdlib.md is up to date.
"""
from __future__ import annotations

import os

from . import ast as A
from .modules import stdlib_dir, stdlib_modules
from .parser import parse


def _comment_above(lines, line):
    out = []
    k = line - 2
    while k >= 0 and lines[k].strip().startswith("#"):
        out.insert(0, lines[k].strip().lstrip("#").strip())
        k -= 1
    return " ".join(out)


def _header(lines):
    out = []
    for ln in lines:
        if not ln.startswith("#"):
            break
        out.append(ln.lstrip("#").strip())
    return out


def _sig_text(lines, fdef):
    text = lines[fdef.line - 1]
    return text[:text.index(")") + 1].strip() if ")" in text else fdef.name


def _result_units(mod, fdef):
    """'energy [J], shown in MeV' for a function whose parameters all have units; None otherwise."""
    from .checker import Checker
    from .errors import Diagnostics, FermiumError
    from .lsp import _type_text
    args = []
    for p in fdef.params:
        if p.unit is None:
            return None
        args.append("1" if p.unit.text.strip() in ("1", "") else f"1 [{p.unit.text.strip()}]")
    src = f"import {mod}\nresult_ = {mod}.{fdef.name}({', '.join(args)})\n"
    d = Diagnostics()
    ck = Checker(d, stdlib_dir())
    try:
        ck.check_program(parse(src, d))
    except FermiumError:
        return None
    sym = ck.globals.names.get("result_")
    return _type_text(ck, sym.ty, getattr(sym, "hint", None)) if sym is not None else None


def render() -> str:
    out = ["# Fermium standard library", "",
           "Modules shipped with Fermium. Import one with `import mechanics` (then `mechanics.kinetic_energy(…)`),",
           "`import astro as a`, or `from nuclear import semf_binding, Q_value`; see "
           "[the reference, Modules](reference.md#modules).", "",
           "Each function checks the units of its arguments (the units in brackets; any unit of the same kind "
           "works, like `km/hr` for `[m/s]`), and its result has the units shown after the arrow. Parameters "
           "without brackets take any units. Every function is tested against a closed form or SciPy in "
           "`legacy/tests/test_stdlib.py`.", "",
           "This page is generated from the modules' source (`fermium/stdlib/*.fm`) by "
           "`python3 -m fermium.stdlib_doc > docs/stdlib.md`.", ""]
    mods = stdlib_modules()
    out.append("Modules: " + ", ".join(f"[{m}](#{m})" for m in mods) + ".")
    out.append("")
    for mod in mods:
        path = os.path.join(stdlib_dir(), mod + ".fm")
        with open(path, encoding="utf-8") as fh:
            src = fh.read()
        lines = src.split("\n")
        prog = parse(src)
        head = _header(lines)
        out += [f"## {mod}", ""]
        if head:
            first = head[0].split(":", 1)[1].strip() if ":" in head[0] else head[0]
            out.append(" ".join([first[:1].upper() + first[1:] + "."] + head[1:]))
            out.append("")
        consts = [s for s in prog.body if isinstance(s, A.Assign)]
        if consts:
            out.append("| Constant | Value | Meaning |")
            out.append("|---|---|---|")
            for s in consts:
                val = lines[s.line - 1].split("=", 1)[1].strip()
                out.append(f"| `{s.name}` | `{val}` | {_comment_above(lines, s.line) or '—'} |")
            out.append("")
        for s in prog.body:
            if not isinstance(s, A.FuncDef) or s.name.startswith("_"):
                continue
            res = _result_units(mod, s)
            arrow = f" → {res}" if res else ""
            out.append(f"- `{_sig_text(lines, s)}`{arrow}  ")
            doc = _comment_above(lines, s.line)
            out.append(f"  {doc[:1].upper() + doc[1:]}")
        out.append("")
    return "\n".join(out).rstrip("\n") + "\n"


if __name__ == "__main__":
    import sys
    sys.stdout.write(render())
