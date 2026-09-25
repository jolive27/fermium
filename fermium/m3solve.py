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
