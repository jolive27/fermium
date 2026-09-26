#!/usr/bin/env python3
"""Record a documented divergence (spec §B2/§B8): the Rust implementation deliberately prints something other
than v1 for this case, as described in a section of rust/DIVERGENCES.md.

  python3 conformance/add_divergence.py --section "Quadrature: narrow peaks …" ID [ID …] [--bin PATH]

Runs the Rust binary on each case and stores what it prints in conformance/divergences/<id>.json; from then on
conformance/run counts the case as a documented divergence only while the Rust output stays exactly that.
Only record a divergence after reading the output and checking that it is the intended one."""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ids", nargs="+")
    ap.add_argument("--section", required=True, help="the heading of the section in rust/DIVERGENCES.md")
    ap.add_argument("--bin", default=os.path.join(ROOT, "rust", "target", "release", "fermium"))
    a = ap.parse_args()
    text = open(os.path.join(ROOT, "rust", "DIVERGENCES.md"), encoding="utf-8").read()
    if f"## {a.section}" not in text:
        sys.exit(f"no section '## {a.section}' in rust/DIVERGENCES.md")
    import importlib.machinery
    import importlib.util
    loader = importlib.machinery.SourceFileLoader("conformance_run", os.path.join(HERE, "run"))
    spec = importlib.util.spec_from_loader("conformance_run", loader)
    run = importlib.util.module_from_spec(spec)
    loader.exec_module(run)
    os.makedirs(os.path.join(HERE, "divergences"), exist_ok=True)
    for cid in a.ids:
        paths = glob.glob(os.path.join(HERE, "cases", "*", cid + ".fm"))
        if not paths:
            sys.exit(f"no case {cid}")
        case = json.load(open(paths[0][:-3] + ".json", encoding="utf-8"))
        out, err, e, code = run.run_binary(a.bin, paths[0], case)
        d = {"id": cid, "section": a.section, "stdout": out, "stderr": err, "error": e, "exit": code}
        with open(os.path.join(HERE, "divergences", cid + ".json"), "w", encoding="utf-8") as fh:
            json.dump(d, fh, ensure_ascii=False, indent=1)
        print(f"{cid}: recorded ({'error: ' + e['message'] if e else out.strip()[:80]!r})")


if __name__ == "__main__":
    main()
