#!/usr/bin/env python3
"""Fermium benchmark runner (spec section 4, Tier 3).

Runs every benchmark in every available language R times, records

  * the median wall-clock time of the whole process ("with startup/compile/JIT"),
  * the median TIME_INNER the program reports ("compute only"; for Julia this is
    measured after a warm-up call, so it excludes JIT compilation),

checks that all languages print the same results, and writes
benchmarks/RESULTS.md (plus the raw data in benchmarks/results.json).

Program output contract (every language, including Fermium):
  * result lines of the form ``<key> <number>`` (a bare ``<number>`` line is
    stored under the key ``value``);
  * a final ``TIME_INNER <seconds>`` line (not needed for ``startup``);
    Julia's unit_loop also prints ``TIME_INNER_UNITFUL <seconds>``;
  * nbody prints ``N <steps>`` so times can be normalized per step.

Usage:
  python benchmarks/run.py                       # everything, R=5 (R=3 for slow Python)
  python benchmarks/run.py -r 3 --langs julia,python,numpy
  python benchmarks/run.py --benchmarks nbody,startup --langs fermium,julia
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent
ROOT = BENCH_DIR.parent
JULIA = ROOT / ".tools" / "julia" / "bin" / "julia"
JULIA_DEPOT = ROOT / ".tools" / "julia-depot"

BENCHMARKS = ["nbody", "spring_rk4", "spring_adaptive", "blackbody", "unit_loop", "startup"]
LANGS = ["fermium", "julia", "python", "numpy"]
REFERENCE_LANG = "julia"   # results and speed ratios are compared against this

# nbody steps used by the runner for each language (normalized per step anyway).
NBODY_N = {"julia": 1_000_000, "python": 1_000_000, "numpy": 1_000_000}
NBODY_N_QUICK = {"python": 100_000, "numpy": 100_000}

# Relative tolerance for "all languages agree" per result key.
TOLERANCE = {
    "energy_before": 1e-9,
    "energy_after": 1e-8,
    "x_10s": 1e-9,
    "x_100s": 1e-6,          # |x(100 s)| ~ 1e-10 m is at the atol=1e-10 noise floor
    "accepted_steps": 0.0,   # exact
    "sum_integrals": 1e-8,
    "ratio_5778K": 1e-8,
    "E_J": 1e-12,
    "E_unitful_J": 1e-12,
    "value": 1e-14,
    "steps": 0.0,
}

LANG_LABEL = {"fermium": "Fermium", "julia": "Julia", "python": "Python (pure)",
              "numpy": "NumPy/SciPy"}


# --------------------------------------------------------------------------- commands

def julia_env():
    env = dict(os.environ)
    env["JULIA_DEPOT_PATH"] = str(JULIA_DEPOT)
    return env


def fermium_cmd():
    """Return the command prefix for Fermium, or None if unavailable."""
    override = os.environ.get("FERMIUM_CMD")
    if override:
        return override.split()
    exe = shutil.which("fermium")
    if exe:
        return [exe]
    return None


def command_for(lang: str, bench: str, quick: bool):
    """Return (argv, env, why_skipped). argv is None when the language is unavailable."""
    if lang == "julia":
        src = BENCH_DIR / "julia" / f"{bench}.jl"
        if not JULIA.exists():
            return None, None, f"no Julia at {JULIA}"
        if not src.exists():
            return None, None, f"missing {src.relative_to(ROOT)}"
        args = [str(NBODY_N["julia"])] if bench == "nbody" else []
        return ([str(JULIA), "--startup-file=no", f"--project={BENCH_DIR / 'julia'}", str(src), *args],
                julia_env(), None)
    if lang in ("python", "numpy"):
        src = BENCH_DIR / lang / f"{bench}.py"
        if not src.exists():
            return None, None, f"missing {src.relative_to(ROOT)}"
        args = []
        if bench == "nbody":
            n = NBODY_N_QUICK[lang] if quick else NBODY_N[lang]
            args = [str(n)]
        return [sys.executable, str(src), *args], dict(os.environ), None
    if lang == "fermium":
        src = BENCH_DIR / "fermium" / f"{bench}.fm"
        if not src.exists():
            return None, None, f"missing {src.relative_to(ROOT)}"
        prefix = fermium_cmd()
        if prefix is None:
            return None, None, "`fermium` not on PATH (set FERMIUM_CMD to override)"
        return [*prefix, "run", str(src.relative_to(ROOT))], dict(os.environ), None
    raise ValueError(lang)


# --------------------------------------------------------------------------- running

NUM_RE = re.compile(r"^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$")


def parse_output(text: str, strings=None):
    """Parse result/timing lines; if `strings` is a dict, also keep the printed text."""
    results, timings = {}, {}
    strings = {} if strings is None else strings
    sup = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
    for line in text.strip().splitlines():
        # Fermium prints 6.674×10⁻¹¹; normalize to 6.674e-11
        line = re.sub(r"×10([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)", lambda m: "e" + m.group(1).translate(sup), line)
        parts = line.split()
        if not parts:
            continue
        if len(parts) == 1 and NUM_RE.match(parts[0]):
            results["value"] = float(parts[0])
            strings["value"] = parts[0]
        elif len(parts) == 2 and NUM_RE.match(parts[1]):
            key, val = parts[0], float(parts[1])
            if key.startswith("TIME_INNER"):
                timings[key] = val
            else:
                results[key] = val
                strings[key] = parts[1]
        # anything else is ignored (but kept in the raw output)
    return results, timings


def run_once(argv, env, timeout):
    t0 = time.perf_counter()
    proc = subprocess.run(argv, env=env, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    wall = time.perf_counter() - t0
    return proc, wall


def bench_one(lang, bench, repeats, warmup, timeout, quick):
    argv, env, why = command_for(lang, bench, quick)
    rec = {"lang": lang, "bench": bench, "status": "skipped", "reason": why}
    if argv is None:
        return rec
    rec["cmd"] = " ".join(argv)
    walls, inners = [], {}
    results, raw, results_printed = None, "", {}
    try:
        for k in range(warmup + repeats):
            proc, wall = run_once(argv, env, timeout)
            if proc.returncode != 0:
                rec.update(status="failed",
                           reason=f"exit {proc.returncode}: {(proc.stderr or proc.stdout).strip()[-400:]}")
                return rec
            printed = {}
            res, tim = parse_output(proc.stdout, printed)
            if bench != "startup" and "TIME_INNER" not in tim:
                rec.update(status="failed", reason="no TIME_INNER line in output")
                return rec
            if k < warmup:
                continue
            walls.append(wall)
            for key, val in tim.items():
                inners.setdefault(key, []).append(val)
            if results is None:
                results, raw, results_printed = res, proc.stdout, printed
    except subprocess.TimeoutExpired:
        rec.update(status="failed", reason=f"timeout after {timeout} s")
        return rec
    except OSError as exc:
        rec.update(status="failed", reason=str(exc))
        return rec
    rec.update(status="ok", reason=None, runs=len(walls), walls=walls,
               wall_median=statistics.median(walls),
               inner={k: statistics.median(v) for k, v in inners.items()},
               inner_all=inners, results=results, results_printed=results_printed,
               raw_output=raw)
    return rec


# --------------------------------------------------------------------------- machine info

def cpu_model():
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def tool_version(argv, env=None):
    try:
        out = subprocess.run(argv, capture_output=True, text=True, timeout=60, env=env)
        return (out.stdout or out.stderr).strip().splitlines()[0]
    except Exception:  # noqa: BLE001 - informational only
        return "unavailable"


def machine_info():
    info = {
        "date": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
        "cpu": cpu_model(),
        "logical_cores": os.cpu_count(),
        "os": f"{platform.system()} {platform.release()}",
        "python": platform.python_version(),
        "julia": tool_version([str(JULIA), "--version"]) if JULIA.exists() else "not installed",
    }
    try:
        import numpy
        info["numpy"] = numpy.__version__
    except ImportError:
        info["numpy"] = "not installed"
    try:
        import scipy
        info["scipy"] = scipy.__version__
    except ImportError:
        info["scipy"] = "not installed"
    fc = fermium_cmd()
    info["fermium"] = tool_version([*fc, "--version"]) if fc else "not on PATH"
    return info


# --------------------------------------------------------------------------- analysis

def rows_for(records):
    """Expand records into table rows (Julia unit_loop gets an extra Unitful row)."""
    rows = []
    for rec in records:
        if rec["status"] != "ok":
            rows.append(dict(bench=rec["bench"], lang=rec["lang"], variant="", rec=rec, inner=None))
            continue
        inner = rec["inner"].get("TIME_INNER")
        rows.append(dict(bench=rec["bench"], lang=rec["lang"], variant="", rec=rec, inner=inner))
        if "TIME_INNER_UNITFUL" in rec["inner"]:
            rows.append(dict(bench=rec["bench"], lang=rec["lang"], variant="Unitful.jl", rec=rec,
                             inner=rec["inner"]["TIME_INNER_UNITFUL"]))
    return rows


def nbody_n(rec):
    return int(rec["results"].get("N", 0)) if rec.get("results") else 0


def check_agreement(records):
    """Compare result values of each language with the reference language."""
    checks = {}
    by_bench = {}
    for rec in records:
        if rec["status"] == "ok":
            by_bench.setdefault(rec["bench"], {})[rec["lang"]] = rec
    for bench, recs in by_bench.items():
        ref_lang = REFERENCE_LANG if REFERENCE_LANG in recs else next(iter(recs))
        ref = recs[ref_lang]["results"]
        for lang, rec in recs.items():
            if lang == ref_lang:
                continue
            res = rec["results"]
            notes, ok = [], True
            for key, val in res.items():
                if key == "N":
                    continue
                if bench == "nbody" and key == "energy_after" and nbody_n(rec) != nbody_n(recs[ref_lang]):
                    notes.append(f"energy_after not compared (N={nbody_n(rec)} vs {nbody_n(recs[ref_lang])})")
                    continue
                refkey = key if key in ref else ("E_J" if key == "E_unitful_J" and "E_J" in ref else None)
                if refkey is None:
                    continue
                rv = ref[refkey]
                tol = TOLERANCE.get(key, 1e-8)
                rel = abs(val - rv) / max(abs(rv), 1e-300)
                if rel > tol:
                    ok = False
                    notes.append(f"{key}: {val:.12g} vs {ref_lang} {rv:.12g} (rel diff {rel:.1e} > {tol:g})")
            missing = [k for k in ref if k not in res and k not in ("N", "E_unitful_J")]
            if missing:
                ok = False
                notes.append("missing " + ", ".join(missing))
            checks[(bench, lang)] = (ok, ref_lang, notes)
        checks[(bench, ref_lang)] = (True, ref_lang, ["reference"])
    return checks


def fmt_t(sec):
    if sec is None:
        return "—"
    if sec >= 1:
        return f"{sec:.3f} s"
    if sec >= 1e-3:
        return f"{sec * 1e3:.2f} ms"
    return f"{sec * 1e6:.1f} µs"


def fmt_ratio(x):
    if x is None:
        return "—"
    return f"{x:.2f}×" if x < 100 else f"{x:.0f}×"


# --------------------------------------------------------------------------- report

def write_report(records, info, args):
    rows = rows_for(records)
    checks = check_agreement(records)

    # Reference (Julia, plain) times per benchmark, nbody inner normalized per step.
    ref_inner, ref_wall = {}, {}
    for r in rows:
        if r["lang"] == REFERENCE_LANG and not r["variant"] and r["rec"]["status"] == "ok":
            inner = r["inner"]
            if r["bench"] == "nbody" and inner is not None:
                inner = inner / nbody_n(r["rec"])
            ref_inner[r["bench"]] = inner
            ref_wall[r["bench"]] = (r["rec"]["wall_median"], nbody_n(r["rec"]))

    out = []
    out.append("# Fermium benchmark results\n")
    out.append("Generated by `python benchmarks/run.py` — do not edit by hand.\n")
    out.append("## Machine\n")
    out.append("| | |\n|---|---|")
    out.append(f"| Date | {info['date']} |")
    out.append(f"| CPU | {info['cpu']} |")
    out.append(f"| Logical cores | {info['logical_cores']} (all benchmarks single-threaded) |")
    out.append(f"| OS | {info['os']} |")
    out.append(f"| Julia | {info['julia']} |")
    out.append(f"| Python | {info['python']} |")
    out.append(f"| NumPy / SciPy | {info['numpy']} / {info['scipy']} |")
    out.append(f"| Fermium | {info['fermium']} |")
    out.append(f"| Repeats | median of {args.repeats} runs ({args.slow_repeats} for pure Python / NumPy), "
               f"after {args.warmup} discarded warm-up run(s) |")
    out.append("")
    out.append("## Results\n")
    out.append("* **Wall** = whole process, median: includes interpreter/runtime startup, package "
               "loading and JIT/compilation.")
    out.append("* **Inner** = the program's own timer around the computation only, median. "
               "Julia runs the kernel once on a tiny problem first, so Inner excludes JIT time.")
    out.append("* **×Julia** = time / Julia time (lower is better; < 1 means faster than Julia).")
    out.append("* nbody: Inner is reported **per step** (and scaled to 1M steps) because pure Python / "
               "NumPy may run fewer steps.")
    out.append("")
    out.append("| Benchmark | Language | Wall (with startup/JIT) | ×Julia wall | Inner (compute only) "
               "| ×Julia inner | Results agree | Notes |")
    out.append("|---|---|---:|---:|---:|---:|:---:|---|")
    for bench in BENCHMARKS:
        for r in [r for r in rows if r["bench"] == bench]:
            rec = r["rec"]
            name = LANG_LABEL[r["lang"]] + (f" + {r['variant']}" if r["variant"] else "")
            if rec["status"] != "ok":
                out.append(f"| {bench} | {name} | — | — | — | — | — | {rec['status']}: "
                           f"{(rec['reason'] or '').replace('|', '/').splitlines()[0][:160]} |")
                continue
            notes = []
            wall = rec["wall_median"]
            inner = r["inner"]
            wall_ratio = inner_ratio = None
            inner_txt = fmt_t(inner)
            if bench == "nbody":
                n = nbody_n(rec)
                notes.append(f"N = {n:,}")
                if inner is not None:
                    per = inner / n
                    inner_txt = f"{per * 1e9:.1f} ns/step ({fmt_t(per * 1e6)} per 1M)"
                    if ref_inner.get(bench):
                        inner_ratio = per / ref_inner[bench]
                rw = ref_wall.get(bench)
                if rw and rw[1] == n:
                    wall_ratio = wall / rw[0]
            else:
                if ref_wall.get(bench):
                    wall_ratio = wall / ref_wall[bench][0]
                if inner is not None and ref_inner.get(bench):
                    inner_ratio = inner / ref_inner[bench]
            if bench == "startup":
                inner_txt = "n/a"
            if bench == "spring_adaptive" and r["lang"] == "fermium":
                notes.append("tolerance 1e-10 chosen so the final error (~1.8e-10 m) matches Julia's "
                             "rtol=1e-8/atol=1e-10 run (~2e-10 m; exact 1.1e-12 m); different error norms, so "
                             "x_100s and step counts differ at the noise floor")
            if bench == "spring_rk4" and r["lang"] == "fermium":
                notes.append("also stores the whole trajectory (dense solution usable after solve); others keep only "
                             "the current state")
            if bench == "unit_loop" and r["lang"] == "julia" and not r["variant"]:
                notes.append("wall includes loading Unitful.jl (both variants run in one process)")
            if r["variant"]:
                wall_txt, wall_ratio = "(same process)", None
            else:
                wall_txt = fmt_t(wall)
            ok, ref_lang, cnotes = checks.get((bench, r["lang"]), (None, None, []))
            agree = "ref" if cnotes == ["reference"] else ("✓" if ok else "✗")
            notes += [c for c in cnotes if c != "reference"]
            out.append(f"| {bench} | {name} | {wall_txt} | {fmt_ratio(wall_ratio)} | {inner_txt} "
                       f"| {fmt_ratio(inner_ratio)} | {agree} | {'; '.join(notes).replace('|', '/')} |")
    out.append("")
    out.append("## Printed results\n")
    out.append("First run of each program (TIME lines omitted).\n")
    for bench in BENCHMARKS:
        recs = [r for r in records if r["bench"] == bench and r["status"] == "ok"]
        if not recs:
            continue
        out.append(f"### {bench}\n")
        keys = []
        for rec in recs:
            keys += [k for k in rec["results"] if k not in keys]
        out.append("| Language | " + " | ".join(keys) + " |")
        out.append("|---|" + "---:|" * len(keys))
        for rec in recs:
            cells = []
            for k in keys:
                cells.append(rec["results_printed"].get(k, "—"))
            out.append(f"| {LANG_LABEL[rec['lang']]} | " + " | ".join(cells) + " |")
        out.append("")
    out.append("## Commands\n")
    for rec in records:
        if rec.get("cmd"):
            cmd = rec["cmd"].replace(str(ROOT) + "/", "")
            out.append(f"* `{cmd}`")
    out.append("")
    (BENCH_DIR / "RESULTS.md").write_text("\n".join(out) + "\n")

    slim = [{k: v for k, v in rec.items() if k != "raw_output"} for rec in records]
    (BENCH_DIR / "results.json").write_text(json.dumps({"machine": info, "records": slim}, indent=2) + "\n")


# --------------------------------------------------------------------------- main

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-r", "--repeats", type=int, default=5, help="timed runs per program (default 5)")
    ap.add_argument("--slow-repeats", type=int, default=None,
                    help="timed runs for pure Python and NumPy (default: min(repeats, 3))")
    ap.add_argument("--warmup", type=int, default=1, help="untimed runs before timing (default 1)")
    ap.add_argument("--langs", default=",".join(LANGS), help="comma-separated subset of " + ",".join(LANGS))
    ap.add_argument("--benchmarks", default=",".join(BENCHMARKS), help="comma-separated subset")
    ap.add_argument("--quick", action="store_true",
                    help="pure Python / NumPy nbody run 100k steps instead of 1M (normalized per step)")
    ap.add_argument("--timeout", type=float, default=900, help="seconds per run (default 900)")
    args = ap.parse_args()
    if args.slow_repeats is None:
        args.slow_repeats = min(args.repeats, 3)

    langs = [l for l in LANGS if l in args.langs.split(",")]
    benches = [b for b in BENCHMARKS if b in args.benchmarks.split(",")]
    records = []
    for bench in benches:
        for lang in langs:
            reps = args.slow_repeats if lang in ("python", "numpy") and bench != "startup" else args.repeats
            print(f"[{bench:>15}] {lang:<8} ...", end=" ", flush=True)
            rec = bench_one(lang, bench, reps, args.warmup, args.timeout, args.quick)
            if rec["status"] == "ok":
                inner = rec["inner"].get("TIME_INNER")
                print(f"wall {fmt_t(rec['wall_median'])}, inner {fmt_t(inner)}  {rec['results']}")
            else:
                print(f"{rec['status']}: {(rec['reason'] or '').splitlines()[0][:200]}")
            records.append(rec)

    info = machine_info()
    write_report(records, info, args)
    bad = [(b, l, n) for (b, l), (ok, _, n) in check_agreement(records).items() if not ok]
    for b, l, n in bad:
        print(f"DISAGREEMENT {b}/{l}: {'; '.join(n)}")
    print(f"wrote {BENCH_DIR / 'RESULTS.md'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
