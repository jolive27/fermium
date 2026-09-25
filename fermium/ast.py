"""Abstract syntax tree for Fermium programs.

Every node records where it came from (line, col, length) so errors can point at it.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from fractions import Fraction


@dataclass(eq=False)
class Node:
    line: int = field(default=0, kw_only=True, repr=False)
    col: int = field(default=0, kw_only=True, repr=False)
    length: int = field(default=1, kw_only=True, repr=False)
    paren: bool = field(default=False, kw_only=True, repr=False)

    def at(self, other):
        """Copy position info from another node or token; returns self."""
        self.line, self.col = other.line, other.col
        self.length = getattr(other, "length", None) or len(getattr(other, "raw", "") or "x")
        return self


# ---------------------------------------------------------------- units
@dataclass(eq=False)
class UnitFactor(Node):
    name: str            # raw spelling, e.g. "km", "°C", "μm"
    exp: Fraction = Fraction(1)


@dataclass(eq=False)
class UnitExpr(Node):
    factors: list        # list[UnitFactor]
    text: str = ""       # source text, for display hints

    def key(self):
        return tuple((f.name, f.exp) for f in self.factors)


# ---------------------------------------------------------------- expressions
@dataclass(eq=False)
class Num(Node):
    value: float
    sigfigs: int | None = None
    digit: bool = True


@dataclass(eq=False)
class Quantity(Node):
    """A number with units: `3 m/s`, `2 [kg]`, or `x [m]`."""
    value: Node
    unit: UnitExpr
    bracket: bool = False


@dataclass(eq=False)
class Str(Node):
    value: str


@dataclass(eq=False)
class Bool(Node):
    value: bool


@dataclass(eq=False)
class Name(Node):
    name: str


@dataclass(eq=False)
class BinOp(Node):
    op: str              # + - * / ^
    left: Node
    right: Node
    implicit: bool = False   # implicit multiplication (juxtaposition)


@dataclass(eq=False)
class Neg(Node):
    operand: Node


@dataclass(eq=False)
class Compare(Node):
    op: str              # == != < > <= >= ~=
    left: Node
    right: Node


@dataclass(eq=False)
class Logic(Node):
    op: str              # and / or
    left: Node
    right: Node


@dataclass(eq=False)
class Not(Node):
    operand: Node


@dataclass(eq=False)
class Call(Node):
    func: Node           # usually Name; may be Prime(Name)
    args: list


@dataclass(eq=False)
class Index(Node):
    target: Node
    index: Node


@dataclass(eq=False)
class Slice(Node):
    """`a:b` inside xs[...]: elements a to b, both included (1-based, D114); a or b may be omitted (None)."""
    lo: Node | None
    hi: Node | None


@dataclass(eq=False)
class End(Node):
    """`end` inside an index: the last element."""


@dataclass(eq=False)
class Field(Node):
    target: Node
    name: str


@dataclass(eq=False)
class Prime(Node):
    target: Node
    order: int


@dataclass(eq=False)
class Deriv(Node):
    """d/dt operand, d²/dt² operand, ∂/∂x operand."""
    var: str
    order: int
    operand: Node
    partial: bool = False


@dataclass(eq=False)
class Integral(Node):
    integrand: Node
    var: str
    lo: Node | None = None
    hi: Node | None = None


@dataclass(eq=False)
class Sum(Node):
    """Σ(body for var from lo to hi step st): a finite sum written in one line (#49, D51)."""
    body: Node
    var: str
    lo: Node
    hi: Node
    step: Node | None = None


@dataclass(eq=False)
class Sqrt(Node):
    operand: Node
    root: int = 2


@dataclass(eq=False)
class Abs(Node):
    operand: Node


@dataclass(eq=False)
class ListLit(Node):
    items: list


@dataclass(eq=False)
class VecCalc(Node):
    """∇f (grad), ∇·F (div), ∇×F (curl), ∇²f (lap) of a one-line function; a function itself."""
    kind: str
    func: Node


@dataclass(eq=False)
class VecLit(Node):
    """<3, 4> or <x, y, z>"""
    items: list


@dataclass(eq=False)
class IfExpr(Node):
    cond: Node
    then: Node
    other: Node


@dataclass(eq=False)
class Convert(Node):
    """`expr in unit` -- same value, displayed in another unit."""
    value: Node
    unit: UnitExpr


@dataclass(eq=False)
class Digits(Node):
    """`print x to 9 digits`"""
    value: Node
    digits: int


@dataclass(eq=False)
class Load(Node):
    path: str


@dataclass(eq=False)
class Where(Node):
    value: Node
    bindings: list       # list[(name, Node)]


@dataclass(eq=False)
class Uncertain(Node):
    """`a ± b`: a measured value with its standard uncertainty (D120).  `5.0 ± 0.2 m` gives the unit to both."""
    value: Node
    err: Node


@dataclass(eq=False)
class Propagate(Node):
    """`propagate montecarlo [N samples]` + a block of formulas (D123)."""
    samples: Node | None
    body: list


# ---------------------------------------------------------------- statements
@dataclass(eq=False)
class Assign(Node):
    name: str
    value: Node
    op: str = "="          # = += -= *= /=


@dataclass(eq=False)
class IndexAssign(Node):
    target: str
    index: Node
    value: Node
    op: str = "="


@dataclass(eq=False)
class Param(Node):
    name: str
    unit: UnitExpr | None = None


@dataclass(eq=False)
class FuncDef(Node):
    name: str
    params: list          # list[Param]
    body: object          # Node (one-liner) or list of statements
    where: list = field(default_factory=list)


@dataclass(eq=False)
class Print(Node):
    items: list


@dataclass(eq=False)
class PlotSeries(Node):
    y: Node
    x: Node
    lo: Node | None = None
    hi: Node | None = None


@dataclass(eq=False)
class Plot(Node):
    series: list
    out: str | None = None


@dataclass(eq=False)
class Equation(Node):
    lhs: Node
    rhs: Node


@dataclass(eq=False)
class Solve(Node):
    equations: list       # list[Equation]
    initial: list         # list[Equation] (lhs like x(0), x'(0))
    var: str
    lo: Node
    hi: Node
    step: Node | None = None
    method: str | None = None
    tolerance: Node | None = None
    until: Node | None = None     # the stop condition `until lhs = rhs` (an Equation, D39)
    absolute: list | None = None  # `absolute a[, b …]`: absolute tolerances, one per unit (D160)


@dataclass(eq=False)
class Fit(Node):
    model: Equation
    data: Node
    guesses: list = field(default_factory=list)   # list[(name, Node)]


@dataclass(eq=False)
class Analyze(Node):
    """`analyze pendulum: T [s] depends on L [m], m [kg], g`: Buckingham Π analysis (D70).

    target and inputs are Params (name + optional unit); `raw` maps each name to its spelling."""
    title: str | None
    target: Param
    inputs: list          # list[Param]
    raw: dict = field(default_factory=dict)


@dataclass(eq=False)
class If(Node):
    cond: Node
    then: list
    other: list | None = None


@dataclass(eq=False)
class For(Node):
    var: str
    lo: Node
    hi: Node
    step: Node | None
    body: list
    parallel: bool = False       # parallel for (M5, D152)


@dataclass(eq=False)
class ForIn(Node):
    var: str
    iterable: Node
    body: list


@dataclass(eq=False)
class While(Node):
    cond: Node
    body: list


@dataclass(eq=False)
class Return(Node):
    value: Node | None


@dataclass(eq=False)
class Break(Node):
    pass


@dataclass(eq=False)
class Continue(Node):
    pass


@dataclass(eq=False)
class ExprStmt(Node):
    value: Node


@dataclass(eq=False)
class Assert(Node):
    cond: Node
    message: str | None = None


@dataclass(eq=False)
class Units(Node):
    """`units natural(ħ = c = 1)` (the rest of the program) or `units nuclear:` + a block (D60)."""
    system: str
    consts: list = field(default_factory=list)
    body: list | None = None


@dataclass(eq=False)
class Import(Node):
    """`import mechanics`, `import "lib/x.fm" as x`, `from nuclear import semf_binding, Q_value as Q` (D100).

    names is None for a plain import (the module is bound to alias or its own name), else a list of
    (name, alias-or-None) for from-import."""
    module: str
    is_path: bool = False
    alias: str | None = None
    names: list | None = None


@dataclass(eq=False)
class Program(Node):
    body: list


# ---------------------------------------------------------------- utilities
def children(n):
    """Direct child nodes of an expression node (for generic walks)."""
    if isinstance(n, (Num, Str, Bool, Name, End, Load)):
        return []
    if isinstance(n, Quantity):
        return [n.value]
    if isinstance(n, (BinOp, Compare, Logic)):
        return [n.left, n.right]
    if isinstance(n, (Neg, Not, Sqrt, Abs)):
        return [n.operand]
    if isinstance(n, Call):
        return [n.func] + list(n.args)
    if isinstance(n, Index):
        return [n.target, n.index]
    if isinstance(n, Slice):
        return [x for x in (n.lo, n.hi) if x is not None]
    if isinstance(n, (Field, Prime)):
        return [n.target]
    if isinstance(n, Deriv):
        return [n.operand]
    if isinstance(n, Integral):
        return [x for x in (n.integrand, n.lo, n.hi) if x is not None]
    if isinstance(n, Sum):
        return [x for x in (n.body, n.lo, n.hi, n.step) if x is not None]
    if isinstance(n, (ListLit, VecLit)):
        return list(n.items)
    if isinstance(n, IfExpr):
        return [n.cond, n.then, n.other]
    if isinstance(n, (Convert, Digits)):
        return [n.value]
    if isinstance(n, Where):
        return [n.value] + [v for _, v in n.bindings]
    if isinstance(n, Uncertain):
        return [n.value, n.err]
    return []


def walk(n):
    yield n
    for c in children(n):
        yield from walk(c)


def free_names(n):
    """Names used in an expression (not counting integration variables bound inside)."""
    out = []
    if isinstance(n, Name):
        return [n.name]
    if isinstance(n, Integral):
        inner = [x for x in free_names(n.integrand) if x != n.var]
        out = inner
        for b in (n.lo, n.hi):
            if b is not None:
                out += free_names(b)
        return out
    if isinstance(n, Sum):
        out = [x for x in free_names(n.body) if x != n.var]
        for b in (n.lo, n.hi, n.step):
            if b is not None:
                out += free_names(b)
        return out
    if isinstance(n, Where):
        bound = {b for b, _ in n.bindings}
        out = [x for x in free_names(n.value) if x not in bound]
        for _, v in n.bindings:
            out += free_names(v)
        return out
    for c in children(n):
        out += free_names(c)
    return out


@dataclass(eq=False)
class PySig(Node):
    """One declared signature in `use python mod as m:` (D140): `f(x [m], n: int) -> list [J]`.

    params: list of (name, UnitExpr-or-None, is_int); ret_shape: None (follow the arguments), 'list' or
    'number'; ret_unit: UnitExpr or None (a plain number)."""
    name: str
    params: list
    ret_shape: str | None = None
    ret_unit: object = None


@dataclass(eq=False)
class UsePython(Node):
    """`use python numpy as np`, `use python scipy.special as sp`, with optional signatures (D140)."""
    module: str
    alias: str | None = None
    sigs: list = field(default_factory=list)
