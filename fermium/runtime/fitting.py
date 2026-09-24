"""Nonlinear least squares for `fit` (uses SciPy when available)."""
from __future__ import annotations

import math

import numpy as np

from ..errors import FermiumRuntimeError


def _sse(f, y, p):
    try:
        r = f(p) - y
    except Exception:
        return math.inf
    v = float(np.dot(r, r))
    return v if math.isfinite(v) else math.inf


def _initial_guess(f, y, guess):
    """Fill in missing guesses by scanning powers of ten (parameters may be 1e-30 or 1e30 in SI)."""
    p = [g if (g is not None and math.isfinite(g)) else 1.0 for g in guess]
    missing = [i for i, g in enumerate(guess) if g is None or not math.isfinite(g)]
    if not missing:
        return p
    scales = [s * 10.0 ** e for e in range(-35, 36) for s in (1.0, -1.0)]
    for _ in range(2):
        for i in missing:
            best, bestv = p[i], _sse(f, y, p)
            for s in scales:
                q = list(p)
                q[i] = s
                v = _sse(f, y, q)
                if v < bestv:
                    best, bestv = s, v
            p[i] = best
    return p


def least_squares_fit(f, y, guess):
    y = np.asarray(y, dtype=float)
    p0 = _initial_guess(f, y, guess)
    n, k = len(y), len(p0)
    if n < k:
        raise FermiumRuntimeError(f"can't fit {k} parameters to only {n} data points")
    try:
        from scipy.optimize import least_squares
    except ImportError:
        raise FermiumRuntimeError("fit needs SciPy: run  pip install scipy")
    x_scale = [abs(v) if v != 0 else 1.0 for v in p0]

    def resid(p):
        r = f(p) - y
        r[~np.isfinite(r)] = 1e300
        return r
    res = least_squares(resid, p0, x_scale=x_scale, method="lm" if n >= k else "trf", xtol=1e-14, ftol=1e-14,
                        gtol=1e-14, max_nfev=20000)
    best = res.x
    r = res.fun
    rss = float(np.dot(r, r))
    dof = max(1, n - k)
    errs = [None] * k
    try:
        J = res.jac
        cov = np.linalg.inv(J.T @ J) * (rss / dof)
        errs = [math.sqrt(cov[i, i]) if cov[i, i] >= 0 else None for i in range(k)]
    except np.linalg.LinAlgError:
        pass
    return list(best), errs, math.sqrt(rss / n)
