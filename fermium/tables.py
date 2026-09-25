"""Post-check resolution of the side tables (formats, plots, fits).  Pure Python, no llvmlite,
so the reference interpreter (and the Pyodide playground) can use it."""
from __future__ import annotations


def finalize_tables(tables, U, start=0):
    """Resolve dimension expressions now that all constraints are known."""
    for f in tables.fmts:
        f["rdim"] = U.resolve(f["dim"])
        if f.get("nat") is not None:
            f["hint"] = _natural_hint(f["nat"], f["rdim"], f["hint"])
    for p in tables.plots:
        for s in p["series"]:
            s["rydim"] = U.resolve(s["ydim"])
            s["rxdim"] = U.resolve(s["xdim"])
            if p.get("nat") is not None:
                s["yhint"] = _natural_hint(p["nat"], s["rydim"], s.get("yhint"))
                s["xhint"] = _natural_hint(p["nat"], s["rxdim"], s.get("xhint"))
    for f in tables.fits:
        f["rdims"] = [U.resolve(d) for d in f["dims"]]
        f["rydim"] = U.resolve(f["ydim"])
        f["col_units"] = {c["unit"].dim: c["unit"] for c in f.get("columns", []) if c["unit"].name not in ("1",)}


def _natural_hint(system, dim, hint):
    """Inside `units natural` / `nuclear` / `astro`: a value with no unit of its own (or one that no longer fits)
    is shown in the system's units: MeV powers, fm, M☉/AU/yr (D60)."""
    if hint is not None and getattr(hint, "dim", None) == dim:
        return hint
    return system.display_unit(dim) or hint
