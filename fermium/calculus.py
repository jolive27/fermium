"""Symbolic calculus on the AST: derivatives, simplification, pretty printing,
isolating the highest derivative of an ODE, and (via SymPy) indefinite integrals.

Derivatives are our own rule-based implementation (spec §3.5).
"""
from __future__ import annotations

import math

from . import ast as A
from .errors import FermiumError
from .units import format_number, is_unit_name
from .lexer import GREEK_TO_ASCII


# ---------------------------------------------------------------- constructors
def num(v):
    return A.Num(float(v), None, False)


def is_num(e, v=None):
    if isinstance(e, A.Num):
        return v is None or e.value == v
    return False


def name(n):
    return A.Name(n)


def call(f, *args):
    return A.Call(A.Name(f) if isinstance(f, str) else f, list(args))


def add(a, b):
    return A.BinOp("+", a, b)


def sub(a, b):
    return A.BinOp("-", a, b)


def mul(a, b):
    return A.BinOp("*", a, b, implicit=True)


def div(a, b):
    return A.BinOp("/", a, b)


def pw(a, b):
    return A.BinOp("^", a, b if isinstance(b, A.Node) else num(b))


def neg(a):
    return A.Neg(a)


# ---------------------------------------------------------------- substitution
def subst(e, mapping):
    """Replace free Names by expressions (mapping: name -> Node)."""
    if isinstance(e, A.Name):
        return mapping.get(e.name, e)
    if isinstance(e, A.Integral):
        inner = {k: v for k, v in mapping.items() if k != e.var}
        return _copy(e, integrand=subst(e.integrand, inner),
                     lo=subst(e.lo, mapping) if e.lo is not None else None,
                     hi=subst(e.hi, mapping) if e.hi is not None else None)
    if isinstance(e, A.Where):
        bound = {b for b, _ in e.bindings}
        inner = {k: v for k, v in mapping.items() if k not in bound}
        return _copy(e, value=subst(e.value, inner), bindings=[(b, subst(v, mapping)) for b, v in e.bindings])
    return map_children(e, lambda c: subst(c, mapping))


def _copy(e, **changes):
    import copy
    n = copy.copy(e)
    for k, v in changes.items():
        setattr(n, k, v)
    return n


def map_children(e, f):
    if isinstance(e, (A.Num, A.Str, A.Bool, A.Name, A.End, A.Load)):
        return e
    if isinstance(e, A.Quantity):
        return _copy(e, value=f(e.value))
    if isinstance(e, (A.BinOp, A.Compare, A.Logic)):
        return _copy(e, left=f(e.left), right=f(e.right))
    if isinstance(e, (A.Neg, A.Not, A.Sqrt, A.Abs)):
        return _copy(e, operand=f(e.operand))
    if isinstance(e, A.Call):
        return _copy(e, func=f(e.func), args=[f(a) for a in e.args])
    if isinstance(e, A.Index):
        return _copy(e, target=f(e.target), index=f(e.index))
    if isinstance(e, (A.Field, A.Prime)):
        return _copy(e, target=f(e.target))
    if isinstance(e, A.Deriv):
        return _copy(e, operand=f(e.operand))
    if isinstance(e, A.Integral):
        return _copy(e, integrand=f(e.integrand), lo=f(e.lo) if e.lo is not None else None,
                     hi=f(e.hi) if e.hi is not None else None)
    if isinstance(e, (A.ListLit, A.VecLit)):
        return _copy(e, items=[f(x) for x in e.items])
    if isinstance(e, A.IfExpr):
        return _copy(e, cond=f(e.cond), then=f(e.then), other=f(e.other))
    if isinstance(e, A.Convert):
        return _copy(e, value=f(e.value))
    if isinstance(e, A.Where):
        return _copy(e, value=f(e.value), bindings=[(b, f(v)) for b, v in e.bindings])
    return e


def inline_where(e):
    """Replace `a where x = b` by a[x := b] everywhere."""
    e = map_children(e, inline_where)
    if isinstance(e, A.Where):
        val = e.value
        for b, v in reversed(e.bindings):
            val = subst(val, {b: v})
        return val
    return e


def depends_on(e, var):
    return var in A.free_names(e)


# ---------------------------------------------------------------- differentiation
BUILTIN_DERIVS = {
    # f: u -> f'(u)
    "sin": lambda u: call("cos", u),
    "cos": lambda u: neg(call("sin", u)),
    "tan": lambda u: div(num(1), pw(call("cos", u), 2)),
    "exp": lambda u: call("exp", u),
    "ln": lambda u: div(num(1), u),
    "log": lambda u: div(num(1), u),
    "log10": lambda u: div(num(1), mul(u, call("ln", num(10)))),
    "log2": lambda u: div(num(1), mul(u, call("ln", num(2)))),
    "sinh": lambda u: call("cosh", u),
    "cosh": lambda u: call("sinh", u),
    "tanh": lambda u: div(num(1), pw(call("cosh", u), 2)),
    "asin": lambda u: div(num(1), A.Sqrt(sub(num(1), pw(u, 2)))),
    "acos": lambda u: neg(div(num(1), A.Sqrt(sub(num(1), pw(u, 2))))),
    "atan": lambda u: div(num(1), add(num(1), pw(u, 2))),
    "sqrt": lambda u: div(num(1), mul(num(2), A.Sqrt(u))),
    "cbrt": lambda u: div(num(1), mul(num(3), pw(A.Sqrt(u, 3), 2))),
    "abs": lambda u: call("sign", u),
    "erf": lambda u: mul(div(num(2), A.Sqrt(name("π"))), call("exp", neg(pw(u, 2)))),
}


class DiffContext:
    """What the differentiator needs to know about names.  The checker subclasses this."""

    def user_function(self, fname):
        """Return (param_names, body_ast) for a one-line user function, or None."""
        return None

    def derived_function(self, fname, param_index, order=1):
        """Name of the function ∂f/∂(param i); registers it if needed."""
        raise NotImplementedError

    def is_solution(self, fname):
        return False

    def is_builtin(self, fname):
        return fname in BUILTIN_DERIVS


def diff(e, var, ctx: DiffContext):
    """d e / d var, as a (simplified) AST."""
    return factor_common(simplify(_d(inline_where(e), var, ctx)))


def _sum_terms(e, sign=1):
    if isinstance(e, A.BinOp) and e.op in "+-" and not e.paren:
        return _sum_terms(e.left, sign) + _sum_terms(e.right, sign if e.op == "+" else -sign)
    if isinstance(e, A.Neg):
        return _sum_terms(e.operand, -sign)
    return [(sign, e)]


def factor_common(e):
    """Pull factors shared by every term of a sum out front: 2 e^u - 4 x² e^u -> (2 - 4x²) e^u."""
    if isinstance(e, A.Neg) and isinstance(e.operand, A.BinOp) and e.operand.op in "+-":
        e = _copy(e, operand=map_children(e.operand, factor_common))
    else:
        e = map_children(e, factor_common)
    terms = _sum_terms(e)
    if len(terms) < 2:
        return e
    from collections import Counter
    fl = []
    for sign, t in terms:
        c, fs = _factors(t)
        fs = [f for f in fs if not isinstance(f, A.Num)]
        fl.append((sign * c, Counter(key(f) for f in fs), {key(f): f for f in fs}, fs))
    # a factor is common if every term has it; take the smallest number of copies (x·x·y and x·y share one x)
    common = Counter(fl[0][1])
    for _, cnt, _, _ in fl[1:]:
        common &= cnt
    if not common:
        return e
    rest = None
    for c, cnt, byk, fs in sorted(fl, key=lambda x: x[0] < 0):
        drop = Counter(common)
        remaining = []
        for f in fs:
            k = key(f)
            if drop[k] > 0:
                drop[k] -= 1
            else:
                remaining.append(f)
        t = _build_product(abs(c), remaining)
        if rest is None:
            rest = neg(t) if c < 0 else t
        else:
            rest = sub(rest, t) if c < 0 else add(rest, t)
    out = rest
    for k, n in common.items():
        for _ in range(n):
            out = mul(out, fl[0][2][k])
    return simplify(out)


def _d(e, var, ctx):
    if isinstance(e, (A.Num, A.Str, A.Bool)):
        return num(0)
    if isinstance(e, A.Quantity):
        return num(0) if not depends_on(e.value, var) else mul(_d(e.value, var, ctx), _copy(e, value=num(1)))
    if isinstance(e, A.Name):
        return num(1) if e.name == var else num(0)
    if isinstance(e, A.Convert):
        return _d(e.value, var, ctx)
    if not depends_on(e, var) and not isinstance(e, (A.Deriv,)):
        return num(0)
    if isinstance(e, A.BinOp):
        a, b = e.left, e.right
        if e.op == "+":
            return add(_d(a, var, ctx), _d(b, var, ctx))
        if e.op == "-":
            return sub(_d(a, var, ctx), _d(b, var, ctx))
        if e.op == "*":
            return add(mul(_d(a, var, ctx), b), mul(a, _d(b, var, ctx)))
        if e.op == "/":
            if not depends_on(b, var):
                return div(_d(a, var, ctx), b)
            return div(sub(mul(_d(a, var, ctx), b), mul(a, _d(b, var, ctx))), pw(b, 2))
        if e.op == "^":
            if not depends_on(b, var):
                return mul(mul(b, pw(a, simplify(sub(b, num(1))))), _d(a, var, ctx))
            if not depends_on(a, var):
                return mul(mul(e, call("ln", a)), _d(b, var, ctx))
            return mul(e, add(mul(_d(b, var, ctx), call("ln", a)), div(mul(b, _d(a, var, ctx)), a)))
    if isinstance(e, A.Neg):
        return neg(_d(e.operand, var, ctx))
    if isinstance(e, A.Sqrt):
        u = e.operand
        if e.root == 2:
            return div(_d(u, var, ctx), mul(num(2), A.Sqrt(u)))
        return div(_d(u, var, ctx), mul(num(e.root), pw(A.Sqrt(u, e.root), e.root - 1)))
    if isinstance(e, A.Abs):
        return mul(call("sign", e.operand), _d(e.operand, var, ctx))
    if isinstance(e, A.IfExpr):
        return A.IfExpr(e.cond, _d(e.then, var, ctx), _d(e.other, var, ctx))
    if isinstance(e, A.VecLit):
        return A.VecLit([_d(x, var, ctx) for x in e.items])
    if isinstance(e, A.Deriv):
        inner = diff(e.operand, e.var, ctx) if not isinstance(e.operand, A.Name) else None
        if inner is None:
            raise FermiumError("can't differentiate this derivative expression symbolically", e.line, e.col)
        return _d(inner, var, ctx)
    if isinstance(e, A.Call):
        f = e.func
        if isinstance(f, A.Name):
            fname = f.name
            if len(e.args) == 1 and fname in BUILTIN_DERIVS:
                u = e.args[0]
                return mul(BUILTIN_DERIVS[fname](u), _d(u, var, ctx))
            if ctx.is_solution(fname) and len(e.args) == 1:
                u = e.args[0]
                return mul(A.Call(A.Prime(A.Name(fname), 1), [u]), _d(u, var, ctx))
            uf = ctx.user_function(fname)
            if uf is not None:
                params, _body = uf
                terms = None
                for i, arg in enumerate(e.args):
                    da = _d(arg, var, ctx)
                    if is_num(simplify(da), 0):
                        continue
                    dname = ctx.derived_function(fname, i)
                    t = mul(A.Call(A.Name(dname), list(e.args)), da)
                    terms = t if terms is None else add(terms, t)
                return terms if terms is not None else num(0)
            if fname in ("min", "max", "floor", "ceil", "round", "sign", "atan2", "hypot"):
                if fname == "hypot" and len(e.args) == 2:
                    a, b = e.args
                    return div(add(mul(a, _d(a, var, ctx)), mul(b, _d(b, var, ctx))), e)
                if fname in ("floor", "ceil", "round", "sign"):
                    return num(0)
            raise FermiumError(f"can't differentiate {fname}(...) symbolically", e.line, e.col,
                               hint="differentiate a formula made of +, -, ×, /, powers and standard functions")
        if isinstance(f, A.Prime) and isinstance(f.target, A.Name) and ctx.is_solution(f.target.name):
            u = e.args[0]
            return mul(A.Call(A.Prime(f.target, f.order + 1), [u]), _d(u, var, ctx))
        if isinstance(f, A.Prime) and isinstance(f.target, A.Name) and ctx.user_function(f.target.name):
            dn = ctx.derived_function(f.target.name, 0, f.order)
            return _d(A.Call(A.Name(dn), e.args), var, ctx)
    if isinstance(e, A.Integral) and e.lo is not None:
        # Fundamental theorem of calculus for variable limits.
        res = num(0)
        if depends_on(e.integrand, var):
            raise FermiumError("can't differentiate an integral whose integrand depends on the variable",
                               e.line, e.col)
        if e.hi is not None and depends_on(e.hi, var):
            res = add(res, mul(subst(e.integrand, {e.var: e.hi}), _d(e.hi, var, ctx)))
        if depends_on(e.lo, var):
            res = sub(res, mul(subst(e.integrand, {e.var: e.lo}), _d(e.lo, var, ctx)))
        return res
    raise FermiumError("can't differentiate this expression symbolically", e.line, e.col)


# ---------------------------------------------------------------- simplification
def key(e):
    return to_source(e, pretty=False)


def _factors(e):
    """Flatten a product/quotient into (coefficient, [factors]); divisors become x^-1 factors."""
    if isinstance(e, A.BinOp) and e.op == "*":
        c1, f1 = _factors(e.left)
        c2, f2 = _factors(e.right)
        return c1 * c2, f1 + f2
    if isinstance(e, A.BinOp) and e.op == "/":
        c1, f1 = _factors(e.left)
        c2, f2 = _factors(e.right)
        if c2 == 0:
            return 1.0, [e]
        return c1 / c2, f1 + [_inverse(f) for f in f2]
    if isinstance(e, A.Neg):
        c, f = _factors(e.operand)
        return -c, f
    if isinstance(e, A.Num):
        return e.value, []
    if isinstance(e, A.Quantity) and isinstance(e.value, A.Num) and e.value.value != 1:
        return e.value.value, [_copy(e, value=num(1))]
    return 1.0, [e]


def _inverse(f):
    if isinstance(f, A.BinOp) and f.op == "^" and is_num(f.right):
        return pw(f.left, num(-f.right.value))
    return pw(f, num(-1))


def _rank(x):
    if isinstance(x, A.Name):
        return 0
    if isinstance(x, A.BinOp) and x.op == "^" and isinstance(x.left, A.Name):
        return 1
    if isinstance(x, A.Quantity):
        return 2
    return 3


def _build_product(c, factors):
    # combine repeated factors into powers
    merged = []
    powers = {}
    for f in factors:
        base, p = (f.left, f.right.value) if isinstance(f, A.BinOp) and f.op == "^" and is_num(f.right) else (f, 1.0)
        k = key(base)
        if k in powers:
            powers[k][1] += p
        else:
            powers[k] = [base, p]
            merged.append(k)
    if c == 0:
        return num(0)
    top, bottom = [], []
    for k in merged:
        base, p = powers[k]
        if p > 0:
            top.append(base if p == 1 else pw(base, num(p)))
        elif p < 0:
            bottom.append(base if p == -1 else pw(base, num(-p)))
    # order: plain names, powers of names, quantities, then everything else (keeps "-A ω sin(ω t)")
    top.sort(key=_rank)
    bottom.sort(key=_rank)
    sign = -1 if c < 0 else 1
    c = abs(c)
    den_c = 1.0
    if c < 1 and c > 0:
        inv = 1 / c
        if abs(inv - round(inv)) < 1e-12 and round(inv) <= 1e6:
            den_c, c = float(round(inv)), 1.0
    out = None
    qs = [q for q in top if isinstance(q, A.Quantity) and isinstance(q.value, A.Num) and q.value.value == 1]
    if qs and c != 0:
        top = [q for q in top if q is not qs[0]]
        out = _copy(qs[0], value=num(c))       # 9 m/s³ rather than 9·1 m/s³
    elif c != 1 or not top:
        out = num(c)
    for it in top:
        out = it if out is None else mul(out, it)
    den = None
    qb = [q for q in bottom if isinstance(q, A.Quantity) and isinstance(q.value, A.Num) and q.value.value == 1]
    if den_c != 1 and qb:
        bottom = [q for q in bottom if q is not qb[0]]
        den = _copy(qb[0], value=num(den_c))
    elif den_c != 1:
        den = num(den_c)
    for it in bottom:
        den = it if den is None else mul(den, it)
    if den is not None:
        out = div(out, den)
    return neg(out) if sign < 0 else out


def simplify(e):
    e = map_children(e, simplify)
    if isinstance(e, A.Neg):
        o = e.operand
        if isinstance(o, A.Num):
            return num(-o.value)
        if isinstance(o, A.Neg):
            return o.operand
        return e
    if isinstance(e, A.BinOp):
        a, b = e.left, e.right
        if isinstance(a, A.Num) and isinstance(b, A.Num) and not a.digit and not b.digit or \
                isinstance(a, A.Num) and isinstance(b, A.Num):
            try:
                v = {"+": lambda: a.value + b.value, "-": lambda: a.value - b.value,
                     "*": lambda: a.value * b.value,
                     "/": lambda: a.value / b.value if b.value != 0 else None,
                     "^": lambda: a.value ** b.value}[e.op]()
                if v is not None and isinstance(v, float) and math.isfinite(v):
                    return num(v)
            except (OverflowError, ZeroDivisionError, ValueError):
                pass
        if e.op == "+":
            if is_num(a, 0):
                return b
            if is_num(b, 0):
                return a
            if isinstance(b, A.Neg):
                return simplify(sub(a, b.operand))
            if isinstance(b, A.Num) and b.value < 0:
                return sub(a, num(-b.value))
            if key(a) == key(b):
                return simplify(mul(num(2), a))
            return e
        if e.op == "-":
            if is_num(b, 0):
                return a
            if is_num(a, 0):
                return simplify(neg(b))
            if isinstance(b, A.Neg):
                return simplify(add(a, b.operand))
            if key(a) == key(b):
                return num(0)
            return e
        if e.op == "*":
            c, fs = _factors(e)
            return _build_product(c, fs)
        if e.op == "/":
            if is_num(b, 1):
                return a
            if is_num(a, 0):
                return num(0)
            if key(a) == key(b):
                return num(1)
            if not (isinstance(a, A.BinOp) and a.op in "+-") and not (isinstance(b, A.BinOp) and b.op in "+-"):
                c, fs = _factors(e)
                return _build_product(c, fs)
            if isinstance(a, A.Neg):
                return simplify(neg(div(a.operand, b)))
            if isinstance(b, A.Num) and b.value != 0:
                ca, fa = _factors(a)
                return _build_product(ca / b.value, fa)
            ca, fa = _factors(a)
            if ca != 1 and fa:
                inner = div(_build_product(1.0, fa), b)
                return _build_product(ca, [inner]) if ca != -1 else neg(inner)
            return e
        if e.op == "^":
            if is_num(b, 1):
                return a
            if is_num(b, 0):
                return num(1)
            if is_num(a, 1):
                return num(1)
            if isinstance(a, A.BinOp) and a.op == "^" and is_num(a.right) and is_num(b):
                return simplify(pw(a.left, num(a.right.value * b.value)))
            if isinstance(a, A.Sqrt) and a.root == 2 and is_num(b, 2):
                return a.operand
            return e
    if isinstance(e, A.Sqrt) and isinstance(e.operand, A.Num) and e.operand.value >= 0 and not e.operand.digit:
        return num(e.operand.value ** (1 / e.root))
    if isinstance(e, A.Call) and isinstance(e.func, A.Name) and len(e.args) == 1 and isinstance(e.args[0], A.Num):
        v = e.args[0].value
        if e.func.name in ("sin", "tan", "sinh", "tanh", "asin", "atan") and v == 0:
            return num(0)
        if e.func.name in ("cos", "cosh", "exp") and v == 0:
            return num(1)
    return e


# ---------------------------------------------------------------- printing
PREC_SUM, PREC_PROD, PREC_NEG, PREC_JUXT, PREC_POW, PREC_ATOM = 1, 2, 3, 4, 5, 6
SUPER = str.maketrans("0123456789-", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻")


def _name(n, pretty):
    if pretty:
        return n
    parts = n.split("_")
    return "_".join(GREEK_TO_ASCII.get(p, p) for p in parts)


def to_source(e, pretty=True) -> str:
    return _src(e, pretty)[0]


def _paren(s, p, need):
    return (f"({s})", PREC_ATOM) if p < need else (s, p)


def _src(e, pretty):
    if isinstance(e, A.Num):
        v = e.value
        if v == int(v) and abs(v) < 1e15:
            s = str(int(v))
        else:
            s = format_number(v, 12)
            if not pretty:
                s = s.replace("×10", "e").translate(str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-"))
        return (s, PREC_ATOM) if v >= 0 else (s, PREC_NEG)
    if isinstance(e, A.Name):
        return _name(e.name, pretty), PREC_ATOM
    if isinstance(e, A.Str):
        return f'"{e.value}"', PREC_ATOM
    if isinstance(e, A.Bool):
        return ("true" if e.value else "false"), PREC_ATOM
    if isinstance(e, A.Quantity):
        v, _ = _src(e.value, pretty)
        text = e.unit.text
        if pretty:
            from .checker import canonical_unit_name
            text = canonical_unit_name(e.unit) or text
        return (f"{v} [{text}]" if e.bracket else f"{v} {text}"), PREC_JUXT
    if isinstance(e, A.Neg):
        s, p = _src(e.operand, pretty)
        # -(a/b) and -a/b are the same number, so products and quotients need no parentheses
        s, p = _paren(s, p, PREC_PROD if isinstance(e.operand, A.BinOp) and e.operand.op in "*/" else PREC_JUXT)
        return "-" + s, PREC_NEG
    if isinstance(e, A.BinOp):
        if e.op in ("+", "-"):
            l, lp = _src(e.left, pretty)
            r, rp = _src(e.right, pretty)
            r, rp = _paren(r, rp, PREC_PROD if e.op == "+" else PREC_PROD)
            if e.op == "-" and rp == PREC_SUM:
                r = f"({r})"
            return f"{l} {e.op} {r}", PREC_SUM
        if e.op == "*":
            l, lp = _src(e.left, pretty)
            r, rp = _src(e.right, pretty)
            if lp >= PREC_JUXT and rp >= PREC_JUXT and not r.startswith("-"):
                sep = " "
                if isinstance(e.right, (A.Num, A.Quantity)):
                    sep = "·" if pretty else "*"
                elif isinstance(e.left, A.Num) and pretty and (
                        isinstance(e.right, A.Name) and not is_unit_name(e.right.name) or
                        isinstance(e.right, A.BinOp) and e.right.op == "^" and isinstance(e.right.left, A.Name)
                        and not is_unit_name(e.right.left.name)):
                    sep = ""
                return f"{l}{sep}{r}", PREC_JUXT
            l, lp = _paren(l, lp, PREC_PROD)
            r, rp = _paren(r, rp, PREC_NEG + 1)
            return f"{l}{'·' if pretty else '*'}{r}", PREC_PROD
        if e.op == "/":
            l, lp = _src(e.left, pretty)
            r, rp = _src(e.right, pretty)
            l, lp = _paren(l, lp, PREC_PROD)
            if rp <= PREC_JUXT:
                r = f"({r})"
            return f"{l}/{r}", PREC_PROD
        if e.op == "^":
            l, lp = _src(e.left, pretty)
            l, lp = _paren(l, lp, PREC_POW + 1)
            if is_num(e.right) and e.right.value == int(e.right.value) and pretty:
                return l + str(int(e.right.value)).translate(SUPER), PREC_POW
            r, rp = _src(e.right, pretty)
            r, rp = _paren(r, rp, PREC_ATOM)
            return f"{l}^{r}", PREC_POW
    if isinstance(e, A.Sqrt):
        s, _ = _src(e.operand, pretty)
        fn = ("√" if e.root == 2 else "∛") if pretty else ("sqrt" if e.root == 2 else "cbrt")
        return f"{fn}({s})", PREC_ATOM
    if isinstance(e, A.Abs):
        return f"|{_src(e.operand, pretty)[0]}|", PREC_ATOM
    if isinstance(e, A.Call):
        f, _ = _src(e.func, pretty)
        args = ", ".join(_src(a, pretty)[0] for a in e.args)
        return f"{f}({args})", PREC_ATOM
    if isinstance(e, A.Prime):
        s, p = _src(e.target, pretty)
        return s + "'" * e.order, PREC_ATOM
    if isinstance(e, A.Index):
        return f"{_src(e.target, pretty)[0]}[{_src(e.index, pretty)[0]}]", PREC_ATOM
    if isinstance(e, A.Field):
        return f"{_src(e.target, pretty)[0]}.{e.name}", PREC_ATOM
    if isinstance(e, A.Compare):
        return f"{_src(e.left, pretty)[0]} {e.op} {_src(e.right, pretty)[0]}", 0
    if isinstance(e, A.Logic):
        return f"{_src(e.left, pretty)[0]} {e.op} {_src(e.right, pretty)[0]}", 0
    if isinstance(e, A.Not):
        return f"not {_src(e.operand, pretty)[0]}", 0
    if isinstance(e, A.IfExpr):
        return (f"if {_src(e.cond, pretty)[0]} then {_src(e.then, pretty)[0]} "
                f"else {_src(e.other, pretty)[0]}"), 0
    if isinstance(e, A.ListLit):
        return "[" + ", ".join(_src(x, pretty)[0] for x in e.items) + "]", PREC_ATOM
    if isinstance(e, A.VecLit):
        return "<" + ", ".join(_src(x, pretty)[0] for x in e.items) + ">", PREC_ATOM
    if isinstance(e, A.Deriv):
        d = "∂" if e.partial and pretty else ("partial" if e.partial else "d")
        return f"{d}/{d}{e.var} {_src(e.operand, pretty)[0]}", PREC_JUXT
    if isinstance(e, A.Integral):
        s = f"{'∫' if pretty else 'integral'} {_src(e.integrand, pretty)[0]} d{e.var}"
        if e.lo is not None:
            s += f" from {_src(e.lo, pretty)[0]} to {_src(e.hi, pretty)[0]}"
        return s, 0
    if isinstance(e, A.Convert):
        return f"{_src(e.value, pretty)[0]} in {e.unit.text}", 0
    if isinstance(e, A.Where):
        b = ", ".join(f"{n} = {_src(v, pretty)[0]}" for n, v in e.bindings)
        return f"{_src(e.value, pretty)[0]} where {b}", 0
    if isinstance(e, A.End):
        return "end", PREC_ATOM
    if isinstance(e, A.Load):
        return f'load "{e.path}"', PREC_ATOM
    return f"<{type(e).__name__}>", PREC_ATOM


# ---------------------------------------------------------------- ODE helpers
def isolate(lhs, rhs, target, placeholder="__H__"):
    """Solve lhs = rhs for `target` (an AST node, e.g. x''), assuming linearity.

    Returns an AST for target, or raises FermiumError.
    """
    tkey = key(target)

    def repl(e):
        if key(e) == tkey:
            return A.Name(placeholder)
        return map_children(e, repl)

    D = simplify(repl(sub(lhs, rhs)))

    class Plain(DiffContext):
        def user_function(self, f):
            return None

    try:
        a = simplify(_d(D, placeholder, Plain()))
    except FermiumError:
        a = None
    if a is None or depends_on(a, placeholder) or is_num(a, 0):
        raise FermiumError(f"can't solve this equation for {to_source(target)}: it must appear linearly "
                           f"(like m x'' = ...)", lhs.line, lhs.col, hint="rewrite it as  x'' = <formula>")
    b = simplify(subst(D, {placeholder: num(0)}))
    return simplify(neg(div(b, a)))


# ---------------------------------------------------------------- SymPy bridge
def to_sympy(e, symbols):
    import sympy as sp

    def conv(e):
        if isinstance(e, A.Num):
            v = e.value
            return sp.Integer(int(v)) if v == int(v) else sp.Float(v)
        if isinstance(e, A.Name):
            if e.name == "π":
                return sp.pi
            if e.name not in symbols:
                symbols[e.name] = sp.Symbol(e.name, real=True)
            return symbols[e.name]
        if isinstance(e, A.BinOp):
            a, b = conv(e.left), conv(e.right)
            return {"+": a + b, "-": a - b, "*": a * b, "/": a / b, "^": a ** b}[e.op] if e.op != "^" else a ** b
        if isinstance(e, A.Neg):
            return -conv(e.operand)
        if isinstance(e, A.Sqrt):
            return conv(e.operand) ** sp.Rational(1, e.root)
        if isinstance(e, A.Call) and isinstance(e.func, A.Name):
            fns = {"sin": sp.sin, "cos": sp.cos, "tan": sp.tan, "exp": sp.exp, "ln": sp.log, "log": sp.log,
                   "sinh": sp.sinh, "cosh": sp.cosh, "tanh": sp.tanh, "asin": sp.asin, "acos": sp.acos,
                   "atan": sp.atan, "sqrt": sp.sqrt, "abs": sp.Abs}
            if e.func.name in fns and len(e.args) == 1:
                return fns[e.func.name](conv(e.args[0]))
        raise FermiumError("this integral is too complicated to do symbolically", e.line, e.col,
                           hint="give limits (from a to b) to compute it numerically")
    return conv(e)


def from_sympy(x):
    import sympy as sp
    if x.is_Integer:
        return num(int(x))
    if x.is_Rational:
        return div(num(x.p), num(x.q))
    if x.is_Float:
        return num(float(x))
    if x is sp.pi:
        return name("π")
    if x.is_Symbol:
        return name(x.name)
    if x.is_Add:
        args = list(x.args)
        out = from_sympy(args[0])
        for a in args[1:]:
            out = add(out, from_sympy(a))
        return out
    if x.is_Mul:
        args = list(x.args)
        out = from_sympy(args[0])
        for a in args[1:]:
            if a.is_Pow and a.exp.is_negative:
                out = div(out, from_sympy(a.base ** (-a.exp)))
            else:
                out = mul(out, from_sympy(a))
        return out
    if x.is_Pow:
        if x.exp == sp.Rational(1, 2):
            return A.Sqrt(from_sympy(x.base))
        if x.exp.is_negative:
            return div(num(1), from_sympy(x.base ** (-x.exp)))
        return pw(from_sympy(x.base), from_sympy(x.exp))
    names = {sp.sin: "sin", sp.cos: "cos", sp.tan: "tan", sp.exp: "exp", sp.log: "ln", sp.sinh: "sinh",
             sp.cosh: "cosh", sp.tanh: "tanh", sp.asin: "asin", sp.acos: "acos", sp.atan: "atan", sp.Abs: "abs",
             sp.erf: "erf"}
    if x.func in names:
        return call(names[x.func], *[from_sympy(a) for a in x.args])
    raise FermiumError(f"SymPy returned something Fermium can't use yet: {x}")


def integrate_symbolic(integrand, var):
    """Indefinite integral via SymPy.  Returns an AST (without +C)."""
    import sympy as sp
    syms = {}
    expr = to_sympy(inline_where(integrand), syms)
    v = syms.get(var) or sp.Symbol(var, real=True)
    res = sp.integrate(expr, v)
    if res.has(sp.Integral):
        raise FermiumError("SymPy couldn't find a formula for this integral",
                           hint="give limits (from a to b) to compute it numerically")
    return simplify(from_sympy(sp.simplify(res)))
