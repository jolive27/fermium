#!/usr/bin/env python3
"""Write the expected output (.json) of every program in this folder, from a fermium binary (argument 1,
default rust/target/fast/fermium) with the tree-walker. Check each new output by hand before committing it:
tests/c5_dispatch.rs runs both back ends against these files."""
import json
import os
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
binary = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "..", "target", "fast", "fermium")
for name in sorted(os.listdir(here)):
    if not name.endswith(".fm"):
        continue
    env = dict(os.environ, FERMIUM_BACKEND="interp")
    r = subprocess.run([binary, "run", name], cwd=here, capture_output=True, text=True, env=env)
    with open(os.path.join(here, name[:-3] + ".json"), "w") as f:
        f.write("{\n")
        f.write(f' "stdout": {json.dumps(r.stdout)},\n')
        f.write(f' "stderr": {json.dumps(r.stderr)},\n')
        f.write(f' "exit": {r.returncode}\n')
        f.write("}\n")
    print(name, r.returncode)
