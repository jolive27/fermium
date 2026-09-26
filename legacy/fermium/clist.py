"""Lists of complex numbers (D243): what `fft(xs)` returns.

A complex list has one unit for every element (like a complex number, D91).  Compiled code stores it in an
ordinary list header whose data holds 2n doubles, (re, im) interleaved, with the header's length set to 2n, so
nothing that copies or frees list memory can cut it in half; every operation below divides by 2.  The
interpreter stores a Python list of (re, im) tuples, the same pairs a complex number is there.

What works on a complex list: `fft`/`ifft` (either way, from real or complex lists), `X[k]` (a complex number,
`X[end]` too), `len(X)`, `for z in X`, `print X`, and element by element `re`, `im`, `abs`/`|X|`, `arg`, `conj`
(and `X.re`, `X.im`); `complex(re_list, im_list)` builds one.  Anything else is a clear error rather than a
silent use of the interleaved storage.
"""
from __future__ import annotations

import math

from . import ir as I
from .types import ComplexListTy, ComplexTy, ListTy, NumTy, DIMLESS

ELEMENTWISE = {"re", "im", "abs", "arg", "conj"}
# fm_fft kinds (runtime/spectral.py, aot_rt.c): forward from real, inverse from complex, forward from complex,
# inverse from real; every one writes 2n interleaved doubles
FFT_KINDS = {("fft", False): 5, ("ifft", True): 6, ("fft", True): 7, ("ifft", False): 8}


def is_cl(v):
    return isinstance(v, I.Expr) and isinstance(v.ty, ComplexListTy)


# ============================================================ checker
def call(ck, name, args, e):
    """fft(xs), ifft(X), complex(re_list, im_list), len/re/im/abs/arg/conj of a complex list."""
    n = len(args)

    def need(k, usage):
        if n != k:
            raise ck.err(f"{name} takes {k} argument{'s' if k != 1 else ''}: {usage}", e)

    def mk(bname, bargs, ty, hint=None):
        r = I.IBuiltin(bname, bargs, ty)
        r.hint, r.sf = hint, None
        return r

    if name in ("fft", "ifft"):
        need(1, f"{name}(xs), with xs a list of numbers or of complex numbers")
        a = args[0]
        if not isinstance(a.ty, (ListTy, ComplexListTy)):
            from .types import type_desc
            raise ck.err(f"{name}(xs): xs must be a list, not {type_desc(a.ty, ck.U)}", e.args[0])
        h = a.hint if a.hint is not None and not getattr(a.hint, "affine", False) else None
        return mk("cl." + name, [a], ComplexListTy(a.ty.dim), h)
    if name == "complex":
        need(2, "complex(re, im) with two lists of the same length and units")
        for i in range(2):
            if not isinstance(args[i].ty, ListTy):
                raise ck.err("complex(re, im) takes two real numbers, or two lists of real numbers", e.args[i])
        if not ck.U.unify(args[0].ty.dim, args[1].ty.dim):
            raise ck.err(f"complex(re, im) needs both parts in the same units, but they are "
                         f"{ck.desc(args[0].ty.dim)} and {ck.desc(args[1].ty.dim)}", e)
        return mk("cl.make", list(args), ComplexListTy(args[0].ty.dim), args[0].hint or args[1].hint)
    if name == "len":
        need(1, "len(X)")
        return mk("len", [args[0]], NumTy(DIMLESS))
    if name in ELEMENTWISE:
        need(1, f"{name}(X)")
        a = args[0]
        if name == "arg":
            return mk("cl.arg", [a], ListTy(DIMLESS))
        if name == "conj":
            return mk("cl.conj", [a], ComplexListTy(a.ty.dim), a.hint)
        return mk("cl." + name, [a], ListTy(a.ty.dim), a.hint)
    raise ck.err(f"{name} doesn't work on a list of complex numbers", e,
                 hint="a list of complex numbers works with X[k], len, for z in X, print, fft, ifft, re, im, "
                      "abs, arg and conj; take one element X[k] for other operations")


def index(ck, t, idx, e):
    r = I.IBuiltin("cl.get", [t, idx], ComplexTy(t.ty.dim))
    r.hint, r.sf = t.hint, None
    r.line = e.line
    return r


# ============================================================ interpreter
def py_builtin(interp, name, args):
    from .runtime.spectral import spectrum
    if name in ("cl.fft", "cl.ifft"):
        a = args[0]
        if len(a) < 1:
            from .interp import _Fail, ERR_EMPTY
            raise _Fail(ERR_EMPTY)
        cplx = bool(a) and isinstance(a[0], tuple)
        flat = [x for z in a for x in z] if cplx else list(a)
        out = spectrum(FFT_KINDS[(name[3:], cplx)], flat)
        return [(out[2 * k], out[2 * k + 1]) for k in range(len(out) // 2)]
    if name == "cl.make":
        re_, im_ = args
        if len(re_) != len(im_):
            from .interp import _Fail, ERR_LEN
            raise _Fail(ERR_LEN, float(len(re_)), float(len(im_)))
        return [(float(x), float(y)) for x, y in zip(re_, im_)]
    if name == "cl.get":
        a = args[0]
        return a[interp.index(args[1], len(a))]
    a = args[0]
    if name == "cl.re":
        return [z[0] for z in a]
    if name == "cl.im":
        return [z[1] for z in a]
    if name == "cl.abs":
        return [math.hypot(*z) for z in a]
    if name == "cl.arg":
        return [math.atan2(z[1], z[0]) for z in a]
    if name == "cl.conj":
        return [(z[0], -z[1]) for z in a]
    raise KeyError(name)


# ============================================================ LLVM
def ll_len(g, lst):
    """The number of complex elements (the header holds 2n doubles)."""
    from llvmlite import ir
    from .codegen_llvm import I64
    return g.b.sdiv(g.llen(lst), ir.Constant(I64, 2))


def ll_elem(g, lst, i):
    """X[i] for a 0-based i64 i, as a <2 x double>."""
    from llvmlite import ir
    from .codegen_llvm import I64
    b = g.b
    data = g.ldata(lst)
    j = b.mul(i, ir.Constant(I64, 2))
    return g.pack([b.load(b.gep(data, [j])), b.load(b.gep(data, [b.add(j, ir.Constant(I64, 1))]))])


def ll_builtin(g, name, e, args):
    from llvmlite import ir
    from .codegen_llvm import F64, I64, ERR_INDEX, ERR_LEN, ERR_EMPTY
    b = g.b
    c64 = lambda v: ir.Constant(I64, v)          # noqa: E731
    if name == "len":
        return b.sitofp(ll_len(g, args[0]), F64)
    if name == "cl.get":
        lst, idx = args
        n = ll_len(g, lst)
        # the same check as a real list's index (FuncGen.elem_ptr), against n complex elements
        inside = b.and_(b.fcmp_ordered(">=", idx, ir.Constant(F64, 1)), b.fcmp_ordered("<=", idx, b.sitofp(n, F64)))
        k = b.fptosi(b.select(inside, idx, ir.Constant(F64, 1)), I64)
        bad = b.or_(b.not_(inside), b.fcmp_unordered("!=", b.sitofp(k, F64), idx))
        with b.if_then(bad, likely=False):
            g.fail(ERR_INDEX, idx, b.sitofp(n, F64))
        return ll_elem(g, lst, b.sub(k, c64(1)))
    F64P = F64.as_pointer()
    if name in ("cl.fft", "cl.ifft"):
        a = args[0]
        cplx = isinstance(e.args[0].ty, ComplexListTy)
        n = ll_len(g, a) if cplx else g.llen(a)
        with b.if_then(b.icmp_signed("<", n, c64(1)), likely=False):
            g.fail(ERR_EMPTY)
        out, data = g.new_list(b.mul(n, c64(2)))
        f = g.mg.extern("fm_fft", I64, [I64, F64P, F64P, I64, F64, F64P])
        st = b.call(f, [c64(FFT_KINDS[(name[3:], cplx)]), g.ldata(a), ir.Constant(F64P, None), n,
                        ir.Constant(F64, 1.0), data])
        g.fail_if(st)
        return out
    if name == "cl.make":
        ra, ia = args
        n, m = g.llen(ra), g.llen(ia)
        with b.if_then(b.icmp_signed("!=", n, m), likely=False):
            g.fail(ERR_LEN, b.sitofp(n, F64), b.sitofp(m, F64))
        out, data = g.new_list(b.mul(n, c64(2)))
        rd, idd = g.ldata(ra), g.ldata(ia)
        with g.lp.range(c64(0), n) as i:
            j = b.mul(i, c64(2))
            b.store(b.load(b.gep(rd, [i])), b.gep(data, [j]))
            b.store(b.load(b.gep(idd, [i])), b.gep(data, [b.add(j, c64(1))]))
        return out
    a = args[0]
    n = ll_len(g, a)
    src = g.ldata(a)
    if name == "cl.conj":
        out, data = g.new_list(b.mul(n, c64(2)))
    else:
        out, data = g.new_list(n)
    with g.lp.range(c64(0), n) as i:
        j = b.mul(i, c64(2))
        x = b.load(b.gep(src, [j]))
        y = b.load(b.gep(src, [b.add(j, c64(1))]))
        if name == "cl.conj":
            b.store(x, b.gep(data, [j]))
            b.store(b.fsub(ir.Constant(F64, -0.0), y), b.gep(data, [b.add(j, c64(1))]))
        else:
            v = {"cl.re": lambda: x, "cl.im": lambda: y,
                 "cl.abs": lambda: b.call(g.mg.libm("hypot", 2), [x, y]),
                 "cl.arg": lambda: b.call(g.mg.libm("atan2", 2), [y, x])}[name]()
            b.store(v, b.gep(data, [i]))
    return out


# ============================================================ printing
def format_clist(pairs, dim, hint, sf, direct, n_total=None):
    """[3 + 0i, -1 + 1i] V: each element as a complex number is printed (D94) -- a part below 10⁻¹⁴ of the
    element's size is rounding noise -- in one style for the list: whole numbers only if every shown part is
    whole (D11); a list longer than 12 shows its first 5 and last 3 elements."""
    from .runtime.core import display_unit
    from .units import _whole, format_number
    from .cplx import DEFAULT_SF
    u = display_unit(dim, hint)
    n = len(pairs) if n_total is None else n_total
    vals = []
    for re_, im_ in pairs:
        x, y = re_ / u.factor, im_ / u.factor
        size = math.hypot(x, y)
        if math.isfinite(size) and size > 0:
            x = 0.0 if abs(x) < 1e-14 * size else x
            y = 0.0 if abs(y) < 1e-14 * size else y
        vals.append((x, y))
    shown = vals[:5] + vals[-3:] if n > 12 else vals
    whole = all(_whole(v) for z in shown for v in z)

    def f(v):
        if sf is None:
            if whole:
                return str(round(v))
            return format_number(v, DEFAULT_SF, trim=False)
        return format_number(v, sf if direct else max(sf, 2), trim=False)
    texts = [f"{f(x)} {'-' if y < 0 else '+'} {f(abs(y)) if y == y else 'NaN'}i" for x, y in shown]
    if n > 12:
        texts = texts[:5] + ["…"] + texts[5:]
    s = "[" + ", ".join(texts) + "]"
    if u.name not in ("", "1"):
        s += " " + u.name
    if n > 12:
        s += f"  ({n} values)"
    return s
