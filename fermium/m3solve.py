"""Checking the M3 forms of `solve` (DECISIONS D82, D83):

- eigenvalue problems:  solve -ħ²/(2m) * ψ'' + V(x) ψ = E ψ  with ψ(a) = 0, ψ(b) = 0  for x from a to b  lowest N
- 1-D PDEs (heat, wave, Schrödinger):  solve ∂u/∂t = D ∂²u/∂x²  with u(x, 0) = …, u(0, t) = 0, u(L, t) = 0
  for x from 0 to L, t from 0 s to T
"""
from __future__ import annotations

from fractions import Fraction

from . import ast as A
from . import calculus as C
from . import ir as I
from .checker import SolView, Scope, Ctx, BUILTINS
from .errors import FermiumError
from .types import DExpr, NumTy, ListTy, SolTy
from .units import DIMLESS


def _whole(ck, node, what, lo, hi, default):
    if node is None:
        return default
    v = ck.expr(node, ck.cur_ctx)
    if not isinstance(v, I.IConst) or not isinstance(v.ty, NumTy) or v.value != int(v.value) or \
            not (lo <= v.value <= hi) or not ck.U.unify(v.ty.dim, DIMLESS):
        raise ck.err(f"{what} must be a whole number from {lo} to {hi}", node)
    return int(v.value)


def _python_only(ck, line, what):
    """fermium build refuses these: they run in Python (NumPy/SciPy), which executables don't carry."""
    if not hasattr(ck.tables, "python_only"):
        ck.tables.python_only = []
    ck.tables.python_only.append((line, what))


def _is_zero(v):
    return isinstance(v, I.IConst) and isinstance(v.ty, NumTy) and v.value == 0


# ============================================================ eigenvalue problems (D82)
def check_eigen(ck, s: A.Solve, ctx):
    from .solve import _find_derivs, _normalize_derivs
    x = s.var
    if s.step is not None or s.tolerance is not None or getattr(s, "until", None) is not None:
        raise ck.err("step, tolerance and until are for initial-value problems; an eigenvalue problem (lowest N) "
                     "takes  grid N  and  using matrix / using shooting", s)
    if len(s.equations) != 1:
        raise ck.err("an eigenvalue problem (lowest N) has one equation, like  -ħ²/(2m) * ψ'' + V(x) ψ = E ψ",
                     s)
    q0 = s.equations[0]
    q = A.Equation(_normalize_derivs(q0.lhs, x), _normalize_derivs(q0.rhs, x)).at(q0)
    orders = {}
    _find_derivs(q.lhs, orders)
    _find_derivs(q.rhs, orders)
    if len(orders) != 1:
        raise ck.err("an eigenvalue problem needs one unknown function with a second derivative, like ψ''", q0)
    psi, order = next(iter(orders.items()))
    if order != 2:
        raise ck.err(f"an eigenvalue problem needs the second derivative {psi}'' (this equation has "
                     f"{psi}{chr(39) * order})", q0)
    method = (s.method or "matrix").lower()
    if method not in ("matrix", "shooting"):
        raise ck.err(f"unknown method '{s.method}' for an eigenvalue problem (use matrix or shooting)", s)
    nstates = _whole(ck, s.lowest, "the number of states after lowest", 1, 500, 1)
    grid = _whole(ck, getattr(s, "grid", None), "the grid (number of intervals)", 16, 200000, 2000)

    a = ck.expr(s.lo, ctx)
    b = ck.expr(s.hi, ctx)
    ck.need_num(a, s.lo, "the start of the range")
    ck.need_num(b, s.hi, "the end of the range")
    ck.unify_or(a.ty.dim, b.ty.dim, lambda: f"the range goes from {ck.desc(a.ty.dim)} to {ck.desc(b.ty.dim)}",
                s.lo)
    xdim = a.ty.dim
    psidim = DExpr.of(xdim) ** Fraction(-1, 2)        # normalised: ∫ψ² dx = 1

    # the boundary conditions: ψ = 0 at both ends
    ends = []
    for ic in s.initial:
        lhs = ic.lhs
        ok = isinstance(lhs, A.Call) and len(lhs.args) == 1 and isinstance(lhs.func, A.Name) and \
            lhs.func.name == psi
        if not ok:
            raise ck.err(f"the boundary conditions of an eigenvalue problem look like  {psi}(a) = 0, {psi}(b) = 0 "
                         f"(the ends of the range)", ic)
        v = ck.expr(ic.rhs, ctx)
        if not _is_zero(v):
            raise ck.err(f"only {psi} = 0 at the ends is supported for now (a wall, or far enough out that "
                         f"{psi} has died away)", ic.rhs)
        at = ck.expr(lhs.args[0], ctx)
        ck.need_num(at, lhs.args[0], "the position of the boundary condition")
        ck.unify_or(at.ty.dim, xdim, lambda: f"the boundary condition is at {ck.desc(at.ty.dim)} but {x} is "
                    f"{ck.desc(xdim)}", lhs.args[0])
        if isinstance(at, I.IConst) and isinstance(a, I.IConst) and isinstance(b, I.IConst):
            tol = 1e-12 * (abs(a.value) + abs(b.value) + 1e-300)
            if abs(at.value - a.value) > tol and abs(at.value - b.value) > tol:
                raise ck.err(f"the boundary conditions must be at the ends of the range ({x} = start and "
                             f"{x} = end)", lhs.args[0])
        ends.append(ic)
    if len(ends) != 2:
        raise ck.err(f"an eigenvalue problem needs {psi} at both ends:  with {psi}({C.to_source(s.lo)}) = 0, "
                     f"{psi}({C.to_source(s.hi)}) = 0", s)

    # the eigenvalue: the one name in the equation that isn't defined yet
    called = set()
    for n in A.walk(q.lhs):
        if isinstance(n, A.Call) and isinstance(n.func, A.Name):
            called.add(n.func.name)
    for n in A.walk(q.rhs):
        if isinstance(n, A.Call) and isinstance(n.func, A.Name):
            called.add(n.func.name)
    unknown = []
    for n in A.free_names(q.lhs) + A.free_names(q.rhs):
        if n in (psi, x) or n in unknown or n in called or n in BUILTINS:
            continue
        if ctx.scope.lookup(n)[0] is None:
            unknown.append(n)
    if not unknown:
        raise ck.err(f"an eigenvalue problem needs an unknown constant, like E in  … = E {psi}; every name here "
                     f"already has a value (use a new name for the eigenvalue)", q0)
    if len(unknown) > 1:
        raise ck.err(f"this equation has {len(unknown)} undefined names ({', '.join(unknown)}); an eigenvalue "
                     f"problem has exactly one unknown constant (the eigenvalue)", q0)
    ename = unknown[0]
    edim = DExpr.fresh(ename)

    # the right-hand side f(x, [ψ, ψ', E]) -> [ψ', ψ'', 0]
    lam = I.ILambda("ode", ck.fresh_name("eigen"))
    lam.locals = []
    scope = Scope(ctx.scope)
    lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
    xsym = I.Sym(x, NumTy(xdim), "local", lam)
    xsym.assigned = True
    lam.params = [xsym]
    scope.names[x] = xsym
    s_psi = I.Sym(psi, NumTy(psidim), "local", lam)
    s_dpsi = I.Sym(psi + "'", NumTy(psidim / DExpr.of(xdim)), "local", lam)
    s_e = I.Sym(ename, NumTy(edim), "local", lam)
    for sym in (s_psi, s_dpsi, s_e):
        sym.assigned = True
        lam.state.append(sym)
        scope.names[sym.name] = sym
    top = I.Sym(psi + "''", NumTy(psidim / (DExpr.of(xdim) ** 2)), "local", lam)
    lam.locals.append(top)
    scope.names[psi + "''"] = top
    lv = ck.expr(q.lhs, lctx)
    rv = ck.expr(q.rhs, lctx)
    ck.need_num(lv, q.lhs, "the left side")
    ck.need_num(rv, q.rhs, "the right side")
    if not ck.U.unify(lv.ty.dim, rv.ty.dim):
        raise ck.err(f"the two sides of this equation don't match: left is {ck.desc(lv.ty.dim)}, right is "
                     f"{ck.desc(rv.ty.dim)}", q0)
    del scope.names[psi + "''"]
    iso = C.isolate(q.lhs, q.rhs, A.Prime(A.Name(psi), 2))
    v = ck.expr(iso, lctx)
    ck.need_num(v, q0, f"{psi}''")
    if not ck.U.unify(v.ty.dim, top.ty.dim):
        raise ck.err(f"{psi}'' works out to {ck.desc(v.ty.dim)} but should be {ck.desc(top.ty.dim)}", q0)
    lam.body = [ck.var_ref(s_dpsi, lctx, s), v, I.IConst(0.0, NumTy(edim / DExpr.of(xdim)))]
    ck.new_lambdas.append(lam)
    ck.all_lambdas.append(lam)

    # the result: ψ_1 … ψ_N as solution views (with ψ_k' and ψ_k''), and E as a list
    names = [f"{psi}_{k}" for k in range(1, nstates + 1)]
    info = {"names": names, "layout": [(nm, j) for nm in names for j in range(2)], "t": x,
            "slots": 3 * nstates, "eigen": True}
    sol_sym = ck.new_sym(ck.fresh_name("__eig"), SolTy(info), ctx)
    sol_sym.assigned = True
    thint = a.hint or b.hint
    for k, nm in enumerate(names):
        pretty = psi + str(k + 1).translate(str.maketrans("0123456789", "₀₁₂₃₄₅₆₇₈₉"))
        view = SolView(sol_sym, 2 * k, 2 * k + 1, psidim, xdim, x, pretty)
        view.hint = None
        view.hints = {}
        view.thint = thint
        view.sf = None
        ctx.scope.names[nm] = view
    st = I.SSolve(sol_sym, lam, [], a, b, None, "eigen", 0.0, s.line)
    st.nstates, st.grid, st.eig_method = nstates, grid, (1 if method == "shooting" else 0)
    st.event = None
    st.evtext = -1
    st.tname = ck.text(x)
    tf = I.IConst(0, NumTy(xdim))
    tf.hint = thint
    st.tfmt = ck.fmt(tf)
    _python_only(ck, s.line, "an eigenvalue problem (solve … lowest N)")
    # E = [E₁, …, E_N]: stored as constant columns 2N … 3N-1 of the solution, read at x = a
    at = ck.expr(s.lo, ctx)
    items = []
    for k in range(nstates):
        e = I.ISolEval(ck.var_ref(sol_sym, ctx, s), 2 * nstates + k, at, False, NumTy(edim))
        e.tfmt = st.tfmt
        items.append(e)
    lst = I.IList(items, ListTy(edim))
    lst.sf = None
    lst.hint = None
    return [st, ck.assign_to(ename, lst, s, ctx)]


# ============================================================ 1-D PDEs (D83)
class PdeView:
    """The name of a PDE's unknown after the solve: u(x, t), ∂u/∂x(x, t), ∂u/∂t(x, t), plot u vs x animate."""
    is_pde = True

    def __init__(self, name, sol_sym, xa_sym, xb_sym, m, ncomp, udim, xdim, tdim, xname, tname):
        self.name, self.sol_sym, self.xa_sym, self.xb_sym = name, sol_sym, xa_sym, xb_sym
        self.m, self.ncomp, self.udim, self.xdim, self.tdim = m, ncomp, udim, xdim, tdim
        self.xname, self.tname = xname, tname
        self.uhint = self.xhint = self.thint = None


PDE_METHODS = {"crank_nicolson": 0, "cn": 0, "cranknicolson": 0, "implicit": 1, "explicit": 2}


def _pde_rewrite(e, u, xv, tv, found):
    """∂u/∂x → u__x1, ∂²u/∂x² → u__x2, ∂u/∂t → u__t1, ∂²u/∂t² → u__t2 (names the probe binds)."""
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Name) and e.operand.name == u:
        if e.var not in (xv, tv):
            raise FermiumError(f"∂{u}/∂{e.var}: {u} is a function of {xv} and {tv}", e.line, e.col)
        if e.order > 2:
            raise FermiumError(f"derivatives of order {e.order} aren't supported in a PDE (up to second order)",
                               e.line, e.col)
        key = f"{u}__{'x' if e.var == xv else 't'}{e.order}"
        found[key] = True
        return A.Name(key).at(e)
    if isinstance(e, A.Deriv) and u in A.free_names(e.operand):
        raise FermiumError(f"only derivatives of {u} itself are supported (like ∂²{u}/∂{xv}²); expand "
                           f"∂/∂{e.var} (…) by hand", e.line, e.col)
    return C.map_children(e, lambda c: _pde_rewrite(c, u, xv, tv, found))


def _split_phase(e):
    """A(x) · exp(i φ(x)) → (A, φ): finds one factor exp(… i …) in a product / quotient; (e, None) if none."""
    if isinstance(e, A.Call) and isinstance(e.func, A.Name) and e.func.name == "exp" and len(e.args) == 1 and \
            "i" in A.free_names(e.args[0]):
        return A.Num(1.0).at(e), C.subst(e.args[0], {"i": A.Num(1.0).at(e)})
    if isinstance(e, A.BinOp) and e.op == "*":
        a, pa = _split_phase(e.left)
        if pa is not None:
            return A.BinOp("*", a, e.right).at(e), pa
        b, pb = _split_phase(e.right)
        if pb is not None:
            return A.BinOp("*", e.left, b).at(e), pb
    if isinstance(e, A.BinOp) and e.op == "/":
        a, pa = _split_phase(e.left)
        if pa is not None:
            return A.BinOp("/", a, e.right).at(e), pa
    return e, None


def _pde_conditions(ck, s, ctx, u, xv, tv, xa, xb, t0, t1, tdim, xdim):
    """Sort the with-conditions: the initial value, the initial velocity (waves) and the boundary values."""
    ic = v0 = None
    bcs = {}
    for cond in s.initial:
        f = cond.lhs
        deriv = None
        if isinstance(f, A.Call) and isinstance(f.func, A.Deriv) and isinstance(f.func.operand, A.Name) and \
                f.func.operand.name == u and f.func.order == 1:
            deriv = f.func.var
        elif not (isinstance(f, A.Call) and isinstance(f.func, A.Name) and f.func.name == u):
            f = None
        if f is None or len(f.args) != 2:
            raise ck.err(f"the conditions of a PDE look like  {u}({xv}, 0 s) = …  (initial) and  {u}(0 m, {tv}) = …"
                         f"  or  ∂{u}/∂{xv}(0 m, {tv}) = …  (boundaries)", cond)
        ax, at = f.args
        if isinstance(ax, A.Name) and ax.name == xv and not (isinstance(at, A.Name) and at.name == tv):
            if deriv == tv:
                if v0 is not None:
                    raise ck.err(f"∂{u}/∂{tv} at the start is given twice", cond)
                v0 = cond
            elif deriv is None:
                if ic is not None:
                    raise ck.err(f"the initial value of {u} is given twice", cond)
                ic = cond
            else:
                raise ck.err(f"an initial condition gives {u}({xv}, start) (and for a wave ∂{u}/∂{tv}({xv}, "
                             f"start))", cond)
            tv_at = ck.expr(at, ctx)
            ck.need_num(tv_at, at, "the time of the initial condition")
            ck.unify_or(tv_at.ty.dim, tdim, lambda: f"the initial condition is at {ck.desc(tv_at.ty.dim)} but {tv} "
                        f"is {ck.desc(tdim)}", at)
            if isinstance(tv_at, I.IConst) and isinstance(t0, I.IConst):
                scale = abs(t0.value) + (abs(t1.value) if isinstance(t1, I.IConst) else 0.0) + 1e-300
                if abs(tv_at.value - t0.value) > 1e-12 * scale:
                    raise ck.err(f"the initial condition must be at the start of the {tv} range", at)
        elif isinstance(at, A.Name) and at.name == tv:
            if deriv not in (None, xv):
                raise ck.err(f"a boundary condition gives {u} or ∂{u}/∂{xv} at an end, as a function of {tv}", cond)
            pos = ck.expr(ax, ctx)
            ck.need_num(pos, ax, "the position of the boundary condition")
            ck.unify_or(pos.ty.dim, xdim, lambda: f"the boundary condition is at {ck.desc(pos.ty.dim)} but {xv} is "
                        f"{ck.desc(xdim)}", ax)
            src = C.to_source(ax)
            if isinstance(pos, I.IConst) and isinstance(xa, I.IConst) and isinstance(xb, I.IConst):
                tol = 1e-12 * (abs(xa.value) + abs(xb.value) + 1e-300)
                side = "a" if abs(pos.value - xa.value) <= tol else "b" if abs(pos.value - xb.value) <= tol else None
            else:
                side = "a" if src == C.to_source(s.lo) else "b" if src == C.to_source(s.hi) else None
            if side is None:
                raise ck.err(f"boundary conditions are at the ends of the range ({xv} = {C.to_source(s.lo)} or "
                             f"{C.to_source(s.hi)})", ax)
            if side in bcs:
                raise ck.err(f"two boundary conditions at the same end ({xv} = {src})", cond)
            bcs[side] = (0 if deriv is None else 1, cond)
        else:
            raise ck.err(f"write the initial condition as  {u}({xv}, {C.to_source(s.lo2)}) = …  and the boundary "
                         f"conditions as  {u}({C.to_source(s.lo)}, {tv}) = …", cond)
    return ic, v0, bcs


def _pde_imaginary_unit(ck, s: A.Solve, ctx):
    """The imaginary unit of a Schrödinger PDE is 𝑖 (D90; `1i`, Tab \\imag), or a bare `i` when the program has no
    `i` of its own (the older spelling, kept). A bare `i` that is also your variable is an error instead of being
    silently read as √-1 (red team round 3 #10, D184). Returns s with 𝑖 written as the solver's internal `i`."""
    names = []
    for q in list(s.equations) + list(s.initial):
        names += A.free_names(q.lhs) + A.free_names(q.rhs)
    uses_imag, uses_i = "𝑖" in names, "i" in names
    if uses_i:
        b, _ = ctx.scope.lookup("i")
        if b is not None:
            raise ck.err("in this PDE, i is your own variable i, not the imaginary unit", s.equations[0],
                         hint="write the imaginary unit as 𝑖 (Tab \\imag) or 1i, like  𝑖 ħ ∂ψ/∂t = …; or rename "
                              "your variable")
    if not uses_imag:
        return s
    m = {"𝑖": A.Name("i")}
    import copy
    s2 = copy.copy(s)
    s2.equations = [A.Equation(C.subst(q.lhs, m), C.subst(q.rhs, m)).at(q) for q in s.equations]
    s2.initial = [A.Equation(C.subst(q.lhs, m), C.subst(q.rhs, m)).at(q) for q in s.initial]
    return s2


def check_pde(ck, s: A.Solve, ctx):
    s = _pde_imaginary_unit(ck, s, ctx)
    xv, tv = s.var, s.var2
    if s.step is not None:
        raise ck.err(f"in a PDE the time step goes after the time range:  {tv} from … to … step …", s.step)
    if s.tolerance is not None or getattr(s, "until", None) is not None or getattr(s, "lowest", None) is not None:
        raise ck.err("tolerance, until and lowest aren't used in a PDE; it takes  step,  grid N  and  using "
                     "crank_nicolson / implicit / explicit", s)
    if len(s.equations) != 1:
        raise ck.err("a PDE solve has one equation, like  ∂u/∂t = D * ∂²u/∂x²", s)
    q0 = s.equations[0]
    unknowns = []
    for side in (q0.lhs, q0.rhs):
        for n in A.walk(side):
            if isinstance(n, A.Deriv) and isinstance(n.operand, A.Name) and n.var in (xv, tv) and \
                    n.operand.name not in unknowns and ctx.scope.lookup(n.operand.name)[0] is None:
                unknowns.append(n.operand.name)
    if len(unknowns) != 1:
        raise ck.err(f"a PDE needs one unknown function with partial derivatives, like ∂u/∂{tv} and "
                     f"∂²u/∂{xv}²" + (f" (found {', '.join(unknowns)})" if unknowns else ""), q0)
    u = unknowns[0]
    found = {}
    try:
        lhs = _pde_rewrite(q0.lhs, u, xv, tv, found)
        rhs = _pde_rewrite(q0.rhs, u, xv, tv, found)
    except FermiumError as ex:
        raise ck.err(ex.message, q0)
    order = 2 if f"{u}__t2" in found else 1 if f"{u}__t1" in found else 0
    if order == 0:
        raise ck.err(f"a PDE needs a time derivative ∂{u}/∂{tv} (or ∂²{u}/∂{tv}²)", q0)
    method = (s.method or "crank_nicolson").lower()
    if method not in PDE_METHODS:
        raise ck.err(f"unknown method '{s.method}' for a PDE (use crank_nicolson, implicit or explicit)", s)
    if order == 2 and s.method is not None:
        raise ck.err("the wave equation (second order in t) is solved with its own explicit scheme; leave out "
                     "using …", s)
    grid = _whole(ck, getattr(s, "grid", None), "the grid (number of intervals in x)", 4, 100000, 400)
    names_eq = A.free_names(lhs) + A.free_names(rhs)
    is_complex = "i" in names_eq
    if is_complex and order == 2:
        raise ck.err("a complex equation (with i) must be first order in t, like the Schrödinger equation", q0)

    # ranges
    xa = ck.expr(s.lo, ctx)
    xb = ck.expr(s.hi, ctx)
    ck.need_num(xa, s.lo, f"the start of the {xv} range")
    ck.need_num(xb, s.hi, f"the end of the {xv} range")
    ck.unify_or(xa.ty.dim, xb.ty.dim, lambda: f"the {xv} range goes from {ck.desc(xa.ty.dim)} to "
                f"{ck.desc(xb.ty.dim)}", s.lo)
    t0 = ck.expr(s.lo2, ctx)
    t1 = ck.expr(s.hi2, ctx)
    ck.need_num(t0, s.lo2, f"the start of the {tv} range")
    ck.need_num(t1, s.hi2, f"the end of the {tv} range")
    ck.unify_or(t0.ty.dim, t1.ty.dim, lambda: f"the {tv} range goes from {ck.desc(t0.ty.dim)} to "
                f"{ck.desc(t1.ty.dim)}", s.lo2)
    xdim, tdim = DExpr.of(xa.ty.dim), DExpr.of(t0.ty.dim)
    step = None
    if getattr(s, "step2", None) is not None:
        step = ck.expr(s.step2, ctx)
        ck.need_num(step, s.step2, "the time step")
        ck.unify_or(step.ty.dim, tdim, lambda: f"the step is {ck.desc(step.ty.dim)} but {tv} is {ck.desc(tdim)}",
                    s.step2)
    udim = DExpr.fresh(u)

    ic, v0, bcs = _pde_conditions(ck, s, ctx, u, xv, tv, xa, xb, t0, t1, tdim, xdim)
    if ic is None:
        raise ck.err(f"missing the initial condition:  with {u}({xv}, {C.to_source(s.lo2)}) = …", s)
    if order == 2 and v0 is None:
        raise ck.err(f"a wave equation also needs the initial velocity:  ∂{u}/∂{tv}({xv}, {C.to_source(s.lo2)}) = …"
                     f"  (0 for a string released at rest)", s)
    if order == 1 and v0 is not None:
        raise ck.err(f"∂{u}/∂{tv} at the start is only given for a wave equation (second order in {tv})", v0)
    missing = [end for end in ("a", "b") if end not in bcs]
    if missing:
        where = " and ".join(C.to_source(s.lo if e == "a" else s.hi) for e in missing)
        raise ck.err(f"missing a boundary condition at {xv} = {where}: give {u}(…, {tv}) = … (fixed value) or "
                     f"∂{u}/∂{xv}(…, {tv}) = … (flux; 0 for an insulated end)", s)

    # the probe f(x, [u, u_x, u_xx, t, u_t, i]) -> [rhs, u0, phase0, v0, left, right]
    lam = I.ILambda("ode", ck.fresh_name("pde"))
    lam.locals = []
    scope = Scope(ctx.scope)
    lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
    xsym = I.Sym(xv, NumTy(xdim), "local", lam)
    xsym.assigned = True
    lam.params = [xsym]
    scope.names[xv] = xsym
    ut_name = f"{u}__t1" if order == 2 else "__ut_unused"
    state = [(u, udim), (f"{u}__x1", udim / xdim), (f"{u}__x2", udim / xdim ** 2), (tv, tdim),
             (ut_name, udim / tdim), ("i" if is_complex else "__i_unused", DExpr.of(DIMLESS))]
    for nm, d in state:
        sym = I.Sym(nm, NumTy(d), "local", lam)
        sym.assigned = True
        lam.state.append(sym)
        scope.names[nm] = sym
    top_name = f"{u}__t{order}"
    top = I.Sym(top_name, NumTy(udim / tdim ** order), "local", lam)
    lam.locals.append(top)
    scope.names[top_name] = top

    # the initial value fixes u's units
    ic_rhs, phase = ic.rhs, None
    if is_complex:
        ic_rhs, phase = _split_phase(ic.rhs)
    for side in (ic_rhs,) + ((phase,) if phase is not None else ()):
        bad = [n for n in A.free_names(side) if n in (u, "i")]
        if bad:
            raise ck.err(f"the initial value can't use {bad[0]}" + (
                f"; a complex initial value is written  A({xv}) exp(i φ({xv}))" if "i" in bad else ""), ic.rhs)
    uv0 = ck.expr(ic_rhs, lctx)
    ck.need_num(uv0, ic.rhs, "the initial value")
    ck.unify_or(uv0.ty.dim, udim, lambda: "the initial value's units don't fit", ic.rhs)
    pv = I.IConst(0.0, NumTy(DIMLESS))
    if phase is not None:
        pv = ck.expr(phase, lctx)
        ck.need_num(pv, ic.rhs, "the phase in exp(i …)")
        ck.unify_or(pv.ty.dim, DIMLESS, lambda: f"the phase in exp(i …) must be a plain number, not "
                    f"{ck.desc(pv.ty.dim)}", ic.rhs)

    lv = ck.expr(lhs, lctx)
    rv = ck.expr(rhs, lctx)
    ck.need_num(lv, q0.lhs, "the left side")
    ck.need_num(rv, q0.rhs, "the right side")
    if not ck.U.unify(lv.ty.dim, rv.ty.dim):
        raise ck.err(f"the two sides of this equation don't match: left is {ck.desc(lv.ty.dim)}, right is "
                     f"{ck.desc(rv.ty.dim)}", q0)
    del scope.names[top_name]
    sup = "²" if order == 2 else ""
    try:
        iso = C.isolate(lhs, rhs, A.Name(top_name))
    except FermiumError:
        raise ck.err(f"can't solve this equation for ∂{sup}{u}/∂{tv}{sup}: it must appear linearly (like "
                     f"∂u/∂t = D * ∂²u/∂x²)", q0)
    body0 = ck.expr(iso, lctx)
    if not ck.U.unify(body0.ty.dim, top.ty.dim):
        raise ck.err(f"∂{sup}{u}/∂{tv}{sup} works out to {ck.desc(body0.ty.dim)} but should be "
                     f"{ck.desc(top.ty.dim)}", q0)

    vv = I.IConst(0.0, NumTy(udim / tdim))
    if v0 is not None:
        if u in A.free_names(v0.rhs):
            raise ck.err(f"the initial velocity can't use {u}", v0.rhs)
        vv = ck.expr(v0.rhs, lctx)
        ck.need_num(vv, v0.rhs, "the initial velocity")
        ck.unify_or(vv.ty.dim, udim / tdim, lambda: f"∂{u}/∂{tv} at the start should be {ck.desc(udim / tdim)}, "
                    f"not {ck.desc(vv.ty.dim)}", v0.rhs)
    bvals = []
    for end in ("a", "b"):
        kind, cond = bcs[end]
        if u in A.free_names(cond.rhs):
            raise ck.err(f"a boundary value can't use {u} itself", cond.rhs)
        bv = ck.expr(cond.rhs, lctx)
        ck.need_num(bv, cond.rhs, "the boundary value")
        want = udim if kind == 0 else udim / xdim
        what = f"{u}" if kind == 0 else f"∂{u}/∂{xv}"
        ck.unify_or(bv.ty.dim, want, lambda: f"{what} at the boundary should be {ck.desc(want)}, not "
                    f"{ck.desc(bv.ty.dim)}", cond.rhs)
        bvals.append(bv)
    lam.body = [body0, uv0, pv, vv, bvals[0], bvals[1]]
    ck.new_lambdas.append(lam)
    ck.all_lambdas.append(lam)

    # results: the solution, and the ends of the grid kept in hidden variables
    out = []
    info = {"names": [u], "layout": [], "t": tv, "slots": 0, "pde": True}
    sol_sym = ck.new_sym(ck.fresh_name("__pde"), SolTy(info), ctx)
    sol_sym.assigned = True
    ends = []
    for val in (xa, xb):
        sym = ck.new_sym(ck.fresh_name("__pdex"), NumTy(xdim), ctx)
        sym.assigned = True
        out.append(I.SAssign(sym, val))
        ends.append(sym)
    st = I.SSolve(sol_sym, lam, [], t0, t1, step, "pde", 0.0, s.line)
    st.xa, st.xb = ck.var_ref(ends[0], ctx, s), ck.var_ref(ends[1], ctx, s)
    st.grid, st.order, st.pmethod = grid, order, PDE_METHODS[method]
    st.bc = (bcs["a"][0], bcs["b"][0])
    st.is_complex = is_complex
    st.tdep = tv in (names_eq + A.free_names(bcs["a"][1].rhs) + A.free_names(bcs["b"][1].rhs))
    st.event = None
    st.evtext = -1
    st.tname = ck.text(tv)
    tf = I.IConst(0, NumTy(tdim))
    tf.hint = t0.hint or t1.hint
    st.tfmt = ck.fmt(tf)
    _python_only(ck, s.line, "a PDE (solve ∂u/∂t = …)")
    view = PdeView(u, sol_sym, ends[0], ends[1], grid, 2 if is_complex else 1, udim, xdim, tdim, xv, tv)
    view.uhint = uv0.hint
    view.xhint = xa.hint or xb.hint
    view.thint = t0.hint or t1.hint
    ctx.scope.names[u] = view
    out.append(st)
    return out


def pde_call(ck, view, e, ctx, deriv=None):
    """u(x, t), ∂u/∂x(x, t), ∂u/∂t(x, t) of a PDE solution; a complex one gives a complex number."""
    from .types import ComplexTy
    nm = view.name
    which = 0
    d = view.udim
    if deriv is not None:
        if deriv.order != 1 or deriv.var not in (view.xname, view.tname):
            raise ck.err(f"of a PDE solution, ∂{nm}/∂{view.xname} and ∂{nm}/∂{view.tname} can be evaluated", e)
        which = 1 if deriv.var == view.xname else 2
        d = view.udim / (view.xdim if which == 1 else view.tdim)
    if len(e.args) != 2:
        raise ck.err(f"{nm} is a function of {view.xname} and {view.tname}: write {nm}({view.xname}, "
                     f"{view.tname})", e)
    x = ck.expr(e.args[0], ctx)
    t = ck.expr(e.args[1], ctx)
    ck.need_num(x, e.args[0], view.xname)
    ck.need_num(t, e.args[1], view.tname)
    ck.unify_or(x.ty.dim, view.xdim, lambda: f"{nm}'s first argument is {view.xname}, {ck.desc(view.xdim)}, "
                f"not {ck.desc(x.ty.dim)}", e.args[0])
    ck.unify_or(t.ty.dim, view.tdim, lambda: f"{nm}'s second argument is {view.tname}, {ck.desc(view.tdim)}, "
                f"not {ck.desc(t.ty.dim)}", e.args[1])
    xf = I.IConst(0, NumTy(view.xdim))
    xf.hint = view.xhint
    tf = I.IConst(0, NumTy(view.tdim))
    tf.hint = view.thint
    xfmt, tfmt = ck.fmt(xf), ck.fmt(tf)
    parts = []
    for c in range(view.ncomp):
        n = I.IPdeEval(ck.var_ref(view.sol_sym, ctx, e), ck.var_ref(view.xa_sym, ctx, e),
                       ck.var_ref(view.xb_sym, ctx, e), view.m, c * (view.m + 1), x, t, which, NumTy(d))
        n.xfmt, n.tfmt = xfmt, tfmt
        n.sf = None
        n.hint = view.uhint if which == 0 else None
        parts.append(n)
    if view.ncomp == 1:
        return parts[0]
    r = I.IVec(parts, ComplexTy(d))      # a complex number (D91), read with .re / .im (red team round 3 #10)
    r.sf = None
    r.hint = parts[0].hint
    return r


def plots_pde(ck, s, ctx):
    """Does this plot show a PDE solution by its bare name (plot u vs x)?"""
    for sr in s.series:
        if isinstance(sr.y, A.Name) and getattr(ctx.scope.lookup(sr.y.name)[0], "is_pde", False):
            return True
    return False


def check_animate(ck, s, ctx):
    """plot u vs x animate over t [frames N] [to "file.gif"]: frames of u(x, t); without `animate`, one plot
    with the solution at 6 times.  (D83)"""
    import os
    opts = getattr(s, "options", {})
    if len(s.series) != 1:
        raise ck.err("an animation shows one PDE solution:  plot u vs x animate over t", s)
    sr = s.series[0]
    view = ctx.scope.lookup(sr.y.name)[0] if isinstance(sr.y, A.Name) else None
    if not getattr(view, "is_pde", False):
        raise ck.err("animate over t works for the solution of a PDE:  plot u vs x animate over t", sr.y)
    if not (isinstance(sr.x, A.Name) and sr.x.name == view.xname) or sr.lo is not None:
        raise ck.err(f"plot a PDE solution against its space variable:  plot {view.name} vs {view.xname}", sr.x)
    if opts.get("animate") not in (None, view.tname):
        raise ck.err(f"{view.name} changes with {view.tname}: write  animate over {view.tname}", s)
    animate = opts.get("animate") is not None
    frames = int(opts.get("frames", 60))
    if not (2 <= frames <= 1000):
        raise ck.err("frames must be from 2 to 1000", s)
    out = s.out or (f"{view.name}_vs_{view.xname}.gif" if animate else f"{view.name}_vs_{view.xname}.png")
    full = out if os.path.isabs(out) else os.path.join(ck.base_dir, out)
    if not hasattr(ck.tables, "m3_anims"):
        ck.tables.m3_anims = []
    ck.tables.m3_anims.append({
        "out": out, "full": full, "animate": animate, "frames": frames, "m": view.m, "ncomp": view.ncomp,
        "name": view.name, "xname": view.xname, "tname": view.tname, "udim": view.udim, "xdim": view.xdim,
        "tdim": view.tdim, "uhint": view.uhint, "xhint": view.xhint, "thint": view.thint,
        "title": opts.get("title")})
    return I.SAnimate(len(ck.tables.m3_anims) - 1, ck.var_ref(view.sol_sym, ctx, s),
                      ck.var_ref(view.xa_sym, ctx, s), ck.var_ref(view.xb_sym, ctx, s))
