"""Special functions: Bessel functions of whole-number order and complete elliptic integrals (#50).

besselj(n, x), bessely(n, x): the C library's jn and yn (the native code calls them directly; the
reference interpreter calls the same functions through ctypes when it can, so both agree to the bit).

besseli(n, x): the power series Σ (x/2)^(2k+n) / (k! (k+n)!), whose terms are all positive, so there is no
cancellation (about x/2 + 30 terms).
besselk(n, x): the trapezoidal rule on K_n(x) = ∫₀^∞ exp(−x cosh t) cosh(n t) dt, whose integrand is
positive and analytic in a strip, so the rule converges geometrically in the step (step h =
min(0.1, 0.5/(x² + n²)^¼), half the width of the integrand's peak; 1e-13 against SciPy).

ellipk(m), ellipe(m): the arithmetic-geometric mean with the parameter m = k² (as in SciPy and
Abramowitz & Stegun 17.6), a fixed 12 steps (enough for 1 − m down to 1e-300), written against an
`ops` object like fermium.linalg so the native code and the interpreter run the same operations.
"""
from __future__ import annotations

import math

AGM_STEPS = 12


def ellip_ops(ops, m):
    """(K(m), E(m)) by the AGM (K(1) = ∞, E(1) = 1 set directly): K = π / (2 AGM(1, √(1 − m))), E = K (1 − Σ 2^(j−1) c_j²), c_0² = m."""
    a = ops.const(1.0)
    b = ops.sqrt(ops.sub(ops.const(1.0), m))
    s = ops.mul(ops.const(0.5), m)                     # Σ 2^(j-1) c_j², the j = 0 term
    w = ops.const(0.5)
    for _ in range(AGM_STEPS):
        c = ops.mul(ops.const(0.5), ops.sub(a, b))
        a, b = ops.mul(ops.const(0.5), ops.add(a, b)), ops.sqrt(ops.mul(a, b))
        w = ops.mul(ops.const(2.0), w)
        s = ops.add(s, ops.mul(w, ops.mul(c, c)))
    k = ops.div(ops.const(math.pi / 2), a)
    e = ops.mul(k, ops.sub(ops.const(1.0), s))
    one = ops.eq(m, ops.const(1.0))                    # E(1) = 1 (K(1) = ∞, and ∞·0 would be NaN)
    return ops.select(one, ops.const(math.inf), k), ops.select(one, ops.const(1.0), e)


class PyOps:
    @staticmethod
    def const(x):
        return float(x)

    @staticmethod
    def add(a, b):
        return a + b

    @staticmethod
    def sub(a, b):
        return a - b

    @staticmethod
    def mul(a, b):
        return a * b

    @staticmethod
    def div(a, b):
        if b == 0:
            return math.copysign(math.inf, a) if a != 0 else math.nan
        return a / b

    @staticmethod
    def sqrt(a):
        return math.sqrt(a) if a >= 0 else math.nan

    @staticmethod
    def eq(a, b):
        return a == b

    @staticmethod
    def select(c, a, b):
        return a if c else b


def ellipk(m):
    return ellip_ops(PyOps, m)[0]


def ellipe(m):
    return ellip_ops(PyOps, m)[1]


# ---------------------------------------------------------------- J and Y
_LIBM = None


def _libm():
    global _LIBM
    if _LIBM is None:
        _LIBM = False
        try:
            import ctypes
            import ctypes.util
            name = ctypes.util.find_library("m")
            lib = ctypes.CDLL(name) if name else ctypes.CDLL(None)
            for f in ("jn", "yn"):
                fn = getattr(lib, f)
                fn.restype = ctypes.c_double
                fn.argtypes = [ctypes.c_int, ctypes.c_double]
            _LIBM = lib
        except Exception:          # no C library (the browser playground): see _bessel_fallback
            _LIBM = False
    return _LIBM


def order_ok(n):
    return math.isfinite(n) and n == math.floor(n) and abs(n) < 2 ** 31


def besselj(n, x):
    if not order_ok(n):
        return math.nan
    lib = _libm()
    if lib:
        return float(lib.jn(int(n), x))
    return _bessel_fallback("jv", int(n), x)


def bessely(n, x):
    if not order_ok(n):
        return math.nan
    lib = _libm()
    if lib:
        return float(lib.yn(int(n), x))
    return _bessel_fallback("yn", int(n), x)


def _bessel_fallback(kind, n, x):
    """Without a C library (the browser playground): SciPy, else mpmath, else the integral."""
    try:
        import scipy.special as sc
        return float(getattr(sc, kind)(n, x))
    except ImportError:
        pass
    try:
        import mpmath
        return float(mpmath.besselj(n, x) if kind == "jv" else mpmath.bessely(n, x))
    except ImportError:
        pass
    if kind != "jv":
        raise RuntimeError("bessely needs SciPy here")
    # J_n(x) = (1/π) ∫₀^π cos(nτ − x sin τ) dτ, trapezoidal rule (exact up to rounding for enough points)
    N = int(2 * (abs(x) + abs(n))) + 40
    h = math.pi / N
    s = 0.5 * (1.0 + math.cos(n * math.pi))
    for k in range(1, N):
        t = k * h
        s += math.cos(n * t - x * math.sin(t))
    return s / N


# ---------------------------------------------------------------- I and K
I_MAX_TERMS = 100000


def besseli(n, x):
    if not order_ok(n) or x != x:
        return math.nan
    n = abs(int(n))
    h = 0.5 * x
    t = 1.0
    for k in range(1, n + 1):          # (x/2)^n / n!
        t = t * h / k
    s = t
    q = h * h
    k = 1
    while k < I_MAX_TERMS:
        t = t * q / (k * (k + n))
        s = s + t
        if abs(t) <= 1e-17 * abs(s):
            break
        k += 1
    return s


K_MAX_STEPS = 100000


def _exp(a):
    try:
        return math.exp(a)
    except OverflowError:
        return math.inf


def besselk_step(n, x):
    return min(0.1, 0.5 / math.sqrt(math.sqrt(x * x + n * n)))


def besselk(n, x):
    if not order_ok(n) or x != x or x < 0:
        return math.nan
    if x == 0:
        return math.inf
    n = abs(int(n))
    h = besselk_step(n, x)
    peak = math.asinh(n / x)                # where n t − x cosh t is largest
    s = 0.5 * math.exp(-x)                   # t = 0, halved (trapezoid end)
    k = 1
    while k < K_MAX_STEPS:
        t = k * h
        ch = math.cosh(t) if t < 700 else math.inf
        term = 0.5 * (_exp(n * t - x * ch) + _exp(-n * t - x * ch))
        s = s + term
        if (t > peak and term <= 1e-18 * s) or s == math.inf:
            break
        k += 1
    return s * h


# ---------------------------------------------------------------- native code (the same algorithms)
def _ll_order_ok(mg, b, n):
    from llvmlite import ir
    F64 = ir.DoubleType()
    whole = b.fcmp_ordered("==", n, b.call(mg.intrinsic("floor"), [n]))
    small = b.fcmp_ordered("<", b.call(mg.intrinsic("fabs"), [n]), ir.Constant(F64, 2.0 ** 31))
    return b.and_(whole, small)


def ll_jn_yn(gen, which, n, x):
    """besselj / bessely at a call site: libm's jn / yn, NaN for an order that isn't a whole number."""
    from llvmlite import ir
    F64, I32 = ir.DoubleType(), ir.IntType(32)
    b, mg = gen.b, gen.mg
    fn = mg.extern(which, F64, [I32, F64])
    ok = _ll_order_ok(mg, b, n)
    ni = b.fptosi(b.select(ok, n, ir.Constant(F64, 0.0)), I32)
    return b.select(ok, b.call(fn, [ni, x]), ir.Constant(F64, math.nan))


def ll_ellip(gen, which, m):
    from .codegen_llvm import LLOps
    k, e = ellip_ops(LLOps(gen), m)
    return k if which == "ellipk" else e


def _new_kernel(mg, name):
    from llvmlite import ir
    F64 = ir.DoubleType()
    fn = ir.Function(mg.module, ir.FunctionType(F64, [F64, F64]), name)
    fn.linkage = "internal"
    b = ir.IRBuilder(fn.append_basic_block("e"))
    return fn, b


def build_besseli(mg):
    """fm_besseli(n, x): the series of besseli() above, instruction for instruction."""
    from llvmlite import ir
    F64, I64 = ir.DoubleType(), ir.IntType(64)

    def f(v):
        return ir.Constant(F64, v)
    fn, b = _new_kernel(mg, "fm_besseli")
    n, x = fn.args
    fabs = mg.intrinsic("fabs")
    ok = b.and_(_ll_order_ok(mg, b, n), b.fcmp_ordered("==", x, x))
    with b.if_then(b.not_(ok), likely=False):
        b.ret(f(math.nan))
    nn = b.call(fabs, [n])
    h = b.fmul(f(0.5), x)
    t, s, k = b.alloca(F64), b.alloca(F64), b.alloca(F64)
    b.store(f(1.0), t)
    b.store(f(1.0), k)
    cond, body, done = (fn.append_basic_block(nm) for nm in ("pc", "pb", "pd"))
    b.branch(cond)
    b.position_at_end(cond)                                  # t = (x/2)^n / n!
    b.cbranch(b.fcmp_ordered("<=", b.load(k), nn), body, done)
    b.position_at_end(body)
    b.store(b.fdiv(b.fmul(b.load(t), h), b.load(k)), t)
    b.store(b.fadd(b.load(k), f(1.0)), k)
    b.branch(cond)
    b.position_at_end(done)
    b.store(b.load(t), s)
    q = b.fmul(h, h)
    b.store(f(1.0), k)
    cond, body, done = (fn.append_basic_block(nm) for nm in ("sc", "sb", "sd"))
    b.branch(cond)
    b.position_at_end(cond)
    b.cbranch(b.fcmp_ordered("<", b.load(k), f(I_MAX_TERMS)), body, done)
    b.position_at_end(body)
    kk = b.load(k)
    tv = b.fdiv(b.fmul(b.load(t), q), b.fmul(kk, b.fadd(kk, nn)))
    b.store(tv, t)
    sv = b.fadd(b.load(s), tv)
    b.store(sv, s)
    b.store(b.fadd(kk, f(1.0)), k)
    small = b.fcmp_ordered("<=", b.call(fabs, [tv]), b.fmul(f(1e-17), b.call(fabs, [sv])))
    b.cbranch(small, done, cond)
    b.position_at_end(done)
    b.ret(b.load(s))
    del I64
    return fn


def build_besselk(mg):
    """fm_besselk(n, x): the trapezoidal rule of besselk() above, instruction for instruction."""
    from llvmlite import ir
    F64 = ir.DoubleType()

    def f(v):
        return ir.Constant(F64, v)
    fn, b = _new_kernel(mg, "fm_besselk")
    n, x = fn.args
    fabs, sqrt, exp = mg.intrinsic("fabs"), mg.intrinsic("sqrt"), mg.intrinsic("exp")
    ok = b.and_(_ll_order_ok(mg, b, n), b.fcmp_ordered(">=", x, f(0.0)))     # false for NaN
    with b.if_then(b.not_(ok), likely=False):
        b.ret(f(math.nan))
    with b.if_then(b.fcmp_ordered("==", x, f(0.0)), likely=False):
        b.ret(f(math.inf))
    nn = b.call(fabs, [n])
    h = b.call(mg.intrinsic("minnum"), [f(0.1), b.fdiv(f(0.5), b.call(sqrt, [b.call(sqrt, [
        b.fadd(b.fmul(x, x), b.fmul(nn, nn))])]))])
    peak = b.call(mg.libm("asinh"), [b.fdiv(nn, x)])
    s, k = b.alloca(F64), b.alloca(F64)
    b.store(b.fmul(f(0.5), b.call(exp, [b.fneg(x)])), s)
    b.store(f(1.0), k)
    cond, body, done = (fn.append_basic_block(nm) for nm in ("kc", "kb", "kd"))
    b.branch(cond)
    b.position_at_end(cond)
    b.cbranch(b.fcmp_ordered("<", b.load(k), f(K_MAX_STEPS)), body, done)
    b.position_at_end(body)
    kk = b.load(k)
    t = b.fmul(kk, h)
    ch = b.select(b.fcmp_ordered("<", t, f(700.0)), b.call(mg.libm("cosh"), [t]), f(math.inf))
    xc = b.fmul(x, ch)
    nt = b.fmul(nn, t)
    term = b.fmul(f(0.5), b.fadd(b.call(exp, [b.fsub(nt, xc)]), b.call(exp, [b.fsub(b.fneg(nt), xc)])))
    sv = b.fadd(b.load(s), term)
    b.store(sv, s)
    b.store(b.fadd(kk, f(1.0)), k)
    stop = b.or_(b.and_(b.fcmp_ordered(">", t, peak), b.fcmp_ordered("<=", term, b.fmul(f(1e-18), sv))),
                 b.fcmp_ordered("==", sv, f(math.inf)))
    b.cbranch(stop, done, cond)
    b.position_at_end(done)
    b.ret(b.fmul(b.load(s), h))
    return fn


KERNELS = {"fm_besseli": build_besseli, "fm_besselk": build_besselk}
