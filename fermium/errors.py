"""Error and warning reporting in plain physics language.

Every user-facing problem is a FermiumError with a line/column, one plain
sentence, a caret under the problem, and (usually) a suggestion.
"""
from __future__ import annotations


class FermiumError(Exception):
    """A problem in the user's program (never a bug in Fermium itself)."""

    def __init__(self, message, line=None, col=None, length=1, hint=None, kind="error"):
        super().__init__(message)
        self.message = message
        self.line = line
        self.col = col
        self.length = max(1, length or 1)
        self.hint = hint
        self.kind = kind

    def format(self, source: str | None = None, filename: str | None = None) -> str:
        where = f"line {self.line}" if self.line else ""
        if filename and self.line:
            where = f"{filename}, line {self.line}"
        head = f"{where}: {self.message}" if where else self.message
        out = [head]
        if source is not None and self.line:
            lines = source.split("\n")
            if 1 <= self.line <= len(lines):
                text = lines[self.line - 1].rstrip("\n")
                out.append("    " + text)
                if self.col:
                    out.append("    " + " " * (self.col - 1) + "^" * min(self.length, max(1, len(text) - self.col + 1)))
        if self.hint:
            out.append(f"  hint: {self.hint}")
        return "\n".join(out)

    def __str__(self):
        return self.format()


class FermiumRuntimeError(FermiumError):
    pass


class Warning_:
    def __init__(self, message, line=None, col=None, length=1, hint=None):
        self.message = message
        self.line = line
        self.col = col
        self.length = length
        self.hint = hint

    def format(self, source=None, filename=None):
        e = FermiumError(self.message, self.line, self.col, self.length, self.hint)
        s = e.format(source, filename)
        return "warning: " + s

    def __repr__(self):
        return f"Warning(line {self.line}: {self.message})"


class Diagnostics:
    """Collects warnings during compilation."""

    def __init__(self):
        self.warnings = []
        self._seen = set()

    def warn(self, message, tok=None, line=None, col=None, length=1, hint=None):
        if tok is not None:
            line, col, length = tok.line, tok.col, len(tok.raw)
        key = (message, line, col)
        if key in self._seen:
            return
        self._seen.add(key)
        self.warnings.append(Warning_(message, line, col, length, hint))


def err(message, tok=None, hint=None, node=None):
    """Build a FermiumError located at a token or AST node."""
    if tok is not None:
        return FermiumError(message, tok.line, tok.col, len(tok.raw), hint)
    if node is not None:
        return FermiumError(message, getattr(node, "line", None), getattr(node, "col", None),
                            getattr(node, "length", 1), hint)
    return FermiumError(message, hint=hint)
