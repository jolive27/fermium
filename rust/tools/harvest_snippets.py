#!/usr/bin/env python3
"""Harvest every string literal of the Python test suite (tests/*.py) as a small .fm program, for the parser
comparison: most are programs or fragments (many of them rejected, which exercises the error messages), the rest
are messages, which both parsers must reject the same way.

    python3 rust/tools/harvest_snippets.py [OUTDIR]     # default rust/target/snippets

Then  python3 rust/tools/compare_parse.py --snippets  compares the two parsers on them too.
"""
import ast
import glob
import hashlib
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "rust", "target", "snippets")
    os.makedirs(out, exist_ok=True)
    seen = set()
    for path in sorted(glob.glob(os.path.join(ROOT, "tests", "*.py"))):
        try:
            with open(path, encoding="utf-8") as f:
                tree = ast.parse(f.read())
        except (SyntaxError, UnicodeDecodeError):
            continue
        for node in ast.walk(tree):
            if isinstance(node, ast.Constant) and isinstance(node.value, str):
                s = node.value
                if len(s) < 2 or len(s) > 20000 or s in seen or "\x00" in s:
                    continue
                seen.add(s)
                h = hashlib.sha1(s.encode("utf-8", "surrogatepass")).hexdigest()[:12]
                try:
                    with open(os.path.join(out, h + ".fm"), "w", encoding="utf-8") as f:
                        f.write(s)
                except UnicodeEncodeError:
                    pass
    print(f"{len(seen)} snippets in {out}")


if __name__ == "__main__":
    main()
