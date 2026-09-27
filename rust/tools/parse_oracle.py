#!/usr/bin/env python3
"""The parse oracle: Fermium 1.5's lexer and parser (frozen, in fermium/) on one or more .fm files, printed in the
format `fermium parse --oracle FILE` prints from the Rust port (rust/crates/fermium-syntax/src/sexpr.rs).

    python3 rust/tools/parse_oracle.py FILE.fm            # one file to stdout
    python3 rust/tools/parse_oracle.py --out DIR LIST     # every file named in LIST (one path per line) to DIR/<n>.txt

Output: the tree as S-expressions (every node with line:col+length, its fields, and the facts the parser attaches
to nodes as attributes), then every token (kind, value, spelling, position, and the role the parser gave it), then
the warnings in the order they were produced.  On a parse error: the error (position, message, hint, fix-mode
edits) and the warnings produced before it.
"""
import json
import os
import sys
from dataclasses import fields
from fractions import Fraction

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "legacy"))   # Fermium 1.5 (the oracle) is in legacy/ since v2.0 (D269)
sys.setrecursionlimit(20000)

from fermium import ast as A                      # noqa: E402
from fermium.errors import FermiumError, Diagnostics   # noqa: E402
from fermium.lexer import Token                   # noqa: E402

POS = {"line", "col", "length", "paren"}
REFS = {"unit_left", "limit_div_of"}


def q(s):
    return json.dumps(s, ensure_ascii=False)


def num(v):
    if isinstance(v, bool):
        return "True" if v else "False"
    if isinstance(v, int):
        return str(v)
    return repr(float(v))


def ref(n):
    return f"<{type(n).__name__}@{n.line}:{n.col}+{n.length}>"


def scalar(v):
    if v is None:
        return "None"
    if isinstance(v, bool):
        return "True" if v else "False"
    if isinstance(v, (int, float)):
        return num(v)
    if isinstance(v, Fraction):
        return str(v)
    if isinstance(v, str):
        return q(v)
    if isinstance(v, Token):
        return f"tok@{v.line}:{v.col}"
    if isinstance(v, A.Node):
        return ref(v)
    raise TypeError(type(v))


def is_scalar(v):
    return v is None or isinstance(v, (bool, int, float, Fraction, str))


def val(v, ind):
    if isinstance(v, A.Node):
        return node(v, ind)
    if isinstance(v, list):
        if not v:
            return "[]"
        return "[" + "".join("\n" + " " * (ind + 2) + val(x, ind + 2) for x in v) + "]"
    if isinstance(v, tuple):
        return "(" + " ".join(val(x, ind) for x in v) + ")"
    if isinstance(v, dict):
        return "{" + " ".join(f"{k}:{val(x, ind)}" for k, x in v.items()) + "}"
    return scalar(v)


def info(d):
    """div_info / sum_info: keys sorted, tokens and nodes as references."""
    if d is None:
        return "None"
    out = []
    for k in sorted(d):
        v = d[k]
        if k == "factors":
            s = "[" + " ".join(f"({ref(f)} {i} {scalar(ws)})" for f, i, ws in v) + "]"
        else:
            s = scalar(v)
        out.append(f"{k}:{s}")
    return "{" + " ".join(out) + "}"


def node(n, ind=0):
    s = f"({type(n).__name__}@{n.line}:{n.col}+{n.length}" + (" paren" if n.paren else "")
    names = []
    for f in fields(n):
        if f.name in POS:
            continue
        names.append(f.name)
        v = getattr(n, f.name)
        if is_scalar(v):
            s += f" {f.name}={scalar(v)}"
        elif isinstance(v, dict) and all(is_scalar(x) for x in v.values()):
            s += f" {f.name}=" + val(v, ind)
        else:
            s += "\n" + " " * (ind + 2) + f"{f.name}=" + val(v, ind + 2)
    for k in sorted(n.__dict__):
        if k in names or k in POS:
            continue
        v = n.__dict__[k]
        if k in REFS:
            s += f" #{k}={ref(v)}"
        elif k in ("div_info", "sum_info"):
            s += f" #{k}={info(v)}"
        elif is_scalar(v):
            s += f" #{k}={scalar(v)}"
        else:
            s += "\n" + " " * (ind + 2) + f"#{k}=" + val(v, ind + 2)
    return s + ")"


def token(t):
    extra = " ".join(f"{k}:{scalar(v)}" for k, v in sorted(t.extra.items()))
    v = t.value
    vs = "None" if v is None else (num(v) if isinstance(v, (int, float)) else q(v))
    s = (f"{t.kind} {vs} {q(t.raw)} {t.line}:{t.col} {t.start}-{t.end} ws={int(t.ws_before)} "
         f"sig={t.sigfigs} digit={int(t.digit)} role={q(t.role or '')} extra={{{extra}}}")
    if getattr(t, "unknown_prime", False):
        s += " unknown_prime"
    return s


def warning(w):
    s = f"W {w.line}:{w.col}+{w.length} {q(w.message)}"
    if w.hint:
        s += f" hint={q(w.hint)}"
    return s


def run(source):
    from fermium.lexer import tokenize
    from fermium.parser import Parser
    d = Diagnostics()
    out = []
    try:
        toks = tokenize(source, d)
        prog = Parser(toks, d).parse_program()
    except FermiumError as e:
        s = f"ERROR {e.line}:{e.col}+{e.length} {q(e.message)}"
        if e.hint:
            s += f" hint={q(e.hint)}"
        fix = getattr(e, "fix", None)
        if fix:
            s += " fix=[" + " ".join(f"({a} {b} {q(r)})" for a, b, r in fix) + "]"
        out.append(s)
    except RecursionError:
        out.append("ERROR nested too deeply")
    else:
        out.append("TREE")
        out.append(node(prog))
        out.append("TOKENS")
        out.extend(token(t) for t in toks)
    out.append("WARNINGS")
    out.extend(warning(w) for w in d.warnings)
    return "\n".join(out) + "\n"


def tokens_run(source):
    """Only the lexer: the tokens (or the error) and the warnings."""
    from fermium.lexer import tokenize
    d = Diagnostics()
    out = []
    try:
        toks = tokenize(source, d)
    except FermiumError as e:
        s = f"ERROR {e.line}:{e.col}+{e.length} {q(e.message)}"
        if e.hint:
            s += f" hint={q(e.hint)}"
        out.append(s)
    else:
        out.append("TOKENS")
        out.extend(token(t) for t in toks)
    out.append("WARNINGS")
    out.extend(warning(w) for w in d.warnings)
    return "\n".join(out) + "\n"


def fix_run(source):
    """Fix mode (`fmt --fix`): the edits, and the error it stops on."""
    from fermium.lexer import tokenize
    from fermium.parser import Parser
    d = Diagnostics()
    try:
        toks = tokenize(source, d)
    except FermiumError as e:            # a lexer error: no edits
        return f"ERROR {e.line}:{e.col}+{e.length} {q(e.message)}\nFIXES \n"
    p = Parser(toks, d)
    p.fix_mode = True
    out = []
    try:
        p.parse_program()
    except FermiumError as e:
        out.append(f"ERROR {e.line}:{e.col}+{e.length} {q(e.message)}")
    out.append("FIXES " + " ".join(f"({a} {b} {q(r)})" for a, b, r in p.fixes))
    return "\n".join(out) + "\n"


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv):
    one = fix_run if "--fix" in argv else tokens_run if "--tokens" in argv else run
    argv = [a for a in argv if a not in ("--fix", "--tokens")]
    if argv and argv[0] == "--out":
        outdir, listing = argv[1], argv[2]
        os.makedirs(outdir, exist_ok=True)
        with open(listing, encoding="utf-8") as f:
            paths = [ln.strip() for ln in f if ln.strip()]
        for k, p in enumerate(paths):
            try:
                text = one(read(p))
            except Exception as e:           # a crash of the oracle itself: recorded, not fatal
                text = f"CRASH {type(e).__name__}: {e}\n"
            with open(os.path.join(outdir, f"{k}.txt"), "w", encoding="utf-8") as f:
                f.write(text)
        return 0
    for p in argv:
        sys.stdout.write(one(read(p)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
