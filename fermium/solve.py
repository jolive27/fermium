"""Checking for `solve` (ODEs), `fit` (least squares) and `plot`."""
from __future__ import annotations

import os

from . import ast as A
from . import calculus as C
from . import ir as I
from .checker import FuncInfo, SolView, SolRef, FuncRef, ConstInfo, Scope, Ctx, BUILTINS
from .types import DExpr, NumTy, ListTy, SolTy, DataTy, VecTy


# ============================================================ solve
def _find_derivs(e, out):
    """Collect {name: max order} for x', x'', d/dt x in an equation side."""
    if isinstance(e, A.Prime) and isinstance(e.target, A.Name):
        out[e.target.name] = max(out.get(e.target.name, 0), e.order)
        return
    if isinstance(e, A.Deriv) and isinstance(e.operand, A.Name):
        out[e.operand.name] = max(out.get(e.operand.name, 0), e.order)
        return
    for c in A.children(e):
        _find_derivs(c, out)


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


def check_root(ck, s: A.Solve, ctx):
    """solve lhs = rhs for x from a to b: find the x in [a, b] where the two sides are equal (the first
    sign change of lhs - rhs, then Illinois regula falsi to full precision) and store it in x."""
    if s.step is not None or s.method is not None or s.tolerance is not None:
        raise ck.err("step, tolerance and using are for differential equations; an equation is solved to full "
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
    r = I.IRoot(lam, lo, hi, NumTy(lo.ty.dim))
    r.hint = lo.hint or hi.hint
    r.sf = None
    tf = I.IConst(0, NumTy(lo.ty.dim))
    tf.hint = r.hint
    r.tfmt = ck.fmt(tf)
    return ck.assign_to(x, r, s, ctx)


def check_solve(ck, s: A.Solve, ctx):
    t = s.var
    eqs = [A.Equation(_normalize_derivs(q.lhs, t), _normalize_derivs(q.rhs, t)).at(q) for q in s.equations]
    orders = {}
    for q in eqs:
        _find_derivs(q.lhs, orders)
        _find_derivs(q.rhs, orders)
    if not orders and not s.initial and len(eqs) == 1 and s.var is not None:
        return check_root(ck, s, ctx)
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
    if method not in ("rk4", "rk45"):
        raise ck.err(f"unknown method '{s.method}' (use rk4 or rk45)", s)
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
    for ic in s.initial:
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
        n_here = v.ty.n if isinstance(v.ty, VecTy) else 1
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
    missing = [x + "'" * k + "(start)" for (x, k) in layout if (x, k) not in y0]
    if missing:
        raise ck.err(f"missing initial condition{'s' if len(missing) > 1 else ''}: {', '.join(missing)}", s,
                     hint="add them after 'with', e.g.  with x(0) = 0.1 m, x'(0) = 0 m/s")

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
        if (lv.ty.n if isinstance(lv.ty, VecTy) else 1) != (rv.ty.n if isinstance(rv.ty, VecTy) else 1):
            raise ck.err("one side of this equation is a vector and the other isn't (or they have different "
                         "lengths)", q)
        if not ck.U.unify(lv.ty.dim, rv.ty.dim):
            raise ck.err(f"the two sides of this equation don't match: left is {ck.desc(lv.ty.dim)}, "
                         f"right is {ck.desc(rv.ty.dim)}", q)
    # assign equations to unknowns and isolate the highest derivative
    assigned = {}
    for q in eqs:
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
    body = []
    for (x, k) in layout:
        if k < orders[x] - 1:
            body.append(ck.var_ref(scope.names[x + "'" * (k + 1)], lctx, s))
        else:
            e = assigned[x]
            v = ck.expr(e, lctx)
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
        if not isinstance(tv, I.IConst) or not (0 < tv.value < 1):
            raise ck.err("the tolerance must be a plain number like 1e-8", s.tolerance)
        rtol = tv.value
    return I.SSolve(sol_sym, lam, [y0[k] for k in layout], t0, t1, step, method, rtol, s.line)


# ============================================================ fit
def check_fit(ck, s: A.Fit, ctx):
    if not ctx.is_main or ctx.lam is not None:
        raise ck.err("fit can only be used at the top level of a program", s)
    data = ck.expr(s.data, ctx)
    if not isinstance(data, I.Expr) or not isinstance(data.ty, DataTy):
        raise ck.err("fit ... to <data>: the data must come from load \"file.csv\"", s.data)
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
    return I.SFit(fit_id, data, out_syms, guesses, lam)


# ============================================================ plot
def _label(ck, node):
    return C.to_source(node)


def check_plot(ck, s: A.Plot, ctx):
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
    return _finish_plot(ck, s, series)


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
        entry = {"ylabel": _label(ck, sr.y), "xlabel": _label(ck, sr.x)}
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
                         points=isinstance(yv, I.IColumn) or isinstance(xv, I.IColumn))
    return entry


def _finish_plot(ck, s, series):
    out = s.out
    if out is None:
        def clean(t):
            return "".join(ch if ch.isalnum() else "_" for ch in t).strip("_") or "plot"
        first = s.series[0]
        out = f"{clean(_label(ck, first.y))}_vs_{clean(_label(ck, first.x))}.png"
    full = out if os.path.isabs(out) else os.path.join(ck.base_dir, out)
    info = {"out": out, "full": full, "options": getattr(s, "options", {}),
            "series": [{k: v for k, v in e.items() if k in ("ylabel", "xlabel", "kind", "ydim", "xdim", "yhint",
                                                          "xhint", "points")} for e in series]}
    ck.tables.plots.append(info)
    pid = len(ck.tables.plots) - 1
    return I.SPlot(pid, series)


def _plot_side(ck, node, ctx):
    return ck.expr(node, ctx, allow_func=True)



