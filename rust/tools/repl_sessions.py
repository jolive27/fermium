#!/usr/bin/env python3
"""Scripted REPL sessions and v1's output for them: the fixtures of rust/crates/fermium-repl/tests/sessions.rs.

    python3 rust/tools/repl_sessions.py            # (re)write the fixtures from Fermium 1.5 (fermium/repl.py)

Sessions: every string given to repl()/repl_lines() in tests/test_repl.py, plus the extra ones below. Each is
run through v1's REPL non-interactively (stdin not a terminal), as `python3 -m fermium repl < session` does.
"""
import ast
import io
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "rust", "crates", "fermium-repl", "tests", "sessions")
sys.path.insert(0, ROOT)

EXTRA = [
    # errors, the terminal-command hint, :vars, blocks, \name expansion
    "xs = [1, 2]\nprint xs[5]\nprint xs[1]\nx = 3 m + 2 s\nfermium run ke.fm\ncd = 3\n:vars\nf(x) = x^2\n"
    "for i from 1 to 2\n    print i\n\nif 1 > 0\n    print \"a\"\nelse\n    print \"b\"\nprint \\theta\nθ = 1\n"
    "print θ\nprint 1 m\n0.1 m\nx=2\n:vars\n",
    ":vars\n",
    "ls\nls -la\npython3 prog.py\ngit status\nls = 2\nls\n",
    "g = 9.81 m/s²\nh = 10 m\nv = √(2 g h)\nv\nv in km/h\nprint v to 2 digits\n",
    "E = 3 J\nE = 2 m\nprint E\nE + 1 m\nE + 1 J\n",
    "f(x) = 3 x²\nprint f(2 m)\nprint f'(2 m)\n:vars\n",
    "v = [1, 2, 3] m\nprint v\nv\n:vars\nM = [[1, 2], [3, 4]]\nM\n",
    "z = 3 + 4i\nprint abs(z)\nz\n",
    "x = 5\nwhile x > 0\n    x -= 2\nprint x\n",
    "total = 0\nfor k from 1 to 4\n    total += k\n    if total > 5\n        print \"big\"\n\nprint total\n",
    "sq(x) =\n    y = x * x\n    return y\nprint sq(3 m)\n",
    "print 1 / 0\nprint 2\n",
    "n = 3\nn = n + 1\nprint n\n",
    "a = 1 m\nb = a +\nprint 3\n",
    "print \"hello\"\n\"text\"\nprint true\n1 < 2\n",
    "k = 4 N/m\nm = 1 kg\nω = √(k/m)\nprint ω\nT = 2π/ω\nprint T\n",
    "print ∫ x² dx from 0 to 3\n",
    "c\nħ\nprint c in km/s\n",
    "units natural(ħ = c = 1)\nprint 1 GeV\n",
    "x = 2\nprint x\\^3\n\\alpha = 0.5\nprint α\n",
    "solve y' = -y / (2 s) with y(0) = 4 m for t from 0 s to 3 s\nprint y(2 s)\n:vars\n",
    "L = [1, 2, 3]\npush(L, 4)\nprint L\nprint len(L)\n",
    ":help\n",
    "quit\nprint 1\n",
    "p = 101325 Pa\np in atm\np in bar\n",
    "T = 20 °C\nprint T\nT in K\n",
    "print sin(30°)\n",
    "f(x) = x^2\nf = 3\nprint f\n",
    "x = 1\nx = [1, 2]\nprint x\n",
    "for i from 1 to 3\n    print i\nprint \"after\"\n",
    "if 2 > 1\n    print \"yes\"\nelif 1 > 2\n    print \"no\"\nelse\n    print \"maybe\"\n",
]


def sessions():
    tree = ast.parse(open(os.path.join(ROOT, "tests", "test_repl.py"), encoding="utf-8").read())
    seen = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Call) and getattr(node.func, "id", None) in ("repl", "repl_lines"):
            if node.args and isinstance(node.args[0], ast.Constant) and isinstance(node.args[0].value, str):
                s = node.args[0].value
                if s not in seen:
                    seen.append(s)
    for s in EXTRA:
        if s not in seen:
            seen.append(s)
    return seen


def main():
    from fermium.repl import main as repl_main
    os.makedirs(OUT, exist_ok=True)
    for f in os.listdir(OUT):
        os.remove(os.path.join(OUT, f))
    cwd = os.getcwd()
    for i, s in enumerate(sessions()):
        out = io.StringIO()
        os.chdir(OUT)
        try:
            repl_main(stdin=io.StringIO(s), stdout=out)
        finally:
            os.chdir(cwd)
        with open(os.path.join(OUT, f"{i:03d}.in"), "w", encoding="utf-8") as fh:
            fh.write(s)
        with open(os.path.join(OUT, f"{i:03d}.out"), "w", encoding="utf-8") as fh:
            fh.write(out.getvalue())
    print(f"wrote {len(sessions())} sessions to {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
