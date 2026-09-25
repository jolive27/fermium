"""LLVM code for the M3 numerics built-ins (DECISIONS D80–D84), kept apart from codegen_llvm.py so the
two can change independently.  Attached to ModuleGen / FuncGen at the end of codegen_llvm.py.

- the random number generator (xoshiro256** seeded by splitmix64, the same as fermium/rng.py)
- FFT built-ins and the PDE / eigenvalue callbacks' glue
"""
from __future__ import annotations

from llvmlite import ir

from . import rng as R

F64 = ir.DoubleType()
I64 = ir.IntType(64)
RNG_T = ir.ArrayType(I64, 4)

M3_BUILTINS = {"rand", "rand2", "randn", "randn2", "seed", "sample", "fft_re", "fft_im", "ifft", "amplitude_spectrum",
               "power_spectrum", "frequencies", "argmax", "argmin"}
FFT_KIND = {"fft_re": 0, "fft_im": 1, "amplitude_spectrum": 2, "power_spectrum": 3, "ifft": 4}
ERR_LEN, ERR_EMPTY, ERR_PENDING = 5, 6, -1


def _i64(v):
    return ir.Constant(I64, v & ((1 << 64) - 1) if v >= 0 else v)


def _u64(v):
    """An i64 constant from an unsigned 64-bit value."""
    v &= (1 << 64) - 1
    return ir.Constant(I64, v - (1 << 64) if v >= 1 << 63 else v)


def rng_ptr(mg, b):
    """Pointer to the four state words: the runtime's own array when running in-process (so the REPL keeps
    one stream across inputs, and the interpreter shares it), else a global in the module (fermium build)."""
    addr = getattr(mg, "rng_addr", None)
    if addr:
        return b.inttoptr(ir.Constant(I64, addr), RNG_T.as_pointer())
    g = getattr(mg, "_rng_global", None)
    if g is None:
        g = ir.GlobalVariable(mg.module, RNG_T, "fm.rng")
        g.linkage = "internal"
        g.initializer = ir.Constant(RNG_T, [_u64(w) for w in R.DEFAULT_STATE])
        mg._rng_global = g
    return g


def _rotl(b, x, k):
    return b.or_(b.shl(x, _i64(k)), b.lshr(x, _i64(64 - k)))


def k_rng_next(mg):
    fn = mg._new_fn("fm_rng_next", I64, [])
    b = ir.IRBuilder(fn.append_basic_block("e"))
    p = rng_ptr(mg, b)
    sp = [b.gep(p, [I32(0), I32(k)]) for k in range(4)]
    s0, s1, s2, s3 = [b.load(q) for q in sp]
    result = b.mul(_rotl(b, b.mul(s1, _i64(5)), 7), _i64(9))
    t = b.shl(s1, _i64(17))
    s2 = b.xor(s2, s0)
    s3 = b.xor(s3, s1)
    s1 = b.xor(s1, s2)
    s0 = b.xor(s0, s3)
    s2 = b.xor(s2, t)
    s3 = _rotl(b, s3, 45)
    for q, v in zip(sp, (s0, s1, s2, s3)):
        b.store(v, q)
    b.ret(result)
    return fn


def I32(v):
    return ir.Constant(ir.IntType(32), v)


def k_rand(mg):
    fn = mg._new_fn("fm_rand", F64, [])
    b = ir.IRBuilder(fn.append_basic_block("e"))
    x = b.call(mg.kernel("fm_rng_next"), [])
    b.ret(b.fmul(b.uitofp(b.lshr(x, _i64(11)), F64), ir.Constant(F64, R.TWO_M53)))
    return fn


def k_randn(mg):
    fn = mg._new_fn("fm_randn", F64, [], inline=False)
    b = ir.IRBuilder(fn.append_basic_block("e"))
    r = mg.kernel("fm_rand")
    u1 = b.call(r, [])
    u2 = b.call(r, [])
    lg = b.call(mg.libm("log"), [b.fsub(ir.Constant(F64, 1.0), u1)])
    rad = b.call(mg.intrinsic("sqrt"), [b.fmul(ir.Constant(F64, -2.0), lg)])
    c = b.call(mg.libm("cos"), [b.fmul(ir.Constant(F64, R.TWO_PI), u2)])
    b.ret(b.fmul(rad, c))
    return fn


def k_seed(mg):
    fn = mg._new_fn("fm_seed", ir.VoidType(), [F64], inline=False)
    s = fn.args[0]
    b = ir.IRBuilder(fn.append_basic_block("e"))
    ok = b.and_(b.fcmp_ordered("==", s, s),
                b.fcmp_ordered("<", b.call(mg.intrinsic("fabs"), [s]), ir.Constant(F64, 9.2e18)))
    x = b.select(ok, b.fptosi(s, I64), _i64(0))
    p = rng_ptr(mg, b)
    for k in range(4):
        x = b.add(x, _u64(R.GOLDEN))
        z = x
        z = b.mul(b.xor(z, b.lshr(z, _i64(30))), _u64(R.SM1))
        z = b.mul(b.xor(z, b.lshr(z, _i64(27))), _u64(R.SM2))
        z = b.xor(z, b.lshr(z, _i64(31)))
        b.store(z, b.gep(p, [I32(0), I32(k)]))
    b.ret_void()
    return fn


def builtin(g, name, e, args):
    """FuncGen.e_IBuiltin for the M3 built-ins."""
    b = g.b
    mg = g.mg
    if name == "rand":
        return b.call(mg.kernel("fm_rand"), [])
    if name == "rand2":
        lo, hi = args
        return b.fadd(lo, b.fmul(b.fsub(hi, lo), b.call(mg.kernel("fm_rand"), [])))
    if name == "randn":
        return b.call(mg.kernel("fm_randn"), [])
    if name == "randn2":
        mu, sig = args
        return b.fadd(mu, b.fmul(sig, b.call(mg.kernel("fm_randn"), [])))
    if name == "seed":
        b.call(mg.kernel("fm_seed"), [args[0]])
        return ir.Constant(F64, 0.0)
    if name == "sample":
        n = g.list_count(args[0])
        fn = mg.lambda_for(e.lam)
        env = g.make_env(e.lam)
        out, data = g.new_list(n)
        with g.lp.range(ir.Constant(I64, 0), n) as i:
            v = b.call(fn, [b.sitofp(b.add(i, ir.Constant(I64, 1)), F64), env])
            b.store(v, b.gep(data, [i]))
        return out
    if name in FFT_KIND:
        return fft(g, name, args)
    if name == "frequencies":
        n = g.list_count(args[0])
        dt = args[1]
        with b.if_then(b.icmp_signed("<", n, ir.Constant(I64, 1)), likely=False):
            g.fail(ERR_EMPTY)
        m = b.add(b.sdiv(n, ir.Constant(I64, 2)), ir.Constant(I64, 1))
        out, data = g.new_list(m)
        span = b.fmul(b.sitofp(n, F64), dt)
        with g.lp.range(ir.Constant(I64, 0), m) as i:
            b.store(b.fdiv(b.sitofp(i, F64), span), b.gep(data, [i]))
        return out
    if name in ("argmax", "argmin"):
        lst = args[0]
        n = g.llen(lst)
        with b.if_then(b.icmp_signed("<", n, ir.Constant(I64, 1)), likely=False):
            g.fail(ERR_EMPTY)
        data = g.ldata(lst)
        best = g.alloca(I64)
        b.store(ir.Constant(I64, 0), best)
        with g.lp.range(ir.Constant(I64, 1), n) as i:
            x = b.load(b.gep(data, [i]))
            y = b.load(b.gep(data, [b.load(best)]))
            better = b.fcmp_ordered(">" if name == "argmax" else "<", x, y)
            better = b.or_(better, b.and_(b.fcmp_unordered("uno", y, y), b.fcmp_ordered("==", x, x)))
            b.store(b.select(better, i, b.load(best)), best)
        return b.sitofp(b.add(b.load(best), ir.Constant(I64, 1)), F64)
    raise KeyError(name)


def fft(g, name, args):
    """fft_re / fft_im / amplitude_spectrum / power_spectrum / ifft: the fm_fft callback (NumPy under the JIT,
    C in `fermium build`) fills a new list."""
    b = g.b
    kind = FFT_KIND[name]
    a = args[0]
    n = g.llen(a)
    with b.if_then(b.icmp_signed("<", n, ir.Constant(I64, 1)), likely=False):
        g.fail(ERR_EMPTY)
    F64P = F64.as_pointer()
    other = ir.Constant(F64P, None)
    dt = ir.Constant(F64, 1.0)
    if name == "ifft":
        nb = g.llen(args[1])
        with b.if_then(b.icmp_signed("!=", n, nb), likely=False):
            g.fail(ERR_LEN, b.sitofp(n, F64), b.sitofp(nb, F64))
        other = g.ldata(args[1])
    if name == "power_spectrum":
        dt = args[1]
    m = b.add(b.sdiv(n, ir.Constant(I64, 2)), ir.Constant(I64, 1)) if kind in (2, 3) else n
    out, data = g.new_list(m)
    f = g.mg.extern("fm_fft", I64, [I64, F64P, F64P, I64, F64, F64P])
    st = b.call(f, [ir.Constant(I64, kind), g.ldata(a), other, n, dt, data])
    with b.if_then(b.icmp_signed("!=", st, ir.Constant(I64, 0)), likely=False):
        g.fail(ERR_PENDING)
    return out


def attach(ModuleGen):
    ModuleGen._k_rng_next = k_rng_next
    ModuleGen._k_rand = k_rand
    ModuleGen._k_randn = k_randn
    ModuleGen._k_seed = k_seed

