#!/usr/bin/env python3
"""Time the benchmark programs (benchmarks/<lang>/<name>.*) with the Rust binary's two back ends, Fermium 1.5
(the Python implementation, `python3 -m fermium.cli run`) and Julia, for rust/crates/fermium-codegen/PERF.md.

  python3 rust/tools/llvm_bench.py [--bin rust/target/release/fermium] [-r 5] [--no-v1] [--no-julia]
                                   [--no-interp] [--before OLD_BIN] [NAME ...]

The implementations run interleaved (A B C D, A B C D, ...), so a change in the machine's load hits them alike;
one untimed warm-up round first. Per program and implementation: the median wall time of the whole process
(start-up, compile, run), the median CPU time (user + system) of the process, the median of the program's own
TIME_INNER line (the computation only), and whether the result lines (every line but the TIME_ ones) are the
same as the LLVM back end's.
"""
import argparse
import os
import resource
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BENCH = os.path.join(ROOT, "benchmarks")
JULIA = os.path.join(ROOT, ".tools", "julia", "bin", "julia")
if not os.path.exists(JULIA):          # a worktree: the main checkout's .tools
    JULIA = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(ROOT))), ".tools", "julia", "bin", "julia")


def run(cmd, env=None):
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT, env=env, timeout=1800)
    wall = time.perf_counter() - t
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime)
    inner = None
    for ln in p.stdout.splitlines():
        if ln.startswith("TIME_INNER "):
            try:
                inner = float(ln.split()[1])
            except ValueError:
                pass
            break
    results = [ln for ln in p.stdout.splitlines() if not ln.startswith("TIME_")]
    return wall, cpu, inner, results, p.returncode, p.stderr


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
    ap.add_argument("--no-julia", action="store_true")
    ap.add_argument("--no-interp", action="store_true")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--before", help="another fermium binary (e.g. an older build), timed as a row of its own")
    ap.add_argument("names", nargs="*")
    a = ap.parse_args()
    names = a.names or sorted(f[:-3] for f in os.listdir(os.path.join(BENCH, "fermium")) if f.endswith(".fm"))
    fenv = dict(os.environ, FERMIUM_THREADS=str(a.threads))
    jenv = dict(os.environ, JULIA_DEPOT_PATH=os.path.join(os.path.dirname(os.path.dirname(JULIA)), "..", "julia-depot"))
    print("| Benchmark | Implementation | Wall (median) | CPU (median) | Inner (median) | Same results |")
    print("|---|---|---:|---:|---:|:---:|")
    for n in names:
        fm = os.path.join("benchmarks", "fermium", n + ".fm")
        impls = [("Fermium 2, LLVM JIT", [a.bin, "run", "--backend", "llvm", fm], fenv)]
        if a.before:
            impls.append(("Fermium 2 before, LLVM JIT", [a.before, "run", "--backend", "llvm", fm], fenv))
        if not a.no_interp:
            impls.append(("Fermium 2, tree-walker", [a.bin, "run", "--backend", "interp", fm], fenv))
        if not a.no_v1:
            impls.append(("Fermium 1.5 (Python + llvmlite)", [sys.executable, "-m", "fermium.cli", "run", fm], fenv))
        jl = os.path.join(BENCH, "julia", n + ".jl")
        if not a.no_julia and os.path.exists(JULIA) and os.path.exists(jl):
            args = ["1000000"] if n == "nbody" else []
            thr = [f"--threads={a.threads}"] if n == "forces" else []
            impls.append(("Julia " + subprocess.run([JULIA, "--version"], capture_output=True, text=True)
                          .stdout.split()[-1], [JULIA, *thr, "--startup-file=no",
                                                f"--project={os.path.join(BENCH, 'julia')}", jl, *args], jenv))
        data = {label: {"wall": [], "cpu": [], "inner": [], "res": None, "fail": None} for label, _, _ in impls}
        for rep in range(a.r + 1):
            for label, cmd, env in impls:
                d = data[label]
                if d["fail"]:
                    continue
                w, c, inner, res, code, err = run(cmd, env)
                if code != 0:
                    d["fail"] = err.strip().splitlines()[-1][:90] if err.strip() else f"exit {code}"
                    continue
                d["res"] = res
                if rep:
                    d["wall"].append(w)
                    d["cpu"].append(c)
                    if inner is not None:
                        d["inner"].append(inner)
        ref = data[impls[0][0]]["res"]
        for label, _, _ in impls:
            d = data[label]
            if d["fail"]:
                print(f"| {n} | {label} | failed: {d['fail']} | | | |")
                continue
            med = lambda xs: statistics.median(xs) if xs else None
            same = "ref" if label == impls[0][0] else ("n/a" if label.startswith("Julia") else
                                                       "✓" if d["res"] == ref else "differs")
            print(f"| {n} | {label} | {fmt_t(med(d['wall']))} | {fmt_t(med(d['cpu']))} | {fmt_t(med(d['inner']))} | "
                  f"{same} |", flush=True)


if __name__ == "__main__":
    main()
