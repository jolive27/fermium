"""Bound states of a linear second-order equation (DECISIONS D82):

    solve -ħ²/(2m) ψ'' + V(x) ψ = E ψ  with ψ(a) = 0, ψ(b) = 0  for x from a to b  lowest N

The checker turns the equation into an ODE right-hand side f(x, [ψ, ψ', E]) -> [ψ', ψ'', 0] (E is a
constant state, as in the shooting method).  Here that right side is probed to get the equation's
coefficients on a grid, ψ'' = α(x) ψ - w(x) E ψ (a first-derivative term or a nonlinear equation is
refused), and the N lowest eigenvalues are found by one of two independent methods:

- matrix:   second-order finite differences on the interior points; the symmetric tridiagonal matrix
            W^-1/2 (-D² + diag α) W^-1/2 goes to LAPACK (scipy.linalg.eigh_tridiagonal, only the N lowest),
            on grids of M and 2M intervals, combined by Richardson extrapolation (4 E_2M - E_M)/3 (O(h⁴))
- shooting: Numerov's method (O(h⁴)) from a with ψ(a) = 0, ψ'(a) = 1; the k-th state is bracketed by the
            number of nodes (Sturm's oscillation theorem: k - 1 nodes), then ψ(b; E) = 0 is solved by brentq

Both return the same thing: the grid, and for each state ψ_k (normalised, ∫ψ² dx = 1, first lobe positive)
with ψ_k', plus the eigenvalues.  Used by both back ends (the compiled code through the fm_eigen callback).
"""
from __future__ import annotations

import math


class EigenFail(Exception):
    def __init__(self, message):
        super().__init__(message)
        self.message = message


def _coefficients(rhs, xs):
    """α(x), w(x) of ψ'' = α ψ - w E ψ at each grid point, and checks that the equation has that form."""
    import numpy as np
    n = len(xs)
    alpha = np.empty(n)
    w = np.empty(n)
    worst_beta = 0.0
    worst_lin = 0.0
    h = abs(xs[1] - xs[0]) if n > 1 else 1.0
    for i, x in enumerate(xs):
        a0 = rhs(x, [1.0, 0.0, 0.0])[1]
        a1 = rhs(x, [1.0, 0.0, 1.0])[1]
        alpha[i] = a0
        w[i] = a0 - a1
        if i % 97 == 0 or i == n - 1:
            b0 = rhs(x, [0.0, 1.0, 0.0])[1]              # a ψ' term
            b1 = rhs(x, [0.0, 1.0, 1.0])[1]
            c = rhs(x, [2.0, 0.0, 3.0])[1]                # linear in ψ, and ψ·E
            worst_lin = max(worst_lin, abs(c - (2 * a0 - 6 * w[i])) / (abs(2 * a0) + abs(6 * w[i]) + 1e-300))
            worst_beta = max(worst_beta, (abs(b0) + abs(b1)) * h)     # β ψ' against ψ''/ψ ~ 1/h²
    if not (np.all(np.isfinite(alpha)) and np.all(np.isfinite(w))):
        bad = int(np.argmax(~(np.isfinite(alpha) & np.isfinite(w))))
        raise EigenFail(f"the equation can't be evaluated at x = {xs[bad]:g} (SI units): NaN or infinite; "
                        f"move the range's end away from a singular point")
    if worst_lin > 1e-6:
        raise EigenFail("an eigenvalue problem must be linear: every term has one factor ψ, ψ' or ψ'' "
                        "(like -ħ²/(2m) ψ'' + V(x) ψ = E ψ)")
    if worst_beta > 1e-9:
        raise EigenFail("a term with ψ' isn't supported in eigenvalue problems yet: write the equation with ψ'' "
                        "and ψ only (for a radial equation use u = r R, which removes the R' term)")
    if not np.all(w > 0) and not np.all(w < 0):
        raise EigenFail("the eigenvalue must appear as E ψ with a coefficient of the same sign everywhere "
                        "(like -ħ²/(2m) ψ'' + V(x) ψ = E ψ)")
    if np.all(w < 0):
        raise EigenFail("this equation has no lowest eigenvalues (they go down without end): check the sign "
                        "of the ψ'' term; the standard form is -ħ²/(2m) ψ'' + V(x) ψ = E ψ")
    return alpha, w


def _find_jumps(rhs, xs, alpha, w):
    """Jumps in the coefficients between grid points (a finite well's walls, a step): located by bisection
    to rounding precision.  Returns [(x_jump, (α, w) left, (α, w) right)]."""
    import numpy as np
    out = []
    for arr in (alpha, w):
        d = np.abs(np.diff(arr))
        scale = float(np.max(np.abs(arr))) + 1e-300
        for j in range(len(d)):
            nb = max(d[j - 1] if j > 0 else 0.0, d[j + 1] if j + 1 < len(d) else 0.0)
            if d[j] > 1e-9 * scale and d[j] > 50.0 * nb and not any(xs[j] <= xj <= xs[j + 1] for xj, _, _ in out):
                lo, hi = float(xs[j]), float(xs[j + 1])
                left, right = (alpha[j], w[j]), (alpha[j + 1], w[j + 1])
                for _ in range(80):
                    mid = 0.5 * (lo + hi)
                    if mid <= lo or mid >= hi:
                        break
                    a0 = rhs(mid, [1.0, 0.0, 0.0])[1]
                    if abs(a0 - left[0]) + abs(a0 - rhs(mid, [1.0, 0.0, 1.0])[1] - left[1]) <= \
                            abs(a0 - right[0]) + abs(a0 - rhs(mid, [1.0, 0.0, 1.0])[1] - right[1]):
                        lo = mid
                    else:
                        hi = mid
                out.append((0.5 * (lo + hi), left, right))
    return out


def _on_grid(alpha, w, xs, jumps, stride):
    """The coefficients on every stride-th grid point, each averaged over its cell where a jump cuts the cell
    (so a wall between grid points is felt at its true position: O(h²) instead of O(h))."""
    import numpy as np
    a = np.array(alpha[::stride], dtype=float)
    q = np.array(w[::stride], dtype=float)
    x = xs[::stride]
    h = x[1] - x[0]
    for xj, left, right in jumps:
        i = int(round((xj - x[0]) / h))
        i = min(max(i, 0), len(x) - 1)
        f = (xj - (x[i] - 0.5 * h)) / h           # the part of node i's cell left of the jump
        f = min(max(f, 0.0), 1.0)
        a[i] = f * left[0] + (1 - f) * right[0]
        q[i] = f * left[1] + (1 - f) * right[1]
    return a, q


def _matrix(alpha, w, h, nstates, vectors):
    import numpy as np
    from scipy.linalg import eigh_tridiagonal
    ai, wi = alpha[1:-1], w[1:-1]
    m = len(ai)
    if m < nstates + 2:
        raise EigenFail(f"the grid is too coarse for {nstates} states")
    d = (2.0 / (h * h) + ai) / wi
    e = -1.0 / (h * h * np.sqrt(wi[:-1] * wi[1:]))
    if vectors:
        vals, vecs = eigh_tridiagonal(d, e, select="i", select_range=(0, nstates - 1), tol=4e-308)
        return vals, vecs / np.sqrt(wi)[:, None]
    return eigh_tridiagonal(d, e, eigvals_only=True, select="i", select_range=(0, nstates - 1), tol=4e-308), None


def _numerov(f, h, rev=False):
    """ψ'' = f ψ from one end (ψ = 0, next point h); returns the values (rescaled against overflow, so only
    their signs and ratios are meaningful) and the number of sign changes, the end included (Sturm: the
    number of eigenvalues below E).

    Numerov's method in summed form: with y = (1 - h²f/12) ψ, the increments d = y_{i+1} - y_i grow by
    h² f_i ψ_i each step.  (The textbook form ψ_{i+1} = [2(1 + 5h²f/12) ψ_i - …]/(1 - h²f/12) loses digits
    in the 1 ± h²f/12 factors: 10⁻⁹ relative in E on a 4000-point grid.)"""
    n = len(f)
    order = list(range(n - 1, -1, -1)) if rev else list(range(n))
    c = h * h / 12.0
    hh = h * h
    ps = [0.0] * n
    i1 = order[1]
    ps[i1] = h
    y_prev = 0.0
    y = (1.0 - c * f[i1]) * h
    d = y - y_prev
    nodes = 0
    for j in range(2, n):
        i1, i2 = order[j - 1], order[j]
        d += hh * f[i1] * ps[i1]
        y += d
        ps[i2] = v = y / (1.0 - c * f[i2])
        if abs(v) > 1e150:
            for k in order[:j + 1]:
                ps[k] *= 1e-150
            y *= 1e-150
            d *= 1e-150
        if (ps[i2] < 0) != (ps[i1] < 0) and ps[i1] != 0 and ps[i2] != 0:
            nodes += 1
    return ps, nodes


def _numerov_end(al, wl, E, h):
    """ψ(b) (rescaled) and the number of sign changes (the end included: eigenvalues below E), for ψ'' = (α - w E) ψ from ψ(a) = 0: the fast loop of
    the shooting method (the same summed recurrence as _numerov)."""
    c = h * h / 12.0
    hh = h * h
    f = [a - q * E for a, q in zip(al, wl)]
    p = h
    y = (1.0 - c * f[1]) * h
    d = y
    nodes = 0
    last = len(f) - 1
    for i in range(2, last + 1):
        d += hh * f[i - 1] * p
        y += d
        p2 = y / (1.0 - c * f[i])
        if abs(p2) > 1e150:
            p2 *= 1e-150
            y *= 1e-150
            d *= 1e-150
        if p2 != 0.0 and p != 0.0 and (p2 < 0.0) != (p < 0.0):     # including the end: # of E_k below E
            nodes += 1
        p = p2
    return p, nodes


def _shoot(alpha, w, h, nstates):
    from scipy.optimize import brentq
    al, wl = list(alpha), list(w)
    cache = {}

    def run(E):
        r = cache.get(E)
        if r is None:
            r = cache[E] = _numerov_end(al, wl, E, h)
        return r

    def end_value(E):
        return run(E)[0]

    def nodes(E):
        return run(E)[1]

    lo = min(a / q for a, q in zip(al, wl))              # no state below the lowest of α/w (V_min)
    span = h * (len(al) - 1)
    step = (math.pi / span) ** 2 / max(wl)
    hi = lo + step
    for _ in range(200):
        if nodes(hi) >= nstates:
            break
        hi = lo + (hi - lo) * 2.0
    else:
        raise EigenFail("the shooting method couldn't bracket the states")
    out = []
    a = lo
    for k in range(nstates):
        # an interval [p, q] with exactly k nodes at p and k + 1 at q: the (k+1)-th eigenvalue is inside
        p, q = a, hi
        while nodes(q) > k + 1 and q - p > 1e-15 * abs(q):
            mid = 0.5 * (p + q)
            if nodes(mid) > k:
                q = mid
            else:
                p = mid
        while nodes(p) < k and q - p > 1e-15 * abs(q):
            mid = 0.5 * (p + q)
            if nodes(mid) < k + 1:
                p = mid
            else:
                q = mid
        fp, fq = end_value(p), end_value(q)
        if fp == 0:
            E = p
        elif fq == 0:
            E = q
        elif (fp < 0) == (fq < 0):
            raise EigenFail(f"the shooting method lost state {k + 1}; try using matrix, or a finer grid")
        else:
            E = brentq(end_value, p, q, xtol=1e-300, rtol=1e-15, maxiter=400)
        out.append(E)
        a = E
    return out


def _shoot_vector(alpha, w, h, E):
    """The state at E by Numerov from both ends, joined at the rightmost classically allowed point."""
    import numpy as np
    f = alpha - w * E
    left, _ = _numerov(list(f), h)
    right, _ = _numerov(list(f), h, rev=True)
    left, right = np.array(left), np.array(right)
    allowed = np.nonzero(f[1:-1] < 0)[0]
    m = int(allowed[-1]) + 1 if len(allowed) else len(f) // 2
    # avoid joining at (or next to) a node
    lo = max(1, m - len(f) // 4)
    seg = np.abs(left[lo:m + 1]) / (np.max(np.abs(left[lo:m + 1])) + 1e-300)
    good = np.nonzero(seg > 0.3)[0]
    if len(good):
        m = lo + int(good[-1])
    if right[m] == 0 or left[m] == 0:
        return left
    psi = left.copy()
    psi[m:] = right[m:] * (left[m] / right[m])
    return psi


def _finish(xs, h, psi, f):
    """Normalise (Simpson/trapezoid, ∫ψ² dx = 1), make the first lobe positive, and derivatives."""
    import numpy as np
    nrm = math.sqrt(float(np.trapezoid(psi * psi, dx=h)) if hasattr(np, "trapezoid")
                    else float(np.trapz(psi * psi, dx=h)))
    psi = psi / nrm
    big = np.nonzero(np.abs(psi) > 1e-3 * np.max(np.abs(psi)))[0]
    if len(big) and psi[big[0]] < 0:
        psi = -psi
    dpsi = np.gradient(psi, h, edge_order=2)
    return psi, dpsi, f * psi


def eigen_solve(rhs, a, b, nstates, grid=2000, method="matrix"):
    """Returns (xs, ys, dys, energies): xs the grid (2·grid intervals), ys/dys flattened rows of
    [ψ1, ψ1', …, ψN, ψN', E1 … EN] and their x-derivatives."""
    import numpy as np
    if not (b > a):
        raise EigenFail("the range of x is empty or reversed: an eigenvalue problem needs from a to b with a < b")
    nstates = int(nstates)
    M = int(grid)
    if M < 8:
        raise EigenFail("the grid needs at least 8 intervals")
    M += M % 2
    xs = np.linspace(a, b, 2 * M + 1)
    h2 = (b - a) / (2 * M)
    raw_alpha, raw_w = _coefficients(rhs, xs)
    jumps = _find_jumps(rhs, xs, raw_alpha, raw_w)
    alpha, w = _on_grid(raw_alpha, raw_w, xs, jumps, 1)
    if method == "shooting":
        energies = _shoot(alpha, w, h2, nstates)
        vecs = [_shoot_vector(alpha, w, h2, E) for E in energies]
    else:
        e_fine, v = _matrix(alpha, w, h2, nstates, True)
        e_mid, _ = _matrix(*_on_grid(raw_alpha, raw_w, xs, jumps, 2), 2 * h2, nstates, False)
        e_coarse, _ = _matrix(*_on_grid(raw_alpha, raw_w, xs, jumps, 4), 4 * h2, nstates, False)
        energies = []
        for ef, em, ec in zip(e_fine, e_mid, e_coarse):
            # Richardson extrapolation where the error is visibly O(h²) (it shrinks ~4× per halving, as for a
            # smooth potential); otherwise (a jump in V, as in a finite well) the finest grid's value is kept
            d1, d2 = em - ec, ef - em
            ratio = d1 / d2 if d2 != 0 else math.inf
            energies.append((4.0 * ef - em) / 3.0 if 3.0 < ratio < 5.0 else ef)
        vecs = []
        for k in range(nstates):
            psi = np.zeros(len(xs))
            psi[1:-1] = v[:, k]
            vecs.append(psi)
    cols, dcols = [], []
    for E, psi in zip(energies, vecs):
        f = raw_alpha - raw_w * E                  # ψ'' = f ψ at the grid points themselves
        p, dp, ddp = _finish(xs, h2, np.asarray(psi, dtype=float), f)
        cols += [p, dp]
        dcols += [dp, ddp]
    for E in energies:
        cols.append(np.full(len(xs), E))
        dcols.append(np.zeros(len(xs)))
    Y = np.column_stack(cols)
    DY = np.column_stack(dcols)
    return list(xs), list(Y.ravel()), list(DY.ravel()), [float(E) for E in energies]
