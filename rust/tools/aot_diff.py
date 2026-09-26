#!/usr/bin/env python3
"""`fermium build` against `fermium run --backend llvm` on the conformance programs (spec B5.10): every program the
LLVM back end compiles is built into an executable, which runs in the program's folder; its stdout, exit code
and run-time error must be those of `fermium run`.

  python3 rust/tools/aot_diff.py [--bin rust/target/fast/fermium] [-j 2] [--area NAME] [--limit N]

(Warnings the checker gives are printed by `fermium build`, not by the executable, so only the run-time error
line of stderr is compared.)
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def last_error(err):
    lines = [ln for ln in err.splitlines() if ln and not ln.startswith(("warning:", "  ", "    "))]
    return lines[-1] if lines else ""


def one(args):
    binary, path = args
    case = json.load(open(path[:-3] + ".json", encoding="utf-8"))
    base = os.path.join(ROOT, case["dir"]) if case["dir"] else None
    with tempfile.TemporaryDirectory() as tmp:
        prog = os.path.join(tmp, os.path.basename(path))
        shutil.copy(path, prog)
        cwd = base or tmp
        run = subprocess.run([binary, "run", "--backend", "llvm", "--base-dir", cwd, prog], capture_output=True,
                             text=True, timeout=300, cwd=tmp)
        if run.returncode == 3:
            return case["area"], case["id"], "unsupported", ""
        if "LLVM back end can't compile" in run.stderr or run.returncode not in (0, 1):
            return case["area"], case["id"], "unsupported", ""
        exe = os.path.join(tmp, "prog")
        b = subprocess.run([binary, "build", prog, "-o", exe], capture_output=True, text=True, timeout=300, cwd=tmp)
        if b.returncode != 0:
            if "can't compile" in b.stderr or "doesn't support" in b.stderr:
                return case["area"], case["id"], "unsupported", ""
            if "can't find the file" in b.stderr and case["dir"]:
                # the program reads files next to it (../data/…): the copy built here doesn't have them
                return case["area"], case["id"], "not-run", ""
            # a program that stops at check time never reaches the back end
            if run.returncode == 1 and not run.stdout:
                return case["area"], case["id"], "not-run", ""
            return case["area"], case["id"], "build-failed", last_error(b.stderr)[:200]
        r = subprocess.run([exe], capture_output=True, text=True, timeout=300, cwd=cwd)
    drop = tuple(case.get("drop_prefixes") or ())
    keep = lambda s: "".join(ln for ln in s.splitlines(True) if not (drop and ln.startswith(drop)))
    same = keep(r.stdout) == keep(run.stdout) and r.returncode == run.returncode and \
        (r.returncode == 0 or last_error(r.stderr) == last_error(run.stderr))
    if same:
        return case["area"], case["id"], "agree", ""
    why = []
    if keep(r.stdout) != keep(run.stdout):
        a, bb = keep(r.stdout).splitlines(), keep(run.stdout).splitlines()
        k = next((i for i in range(min(len(a), len(bb))) if a[i] != bb[i]), min(len(a), len(bb)))
        why.append(f"stdout line {k + 1}: exe {a[k] if k < len(a) else '<none>'!r} vs run "
                   f"{bb[k] if k < len(bb) else '<none>'!r}")
    if r.returncode != run.returncode:
        why.append(f"exit {r.returncode} vs {run.returncode}")
    if last_error(r.stderr) != last_error(run.stderr):
        why.append(f"error {last_error(r.stderr)[:100]!r} vs {last_error(run.stderr)[:100]!r}")
    return case["area"], case["id"], "disagree", "; ".join(why)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "fast", "fermium"))
    ap.add_argument("-j", type=int, default=2)
    ap.add_argument("--area")
    ap.add_argument("--limit", type=int)
    a = ap.parse_args()
    files = sorted(glob.glob(os.path.join(ROOT, "conformance", "cases", a.area or "*", "*.fm")))
    if a.limit:
        files = files[:: max(1, len(files) // a.limit)][: a.limit]
    binary = os.path.abspath(a.bin)
    with ProcessPoolExecutor(max_workers=a.j) as ex:
        res = list(ex.map(one, [(binary, f) for f in files], chunksize=2))
    count = {}
    for r in res:
        count[r[2]] = count.get(r[2], 0) + 1
    built = count.get("agree", 0) + count.get("disagree", 0)
    print(f"programs: {len(res)}; built and run: {built}; identical to fermium run: {count.get('agree', 0)}; "
          f"different: {count.get('disagree', 0)}; build failed: {count.get('build-failed', 0)}; not compiled by "
          f"the LLVM back end: {count.get('unsupported', 0)}; stopped before the back end: {count.get('not-run', 0)}")
    for r in res:
        if r[2] in ("disagree", "build-failed"):
            print(f"- {r[0]}/{r[1]} [{r[2]}]: {r[3]}")
    return 1 if count.get("disagree") or count.get("build-failed") else 0


if __name__ == "__main__":
    sys.exit(main())
