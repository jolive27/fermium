#!/usr/bin/env python3
"""Compare `fermium fmt` of the Rust port with Fermium 1.5's (fermium/cli.py cmd_fmt, the oracle) on every
conformance program and every .fm in the repository, in each mode: --pretty, --ascii, --fix, --fix --pretty and
--fix --ascii, and also the round trip (--ascii of the --pretty output, and --pretty of the --ascii output).

    python3 rust/tools/compare_fmt.py [--bin rust/target/debug/fermium] [--show N] [--only SUBSTR] [--snippets|--fuzz]

Stdout, stderr and the exit code must agree exactly.  Python's outputs are cached in rust/target/fmt-oracle/.
"""
import argparse
import contextlib
import glob
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "legacy"))   # Fermium 1.5 (the oracle) is in legacy/ since v2.0 (D269)
sys.path.insert(0, os.path.join(ROOT, "rust", "tools"))
MODES = [["--pretty"], ["--ascii"], ["--fix"], ["--fix", "--pretty"], ["--fix", "--ascii"]]


def python_fmt(args):
    from fermium.cli import main
    out, err = io.StringIO(), io.StringIO()
    code = 0
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            code = main(["fmt"] + args)
        except SystemExit as e:
            code = e.code if isinstance(e.code, int) else 1
        except Exception as e:                 # an internal error of the oracle
            err.write(f"CRASH {type(e).__name__}: {e}\n")
            code = 99
    return f"EXIT {code}\n--- stdout\n{out.getvalue()}--- stderr\n{err.getvalue()}"


def rust_fmt(binary, args):
    r = subprocess.run([binary, "fmt"] + args, capture_output=True)
    return (f"EXIT {r.returncode}\n--- stdout\n{r.stdout.decode('utf-8', 'replace')}--- stderr\n"
            f"{r.stderr.decode('utf-8', 'replace')}")


def stdout_of(text):
    return text.split("--- stdout\n", 1)[1].rsplit("--- stderr\n", 1)[0]


def main():
    from compare_parse import programs
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "debug", "fermium"))
    ap.add_argument("--show", type=int, default=10)
    ap.add_argument("--only", default="")
    ap.add_argument("--snippets", action="store_true")
    ap.add_argument("--fuzz", action="store_true")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    if a.snippets or a.fuzz:
        files = sorted(glob.glob(os.path.join(ROOT, "rust", "target", "fuzz" if a.fuzz else "snippets", "*.fm")))
    else:
        files = programs()
    files = [f for f in files if a.only in f]
    cache = os.path.join(ROOT, "rust", "target", "fmt-oracle")
    os.makedirs(cache, exist_ok=True)
    tmpdir = tempfile.mkdtemp(prefix="fmt-rt-", dir=os.path.join(ROOT, "rust", "target"))
    counts = {}
    bad = []
    for f in files:
        with open(f, "rb") as fh:
            key = hashlib.sha1(fh.read() + f.encode()).hexdigest()
        cpath = os.path.join(cache, key + ".json")
        if os.path.exists(cpath):
            with open(cpath, encoding="utf-8") as fh:
                want = json.load(fh)
        else:
            want = {}
            for m in MODES:
                want[" ".join(m)] = python_fmt(m + [f])
            # round trips: format the formatted text back (in a file with the same name)
            for first, second in (("--pretty", "--ascii"), ("--ascii", "--pretty")):
                src = stdout_of(want[first])
                rt = os.path.join(tmpdir, os.path.basename(f))
                with open(rt, "w", encoding="utf-8") as fh:
                    fh.write(src)
                want[f"{first}>{second}"] = python_fmt([second, rt])
            with open(cpath, "w", encoding="utf-8") as fh:
                json.dump(want, fh)
        for mode, w in want.items():
            if ">" in mode:
                first, second = mode.split(">")
                got_first = rust_fmt(a.bin, [first, f])
                rt = os.path.join(tmpdir, os.path.basename(f))
                with open(rt, "w", encoding="utf-8") as fh:
                    fh.write(stdout_of(got_first))
                g = rust_fmt(a.bin, [second, rt])
            else:
                g = rust_fmt(a.bin, mode.split() + [f])
            ok, total = counts.get(mode, (0, 0))
            if g == w:
                ok += 1
            else:
                wl, gl = w.split("\n"), g.split("\n")
                k = next((i for i in range(max(len(wl), len(gl))) if (wl[i:i + 1] or [None]) != (gl[i:i + 1] or [None])), 0)
                bad.append((f, mode, k, wl[k] if k < len(wl) else "<end>", gl[k] if k < len(gl) else "<end>"))
            counts[mode] = (ok, total + 1)
    for f, mode, k, w, g in bad[:a.show]:
        print(f"--- {os.path.relpath(f, ROOT)} [{mode}] (line {k + 1})\n  python: {w[:300]}\n  rust:   {g[:300]}")
    if a.list:
        for f, mode, *_ in bad:
            print(mode, os.path.relpath(f, ROOT))
    allok = sum(c[0] for c in counts.values())
    alln = sum(c[1] for c in counts.values())
    for mode, (ok, total) in counts.items():
        print(f"fmt {mode}: {ok} / {total} agree exactly")
    print(f"fmt total: {allok} / {alln} agree exactly")


if __name__ == "__main__":
    main()
