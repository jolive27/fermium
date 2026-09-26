"""Complex numbers (D90-D95).

A complex value is stored like a 2-vector (real part, imaginary part: an LLVM <2 x double>, a Python
tuple in the reference interpreter) with type ComplexTy(dim), so variables, captures, function
arguments and ODE state slots reuse the vector machinery.  This module has

  * the checker's rules (arithmetic, powers, built-in functions, comparisons), called from checker.py;
  * the numerical kernels (multiply, Smith's division, exp, log, a stable square root, ...), written once
    against an `ops` object like fermium/linalg.py, so the LLVM backend and the reference interpreter run
    the very same operations;
  * the print format: 3 + 4i, (3 + 4i) Ω.
"""
from __future__ import annotations

import math

from . import ir as I
from .types import BOOL, ComplexTy, DExpr, ListTy, MatTy, NumTy, VecTy
from .units import DIMLESS, format_number

DEFAULT_SF = 3       # digits of each part when the inputs don't give a precision (D11, D94)

# built-ins that only make sense for complex numbers (they also accept real numbers)
COMPLEX_FUNCS = {"re", "im", "conj", "arg", "complex", "polar", "cis"}
# built-ins that also take a complex argument
COMPLEX_MATH = {"exp", "ln", "log", "sqrt", "sin", "cos", "tan", "sinh", "cosh", "tanh"}


def is_c(v):
    return isinstance(v, I.Expr) and isinstance(v.ty, ComplexTy)


def promote(v):
    """A real number as a complex one (imaginary part 0, same units)."""
    if is_c(v):
        return v
    r = I.IVec([v, I.IConst(0, NumTy(v.ty.dim))], ComplexTy(v.ty.dim))
    r.hint, r.sf, r.direct = v.hint, v.sf, v.direct
    return r


def _bi(ck, name, args, ty, sfargs=None):
    r = I.IBuiltin(name, args, ty)
    r.sf = ck._minsf(*(sfargs if sfargs is not None else args))
    return r


def _no_affine(ck, v, node):
    if v.hint is not None and getattr(v.hint, "affine", False):
        raise ck.err(f"°C/°F can't be used for complex numbers ({v.hint.name} has an offset)", node,
                     hint="write temperatures in K")


# ============================================================ checker rules
def arith(ck, op, a, b, e):
    """+ - * / where at least one side is complex."""
    for v in (a, b):
        if isinstance(v.ty, (ListTy, MatTy)) or (isinstance(v.ty, VecTy) and not isinstance(v.ty, ComplexTy)):
            what = "a list" if isinstance(v.ty, ListTy) else "a matrix" if isinstance(v.ty, MatTy) else "a vector"
            raise ck.err(f"can't mix complex numbers and {what} in arithmetic", e,
                         hint="lists, vectors and matrices of complex numbers aren't supported yet")
    if op == "×":
        op = "*"
    if op in ("+", "-"):
        if not ck.U.unify(a.ty.dim, b.ty.dim):
            da, db = ck.desc(a.ty.dim), ck.desc(b.ty.dim)
            msg = f"can't add {da} to {db}" if op == "+" else f"can't subtract {db} from {da}"
            raise ck.err(msg, e, hint="both sides of + and - must have the same units (for complex numbers too)")
        for v in (a, b):
            _no_affine(ck, v, e)
        r = I.IBin(op, promote(a), promote(b), ComplexTy(a.ty.dim))
        r.hint = a.hint if a.hint is not None else b.hint
    elif op == "*":
        ty = ComplexTy(a.ty.dim * b.ty.dim)
        r = I.IBuiltin("c.mul", [a, b], ty) if is_c(a) and is_c(b) else I.IBin("*", a, b, ty)
        r.hint = ck._keep_hint(a, b)
    elif op == "/":
        ty = ComplexTy(a.ty.dim / b.ty.dim)
        r = I.IBuiltin("c.div", [promote(a), b], ty) if is_c(b) else I.IBin("/", a, b, ty)
        r.hint = a.hint if ck._dimless(b) and a.hint is not None and b.hint is None else None
    else:
        raise ck.err(f"unknown operator {op}", e)
    r.sf = ck._minsf(a, b)
    return r


def power(ck, e, a, ctx, b=None):
    """a^p where the base a or the exponent is complex."""
    from fractions import Fraction
    p = ck.const_value(e.right)
    if p is not None and is_c(a):
        if p == int(p) and abs(p) <= 64:
            r = I.IBuiltin("c.powi", [a], ComplexTy(a.ty.dim ** p))
        else:
            r = I.IBuiltin("c.sqrt" if p == Fraction(1, 2) else "c.powr", [a], ComplexTy(a.ty.dim ** p))
        r.p = float(p)
        r.sf = a.sf
        if p == 1:
            r.hint = a.hint
        return r
    if b is None:
        b = ck.expr(e.right, ctx)
    ck.need_numlike(b, e.right, "the exponent", allow_vec=True)
    if not (is_c(b) or isinstance(b.ty, NumTy)):
        raise ck.err("the exponent must be a number", e.right)
    if not ck.U.unify(b.ty.dim, DIMLESS):
        raise ck.err(f"an exponent must be a plain number, but this is {ck.desc(b.ty.dim)}", e.right)
    if not isinstance(a.ty, (NumTy, ComplexTy)):
        raise ck.err("the base of a complex power must be a number", e.left)
    if not ck.U.unify(a.ty.dim, DIMLESS):
        raise ck.err(f"can't raise {ck.desc(a.ty.dim)} to a power that isn't a fixed number", e,
                     hint="with units, the exponent must be a number written in the program, like z^2")
    return _bi(ck, "c.pow", [promote(a), promote(b)], ComplexTy(DIMLESS))


def builtin(ck, name, args, e):
    """A call of a built-in function with a complex argument, or of re/im/conj/arg/complex/polar/cis."""
    n = len(args)

    def need(k):
        if n != k:
            raise ck.err(f"{name} takes {k} argument{'s' if k != 1 else ''} but was given {n}", e)

    for i, a in enumerate(args):
        if not isinstance(a, I.Expr) or not isinstance(a.ty, (NumTy, ComplexTy)):
            got = "a list" if isinstance(a.ty, ListTy) else "a vector" if isinstance(a.ty, VecTy) else \
                "a matrix" if isinstance(a.ty, MatTy) else "this value"
            raise ck.err(f"{name} needs a number or a complex number, but got {got}", e.args[i],
                         hint="lists, vectors and matrices of complex numbers aren't supported yet")
    if name == "complex":
        need(2)
        for i in range(2):
            if is_c(args[i]):
                raise ck.err("complex(a, b) takes two real numbers: the real and imaginary parts", e.args[i])
        if not ck.U.unify(args[0].ty.dim, args[1].ty.dim):
            raise ck.err(f"complex(a, b) needs both parts in the same units, but they are "
                         f"{ck.desc(args[0].ty.dim)} and {ck.desc(args[1].ty.dim)}", e)
        r = I.IVec(list(args), ComplexTy(args[0].ty.dim))
        r.hint = args[0].hint or args[1].hint
        r.sf = ck._minsf(*args)
        return r
    if name in ("polar", "cis"):
        need(2 if name == "polar" else 1)
        th = args[-1]
        for i, a in enumerate(args):
            if is_c(a):
                raise ck.err(f"{name} takes real numbers", e.args[i])
        if not ck.U.unify(th.ty.dim, DIMLESS):
            raise ck.err(f"the angle in {name} must be a plain number (radians or degrees), but it is "
                         f"{ck.desc(th.ty.dim)}", e.args[-1])
        rr = args[0] if name == "polar" else I.IConst(1.0, NumTy(DIMLESS))
        r = _bi(ck, "c.polar", [rr, th], ComplexTy(rr.ty.dim))
        r.hint = rr.hint
        return r
    if name not in COMPLEX_FUNCS and name not in COMPLEX_MATH and name != "abs":
        raise ck.err(f"{name} doesn't work on complex numbers", e,
                     hint="complex numbers work with + - * / ^, exp, ln, sqrt, sin, cos, tan, sinh, cosh, tanh, "
                          "abs, arg, re, im and conj")
    need(1)
    z = args[0]
    if name in ("re", "im"):
        z = promote(z)
        r = I.IVecElem(z, 0 if name == "re" else 1, NumTy(z.ty.dim))
        r.hint, r.sf = z.hint, z.sf
        return r
    if name == "abs":
        r = _bi(ck, "c.abs", [z], NumTy(z.ty.dim))
        r.hint = z.hint
        return r
    if name == "arg":
        return _bi(ck, "c.arg", [promote(z)], NumTy(DIMLESS))
    if name == "conj":
        r = _bi(ck, "c.conj", [promote(z)], ComplexTy(z.ty.dim))
        r.hint = z.hint
        return r
    if name == "sqrt":
        from fractions import Fraction
        return _bi(ck, "c.sqrt", [promote(z)], ComplexTy(z.ty.dim ** Fraction(1, 2)))
    if name in COMPLEX_MATH:
        if not ck.U.unify(z.ty.dim, DIMLESS):
            raise ck.err(f"{name} needs a plain number, but got a complex number of {ck.desc(z.ty.dim)}", e.args[0],
                         hint="the argument of exp, ln, sin, ... must be a plain number")
        return _bi(ck, "c." + ("ln" if name == "log" else name), [z], ComplexTy(DIMLESS))
    raise ck.err(f"{name} doesn't work on complex numbers", e)


def compare(ck, op, a, b, e, tols=None):
    if op in ("<", ">", "<=", ">="):
        raise ck.err(f"complex numbers can't be compared with {op} (they aren't ordered)", e,
                     hint="compare their sizes |z| or their real parts re(z) instead")
    for v, node in ((a, e.left), (b, e.right)):
        if not isinstance(v, I.Expr) or not isinstance(v.ty, (NumTy, ComplexTy)):
            raise ck.err("each side of a comparison must be a number", node)
    if not ck.U.unify(a.ty.dim, b.ty.dim):
        raise ck.err(f"can't compare {ck.desc(a.ty.dim)} with {ck.desc(b.ty.dim)}", e)
    name = {"==": "c.eq", "!=": "c.ne", "~=": "c.approx"}.get(op)
    if name is None:
        raise ck.err(f"complex numbers can't be compared with {op}", e)
    return I.IBuiltin(name, [promote(a), promote(b)] + (list(tols) if op == "~=" else []), BOOL)


def quantity(ck, v, u, e):
    """(3 + 4i) Ω: a complex number times a unit."""
    if u.affine:
        raise ck.err("°C/°F can't be used for complex numbers", e, hint="write temperatures in K")
    vd = ck.U.norm(v.ty.dim)
    if vd.concrete and not vd.const.dimensionless:
        raise ck.err(f"this already has units ({ck.desc(v.ty.dim)})", e)
    ck.U.unify(v.ty.dim, DIMLESS)
    r = I.IBin("*", v, I.IConst(u.factor, NumTy(DIMLESS)), ComplexTy(DExpr.of(u.dim)))
    r.hint, r.sf = u, v.sf
    r.direct = bool(getattr(e.value, "imag_literal", False))
    return r


def component_integral(ck, e, ctx, make):
    """∫ f dx for a complex integrand f: ∫ re(f) dx + i ∫ im(f) dx (D93); `make(part)` builds the AST of the
    integral (or sum) of one part."""
    from . import ast as A
    parts = []
    for name in ("re", "im"):
        node = make(A.Field(e, name).at(e))
        node._component = True
        parts.append(ck.expr(node, ctx))
    r = I.IVec(parts, ComplexTy(parts[0].ty.dim))
    ck.U.unify(parts[0].ty.dim, parts[1].ty.dim)
    r.hint = parts[0].hint
    r.sf = ck._minsf(*parts)
    return r


# ============================================================ kernels (shared by LLVM and the interpreter)
def k_mul(o, a, b):
    return (o.sub(o.mul(a[0], b[0]), o.mul(a[1], b[1])), o.add(o.mul(a[0], b[1]), o.mul(a[1], b[0])))


def k_div(o, a, b):
    """Smith's algorithm: no overflow for large |b|."""
    big = o.lt(o.fn("fabs", b[1]), o.fn("fabs", b[0]))      # |br| > |bi|
    # |br| > |bi|: r = bi/br, d = br + bi r
    r1 = o.div(b[1], b[0])
    d1 = o.add(b[0], o.mul(b[1], r1))
    x1 = o.div(o.add(a[0], o.mul(a[1], r1)), d1)
    y1 = o.div(o.sub(a[1], o.mul(a[0], r1)), d1)
    # otherwise: r = br/bi, d = br r + bi
    r2 = o.div(b[0], b[1])
    d2 = o.add(o.mul(b[0], r2), b[1])
    x2 = o.div(o.add(o.mul(a[0], r2), a[1]), d2)
    y2 = o.div(o.sub(o.mul(a[1], r2), a[0]), d2)
    return (o.select(big, x1, x2), o.select(big, y1, y2))


def k_exp(o, a):
    m = o.fn("exp", a[0])
    return (o.mul(m, o.fn("cos", a[1])), o.mul(m, o.fn("sin", a[1])))


def k_ln(o, a):
    return (o.fn("log", o.fn("hypot", a[0], a[1])), o.fn("atan2", a[1], a[0]))


def k_sqrt(o, a):
    """√z without cancellation: t = √((|x| + |z|)/2), then the other part is y/(2t)."""
    h = o.fn("hypot", a[0], a[1])
    t = o.fn("sqrt", o.mul(o.add(o.fn("fabs", a[0]), h), o.const(0.5)))
    zero = o.eq(t, o.const(0))
    s = o.select(zero, o.const(1), o.add(t, t))          # 2t, kept away from 0 (z = 0 gives 0 below)
    q = o.div(a[1], s)
    neg = o.lt(a[0], o.const(0))
    x = o.select(neg, o.fn("fabs", q), t)
    y = o.select(neg, o.fn("copysign", t, a[1]), q)
    return (o.select(zero, o.const(0), x), o.select(zero, a[1], y))


def k_sin(o, a):
    return (o.mul(o.fn("sin", a[0]), o.fn("cosh", a[1])), o.mul(o.fn("cos", a[0]), o.fn("sinh", a[1])))


def k_cos(o, a):
    return (o.mul(o.fn("cos", a[0]), o.fn("cosh", a[1])), o.neg(o.mul(o.fn("sin", a[0]), o.fn("sinh", a[1]))))


def k_sinh(o, a):
    return (o.mul(o.fn("sinh", a[0]), o.fn("cos", a[1])), o.mul(o.fn("cosh", a[0]), o.fn("sin", a[1])))


def k_cosh(o, a):
    return (o.mul(o.fn("cosh", a[0]), o.fn("cos", a[1])), o.mul(o.fn("sinh", a[0]), o.fn("sin", a[1])))


def k_powi(o, a, n):
    """z^n for a whole number n (|n| <= 64): repeated squaring, then 1/z^|n| for n < 0."""
    m = abs(n)
    result = None
    base = a
    while m:
        if m & 1:
            result = base if result is None else k_mul(o, result, base)
        m >>= 1
        if m:
            base = k_mul(o, base, base)
    if result is None:
        return (o.const(1), o.const(0))
    if n < 0:
        return k_div(o, (o.const(1), o.const(0)), result)
    return result


def k_powr(o, a, p):
    """z^p for a real constant p: |z|^p (cos pθ, sin pθ), the principal value."""
    m = o.fn("pow", o.fn("hypot", a[0], a[1]), o.const(p))
    th = o.mul(o.fn("atan2", a[1], a[0]), o.const(p))
    return (o.mul(m, o.fn("cos", th)), o.mul(m, o.fn("sin", th)))


def k_pow(o, a, b):
    """z^w = exp(w ln z); 0^w is 0 (for any w, like Python's 0j ** w with re(w) > 0)."""
    w = k_exp(o, k_mul(o, b, k_ln(o, a)))
    zero = o.and_(o.eq(a[0], o.const(0)), o.eq(a[1], o.const(0)))
    wzero = o.and_(o.eq(b[0], o.const(0)), o.eq(b[1], o.const(0)))
    x = o.select(zero, o.select(wzero, o.const(1), o.const(0)), w[0])
    y = o.select(zero, o.const(0), w[1])
    return (x, y)


def approx_core(o, same, diff, sa, sb, atol, rtol):
    """Julia's isapprox (D260): a == b, or |a − b| ≤ max(atol, rtol·max(|a|, |b|)) with a finite difference
    (so ∞ ≈ ∞ but not ∞ ≈ 1).  diff, sa, sb: |a − b|, |a|, |b| (norms for vectors, moduli for complex)."""
    tol = o.fn("maxnum", atol, o.mul(rtol, o.fn("maxnum", sa, sb)))
    close = o.and_(o.le(diff, tol), o.not_(o.eq(diff, o.const(math.inf))))
    return o.not_(o.and_(o.not_(same), o.not_(close)))


def k_approx(o, a, b, atol, rtol):
    """The complex version of ≈: moduli in place of absolute values (D21, D260)."""
    same = o.and_(o.eq(a[0], b[0]), o.eq(a[1], b[1]))
    diff = o.fn("hypot", o.sub(a[0], b[0]), o.sub(a[1], b[1]))
    return approx_core(o, same, diff, o.fn("hypot", a[0], a[1]), o.fn("hypot", b[0], b[1]), atol, rtol)


def k_approx_real(o, a, b, atol, rtol):
    """≈ for numbers (a, b one-element lists) and vectors (their components): norms, summed in order (D260)."""
    if len(a) == 1:
        return approx_core(o, o.eq(a[0], b[0]), o.fn("fabs", o.sub(a[0], b[0])), o.fn("fabs", a[0]),
                           o.fn("fabs", b[0]), atol, rtol)

    def norm(xs):
        s = o.mul(xs[0], xs[0])
        for x in xs[1:]:
            s = o.add(s, o.mul(x, x))
        return o.fn("sqrt", s)
    same = o.eq(a[0], b[0])
    for x, y in zip(a[1:], b[1:]):
        same = o.and_(same, o.eq(x, y))
    return approx_core(o, same, norm([o.sub(x, y) for x, y in zip(a, b)]), norm(a), norm(b), atol, rtol)


def run_kernel(o, e, args, iscplx):
    """Evaluate IBuiltin `e` (name c.*) on unpacked arguments: each complex one a (re, im) pair."""
    name = e.name[2:]
    z = args[0]
    if name == "mul":
        return k_mul(o, *args)
    if name == "div":
        return k_div(o, *args)
    if name in ("exp", "ln", "sqrt", "sin", "cos", "sinh", "cosh"):
        return globals()["k_" + name](o, z)
    if name == "tan":
        return k_div(o, k_sin(o, z), k_cos(o, z))
    if name == "tanh":
        return k_div(o, k_sinh(o, z), k_cosh(o, z))
    if name == "abs":
        return o.fn("hypot", z[0], z[1]) if iscplx[0] else o.fn("fabs", z)
    if name == "arg":
        return o.fn("atan2", z[1], z[0])
    if name == "conj":
        return (z[0], o.neg(z[1]))
    if name == "polar":
        return (o.mul(args[0], o.fn("cos", args[1])), o.mul(args[0], o.fn("sin", args[1])))
    if name == "powi":
        return k_powi(o, z, int(e.p))
    if name == "powr":
        return k_powr(o, z, e.p)
    if name == "pow":
        return k_pow(o, *args)
    if name == "eq":
        return o.and_(o.eq(z[0], args[1][0]), o.eq(z[1], args[1][1]))
    if name == "ne":
        return o.not_(o.and_(o.eq(z[0], args[1][0]), o.eq(z[1], args[1][1])))
    if name == "approx":
        return k_approx(o, *args)
    raise KeyError(e.name)


# ------------------------------------------------------------ ops for the interpreter
def _ieee(fn, npname):
    import numpy as np

    def g(*xs):
        try:
            return float(fn(*xs))
        except (ValueError, OverflowError, ZeroDivisionError):
            with np.errstate(all="ignore"):
                return float(getattr(np, npname)(*[np.float64(x) for x in xs]))
    return g


def _pow(x, y):
    try:
        r = x ** y
    except (OverflowError, ZeroDivisionError):
        return math.inf
    return math.nan if isinstance(r, complex) else float(r)


class PyCOps:
    FNS = {"exp": _ieee(math.exp, "exp"), "log": _ieee(math.log, "log"), "sin": _ieee(math.sin, "sin"),
           "cos": _ieee(math.cos, "cos"), "sinh": _ieee(math.sinh, "sinh"), "cosh": _ieee(math.cosh, "cosh"),
           "sqrt": _ieee(math.sqrt, "sqrt"), "atan2": _ieee(math.atan2, "arctan2"),
           "hypot": _ieee(math.hypot, "hypot"), "fabs": abs, "copysign": math.copysign, "pow": _pow,
           "maxnum": lambda x, y: y if x != x else x if y != y else max(x, y)}

    @staticmethod
    def add(x, y):
        return x + y

    @staticmethod
    def sub(x, y):
        return x - y

    @staticmethod
    def mul(x, y):
        return x * y

    @staticmethod
    def div(x, y):
        try:
            return x / y
        except ZeroDivisionError:
            if x == 0 or x != x:
                return math.nan
            return math.copysign(math.inf, x) * math.copysign(1.0, y)

    @staticmethod
    def neg(x):
        return -x

    @staticmethod
    def const(v):
        return float(v)

    @staticmethod
    def select(c, x, y):
        return x if c else y

    @staticmethod
    def lt(x, y):
        return x < y

    @staticmethod
    def le(x, y):
        return x <= y

    @staticmethod
    def eq(x, y):
        return x == y

    @staticmethod
    def and_(x, y):
        return x and y

    @staticmethod
    def not_(x):
        return not x

    def fn(self, name, *xs):
        return self.FNS[name](*xs)


def py_builtin(e, args):
    iscplx = [isinstance(a.ty, ComplexTy) for a in e.args]
    r = run_kernel(PyCOps(), e, args, iscplx)
    return tuple(r) if isinstance(r, tuple) else r


# ------------------------------------------------------------ ops for LLVM
def ll_ops(gen):
    """cplx's ops, building LLVM instructions."""
    from llvmlite import ir
    from .codegen_llvm import F64, LLOps
    b = gen.b

    class LLCOps(LLOps):
        def fn(self, name, *xs):
            if name in ("sqrt", "fabs", "copysign", "pow", "maxnum"):
                return b.call(gen.mg.intrinsic(name), list(xs))
            return b.call(gen.mg.libm(name, len(xs)), list(xs))

        def le(self, x, y):
            return b.fcmp_ordered("<=", x, y)

        def and_(self, x, y):
            return b.and_(x, y)

        def not_(self, x):
            return b.not_(x)

        def const(self, v):
            return ir.Constant(F64, float(v))
    return LLCOps(gen)


def ll_approx(gen, e, args):
    """IBuiltin approx(a, b, atol, rtol) on numbers or vectors, in LLVM (D260)."""
    from .codegen_llvm import I32
    a, c, atol, rtol = args
    if isinstance(e.args[0].ty, VecTy):
        n = e.args[0].ty.n
        a = [gen.b.extract_element(a, I32(k)) for k in range(n)]
        c = [gen.b.extract_element(c, I32(k)) for k in range(n)]
    else:
        a, c = [a], [c]
    return k_approx_real(ll_ops(gen), a, c, atol, rtol)


def py_approx(e, args):
    """IBuiltin approx(a, b, atol, rtol) on numbers or vectors, in the interpreter (D260)."""
    a, c, atol, rtol = args
    if not isinstance(a, tuple):
        a, c = (a,), (c,)
    return bool(k_approx_real(PyCOps(), list(a), list(c), atol, rtol))


def ll_builtin(gen, e, args):
    from .codegen_llvm import I32
    b = gen.b
    iscplx = [isinstance(a.ty, ComplexTy) for a in e.args]
    unpacked = [(b.extract_element(v, I32(0)), b.extract_element(v, I32(1))) if c else v
                for v, c in zip(args, iscplx)]
    r = run_kernel(ll_ops(gen), e, unpacked, iscplx)
    if isinstance(r, tuple):
        return gen.pack(list(r))
    return r


# ============================================================ printing
def format_complex(re_, im_, dim, hint, sf, direct):
    """3 + 4i, 3 - 4i, (3 + 4i) Ω; a part smaller than 10⁻¹⁴ |z| is rounding noise and prints as 0 (D94)."""
    from .runtime.core import display_unit
    u = display_unit(dim, hint)
    x, y = re_ / u.factor, im_ / u.factor
    size = math.hypot(x, y)
    if math.isfinite(size) and size > 0:
        if abs(x) < 1e-14 * size:
            x = 0.0
        if abs(y) < 1e-14 * size:
            y = 0.0

    def f(v):
        if sf is None:
            if direct:
                return format_number(v, 15, trim=True)
            if whole:
                return str(round(v))
            return format_number(v, DEFAULT_SF, trim=False)
        return format_number(v, sf if direct else max(sf, 2), trim=False)
    # like a 2-vector: whole numbers print exactly only if both parts are whole (3 + 4i, but 0.500 + 1.00i)
    from .units import _whole
    whole = all(_whole(v) for v in (x, y))        # allowing for rounding in the last bits (D11)
    body = f"{f(x)} {'-' if y < 0 else '+'} {f(abs(y)) if y == y else 'NaN'}i"      # -0 prints as 0
    name = u.name
    if name in ("", "1"):
        return body
    if name in ("°", "%", "′", "″"):
        return f"({body}){name}"
    return f"({body}) {name}"
