"""The checker's side of `import` (M7, D100-D102): loading a module, binding its names, and pointing
errors inside modules back at the user's program.

A module is checked once per compilation, in its own scope (whose parent is the built-in constants), so
its functions see its own names and never the importing program's.  Its constants become globals of the
program's main function (computed where the import is); its functions are ordinary generic functions,
instantiated at each call with the caller's units, like any Fermium function.
"""
from __future__ import annotations

import os
from difflib import get_close_matches

from . import ast as A
from .errors import Diagnostics, FermiumError
from .modules import available_modules, resolve_module, stdlib_dir


class ModuleInfo:
    def __init__(self, name, path, scope, display):
        self.name = name          # the module's own name (file stem)
        self.path = path
        self.scope = scope
        self.display = display    # short path for messages: springs.fm, stdlib/mechanics.fm

    def exported(self):
        return sorted(n for n in self.scope.names if not n.startswith("_"))


class ModuleRef:
    """What a module's name (or its `as` alias) is bound to in the importing scope."""

    def __init__(self, info):
        self.info = info


def _top_defs(body):
    """Names a program or module defines at its top level: name -> the defining statement."""
    out = {}
    for s in body:
        if isinstance(s, A.FuncDef) or (isinstance(s, A.Assign) and s.op == "="):
            out.setdefault(s.name, s)
        elif isinstance(s, A.Analyze) and s.title:
            out.setdefault(s.title, s)
    return out


_ALLOWED = (A.FuncDef, A.Assign, A.Import)
_STMT_WORDS = {"Print": "a print", "Plot": "a plot", "Solve": "a solve", "Fit": "a fit", "If": "an if",
               "For": "a for loop", "ForIn": "a for loop", "While": "a while loop", "ExprStmt": "a bare expression",
               "Assert": "an assert", "Analyze": "an analyze", "IndexAssign": "an element assignment"}


class ImportMixin:
    """Mixed into Checker.  Uses Checker's err/block/Scope/Ctx machinery."""

    # ------------------------------------------------------------ bookkeeping
    def note_program(self, prog, scope):
        """Called by check_program: remember the top-level definitions, for clash errors."""
        scope.top_defs = _top_defs(prog.body)

    def _mods(self):
        d = self.__dict__
        if "modules" not in d:
            d["modules"] = {}         # path -> ModuleInfo, loaded in this compilation
            d["loading"] = []         # [(name, path)] being loaded now (cycle detection)
        return d["modules"]

    def _display(self, path):
        std = stdlib_dir()
        if os.path.dirname(path) == std:
            return "stdlib/" + os.path.basename(path)
        base = os.path.abspath(self.base_dir or ".")
        rel = os.path.relpath(path, base)
        if rel == ".":
            return "the program's folder"
        return path if rel.startswith("..") else rel

    # ------------------------------------------------------------ the statement
    def s_Import(self, s, ctx):
        if not ctx.is_main or ctx.lam is not None or getattr(ctx, "branch", 0) or ctx.loop \
                or ctx.scope.kind not in ("global", "module"):
            raise self.err("import must be at the top level of the program (not inside a block or function)", s)
        mods = self._mods()
        importer = getattr(ctx.scope, "module", None)
        importer_dir = os.path.dirname(importer.path) if importer else None
        try:
            path, folders = resolve_module(s.module, s.is_path, self.base_dir, importer_dir)
        except FermiumError as e:           # a broken fermium.toml
            if e.line is None:
                e.line, e.col, e.length = s.line, s.col, s.length
            raise
        if path is None:
            raise self._not_found(s, folders)
        stem = os.path.splitext(os.path.basename(path))[0]
        if s.names is None and not s.alias and not stem.isidentifier():
            raise self.err(f"the module file {os.path.basename(path)} has a name that can't be used in a program",
                           s, hint=f'give it a name with as:  import "{s.module}" as {_safe(stem)}')
        stmts = []
        info = mods.get(path)
        if info is None:
            info, stmts = self._load(path, stem, s, ctx)
        if s.names is None:
            self._bind(ctx.scope, s.alias or stem, ModuleRef(info), s, info)
        else:
            for name, alias in s.names:
                if name.startswith("_"):
                    raise self.err(f"{name} is private to the module {info.name} (names starting with _ aren't "
                                   f"exported)", s)
                b = info.scope.names.get(name)
                if b is None:
                    raise self._no_member(info, name, s)
                self._bind(ctx.scope, alias or name, b, s, info, member=name)
        return stmts

    def _not_found(self, s, folders):
        if s.is_path:
            return self.err(f"can't find the module file \"{s.module}\"", s,
                            hint=f"the path is relative to the folder of the file with the import ({folders[0]})")
        known = available_modules(folders)
        close = get_close_matches(s.module, known, n=1, cutoff=0.6)
        where = ", ".join(self._display(f) if f != stdlib_dir() else "the standard library" for f in folders)
        hint = f"did you mean {close[0]}? " if close else ""
        hint += f"looked for {s.module}.fm in: {where}"
        return self.err(f"can't find a module called {s.module}", s, hint=hint)

    def _no_member(self, info, name, node):
        close = get_close_matches(name, info.exported(), n=1, cutoff=0.6)
        hint = f"did you mean {close[0]}?" if close else \
            f"{info.name} defines: {', '.join(info.exported()[:12])}" + (" …" if len(info.exported()) > 12 else "")
        return self.err(f"{info.name} has no {name}", node, hint=hint)

    def _bind(self, scope, name, obj, s, info, member=None):
        imported = scope.__dict__.setdefault("imported", {})
        existing = scope.names.get(name)
        same = existing is obj or (isinstance(existing, ModuleRef) and isinstance(obj, ModuleRef)
                                   and existing.info is obj.info)
        what = f"{info.name}.{member}" if member else f"the module {info.name}"
        if existing is not None and not same:
            if name in imported:
                src, ln = imported[name]
                raise self.err(f"{name} is already imported from {src} (line {ln}); importing it from {info.name} "
                               f"too would be ambiguous", s,
                               hint=f"give one a new name, like  from {info.name} import {member or name} as "
                                    f"{name}_{info.name}, or  import {info.name} as ... and write the full name")
            raise self.err(f"{name} already means something in this program, so it can't also be {what}", s,
                           hint=f"import it under another name:  " +
                                (f"from {info.name} import {member} as {member}_{info.name}" if member
                                 else f"import {info.name} as {info.name}_mod"))
        later = getattr(scope, "top_defs", {}).get(name)
        if later is not None and (later.line or 0) > (s.line or 0) and not self.repl:
            raise self.err(f"{name} is {what} (imported on line {s.line}) and is defined again on line "
                           f"{later.line}", later,
                           hint=f"rename your {name}, or import it under another name with as")
        scope.names[name] = obj
        imported.setdefault(name, (info.name, s.line))

    # ------------------------------------------------------------ loading
    def _load(self, path, stem, s, ctx):
        from .checker import Ctx, Scope, _positive_names  # noqa: F401  (circular import at module level)
        from .parser import parse
        mods, loading = self._mods(), self.__dict__["loading"]
        if any(p == path for _, p in loading):
            chain = [n for n, _ in loading[[p for _, p in loading].index(path):]] + [stem]
            raise self.err(f"circular import: {' → '.join(chain)}", s,
                           hint="modules can't import each other in a circle; move the shared functions into a "
                                "third module that both import")
        display = self._display(path)
        try:
            with open(path, encoding="utf-8") as fh:
                src = fh.read()
        except (OSError, UnicodeDecodeError) as e:
            raise self.err(f"can't read the module {display}: {e}", s)
        info = ModuleInfo(stem, path, None, display)
        pdiags = Diagnostics()
        try:
            prog = parse(src, pdiags)
        except FermiumError as e:
            raise self._wrap(e, info, s)
        for st in prog.body:
            if not isinstance(st, _ALLOWED) or (isinstance(st, A.Assign) and st.op != "="):
                what = _STMT_WORDS.get(type(st).__name__, "a statement")
                if isinstance(st, A.Assign):
                    what = f"a change to {st.name}"
                e = FermiumError(f"a module can only define functions and constants, but this line has {what}",
                                 st.line, st.col, st.length,
                                 hint="modules can't print or compute things when imported; put that in your program")
                raise self._wrap(e, info, s)
        scope = Scope(self.root, kind="module")
        scope.module = info
        info.scope = scope
        self.note_program(prog, scope)
        mctx = Ctx(ctx.func, scope, is_main=True)
        saved = (self.future_funcs, getattr(self, "unit_collisions", {}), getattr(self, "positive_names", set()),
                 getattr(self, "collision_taint", {}))
        self.future_funcs = {st.name: st.line for st in prog.body if isinstance(st, A.FuncDef)}
        self.unit_collisions = getattr(prog, "unit_collisions", {})
        self.positive_names = set()
        self.collision_taint = {}
        n0 = len(self.diags.warnings)
        loading.append((stem, path))
        try:
            stmts = self.block(prog.body, mctx, new_scope=False)
        except FermiumError as e:
            raise self._wrap(e, info, s)
        finally:
            loading.pop()
            (self.future_funcs, self.unit_collisions, self.positive_names, self.collision_taint) = saved
        for w in pdiags.warnings:
            self.diags.warnings.append(w)
        self._relocate_warnings(n0, info, s)
        from .checker import FuncInfo
        for name, b in scope.names.items():
            if isinstance(b, FuncInfo) and getattr(b, "module", None) is None:
                b.module = info
                if b.display_name == name:
                    b.display_name = f"{stem}.{name}"
        mods[path] = info
        return info, stmts

    def _wrap(self, e, info, s):
        ln = f", line {e.line}" if e.line else ""
        w = FermiumError(f"in the module {info.name} ({info.display}{ln}): {e.message}", s.line, s.col, s.length,
                         e.hint)
        w.warnings = getattr(e, "warnings", [])
        return w

    def _relocate_warnings(self, n0, info, node, fname=None):
        for w in self.diags.warnings[n0:]:
            if getattr(w, "module_noted", False):
                w.line, w.col, w.length = node.line, node.col, node.length
                continue
            w.module_noted = True
            where = f"{info.display}, line {w.line}" if w.line else info.display
            w.message = f"{w.message} (in {fname or 'the module ' + info.name}, {where})"
            w.line, w.col, w.length = node.line, node.col, node.length

    # ------------------------------------------------------------ calls into a module
    def module_call(self, info, args, node, cache):
        """Instantiate a module's function: warnings from its body are shown at the call."""
        n0 = len(self.diags.warnings)
        info.in_call = getattr(info, "in_call", 0) + 1
        try:
            return self.instantiate(info, args, node, cache)
        except FermiumError as e:
            f = info.fdef
            if (e.line, e.col) == (f.line, f.col):       # about the definition (e.g. never returns)
                self.module_body_error(e, info, node)
            raise
        finally:
            info.in_call -= 1
            if node is not None and node is not info.fdef and node.line:
                self._relocate_warnings(n0, info.module, node, info.display_name)

    def module_body_error(self, e, info, node):
        """An error inside a module function's body: say where in the module, and point at the call."""
        if node is None or node is info.fdef or not node.line:
            return
        if not getattr(e, "module_noted", False):
            e.module_noted = True
            where = f"{info.module.display}, line {e.line}" if e.line else info.module.display
            e.message = f"{e.message} (in {info.display_name}, {where})"
        e.line, e.col, e.length = node.line, node.col, node.length

    # ------------------------------------------------------------ using a module's names
    def module_of(self, node, ctx):
        """The ModuleInfo a Name / Field node refers to, or None."""
        if isinstance(node, A.Name):
            b, _ = ctx.scope.lookup(node.name)
            return b.info if isinstance(b, ModuleRef) else None
        if isinstance(node, A.Field):
            m = self.module_of(node.target, ctx)
            if m is not None:
                b = m.scope.names.get(node.name)
                return b.info if isinstance(b, ModuleRef) else None
        return None

    def module_member(self, info, e, ctx):
        if e.name.startswith("_"):
            raise self.err(f"{e.name} is private to the module {info.name} (names starting with _ aren't "
                           f"exported)", e)
        b = info.scope.names.get(e.name)
        if b is None:
            raise self._no_member(info, e.name, e)
        return self.use_binding(b, f"{info.name}.{e.name}", e, ctx)

    def module_as_value(self, b, name, e):
        ex = b.info.exported()
        eg = f"{name}.{ex[0]}" if ex else f"{name}.name"
        return self.err(f"{name} is a module, not a value; use the names it defines, like {eg}", e)

    def module_hint(self, name, ctx):
        """`hooke isn't defined` after `import springs`: point to springs.hooke."""
        s = ctx.scope
        while s is not None:
            for alias, b in s.names.items():
                if isinstance(b, ModuleRef) and name in b.info.scope.names and not name.startswith("_"):
                    return f"{name} is in the module {b.info.name}: write {alias}.{name}, or  " \
                           f"from {b.info.name} import {name}"
            s = s.parent
        return None


def _safe(stem):
    out = "".join(c if c.isalnum() or c == "_" else "_" for c in stem)
    return out if out and not out[0].isdigit() else "m_" + out
