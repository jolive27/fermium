#!/usr/bin/env python3
"""Compare the Rust parser with the Python one (Fermium 1.5, the oracle) on every conformance program and every
.fm file in the repository.

    python3 rust/tools/compare_parse.py [--tokens|--fix] [--bin rust/target/debug/fermium] [--show N] [--only SUBSTR]

For each program both implementations print the tree, the tokens and the warnings (or the error) in one format
(rust/tools/parse_oracle.py and `fermium parse --oracle`); the report counts the exact agreements and shows the
first differing line of each disagreement.  Oracle outputs are cached in rust/target/parse-oracle/ keyed by the
file's content, so a rerun only runs the Rust side.
"""
import argparse
import glob
import hashlib
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def programs():
    files = sorted(glob.glob(os.path.join(ROOT, "conformance", "cases", "*", "*.fm")))
    seen = set(files)
    for dirpath, dirnames, filenames in os.walk(ROOT):
        rel = os.path.relpath(dirpath, ROOT)
        if rel.startswith((".git", ".claude", "rust" + os.sep + "target", "conformance")) or "node_modules" in rel:
            dirnames[:] = []
            continue
        for fn in sorted(filenames):
            p = os.path.join(dirpath, fn)
            if fn.endswith(".fm") and p not in seen:
                files.append(p)
                seen.add(p)
    return files


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tokens", action="store_true")
    ap.add_argument("--fix", action="store_true")
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "debug", "fermium"))
    ap.add_argument("--show", type=int, default=15)
    ap.add_argument("--only", default="")
    ap.add_argument("--list", action="store_true", help="print every disagreeing file")
    a = ap.parse_args()
    mode = "--tokens" if a.tokens else "--fix" if a.fix else "--oracle"
    files = [f for f in programs() if a.only in f]
    cache = os.path.join(ROOT, "rust", "target", "parse-oracle", mode.strip("-"))
    os.makedirs(cache, exist_ok=True)
    keys, todo = {}, []
    for f in files:
        with open(f, "rb") as fh:
            k = hashlib.sha1(fh.read()).hexdigest()
        keys[f] = k
        if not os.path.exists(os.path.join(cache, k + ".txt")):
            todo.append(f)
    if todo:
        listing = os.path.join(cache, "todo.lst")
        with open(listing, "w", encoding="utf-8") as fh:
            fh.write("\n".join(todo) + "\n")
        tmp = os.path.join(cache, "tmp")
        args = [sys.executable, os.path.join(ROOT, "rust", "tools", "parse_oracle.py")]
        if mode != "--oracle":
            args.append(mode)
        subprocess.run(args + ["--out", tmp, listing], check=True)
        for n, f in enumerate(todo):
            os.replace(os.path.join(tmp, f"{n}.txt"), os.path.join(cache, keys[f] + ".txt"))
    agree, bad = 0, []
    for f in files:
        with open(os.path.join(cache, keys[f] + ".txt"), encoding="utf-8") as fh:
            want = fh.read()
        r = subprocess.run([a.bin, "parse", mode, f], capture_output=True)
        got = r.stdout.decode("utf-8", "replace")
        if r.returncode != 0 and not got:
            got = "RUST FAILED: " + r.stderr.decode("utf-8", "replace")[-2000:]
        if got == want:
            agree += 1
        else:
            wl, gl = want.split("\n"), got.split("\n")
            k = next((i for i in range(max(len(wl), len(gl))) if (wl[i:i + 1] or [None]) != (gl[i:i + 1] or [None])), 0)
            bad.append((f, k, wl[k] if k < len(wl) else "<end>", gl[k] if k < len(gl) else "<end>"))
    for f, k, w, g in bad[:a.show]:
        print(f"--- {os.path.relpath(f, ROOT)} (line {k + 1})\n  python: {w[:300]}\n  rust:   {g[:300]}")
    if a.list:
        for f, *_ in bad:
            print(os.path.relpath(f, ROOT))
    print(f"{mode.strip('-')}: {agree} / {len(files)} agree exactly ({len(bad)} differ)")


if __name__ == "__main__":
    main()
