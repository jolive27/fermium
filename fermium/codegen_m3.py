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
    ModuleGen._k_pde_eval = k_pde_eval


def attach_funcgen(FuncGen):
    FuncGen.e_IPdeEval = e_IPdeEval
    FuncGen.s_SAnimate = s_SAnimate


def k_pde_eval(mg):
    """double fm_pde_eval(sol, xa, xb, m, comp0, x, t, which): cubic Lagrange interpolation in x through the 4
    grid points around x (fm_sol_eval, Hermite in t, at each); mirrors runtime/m3rt.pde_eval_py."""
    from .codegen_llvm import SOLP
    fn = mg._new_fn("fm_pde_eval", F64, [SOLP, F64, F64, I64, I64, F64, F64, I64], inline=False)
    sp, xa, xb, m, comp0, x, t, which = fn.args
    b = ir.IRBuilder(fn.append_basic_block("e"))
    c = lambda v: ir.Constant(F64, v)          # noqa: E731
    h = b.fdiv(b.fsub(xb, xa), b.sitofp(m, F64))
    s = b.fdiv(b.fsub(x, xa), h)
    j = b.fptosi(b.call(mg.intrinsic("floor"), [s]), I64)
    one = ir.Constant(I64, 1)
    j = b.select(b.icmp_signed("<", j, one), one, j)
    mm2 = b.sub(m, ir.Constant(I64, 2))
    j = b.select(b.icmp_signed(">", j, mm2), mm2, j)
    r = b.fsub(s, b.sitofp(j, F64))
    r2 = b.fmul(r, r)
    # values: -r(r-1)(r-2)/6, (r+1)(r-1)(r-2)/2, -(r+1)r(r-2)/2, (r+1)r(r-1)/6
    rm1, rm2, rp1 = b.fsub(r, c(1)), b.fsub(r, c(2)), b.fadd(r, c(1))
    wv = [b.fdiv(b.fmul(b.fmul(b.fsub(c(0), r), rm1), rm2), c(6)),
          b.fdiv(b.fmul(b.fmul(rp1, rm1), rm2), c(2)),
          b.fdiv(b.fmul(b.fmul(b.fsub(c(0), rp1), r), rm2), c(2)),
          b.fdiv(b.fmul(b.fmul(rp1, r), rm1), c(6))]
    # x-derivatives: -(3r²-6r+2)/6, (3r²-4r-1)/2, -(3r²-2r-2)/2, (3r²-1)/6, over h
    t3 = b.fmul(c(3), r2)
    wd = [b.fdiv(b.fsub(c(0), b.fadd(b.fsub(t3, b.fmul(c(6), r)), c(2))), b.fmul(c(6), h)),
          b.fdiv(b.fsub(b.fsub(t3, b.fmul(c(4), r)), c(1)), b.fmul(c(2), h)),
          b.fdiv(b.fsub(c(0), b.fsub(b.fsub(t3, b.fmul(c(2), r)), c(2))), b.fmul(c(2), h)),
          b.fdiv(b.fsub(t3, c(1)), b.fmul(c(6), h))]
    isd = b.icmp_signed("==", which, one)
    use_dy = b.select(b.icmp_signed("==", which, ir.Constant(I64, 2)), one, ir.Constant(I64, 0))
    ev = mg.kernel("fm_sol_eval")
    base = b.add(comp0, b.sub(j, one))
    acc = c(0)
    for k in range(4):
        w = b.select(isd, wd[k], wv[k])
        val = b.call(ev, [sp, b.add(base, ir.Constant(I64, k)), t, use_dy])
        acc = b.fadd(acc, b.fmul(w, val))
    b.ret(acc)
    return fn


def e_IPdeEval(g, e):
    """u(x, t): x must be inside the grid (the error shows it in x's units), then fm_pde_eval."""
    from .codegen_llvm import ERR_SOLRANGE
    b = g.b
    mg = g.mg
    sol = g.expr(e.sol)
    xa, xb = g.expr(e.xa), g.expr(e.xb)
    x, t = g.expr(e.x), g.expr(e.t)
    b.store(ir.Constant(I64, getattr(e, "xfmt", -1)), mg.errfmt)
    g.mark_line()
    slack = b.fmul(ir.Constant(F64, 1e-9), b.call(mg.intrinsic("fabs"), [b.fsub(xb, xa)]))
    bad = b.or_(b.fcmp_ordered("<", x, b.fsub(xa, slack)), b.fcmp_ordered(">", x, b.fadd(xb, slack)))
    bad = b.or_(bad, b.fcmp_unordered("uno", x, x))
    with b.if_then(bad, likely=False):
        g.fail(ERR_SOLRANGE, x, b.select(b.fcmp_ordered("<", x, xa), xa, xb))
    b.store(ir.Constant(I64, getattr(e, "tfmt", -1)), mg.errfmt)
    return b.call(mg.kernel("fm_pde_eval"), [sol, xa, xb, ir.Constant(I64, e.m), ir.Constant(I64, e.comp0), x, t,
                                             ir.Constant(I64, e.which)])


def s_SAnimate(g, s):
    from .codegen_llvm import I8P, SOLP  # noqa: F401
    b = g.b
    mg = g.mg
    g.mark_line()
    ext = mg.extern("fm_animate", I64, [I64, SOLP, F64, F64])
    st = b.call(ext, [ir.Constant(I64, s.anim_id), g.expr(s.sol), g.expr(s.xa), g.expr(s.xb)])
    with b.if_then(b.icmp_signed("!=", st, ir.Constant(I64, 0)), likely=False):
        g.fail(ERR_PENDING)


PY_SOLVES = {"eigen", "pde"}


def py_solve(g, s):
    """An eigenvalue problem (D82) or PDE (D83): a Python solver (runtime/m3rt.py) calls the compiled right-hand
    side back through fm_ode_guard and returns a SolStruct; status 1: it stopped with a message, 2: the right
    side stopped with its own error (already reported, so just unwind)."""
    b = g.b
    mg = g.mg
    from .codegen_llvm import SOLP, I8P, F64P, ODE_FN  # noqa: F401
    fn = mg.lambda_for(s.rhs)
    env = g.make_env(s.rhs)
    t0 = g.expr(s.t0)
    t1 = g.expr(s.t1)
    fmt = getattr(s, "tfmt", -1)
    b.store(ir.Constant(I64, fmt), mg.errfmt)
    g.mark_line()
    guard = mg.kernel("fm_ode_guard")
    out = g.alloca(SOLP)
    if s.method == "eigen":
        ext = mg.extern("fm_eigen", I64, [I8P, I8P, F64P, F64, F64, I64, I64, I64, SOLP.as_pointer()])
        status = b.call(ext, [b.bitcast(guard, I8P), b.bitcast(fn, I8P), env, t0, t1,
                              ir.Constant(I64, s.nstates), ir.Constant(I64, s.grid),
                              ir.Constant(I64, s.eig_method), out])
    else:
        ext = mg.extern("fm_pde", I64, [I8P, I8P, F64P, F64, F64, F64, F64, F64, I64, I64, I64, I64, I64, I64, I64,
                                         I64, SOLP.as_pointer()])
        step = g.expr(s.step) if s.step is not None else ir.Constant(F64, float("nan"))
        xa, xb = g.expr(s.xa), g.expr(s.xb)
        status = b.call(ext, [b.bitcast(guard, I8P), b.bitcast(fn, I8P), env, xa, xb, t0, t1, step,
                              ir.Constant(I64, s.grid), ir.Constant(I64, s.order), ir.Constant(I64, s.pmethod),
                              ir.Constant(I64, s.bc[0]), ir.Constant(I64, s.bc[1]),
                              ir.Constant(I64, 1 if s.is_complex else 0), ir.Constant(I64, 1 if s.tdep else 0),
                              ir.Constant(I64, getattr(g, "line", 0) or 0), out])
    with b.if_then(b.icmp_signed("!=", status, ir.Constant(I64, 0)), likely=False):
        with b.if_then(b.icmp_signed("==", status, ir.Constant(I64, 2))):
            b.call(mg.externs["longjmp"], [b.bitcast(mg.jmpbuf, I8P), ir.Constant(ir.IntType(32), 1)])
            b.unreachable()
        g.fail(ERR_PENDING)
    g.store(s.sol_sym, b.load(out))

