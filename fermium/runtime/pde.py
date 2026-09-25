"""1-D partial differential equations on a grid (DECISIONS D83):

    solve ∂u/∂t = D ∂²u/∂x²                        (heat / diffusion: first order in t)
    solve ∂²u/∂t² = c² ∂²u/∂x²                     (waves: second order in t)
    solve i ħ ∂ψ/∂t = -ħ²/(2m) ∂²ψ/∂x² + V(x) ψ     (time-dependent Schrödinger: complex)
      with u(x, t0) = …, [∂u/∂t(x, t0) = …,] u(a, t) = … or ∂u/∂x(a, t) = …, (the same at b)
      for x from a to b, t from t0 to t1 [step dt] [grid M] [using crank_nicolson | implicit | explicit]

The checker compiles one "probe" function f(x, [u, u_x, u_xx, t, u_t, i]) -> [rhs, u0, phase0, v0, left, right]:
rhs is the equation solved for its highest t-derivative, u0·exp(i phase0) the initial value, v0 the initial
∂u/∂t (waves), left/right the boundary values.  Here the probe is evaluated to get the equation's
coefficients on the grid, rhs = A u_xx + B u_x + C u + Dt u_t + S(x, t): linear, coefficients constant in t
(the source S may depend on t).  The imaginary unit is recovered from how rhs depends on the probe's i:
rhs(i) = R + G/i + K i, so the true right side is R + i (K - G).

Methods (second-order differences in x; the grid has M intervals):
- first order in t: the θ-method on the semi-discrete system u' = T u + b(t): Crank–Nicolson (θ = ½, the
  default: second order and unconditionally stable; unitary for Schrödinger, so ‖ψ‖ is conserved to rounding),
  implicit (backward Euler, θ = 1), explicit (forward Euler, θ = 0, stable only for dt ≤ h²/(2 max D)).
  One sparse LU factorisation, then one solve per step.
- second order in t: the explicit central-difference (leapfrog) scheme, stable for c dt ≤ h (Courant ≤ 1);
  by default dt is the largest step with Courant number ≤ 1 (at exactly 1, it is exact for constant c).
Boundary conditions: u = g(t) (Dirichlet) or ∂u/∂x = g(t) (Neumann, by a ghost point).

The solution is returned as snapshots (at most ~1000 times, always including the last) of u at every grid
point, with ∂u/∂t, for the SolStruct: rows [u_0 … u_M] (real) or [Re u_0 … Re u_M, Im u_0 … Im u_M].
"""
from __future__ import annotations

import math


class PdeFail(Exception):
    def __init__(self, message):
        super().__init__(message)
        self.message = message


RHS, U0, PHASE0, V0, LEFT, RIGHT = range(6)
MAX_SNAPSHOTS = 1000
RANNACHER_STEPS = 4      # CN's first steps are done with L-stable SDIRK2 (D131)
CHECKPOINTS = 8          # times at which step doubling compares two solutions (D130)
PDE_TOL = 1e-3           # the estimated error allowed, relative to the solution's size (as RK4_WARN)
PDE_MAX_STEPS = 32000    # the default step is halved at most until there are this many steps


class _Probe:
    def __init__(self, probe, is_complex):
        self.f = probe
        self.cx = is_complex

    def val(self, x, u, ux, uxx, t, ut):
        """The right side at x for given (u, u_x, u_xx, u_t) values, as a complex number."""
        if not self.cx:
            return complex(self.f(x, [u, ux, uxx, t, ut, 0.0])[RHS])
        g1 = self.f(x, [u, ux, uxx, t, ut, 1.0])[RHS]
        gm = self.f(x, [u, ux, uxx, t, ut, -1.0])[RHS]
        g2 = self.f(x, [u, ux, uxx, t, ut, 2.0])[RHS]
        R = 0.5 * (g1 + gm)
        S = 0.5 * (g1 - gm)          # G + K
        K = (g2 - R - 0.5 * S) / 1.5
        G = S - K
        return complex(R, K - G)

    def coefficients(self, xs, t):
        import numpy as np
        n = len(xs)
        A = np.empty(n, complex)
        B = np.empty(n, complex)
        C = np.empty(n, complex)
        D = np.empty(n, complex)
        S = np.empty(n, complex)
        for j, x in enumerate(xs):
            s0 = self.val(x, 0.0, 0.0, 0.0, t, 0.0)
            S[j] = s0
            C[j] = self.val(x, 1.0, 0.0, 0.0, t, 0.0) - s0
            B[j] = self.val(x, 0.0, 1.0, 0.0, t, 0.0) - s0
            A[j] = self.val(x, 0.0, 0.0, 1.0, t, 0.0) - s0
            D[j] = self.val(x, 0.0, 0.0, 0.0, t, 1.0) - s0
        return A, B, C, D, S

    def source(self, xs, t):
        import numpy as np
        return np.array([self.val(x, 0.0, 0.0, 0.0, t, 0.0) for x in xs], complex)

    def out(self, x, t, k):
        return self.f(x, [0.0, 0.0, 0.0, t, 0.0, 0.0])[k]


def _check_finite(arrs, xs, what):
    import numpy as np
    for a in arrs:
        bad = ~np.isfinite(a)
        if np.any(bad):
            j = int(np.argmax(bad))
            raise PdeFail(f"{what} is NaN or infinite at x = {xs[j]:g} (SI units)")


def _average_jumps(P, xs, h, t, coefs):
    """A jump in a coefficient between grid points (a potential barrier's walls, a change of material) is
    located by bisection and the coefficient of the node whose cell it cuts becomes the cell average, so the
    wall is felt at its true position: O(h²) instead of O(h) (the same treatment as eigenvalue problems, D82)."""
    import numpy as np
    probes = (lambda x: P.val(x, 0.0, 0.0, 1.0, t, 0.0) - P.val(x, 0.0, 0.0, 0.0, t, 0.0),
              lambda x: P.val(x, 0.0, 1.0, 0.0, t, 0.0) - P.val(x, 0.0, 0.0, 0.0, t, 0.0),
              lambda x: P.val(x, 1.0, 0.0, 0.0, t, 0.0) - P.val(x, 0.0, 0.0, 0.0, t, 0.0))
    n = len(xs)
    for arr, f in zip(coefs, probes):
        d = np.abs(np.diff(arr))
        scale = float(np.max(np.abs(arr))) + 1e-300
        for j in range(len(d)):
            nb = max(d[j - 1] if j > 0 else 0.0, d[j + 1] if j + 1 < len(d) else 0.0)
            if not (d[j] > 1e-9 * scale and d[j] > 50.0 * nb):
                continue
            left, right = arr[j], arr[j + 1]
            lo, hi = float(xs[j]), float(xs[j + 1])
            for _ in range(80):
                mid = 0.5 * (lo + hi)
                if mid <= lo or mid >= hi:
                    break
                if abs(f(mid) - left) <= abs(f(mid) - right):
                    lo = mid
                else:
                    hi = mid
            xj = 0.5 * (lo + hi)
            i = min(max(int(round((xj - xs[0]) / h)), 0), n - 1)
            frac = min(max((xj - (xs[i] - 0.5 * h)) / h, 0.0), 1.0)      # part of node i's cell left of the jump
            arr[i] = frac * left + (1 - frac) * right


def pde_solve(probe, xa, xb, t0, t1, *, grid=400, order=1, method="crank_nicolson", step=None,
              bc=(0, 0), is_complex=False, tdep=False, warn=None):
    """Returns (ts, ys, dys, ncomp): snapshot times, flattened rows of u (and ∂u/∂t) at the M + 1 grid points."""
    import numpy as np
    if not (xb > xa):
        raise PdeFail("the range of x is empty or reversed: write  for x from a to b  with a < b")
    if not (t1 > t0):
        raise PdeFail("the range of t is empty or reversed: a PDE is solved forward in time, from t0 to t1 > t0")
    M = int(grid)
    xs = np.linspace(xa, xb, M + 1)
    h = (xb - xa) / M
    P = _Probe(probe, is_complex)
    A, B, C, D, S = P.coefficients(xs, t0)
    _check_finite((A, B, C, D, S), xs, "the equation")
    # coefficients that change in time aren't supported; a source that does is re-evaluated every step
    A1, B1, C1, D1, S1 = P.coefficients(xs[:: max(1, M // 16)], t1)
    for name, c0, c1 in (("∂²u/∂x²", A, A1), ("∂u/∂x", B, B1), ("u", C, C1), ("∂u/∂t", D, D1)):
        c0s = c0[:: max(1, M // 16)]
        if np.any(np.abs(c0s - c1) > 1e-9 * (np.abs(c0s) + np.abs(c1)) + 1e-300):
            raise PdeFail(f"the coefficient of {name} changes with t; only a source term (a part without u) "
                          f"may depend on t for now")
    src_tdep = tdep and np.any(np.abs(S[:: max(1, M // 16)] - S1) > 1e-12 * (np.abs(S1) + 1e-300))
    # linearity: every term has exactly one factor u, u_x, u_xx (or u_t)
    for j in range(0, M + 1, max(1, M // 7)):
        x = xs[j]
        want = 2 * C[j] + 3 * B[j] + 5 * A[j] + 7 * D[j] + S[j]
        got = P.val(x, 2.0, 3.0, 5.0, t0, 7.0)
        if abs(got - want) > 1e-8 * (abs(2 * C[j]) + abs(3 * B[j]) + abs(5 * A[j]) + abs(7 * D[j]) + abs(S[j])) \
                + 1e-300:
            raise PdeFail("a PDE must be linear in u: each term may have one factor u, ∂u/∂x or ∂²u/∂x² "
                          "(no u², no products of them)")
    if order == 1 and np.any(D != 0):
        raise PdeFail("internal: ∂u/∂t on the right of a first-order equation")
    if not is_complex:
        for arr in (A, B, C, D, S):
            if np.any(arr.imag != 0):
                raise PdeFail("internal: complex coefficients in a real equation")

    _average_jumps(P, xs, h, t0, (A, B, C))

    # initial values
    u0 = np.array([P.out(x, t0, U0) for x in xs], float)
    ph = np.array([P.out(x, t0, PHASE0) for x in xs], float)
    u = u0 * np.exp(1j * ph) if is_complex else u0.astype(complex)
    v = np.array([P.out(x, t0, V0) for x in xs], complex) if order == 2 else None
    _check_finite([u] + ([v] if v is not None else []), xs, "the initial value")

    def bval(t):
        return P.out(xa, t, LEFT), P.out(xb, t, RIGHT)

    # the tridiagonal operator T (interior and Neumann rows) and the boundary/source vector b(t)
    lo = np.zeros(M + 1, complex)        # T[j, j-1]
    di = np.zeros(M + 1, complex)        # T[j, j]
    up = np.zeros(M + 1, complex)        # T[j, j+1]
    lo[1:M] = A[1:M] / h ** 2 - B[1:M] / (2 * h)
    di[1:M] = -2 * A[1:M] / h ** 2 + C[1:M]
    up[1:M] = A[1:M] / h ** 2 + B[1:M] / (2 * h)
    if bc[0] == 1:
        di[0] = -2 * A[0] / h ** 2 + C[0]
        up[0] = 2 * A[0] / h ** 2
    if bc[1] == 1:
        di[M] = -2 * A[M] / h ** 2 + C[M]
        lo[M] = 2 * A[M] / h ** 2
    dirichlet = [j for j, k in ((0, bc[0]), (M, bc[1])) if k == 0]

    def bvec(t, Sx=None):
        b = (Sx if Sx is not None else (P.source(xs, t) if src_tdep else S)).copy()
        gl, gr = bval(t)
        if bc[0] == 1:
            b[0] += -2 * A[0] * gl / h + B[0] * gl
        if bc[1] == 1:
            b[M] += 2 * A[M] * gr / h + B[M] * gr
        for j in dirichlet:
            b[j] = 0.0
        return b

    def apply_T(w):
        r = di * w
        r[1:] += lo[1:] * w[:-1]
        r[:-1] += up[:-1] * w[1:]
        return r

    def set_dirichlet(w, t):
        gl, gr = bval(t)
        if bc[0] == 0:
            w[0] = gl
        if bc[1] == 0:
            w[M] = gr

    set_dirichlet(u, t0)
    span = t1 - t0
    amax = float(np.max(np.abs(A))) if M > 0 else 0.0
    if order == 1:
        theta = {"crank_nicolson": 0.5, "implicit": 1.0, "explicit": 0.0}[method]
        if step is None:
            nt = 1000
            if theta == 0.0 and amax > 0:
                nt = max(nt, int(math.ceil(span / (0.45 * h * h / amax))))
        else:
            nt = max(1, int(math.ceil(span / step - 1e-9)))
        dt = span / nt
        if theta == 0.0:
            if is_complex and np.any(A.imag != 0):
                raise PdeFail("the explicit method is unstable for the Schrödinger equation at any step; use "
                              "crank_nicolson (the default)")
            if amax * dt / (h * h) > 0.5 + 1e-12:
                raise PdeFail(f"the explicit method is unstable with this step: it needs dt ≤ h²/(2D) = "
                              f"{0.5 * h * h / amax:.4g} s (SI) on this grid; use a smaller step, or crank_nicolson")
    else:
        if is_complex:
            raise PdeFail("a complex equation must be first order in t (like the Schrödinger equation)")
        cmax = math.sqrt(float(np.max(np.abs(A.real)))) if amax > 0 else 0.0
        if np.any(A.real < 0):
            raise PdeFail("this second-order equation isn't a wave equation (the coefficient of ∂²u/∂x² must be "
                          "positive, c²)")
        if step is None:
            nt = max(1, int(math.ceil(span * cmax / h - 1e-9))) if cmax > 0 else 1000
        else:
            nt = max(1, int(math.ceil(span / step - 1e-9)))
        dt = span / nt
        if cmax * dt > h * (1 + 1e-9):
            raise PdeFail(f"the wave equation's explicit scheme is unstable with this step: it needs c dt ≤ h, "
                          f"dt ≤ {h / cmax:.4g} s (SI) on this grid (Courant number {cmax * dt / h:.3g}); use a "
                          f"smaller step or leave out step")
    every = max(1, int(math.ceil(nt / MAX_SNAPSHOTS)))
    ts, rows, drows = [], [], []

    def ut_of(w, t):
        d = apply_T(w) + bvec(t)
        for j in dirichlet:
            e = 1e-6 * max(abs(span), 1e-300)
            g1, g2 = bval(t + e), bval(t - e)
            d[j] = ((g1[0] - g2[0]) if j == 0 else (g1[1] - g2[1])) / (2 * e)
        return d

    def keep(t, w, dw):
        if not (np.all(np.isfinite(w))):
            raise PdeFail(f"the solution became NaN or infinite at t = {t:g} s (SI): the scheme is unstable or "
                          f"the equation blows up")
        ts.append(t)
        if is_complex:
            rows.append(np.concatenate([w.real, w.imag]))
            drows.append(np.concatenate([dw.real, dw.imag]))
        else:
            rows.append(w.real.copy())
            drows.append(dw.real.copy())

    lus = {}

    def _lu(tau):
        """The factorisation of (I - tau T) with the Dirichlet rows replaced by identity rows (cached)."""
        if tau not in lus:
            from scipy.sparse import diags
            from scipy.sparse.linalg import splu
            T = diags([lo[1:], di, up[:-1]], [-1, 0, 1], shape=(M + 1, M + 1), format="lil", dtype=complex)
            I_ = diags([np.ones(M + 1)], [0], shape=(M + 1, M + 1), format="lil", dtype=complex)
            L_ = (I_ - tau * T).tolil()
            for j in dirichlet:
                L_[j, :] = 0
                L_[j, j] = 1.0
            lus[tau] = splu(L_.tocsc())
        return lus[tau]

    def _step(w, t_old, dt_, th, b_old):
        """One θ-method step from t_old to t_old + dt_; returns (w_new, b_new)."""
        t = t_old + dt_
        b_new = bvec(t)
        rhs = w + (1 - th) * dt_ * (apply_T(w) + b_old) + th * dt_ * b_new
        gl, gr = bval(t)
        if bc[0] == 0:
            rhs[0] = gl
        if bc[1] == 0:
            rhs[M] = gr
        return (_lu(th * dt_).solve(rhs) if th > 0 else rhs), b_new

    G2 = 1.0 - 1.0 / math.sqrt(2.0)

    def _sdirk2(w, t_old, dt_, b_old):
        """One step of the two-stage, stiffly accurate SDIRK method (γ = 1 − 1/√2): second order and L-stable,
        so the grid-scale modes are damped (by ~0.4/(γ² λ dt) per step) where CN only flips their sign."""
        tau = G2 * dt_
        lu_ = _lu(tau)

        def solve(rhs, t):
            gl, gr = bval(t)
            if bc[0] == 0:
                rhs[0] = gl
            if bc[1] == 0:
                rhs[M] = gr
            return lu_.solve(rhs)
        b1 = bvec(t_old + tau)
        k1 = solve(w + tau * b1, t_old + tau)
        b2 = bvec(t_old + dt_)
        return solve(w + (1 - G2) * dt_ * (apply_T(k1) + b1) + tau * b2, t_old + dt_), b2

    def _march(n_steps, record, checks=()):
        """Solve with n_steps equal steps.  Crank–Nicolson on real equations starts with Rannacher-style
        smoothing (D131): its first RANNACHER_STEPS steps use an L-stable method (SDIRK2), which damps the
        grid-scale modes that a jump between the initial and boundary data excites (CN's factor for them is
        ≈ −1, so they would never decay).  Returns (ts, rows, drows, {step: u at that step})."""
        dt_ = span / n_steps
        smooth = RANNACHER_STEPS if (theta == 0.5 and not is_complex) else 0
        w = u.copy()
        tss, rws, drws, at = [], [], [], {}
        if record:
            tss.append(t0), rws.append(None), drws.append(None)
            _keep_into(tss, rws, drws, 0, t0, w)
        b_old = bvec(t0)
        want = set(checks)
        for n in range(1, n_steps + 1):
            t_old = t0 + (n - 1) * dt_
            if n <= smooth:
                w, b_old = _sdirk2(w, t_old, dt_, b_old)
            else:
                w, b_old = _step(w, t_old, dt_, theta, b_old)
            t = t0 + n * dt_
            if n in want:
                at[n] = w.copy()
            if record and (n % every_of(n_steps) == 0 or n == n_steps):
                tss.append(t), rws.append(None), drws.append(None)
                _keep_into(tss, rws, drws, len(tss) - 1, t, w)
        return tss, rws, drws, at

    def every_of(n_steps):
        return max(1, int(math.ceil(n_steps / MAX_SNAPSHOTS)))

    def _keep_into(tss, rws, drws, k, t, w):
        if not (np.all(np.isfinite(w))):
            raise PdeFail(f"the solution became NaN or infinite at t = {t:g} s (SI): the scheme is unstable or "
                          f"the equation blows up")
        dw = ut_of(w, t)
        if is_complex:
            rws[k] = np.concatenate([w.real, w.imag])
            drws[k] = np.concatenate([dw.real, dw.imag])
        else:
            rws[k] = w.real.copy()
            drws[k] = dw.real.copy()

    def _estimate(fine, coarse_n, fine_n, scale):
        """Step doubling: the largest difference between the solutions with coarse_n and fine_n = 2 coarse_n
        steps, at CHECKPOINTS common times, relative to the solution's size."""
        worst = 0.0
        for k, wc in coarse.items():
            wf = fine[2 * k] if fine_n == 2 * coarse_n else fine[k // 2]
            worst = max(worst, float(np.max(np.abs(wf - wc))))
        return worst / scale

    def _checks(n_steps):
        return sorted({max(1, (k * n_steps) // CHECKPOINTS) for k in range(1, CHECKPOINTS + 1)})

    def _scale(rws):
        m = max(float(np.max(np.abs(r))) for r in rws)
        gl, gr = bval(t0)
        return max(m, abs(gl), abs(gr))

    def _controlled(n_steps):
        """Crank–Nicolson / implicit with an accuracy check (D130).  With a step of your own: solve with it and
        with half of it; if the estimated error is over PDE_TOL of the solution's size, warn.  With the
        default step: halve it until the estimate is under PDE_TOL (up to PDE_MAX_STEPS steps), or warn."""
        nonlocal coarse
        p = 2 if theta == 0.5 else 1
        if step is not None:
            ck = _checks(n_steps)
            tss, rws, drws, at = _march(n_steps, True, ck)
            coarse = at
            fine_at = _march(2 * n_steps, False, [2 * k for k in ck])[3]
            scale = _scale(rws)
            if scale > 0:
                est = _estimate(fine_at, n_steps, 2 * n_steps, scale) * 2 ** p / (2 ** p - 1)
                if est > PDE_TOL and warn is not None:
                    warn(8, est)
            return tss, rws, drws
        half = max(1, n_steps // 2)
        coarse = _march(half, False, _checks(half))[3]
        while True:
            ckh = _checks(half)
            tss, rws, drws, at = _march(2 * half, True, sorted({2 * k for k in ckh} | set(_checks(2 * half))))
            scale = _scale(rws)
            est = _estimate(at, half, 2 * half, scale) / (2 ** p - 1) if scale > 0 else 0.0
            if est <= PDE_TOL:
                return tss, rws, drws
            if 2 * half >= PDE_MAX_STEPS:
                if warn is not None:
                    warn(9, est)
                return tss, rws, drws
            half *= 2
            coarse = {k: at[k] for k in _checks(half)}

    coarse = {}

    if order == 1:
        if theta == 0.0:
            ts, rows, drows = _march(nt, True)[:3]
        else:
            ts, rows, drows = _controlled(nt)
    else:
        def acc(w, vel, t):
            return apply_T(w) + D * vel + bvec(t)
        a0 = acc(u, v, t0)
        u_prev = u
        u_cur = u + dt * v + 0.5 * dt * dt * a0
        set_dirichlet(u_cur, t0 + dt)
        keep(t0, u, v)
        damp = D * dt / 2
        for n in range(1, nt + 1):
            t = t0 + n * dt
            # (1 - D dt/2) u⁺ = 2u - (1 + D dt/2) u⁻ + dt² (T u + b)
            u_next = (2 * u_cur - (1 + damp) * u_prev + dt * dt * (apply_T(u_cur) + bvec(t))) / (1 - damp)
            set_dirichlet(u_next, t + dt)
            if n % every == 0 or n == nt:
                keep(t, u_cur, (u_next - u_prev) / (2 * dt))
            u_prev, u_cur = u_cur, u_next
    ncomp = 2 if is_complex else 1
    Y = np.array(rows)
    DY = np.array(drows)
    return ts, list(Y.ravel()), list(DY.ravel()), ncomp, M
