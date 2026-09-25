"""Glue between the browser playground and Fermium.  Runs inside Pyodide (web/worker.js loads it),
and also under plain CPython, which is how tests/test_playground.py checks it without a browser.

Programs run through the reference interpreter (fermium.interp), because llvmlite doesn't exist in
Pyodide.  Plots are saved as PNG files by the runtime as usual; we read back every PNG written
during the run and hand it to the page as base64."""
import base64
import io
import json
import os
import re
import time


# optional packages fermium imports lazily; Pyodide has them all
OPTIONAL = {"matplotlib", "scipy", "sympy", "mpmath"}


def packages_needed(code):
    """Pyodide packages a program probably needs beyond numpy (fermium imports these lazily, so
    Pyodide's import scanner can't see them).  A guess to save a rerun: when the guess misses,
    run() reports the missing package and the worker loads it and runs the program again."""
    pkgs = []
    if re.search(r"\bplot\b", code):
        pkgs.append("matplotlib")
    if re.search(r"\bfit\b|\busing\s+(radau|bdf)\b", code):
        pkgs.append("scipy")
    return pkgs


def _pngs(root):
    found = {}
    for d, _, files in os.walk(root):
        for f in files:
            if f.lower().endswith(".png"):
                p = os.path.join(d, f)
                try:
                    found[p] = os.stat(p).st_mtime_ns
                except OSError:
                    pass
    return found


def run(code, base_dir="."):
    """Run a Fermium program; return a JSON string with stdout, warnings, error and plots."""
    from fermium.errors import Diagnostics, FermiumError
    from fermium.interp import run_interpreted

    base_dir = os.path.abspath(base_dir)
    os.makedirs(base_dir, exist_ok=True)
    before = _pngs(base_dir)
    out = io.StringIO()
    diags = Diagnostics()
    error = None
    t0 = time.time()
    try:
        run_interpreted(code, "<playground>", out=out, base_dir=base_dir, diags=diags)
    except FermiumError as e:
        error = e.format(code, None)
    except ModuleNotFoundError as e:
        if (e.name or "").split(".")[0] in OPTIONAL:
            return json.dumps({"missing": e.name.split(".")[0]})
        error = f"internal error in Fermium: {type(e).__name__}: {e}"
    except RecursionError:
        error = ("this program recurses or nests too deeply for the browser playground "
                 "(its stack is much smaller than a desktop's)\n  hint: run it with  fermium run  on your computer")
    except Exception as e:     # a bug in Fermium, not in the program
        error = (f"internal error in Fermium: {type(e).__name__}: {e}\n"
                 f"  (this is a bug in Fermium, not in your program)")
    elapsed = time.time() - t0
    plots = []
    for p, mtime in sorted(_pngs(base_dir).items()):
        if before.get(p) != mtime:
            with open(p, "rb") as fh:
                plots.append({"name": os.path.relpath(p, base_dir),
                              "png": base64.b64encode(fh.read()).decode("ascii")})
    return json.dumps({
        "stdout": out.getvalue(),
        "warnings": [w.format(code, None) for w in diags.warnings],
        "error": error,
        "plots": plots,
        "seconds": round(elapsed, 3),
    })
