"""Post-check resolution of the side tables (formats, plots, fits).  Pure Python, no llvmlite,
so the reference interpreter (and the Pyodide playground) can use it."""
from __future__ import annotations


def finalize_tables(tables, U, start=0):
    """Resolve dimension expressions now that all constraints are known."""
    for f in tables.fmts:
        f["rdim"] = U.resolve(f["dim"])
    for p in tables.plots:
        for s in p["series"]:
            s["rydim"] = U.resolve(s["ydim"])
            s["rxdim"] = U.resolve(s["xdim"])
    for f in tables.fits:
        f["rdims"] = [U.resolve(d) for d in f["dims"]]
        f["rydim"] = U.resolve(f["ydim"])
        f["col_units"] = {c["unit"].dim: c["unit"] for c in f.get("columns", []) if c["unit"].name not in ("1",)}
