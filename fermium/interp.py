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
from .errors import FermiumRuntimeError
from .numerics import PyOps, quintic_hermite, odd_root_numerator, XGK, WGK, WG
from .types import ListTy, VecTy, BoolTy

ERR_INDEX, ERR_SOLRANGE, ERR_ODE_STEPS, ERR_ASSERT, ERR_LEN, ERR_EMPTY, ERR_STEP, ERR_ODE_H = 1, 2, 3, 4, 5, 6, 7, 8
ERR_QUAD = 9
ERR_SIZE, ERR_RANGE, MAX_LIST = 11, 12, 1e9


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


def math1(name, x):
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
        if t < tfirst - slack or t > tlast + slack or t != t:
            raise _Fail(ERR_SOLRANGE, t, tlast)
        lo, hi = 0, n - 1
        while hi - lo > 1:
            mid = (lo + hi) // 2
            if self.t[mid] <= t:
                lo = mid
            else:
                hi = mid
        i = lo
        if n <= 1:
            return (self.dy if use_dy else self.y)[comp]
        ta, tb = self.t[i], self.t[i + 1]
        h = tb - ta
        s = (t - ta) / h
        ia, ib = i * dim + comp, (i + 1) * dim + comp
        ya, yb = self.y[ia], self.y[ib]
        ma, mb = self.dy[ia] * h, self.dy[ib] * h
        s2 = s * s
        s3 = s2 * s
        if use_dy:
            d00 = 6 * s2 - 6 * s
            d10 = 3 * s2 - 4 * s + 1
            d01 = 6 * s - 6 * s2
            d11 = 3 * s2 - 2 * s
            return ((d00 * ya + d10 * ma) + (d01 * yb + d11 * mb)) / h
        h00 = 2 * s3 - 3 * s2 + 1
        h10 = s3 - 2 * s2 + s
        h01 = -2 * s3 + 3 * s2
        h11 = s3 - s2
        return h00 * ya + h10 * ma + (h01 * yb + h11 * mb)


def rk4(f, y0, t0, t1, h0):
    span = t1 - t0
    ratio = fdiv(span, h0)
    if ratio != ratio or ratio <= 0 or ratio > 1e12:
        raise _Fail(ERR_STEP, h0, span)
    steps = int(math.ceil(ratio - 1e-9))
    steps = max(1, steps)
    h = span / steps
    n = len(y0)
    sol = Sol(n)
    y = list(y0)
    half = h * 0.5
    for s in range(steps):
        t = t0 + s * h
        k1 = f(t, y)
        sol.push(t, y, k1)
        tmp = [y[k] + half * k1[k] for k in range(n)]
        th = t + half
        k2 = f(th, tmp)
        tmp = [y[k] + half * k2[k] for k in range(n)]
        k3 = f(th, tmp)
        tmp = [y[k] + h * k3[k] for k in range(n)]
        k4 = f(t + h, tmp)
        h6 = h / 6
        y = [y[k] + h6 * (k1[k] + 2 * (k2[k] + k3[k]) + k4[k]) for k in range(n)]
    sol.push(t1, y, f(t1, y))
    return sol


A_DP = [[], [1 / 5], [3 / 40, 9 / 40], [44 / 45, -56 / 15, 32 / 9],
        [19372 / 6561, -25360 / 2187, 64448 / 6561, -212 / 729],
        [9017 / 3168, -355 / 33, 46732 / 5247, 49 / 176, -5103 / 18656],
        [35 / 384, 0, 500 / 1113, 125 / 192, -2187 / 6784, 11 / 84]]
C_DP = [0, 1 / 5, 3 / 10, 4 / 5, 8 / 9, 1, 1]
E_DP = [71 / 57600, 0, -71 / 16695, 71 / 1920, -17253 / 339200, 22 / 525, -1 / 40]


def dp45(f, y0, t0, t1, rtol):
    n = len(y0)
    y = list(y0)
    ymax = [abs(v) for v in y]
    sol = Sol(n)
    span = t1 - t0
    if span <= 0:
        raise _Fail(ERR_STEP, span, 0.0)
    t = t0
    hv = span * 1e-4
    count = 0
    k = [None] * 7
    k[0] = f(t0, y)
    sol.push(t0, y, k[0])
    rej, first_rej = 0, 0.0
    while True:
        remaining = t1 - t
        if not (remaining > 1e-14 * abs(t1) and remaining > 0):
            break
        count += 1
        if count > 20_000_000:
            raise _Fail(ERR_ODE_STEPS, t, 0.0)
        h = min(hv, remaining)
        if h < 1e-15 * (abs(t) + abs(span)):
            raise _Fail(ERR_ODE_H, t, h)
        ynew = None
        for s in range(1, 7):
            tmp = []
            for j in range(n):
                acc = y[j]
                for m in range(s):
                    if A_DP[s][m] != 0:
                        acc = acc + (h * A_DP[s][m]) * k[m][j]
                tmp.append(acc)
            if s == 6:
                ynew = tmp
            k[s] = f(t + h * C_DP[s], tmp)
        errsum = 0.0
        for j in range(n):
            e = 0.0
            for m in range(7):
                if E_DP[m] != 0:
                    e = e + E_DP[m] * k[m][j]
            e = e * h
            sc = rtol * (max(abs(y[j]), abs(ynew[j])) + abs(ynew[j] - y[j])) + 5e-324
            r = fdiv(e, sc)
            errsum = errsum + r * r
        errn = math.sqrt(errsum / n)
        fac = 0.9 * fpow(max(errn, 1e-10), -0.2)
        fac = min(5.0, max(0.2, fac))
        stalled = rej >= 4 and errn >= 0.5 * first_rej
        if errn <= 1.0 or stalled:
            t = t + h
            y = ynew
            ymax = [max(ymax[j], abs(y[j])) for j in range(n)]
            k[0] = k[6]
            sol.push(t, y, k[0])
            hv = h * 2.0 if stalled else h * fac
            rej = 0
        else:
            if rej == 0:
                first_rej = errn
            rej += 1
            hv = h * min(fac, 1.0)
    return sol


class _Split(Exception):
    def __init__(self, c):
        self.c = c


def _quadfin(f, a, b, rtol, atol):
    """Mirrors fm_quadfin: split a finite range at an interior singularity."""
    try:
        return _quadcore(f, 0, a, b, rtol, atol, split=True)
    except _Split as sp:
        return _quadcore(f, 0, sp.c, b, rtol, atol) - _quadcore(f, 0, sp.c, a, rtol, atol)


def _quadcore(f, mode, p, q, rtol, atol, split=False):
    def qf(u):
        if mode == 0:
            L = q - p
            v = 1.0 - u
            x = p + L * (u * u * (3.0 - 2.0 * u)) if u <= 0.5 else q - L * (v * v * (1.0 + 2.0 * u))
            if x == p or x == q:
                return 0.0
            return f(x) * (6.0 * (u * v) * L)
        om = 1.0 - u
        sx = q * fdiv(u, om)
        return fdiv(f(p + sx if mode == 1 else p - sx) * q, om * om)

    def gk(lo, hi):
        c = 0.5 * (lo + hi)
        h = 0.5 * (hi - lo)
        fc = qf(c)
        resk = fc * WGK[7]
        resg = fc * WG[3]
        for j in range(7):
            dx = h * XGK[j]
            s = qf(c - dx) + qf(c + dx)
            resk = resk + s * WGK[j]
            if j % 2 == 1:
                resg = resg + s * WG[j // 2]
        return resk * h, abs((resk - resg) * h)

    P, M = 8, 2000
    width = 1.0 / P
    panels = []
    for i in range(P):
        x0 = i * width
        x1 = 1.0 if i == P - 1 else x0 + width
        r, e = gk(x0, x1)
        panels.append([x0, x1, r, e])
    while True:
        total = 0.0
        toterr = 0.0
        w = 0
        for i, (_, _, r, e) in enumerate(panels):
            total = total + r
            toterr = toterr + e
            if e > panels[w][3] or (e != e):
                w = i
        finite = abs(total) < math.inf
        goal = max(atol, rtol * abs(total)) if atol == atol else rtol * abs(total)
        if finite and (toterr <= goal or toterr <= 1e-14 * abs(total)):
            return total
        wl, wh = panels[w][0], panels[w][1]
        mid = 0.5 * (wl + wh)
        stuck = (wh - wl) <= 1e-13 * max(abs(wl), abs(wh))
        if stuck and finite and toterr <= 1e-7 * abs(total):
            return total
        if split and (len(panels) >= M - 1 or stuck):
            um = 1.0 - mid
            c = p + (q - p) * (mid * mid * (3.0 - 2.0 * mid)) if mid <= 0.5 else q - (q - p) * (um * um * (1.0 + 2.0 * mid))
            if abs(c) <= 1e-9 * (q - p):
                c = 0.0
            if p < c < q:
                raise _Split(c)
        if len(panels) >= M - 1 or stuck or toterr != toterr or abs(total) == math.inf:
            raise _Fail(ERR_QUAD, total, toterr)
        r1, e1 = gk(wl, mid)
        r2, e2 = gk(mid, wh)
        panels[w] = [wl, mid, r1, e1]
        panels.append([mid, wh, r2, e2])


def _qscan(f, base, sign):
    best, bs, sc = 0.0, 1.0, 1e-40
    for _ in range(321):
        v = abs(f(base + sign * sc)) * sc
        if v > best and v < math.inf:
            best, bs = v, sc
        sc = sc * 10 ** 0.25
    return bs


def _count(nf):
    """Mirrors FuncGen.list_count."""
    if not (nf <= MAX_LIST):
        raise _Fail(ERR_SIZE, nf, 0.0)
    return max(0, int(nf))


def quad(f, a, b, rtol=1e-10, atol=0.0):
    """Mirrors fm_quad / fm_quadcore / fm_qscan in codegen_llvm.py."""
    if a > b:
        return -quad(f, b, a, rtol, atol)
    if a == b:
        return 0.0
    a_inf, b_inf = abs(a) == math.inf, abs(b) == math.inf
    if not (a_inf or b_inf):
        return _quadfin(f, a, b, rtol, atol)
    if not a_inf:
        L = _qscan(f, a, 1.0)
        return _quadfin(f, a, a + L, rtol, atol) + _quadcore(f, 1, a + L, L, rtol, atol)
    if not b_inf:
        L = _qscan(f, b, -1.0)
        return _quadcore(f, 2, b - L, L, rtol, atol) + _quadfin(f, b - L, b, rtol, atol)
    lr, ll = _qscan(f, 0.0, 1.0), _qscan(f, 0.0, -1.0)
    vr, vl = abs(f(lr)) * lr, abs(f(-ll)) * ll
    c = lr if not (vr < vl) else -ll
    return _quadcore(f, 2, c, abs(c), rtol, atol) + _quadcore(f, 1, c, abs(c), rtol, atol)


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

    def run(self):
        rt = self.rt
        try:
            with np.errstate(all="ignore"):
                self.block(self.mod.main.body, self.globals)
        except _Fail as f:
            rt.error = rt.describe_error(f.kind, f.a, f.b, getattr(f, "fmt", -1))
            rt.error_line = self.line or None
            raise FermiumRuntimeError(rt.error, rt.error_line)
        except RecursionError:
            raise FermiumRuntimeError("the program recursed too deeply", self.line or None)

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
            elif kind == "vec":
                v = self.eval(payload, fr)
                py["print_vec"](fid, list(v), len(v))
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

    def ode_rhs(self, lam, fr):
        state = lam.state

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
            return out
        return f

    def s_SSolve(self, s, fr):
        y0 = []
        for e in s.y0:
            v = self.eval(e, fr)
            y0.extend(v if isinstance(v, tuple) else [v])
        f = self.ode_rhs(s.rhs, fr)
        t0, t1 = self.eval(s.t0, fr), self.eval(s.t1, fr)
        if s.method == "rk4":
            sol = rk4(f, y0, t0, t1, self.eval(s.step, fr))
        else:
            sol = dp45(f, y0, t0, t1, s.rtol)
        fr.set(s.sol_sym, sol)

    def s_SFit(self, s, fr):
        lam = s.model
        p = [self.eval(g, fr) if g is not None else math.nan for g in s.guesses]
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
                out[i] = self.eval(lam.body, lf)
            return out
        self.rt.fit(s.fit_id, h, p, model_fn=model)
        if self.rt.error:
            raise FermiumRuntimeError(self.rt.error, self.line)
        for sym, v in zip(s.param_syms, p):
            fr.set(sym, float(v))

    def s_SPlot(self, s, fr):
        py = self.rt.py
        for idx, e in enumerate(s.series):
            kind = e["kind"]
            if kind == "lists":
                y, x = self.eval(e["y"], fr), self.eval(e["x"], fr)
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
        return fr.get(e.sym)

    def e_IVec(self, e, fr):
        return tuple(float(self.eval(x, fr)) for x in e.items)

    def e_IVecElem(self, e, fr):
        return self.eval(e.v, fr)[e.k]

    def e_IBin(self, e, fr):
        a, b = self.eval(e.a, fr), self.eval(e.b, fr)
        op = {"+": lambda x, y: x + y, "-": lambda x, y: x - y, "*": fmul, "/": fdiv}[e.op]
        if isinstance(e.ty, VecTy):
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
        try:
            self.block(func.body, f)
        except _Return as r:
            return r.value
        finally:
            self.line = saved
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
        def g(x):
            lf = Frame(fr)
            lf.vars[lam.params[0].id] = x
            return self.eval(lam.body, lf)
        return g

    def e_IIntegral(self, e, fr):
        return quad(self.scalar_fn(e.lam, fr), self.eval(e.lo, fr), self.eval(e.hi, fr))

    def e_ISolEval(self, e, fr):
        try:
            return self.eval(e.sol, fr).eval(e.comp, self.eval(e.t, fr), e.use_dy)
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

    def e_IBuiltin(self, e, fr):
        name = e.name
        if name in ("min_list", "max_list") and isinstance(e.args[0], I.ISolList) and e.args[0].what == "y":
            return self.sol_ext(self.eval(e.args[0].sol, fr), e.args[0].comp, 1.0 if name == "max_list" else -1.0)
        args = [self.eval(a, fr) for a in e.args]
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
        if name == "isnan":
            return args[0] != args[0]
        if name == "atan2":
            return math.atan2(*args)
        if name == "hypot":
            return math.hypot(*args)
        if name == "mod":
            a, c = args
            return a - c * math.floor(fdiv(a, c)) if c != 0 else math.nan
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
        if name == "rand":
            import random
            return random.random()
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
        return math.sqrt(acc / (n - 1 if n >= 2 else 1))


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


def run_interpreted(source, filename="<program>", out=None, base_dir=None, diags=None):
    """Compile to IR and run it with the interpreter (no LLVM, so it also runs in Pyodide).

    Pass a Diagnostics object as `diags` to see the warnings afterwards."""
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
    prog = parse(source, d)
    ck = Checker(d, base_dir)
    mod = ck.check_program(prog)
    finalize_tables(mod.tables, ck.U)
    rt = Runtime(out, base_dir)
    rt.tables = mod.tables
    Interpreter(mod, rt).run()
    if rt.line:
        out.write(" ".join(rt.line) + "\n")
    return rt
