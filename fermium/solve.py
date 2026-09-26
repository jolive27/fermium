"""Checking for `solve` (ODEs), `fit` (least squares) and `plot`."""
from __future__ import annotations

import dataclasses
import math
import os
import re

from . import ast as A
from . import calculus as C
from . import ir as I
from .checker import FuncInfo, SolView, SolRef, FuncRef, ConstInfo, Scope, Ctx, BUILTINS
from .types import DExpr, NumTy, ListTy, SolTy, DataTy, VecTy, MatTy, ComplexTy
from .units import DIMLESS, preferred_unit, format_number
from . import cplx


class _NeedComplex(Exception):
    """An equation turned out complex (1i ħ ψ' = E ψ) while its unknowns started real: check again with them
    complex (D93)."""

    def __init__(self, names):
        super().__init__(names)
        self.names = frozenset(names)


# ============================================================ solve
def _find_derivs(e, out, known=()):
    """Collect {name: max order} for x', x'', d/dt x in an equation side.  `known` holds defined functions
    whose derivative is only applied to an argument (`V'(x)`): those are values, not unknowns (D210)."""
    if known and isinstance(e, A.Call) and isinstance(e.func, (A.Prime, A.Deriv)):
        f = e.func
        tgt = f.target if isinstance(f, A.Prime) else f.operand
        if isinstance(tgt, A.Name) and tgt.name in known:
            for a in e.args:
                _find_derivs(a, out, known)
            return
    if isinstance(e, A.Prime) and isinstance(e.target, A.Name):
        out[e.target.name] = max(out.get(e.target.name, 0), e.order)
        return
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Name):
        out[e.operand.name] = max(out.get(e.operand.name, 0), e.order)
        return
    for c in A.children(e):
        _find_derivs(c, out, known)


def _known_called(ctx, sides, keep=()):
    """Defined functions (or solutions) whose derivative appears only applied to an argument, `V'(x)`, in
    these equation sides: they are known values, so only the other derivatives are unknowns (D210)."""
    called, bare = set(), set()
    for e in sides:
        _prime_uses(e, called, bare)
    return {n for n in called - bare if n not in keep and ctx.scope.lookup(n)[0] is not None}


def _normalize_derivs(e, tvar):
    """Rewrite d/dt x, dx/dt, d²x/dt² and d/dt (x') as primes so the rest of the code sees one form."""
    from .lexer import canonical_name
    e = C.map_children(e, lambda c: _normalize_derivs(c, tvar))
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Name) and e.var == tvar:
        return A.Prime(e.operand, e.order).at(e)
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Prime) and isinstance(e.operand.target, A.Name) \
            and e.var == tvar:
        return A.Prime(e.operand.target, e.operand.order + e.order).at(e)
    if isinstance(e, A.BinOp) and e.op == "/" and isinstance(e.left, A.Name) and isinstance(e.right, A.Name) \
            and e.right.name == "d" + tvar and e.left.name.startswith("d") and len(e.left.name) > 1 \
            and not e.left.paren:
        return A.Prime(A.Name(canonical_name(e.left.name[1:])).at(e.left), 1).at(e)
    return e


def _unit_primes(e, unknowns):
    """`0.5 b''` with b an unknown of the solve: after a number, b reads as a unit (barn), and the prime
    lands on the quantity 0.5 b.  A prime on a quantity means nothing, so read it as 0.5 · b'' (M9)."""
    e = C.map_children(e, lambda c: _unit_primes(c, unknowns))
    if isinstance(e, A.Prime) and isinstance(e.target, A.Quantity) and not e.target.bracket:
        fs = e.target.unit.factors
        if len(fs) == 1 and fs[0].exp == 1 and fs[0].name in unknowns:
            return A.BinOp("*", e.target.value, A.Prime(A.Name(fs[0].name).at(e.target), e.order).at(e),
                           True).at(e)
    return e


def _ic_names(initial):
    """The unknowns named by the initial conditions: x(0) = …, x'(0) = …"""
    out = set()
    for ic in initial:
        f = ic.lhs.func if isinstance(ic.lhs, A.Call) else None
        while isinstance(f, A.Prime):
            f = f.target
        if isinstance(f, A.Name):
            out.add(f.name)
    return out


def _prime_uses(e, called, bare):
    """Names under a prime or d/dt: `called` when the derivative is applied to an argument (I'(θ)),
    `bare` otherwise (x' in an ODE)."""
    if isinstance(e, A.Call) and isinstance(e.func, (A.Prime, A.Deriv)):
        f = e.func
        tgt = f.target if isinstance(f, A.Prime) else f.operand
        if isinstance(tgt, A.Name):
            called.add(tgt.name)
            for a in e.args:
                _prime_uses(a, called, bare)
            return
    if isinstance(e, A.Prime) and isinstance(e.target, A.Name):
        bare.add(e.target.name)
        return
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Name):
        bare.add(e.operand.name)
        return
    for c in A.children(e):
        _prime_uses(c, called, bare)


def _is_until(e):
    """`until y = 0 m` written as a line of the solve: the parser reads it as the product `until · y`."""
    while isinstance(e, A.BinOp):
        if isinstance(e.left, A.Name) and e.left.name == "until" and e.op in ("*", "-") and \
                (e.op == "-" or getattr(e, "implicit", False)):
            return True
        e = e.left
    return False


def _strip_until(e):
    if isinstance(e.left, A.Name) and e.left.name == "until":
        return e.right if e.op == "*" else A.Neg(e.right).at(e)
    return A.BinOp(e.op, _strip_until(e.left), e.right, getattr(e, "implicit", False)).at(e)


def _take_until(ck, s, ctx):
    """Split the stop condition (D39) off a solve: returns (equations, initial conditions, until)."""
    until = getattr(s, "until", None)
    eqs, inits = list(s.equations), list(s.initial)
    b, _ = ctx.scope.lookup("until")
    if b is None:
        for lst in (eqs, inits):
            for q in list(lst):
                if _is_until(q.lhs):
                    if until is not None:
                        raise ck.err("a solve can have only one stop condition (until ...)", q)
                    lst.remove(q)
                    until = A.Equation(_strip_until(q.lhs), q.rhs).at(q)
    return eqs, inits, until


def check_root(ck, s: A.Solve, ctx):
    """solve lhs = rhs for x from a to b: find the x in [a, b] where the two sides are equal (the first
    sign change of lhs - rhs, then Illinois regula falsi to full precision) and store it in x."""
    if s.step is not None or s.method is not None or s.tolerance is not None or s.absolute:
        raise ck.err("step, tolerance, absolute and using are for differential equations; an equation is solved "
                     "to full "
                     "precision", s)
    x = s.var
    lo = ck.expr(s.lo, ctx)
    hi = ck.expr(s.hi, ctx)
    ck.need_num(lo, s.lo, "the start of the search range")
    ck.need_num(hi, s.hi, "the end of the search range")
    ck.unify_or(lo.ty.dim, hi.ty.dim, lambda: f"the search range goes from {ck.desc(lo.ty.dim)} to "
                f"{ck.desc(hi.ty.dim)}; both ends need the same units", s.lo)
    lam = I.ILambda("scalar", ck.fresh_name("root"))
    lam.locals = []
    scope = Scope(ctx.scope)
    lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
    lctx.enclosing = ctx.func
    xs = I.Sym(x, NumTy(lo.ty.dim), "local", lam)
    xs.assigned = True
    lam.params = [xs]
    scope.names[x] = xs
    q = s.equations[0]
    left = ck.expr(q.lhs, lctx)
    right = ck.expr(q.rhs, lctx)
    ck.need_num(left, q.lhs, "the left side")
    ck.need_num(right, q.rhs, "the right side")
    ck.unify_or(left.ty.dim, right.ty.dim, lambda: f"the two sides of this equation don't match: left is "
                f"{ck.desc(left.ty.dim)}, right is {ck.desc(right.ty.dim)}", q)
    lam.body = ck.arith("-", left, right, q)
    ck.new_lambdas.append(lam)
    ck.all_lambdas.append(lam)
    # the size of the terms that are added up on the two sides (|A| + |B| + |C| for A - B = C), to notice
    # a root in rounding noise (#36); it shares the equation's argument and env
    slam = I.ILambda("scalar", ck.fresh_name("rootscale"))
    slam.params, slam.locals, slam.captures = lam.params, lam.locals, lam.captures
    sdim = NumTy(left.ty.dim)

    def terms(e):
        if isinstance(e, I.IBin) and e.op in ("+", "-") and isinstance(e.ty, NumTy):
            return I.IBin("+", terms(e.a), terms(e.b), sdim)
        if isinstance(e, I.INeg):
            return terms(e.a)
        return I.IBuiltin("abs", [e], e.ty)
    slam.body = I.IBin("+", terms(left), terms(right), sdim)
    r = I.IRoot(lam, lo, hi, NumTy(lo.ty.dim))
    r.scale = slam
    r.hint = lo.hint or hi.hint
    r.sf = None
    tf = I.IConst(0, NumTy(lo.ty.dim))
    tf.hint = r.hint
    r.tfmt = ck.fmt(tf)
    return ck.assign_to(x, r, s, ctx)


def check_solve(ck, s: A.Solve, ctx, force_complex=frozenset()):
    if getattr(s, "lowest", None) is not None:        # an eigenvalue problem (D82)
        from .m3solve import check_eigen
        return check_eigen(ck, s, ctx)
    if getattr(s, "var2", None) is not None:          # a PDE in x and t (D83)
        from .m3solve import check_pde
        return check_pde(ck, s, ctx)
    try:
        return _check_solve(ck, s, ctx, force_complex)
    except _NeedComplex as nc:
        if nc.names <= force_complex:
            raise ck.err("this equation is complex, but its unknowns can't be made complex", s)
        return check_solve(ck, s, ctx, force_complex | nc.names)


def _check_solve(ck, s: A.Solve, ctx, force_complex):
    t = s.var
    src_eqs, initial, until = _take_until(ck, s, ctx)
    unknowns = _ic_names(initial)
    src_eqs = [A.Equation(_unit_primes(q.lhs, unknowns), _unit_primes(q.rhs, unknowns)).at(q) for q in src_eqs]
    eqs = [A.Equation(_normalize_derivs(q.lhs, t), _normalize_derivs(q.rhs, t)).at(q) for q in src_eqs]
    orders = {}
    known = _known_called(ctx, [side for q in eqs for side in (q.lhs, q.rhs)], unknowns)
    for q in eqs:
        _find_derivs(q.lhs, orders, known)
        _find_derivs(q.rhs, orders, known)
    if not initial and len(eqs) == 1 and s.var is not None and until is None and s.var not in orders:
        # I'(θ) of a function or ODE solution that already exists is a value, not an unknown (#3)
        called, bare = set(), set()
        _prime_uses(eqs[0].lhs, called, bare)
        _prime_uses(eqs[0].rhs, called, bare)
        known = all(ctx.scope.lookup(x)[0] is not None for x in orders)
        if not orders or (known and not bare):
            return check_root(ck, A.Solve(src_eqs, [], s.var, s.lo, s.hi, s.step, s.method,
                                          s.tolerance, absolute=s.absolute).at(s), ctx)
    if until is not None and not orders:
        raise ck.err("until (a stop condition) is for differential equations", until)
    if not orders:
        raise ck.err("this solve has no derivatives in it, so there's no differential equation to solve", s,
                     hint="write e.g.  solve x' = -x / τ  with x(0) = 1 for t from 0 s to 5 s,  or for an "
                          "equation:  solve x² = 2 for x from 0 to 2")
    if len(eqs) != len(orders):
        names = ", ".join(orders)
        raise ck.err(f"this solve has {len(eqs)} equation{'s' if len(eqs) != 1 else ''} for {len(orders)} "
                     f"unknown function{'s' if len(orders) != 1 else ''} ({names}); they must match", s)
    # time range
    t0 = ck.expr(s.lo, ctx)
    t1 = ck.expr(s.hi, ctx)
    ck.need_num(t0, s.lo, "the start time")
    ck.need_num(t1, s.hi, "the end time")
    ck.unify_or(t0.ty.dim, t1.ty.dim, lambda: f"the range goes from {ck.desc(t0.ty.dim)} to {ck.desc(t1.ty.dim)}",
                s.lo)
    tdim = t0.ty.dim
    step = None
    if s.step is not None:
        step = ck.expr(s.step, ctx)
        ck.need_num(step, s.step, "the step")
        ck.unify_or(step.ty.dim, tdim, lambda: f"the step is {ck.desc(step.ty.dim)} but {t} is {ck.desc(tdim)}",
                    s.step)
    method = (s.method or ("rk4" if step is not None else "rk45")).lower()
    if method not in ("rk4", "rk45", "radau", "bdf"):
        raise ck.err(f"unknown method '{s.method}' (use rk45, rk4 with a step, or radau for stiff equations)", s)
    if method in ("radau", "bdf") and step is not None:
        raise ck.err(f"{method} chooses its own steps: remove  step …  (a fixed step is for rk4)", s.step)
    if method == "rk4" and step is None:
        raise ck.err("the rk4 method needs a fixed step:  for t from 0 s to 5 s step 0.01 s", s)

    # state layout: for each unknown x of order n: x, x', ..., x^(n-1)
    names = list(orders)
    dims = {x: DExpr.fresh(x) for x in names}
    layout = []
    for x in names:
        for k in range(orders[x]):
            layout.append((x, k))

    # initial conditions
    y0 = {}
    shape = {}
    is_c = {x: x in force_complex for x in names}      # complex unknowns: two state slots each (D93)
    for ic in initial:
        lhs = ic.lhs
        if isinstance(lhs, A.BinOp) and lhs.op == "/" and isinstance(lhs.right, A.Call) and \
                isinstance(lhs.right.func, A.Name) and lhs.right.func.name == "d" + t and \
                isinstance(lhs.left, A.Name) and lhs.left.name.startswith("d"):
            from .lexer import canonical_name       # dx/dt(0) = ...
            lhs = A.Call(A.Prime(A.Name(canonical_name(lhs.left.name[1:])), 1), lhs.right.args).at(lhs)
        if not isinstance(lhs, A.Call) or len(lhs.args) != 1:
            raise ck.err("initial conditions look like  x(0) = 1 m  or  x'(0) = 0 m/s", ic)
        f = lhs.func
        k = 0
        if isinstance(f, A.Prime):
            k = f.order
            f = f.target
        if not isinstance(f, A.Name) or f.name not in orders:
            raise ck.err(f"this initial condition isn't for one of the unknowns ({', '.join(names)})", ic)
        x = f.name
        if k >= orders[x]:
            raise ck.err(f"{x}{chr(39) * k}(…) isn't needed: the equation for {x} is order {orders[x]}", ic)
        v = ck.expr(ic.rhs, ctx)
        ck.need_numlike(v, ic.rhs, "an initial value", allow_vec=True)
        if isinstance(v.ty, ListTy):
            raise ck.err("an initial value must be a number or a vector like <1, 0> m, not a list", ic.rhs)
        if isinstance(v.ty, MatTy):
            raise ck.err("an initial value must be a number or a vector like <1, 0> m, not a matrix", ic.rhs)
        if isinstance(v.ty, VecTy) and v.ty.mixed:
            raise ck.err("a vector unknown needs the same units in every component (write separate unknowns "
                         "for quantities in different units, like x and v)", ic.rhs)
        if isinstance(v.ty, ComplexTy):
            is_c[x] = True
        n_here = v.ty.n if isinstance(v.ty, VecTy) and not isinstance(v.ty, ComplexTy) else 1
        if shape.setdefault(x, n_here) != n_here:
            raise ck.err(f"the initial values of {x} don't match: one is a {shape[x]}-vector, another a "
                         f"{'number' if n_here == 1 else f'{n_here}-vector'}", ic.rhs)
        want = dims[x] / (DExpr.of(tdim) ** k)
        if not ck.U.unify(want, v.ty.dim):
            raise ck.err(f"{x}{chr(39) * k}(…) should be {ck.desc(want)} but this is {ck.desc(v.ty.dim)}", ic.rhs)
        at = ck.expr(lhs.args[0], ctx)
        ck.need_num(at, lhs.args[0], "the time of the initial condition")
        if not ck.U.unify(at.ty.dim, tdim):
            raise ck.err(f"the initial condition is given at {ck.desc(at.ty.dim)} but {t} is {ck.desc(tdim)}",
                         lhs.args[0])
        if isinstance(at, I.IConst) and isinstance(t0, I.IConst) and abs(at.value - t0.value) > 1e-12 * (
                abs(t0.value) + 1e-300) and not (at.value == 0 and t0.value == 0):
            raise ck.err(f"initial conditions must be at the start of the range ({t} = start)", lhs.args[0])
        y0[(x, k)] = v
    for (x, k), v in list(y0.items()):
        if is_c[x]:
            if shape[x] != 1:
                raise ck.err(f"{x} can't be both complex and a vector (vectors of complex numbers aren't supported "
                             f"yet)", s)
            y0[(x, k)] = cplx.promote(v)          # ψ'(0) = 0 for a complex ψ is 0 + 0i
    for x in names:
        if is_c[x]:
            shape[x] = 2
    missing = [x + "'" * k + "(start)" for (x, k) in layout if (x, k) not in y0]
    if missing:
        raise ck.err(f"missing initial condition{'s' if len(missing) > 1 else ''}: {', '.join(missing)}", s,
                     hint="add them after 'with', e.g.  with x(0) = 0.1 [m], x'(0) = 0 m/s")

    # right-hand side lambda
    lam = I.ILambda("ode", ck.fresh_name("ode"))
    lam.locals = []
    scope = Scope(ctx.scope)
    lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
    tsym = I.Sym(t, NumTy(tdim), "local", lam)
    tsym.assigned = True
    lam.params = [tsym]
    scope.names[t] = tsym
    def ty_of(x, k):
        d = dims[x] / (DExpr.of(tdim) ** k)
        if is_c[x]:
            return ComplexTy(d)
        return VecTy(d, shape[x]) if shape[x] > 1 else NumTy(d)
    for (x, k) in layout:
        sym = I.Sym(x + "'" * k, ty_of(x, k), "local", lam)
        sym.assigned = True
        lam.state.append(sym)
        scope.names[x + "'" * k] = sym
    # highest derivatives, only for checking the equations as written
    tops = {}
    for x in names:
        n = orders[x]
        sym = I.Sym(x + "'" * n, ty_of(x, n), "local", lam)
        lam.locals.append(sym)      # only used to check the equation as written
        tops[x] = sym
        scope.names[x + "'" * n] = sym
    for q in eqs:
        lv = ck.expr(q.lhs, lctx)
        rv = ck.expr(q.rhs, lctx)
        ck.need_numlike(lv, q.lhs, "the left side", allow_vec=True)
        ck.need_numlike(rv, q.rhs, "the right side", allow_vec=True)
        if cplx.is_c(lv) or cplx.is_c(rv):
            present = {}
            _find_derivs(q.lhs, present)
            _find_derivs(q.rhs, present)
            real = [x for x in present if x in is_c and not is_c[x]]
            if real:
                raise _NeedComplex(real)
            lv, rv = cplx.promote(lv), cplx.promote(rv)
        if (lv.ty.n if isinstance(lv.ty, VecTy) else 1) != (rv.ty.n if isinstance(rv.ty, VecTy) else 1):
            raise ck.err("one side of this equation is a vector and the other isn't (or they have different "
                         "lengths)", q)
        if not ck.U.unify(lv.ty.dim, rv.ty.dim):
            raise ck.err(f"the two sides of this equation don't match: left is {ck.desc(lv.ty.dim)}, "
                         f"right is {ck.desc(rv.ty.dim)}", q)
    # assign equations to unknowns and isolate the highest derivative
    assigned = {}
    coupled = None
    tops_in = []
    for q in eqs:
        present = {}
        _find_derivs(q.lhs, present)
        _find_derivs(q.rhs, present)
        tops_in.append([x for x in names if present.get(x) == orders[x]])
    if any(len(ts) > 1 for ts in tops_in):
        coupled = _mass_matrix(ck, eqs, tops_in, names, orders, shape, t)
    for q in (eqs if coupled is None else []):
        present = {}
        _find_derivs(q.lhs, present)
        _find_derivs(q.rhs, present)
        cands = [x for x in names if present.get(x) == orders[x] and x not in assigned]
        if not cands:
            raise ck.err("can't tell which unknown this equation is for", q,
                         hint="each equation should contain the highest derivative of one unknown, like x'' = ...")
        x = cands[0]
        target = A.Prime(A.Name(x), orders[x])
        assigned[x] = C.isolate(q.lhs, q.rhs, target)
    for x in names:
        del scope.names[x + "'" * orders[x]]
    if coupled is not None:
        tops_ir = _mass_matrix_ir(ck, coupled, names, orders, dims, tdim, lam, lctx, t)
    body = []
    for (x, k) in layout:
        if k < orders[x] - 1:
            body.append(ck.var_ref(scope.names[x + "'" * (k + 1)], lctx, s))
        elif coupled is not None:
            body.append(tops_ir[x])
        else:
            e = assigned[x]
            v = ck.expr(e, lctx)
            if is_c[x] and isinstance(v.ty, NumTy):
                v = cplx.promote(v)
            elif cplx.is_c(v) and not is_c[x]:
                raise _NeedComplex([x])
            if (v.ty.n if isinstance(v.ty, VecTy) else 1) != shape[x]:
                raise ck.err(f"{x}{chr(39) * orders[x]} must be {'a number' if shape[x] == 1 else 'a vector'} "
                             f"like {x}", s)
            want = dims[x] / (DExpr.of(tdim) ** orders[x])
            if not ck.U.unify(v.ty.dim, want):
                raise ck.err(f"{x}{chr(39) * orders[x]} works out to {ck.desc(v.ty.dim)} but should be "
                             f"{ck.desc(want)}", s)
            body.append(v)
    lam.body = body
    ck.new_lambdas.append(lam)
    ck.all_lambdas.append(lam)
    event = evtext = None
    if until is not None:
        event, evtext = _check_until(ck, until, orders, lam, lctx, t)
    info = {"names": names, "layout": layout, "t": t}
    sol_sym = ck.new_sym(ck.fresh_name("__sol"), SolTy(info), ctx)
    sol_sym.assigned = True
    base = 0
    sfs = [v.sf for v in y0.values() if v.sf is not None] + [v.sf for v in (t0, t1) if v.sf is not None]
    for x in names:
        n = orders[x]
        w = shape[x]
        view = SolView(sol_sym, base, base + (n - 1) * w, dims[x], tdim, t, x)
        view.n = w
        view.stride = w
        view.cplx = is_c[x]
        view.hint = y0[(x, 0)].hint
        view.hints = {k: y0[(x, k)].hint for k in range(n)}
        view.thint = t0.hint or t1.hint
        view.sf = min(sfs) if sfs else None
        ctx.scope.names[x] = view
        base += n * w
    info["slots"] = base
    rtol = 1e-9
    if s.tolerance is not None:
        tv = ck.expr(s.tolerance, ctx)
        # a relative tolerance: a plain number between 0 and 1 (red team round 3 #13: units were accepted)
        if isinstance(tv, I.Expr) and isinstance(tv.ty, NumTy) and not ck.U.unify(tv.ty.dim, DIMLESS):
            raise ck.err(f"the tolerance is relative, so it must be a plain number (no units) like 1e-8, but it "
                         f"is {ck.desc(tv.ty.dim)}", s.tolerance)
        if not isinstance(tv, I.IConst) or not isinstance(tv.ty, NumTy):
            raise ck.err("the tolerance must be a plain number written out, like 1e-8", s.tolerance)
        if not (0 < tv.value < 1):
            raise ck.err(f"the tolerance is relative, so it must be between 0 and 1 (like 1e-8), but it is "
                         f"{tv.value:g}", s.tolerance)
        rtol = tv.value
    atol = _check_absolute(ck, s, ctx, layout, dims, shape, tdim, t, step, y0) if s.absolute else None
    if method in ("radau", "bdf"):
        ck.tables.stiff.append((s.line, method))
    st = I.SSolve(sol_sym, lam, [y0[k] for k in layout], t0, t1, step, method, rtol, s.line)
    st.atol = atol                        # per state slot (value in SI, power of 1/|t1 - t0|), or None (D160)
    st.event = event                      # the stop condition's g = lhs - rhs (D39), or None
    st.evtext = evtext if evtext is not None else -1
    st.tname = ck.text(t)                 # for runtime errors: "at ξ = 0"
    tf = I.IConst(0, NumTy(tdim))
    tf.hint = t0.hint or t1.hint
    st.tfmt = ck.fmt(tf)
    if coupled is not None:            # the singular-mass-matrix error shows t like the solve's other errors
        tops_ir[names[0]].binds[0][1].sing_fmt = st.tfmt
    # does the right side depend on t itself (not only through the unknowns)?  Only then can it jump in t
    # (`if t < 0.3 s`), so only then does the step control look for jumps (D40)
    st.tdep = any(t in A.free_names(q.lhs) + A.free_names(q.rhs) for q in eqs)
    return st


def _same_dim(ck, a, b):
    d = ck.U.norm(DExpr.of(a) / DExpr.of(b))
    return not d.terms and d.const.dimensionless


def _check_absolute(ck, s, ctx, layout, dims, shape, tdim, t, step, y0=None):
    """`absolute a[, b …]`: absolute tolerances (D160).  Each value is a constant with a unit; each unknown
    takes the one in its units, and a derivative slot x' (of a second-order x) takes the one in x's units
    per unit of time when given, else x's divided by |t1 - t0|.  An unknown with no value in its units, two
    values in the same units, or a value no unknown uses, is an error.  Returns, per state slot (vectors
    and complex numbers repeated per component), (value in SI, power k of 1/|t1 - t0|)."""
    if step is not None:
        raise ck.err("absolute sets the error control of the adaptive solvers (rk45, radau, bdf); with  step  "
                     "the steps are fixed, so leave out one of them", s.absolute[0])
    vals = []
    for node in s.absolute:
        v = ck.expr(node, ctx)
        if not isinstance(v, I.IConst) or not isinstance(v.ty, NumTy):
            raise ck.err("an absolute tolerance must be a positive constant, like 1e-16 or 1e-9 m", node)
        u = getattr(v, "hint", None)
        if u is not None and getattr(u, "affine", False):
            # a tolerance is a size of error, so `absolute 1e-6 °C` is a temperature step of 10⁻⁶ K, not the
            # absolute temperature 273.15 K + 10⁻⁶ K (D12 reads a °C value that way), which would switch the
            # error control off (red team round 4 #1, D200); like `± 0.5 °C` (D120)
            v = I.IConst(v.value - u.offset, v.ty)
            v.hint = u
        if not (0 < v.value < math.inf):
            raise ck.err("an absolute tolerance must be a positive constant, like 1e-16 or 1e-9 m", node)
        for w, _ in vals:
            if _same_dim(ck, w.ty.dim, v.ty.dim):
                raise ck.err(f"two absolute tolerances in {ck.desc(v.ty.dim)}; give one value per unit", node)
        vals.append((v, node))
    used = set()
    out = []
    for (x, k) in layout:
        want = dims[x] / (DExpr.of(tdim) ** k)
        hit = next((i for i, (v, _) in enumerate(vals) if _same_dim(ck, v.ty.dim, want)), None)
        power = 0
        if hit is None and k > 0:
            hit = next((i for i, (v, _) in enumerate(vals) if _same_dim(ck, v.ty.dim, dims[x])), None)
            power = k
        if hit is None:
            have = ", ".join(ck.desc(v.ty.dim) for v, _ in vals)

            def sym(d):
                u = preferred_unit(ck.U.resolve(d))
                return f" {u.name}" if u.name not in ("", "1") else ""
            like = ", ".join(f"1e-9{sym(v.ty.dim)}" for v, _ in vals) + f", 1e-12{sym(want)}"
            raise ck.err(f"no absolute tolerance for {x}{chr(39) * k}, which is {ck.desc(want)} (the values given "
                         f"are in {have}); add one in its units after a comma, like  absolute {like}",
                         s.absolute[0])
        used.add(hit)
        out.extend([(vals[hit][0].value, power)] * shape[x])
    for i, (v, node) in enumerate(vals):
        if i not in used:
            raise ck.err(f"no unknown of this solve is in {ck.desc(v.ty.dim)}, so this absolute tolerance isn't "
                         f"used", node)
    if y0 is not None:
        _warn_large_absolute(ck, vals, layout, dims, y0)
    return out


def _warn_large_absolute(ck, vals, layout, dims, y0):
    """An absolute tolerance at least as large as the largest initial value in its units switches the error
    control off for those unknowns (`absolute 1 km` for a 1 m oscillator, a mistyped km for mm), so the
    answer can be silently wrong: warn (red team round 4 #18, D201).  Only initial values written as
    constants count; unknowns that all start at 0 give no scale, so nothing is said."""
    for v, node in vals:
        scale = 0.0
        for (x, k) in layout:
            if k != 0 or not _same_dim(ck, v.ty.dim, dims[x]):
                continue
            c = y0.get((x, 0))
            if isinstance(c, I.IConst) and isinstance(c.value, (int, float)) and math.isfinite(c.value):
                scale = max(scale, abs(float(c.value)))
        if scale > 0 and v.value >= scale:
            ratio = v.value / scale
            times = f"{format_number(ratio, 3)}×" if ratio >= 1.995 else "as large as"
            u = preferred_unit(ck.U.resolve(v.ty.dim))
            sym = f" {u.name}" if u.name not in ("", "1") else ""
            shown = format_number(scale / u.factor, 3) + sym
            ck.diags.warn(f"this absolute tolerance is {times} the largest starting value in its units ({shown}), "
                          f"so the error control is effectively off and the result may be far off",
                          line=node.line, col=node.col,
                          hint="an absolute tolerance is the size of error you accept; make it much smaller than "
                               "the values, e.g. 10⁻⁶ of them (check the unit: mm, not km?)")


def _mass_matrix(ck, eqs, tops_in, names, orders, shape, t):
    """Equations with several highest derivatives, like Lagrange's equations for a double pendulum,
    M(q, q') q'' = f(q, q') (D47).  Returns, per equation, (equation, the coefficients of the highest
    derivatives in the order of `names`, the rest) as ASTs: Σ_j a_ij x_j^(n) + r_i = 0."""
    tops = [x + "'" * orders[x] for x in names]
    q0 = next(q for q, ts in zip(eqs, tops_in) if len(ts) > 1)
    both = " and ".join(x + "'" * orders[x] for x in tops_in[eqs.index(q0)])
    vec = [x for x in names if shape[x] > 1]
    if vec:
        raise ck.err(f"{both} both appear in one equation, which works only for unknowns that are numbers, but "
                     f"{vec[0]} is a vector; write its components as separate unknowns", q0)
    if len(names) > 4:
        raise ck.err(f"{both} both appear in one equation; that works for up to 4 unknowns (this solve has "
                     f"{len(names)}); solve for the highest derivatives yourself, e.g. with solve_linear", q0)
    targets = [A.Prime(A.Name(x), orders[x]) for x in names]
    out = []
    for q in eqs:
        r = C.linear_coeffs(q.lhs, q.rhs, targets)
        if r is None:
            raise ck.err(f"{both} both appear in one equation; that works when every equation is linear in "
                         f"{', '.join(tops)} (like m1 a'' + k b'' = F, with coefficients that may depend on {t} and "
                         f"the unknowns), and this one isn't", q)
        out.append((q, r[0], r[1]))
    for j in range(len(names)):
        if all(C.is_num(coeffs[j], 0) for _, coeffs, _ in out):
            raise ck.err(f"{tops[j]} drops out of the equations (its coefficients are all 0), so they can't be "
                         f"solved for it", q0)
    return out


def _mass_matrix_ir(ck, coupled, names, orders, dims, tdim, lam, lctx, t):
    """The highest derivatives from M x'' = -r at each evaluation of the right side: fermium.linalg's
    Gaussian elimination with partial pivoting on the n×n system (n ≤ 4), as in solve_linear (D47).  The
    coefficients are checked like any expression of the right side; their units are consistent because
    each equation was (the matrix mixes units, which is fine once units are erased)."""
    n = len(names)
    entries, rhs = [], []
    for q, coeffs, r0 in coupled:
        for a in coeffs:
            v = ck.expr(a, lctx)
            ck.need_num(v, q, "a coefficient of a highest derivative")
            entries.append(v)
        v = ck.expr(A.Neg(r0).at(q), lctx)
        ck.need_num(v, q, "the equation")
        rhs.append(v)
    plain = DExpr.of(DIMLESS)
    acc = I.IBuiltin("solve_linear", [I.IVec(entries, MatTy(plain, n, n)), I.IVec(rhs, VecTy(plain, n))],
                     VecTy(plain, n))
    acc.line = coupled[0][0].line
    tops = [x + "'" * orders[x] for x in names]
    acc.sing_t = I.IVar(lam.params[0])       # a singular matrix: an ODE error naming t (not "this matrix")
    acc.sing_text = ck.text(f"the equations don't determine {' and '.join(tops)} at {t} = ")
    sym = I.Sym("__accel", VecTy(plain, n), "local", lam)
    sym.assigned = True
    lam.locals.append(sym)
    out = {}
    for j, x in enumerate(names):
        e = I.IVecElem(I.IVar(sym), j, NumTy(dims[x] / (DExpr.of(tdim) ** orders[x])))
        out[x] = I.ILet([(sym, acc)], e) if j == 0 else e
    return out


def _check_until(ck, until, orders, lam, lctx, t):
    """`until lhs = rhs`: the solve stops where lhs - rhs first changes sign (D39).  Returns the event
    lambda (same arguments, state and env as the right-hand side) and the text of its 'never happened' error."""
    used = {}
    _find_derivs(until.lhs, used)
    _find_derivs(until.rhs, used)
    for x, k in used.items():
        if x in orders and k >= orders[x]:
            have = ", ".join(x + "'" * j for j in range(orders[x]))
            raise ck.err(f"the stop condition can use {have} (not {x}{chr(39) * k})", until)
    left = ck.expr(until.lhs, lctx)
    right = ck.expr(until.rhs, lctx)
    ck.need_num(left, until.lhs, "the left side of the stop condition")
    ck.need_num(right, until.rhs, "the right side of the stop condition")
    ck.unify_or(left.ty.dim, right.ty.dim, lambda: f"the two sides of the stop condition don't match: left is "
                f"{ck.desc(left.ty.dim)}, right is {ck.desc(right.ty.dim)}", until)
    ev = I.ILambda("ode", ck.fresh_name("until"))
    ev.params, ev.state, ev.locals, ev.captures = lam.params, lam.state, lam.locals, lam.captures
    ev.body = [ck.arith("-", left, right, until)]
    text = f"the stop condition (until {C.to_source(until.lhs)} = {C.to_source(until.rhs)}) never happened up to {t} = "
    return ev, ck.text(text)


# ============================================================ fit
def check_fit(ck, s: A.Fit, ctx):
    if not ctx.is_main or ctx.lam is not None:
        raise ck.err("fit can only be used at the top level of a program", s)
    data = ck.expr(s.data, ctx)
    if not isinstance(data, I.Expr) or not isinstance(data.ty, DataTy):
        raise ck.err("fit ... to <data>: the data must come from load \"file.csv\" or table(x = xs, y = ys)", s.data)
    cols = data.ty.info["columns"]
    colnames = [c["name"] for c in cols]
    lhs, rhs = s.model.lhs, s.model.rhs
    lhs_names = A.free_names(lhs)
    if not any(n in colnames for n in lhs_names):
        raise ck.err(f"the left side of a fit must use a column of the data ({', '.join(colnames)})", lhs)
    used = []
    for n in A.free_names(rhs) + [x for x in lhs_names if x not in colnames]:
        if n not in used:
            used.append(n)
    guess_names = [g for g, _ in s.guesses]
    params, user_vars = [], []
    for n in used:
        if n in colnames:
            continue
        b, _ = ctx.scope.lookup(n)
        if isinstance(b, (ConstInfo, FuncInfo)) or (b is None and n in BUILTINS):
            continue
        if isinstance(b, I.Sym) and n not in guess_names:
            user_vars.append(n)
            continue
        params.append(n)
    if not params and user_vars:
        params, user_vars = user_vars, []
    if not params:
        raise ck.err("this fit has no unknown parameters to adjust", s,
                     hint="parameters are the names in the model that aren't data columns or known values")
    # model lambda: columns and parameters are its inputs
    lam = I.ILambda("model", ck.fresh_name("model"))
    lam.locals = []
    scope = Scope(ctx.scope)
    lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
    pdims = {}
    for n in params:
        b, _ = ctx.scope.lookup(n)
        d = b.ty.dim if isinstance(b, I.Sym) and isinstance(b.ty, NumTy) else DExpr.fresh(n)
        pdims[n] = d
        sym = I.Sym(n, NumTy(d), "local", lam)
        sym.assigned = True
        lam.param_syms.append(sym)
        scope.names[n] = sym
    col_used = []
    for i, c in enumerate(cols):
        if c["name"] in used or c["name"] in lhs_names:
            sym = I.Sym(c["name"], NumTy(c["unit"].dim), "local", lam)
            sym.assigned = True
            sym.col_index = i
            lam.col_syms.append(sym)
            scope.names[c["name"]] = sym
            col_used.append(i)
    body = ck.expr(rhs, lctx)
    ck.need_num(body, rhs, "the model")
    lv = ck.expr(lhs, lctx)
    ck.need_num(lv, lhs, "the left side of the fit")
    ydim = lv.ty.dim
    lname = C.to_source(lhs)
    if not ck.U.unify(body.ty.dim, ydim):
        raise ck.err(f"the model gives {ck.desc(body.ty.dim)} but {lname} is {ck.desc(ydim)}", s.model,
                     hint="check the formula: both sides of the fit equation need the same units")
    # residual = model - left side; the fit drives it to zero
    lam.body = I.IBin("-", body, lv, NumTy(ydim))
    lam.ycol = -1
    ck.new_lambdas.append(lam)
    ck.all_lambdas.append(lam)
    # initial guesses
    guesses = []
    for n in params:
        g = None
        for gn, gv in s.guesses:
            if gn == n:
                v = ck.expr(gv, ctx)
                ck.need_num(v, gv, "a starting guess")
                if not ck.U.unify(v.ty.dim, pdims[n]):
                    raise ck.err(f"the starting guess for {n} is {ck.desc(v.ty.dim)} but {n} must be "
                                 f"{ck.desc(pdims[n])}", gv)
                g = v
        if g is None:
            b, _ = ctx.scope.lookup(n)
            if isinstance(b, I.Sym) and isinstance(b.ty, NumTy):
                g = ck.var_ref(b, ctx, s)
        guesses.append(g)
    # result variables
    out_syms = []
    for n in params:
        b, _ = ctx.scope.lookup(n)
        if isinstance(b, I.Sym) and isinstance(b.ty, NumTy) and (b.func is ctx.func or b.storage == "arena"):
            sym = b
        else:
            sym = ck.new_sym(n, NumTy(pdims[n]), ctx)
            ctx.scope.names[n] = sym
        sym.assigned = True
        sym.sf = 3
        sym.direct = False
        d = ck.U.norm(pdims[n])
        if d.concrete:     # show it in the data's unit if a column has the same dimension (τ in min)
            for c in cols:
                if c["unit"].dim == d.const and c["unit"].name not in ("1",) and sym.hint is None:
                    sym.hint = c["unit"]
        out_syms.append(sym)
    fit_id = len(ck.tables.fits)
    ck.tables.fits.append({"params": params, "dims": [pdims[n] for n in params], "model": lam.name,
                           "text": C.to_source(s.model.lhs) + " = " + C.to_source(s.model.rhs),
                           "ycol": lam.ycol, "cols": [sym.col_index for sym in lam.col_syms],
                           "ydim": ydim, "yname": lname, "path": data.ty.info["path"],
                           "columns": cols})
    # standard errors, for err(x) (gauntlet friction #26): hidden variables written by the fit
    err_syms = []
    for n, sym in zip(params, out_syms):
        es = getattr(sym, "err_sym", None)
        if es is None:
            es = ck.new_sym(f"__err_{n}", NumTy(pdims[n]), ctx)
            ctx.scope.names[es.name] = es
            sym.err_sym = es
        es.assigned = True
        es.hint, es.sf, es.direct = sym.hint, 2, False
        err_syms.append(es)
    r = I.SFit(fit_id, data, out_syms, guesses, lam)
    r.err_syms = err_syms
    return r


# ============================================================ plot
def _label(ck, node):
    return C.to_source(node)


def _data_names(node, ctx, found):
    """The names of loaded data sets used as `name.column` inside node."""
    if isinstance(node, A.Field) and isinstance(node.target, A.Name):
        b, _ = ctx.scope.lookup(node.target.name)
        if isinstance(getattr(b, "ty", None), DataTy):
            found.add(node.target.name)
    if isinstance(node, A.Node):
        for f in dataclasses.fields(node):
            _data_names(getattr(node, f.name), ctx, found)
    elif isinstance(node, (list, tuple)):
        for x in node:
            _data_names(x, ctx, found)
    return found


def _axis_name(ck, node, ctx):
    """What an axis and the legend call a plotted thing: its source, with a data set's name dropped before
    its columns (`T`, not `data.T`; `T^2`, not `data.T^2`) (spec A6.4, D253)."""
    src = C.to_source(node)
    for nm in _data_names(node, ctx, set()):
        src = re.sub(rf"(?<![\w.]){re.escape(nm)}\.(?=\w)", "", src)
    return src



def check_plot(ck, s: A.Plot, ctx):
    from .m3solve import plots_pde, check_animate
    if getattr(s, "options", {}).get("animate") or plots_pde(ck, s, ctx):
        return check_animate(ck, s, ctx)             # a PDE solution u(x, t) (D83)
    series = []
    labels = []
    for sr in s.series:
        yunit = xunit = None
        if isinstance(sr.y, A.Convert):
            yunit = ck.resolve_unit(sr.y.unit)
            sr = A.PlotSeries(sr.y.value, sr.x, sr.lo, sr.hi).at(sr)
        if isinstance(sr.x, A.Convert):
            xunit = ck.resolve_unit(sr.x.unit)
            sr = A.PlotSeries(sr.y, sr.x.value, sr.lo, sr.hi).at(sr)
        entry = _plot_series(ck, sr, ctx, s)
        for which, u in (("y", yunit), ("x", xunit)):
            if u is not None:
                if not ck.U.unify(entry[which + "dim"], u.dim):
                    raise ck.err(f"can't show {ck.desc(entry[which + 'dim'])} in {u.name}", sr)
                entry[which + "hint"] = u
        if series:          # one pair of axes: every series in the same units (A23)
            for which in ("x", "y"):
                ck.unify_or(series[0][which + "dim"], entry[which + "dim"],
                            lambda: f"all series in one plot need the same {which} units (here "
                                    f"{ck.desc(series[0][which + 'dim'])} and {ck.desc(entry[which + 'dim'])})", sr)
        series.append(entry)
        labels.append(entry)
    return _finish_plot(ck, s, series, ctx)


def _plot_options(ck, s, series, ctx):
    """The plot's options for the runtime, with `x from a to b` / `y from a to b` checked against the axis
    units and turned into SI numbers (D161); the plot's own units (the first series') show them."""
    opts = dict(getattr(s, "options", {}))
    for which in ("x", "y"):
        rng = opts.pop(which + "range", None)
        if rng is None:
            continue
        vals = []
        for node in rng:
            v = ck.expr(node, ctx)
            ck.need_num(v, node, f"the {which} range")
            if not isinstance(v, I.IConst):
                raise ck.err(f"the {which} range must be constants, like  {which} from 1e-12 to 1  or  "
                             f"{which} from 0 s to 10 s", node)
            want = series[0][which + "dim"]
            ck.unify_or(v.ty.dim, want, lambda v=v, want=want: f"the {which} axis is {ck.desc(want)}, but this end of "
                        f"its range is {ck.desc(v.ty.dim)}", node)
            vals.append(float(v.value))
        if not (vals[0] < vals[1]):
            raise ck.err(f"the {which} range must go from the smaller value to the larger one; to have {which} "
                         f"decrease along the axis, add  reversed {which}", rng[0])
        if opts.get("log" + which) and vals[0] <= 0:
            raise ck.err(f"a log {which} axis can't start at 0 or below", rng[0])
        opts[which + "lim"] = tuple(vals)
    return opts


def _plot_series(ck, sr, ctx, s):
    if sr.lo is not None and isinstance(sr.x, A.Name):
        yv = None
        b, _ = ctx.scope.lookup(sr.y.func.name) if isinstance(sr.y, A.Call) and isinstance(sr.y.func, A.Name) \
            else (None, None)
        if isinstance(sr.y, A.Name):
            b, _ = ctx.scope.lookup(sr.y.name)
            if isinstance(b, FuncInfo):
                yv = FuncRef(b)
        xv = None
    else:
        yv = _plot_side(ck, sr.y, ctx)
        xv = None
        if isinstance(sr.x, A.Name):
            b, _ = ctx.scope.lookup(sr.x.name)
            if b is None or isinstance(b, (ConstInfo,)) and sr.x.name not in ("π",):
                xv = None
            else:
                xv = _plot_side(ck, sr.x, ctx)
        else:
            xv = _plot_side(ck, sr.x, ctx)
    if True:
        entry = {"ylabel": _axis_name(ck, sr.y, ctx), "xlabel": _axis_name(ck, sr.x, ctx)}
        for side, node in ((yv, sr.y), (xv, sr.x)):
            if isinstance(side, SolRef) and side.view.n > 1:
                nm = side.view.name
                raise ck.err(f"{nm} is a vector; plot its components, e.g.  plot {nm}.y vs {nm}.x", node)
        if isinstance(yv, SolRef) and xv is None:
            v = yv.view
            if sr.x.name != v.tname:
                raise ck.err(f"{v.name} is a function of {v.tname}; plot it  vs {v.tname}", sr.x)
            sol = ck.var_ref(v.sol_sym, ctx, s)
            comp, use_dy = (v.comp, False) if v.comp <= v.top else (v.top, True)
            entry.update(kind="sol", sol=sol, comp=comp, dy=use_dy, ydim=v.dim, xdim=v.tdim,
                         yhint=getattr(v, "hint", None), xhint=getattr(v, "thint", None))
            if sr.lo is not None:
                raise ck.err("a solution is plotted over the range it was solved for (no 'from ... to' needed)", sr)
        elif isinstance(yv, SolRef) and isinstance(xv, SolRef):
            a, b = yv.view, xv.view
            if a.sol_sym is not b.sol_sym:
                raise ck.err("can only plot two solutions against each other if they come from the same solve", sr)
            sol = ck.var_ref(a.sol_sym, ctx, s)
            entry.update(kind="solxy", sol=sol, comp=a.comp if a.comp <= a.top else a.top, dy=a.comp > a.top,
                         comp2=b.comp if b.comp <= b.top else b.top, dy2=b.comp > b.top,
                         ydim=a.dim, xdim=b.dim, yhint=getattr(a, "hint", None), xhint=getattr(b, "hint", None))
        elif xv is None or sr.lo is not None:
            # y is a formula in the (undefined) x variable, sampled over a range
            if sr.lo is None:
                raise ck.err(f"{C.to_source(sr.x)} isn't defined; to plot a formula give a range, like "
                             f"plot y vs {C.to_source(sr.x)} from 0 to 10", sr.x)
            if not isinstance(sr.x, A.Name):
                raise ck.err("to plot a formula, the thing after 'vs' must be a variable name", sr.x)
            lo = ck.expr(sr.lo, ctx)
            hi = ck.expr(sr.hi, ctx)
            ck.need_num(lo, sr.lo)
            ck.need_num(hi, sr.hi)
            ck.unify_or(lo.ty.dim, hi.ty.dim, lambda: "the two ends of the plot range need the same units", sr)
            lam = I.ILambda("scalar", ck.fresh_name("plotfn"))
            lam.locals = []
            scope = Scope(ctx.scope)
            lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
            xs = I.Sym(sr.x.name, NumTy(lo.ty.dim), "local", lam)
            xs.assigned = True
            lam.params = [xs]
            scope.names[sr.x.name] = xs
            yexpr = sr.y
            if isinstance(yv, FuncRef):
                yexpr = A.Call(sr.y, [A.Name(sr.x.name)]).at(sr.y)
            body = ck.expr(yexpr, lctx)
            ck.need_num(body, sr.y, "the thing to plot")
            lam.body = body
            ck.new_lambdas.append(lam)
            ck.all_lambdas.append(lam)
            entry.update(kind="func", lam=lam, lo=lo, hi=hi, ydim=body.ty.dim, xdim=lo.ty.dim,
                         yhint=body.hint, xhint=lo.hint or hi.hint)
        else:
            if isinstance(yv, SolRef):
                yv = ck.sol_values(yv.view)
            if isinstance(xv, SolRef):
                xv = ck.sol_values(xv.view)
            for v, node in ((yv, sr.y), (xv, sr.x)):
                if isinstance(v, FuncRef) or not isinstance(v.ty, ListTy):
                    what = "a function" if isinstance(v, FuncRef) else "a single value"
                    raise ck.err(f"can't plot {what} here; plot needs lists of values (or a solution, or a formula "
                                 f"with a range)", node,
                                 hint="e.g.  plot v vs t from 0 s to 5 s   or   plot ys vs xs")
            entry.update(kind="lists", y=yv, x=xv, ydim=yv.ty.dim, xdim=xv.ty.dim, yhint=yv.hint, xhint=xv.hint,
                         points=isinstance(yv, I.IColumn) or isinstance(xv, I.IColumn)
                         or bool(getattr(s, "options", {}).get("points")))
    return entry


def _finish_plot(ck, s, series, ctx):
    out = s.out
    if out is None:
        def clean(t):
            return "".join(ch if ch.isalnum() else "_" for ch in t).strip("_") or "plot"
        first = s.series[0]
        out = f"{clean(_label(ck, first.y))}_vs_{clean(_label(ck, first.x))}.png"
    full = out if os.path.isabs(out) else os.path.join(ck.base_dir, out)
    info = {"out": out, "full": full, "options": _plot_options(ck, s, series, ctx),
            "series": [{k: v for k, v in e.items() if k in ("ylabel", "xlabel", "kind", "ydim", "xdim", "yhint",
                                                          "xhint", "points")} for e in series]}
    ck.tables.plots.append(info)
    pid = len(ck.tables.plots) - 1
    return I.SPlot(pid, series)


def _plot_side(ck, node, ctx):
    return ck.expr(node, ctx, allow_func=True)



