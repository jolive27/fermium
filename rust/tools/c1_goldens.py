#!/usr/bin/env python3
"""Write the .json of the spec C1 programs (rust/c-cases/c1/*.fm) from what the tree-walker prints now, one key
per line as crates/fermium-cli/tests/c1_cases.rs reads them. Check the output by hand before committing: these
are goldens. A .json's extra keys (like "llvm": "auto") are kept.

  python3 rust/tools/c1_goldens.py [--bin rust/target/fast/fermium] [names ...]
"""
import argparse
import glob
import json
import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "target", "fast", "fermium"))
    ap.add_argument("names", nargs="*")
    a = ap.parse_args()
    for fm in sorted(glob.glob(os.path.join(ROOT, "c-cases", "c1", "*.fm"))):
        name = os.path.basename(fm)
        if a.names and name not in a.names and name[:-3] not in a.names:
            continue
        env = dict(os.environ, FERMIUM_BACKEND="interp")
        env.pop("FERMIUM_GC_STATS", None)
        p = subprocess.run([os.path.abspath(a.bin), "run", name], cwd=os.path.dirname(fm), env=env,
                           capture_output=True, text=True)
        path = fm[:-3] + ".json"
        extra = {}
        if os.path.exists(path):
            old = json.load(open(path, encoding="utf-8"))
            extra = {k: v for k, v in old.items() if k not in ("stdout", "stderr", "exit")}
        data = {"stdout": p.stdout, "stderr": p.stderr, "exit": p.returncode, **extra}
        lines = [f" {json.dumps(k)}: {json.dumps(v, ensure_ascii=False)}" for k, v in data.items()]
        with open(path, "w", encoding="utf-8") as f:
            f.write("{\n" + ",\n".join(lines) + "\n}\n")
        print(name, p.returncode)


if __name__ == "__main__":
    main()
