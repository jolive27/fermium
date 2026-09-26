"""Build the conformance suite (spec §B3) from the legacy (v1.5) implementation, the oracle.

Sources:
  1. every program the test suite runs in-process (tests/conftest.py records them when FERMIUM_HARVEST is set);
  2. every .fm program in the repository: examples, rosetta, gauntlet, research, tests/programs (with John's
     Appendix 1 programs, mandatory), benchmarks.

Each case is conformance/cases/<area>/<id>.fm with <id>.json: the expected stdout, stderr (warnings), the error
(message, line, column, hint) or none, the exit code, the directory it runs in (relative to the repo), and where it
came from.  Programs whose output depends on the machine (timings, the clock, temporary directories) are left out
and counted in conformance/MANIFEST.md.

Usage:
  FERMIUM_HARVEST=/tmp/h python3 -m pytest -q -n 4 --dist loadfile     # record the test programs
  python3 conformance/harvest.py /tmp/h                                # write the cases
"""
from __future__ import annotations

import glob
import hashlib
import io
import json
import os
import re
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
HERE = os.path.join(ROOT, "conformance")
CASES = os.path.join(HERE, "cases")

AREAS = [   # first match wins; the order puts the most specific features first
    ("python-interop", r"\buse python\b|\bpyimport\b"),
    ("modules", r"(?m)^\s*(import|from)\s+\w"),
    ("uncertainty", r"±|\+-|propagate\s+montecarlo|\buncertainty\("),
    ("pde", r"solve\s+∂|∂\w+/∂t|partial\s*\w+\s*/\s*partial\s*t"),
    ("eigen", r"\blowest\s+\d"),
    ("data", r"\bload\s+\"|\bfit\s+\w|\bplot\b|\btable\(|\banimate\b"),
    ("ode", r"\bsolve\b[^\n]*'|\bsolve\b[\s\S]*\bwith\b"),
    ("algebraic-solve", r"\bsolve\b"),
    ("fft", r"\bi?fft(_re|_im)?\("),
    ("integrals", r"∫|\bintegral\b"),
    ("derivatives", r"\bd/d|d²/d|∂|∇|\bgrad\(|\bdiv\(|\bcurl\(|\w'+\s*(\(|$|\s)"),
    ("complex", r"\d(\.\d+)?i\b|𝑖|\bcomplex\(|\bconj\(|\bcis\("),
    ("natural-units", r"\bunits\s+(natural|nuclear|astro|SI)"),
    ("analyze", r"\banalyze\b"),
    ("parallel", r"\bparallel\s+for\b"),
    ("rng", r"\brand(n)?2?\(|\bseed\(|\bsample\("),
    ("vectors-matrices", r"<[^<>\n]*,[^<>\n]*>|\[\s*\[|\bvec\(|\bdet\(|\binverse\("),
    ("functions", r"(?m)^\s*\w+\([\w\s,\[\]/²³]*\)\s*=|\bwhere\b|\breturn\b"),
    ("control-flow", r"\bif\b|\bfor\b|\bwhile\b"),
    ("lists", r"\[[^\]]*,"),
]


def area_of(src: str, err) -> str:
    for name, pat in AREAS:
        if re.search(pat, src):
            return name
    return "units-and-printing"


NONDETERMINISTIC = re.compile(r"\bclock\(\)|\bTIME\b|\btime:\s", re.M)
FILE_REFS = re.compile(r"\bload\b|\bplot\b|\banimate\b|\bimport\b|\buse python\b|\bsave\b|\"[^\"]*\.(csv|txt|png|svg|gif|fm)\"")


def normalise(text: str, base: str | None) -> str:
    text = text.replace(ROOT + os.sep, "<ROOT>/").replace(ROOT, "<ROOT>")
    return text


def legacy_run(src: str, base_dir: str | None, filename="<program>"):
    """Run a program with the v1.5 implementation, in-process; (stdout, stderr, error-or-None)."""
    from fermium.driver import run_source
    from fermium.errors import FermiumError
    out, err = io.StringIO(), io.StringIO()
    try:
        run_source(src, filename, out=out, base_dir=base_dir, err=err)
        return out.getvalue(), err.getvalue(), None
    except FermiumError as e:
        return out.getvalue(), err.getvalue(), {"message": e.message, "line": e.line, "col": e.col, "hint": e.hint,
                                                "kind": type(e).__name__}


def _clean_one(r):
    base = r["base_dir"] if r["base_dir"] and os.path.isdir(r["base_dir"]) else None
    try:
        out, err, e = legacy_run(r["src"], base)
    except Exception:
        return None
    return dict(r, stdout=out, stderr=err, error=e)


def rerun_clean(records):
    from concurrent.futures import ProcessPoolExecutor
    uniq = {}
    for r in records:
        uniq.setdefault((r["src"], r["base_dir"]), r)
    with ProcessPoolExecutor(max_workers=os.cpu_count() or 2) as ex:
        return [r for r in ex.map(_clean_one, list(uniq.values()), chunksize=8) if r is not None]


def main(harvest_dir: str | None):
    records, skipped = [], {"machine-dependent output": 0, "temporary directory with files": 0, "duplicate": 0,
                             "crashed in legacy": 0}
    if harvest_dir:
        for f in sorted(glob.glob(os.path.join(harvest_dir, "*.jsonl"))):
            for line in open(f, encoding="utf-8"):
                r = json.loads(line)
                r["origin"] = "tests"
                records.append(r)
    # the golden output of a recorded program is what the oracle prints on its own, in a clean state: a test may
    # have changed a setting first (monkeypatch), so the recorded output is only a way to find the programs
    records = rerun_clean(records)
    programs = []
    for pat in ("examples/*.fm", "examples/rosetta/*.fm", "gauntlet/*/*.fm", "research/*/*.fm", "tests/programs/*.fm",
                "tests/programs/john/*.fm", "benchmarks/fermium/*.fm"):
        programs += sorted(glob.glob(os.path.join(ROOT, pat)))
    for path in programs:
        src = open(path, encoding="utf-8").read()
        d = os.path.dirname(path)
        try:
            out, err, e = legacy_run(src, d, os.path.basename(path))
        except Exception:
            skipped["crashed in legacy"] += 1
            continue
        records.append({"src": src, "base_dir": d, "stdout": out, "stderr": err, "error": e,
                        "origin": os.path.relpath(path, ROOT)})
    if os.path.isdir(CASES):
        shutil.rmtree(CASES)
    seen, counts, john = set(), {}, 0
    for r in records:
        src, base = r["src"], r["base_dir"]
        inside = base is None or os.path.abspath(base).startswith(ROOT)
        if not inside:
            if FILE_REFS.search(src):
                skipped["temporary directory with files"] += 1
                continue
            base = None
        if NONDETERMINISTIC.search(src) or NONDETERMINISTIC.search(r["stdout"]) and "benchmarks" in r["origin"]:
            if "benchmarks" not in r["origin"]:
                skipped["machine-dependent output"] += 1
                continue
        rel = os.path.relpath(base, ROOT) if base else None
        key = hashlib.sha1((src + "\0" + (rel or "")).encode()).hexdigest()[:12]
        if key in seen:
            skipped["duplicate"] += 1
            continue
        seen.add(key)
        area = "appendix1" if "tests/programs/john" in r["origin"] else area_of(src, r["error"])
        john += area == "appendix1"
        stdout = normalise(r["stdout"], base)
        if "benchmarks" in r["origin"]:        # timings vary: keep the physics, drop the TIME lines
            stdout = "".join(ln for ln in stdout.splitlines(True) if not ln.startswith("TIME"))
        e = r["error"]
        case = {"id": key, "area": area, "origin": r["origin"], "dir": rel, "stdout": stdout,
                "stderr": normalise(r["stderr"], base), "error": e, "exit": 1 if e else 0,
                "drop_prefixes": ["TIME"] if "benchmarks" in r["origin"] else []}
        os.makedirs(os.path.join(CASES, area), exist_ok=True)
        with open(os.path.join(CASES, area, key + ".fm"), "w", encoding="utf-8") as fh:
            fh.write(src)
        with open(os.path.join(CASES, area, key + ".json"), "w", encoding="utf-8") as fh:
            json.dump(case, fh, ensure_ascii=False, indent=1)
        counts[area] = counts.get(area, 0) + 1
    lines = ["# Conformance suite manifest", "",
             "Generated by `conformance/harvest.py` from Fermium 1.5 (the oracle). Each case is a program with the output,",
             "warnings and error the oracle gives. `conformance/run --impl legacy|rust` scores an implementation.", "",
             "| Area | Cases |", "|---|---|"]
    lines += [f"| {a} | {n} |" for a, n in sorted(counts.items())]
    lines += [f"| **total** | **{sum(counts.values())}** |", "", "Left out:", ""]
    lines += [f"- {k}: {v}" for k, v in skipped.items()]
    lines += ["", f"Appendix 1 (John's programs, mandatory): {john} cases."]
    open(os.path.join(HERE, "MANIFEST.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else None)
