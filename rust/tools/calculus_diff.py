#!/usr/bin/env python3
"""Differential test of the calculus printing and values against Fermium 1.5 (the oracle, `python3 -m fermium`).

  python3 rust/tools/calculus_diff.py textbook [--bin PATH]          # 45 textbook antiderivatives: printed F(x)
  python3 rust/tools/calculus_diff.py antiderivatives SEED N [--bin PATH]   # N random integrands: F and F(1.7)
  python3 rust/tools/calculus_diff.py derivatives SEED N [--bin PATH]       # f', f'', f'(1.3), ∫ f from 1 to 2
  python3 rust/tools/calculus_diff.py nabla [--bin PATH]              # ∇, ∇², ∇×, ∇· of 16 textbook potentials

Each program is run by both implementations and the outputs are compared exactly (stdout and stderr, with the file
name removed); the differences are listed and counted. Used for the coverage numbers in rust/DIVERGENCES.md
("Indefinite integrals", "Display of ∇ results"). Needs the Python implementation importable from the repo root.
"""
import argparse
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TEXTBOOK = [
    "x^3 - 2 x + 1", "1/x", "1/x^2", "sqrt(x)", "x sqrt(x^2 + 1)", "sin(x)^2", "cos(x)^3", "sin(x) cos(x)^2",
    "tan(x)^2", "x cos(x)", "x^2 sin(x)", "exp(2 x) cos(3 x)", "ln(x)^2", "x^2 ln(x)", "atan(x)",
    "1/(x^2 + 4)", "1/(x^2 - 4)", "x/(x^2 + 1)", "1/(x (x + 1))", "(x + 1)/(x^2 + 2 x + 2)",
    "1/sqrt(4 - x^2)", "sqrt(4 - x^2)", "sqrt(x^2 + 4)", "1/sqrt(x^2 - 1)", "exp(-x^2)", "x exp(-x^2)",
    "sinh(x) cosh(x)", "1/cosh(x)^2", "x^2 exp(-x)", "exp(x)/(1 + exp(x))", "cos(x)/sin(x)", "sec(x)",
    "1/(1 + cos(x))", "x^4/(x^2 + 1)", "1/(x^3 + 1)", "(2x + 3)^5", "x (2x + 3)^5", "sin(x)^3", "exp(sqrt(x))",
    "x/sqrt(1 - x^2)", "asin(x)", "1/(x ln(x))", "cos(ln(x))/x", "x^3 exp(x^2)", "sin(2x) sin(3x)",
]

POTENTIALS = [
    "k / √(x² + y² + z²)", "½ k (x² + y² + z²)", "exp(-(x² + y²)/σ²)", "x y z", "x² y - y³/3",
    "sin(k x) cos(k y)", "A exp(-a √(x² + y² + z²)) / √(x² + y² + z²)", "ln(x² + y²)", "atan(y / x)",
    "(x² + y²) z", "q / (4π ε_0 √(x² + y² + z²)) + E0 z", "x / (x² + y² + z²)^(3/2)", "cos(x) exp(-y)",
    "GM / √(x² + y² + z²)", "-GM m / √(x² + y²)", "sin(π x / L) sin(π y / L)",
]


def run_both(prog, binp):
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "p.fm")
        with open(path, "w") as f:
            f.write(prog)
        r = subprocess.run([binp, "run", path], capture_output=True, text=True, timeout=120)
        try:
            p = subprocess.run([sys.executable, "-m", "fermium", "run", path], capture_output=True, text=True,
                               timeout=300, cwd=ROOT)
        except subprocess.TimeoutExpired:
            return None
    clean = lambda s: s.replace(path, "").replace("p.fm", "")  # noqa: E731
    return clean(r.stdout + r.stderr), clean(p.stdout + p.stderr)


def report(label, pairs):
    n = diff = 0
    for what, res in pairs:
        if res is None:
            continue
        n += 1
        rs, py = res
        if rs != py:
            diff += 1
            print("DIFF", what)
            for a, b in zip(py.split("\n"), rs.split("\n")):
                if a != b:
                    print("   v1:", a[:220])
                    print("   v2:", b[:220])
                    break
    print(f"{label}: {n - diff} of {n} identical")


def gen(rnd, atoms, funcs, depth, fn_prob=0.3, ops=("+", "-", "*", "/")):
    if depth == 0:
        return rnd.choice(atoms)
    if funcs and rnd.random() < fn_prob:
        return f"{rnd.choice(funcs)}({gen(rnd, atoms, funcs, depth - 1, fn_prob, ops)})"
    op = rnd.choice(ops)
    return f"({gen(rnd, atoms, funcs, depth - 1, fn_prob, ops)} {op} {gen(rnd, atoms, funcs, depth - 1, fn_prob, ops)})"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["textbook", "antiderivatives", "derivatives", "nabla"])
    ap.add_argument("seed", nargs="?", type=int, default=1)
    ap.add_argument("n", nargs="?", type=int, default=80)
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "release", "fermium"))
    a = ap.parse_args()
    rnd = random.Random(a.seed)
    if a.mode == "textbook":
        report("textbook antiderivatives", ((c, run_both(f"F = ∫ {c} dx\nprint F\n", a.bin)) for c in TEXTBOOK))
    elif a.mode == "antiderivatives":
        atoms = ["x", "x^2", "x^3", "k", "b", "2", "3 x", "sin(k x)", "cos(x)", "exp(-k x)", "exp(2 x)", "1/x",
                 "(x + 1)", "sqrt(x)", "ln(x)", "1/(x^2 + k^2)", "x exp(x)"]
        progs = []
        for _ in range(a.n):
            e = gen(rnd, atoms, [], rnd.randint(0, 2), 0, ("+", "-", "*", "*"))
            progs.append((e, f"k = 2\nb = -3\nF = ∫ {e} dx\nprint F\nprint F(1.7)\n"))
        report("random antiderivatives", ((e, run_both(p, a.bin)) for e, p in progs))
    elif a.mode == "derivatives":
        atoms = ["x", "2", "a", "3 x", "x^2", "(x + 1)", "(1 - x)", "0.5", "x^3"]
        funcs = ["sin", "cos", "exp", "ln", "sqrt", "atan", "tanh", "sinh", "cbrt", "abs"]
        progs = []
        for _ in range(a.n):
            e = gen(rnd, atoms, funcs, rnd.randint(1, 3), 0.3, ("+", "-", "*", "/", "*"))
            progs.append((e, f"a = 2\nf(x) = {e}\nprint f'\nprint f''\nprint f'(1.3), f''(1.3)\n"
                             f"print ∫ f(x) dx from 1 to 2\n"))
        report("random derivatives", ((e, run_both(p, a.bin)) for e, p in progs))
    else:
        head = "k = 2\nσ = 1.5\nA = 3\na = 0.5\nq = 1\nE0 = 2\nGM = 1\nm = 1\nL = 2\n"
        report("∇ of potentials", ((f, run_both(head + f"f(x, y, z) = {f}\nprint ∇f\nprint ∇²f\n"
                                                        f"F(x, y, z) = ∇f(x, y, z)\nprint ∇×F\nprint ∇·F\n", a.bin))
                                   for f in POTENTIALS))


if __name__ == "__main__":
    main()
