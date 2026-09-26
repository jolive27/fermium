"""Nonlinear least squares for `fit` (uses SciPy when available)."""
from __future__ import annotations

import math

import numpy as np

from ..errors import FermiumRuntimeError


def _sse(f, y, p):
    try:
        with np.errstate(all="ignore"):
            r = f(p) - y
            v = float(np.dot(r, r))
    except Exception:
        return math.inf
    return v if math.isfinite(v) else math.inf


_POW10 = [s * 10.0 ** e for e in range(-35, 36) for s in (1.0, -1.0)]
# a finer, positive-only grid: most physics parameters (amplitudes, time constants) are positive, and
# 1-2-5 steps keep a scan of A exp(-t/τ) from sticking at a growing exponential (A45)
_POS125 = [m * 10.0 ** e for e in range(-35, 36) for m in (1.0, 2.0, 5.0)]


def _initial_guess(f, y, guess, scales=_POW10):
    """Fill in missing guesses by scanning powers of ten (parameters may be 1e-30 or 1e30 in SI)."""
    p = [g if (g is not None and math.isfinite(g)) else 1.0 for g in guess]
    missing = [i for i, g in enumerate(guess) if g is None or not math.isfinite(g)]
    if not missing:
        return p
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


def least_squares_fit(f, y, guess, extra=None):
    """extra: a dict that receives the covariance matrix of the parameters as extra["cov"] (D124)."""
    y = np.asarray(y, dtype=float)
    n, k = len(y), len(guess)
    if n < k:
        raise FermiumRuntimeError(f"can't fit {k} parameters to only {n} data points")
    try:
        from scipy.optimize import least_squares
    except ImportError:
        raise FermiumRuntimeError("fit needs SciPy: in the fermium folder run  python3 -m pip install -e \".[full]\"")

    def resid(p):
        r = f(p) - y
        r[~np.isfinite(r)] = 1e300
        return r
    starts = [_initial_guess(f, y, guess)]
    if any(g is None or not math.isfinite(g) for g in guess):     # several starts; keep the best fit
        starts.append(_initial_guess(f, y, guess, _POS125))
    res = None
    for p0 in starts:
        x_scale = [abs(v) if v != 0 else 1.0 for v in p0]
        with np.errstate(all="ignore"):
            try:
                cand = _run(least_squares, resid, p0, x_scale, n, k)
            except ValueError:
                continue
        if res is None or cand.cost < res.cost:
            res = cand
    if res is None:
        raise FermiumRuntimeError("the fit failed: the model can't be evaluated at the starting guesses")
    return _finish(res, res.x, res.fun, n, k, extra)


def _run(least_squares, resid, p0, x_scale, n, k):
    return least_squares(resid, p0, x_scale=x_scale, method="lm" if n >= k else "trf", xtol=1e-14, ftol=1e-14,
                        gtol=1e-14, max_nfev=20000)


def degenerate(A, tol=1e-12):
    """Is JᵀJ singular up to rounding?  Scale-free: the matrix is normalised to unit diagonal (a correlation
    matrix) and Gaussian elimination with partial pivoting must not meet a pivot below tol.  Parameters that only
    appear together (`A B`) make it exactly singular in exact arithmetic but only nearly so in floating point,
    where the outcome used to depend on the platform (macOS CI, D262).  aot_data.c:degenerate does the same."""
    A = np.array(A, dtype=float)
    d = np.sqrt(np.abs(np.diag(A)))
    if not np.all(np.isfinite(A)) or np.any(d == 0):
        return True
    M = A / np.outer(d, d)
    k = len(M)
    for c in range(k):
        piv = c + int(np.argmax(np.abs(M[c:, c])))
        if abs(M[piv, c]) < tol:
            return True
        if piv != c:
            M[[c, piv]] = M[[piv, c]]
        for r in range(c + 1, k):
            M[r, c:] -= M[r, c] / M[c, c] * M[c, c:]
    return False


def _finish(res, best, r, n, k, extra=None):
    rss = float(np.dot(r, r))
    dof = max(1, n - k)
    errs = [None] * k
    try:
        J = res.jac
        if degenerate(J.T @ J):
            raise np.linalg.LinAlgError("degenerate")
        cov = np.linalg.inv(J.T @ J) * (rss / dof)
        errs = [math.sqrt(cov[i, i]) if cov[i, i] >= 0 else None for i in range(k)]
        if extra is not None:
            extra["cov"] = cov
    except np.linalg.LinAlgError:
        pass
    return list(best), errs, math.sqrt(rss / n)
