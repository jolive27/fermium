"""Apply `fermium fmt --fix` (D235) to the Fermium programs written as string literals in test files.

Only literals that parse as a program and stop on a unit/variable collision are touched; each edit only adds
brackets around a unit.  Usage:  python3 tools/fix_test_literals.py tests/test_x.py [...]
"""
import ast
import io
import os
import sys
import tokenize

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from fermium.errors import FermiumError   # noqa: E402
from fermium.fmt import fix_source        # noqa: E402
from fermium.parser import parse          # noqa: E402

ESC = {"n": "\n", "t": "\t", "\\": "\\", '"': '"', "'": "'"}


def body_map(tok):
    """(prefix, body, suffix, offsets): offsets[k] is the index in body of the value's k-th character."""
    s = tok
    i = 0
    while s[i] in "rbuRBU":
        i += 1
    q = s[i:i + 3] if s[i:i + 3] in ('"""', "'''") else s[i]
    pre, body, suf = s[:i + len(q)], s[i + len(q):len(s) - len(q)], q
    raw = "r" in s[:i].lower()
    offs, j = [], 0
    while j < len(body):
        offs.append(j)
        if body[j] == "\\" and not raw and j + 1 < len(body) and body[j + 1] in ESC:
            j += 2
        elif body[j] == "\\" and not raw:
            return None
        else:
            j += 1
    offs.append(len(body))
    return pre, body, suf, offs


def fix_literal(tok):
    try:
        val = ast.literal_eval(tok)
    except Exception:
        return None
    if not isinstance(val, str) or "\n" not in val and len(val) < 4:
        return None
    try:
        parse(val)
        return None
    except FermiumError as e:
        if not getattr(e, "fix", None):
            return None
    except Exception:
        return None
    try:
        from fermium.lexer import tokenize as ftok
        from fermium.parser import Parser
        from fermium.errors import Diagnostics
        p = Parser(ftok(val, Diagnostics()), Diagnostics())
        p.fix_mode = True
        try:
            p.parse_program()
        except FermiumError:
            pass
        fixes = p.fixes
        new_val, _ = fix_source(val)
    except FermiumError:
        return None
    m = body_map(tok)
    if m is None:
        return None
    pre, body, suf, offs = m
    for a, b, rep in sorted(fixes, reverse=True):
        body = body[:offs[a]] + rep + body[offs[b]:]
    new_tok = pre + body + suf
    if ast.literal_eval(new_tok) != new_val:
        return None                    # more than one round of fixes; leave it for a human
    return new_tok


def main():
    for path in sys.argv[1:]:
        src = open(path, encoding="utf-8").read()
        toks = list(tokenize.generate_tokens(io.StringIO(src).readline))
        lines = src.split("\n")
        edits = []
        for t in toks:
            if t.type == tokenize.STRING:
                new = fix_literal(t.string)
                if new is not None:
                    edits.append((t.start, t.end, new))
        if not edits:
            continue
        # apply from the end: convert (row, col) to offsets
        starts = [0]
        for ln in lines:
            starts.append(starts[-1] + len(ln) + 1)
        for (r0, c0), (r1, c1), new in sorted(edits, reverse=True):
            a, b = starts[r0 - 1] + c0, starts[r1 - 1] + c1
            src = src[:a] + new + src[b:]
        open(path, "w", encoding="utf-8").write(src)
        print(f"{path}: {len(edits)} literal(s) fixed")


if __name__ == "__main__":
    main()
