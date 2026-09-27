"""Helpers for comparing Fermium ASTs in tests.

`dump(node)` turns an AST into nested tuples, ignoring source positions, the `paren`
flag and `UnitExpr.text` (a display hint), so two programs that mean the same thing
dump equal.  `sx(expr)` renders an expression as a short s-expression for readable
assertions, e.g. `(/ (* h c) (* (* λ k_B) T))`.
"""
from dataclasses import fields, is_dataclass
from fractions import Fraction

from fermium import ast as A
from fermium.errors import Diagnostics
from fermium.parser import parse

IGNORED = {"line", "col", "length", "paren"}


def dump(n):
    if isinstance(n, list):
        return tuple(dump(x) for x in n)
    if isinstance(n, tuple):
        return tuple(dump(x) for x in n)
    if isinstance(n, float):
        return round(n, 12)
    if is_dataclass(n):
        items = []
        for f in fields(n):
            if f.name in IGNORED:
                continue
            if isinstance(n, A.UnitExpr) and f.name == "text":
                continue
            if isinstance(n, A.Num) and f.name == "digit":
                # `½` (digit=False) and `(1/2)` differ only in whether a unit may follow
                continue
            items.append((f.name, dump(getattr(n, f.name))))
        return (type(n).__name__, tuple(items))
    return n


def parse_dump(src):
    return dump(parse(src, Diagnostics()))


def _num(v):
    if isinstance(v, Fraction):
        return str(v)
    return str(int(v)) if float(v).is_integer() else repr(v)


def _unit(u):
    parts = []
    for f in u.factors:
        parts.append(f.name if f.exp == 1 else f"{f.name}^{_num(f.exp)}")
    return " ".join(parts)


def sx(n):
    """Compact s-expression of an expression node."""
    if isinstance(n, A.Name):
        return n.name
    if isinstance(n, A.Num):
        return _num(n.value)
    if isinstance(n, A.Str):
        return repr(n.value)
    if isinstance(n, A.Bool):
        return "true" if n.value else "false"
    if isinstance(n, A.BinOp):
        return f"({n.op} {sx(n.left)} {sx(n.right)})"
    if isinstance(n, A.Neg):
        return f"(neg {sx(n.operand)})"
    if isinstance(n, A.Compare):
        return f"({n.op} {sx(n.left)} {sx(n.right)})"
    if isinstance(n, A.Logic):
        return f"({n.op} {sx(n.left)} {sx(n.right)})"
    if isinstance(n, A.Not):
        return f"(not {sx(n.operand)})"
    if isinstance(n, A.Call):
        return f"(call {sx(n.func)}" + "".join(" " + sx(a) for a in n.args) + ")"
    if isinstance(n, A.Index):
        return f"(index {sx(n.target)} {sx(n.index)})"
    if isinstance(n, A.End):
        return "end"
    if isinstance(n, A.Field):
        return f"(. {sx(n.target)} {n.name})"
    if isinstance(n, A.Prime):
        return f"(prime{n.order} {sx(n.target)})"
    if isinstance(n, A.Deriv):
        kind = "partial" if n.partial else "d"
        return f"({kind}/{n.var}^{n.order} {sx(n.operand)})"
    if isinstance(n, A.Integral):
        lim = f" {sx(n.lo)} {sx(n.hi)}" if n.lo is not None else ""
        return f"(int {sx(n.integrand)} d{n.var}{lim})"
    if isinstance(n, A.Sqrt):
        return f"({'sqrt' if n.root == 2 else 'cbrt'} {sx(n.operand)})"
    if isinstance(n, A.Abs):
        return f"(abs {sx(n.operand)})"
    if isinstance(n, A.ListLit):
        return "[" + ", ".join(sx(x) for x in n.items) + "]"
    if isinstance(n, A.IfExpr):
        return f"(if {sx(n.cond)} {sx(n.then)} {sx(n.other)})"
    if isinstance(n, A.Convert):
        return f"(in {sx(n.value)} [{_unit(n.unit)}])"
    if isinstance(n, A.Quantity):
        return f"(q {sx(n.value)} [{_unit(n.unit)}])"
    if isinstance(n, A.Where):
        return f"(where {sx(n.value)}" + "".join(f" {k}={sx(v)}" for k, v in n.bindings) + ")"
    if isinstance(n, A.Load):
        return f"(load {n.path!r})"
    return f"<{type(n).__name__}>"


def expr_of(src):
    """Parse `y = <src>` and return the expression node."""
    prog = parse("__y = " + src, Diagnostics())
    return prog.body[0].value


def sx_of(src):
    return sx(expr_of(src))
