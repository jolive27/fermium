"""The Fermium language server (`fermium lsp`, used by the VS Code extension).

- Errors and warnings are underlined as you type (the program is checked, never run).
- Hovering over a name shows its units: variables, functions (with the units of their result),
  ODE solutions, constants and units themselves.
- `\\name` completes to a symbol (`\\omega` → ω), and names in the program complete too.

The analysis functions (`analyze`, `hover_text`, `completions`) are plain Python, so they are
tested without an editor; `serve()` wraps them in the Language Server Protocol with pygls.
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

from .checker import Checker, FuncInfo, SolView
from .constants import all_constants
from .errors import Diagnostics, FermiumError
from .importer import ModuleRef
from .lexer import KEYWORDS
from .parser import parse
from .symbols import LATEX
from . import ir as I
from .types import BoolTy, ComplexTy, ListTy, MatTy, NumTy, SolTy, StrTy, TextListTy, VecTy
from .units import lookup_unit, preferred_unit

WORD = re.compile(r"[A-Za-z_\u0370-\u03ff\u1f00-\u1fffħ][A-Za-z0-9_\u0370-\u03ff\u1f00-\u1fffħ₀-₉]*")


@dataclass
class Problem:
    line: int          # 1-based, like Fermium's messages
    col: int           # 1-based
    length: int
    message: str
    severity: str      # "error" or "warning"
    hint: str | None = None


@dataclass
class Analysis:
    problems: list = field(default_factory=list)
    checker: Checker | None = None      # the checker after the longest prefix that checks


def _check(source, base_dir):
    diags = Diagnostics()
    ck = Checker(diags, base_dir)
    ck.check_program(parse(source, diags))
    return ck, diags


def analyze(source: str, base_dir: str = ".") -> Analysis:
    """Check a program; collect its error (if any) and warnings, and keep the symbols for hover."""
    an = Analysis()
    try:
        ck, diags = _check(source, base_dir)
        an.checker = ck
    except FermiumError as e:
        diags = None
        an.problems.append(Problem(e.line or 1, e.col or 1, e.length, e.message, "error", e.hint))
        # hover still works for everything above the error
        lines = source.split("\n")
        stop = (e.line or 1) - 1
        while stop > 0 and an.checker is None:
            try:
                an.checker, _ = _check("\n".join(lines[:stop]) + "\n", base_dir)
            except FermiumError:
                stop -= 1
            except Exception:
                break
    except Exception as e:      # never let a compiler bug take the editor down
        an.problems.append(Problem(1, 1, 1, f"internal error in Fermium: {type(e).__name__}: {e}", "error"))
        return an
    if diags is not None:
        for w in diags.warnings:
            an.problems.append(Problem(w.line or 1, w.col or 1, w.length or 1, w.message, "warning", w.hint))
    return an


def word_at(source: str, line: int, char: int):
    """The identifier touching the 0-based (line, char) position, or None."""
    lines = source.split("\n")
    if not 0 <= line < len(lines):
        return None
    text = lines[line]
    for m in WORD.finditer(text):
        if m.start() <= char <= m.end():
            return m.group()
    return None


def _type_text(ck: Checker, ty, hint=None):
    if isinstance(ty, NumTy):
        d = ck.U.resolve(ty.dim)
        s = ck.desc(d) if not d.dimensionless else "a plain number (no units)"
        hname = getattr(hint, "name", "")
        if hname not in ("", "1") and hname != preferred_unit(d).name:
            s += f", shown in {hname}"
        return s
    if isinstance(ty, ListTy):
        return "a list of " + _type_text(ck, NumTy(ty.dim), hint).replace("a plain number", "plain numbers")
    if isinstance(ty, ComplexTy):
        return "a complex number of " + _type_text(ck, NumTy(ty.dim), hint).replace("a plain number (no units)",
                                                                                   "plain numbers")
    if isinstance(ty, VecTy) and getattr(ty, "mixed", False):
        return f"a {ty.n}-D vector of (" + ", ".join(ck.desc(ck.U.resolve(d)) for d in ty.dims) + ")"
    if isinstance(ty, VecTy):
        return f"a {ty.n}-D vector of " + _type_text(ck, NumTy(ty.dim), hint)
    if isinstance(ty, MatTy):
        return f"a {ty.r}×{ty.c} matrix of " + _type_text(ck, NumTy(ty.dim), hint)
    if isinstance(ty, TextListTy):
        return "a list of text"
    if isinstance(ty, StrTy):
        return "text"
    if isinstance(ty, BoolTy):
        return "true or false"
    if isinstance(ty, SolTy):
        return "the solution of an ODE"
    return getattr(ty, "kind", "a value")


def hover_text(an: Analysis, source: str, line: int, char: int):
    """Markdown for the name at the 0-based position, or None."""
    name = word_at(source, line, char)
    if not name:
        return None
    ck = an.checker
    b = ck.globals.names.get(name) if ck is not None else None
    if isinstance(b, I.Sym):
        return f"**{name}**: {_type_text(ck, b.ty, getattr(b, 'hint', None))}"
    if isinstance(b, FuncInfo):
        try:
            return f"```fermium\n{ck.describe_function(b)}\n```"
        except Exception:
            return f"**{name}**: a function"
    if isinstance(b, ModuleRef):
        names = b.info.exported()
        more = ", …" if len(names) > 12 else ""
        return f"**{name}**: the module {b.info.name} ({b.info.display}): {', '.join(names[:12])}{more}"
    if isinstance(b, SolView):
        d = ck.U.resolve(b.dim)
        what = "vector " if getattr(b, "n", 1) > 1 else ""
        return f"**{name}**: {what}solution of an ODE, {ck.desc(d)}; use {name}(t), {name}'(t), max({name}), times({name})"
    consts = all_constants()
    if name in consts:
        val, unit, desc = consts[name]
        return f"**{name}**: {desc}  \n= {val:.10g} {unit.name}"
    u = lookup_unit(name)
    if u is not None:
        base = preferred_unit(u.dim)
        dname = ck.desc(u.dim) if ck is not None else base.name
        return f"**{name}**: unit of {dname}" + (f", = {u.factor:.10g} {base.name}" if base.name != name else "")
    if name in KEYWORDS:
        return f"**{name}**: keyword"
    return None


def completions(an: Analysis, source: str, line: int, char: int):
    """Completion items as (label, insert_text, start_char, detail) for the 0-based position."""
    lines = source.split("\n")
    text = lines[line][:char] if 0 <= line < len(lines) else ""
    m = re.search(r"\\([A-Za-z]*|\^-?\d?|_\d?)$", text)
    if m:
        start = char - len(m.group(0))
        return [(f"\\{k}", v, start, v) for k, v in sorted(LATEX.items()) if k.startswith(m.group(1))]
    m = WORD.search(text[::-1])
    prefix = ""
    if text and m and m.start() == 0:
        prefix = m.group()[::-1]
    start = char - len(prefix)
    out = []
    mod = re.search(r"(" + WORD.pattern + r")\.$", text[:start])
    b = an.checker.globals.names.get(mod.group(1)) if mod and an.checker is not None else None
    if isinstance(b, ModuleRef):                 # springs.<Tab>: the module's names
        return [(n, n, start, "") for n in b.info.exported() if n.startswith(prefix)]
    names = sorted(an.checker.globals.names) if an.checker is not None else []
    for n in names:
        if n.startswith(prefix) and not n.startswith("__") and "'" not in n and "_∂" not in n:
            out.append((n, n, start, hover_text(an, n, 0, 0) or ""))
    for k in sorted(KEYWORDS):
        if k.startswith(prefix) and prefix:
            out.append((k, k, start, "keyword"))
    return out


# ------------------------------------------------------------------ the LSP server
def serve():                      # pragma: no cover - exercised by tests/test_lsp.py over stdio
    from lsprotocol import types as T
    from pygls.lsp.server import LanguageServer

    server = LanguageServer("fermium", "0.1.0")
    cache = {}

    def doc_dir(uri):
        path = uri[7:] if uri.startswith("file://") else ""
        return os.path.dirname(path) or "."

    def refresh(ls, uri):
        doc = ls.workspace.get_text_document(uri)
        an = analyze(doc.source, doc_dir(uri))
        cache[uri] = an
        diags = []
        for p in an.problems:
            ln, c = max(p.line - 1, 0), max(p.col - 1, 0)
            msg = p.message + (f"\nhint: {p.hint}" if p.hint else "")
            diags.append(T.Diagnostic(
                range=T.Range(T.Position(ln, c), T.Position(ln, c + max(p.length, 1))), message=msg,
                severity=T.DiagnosticSeverity.Error if p.severity == "error" else T.DiagnosticSeverity.Warning,
                source="fermium"))
        ls.text_document_publish_diagnostics(T.PublishDiagnosticsParams(uri=uri, diagnostics=diags))

    @server.feature(T.TEXT_DOCUMENT_DID_OPEN)
    def did_open(ls, params):
        refresh(ls, params.text_document.uri)

    @server.feature(T.TEXT_DOCUMENT_DID_CHANGE)
    def did_change(ls, params):
        refresh(ls, params.text_document.uri)

    @server.feature(T.TEXT_DOCUMENT_DID_SAVE)
    def did_save(ls, params):
        refresh(ls, params.text_document.uri)

    @server.feature(T.TEXT_DOCUMENT_HOVER)
    def hover(ls, params):
        uri = params.text_document.uri
        doc = ls.workspace.get_text_document(uri)
        an = cache.get(uri) or analyze(doc.source, doc_dir(uri))
        text = hover_text(an, doc.source, params.position.line, params.position.character)
        if text is None:
            return None
        return T.Hover(contents=T.MarkupContent(kind=T.MarkupKind.Markdown, value=text))

    @server.feature(T.TEXT_DOCUMENT_COMPLETION, T.CompletionOptions(trigger_characters=["\\"]))
    def complete(ls, params):
        uri = params.text_document.uri
        doc = ls.workspace.get_text_document(uri)
        an = cache.get(uri) or analyze(doc.source, doc_dir(uri))
        ln, ch = params.position.line, params.position.character
        items = []
        for label, insert, start, detail in completions(an, doc.source, ln, ch):
            items.append(T.CompletionItem(
                label=label, detail=detail, filter_text=label,
                text_edit=T.TextEdit(range=T.Range(T.Position(ln, start), T.Position(ln, ch)), new_text=insert)))
        return T.CompletionList(is_incomplete=False, items=items)

    server.start_io()


if __name__ == "__main__":        # pragma: no cover
    serve()
