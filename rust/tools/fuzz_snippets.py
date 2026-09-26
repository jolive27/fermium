#!/usr/bin/env python3
"""Make mutated programs for the parser comparison (seeded, reproducible): lines of the conformance programs with a
character deleted, inserted or replaced, truncated, or placed after definitions of common unit-named variables
(m, g, s, h, T, L ...), which exercises the A1 unit rule and the rarer error messages.

    python3 rust/tools/fuzz_snippets.py [N] [SEED]    # writes rust/target/fuzz/*.fm (default 20000, seed 1)

Then  python3 rust/tools/compare_parse.py --fuzz
"""
import glob
import os
import random
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
INTERESTING = list("()[]<>|/*^²³⁻'=,:.±≈√∫∂∇½ 1 2 m s g h d k T L μ \n\t\"#_-+·×") + [
    " m", " g", " s", " h", " kg", "/s", " [m]", " dx", " d/dt ", " where m = 2 kg", " in km", " ± 0.1",
    " to 3 digits", " from 0 to 1", " (", ") ", " 1/2 ", " 73/24 ", " π", " 𝑖", " ∞", " then", " else", " and ",
]
PRELUDES = ["", "m = 2 kg\n", "g = 9.81 m/s²\n", "m = 1\ng = 2\ns = 3\nh = 4\nT = 5\nL = 6\n", "k = 1\nT = 2\n",
            "u = 1\nc = 2\n", "x = 1\nt = 2\n"]


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 20000
    rng = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 1)
    out = os.path.join(ROOT, "rust", "target", "fuzz")
    os.makedirs(out, exist_ok=True)
    for f in glob.glob(os.path.join(out, "*.fm")):
        os.remove(f)
    lines = []
    progs = []
    for p in sorted(glob.glob(os.path.join(ROOT, "conformance", "cases", "*", "*.fm"))):
        with open(p, encoding="utf-8") as fh:
            text = fh.read()
        progs.append(text)
        lines.extend(ln for ln in text.split("\n") if ln.strip() and not ln.lstrip().startswith("#"))
    for k in range(n):
        r = rng.random()
        if r < 0.15:
            src = rng.choice(progs)
            cut = rng.randrange(len(src) + 1)
            src = src[:cut]
        else:
            ln = rng.choice(lines)
            for _ in range(rng.choice([1, 1, 1, 2, 3])):
                op = rng.random()
                pos = rng.randrange(len(ln) + 1)
                if op < 0.3 and ln:
                    ln = ln[:pos] + ln[pos + 1:]
                elif op < 0.7:
                    ln = ln[:pos] + rng.choice(INTERESTING) + ln[pos:]
                elif op < 0.85 and ln:
                    ln = ln[:pos] + rng.choice(INTERESTING) + ln[pos + 1:]
                else:
                    ln = ln[:pos]
            src = rng.choice(PRELUDES) + ln.lstrip() + "\n"
        with open(os.path.join(out, f"{k:05d}.fm"), "w", encoding="utf-8") as fh:
            fh.write(src)
    print(f"{n} mutated programs in {out}")


if __name__ == "__main__":
    main()
