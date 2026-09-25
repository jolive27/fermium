"""Name resolution, type checking and dimension checking: AST -> typed IR.

This is where "can't add length [m] to time [s]" comes from.  All unit logic
lives here; the IR produced contains plain SI numbers only.
"""
from __future__ import annotations

import csv
import math
import os
from difflib import get_close_matches
from fractions import Fraction

from . import ast as A
from . import calculus as C
from . import ir as I
from .constants import all_constants
from .errors import FermiumError, Diagnostics
from .types import (DExpr, Unifier, NumTy, ListTy, BoolTy, StrTy, SolTy, DataTy, VecTy, TextListTy, BOOL, STR,
                    VOID, Ty,
                    type_desc)
from .units import DIMLESS, Unit, lookup_unit, parse_unit_string, UnitSyntaxError, T as TIME_DIM, dim_name

MATH1 = {"sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
         "exp", "ln", "log", "log10", "log2", "erf", "erfc", "gamma", "lgamma", "expm1", "log1p"}
SAME1 = {"abs", "floor", "ceil", "round"}
LIST_FUNCS = {"len", "sum", "mean", "std", "first", "last", "cumsum", "diff", "reverse", "sort"}
BUILTINS = MATH1 | SAME1 | LIST_FUNCS | {
    "sqrt", "cbrt", "min", "max", "atan2", "hypot", "sign", "mod", "linspace", "zeros", "ones", "range",
    "push", "append", "to", "values", "times", "dot", "factorial", "clamp", "isnan", "rand", "interp", "trapz", "clock", "norm", "unit", "hat", "cross", "vec",
}


class FuncInfo:
    def __init__(self, name, fdef: A.FuncDef, scope):
        self.name = name
        self.fdef = fdef
        self.scope = scope            # defining scope
        self.instances = {}
        self.derived = {}
        self.display_name = name
        self.checked_generic = False

    @property
    def one_liner(self):
        return isinstance(self.fdef.body, A.Node)

    def body_expr(self):
        b = self.fdef.body
        if self.fdef.where:
            b = A.Where(b, self.fdef.where)
        return b


class SolView:
    n = 1          # vector length (1 = a plain number)
    stride = 1     # slots between successive derivatives

    def __init__(self, sol_sym, comp, top, dim, tdim, tname, name):
        self.sol_sym = sol_sym        # Sym holding the solution handle
        self.comp = comp              # component index of this derivative
        self.top = top                # index of the var's highest stored derivative
        self.dim = dim                # DExpr of this derivative
        self.tdim = tdim
        self.tname = tname
        self.name = name


class ConstInfo:
    def __init__(self, name, value, unit, desc):
        self.name, self.value, self.unit, self.desc = name, value, unit, desc


class FuncRef:
    def __init__(self, info):
        self.info = info


class SolRef:
    def __init__(self, view):
        self.view = view


class Scope:
    def __init__(self, parent=None, kind="block"):
        self.parent = parent
        self.names = {}
        self.kind = kind

    def lookup(self, name):
        s = self
        while s is not None:
            if name in s.names:
                return s.names[name], s
            s = s.parent
        return None, None


class Ctx:
    """Where we are: which function (or lambda) owns new locals, and its scope."""

    def __init__(self, func, scope, is_main=False, parent=None, lam=None):
        self.func = func
        self.scope = scope
        self.is_main = is_main
        self.parent = parent
        self.lam = lam
        self.loop = 0
        self.ret_types = []

    def child(self, scope):
        c = Ctx(self.func, scope, self.is_main, self.parent, self.lam)
        c.loop = self.loop
        c.ret_types = self.ret_types
        return c


class Tables:
    """Runtime tables shared with the Python side of the runtime (printing, plots, data, fits)."""

    def __init__(self):
        self.fmts = []      # print formats
        self.texts = []     # constant strings
        self.plots = []
        self.loads = []
        self.fits = []


class CheckedModule:
    def __init__(self, main, funcs, lambdas, tables, unifier):
        self.main = main
        self.funcs = funcs
        self.lambdas = lambdas
        self.tables = tables
        self.U = unifier


class Checker(C.DiffContext):
    def __init__(self, diags=None, base_dir=".", repl=False, source_name="<program>"):
        self.diags = diags or Diagnostics()
        self.U = Unifier()
        self.base_dir = base_dir
        self.repl = repl
        self.tables = Tables()
        self.root = Scope(kind="root")
        for name, (val, unit, desc) in all_constants().items():
            self.root.names[name] = ConstInfo(name, val, unit, desc)
        self.globals = Scope(self.root, kind="global")
        self.all_funcs = []
        self.all_lambdas = []
        self.counter = 0
        self.main_count = 0
        self.cur_ctx = None
        self.new_funcs = []
        self.new_lambdas = []

    # ============================================================ helpers
    def fresh_name(self, base):
        self.counter += 1
        return f"{base}.{self.counter}"

    def err(self, msg, node=None, hint=None):
        if node is not None:
            return FermiumError(msg, node.line or None, node.col or None, getattr(node, "length", 1), hint)
        return FermiumError(msg, hint=hint)

    def desc(self, d):
        return self.U.describe(d)

    def unify_or(self, a, b, msg_fn, node, hint=None):
        if not self.U.unify(a, b):
            raise self.err(msg_fn(), node, hint)

    def resolve_unit(self, uexpr: A.UnitExpr) -> Unit:
        total = None
        for f in uexpr.factors:
            u = lookup_unit(f.name)
            if u is None:
                sugg = ""
                if f.name in ("h",):
                    sugg = "for hours write hr"
                elif f.name in ("t",):
                    sugg = "for metric tons write tonne"
                raise FermiumError(f"'{f.name}' is not a unit Fermium knows", f.line, f.col, len(f.name),
                                   hint=sugg or "see the units list in docs/reference.md")
            if u.affine:
                if len(uexpr.factors) > 1 or f.exp != 1:
                    raise FermiumError(f"{f.name} can't be combined with other units (it has an offset)", f.line,
                                       f.col, len(f.name), hint="use K for temperature differences and in formulas")
            else:
                u = u ** f.exp if f.exp != 1 else u
            total = u if total is None else total * u
        if total is None:
            total = Unit("1", DIMLESS, 1.0)
        total.name = canonical_unit_name(uexpr) or total.name
        return total

    # ============================================================ program
    def check_program(self, prog: A.Program, name="main") -> CheckedModule:
        self.main_count += 1
        fname = name if self.main_count == 1 else f"{name}.{self.main_count}"
        main = I.IFunc(fname, [], VOID)
        ctx = Ctx(main, self.globals, is_main=True)
        self.new_funcs = []
        self.new_lambdas = []
        self._prescan_functions(prog.body, ctx)
        main.body = self.block(prog.body, ctx, new_scope=False)
        self.check_uncalled()
        return CheckedModule(main, self.new_funcs, self.new_lambdas, self.tables, self.U)

    def _prescan_functions(self, body, ctx):
        pass

    def check_uncalled(self):
        """Check the bodies of functions that were never called, so their errors still show."""
        for name, b in list(self.globals.names.items()):
            if isinstance(b, FuncInfo) and not b.instances and not b.checked_generic:
                b.checked_generic = True
                if b.fdef is None:
                    continue
                if not b.one_liner and not b.fdef.params:
                    continue
                args = [I.IConst(0, NumTy(DExpr.fresh())) for _ in b.fdef.params]
                try:
                    saved = (self.new_funcs, self.new_lambdas)
                    self.new_funcs, self.new_lambdas = [], []
                    self.instantiate(b, args, b.fdef, cache=False)
                except FermiumError as e:
                    msg = str(e.message)
                    if "isn't defined" in msg or "used before" in msg:
                        continue
                    raise
                finally:
                    self.new_funcs, self.new_lambdas = saved

    def block(self, stmts, ctx, new_scope=True):
        out = []
        for s in stmts:
            r = self.stmt(s, ctx)
            if r is None:
                continue
            if isinstance(r, list):
                out.extend(r)
            else:
                r.line = getattr(s, "line", 0)
                out.append(r)
        return out

    # ============================================================ statements
    def new_sym(self, name, ty, ctx):
        storage = "local"
        if ctx.is_main and self.repl and ctx.lam is None:
            storage = "arena"
        sym = I.Sym(name, ty, storage, ctx.func)
        if ctx.lam is None:
            ctx.func.locals.append(sym)
        else:
            ctx.lam.locals.append(sym)
        return sym

    def stmt(self, s, ctx):
        self.cur_ctx = ctx
        m = getattr(self, "s_" + type(s).__name__, None)
        if m is None:
            raise self.err(f"this kind of statement ({type(s).__name__}) isn't supported here", s)
        return m(s, ctx)

    def s_ExprStmt(self, s, ctx):
        e = s.value
        if isinstance(e, A.Call) and isinstance(e.func, A.Name) and e.func.name in ("push", "append"):
            return self.push_stmt(e, ctx)
        v = self.expr(e, ctx, allow_func=True)
        if ctx.is_main and self.repl and ctx.lam is None:
            return self.print_items([v], [e], ctx)
        if isinstance(v, (FuncRef, SolRef)):
            return None
        if isinstance(v, I.ICall) or isinstance(v, I.IMap):
            return I.SExpr(v)
        self.diags.warn("this line computes a value but doesn't use it", line=s.line, col=s.col,
                        hint="use print to show it, or store it:  name = ...")
        return I.SExpr(v)

    def push_stmt(self, e, ctx):
        if len(e.args) != 2 or not isinstance(e.args[0], A.Name):
            raise self.err("push needs a list variable and a value: push(xs, x)", e)
        lst = self.expr(e.args[0], ctx)
        if isinstance(lst, I.IVar) and isinstance(lst.ty, TextListTy):
            v = self.expr(e.args[1], ctx)
            if not isinstance(v.ty, StrTy):
                raise self.err("this list holds text, so you can only push text onto it", e.args[1])
            return I.SPush(lst.sym, v)
        if not isinstance(lst, I.IVar) or not isinstance(lst.ty, ListTy):
            raise self.err(f"push needs a list as its first argument, not {type_desc(lst.ty, self.U)}", e.args[0])
        v = self.expr(e.args[1], ctx)
        self.need_num(v, e.args[1], "the value to push")
        self.unify_or(lst.ty.dim, v.ty.dim, lambda: f"can't add {self.desc(v.ty.dim)} to a list of "
                      f"{self.desc(lst.ty.dim)}", e.args[1])
        return I.SPush(lst.sym, v)

    def s_Assign(self, s, ctx):
        if s.op != "=":
            target = A.Name(s.name).at(s)
            op = s.op[0]
            val_ast = A.BinOp(op, target, s.value)
            val_ast.line, val_ast.col, val_ast.length = s.line, s.col, s.length
            v = self.expr(val_ast, ctx)
            b, _ = ctx.scope.lookup(s.name)
            if not isinstance(b, I.Sym):
                raise self.err(f"{s.name} needs a value before you can use {s.op} on it", s)
            return self.assign_to(s.name, v, s, ctx)
        v = self.expr(s.value, ctx, allow_func=True)
        if isinstance(v, FuncRef):
            ctx.scope.names[s.name] = v.info
            if v.info.display_name.startswith("<") or "'" in v.info.display_name or "∂" in v.info.display_name \
                    or v.info.display_name.startswith("λ"):
                v.info.display_name = s.name
            return None
        if isinstance(v, SolRef):
            ctx.scope.names[s.name] = v.view
            return None
        return self.assign_to(s.name, v, s, ctx)

    def assign_to(self, name, v, node, ctx):
        if isinstance(v.ty, type(VOID)) or v.ty is None:
            raise self.err("this doesn't produce a value to store", node)
        b, scope = ctx.scope.lookup(name)
        owned = isinstance(b, I.Sym) and (b.func is ctx.func or (b.storage == "arena" and ctx.is_main)) \
            and (ctx.lam is None or b in getattr(ctx.lam, "locals", []))
        if owned:
            sym = b
            if type(sym.ty) is not type(v.ty):
                raise self.err(f"{name} holds {type_desc(sym.ty, self.U)}; it can't now hold "
                               f"{type_desc(v.ty, self.U)}", node, hint="use a different name for the new value")
            if isinstance(sym.ty, VecTy) and sym.ty.n != v.ty.n:
                raise self.err(f"{name} holds a {sym.ty.n}-vector; it can't now hold a {v.ty.n}-vector", node)
            if isinstance(sym.ty, (NumTy, ListTy, VecTy)):
                if not self.U.unify(sym.ty.dim, v.ty.dim):
                    if self.repl and ctx.is_main:
                        sym = self.new_sym(name, v.ty, ctx)
                        ctx.scope.names[name] = sym
                    else:
                        raise self.err(
                            f"{name} is {self.desc(sym.ty.dim)}; it can't now hold {self.desc(v.ty.dim)}",
                            node, hint="each variable keeps its units; use a new name for a different quantity")
            if ctx.loop == 0 and not getattr(ctx, "branch", 0):
                # straight-line code: the variable now shows the new value's precision and unit
                sym.sf, sym.direct = v.sf, v.direct
                if v.hint is not None:
                    sym.hint = v.hint
            else:
                if v.sf is not None:
                    sym.sf = v.sf if sym.sf is None else min(sym.sf, v.sf)
                sym.direct = False
                if v.hint is not None and sym.hint is None:
                    sym.hint = v.hint
        else:
            if isinstance(v.ty, SolTy):
                raise self.err("can't store an ODE solution in a variable this way", node)
            ty = v.ty
            sym = self.new_sym(name, ty, ctx)
            ctx.scope.names[name] = sym
            sym.sf = v.sf
            sym.hint = v.hint
            sym.direct = v.direct
        sym.assigned = True
        return I.SAssign(sym, v)

    def s_IndexAssign(self, s, ctx):
        b, _ = ctx.scope.lookup(s.target)
        if not isinstance(b, I.Sym) or not isinstance(b.ty, ListTy):
            raise self.err(f"{s.target} isn't a list, so you can't set {s.target}[...]", s)
        tgt = self.expr(A.Name(s.target).at(s), ctx)
        idx = self.index_expr(s.index, tgt, ctx)
        if s.op != "=":
            cur = A.Index(A.Name(s.target).at(s), s.index).at(s)
            val_ast = A.BinOp(s.op[0], cur, s.value).at(s)
            v = self.expr(val_ast, ctx)
        else:
            v = self.expr(s.value, ctx)
        self.need_num(v, s.value, "a list element")
        self.unify_or(b.ty.dim, v.ty.dim,
                      lambda: f"{s.target} is a list of {self.desc(b.ty.dim)}; can't put "
                              f"{self.desc(v.ty.dim)} in it", s.value)
        return I.SIndexAssign(tgt.sym, idx, v, s.line)

    def s_FuncDef(self, s, ctx):
        if not ctx.is_main or ctx.lam is not None:
            raise self.err("functions must be defined at the top level of the program (not inside a block)", s)
        info = FuncInfo(s.name, s, ctx.scope)
        ctx.scope.names[s.name] = info
        return None

    def s_Print(self, s, ctx):
        vals = [self.expr(it, ctx, allow_func=True) for it in s.items]
        return self.print_items(vals, s.items, ctx)

    def print_items(self, vals, asts, ctx):
        items = []
        for v, a in zip(vals, asts):
            if isinstance(v, FuncRef):
                items.append(("text", None, self.text(self.describe_function(v.info))))
            elif isinstance(v, SolRef):
                sv = v.view
                items.append(("text", None, self.text(
                    f"{sv.name}({sv.tname}): solution of an ODE (use {sv.name}({sv.tname}) for a value, "
                    f"or plot {sv.name} vs {sv.tname})")))
            elif isinstance(v.ty, NumTy):
                items.append(("num", v, self.fmt(v)))
            elif isinstance(v.ty, ListTy):
                items.append(("list", v, self.fmt(v)))
            elif isinstance(v.ty, VecTy):
                items.append(("vec", v, self.fmt(v)))
            elif isinstance(v.ty, TextListTy):
                items.append(("textlist", v, None))
            elif isinstance(v.ty, BoolTy):
                items.append(("bool", v, None))
            elif isinstance(v.ty, StrTy):
                if isinstance(v, I.IStr):
                    items.append(("text", None, self.text(v.value)))
                else:
                    items.append(("textvar", v, None))
            elif isinstance(v.ty, DataTy):
                info = v.ty.info
                cols = ", ".join(f"{c['name']} [{c['unit'].name}]" if c['unit'].name != "1" else c['name']
                                 for c in info["columns"])
                items.append(("data", v, self.text(f"data from {info['path']}: columns {cols}")))
            else:
                raise self.err("can't print this", a)
        return I.SPrint(items)

    def text(self, s):
        self.tables.texts.append(s)
        return len(self.tables.texts) - 1

    def fmt(self, v):
        self.tables.fmts.append({"dim": v.ty.dim, "hint": v.hint, "sf": v.sf, "direct": v.direct})
        return len(self.tables.fmts) - 1

    def describe_function(self, info: FuncInfo):
        f = info.fdef
        params = ", ".join(p.name for p in f.params)
        if info.one_liner:
            body = C.to_source(info.body_expr())
            s = f"{info.display_name}({params}) = {body}"
            units = self.function_units(info)
            if units:
                s += f"   [{units}]"
            return s
        return f"{info.display_name}({params}): a function defined over several lines"

    def function_units(self, info):
        parent = getattr(info, "parent", None)
        if parent is not None:          # a derivative: units of f / units of the variable^order
            base, i, order = parent
            target = base
            try:
                args = [I.IConst(1, NumTy(DExpr.fresh(p.name))) for p in target.fdef.params]
                saved = (self.new_funcs, self.new_lambdas)
                self.new_funcs, self.new_lambdas = [], []
                try:
                    call = self.instantiate(target, args, target.fdef, cache=False)
                finally:
                    self.new_funcs, self.new_lambdas = saved
                if isinstance(call.ty, NumTy):
                    from .units import preferred_unit
                    res = self.U.norm(call.ty.dim / (args[i].ty.dim ** order))
                    pds = [self.U.norm(a.ty.dim) for a in args]
                    if res.concrete and all(d.concrete for d in pds):
                        if res.const.dimensionless and all(d.const.dimensionless for d in pds):
                            return ""
                        parts = [preferred_unit(res.const).name or "no units"]
                        parts += [f"for {p.name} in {preferred_unit(d.const).name or 'plain numbers'}"
                                  for p, d in zip(target.fdef.params, pds)]
                        return ", ".join(parts)
            except FermiumError:
                pass
        try:
            args = [I.IConst(1, NumTy(DExpr.fresh(p.name))) for p in info.fdef.params]
            saved = (self.new_funcs, self.new_lambdas, len(self.U.subst))
            self.new_funcs, self.new_lambdas = [], []
            try:
                call = self.instantiate(info, args, info.fdef, cache=False)
            finally:
                self.new_funcs, self.new_lambdas = saved[0], saved[1]
            from .units import preferred_unit
            if not isinstance(call.ty, NumTy):
                return ""
            res = self.U.norm(call.ty.dim)
            parts = []
            if res.terms:
                return ""
            parts.append(preferred_unit(res.const).name or "no units")
            all_plain = res.const.dimensionless
            for p, a in zip(info.fdef.params, args):
                d = self.U.norm(a.ty.dim)
                if not d.terms:
                    all_plain = all_plain and d.const.dimensionless
                    parts.append(f"for {p.name} in {preferred_unit(d.const).name or 'plain numbers'}")
            if all_plain:
                return ""
            return ", ".join(parts)
        except FermiumError:
            return ""

    def s_If(self, s, ctx):
        c = self.cond(s.cond, ctx)
        ctx.branch = getattr(ctx, "branch", 0) + 1
        try:
            then = self.block(s.then, ctx)
            other = self.block(s.other, ctx) if s.other else []
        finally:
            ctx.branch -= 1
        return I.SIf(c, then, other)

    def cond(self, e, ctx):
        c = self.expr(e, ctx)
        if not isinstance(c.ty, BoolTy):
            raise self.err(f"a condition must be true or false, but this is {type_desc(c.ty, self.U)}", e,
                           hint="compare values, e.g.  if x > 0 m")
        return c

    def s_While(self, s, ctx):
        c = self.cond(s.cond, ctx)
        ctx.loop += 1
        body = self.block(s.body, ctx)
        ctx.loop -= 1
        return I.SWhile(c, body)

    def loop_var(self, name, ty, ctx, node):
        b, _ = ctx.scope.lookup(name)
        if isinstance(b, I.Sym) and b.func is ctx.func and type(b.ty) is type(ty):
            if isinstance(ty, NumTy) and self.U.unify(b.ty.dim, ty.dim):
                return b
        sym = self.new_sym(name, ty, ctx)
        ctx.scope.names[name] = sym
        return sym

    def s_For(self, s, ctx):
        lo = self.expr(s.lo, ctx)
        hi = self.expr(s.hi, ctx)
        self.need_num(lo, s.lo, "the start of the range")
        self.need_num(hi, s.hi, "the end of the range")
        self.unify_or(lo.ty.dim, hi.ty.dim, lambda: f"the range goes from {self.desc(lo.ty.dim)} to "
                      f"{self.desc(hi.ty.dim)}; both ends need the same units", s)
        if s.step is not None:
            st = self.expr(s.step, ctx)
            self.need_num(st, s.step, "the step")
            self.unify_or(lo.ty.dim, st.ty.dim, lambda: f"the step is {self.desc(st.ty.dim)} but the range is "
                          f"{self.desc(lo.ty.dim)}", s.step)
        else:
            if not self.U.unify(lo.ty.dim, DIMLESS):
                raise self.err(f"this range is {self.desc(lo.ty.dim)}, so it needs a step with units", s,
                               hint="add e.g.  step 0.1 s")
            st = I.IConst(1, NumTy(DIMLESS))
        sym = self.loop_var(s.var, NumTy(lo.ty.dim), ctx, s)
        sym.hint = lo.hint
        sym.sf = None
        sym.assigned = True
        ctx.loop += 1
        body = self.block(s.body, ctx)
        ctx.loop -= 1
        return I.SFor(sym, lo, hi, st, body)

    def s_ForIn(self, s, ctx):
        lst = self.expr(s.iterable, ctx, allow_func=True)
        if isinstance(lst, SolRef):
            lst = self.sol_values(lst.view)
        if isinstance(lst.ty, TextListTy):
            sym = self.loop_var(s.var, STR, ctx, s)
            sym.assigned = True
            ctx.loop += 1
            body = self.block(s.body, ctx)
            ctx.loop -= 1
            return I.SForIn(sym, lst, body)
        if not isinstance(lst.ty, ListTy):
            raise self.err(f"can't loop over {type_desc(lst.ty, self.U)}; 'for x in ...' needs a list", s.iterable,
                           hint="to count, write  for i from 1 to 10")
        sym = self.loop_var(s.var, NumTy(lst.ty.dim), ctx, s)
        sym.hint = lst.hint
        sym.assigned = True
        ctx.loop += 1
        body = self.block(s.body, ctx)
        ctx.loop -= 1
        return I.SForIn(sym, lst, body)

    def s_Return(self, s, ctx):
        if ctx.is_main:
            raise self.err("return can only be used inside a function", s)
        v = self.expr(s.value, ctx) if s.value is not None else None
        if v is None:
            raise self.err("return needs a value", s)
        ctx.ret_types.append(v)
        return I.SReturn(v)

    def s_Break(self, s, ctx):
        if ctx.loop == 0:
            raise self.err("break can only be used inside a loop", s)
        return I.SBreak()

    def s_Continue(self, s, ctx):
        if ctx.loop == 0:
            raise self.err("continue can only be used inside a loop", s)
        return I.SContinue()

    def s_Assert(self, s, ctx):
        c = self.cond(s.cond, ctx)
        msg = s.message or f"check failed: {C.to_source(s.cond)}"
        return I.SAssert(c, self.text(f"line {s.line}: {msg}"))

    # ============================================================ expressions
    def need_num(self, v, node, what="this value"):
        if isinstance(v, (FuncRef, SolRef)) or not isinstance(v.ty, NumTy):
            got = "a function" if isinstance(v, (FuncRef, SolRef)) else type_desc(v.ty, self.U)
            raise self.err(f"{what} must be a number, but it is {got}", node)

    def need_numlike(self, v, node, what="this value", allow_vec=False):
        if allow_vec and isinstance(v, I.Expr) and isinstance(v.ty, VecTy):
            return
        if isinstance(v, (FuncRef, SolRef)) or not isinstance(v.ty, (NumTy, ListTy)):
            got = "a function" if isinstance(v, FuncRef) else ("an ODE solution" if isinstance(v, SolRef)
                                                              else type_desc(v.ty, self.U))
            hint = None
            if isinstance(v, FuncRef):
                hint = f"call it with an argument, e.g. {v.info.display_name}(x)"
            if isinstance(v, SolRef):
                hint = f"use {v.view.name}({v.view.tname}) for its value at a time"
            raise self.err(f"{what} must be a number, but it is {got}", node, hint)

    def expr(self, e, ctx, allow_func=False):
        m = getattr(self, "e_" + type(e).__name__, None)
        if m is None:
            raise self.err(f"this kind of expression ({type(e).__name__}) isn't supported here", e)
        r = m(e, ctx)
        if isinstance(r, (FuncRef, SolRef)):
            if not allow_func:
                if isinstance(r, FuncRef):
                    raise self.err(f"{r.info.display_name} is a function; give it an argument, "
                                   f"like {r.info.display_name}(x)", e)
                v = r.view
                raise self.err(f"{v.name} is the solution of an ODE (a function of {v.tname}); "
                               f"use {v.name}({v.tname}) for its value at a time", e,
                               hint=f"e.g. {v.name}(1 s), or values({v.name}) for all computed values")
            return r
        r.line = e.line
        return r

    def e_Num(self, e, ctx):
        if e.value == 0 and e.digit:
            r = I.IConst(0, NumTy(DExpr.fresh("0")))   # a plain 0 fits any units
        else:
            r = I.IConst(e.value, NumTy(DIMLESS))
        r.sf = e.sigfigs
        r.direct = True
        return r

    def e_Str(self, e, ctx):
        r = I.IStr(e.value, STR)
        r.text_id = self.text(e.value)
        return r

    def e_Bool(self, e, ctx):
        return I.IBool(e.value, BOOL)

    def e_Quantity(self, e, ctx):
        u = self.resolve_unit(e.unit)
        v = self.expr(e.value, ctx)
        self.need_numlike(v, e.value, allow_vec=True)
        if isinstance(v.ty, VecTy):
            if u.affine:
                raise self.err("°C/°F can't be used for vectors", e)
            if not isinstance(e.value, A.VecLit):
                vd = self.U.norm(v.ty.dim)
                if vd.concrete and not vd.const.dimensionless:
                    raise self.err(f"this already has units ({self.desc(v.ty.dim)})", e)
            self.U.unify(v.ty.dim, DIMLESS)
            r = I.IBin("*", v, I.IConst(u.factor, NumTy(DIMLESS)), VecTy(DExpr.of(u.dim), v.ty.n))
            r.hint, r.sf, r.direct = u, v.sf, isinstance(e.value, A.VecLit)
            return r
        if not isinstance(e.value, A.Num):
            vd = self.U.norm(v.ty.dim)
            if vd.concrete and not vd.const.dimensionless:
                raise self.err(f"this already has units ({self.desc(v.ty.dim)}), so [{e.unit.text}] would "
                               f"multiply them", e, hint=f"to show it in {e.unit.text}, write:  ... in {e.unit.text}")
            self.U.unify(v.ty.dim, DIMLESS)
        dim = DExpr.of(u.dim)
        ty = NumTy(dim) if isinstance(v.ty, NumTy) else ListTy(dim)
        if isinstance(v, I.IConst):
            r = I.IConst(v.value * u.factor + u.offset, ty)
        else:
            r = I.IBin("*", v, I.IConst(u.factor, NumTy(DIMLESS)), ty) if u.factor != 1 else v
            if u.offset:
                r = I.IBin("+", r, I.IConst(u.offset, NumTy(DIMLESS)), ty)
            if r is v:
                r = I.IBin("*", v, I.IConst(1.0, NumTy(DIMLESS)), ty)
        r.ty = ty
        r.hint = u
        r.sf = v.sf
        r.direct = isinstance(e.value, A.Num)
        return r

    def lookup(self, name, ctx, node):
        b, scope = ctx.scope.lookup(name)
        return b, scope

    def e_Name(self, e, ctx):
        name = e.name
        b, scope = self.lookup(name, ctx, e)
        if b is None:
            raise self.undefined(name, e, ctx)
        return self.use_binding(b, name, e, ctx)

    def use_binding(self, b, name, e, ctx):
        if isinstance(b, I.Sym):
            return self.var_ref(b, ctx, e)
        if isinstance(b, ConstInfo):
            if name == "∞":
                r = I.IConst(math.inf, NumTy(DExpr.fresh("∞")))
            else:
                r = I.IConst(b.value, NumTy(b.unit.dim))
                r.hint = b.unit if b.unit.name not in ("1",) else None
            return r
        if isinstance(b, FuncInfo):
            return FuncRef(b)
        if isinstance(b, SolView):
            self.var_ref(b.sol_sym, ctx, e)   # marks capture/global as needed
            return SolRef(b)
        raise self.err(f"can't use {name} here", e)

    def var_ref(self, sym, ctx, node):
        """Reference a variable, marking it global or captured when used from another function."""
        if ctx.lam is not None:
            lam = ctx.lam
            if sym in lam.locals or sym in lam.params or sym in lam.state or sym in lam.param_syms \
                    or sym in lam.col_syms:
                return I.IVar(sym)
            # symbol from an enclosing function
            if sym.storage in ("global", "arena"):
                return self._ivar(sym)
            if sym.func is not None and getattr(sym.func, "is_main", False) or self._is_main_sym(sym):
                sym.storage = "global"
                return self._ivar(sym)
            if sym not in lam.captures:
                if not isinstance(sym.ty, (NumTy, BoolTy)):
                    raise self.err(f"{sym.name} can't be used inside this integral/equation (only numbers can be "
                                   f"captured from a function)", node)
                lam.captures.append(sym)
            return self._ivar(sym)
        if sym.func is not ctx.func and sym.storage == "local":
            if self._is_main_sym(sym):
                sym.storage = "global"
            else:
                raise self.err(f"{sym.name} belongs to another function and can't be used here", node)
        return self._ivar(sym)

    def _ivar(self, sym):
        r = I.IVar(sym)
        r.sf = sym.sf
        r.hint = sym.hint
        r.direct = sym.direct
        return r

    def _is_main_sym(self, sym):
        return sym.func is not None and sym.func.name.startswith("main")

    def undefined(self, name, e, ctx):
        # collect known names for suggestions
        known = set()
        s = ctx.scope
        while s is not None:
            known |= set(s.names)
            s = s.parent
        hint = None
        from .lexer import canonical_name, KEYWORDS
        # LT -> L T ?  (also omegat -> ω t)
        for i in range(1, len(name)):
            a, b = canonical_name(name[:i]), canonical_name(name[i:])
            if a in known and b in known:
                hint = f"did you mean {a} {b} ({a} times {b})? Fermium reads {name} as one name; put a space between"
                break
        if hint is None and lookup_unit(name) is not None:
            hint = f"{name} is a unit; units go right after a number, like 1 {name}, or in brackets [{name}]"
        if hint is None:
            cands = [k for k in known if not k.startswith("__")] + sorted(KEYWORDS) + sorted(BUILTINS)
            close = get_close_matches(name, cands, n=1, cutoff=0.7)
            if close:
                hint = f"did you mean {close[0]}?"
        if hint is None and getattr(self, "_after_number", False):
            return self.err(f"'{name}' isn't a unit Fermium knows (or a variable you've defined)", e,
                            hint="see the list of units in docs/reference.md §15")
        if hint is None and getattr(self, "_calling", False):
            hint = f"define the function first, e.g.  {name}(x) = 2 x"
        if name == "%":
            return self.err("% is the percent unit in Fermium (5 % = 0.05)", e,
                            hint="for the remainder of a division use mod(n, 2)")
        if name in BUILTINS:
            return self.err(f"{name} is a built-in function; call it with arguments like {name}(x)", e)
        return self.err(f"{name} isn't defined", e, hint=hint or f"give it a value first, e.g.  {name} = 1.0 m")

    # ------------------------------------------------------------ arithmetic
    def _leibniz(self, e, ctx):
        """dx/dt written as a fraction: a derivative when dx and dt aren't variables but x is a function."""
        from .lexer import canonical_name
        right, args = e.right, None
        if isinstance(right, A.Call) and isinstance(right.func, A.Name):      # dx/dt(2 s)
            right, args = right.func, right.args
        if e.op == "/" and isinstance(e.left, A.Name) and isinstance(right, A.Name) and not e.left.paren:
            ln, rn = e.left.name, right.name
            if len(ln) > 1 and len(rn) > 1 and ln.startswith("d") and rn.startswith("d"):
                if ctx.scope.lookup(ln)[0] is None and ctx.scope.lookup(rn)[0] is None:
                    x, t = canonical_name(ln[1:]), canonical_name(rn[1:])
                    if isinstance(ctx.scope.lookup(x)[0], (FuncInfo, SolView)):
                        d = A.Deriv(t, 1, A.Name(x).at(e.left)).at(e)
                        return A.Call(d, args).at(e) if args is not None else d
        return None

    def e_BinOp(self, e, ctx):
        if e.op == "^":
            return self.power(e, ctx)
        lz = self._leibniz(e, ctx)
        if lz is not None:
            return self.expr(lz, ctx, allow_func=True)
        a = self.expr(e.left, ctx, allow_func=e.implicit)
        if isinstance(a, (FuncRef, SolRef)):
            if e.implicit and e.right.paren:
                return self.e_Call(A.Call(e.left, [e.right]).at(e), ctx)
            self.need_numlike(a, e.left, allow_vec=True)
        self._after_number = e.implicit and isinstance(e.left, (A.Num, A.Quantity)) and isinstance(e.right, A.Name)
        try:
            b = self.expr(e.right, ctx)
        finally:
            self._after_number = False
        self.need_numlike(a, e.left, allow_vec=True)
        self.need_numlike(b, e.right, allow_vec=True)
        return self.arith(e.op, a, b, e)

    def arith(self, op, a, b, e):
        if isinstance(a.ty, VecTy) or isinstance(b.ty, VecTy):
            return self.vec_arith(op, a, b, e)
        if op == "×":
            op = "*"
        is_list = isinstance(a.ty, ListTy) or isinstance(b.ty, ListTy)
        mk = ListTy if is_list else NumTy
        if op in ("+", "-"):
            if not self.U.unify(a.ty.dim, b.ty.dim):
                da, db = self.desc(a.ty.dim), self.desc(b.ty.dim)
                if op == "+":
                    msg = f"can't add {da} to {db}"
                else:
                    msg = f"can't subtract {db} from {da}"
                raise self.err(msg, e, hint=self.mismatch_hint(a, b))
            r = I.IBin(op, a, b, mk(a.ty.dim))
            aff_a = a.hint is not None and a.hint.affine
            aff_b = b.hint is not None and b.hint.affine
            if op == "+" and aff_a and aff_b:
                raise self.err(f"can't add two absolute temperatures ({a.hint.name} + {b.hint.name})", e,
                               hint="to add a temperature change, write it in K, e.g. 20 °C + 5 K")
            if op == "-" and aff_a and aff_b:
                r.hint = None      # the difference of two temperatures is a difference: shown in K
            elif aff_b and op == "-":
                raise self.err(f"can't subtract an absolute temperature ({b.hint.name}) from this", e,
                               hint="write temperature changes in K")
            else:
                r.hint = a.hint if a.hint is not None else b.hint
        elif op == "*":
            r = I.IBin("*", a, b, mk(a.ty.dim * b.ty.dim))
            r.hint = self._keep_hint(a, b)
        elif op == "/":
            r = I.IBin("/", a, b, mk(a.ty.dim / b.ty.dim))
            r.hint = a.hint if self._dimless(b) and a.hint is not None and b.hint is None else None
        else:
            raise self.err(f"unknown operator {op}", e)
        r.sf = self._minsf(a, b)
        return r

    def vec_arith(self, op, a, b, e):
        va, vb = isinstance(a.ty, VecTy), isinstance(b.ty, VecTy)
        if isinstance(a.ty, ListTy) or isinstance(b.ty, ListTy):
            raise self.err("can't mix vectors and lists in arithmetic", e)
        if op in ("+", "-"):
            if not (va and vb):
                raise self.err(f"can't {'add' if op == '+' else 'subtract'} a vector and a single number", e,
                               hint="both sides must be vectors, e.g. <1, 2> m + <3, 4> m")
            if a.ty.n != b.ty.n:
                raise self.err(f"can't {'add' if op == '+' else 'subtract'} a {a.ty.n}-vector and a "
                               f"{b.ty.n}-vector", e)
            if not self.U.unify(a.ty.dim, b.ty.dim):
                da, db = self.desc(a.ty.dim), self.desc(b.ty.dim)
                raise self.err(f"can't {'add' if op == '+' else 'subtract'} vectors of {da} and {db}", e,
                               hint=self.mismatch_hint(a, b))
            r = I.IBin(op, a, b, VecTy(a.ty.dim, a.ty.n))
            r.hint = a.hint or b.hint
        elif op == "*" and va and vb:
            if a.ty.n != b.ty.n:
                raise self.err(f"can't take the dot product of a {a.ty.n}-vector and a {b.ty.n}-vector", e)
            r = I.IBuiltin("vdot", [a, b], NumTy(a.ty.dim * b.ty.dim))
        elif op == "×" and va and vb:
            if a.ty.n != b.ty.n:
                raise self.err(f"can't take the cross product of a {a.ty.n}-vector and a {b.ty.n}-vector", e)
            ty = VecTy(a.ty.dim * b.ty.dim, 3) if a.ty.n == 3 else NumTy(a.ty.dim * b.ty.dim)
            r = I.IBuiltin("cross", [a, b], ty)
        elif op in ("*", "×"):
            if op == "×":
                raise self.err("× between a vector and a number: use * (or a space) to scale a vector", e)
            v, k = (a, b) if va else (b, a)
            r = I.IBin("*", a, b, VecTy(a.ty.dim * b.ty.dim, v.ty.n))
            r.hint = v.hint if self._dimless(k) else None
        elif op == "/":
            if vb:
                raise self.err("can't divide by a vector", e)
            r = I.IBin("/", a, b, VecTy(a.ty.dim / b.ty.dim, a.ty.n))
            r.hint = a.hint if self._dimless(b) else None
        else:
            raise self.err(f"unknown operator {op}", e)
        r.sf = self._minsf(a, b)
        return r

    def e_VecLit(self, e, ctx):
        items = [self.expr(x, ctx) for x in e.items]
        dim = DExpr.fresh("vec")
        for it, node in zip(items, e.items):
            self.need_num(it, node, "a vector component")
            self.unify_or(dim, it.ty.dim, lambda: f"all components of a vector need the same units; this one is "
                          f"{self.desc(it.ty.dim)} but the others are {self.desc(dim)}", node)
        r = I.IVec(items, VecTy(dim, len(items)))
        r.hint = next((it.hint for it in items if it.hint is not None), None)
        r.sf = self._minsf(*items)
        return r

    def mismatch_hint(self, a, b):
        return "both sides of + and - must have the same units"

    def _dimless(self, v):
        d = self.U.norm(v.ty.dim)
        return d.concrete and d.const.dimensionless

    def _keep_hint(self, a, b):
        """Scaling by a plain number keeps the unit the user wrote (2 × 3 eV = 6 eV)."""
        if a.hint is not None and self._dimless(b) and b.hint is None:
            return a.hint
        if b.hint is not None and self._dimless(a) and a.hint is None:
            return b.hint
        return None

    def _minsf(self, *vs):
        s = [v.sf for v in vs if getattr(v, "sf", None) is not None]
        return min(s) if s else None

    def const_value(self, e):
        """Evaluate a compile-time constant exponent; returns Fraction or None."""
        if isinstance(e, A.Num):
            return Fraction(e.value).limit_denominator(10000)
        if isinstance(e, A.Neg):
            v = self.const_value(e.operand)
            return -v if v is not None else None
        if isinstance(e, A.BinOp) and e.op in "+-*/":
            a, b = self.const_value(e.left), self.const_value(e.right)
            if a is None or b is None:
                return None
            if e.op == "/" and b == 0:
                return None
            return {"+": a + b, "-": a - b, "*": a * b, "/": a / b if b else None}[e.op]
        return None

    def power(self, e, ctx):
        pconst = self.const_value(e.right)
        if isinstance(e.left, A.Name) and e.left.name == "e" and (pconst is None or pconst <= 0):
            b, _ = ctx.scope.lookup("e")
            if isinstance(b, ConstInfo):
                raise self.err("e is the elementary charge (1.602×10⁻¹⁹ C) in Fermium", e.left,
                               hint="for the exponential function write exp(x)")
        a = self.expr(e.left, ctx)
        self.need_numlike(a, e.left, "the base of a power")
        p = self.const_value(e.right)
        mk = ListTy if isinstance(a.ty, ListTy) else NumTy
        if p is not None:
            r = I.IPowC(a, float(p), mk(a.ty.dim ** p))
            r.sf = a.sf
            if a.hint is not None and p == 1:
                r.hint = a.hint
            return r
        b = self.expr(e.right, ctx)
        self.need_num(b, e.right, "the exponent")
        if not self.U.unify(b.ty.dim, DIMLESS):
            raise self.err(f"an exponent must be a plain number, but this is {self.desc(b.ty.dim)}", e.right)
        if not self.U.unify(a.ty.dim, DIMLESS):
            raise self.err(f"can't raise {self.desc(a.ty.dim)} to a power that isn't a fixed number", e,
                           hint="with units, the exponent must be a number written in the program, like x^2 or x^(1/3)")
        r = I.IPow(a, b, mk(DIMLESS))
        r.sf = self._minsf(a, b)
        return r

    def e_Neg(self, e, ctx):
        q = e.operand
        if isinstance(q, A.Quantity) and isinstance(q.value, A.Num) and not q.paren:
            u = self.resolve_unit(q.unit)
            if u.affine:        # -40 °C is minus forty degrees, not -(313.15 K)
                neg = A.Quantity(A.Num(-q.value.value, q.value.sigfigs, q.value.digit).at(q.value), q.unit,
                                 q.bracket).at(e)
                return self.e_Quantity(neg, ctx)
        a = self.expr(e.operand, ctx)
        self.need_numlike(a, e.operand, allow_vec=True)
        if a.hint is not None and a.hint.affine:
            raise self.err(f"can't negate an absolute temperature ({a.hint.name})", e,
                           hint="write the negative number directly, like -5 °C, or use K")
        r = I.INeg(a)
        r.hint, r.sf, r.direct = a.hint, a.sf, a.direct
        return r

    def e_Compare(self, e, ctx):
        a = self.expr(e.left, ctx)
        b = self.expr(e.right, ctx)
        if isinstance(a.ty, BoolTy) and isinstance(b.ty, BoolTy) and e.op in ("==", "!="):
            return I.ICmp(e.op, a, b, BOOL)
        self.need_num(a, e.left, "each side of a comparison")
        self.need_num(b, e.right, "each side of a comparison")
        if not self.U.unify(a.ty.dim, b.ty.dim):
            raise self.err(f"can't compare {self.desc(a.ty.dim)} with {self.desc(b.ty.dim)}", e)
        return I.ICmp(e.op, a, b, BOOL)

    def e_Logic(self, e, ctx):
        a = self.cond(e.left, ctx)
        b = self.cond(e.right, ctx)
        return I.ILogic(e.op, a, b, BOOL)

    def e_Not(self, e, ctx):
        return I.INot(self.cond(e.operand, ctx), BOOL)

    def e_Sqrt(self, e, ctx):
        a = self.expr(e.operand, ctx)
        self.need_numlike(a, e.operand, "the value under the root")
        mk = ListTy if isinstance(a.ty, ListTy) else NumTy
        r = I.IPowC(a, 0.5 if e.root == 2 else 1 / 3, mk(a.ty.dim ** Fraction(1, e.root)))
        r.sf = a.sf
        return r

    def e_Abs(self, e, ctx):
        a = self.expr(e.operand, ctx)
        self.need_numlike(a, e.operand, allow_vec=True)
        if isinstance(a.ty, VecTy):
            r = I.IBuiltin("norm", [a], NumTy(a.ty.dim))
            r.hint, r.sf = a.hint, a.sf
            return r
        r = I.IBuiltin("abs", [a], a.ty)
        r.hint, r.sf = a.hint, a.sf
        return r

    def e_IfExpr(self, e, ctx):
        c = self.cond(e.cond, ctx)
        a = self.expr(e.then, ctx)
        b = self.expr(e.other, ctx)
        if type(a.ty) is not type(b.ty):
            raise self.err("both branches of an if-expression must give the same kind of value", e)
        if isinstance(a.ty, VecTy) and a.ty.n != b.ty.n:
            raise self.err("both branches of an if-expression must give vectors of the same length", e)
        if isinstance(a.ty, (NumTy, ListTy, VecTy)):
            self.unify_or(a.ty.dim, b.ty.dim, lambda: f"the two branches give {self.desc(a.ty.dim)} and "
                          f"{self.desc(b.ty.dim)}; they must match", e)
        r = I.IIf(c, a, b, a.ty)
        r.hint = a.hint or b.hint
        r.sf = self._minsf(a, b)
        return r

    def e_Convert(self, e, ctx):
        v = self.expr(e.value, ctx)
        self.need_numlike(v, e.value, "the value to convert", allow_vec=True)
        u = self.resolve_unit(e.unit)
        if not self.U.unify(v.ty.dim, u.dim):
            raise self.err(f"can't show {self.desc(v.ty.dim)} in {u.name} ({dim_name(u.dim)})", e,
                           hint="the units you convert to must measure the same kind of quantity")
        v.hint = u
        v.direct = False
        return v

    def e_Digits(self, e, ctx):
        v = self.expr(e.value, ctx)
        if e.digits < 1 or e.digits > 17:
            raise self.err("the number of digits must be between 1 and 17", e)
        v.sf = e.digits
        v.direct = True
        return v

    def e_Where(self, e, ctx):
        scope = Scope(ctx.scope)
        c2 = ctx.child(scope)
        binds = []
        for name, val in e.bindings:
            v = self.expr(val, c2)
            sym = self.new_sym(name, v.ty, c2)
            sym.sf, sym.hint, sym.direct = v.sf, v.hint, v.direct
            scope.names[name] = sym
            binds.append((sym, v))
        body = self.expr(e.value, c2)
        r = I.ILet(binds, body)
        r.hint, r.sf, r.direct = body.hint, body.sf, False
        return r

    def e_Uncertain(self, e, ctx):
        raise self.err("uncertainties (±) are planned for a future version of Fermium", e)

    def e_ListLit(self, e, ctx):
        items = [self.expr(x, ctx) for x in e.items]
        if items and all(isinstance(it.ty, StrTy) for it in items):
            return I.IList(items, TextListTy())
        if any(isinstance(it.ty, StrTy) for it in items):
            raise self.err("a list can hold numbers or text, but not both", e)
        dim = DExpr.fresh("list")
        for it, node in zip(items, e.items):
            self.need_num(it, node, "a list element")
            self.unify_or(dim, it.ty.dim, lambda: f"all elements of a list need the same units; this one is "
                          f"{self.desc(it.ty.dim)} but earlier ones are {self.desc(dim)}", node)
        r = I.IList(items, ListTy(dim))
        r.hint = items[0].hint if items else None
        r.sf = self._minsf(*items) if items else None
        r.direct = bool(items) and all(getattr(it, "direct", False) for it in items)
        return r

    def e_Load(self, e, ctx):
        path = e.path
        full = path if os.path.isabs(path) else os.path.join(self.base_dir, path)
        if not os.path.exists(full):
            raise self.err(f"can't find the file '{path}'", e,
                           hint=f"looked in {os.path.abspath(os.path.dirname(full) or '.')}")
        cols = read_csv_header(full, e)
        info = {"path": path, "full": os.path.abspath(full), "columns": cols}
        self.tables.loads.append(info)
        return I.ILoad(len(self.tables.loads) - 1, DataTy(info))

    def e_Field(self, e, ctx):
        t = self.expr(e.target, ctx, allow_func=True)
        if isinstance(t, SolRef):
            v = t.view
            if e.name in ("x", "y", "z") and v.n > 1:
                k = "xyz".index(e.name)
                if k >= v.n:
                    raise self.err(f"{v.name} is a {v.n}-vector, so it has no .{e.name}", e)
                nv = SolView(v.sol_sym, v.comp + k, v.top + k, v.dim, v.tdim, v.tname, f"{v.name}.{e.name}")
                nv.n, nv.stride = 1, v.stride
                nv.hint, nv.thint, nv.sf = getattr(v, "hint", None), getattr(v, "thint", None), getattr(v, "sf", None)
                return SolRef(nv)
            if e.name in ("t", "time", "times"):
                return I.ISolList(v.sol_sym and self.var_ref(v.sol_sym, ctx, e), v.comp, "t", ListTy(v.tdim))
            if e.name in ("values", "v"):
                return self.sol_values(v)
        if isinstance(t, I.Expr) and isinstance(t.ty, VecTy):
            if e.name not in ("x", "y", "z"):
                raise self.err(f"a vector's components are .x, .y and .z (not .{e.name})", e)
            return self.vec_elem(t, "xyz".index(e.name), e)
        if not isinstance(t, I.Expr) or not isinstance(t.ty, DataTy):
            raise self.err(f"'.{e.name}' only works on data loaded from a file (like data.{e.name})", e)
        cols = t.ty.info["columns"]
        for i, c in enumerate(cols):
            if c["name"] == e.name:
                r = I.IColumn(t, i, ListTy(c["unit"].dim))
                r.hint = c["unit"] if c["unit"].name != "1" else None
                return r
        names = ", ".join(c["name"] for c in cols)
        raise self.err(f"the data has no column called {e.name} (columns: {names})", e)

    def sol_values(self, v: SolView):
        if v.n > 1:
            raise self.err(f"{v.name} is a vector; use its components, like {v.name}.x", None)
        s = self._ivar(v.sol_sym)
        if self.cur_ctx is not None:
            s = self.var_ref(v.sol_sym, self.cur_ctx, None)
        if v.comp > v.top:
            return I.ISolList(s, v.top, "dy", ListTy(v.dim))
        return I.ISolList(s, v.comp, "y", ListTy(v.dim))

    def index_expr(self, idx_ast, target, ctx):
        if isinstance(idx_ast, A.End):
            return I.IBuiltin("len", [target], NumTy(DIMLESS))
        idx = self._with_end(idx_ast, target, ctx)
        self.need_num(idx, idx_ast, "a list index")
        if isinstance(idx, I.IConst) and idx.value != int(idx.value):
            raise self.err(f"a list index must be a whole number (1, 2, 3, ...), not {idx.value:g}", idx_ast)
        if not self.U.unify(idx.ty.dim, DIMLESS):
            raise self.err(f"a list index must be a plain number (1, 2, 3, ...), not {self.desc(idx.ty.dim)}",
                           idx_ast)
        return idx

    def _with_end(self, idx_ast, target, ctx):
        if any(isinstance(n, A.End) for n in A.walk(idx_ast)):
            scope = Scope(ctx.scope)
            c2 = ctx.child(scope)
            sym = self.new_sym("end", NumTy(DIMLESS), c2)
            scope.names["end"] = sym

            def repl(n):
                if isinstance(n, A.End):
                    return A.Name("end").at(n)
                return C.map_children(n, repl)
            body = self.expr(repl(idx_ast), c2)
            return I.ILet([(sym, I.IBuiltin("len", [target], NumTy(DIMLESS)))], body)
        return self.expr(idx_ast, ctx)

    def vec_elem(self, t, k, node):
        if not 0 <= k < t.ty.n:
            raise self.err(f"this vector has {t.ty.n} components, so there is no component {k + 1}", node)
        r = I.IVecElem(t, k, NumTy(t.ty.dim))
        r.hint, r.sf = t.hint, t.sf
        return r

    def e_Index(self, e, ctx):
        t = self.expr(e.target, ctx, allow_func=True)
        if isinstance(t, I.Expr) and isinstance(t.ty, VecTy):
            idx = self.index_expr(e.index, I.IConst(t.ty.n, NumTy(DIMLESS)), ctx) \
                if not isinstance(e.index, A.End) else I.IConst(t.ty.n, NumTy(DIMLESS))
            if not isinstance(idx, I.IConst):
                raise self.err("a vector's component must be picked with a fixed number, like v[1], or v.x", e.index)
            return self.vec_elem(t, int(idx.value) - 1, e.index)
        if isinstance(t, SolRef):
            t = self.sol_values(t.view)
        if isinstance(t, I.Expr) and isinstance(t.ty, TextListTy):
            return I.IIndex(t, self.index_expr(e.index, t, ctx), STR, e.line)
        if isinstance(t, FuncRef) or not isinstance(t.ty, ListTy):
            raise self.err("only lists can be indexed with [...]", e.target,
                           hint="to call a function use parentheses: f(x)")
        idx = self.index_expr(e.index, t, ctx)
        r = I.IIndex(t, idx, NumTy(t.ty.dim), e.line)
        r.hint = t.hint
        r.sf = t.sf
        return r

    def e_End(self, e, ctx):
        raise self.err("'end' can only be used inside [...] to mean the last element", e)

    # ------------------------------------------------------------ calls
    def e_Call(self, e, ctx):
        f = e.func
        if isinstance(f, A.Name):
            b, _ = self.lookup(f.name, ctx, f)
            if b is None:
                if f.name in BUILTINS:
                    return self.builtin(f.name, e, ctx)
                self._calling = True
                try:
                    raise self.undefined(f.name, f, ctx)
                finally:
                    self._calling = False
            if isinstance(b, FuncInfo):
                args = [self.expr(a, ctx) for a in e.args]
                return self.call_user(b, args, e)
            if isinstance(b, SolView):
                return self.sol_eval(b, e, ctx)
            if isinstance(b, (I.Sym, ConstInfo)):
                v = self.use_binding(b, f.name, f, ctx)
                if isinstance(v, I.Expr) and isinstance(v.ty, NumTy) and len(e.args) == 1:
                    # k(x + 1) means k × (x + 1)
                    arg = self.expr(e.args[0], ctx)
                    self.need_numlike(arg, e.args[0])
                    return self.arith("*", v, arg, e)
                raise self.err(f"{f.name} isn't a function, so it can't be called with ( )", f)
        if isinstance(f, A.Prime):
            target = self.expr(f, ctx, allow_func=True)
            if isinstance(target, FuncRef):
                args = [self.expr(a, ctx) for a in e.args]
                return self.call_user(target.info, args, e)
            if isinstance(target, SolRef):
                return self.sol_eval(target.view, e, ctx)
            raise self.err("only functions can be called", f)
        if isinstance(f, (A.Deriv, A.Field)):
            target = self.expr(f, ctx, allow_func=True)
            if isinstance(target, FuncRef):
                args = [self.expr(a, ctx) for a in e.args]
                return self.call_user(target.info, args, e)
            if isinstance(target, SolRef):
                return self.sol_eval(target.view, e, ctx)
        if isinstance(f, (A.Num, A.Quantity)) or f.paren:
            v = self.expr(f, ctx)
            if len(e.args) == 1:
                arg = self.expr(e.args[0], ctx)
                self.need_numlike(v, f)
                self.need_numlike(arg, e.args[0])
                return self.arith("*", v, arg, e)
        raise self.err("this can't be called like a function", e)

    def sol_eval(self, view: SolView, e, ctx):
        if len(e.args) != 1:
            raise self.err(f"{view.name} takes one argument ({view.tname})", e)
        t = self.expr(e.args[0], ctx)
        self.need_num(t, e.args[0])
        self.unify_or(t.ty.dim, view.tdim, lambda: f"{view.name} is a function of {view.tname}, which is "
                      f"{self.desc(view.tdim)}, not {self.desc(t.ty.dim)}", e.args[0])
        sol = self.var_ref(view.sol_sym, ctx, e)
        r = self._sol_eval_node(view, sol, t, e)
        tf = I.IConst(0, NumTy(view.tdim))
        tf.hint = getattr(view, "thint", None)
        tfmt = self.fmt(tf)
        for node in ([r] + list(getattr(r, "items", []))):
            node.tfmt = tfmt
        r.sf = self._minsf(t, view) if getattr(view, "sf", None) is not None else t.sf
        if getattr(view, "hint", None) is not None:
            r.hint = view.hint
        return r

    def _sol_eval_node(self, view, sol, t, e):
        if view.n > 1:
            comps = []
            for k in range(view.n):
                sub = SolView(view.sol_sym, view.comp + k, view.top + k, view.dim, view.tdim, view.tname, view.name)
                comps.append(self._sol_eval_node(sub, sol, t, e))
            return I.IVec(comps, VecTy(view.dim, view.n))
        if view.comp <= view.top:
            r = I.ISolEval(sol, view.comp, t, False, NumTy(view.dim))
        elif view.comp == view.top + view.stride:
            r = I.ISolEval(sol, view.top, t, True, NumTy(view.dim))
        else:
            raise self.err(f"can't take that many derivatives of the solution {view.name}", e)
        return r

    def call_user(self, info: FuncInfo, args, node):
        nparams = len(info.fdef.params)
        if len(args) != nparams:
            raise self.err(f"{info.display_name} takes {nparams} argument{'s' if nparams != 1 else ''} "
                           f"but was given {len(args)}", node)
        list_args = [i for i, a in enumerate(args) if isinstance(a.ty, ListTy)]
        if list_args and not self._takes_lists(info):
            if len(list_args) > 1:
                raise self.err("can't apply a function element-wise over two lists at once", node)
            i = list_args[0]
            scalar_args = list(args)
            scalar_args[i] = I.IConst(0, NumTy(args[i].ty.dim))
            call = self.instantiate(info, scalar_args, node)
            r = I.IMap(call.func, args, i, ListTy(call.ty.dim))
            r.sf = call.sf
            return r
        return self.instantiate(info, args, node)

    def _takes_lists(self, info):
        f = info.fdef
        names = {p.name for p in f.params}
        body = f.body if isinstance(f.body, list) else [A.ExprStmt(f.body)]
        found = False

        def visit(n):
            nonlocal found
            if isinstance(n, A.Index) and isinstance(n.target, A.Name) and n.target.name in names:
                found = True
            if isinstance(n, A.Call) and isinstance(n.func, A.Name) and n.func.name in (LIST_FUNCS | {"push", "append", "max", "min", "dot"}):
                for a in n.args:
                    if isinstance(a, A.Name) and a.name in names:
                        if n.func.name in ("max", "min") and len(n.args) > 1:
                            continue
                        found = True
            for c in A.children(n):
                visit(c)

        def visit_stmt(s):
            nonlocal found
            for v in vars(s).values():
                if isinstance(v, A.Node):
                    if isinstance(v, (A.ForIn,)):
                        pass
                    visit(v) if not isinstance(v, list) else None
                elif isinstance(v, list):
                    for x in v:
                        if isinstance(x, A.Node) and not isinstance(x, tuple):
                            if hasattr(x, "body") or isinstance(x, (A.Assign, A.ExprStmt, A.Return, A.If, A.For,
                                                                      A.ForIn, A.While, A.Print, A.IndexAssign)):
                                visit_stmt(x)
                            else:
                                visit(x)
            if isinstance(s, A.ForIn) and isinstance(s.iterable, A.Name) and s.iterable.name in names:
                found = True
        for s in body:
            visit_stmt(s)
        return found

    def instantiate(self, info: FuncInfo, args, node, cache=True):
        f = info.fdef
        keyparts = []
        concrete = True
        for a in args:
            if isinstance(a.ty, (NumTy, ListTy)):
                d = self.U.norm(a.ty.dim)
                if not d.concrete:
                    concrete = False
                keyparts.append((a.ty.kind, tuple(d.const.e)))
            else:
                keyparts.append((a.ty.kind,))
        key = tuple(keyparts)
        if cache and concrete and key in info.instances:
            inst = info.instances[key]
            r = I.ICall(inst, args, inst.ret_ty)
            r.sf = self._minsf(*args, inst) if inst.sf is not None else self._minsf(*args)
            r.hint = getattr(inst, "ret_hint", None)
            return r
        inst = I.IFunc(self.fresh_name(info.name), [])
        inst.display = info.display_name
        inst.ret_ty = None
        inst.ret_placeholder = NumTy(DExpr.fresh("ret"))
        if cache and concrete:
            info.instances[key] = inst
        scope = Scope(info.scope, kind="func")
        fctx = Ctx(inst, scope, is_main=False)
        for p, a in zip(f.params, args):
            ty = a.ty
            if isinstance(ty, NumTy):
                ty = NumTy(DExpr.of(a.ty.dim))
            elif isinstance(ty, ListTy):
                ty = ListTy(DExpr.of(a.ty.dim))
            sym = I.Sym(p.name, ty, "local", inst)
            sym.assigned = True
            if p.unit is not None:
                u = self.resolve_unit(p.unit)
                if not isinstance(ty, (NumTy, ListTy)) or not self.U.unify(ty.dim, u.dim):
                    raise self.err(f"{info.display_name} expects {p.name} in {u.name} ({dim_name(u.dim)}), "
                                   f"but got {type_desc(a.ty, self.U)}", node)
                sym.hint = u
            inst.params.append(sym)
            scope.names[p.name] = sym
        # recursion: calls to this instance while checking use the placeholder type
        inst.ret_ty = inst.ret_placeholder
        try:
            if info.one_liner:
                v = self.expr(info.body_expr(), fctx)
                if not isinstance(v, I.Expr):
                    raise self.err("a function must produce a value", f)
                inst.body = [I.SReturn(v)]
                fctx.ret_types.append(v)
            else:
                stmts = list(f.body)
                if stmts and isinstance(stmts[-1], A.ExprStmt):
                    last = stmts[-1]
                    stmts[-1] = A.Return(last.value).at(last)
                inst.body = self.block(stmts, fctx)
                if fctx.ret_types and not _always_returns(inst.body):
                    raise self.err(f"{info.display_name} doesn't return a value on every path (for example when "
                                   f"an if is false, or a loop doesn't run)", f,
                                   hint="make sure the function ends with a value or a return that always runs")
        except FermiumError as e:
            info.instances.pop(key, None)
            if e.line is None and node is not None:
                e.line, e.col = node.line, node.col
            elif node is not None and node.line and e.line != node.line and not getattr(e, "call_noted", False):
                argd = ", ".join(f"{p.name} = {type_desc(a.ty, self.U)}" for p, a in zip(f.params, args))
                note = f"this happened when calling {info.display_name} on line {node.line} (with {argd})"
                e.hint = f"{e.hint}; {note}" if e.hint else note
                e.call_noted = True
            raise
        rets = fctx.ret_types
        if not rets:
            raise self.err(f"the function {info.display_name} never returns a value", f,
                           hint="end it with the value to return, or use  return value")
        rt = rets[0].ty
        for r in rets[1:]:
            if type(r.ty) is not type(rt) or (isinstance(rt, (NumTy, ListTy)) and not self.U.unify(rt.dim, r.ty.dim)):
                raise self.err(f"{info.display_name} returns different kinds of values in different places", f)
        if isinstance(rt, NumTy):
            if not self.U.unify(inst.ret_placeholder.dim, rt.dim):
                raise self.err(f"the units of {info.display_name} don't work out recursively", f)
        inst.ret_ty = rt
        inst.sf = self._minsf(*rets)
        self.new_funcs.append(inst)
        self.all_funcs.append(inst)
        inst.ret_hint = rets[0].hint if len(rets) == 1 else None
        r = I.ICall(inst, args, rt)
        r.hint = inst.ret_hint
        r.sf = self._minsf(*args, inst)
        if len(rets) == 1 and isinstance(rets[0], I.Expr) and rets[0].hint is not None and \
                isinstance(rets[0], I.IVar) is False and info.one_liner and False:
            r.hint = rets[0].hint
        return r

    # ------------------------------------------------------------ builtins
    def builtin(self, name, e, ctx):
        if name in ("push", "append"):
            raise self.err(f"{name}(list, value) changes a list and doesn't give a value; "
                           f"write it on its own line", e)
        if name == "to":
            if len(e.args) != 2:
                raise self.err("to needs a value and a unit: to(x, eV)", e)
            u_ast = e.args[1]
            text = C.to_source(u_ast)
            try:
                u = parse_unit_string(text.replace(" ", " "))
            except UnitSyntaxError as ex:
                raise self.err(str(ex), u_ast)
            ue = A.UnitExpr([A.UnitFactor(text)], text)
            del ue
            v = self.expr(e.args[0], ctx)
            if not self.U.unify(v.ty.dim, u.dim):
                raise self.err(f"can't show {self.desc(v.ty.dim)} in {u.name} ({dim_name(u.dim)})", e)
            v.hint = u
            return v
        if name == "times" and len(e.args) == 1:
            v = self.expr(e.args[0], ctx, allow_func=True)
            if isinstance(v, SolRef):
                sol = self.var_ref(v.view.sol_sym, ctx, e)
                r = I.ISolList(sol, v.view.comp, "t", ListTy(v.view.tdim))
                r.hint = getattr(v.view, "thint", None)
                return r
        args = []
        for a in e.args:
            v = self.expr(a, ctx, allow_func=True)
            if isinstance(v, SolRef):
                v = self.sol_values(v.view)
            if isinstance(v, FuncRef):
                raise self.err(f"{v.info.display_name} is a function; give it an argument", a)
            args.append(v)
        n = len(args)

        def need(k):
            if n != k:
                raise self.err(f"{name} takes {k} argument{'s' if k != 1 else ''} but was given {n}", e)

        def num_or_list(i, what="argument"):
            self.need_numlike(args[i], e.args[i], f"the {what} of {name}")

        if name in MATH1:
            need(1)
            num_or_list(0)
            if not self.U.unify(args[0].ty.dim, DIMLESS):
                d = self.desc(args[0].ty.dim)
                hint = "divide by a reference value first, e.g. ln(p / (1 atm))" if name in ("ln", "log", "log10",
                                                                                          "log2") else \
                    "the argument of sin, cos, exp, ... must be a plain number (angles in rad are plain numbers)"
                raise self.err(f"{name} needs a plain number, but got {d}", e.args[0], hint=hint)
            return self._bi(name, args, args[0].ty.__class__(DIMLESS), args)
        if name in ("sqrt", "cbrt"):
            need(1)
            num_or_list(0)
            p = Fraction(1, 2) if name == "sqrt" else Fraction(1, 3)
            r = I.IPowC(args[0], float(p) if name == "sqrt" else 1 / 3, args[0].ty.__class__(args[0].ty.dim ** p))
            r.sf = args[0].sf
            return r
        if name in SAME1:
            need(1)
            num_or_list(0)
            r = self._bi(name, args, args[0].ty, args)
            r.hint = args[0].hint
            if name != "abs":
                r.sf = None      # a whole number: print it exactly
            return r
        if name == "sign" or name == "isnan":
            need(1)
            num_or_list(0)
            return self._bi(name, args, NumTy(DIMLESS) if name == "sign" else BOOL, args)
        if name in ("atan2",):
            need(2)
            for i in range(2):
                self.need_num(args[i], e.args[i])
            self.unify_or(args[0].ty.dim, args[1].ty.dim, lambda: "atan2(y, x) needs y and x in the same units", e)
            return self._bi(name, args, NumTy(DIMLESS), args)
        if name in ("hypot", "mod"):
            need(2)
            for i in range(2):
                self.need_num(args[i], e.args[i])
            self.unify_or(args[0].ty.dim, args[1].ty.dim,
                          lambda: f"{name} needs both values in the same units", e)
            r = self._bi(name, args, NumTy(args[0].ty.dim), args)
            r.hint = args[0].hint
            return r
        if name == "clamp":
            need(3)
            for i in range(3):
                self.need_num(args[i], e.args[i])
                self.unify_or(args[0].ty.dim, args[i].ty.dim, lambda: "clamp needs all values in the same units", e)
            r = self._bi(name, args, NumTy(args[0].ty.dim), args)
            r.hint = args[0].hint
            return r
        if name in ("min", "max"):
            if n == 1:
                if not isinstance(args[0].ty, ListTy):
                    raise self.err(f"{name} of a single value needs a list", e)
                r = self._bi(name + "_list", args, NumTy(args[0].ty.dim), args)
                r.hint = args[0].hint
                return r
            if n < 2:
                raise self.err(f"{name} needs at least one argument", e)
            for i in range(n):
                self.need_num(args[i], e.args[i])
                self.unify_or(args[0].ty.dim, args[i].ty.dim,
                              lambda: f"{name} needs all values in the same units", e.args[i])
            r = self._bi(name, args, NumTy(args[0].ty.dim), args)
            r.hint = args[0].hint
            return r
        if name in ("factorial", "gamma"):
            need(1)
            self.need_num(args[0], e.args[0])
            self.U.unify(args[0].ty.dim, DIMLESS)
            return self._bi(name, args, NumTy(DIMLESS), args)
        if name == "len" and n == 1 and isinstance(args[0].ty, TextListTy):
            r = self._bi("len", args, NumTy(DIMLESS), args)
            r.sf = None
            return r
        if name in LIST_FUNCS or name in ("values", "times"):
            need(1)
            a = args[0]
            if not isinstance(a.ty, ListTy):
                raise self.err(f"{name} needs a list, but got {type_desc(a.ty, self.U)}", e.args[0])
            if name == "len":
                r = self._bi("len", args, NumTy(DIMLESS), args)
                r.sf = None    # a count is exact
                return r
            if name in ("sum", "mean", "first", "last"):
                r = self._bi(name, args, NumTy(a.ty.dim), args)
                r.hint = a.hint
                return r
            if name == "std":
                r = self._bi(name, args, NumTy(a.ty.dim), args)
                r.hint = a.hint if not (a.hint is not None and a.hint.affine) else None   # a spread: K, not °C
                return r
            if name in ("cumsum", "diff", "reverse", "sort", "values"):
                r = self._bi(name if name != "values" else "copy", args, ListTy(a.ty.dim), args)
                r.hint = a.hint
                if name in ("diff", "cumsum") and a.hint is not None and a.hint.affine:
                    r.hint = None     # differences of temperatures are shown in K
                return r
            if name == "times":
                if isinstance(a, I.ISolList):
                    return I.ISolList(a.sol, a.comp, "t", ListTy(DExpr.of(TIME_DIM)))
                raise self.err("times(...) needs an ODE solution", e)
        if name in ("norm", "unit", "hat"):
            need(1)
            if not isinstance(args[0].ty, VecTy):
                raise self.err(f"{name} needs a vector, like <3, 4> m", e.args[0])
            if name == "norm":
                r = self._bi("norm", args, NumTy(args[0].ty.dim), args)
                r.hint = args[0].hint
                return r
            return self._bi("unit", args, VecTy(DIMLESS, args[0].ty.n), args)
        if name == "cross":
            need(2)
            if not all(isinstance(a.ty, VecTy) for a in args):
                raise self.err("cross(a, b) needs two vectors", e)
            return self.vec_arith("×", args[0], args[1], e)
        if name == "vec":
            if n not in (2, 3):
                raise self.err("vec(...) takes 2 or 3 components", e)
            return self.e_VecLit(A.VecLit(e.args).at(e), ctx)
        if name == "dot" and n == 2 and all(isinstance(a.ty, VecTy) for a in args):
            return self.vec_arith("*", args[0], args[1], e)
        if name == "trapz":
            need(2)
            for i in range(2):
                if not isinstance(args[i].ty, ListTy):
                    raise self.err("trapz(ys, xs) needs two lists", e)
            return self._bi(name, args, NumTy(args[0].ty.dim * args[1].ty.dim), args)
        if name == "dot":
            need(2)
            for i in range(2):
                if not isinstance(args[i].ty, ListTy):
                    raise self.err("dot(a, b) needs two lists", e)
            return self._bi(name, args, NumTy(args[0].ty.dim * args[1].ty.dim), args)
        if name == "interp":
            need(3)
            x, xs, ys = args
            self.need_num(x, e.args[0])
            if not isinstance(xs.ty, ListTy) or not isinstance(ys.ty, ListTy):
                raise self.err("interp(x, xs, ys) needs a value and two lists", e)
            self.unify_or(x.ty.dim, xs.ty.dim, lambda: "interp: x and xs need the same units", e)
            return self._bi(name, args, NumTy(ys.ty.dim), args)
        if name == "linspace":
            need(3)
            for i in range(3):
                self.need_num(args[i], e.args[i])
            self.unify_or(args[0].ty.dim, args[1].ty.dim, lambda: "linspace(a, b, n): a and b need the same units",
                          e)
            self.unify_or(args[2].ty.dim, DIMLESS, lambda: "linspace(a, b, n): n must be a plain number", e.args[2])
            r = self._bi(name, args, ListTy(args[0].ty.dim), args)
            r.hint = args[0].hint or args[1].hint
            return r
        if name in ("zeros", "ones"):
            need(1)
            self.need_num(args[0], e.args[0])
            self.unify_or(args[0].ty.dim, DIMLESS, lambda: f"{name}(n): n must be a plain number", e.args[0])
            return self._bi(name, args, ListTy(DExpr.fresh(name) if name == "zeros" else DIMLESS), args)
        if name == "range":
            if n not in (2, 3):
                raise self.err("range(a, b) or range(a, b, step)", e)
            for i in range(n):
                self.need_num(args[i], e.args[i])
                self.unify_or(args[0].ty.dim, args[i].ty.dim, lambda: "range: all values need the same units", e)
            if n == 2:
                args.append(I.IConst(1, NumTy(args[0].ty.dim)))
            return self._bi(name, args, ListTy(args[0].ty.dim), args)
        if name == "clock":
            if n != 0:
                raise self.err("clock() takes no arguments", e)
            return self._bi(name, args, NumTy(TIME_DIM), args)
        if name == "rand":
            if n != 0:
                raise self.err("rand() takes no arguments", e)
            return self._bi(name, args, NumTy(DIMLESS), args)
        raise self.err(f"{name} can't be used this way", e)

    def _bi(self, name, args, ty, sfargs):
        r = I.IBuiltin(name, args, ty)
        r.sf = self._minsf(*sfargs)
        return r

    # ------------------------------------------------------------ calculus
    def e_Prime(self, e, ctx):
        # inside an ODE right-hand side, x' is a state variable
        if isinstance(e.target, A.Name):
            key = e.target.name + "'" * e.order
            b, _ = ctx.scope.lookup(key)
            if isinstance(b, I.Sym):
                return self.var_ref(b, ctx, e)
            t = self.expr(e.target, ctx, allow_func=True)
        else:
            t = self.expr(e.target, ctx, allow_func=True)
        if isinstance(t, FuncRef):
            info = t.info
            if len(info.fdef.params) != 1:
                raise self.err(f"{info.display_name}' is ambiguous: {info.display_name} has several parameters",
                               e, hint="use ∂/∂x to say which one")
            return FuncRef(self.derived_info(info, 0, e.order, e))
        if isinstance(t, SolRef):
            v = t.view
            nv = SolView(v.sol_sym, v.comp + e.order * v.stride, v.top, v.dim / (DExpr.of(v.tdim) ** e.order),
                         v.tdim, v.tname, v.name + "'" * e.order)
            nv.n, nv.stride = v.n, v.stride
            nv.sf = getattr(v, "sf", None)
            nv.thint = getattr(v, "thint", None)
            k = (v.comp - v.top) // v.stride + e.order + (v.top // v.stride if False else 0)
            hints = getattr(v, "hints", {})
            order = v.name.count("'") + e.order
            nv.hints = hints
            nv.hint = hints.get(order)
            del k
            return SolRef(nv)
        raise self.err("' (prime) means a derivative; it only works on functions and ODE solutions", e,
                       hint="to differentiate a formula write d/dt (formula)")

    def derived_info(self, info: FuncInfo, i, order, node):
        key = (i, order)
        if key in info.derived:
            return info.derived[key]
        if not info.one_liner:
            raise self.err(f"can only differentiate one-line functions like f(x) = ..., and "
                           f"{info.display_name} is defined over several lines", node)
        f = info.fdef
        pname = f.params[i].name
        body = info.body_expr()
        for _ in range(order):
            saved = self.cur_ctx
            body = C.diff(body, pname, self._diffctx(info.scope))
            self.cur_ctx = saved
        suffix = "'" * order if len(f.params) == 1 else f"_∂{pname}" * order
        pretty = (info.display_name + "'" * order) if len(f.params) == 1 else \
            (f"∂{info.display_name}/∂{pname}" if order == 1 else f"∂{order}{info.display_name}/∂{pname}{order}")
        nm = info.name + suffix
        fd = A.FuncDef(nm, f.params, body)
        fd.line, fd.col = f.line, f.col
        d = FuncInfo(nm, fd, info.scope)
        d.display_name = pretty
        d.parent = (info if getattr(info, "parent", None) is None else info.parent[0], i,
                    order + (info.parent[2] if getattr(info, "parent", None) else 0))
        info.derived[key] = d
        return d

    def _diffctx(self, scope):
        checker = self

        class DC(C.DiffContext):
            def user_function(self, fname):
                b, _ = scope.lookup(fname)
                if isinstance(b, FuncInfo):
                    if not b.one_liner:
                        raise FermiumError(f"can't differentiate through {fname}: it's defined over several lines")
                    return ([p.name for p in b.fdef.params], b.body_expr())
                return None

            def derived_function(self, fname, i, order=1):
                b, sc = scope.lookup(fname)
                d = checker.derived_info(b, i, order, b.fdef)
                sc.names[d.name] = d
                return d.name

            def is_solution(self, fname):
                b, _ = scope.lookup(fname)
                return isinstance(b, SolView)
        return DC()

    def e_Deriv(self, e, ctx):
        op = e.operand
        if isinstance(op, A.Name):
            b, _ = ctx.scope.lookup(op.name)
            if isinstance(b, FuncInfo):
                params = [p.name for p in b.fdef.params]
                if e.var in params:
                    i = params.index(e.var)
                elif len(params) == 1 and not e.partial:
                    i = 0
                else:
                    raise self.err(f"{b.display_name} has no parameter called {e.var}", e)
                return FuncRef(self.derived_info(b, i, e.order, e))
            if isinstance(b, SolView):
                return self.e_Prime(A.Prime(op, e.order).at(e), ctx)
        # derivative of a formula
        bound, _ = ctx.scope.lookup(e.var)
        body = op
        for _ in range(e.order):
            body = C.diff(body, e.var, self._diffctx(ctx.scope))
        if isinstance(bound, I.Sym):
            return self.expr(body, ctx)
        # d/dt (formula in t) with t not defined -> a new function of t
        fd = A.FuncDef(f"λ{self.counter}", [A.Param(e.var)], body)
        fd.line, fd.col = e.line, e.col
        info = FuncInfo(self.fresh_name("deriv"), fd, ctx.scope if ctx.is_main else self.globals)
        info.display_name = f"d/d{e.var}(...)"
        return FuncRef(info)

    def e_Integral(self, e, ctx):
        if e.lo is None:
            return self.indefinite_integral(e, ctx)
        lo = self.expr(e.lo, ctx)
        hi = self.expr(e.hi, ctx)
        self.need_num(lo, e.lo, "the lower limit")
        self.need_num(hi, e.hi, "the upper limit")
        self.unify_or(lo.ty.dim, hi.ty.dim, lambda: f"the limits of this integral are {self.desc(lo.ty.dim)} and "
                      f"{self.desc(hi.ty.dim)}; they need the same units", e,
                      hint=("e is the elementary charge in Fermium; for Euler's number write exp(1)"
                            if any(isinstance(n, A.Name) and n.name == "e" for n in (e.lo, e.hi)) else
                            "if you divide or multiply the integral by something, put the integral in parentheses: "
                            "(∫ ... dx from a to b) / M"))
        lam = I.ILambda("scalar", self.fresh_name("integrand"))
        lam.locals = []
        scope = Scope(ctx.scope)
        lctx = Ctx(lam, scope, is_main=False, parent=ctx, lam=lam)
        lctx.enclosing = ctx.func
        xs = I.Sym(e.var, NumTy(lo.ty.dim), "local", lam)
        xs.assigned = True
        lam.params = [xs]
        scope.names[e.var] = xs
        body = self.expr(e.integrand, lctx)
        self.need_num(body, e.integrand, "the thing being integrated")
        lam.body = body
        self.new_lambdas.append(lam)
        self.all_lambdas.append(lam)
        r = I.IIntegral(lam, lo, hi, NumTy(body.ty.dim * lo.ty.dim))
        r.sf = self._minsf(lo, hi, body)
        return r

    def indefinite_integral(self, e, ctx):
        # inline calls to one-line user functions so SymPy sees a plain formula
        def inline(n):
            n = C.map_children(n, inline)
            if isinstance(n, A.Call) and isinstance(n.func, A.Name):
                b, _ = ctx.scope.lookup(n.func.name)
                if isinstance(b, FuncInfo) and b.one_liner:
                    m = {p.name: a for p, a in zip(b.fdef.params, n.args)}
                    return C.subst(C.inline_where(b.body_expr()), m)
            return n
        body = C.integrate_symbolic(inline(e.integrand), e.var)
        fd = A.FuncDef(f"∫d{e.var}", [A.Param(e.var)], body)
        fd.line, fd.col = e.line, e.col
        info = FuncInfo(self.fresh_name("antideriv"), fd, ctx.scope if ctx.is_main else self.globals)
        info.display_name = f"∫d{e.var}"
        return FuncRef(info)

    # ------------------------------------------------------------ solve
    def s_Solve(self, s, ctx):
        from .solve import check_solve
        return check_solve(self, s, ctx)

    def s_Fit(self, s, ctx):
        from .solve import check_fit
        return check_fit(self, s, ctx)

    def s_Plot(self, s, ctx):
        from .solve import check_plot
        return check_plot(self, s, ctx)


def _always_returns(stmts) -> bool:
    for st in stmts:
        if isinstance(st, I.SReturn):
            return True
        if isinstance(st, I.SIf) and st.other and _always_returns(st.then) and _always_returns(st.other):
            return True
    return False


def canonical_unit_name(uexpr: A.UnitExpr) -> str:
    """The display spelling of a written unit, independent of how it was typed:
    `ft/s^2`, `ft/s²` and `ft s^-2` all display as `ft/s²`; `N·m` as `N m`; `m m` as `m²`."""
    from .units import UNIT_PRETTY, join_units, _fmt_exp
    order, exps = [], {}
    for f in uexpr.factors:
        name = UNIT_PRETTY.get(f.name, f.name)
        if name.startswith(("u", "µ")) and len(name) > 1 and lookup_unit("μ" + name[1:]) is not None \
                and name[1:] not in ("", "n") and lookup_unit(name) is not None and \
                lookup_unit(name).factor == lookup_unit("μ" + name[1:]).factor:
            name = "μ" + name[1:]
        if name not in exps:
            order.append(name)
            exps[name] = Fraction(0)
        exps[name] += f.exp
    num = [n + _fmt_exp(exps[n]) for n in order if exps[n] > 0]
    den = [n + _fmt_exp(-exps[n]) for n in order if exps[n] < 0]
    return join_units(num, den)


def read_csv_header(full, node=None):
    """Read column names and units from a CSV header like  L [m], T [s]."""
    try:
        with open(full, newline="", encoding="utf-8-sig") as fh:
            reader = csv.reader(fh)
            header = next(reader)
    except StopIteration:
        raise FermiumError(f"the file {os.path.basename(full)} is empty")
    cols = []
    for h in header:
        h = h.strip()
        unit = Unit("1", DIMLESS, 1.0)
        name = h
        if "[" in h and h.endswith("]"):
            name, _, ut = h.partition("[")
            name = name.strip()
            ut = ut[:-1].strip()
            try:
                unit = parse_unit_string(ut) if ut not in ("", "1", "-") else unit
            except UnitSyntaxError as ex:
                raise FermiumError(f"in {os.path.basename(full)}, column '{h}': {ex}")
        from .lexer import canonical_name
        name = canonical_name(name.replace(" ", "_"))
        cols.append({"name": name, "unit": unit})
    return cols


def check(source_ast, diags=None, base_dir=".", repl=False):
    return Checker(diags, base_dir, repl).check_program(source_ast)


Ty  # re-export for type hints
