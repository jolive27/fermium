#!/usr/bin/env python3
"""Differential test of the two back ends of the Rust `fermium` (spec §B4): every conformance program
(conformance/cases/*/*.fm) runs with `--backend llvm` and with `--backend interp`, the way conformance/run runs
the binary (a copy in an empty directory, an empty PATH). Where the LLVM back end compiles the program, both must
give the same stdout, the same stderr (warnings and the error with its line and hint) and the same exit code.

  python3 rust/tools/llvm_diff.py [--bin rust/target/fast/fermium] [-j 2] [--area NAME] [--out FILE] [-v]

Prints how many programs the LLVM back end compiles, how many agree, and every disagreement, each judged
against the oracle's expected output too (so "llvm right, interp wrong" is told apart from a back-end bug).
Exit code 1 if any supported program disagrees.
"""
import argparse
import glob
import importlib.machinery
import importlib.util
import json
import os
import stat
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CASES = os.path.join(ROOT, "conformance", "cases")


def load_runner():
    path = os.path.join(ROOT, "conformance", "run")
    loader = importlib.machinery.SourceFileLoader("conformance_run", path)
    spec = importlib.util.spec_from_loader("conformance_run", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


RUN = load_runner()


def wrapper(binary, backend, tmpdir):
    """A tiny script that runs `binary <cmd> --backend <backend> ...` (the runner can't pass flags or env); the
    binary writes the back end that ran to MARK/<program>.<backend> (FERMIUM_BACKEND_INFO)."""
    path = os.path.join(tmpdir, f"fermium-{backend}")
    mark = os.path.join(tmpdir, "marks")
    os.makedirs(mark, exist_ok=True)
    with open(path, "w") as f:
        f.write(f'#!/bin/sh\ncmd=$1\nshift\nfor last; do :; done\n'
                f'FERMIUM_BACKEND_INFO="{mark}/${{last##*/}}.{backend}" exec "{os.path.abspath(binary)}" "$cmd" '
                f'--backend {backend} "$@"\n')
    os.chmod(path, os.stat(path).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return path


def one(args):
    wl, wi, path = args
    case = json.load(open(path[:-3] + ".json", encoding="utf-8"))
    mark = os.path.join(os.path.dirname(wl), "marks", os.path.basename(path) + ".llvm")
    ol, el, errl, cl = RUN.run_binary(wl, path, case)
    if cl == 3 and errl and "LLVM back end can't compile" in (errl.get("message") or ""):
        return case["area"], case["id"], "unsupported", errl["message"].split("yet: ", 1)[-1], None
    if not os.path.exists(mark):
        return case["area"], case["id"], "not-run", "", None
    oi, ei, erri, ci = RUN.run_binary(wi, path, case)
    # lines that read the clock (the case's drop_prefixes, e.g. TIME_INNER) differ from run to run
    drop = tuple(case.get("drop_prefixes") or ())
    if drop:
        ol = "".join(ln for ln in ol.splitlines(True) if not ln.startswith(drop))
        oi = "".join(ln for ln in oi.splitlines(True) if not ln.startswith(drop))
    same = (ol, el, errl, cl) == (oi, ei, erri, ci)
    if same:
        return case["area"], case["id"], "agree", "", None
    okl, whyl = RUN.judge(case, ol, el, errl, cl)
    oki, whyi = RUN.judge(case, oi, ei, erri, ci)
    diff = []
    if ol != oi:
        a, b = ol.splitlines(), oi.splitlines()
        k = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
        diff.append(f"stdout line {k + 1}: llvm {a[k] if k < len(a) else '<none>'!r} vs interp "
                    f"{b[k] if k < len(b) else '<none>'!r}")
    if errl != erri:
        diff.append(f"error: llvm {errl} vs interp {erri}")
    if el != ei:
        diff.append(f"stderr: llvm {el[:120]!r} vs interp {ei[:120]!r}")
    if cl != ci:
        diff.append(f"exit: llvm {cl} vs interp {ci}")
    verdict = "llvm-better" if okl and not oki else ("llvm-worse" if oki and not okl else "both-wrong")
    return case["area"], case["id"], "disagree", "; ".join(diff), verdict


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "fast", "fermium"))
    ap.add_argument("-j", type=int, default=2)
    ap.add_argument("--area")
    ap.add_argument("--out")
    ap.add_argument("-v", action="store_true", help="list the unsupported reasons")
    a = ap.parse_args()
    files = sorted(glob.glob(os.path.join(CASES, a.area or "*", "*.fm")))
    with tempfile.TemporaryDirectory() as tmp:
        wl, wi = wrapper(a.bin, "llvm", tmp), wrapper(a.bin, "interp", tmp)
        with ProcessPoolExecutor(max_workers=a.j) as ex:
            res = list(ex.map(one, [(wl, wi, f) for f in files], chunksize=4))
    ran = [r for r in res if r[2] != "not-run"]
    sup = [r for r in ran if r[2] != "unsupported"]
    agree = [r for r in sup if r[2] == "agree"]
    dis = [r for r in sup if r[2] == "disagree"]
    lines = ["# LLVM back end: differential test against the tree-walker", "",
             f"- programs: {len(res)}; reach a back end (parse and check pass): {len(ran)}",
             f"- compiled by the LLVM back end: {len(sup)} of {len(ran)} "
             f"({100.0 * len(sup) / max(len(ran), 1):.1f} %)",
             f"- identical output (stdout, stderr, exit code): {len(agree)} of {len(sup)}",
             f"- different: {len(dis)} (llvm right and interp wrong: {sum(r[4] == 'llvm-better' for r in dis)}; "
             f"llvm wrong: {sum(r[4] == 'llvm-worse' for r in dis)}; both wrong: "
             f"{sum(r[4] == 'both-wrong' for r in dis)})", ""]
    areas = {}
    for r in res:
        t = areas.setdefault(r[0], [0, 0, 0, 0])
        t[0] += 1
        t[1] += r[2] != "not-run"
        t[2] += r[2] in ("agree", "disagree")
        t[3] += r[2] == "agree"
    lines += ["| Area | Programs | Reach a back end | Compiled by LLVM | Agree |", "|---|---|---|---|---|"]
    lines += [f"| {k} | {v[0]} | {v[1]} | {v[2]} | {v[3]} |" for k, v in sorted(areas.items())]
    if dis:
        lines += ["", "## Differences", ""]
        lines += [f"- `{r[0]}/{r[1]}` [{r[4]}]: {r[3]}" for r in dis]
    reasons = {}
    for r in res:
        if r[2] == "unsupported":
            reasons[r[3]] = reasons.get(r[3], 0) + 1
    lines += ["", "## Why the others aren't compiled", ""]
    lines += [f"- {n}× {why}" for why, n in sorted(reasons.items(), key=lambda x: -x[1])[:40 if a.v else 15]]
    text = "\n".join(lines) + "\n"
    if a.out:
        open(a.out, "w", encoding="utf-8").write(text)
    print(text)
    return 1 if any(r[4] != "llvm-better" for r in dis) else 0


if __name__ == "__main__":
    sys.exit(main())
