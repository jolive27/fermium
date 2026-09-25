"""Implicit ODE solvers for stiff equations: `solve ... for t from a to b using radau` (or `using bdf`).

Both the compiled code (through a ctypes callback, see Runtime.stiff) and the reference interpreter call
`stiff_solve`, so they take exactly the same steps (DECISIONS D42).  The stepping is SciPy's (Radau IIA
of order 5, or variable-order BDF); what this module adds is Fermium's side: the tolerance mapping, the
error messages, the stop condition `until` (D39) located on the step's dense output, and the stored
solution (t, y, dy at every step, interpolated by the same cubic Hermite as RK45's).
"""
from __future__ import annotations

import math

from ..errors import FermiumRuntimeError

# error kinds shared with codegen_llvm / interp (describe_error in core.py)
ERR_ODE_H, ERR_ODE_NAN, ERR_ODE_RANGE, ERR_NO_EVENT, ERR_STIFF_STEPS = 8, 16, 17, 18, 23
MAX_STEPS = 1_000_000
METHODS = ("radau", "bdf")


class StiffFail(Exception):
    def __init__(self, kind, a=0.0, b=0.0):
        super().__init__(kind)
        self.kind, self.a, self.b = kind, a, b


def _finite(v):
    return all(x - x == 0 for x in v)


def _illinois(g, a, ga, c, gc):
    """A sign change of g between a and c: Illinois to full precision (as interp._illinois)."""
    side = 0
    for _ in range(200):
        if abs(c - a) <= 4e-16 * max(abs(a), abs(c)):
            break
        x = c - gc * (c - a) / (gc - ga)
        if not (min(a, c) < x < max(a, c)):
            x = 0.5 * (a + c)
        gx = g(x)
        if gx == 0 or gx != gx:
            return x
        if gx * gc < 0:
            a, ga, side = c, gc, 0
        else:
            if side == 1:
                ga = 0.5 * ga
            side = 1
        c, gc = x, gx
    return c


def stiff_solve(f, y0, t0, t1, rtol, method="radau", g=None, tname=-1.0, evtext=-1.0):
    """Solve y' = f(t, y) from t0 to t1 (either direction) with an implicit method.

    f(t, y) takes and returns sequences of floats; g(t, y) is the stop condition's lhs - rhs (or None).
    Returns (ts, ys, dys): the accepted steps' times, and the state and its derivative there (flattened,
    row by row).  Raises StiffFail(kind, a, b) with the same error kinds as the RK45 kernel.
    """
    try:
        import numpy as np
        from scipy.integrate import BDF, Radau
    except ImportError:
        raise FermiumRuntimeError(f"`using {method}` needs SciPy: run  pip install scipy") from None
    n = len(y0)
    span = t1 - t0
    if not (span != 0):
        raise StiffFail(ERR_ODE_RANGE, t0, tname)
    y0 = [float(v) for v in y0]
    k0 = list(f(t0, y0))
    if not _finite(k0):
        raise StiffFail(ERR_ODE_NAN, t0, tname)
    if not _finite(y0):
        raise StiffFail(ERR_ODE_H, t0, tname)
    # Tolerance (D17, D42): Fermium's error norm is relative, sc = rtol·(max(|y|, |y_new|) + |y_new − y|).
    # SciPy's is atol + rtol·max(|y|, |y_new|), so atol stands in for the step's change: after each step it
    # is set to rtol·|y_new − y| of that step (a floor for components passing through zero that follows
    # the solution down a decay).  Before the first step there is no change to go by, so the floor is
    # rtol·10⁻⁶ of the size a component starts at or could reach over the range at its initial rate; a
    # component with neither (it starts at 0 with zero slope) borrows the smallest of the others' sizes.
    # (A zero floor makes SciPy's first step and Newton test divide by zero.)
    sizes = [max(abs(y0[j]), abs(k0[j]) * abs(span)) for j in range(n)]
    known = [s for s in sizes if 0 < s < math.inf]
    fallback = min(known) if known else 1.0
    atol = np.array([rtol * 1e-6 * (s if 0 < s < math.inf else fallback) for s in sizes])

    def fun(t, y):
        return np.asarray(f(t, y.tolist()), dtype=float)

    cls = Radau if method == "radau" else BDF
    solver = cls(fun, t0, np.array(y0), t1, rtol=max(rtol, 1e-13), atol=atol)
    ts, ys, dys = [t0], list(y0), list(k0)
    sgn = 0.0
    gprev = 0.0
    if g is not None:
        gprev = g(t0, y0)[0]
        sgn = 1.0 if gprev > 0 else (-1.0 if gprev < 0 else 0.0)     # 0: not known yet (start on it)
    steps = 0
    while solver.status == "running":
        told = solver.t
        try:
            with np.errstate(all="ignore"):
                solver.step()
        except (ValueError, np.linalg.LinAlgError, ZeroDivisionError, OverflowError):
            raise StiffFail(ERR_ODE_H, told, tname) from None     # inf/NaN in the Newton matrix: blows up
        if solver.status == "failed":
            raise StiffFail(ERR_ODE_H, told, tname)
        steps += 1
        if steps > MAX_STEPS:
            raise StiffFail(ERR_STIFF_STEPS, solver.t, tname)
        t = float(solver.t)
        y = solver.y.tolist()
        # the floor follows the step's own change, like the |y_new − y| term of RK45's norm (D17)
        solver.atol = np.maximum(rtol * np.abs(solver.y - np.asarray(ys[-n:])), 1e-300)
        if not _finite(y):
            raise StiffFail(ERR_ODE_H, told, tname)
        if g is not None:
            gn = g(t, y)[0]
            if gn == gn:
                if sgn == 0.0:
                    sgn = 1.0 if gn > 0 else (-1.0 if gn < 0 else 0.0)
                elif gn == 0 or gn * sgn < 0:
                    dense = solver.dense_output()
                    te = t
                    if gn != 0:
                        ga = gprev if gprev * sgn > 0 else sgn
                        te = _illinois(lambda x: g(x, dense(x).tolist())[0], told, ga, t, gn)
                    ye = y if te == t else dense(te).tolist()
                    ts.append(te)
                    ys.extend(ye)
                    dys.extend(f(te, ye))
                    return ts, ys, dys
                gprev = gn
        ts.append(t)
        ys.extend(y)
        dys.extend(solver.f.tolist() if method == "radau" else f(t, y))    # Radau has f at the new point
    if g is not None:
        raise StiffFail(ERR_NO_EVENT, t1, evtext)
    return ts, ys, dys
