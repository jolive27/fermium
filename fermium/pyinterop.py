"""The checker's side of `use python` (M6, DECISIONS D140-D141): calling Python functions from Fermium with
the units checked at the boundary.

    use python numpy as np
    use python scipy.special as sp
    use python mylib as ml:
        energy(m [kg], v [m/s]) -> [J]
        positions(t [s]) -> list [m]

Python functions take and return plain numbers.  Without a declared signature every argument must be
dimensionless (a unit error at compile time says how to fix it: divide by a unit, like r / (1 m)) and the
result is a plain number (a list when an argument is a list).  With a signature, each argument must have the
declared unit's dimension and is passed as a number in that unit; the result gets the declared unit.
The module and the function are looked up when the program is checked, so a misspelt name is a compile error.
"""
from __future__ import annotations

from difflib import get_close_matches

from . import ast as A
from . import ir as I
from .types import DIMLESS, ComplexTy, ListTy, NumTy, VecTy, MatTy
from .units import preferred_unit


class PyModRef:
    """What `np` is bound to after `use python numpy as np`."""

    def __init__(self, module, alias, pymod, sigs, line):
        self.module = module          # "scipy.special"
        self.alias = alias            # "sp"
        self.pymod = pymod            # the imported Python module
        self.sigs = sigs              # name -> {"params": [(name, Unit|None, is_int)], "shape", "unit"}
        self.line = line


def _ascii(name):
    """`sp.gamma` is lexed as sp.γ; Python wants the spelling as written (or its ASCII name)."""
    from .lexer import GREEK_TO_ASCII
    return "".join(GREEK_TO_ASCII.get(c, c) for c in name)


def _is_number(x):
    import numbers
    return isinstance(x, numbers.Real) and not isinstance(x, bool)


class PythonMixin:
    """Mixed into Checker."""

    # ------------------------------------------------------------ the statement
    def s_UsePython(self, s, ctx):
        if not ctx.is_main or ctx.lam is not None or getattr(ctx, "branch", 0) or ctx.loop \
                or ctx.scope.kind not in ("global", "module"):
            raise self.err("use python must be at the top level of the program (not inside a block or function)", s)
        if getattr(ctx.scope, "module", None) is not None:
            raise self.err("a Fermium module can't use Python yet; put the  use python  line in the program", s)
        from .runtime.pycall import import_python_module
        try:
            pymod = import_python_module(s.module, self.base_dir)
        except ModuleNotFoundError as ex:
            missing = getattr(ex, "name", None) or s.module
            raise self.err(f"can't find the Python module {s.module}", s,
                           hint=f"install it with  pip install {missing.split('.')[0]}  (or put {missing}.py in the "
                                f"program's folder)" if missing == s.module or s.module.startswith(missing)
                           else f"{s.module} needs {missing}: install it with  pip install {missing.split('.')[0]}")
        except Exception as ex:
            raise self.err(f"importing the Python module {s.module} failed: {type(ex).__name__}: {ex}", s)
        name = s.alias or s.module
        sigs = {}
        for sig in s.sigs:
            if _ascii(sig.name) in sigs:
                raise self.err(f"{sig.name} has two signatures in this use line", sig)
            self._py_attr(pymod, s.module, name, sig.name, sig, want_callable=True)
            params = []
            for pname, u, is_int in sig.params:
                unit = self._py_unit(u, sig) if u is not None else None
                params.append((pname, unit, is_int))
            runit = self._py_unit(sig.ret_unit, sig) if sig.ret_unit is not None else None
            sigs[_ascii(sig.name)] = {"params": params, "shape": sig.ret_shape, "unit": runit}
        existing = ctx.scope.names.get(name)
        if existing is not None and not (isinstance(existing, PyModRef) and self.repl):   # the REPL may redo it
            if isinstance(existing, PyModRef) and existing.module == s.module and not s.sigs:
                return []
            raise self.err(f"{name} already means something in this program, so it can't also be the Python "
                           f"module {s.module}", s, hint=f"give the module another name:  use python {s.module} as "
                                                         f"{name}_py")
        later = getattr(ctx.scope, "top_defs", {}).get(name)
        if later is not None and (later.line or 0) > (s.line or 0) and not self.repl:
            raise self.err(f"{name} is the Python module {s.module} (line {s.line}) and is defined again on line "
                           f"{later.line}", later, hint=f"rename your {name}, or use another name after as")
        ctx.scope.names[name] = PyModRef(s.module, name, pymod, sigs, s.line)
        return []

    def _py_unit(self, uexpr, node):
        u = self.resolve_unit(uexpr)
        if u.offset:
            raise self.err(f"a Python function's unit can't be {u.name} (a scale with an offset); use K", node)
        return u

    def _py_attr(self, pymod, module, alias, attr, node, want_callable):
        try:
            obj = getattr(pymod, attr)
        except AttributeError:
            obj = None
            if _ascii(attr) != attr:
                obj = getattr(pymod, _ascii(attr), None)
            if obj is None:
                names = [n for n in dir(pymod) if not n.startswith("_")]
                close = get_close_matches(attr, names, n=1, cutoff=0.6)
                raise self.err(f"the Python module {module} has no {attr}", node,
                               hint=f"did you mean {alias}.{close[0]}?" if close else None)
        if want_callable and not callable(obj):
            raise self.err(f"{alias}.{attr} is {'a number' if _is_number(obj) else 'not a function'} in Python, so "
                           f"it can't be called", node, hint=f"write {alias}.{attr} without ( )" if _is_number(obj)
                           else None)
        return obj

    # ------------------------------------------------------------ using it
    def py_ref_of(self, node, ctx):
        if isinstance(node, A.Name):
            b, _ = ctx.scope.lookup(node.name)
            return b if isinstance(b, PyModRef) else None
        return None

    def python_value(self, ref, e, ctx):
        """np.pi: a number attribute of a Python module, read when the program is checked (a plain number)."""
        attr = getattr(e, "raw", e.name)
        obj = self._py_attr(ref.pymod, ref.module, ref.alias, attr, e, want_callable=False)
        if callable(obj):
            raise self.err(f"{ref.alias}.{attr} is a Python function; call it, like {ref.alias}.{attr}(x)", e)
        if not _is_number(obj):
            raise self.err(f"{ref.alias}.{attr} is a {type(obj).__name__} in Python; Fermium can only use Python "
                           f"numbers and functions", e)
        return I.IConst(float(obj), NumTy(DIMLESS))

    def python_call(self, ref, e, ctx):
        f = e.func
        attr = getattr(f, "raw", f.name)
        self._py_attr(ref.pymod, ref.module, ref.alias, attr, f, want_callable=True)
        if self.nat.natural:
            raise self.err(f"{ref.alias}.{attr}: Python functions can't be called inside  {self.nat.label()}  "
                           f"yet", e, hint="call it outside the region and bring the value in")
        display = f"{ref.alias}.{attr}"
        sig = ref.sigs.get(_ascii(attr))
        if not hasattr(ref.pymod, attr):          # sp.γ after fmt --pretty is sp.gamma
            attr = _ascii(attr)
        args = [self.expr(a, ctx) for a in e.args]
        if sig is not None and len(args) != len(sig["params"]):
            n = len(sig["params"])
            raise self.err(f"{display} takes {n} argument{'s' if n != 1 else ''} (as declared in the use line on "
                           f"line {ref.line}), but got {len(args)}", e)
        facs, ints, pnames = [], [], []
        any_list = False
        for k, (a, node) in enumerate(zip(args, e.args)):
            pname, unit, is_int = sig["params"][k] if sig is not None else (f"argument {k + 1}", None, False)
            if not isinstance(a, I.Expr) or not isinstance(a.ty, (NumTy, ListTy)):
                what = "a complex number" if isinstance(getattr(a, "ty", None), ComplexTy) else \
                    "a vector" if isinstance(getattr(a, "ty", None), VecTy) else \
                    "a matrix" if isinstance(getattr(a, "ty", None), MatTy) else \
                    "a function" if not isinstance(a, I.Expr) else "not a number"
                raise self.err(f"{display}: a Python function takes numbers and lists of numbers, but "
                               f"{pname} is {what}", node,
                               hint="pass the components one at a time, like v.x" if what == "a vector" else
                               "pass the real and imaginary parts one at a time, like re(z) and im(z)"
                               if what == "a complex number" else None)
            any_list = any_list or isinstance(a.ty, ListTy)
            if unit is None:
                self.unify_or(a.ty.dim, DIMLESS, lambda: self._py_unit_msg(display, pname, a, sig), node,
                              hint=self._py_unit_hint(a, node, display, attr, len(args), k))
                facs.append(1.0)
            else:
                self.unify_or(a.ty.dim, unit.dim, lambda: f"{display} expects {pname} in {unit.name} (declared on "
                              f"line {ref.line}), but got {self.desc(a.ty.dim)}", node)
                facs.append(float(unit.factor))
            ints.append(bool(is_int))
            pnames.append(pname)
        runit = sig["unit"] if sig is not None else None
        shape = sig["shape"] if sig is not None else None
        rlist = shape == "list" if shape is not None else any_list
        rdim = runit.dim if runit is not None else DIMLESS
        tables = self.tables
        if not hasattr(tables, "pycalls"):
            tables.pycalls = []
        tables.pycalls.append({"module": ref.module, "func": attr, "display": display, "nargs": len(args),
                               "facs": facs, "ints": ints, "pnames": pnames, "rlist": rlist,
                               "rfac": float(runit.factor) if runit is not None else 1.0, "line": e.line, "declared": shape is not None})
        from .m3solve import _python_only
        _python_only(self, e.line, f"a call into Python ({display})")
        r = I.IPyCall(len(tables.pycalls) - 1, args, ListTy(rdim) if rlist else NumTy(rdim))
        if runit is not None and runit.name not in ("1", ""):
            r.hint = runit
        return r

    def _py_unit_msg(self, display, pname, a, sig):
        why = "" if sig is None else " (declare a unit for it in the use line if the function expects one)"
        return (f"{display} is a Python function, which takes plain numbers, but {pname} is {self.desc(a.ty.dim)}"
                + why)

    def _py_unit_hint(self, a, node, display, attr, nargs, k):
        d = self.U.resolve(a.ty.dim)
        u = preferred_unit(d).name if not d.dimensionless else "m"
        src = node.name if isinstance(node, A.Name) else "…"
        params = [f"x{i + 1}" for i in range(nargs)]
        params[k] = f"x{k + 1} [{u}]"
        return (f"divide by a unit, like  {src} / (1 {u}),  or declare the unit in the use line:  "
                f"{attr}({', '.join(params)})")
