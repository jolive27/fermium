"""LLVM code generation: typed IR -> LLVM IR (via llvmlite) -> native code (MCJIT).

The IR contains only SI numbers, so no unit logic appears here (spec §3.3).
Numerical kernels (adaptive Gauss–Kronrod quadrature, RK4, Dormand–Prince RK45,
Hermite interpolation of ODE solutions) are generated here as LLVM IR too, so
the integrand / right-hand side is inlined into them and everything is native.
"""
from __future__ import annotations

import math

from llvmlite import ir

from .numerics import quintic_hermite
from . import ir as I
from .types import NumTy, BoolTy, ListTy, SolTy, DataTy, StrTy, VecTy, TextListTy


def odd_root_numerator(p):
    """For an exponent p = n/q with q odd and > 1 (1/3, 2/3, -2/3, 1/5, ...) return n, else None.

    Such powers have a real value for negative bases: (-8)^(2/3) = 4, (-8)^(1/3) = -2."""
    if not math.isfinite(p) or p == int(p):
        return None
    from fractions import Fraction
    f = Fraction(p).limit_denominator(99)
    if f.denominator % 2 == 0 or abs(float(f) - p) > 1e-12 * max(1.0, abs(p)):
        return None
    return f.numerator

F64 = ir.DoubleType()
I64 = ir.IntType(64)
I32 = ir.IntType(32)
I8 = ir.IntType(8)
I1 = ir.IntType(1)
VOID = ir.VoidType()
F64P = F64.as_pointer()
F64PP = F64P.as_pointer()
I8P = I8.as_pointer()
LISTH = ir.LiteralStructType([F64P, I64, I64])    # list header: data, len, capacity
LIST = LISTH.as_pointer()                          # a list value is a pointer to its (shared) header
# solution: n, dim, cap, t*, y*, dy*
SOL = ir.LiteralStructType([I64, I64, I64, F64P, F64P, F64P])
SOLP = SOL.as_pointer()

SCALAR_FN = ir.FunctionType(F64, [F64, F64P])
ODE_FN = ir.FunctionType(VOID, [F64, F64P, F64P, F64P])
MODEL_FN = ir.FunctionType(VOID, [F64P, F64PP, I64, F64P])

ERR_INDEX, ERR_SOLRANGE, ERR_ODE_STEPS, ERR_ASSERT, ERR_LEN, ERR_EMPTY, ERR_STEP, ERR_ODE_H = 1, 2, 3, 4, 5, 6, 7, 8
ERR_QUAD = 9
ERR_DEEP = 10
ERR_SIZE = 11               # a list too big for memory (or of NaN length)
ERR_RANGE = 12              # a for loop over a range with a NaN end or step
MAX_LIST = 1e9              # most numbers a list may hold (8 GB)
STACK_LIMIT = 400 << 20     # bytes of stack a program may use (it runs on a thread with a 512 MB stack)

# Gauss–Kronrod 7-15 nodes/weights (from QUADPACK qk15)
XGK = [0.991455371120812639206854697526329, 0.949107912342758524526189684047851,
       0.864864423359769072789712788640926, 0.741531185599394439863864773280788,
       0.586087235467691130294144845693013, 0.405845151377397166906606412076961,
       0.207784955007898467600689403773245, 0.000000000000000000000000000000000]
WGK = [0.022935322010529224963732008058970, 0.063092092629978553290700663189204,
       0.104790010322250183839876322541518, 0.140653259715525918745189590510238,
       0.169004726639267902826583426598550, 0.190350578064785409913256402421014,
       0.204432940075298892414161999234649, 0.209482141084727828012999174891714]
WG = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780,
      0.381830050505118944950369775488975, 0.417959183673469387755102040816327]


def lltype(ty):
    if isinstance(ty, NumTy):
        return F64
    if isinstance(ty, VecTy):
        return ir.VectorType(F64, ty.n)
    if isinstance(ty, BoolTy):
        return I1
    if isinstance(ty, (ListTy, TextListTy)):
        return LIST
    if isinstance(ty, SolTy):
        return SOLP
    if isinstance(ty, DataTy):
        return I64
    if isinstance(ty, StrTy):
        return I64
    raise TypeError(f"no LLVM type for {ty}")


def f64(v):
    return ir.Constant(F64, float(v))


def i64(v):
    return ir.Constant(I64, int(v))


class ModuleGen:
    def __init__(self, name="fermium", arena_base=None):
        self.module = ir.Module(name=name)
        self.arena_base = arena_base
        self.globals = {}
        self.funcs = {}
        self.pending = []
        self.externs = {}
        self.lambda_fns = {}
        self._declare_runtime()
        self.jmpbuf = ir.GlobalVariable(self.module, ir.ArrayType(I8, 1024), "fm.jmpbuf")
        self.jmpbuf.initializer = ir.Constant(ir.ArrayType(I8, 1024), None)
        self.jmpbuf.align = 16
        self.jmpbuf.linkage = "internal"
        self.curline = ir.GlobalVariable(self.module, I64, "fm.line")
        self.curline.initializer = i64(0)
        self.curline.linkage = "internal"
        self.stackbase = ir.GlobalVariable(self.module, I64, "fm.stackbase")   # stack address at start
        self.stackbase.initializer = i64(0)
        self.stackbase.linkage = "internal"
        self.errfmt = ir.GlobalVariable(self.module, I64, "fm.errfmt")   # print format for error values
        self.errfmt.initializer = i64(-1)
        self.errfmt.linkage = "internal"
        self._kernels_built = set()
        self._checked_malloc()

    def _checked_malloc(self):
        """Every allocation goes through fm_xalloc, which stops with 'not enough memory' instead of
        handing back a null pointer (externs['malloc'] points at it)."""
        real = self.externs["malloc"]
        fn = ir.Function(self.module, ir.FunctionType(I8P, [I64]), "fm_xalloc")
        fn.linkage = "internal"
        b = ir.IRBuilder(fn.append_basic_block("e"))
        p = b.call(real, [fn.args[0]])
        with b.if_then(b.icmp_unsigned("==", p, ir.Constant(I8P, None)), likely=False):
            self.raise_error(b, ERR_SIZE, b.fdiv(b.uitofp(fn.args[0], F64), f64(8)), f64(0))
        b.ret(p)
        self.externs["malloc"] = fn

    # ------------------------------------------------------------ declarations
    def extern(self, name, ret, args, var_arg=False, attrs=()):
        if name in self.externs:
            return self.externs[name]
        f = ir.Function(self.module, ir.FunctionType(ret, args, var_arg=var_arg), name)
        for a in attrs:
            f.attributes.add(a)
        self.externs[name] = f
        return f

    def _declare_runtime(self):
        e = self.extern
        e("fm_print_num", VOID, [I64, F64])
        e("fm_print_list", VOID, [I64, F64P, I64])
        e("fm_print_vec", VOID, [I64, F64P, I64])
        e("fm_print_textlist", VOID, [F64P, I64])
        e("fm_print_bool", VOID, [I64])
        e("fm_print_text", VOID, [I64])
        e("fm_print_end", VOID, [])
        e("fm_error", VOID, [I64, F64, F64, I64, I64])
        e("fm_plot_series", VOID, [I64, I64, F64P, I64, F64P, I64])
        e("fm_plot_sol", VOID, [I64, I64, I8P, I64, I64, I64, I64])
        e("fm_plot_done", VOID, [I64])
        e("fm_load", I64, [I64])
        e("fm_column", I64, [I64, I64, F64PP])
        e("fm_fit", VOID, [I64, I64, F64P])
        e("fm_sort", VOID, [F64P, I64])
        e("fm_clock", F64, [])
        e("malloc", I8P, [I64])
        e("realloc", I8P, [I8P, I64])
        e("drand48", F64, [])
        sj = e("_setjmp", I32, [I8P])
        sj.attributes.add("returns_twice")
        lj = e("longjmp", VOID, [I8P, I32])
        lj.attributes.add("noreturn")

    TWO_ARG = {"maxnum", "minnum", "pow", "copysign"}

    def intrinsic(self, name, nargs=1):
        key = "llvm." + name + ".f64"
        if key not in self.externs:
            n = 2 if name in self.TWO_ARG else 1
            self.externs[key] = ir.Function(self.module, ir.FunctionType(F64, [F64] * n), key)
        return self.externs[key]

    def libm(self, name, nargs=1):
        return self.extern(name, F64, [F64] * nargs)

    # ------------------------------------------------------------ storage
    def global_for(self, sym):
        if sym.id in self.globals:
            return self.globals[sym.id]
        t = lltype(sym.ty)
        if sym.storage == "arena":
            raise RuntimeError("arena symbols are addressed directly")
        g = ir.GlobalVariable(self.module, t, f"g.{sym.name}.{sym.id}")
        g.linkage = "internal"
        g.initializer = ir.Constant(t, None)
        self.globals[sym.id] = g
        return g

    # ------------------------------------------------------------ functions
    def func_for(self, inst: I.IFunc):
        if inst.name in self.funcs:
            return self.funcs[inst.name]
        ret = lltype(inst.ret_ty)
        fty = ir.FunctionType(ret, [lltype(p.ty) for p in inst.params])
        fn = ir.Function(self.module, fty, "fn." + inst.name)
        fn.linkage = "internal"
        self.funcs[inst.name] = fn
        self.pending.append((inst, fn))
        return fn

    def lambda_for(self, lam: I.ILambda):
        if lam.name in self.lambda_fns:
            return self.lambda_fns[lam.name]
        fty = {"scalar": SCALAR_FN, "ode": ODE_FN, "model": MODEL_FN}[lam.kind]
        fn = ir.Function(self.module, fty, "lam." + lam.name)
        fn.linkage = "internal" if lam.kind != "model" else "external"
        self.lambda_fns[lam.name] = fn
        self.pending.append((lam, fn))
        return fn

    def emit_main(self, main: I.IFunc, entry_name):
        fn = ir.Function(self.module, ir.FunctionType(VOID, []), "fm.body." + entry_name)
        fn.linkage = "internal"
        g = FuncGen(self, fn, main)
        g.emit_body(main.body)
        if not g.b.block.is_terminated:
            g.b.ret_void()
        # trampoline with setjmp so runtime errors can unwind
        run = ir.Function(self.module, ir.FunctionType(I32, []), entry_name)
        b = ir.IRBuilder(run.append_basic_block("entry"))
        b.store(b.ptrtoint(b.call(self.frameaddr(), [ir.Constant(I32, 0)]), I64), self.stackbase)
        buf = b.bitcast(self.jmpbuf, I8P)
        r = b.call(self.externs["_setjmp"], [buf])
        ok = b.icmp_signed("==", r, ir.Constant(I32, 0))
        with b.if_else(ok) as (then, other):
            with then:
                b.call(fn, [])
            with other:
                pass
        res = b.select(ok, ir.Constant(I32, 0), ir.Constant(I32, 1))
        b.ret(res)
        self.flush()

    def frameaddr(self):
        if "llvm.frameaddress" not in self.externs:
            self.externs["llvm.frameaddress"] = ir.Function(self.module, ir.FunctionType(I8P, [I32]),
                                                            "llvm.frameaddress.p0")
        return self.externs["llvm.frameaddress"]

    def stack_check(self, g, name_id, line=None):
        """Runaway recursion: stop with a clear error before the stack overflows (instead of a crash)."""
        b = g.b
        sp = b.ptrtoint(b.call(self.frameaddr(), [ir.Constant(I32, 0)]), I64)
        used = b.sub(b.load(self.stackbase), sp)
        with b.if_then(b.icmp_signed(">", used, i64(STACK_LIMIT)), likely=False):
            self.raise_error(b, ERR_DEEP, f64(name_id), f64(0), line)

    def flush(self):
        while self.pending:
            item, fn = self.pending.pop()
            if isinstance(item, I.IFunc):
                g = FuncGen(self, fn, item)
                self.stack_check(g, getattr(item, "name_text", -1), getattr(item, "def_line", None))
                for p, a in zip(item.params, fn.args):
                    slot = g.slot(p)
                    g.b.store(a, slot)
                g.emit_body(item.body)
                if not g.b.block.is_terminated:
                    # unreachable in well-formed functions; return a zero value
                    g.b.ret(ir.Constant(fn.function_type.return_type, None))
            else:
                LambdaGen(self, fn, item).emit()

    # ------------------------------------------------------------ kernels
    def kernel(self, name):
        if name in self._kernels_built:
            return self.module.get_global(name)
        self._kernels_built.add(name)
        return getattr(self, "_k_" + name.replace("fm_", ""))()

    def raise_error(self, b, kind, a=None, c=None, line=None):
        ln = i64(line) if line else b.load(self.curline)
        b.call(self.externs["fm_error"], [i64(kind), a if a is not None else f64(0), c if c is not None else f64(0),
                                          ln, b.load(self.errfmt)])
        b.call(self.externs["longjmp"], [b.bitcast(self.jmpbuf, I8P), ir.Constant(I32, 1)])
        b.unreachable()

    def _new_fn(self, name, ret, args, inline=True):
        fn = ir.Function(self.module, ir.FunctionType(ret, args), name)
        fn.linkage = "internal"
        if inline:
            fn.attributes.add("alwaysinline")
        return fn

    def _k_qf(self):
        """The integrand in u ∈ [0, 1].  mode 0: x from p to q with the smoothstep substitution
        x = p + (q-p)(3u² - 2u³), which removes 1/√ singularities at the ends (dx/du vanishes there);
        mode 1: x = p + q u/(1-u) (the half-line [p, ∞) with length scale q); mode 2: x = p - q u/(1-u)."""
        fn = self._new_fn("fm_qf", F64, [SCALAR_FN.as_pointer(), F64P, I64, F64, F64, F64])
        f, env, mode, p, q, u = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        blk_fin = fn.append_basic_block("fin")
        blk_half = fn.append_basic_block("half")
        b.cbranch(b.icmp_signed("==", mode, i64(0)), blk_fin, blk_half)
        b.position_at_end(blk_fin)
        L = b.fsub(q, p)
        v = b.fsub(f64(1), u)
        lower = b.fcmp_ordered("<=", u, f64(0.5))
        # measure from the nearer end so that points close to q keep their full precision
        xl = b.fadd(p, b.fmul(L, b.fmul(b.fmul(u, u), b.fsub(f64(3), b.fmul(f64(2), u)))))
        xh = b.fsub(q, b.fmul(L, b.fmul(b.fmul(v, v), b.fadd(f64(1), b.fmul(f64(2), u)))))
        x = b.select(lower, xl, xh)
        w = b.fmul(b.fmul(f64(6), b.fmul(u, v)), L)
        at_end = b.or_(b.fcmp_ordered("==", x, p), b.fcmp_ordered("==", x, q))
        with b.if_then(at_end):      # a node that rounds onto an end point has no weight
            b.ret(f64(0))
        b.ret(b.fmul(b.call(f, [x, env]), w))
        b.position_at_end(blk_half)
        om = b.fsub(f64(1), u)
        sx = b.fmul(q, b.fdiv(u, om))
        x = b.select(b.icmp_signed("==", mode, i64(1)), b.fadd(p, sx), b.fsub(p, sx))
        b.ret(b.fdiv(b.fmul(b.call(f, [x, env]), q), b.fmul(om, om)))
        return fn

    def _k_qscan(self):
        """Length scale of an integrand on a half-line: the s = 10^(k/4), 10^-40 ≤ s ≤ 10^40, where
        s·|f(base + sign·s)| is largest (1 if f is zero everywhere).  Integrals to ∞ use it so that
        femtometres and astronomical units work alike."""
        fn = self._new_fn("fm_qscan", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64], inline=False)
        f, env, base, sign = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        sv, best, bs = b.alloca(F64), b.alloca(F64), b.alloca(F64)
        b.store(f64(1e-40), sv)
        b.store(f64(0), best)
        b.store(f64(1), bs)
        with lp.range(i64(0), i64(321)):
            sc = b.load(sv)
            val = b.fmul(b.call(fabs, [b.call(f, [b.fadd(base, b.fmul(sign, sc)), env])]), sc)
            better = b.and_(b.fcmp_ordered(">", val, b.load(best)), b.fcmp_ordered("<", val, f64(math.inf)))
            with b.if_then(better):
                b.store(val, best)
                b.store(sc, bs)
            b.store(b.fmul(sc, f64(10 ** 0.25)), sv)
        b.ret(b.load(bs))
        return fn

    def _k_gk15(self):
        qf = self.kernel("fm_qf")
        fn = self._new_fn("fm_gk15", F64, [SCALAR_FN.as_pointer(), F64P, I64, F64, F64, F64, F64, F64P],
                          inline=False)
        f, env, mode, a, bb, lo, hi, errp = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        c = b.fmul(f64(0.5), b.fadd(lo, hi))
        h = b.fmul(f64(0.5), b.fsub(hi, lo))
        fc = b.call(qf, [f, env, mode, a, bb, c])
        resk = b.fmul(fc, f64(WGK[7]))
        resg = b.fmul(fc, f64(WG[3]))
        for j in range(7):
            dx = b.fmul(h, f64(XGK[j]))
            f1 = b.call(qf, [f, env, mode, a, bb, b.fsub(c, dx)])
            f2 = b.call(qf, [f, env, mode, a, bb, b.fadd(c, dx)])
            s = b.fadd(f1, f2)
            resk = b.fadd(resk, b.fmul(s, f64(WGK[j])))
            if j % 2 == 1:
                resg = b.fadd(resg, b.fmul(s, f64(WG[j // 2])))
        fabs = self.intrinsic("fabs")
        b.store(b.call(fabs, [b.fmul(b.fsub(resk, resg), h)]), errp)
        b.ret(b.fmul(resk, h))
        return fn

    QUAD_PANELS = 8          # initial uniform panels (helps with narrow peaks)
    QUAD_MAX = 2000          # subdivision budget; beyond it the integral is reported as not converging

    def _k_quad(self):
        """∫ f from a to b.  Finite ranges go straight to fm_quadcore; a half-line [a, ∞) is split at
        a + L, with L the integrand's length scale (fm_qscan), into a finite piece and a tail; (-∞, ∞)
        is split at the scan's peak into two tails."""
        corek = self.kernel("fm_quadcore")
        fin = self.kernel("fm_quadfin")
        scan = self.kernel("fm_qscan")
        fn = self._new_fn("fm_quad", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64, F64, F64], inline=False)
        f, env, a, bb, rtol, atol = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        nosplit = b.alloca(F64, size=2)
        b.store(f64(0), nosplit)

        def core(args):
            if args[2].constant == 0:
                return b.call(fin, [args[0], args[1], args[3], args[4], args[5], args[6]])
            return b.call(corek, args + [nosplit])
        fabs = self.intrinsic("fabs")
        inf = f64(math.inf)
        with b.if_then(b.fcmp_ordered(">", a, bb)):
            b.ret(b.fsub(f64(0), b.call(fn, [f, env, bb, a, rtol, atol])))
        with b.if_then(b.fcmp_ordered("==", a, bb)):
            b.ret(f64(0))
        a_inf = b.fcmp_ordered("==", b.call(fabs, [a]), inf)
        b_inf = b.fcmp_ordered("==", b.call(fabs, [bb]), inf)
        with b.if_then(b.not_(b.or_(a_inf, b_inf))):
            b.ret(core([f, env, i64(0), a, bb, rtol, atol]))
        with b.if_then(b.not_(a_inf)):          # [a, ∞)
            L = b.call(scan, [f, env, a, f64(1)])
            m = b.fadd(a, L)
            b.ret(b.fadd(core([f, env, i64(0), a, m, rtol, atol]),
                         core([f, env, i64(1), m, L, rtol, atol])))
        with b.if_then(b.not_(b_inf)):          # (-∞, b]
            L = b.call(scan, [f, env, bb, f64(-1)])
            m = b.fsub(bb, L)
            b.ret(b.fadd(core([f, env, i64(2), m, L, rtol, atol]),
                         core([f, env, i64(0), m, bb, rtol, atol])))
        # (-∞, ∞): split at the larger of the two peaks the scans find
        lr = b.call(scan, [f, env, f64(0), f64(1)])
        ll = b.call(scan, [f, env, f64(0), f64(-1)])
        vr = b.fmul(b.call(fabs, [b.call(f, [lr, env])]), lr)
        vl = b.fmul(b.call(fabs, [b.call(f, [b.fneg(ll), env])]), ll)
        c = b.select(b.fcmp_unordered(">=", vr, vl), lr, b.fneg(ll))
        L = b.call(fabs, [c])
        b.ret(b.fadd(core([f, env, i64(2), c, L, rtol, atol]),
                     core([f, env, i64(1), c, L, rtol, atol])))
        return fn

    def _k_quadfin(self):
        """A finite range; if fm_quadcore finds an interior singularity, integrate up to it and from it,
        so it becomes an end-point singularity (which the smoothstep substitution handles)."""
        core = self.kernel("fm_quadcore")
        fn = self._new_fn("fm_quadfin", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64, F64, F64], inline=False)
        f, env, a, bb, rtol, atol = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        sp = b.alloca(F64, size=2)
        b.store(f64(1), sp)
        r = b.call(core, [f, env, i64(0), a, bb, rtol, atol, sp])
        with b.if_then(b.fcmp_ordered("!=", b.load(sp), f64(2))):
            b.ret(r)
        c = b.load(b.gep(sp, [i64(1)]))
        b.store(f64(0), sp)
        # both pieces start at c (u = 0, where the u grid is finest): ∫_a^c = -∫_c^a
        b.ret(b.fsub(b.call(core, [f, env, i64(0), c, bb, rtol, atol, sp]),
                     b.call(core, [f, env, i64(0), c, a, rtol, atol, sp])))
        return fn

    def _k_quadcore(self):
        """Globally adaptive Gauss–Kronrod (QUADPACK-style) in u ∈ [0, 1] (see fm_qf for the modes):
        keep a list of panels, always bisect the one with the largest error estimate, stop when the
        total error is small enough."""
        gk = self.kernel("fm_gk15")
        fn = self._new_fn("fm_quadcore", F64, [SCALAR_FN.as_pointer(), F64P, I64, F64, F64, F64, F64, F64P],
                          inline=False)
        f, env, mode, a, bb, rtol, atol, split = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        inf = f64(math.inf)
        lo, hi = f64(0), f64(1)
        M = self.QUAD_MAX
        mal = self.externs["malloc"]
        free = self.extern("free", VOID, [I8P])
        raw = b.call(mal, [i64(8 * 4 * M)])
        base = b.bitcast(raw, F64P)
        plo, phi, pres, perr = (b.gep(base, [i64(k * M)]) for k in range(4))
        errp = b.alloca(F64)
        n = b.alloca(I64)
        P = self.QUAD_PANELS
        width = b.fdiv(b.fsub(hi, lo), f64(P))
        with lp.range(i64(0), i64(P)) as i:
            x0 = b.fadd(lo, b.fmul(b.sitofp(i, F64), width))
            x1 = b.select(b.icmp_signed("==", i, i64(P - 1)), hi, b.fadd(x0, width))
            b.store(x0, b.gep(plo, [i]))
            b.store(x1, b.gep(phi, [i]))
            b.store(b.call(gk, [f, env, mode, a, bb, x0, x1, errp]), b.gep(pres, [i]))
            b.store(b.load(errp), b.gep(perr, [i]))
        b.store(i64(P), n)
        tot = b.alloca(F64)
        terr = b.alloca(F64)
        worst = b.alloca(I64)
        cond_bb = fn.append_basic_block("q.cond")
        body_bb = fn.append_basic_block("q.body")
        b.branch(cond_bb)
        b.position_at_end(cond_bb)
        b.store(f64(0), tot)
        b.store(f64(0), terr)
        b.store(i64(0), worst)
        cnt = b.load(n)
        with lp.range(i64(0), cnt) as i:
            e = b.load(b.gep(perr, [i]))
            b.store(b.fadd(b.load(tot), b.load(b.gep(pres, [i]))), tot)
            b.store(b.fadd(b.load(terr), e), terr)
            with b.if_then(b.fcmp_unordered(">", e, b.load(b.gep(perr, [b.load(worst)])))):
                b.store(i, worst)
        total, toterr = b.load(tot), b.load(terr)
        goal = b.call(self.intrinsic("maxnum"), [atol, b.fmul(rtol, b.call(fabs, [total]))])
        finite = b.fcmp_ordered("<", b.call(fabs, [total]), inf)
        done = b.and_(finite, b.fcmp_ordered("<=", toterr, goal))
        done = b.or_(done, b.and_(finite, b.fcmp_ordered("<=", toterr, b.fmul(f64(1e-14), b.call(fabs, [total])))))
        with b.if_then(done):
            b.call(free, [raw])
            b.ret(total)
        # not converged: give up with an error when out of budget, the result is not finite,
        # or the worst panel can't be split any more
        w = b.load(worst)
        wl, wh = b.load(b.gep(plo, [w])), b.load(b.gep(phi, [w]))
        mid = b.fmul(f64(0.5), b.fadd(wl, wh))
        # a panel near a singularity that has shrunk to a few hundred ulps can't usefully be split:
        # accept the result if the error is still small (≤ 1e-7 relative), else report it
        big = b.call(self.intrinsic("maxnum"), [b.call(fabs, [wl]), b.call(fabs, [wh])])
        stuck = b.fcmp_ordered("<=", b.fsub(wh, wl), b.fmul(f64(1e-13), big))
        with b.if_then(b.and_(stuck, b.and_(finite, b.fcmp_ordered("<=", toterr,
                                                                    b.fmul(f64(1e-7), b.call(fabs, [total])))))):
            b.call(free, [raw])
            b.ret(total)
        bad = b.or_(b.icmp_signed(">=", cnt, i64(M - 1)), stuck)
        # a finite range that gets stuck or runs out of panels may have an interior singularity: ask
        # the caller (fm_quadfin) to split the range there, snapping to 0 when it is that close
        with b.if_then(b.and_(bad, b.fcmp_ordered("==", b.load(split), f64(1)))):
            um = b.fsub(f64(1), mid)
            xl = b.fadd(a, b.fmul(b.fsub(bb, a), b.fmul(b.fmul(mid, mid), b.fsub(f64(3), b.fmul(f64(2), mid)))))
            xh = b.fsub(bb, b.fmul(b.fsub(bb, a), b.fmul(b.fmul(um, um), b.fadd(f64(1), b.fmul(f64(2), mid)))))
            c = b.select(b.fcmp_ordered("<=", mid, f64(0.5)), xl, xh)
            near0 = b.fcmp_ordered("<=", b.call(fabs, [c]), b.fmul(f64(1e-9), b.fsub(bb, a)))
            c = b.select(near0, f64(0), c)
            with b.if_then(b.and_(b.fcmp_ordered(">", c, a), b.fcmp_ordered("<", c, bb))):
                b.store(f64(2), split)
                b.store(c, b.gep(split, [i64(1)]))
                b.call(free, [raw])
                b.ret(f64(math.nan))
        bad = b.or_(bad, b.fcmp_unordered("uno", toterr, toterr))
        bad = b.or_(bad, b.fcmp_ordered("==", b.call(fabs, [total]), inf))
        with b.if_then(bad):
            self.raise_error(b, ERR_QUAD, total, toterr)
        b.branch(body_bb)
        b.position_at_end(body_bb)
        # bisect the worst panel: left half stays at index w, right half goes to the end
        r1 = b.call(gk, [f, env, mode, a, bb, wl, mid, errp])
        e1 = b.load(errp)
        r2 = b.call(gk, [f, env, mode, a, bb, mid, wh, errp])
        e2 = b.load(errp)
        b.store(mid, b.gep(phi, [w]))
        b.store(r1, b.gep(pres, [w]))
        b.store(e1, b.gep(perr, [w]))
        b.store(mid, b.gep(plo, [cnt]))
        b.store(wh, b.gep(phi, [cnt]))
        b.store(r2, b.gep(pres, [cnt]))
        b.store(e2, b.gep(perr, [cnt]))
        b.store(b.add(cnt, i64(1)), n)
        b.branch(cond_bb)
        return fn

    def _sol_alloc(self, b, dim, cap, arrays=True):
        mal = self.externs["malloc"]
        sp = b.bitcast(b.call(mal, [i64(48)]), SOLP)
        b.store(i64(0), b.gep(sp, [I32(0), I32(0)]))
        b.store(dim, b.gep(sp, [I32(0), I32(1)]))
        b.store(cap, b.gep(sp, [I32(0), I32(2)]))
        if not arrays:
            return sp
        tp = b.bitcast(b.call(mal, [b.mul(cap, i64(8))]), F64P)
        yp = b.bitcast(b.call(mal, [b.mul(b.mul(cap, dim), i64(8))]), F64P)
        dp = b.bitcast(b.call(mal, [b.mul(b.mul(cap, dim), i64(8))]), F64P)
        b.store(tp, b.gep(sp, [I32(0), I32(3)]))
        b.store(yp, b.gep(sp, [I32(0), I32(4)]))
        b.store(dp, b.gep(sp, [I32(0), I32(5)]))
        return sp

    def _k_sol_push(self):
        # append (t, y[0..n), dy[0..n)) to a solution, growing its arrays
        fn = self._new_fn("fm_sol_push", VOID, [SOLP, F64, F64P, F64P], inline=True)
        sp, t, y, dy = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        n = b.load(b.gep(sp, [I32(0), I32(0)]))
        dim = b.load(b.gep(sp, [I32(0), I32(1)]))
        cap = b.load(b.gep(sp, [I32(0), I32(2)]))
        with b.if_then(b.icmp_signed(">=", n, cap)):
            nc = b.mul(cap, i64(2))
            b.store(nc, b.gep(sp, [I32(0), I32(2)]))
            rea = self.externs["realloc"]
            for idx, per in ((3, None), (4, dim), (5, dim)):
                pp = b.gep(sp, [I32(0), I32(idx)])
                old = b.bitcast(b.load(pp), I8P)
                size = b.mul(nc, i64(8)) if per is None else b.mul(b.mul(nc, per), i64(8))
                b.store(b.bitcast(b.call(rea, [old, size]), F64P), pp)
        tp = b.load(b.gep(sp, [I32(0), I32(3)]))
        b.store(t, b.gep(tp, [n]))
        yp = b.load(b.gep(sp, [I32(0), I32(4)]))
        dp = b.load(b.gep(sp, [I32(0), I32(5)]))
        base = b.mul(n, dim)
        lp = LoopHelper(b, fn)
        with lp.range(i64(0), dim) as k:
            b.store(b.load(b.gep(y, [k])), b.gep(yp, [b.add(base, k)]))
            b.store(b.load(b.gep(dy, [k])), b.gep(dp, [b.add(base, k)]))
        b.store(b.add(n, i64(1)), b.gep(sp, [I32(0), I32(0)]))
        b.ret_void()
        return fn

    def _k_rk4(self):
        fn = self._new_fn("fm_rk4", SOLP, [ODE_FN.as_pointer(), F64P, I64, F64P, F64, F64, F64])
        f, env, n, y0, t0, t1, h0 = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        span = b.fsub(t1, t0)
        ratio = b.fdiv(span, h0)
        with b.if_then(b.or_(b.or_(b.fcmp_unordered("uno", ratio, ratio), b.fcmp_ordered(">", ratio, f64(1e12))),
                             b.fcmp_ordered("<=", ratio, f64(0)))):
            self.raise_error(b, ERR_STEP, h0, span)
        steps = b.fptosi(b.call(self.intrinsic("ceil"), [b.fsub(ratio, f64(1e-9))]), I64)
        steps = b.select(b.icmp_signed("<", steps, i64(1)), i64(1), steps)
        h = b.fdiv(span, b.sitofp(steps, F64))
        sp = self._sol_alloc(b, n, b.add(steps, i64(1)), arrays=False)
        mal = self.externs["malloc"]
        # The step count is known, so the output arrays are written directly (the pointers come straight
        # from malloc, so LLVM knows they don't alias the program's variables and can keep those in registers).
        cap = b.add(steps, i64(1))
        tp = b.bitcast(b.call(mal, [b.mul(cap, i64(8))]), F64P)
        yp = b.bitcast(b.call(mal, [b.mul(b.mul(cap, n), i64(8))]), F64P)
        dp = b.bitcast(b.call(mal, [b.mul(b.mul(cap, n), i64(8))]), F64P)
        b.store(tp, b.gep(sp, [I32(0), I32(3)]))
        b.store(yp, b.gep(sp, [I32(0), I32(4)]))
        b.store(dp, b.gep(sp, [I32(0), I32(5)]))
        b.store(cap, b.gep(sp, [I32(0), I32(0)]))

        def arr():
            return b.bitcast(b.call(mal, [b.mul(n, i64(8))]), F64P)
        y, k1, k2, k3, k4, tmp = arr(), arr(), arr(), arr(), arr(), arr()
        with lp.range(i64(0), n) as k:
            b.store(b.load(b.gep(y0, [k])), b.gep(y, [k]))
        half = b.fmul(h, f64(0.5))

        def record(idx, t, y, d):
            b.store(t, b.gep(tp, [idx]))
            base = b.mul(idx, n)
            with lp.range(i64(0), n) as k:
                b.store(b.load(b.gep(y, [k])), b.gep(yp, [b.add(base, k)]))
                b.store(b.load(b.gep(d, [k])), b.gep(dp, [b.add(base, k)]))
        with lp.range(i64(0), steps) as s:
            t = b.fadd(t0, b.fmul(b.sitofp(s, F64), h))
            b.call(f, [t, y, k1, env])
            record(s, t, y, k1)
            with lp.range(i64(0), n) as k:
                b.store(b.fadd(b.load(b.gep(y, [k])), b.fmul(half, b.load(b.gep(k1, [k])))), b.gep(tmp, [k]))
            th = b.fadd(t, half)
            b.call(f, [th, tmp, k2, env])
            with lp.range(i64(0), n) as k:
                b.store(b.fadd(b.load(b.gep(y, [k])), b.fmul(half, b.load(b.gep(k2, [k])))), b.gep(tmp, [k]))
            b.call(f, [th, tmp, k3, env])
            with lp.range(i64(0), n) as k:
                b.store(b.fadd(b.load(b.gep(y, [k])), b.fmul(h, b.load(b.gep(k3, [k])))), b.gep(tmp, [k]))
            b.call(f, [b.fadd(t, h), tmp, k4, env])
            h6 = b.fdiv(h, f64(6))
            with lp.range(i64(0), n) as k:
                s23 = b.fadd(b.load(b.gep(k2, [k])), b.load(b.gep(k3, [k])))
                acc = b.fadd(b.fadd(b.load(b.gep(k1, [k])), b.fmul(f64(2), s23)), b.load(b.gep(k4, [k])))
                b.store(b.fadd(b.load(b.gep(y, [k])), b.fmul(h6, acc)), b.gep(y, [k]))
        b.call(f, [t1, y, k1, env])
        record(steps, t1, y, k1)
        b.ret(sp)
        return fn

    def _k_dp45(self):
        push = self.kernel("fm_sol_push")
        fn = self._new_fn("fm_dp45", SOLP, [ODE_FN.as_pointer(), F64P, I64, F64P, F64, F64, F64])
        f, env, n, y0, t0, t1, rtol = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        mal = self.externs["malloc"]
        fabs = self.intrinsic("fabs")
        fmax = self.intrinsic("maxnum")
        fmin = self.intrinsic("minnum")

        def arr():
            return b.bitcast(b.call(mal, [b.mul(n, i64(8))]), F64P)
        y, ynew, tmp, ymax, _err = arr(), arr(), arr(), arr(), arr()
        k = [arr() for _ in range(7)]
        with lp.range(i64(0), n) as j:
            v = b.load(b.gep(y0, [j]))
            b.store(v, b.gep(y, [j]))
            b.store(b.call(fabs, [v]), b.gep(ymax, [j]))
        sp = self._sol_alloc(b, n, i64(256))
        span = b.fsub(t1, t0)
        with b.if_then(b.fcmp_ordered("<=", span, f64(0))):
            self.raise_error(b, ERR_STEP, span, f64(0))
        tv = b.alloca(F64)
        hv = b.alloca(F64)
        nsteps = b.alloca(I64)
        rejcount = b.alloca(I64)
        firstrej = b.alloca(F64)
        b.store(i64(0), rejcount)
        b.store(f64(0), firstrej)
        b.store(t0, tv)
        b.store(b.fmul(span, f64(1e-4)), hv)
        b.store(i64(0), nsteps)
        b.call(f, [t0, y, k[0], env])
        b.call(push, [sp, t0, y, k[0]])
        fmx = _err     # largest |dy/dt| seen so far, per component
        with lp.range(i64(0), n) as j:
            b.store(b.call(fabs, [b.load(b.gep(k[0], [j]))]), b.gep(fmx, [j]))
        # Dormand–Prince coefficients
        A = [[], [1 / 5], [3 / 40, 9 / 40], [44 / 45, -56 / 15, 32 / 9],
             [19372 / 6561, -25360 / 2187, 64448 / 6561, -212 / 729],
             [9017 / 3168, -355 / 33, 46732 / 5247, 49 / 176, -5103 / 18656],
             [35 / 384, 0, 500 / 1113, 125 / 192, -2187 / 6784, 11 / 84]]
        Cn = [0, 1 / 5, 3 / 10, 4 / 5, 8 / 9, 1, 1]
        E = [71 / 57600, 0, -71 / 16695, 71 / 1920, -17253 / 339200, 22 / 525, -1 / 40]
        cond_bb = fn.append_basic_block("dp.cond")
        body_bb = fn.append_basic_block("dp.body")
        end_bb = fn.append_basic_block("dp.end")
        b.branch(cond_bb)
        b.position_at_end(cond_bb)
        t = b.load(tv)
        remaining = b.fsub(t1, t)
        go = b.fcmp_ordered(">", remaining, b.fmul(f64(1e-14), b.call(fabs, [t1])))
        go = b.and_(go, b.fcmp_ordered(">", remaining, f64(0)))
        b.cbranch(go, body_bb, end_bb)
        b.position_at_end(body_bb)
        cnt = b.add(b.load(nsteps), i64(1))
        b.store(cnt, nsteps)
        with b.if_then(b.icmp_signed(">", cnt, i64(20_000_000))):
            self.raise_error(b, ERR_ODE_STEPS, t, f64(0))
        h = b.call(fmin, [b.load(hv), remaining])
        with b.if_then(b.fcmp_ordered("<", h, b.fmul(f64(1e-15), b.fadd(b.call(fabs, [t]), b.call(fabs, [span]))))):
            self.raise_error(b, ERR_ODE_H, t, h)
        for s in range(1, 7):
            with lp.range(i64(0), n) as j:
                acc = b.load(b.gep(y, [j]))
                for m in range(s):
                    if A[s][m] != 0:
                        acc = b.fadd(acc, b.fmul(b.fmul(h, f64(A[s][m])), b.load(b.gep(k[m], [j]))))
                b.store(acc, b.gep(ynew if s == 6 else tmp, [j]))
            ts = b.fadd(t, b.fmul(h, f64(Cn[s])))
            b.call(f, [ts, ynew if s == 6 else tmp, k[s], env])
        # error estimate (scale-free: relative to the size each component has reached)
        errsum = b.alloca(F64)
        b.store(f64(0), errsum)
        with lp.range(i64(0), n) as j:
            e = f64(0)
            for m in range(7):
                if E[m] != 0:
                    e = b.fadd(e, b.fmul(f64(E[m]), b.load(b.gep(k[m], [j]))))
            e = b.fmul(e, h)
            yo = b.call(fabs, [b.load(b.gep(y, [j]))])
            yn = b.call(fabs, [b.load(b.gep(ynew, [j]))])
            # relative error norm (scale-free, so it works in any units and keeps decays accurate):
            # relative to the size of the component, or to this step's own change when the component
            # passes through zero
            dlt = b.call(fabs, [b.fsub(b.load(b.gep(ynew, [j])), b.load(b.gep(y, [j])))])
            sc = b.fmul(rtol, b.fadd(b.call(fmax, [yo, yn]), dlt))
            sc = b.fadd(sc, f64(5e-324))   # smallest subnormal: only guards 0/0 (A37)
            r = b.fdiv(e, sc)
            b.store(b.fadd(b.load(errsum), b.fmul(r, r)), errsum)
        errn = b.call(self.intrinsic("sqrt"), [b.fdiv(b.load(errsum), b.sitofp(n, F64))])
        # step size factor
        pw = self.intrinsic("pow")
        fac = b.fmul(f64(0.9), b.call(pw, [b.call(fmax, [errn, f64(1e-10)]), f64(-0.2)]))
        fac = b.call(fmin, [f64(5.0), b.call(fmax, [f64(0.2), fac])])
        # If shrinking the step no longer reduces the (relative) error, the tolerance can't be met by
        # refining -- typically an unknown that starts at exactly 0 (x' = t⁴, x(0) = 0). Accept then.
        nrej = b.load(rejcount)
        stalled = b.and_(b.icmp_signed(">=", nrej, i64(4)),
                         b.fcmp_ordered(">=", errn, b.fmul(f64(0.5), b.load(firstrej))))
        accept = b.or_(b.fcmp_ordered("<=", errn, f64(1.0)), stalled)
        with b.if_else(accept) as (yes, no):
            with yes:
                tn = b.fadd(t, h)
                b.store(tn, tv)
                with lp.range(i64(0), n) as j:
                    v = b.load(b.gep(ynew, [j]))
                    b.store(v, b.gep(y, [j]))
                    b.store(b.call(fmax, [b.load(b.gep(ymax, [j])), b.call(fabs, [v])]), b.gep(ymax, [j]))
                    b.store(b.load(b.gep(k[6], [j])), b.gep(k[0], [j]))   # FSAL
                    b.store(b.call(fmax, [b.load(b.gep(fmx, [j])), b.call(fabs, [b.load(b.gep(k[6], [j]))])]),
                            b.gep(fmx, [j]))
                b.call(push, [sp, tn, y, k[0]])
                b.store(b.select(stalled, b.fmul(h, f64(2.0)), b.fmul(h, fac)), hv)
                b.store(i64(0), rejcount)
            with no:
                with b.if_then(b.icmp_signed("==", nrej, i64(0))):
                    b.store(errn, firstrej)
                b.store(b.add(nrej, i64(1)), rejcount)
                b.store(b.fmul(h, b.call(fmin, [fac, f64(1.0)])), hv)
        b.branch(cond_bb)
        b.position_at_end(end_bb)
        b.ret(sp)
        return fn

    def _k_sol_ext(self):
        """max (sgn = 1) or -min (sgn = -1) of a solution component: the best step point, refined on
        the quintic Hermite through the three nearest step points (values and exact slopes, O(h⁶))."""
        fn = self._new_fn("fm_sol_ext", F64, [SOLP, I64, F64], inline=False)
        sp, comp, sgn = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        n = b.load(b.gep(sp, [I32(0), I32(0)]))
        dim = b.load(b.gep(sp, [I32(0), I32(1)]))
        tp = b.load(b.gep(sp, [I32(0), I32(3)]))
        yp = b.load(b.gep(sp, [I32(0), I32(4)]))
        dp = b.load(b.gep(sp, [I32(0), I32(5)]))

        def at(ptr, k):
            return b.load(b.gep(ptr, [b.add(b.mul(k, dim), comp)]))
        best, bk = b.alloca(F64), b.alloca(I64)
        b.store(b.fmul(sgn, at(yp, i64(0))), best)
        b.store(i64(0), bk)
        with lp.range(i64(1), n) as i:
            v = b.fmul(sgn, at(yp, i))
            with b.if_then(b.fcmp_ordered(">", v, b.load(best))):
                b.store(v, best)
                b.store(i, bk)
        with b.if_then(b.icmp_signed("<", n, i64(3))):
            b.ret(b.load(best))
        k = b.load(bk)
        k = b.select(b.icmp_signed("<", k, i64(1)), i64(1), k)
        k = b.select(b.icmp_signed(">", k, b.sub(n, i64(2))), b.sub(n, i64(2)), k)
        ks = [b.sub(k, i64(1)), k, b.add(k, i64(1))]
        t = [b.load(b.gep(tp, [kk])) for kk in ks]
        y = [b.fmul(sgn, at(yp, kk)) for kk in ks]
        d = [b.fmul(sgn, at(dp, kk)) for kk in ks]
        coef = quintic_hermite(_IROps(b), t, y, d)
        z = [t[0], t[0], t[1], t[1], t[2], t[2]]

        def poly(x):
            acc = coef[5]
            for j in range(4, -1, -1):
                acc = b.fadd(coef[j], b.fmul(b.fsub(x, z[j]), acc))
            return acc
        lo, hi = b.alloca(F64), b.alloca(F64)
        b.store(t[0], lo)
        b.store(t[2], hi)
        g = (math.sqrt(5) - 1) / 2
        with lp.range(i64(0), i64(80)):       # golden-section search for the peak of the quintic
            l_, h_ = b.load(lo), b.load(hi)
            x1 = b.fsub(h_, b.fmul(f64(g), b.fsub(h_, l_)))
            x2 = b.fadd(l_, b.fmul(f64(g), b.fsub(h_, l_)))
            left = b.fcmp_ordered(">", poly(x1), poly(x2))
            b.store(b.select(left, l_, x1), lo)
            b.store(b.select(left, x2, h_), hi)
        pm = poly(b.fmul(f64(0.5), b.fadd(b.load(lo), b.load(hi))))
        b.ret(b.call(self.intrinsic("maxnum"), [b.load(best), pm]))
        return fn

    def _k_sol_eval(self):
        # cubic Hermite interpolation of component `comp` at time t (or linear on dy)
        fn = self._new_fn("fm_sol_eval", F64, [SOLP, I64, F64, I64], inline=False)
        sp, comp, t, use_dy = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        n = b.load(b.gep(sp, [I32(0), I32(0)]))
        dim = b.load(b.gep(sp, [I32(0), I32(1)]))
        tp = b.load(b.gep(sp, [I32(0), I32(3)]))
        yp = b.load(b.gep(sp, [I32(0), I32(4)]))
        dp = b.load(b.gep(sp, [I32(0), I32(5)]))
        tfirst = b.load(tp)
        tlast = b.load(b.gep(tp, [b.sub(n, i64(1))]))
        span = b.fsub(tlast, tfirst)
        slack = b.fmul(f64(1e-9), b.call(self.intrinsic("fabs"), [span]))
        bad = b.or_(b.fcmp_ordered("<", t, b.fsub(tfirst, slack)), b.fcmp_ordered(">", t, b.fadd(tlast, slack)))
        bad = b.or_(bad, b.fcmp_unordered("uno", t, t))
        with b.if_then(bad):
            self.raise_error(b, ERR_SOLRANGE, t, tlast)
        # binary search: largest i with t[i] <= t, 0 <= i <= n-2
        lo = b.alloca(I64)
        hi = b.alloca(I64)
        b.store(i64(0), lo)
        b.store(b.sub(n, i64(1)), hi)
        c_bb = fn.append_basic_block("bs.c")
        l_bb = fn.append_basic_block("bs.l")
        e_bb = fn.append_basic_block("bs.e")
        b.branch(c_bb)
        b.position_at_end(c_bb)
        b.cbranch(b.icmp_signed(">", b.sub(b.load(hi), b.load(lo)), i64(1)), l_bb, e_bb)
        b.position_at_end(l_bb)
        mid = b.sdiv(b.add(b.load(lo), b.load(hi)), i64(2))
        tm = b.load(b.gep(tp, [mid]))
        with b.if_else(b.fcmp_ordered("<=", tm, t)) as (yes, no):
            with yes:
                b.store(mid, lo)
            with no:
                b.store(mid, hi)
        b.branch(c_bb)
        b.position_at_end(e_bb)
        i = b.load(lo)
        with b.if_then(b.icmp_signed("<=", n, i64(1))):
            b.ret(b.load(b.gep(b.select(b.icmp_signed("!=", use_dy, i64(0)), dp, yp), [comp])))
        i1 = b.add(i, i64(1))
        ta = b.load(b.gep(tp, [i]))
        tb = b.load(b.gep(tp, [i1]))
        hh = b.fsub(tb, ta)
        s = b.fdiv(b.fsub(t, ta), hh)
        ia = b.add(b.mul(i, dim), comp)
        ib = b.add(b.mul(i1, dim), comp)
        ya = b.load(b.gep(yp, [ia]))
        yb = b.load(b.gep(yp, [ib]))
        ma = b.fmul(b.load(b.gep(dp, [ia])), hh)
        mb = b.fmul(b.load(b.gep(dp, [ib])), hh)
        s2 = b.fmul(s, s)
        s3 = b.fmul(s2, s)
        with b.if_then(b.icmp_signed("!=", use_dy, i64(0))):
            # derivative of the Hermite cubic: third-order accurate (linear interpolation of the
            # stored slopes would only be second order)
            d00 = b.fsub(b.fmul(f64(6), s2), b.fmul(f64(6), s))
            d10 = b.fadd(b.fsub(b.fmul(f64(3), s2), b.fmul(f64(4), s)), f64(1))
            d01 = b.fsub(b.fmul(f64(6), s), b.fmul(f64(6), s2))
            d11 = b.fsub(b.fmul(f64(3), s2), b.fmul(f64(2), s))
            num_ = b.fadd(b.fadd(b.fmul(d00, ya), b.fmul(d10, ma)), b.fadd(b.fmul(d01, yb), b.fmul(d11, mb)))
            b.ret(b.fdiv(num_, hh))
        h00 = b.fadd(b.fsub(b.fmul(f64(2), s3), b.fmul(f64(3), s2)), f64(1))
        h10 = b.fadd(b.fsub(s3, b.fmul(f64(2), s2)), s)
        h01 = b.fadd(b.fmul(f64(-2), s3), b.fmul(f64(3), s2))
        h11 = b.fsub(s3, s2)
        r = b.fadd(b.fadd(b.fmul(h00, ya), b.fmul(h10, ma)), b.fadd(b.fmul(h01, yb), b.fmul(h11, mb)))
        b.ret(r)
        return fn


class _IROps:
    def __init__(self, b):
        self.b = b

    def sub(self, x, y):
        return self.b.fsub(x, y)

    def div(self, x, y):
        return self.b.fdiv(x, y)


class LoopHelper:
    def __init__(self, b, fn):
        self.b = b
        self.fn = fn

    class _Range:
        def __init__(self, h, start, stop):
            self.h, self.start, self.stop = h, start, stop

        def __enter__(self):
            b, fn = self.h.b, self.h.fn
            self.iv = b.alloca(I64) if False else None
            entry = fn.entry_basic_block
            saved = b.block
            with b.goto_block(entry):
                b.position_at_start(entry)
                self.iv = b.alloca(I64)
            b.position_at_end(saved)
            b.store(self.start, self.iv)
            self.cond = fn.append_basic_block("r.c")
            self.body = fn.append_basic_block("r.b")
            self.end = fn.append_basic_block("r.e")
            b.branch(self.cond)
            b.position_at_end(self.cond)
            i = b.load(self.iv)
            b.cbranch(b.icmp_signed("<", i, self.stop), self.body, self.end)
            b.position_at_end(self.body)
            return b.load(self.iv)

        def __exit__(self, *exc):
            b = self.h.b
            if not b.block.is_terminated:
                b.store(b.add(b.load(self.iv), i64(1)), self.iv)
                b.branch(self.cond)
            b.position_at_end(self.end)
            return False

    def range(self, start, stop):
        return LoopHelper._Range(self, start, stop)


class FuncGen:
    """Emits the statements of one function (main or a user function instance)."""

    def __init__(self, mg: ModuleGen, fn, owner):
        self.mg = mg
        self.fn = fn
        self.owner = owner
        self.entry = fn.append_basic_block("entry")
        self.body_bb = fn.append_basic_block("body")
        self.b = ir.IRBuilder(self.body_bb)
        with self.b.goto_block(self.entry):
            self.b.branch(self.body_bb)
        self.slots = {}
        self.loops = []
        self.lp = LoopHelper(self.b, fn)

    # ------------------------------------------------------------ storage
    def alloca(self, ty, name=""):
        with self.b.goto_block(self.entry):
            self.b.position_at_start(self.entry)
            a = self.b.alloca(ty, name=name)
        return a

    def slot(self, sym):
        if sym.id in self.slots:
            return self.slots[sym.id]
        t = lltype(sym.ty)
        if sym.storage == "global":
            p = self.mg.global_for(sym)
        elif sym.storage == "arena":
            addr = self.mg.arena_base + 8 * sym.slot
            p = ir.Constant(I64, addr).inttoptr(t.as_pointer())
        else:
            p = self.alloca(t, sym.name)
            if isinstance(sym.ty, ListTy):
                pass
        self.slots[sym.id] = p
        return p

    def load(self, sym):
        return self.b.load(self.slot(sym))

    def store(self, sym, v):
        self.b.store(v, self.slot(sym))

    # ------------------------------------------------------------ statements
    def emit_body(self, stmts):
        for s in stmts:
            if self.b.block.is_terminated:
                break
            self.stmt(s)

    def stmt(self, s):
        if getattr(s, "line", 0):
            self.line = s.line
        m = getattr(self, "s_" + type(s).__name__)
        m(s)

    def mark_line(self):
        """Record the current source line for errors raised inside kernels."""
        if getattr(self, "line", 0):
            self.b.store(i64(self.line), self.mg.curline)

    def fail(self, kind, a=None, c=None):
        self.mg.raise_error(self.b, kind, a, c, getattr(self, "line", 0) or None)

    def s_SAssign(self, s):
        self.store(s.sym, self.expr(s.value))

    def s_SExpr(self, s):
        self.expr(s.value)

    def s_SIndexAssign(self, s):
        lst = self.load(s.sym)
        idx = self.expr(s.idx)
        p = self.elem_ptr(lst, idx)
        self.b.store(self.expr(s.value), p)

    def s_SPush(self, s):
        b = self.b
        v = self.expr(s.value)
        if isinstance(s.value.ty, StrTy):
            v = b.sitofp(v, F64)
        hdr = b.load(self.slot(s.sym))
        pdata = b.gep(hdr, [I32(0), I32(0)])
        plen = b.gep(hdr, [I32(0), I32(1)])
        pcap = b.gep(hdr, [I32(0), I32(2)])
        n, cap = b.load(plen), b.load(pcap)
        with b.if_then(b.icmp_signed(">=", n, cap)):
            nc = b.select(b.icmp_signed("<", cap, i64(4)), i64(8), b.mul(cap, i64(2)))
            # a fresh block (old one is never freed): other code may still be reading the old one
            nd = b.bitcast(b.call(self.mg.externs["malloc"], [b.mul(nc, i64(8))]), F64P)
            old = b.load(pdata)
            with self.lp.range(i64(0), n) as k:
                b.store(b.load(b.gep(old, [k])), b.gep(nd, [k]))
            b.store(nd, pdata)
            b.store(nc, pcap)
        b.store(v, b.gep(b.load(pdata), [n]))
        b.store(b.add(n, i64(1)), plen)

    def s_SIf(self, s):
        c = self.expr(s.cond)
        b = self.b
        if s.other:
            with b.if_else(c) as (then, other):
                with then:
                    self.emit_body(s.then)
                with other:
                    self.emit_body(s.other)
        else:
            with b.if_then(c):
                self.emit_body(s.then)
        self._fix_after_if()

    def _fix_after_if(self):
        # if both branches returned, the merge block is unreachable but must be terminated
        pass

    def s_SWhile(self, s):
        b, fn = self.b, self.fn
        cond = fn.append_basic_block("w.c")
        body = fn.append_basic_block("w.b")
        end = fn.append_basic_block("w.e")
        b.branch(cond)
        b.position_at_end(cond)
        b.cbranch(self.expr(s.cond), body, end)
        b.position_at_end(body)
        self.loops.append((cond, end))
        self.emit_body(s.body)
        self.loops.pop()
        if not b.block.is_terminated:
            b.branch(cond)
        b.position_at_end(end)

    def s_SFor(self, s):
        b, fn = self.b, self.fn
        lo = self.expr(s.lo)
        hi = self.expr(s.hi)
        st = self.expr(s.step)
        with b.if_then(b.fcmp_unordered("==", st, f64(0))):
            self.fail(ERR_STEP, st, f64(0))
        span = b.fdiv(b.fsub(hi, lo), st)
        cnt = b.fadd(b.call(self.mg.intrinsic("floor"), [b.fadd(span, f64(1e-9))]), f64(1))
        cnt = b.select(b.fcmp_ordered("<", cnt, f64(0)), f64(0), cnt)
        with b.if_then(b.fcmp_unordered("uno", cnt, cnt), likely=False):
            self.fail(ERR_RANGE, lo, hi)
        # a range to ∞ runs until a break
        cnt = b.select(b.fcmp_ordered(">", cnt, f64(2.0 ** 62)), f64(2.0 ** 62), cnt)
        n = b.fptosi(cnt, I64)
        iv = self.alloca(I64)
        b.store(i64(0), iv)
        cond = fn.append_basic_block("f.c")
        body = fn.append_basic_block("f.b")
        inc = fn.append_basic_block("f.i")
        end = fn.append_basic_block("f.e")
        b.branch(cond)
        b.position_at_end(cond)
        b.cbranch(b.icmp_signed("<", b.load(iv), n), body, end)
        b.position_at_end(body)
        self.store(s.sym, b.fadd(lo, b.fmul(b.sitofp(b.load(iv), F64), st)))
        self.loops.append((inc, end))
        self.emit_body(s.body)
        self.loops.pop()
        if not b.block.is_terminated:
            b.branch(inc)
        b.position_at_end(inc)
        b.store(b.add(b.load(iv), i64(1)), iv)
        b.branch(cond)
        b.position_at_end(end)

    def s_SForIn(self, s):
        b, fn = self.b, self.fn
        lst = self.expr(s.lst)
        data = self.ldata(lst)
        n = self.llen(lst)
        iv = self.alloca(I64)
        b.store(i64(0), iv)
        cond = fn.append_basic_block("fi.c")
        body = fn.append_basic_block("fi.b")
        inc = fn.append_basic_block("fi.i")
        end = fn.append_basic_block("fi.e")
        b.branch(cond)
        b.position_at_end(cond)
        b.cbranch(b.icmp_signed("<", b.load(iv), n), body, end)
        b.position_at_end(body)
        el = b.load(b.gep(data, [b.load(iv)]))
        self.store(s.sym, b.fptosi(el, I64) if isinstance(s.sym.ty, StrTy) else el)
        self.loops.append((inc, end))
        self.emit_body(s.body)
        self.loops.pop()
        if not b.block.is_terminated:
            b.branch(inc)
        b.position_at_end(inc)
        b.store(b.add(b.load(iv), i64(1)), iv)
        b.branch(cond)
        b.position_at_end(end)

    def s_SBreak(self, s):
        self.b.branch(self.loops[-1][1])

    def s_SContinue(self, s):
        self.b.branch(self.loops[-1][0])

    def s_SReturn(self, s):
        self.b.ret(self.expr(s.value))

    def s_SAssert(self, s):
        c = self.expr(s.cond)
        with self.b.if_then(self.b.not_(c)):
            self.fail(ERR_ASSERT, f64(s.msg_id))

    def s_SPrint(self, s):
        b, ex = self.b, self.mg.externs
        for kind, payload, fid in s.items:
            if kind == "num":
                b.call(ex["fm_print_num"], [i64(fid), self.expr(payload)])
            elif kind == "list":
                lst = self.expr(payload)
                b.call(ex["fm_print_list"], [i64(fid), self.ldata(lst), self.llen(lst)])
            elif kind == "vec":
                v = self.expr(payload)
                n = payload.ty.n
                arr = self.alloca(ir.ArrayType(F64, n))
                for k in range(n):
                    b.store(b.extract_element(v, I32(k)), b.gep(arr, [I32(0), I32(k)]))
                b.call(ex["fm_print_vec"], [i64(fid), b.gep(arr, [I32(0), I32(0)]), i64(n)])
            elif kind == "bool":
                b.call(ex["fm_print_bool"], [b.zext(self.expr(payload), I64)])
            elif kind in ("text", "data"):
                b.call(ex["fm_print_text"], [i64(fid)])
            elif kind == "textlist":
                lst = self.expr(payload)
                b.call(ex["fm_print_textlist"], [self.ldata(lst), self.llen(lst)])
            elif kind == "textvar":
                b.call(ex["fm_print_text"], [self.expr(payload)])
        b.call(ex["fm_print_end"], [])

    def s_SSolve(self, s):
        b = self.b
        n = sum(getattr(e.ty, "n", 1) for e in s.y0)
        y0 = self.alloca(ir.ArrayType(F64, n))
        i = 0
        for e in s.y0:
            v = self.expr(e)
            if isinstance(e.ty, VecTy):
                for k in range(e.ty.n):
                    b.store(b.extract_element(v, I32(k)), b.gep(y0, [I32(0), I32(i)]))
                    i += 1
            else:
                b.store(v, b.gep(y0, [I32(0), I32(i)]))
                i += 1
        y0p = b.gep(y0, [I32(0), I32(0)])
        fn = self.mg.lambda_for(s.rhs)
        env = self.make_env(s.rhs)
        t0 = self.expr(s.t0)
        t1 = self.expr(s.t1)
        if s.method == "rk4":
            self.mark_line()
            k = self.mg.kernel("fm_rk4")
            sol = b.call(k, [fn, env, i64(n), y0p, t0, t1, self.expr(s.step)])
        else:
            self.mark_line()
            k = self.mg.kernel("fm_dp45")
            sol = b.call(k, [fn, env, i64(n), y0p, t0, t1, f64(s.rtol)])
        self.store(s.sol_sym, sol)

    def make_env(self, lam):
        b = self.b
        if not lam.captures:
            return ir.Constant(F64P, None)
        env = self.alloca(ir.ArrayType(F64, len(lam.captures)))
        for i, sym in enumerate(lam.captures):
            v = self.load(sym)
            if isinstance(sym.ty, BoolTy):
                v = b.uitofp(v, F64)
            b.store(v, b.gep(env, [I32(0), I32(i)]))
        return b.gep(env, [I32(0), I32(0)])

    def s_SFit(self, s):
        b = self.b
        self.mg.lambda_for(s.model)
        n = len(s.param_syms)
        p = self.alloca(ir.ArrayType(F64, n))
        for i, g in enumerate(s.guesses):
            v = self.expr(g) if g is not None else f64(math.nan)
            b.store(v, b.gep(p, [I32(0), I32(i)]))
        h = self.expr(s.data)
        b.call(self.mg.externs["fm_fit"], [i64(s.fit_id), h, b.gep(p, [I32(0), I32(0)])])
        for i, sym in enumerate(s.param_syms):
            self.store(sym, b.load(b.gep(p, [I32(0), I32(i)])))

    def s_SPlot(self, s):
        b, ex = self.b, self.mg.externs
        for idx, e in enumerate(s.series):
            kind = e["kind"]
            if kind == "lists":
                y = self.expr(e["y"])
                x = self.expr(e["x"])
                b.call(ex["fm_plot_series"], [i64(s.plot_id), i64(idx), self.ldata(x), self.llen(x),
                                              self.ldata(y), self.llen(y)])
            elif kind in ("sol", "solxy"):
                sol = b.bitcast(self.expr(e["sol"]), I8P)
                c2 = e.get("comp2", -1)
                d2 = 1 if e.get("dy2") else 0
                b.call(ex["fm_plot_sol"], [i64(s.plot_id), i64(idx), sol, i64(e["comp"]), i64(1 if e["dy"] else 0),
                                           i64(c2), i64(d2)])
            elif kind == "func":
                npts = 400
                lo = self.expr(e["lo"])
                hi = self.expr(e["hi"])
                fn = self.mg.lambda_for(e["lam"])
                env = self.make_env(e["lam"])
                xs = b.bitcast(b.call(ex["malloc"], [i64(8 * npts)]), F64P)
                ys = b.bitcast(b.call(ex["malloc"], [i64(8 * npts)]), F64P)
                dx = b.fdiv(b.fsub(hi, lo), f64(npts - 1))
                with self.lp.range(i64(0), i64(npts)) as i:
                    x = b.fadd(lo, b.fmul(b.sitofp(i, F64), dx))
                    b.store(x, b.gep(xs, [i]))
                    b.store(b.call(fn, [x, env]), b.gep(ys, [i]))
                b.call(ex["fm_plot_series"], [i64(s.plot_id), i64(idx), xs, i64(npts), ys, i64(npts)])
        b.call(ex["fm_plot_done"], [i64(s.plot_id)])

    # ------------------------------------------------------------ expressions
    def expr(self, e):
        m = getattr(self, "e_" + type(e).__name__)
        return m(e)

    def e_IConst(self, e):
        return f64(e.value)

    def e_IBool(self, e):
        return ir.Constant(I1, 1 if e.value else 0)

    def e_IStr(self, e):
        return i64(getattr(e, "text_id", 0))

    def e_IVar(self, e):
        return self.load(e.sym)

    def list_count(self, nf):
        """A list length computed from a number: NaN or more than MAX_LIST is an error, negative is 0."""
        b = self.b
        with b.if_then(b.fcmp_unordered(">", nf, f64(MAX_LIST)), likely=False):
            self.fail(ERR_SIZE, nf, f64(0))
        n = b.fptosi(nf, I64)
        return b.select(b.icmp_signed("<", n, i64(0)), i64(0), n)

    def new_list(self, n):
        b = self.b
        data = b.bitcast(b.call(self.mg.externs["malloc"], [b.mul(b.select(b.icmp_signed("<", n, i64(1)), i64(1), n),
                                                                  i64(8))]), F64P)
        return self.make_header(data, n), data

    def make_header(self, data, n):
        b = self.b
        hdr = b.bitcast(b.call(self.mg.externs["malloc"], [i64(24)]), LIST)
        b.store(data, b.gep(hdr, [I32(0), I32(0)]))
        b.store(n, b.gep(hdr, [I32(0), I32(1)]))
        b.store(n, b.gep(hdr, [I32(0), I32(2)]))
        return hdr

    def ldata(self, lst):
        return self.b.load(self.b.gep(lst, [I32(0), I32(0)]))

    def llen(self, lst):
        return self.b.load(self.b.gep(lst, [I32(0), I32(1)]))

    def map_list(self, lst, fn_elem):
        """New list with fn_elem(x) for each element."""
        b = self.b
        src = self.ldata(lst)
        n = self.llen(lst)
        out, data = self.new_list(n)
        with self.lp.range(i64(0), n) as i:
            b.store(fn_elem(b.load(b.gep(src, [i])), i), b.gep(data, [i]))
        return out

    def splat(self, x, n):
        v = ir.Constant(ir.VectorType(F64, n), ir.Undefined)
        for k in range(n):
            v = self.b.insert_element(v, x, I32(k))
        return v

    def e_IVec(self, e):
        v = ir.Constant(lltype(e.ty), ir.Undefined)
        for k, it in enumerate(e.items):
            v = self.b.insert_element(v, self.expr(it), I32(k))
        return v

    def e_IVecElem(self, e):
        return self.b.extract_element(self.expr(e.v), I32(e.k))

    def hsum(self, v, n):
        b = self.b
        acc = b.extract_element(v, I32(0))
        for k in range(1, n):
            acc = b.fadd(acc, b.extract_element(v, I32(k)))
        return acc

    def e_IBin(self, e):
        b = self.b
        a = self.expr(e.a)
        c = self.expr(e.b)
        op = {"+": b.fadd, "-": b.fsub, "*": b.fmul, "/": b.fdiv}[e.op]
        if isinstance(e.ty, VecTy):
            n = e.ty.n
            if not isinstance(e.a.ty, VecTy):
                a = self.splat(a, n)
            if not isinstance(e.b.ty, VecTy):
                c = self.splat(c, n)
            return op(a, c)
        la = isinstance(e.a.ty, ListTy)
        lc = isinstance(e.b.ty, ListTy)
        if not la and not lc:
            return op(a, c)
        if la and lc:
            na = self.llen(a)
            nc = self.llen(c)
            with b.if_then(b.icmp_signed("!=", na, nc)):
                self.fail(ERR_LEN, b.sitofp(na, F64), b.sitofp(nc, F64))
            cd = self.ldata(c)
            return self.map_list(a, lambda x, i: op(x, b.load(b.gep(cd, [i]))))
        if la:
            return self.map_list(a, lambda x, i: op(x, c))
        return self.map_list(c, lambda x, i: op(a, x))

    def powc(self, x, p):
        b = self.b
        if p == 2:
            return b.fmul(x, x)
        if p == 3:
            return b.fmul(b.fmul(x, x), x)
        if p == 1:
            return x
        if p == 0.5:
            return b.call(self.mg.intrinsic("sqrt"), [x])
        if p == -1:
            return b.fdiv(f64(1), x)
        if p == -2:
            return b.fdiv(f64(1), b.fmul(x, x))
        if p == 4:
            x2 = b.fmul(x, x)
            return b.fmul(x2, x2)
        if p == -0.5:
            return b.fdiv(f64(1), b.call(self.mg.intrinsic("sqrt"), [x]))
        if p == 1.5:
            return b.fmul(x, b.call(self.mg.intrinsic("sqrt"), [x]))
        if p == -1.5:
            return b.fdiv(f64(1), b.fmul(x, b.call(self.mg.intrinsic("sqrt"), [x])))
        if abs(p - 1 / 3) < 1e-15:
            return b.call(self.mg.libm("cbrt"), [x])
        n = odd_root_numerator(p)
        if n is not None:           # x^(n/q), q odd: the real root, also for x < 0 (like cbrt)
            r = b.call(self.mg.intrinsic("pow"), [b.call(self.mg.intrinsic("fabs"), [x]), f64(p)])
            return b.call(self.mg.intrinsic("copysign"), [r, x]) if n % 2 else r
        if p == int(p) and abs(p) <= 16:
            return b.call(self.mg.intrinsic("powi") if False else self.mg.intrinsic("pow"), [x, f64(p)])
        return b.call(self.mg.intrinsic("pow"), [x, f64(p)])

    def e_IPowC(self, e):
        a = self.expr(e.a)
        if isinstance(e.a.ty, ListTy):
            return self.map_list(a, lambda x, i: self.powc(x, e.p))
        return self.powc(a, e.p)

    def e_IPow(self, e):
        a = self.expr(e.a)
        c = self.expr(e.b)
        pw = self.mg.intrinsic("pow")
        if isinstance(e.a.ty, ListTy):
            return self.map_list(a, lambda x, i: self.b.call(pw, [x, c]))
        return self.b.call(pw, [a, c])

    def e_INeg(self, e):
        a = self.expr(e.a)
        if isinstance(e.ty, ListTy):
            return self.map_list(a, lambda x, i: self.b.fneg(x))
        return self.b.fneg(a)

    def e_ICmp(self, e):
        b = self.b
        a = self.expr(e.a)
        c = self.expr(e.b)
        if isinstance(e.a.ty, BoolTy):
            return b.icmp_unsigned(e.op, a, c)
        if e.op == "~=":
            fabs = self.mg.intrinsic("fabs")
            diff = b.call(fabs, [b.fsub(a, c)])
            scale = b.call(self.mg.intrinsic("maxnum"), [b.call(fabs, [a]), b.call(fabs, [c])])
            return b.fcmp_ordered("<=", diff, b.fadd(b.fmul(scale, f64(1e-6)), f64(1e-300)))
        if e.op == "!=":
            return b.fcmp_unordered("!=", a, c)
        return b.fcmp_ordered(e.op, a, c)

    def e_ILogic(self, e):
        b = self.b
        a = self.expr(e.a)
        start = b.block
        rhs = self.fn.append_basic_block("l.r")
        end = self.fn.append_basic_block("l.e")
        if e.op == "and":
            b.cbranch(a, rhs, end)
        else:
            b.cbranch(a, end, rhs)
        b.position_at_end(rhs)
        c = self.expr(e.b)
        rhs_end = b.block
        b.branch(end)
        b.position_at_end(end)
        phi = b.phi(I1)
        phi.add_incoming(ir.Constant(I1, 0 if e.op == "and" else 1), start)
        phi.add_incoming(c, rhs_end)
        return phi

    def e_INot(self, e):
        return self.b.not_(self.expr(e.a))

    def e_IIf(self, e):
        b = self.b
        c = self.expr(e.cond)
        t_bb = self.fn.append_basic_block("if.t")
        f_bb = self.fn.append_basic_block("if.f")
        end = self.fn.append_basic_block("if.e")
        b.cbranch(c, t_bb, f_bb)
        b.position_at_end(t_bb)
        va = self.expr(e.a)
        ta = b.block
        b.branch(end)
        b.position_at_end(f_bb)
        vb = self.expr(e.b)
        tb = b.block
        b.branch(end)
        b.position_at_end(end)
        phi = b.phi(lltype(e.ty))
        phi.add_incoming(va, ta)
        phi.add_incoming(vb, tb)
        return phi

    def e_ILet(self, e):
        for sym, v in e.binds:
            self.store(sym, self.expr(v))
        return self.expr(e.value)

    def e_ICall(self, e):
        fn = self.mg.func_for(e.func)
        return self.b.call(fn, [self.expr(a) for a in e.args])

    def e_IMap(self, e):
        b = self.b
        fn = self.mg.func_for(e.func)
        args = [self.expr(a) for a in e.args]
        lst = args[e.list_pos]

        def elem(x, i):
            a2 = list(args)
            a2[e.list_pos] = x
            return b.call(fn, a2)
        return self.map_list(lst, elem)

    def e_IList(self, e):
        b = self.b
        out, data = self.new_list(i64(len(e.items)))
        for i, it in enumerate(e.items):
            v = self.expr(it)
            if isinstance(it.ty, StrTy):
                v = b.sitofp(v, F64)
            b.store(v, b.gep(data, [i64(i)]))
        return out

    def elem_ptr(self, lst, idx):
        b = self.b
        n = self.llen(lst)
        # check in floating point first: fptosi of NaN or of a number beyond 2^63 is undefined
        inside = b.and_(b.fcmp_ordered(">=", idx, f64(1)), b.fcmp_ordered("<=", idx, b.sitofp(n, F64)))
        i = b.fptosi(b.select(inside, idx, f64(1)), I64)
        bad = b.or_(b.not_(inside), b.fcmp_unordered("!=", b.sitofp(i, F64), idx))
        with b.if_then(bad, likely=False):
            self.fail(ERR_INDEX, idx, b.sitofp(n, F64))
        return b.gep(self.ldata(lst), [b.sub(i, i64(1))])

    def e_IIndex(self, e):
        lst = self.expr(e.lst)
        v = self.b.load(self.elem_ptr(lst, self.expr(e.idx)))
        return self.b.fptosi(v, I64) if isinstance(e.ty, StrTy) else v

    def e_IIntegral(self, e):
        fn = self.mg.lambda_for(e.lam)
        env = self.make_env(e.lam)
        self.mark_line()
        q = self.mg.kernel("fm_quad")
        return self.b.call(q, [fn, env, self.expr(e.lo), self.expr(e.hi), f64(1e-10), f64(0)])

    def e_ISolEval(self, e):
        self.b.store(i64(getattr(e, "tfmt", -1)), self.mg.errfmt)
        self.mark_line()
        k = self.mg.kernel("fm_sol_eval")
        return self.b.call(k, [self.expr(e.sol), i64(e.comp), self.expr(e.t), i64(1 if e.use_dy else 0)])

    def e_ISolList(self, e):
        b = self.b
        sp = self.expr(e.sol)
        n = b.load(b.gep(sp, [I32(0), I32(0)]))
        dim = b.load(b.gep(sp, [I32(0), I32(1)]))
        out, data = self.new_list(n)
        if e.what == "t":
            src = b.load(b.gep(sp, [I32(0), I32(3)]))
            with self.lp.range(i64(0), n) as i:
                b.store(b.load(b.gep(src, [i])), b.gep(data, [i]))
        else:
            src = b.load(b.gep(sp, [I32(0), I32(4 if e.what == "y" else 5)]))
            with self.lp.range(i64(0), n) as i:
                b.store(b.load(b.gep(src, [b.add(b.mul(i, dim), i64(e.comp))])), b.gep(data, [i]))
        return out

    def e_ILoad(self, e):
        return self.b.call(self.mg.externs["fm_load"], [i64(e.load_id)])

    def e_IColumn(self, e):
        b = self.b
        h = self.expr(e.data)
        pp = self.alloca(F64P)
        n = b.call(self.mg.externs["fm_column"], [h, i64(e.col), pp])
        return self.make_header(b.load(pp), n)

    # ------------------------------------------------------------ builtins
    MATH_INTRINSICS = {"sin": "sin", "cos": "cos", "exp": "exp", "ln": "log", "log": "log", "log10": "log10",
                       "log2": "log2", "abs": "fabs", "floor": "floor", "ceil": "ceil", "round": "round",
                       "tan": "tan", "asin": "asin", "acos": "acos", "atan": "atan", "sinh": "sinh",
                       "cosh": "cosh", "tanh": "tanh"}
    MATH_LIBM = {"asinh": "asinh", "acosh": "acosh", "atanh": "atanh", "erf": "erf", "erfc": "erfc",
                 "gamma": "tgamma", "lgamma": "lgamma", "expm1": "expm1", "log1p": "log1p"}
    NEW_INTRINSICS = {"tan", "asin", "acos", "atan", "sinh", "cosh", "tanh"}

    def math1(self, name, x):
        b = self.b
        if name in self.MATH_INTRINSICS and name not in self.NEW_INTRINSICS:
            return b.call(self.mg.intrinsic(self.MATH_INTRINSICS[name]), [x])
        if name in self.NEW_INTRINSICS:
            return b.call(self.mg.libm(name), [x])
        if name in self.MATH_LIBM:
            return b.call(self.mg.libm(self.MATH_LIBM[name]), [x])
        if name == "sign":
            pos = b.uitofp(b.fcmp_ordered(">", x, f64(0)), F64)
            neg = b.uitofp(b.fcmp_ordered("<", x, f64(0)), F64)
            return b.fsub(pos, neg)
        raise KeyError(name)

    def e_IBuiltin(self, e):
        b = self.b
        name = e.name
        if name in ("min_list", "max_list") and isinstance(e.args[0], I.ISolList) and e.args[0].what == "y":
            sg = f64(1 if name == "max_list" else -1)      # refined between step points (A55)
            r = b.call(self.mg.kernel("fm_sol_ext"), [self.expr(e.args[0].sol), i64(e.args[0].comp), sg])
            return b.fmul(sg, r)
        args = [self.expr(a) for a in e.args]
        if name in ("vdot", "norm", "unit", "cross"):
            n = e.args[0].ty.n
            if name == "vdot":
                return self.hsum(b.fmul(args[0], args[1]), n)
            if name == "cross":
                a, c = args
                x = [b.extract_element(a, I32(k)) for k in range(n)]
                y = [b.extract_element(c, I32(k)) for k in range(n)]
                if n == 2:
                    return b.fsub(b.fmul(x[0], y[1]), b.fmul(x[1], y[0]))
                comps = [b.fsub(b.fmul(x[1], y[2]), b.fmul(x[2], y[1])),
                         b.fsub(b.fmul(x[2], y[0]), b.fmul(x[0], y[2])),
                         b.fsub(b.fmul(x[0], y[1]), b.fmul(x[1], y[0]))]
                v = ir.Constant(ir.VectorType(F64, 3), ir.Undefined)
                for k in range(3):
                    v = b.insert_element(v, comps[k], I32(k))
                return v
            nrm = b.call(self.mg.intrinsic("sqrt"), [self.hsum(b.fmul(args[0], args[0]), n)])
            if name == "norm":
                return nrm
            return b.fdiv(args[0], self.splat(nrm, n))
        if name in self.MATH_INTRINSICS or name in self.MATH_LIBM or name == "sign":
            if isinstance(e.args[0].ty, ListTy):
                return self.map_list(args[0], lambda x, i: self.math1(name, x))
            return self.math1(name, args[0])
        if name == "isnan":
            return b.fcmp_unordered("uno", args[0], args[0])
        if name == "atan2":
            return b.call(self.mg.libm("atan2", 2), args)
        if name == "hypot":
            return b.call(self.mg.libm("hypot", 2), args)
        if name == "mod":
            a, c = args
            return b.fsub(a, b.fmul(c, b.call(self.mg.intrinsic("floor"), [b.fdiv(a, c)])))
        if name in ("min", "max"):
            fn = self.mg.intrinsic("minnum" if name == "min" else "maxnum")
            r = args[0]
            for a in args[1:]:
                r = b.call(fn, [r, a])
            return r
        if name == "clamp":
            x, lo, hi = args
            return b.call(self.mg.intrinsic("minnum"), [b.call(self.mg.intrinsic("maxnum"), [x, lo]), hi])
        if name == "factorial":
            return b.call(self.mg.libm("tgamma"), [b.fadd(args[0], f64(1))])
        if name == "rand":
            return b.call(self.mg.externs["drand48"], [])
        if name == "clock":
            return b.call(self.mg.externs["fm_clock"], [])
        if name == "len":
            return b.sitofp(self.llen(args[0]), F64)
        if name in ("sum", "mean", "std", "min_list", "max_list", "first", "last"):
            return self.reduce(name, args[0])
        if name == "dot":
            a, c = args
            na = self.llen(a)
            with b.if_then(b.icmp_signed("!=", na, self.llen(c))):
                self.fail(ERR_LEN, b.sitofp(na, F64), b.sitofp(self.llen(c), F64))
            acc = self.alloca(F64)
            b.store(f64(0), acc)
            pa, pc = self.ldata(a), self.ldata(c)
            with self.lp.range(i64(0), na) as i:
                b.store(b.fadd(b.load(acc), b.fmul(b.load(b.gep(pa, [i])), b.load(b.gep(pc, [i])))), acc)
            return b.load(acc)
        if name == "trapz":
            ys, xs = args
            n = self.llen(ys)
            with b.if_then(b.icmp_signed("!=", n, self.llen(xs))):
                self.fail(ERR_LEN, b.sitofp(n, F64), b.sitofp(self.llen(xs), F64))
            acc = self.alloca(F64)
            b.store(f64(0), acc)
            py, px = self.ldata(ys), self.ldata(xs)
            with self.lp.range(i64(1), n) as i:
                im = b.sub(i, i64(1))
                dx = b.fsub(b.load(b.gep(px, [i])), b.load(b.gep(px, [im])))
                s = b.fadd(b.load(b.gep(py, [i])), b.load(b.gep(py, [im])))
                b.store(b.fadd(b.load(acc), b.fmul(f64(0.5), b.fmul(dx, s))), acc)
            return b.load(acc)
        if name == "interp":
            x, xs, ys = args
            n = self.llen(xs)
            px, py = self.ldata(xs), self.ldata(ys)
            with b.if_then(b.icmp_signed("!=", n, self.llen(ys))):
                self.fail(ERR_LEN, b.sitofp(n, F64), b.sitofp(self.llen(ys), F64))
            with b.if_then(b.icmp_signed("<", n, i64(2))):
                self.fail(ERR_EMPTY, f64(0), f64(0))
            res = self.alloca(F64)
            # clamp to the ends
            b.store(b.load(py), res)
            last = b.sub(n, i64(1))
            with b.if_then(b.fcmp_ordered(">=", x, b.load(b.gep(px, [last])))):
                b.store(b.load(b.gep(py, [last])), res)
            with self.lp.range(i64(1), n) as i:
                im = b.sub(i, i64(1))
                xa, xb = b.load(b.gep(px, [im])), b.load(b.gep(px, [i]))
                inside = b.and_(b.fcmp_ordered(">=", x, xa), b.fcmp_ordered("<", x, xb))
                with b.if_then(inside):
                    ya, yb = b.load(b.gep(py, [im])), b.load(b.gep(py, [i]))
                    s = b.fdiv(b.fsub(x, xa), b.fsub(xb, xa))
                    b.store(b.fadd(ya, b.fmul(s, b.fsub(yb, ya))), res)
            return b.load(res)
        if name in ("zeros", "ones"):
            n = self.list_count(args[0])
            out, data = self.new_list(n)
            with self.lp.range(i64(0), n) as i:
                b.store(f64(0 if name == "zeros" else 1), b.gep(data, [i]))
            return out
        if name == "linspace":
            a, c, nf = args
            n = self.list_count(nf)
            out, data = self.new_list(n)
            den = b.sitofp(b.select(b.icmp_signed("<", n, i64(2)), i64(1), b.sub(n, i64(1))), F64)
            step = b.fdiv(b.fsub(c, a), den)
            with self.lp.range(i64(0), n) as i:
                b.store(b.fadd(a, b.fmul(b.sitofp(i, F64), step)), b.gep(data, [i]))
            return out
        if name == "range":
            a, c, st = args
            with b.if_then(b.fcmp_unordered("==", st, f64(0))):
                self.fail(ERR_STEP, st, f64(0))
            cnt = b.fadd(b.call(self.mg.intrinsic("floor"), [b.fadd(b.fdiv(b.fsub(c, a), st), f64(1e-9))]), f64(1))
            cnt = b.select(b.fcmp_ordered("<", cnt, f64(0)), f64(0), cnt)
            n = self.list_count(cnt)
            out, data = self.new_list(n)
            with self.lp.range(i64(0), n) as i:
                b.store(b.fadd(a, b.fmul(b.sitofp(i, F64), st)), b.gep(data, [i]))
            return out
        if name == "copy":
            return self.map_list(args[0], lambda x, i: x)
        if name == "reverse":
            src = self.ldata(args[0])
            n = self.llen(args[0])
            out, data = self.new_list(n)
            with self.lp.range(i64(0), n) as i:
                b.store(b.load(b.gep(src, [b.sub(b.sub(n, i64(1)), i)])), b.gep(data, [i]))
            return out
        if name == "sort":
            out = self.map_list(args[0], lambda x, i: x)
            b.call(self.mg.externs["fm_sort"], [self.ldata(out), self.llen(out)])
            return out
        if name == "cumsum":
            acc = self.alloca(F64)
            b.store(f64(0), acc)

            def step(x, i):
                v = b.fadd(b.load(acc), x)
                b.store(v, acc)
                return v
            return self.map_list(args[0], step)
        if name == "diff":
            src = self.ldata(args[0])
            n = self.llen(args[0])
            m = b.select(b.icmp_signed("<", n, i64(1)), i64(0), b.sub(n, i64(1)))
            out, data = self.new_list(m)
            with self.lp.range(i64(0), m) as i:
                b.store(b.fsub(b.load(b.gep(src, [b.add(i, i64(1))])), b.load(b.gep(src, [i]))), b.gep(data, [i]))
            return out
        raise NotImplementedError(f"builtin {name}")

    def reduce(self, name, lst):
        b = self.b
        data = self.ldata(lst)
        n = self.llen(lst)
        if name in ("mean", "std", "min_list", "max_list", "first", "last"):
            with b.if_then(b.icmp_signed("<", n, i64(1))):
                self.fail(ERR_EMPTY, f64(0), f64(0))
        if name == "first":
            return b.load(data)
        if name == "last":
            return b.load(b.gep(data, [b.sub(n, i64(1))]))
        acc = self.alloca(F64)
        if name in ("min_list", "max_list"):
            b.store(b.load(data), acc)
            fn = self.mg.intrinsic("minnum" if name == "min_list" else "maxnum")
            with self.lp.range(i64(1), n) as i:
                b.store(b.call(fn, [b.load(acc), b.load(b.gep(data, [i]))]), acc)
            return b.load(acc)
        b.store(f64(0), acc)
        with self.lp.range(i64(0), n) as i:
            b.store(b.fadd(b.load(acc), b.load(b.gep(data, [i]))), acc)
        s = b.load(acc)
        if name == "sum":
            return s
        mean = b.fdiv(s, b.sitofp(n, F64))
        if name == "mean":
            return mean
        # sample standard deviation (n-1)
        b.store(f64(0), acc)
        with self.lp.range(i64(0), n) as i:
            d = b.fsub(b.load(b.gep(data, [i])), mean)
            b.store(b.fadd(b.load(acc), b.fmul(d, d)), acc)
        den = b.sitofp(b.select(b.icmp_signed("<", n, i64(2)), i64(1), b.sub(n, i64(1))), F64)
        return b.call(self.mg.intrinsic("sqrt"), [b.fdiv(b.load(acc), den)])


class LambdaGen(FuncGen):
    def __init__(self, mg, fn, lam: I.ILambda):
        super().__init__(mg, fn, lam)
        self.lam = lam

    def load_env(self, env):
        b = self.b
        for i, sym in enumerate(self.lam.captures):
            v = b.load(b.gep(env, [i64(i)]))
            if isinstance(sym.ty, BoolTy):
                v = b.fcmp_ordered("!=", v, f64(0))
            p = self.alloca(lltype(sym.ty), sym.name)
            b.store(v, p)
            self.slots[sym.id] = p

    def emit(self):
        lam, b, fn = self.lam, self.b, self.fn
        if lam.kind == "scalar":
            x, env = fn.args
            self.load_env(env)
            self.store(lam.params[0], x)
            b.ret(self.expr(lam.body))
        elif lam.kind == "ode":
            t, y, dy, env = fn.args
            self.load_env(env)
            self.store(lam.params[0], t)
            off = 0
            for sym in lam.state:
                if isinstance(sym.ty, VecTy):
                    v = ir.Constant(lltype(sym.ty), ir.Undefined)
                    for k in range(sym.ty.n):
                        v = b.insert_element(v, b.load(b.gep(y, [i64(off + k)])), I32(k))
                    self.store(sym, v)
                    off += sym.ty.n
                else:
                    self.store(sym, b.load(b.gep(y, [i64(off)])))
                    off += 1
            vals = [(self.expr(e), e.ty) for e in lam.body]
            off = 0
            for v, ty in vals:
                if isinstance(ty, VecTy):
                    for k in range(ty.n):
                        b.store(b.extract_element(v, I32(k)), b.gep(dy, [i64(off + k)]))
                    off += ty.n
                else:
                    b.store(v, b.gep(dy, [i64(off)]))
                    off += 1
            b.ret_void()
        elif lam.kind == "model":
            p, cols, n, out = fn.args
            for i, sym in enumerate(lam.param_syms):
                self.store(sym, b.load(b.gep(p, [i64(i)])))
            with self.lp.range(i64(0), n) as i:
                for k, sym in enumerate(lam.col_syms):
                    colp = b.load(b.gep(cols, [i64(k)]))
                    self.store(sym, b.load(b.gep(colp, [i])))
                b.store(self.expr(lam.body), b.gep(out, [i]))
            b.ret_void()
