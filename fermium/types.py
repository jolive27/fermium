"""Types and dimension inference.

Dimensions during checking are *affine expressions* over unknowns:

    D = D0 · x1^a1 · x2^a2 ...     (written additively on exponents: D0 + a1 x1 + ...)

where D0 is a concrete Dim and each xi is an unknown dimension (e.g. the units of
a plain `0`, of a function parameter, or of a parameter being fitted).  Checking
`a + b` unifies dim(a) and dim(b), which is a linear equation on the exponents; we
solve it by substitution (Kennedy-style dimension inference).  This is how
`E = 0` followed by `E += ½ m v²` learns that E is an energy, and how
`fit T = 2π √(L/g)` works out that g is an acceleration.
"""
from __future__ import annotations

from fractions import Fraction
from itertools import count

from .units import Dim, DIMLESS, dim_name, format_dim

_ids = count(1)


class DimVar:
    __slots__ = ("id", "name")

    def __init__(self, name=""):
        self.id = next(_ids)
        self.name = name

    def __repr__(self):
        return f"?{self.name or self.id}"


class DExpr:
    """const Dim + sum(coeff * var)."""

    __slots__ = ("const", "terms")

    def __init__(self, const: Dim = DIMLESS, terms=None):
        self.const = const
        self.terms = {k: v for k, v in (terms or {}).items() if v != 0}

    @staticmethod
    def of(d):
        if isinstance(d, DExpr):
            return d
        if isinstance(d, DimVar):
            return DExpr(DIMLESS, {d: Fraction(1)})
        return DExpr(d)

    @staticmethod
    def fresh(name=""):
        return DExpr.of(DimVar(name))

    def __mul__(self, o):  # dimension product = add exponents
        o = DExpr.of(o)
        t = dict(self.terms)
        for k, v in o.terms.items():
            t[k] = t.get(k, 0) + v
        return DExpr(self.const * o.const, t)

    def __truediv__(self, o):
        o = DExpr.of(o)
        t = dict(self.terms)
        for k, v in o.terms.items():
            t[k] = t.get(k, 0) - v
        return DExpr(self.const / o.const, t)

    def __pow__(self, p):
        p = Fraction(p)
        return DExpr(self.const ** p, {k: v * p for k, v in self.terms.items()})

    @property
    def concrete(self):
        return not self.terms

    def __repr__(self):
        s = format_dim(self.const) or "1"
        for k, v in self.terms.items():
            s += f" {k}^{v}"
        return f"DExpr({s})"


class Unifier:
    """Holds the substitution for dimension variables."""

    def __init__(self):
        self.subst = {}   # DimVar -> DExpr

    def norm(self, d) -> DExpr:
        d = DExpr.of(d)
        if not d.terms:
            return d
        out = DExpr(d.const)
        for var, c in d.terms.items():
            if var in self.subst:
                out = out * (self.norm(self.subst[var]) ** c)
            else:
                out = out * (DExpr.of(var) ** c)
        return out

    def unify(self, a, b) -> bool:
        """Make a == b; return False if impossible."""
        diff = self.norm(DExpr.of(a) / DExpr.of(b))
        if not diff.terms:
            return diff.const.dimensionless
        # pick the variable with the "simplest" coefficient
        var, c = min(diff.terms.items(), key=lambda kv: (abs(kv[1]) != 1, kv[0].id))
        rest = DExpr(diff.const, {k: v for k, v in diff.terms.items() if k is not var})
        # c*var + rest = 0  ->  var = -rest / c
        self.subst[var] = rest ** (Fraction(-1) / c)
        return True

    def resolve(self, d) -> Dim:
        """Concrete Dim, defaulting any still-unknown variable to dimensionless."""
        n = self.norm(d)
        return n.const

    def is_concrete(self, d) -> bool:
        return self.norm(d).concrete

    def describe(self, d) -> str:
        return dim_name(self.resolve(d))


# ---------------------------------------------------------------- value types
class Ty:
    kind = "?"

    def __eq__(self, o):
        return type(self) is type(o) and self.key() == o.key()

    def __hash__(self):
        return hash((type(self).__name__, self.key()))

    def key(self):
        return ()


class NumTy(Ty):
    kind = "num"

    def __init__(self, dim):
        self.dim = DExpr.of(dim)

    def key(self):
        return (tuple(self.dim.const.e), tuple(sorted((k.id, v) for k, v in self.dim.terms.items())))

    def __repr__(self):
        return f"Num[{self.dim}]"


class BoolTy(Ty):
    kind = "bool"

    def __repr__(self):
        return "Bool"


class StrTy(Ty):
    kind = "str"

    def __repr__(self):
        return "Str"


class ListTy(Ty):
    kind = "list"

    def __init__(self, dim):
        self.dim = DExpr.of(dim)

    def key(self):
        return NumTy(self.dim).key()

    def __repr__(self):
        return f"List[{self.dim}]"


class VecTy(Ty):
    """A small fixed-length vector (2 or 3 components) sharing one dimension: <3, 4> m/s."""
    kind = "vec"

    def __init__(self, dim, n):
        self.dim = DExpr.of(dim)
        self.n = n

    def key(self):
        return NumTy(self.dim).key() + (self.n,)

    def __repr__(self):
        return f"Vec{self.n}[{self.dim}]"


class SolTy(Ty):
    """Handle to an ODE solution (all components share one handle)."""
    kind = "sol"

    def __init__(self, info):
        self.info = info

    def key(self):
        return (id(self.info),)


class DataTy(Ty):
    kind = "data"

    def __init__(self, info):
        self.info = info

    def key(self):
        return (id(self.info),)


class VoidTy(Ty):
    kind = "void"


BOOL = BoolTy()
STR = StrTy()
VOID = VoidTy()


def num(d=DIMLESS):
    return NumTy(d)


def type_desc(t: Ty, U: Unifier) -> str:
    if isinstance(t, NumTy):
        return U.describe(t.dim)
    if isinstance(t, ListTy):
        return f"a list of {U.describe(t.dim)}"
    if isinstance(t, VecTy):
        return f"a {t.n}-vector of {U.describe(t.dim)}"
    return {"bool": "true/false value", "str": "text", "sol": "ODE solution", "data": "data table",
            "void": "nothing"}.get(t.kind, t.kind)
