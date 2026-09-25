"""A reference interpreter for the typed IR.

It runs exactly the same IR the LLVM code generator compiles, in plain Python.  Uses:
  * differential testing: every program must print the same thing with `fermium run` (native code)
    and `fermium run --interp` (this file) -- see tests/test_differential.py;
  * a fallback where llvmlite isn't available (e.g. a browser playground).
It mirrors the code generator's numerical kernels (GK15 quadrature, RK4, Dormand–Prince 5(4),
Hermite interpolation) operation for operation, so results agree to rounding.
It is slow (a tree-walking interpreter) -- use it for checking, not for speed.
"""
from __future__ import annotations

import math
import time

import numpy as np

from . import ir as I
from .errors import FermiumError, FermiumRuntimeError
from .numerics import PyOps, quintic_hermite, odd_root_numerator, XGK, WGK, WG
from .types import ListTy, VecTy, MatTy, BoolTy, NumTy
from . import linalg
from .runtime.stiff import abs_tolerances, step_small_kind
from . import special
from .uncertain import UFloat, UncertainUse
from . import uncertain as U

ERR_INDEX, ERR_SOLRANGE, ERR_ODE_STEPS, ERR_ASSERT, ERR_LEN, ERR_EMPTY, ERR_STEP, ERR_ODE_H = 1, 2, 3, 4, 5, 6, 7, 8
ERR_QUAD = 9
ERR_QUAD_NAN, ERR_QUAD_INF = 31, 32
ERR_SIZE, ERR_RANGE, MAX_LIST = 11, 12, 1e9
ERR_ROOT, ERR_POLE = 13, 14
ERR_SINGULAR = 15
ERR_STD_ONE = 24
ERR_SAMPLE_COUNT, ERR_NEG_SIGMA = 25, 26     # red team round 2 #14
ERR_NOT_SYMMETRIC, ERR_NOT_POSDEF = 21, 22
ERR_ODE_NAN, ERR_ODE_RANGE, ERR_NO_EVENT = 16, 17, 18
ERR_ODE_SINGULAR = 33


class _Break(Exception):
    pass


class _Continue(Exception):
    pass


class _Return(Exception):
    def __init__(self, value):
        self.value = value


class _Fail(Exception):
    def __init__(self, kind, a=0.0, b=0.0):
        self.kind, self.a, self.b = kind, a, b


# ---------------------------------------------------------------- IEEE-style arithmetic helpers
def fdiv(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        if a == 0 or a != a:
            return math.nan
        return math.copysign(math.inf, a) * math.copysign(1.0, b)


def fpow(a, b):
    if isinstance(a, UFloat) or isinstance(b, UFloat):
        return a ** b if isinstance(a, UFloat) else b.__rpow__(a)
    if type(a) is np.ndarray or type(b) is np.ndarray:        # propagate montecarlo, vectorized (D123)
        return np.power(np.asarray(a, dtype=float), b)
    try:
        r = a ** b
    except OverflowError:
        return math.inf
    except ZeroDivisionError:
        return math.inf
    if isinstance(r, complex):
        return math.nan
    return float(r)


def fmul(a, b):
    return a * b


def _m(fn):
    def g(x):
        try:
            return float(fn(x))
        except (ValueError, OverflowError):
            with np.errstate(all="ignore"):
                return float(getattr(np, fn.__name__, fn)(np.float64(x)))
    return g


MATH = {"sin": math.sin, "cos": math.cos, "tan": math.tan, "asin": math.asin, "acos": math.acos,
        "atan": math.atan, "sinh": math.sinh, "cosh": math.cosh, "tanh": math.tanh, "asinh": math.asinh,
        "acosh": math.acosh, "atanh": math.atanh, "exp": math.exp, "ln": math.log, "log": math.log,
        "log10": math.log10, "log2": math.log2, "erf": math.erf, "erfc": math.erfc, "gamma": math.gamma,
        "lgamma": math.lgamma, "expm1": math.expm1, "log1p": math.log1p, "abs": abs, "floor": math.floor,
        "ceil": math.ceil}


def _recip(fn):
    """cot, sec, csc as 1/tan, 1/cos, 1/sin, with IEEE division like the compiled code (#63)."""
    def g(x):
        return fdiv(1.0, math1(fn, x))
    return g


MATH.update({"cot": _recip("tan"), "sec": _recip("cos"), "csc": _recip("sin")})


NP_MATH = {"ln": np.log, "log": np.log, "abs": np.abs, "asin": np.arcsin, "acos": np.arccos, "atan": np.arctan,
           "asinh": np.arcsinh, "acosh": np.arccosh, "atanh": np.arctanh}


def math1(name, x):
    if isinstance(x, UFloat):
        return U.apply1(name, x, math1)
    if type(x) is np.ndarray:                   # propagate montecarlo, vectorized (D123)
        f = NP_MATH.get(name) or getattr(np, name, None)
        if f is None or name in ("round", "gamma", "lgamma", "erf", "erfc"):
            raise TypeError("not vectorized")
        return f(x)
    if name == "round":
        return math.copysign(math.floor(abs(x) + 0.5), x) if math.isfinite(x) else x
    if name == "sign":
        return (x > 0) - (x < 0) + 0.0
    if name in ("floor", "ceil") and not math.isfinite(x):
        return x
    f = MATH[name]
    try:
        return float(f(x))
    except (ValueError, OverflowError, ZeroDivisionError):
        with np.errstate(all="ignore"):
            npf = {"ln": np.log, "log": np.log, "gamma": None, "lgamma": None}.get(name, getattr(np, name, None))
            if npf is None:
                return math.inf
            return float(npf(np.float64(x)))


def powc(x, p):
    if isinstance(x, UFloat):
        return x.powc(p, powc)
    if type(x) is np.ndarray:                   # propagate montecarlo, vectorized (D123)
        if p == int(p):
            return x ** p
        if abs(p - 1 / 3) < 1e-15:
            return np.cbrt(x)
        r = np.abs(x) ** p
        n = odd_root_numerator(p)
        if n is None:
            return np.where(x < 0, math.nan, r)
        return np.where(x < 0, -r if n % 2 else r, r)
    if p == 2:
        return x * x
    if p == 3:
        return x * x * x
    if p == 1:
        return x
    if p == 0.5:
        return math.sqrt(x) if x >= 0 else (math.nan if x == x else x)
    if p == -1:
        return fdiv(1.0, x)
    if p == -2:
        return fdiv(1.0, x * x)
    if p == 4:
        x2 = x * x
        return x2 * x2
    if p == -0.5:
        return fdiv(1.0, math.sqrt(x)) if x >= 0 else math.nan
    if p == 1.5:
        return x * math.sqrt(x) if x >= 0 else math.nan
    if p == -1.5:
        return fdiv(1.0, x * math.sqrt(x)) if x >= 0 else math.nan
    if abs(p - 1 / 3) < 1e-15:
        return math.copysign(abs(x) ** (1 / 3), x) if math.isfinite(x) else x
    n = odd_root_numerator(p)
    if n is not None and x < 0:     # real odd root of a negative number, as in codegen
        r = fpow(-x, p)
        return -r if n % 2 else r
    return fpow(x, p)


# ---------------------------------------------------------------- ODE solutions
class Sol:
    def __init__(self, dim):
        self.dim = dim
        self.t = []
        self.y = []
        self.dy = []
        self.rhs = None         # f(t, y) for x'(t) at the interpolated state (D46)

    @property
    def n(self):
        return len(self.t)

    def push(self, t, y, dy):
        self.t.append(t)
        self.y.extend(y)
        self.dy.extend(dy)

    def eval(self, comp, t, use_dy):
        n, dim = self.n, self.dim
        tfirst, tlast = self.t[0], self.t[-1]
        slack = 1e-9 * abs(tlast - tfirst)
        lo_t, hi_t = min(tfirst, tlast), max(tfirst, tlast)
        if t < lo_t - slack or t > hi_t + slack or t != t:
            raise _Fail(ERR_SOLRANGE, t, tlast)
        # times increase, or decrease for a solve towards smaller t (D39): compare along the direction
        sg = 1.0 if tlast >= tfirst else -1.0
        lo, hi = 0, n - 1
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if self.t[mid] * sg <= t * sg:
                lo = mid
            else:
                hi = mid
        i = lo
        if n <= 1:
            return (self.dy if use_dy else self.y)[comp]
        ta, tb = self.t[i], self.t[i + 1]
        h = tb - ta
        s = (t - ta) / h
        s2 = s * s
        s3 = s2 * s
        h00 = 2 * s3 - 3 * s2 + 1
        h10 = s3 - 2 * s2 + s
        h01 = -2 * s3 + 3 * s2
        h11 = s3 - s2

        def herm(c):
            ia, ib = i * dim + c, (i + 1) * dim + c
            ya, yb = self.y[ia], self.y[ib]
            ma, mb = self.dy[ia] * h, self.dy[ib] * h
            return ya, yb, ma, mb, h00 * ya + h10 * ma + (h01 * yb + h11 * mb)
        if use_dy and self.rhs is not None:
            return self.rhs(t, [herm(c)[4] for c in range(dim)])[comp]
        ya, yb, ma, mb, r = herm(comp)
        if use_dy:
            d00 = 6 * s2 - 6 * s
            d10 = 3 * s2 - 4 * s + 1
            d01 = 6 * s - 6 * s2
            d11 = 3 * s2 - 2 * s
            return ((d00 * ya + d10 * ma) + (d01 * yb + d11 * mb)) / h
        return r


# Dormand–Prince's dense output (Hairer, Nørsett & Wanner, DOPRI5): stage weights of the 4th-order term
D_DP = [-12715105075 / 11282082432, 0, 87487479700 / 32700410799, -10690763975 / 1880347072,
        701980252875 / 199316789632, -1453857185 / 822651844, 69997945 / 29380423]


def _dense(ya, yb, da, db, r5, h, th):
    """A step's dense output at the fraction th: the cubic Hermite through (ya, da), (yb, db) plus the
    4th-order term r5 (0 for the Hermite alone).  Mirrors ModuleGen._emit_dense."""
    r2 = yb - ya
    r3 = h * da - r2
    r4 = (r2 - h * db) - r3
    th1 = 1.0 - th
    return ya + th * (r2 + th1 * (r3 + th * (r4 + th1 * r5)))


def _finite(v):
    """True if every number in v is finite (x - x is NaN for ±∞ and NaN)."""
    for x in v:
        if not (x - x == 0):
            return False
    return True


def _start(f, t0, y0, tname):
    """The derivative at the start; stop with a clear message if it is NaN or infinite (A1, #32)."""
    k0 = f(t0, y0)
    if not _finite(k0):
        raise _Fail(ERR_ODE_NAN, t0, float(tname))
    return k0


def _first_step(f, t0, y, k0, dirn, aspan, rtol):
    """Mirrors ModuleGen._emit_first_step (Hairer–Wanner first step, gauntlet A5)."""
    d0 = d1 = cnt = 0.0
    for yj, fj in zip(y, k0):
        if abs(yj) > 0:
            sc = rtol * abs(yj)
            d0 += (abs(yj) / sc) ** 2
            d1 += (fj / sc) ** 2
            cnt += 1.0
    hv = aspan * 1e-4
    if cnt > 0 and 0 < d1 < math.inf:
        h0 = min(0.01 * math.sqrt(d0 / d1), aspan)
        k1 = f(t0 + dirn * h0, [yj + (dirn * h0) * fj for yj, fj in zip(y, k0)])
        d2 = 0.0
        for yj, fj, gj in zip(y, k0, k1):
            if abs(yj) > 0:
                d2 += ((gj - fj) / (rtol * abs(yj))) ** 2
        dd1 = math.sqrt(d1 / cnt) * rtol
        dd2 = math.sqrt(math.sqrt(d2 / cnt) / h0 * rtol)
        m = max(dd1, dd2) if dd2 == dd2 else dd2
        h1 = fpow(rtol, 0.2) / m if m > 0 else 100 * h0
        h = min(min(100 * h0, h1), aspan)
        if h == h:
            hv = h
    return hv


def _illinois(g, a, ga, c, gc):
    """A sign change of g between a and c (ga, gc of opposite signs): Illinois to full precision.
    Mirrors the loop in ModuleGen._k_event_locate."""
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


class _Event:
    """The stop condition of `solve ... until lhs = rhs` (D39): g = lhs - rhs changes sign.
    Mirrors ModuleGen._emit_event_check."""

    def __init__(self, g, t0, y0):
        self.g = g
        g0 = g(t0, y0)[0]
        self.sgn = 1.0 if g0 > 0 else (-1.0 if g0 < 0 else 0.0)     # 0: not known yet

    def check(self, f, sol, t, y, k, tn, yn, kn, ks=None):
        """After a step from (t, y, k) to (tn, yn, kn): if g crossed zero, end the solution at the
        crossing and return True.  The crossing is located on the step's dense output: Dormand–Prince's
        4th-order continuous extension when its stages ks are given, else the cubic Hermite."""
        gn = self.g(tn, yn)[0]
        if not (gn == gn):
            return False
        if self.sgn == 0.0:
            self.sgn = 1.0 if gn > 0 else (-1.0 if gn < 0 else 0.0)
            return False
        if not (gn == 0 or gn * self.sgn < 0):
            return False
        n = len(y)
        h = tn - t
        r5 = [0.0] * n
        if ks is not None:
            for j in range(n):
                acc = D_DP[0] * ks[0][j]
                for m in range(2, 7):
                    acc = acc + D_DP[m] * ks[m][j]
                r5[j] = h * acc

        def state(x):
            th = (x - t) / h
            return [_dense(y[j], yn[j], k[j], kn[j], r5[j], h, th) for j in range(n)]
        te = tn
        if gn != 0:
            te = _illinois(lambda x: self.g(x, state(x))[0], t, self.sgn, tn, gn)
        ye = yn if te == tn else state(te)
        sol.push(te, ye, f(te, ye))
        return True


def rk4(f, y0, t0, t1, h0, ev=None, tname=-1, evtext=-1):
    span = t1 - t0
    if not (span != 0):
        raise _Fail(ERR_ODE_RANGE, t0, float(tname))
    ratio = abs(fdiv(span, h0))            # the range gives the direction (D39), the step its size
    if ratio != ratio or ratio <= 0 or ratio > 1e12:
        raise _Fail(ERR_STEP, h0, span)
    steps = int(math.ceil(ratio - 1e-9))
    steps = max(1, steps)
    h = span / steps
    n = len(y0)
    sol = Sol(n)
    y = list(y0)
    half = h * 0.5
    k1 = _start(f, t0, y, tname)
    event = _Event(ev, t0, y) if ev is not None else None
    for s in range(steps):
        t = t0 + s * h
        sol.push(t, y, k1)
        tmp = [y[k] + half * k1[k] for k in range(n)]
        th = t + half
        k2 = f(th, tmp)
        tmp = [y[k] + half * k2[k] for k in range(n)]
        k3 = f(th, tmp)
        tmp = [y[k] + h * k3[k] for k in range(n)]
        k4 = f(t + h, tmp)
        h6 = h / 6
        yn = [y[k] + h6 * (k1[k] + 2 * (k2[k] + k3[k]) + k4[k]) for k in range(n)]
        tn = t1 if s == steps - 1 else t0 + (s + 1) * h
        kn = f(tn, yn)
        if event is not None and event.check(f, sol, t, y, k1, tn, yn, kn):
            return sol
        y, k1 = yn, kn
    if event is not None:
        raise _Fail(ERR_NO_EVENT, t1, float(evtext))
    sol.push(t1, y, k1)
    return sol


RK4_SAMPLES = 8        # step-doubling checks after a fixed-step solve (redteam #5)
RK4_WARN = 1e-3        # warn when the estimated relative error is larger than this


def rk4_error(f, sol):
    """Mirrors fm_rk4_check in codegen_llvm.py.  A cheap error estimate for a fixed-step RK4 solution:
    at RK4_SAMPLES evenly spaced steps s, one RK4 step of 2h from the stored (t_s, y_s) is compared with
    the stored y_{s+2} (two steps of h).  Their difference is 30 times the local error of one h step
    (step doubling, error ~ h⁵); times the number of steps it estimates the global error, relative to
    max(|y_s|, |y_s+2|) + |y_s+2 - y_s| per component (D17's scale-free norm).  Costs 3 × RK4_SAMPLES
    evaluations of the right-hand side."""
    N, n = sol.n, sol.dim
    if N < 3:
        return 0.0
    worst = 0.0
    last = -1
    for k in range(RK4_SAMPLES):
        s = (k * (N - 3)) // (RK4_SAMPLES - 1)
        if s == last:
            continue
        last = s
        t, tm, t2 = sol.t[s], sol.t[s + 1], sol.t[s + 2]
        h = tm - t
        if not (abs((t2 - tm) - h) <= 1e-9 * abs(h)):      # an event cut the last step short
            continue
        H = t2 - t
        half = H * 0.5
        y = sol.y[s * n:(s + 1) * n]
        k1 = sol.dy[s * n:(s + 1) * n]
        tmp = [y[j] + half * k1[j] for j in range(n)]
        k2 = f(t + half, tmp)
        tmp = [y[j] + half * k2[j] for j in range(n)]
        k3 = f(t + half, tmp)
        tmp = [y[j] + H * k3[j] for j in range(n)]
        k4 = f(t + H, tmp)
        H6 = H / 6
        for j in range(n):
            y2 = y[j] + H6 * (k1[j] + 2 * (k2[j] + k3[j]) + k4[j])
            yr = sol.y[(s + 2) * n + j]
            sc = max(abs(y[j]), abs(yr)) + abs(yr - y[j])
            if sc > 0:
                e = abs(y2 - yr) / sc
                if e > worst:
                    worst = e
    return worst / 30 * (N - 1)


A_DP = [[], [1 / 5], [3 / 40, 9 / 40], [44 / 45, -56 / 15, 32 / 9],
        [19372 / 6561, -25360 / 2187, 64448 / 6561, -212 / 729],
        [9017 / 3168, -355 / 33, 46732 / 5247, 49 / 176, -5103 / 18656],
        [35 / 384, 0, 500 / 1113, 125 / 192, -2187 / 6784, 11 / 84]]
C_DP = [0, 1 / 5, 3 / 10, 4 / 5, 8 / 9, 1, 1]
E_DP = [71 / 57600, 0, -71 / 16695, 71 / 1920, -17253 / 339200, 22 / 525, -1 / 40]


def _jdist(u, v, fa, fe):
    """Distance between two right-hand-side values, each component relative to |fa| + |fe|."""
    d = 0.0
    for j in range(len(u)):
        w = abs(fa[j]) + abs(fe[j])
        if w > 0:
            d = d + abs(u[j] - v[j]) / w
    return d


def _find_jump(f, t, tn, y, fa):
    """Is there a jump in f(·, y) (y held fixed) between t and tn, like `if t < 0.3 s then ...`?
    Returns (lo, hi): adjacent times with f(lo) on the near side of the jump and f(hi) on the far side,
    or None.  Mirrors ModuleGen._k_find_jump (D40)."""
    fe = f(tn, y)
    fm = f(t + 0.5 * (tn - t), y)
    dfe = _jdist(fa, fe, fa, fe)
    if not (dfe > 1e-12):
        return None
    mid = [0.5 * (fa[j] + fe[j]) for j in range(len(fa))]
    if not (_jdist(fm, mid, fa, fe) > 0.4 * dfe):       # a jump puts f(midpoint) at one end
        return None
    lo, hi, flo, fhi = t, tn, fa, fe
    for _ in range(200):
        m = lo + 0.5 * (hi - lo)
        if m == lo or m == hi:
            break
        fx = f(m, y)
        if _jdist(fx, fa, fa, fe) <= _jdist(fx, fe, fa, fe):
            lo, flo = m, fx
        else:
            hi, fhi = m, fx
    if _jdist(flo, fhi, fa, fe) > 0.5 * dfe:            # most of the change happens in one rounding step
        return lo, hi
    return None


STIFF_AFTER = 100_000


def _stiff_test(state, count, h, k6, k7, y6, y7, warn):
    """Hairer's stiffness detection for DOPRI5 (mirrors ModuleGen._emit_stiff_test): warn once, via
    warn(count), when 15 checks in a row find h·|λ| ≈ |h|‖k7 − k6‖/‖y7 − y6‖ above 1.8."""
    if count < STIFF_AFTER or not (count % 1000 == 0 or state[0] > 0):
        return
    num = den = 0.0
    for j in range(len(y7)):
        dk = k7[j] - k6[j]
        dy = y7[j] - y6[j]
        num = num + dk * dk
        den = den + dy * dy
    if den > 0 and h * h * num > 1.8 * 1.8 * den:
        state[1] = 0
        state[0] += 1
        if state[0] >= 15:
            state[0] = -1
            if warn is not None:
                warn(float(count))
    else:
        state[1] += 1
        if state[1] >= 6:
            state[0] = 0


def dp45(f, y0, t0, t1, rtol, ev=None, tname=-1, evtext=-1, tdep=False, warn=None, atol=None):
    n = len(y0)
    atol = atol if atol is not None else [0.0] * n      # absolute tolerances per component (D160)
    y = list(y0)
    sol = Sol(n)
    span = t1 - t0
    if not (span != 0):
        raise _Fail(ERR_ODE_RANGE, t0, float(tname))
    dirn = 1.0 if span > 0 else -1.0       # towards smaller t for a decreasing range (D39)
    aspan = abs(span)
    t = t0
    count = 0
    k = [None] * 7
    k[0] = _start(f, t0, y, tname)
    hv = _first_step(f, t0, y, k[0], dirn, aspan, rtol)
    sol.push(t0, y, k[0])
    event = _Event(ev, t0, y) if ev is not None else None
    rej, first_rej = 0, 0.0
    has_tgt, tgt_lo, tgt_hi = False, 0.0, 0.0     # a located jump in f (D40): land exactly on it
    probed = False
    stiff = [0, 0]                         # stiff-looking steps in a row (-1: warned), then non-stiff ones
    tmp6 = None
    while True:
        remaining = dirn * (t1 - t)
        if not (remaining > 1e-14 * abs(t1) and remaining > 0):
            break
        count += 1
        if count > 20_000_000:
            raise _Fail(ERR_ODE_STEPS, t, float(tname))
        stop = tgt_lo if has_tgt else t1
        rstop = dirn * (stop - t)
        land = hv >= rstop
        h = rstop if land else hv
        if h < 1e-15 * (abs(t) + aspan):
            raise _Fail(step_small_kind(y0, y), t, float(tname))
        tn = stop if land else t + dirn * h
        hs = stop - t if land else dirn * h
        ynew = None
        for s in range(1, 7):
            tmp = []
            for j in range(n):
                acc = y[j]
                for m in range(s):
                    if A_DP[s][m] != 0:
                        acc = acc + (hs * A_DP[s][m]) * k[m][j]
                tmp.append(acc)
            if s == 6:
                ynew = tmp
            elif s == 5:
                tmp6 = tmp
            k[s] = f(tn if C_DP[s] == 1 else t + hs * C_DP[s], tmp)
        errsum = 0.0
        for j in range(n):
            e = 0.0
            for m in range(7):
                if E_DP[m] != 0:
                    e = e + E_DP[m] * k[m][j]
            e = e * hs
            sc = rtol * (max(abs(y[j]), abs(ynew[j])) + abs(ynew[j] - y[j])) + atol[j] + 5e-324
            r = fdiv(e, sc)
            errsum = errsum + r * r
        errn = math.sqrt(errsum / n)
        fac = 0.9 * fpow(max(errn, 1e-10), -0.2)
        fac = min(5.0, max(0.2, fac))
        stalled = rej >= 4 and errn >= 0.5 * first_rej
        if errn <= 1.0 or stalled:
            if stiff[0] >= 0:
                _stiff_test(stiff, count, hs, k[5], k[6], tmp6, ynew, warn)
            if event is not None and event.check(f, sol, t, y, k[0], tn, ynew, k[6], k):
                return sol
            t = tn
            y = ynew
            k[0] = k[6]
            sol.push(t, y, k[0])
            hv = h * 2.0 if stalled else h * fac
            if land and has_tgt:           # at the jump: restart on its far side
                has_tgt = False
                t = tgt_hi
                k[0] = f(t, y)
                sol.push(t, y, k[0])
                hv = h
            rej = 0
            probed = False
        else:
            if rej == 0:
                first_rej = errn
            rej += 1
            hv = h * min(fac, 1.0)
            if tdep and not probed and not has_tgt:
                probed = True
                jump = _find_jump(f, t, tn, y, k[0])
                if jump is not None:
                    lo, hi = jump
                    hv = h
                    if lo == t:                # the jump is right at t: k[0] is from the near side
                        t = hi
                        k[0] = f(t, y)
                        sol.push(t, y, k[0])
                        rej = 0
                        probed = False
                    else:
                        has_tgt, tgt_lo, tgt_hi = True, lo, hi
    if event is not None:
        raise _Fail(ERR_NO_EVENT, t1, float(evtext))
    return sol


def _stiff(f, y0, t0, t1, rtol, method, ev, tname, evtext, atol=None):
    """`solve ... using radau` / `using bdf`: the same SciPy stepping as the compiled code (D42)."""
    from .runtime.stiff import StiffFail, stiff_solve
    try:
        ts, ys, dys = stiff_solve(f, y0, t0, t1, rtol, method, ev, float(tname), float(evtext), atol)
    except StiffFail as fl:
        raise _Fail(fl.kind, fl.a, fl.b) from None
    sol = Sol(len(y0))
    sol.t, sol.y, sol.dy = list(ts), list(ys), list(dys)
    return sol


class _Split(Exception):
    def __init__(self, c):
        self.c = c


def _quadfin(f, a, b, rtol, atol, name=-1):
    """Mirrors fm_quadfin: split a finite range at an interior singularity."""
    try:
        return _quadcore(f, 0, a, b, rtol, atol, split=True, name=name)
    except _Split as sp:
        return _quadcore(f, 0, sp.c, b, rtol, atol, name=name) - _quadcore(f, 0, sp.c, a, rtol, atol, name=name)


QUAD_ULPS = 8 * 2.220446049250313e-16       # a NaN panel this narrow (relative) is one point (D45)
QUAD_ROUND = 50 * 2.220446049250313e-16      # an error below this × ∫|f| is rounding (D44)


def _qx(mode, p, q, u):
    """Mirrors ModuleGen._qx: x for the quadrature variable u."""
    if mode == 0:
        L = q - p
        v = 1.0 - u
        return p + L * (u * u * (3.0 - 2.0 * u)) if u <= 0.5 else q - L * (v * v * (1.0 + 2.0 * u))
    sx = q * fdiv(u, 1.0 - u)
    return p + sx if mode == 1 else p - sx


def _quadcore(f, mode, p, q, rtol, atol, split=False, name=-1):
    """Mirrors fm_quadcore / fm_gk15 (D44, D45)."""
    def qf(u):
        if mode == 0:
            L = q - p
            v = 1.0 - u
            x = _qx(0, p, q, u)
            if x == p or x == q:
                return 0.0
            return f(x) * (6.0 * (u * v) * L)
        om = 1.0 - u
        return fdiv(f(_qx(mode, p, q, u)) * q, om * om)

    def gk(lo, hi):
        """(result, error, ∫|g|, u of a NaN/∞ node or NaN, largest finite |g|)"""
        c = 0.5 * (lo + hi)
        h = 0.5 * (hi - lo)
        st = [math.nan, 0.0]

        def node(u):
            g = qf(u)
            if abs(g) < math.inf:
                st[1] = max(st[1], abs(g))
                return g
            if g != g:
                st[0] = u               # a NaN node: +u, wins over a ±∞ node: -u
            elif not (st[0] > 0):
                st[0] = -u
            st[1] = max(st[1], 0.0)
            return 0.0
        fc = node(c)
        resk = fc * WGK[7]
        resg = fc * WG[3]
        rabs = abs(fc) * WGK[7]
        for j in range(7):
            dx = h * XGK[j]
            f1 = node(c - dx)
            f2 = node(c + dx)
            s = f1 + f2
            resk = resk + s * WGK[j]
            rabs = rabs + (abs(f1) + abs(f2)) * WGK[j]
            if j % 2 == 1:
                resg = resg + s * WG[j // 2]
        return resk * h, abs((resk - resg) * h), abs(rabs * h), st[0], st[1]

    def panel(x0, x1, g):
        r, e, ra, badu, gmax = g
        if badu == badu:            # a NaN/∞ node: 0 with an infinite error (D45)
            return [x0, x1, 0.0, math.inf, 0.0, badu, gmax]
        return [x0, x1, r, e, ra, badu, gmax]

    def tiny_x(u0, u1):
        xa, xb = _qx(mode, p, q, u0), _qx(mode, p, q, u1)
        ext = abs(xb - xa)
        return ext <= QUAD_ULPS * max(abs(q - p) if mode == 0 else abs(q), max(abs(xa), abs(xb))) \
            and ext < math.inf

    def exclude_run(wl, wh):
        """Mirrors the run search in fm_quadcore: True if the NaN/∞ panels around [wl, wh] are one point."""
        ends, gs = [], []
        by_end = ({pn[0]: i for i, pn in enumerate(panels)}, {pn[1]: i for i, pn in enumerate(panels)})
        for fr, to, cur in ((1, 0, wl), (0, 1, wh)):
            while True:
                j = by_end[fr].get(cur, -1)        # panel ends are unique: the one panel that ends here
                if j < 0:
                    break
                pn = panels[j]
                if pn[5] == pn[5]:
                    cur = pn[to]
                    continue
                break
            ends.append(cur)
            gs.append(panels[j][6] if j >= 0 else -1.0)
        G = max(gs[0], gs[1])
        if not (G >= 0 and tiny_x(ends[0], ends[1])):
            return False
        for pn in panels:
            if pn[0] >= ends[0] and pn[1] <= ends[1]:
                pn[2], pn[3], pn[4], pn[6] = 0.0, (pn[1] - pn[0]) * G, 0.0, -1.0
        return True

    P, M = 8, 2000
    limit = 250 if atol < 0 else M - 1          # QUAD_SOFT for the quiet first try (D44)
    width = 1.0 / P
    panels = []
    for i in range(P):
        x0 = i * width
        x1 = 1.0 if i == P - 1 else x0 + width
        panels.append(panel(x0, x1, gk(x0, x1)))
    while True:
        total = 0.0
        toterr = 0.0
        totabs = 0.0
        w = 0
        for i, pn in enumerate(panels):
            e = pn[3]
            total = total + pn[2]
            toterr = toterr + e
            totabs = totabs + pn[4]
            if e > panels[w][3] or (e != e):
                w = i
        finite = abs(total) < math.inf
        goal = max(atol, rtol * abs(total)) if atol == atol else rtol * abs(total)
        if finite and (toterr <= goal or toterr <= 1e-14 * abs(total) or toterr <= QUAD_ROUND * totabs):
            _QABS[0] += totabs
            return total
        wl, wh, wbad = panels[w][0], panels[w][1], panels[w][5]
        if wbad == wbad and panels[w][6] >= 0 and tiny_x(wl, wh) and exclude_run(wl, wh):
            continue
        mid = 0.5 * (wl + wh)
        stuck = (wh - wl) <= 1e-13 * max(abs(wl), abs(wh))
        if stuck and finite and toterr <= 1e-7 * abs(total):
            return total
        if split and (len(panels) >= limit or stuck):
            um = 1.0 - mid
            c = p + (q - p) * (mid * mid * (3.0 - 2.0 * mid)) if mid <= 0.5 else q - (q - p) * (um * um * (1.0 + 2.0 * mid))
            if abs(c) <= 1e-9 * (q - p):
                c = 0.0
            if p < c < q:
                raise _Split(c)
        if atol < 0 and (len(panels) >= limit or stuck or toterr != toterr or abs(total) == math.inf):
            return math.nan     # the quiet first try for a component of a vector integral (D44)
        if (len(panels) >= limit or stuck) and wbad == wbad:
            raise _Fail(ERR_QUAD_NAN if wbad > 0 else ERR_QUAD_INF, _qx(mode, p, q, abs(wbad)), float(name))
        if len(panels) >= limit or stuck or toterr != toterr or abs(total) == math.inf:
            raise _Fail(ERR_QUAD, total, toterr)
        g1 = gk(wl, mid)
        g2 = gk(mid, wh)
        panels[w] = panel(wl, mid, g1)
        panels.append(panel(mid, wh, g2))


def _qscan(f, base, sign):
    best, bs, sc = 0.0, 1.0, 1e-40
    for _ in range(321):
        v = abs(f(base + sign * sc)) * sc
        if v > best and v < math.inf:
            best, bs = v, sc
        sc = sc * 10 ** 0.25
    return bs


NOISE = 1e-12        # |lhs - rhs| at most this times |lhs| + |rhs| near the root: rounding noise (#36)


def root(f, a0, b0, scan=200, scale=None, warn=None):
    """Mirrors fm_root in codegen_llvm.py: the FIRST root after a0.  Always scans `scan` sub-intervals
    from a0 for the first sign change (D32, #2), then refines it with Illinois.  scale(x) = |lhs| + |rhs|:
    if lhs - rhs is rounding noise on both sides of the sign change, warn(x) is called (#36)."""
    fa = f(a0)
    if fa == 0:
        return a0
    h = (b0 - a0) / scan
    fprev, found = fa, False
    a = c = fc = 0.0
    fjump, xjump, pole = math.nan, 0.0, math.nan      # a scan point that landed on a pole (redteam #4)
    for i in range(1, scan + 1):
        xi = b0 if i == scan else a0 + i * h
        fi = f(xi)
        if fi == 0:
            if scale is not None and abs(fprev) <= NOISE * scale(xi - h):
                warn(xi)
            return xi
        if abs(fi) == math.inf:       # on a pole: not a crossing; skip past it
            fjump, xjump, fprev = fprev, xi, math.nan
            continue
        if fjump * fi < 0 and pole != pole:
            pole = xjump
        fjump = math.nan
        if fprev * fi < 0:
            a, fa, c, fc, found = xi - h, fprev, xi, fi, True
            break
        fprev = fi
    if not found:
        if pole == pole:
            raise _Fail(ERR_POLE, pole, 0.0)
        raise _Fail(ERR_ROOT, a0, b0)
    if scale is not None and abs(fa) <= NOISE * scale(a) and abs(fc) <= NOISE * scale(c):
        warn(c)
    side = 0
    m0 = max(abs(fa), abs(fc))
    for _ in range(300):
        if abs(c - a) <= 4e-16 * max(abs(a), abs(c)):
            best = a if abs(fa) < abs(fc) else c
            if abs(f(best)) > m0:
                raise _Fail(ERR_POLE, best, 0.0)
            return best
        x = c - fc * (c - a) / (fc - fa)
        if not (min(a, c) < x < max(a, c)):
            x = 0.5 * (a + c)
        fx = f(x)
        if fx == 0:
            return x
        if fx != fx:                  # undefined inside the bracket: not a verified root
            raise _Fail(ERR_POLE, x, 0.0)
        if fx * fc < 0:
            a, fa, side = c, fc, 0
        else:
            if side == 1:
                fa = 0.5 * fa
            side = 1
        c, fc = x, fx
    return c


def _count(nf):
    """Mirrors FuncGen.list_count."""
    if not (nf <= MAX_LIST):
        raise _Fail(ERR_SIZE, nf, 0.0)
    return max(0, int(nf))


_QABS = [0.0]       # fm.qabs (D110)


def quad(f, a, b, rtol=1e-10, atol=0.0, name=-1, warn=None):
    """Mirrors fm_quad: warn() if the result is exactly 0 because the integrand was 0 at every node (D110)."""
    saved = _QABS[0]
    _QABS[0] = 0.0
    try:
        r = _quad_in(f, a, b, rtol, atol, name)
        mine = _QABS[0]
    finally:
        _QABS[0] = saved
    if r == 0 and mine == 0 and a != b and not (atol < 0) and warn is not None:
        warn()
    return r


def _quad_in(f, a, b, rtol=1e-10, atol=0.0, name=-1):
    """Mirrors fm_quadin / fm_quadcore / fm_qscan in codegen_llvm.py."""
    if a > b:
        return -_quad_in(f, b, a, rtol, atol, name)
    if a == b:
        return 0.0
    a_inf, b_inf = abs(a) == math.inf, abs(b) == math.inf
    if not (a_inf or b_inf):
        return _quadfin(f, a, b, rtol, atol, name=name)
    if not a_inf:
        L = _qscan(f, a, 1.0)
        return _quadfin(f, a, a + L, rtol, atol, name=name) + _quadcore(f, 1, a + L, L, rtol, atol, name=name)
    if not b_inf:
        L = _qscan(f, b, -1.0)
        return _quadcore(f, 2, b - L, L, rtol, atol, name=name) + _quadfin(f, b - L, b, rtol, atol, name=name)
    lr, ll = _qscan(f, 0.0, 1.0), _qscan(f, 0.0, -1.0)
    vr, vl = abs(f(lr)) * lr, abs(f(-ll)) * ll
    c = lr if not (vr < vl) else -ll
    return _quadcore(f, 2, c, abs(c), rtol, atol, name=name) + _quadcore(f, 1, c, abs(c), rtol, atol, name=name)


# ---------------------------------------------------------------- the interpreter
class Frame:
    def __init__(self, parent=None):
        self.vars = {}
        self.parent = parent

    def get(self, sym):
        f = self
        while f is not None:
            if sym.id in f.vars:
                return f.vars[sym.id]
            f = f.parent
        raise FermiumRuntimeError(f"{sym.name} is used before it has a value")

    def set(self, sym, v):
        f = self
        while f is not None:
            if sym.id in f.vars:
                f.vars[sym.id] = v
                return
            f = f.parent
        self.vars[sym.id] = v


class Interpreter:
    def __init__(self, module, runtime, globals_frame=None):
        self.mod = module
        self.rt = runtime
        self.globals = globals_frame or Frame()
        self.line = 0
        self.mc = None          # the sampler inside propagate montecarlo (D123)

    def run(self):
        rt = self.rt
        try:
            with np.errstate(all="ignore"):
                self.block(self.mod.main.body, self.globals)
        except _Fail as f:
            rt.error = rt.describe_error(f.kind, f.a, f.b, getattr(f, "fmt", -1))
            rt.error_line = getattr(f, "line", None) or self.line or None
            raise FermiumRuntimeError(rt.error, rt.error_line)
        except RecursionError:
            raise FermiumRuntimeError("the program recursed too deeply", self.line or None)
        except UncertainUse as u:
            rt.error, rt.error_line = u.message, self.line or None
            raise FermiumRuntimeError(u.message, self.line or None)

    # ------------------------------------------------------------ statements
    def block(self, stmts, fr):
        for s in stmts:
            if getattr(s, "line", 0):
                self.line = s.line
            getattr(self, "s_" + type(s).__name__)(s, fr)

    def s_SAssign(self, s, fr):
        v = self.eval(s.value, fr)
        if isinstance(v, list):
            v = v  # lists are shared by reference, like the compiled {pointer, length} pairs
        fr.set(s.sym, v)

    def s_SExpr(self, s, fr):
        self.eval(s.value, fr)

    def s_SIndexAssign(self, s, fr):
        lst = fr.get(s.sym)
        i = self.index(self.eval(s.idx, fr), len(lst))
        lst[i] = self.eval(s.value, fr)

    def s_SPush(self, s, fr):
        v = self.eval(s.value, fr)
        fr.get(s.sym).append(v)

    def s_SIf(self, s, fr):
        if self.eval(s.cond, fr):
            self.block(s.then, fr)
        elif s.other:
            self.block(s.other, fr)

    def s_SWhile(self, s, fr):
        while self.eval(s.cond, fr):
            try:
                self.block(s.body, fr)
            except _Break:
                break
            except _Continue:
                continue

    def s_SFor(self, s, fr):
        lo, hi, st = self.eval(s.lo, fr), self.eval(s.hi, fr), self.eval(s.step, fr)
        if st == 0 or st != st:
            raise _Fail(ERR_STEP, st, 0.0)
        span = fdiv(hi - lo, st)
        if span != span:
            raise _Fail(ERR_RANGE, lo, hi)
        n = max(0, int(math.floor(span + 1e-9) + 1.0)) if abs(span) < math.inf else (2 ** 62 if span > 0 else 0)
        for i in range(n):
            fr.set(s.sym, lo + i * st)
            try:
                self.block(s.body, fr)
            except _Break:
                break
            except _Continue:
                continue

    def s_SForIn(self, s, fr):
        lst = self.eval(s.lst, fr)
        n = len(lst)
        for i in range(n):
            fr.set(s.sym, lst[i])
            try:
                self.block(s.body, fr)
            except _Break:
                break
            except _Continue:
                continue

    def s_SBreak(self, s, fr):
        raise _Break()

    def s_SContinue(self, s, fr):
        raise _Continue()

    def s_SReturn(self, s, fr):
        raise _Return(self.eval(s.value, fr))

    def s_SAssert(self, s, fr):
        if not self.eval(s.cond, fr):
            raise _Fail(ERR_ASSERT, float(s.msg_id), 0.0)

    def s_SPrint(self, s, fr):
        py = self.rt.py
        for kind, payload, fid in s.items:
            if kind == "num":
                py["print_num"](fid, self.eval(payload, fr))
            elif kind == "list":
                v = self.eval(payload, fr)
                py["print_list"](fid, v, len(v))
            elif kind in ("vec", "mvec"):
                v = self.eval(payload, fr)
                py["print_" + kind](fid, list(v), len(v))
            elif kind == "mat":
                py["print_mat"](fid, list(self.eval(payload, fr)), payload.ty.r, payload.ty.c)
            elif kind == "cplx":
                py["print_cplx"](fid, *self.eval(payload, fr))
            elif kind == "bool":
                py["print_bool"](1 if self.eval(payload, fr) else 0)
            elif kind in ("text", "data"):
                py["print_text"](fid)
            elif kind == "textlist":
                v = self.eval(payload, fr)
                py["print_textlist"](v, len(v))
            elif kind == "textvar":
                py["print_text"](self.eval(payload, fr))
        py["print_end"]()

    def guard(self, fn):
        """Wrap a callback (integrand, right-hand side, equation) given to a kernel: an error inside it keeps
        the line where it happened, so the kernel doesn't report it as its own (#13)."""
        def g(*args):
            try:
                return fn(*args)
            except _Fail as f:
                if not hasattr(f, "line"):
                    f.line = self.line
                f.inner = True
                raise
        return g

    def kernel(self, thunk, fmt=-1):
        """Run a kernel (root, ODE, integral).  Its own errors report the line it was called from and its
        display format, whatever lines its callbacks ran (the compiled kernels save the line and format
        at entry the same way)."""
        line0 = self.line
        try:
            return thunk()
        except _Fail as f:
            if not getattr(f, "inner", False) and not hasattr(f, "line"):
                f.line = line0
                f.fmt = fmt
            raise
        finally:
            self.line = line0

    def ode_rhs(self, lam, fr):
        state = lam.state

        @self.guard
        def f(t, y):
            lf = Frame(fr)
            lf.vars[lam.params[0].id] = t
            off = 0
            for sym in state:
                if isinstance(sym.ty, VecTy):
                    lf.vars[sym.id] = tuple(y[off:off + sym.ty.n])
                    off += sym.ty.n
                else:
                    lf.vars[sym.id] = y[off]
                    off += 1
            out = []
            for e in lam.body:
                v = self.eval(e, lf)
                if isinstance(v, tuple):
                    out.extend(v)
                else:
                    out.append(v)
            for v in out:
                if isinstance(v, UFloat):
                    raise UncertainUse("a differential equation (solve) can't use uncertain values (±) yet; put "
                                       "the solve inside a  propagate montecarlo  block, or use value(x)")
                if type(v) is np.ndarray:
                    raise TypeError("vectorized sample in an ODE")
            return out
        return f

    def s_SSolve(self, s, fr):
        if s.method == "eigen":                 # D82
            return self.m3_eigen(s, fr)
        if s.method == "pde":                   # D83
            return self.m3_pde(s, fr)
        y0 = []
        for e in s.y0:
            v = self.eval(e, fr)
            y0.extend(v if isinstance(v, tuple) else [v])
        if any(isinstance(v, UFloat) for v in y0):
            raise UncertainUse("a starting value of solve can't be uncertain (±) yet; put the solve inside a  "
                               "propagate montecarlo  block, or use value(x)")
        if any(type(v) is np.ndarray for v in y0):
            raise TypeError("vectorized starting value")
        f = self.ode_rhs(s.rhs, fr)
        ev = self.ode_rhs(s.event, fr) if getattr(s, "event", None) is not None else None
        t0, t1 = self.eval(s.t0, fr), self.eval(s.t1, fr)
        tname, evtext, fmt = getattr(s, "tname", -1), getattr(s, "evtext", -1), getattr(s, "tfmt", -1)
        atol = abs_tolerances(getattr(s, "atol", None), t0, t1)
        if s.method in ("radau", "bdf"):
            sol = self.kernel(lambda: _stiff(f, y0, t0, t1, s.rtol, s.method, ev, tname, evtext, atol), fmt)
        elif s.method == "rk4":
            h0 = self.eval(s.step, fr)
            sol = self.kernel(lambda: rk4(f, y0, t0, t1, h0, ev, tname, evtext), fmt)
            est = rk4_error(f, sol)
            if est > RK4_WARN:
                self.rt.warn(7, est, self.line, -1)
        else:
            line = self.line
            sol = self.kernel(lambda: dp45(f, y0, t0, t1, s.rtol, ev, tname, evtext, getattr(s, "tdep", False),
                                           lambda c: self.rt.warn(2, c, line, -1), atol), fmt)
        if getattr(s.sol_sym, "needs_rhs", False):
            # mirrors FuncGen.attach_rhs: the numbers the right side reads, as they are now (D46)
            lam = s.rhs
            own = {x.id for x in list(lam.params) + list(lam.state) + list(lam.locals)}
            snap = Frame(fr)
            for x in I.referenced_syms(lam.body):
                if x.id not in own and (x in lam.captures or x.storage in ("global", "arena") and
                                        isinstance(x.ty, (NumTy, BoolTy, VecTy, MatTy))):
                    snap.vars[x.id] = fr.get(x)
            sol.rhs = self.ode_rhs(lam, snap)
        fr.set(s.sol_sym, sol)

    def s_SFit(self, s, fr):
        lam = s.model
        p = [U.nominal(self.eval(g, fr)) if g is not None else math.nan for g in s.guesses] + \
            [math.nan] * len(s.guesses)          # an uncertain starting guess: its value (D124)
        h = self.eval(s.data, fr)
        data = self.rt.datasets[h]

        def model(params):
            out = np.zeros(len(data[0]) if data else 0)
            for i in range(len(out)):
                lf = Frame(fr)
                for sym, v in zip(lam.param_syms, params):
                    lf.vars[sym.id] = float(v)
                for sym in lam.col_syms:
                    lf.vars[sym.id] = float(data[sym.col_index][i])
                v = self.eval(lam.body, lf)
                if isinstance(v, UFloat):
                    raise UncertainUse("a fit model can't use uncertain values (±) other than the parameters being "
                                       "fitted; write value(x) in the model")
                out[i] = v
            return out
        self.rt.fit(s.fit_id, h, p, model_fn=model)
        if self.rt.error:
            raise FermiumRuntimeError(self.rt.error, self.line)
        k = len(s.param_syms)
        vals = [float(v) for v in p[:k]]
        if getattr(self.mod, "uses_unc", False):
            # the fitted parameters carry their standard errors and correlations (D124)
            cov = getattr(self.rt, "last_fit_cov", None)
            ok = all(math.isfinite(float(e)) for e in p[k:2 * k])
            us = U.correlated(vals, cov) if cov is not None and ok else None
            if us is None:
                us = [UFloat.measured(v, e) if math.isfinite(e) else v for v, e in zip(vals, p[k:])]
            vals = us
        for sym, v in zip(s.param_syms, vals):
            fr.set(sym, v)
        for sym, v in zip(getattr(s, "err_syms", []), p[len(s.param_syms):]):
            fr.set(sym, float(v))

    def s_SPlot(self, s, fr):
        py = self.rt.py
        for idx, e in enumerate(s.series):
            kind = e["kind"]
            if kind == "lists":
                y, x = self.eval(e["y"], fr), self.eval(e["x"], fr)
                if U.any_uncertain([x, y]):         # error bars (D124)
                    self.rt.plot_err[(s.plot_id, idx)] = ([U.sigma(v) for v in x], [U.sigma(v) for v in y], "bars")
                    x, y = [U.nominal(v) for v in x], [U.nominal(v) for v in y]
                if py["plot_series"](s.plot_id, idx, x, len(x), y, len(y)):
                    raise FermiumRuntimeError(self.rt.error, self.line)
            elif kind in ("sol", "solxy"):
                sol = self.eval(e["sol"], fr)
                ts, ys = self.sample(sol, e["comp"], e["dy"])
                xs = ts if kind == "sol" else self.sample(sol, e["comp2"], e.get("dy2"))[1]
                self.rt.plot_series.setdefault(s.plot_id, []).append((idx, list(xs), list(ys)))
            elif kind == "func":
                lo, hi = self.eval(e["lo"], fr), self.eval(e["hi"], fr)
                g = self.scalar_fn(e["lam"], fr)
                xs = [lo + i * ((hi - lo) / 399) for i in range(400)]
                ys = [g(x) for x in xs]
                if U.any_uncertain([ys]):            # a band of ±1σ around the curve (D124)
                    self.rt.plot_err[(s.plot_id, idx)] = (None, [U.sigma(v) for v in ys], "band")
                    ys = [U.nominal(v) for v in ys]
                if py["plot_series"](s.plot_id, idx, xs, 400, ys, 400):
                    raise FermiumRuntimeError(self.rt.error, self.line)
        py["plot_done"](s.plot_id)

    def sample(self, sol, comp, use_dy, npts=600):
        from .runtime.core import SolStruct, sample_solution
        import ctypes
        n = sol.n
        arr = lambda v: (ctypes.c_double * max(1, len(v)))(*v)  # noqa: E731
        st = SolStruct(n, sol.dim, n, arr(sol.t), arr(sol.y), arr(sol.dy))
        return sample_solution(st, comp, use_dy, npts)

    def s_SPropagate(self, s, fr):
        """propagate montecarlo (D123): the block runs with every uncertain input replaced by samples.  First all
        samples at once (NumPy arrays through the same code); a block with branches, loops or numerical kernels
        falls back to one sample at a time.  Outputs become mean ± standard deviation, linked to the input sources
        by regression so later formulas keep the correlations."""
        from . import rng
        given = s.n is not None
        n = _count(self.eval(s.n, fr)) if given else MC_DEFAULT
        if n < 2:
            raise FermiumRuntimeError("propagate montecarlo needs at least 2 samples", self.line)
        st = self.rt.rng_state
        zs = {}

        def z(key):
            if key not in zs:
                zs[key] = np.array([rng.randn(st) for _ in range(n)])
            return zs[key]
        assigned = []
        for x in s.body:
            for y in _walk_stmts(x):
                if isinstance(y, I.SAssign) and y.sym not in assigned:
                    assigned.append(y.sym)
        snap = {}
        for sym in assigned:
            try:
                snap[sym.id] = fr.get(sym)
            except FermiumRuntimeError:
                pass

        def restore():
            for sym in assigned:
                if sym.id in snap:
                    fr.set(sym, snap[sym.id])
        line0 = self.line
        results = None
        try:
            restore()
            self.mc = _Sampler(z, None, n)
            self.block(s.body, fr)
            results = [np.broadcast_to(np.asarray(fr.get(sym), dtype=float), (n,)).copy() for sym in s.outs]
        except (_Break, _Continue, _Return):
            raise
        except Exception:          # something that can't run on arrays: one sample at a time
            results = None
        finally:
            self.mc = None
            self.line = line0
        if results is None:
            m = n if given else MC_DEFAULT_SLOW
            cols = [[] for _ in s.outs]
            try:
                for k in range(m):
                    restore()
                    self.mc = _Sampler(z, k, m)
                    self.block(s.body, fr)
                    for c, sym in zip(cols, s.outs):
                        v = fr.get(sym)
                        c.append(float(v) if not isinstance(v, UFloat) else math.nan)
            finally:
                self.mc = None
            results = [np.array(c) for c in cols]
            n = m
        # link the ± inside the block to new sources, shared by all the outputs
        src = {key: (key if isinstance(key, int) else U.new_source()) for key in zs}
        for sym, y in zip(s.outs, results):
            bad = int(np.count_nonzero(~np.isfinite(y)))
            if bad:
                raise FermiumRuntimeError(f"propagate montecarlo: {bad} of the {n} samples of {sym.name} aren't "
                                          f"finite numbers (the formula fails for some sampled inputs)", line0)
            mean = float(np.mean(y))
            var = float(np.var(y, ddof=1))
            if not var > 0:
                fr.set(sym, UFloat(mean, {}))
                continue
            # joint least squares on the sources: an output that is exactly linear in them (c = b + a) gets
            # exact coefficients, so identities like c - b - a = 0 survive; the rest is the nonlinear part
            dy = y - float(np.mean(y))
            keys = list(zs)
            d = {}
            r = dy
            if keys:
                Z = np.column_stack([zs[k][:n] for k in keys])
                Zc = Z - Z.mean(axis=0)
                beta = np.linalg.lstsq(Zc, dy, rcond=None)[0]
                r = dy - Zc @ beta
                # the value: the fitted linear model at z = 0 (the inputs' true values), i.e. the mean corrected
                # with the sources as control variates; exact for a linear formula, less noisy otherwise
                mean -= float(Z.mean(axis=0) @ beta)
                d = {src[k]: float(b) for k, b in zip(keys, beta) if b != 0}
            resid = float(np.dot(r, r)) / (n - 1)
            if resid > 1e-20 * var:
                d[U.new_source()] = math.sqrt(resid)       # the nonlinear part: its own source
            fr.set(sym, UFloat(mean, d))

    # ------------------------------------------------------------ expressions
    def eval(self, e, fr):
        return getattr(self, "e_" + type(e).__name__)(e, fr)

    def e_IConst(self, e, fr):
        return e.value

    def e_IBool(self, e, fr):
        return e.value

    def e_IStr(self, e, fr):
        return getattr(e, "text_id", 0)

    def e_IVar(self, e, fr):
        if self.mc is not None:
            return self.mc.sample(fr.get(e.sym))
        return fr.get(e.sym)

    def e_IVec(self, e, fr):
        return tuple(float(self.eval(x, fr)) for x in e.items)

    def e_IVecElem(self, e, fr):
        return self.eval(e.v, fr)[e.k]

    def e_IVecIndex(self, e, fr):
        v = self.eval(e.v, fr)
        base = 0
        for idx_e, size, stride in e.idxs:
            idx = self.eval(idx_e, fr)
            i = int(idx) if math.isfinite(idx) else -1
            if not (1 <= i <= size) or float(i) != idx:
                raise _Fail(ERR_INDEX, idx, float(-size))
            base += (i - 1) * stride
        xs = tuple(v[base + o] for o in e.offs)
        return xs[0] if len(xs) == 1 else xs

    def e_IBin(self, e, fr):
        a, b = self.eval(e.a, fr), self.eval(e.b, fr)
        op = {"+": lambda x, y: x + y, "-": lambda x, y: x - y, "*": fmul, "/": fdiv}[e.op]
        if isinstance(e.ty, (VecTy, MatTy)):
            n = e.ty.n
            av = a if isinstance(a, tuple) else (a,) * n
            bv = b if isinstance(b, tuple) else (b,) * n
            return tuple(op(x, y) for x, y in zip(av, bv))
        la, lb = isinstance(e.a.ty, ListTy), isinstance(e.b.ty, ListTy)
        if la and lb:
            if len(a) != len(b):
                raise _Fail(ERR_LEN, float(len(a)), float(len(b)))
            return [op(x, y) for x, y in zip(a, b)]
        if la:
            return [op(x, b) for x in a]
        if lb:
            return [op(a, y) for y in b]
        return op(a, b)

    def e_IPowC(self, e, fr):
        a = self.eval(e.a, fr)
        if isinstance(e.a.ty, ListTy):
            return [powc(x, e.p) for x in a]
        return powc(a, e.p)

    def e_IPow(self, e, fr):
        a, b = self.eval(e.a, fr), self.eval(e.b, fr)
        if isinstance(e.a.ty, ListTy):
            return [fpow(x, b) for x in a]
        return fpow(a, b)

    def e_INeg(self, e, fr):
        a = self.eval(e.a, fr)
        if isinstance(a, list):
            return [-x for x in a]
        if isinstance(a, tuple):
            return tuple(-x for x in a)
        return -a

    def e_ICmp(self, e, fr):
        a, b = self.eval(e.a, fr), self.eval(e.b, fr)
        if isinstance(e.a.ty, BoolTy):
            return {"==": a == b, "!=": a != b}[e.op]
        if e.op == "~=":
            return abs(a - b) <= max(abs(a), abs(b)) * 1e-6 + 1e-300
        return {"==": a == b, "!=": a != b, "<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b}[e.op]

    def e_ILogic(self, e, fr):
        if e.op == "and":
            return bool(self.eval(e.a, fr)) and bool(self.eval(e.b, fr))
        return bool(self.eval(e.a, fr)) or bool(self.eval(e.b, fr))

    def e_INot(self, e, fr):
        return not self.eval(e.a, fr)

    def e_IIf(self, e, fr):
        return self.eval(e.a, fr) if self.eval(e.cond, fr) else self.eval(e.b, fr)

    def e_ILet(self, e, fr):
        for sym, v in e.binds:
            fr.set(sym, self.eval(v, fr))
        return self.eval(e.value, fr)

    def call(self, func, args):
        f = Frame(self.globals)
        for p, a in zip(func.params, args):
            f.vars[p.id] = a
        saved = self.line
        self.line = getattr(func, "def_line", None) or saved     # errors inside report their own line (#13)
        try:
            self.block(func.body, f)
        except _Return as r:
            self.line = saved
            return r.value
        raise FermiumRuntimeError(f"the function {func.name} finished without returning a value")

    def e_ICall(self, e, fr):
        return self.call(e.func, [self.eval(a, fr) for a in e.args])

    def e_IMap(self, e, fr):
        args = [self.eval(a, fr) for a in e.args]
        out = []
        for x in args[e.list_pos]:
            a2 = list(args)
            a2[e.list_pos] = x
            out.append(self.call(e.func, a2))
        return out

    def e_IList(self, e, fr):
        return [self.eval(x, fr) for x in e.items]

    def index(self, idx, n):
        i = int(idx) if math.isfinite(idx) else -1
        if not (1 <= i <= n) or float(i) != idx:
            raise _Fail(ERR_INDEX, idx, float(n))
        return i - 1

    def e_IIndex(self, e, fr):
        lst = self.eval(e.lst, fr)
        return lst[self.index(self.eval(e.idx, fr), len(lst))]

    def scalar_fn(self, lam, fr):
        @self.guard
        def g(x):
            lf = Frame(fr)
            lf.vars[lam.params[0].id] = x
            return self.eval(lam.body, lf)
        return g

    def plain_fn(self, f, what):
        """A callback given to a numerical kernel must return plain numbers (D122)."""
        def g(*a):
            r = f(*a)
            if isinstance(r, UFloat):
                raise UncertainUse(f"{what} can't use uncertain values (±) yet; put it inside a  propagate "
                                   f"montecarlo  block, or use value(x)")
            if type(r) is np.ndarray:
                raise TypeError("vectorized sample in a kernel")
            return r
        return g

    def e_IIntegral(self, e, fr):
        f = self.plain_fn(self.scalar_fn(e.lam, fr), "an integral")
        lo, hi = self.eval(e.lo, fr), self.eval(e.hi, fr)
        name = getattr(e, "xname", -1)
        atol = -1.0 if getattr(e, "soft", False) else \
            self.eval(e.atol, fr) if getattr(e, "atol", None) is not None else 0.0
        line = self.line
        return self.kernel(lambda: quad(f, lo, hi, atol=atol, name=name, warn=lambda: self.rt.warn(3, 0.0, line)),
                           getattr(e, "xfmt", -1))

    def e_ISum(self, e, fr):
        f = self.scalar_fn(e.lam, fr)
        lo, hi, st = self.eval(e.lo, fr), self.eval(e.hi, fr), self.eval(e.step, fr)
        if st == 0 or st != st:
            raise _Fail(ERR_STEP, st, 0.0)
        span = fdiv(hi - lo, st)
        if span != span:
            raise _Fail(ERR_RANGE, lo, hi)
        n = max(0, int(math.floor(span + 1e-9) + 1.0)) if abs(span) < math.inf else (2 ** 62 if span > 0 else 0)
        acc = 0.0
        for i in range(n):
            acc = acc + f(lo + i * st)
        return acc

    def e_IRoot(self, e, fr):
        f = self.plain_fn(self.scalar_fn(e.lam, fr), "solve … for x")
        lo, hi = self.eval(e.lo, fr), self.eval(e.hi, fr)
        fmt = getattr(e, "tfmt", -1)
        scale = self.scalar_fn(e.scale, fr) if getattr(e, "scale", None) is not None else None
        line = self.line
        return self.kernel(lambda: root(f, lo, hi, scale=scale,
                                        warn=lambda x: self.rt.warn(1, x, line, fmt)), fmt)

    def e_ISolEval(self, e, fr):
        try:
            sol, t = self.eval(e.sol, fr), self.eval(e.t, fr)
            if isinstance(t, list):          # u(ts): the value at each time in a list (#62)
                return [sol.eval(e.comp, x, e.use_dy) for x in t]
            return sol.eval(e.comp, t, e.use_dy)
        except _Fail as f:
            f.fmt = getattr(e, "tfmt", -1)
            raise

    @staticmethod
    def sol_ext(sol, comp, sg):
        """Mirrors fm_sol_ext in codegen_llvm.py."""
        n, dim = sol.n, sol.dim
        ys = [sg * sol.y[i * dim + comp] for i in range(n)]
        k = 0
        for i in range(1, n):
            if ys[i] > ys[k]:
                k = i
        best = ys[k]
        if n < 3:
            return sg * best
        k = min(max(k, 1), n - 2)
        ks = [k - 1, k, k + 1]
        t = [sol.t[j] for j in ks]
        coef = quintic_hermite(PyOps, t, [ys[j] for j in ks], [sg * sol.dy[j * dim + comp] for j in ks])
        z = [t[0], t[0], t[1], t[1], t[2], t[2]]

        def poly(x):
            acc = coef[5]
            for j in range(4, -1, -1):
                acc = coef[j] + (x - z[j]) * acc
            return acc
        lo, hi = t[0], t[2]
        g = (math.sqrt(5) - 1) / 2
        for _ in range(80):
            x1, x2 = hi - g * (hi - lo), lo + g * (hi - lo)
            if poly(x1) > poly(x2):
                hi = x2
            else:
                lo = x1
        pm = poly(0.5 * (lo + hi))
        return sg * (pm if pm > best or best != best else best)

    def e_ISolList(self, e, fr):
        sol = self.eval(e.sol, fr)
        if e.what == "t":
            return list(sol.t)
        src = sol.y if e.what == "y" else sol.dy
        return [src[i * sol.dim + e.comp] for i in range(sol.n)]

    def e_ILoad(self, e, fr):
        h = self.rt.load(e.load_id)
        if self.rt.error:
            raise FermiumRuntimeError(self.rt.error, self.line)
        return h

    def e_IColumn(self, e, fr):
        return [float(v) for v in self.rt.datasets[self.eval(e.data, fr)][e.col]]

    def matrix_op(self, e, args):
        """Mirrors CodeGen.matrix_op: the same fermium.linalg routines, on floats."""
        name = e.name
        if name == "shuffle":
            return tuple(args[0][k] for k in e.idx)
        ops = linalg.FloatOps
        m = e.args[0].ty
        a = list(args[0])
        if name == "matmul":
            r, k, c = e.dims3
            out = linalg.matmul(ops, a, r, k, list(args[1]), c)
            return out[0] if len(out) == 1 else tuple(out)
        if name == "det":
            return linalg.det(ops, a, m.r)
        if name in ("eigenvalues", "eigenvectors"):     # mirrors CodeGen.eigen_op
            n = m.r
            mats = [a] + [list(x) for x in args[1:]]
            fail = None
            if any(v < 0 for mat in mats for v in linalg.asymmetry(ops, mat, n)):
                fail = ERR_NOT_SYMMETRIC
            elif len(mats) == 1:
                vals, vecs = linalg.jacobi_eigen(ops, a, n)
            else:
                vals, vecs, piv = linalg.generalized_eigen(ops, a, mats[1], n)
                if any(p <= 0 for p in piv):
                    fail = ERR_NOT_POSDEF
            if fail is not None:
                if e.line:
                    self.line = e.line
                raise _Fail(fail)
            return tuple(vals if name == "eigenvalues" else vecs)
        if name == "inverse":
            out, piv = linalg.inverse(ops, a, m.r, 1.0, 0.0)
        else:
            out, piv = linalg.solve(ops, a, m.r, list(args[1]), 1)
        if any(p == 0 for p in piv):
            if e.line:
                self.line = e.line
            if hasattr(e, "sing_t"):          # the mass matrix of an ODE (D47)
                f = _Fail(ERR_ODE_SINGULAR, self._sing_t, float(e.sing_text))
                f.fmt = getattr(e, "sing_fmt", -1)
                raise f
            raise _Fail(ERR_SINGULAR)
        return tuple(out)

    def m3_pde(self, s, fr):
        """solve ∂u/∂t = …: the same Python solver the compiled code calls (runtime/pde.py)."""
        from .runtime.pde import PdeFail, pde_solve
        from .runtime.m3rt import PDE_METHOD_NAMES
        f = self.ode_rhs(s.rhs, fr)
        xa, xb = self.eval(s.xa, fr), self.eval(s.xb, fr)
        t0, t1 = self.eval(s.t0, fr), self.eval(s.t1, fr)
        step = self.eval(s.step, fr) if s.step is not None else None
        line = self.line
        try:
            ts, ys, dys, ncomp, m = self.kernel(
                lambda: pde_solve(f, xa, xb, t0, t1, grid=s.grid, order=s.order, method=PDE_METHOD_NAMES[s.pmethod],
                                  step=step, bc=s.bc, is_complex=s.is_complex, tdep=s.tdep,
                                  warn=lambda kind, est: self.rt.warn(kind, est, line, -1)), getattr(s, "tfmt", -1))
        except PdeFail as ex:
            raise FermiumRuntimeError(ex.message, self.line) from None
        sol = Sol(ncomp * (m + 1))
        sol.t, sol.y, sol.dy = list(ts), list(ys), list(dys)
        fr.set(s.sol_sym, sol)

    def e_IPdeEval(self, e, fr):
        from .runtime.m3rt import pde_eval_py
        sol = self.eval(e.sol, fr)
        xa, xb = self.eval(e.xa, fr), self.eval(e.xb, fr)
        x, t = self.eval(e.x, fr), self.eval(e.t, fr)
        slack = 1e-9 * abs(xb - xa)
        if x < xa - slack or x > xb + slack or x != x:
            f = _Fail(ERR_SOLRANGE, x, xa if x < xa else xb)
            f.fmt = getattr(e, "xfmt", -1)
            raise f
        try:
            return pde_eval_py(sol.eval, xa, xb, e.m, e.comp0, x, t, e.which)
        except _Fail as f:
            f.fmt = getattr(e, "tfmt", -1)
            raise

    def s_SAnimate(self, s, fr):
        from .runtime.m3rt import animate
        sol = self.eval(s.sol, fr)
        animate(self.rt, s.anim_id, sol.t, sol.y, self.eval(s.xa, fr), self.eval(s.xb, fr))

    def m3_eigen(self, s, fr):
        """solve … lowest N: the same Python solver the compiled code calls (runtime/eigen.py)."""
        from .runtime.eigen import EigenFail, eigen_solve, singular_text
        f = self.ode_rhs(s.rhs, fr)
        a, b = self.eval(s.t0, fr), self.eval(s.t1, fr)
        try:
            xs, ys, dys, _ = self.kernel(lambda: eigen_solve(f, a, b, s.nstates, s.grid,
                                                             "shooting" if s.eig_method == 1 else "matrix"),
                                         getattr(s, "tfmt", -1))
        except EigenFail as ex:
            msg = ex.message if ex.x is None else \
                singular_text(self.rt.tname(getattr(s, "tname", -1)), self.rt.fmt_value(ex.x, getattr(s, "tfmt", -1)))
            raise FermiumRuntimeError(msg, self.line) from None
        sol = Sol(3 * s.nstates)
        sol.t, sol.y, sol.dy = list(xs), list(ys), list(dys)
        fr.set(s.sol_sym, sol)

    def m3_fourier(self, name, args):
        """Mirrors codegen_m3.fft / frequencies / argmax (D81)."""
        if name == "frequencies":
            n = _count(args[0])
            if n < 1:
                raise _Fail(ERR_EMPTY)
            return [i / (n * args[1]) for i in range(n // 2 + 1)]
        a = args[0]
        if len(a) < 1:
            raise _Fail(ERR_EMPTY)
        if name in ("argmax", "argmin"):
            best = 0
            for i in range(1, len(a)):
                x, y = a[i], a[best]
                if (x > y if name == "argmax" else x < y) or (y != y and x == x):
                    best = i
            return float(best + 1)
        from .runtime.spectral import spectrum
        kind = {"fft_re": 0, "fft_im": 1, "amplitude_spectrum": 2, "power_spectrum": 3, "ifft": 4}[name]
        if name == "ifft" and len(args[1]) != len(a):
            raise _Fail(ERR_LEN, float(len(a)), float(len(args[1])))
        return spectrum(kind, a, args[1] if name == "ifft" else None, args[1] if name == "power_spectrum" else 1.0)

    def m3_random(self, name, args):
        """The same generator as the compiled code, on the runtime's state (D80)."""
        from . import rng
        st = self.rt.rng_state
        if name == "rand":
            return rng.rand(st)
        if name == "rand2":
            return args[0] + (args[1] - args[0]) * rng.rand(st)
        if name == "randn":
            return rng.randn(st)
        if name == "randn2":
            if args[1] < 0:
                raise _Fail(ERR_NEG_SIGMA, args[1])
            return args[0] + args[1] * rng.randn(st)
        rng.seed(st, args[0])
        return 0.0

    def unc_builtin(self, e, fr):
        """±, value(x), uncertainty(x), rel(x) (D120, D121)."""
        name = e.name
        args = [self.eval(a, fr) for a in e.args]
        if name in ("pm", "pm_rel"):
            v, sg = args
            if isinstance(v, list):
                sgs = sg if isinstance(sg, list) else [sg] * len(v)
                if len(sgs) != len(v):
                    raise _Fail(ERR_LEN, float(len(v)), float(len(sgs)))
                return [self.pm(e, x, y, name == "pm_rel", i) for i, (x, y) in enumerate(zip(v, sgs))]
            return self.pm(e, v, sg, name == "pm_rel", 0)
        a = args[0]
        f = {"unc_value": U.nominal, "unc_uncertainty": U.sigma,
             "unc_rel": lambda x: fdiv(U.sigma(x), abs(U.nominal(x)))}[name]
        if isinstance(a, list):
            return [f(x) for x in a]
        if type(a) is np.ndarray:
            raise TypeError("value/uncertainty of a Monte Carlo sample")
        return f(a)

    def pm(self, e, v, sg, rel, i):
        if isinstance(sg, UFloat):
            sg = sg.v          # the uncertainty of an uncertainty isn't propagated
        if rel:
            sg = abs(U.nominal(v)) * sg
        if type(sg) is np.ndarray:
            raise TypeError("a sampled uncertainty")
        if sg < 0 or sg != sg:
            raise UncertainUse(f"an uncertainty after ± can't be negative or NaN (got {sg:g} in SI units)")
        if self.mc is not None:
            return self.mc.new(e, i, v, sg)
        return v + UFloat.measured(0.0, sg) if isinstance(v, UFloat) else UFloat.measured(v, sg)

    def e_IPyCall(self, e, fr):
        """A call into Python (D140): the same conversion code as the compiled fm_pycall callback."""
        from .runtime.pycall import call, PyCallError
        args = [self.eval(a, fr) for a in e.args]
        try:
            return call(self.rt.tables.pycalls[e.call_id], args, self.rt.base_dir)
        except PyCallError as ex:
            self.rt.error = str(ex)
            raise FermiumRuntimeError(str(ex), self.line or None) from None

    def e_IBuiltin(self, e, fr):
        name = e.name
        if name in ("pm", "pm_rel", "unc_value", "unc_uncertainty", "unc_rel"):
            return self.unc_builtin(e, fr)
        if name in ("min_list", "max_list") and isinstance(e.args[0], I.ISolList) and e.args[0].what == "y":
            return self.sol_ext(self.eval(e.args[0].sol, fr), e.args[0].comp, 1.0 if name == "max_list" else -1.0)
        args = [self.eval(a, fr) for a in e.args]
        if name.startswith("c."):                       # complex numbers (D90): fermium/cplx.py
            from . import cplx
            return cplx.py_builtin(e, args)
        if name in ("shuffle", "matmul", "det", "inverse", "solve_linear", "eigenvalues", "eigenvectors"):
            if hasattr(e, "sing_t"):
                self._sing_t = self.eval(e.sing_t, fr)
            return self.matrix_op(e, args)
        if name in ("vdot", "norm", "unit", "cross"):
            a = args[0]
            if name == "vdot":
                return sum_seq([x * y for x, y in zip(a, args[1])])
            if name == "cross":
                b = args[1]
                if len(a) == 2:
                    return a[0] * b[1] - a[1] * b[0]
                return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
            nrm = math.sqrt(sum_seq([x * x for x in a]))
            return nrm if name == "norm" else tuple(fdiv(x, nrm) for x in a)
        if name in MATH or name in ("round", "sign"):
            if isinstance(args[0], list):
                return [math1(name, x) for x in args[0]]
            return math1(name, args[0])
        if name in ("besselj", "bessely", "besseli", "besselk", "ellipk", "ellipe"):
            if U.any_uncertain(args):
                return U.lift(getattr(special, name), args)
            return getattr(special, name)(*args)
        if name == "isnan":
            return args[0] != args[0]
        if name == "atan2":
            if U.any_uncertain(args):
                return U.lift(math.atan2, args, lambda y, x: (fdiv(x, x * x + y * y), -fdiv(y, x * x + y * y)))
            return math.atan2(*args)
        if name == "hypot":
            if U.any_uncertain(args):
                return U.lift(math.hypot, args, lambda x, y: (fdiv(x, math.hypot(x, y)), fdiv(y, math.hypot(x, y))))
            return math.hypot(*args)
        if name == "mod":
            a, c = args
            return a - c * math.floor(fdiv(a, c)) if c != 0 else math.nan
        if name in ("min_ew", "max_ew"):       # max(xs, 1e-12): element by element (D162)
            lists = [a for a in args if isinstance(a, list)]
            for a in lists[1:]:
                if len(a) != len(lists[0]):
                    raise _Fail(ERR_LEN, float(len(lists[0])), float(len(a)))
            out = []
            for k in range(len(lists[0])):
                vals = [a[k] if isinstance(a, list) else a for a in args]
                r = vals[0]
                for v in vals[1:]:
                    if name == "min_ew":
                        r = v if (v < r or r != r) else r
                    else:
                        r = v if (v > r or r != r) else r
                out.append(r)
            return out
        if name == "min":
            r = args[0]
            for a in args[1:]:
                r = a if (a < r or r != r) else r
            return r
        if name == "max":
            r = args[0]
            for a in args[1:]:
                r = a if (a > r or r != r) else r
            return r
        if name == "clamp":
            return min(max(args[0], args[1]), args[2])
        if name == "factorial":
            return math1("gamma", args[0] + 1)
        if name in ("rand", "rand2", "randn", "randn2", "seed"):
            return self.m3_random(name, args)
        if name in ("fft_re", "fft_im", "ifft", "amplitude_spectrum", "power_spectrum", "frequencies", "argmax",
                    "argmin"):
            return self.m3_fourier(name, args)
        if name == "sample":
            if args[0] < 0 or (args[0] == args[0] and abs(args[0]) < math.inf and args[0] != math.floor(args[0])):
                raise _Fail(ERR_SAMPLE_COUNT, args[0])
            f = self.scalar_fn(e.lam, fr)
            return [f(float(k + 1)) for k in range(_count(args[0]))]
        if name == "clock":
            return time.perf_counter()
        if name == "len":
            return float(len(args[0]))
        if name in ("sum", "mean", "std", "min_list", "max_list", "first", "last"):
            return self.reduce(name, args[0])
        if name == "dot":
            a, c = args
            if len(a) != len(c):
                raise _Fail(ERR_LEN, float(len(a)), float(len(c)))
            acc = 0.0
            for x, y in zip(a, c):
                acc = acc + x * y
            return acc
        if name == "trapz":
            ys, xs = args
            if len(ys) != len(xs):
                raise _Fail(ERR_LEN, float(len(ys)), float(len(xs)))
            acc = 0.0
            for i in range(1, len(ys)):
                acc = acc + 0.5 * ((xs[i] - xs[i - 1]) * (ys[i] + ys[i - 1]))
            return acc
        if name == "interp":
            x, xs, ys = args
            if len(xs) != len(ys):
                raise _Fail(ERR_LEN, float(len(xs)), float(len(ys)))
            if len(xs) < 2:
                raise _Fail(ERR_EMPTY)
            res = ys[0]
            if x >= xs[-1]:
                res = ys[-1]
            for i in range(1, len(xs)):
                if xs[i - 1] <= x < xs[i]:
                    s = (x - xs[i - 1]) / (xs[i] - xs[i - 1])
                    res = ys[i - 1] + s * (ys[i] - ys[i - 1])
            return res
        if name in ("zeros", "ones"):
            n = _count(args[0])
            return [0.0 if name == "zeros" else 1.0] * n
        if name == "linspace":
            a, c, nf = args
            n = _count(nf)
            den = float(n - 1 if n >= 2 else 1)
            step = (c - a) / den
            return [a + i * step for i in range(n)]
        if name == "range":
            a, c, st = args
            if st == 0 or st != st:
                raise _Fail(ERR_STEP, st, 0.0)
            cnt = math.floor(fdiv(c - a, st) + 1e-9) + 1.0 if abs(fdiv(c - a, st)) < math.inf else fdiv(c - a, st)
            n = _count(max(cnt, 0.0))
            return [a + i * st for i in range(n)]
        if name == "copy":
            return list(args[0])
        if name == "slice":        # mirrors codegen_llvm (D114)
            lst, lo, hi = args
            if hi == lo - 1:
                return []
            if hi < lo - 1:
                raise _Fail(ERR_ASSERT, float(e.msg_id), 0.0)
            i = self.index(lo, len(lst))
            j = self.index(hi, len(lst))
            return lst[i:j + 1]
        if name == "reverse":
            return list(reversed(args[0]))
        if name == "sort":
            return sorted(args[0], key=lambda v: (v != v, v if v == v else 0.0))    # NaN last
        if name == "cumsum":
            out, acc = [], 0.0
            for x in args[0]:
                acc = acc + x
                out.append(acc)
            return out
        if name == "diff":
            a = args[0]
            return [a[i + 1] - a[i] for i in range(len(a) - 1)]
        raise FermiumRuntimeError(f"the interpreter doesn't know the built-in {name}")

    def reduce(self, name, lst):
        n = len(lst)
        if name in ("mean", "std", "min_list", "max_list", "first", "last") and n < 1:
            raise _Fail(ERR_EMPTY)
        if name == "std" and n < 2:            # the N − 1 sample std of one value is 0/0 (red team round 2 #4)
            raise _Fail(ERR_STD_ONE)
        if name == "first":
            return lst[0]
        if name == "last":
            return lst[-1]
        if name in ("min_list", "max_list"):
            r = lst[0]
            for x in lst[1:]:
                if name == "min_list":
                    r = x if (x < r or r != r) else r
                else:
                    r = x if (x > r or r != r) else r
            return r
        s = sum_seq(lst)
        if name == "sum":
            return s
        mean = s / n
        if name == "mean":
            return mean
        acc = 0.0
        for x in lst:
            d = x - mean
            acc = acc + d * d
        return math.sqrt(acc / (n - 1))


MC_DEFAULT = 100_000          # samples when the block runs vectorized
MC_DEFAULT_SLOW = 10_000      # ... and when it has to run one sample at a time


class _Sampler:
    """Inside propagate montecarlo: uncertain values read from variables become samples (D123).
    k is None: all n samples as a NumPy array; else the k-th sample as a float."""

    def __init__(self, z, k, n):
        self.z, self.k, self.n = z, k, n

    def sample(self, v):
        if isinstance(v, UFloat):
            if self.k is None:
                out = np.full(self.n, v.v)
                for key, c in v.d.items():
                    out = out + c * self.z(key)
                return out
            return v.v + math.fsum(c * self.z(key)[self.k] for key, c in v.d.items())
        if isinstance(v, list) and any(isinstance(x, UFloat) for x in v):
            return [self.sample(x) for x in v]
        return v

    def new(self, e, i, v, sg):
        """a ± σ inside the block: a new source, the same one for every sample of this ± (and list element)."""
        zk = self.z(("pm", id(e), i))
        v = self.sample(v)
        return v + sg * (zk if self.k is None else zk[self.k])


def _walk_stmts(st):
    yield st
    for attr in ("then", "other", "body"):
        sub = getattr(st, attr, None)
        if isinstance(sub, list):
            for x in sub:
                if isinstance(x, I.Stmt):
                    yield from _walk_stmts(x)


def sum_seq(xs):
    """Left-to-right summation (the same order as the compiled loops, unlike Python's fsum)."""
    acc = 0.0
    first = True
    for x in xs:
        if first:
            acc, first = x, False
        else:
            acc = acc + x
    return acc


def run_interpreted(source, filename="<program>", out=None, base_dir=None, diags=None, show_warnings=False,
                    err=None):
    """Compile to IR and run it with the interpreter (no LLVM, so it also runs in Pyodide).

    Pass a Diagnostics object as `diags` to see the warnings afterwards, or show_warnings=True to have the
    compile-time warnings written to `err` (stderr) before the program runs, as `driver.run_source` does for
    the JIT (`fermium run --interp`, red team round 2 #6)."""
    import os
    import sys
    from .checker import Checker
    from .tables import finalize_tables
    from .errors import Diagnostics
    from .parser import parse
    from .runtime.core import Runtime
    out = out or sys.stdout
    base_dir = base_dir or (os.path.dirname(os.path.abspath(filename)) if not filename.startswith("<")
                            else os.getcwd())
    d = diags if diags is not None else Diagnostics()
    err = err or sys.stderr

    def show():
        if show_warnings:
            for w in d.warnings:
                err.write(w.format(source, None) + "\n")
    try:
        prog = parse(source, d)
        ck = Checker(d, base_dir)
        mod = ck.check_program(prog)
    except FermiumError:
        show()                   # the warnings collected before the error often explain it
        raise
    show()
    finalize_tables(mod.tables, ck.U)
    rt = Runtime(out, base_dir)
    rt.err = err
    rt.tables = mod.tables
    Interpreter(mod, rt).run()
    if rt.line:
        out.write(" ".join(rt.line) + "\n")
    return rt
