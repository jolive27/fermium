#!/usr/bin/env python3
"""Time the benchmark programs (benchmarks/fermium/*.fm) with the Rust binary's two back ends and with Fermium 1.5
(the Python implementation, `python3 -m fermium.cli run`), for rust/crates/fermium-codegen/PERF.md.

  python3 rust/tools/llvm_bench.py [--bin rust/target/release/fermium] [-r 5] [--no-v1] [NAME ...]

Per program and implementation: the median wall time of the whole process (start-up, compile, run) and the
median of the program's own TIME_INNER line (the computation only), and whether the result lines (every line but
the TIME_ ones) are the same as the LLVM back end's.
"""
import argparse
import os
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BENCH = os.path.join(ROOT, "benchmarks", "fermium")


def run(cmd, env=None):
    t = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT, env=env, timeout=1200)
    wall = time.perf_counter() - t
    inner = None
    for ln in p.stdout.splitlines():
        if ln.startswith("TIME_INNER "):
            try:
                inner = float(ln.split()[1])
            except ValueError:
                pass
            break
    results = [ln for ln in p.stdout.splitlines() if not ln.startswith("TIME_")]
    return wall, inner, results, p.returncode, p.stderr


def fmt_t(x):
    if x is None:
        return "—"
    if x >= 1:
        return f"{x:.2f} s"
    if x >= 1e-3:
        return f"{x * 1e3:.1f} ms"
    return f"{x * 1e6:.0f} µs"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "release", "fermium"))
    ap.add_argument("-r", type=int, default=5)
    ap.add_argument("--no-v1", action="store_true")
    ap.add_argument("names", nargs="*")
    a = ap.parse_args()
    names = a.names or sorted(f[:-3] for f in os.listdir(BENCH) if f.endswith(".fm"))
    impls = [("Rust, LLVM JIT", [a.bin, "run", "--backend", "llvm"]),
             ("Rust, tree-walker", [a.bin, "run", "--backend", "interp"])]
    if not a.no_v1:
        impls.append(("Fermium 1.5 (Python + llvmlite)", [sys.executable, "-m", "fermium.cli", "run"]))
    print("| Benchmark | Implementation | Wall (median) | Inner (median) | Same results as LLVM |")
    print("|---|---|---:|---:|:---:|")
    for n in names:
        path = os.path.join("benchmarks", "fermium", n + ".fm")
        ref = None
        for label, cmd in impls:
            walls, inners, res, code, err = [], [], None, 0, ""
            for i in range(a.r + 1):          # one warm-up run
                w, inner, res, code, err = run(cmd + [path])
                if code != 0:
                    break
                if i:
                    walls.append(w)
                    if inner is not None:
                        inners.append(inner)
            if code != 0:
                why = err.strip().splitlines()[-1][:80] if err.strip() else f"exit {code}"
                print(f"| {n} | {label} | failed: {why} | | |")
                continue
            if ref is None:
                ref = res
            same = "✓" if res == ref else "✗"
            print(f"| {n} | {label} | {fmt_t(statistics.median(walls))} | "
                  f"{fmt_t(statistics.median(inners) if inners else None)} | {same} |", flush=True)


if __name__ == "__main__":
    main()
