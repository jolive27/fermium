"""Calls from Fermium into Python functions (`use python numpy as np`, DECISIONS D140-D142).

The checker records one entry per call site in `tables.pycalls`: the module and function names, the unit of
each parameter (as a factor: the value passed is SI / factor), which parameters are ints, and the unit and
shape (number or list) of the result.  Both back ends end up in `call()` below, so the JIT (through the
`fm_pycall` / `fm_pyfetch` ctypes callbacks) and the reference interpreter convert values identically.
No llvmlite here: the interpreter and the playground import this module.
"""
from __future__ import annotations

import importlib
import math
import sys

import numpy as np


class PyCallError(Exception):
    """A call into Python failed; the message is what the Fermium program reports."""


def import_python_module(name, base_dir=None):
    """Import a Python module, also looking in the program's folder (for the user's own mylib.py)."""
    added = False
    if base_dir and base_dir not in sys.path:
        sys.path.insert(0, base_dir)
        added = True
    try:
        return importlib.import_module(name)
    finally:
        if added:
            try:
                sys.path.remove(base_dir)
            except ValueError:
                pass


def _function(entry, base_dir):
    fn = entry.get("_fn")
    if fn is None:
        try:
            mod = import_python_module(entry["module"], base_dir)
            fn = getattr(mod, entry["func"])
        except Exception as ex:        # checked when compiling, so only a module that changed since
            raise PyCallError(f"can't load the Python function {entry['display']}: {ex}") from None
        entry["_fn"] = fn
    return fn


def _whole(x):
    return x == x and abs(x) < math.inf and x == math.floor(x)


def _what(r):
    if isinstance(r, str):
        return f"the text {r!r}"
    if isinstance(r, (tuple, list, np.ndarray)):
        a = np.asarray(r, dtype=object)
        if a.ndim == 1:
            return f"a list of {len(a)} numbers"
        return f"an array of shape {a.shape}"
    return f"a {type(r).__name__}"


def call(entry, args, base_dir=None):
    """Call the Python function of a call site.  args: floats and lists (or arrays) of floats, in SI units.
    Returns a float, or a list of floats, in SI units.  Raises PyCallError with a one-line message."""
    name = entry["display"]
    fn = _function(entry, base_dir)
    conv = []
    for k, a in enumerate(args):
        fac, is_int, pname = entry["facs"][k], entry["ints"][k], entry["pnames"][k]
        if isinstance(a, (list, tuple, np.ndarray)):
            arr = np.array(a, dtype=float)
            if fac != 1.0:
                arr = arr / fac
            if is_int:
                if not all(_whole(x) for x in arr):
                    raise PyCallError(f"{name}: {pname} must be a list of whole numbers (it is passed as ints)")
                arr = arr.astype(np.int64)
            conv.append(arr)
        else:
            x = float(a) / fac if fac != 1.0 else float(a)
            if is_int:
                if not _whole(x):
                    raise PyCallError(f"{name}: {pname} must be a whole number (it is passed as an int), "
                                      f"not {x:g}")
                x = int(x)
            conv.append(x)
    try:
        with np.errstate(all="ignore"):
            r = fn(*conv)
    except Exception as ex:
        msg = str(ex).strip().splitlines()[0] if str(ex).strip() else ""
        hint = ""
        if isinstance(ex, TypeError) and "integer" in msg and not any(entry["ints"]):
            hint = (f" (Fermium passes numbers as floats; mark a whole-number parameter in the use line, like  "
                    f"{entry['func']}(a, b, n: int))")
        raise PyCallError(f"the Python function {name} failed: {type(ex).__name__}" + (f": {msg}" if msg else "")
                          + hint) from None
    return convert_result(entry, r)


def convert_result(entry, r):
    name, rfac = entry["display"], entry["rfac"]
    if r is None:
        raise PyCallError(f"the Python function {name} returned nothing (None), but Fermium needs a number")
    if entry["rlist"]:
        if isinstance(r, str) or np.iscomplexobj(r):
            raise PyCallError(f"{name} returned {_what(r) if isinstance(r, str) else 'complex numbers'}, but "
                              f"Fermium expected a list of real numbers here")
        try:
            arr = np.asarray(r, dtype=float)
        except (TypeError, ValueError):
            raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a list of numbers here") from None
        if arr.ndim == 0:
            why = "declared in the use line" if entry.get("declared") else "one of its arguments is a list"
            raise PyCallError(f"{name} returned a single number, but Fermium expected a list here ({why}); "
                              f"declare the result as a number in the use line, like  {entry['func']}(...) -> number")
        if arr.ndim > 1:
            raise PyCallError(f"{name} returned {_what(r)}; Fermium lists have one dimension")
        if rfac != 1.0:
            arr = arr * rfac
        return arr.tolist()
    if isinstance(r, (bool, np.bool_)):
        r = float(r)
    if np.iscomplexobj(r):
        raise PyCallError(f"{name} returned a complex number, but Fermium expected a real number here")
    if np.ndim(r) != 0 or isinstance(r, str):
        raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a number here; if it returns a "
                          f"list, declare it in the use line, like  {entry['func']}(...) -> list")
    try:
        x = float(r)
    except (TypeError, ValueError):
        raise PyCallError(f"{name} returned {_what(r)}, but Fermium expected a number here") from None
    return x * rfac if rfac != 1.0 else x


# ---------------------------------------------------------------- the JIT's side (ctypes callbacks)
def jit_call(rt, cid, ptrs, lens, out):
    """fm_pycall(id, double** ptrs, i64* lens, double* out): a number argument has len -1 (ptrs[k] points at
    it).  Returns 0 (number result in out[0]), the length of a list result (fetched by fm_pyfetch), or -1
    after setting rt.error."""
    try:
        entry = rt.tables.pycalls[cid]
        args = []
        for k in range(entry["nargs"]):
            n = lens[k]
            if n < 0:
                args.append(ptrs[k][0])
            elif n == 0:
                args.append(np.zeros(0))
            else:
                args.append(np.ctypeslib.as_array(ptrs[k], shape=(n,)).copy())
        r = call(entry, args, rt.base_dir)
        if entry["rlist"]:
            rt.py_pending = r
            return len(r)
        out[0] = r
        return 0
    except PyCallError as ex:
        rt.error = str(ex)
        return -1
    except BaseException as ex:         # nothing may escape into the compiled code
        rt.error = f"calling Python failed: {type(ex).__name__}: {ex}"
        return -1


def jit_fetch(rt, dst):
    """fm_pyfetch(double* dst): copy the list result of the last fm_pycall into the new Fermium list."""
    r = getattr(rt, "py_pending", None) or []
    for i, x in enumerate(r):
        dst[i] = x
    rt.py_pending = None
