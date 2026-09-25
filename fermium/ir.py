"""Typed intermediate representation.

The checker turns the AST into this IR.  Every expression node has `.ty` (a Ty),
plus display-only info: `.sf` (significant figures, None = exact) and `.hint`
(a Unit the user wrote, used when printing).  Units are gone at this point:
all numbers are in SI base units, so the code generator never sees a unit.
"""
from __future__ import annotations

from itertools import count

_sym_ids = count(1)


class Sym:
    """A variable.  storage: 'local' (in its function), 'global' (module-level), 'arena' (REPL)."""

    def __init__(self, name, ty, storage="local", func=None):
        self.name = name
        self.ty = ty
        self.storage = storage
        self.func = func           # IFunc that owns a local
        self.id = next(_sym_ids)
        self.sf = None
        self.hint = None
        self.direct = False        # value came straight from a literal (for sig-fig display)
        self.slot = None           # arena slot (REPL)
        self.assigned = False

    def __repr__(self):
        return f"Sym({self.name}#{self.id}:{self.ty})"


class Expr:
    ty = None
    sf = None
    hint = None
    direct = False
    line = 0


class IConst(Expr):
    def __init__(self, value, ty):
        self.value = float(value)
        self.ty = ty


class IBool(Expr):
    def __init__(self, value, ty):
        self.value = bool(value)
        self.ty = ty


class IStr(Expr):
    def __init__(self, value, ty):
        self.value = value
        self.ty = ty


class IVar(Expr):
    def __init__(self, sym):
        self.sym = sym
        self.ty = sym.ty


class IBin(Expr):
    """+ - * / on numbers, or element-wise on lists (broadcasting a scalar)."""

    def __init__(self, op, a, b, ty):
        self.op, self.a, self.b, self.ty = op, a, b, ty


class IPowC(Expr):
    """a ** p with p a compile-time constant."""

    def __init__(self, a, p, ty):
        self.a, self.p, self.ty = a, float(p), ty


class IPow(Expr):
    def __init__(self, a, b, ty):
        self.a, self.b, self.ty = a, b, ty


class INeg(Expr):
    def __init__(self, a):
        self.a, self.ty = a, a.ty


class ICmp(Expr):
    def __init__(self, op, a, b, ty):
        self.op, self.a, self.b, self.ty = op, a, b, ty


class ILogic(Expr):
    def __init__(self, op, a, b, ty):
        self.op, self.a, self.b, self.ty = op, a, b, ty


class INot(Expr):
    def __init__(self, a, ty):
        self.a, self.ty = a, ty


class ICall(Expr):
    def __init__(self, func, args, ty):
        self.func, self.args, self.ty = func, args, ty


class IMap(Expr):
    """Apply a scalar user function element-wise over one list argument."""

    def __init__(self, func, args, list_pos, ty):
        self.func, self.args, self.list_pos, self.ty = func, args, list_pos, ty


class IBuiltin(Expr):
    def __init__(self, name, args, ty):
        self.name, self.args, self.ty = name, args, ty


class IList(Expr):
    def __init__(self, items, ty):
        self.items, self.ty = items, ty


class IVec(Expr):
    def __init__(self, items, ty):
        self.items, self.ty = items, ty


class IVecElem(Expr):
    def __init__(self, v, k, ty):
        self.v, self.k, self.ty = v, k, ty


class IVecIndex(Expr):
    """Entries of a vector or matrix picked by indexes known only at run time (v[i], M[i, j], M[i]).

    idxs: [(index expr, size, stride)]: the flat 0-based base is Σ (index − 1)·stride, each index
    checked to be a whole number from 1 to size; offs: the flat offsets from the base that make the
    result (one: a number; several: a vector, like a row or a column)."""

    def __init__(self, v, idxs, offs, ty, line=0):
        self.v, self.idxs, self.offs, self.ty, self.line = v, idxs, offs, ty, line


class IIndex(Expr):
    def __init__(self, lst, idx, ty, line=0):
        self.lst, self.idx, self.ty, self.line = lst, idx, ty, line


class IIf(Expr):
    def __init__(self, cond, a, b, ty):
        self.cond, self.a, self.b, self.ty = cond, a, b, ty


class ILet(Expr):
    """`value where name = e, ...` -- assign locals, then evaluate."""

    def __init__(self, binds, value):
        self.binds, self.value = binds, value
        self.ty = value.ty


class IIntegral(Expr):
    def __init__(self, lam, lo, hi, ty):
        self.lam, self.lo, self.hi, self.ty = lam, lo, hi, ty


class ISum(Expr):
    """Σ(body for k from lo to hi step st): lam(k) summed over the range, as a for loop counts it (D51)."""

    def __init__(self, lam, lo, hi, step, ty):
        self.lam, self.lo, self.hi, self.step, self.ty = lam, lo, hi, step, ty


class IRoot(Expr):
    """The x in [lo, hi] where lam(x) = 0 (solve lhs = rhs for x from lo to hi)."""

    def __init__(self, lam, lo, hi, ty):
        self.lam, self.lo, self.hi, self.ty = lam, lo, hi, ty


class ISolEval(Expr):
    """Value of solution component `comp` at time t (use_dy: derivative array)."""

    def __init__(self, sol, comp, t, use_dy, ty):
        self.sol, self.comp, self.t, self.use_dy, self.ty = sol, comp, t, use_dy, ty


class ISolList(Expr):
    """All samples of a solution component (or its times) as a list."""

    def __init__(self, sol, comp, what, ty):
        self.sol, self.comp, self.what, self.ty = sol, comp, what, ty


class ILoad(Expr):
    def __init__(self, load_id, ty):
        self.load_id, self.ty = load_id, ty


class IColumn(Expr):
    def __init__(self, data, col, ty):
        self.data, self.col, self.ty = data, col, ty


# ---------------------------------------------------------------- statements
class Stmt:
    line = 0


class SAssign(Stmt):
    def __init__(self, sym, value):
        self.sym, self.value = sym, value


class SIndexAssign(Stmt):
    def __init__(self, sym, idx, value, line=0):
        self.sym, self.idx, self.value, self.line = sym, idx, value, line


class SPush(Stmt):
    def __init__(self, sym, value):
        self.sym, self.value = sym, value


class SIf(Stmt):
    def __init__(self, cond, then, other):
        self.cond, self.then, self.other = cond, then, other


class SWhile(Stmt):
    def __init__(self, cond, body):
        self.cond, self.body = cond, body


class SFor(Stmt):
    """for sym from lo to hi step st (inclusive, computed as lo + i*st)."""

    def __init__(self, sym, lo, hi, step, body):
        self.sym, self.lo, self.hi, self.step, self.body = sym, lo, hi, step, body


PAR_BLOCKS = 256     # a parallel for's range is cut into at most this many blocks (D152)
ERR_PAR_ALIAS = 40   # a list written as xs[i] in a parallel for is also used there under another name


def par_blocks(n):
    """The blocks [start, end) of a parallel for with n iterations: the same for any number of threads, so
    the sums (reductions) are added up in the same order on every machine and in the interpreter."""
    nb = min(n, PAR_BLOCKS)
    if nb == 0:
        return []
    q, r = divmod(n, nb)
    start = [k * q + min(k, r) for k in range(nb + 1)]
    return [(start[k], start[k + 1]) for k in range(nb)]


class SForIn(Stmt):
    def __init__(self, sym, lst, body):
        self.sym, self.lst, self.body = sym, lst, body


class SPrint(Stmt):
    """items: list of (kind, payload, fmt_id). kind: 'num' | 'list' | 'bool' | 'text' | 'sol'."""

    def __init__(self, items):
        self.items = items


class SPlot(Stmt):
    """series: list of dicts describing arrays to send; plot_id indexes Python-side labels."""

    def __init__(self, plot_id, series):
        self.plot_id, self.series = plot_id, series


class SSolve(Stmt):
    def __init__(self, sol_sym, rhs, y0, t0, t1, step, method, rtol, line=0):
        self.sol_sym, self.rhs, self.y0, self.t0, self.t1 = sol_sym, rhs, y0, t0, t1
        self.step, self.method, self.rtol, self.line = step, method, rtol, line


class SFit(Stmt):
    def __init__(self, fit_id, data, param_syms, guesses, model):
        self.fit_id, self.data, self.param_syms, self.guesses, self.model = fit_id, data, param_syms, guesses, model


class SReturn(Stmt):
    def __init__(self, value):
        self.value = value


class SBreak(Stmt):
    pass


class SContinue(Stmt):
    pass


class SExpr(Stmt):
    def __init__(self, value):
        self.value = value


class SAssert(Stmt):
    def __init__(self, cond, msg_id):
        self.cond, self.msg_id = cond, msg_id


# ---------------------------------------------------------------- functions
class IFunc:
    """A monomorphic instance of a user function."""

    def __init__(self, name, params, ret_ty=None):
        self.name = name              # unique (mangled) name
        self.params = params          # list[Sym]
        self.ret_ty = ret_ty
        self.body = []                # list[Stmt]
        self.locals = []              # list[Sym]
        self.lambdas = []
        self.sf = None

    def __repr__(self):
        return f"IFunc({self.name})"


def referenced_syms(node):
    """The variables an expression (or a list of them) reads, in order, without looking inside nested
    lambdas (integrands and the like have their own captures)."""
    out, seen = [], set()

    def visit(x):
        if isinstance(x, IVar):
            if x.sym.id not in seen:
                seen.add(x.sym.id)
                out.append(x.sym)
        elif isinstance(x, (list, tuple)):
            for y in x:
                visit(y)
        elif isinstance(x, Expr):
            for v in vars(x).values():
                visit(v)
    visit(node)
    return out


class ILambda:
    """A nested function: integrand f(x), ODE right-hand side f(t, y), fit model, plot sampler.

    kind: 'scalar'  double f(double x, double* env)           body: Expr
          'ode'     void f(double t, double* y, double* dy, double* env)  body: list of Expr (dy)
          'model'   void f(double* p, double** cols, i64 n, double* out)  body: Expr
    captures: locals of the enclosing function passed through env.
    """

    def __init__(self, kind, name):
        self.kind = kind
        self.name = name
        self.params = []
        self.captures = []
        self.body = None
        self.state = []       # for 'ode': syms bound to y[i]
        self.col_syms = []    # for 'model'
        self.param_syms = []  # for 'model'


class IPdeEval(Expr):
    """u(x, t) of a PDE solution (D83): cubic interpolation in x between grid points (xa + k (xb - xa)/m),
    Hermite in t between snapshots; comp0: the solution component of grid point 0 (the imaginary part of a
    complex solution starts at m + 1).  which: 0 = u, 1 = ∂u/∂x, 2 = ∂u/∂t."""

    def __init__(self, sol, xa, xb, m, comp0, x, t, which, ty):
        self.sol, self.xa, self.xb, self.m, self.comp0 = sol, xa, xb, m, comp0
        self.x, self.t, self.which, self.ty = x, t, which, ty


class SAnimate(Stmt):
    """plot u vs x animate over t (D83): the runtime draws the frames from the PDE solution."""

    def __init__(self, anim_id, sol, xa, xb):
        self.anim_id, self.sol, self.xa, self.xb = anim_id, sol, xa, xb
