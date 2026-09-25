"""Runtime glue for the M3 solvers that run in Python (DECISIONS D82, D83): compiled code calls these
through ctypes callbacks registered in runtime/core.py, passing the address of a compiled right-hand
side; the result goes back as a malloc'ed SolStruct, like the stiff solvers' (D42)."""
from __future__ import annotations

import ctypes

from .core import SolStruct, _libc, DPTR

c_double, c_void_p = ctypes.c_double, ctypes.c_void_p


class _Inner(Exception):
    """The compiled right-hand side stopped with its own run-time error (message and line already set)."""


def compiled_rhs(guard, fn, env, n):
    """A Python f(t, y) -> dy calling a compiled ODE_FN lambda through fm_ode_guard."""
    call = ctypes.CFUNCTYPE(ctypes.c_int32, c_void_p, DPTR, c_double, DPTR, DPTR)(guard)
    yb = (c_double * n)()
    ob = (c_double * n)()

    def f(t, y):
        for j in range(n):
            yb[j] = y[j]
        if call(fn, env, t, yb, ob):
            raise _Inner()
        return ob[:n]
    f._keep = call
    return f


def solstruct(ts, ys, dys, dim):
    """A malloc'ed SolStruct holding copies of the arrays (the compiled program owns it)."""
    libc = _libc()
    m = len(ts)
    sp = ctypes.cast(libc.malloc(ctypes.sizeof(SolStruct)), ctypes.POINTER(SolStruct))
    s = sp.contents
    s.n, s.dim, s.cap = m, dim, m
    for name, vals in (("t", ts), ("y", ys), ("dy", dys)):
        buf = libc.malloc(max(1, len(vals)) * 8)
        ctypes.memmove(buf, (c_double * len(vals))(*vals), len(vals) * 8)
        setattr(s, name, ctypes.cast(buf, DPTR))
    return ctypes.cast(sp, c_void_p).value


def eigen_cb(rt, guard, fn, env, a, b, nstates, grid, method, out):
    """fm_eigen: 0 = ok, 1 = solver error (rt.error set), 2 = the equation stopped with its own error."""
    from .eigen import EigenFail, eigen_solve
    try:
        f = compiled_rhs(guard, fn, env, 3)
        xs, ys, dys, _ = eigen_solve(f, a, b, nstates, grid, "shooting" if method == 1 else "matrix")
    except _Inner:
        return 2
    except EigenFail as ex:
        rt.error = ex.message
        return 1
    except BaseException as ex:          # nothing may escape into the compiled code
        rt.error = f"the eigenvalue solver failed: {ex}"
        return 1
    out[0] = solstruct(xs, ys, dys, 3 * int(nstates))
    return 0
