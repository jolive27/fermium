"""LLVM code generation: typed IR -> LLVM IR (via llvmlite) -> native code (MCJIT).

The IR contains only SI numbers, so no unit logic appears here (spec §3.3).
Numerical kernels (adaptive Gauss–Kronrod quadrature, RK4, Dormand–Prince RK45,
Hermite interpolation of ODE solutions) are generated here as LLVM IR too, so
the integrand / right-hand side is inlined into them and everything is native.
"""
from __future__ import annotations

import math

from llvmlite import ir

from .numerics import quintic_hermite, odd_root_numerator, XGK, WGK, WG
from . import ir as I
from .types import NumTy, BoolTy, ListTy, SolTy, DataTy, StrTy, VecTy, MatTy, TextListTy
from . import linalg
from . import special


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
# solution: n, dim, cap, t*, y*, dy*, the right-hand side f(t, y, dy, env) (or null) and its env (D46)
SOL = ir.LiteralStructType([I64, I64, I64, F64P, F64P, F64P, I8P, F64P])
SOLP = SOL.as_pointer()

SCALAR_FN = ir.FunctionType(F64, [F64, F64P])
ODE_FN = ir.FunctionType(VOID, [F64, F64P, F64P, F64P])
MODEL_FN = ir.FunctionType(VOID, [F64P, F64PP, I64, F64P])

ERR_INDEX, ERR_SOLRANGE, ERR_ODE_STEPS, ERR_ASSERT, ERR_LEN, ERR_EMPTY, ERR_STEP, ERR_ODE_H = 1, 2, 3, 4, 5, 6, 7, 8
ERR_QUAD = 9
ERR_QUAD_NAN = 31          # the integrand is NaN on more than an isolated point (D45)
ERR_QUAD_INF = 32          # ... or ±∞ (and nowhere NaN): it may blow up there (D45)
ERR_DEEP = 10
ERR_STD_ONE = 24
ERR_SIZE = 11               # a list too big for memory (or of NaN length)
ERR_RANGE = 12              # a for loop over a range with a NaN end or step
ERR_SINGULAR = 15           # inverse / solve_linear of a singular matrix
ERR_NOT_SYMMETRIC = 21      # eigenvalues / eigenvectors of a matrix that isn't symmetric (D38)
ERR_NOT_POSDEF = 22         # eigenvalues(K, M) with a mass matrix M that isn't positive definite (D38)
ERR_PENDING = -1            # a runtime callback (plot, load, fit) failed and already set the message
ERR_ROOT = 13               # solve lhs = rhs: no sign change in the search range
ERR_POLE = 14               # solve lhs = rhs: the sign change is a jump (tan at 90°), not a root
ERR_ODE_NAN = 16            # the right side of an ODE is NaN or infinite at the start (#32)
ERR_ODE_RANGE = 17          # an ODE's range starts and ends at the same value
ERR_NO_EVENT = 18           # solve ... until: the stop condition never happened (D39)
ERR_ODE_SINGULAR = 33      # the mass matrix of an ODE's highest derivatives is singular (D47)
ERR_ODE_H_FLAT = 34        # the step became too small but nothing grew: the tolerance, not a blow-up (D160)
MAX_LIST = 1e9              # most numbers a list may hold (8 GB)
STACK_LIMIT = 400 << 20     # bytes of stack a program may use (it runs on a thread with a 512 MB stack)


def lltype(ty):
    if isinstance(ty, NumTy):
        return F64
    if isinstance(ty, (VecTy, MatTy)):
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


class LLOps:
    """fermium.linalg's ops, building LLVM instructions."""

    def __init__(self, gen):
        self.b, self.gen = gen.b, gen

    def add(self, x, y):
        return self.b.fadd(x, y)

    def sub(self, x, y):
        return self.b.fsub(x, y)

    def mul(self, x, y):
        return self.b.fmul(x, y)

    def div(self, x, y):
        return self.b.fdiv(x, y)

    def neg(self, x):
        return self.b.fneg(x)

    def gt_abs(self, x, y):
        fabs = self.gen.mg.intrinsic("fabs")
        return self.b.fcmp_ordered(">", self.b.call(fabs, [x]), self.b.call(fabs, [y]))

    def select(self, c, x, y):
        return self.b.select(c, x, y)

    def const(self, v):
        return f64(v)

    def sqrt(self, x):
        return self.b.call(self.gen.mg.intrinsic("sqrt"), [x])

    def abs(self, x):
        return self.b.call(self.gen.mg.intrinsic("fabs"), [x])

    def lt(self, x, y):
        return self.b.fcmp_ordered("<", x, y)

    def eq(self, x, y):
        return self.b.fcmp_ordered("==", x, y)


def i64(v):
    return ir.Constant(I64, int(v))


def env_slots(sym):
    """Doubles a captured variable takes in a nested function's environment."""
    return sym.ty.n if isinstance(sym.ty, (VecTy, MatTy)) else 1


class ModuleGen:
    def __init__(self, name="fermium", arena_base=None, rng_addr=None):
        self.module = ir.Module(name=name)
        self.arena_base = arena_base
        self.rng_addr = rng_addr      # the runtime's random-number state (D80), or None for a module global
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
        # text id of the variable of the integral being computed, for "the integrand is NaN at x = …" (D45)
        self.qvar = ir.GlobalVariable(self.module, F64, "fm.qvar")
        self.qvar.initializer = f64(-1)
        self.qvar.linkage = "internal"
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
        e("fm_print_mvec", VOID, [I64, F64P, I64])
        e("fm_print_mat", VOID, [I64, F64P, I64, I64])
        e("fm_print_textlist", VOID, [F64P, I64])
        e("fm_print_bool", VOID, [I64])
        e("fm_print_text", VOID, [I64])
        e("fm_print_end", VOID, [])
        e("fm_error", VOID, [I64, F64, F64, I64, I64])
        e("fm_warn", VOID, [I64, F64, I64, I64])
        e("fm_plot_series", I64, [I64, I64, F64P, I64, F64P, I64])
        e("fm_plot_sol", VOID, [I64, I64, I8P, I64, I64, I64, I64])
        e("fm_plot_done", VOID, [I64])
        e("fm_load", I64, [I64])
        e("fm_column", I64, [I64, I64, F64PP])
        e("fm_fit", I64, [I64, I64, F64P])
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
                if getattr(item, "def_line", None):
                    g.line = item.def_line
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
        if name in special.KERNELS:
            return special.KERNELS[name](self)
        return getattr(self, "_k_" + name.replace("fm_", ""))()

    def raise_error(self, b, kind, a=None, c=None, line=None):
        ln = i64(line) if line else b.load(self.curline)
        kind = kind if isinstance(kind, ir.Value) else i64(kind)
        b.call(self.externs["fm_error"], [kind, a if a is not None else f64(0), c if c is not None else f64(0),
                                          ln, b.load(self.errfmt)])
        b.call(self.externs["longjmp"], [b.bitcast(self.jmpbuf, I8P), ir.Constant(I32, 1)])
        b.unreachable()

    def _new_fn(self, name, ret, args, inline=True):
        fn = ir.Function(self.module, ir.FunctionType(ret, args), name)
        fn.linkage = "internal"
        if inline:
            fn.attributes.add("alwaysinline")
        return fn

    def _qx(self, b, mode, p, q, u):
        """x for the quadrature variable u (see fm_qf); mirrored in interp._qx."""
        L = b.fsub(q, p)
        v = b.fsub(f64(1), u)
        lower = b.fcmp_ordered("<=", u, f64(0.5))
        # measure from the nearer end so that points close to q keep their full precision
        xl = b.fadd(p, b.fmul(L, b.fmul(b.fmul(u, u), b.fsub(f64(3), b.fmul(f64(2), u)))))
        xh = b.fsub(q, b.fmul(L, b.fmul(b.fmul(v, v), b.fadd(f64(1), b.fmul(f64(2), u)))))
        xfin = b.select(lower, xl, xh)
        sx = b.fmul(q, b.fdiv(u, v))
        xhalf = b.select(b.icmp_signed("==", mode, i64(1)), b.fadd(p, sx), b.fsub(p, sx))
        return b.select(b.icmp_signed("==", mode, i64(0)), xfin, xhalf)

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
        x = self._qx(b, i64(0), p, q, u)
        w = b.fmul(b.fmul(f64(6), b.fmul(u, v)), L)
        at_end = b.or_(b.fcmp_ordered("==", x, p), b.fcmp_ordered("==", x, q))
        with b.if_then(at_end):      # a node that rounds onto an end point has no weight
            b.ret(f64(0))
        b.ret(b.fmul(b.call(f, [x, env]), w))
        b.position_at_end(blk_half)
        om = b.fsub(f64(1), u)
        x = self._qx(b, mode, p, q, u)
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
        """One Gauss–Kronrod 15-point panel [lo, hi] in u.  Returns the Kronrod estimate; out[0] = error
        estimate, out[1] = the Kronrod estimate of ∫|g| (QUADPACK's resabs, D44), out[2] = u of a node where
        the integrand is NaN, or -u of one where it is ±∞ (NaN if there is none; such nodes count as 0 in
        the sums, D45),
        out[3] = the largest |g| at the finite nodes."""
        qf = self.kernel("fm_qf")
        fn = self._new_fn("fm_gk15", F64, [SCALAR_FN.as_pointer(), F64P, I64, F64, F64, F64, F64, F64P],
                          inline=False)
        f, env, mode, a, bb, lo, hi, outp = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        fabs = self.intrinsic("fabs")
        maxnum = self.intrinsic("maxnum")
        inf = f64(math.inf)
        c = b.fmul(f64(0.5), b.fadd(lo, hi))
        h = b.fmul(f64(0.5), b.fsub(hi, lo))
        state = {"bad": f64(math.nan), "gmax": f64(0)}

        def node(u):
            g = b.call(qf, [f, env, mode, a, bb, u])
            ok = b.fcmp_ordered("<", b.call(fabs, [g]), inf)
            gz = b.select(ok, g, f64(0))
            # a NaN node is recorded as +u and wins over a ±∞ node, recorded as -u
            cur = state["bad"]
            keep = b.or_(ok, b.fcmp_ordered(">", cur, f64(0)))
            state["bad"] = b.select(b.fcmp_unordered("uno", g, g), u, b.select(keep, cur, b.fneg(u)))
            state["gmax"] = b.call(maxnum, [state["gmax"], b.call(fabs, [gz])])
            return gz
        fc = node(c)
        resk = b.fmul(fc, f64(WGK[7]))
        resg = b.fmul(fc, f64(WG[3]))
        rabs = b.fmul(b.call(fabs, [fc]), f64(WGK[7]))
        for j in range(7):
            dx = b.fmul(h, f64(XGK[j]))
            f1 = node(b.fsub(c, dx))
            f2 = node(b.fadd(c, dx))
            s = b.fadd(f1, f2)
            resk = b.fadd(resk, b.fmul(s, f64(WGK[j])))
            rabs = b.fadd(rabs, b.fmul(b.fadd(b.call(fabs, [f1]), b.call(fabs, [f2])), f64(WGK[j])))
            if j % 2 == 1:
                resg = b.fadd(resg, b.fmul(s, f64(WG[j // 2])))
        b.store(b.call(fabs, [b.fmul(b.fsub(resk, resg), h)]), outp)
        b.store(b.call(fabs, [b.fmul(rabs, h)]), b.gep(outp, [i64(1)]))
        b.store(state["bad"], b.gep(outp, [i64(2)]))
        b.store(state["gmax"], b.gep(outp, [i64(3)]))
        b.ret(b.fmul(resk, h))
        return fn

    QUAD_PANELS = 8          # initial uniform panels (helps with narrow peaks)
    QUAD_MAX = 2000          # subdivision budget; beyond it the integral is reported as not converging
    QUAD_SOFT = 250          # budget of the quiet first try for a component of a vector integral (D44)
    QUAD_ULPS = 8 * 2.220446049250313e-16    # a NaN panel this narrow (relative) is one point (D45)
    QUAD_ROUND = 50 * 2.220446049250313e-16   # an error below this × ∫|f| is rounding (QUADPACK's 50 ε, D44)

    def _qabs(self):
        """fm.qabs: the ∫|g| estimates of the fm_quadcore calls of the current fm_quad call, added up."""
        g = self.module.globals.get("fm.qabs")
        if g is None:
            g = ir.GlobalVariable(self.module, F64, "fm.qabs")
            g.initializer = f64(0)
            g.linkage = "internal"
        return g

    def _k_quad(self):
        """∫ f from a to b (fm_quadin), with a warning when the result is exactly 0 because the integrand was
        0 at every node: a narrow peak in a wide range can hide between the nodes (D110).  fm.qabs is saved
        and restored around the call, so an integral inside the integrand doesn't count."""
        inner = self.kernel("fm_quadin")
        g = self._qabs()
        fn = self._new_fn("fm_quad", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64, F64, F64], inline=False)
        f, env, a, bb, rtol, atol = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        saved = b.load(g)
        b.store(f64(0), g)
        r = b.call(inner, [f, env, a, bb, rtol, atol])
        mine = b.load(g)
        b.store(saved, g)
        zero = b.and_(b.fcmp_ordered("==", r, f64(0)), b.fcmp_ordered("==", mine, f64(0)))
        zero = b.and_(zero, b.fcmp_ordered("!=", a, bb))
        zero = b.and_(zero, b.fcmp_unordered(">=", atol, f64(0)))     # not the quiet first try (D44)
        with b.if_then(zero):
            b.call(self.externs["fm_warn"], [i64(3), f64(0), b.load(self.curline), i64(-1)])
        b.ret(r)
        return fn

    def _k_quadin(self):
        """∫ f from a to b.  Finite ranges go straight to fm_quadcore; a half-line [a, ∞) is split at
        a + L, with L the integrand's length scale (fm_qscan), into a finite piece and a tail; (-∞, ∞)
        is split at the scan's peak into two tails."""
        corek = self.kernel("fm_quadcore")
        fin = self.kernel("fm_quadfin")
        scan = self.kernel("fm_qscan")
        fn = self._new_fn("fm_quadin", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64, F64, F64], inline=False)
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
        total error is small enough: rtol of |total|, or rounding level (50 ε) of ∫|f|, so that an
        integral that is 0 by symmetry converges (D44).  A panel with a NaN/∞ node counts as 0 with an
        infinite error, so it is split first.  When such panels have shrunk to a run a few ulps wide in x
        (one point, as far as the floating-point numbers can tell) with finite panels next to it, the run
        counts as 0 with error width × the neighbours' largest |g| (D45).  If a NaN/∞ panel is left when
        the budget runs out, the error says where the integrand was NaN or infinite."""
        gk = self.kernel("fm_gk15")
        fn = self._new_fn("fm_quadcore", F64, [SCALAR_FN.as_pointer(), F64P, I64, F64, F64, F64, F64, F64P],
                          inline=False)
        f, env, mode, a, bb, rtol, atol, split = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        maxnum = self.intrinsic("maxnum")
        inf = f64(math.inf)
        lo, hi = f64(0), f64(1)
        M = self.QUAD_MAX
        mal = self.externs["malloc"]
        free = self.extern("free", VOID, [I8P])
        raw = b.call(mal, [i64(8 * 7 * M)])
        base = b.bitcast(raw, F64P)
        # per panel: ends in u, result, error, ∫|g|, u of a NaN/∞ node (NaN: none), largest finite |g|
        # (-1: part of a NaN/∞ run that counts as one point)
        plo, phi, pres, perr, pabs, pbad, pgmax = (b.gep(base, [i64(k * M)]) for k in range(7))
        out1 = b.alloca(F64, size=4)
        out2 = b.alloca(F64, size=4)
        n = b.alloca(I64)
        walkv = [(b.alloca(F64), b.alloca(F64)) for _ in range(2)]
        j = b.alloca(I64)

        def get(p, k):
            return b.load(b.gep(p, [k if isinstance(k, ir.Value) else i64(k)]))

        def put(p, k, v):
            b.store(v, b.gep(p, [k]))

        def isbad(v):
            return b.fcmp_ordered("==", v, v)

        def store_panel(i, x0, x1, r, o):
            """A panel with a NaN/∞ node counts 0 with an infinite error (D45)."""
            badu = get(o, 2)
            bad = isbad(badu)
            put(plo, i, x0)
            put(phi, i, x1)
            put(pres, i, b.select(bad, f64(0), r))
            put(perr, i, b.select(bad, inf, get(o, 0)))
            put(pabs, i, b.select(bad, f64(0), get(o, 1)))
            put(pbad, i, badu)
            put(pgmax, i, get(o, 3))

        def tiny_x(u0, u1):
            """[u0, u1] is a few ulps wide in x: of x itself, or of the range's length (the length scale
            on a half-line)."""
            xa, xb = self._qx(b, mode, a, bb, u0), self._qx(b, mode, a, bb, u1)
            scale = b.select(b.icmp_signed("==", mode, i64(0)), b.fsub(bb, a), bb)
            scale = b.call(maxnum, [b.call(fabs, [scale]), b.call(maxnum, [b.call(fabs, [xa]), b.call(fabs, [xb])])])
            ext = b.call(fabs, [b.fsub(xb, xa)])
            return b.and_(b.fcmp_ordered("<=", ext, b.fmul(f64(self.QUAD_ULPS), scale)),
                          b.fcmp_ordered("<", ext, inf))
        P = self.QUAD_PANELS
        width = b.fdiv(b.fsub(hi, lo), f64(P))
        with lp.range(i64(0), i64(P)) as i:
            x0 = b.fadd(lo, b.fmul(b.sitofp(i, F64), width))
            x1 = b.select(b.icmp_signed("==", i, i64(P - 1)), hi, b.fadd(x0, width))
            r = b.call(gk, [f, env, mode, a, bb, x0, x1, out1])
            store_panel(i, x0, x1, r, out1)
        b.store(i64(P), n)
        tot = b.alloca(F64)
        terr = b.alloca(F64)
        tabs = b.alloca(F64)
        worst = b.alloca(I64)
        cond_bb = fn.append_basic_block("q.cond")
        body_bb = fn.append_basic_block("q.body")
        b.branch(cond_bb)
        b.position_at_end(cond_bb)
        b.store(f64(0), tot)
        b.store(f64(0), terr)
        b.store(f64(0), tabs)
        b.store(i64(0), worst)
        cnt = b.load(n)
        with lp.range(i64(0), cnt) as i:
            e = b.load(b.gep(perr, [i]))
            b.store(b.fadd(b.load(tot), b.load(b.gep(pres, [i]))), tot)
            b.store(b.fadd(b.load(terr), e), terr)
            b.store(b.fadd(b.load(tabs), b.load(b.gep(pabs, [i]))), tabs)
            with b.if_then(b.fcmp_unordered(">", e, b.load(b.gep(perr, [b.load(worst)])))):
                b.store(i, worst)
        total, toterr, totabs = b.load(tot), b.load(terr), b.load(tabs)
        goal = b.call(maxnum, [atol, b.fmul(rtol, b.call(fabs, [total]))])
        finite = b.fcmp_ordered("<", b.call(fabs, [total]), inf)
        done = b.fcmp_ordered("<=", toterr, goal)
        done = b.or_(done, b.fcmp_ordered("<=", toterr, b.fmul(f64(1e-14), b.call(fabs, [total]))))
        done = b.or_(done, b.fcmp_ordered("<=", toterr, b.fmul(f64(self.QUAD_ROUND), totabs)))
        done = b.and_(finite, done)
        with b.if_then(done):
            b.call(free, [raw])
            qabs = self._qabs()
            b.store(b.fadd(b.load(qabs), totabs), qabs)
            b.ret(total)
        # not converged: give up with an error when out of budget, the result is not finite,
        # or the worst panel can't be split any more
        w = b.load(worst)
        wl, wh = b.load(b.gep(plo, [w])), b.load(b.gep(phi, [w]))
        wbad = b.load(b.gep(pbad, [w]))
        # the worst panel has a NaN/∞ node and is a few ulps wide: find the run of such panels around it
        # (walking through panels that share an end); if the whole run is a few ulps wide and some
        # neighbour is finite, it is one point: count it 0, with error width × the neighbours' largest |g|
        with b.if_then(b.and_(b.and_(isbad(wbad), b.fcmp_ordered(">=", get(pgmax, w), f64(0))), tiny_x(wl, wh))):
            ends, gs = [], []
            for (from_arr, to_arr, start), (cur, g) in zip(((phi, plo, wl), (plo, phi, wh)), walkv):
                b.store(start, cur)
                b.store(f64(-1), g)
                walk = fn.append_basic_block("q.walk")
                step = fn.append_basic_block("q.step")
                stop = fn.append_basic_block("q.stop")
                b.branch(walk)
                b.position_at_end(walk)
                b.store(i64(-1), j)
                with lp.range(i64(0), cnt) as i:
                    with b.if_then(b.fcmp_ordered("==", get(from_arr, i), b.load(cur))):
                        b.store(i, j)
                jj = b.load(j)
                with b.if_then(b.icmp_signed("<", jj, i64(0))):     # an end of the range
                    b.branch(stop)
                b.cbranch(isbad(get(pbad, jj)), step, stop)
                b.position_at_end(step)
                b.store(get(to_arr, jj), cur)
                b.branch(walk)
                b.position_at_end(stop)
                # (a finite neighbour: its largest |g|; the end of the range: -1)
                jj = b.load(j)
                has = b.icmp_signed(">=", jj, i64(0))
                safe = b.select(has, jj, i64(0))
                b.store(b.select(has, get(pgmax, safe), f64(-1)), g)
                ends.append(b.load(cur))
                gs.append(b.load(g))
            G = b.call(maxnum, [gs[0], gs[1]])
            with b.if_then(b.and_(b.fcmp_ordered(">=", G, f64(0)), tiny_x(ends[0], ends[1]))):
                with lp.range(i64(0), cnt) as i:
                    x0, x1 = get(plo, i), get(phi, i)
                    inside = b.and_(b.fcmp_ordered(">=", x0, ends[0]), b.fcmp_ordered("<=", x1, ends[1]))
                    with b.if_then(inside):
                        put(pres, i, f64(0))
                        put(perr, i, b.fmul(b.fsub(x1, x0), G))
                        put(pabs, i, f64(0))
                        put(pgmax, i, f64(-1))
                b.branch(cond_bb)
        mid = b.fmul(f64(0.5), b.fadd(wl, wh))
        # a panel near a singularity that has shrunk to a few hundred ulps can't usefully be split:
        # accept the result if the error is still small (≤ 1e-7 relative), else report it
        big = b.call(maxnum, [b.call(fabs, [wl]), b.call(fabs, [wh])])
        stuck = b.fcmp_ordered("<=", b.fsub(wh, wl), b.fmul(f64(1e-13), big))
        with b.if_then(b.and_(stuck, b.and_(finite, b.fcmp_ordered("<=", toterr,
                                                                    b.fmul(f64(1e-7), b.call(fabs, [total])))))):
            b.call(free, [raw])
            b.ret(total)
        # (the quiet first try of a vector component, atol < 0, gets a smaller budget: D44)
        limit = b.select(b.fcmp_ordered("<", atol, f64(0)), i64(self.QUAD_SOFT), i64(M - 1))
        bad = b.or_(b.icmp_signed(">=", cnt, limit), stuck)
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
        fail = b.or_(bad, b.fcmp_unordered("uno", toterr, toterr))
        fail = b.or_(fail, b.fcmp_ordered("==", b.call(fabs, [total]), inf))
        # atol < 0: a first try for a component of a vector integral, which fails quietly with NaN (D44)
        with b.if_then(b.and_(fail, b.fcmp_ordered("<", atol, f64(0)))):
            b.call(free, [raw])
            b.ret(f64(math.nan))
        # the worst panel still has a NaN/∞ node (any such panel has an infinite error), or is a NaN/∞ run
        # whose possible contribution is too big: say where
        with b.if_then(b.and_(bad, isbad(wbad))):
            kind = b.select(b.fcmp_ordered(">", wbad, f64(0)), i64(ERR_QUAD_NAN), i64(ERR_QUAD_INF))
            self.raise_error(b, kind, self._qx(b, mode, a, bb, b.call(fabs, [wbad])), b.load(self.qvar))
        with b.if_then(fail):
            self.raise_error(b, ERR_QUAD, total, toterr)
        b.branch(body_bb)
        b.position_at_end(body_bb)
        # bisect the worst panel: left half stays at index w, right half goes to the end
        r1 = b.call(gk, [f, env, mode, a, bb, wl, mid, out1])
        r2 = b.call(gk, [f, env, mode, a, bb, mid, wh, out2])
        store_panel(w, wl, mid, r1, out1)
        store_panel(cnt, mid, wh, r2, out2)
        b.store(b.add(cnt, i64(1)), n)
        b.branch(cond_bb)
        return fn

    def _sol_alloc(self, b, dim, cap, arrays=True):
        mal = self.externs["malloc"]
        sp = b.bitcast(b.call(mal, [i64(64)]), SOLP)
        b.store(ir.Constant(I8P, None), b.gep(sp, [I32(0), I32(6)]))
        b.store(ir.Constant(F64P, None), b.gep(sp, [I32(0), I32(7)]))
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

    def _k_ode_guard(self):
        """int fm_ode_guard(f, env, t, y, out): call a compiled right-hand side from Python (the stiff solver,
        D42).  A run-time error inside it (fm_error + longjmp) must not unwind through Python's frames, so
        the guard saves the program's jump buffer, sets its own, and restores it: 0 = ok, 1 = the right
        side stopped with an error (fm_error has already set the message and its line)."""
        fn = self._new_fn("fm_ode_guard", I32, [I8P, F64P, F64, F64P, F64P], inline=False)
        fn.attributes.add("noinline")
        f, env, t, y, out = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        words = 1024 // 8
        saved = b.alloca(ir.ArrayType(I64, words))
        buf = b.bitcast(self.jmpbuf, I64.as_pointer())
        sv = b.gep(saved, [I32(0), I32(0)])
        with lp.range(i64(0), i64(words)) as k:
            b.store(b.load(b.gep(buf, [k])), b.gep(sv, [k]))
        r = b.call(self.externs["_setjmp"], [b.bitcast(self.jmpbuf, I8P)])
        ok = b.icmp_signed("==", r, ir.Constant(I32, 0))
        with b.if_then(ok):
            b.call(b.bitcast(f, ODE_FN.as_pointer()), [t, y, out, env])
        with lp.range(i64(0), i64(words)) as k:
            b.store(b.load(b.gep(sv, [k])), b.gep(buf, [k]))
        b.ret(b.select(ok, ir.Constant(I32, 0), ir.Constant(I32, 1)))
        return fn

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

    # ------------------------------------------------------------ ODE helpers (mirrored in interp.py)
    D_DP = [-12715105075 / 11282082432, 0, 87487479700 / 32700410799, -10690763975 / 1880347072,
            701980252875 / 199316789632, -1453857185 / 822651844, 69997945 / 29380423]

    def _emit_dense(self, b, ya, yb, da, db, r5, h, th):
        """interp._dense: a step's dense output at the fraction th (Hermite + DOPRI5's 4th-order term r5)."""
        r2 = b.fsub(yb, ya)
        r3 = b.fsub(b.fmul(h, da), r2)
        r4 = b.fsub(b.fsub(r2, b.fmul(h, db)), r3)
        th1 = b.fsub(f64(1.0), th)
        return b.fadd(ya, b.fmul(th, b.fadd(r2, b.fmul(th1, b.fadd(r3, b.fmul(th, b.fadd(r4, b.fmul(th1, r5))))))))

    STIFF_AFTER = 100_000     # the stiffness test starts after this many steps (a slow solve)

    def _emit_stiff_test(self, b, lp, n, cnt, h, k6, k7, y6, y7, stiffn, nonstiff):
        """interp._stiff_test: Hairer's stiffness detection for DOPRI5, h·|λ| ≈ h‖k7 − k6‖/‖y7 − y6‖ (stages 6
        and 7 are both at t + h).  Every 1000th step of a long solve, and each step while a run of stiff-looking
        steps lasts; after 15 in a row with h·|λ| > 1.8, warn once that `using radau` fits (D42).  RK45's
        stability limit is 3.3, and this controller settles between 2 and 3.22 on a stiff problem (Hairer's 3.25
        is for DOPRI5's own controller); accuracy-limited steps give under 0.4 at 10⁻⁶ (single steps up to 2.4
        at 10⁻³).  stiffn = -1 once warned."""
        run = b.load(stiffn)
        due = b.and_(b.icmp_signed(">=", cnt, i64(self.STIFF_AFTER)),
                     b.or_(b.icmp_signed("==", b.srem(cnt, i64(1000)), i64(0)), b.icmp_signed(">", run, i64(0))))
        with b.if_then(b.and_(due, b.icmp_signed(">=", run, i64(0))), likely=False):
            num = b.alloca(F64)
            den = b.alloca(F64)
            b.store(f64(0), num)
            b.store(f64(0), den)
            with lp.range(i64(0), n) as j:
                dk = b.fsub(b.load(b.gep(k7, [j])), b.load(b.gep(k6, [j])))
                dy = b.fsub(b.load(b.gep(y7, [j])), b.load(b.gep(y6, [j])))
                b.store(b.fadd(b.load(num), b.fmul(dk, dk)), num)
                b.store(b.fadd(b.load(den), b.fmul(dy, dy)), den)
            hl2 = b.fmul(b.fmul(h, h), b.load(num))
            stiff = b.and_(b.fcmp_ordered(">", b.load(den), f64(0)),
                           b.fcmp_ordered(">", hl2, b.fmul(f64(1.8 * 1.8), b.load(den))))
            with b.if_else(stiff) as (yes, no):
                with yes:
                    b.store(i64(0), nonstiff)
                    r = b.add(b.load(stiffn), i64(1))
                    b.store(r, stiffn)
                    with b.if_then(b.icmp_signed(">=", r, i64(15))):
                        b.call(self.externs["fm_warn"], [i64(2), b.sitofp(cnt, F64), b.load(self.curline), i64(-1)])
                        b.store(i64(-1), stiffn)
                with no:
                    nn = b.add(b.load(nonstiff), i64(1))
                    b.store(nn, nonstiff)
                    with b.if_then(b.icmp_signed(">=", nn, i64(6))):
                        b.store(i64(0), stiffn)

    def _emit_nan_check(self, b, lp, n, k, t, tname):
        """interp._start: stop if the derivative at the start is NaN or infinite (#32)."""
        ok = b.alloca(I1)
        b.store(ir.Constant(I1, 1), ok)
        with lp.range(i64(0), n) as j:
            v = b.load(b.gep(k, [j]))
            b.store(b.and_(b.load(ok), b.fcmp_ordered("==", b.fsub(v, v), f64(0))), ok)
        with b.if_then(b.not_(b.load(ok)), likely=False):
            self.raise_error(b, ERR_ODE_NAN, t, tname)

    def _emit_sign(self, b, g):
        return b.select(b.fcmp_ordered(">", g, f64(0)), f64(1),
                        b.select(b.fcmp_ordered("<", g, f64(0)), f64(-1), f64(0)))

    def _k_event_locate(self):
        """interp._Event.check's location step: the time in (t, tn) where the stop condition's g crosses 0,
        by Illinois on g(x, cubic Hermite of the step at x).  Leaves the state at the result in ys."""
        fn = self._new_fn("fm_event_locate", F64, [ODE_FN.as_pointer(), F64P, I64, F64, F64P, F64P, F64, F64P,
                                                   F64P, F64P, F64, F64, F64P, F64P], inline=False)
        ev, env, n, t, y, k, tn, yn, kn, r5, ga0, gc0, ys, gbuf = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        h = b.fsub(tn, t)

        def g_at(x):
            th = b.fdiv(b.fsub(x, t), h)
            with lp.range(i64(0), n) as j:
                v = self._emit_dense(b, b.load(b.gep(y, [j])), b.load(b.gep(yn, [j])), b.load(b.gep(k, [j])),
                                     b.load(b.gep(kn, [j])), b.load(b.gep(r5, [j])), h, th)
                b.store(v, b.gep(ys, [j]))
            b.call(ev, [x, ys, gbuf, env])
            return b.load(gbuf)
        a, c, ga, gc, side = b.alloca(F64), b.alloca(F64), b.alloca(F64), b.alloca(F64), b.alloca(I64)
        b.store(t, a)
        b.store(ga0, ga)
        b.store(tn, c)
        b.store(gc0, gc)
        b.store(i64(0), side)
        done_bb = fn.append_basic_block("ev.done")
        with lp.range(i64(0), i64(200)):
            xa, xc, ya, yc = b.load(a), b.load(c), b.load(ga), b.load(gc)
            big = b.call(self.intrinsic("maxnum"), [b.call(fabs, [xa]), b.call(fabs, [xc])])
            with b.if_then(b.fcmp_ordered("<=", b.call(fabs, [b.fsub(xc, xa)]), b.fmul(f64(4e-16), big))):
                b.branch(done_bb)
            x = b.fsub(xc, b.fdiv(b.fmul(yc, b.fsub(xc, xa)), b.fsub(yc, ya)))
            mid = b.fmul(f64(0.5), b.fadd(xa, xc))
            lo_, hi_ = b.call(self.intrinsic("minnum"), [xa, xc]), b.call(self.intrinsic("maxnum"), [xa, xc])
            inside = b.and_(b.fcmp_ordered(">", x, lo_), b.fcmp_ordered("<", x, hi_))
            x = b.select(inside, x, mid)
            gx = g_at(x)
            with b.if_then(b.fcmp_unordered("==", gx, f64(0))):
                b.ret(x)
            with b.if_else(b.fcmp_ordered("<", b.fmul(gx, yc), f64(0))) as (then, other):
                with then:
                    b.store(xc, a)
                    b.store(yc, ga)
                    b.store(i64(0), side)
                with other:
                    with b.if_then(b.icmp_signed("==", b.load(side), i64(1))):
                        b.store(b.fmul(f64(0.5), b.load(ga)), ga)
                    b.store(i64(1), side)
            b.store(x, c)
            b.store(gx, gc)
        b.branch(done_bb)
        b.position_at_end(done_bb)
        b.ret(b.load(c))
        return fn

    def _emit_event_check(self, b, lp, f, ev, env, n, sp, evsgn, gbuf, ys, kbuf, r5, t, y, k, tn, yn, kn,
                          ks=None, before_push=None):
        """interp._Event.check: after a step (t, y, k) -> (tn, yn, kn), return from the kernel with the
        solution ended at the crossing if the stop condition's g changed sign."""
        b.call(ev, [tn, yn, gbuf, env])
        gn = b.load(gbuf)
        with b.if_then(b.fcmp_ordered("==", gn, gn)):
            sgn = b.load(evsgn)
            with b.if_else(b.fcmp_ordered("==", sgn, f64(0))) as (unknown, known):
                with unknown:
                    b.store(self._emit_sign(b, gn), evsgn)
                with known:
                    crossed = b.or_(b.fcmp_ordered("==", gn, f64(0)), b.fcmp_ordered("<", b.fmul(gn, sgn), f64(0)))
                    with b.if_then(crossed):
                        h = b.fsub(tn, t)
                        with lp.range(i64(0), n) as j:
                            if ks is None:
                                v = f64(0)
                            else:
                                acc = b.fmul(f64(self.D_DP[0]), b.load(b.gep(ks[0], [j])))
                                for m in range(2, 7):
                                    acc = b.fadd(acc, b.fmul(f64(self.D_DP[m]), b.load(b.gep(ks[m], [j]))))
                                v = b.fmul(h, acc)
                            b.store(v, b.gep(r5, [j]))
                        te = b.alloca(F64)
                        b.store(tn, te)
                        with b.if_then(b.fcmp_unordered("!=", gn, f64(0))):
                            b.store(b.call(self.kernel("fm_event_locate"),
                                           [ev, env, n, t, y, k, tn, yn, kn, r5, sgn, gn, ys, gbuf]), te)
                        tev = b.load(te)
                        at_end = b.fcmp_ordered("==", tev, tn)
                        th = b.fdiv(b.fsub(tev, t), h)
                        with lp.range(i64(0), n) as j:
                            yj = b.load(b.gep(y, [j]))
                            ynj = b.load(b.gep(yn, [j]))
                            v = self._emit_dense(b, yj, ynj, b.load(b.gep(k, [j])), b.load(b.gep(kn, [j])),
                                                 b.load(b.gep(r5, [j])), h, th)
                            b.store(b.select(at_end, ynj, v), b.gep(ys, [j]))
                        b.call(f, [tev, ys, kbuf, env])
                        if before_push is not None:
                            before_push()
                        b.call(self.kernel("fm_sol_push"), [sp, tev, ys, kbuf])
                        b.ret(sp)

    def _emit_jdist(self, b, lp, n, u, v, fa, fe, vfun=None):
        """interp._jdist: sum over components of |u - v| / (|fa| + |fe|) (components with a zero scale skipped).
        vfun(j) gives v's component when v isn't an array."""
        fabs = self.intrinsic("fabs")
        d = b.alloca(F64)
        b.store(f64(0), d)
        with lp.range(i64(0), n) as j:
            w = b.fadd(b.call(fabs, [b.load(b.gep(fa, [j]))]), b.call(fabs, [b.load(b.gep(fe, [j]))]))
            vj = vfun(j) if vfun is not None else b.load(b.gep(v, [j]))
            q = b.fdiv(b.call(fabs, [b.fsub(b.load(b.gep(u, [j])), vj)]), w)
            b.store(b.select(b.fcmp_ordered(">", w, f64(0)), b.fadd(b.load(d), q), b.load(d)), d)
        return b.load(d)

    def _k_find_jump(self):
        """interp._find_jump: is there a jump in f(·, y), y held fixed, between t and tn (`if t < 0.3 s`)?
        Returns 1 and out = (lo, hi), adjacent times on the near and far side of it, or 0 (D40)."""
        fn = self._new_fn("fm_find_jump", I64, [ODE_FN.as_pointer(), F64P, I64, F64, F64, F64P, F64P, F64P,
                                                F64P, F64P, F64P, F64P, F64P], inline=False)
        f, env, n, t, tn, y, fa, fe, fm, fx, flo, fhi, out = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        b.call(f, [tn, y, fe, env])
        b.call(f, [b.fadd(t, b.fmul(f64(0.5), b.fsub(tn, t))), y, fm, env])
        dfe = self._emit_jdist(b, lp, n, fa, fe, fa, fe)
        with b.if_then(b.not_(b.fcmp_ordered(">", dfe, f64(1e-12)))):
            b.ret(i64(0))
        dm = self._emit_jdist(b, lp, n, fm, None, fa, fe, vfun=lambda j: b.fmul(
            f64(0.5), b.fadd(b.load(b.gep(fa, [j])), b.load(b.gep(fe, [j])))))
        with b.if_then(b.not_(b.fcmp_ordered(">", dm, b.fmul(f64(0.4), dfe)))):
            b.ret(i64(0))
        lo, hi = b.alloca(F64), b.alloca(F64)
        b.store(t, lo)
        b.store(tn, hi)
        with lp.range(i64(0), n) as j:
            b.store(b.load(b.gep(fa, [j])), b.gep(flo, [j]))
            b.store(b.load(b.gep(fe, [j])), b.gep(fhi, [j]))
        done_bb = fn.append_basic_block("fj.done")
        with lp.range(i64(0), i64(200)):
            lv, hv = b.load(lo), b.load(hi)
            m = b.fadd(lv, b.fmul(f64(0.5), b.fsub(hv, lv)))
            with b.if_then(b.or_(b.fcmp_ordered("==", m, lv), b.fcmp_ordered("==", m, hv))):
                b.branch(done_bb)
            b.call(f, [m, y, fx, env])
            near = b.fcmp_ordered("<=", self._emit_jdist(b, lp, n, fx, fa, fa, fe),
                                  self._emit_jdist(b, lp, n, fx, fe, fa, fe))
            with b.if_else(near) as (yes, no):
                with yes:
                    b.store(m, lo)
                    with lp.range(i64(0), n) as j:
                        b.store(b.load(b.gep(fx, [j])), b.gep(flo, [j]))
                with no:
                    b.store(m, hi)
                    with lp.range(i64(0), n) as j:
                        b.store(b.load(b.gep(fx, [j])), b.gep(fhi, [j]))
        b.branch(done_bb)
        b.position_at_end(done_bb)
        with b.if_then(b.fcmp_ordered(">", self._emit_jdist(b, lp, n, flo, fhi, fa, fe), b.fmul(f64(0.5), dfe))):
            b.store(b.load(lo), out)
            b.store(b.load(hi), b.gep(out, [i64(1)]))
            b.ret(i64(1))
        b.ret(i64(0))
        return fn

    def _k_rk4(self):
        self.kernel("fm_sol_push")          # used by the stop condition (until)
        fn = self._new_fn("fm_rk4", SOLP, [ODE_FN.as_pointer(), F64P, I64, F64P, F64, F64, F64,
                                           ODE_FN.as_pointer(), F64, F64])
        f, env, n, y0, t0, t1, h0, ev, tname, evtext = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        span = b.fsub(t1, t0)
        with b.if_then(b.not_(b.fcmp_unordered("!=", span, f64(0))), likely=False):
            self.raise_error(b, ERR_ODE_RANGE, t0, tname)
        # the range gives the direction (D39), the step its size
        ratio = b.call(self.intrinsic("fabs"), [b.fdiv(span, h0)])
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
        y, k1, k2, k3, k4, tmp, yn, kn = arr(), arr(), arr(), arr(), arr(), arr(), arr(), arr()
        with lp.range(i64(0), n) as k:
            b.store(b.load(b.gep(y0, [k])), b.gep(y, [k]))
        half = b.fmul(h, f64(0.5))
        has_ev = b.icmp_unsigned("!=", b.ptrtoint(ev, I64), i64(0))
        evsgn = b.alloca(F64)
        gbuf, ys, kbuf, r5 = arr(), arr(), arr(), arr()
        b.call(f, [t0, y, k1, env])
        self._emit_nan_check(b, lp, n, k1, t0, tname)
        b.store(f64(0), evsgn)
        with b.if_then(has_ev):
            b.call(ev, [t0, y, gbuf, env])
            b.store(self._emit_sign(b, b.load(gbuf)), evsgn)

        def record(idx, t, y, d):
            b.store(t, b.gep(tp, [idx]))
            base = b.mul(idx, n)
            with lp.range(i64(0), n) as k:
                b.store(b.load(b.gep(y, [k])), b.gep(yp, [b.add(base, k)]))
                b.store(b.load(b.gep(d, [k])), b.gep(dp, [b.add(base, k)]))
        def stages(t, ydst):
            """One RK4 step from (t, y) with k1 = f(t, y) already in k1; the new state goes to ydst."""
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
                b.store(b.fadd(b.load(b.gep(y, [k])), b.fmul(h6, acc)), b.gep(ydst, [k]))
        with b.if_else(has_ev) as (with_event, plain):
            with with_event:
                # keep the step's start for locating the crossing (interp.rk4 with an event)
                with lp.range(i64(0), steps) as s:
                    t = b.fadd(t0, b.fmul(b.sitofp(s, F64), h))
                    record(s, t, y, k1)
                    stages(t, yn)
                    s1 = b.add(s, i64(1))
                    tn = b.select(b.icmp_signed("==", s1, steps), t1, b.fadd(t0, b.fmul(b.sitofp(s1, F64), h)))
                    b.call(f, [tn, yn, kn, env])
                    self._emit_event_check(b, lp, f, ev, env, n, sp, evsgn, gbuf, ys, kbuf, r5, t, y, k1, tn, yn,
                                           kn, before_push=lambda: b.store(s1, b.gep(sp, [I32(0), I32(0)])))
                    with lp.range(i64(0), n) as k:
                        b.store(b.load(b.gep(yn, [k])), b.gep(y, [k]))
                        b.store(b.load(b.gep(kn, [k])), b.gep(k1, [k]))
                self.raise_error(b, ERR_NO_EVENT, t1, evtext)
            with plain:
                # the same numbers in place, without copies (the fast path: this is the RK4 benchmark)
                with lp.range(i64(0), steps) as s:
                    t = b.fadd(t0, b.fmul(b.sitofp(s, F64), h))
                    b.call(f, [t, y, k1, env])
                    record(s, t, y, k1)
                    stages(t, y)
                b.call(f, [t1, y, k1, env])
        record(steps, t1, y, k1)
        b.ret(sp)
        return fn

    RK4_SAMPLES = 8          # step-doubling checks after a fixed-step solve (redteam #5)
    RK4_WARN = 1e-3

    def _k_rk4_check(self):
        """A cheap error estimate for a fixed-step RK4 solution (step doubling at RK4_SAMPLES steps, from
        the stored points).  Mirrors interp.rk4_error."""
        fn = self._new_fn("fm_rk4_check", F64, [ODE_FN.as_pointer(), F64P, I64, SOLP], inline=False)
        f, env, n, sp = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        N = b.load(b.gep(sp, [I32(0), I32(0)]))
        tp = b.load(b.gep(sp, [I32(0), I32(3)]))
        yp = b.load(b.gep(sp, [I32(0), I32(4)]))
        dp = b.load(b.gep(sp, [I32(0), I32(5)]))
        with b.if_then(b.icmp_signed("<", N, i64(3))):
            b.ret(f64(0))
        mal = self.externs["malloc"]

        def arr():
            return b.bitcast(b.call(mal, [b.mul(n, i64(8))]), F64P)
        tmp, k2, k3, k4 = arr(), arr(), arr(), arr()
        worst, last = b.alloca(F64), b.alloca(I64)
        b.store(f64(0), worst)
        b.store(i64(-1), last)
        with lp.range(i64(0), i64(self.RK4_SAMPLES)) as k:
            s = b.sdiv(b.mul(k, b.sub(N, i64(3))), i64(self.RK4_SAMPLES - 1))
            t = b.load(b.gep(tp, [s]))
            tm = b.load(b.gep(tp, [b.add(s, i64(1))]))
            t2 = b.load(b.gep(tp, [b.add(s, i64(2))]))
            h = b.fsub(tm, t)
            even = b.fcmp_ordered("<=", b.call(fabs, [b.fsub(b.fsub(t2, tm), h)]),
                                  b.fmul(f64(1e-9), b.call(fabs, [h])))
            with b.if_then(b.and_(b.icmp_signed("!=", s, b.load(last)), even)):
                b.store(s, last)
                H = b.fsub(t2, t)
                half = b.fmul(H, f64(0.5))
                base = b.mul(s, n)

                def y(j):
                    return b.load(b.gep(yp, [b.add(base, j)]))
                with lp.range(i64(0), n) as j:
                    b.store(b.fadd(y(j), b.fmul(half, b.load(b.gep(dp, [b.add(base, j)])))), b.gep(tmp, [j]))
                th = b.fadd(t, half)
                b.call(f, [th, tmp, k2, env])
                with lp.range(i64(0), n) as j:
                    b.store(b.fadd(y(j), b.fmul(half, b.load(b.gep(k2, [j])))), b.gep(tmp, [j]))
                b.call(f, [th, tmp, k3, env])
                with lp.range(i64(0), n) as j:
                    b.store(b.fadd(y(j), b.fmul(H, b.load(b.gep(k3, [j])))), b.gep(tmp, [j]))
                b.call(f, [b.fadd(t, H), tmp, k4, env])
                H6 = b.fdiv(H, f64(6))
                with lp.range(i64(0), n) as j:
                    k1j = b.load(b.gep(dp, [b.add(base, j)]))
                    s23 = b.fadd(b.load(b.gep(k2, [j])), b.load(b.gep(k3, [j])))
                    acc = b.fadd(b.fadd(k1j, b.fmul(f64(2), s23)), b.load(b.gep(k4, [j])))
                    yj = y(j)
                    y2 = b.fadd(yj, b.fmul(H6, acc))
                    yr = b.load(b.gep(yp, [b.add(b.mul(b.add(s, i64(2)), n), j)]))
                    sc = b.fadd(b.call(self.intrinsic("maxnum"), [b.call(fabs, [yj]), b.call(fabs, [yr])]),
                                b.call(fabs, [b.fsub(yr, yj)]))
                    with b.if_then(b.fcmp_ordered(">", sc, f64(0))):
                        e = b.fdiv(b.call(fabs, [b.fsub(y2, yr)]), sc)
                        with b.if_then(b.fcmp_ordered(">", e, b.load(worst))):
                            b.store(e, worst)
        free = self.extern("free", VOID, [I8P])
        for p in (tmp, k2, k3, k4):
            b.call(free, [b.bitcast(p, I8P)])
        b.ret(b.fmul(b.fdiv(b.load(worst), f64(30)), b.sitofp(b.sub(N, i64(1)), F64)))
        return fn

    def _emit_first_step(self, b, lp, f, env, n, y, k, tmp, t0, dirn, aspan, rtol, hv):
        """The first trial step (Hairer–Wanner, gauntlet A5): about 1% of the time over which the solution
        changes by itself (|y|/|y'|), refined by a probe of y''; the old 10⁻⁴ of the range when all y are 0.
        Mirrored by _first_step in interp.py."""
        fabs = self.intrinsic("fabs")
        sq = self.intrinsic("sqrt")
        d0, d1, cnt = b.alloca(F64), b.alloca(F64), b.alloca(F64)
        for v in (d0, d1, cnt):
            b.store(f64(0), v)
        with lp.range(i64(0), n) as j:
            yj = b.call(fabs, [b.load(b.gep(y, [j]))])
            with b.if_then(b.fcmp_ordered(">", yj, f64(0))):
                sc = b.fmul(rtol, yj)
                r0 = b.fdiv(yj, sc)
                r1 = b.fdiv(b.load(b.gep(k[0], [j])), sc)
                b.store(b.fadd(b.load(d0), b.fmul(r0, r0)), d0)
                b.store(b.fadd(b.load(d1), b.fmul(r1, r1)), d1)
                b.store(b.fadd(b.load(cnt), f64(1)), cnt)
        b.store(b.fmul(aspan, f64(1e-4)), hv)
        ok = b.and_(b.fcmp_ordered(">", b.load(cnt), f64(0)), b.fcmp_ordered(">", b.load(d1), f64(0)))
        ok = b.and_(ok, b.fcmp_ordered("<", b.load(d1), f64(math.inf)))
        with b.if_then(ok):
            h0 = b.fmul(f64(0.01), b.call(sq, [b.fdiv(b.load(d0), b.load(d1))]))
            h0 = b.call(self.intrinsic("minnum"), [h0, aspan])
            with lp.range(i64(0), n) as j:
                b.store(b.fadd(b.load(b.gep(y, [j])), b.fmul(b.fmul(dirn, h0), b.load(b.gep(k[0], [j])))),
                        b.gep(tmp, [j]))
            b.call(f, [b.fadd(t0, b.fmul(dirn, h0)), tmp, k[1], env])
            d2 = b.alloca(F64)
            b.store(f64(0), d2)
            with lp.range(i64(0), n) as j:
                yj = b.call(fabs, [b.load(b.gep(y, [j]))])
                with b.if_then(b.fcmp_ordered(">", yj, f64(0))):
                    r2 = b.fdiv(b.fsub(b.load(b.gep(k[1], [j])), b.load(b.gep(k[0], [j]))), b.fmul(rtol, yj))
                    b.store(b.fadd(b.load(d2), b.fmul(r2, r2)), d2)
            # a rate ω = max(|y'/y|, √|y''/y|) (dimensionally consistent, so any time unit works):
            # a 5th-order step with (h ω)⁵ ≈ rtol
            dd1 = b.fmul(b.call(sq, [b.fdiv(b.load(d1), b.load(cnt))]), rtol)
            dd2 = b.call(sq, [b.fmul(b.fdiv(b.call(sq, [b.fdiv(b.load(d2), b.load(cnt))]), h0), rtol)])
            m = b.call(self.intrinsic("maxnum"), [dd1, dd2])
            h1 = b.select(b.fcmp_ordered(">", m, f64(0)),
                          b.fdiv(b.call(self.intrinsic("pow"), [rtol, f64(0.2)]), m),
                          b.fmul(f64(100), h0))
            h = b.call(self.intrinsic("minnum"), [b.fmul(f64(100), h0), h1])
            h = b.call(self.intrinsic("minnum"), [h, aspan])
            with b.if_then(b.fcmp_ordered("==", h, h)):       # not NaN (a NaN probe keeps the default)
                b.store(h, hv)

    def _k_dp45(self):
        push = self.kernel("fm_sol_push")
        fn = self._new_fn("fm_dp45", SOLP, [ODE_FN.as_pointer(), F64P, I64, F64P, F64, F64, F64,
                                            ODE_FN.as_pointer(), F64, F64, I64, F64P])
        f, env, n, y0, t0, t1, rtol, ev, tname, evtext, tdep, atol = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        mal = self.externs["malloc"]
        fabs = self.intrinsic("fabs")
        fmax = self.intrinsic("maxnum")
        fmin = self.intrinsic("minnum")

        def arr():
            return b.bitcast(b.call(mal, [b.mul(n, i64(8))]), F64P)
        y, ynew, tmp = arr(), arr(), arr()
        k = [arr() for _ in range(7)]
        with lp.range(i64(0), n) as j:
            b.store(b.load(b.gep(y0, [j])), b.gep(y, [j]))
        sp = self._sol_alloc(b, n, i64(256))
        span = b.fsub(t1, t0)
        with b.if_then(b.not_(b.fcmp_unordered("!=", span, f64(0))), likely=False):
            self.raise_error(b, ERR_ODE_RANGE, t0, tname)
        dirn = b.select(b.fcmp_ordered(">", span, f64(0)), f64(1), f64(-1))   # towards smaller t (D39)
        aspan = b.call(fabs, [span])
        tv = b.alloca(F64)
        hv = b.alloca(F64)
        nsteps = b.alloca(I64)
        rejcount = b.alloca(I64)
        firstrej = b.alloca(F64)
        hastgt = b.alloca(I64)         # a located jump in f (D40): land exactly on it
        tgtlo = b.alloca(F64)
        tgthi = b.alloca(F64)
        probed = b.alloca(I64)
        b.store(i64(0), rejcount)
        b.store(f64(0), firstrej)
        b.store(i64(0), hastgt)
        b.store(f64(0), tgtlo)
        b.store(f64(0), tgthi)
        b.store(i64(0), probed)
        b.store(t0, tv)
        b.store(b.fmul(aspan, f64(1e-4)), hv)
        b.store(i64(0), nsteps)
        stiffn = b.alloca(I64)         # stiffness test (Hairer's DOPRI5): stiff-looking steps in a row
        nonstiff = b.alloca(I64)
        b.store(i64(0), stiffn)
        b.store(i64(0), nonstiff)
        b.call(f, [t0, y, k[0], env])
        self._emit_nan_check(b, lp, n, k[0], t0, tname)
        self._emit_first_step(b, lp, f, env, n, y, k, tmp, t0, dirn, aspan, rtol, hv)
        b.call(push, [sp, t0, y, k[0]])
        has_ev = b.icmp_unsigned("!=", b.ptrtoint(ev, I64), i64(0))
        evsgn = b.alloca(F64)
        gbuf, ys, kbuf, r5 = arr(), arr(), arr(), arr()
        jfe, jfm, jfx, jflo, jfhi, jout = arr(), arr(), arr(), arr(), arr(), b.bitcast(b.call(mal, [i64(16)]), F64P)
        b.store(f64(0), evsgn)
        with b.if_then(has_ev):
            b.call(ev, [t0, y, gbuf, env])
            b.store(self._emit_sign(b, b.load(gbuf)), evsgn)
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
        remaining = b.fmul(dirn, b.fsub(t1, t))
        go = b.fcmp_ordered(">", remaining, b.fmul(f64(1e-14), b.call(fabs, [t1])))
        go = b.and_(go, b.fcmp_ordered(">", remaining, f64(0)))
        b.cbranch(go, body_bb, end_bb)
        b.position_at_end(body_bb)
        cnt = b.add(b.load(nsteps), i64(1))
        b.store(cnt, nsteps)
        with b.if_then(b.icmp_signed(">", cnt, i64(20_000_000))):
            self.raise_error(b, ERR_ODE_STEPS, t, tname)
        tgt = b.icmp_signed("!=", b.load(hastgt), i64(0))
        stop = b.select(tgt, b.load(tgtlo), t1)
        rstop = b.fmul(dirn, b.fsub(stop, t))
        hvv = b.load(hv)
        land = b.fcmp_ordered(">=", hvv, rstop)
        h = b.select(land, rstop, hvv)
        with b.if_then(b.fcmp_ordered("<", h, b.fmul(f64(1e-15), b.fadd(b.call(fabs, [t]), aspan)))):
            # a blow-up (some component grew over 10³ times both its start and the largest start, or
            # isn't finite) or, if nothing grew, the tolerance (D160); mirrors runtime/stiff.step_small_kind
            big = b.alloca(F64)
            grew = b.alloca(I64)
            b.store(f64(0), big)
            b.store(i64(0), grew)
            with lp.range(i64(0), n) as j:
                b.store(b.call(fmax, [b.load(big), b.call(fabs, [b.load(b.gep(y0, [j]))])]), big)
            with lp.range(i64(0), n) as j:
                vj = b.load(b.gep(y, [j]))
                lim = b.fmul(f64(1e3), b.call(fmax, [b.call(fabs, [b.load(b.gep(y0, [j]))]), b.load(big)]))
                bad = b.or_(b.fcmp_unordered("!=", b.fsub(vj, vj), f64(0)),
                            b.fcmp_ordered(">", b.call(fabs, [vj]), lim))
                with b.if_then(bad):
                    b.store(i64(1), grew)
            kind = b.select(b.icmp_signed("!=", b.load(grew), i64(0)), i64(ERR_ODE_H), i64(ERR_ODE_H_FLAT))
            self.raise_error(b, kind, t, tname)
        tn = b.select(land, stop, b.fadd(t, b.fmul(dirn, h)))
        hs = b.select(land, b.fsub(stop, t), b.fmul(dirn, h))
        for s in range(1, 7):
            with lp.range(i64(0), n) as j:
                acc = b.load(b.gep(y, [j]))
                for m in range(s):
                    if A[s][m] != 0:
                        acc = b.fadd(acc, b.fmul(b.fmul(hs, f64(A[s][m])), b.load(b.gep(k[m], [j]))))
                b.store(acc, b.gep(ynew if s == 6 else tmp, [j]))
            ts = tn if Cn[s] == 1 else b.fadd(t, b.fmul(hs, f64(Cn[s])))
            b.call(f, [ts, ynew if s == 6 else tmp, k[s], env])
        errsum = b.alloca(F64)
        b.store(f64(0), errsum)
        with lp.range(i64(0), n) as j:
            e = f64(0)
            for m in range(7):
                if E[m] != 0:
                    e = b.fadd(e, b.fmul(f64(E[m]), b.load(b.gep(k[m], [j]))))
            e = b.fmul(e, hs)
            yo = b.call(fabs, [b.load(b.gep(y, [j]))])
            yn = b.call(fabs, [b.load(b.gep(ynew, [j]))])
            # relative error norm (scale-free, so it works in any units and keeps decays accurate):
            # relative to the size of the component, or to this step's own change when the component
            # passes through zero
            dlt = b.call(fabs, [b.fsub(b.load(b.gep(ynew, [j])), b.load(b.gep(y, [j])))])
            sc = b.fmul(rtol, b.fadd(b.call(fmax, [yo, yn]), dlt))
            sc = b.fadd(sc, b.load(b.gep(atol, [j])))    # `absolute a` (D160); 0 without it
            sc = b.fadd(sc, f64(5e-324))   # smallest subnormal: only guards 0/0 (A37)
            r = b.fdiv(e, sc)
            b.store(b.fadd(b.load(errsum), b.fmul(r, r)), errsum)
        errn = b.call(self.intrinsic("sqrt"), [b.fdiv(b.load(errsum), b.sitofp(n, F64))])
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
                self._emit_stiff_test(b, lp, n, cnt, h, k[5], k[6], tmp, ynew, stiffn, nonstiff)
                with b.if_then(has_ev):
                    self._emit_event_check(b, lp, f, ev, env, n, sp, evsgn, gbuf, ys, kbuf, r5, t, y, k[0], tn,
                                           ynew, k[6], ks=k)
                b.store(tn, tv)
                with lp.range(i64(0), n) as j:
                    b.store(b.load(b.gep(ynew, [j])), b.gep(y, [j]))
                    b.store(b.load(b.gep(k[6], [j])), b.gep(k[0], [j]))   # FSAL
                b.call(push, [sp, tn, y, k[0]])
                b.store(b.select(stalled, b.fmul(h, f64(2.0)), b.fmul(h, fac)), hv)
                with b.if_then(b.and_(land, tgt)):          # at the jump: restart on its far side
                    b.store(i64(0), hastgt)
                    th = b.load(tgthi)
                    b.store(th, tv)
                    b.call(f, [th, y, k[0], env])
                    b.call(push, [sp, th, y, k[0]])
                    b.store(h, hv)
                b.store(i64(0), rejcount)
                b.store(i64(0), probed)
            with no:
                with b.if_then(b.icmp_signed("==", nrej, i64(0))):
                    b.store(errn, firstrej)
                b.store(b.add(nrej, i64(1)), rejcount)
                b.store(b.fmul(h, b.call(fmin, [fac, f64(1.0)])), hv)
                probe = b.and_(b.icmp_signed("!=", tdep, i64(0)),
                               b.and_(b.icmp_signed("==", b.load(probed), i64(0)), b.not_(tgt)))
                with b.if_then(probe):
                    b.store(i64(1), probed)
                    found = b.call(self.kernel("fm_find_jump"), [f, env, n, t, tn, y, k[0], jfe, jfm, jfx, jflo,
                                                                 jfhi, jout])
                    with b.if_then(b.icmp_signed("!=", found, i64(0))):
                        lo = b.load(jout)
                        hi = b.load(b.gep(jout, [i64(1)]))
                        b.store(h, hv)
                        with b.if_else(b.fcmp_ordered("==", lo, t)) as (at_t, later):
                            with at_t:             # the jump is right at t: k[0] is from the near side
                                b.store(hi, tv)
                                b.call(f, [hi, y, k[0], env])
                                b.call(push, [sp, hi, y, k[0]])
                                b.store(i64(0), rejcount)
                                b.store(i64(0), probed)
                            with later:
                                b.store(i64(1), hastgt)
                                b.store(lo, tgtlo)
                                b.store(hi, tgthi)
        b.branch(cond_bb)
        b.position_at_end(end_bb)
        with b.if_then(has_ev, likely=False):
            self.raise_error(b, ERR_NO_EVENT, t1, evtext)
        b.ret(sp)
        return fn

    ROOT_SCAN = 200          # sub-intervals searched for the first sign change
    NOISE = 1e-12            # |lhs - rhs| at most this times |lhs| + |rhs| near the root: rounding noise (#36)

    def _k_root(self):
        """The FIRST root of f in [a, b] (D32, #2): scan ROOT_SCAN sub-intervals from a for the first sign
        change (or an exact zero), always; then Illinois (modified regula falsi), which keeps a bracket
        and converges superlinearly, to full double precision.  g(x) = |lhs| + |rhs| (or null): a sign
        change in rounding noise gets a warning (#36).  Mirrors interp.root."""
        fn = self._new_fn("fm_root", F64, [SCALAR_FN.as_pointer(), F64P, F64, F64, SCALAR_FN.as_pointer()],
                          inline=False)
        f, env, a0, b0, g = fn.args
        b = ir.IRBuilder(fn.append_basic_block("e"))
        lp = LoopHelper(b, fn)
        fabs = self.intrinsic("fabs")
        has_g = b.icmp_unsigned("!=", b.ptrtoint(g, I64), i64(0))
        ln0, fmt0 = b.load(self.curline), b.load(self.errfmt)

        def noisy(fv, x):
            return b.fcmp_ordered("<=", b.call(fabs, [fv]), b.fmul(f64(self.NOISE), b.call(g, [x, env])))

        def warn(x):
            b.call(self.externs["fm_warn"], [i64(1), x, ln0, fmt0])
        a, c, fa, fc = b.alloca(F64), b.alloca(F64), b.alloca(F64), b.alloca(F64)
        b.store(b.call(f, [a0, env]), fa)
        with b.if_then(b.fcmp_ordered("==", b.load(fa), f64(0))):
            b.ret(a0)

        def opposite(x, y):
            return b.fcmp_ordered("<", b.fmul(x, y), f64(0))
        found = b.alloca(I64)
        b.store(i64(0), found)
        fprev = b.alloca(F64)
        b.store(b.load(fa), fprev)
        nan = f64(math.nan)
        fjump, xjump, pole = b.alloca(F64), b.alloca(F64), b.alloca(F64)   # a scan point on a pole (redteam #4)
        b.store(nan, fjump)
        b.store(f64(0), xjump)
        b.store(nan, pole)
        h = b.fdiv(b.fsub(b0, a0), f64(self.ROOT_SCAN))
        with lp.range(i64(1), i64(self.ROOT_SCAN + 1)) as i:
            with b.if_then(b.icmp_signed("==", b.load(found), i64(0))):
                xi = b.select(b.icmp_signed("==", i, i64(self.ROOT_SCAN)), b0,
                              b.fadd(a0, b.fmul(b.sitofp(i, F64), h)))
                fi = b.call(f, [xi, env])
                with b.if_then(b.fcmp_ordered("==", fi, f64(0))):
                    with b.if_then(has_g):
                        with b.if_then(noisy(b.load(fprev), b.fsub(xi, h))):
                            warn(xi)
                    b.ret(xi)
                with b.if_else(b.fcmp_ordered("==", b.call(fabs, [fi]), f64(math.inf))) as (onpole, other):
                    with onpole:      # on a pole: not a crossing; skip past it
                        b.store(b.load(fprev), fjump)
                        b.store(xi, xjump)
                        b.store(nan, fprev)
                    with other:
                        pl = b.load(pole)
                        with b.if_then(b.and_(opposite(b.load(fjump), fi), b.fcmp_unordered("uno", pl, pl))):
                            b.store(b.load(xjump), pole)
                        b.store(nan, fjump)
                        with b.if_then(opposite(b.load(fprev), fi)):
                            b.store(b.fsub(xi, h), a)
                            b.store(b.load(fprev), fa)
                            b.store(xi, c)
                            b.store(fi, fc)
                            b.store(i64(1), found)
                        b.store(fi, fprev)
        with b.if_then(b.icmp_signed("==", b.load(found), i64(0))):
            pl = b.load(pole)
            with b.if_then(b.fcmp_ordered("ord", pl, pl)):
                self.raise_error(b, ERR_POLE, pl, f64(0))
            self.raise_error(b, ERR_ROOT, a0, b0)
        with b.if_then(has_g):
            with b.if_then(noisy(b.load(fa), b.load(a))):
                with b.if_then(noisy(b.load(fc), b.load(c))):
                    warn(b.load(c))
        side = b.alloca(I64)          # which end was kept last time (Illinois halves the stale end)
        b.store(i64(0), side)
        m0 = b.call(self.intrinsic("maxnum"), [b.call(fabs, [b.load(fa)]), b.call(fabs, [b.load(fc)])])
        with lp.range(i64(0), i64(300)):
            xa, xc, ya, yc = b.load(a), b.load(c), b.load(fa), b.load(fc)
            big = b.call(self.intrinsic("maxnum"), [b.call(fabs, [xa]), b.call(fabs, [xc])])
            with b.if_then(b.fcmp_ordered("<=", b.call(fabs, [b.fsub(xc, xa)]), b.fmul(f64(4e-16), big))):
                best = b.select(b.fcmp_ordered("<", b.call(fabs, [ya]), b.call(fabs, [yc])), xa, xc)
                # a jump across 0 (a pole) instead of a crossing: |f| grew instead of shrinking
                fb = b.call(fabs, [b.call(f, [best, env])])
                with b.if_then(b.fcmp_ordered(">", fb, m0)):
                    self.raise_error(b, ERR_POLE, best, f64(0))
                b.ret(best)
            x = b.fsub(xc, b.fdiv(b.fmul(yc, b.fsub(xc, xa)), b.fsub(yc, ya)))
            mid = b.fmul(f64(0.5), b.fadd(xa, xc))
            lo_, hi_ = b.call(self.intrinsic("minnum"), [xa, xc]), b.call(self.intrinsic("maxnum"), [xa, xc])
            inside = b.and_(b.fcmp_ordered(">", x, lo_), b.fcmp_ordered("<", x, hi_))
            x = b.select(inside, x, mid)
            fx = b.call(f, [x, env])
            with b.if_then(b.fcmp_ordered("==", fx, f64(0))):     # exact root
                b.ret(x)
            with b.if_then(b.fcmp_unordered("uno", fx, fx)):      # undefined inside the bracket: not a root
                self.raise_error(b, ERR_POLE, x, f64(0))
            with b.if_else(opposite(fx, yc)) as (then, other):
                with then:            # root between x and c: a <- c
                    b.store(xc, a)
                    b.store(yc, fa)
                    b.store(i64(0), side)
                with other:           # root between a and x: keep a, halve its value if kept twice
                    with b.if_then(b.icmp_signed("==", b.load(side), i64(1))):
                        b.store(b.fmul(f64(0.5), b.load(fa)), fa)
                    b.store(i64(1), side)
            b.store(x, c)
            b.store(fx, fc)
        b.ret(b.load(c))
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
        rhs = b.load(b.gep(sp, [I32(0), I32(6)]))
        ybuf = b.alloca(F64, size=dim)
        kbuf = b.alloca(F64, size=dim)
        tfirst = b.load(tp)
        tlast = b.load(b.gep(tp, [b.sub(n, i64(1))]))
        span = b.fsub(tlast, tfirst)
        slack = b.fmul(f64(1e-9), b.call(self.intrinsic("fabs"), [span]))
        lo_t = b.call(self.intrinsic("minnum"), [tfirst, tlast])
        hi_t = b.call(self.intrinsic("maxnum"), [tfirst, tlast])
        bad = b.or_(b.fcmp_ordered("<", t, b.fsub(lo_t, slack)), b.fcmp_ordered(">", t, b.fadd(hi_t, slack)))
        bad = b.or_(bad, b.fcmp_unordered("uno", t, t))
        sg = b.select(b.fcmp_ordered(">=", tlast, tfirst), f64(1), f64(-1))
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
        with b.if_else(b.fcmp_ordered("<=", b.fmul(tm, sg), b.fmul(t, sg))) as (yes, no):
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
        s2 = b.fmul(s, s)
        s3 = b.fmul(s2, s)
        h00 = b.fadd(b.fsub(b.fmul(f64(2), s3), b.fmul(f64(3), s2)), f64(1))
        h10 = b.fadd(b.fsub(s3, b.fmul(f64(2), s2)), s)
        h01 = b.fadd(b.fmul(f64(-2), s3), b.fmul(f64(3), s2))
        h11 = b.fsub(s3, s2)

        def herm(c):
            ia = b.add(b.mul(i, dim), c)
            ib = b.add(b.mul(i1, dim), c)
            ya = b.load(b.gep(yp, [ia]))
            yb = b.load(b.gep(yp, [ib]))
            ma = b.fmul(b.load(b.gep(dp, [ia])), hh)
            mb = b.fmul(b.load(b.gep(dp, [ib])), hh)
            return ya, yb, ma, mb, b.fadd(b.fadd(b.fmul(h00, ya), b.fmul(h10, ma)),
                                          b.fadd(b.fmul(h01, yb), b.fmul(h11, mb)))
        # a derivative from the right-hand side at the interpolated state: as accurate as the solution
        # itself (the Hermite's own derivative is an order less accurate: S9, D46)
        has_rhs = b.icmp_unsigned("!=", b.ptrtoint(rhs, I64), i64(0))
        with b.if_then(b.and_(b.icmp_signed("!=", use_dy, i64(0)), has_rhs)):
            lp = LoopHelper(b, fn)
            with lp.range(i64(0), dim) as k:
                b.store(herm(k)[4], b.gep(ybuf, [k]))
            env = b.load(b.gep(sp, [I32(0), I32(7)]))
            b.call(b.bitcast(rhs, ODE_FN.as_pointer()), [t, ybuf, kbuf, env])
            b.ret(b.load(b.gep(kbuf, [comp])))
        ya, yb, ma, mb, r = herm(comp)
        with b.if_then(b.icmp_signed("!=", use_dy, i64(0))):
            # derivative of the Hermite cubic: third-order accurate (linear interpolation of the
            # stored slopes would only be second order)
            d00 = b.fsub(b.fmul(f64(6), s2), b.fmul(f64(6), s))
            d10 = b.fadd(b.fsub(b.fmul(f64(3), s2), b.fmul(f64(4), s)), f64(1))
            d01 = b.fsub(b.fmul(f64(6), s), b.fmul(f64(6), s2))
            d11 = b.fsub(b.fmul(f64(3), s2), b.fmul(f64(2), s))
            num_ = b.fadd(b.fadd(b.fmul(d00, ya), b.fmul(d10, ma)), b.fadd(b.fmul(d01, yb), b.fmul(d11, mb)))
            b.ret(b.fdiv(num_, hh))
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
        if sym.storage == "arena" and isinstance(sym.ty, (VecTy, MatTy)):
            return self.b.load(self.slot(sym), align=8)     # REPL slots are only 8-byte aligned
        return self.b.load(self.slot(sym))

    def store(self, sym, v):
        if sym.storage == "arena" and isinstance(sym.ty, (VecTy, MatTy)):
            self.b.store(v, self.slot(sym), align=8)
            return
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

    def fail_if(self, status):
        """Stop the program if a runtime callback returned a nonzero status (it set the message)."""
        with self.b.if_then(self.b.icmp_signed("!=", status, i64(0)), likely=False):
            self.fail(ERR_PENDING)

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
            elif kind in ("vec", "mvec", "mat"):
                v = self.expr(payload)
                n = payload.ty.n
                arr = self.alloca(ir.ArrayType(F64, n))
                for k in range(n):
                    b.store(b.extract_element(v, I32(k)), b.gep(arr, [I32(0), I32(k)]))
                p = b.gep(arr, [I32(0), I32(0)])
                if kind == "mat":
                    b.call(ex["fm_print_mat"], [i64(fid), p, i64(payload.ty.r), i64(payload.ty.c)])
                else:
                    b.call(ex["fm_print_" + kind], [i64(fid), p, i64(n)])
            elif kind == "cplx":
                v = self.expr(payload)
                f = self.mg.extern("fm_print_cplx", VOID, [I64, F64, F64])
                b.call(f, [i64(fid), b.extract_element(v, I32(0)), b.extract_element(v, I32(1))])
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
        if s.method in codegen_m3.PY_SOLVES:           # eigenvalue problems, PDEs (D82, D83)
            return codegen_m3.py_solve(self, s)
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
        ev = self.mg.lambda_for(s.event) if getattr(s, "event", None) is not None else \
            ir.Constant(ODE_FN.as_pointer(), None)
        extra = [ev, f64(getattr(s, "tname", -1)), f64(getattr(s, "evtext", -1))]
        if s.method in ("radau", "bdf"):
            # implicit methods for stiff equations run in Python (SciPy, D42) and call the right-hand side
            # back through fm_ode_guard; status 1: the solver stopped (message set), 2: the right side did
            fmt = getattr(s, "tfmt", -1)
            self.b.store(i64(fmt), self.mg.errfmt)
            self.mark_line()
            guard = self.mg.kernel("fm_ode_guard")
            stiff = self.mg.extern("fm_stiff", I64, [I8P, I8P, F64P, I64, F64P, F64, F64, F64, I8P, I64, F64, F64,
                                                     I64, SOLP.as_pointer(), F64P])
            out = self.alloca(SOLP)
            evp = b.bitcast(ev, I8P)
            status = b.call(stiff, [b.bitcast(guard, I8P), b.bitcast(fn, I8P), env, i64(n), y0p, t0, t1,
                                    f64(s.rtol), evp, i64(1 if s.method == "bdf" else 0), extra[1], extra[2],
                                    i64(fmt), out, self.abs_tolerances(s, n, t0, t1, null_if_none=True)])
            with b.if_then(b.icmp_signed("!=", status, i64(0)), likely=False):
                with b.if_then(b.icmp_signed("==", status, i64(2))):
                    b.call(self.mg.externs["longjmp"], [b.bitcast(self.mg.jmpbuf, I8P), ir.Constant(I32, 1)])
                    b.unreachable()
                self.fail(ERR_PENDING)
            sol = b.load(out)
        elif s.method == "rk4":
            h0 = self.expr(s.step)
            self.b.store(i64(getattr(s, "tfmt", -1)), self.mg.errfmt)
            self.mark_line()
            k = self.mg.kernel("fm_rk4")
            sol = b.call(k, [fn, env, i64(n), y0p, t0, t1, h0] + extra)
            est = b.call(self.mg.kernel("fm_rk4_check"), [fn, env, i64(n), sol])
            with b.if_then(b.fcmp_ordered(">", est, f64(self.mg.RK4_WARN)), likely=False):
                b.call(self.mg.externs["fm_warn"], [i64(7), est, b.load(self.mg.curline), i64(-1)])
        else:
            self.b.store(i64(getattr(s, "tfmt", -1)), self.mg.errfmt)
            self.mark_line()
            k = self.mg.kernel("fm_dp45")
            sol = b.call(k, [fn, env, i64(n), y0p, t0, t1, f64(s.rtol)] + extra +
                         [i64(1 if getattr(s, "tdep", False) else 0), self.abs_tolerances(s, n, t0, t1)])
        if getattr(s.sol_sym, "needs_rhs", False):
            self.attach_rhs(s, sol)
        self.store(s.sol_sym, sol)

    def abs_tolerances(self, s, n, t0, t1, null_if_none=False):
        """The solve's absolute tolerance per state component (D160), as a stack array: the value, divided
        by |t1 - t0|^k for a derivative slot that borrows its unknown's (runtime/stiff.abs_tolerances);
        zeros (or a null pointer for the stiff callback) without  absolute."""
        spec = getattr(s, "atol", None)
        if not spec and null_if_none:
            return ir.Constant(F64P, None)
        b = self.b
        arr = self.alloca(ir.ArrayType(F64, n))
        span = b.call(self.mg.intrinsic("fabs"), [b.fsub(t1, t0)]) if spec else None
        for j in range(n):
            if not spec:
                v = f64(0)
            else:
                val, k = spec[j]
                v = f64(val)
                for _ in range(k):
                    v = b.fdiv(v, span)
            b.store(v, b.gep(arr, [I32(0), I32(j)]))
        return b.gep(arr, [I32(0), I32(0)])

    def attach_rhs(self, s, sol):
        """Keep the right-hand side with the solution, so x'(t) is f at the interpolated state (D46).  Its
        env is a heap copy of the captured values and of the module-level numbers it reads, taken now:
        a variable changed after the solve doesn't change the solution's derivative."""
        b = self.b
        elam = getattr(s, "_eval_lam", None)
        if elam is None:
            lam = s.rhs
            own = {x.id for x in list(lam.params) + list(lam.state) + list(lam.locals) + list(lam.captures)}
            extra = [x for x in I.referenced_syms(lam.body) if x.id not in own and
                     x.storage in ("global", "arena") and isinstance(x.ty, (NumTy, BoolTy, VecTy, MatTy))]
            elam = I.ILambda("ode", lam.name + "_eval")
            elam.params, elam.state, elam.locals, elam.body = lam.params, lam.state, lam.locals, lam.body
            elam.captures = list(lam.captures) + extra
            s._eval_lam = elam
        b.store(b.bitcast(self.mg.lambda_for(elam), I8P), b.gep(sol, [I32(0), I32(6)]))
        nslots = sum(env_slots(x) for x in elam.captures)
        if nslots:
            env = b.bitcast(b.call(self.mg.externs["malloc"], [i64(8 * nslots)]), F64P)
            self.fill_env(elam, env)
            b.store(env, b.gep(sol, [I32(0), I32(7)]))

    def make_env(self, lam):
        if not lam.captures:
            return ir.Constant(F64P, None)
        env = self.alloca(ir.ArrayType(F64, sum(env_slots(s) for s in lam.captures)))
        return self.fill_env(lam, self.b.gep(env, [I32(0), I32(0)]))

    def fill_env(self, lam, env):
        """Store the captured values of lam into env (a double*); returns env."""
        b = self.b
        i = 0
        for sym in lam.captures:
            v = self.load(sym)
            if isinstance(sym.ty, BoolTy):
                v = b.uitofp(v, F64)
            if isinstance(sym.ty, SolTy):             # a solution: its pointer's bits (D48)
                v = b.bitcast(b.ptrtoint(v, I64), F64)
            if isinstance(sym.ty, (VecTy, MatTy)):       # a vector or matrix takes n slots
                for k in range(sym.ty.n):
                    b.store(b.extract_element(v, I32(k)), b.gep(env, [i64(i)]))
                    i += 1
                continue
            b.store(v, b.gep(env, [i64(i)]))
            i += 1
        return env

    def s_SFit(self, s):
        b = self.b
        self.mg.lambda_for(s.model)
        n = len(s.param_syms)
        p = self.alloca(ir.ArrayType(F64, 2 * n))      # parameters, then their standard errors
        for i, g in enumerate(s.guesses):
            v = self.expr(g) if g is not None else f64(math.nan)
            b.store(v, b.gep(p, [I32(0), I32(i)]))
        h = self.expr(s.data)
        self.fail_if(b.call(self.mg.externs["fm_fit"], [i64(s.fit_id), h, b.gep(p, [I32(0), I32(0)])]))
        for i, sym in enumerate(s.param_syms):
            self.store(sym, b.load(b.gep(p, [I32(0), I32(i)])))
        for i, sym in enumerate(getattr(s, "err_syms", [])):
            self.store(sym, b.load(b.gep(p, [I32(0), I32(n + i)])))

    def s_SPlot(self, s):
        b, ex = self.b, self.mg.externs
        for idx, e in enumerate(s.series):
            kind = e["kind"]
            if kind == "lists":
                y = self.expr(e["y"])
                x = self.expr(e["x"])
                self.fail_if(b.call(ex["fm_plot_series"], [i64(s.plot_id), i64(idx), self.ldata(x), self.llen(x),
                                                           self.ldata(y), self.llen(y)]))
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
                self.fail_if(b.call(ex["fm_plot_series"], [i64(s.plot_id), i64(idx), xs, i64(npts), ys, i64(npts)]))
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

    def e_IVecIndex(self, e):
        """v[i], M[i, j], M[i] with indexes known at run time, each checked (#54)."""
        b = self.b
        v = self.expr(e.v)
        base = i64(0)
        for idx_e, size, stride in e.idxs:
            idx = self.expr(idx_e)
            inside = b.and_(b.fcmp_ordered(">=", idx, f64(1)), b.fcmp_ordered("<=", idx, f64(size)))
            i = b.fptosi(b.select(inside, idx, f64(1)), I64)
            bad = b.or_(b.not_(inside), b.fcmp_unordered("!=", b.sitofp(i, F64), idx))
            with b.if_then(bad, likely=False):
                self.fail(ERR_INDEX, idx, f64(-size))
            base = b.add(base, b.mul(b.sub(i, i64(1)), i64(stride)))
        base = b.trunc(base, I32)
        xs = [b.extract_element(v, b.add(base, I32(o))) for o in e.offs]
        return xs[0] if len(xs) == 1 else self.pack(xs)

    # ------------------------------------------------------------ matrices (D29)
    def unpack(self, v, n):
        return [self.b.extract_element(v, I32(k)) for k in range(n)]

    def pack(self, xs):
        v = ir.Constant(ir.VectorType(F64, len(xs)), ir.Undefined)
        for k, x in enumerate(xs):
            v = self.b.insert_element(v, x, I32(k))
        return v

    def matrix_op(self, e, args):
        """Matrix built-ins, unrolled into straight-line code by fermium.linalg (same ops as interp.py)."""
        b = self.b
        name = e.name
        if name == "shuffle":
            src = args[0]
            mask = ir.Constant(ir.VectorType(I32, len(e.idx)), [ir.Constant(I32, k) for k in e.idx])
            return b.shuffle_vector(src, ir.Constant(src.type, ir.Undefined), mask)
        ops = LLOps(self)
        m = e.args[0].ty
        a = self.unpack(args[0], m.n)
        if name == "matmul":
            r, k, c = e.dims3
            out = linalg.matmul(ops, a, r, k, self.unpack(args[1], k * c), c)
            return out[0] if len(out) == 1 else self.pack(out)
        if name == "det":
            return linalg.det(ops, a, m.r)
        if name in ("eigenvalues", "eigenvectors"):
            return self.eigen_op(e, args, ops, a, m.r)
        if name == "inverse":
            out, piv = linalg.inverse(ops, a, m.r, f64(1), f64(0))
        else:
            out, piv = linalg.solve(ops, a, m.r, self.unpack(args[1], m.r), 1)
        bad = None
        for p in piv:
            z = b.fcmp_ordered("==", p, f64(0))
            bad = z if bad is None else b.or_(bad, z)
        saved = getattr(self, "line", 0)
        self.line = e.line or saved
        # the mass matrix of an ODE (D47): the error names the equation's variable and its value
        sing = (ERR_ODE_SINGULAR, self.expr(e.sing_t), f64(e.sing_text)) if hasattr(e, "sing_t") else \
            (ERR_SINGULAR,)
        with b.if_then(bad, likely=False):
            self.fail(*sing)
        self.line = saved
        return self.pack(out)

    def eigen_op(self, e, args, ops, a, n):
        """eigenvalues / eigenvectors (D38): fermium.linalg's Jacobi rotations, as straight-line code."""
        b = self.b
        mats = [a] + [self.unpack(x, n * n) for x in args[1:]]

        def check(values, bad_if, kind):
            bad = None
            for v in values:
                z = b.fcmp_ordered(bad_if, v, f64(0))
                bad = z if bad is None else b.or_(bad, z)
            saved = getattr(self, "line", 0)
            self.line = e.line or saved
            with b.if_then(bad, likely=False):
                self.fail(kind)
            self.line = saved
        for mat in mats:
            check(linalg.asymmetry(ops, mat, n), "<", ERR_NOT_SYMMETRIC)
        if len(mats) == 1:
            vals, vecs = linalg.jacobi_eigen(ops, a, n)
        else:
            vals, vecs, piv = linalg.generalized_eigen(ops, a, mats[1], n)
            check(piv, "<=", ERR_NOT_POSDEF)
        return self.pack(vals if e.name == "eigenvalues" else vecs)

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
        if isinstance(e.ty, (VecTy, MatTy)):
            n = e.ty.n
            if not isinstance(e.a.ty, (VecTy, MatTy)):
                a = self.splat(a, n)
            if not isinstance(e.b.ty, (VecTy, MatTy)):
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
        lo, hi = self.expr(e.lo), self.expr(e.hi)
        self.b.store(i64(getattr(e, "xfmt", -1)), self.mg.errfmt)
        self.mark_line()
        q = self.mg.kernel("fm_quad")
        # the variable's name for the kernel's NaN message; restored after, for an enclosing integral (D45)
        saved = self.b.load(self.mg.qvar)
        self.b.store(f64(getattr(e, "xname", -1)), self.mg.qvar)
        # atol: 0, or -1 for the quiet first try of a vector integral's component, or an expression for its
        # second try (D44)
        atol = f64(-1) if getattr(e, "soft", False) else \
            self.expr(e.atol) if getattr(e, "atol", None) is not None else f64(0)
        r = self.b.call(q, [fn, env, lo, hi, f64(1e-10), atol])
        self.b.store(saved, self.mg.qvar)
        return r

    def e_ISum(self, e):
        """Σ(term for k from lo to hi step st): the count of a for loop (s_SFor), the terms added in order."""
        b, fn = self.b, self.fn
        f = self.mg.lambda_for(e.lam)
        env = self.make_env(e.lam)
        lo, hi, st = self.expr(e.lo), self.expr(e.hi), self.expr(e.step)
        with b.if_then(b.fcmp_unordered("==", st, f64(0))):
            self.fail(ERR_STEP, st, f64(0))
        span = b.fdiv(b.fsub(hi, lo), st)
        cnt = b.fadd(b.call(self.mg.intrinsic("floor"), [b.fadd(span, f64(1e-9))]), f64(1))
        cnt = b.select(b.fcmp_ordered("<", cnt, f64(0)), f64(0), cnt)
        with b.if_then(b.fcmp_unordered("uno", cnt, cnt), likely=False):
            self.fail(ERR_RANGE, lo, hi)
        cnt = b.select(b.fcmp_ordered(">", cnt, f64(2.0 ** 62)), f64(2.0 ** 62), cnt)
        n = b.fptosi(cnt, I64)
        iv, acc = self.alloca(I64), self.alloca(F64)
        b.store(i64(0), iv)
        b.store(f64(0), acc)
        cond, body, end = (fn.append_basic_block(nm) for nm in ("s.c", "s.b", "s.e"))
        b.branch(cond)
        b.position_at_end(cond)
        b.cbranch(b.icmp_signed("<", b.load(iv), n), body, end)
        b.position_at_end(body)
        k = b.fadd(lo, b.fmul(b.sitofp(b.load(iv), F64), st))
        b.store(b.fadd(b.load(acc), b.call(f, [k, env])), acc)
        b.store(b.add(b.load(iv), i64(1)), iv)
        b.branch(cond)
        b.position_at_end(end)
        return b.load(acc)

    def e_IRoot(self, e):
        fn = self.mg.lambda_for(e.lam)
        env = self.make_env(e.lam)
        lo, hi = self.expr(e.lo), self.expr(e.hi)
        scale = self.mg.lambda_for(e.scale) if getattr(e, "scale", None) is not None else \
            ir.Constant(SCALAR_FN.as_pointer(), None)
        self.b.store(i64(getattr(e, "tfmt", -1)), self.mg.errfmt)
        self.mark_line()
        return self.b.call(self.mg.kernel("fm_root"), [fn, env, lo, hi, scale])

    def e_ISolEval(self, e):
        self.b.store(i64(getattr(e, "tfmt", -1)), self.mg.errfmt)
        self.mark_line()
        k = self.mg.kernel("fm_sol_eval")
        sol = self.expr(e.sol)
        dy = i64(1 if e.use_dy else 0)
        if isinstance(e.ty, ListTy):         # u(ts): the value at each time in a list (#62)
            return self.map_list(self.expr(e.t), lambda t, i: self.b.call(k, [sol, i64(e.comp), t, dy]))
        return self.b.call(k, [sol, i64(e.comp), self.expr(e.t), dy])

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
        h = self.b.call(self.mg.externs["fm_load"], [i64(e.load_id)])
        with self.b.if_then(self.b.icmp_signed("==", h, i64(0)), likely=False):
            self.fail(ERR_PENDING)          # a bad file: the loader set the message (A24)
        return h

    def e_IColumn(self, e):
        b = self.b
        h = self.expr(e.data)
        pp = self.alloca(F64P)
        n = b.call(self.mg.externs["fm_column"], [h, i64(e.col), pp])
        # copy out of NumPy's buffer: push may realloc a list's data, which must be our own (A50)
        src = b.load(pp)
        out, data = self.new_list(n)
        with self.lp.range(i64(0), n) as i:
            b.store(b.load(b.gep(src, [i])), b.gep(data, [i]))
        return out

    # ------------------------------------------------------------ builtins
    MATH_INTRINSICS = {"sin": "sin", "cos": "cos", "exp": "exp", "ln": "log", "log": "log", "log10": "log10",
                       "log2": "log2", "abs": "fabs", "floor": "floor", "ceil": "ceil", "round": "round",
                       "tan": "tan", "asin": "asin", "acos": "acos", "atan": "atan", "sinh": "sinh",
                       "cosh": "cosh", "tanh": "tanh"}
    MATH_LIBM = {"asinh": "asinh", "acosh": "acosh", "atanh": "atanh", "erf": "erf", "erfc": "erfc",
                 "gamma": "tgamma", "lgamma": "lgamma", "expm1": "expm1", "log1p": "log1p"}
    NEW_INTRINSICS = {"tan", "asin", "acos", "atan", "sinh", "cosh", "tanh"}
    RECIPROCAL_TRIG = {"cot": "tan", "sec": "cos", "csc": "sin"}

    def math1(self, name, x):
        b = self.b
        if name in self.MATH_INTRINSICS and name not in self.NEW_INTRINSICS:
            return b.call(self.mg.intrinsic(self.MATH_INTRINSICS[name]), [x])
        if name in self.NEW_INTRINSICS:
            return b.call(self.mg.libm(name), [x])
        if name in self.MATH_LIBM:
            return b.call(self.mg.libm(self.MATH_LIBM[name]), [x])
        if name in self.RECIPROCAL_TRIG:      # cot = 1/tan, sec = 1/cos, csc = 1/sin (#63)
            return b.fdiv(f64(1), self.math1(self.RECIPROCAL_TRIG[name], x))
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
        if name in codegen_m3.M3_BUILTINS:
            return codegen_m3.builtin(self, name, e, args)
        if name.startswith("c."):                       # complex numbers (D90): fermium/cplx.py
            from . import cplx
            return cplx.ll_builtin(self, e, args)
        if name in ("shuffle", "matmul", "det", "inverse", "solve_linear", "eigenvalues", "eigenvectors"):
            return self.matrix_op(e, args)
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
        if name in self.MATH_INTRINSICS or name in self.MATH_LIBM or name in self.RECIPROCAL_TRIG or name == "sign":
            if isinstance(e.args[0].ty, ListTy):
                return self.map_list(args[0], lambda x, i: self.math1(name, x))
            return self.math1(name, args[0])
        if name in ("besselj", "bessely"):
            return special.ll_jn_yn(self, "jn" if name == "besselj" else "yn", *args)
        if name in ("besseli", "besselk"):
            return b.call(self.mg.kernel("fm_" + name), args)
        if name in ("ellipk", "ellipe"):
            return special.ll_ellip(self, name, args[0])
        if name == "isnan":
            return b.fcmp_unordered("uno", args[0], args[0])
        if name == "atan2":
            return b.call(self.mg.libm("atan2", 2), args)
        if name == "hypot":
            return b.call(self.mg.libm("hypot", 2), args)
        if name == "mod":
            a, c = args
            return b.fsub(a, b.fmul(c, b.call(self.mg.intrinsic("floor"), [b.fdiv(a, c)])))
        if name in ("min_ew", "max_ew"):     # max(xs, 1e-12): element by element (D162); mirrors interp
            fn = self.mg.intrinsic("minnum" if name == "min_ew" else "maxnum")
            lists = [i for i, a in enumerate(e.args) if isinstance(a.ty, ListTy)]
            first = args[lists[0]]
            n0 = self.llen(first)
            for i in lists[1:]:
                ni = self.llen(args[i])
                with b.if_then(b.icmp_signed("!=", n0, ni)):
                    self.fail(ERR_LEN, b.sitofp(n0, F64), b.sitofp(ni, F64))
            datas = {i: self.ldata(args[i]) for i in lists}

            def elem(x, k):
                vals = [b.load(b.gep(datas[i], [k])) if i in datas else a for i, a in enumerate(args)]
                r = vals[0]
                for v in vals[1:]:
                    r = b.call(fn, [r, v])
                return r
            return self.map_list(first, elem)
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
            den = b.sitofp(b.sub(n, i64(1)), F64)
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
        if name == "slice":        # xs[a:b], both ends included; xs[a:a-1] is empty (D114)
            lst, lo, hi = args
            cnt = self.alloca(I64)
            srcp = self.alloca(F64P)
            with b.if_else(b.fcmp_ordered("==", hi, b.fsub(lo, f64(1)))) as (empty, other):
                with empty:
                    b.store(i64(0), cnt)
                    b.store(self.ldata(lst), srcp)
                with other:
                    with b.if_then(b.fcmp_ordered("<", hi, b.fsub(lo, f64(1))), likely=False):
                        self.fail(ERR_ASSERT, f64(e.msg_id))
                    p = self.elem_ptr(lst, lo)
                    self.elem_ptr(lst, hi)
                    b.store(b.add(b.sub(b.fptosi(hi, I64), b.fptosi(lo, I64)), i64(1)), cnt)
                    b.store(p, srcp)
            n = b.load(cnt)
            src = b.load(srcp)
            out, data = self.new_list(n)
            with self.lp.range(i64(0), n) as i:
                b.store(b.load(b.gep(src, [i])), b.gep(data, [i]))
            return out
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
        if name == "std":        # the N − 1 sample std of one value is 0/0 (red team round 2 #4)
            with b.if_then(b.icmp_signed("<", n, i64(2))):
                self.fail(ERR_STD_ONE, f64(0), f64(0))
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
        den = b.sitofp(b.sub(n, i64(1)), F64)
        return b.call(self.mg.intrinsic("sqrt"), [b.fdiv(b.load(acc), den)])


class LambdaGen(FuncGen):
    def __init__(self, mg, fn, lam: I.ILambda):
        super().__init__(mg, fn, lam)
        self.lam = lam

    def load_env(self, env):
        b = self.b
        i = 0
        for sym in self.lam.captures:
            if isinstance(sym.ty, (VecTy, MatTy)):
                v = ir.Constant(lltype(sym.ty), ir.Undefined)
                for k in range(sym.ty.n):
                    v = b.insert_element(v, b.load(b.gep(env, [i64(i + k)])), I32(k))
                i += sym.ty.n
            else:
                v = b.load(b.gep(env, [i64(i)]))
                i += 1
            if isinstance(sym.ty, BoolTy):
                v = b.fcmp_ordered("!=", v, f64(0))
            if isinstance(sym.ty, SolTy):
                v = b.inttoptr(b.bitcast(v, I64), SOLP)
            p = self.alloca(lltype(sym.ty), sym.name)
            b.store(v, p)
            self.slots[sym.id] = p

    def save_errctx(self):
        return self.b.load(self.mg.curline), self.b.load(self.mg.errfmt)

    def restore_errctx(self, saved):
        self.b.store(saved[0], self.mg.curline)
        self.b.store(saved[1], self.mg.errfmt)

    def emit(self):
        lam, b, fn = self.lam, self.b, self.fn
        if lam.kind == "scalar":
            x, env = fn.args
            saved = self.save_errctx()
            self.load_env(env)
            self.store(lam.params[0], x)
            v = self.expr(lam.body)
            self.restore_errctx(saved)
            b.ret(v)
        elif lam.kind == "ode":
            t, y, dy, env = fn.args
            saved = self.save_errctx()
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
            self.restore_errctx(saved)
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


from . import codegen_m3  # noqa: E402  (M3 numerics: random numbers, FFT, PDEs; D80–D84)
codegen_m3.attach(ModuleGen)
codegen_m3.attach_funcgen(FuncGen)
