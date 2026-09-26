"""Dimensional analysis: the Buckingham Π theorem with exact rational arithmetic (D70).

`analyze pendulum: T depends on L, m, g` asks which dimensionless groups can be made from
T, L, m and g, and what that says about T.  Everything here works on exponent vectors over
the 7 SI base dimensions, with `Fraction` entries, so the answer is exact (√, ∛, fifth roots).

Method (the classical "repeating variables" form of the Π theorem):

1. Build the dimension matrix: one column per quantity (target first), one row per base dimension.
2. Walk the inputs in the order written and keep each one that is independent of those already kept
   (Gaussian elimination on Fractions).  These r "repeating" quantities span every dimension the
   inputs can make; r is the rank of the matrix.
3. If the target's dimension is not a combination of them, no formula can give the target: error.
4. Every other quantity q (the target and the inputs not kept) makes one group
   q · Π rep^(−a), where rep^a has q's dimension (a unique rational solve).  That is n − r groups,
   and the target is in exactly one of them, with exponent 1: target = rep^a · f(other groups).
5. The other groups are rescaled to the nicest form (exponents that are integers or halves, small,
   mostly positive: Re = ρ v √A/μ rather than μ²/(ρ² v² A)).

A repeating quantity whose exponent is zero in every group "drops out" (the pendulum's mass).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from fractions import Fraction
from math import lcm

from .units import BASE_NAMES, Dim

_SUP = str.maketrans("0123456789-", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻")
_SUBSCRIPT = str.maketrans("0123456789", "₀₁₂₃₄₅₆₇₈₉")


class AnalysisError(Exception):
    """The analysis has no answer; the message says why in physics terms."""

    def __init__(self, message, hint=None, index=None):
        super().__init__(message)
        self.message = message
        self.hint = hint
        self.index = index      # which quantity the problem is about (0 = target), if any


@dataclass
class Analysis:
    target: str
    names: list                  # the inputs, in the order written
    dims: dict                   # name -> Dim
    rank: int
    repeating: list              # names of the repeating quantities
    groups: list                 # list[dict name -> Fraction]; groups[0] holds the target
    prefactor: dict              # name -> Fraction: target = C · Π name^exp · f(other groups)
    dropped: list = field(default_factory=list)       # [(name, reason)]

    @property
    def n(self):
        return 1 + len(self.names)


# ---------------------------------------------------------------- exact linear algebra
def _vec(d: Dim):
    return [Fraction(x) for x in d.e]


def rref(rows):
    """Reduced row echelon form of a list of Fraction rows; returns (matrix, pivot columns)."""
    m = [list(r) for r in rows]
    if not m:
        return m, []
    ncols = len(m[0])
    pivots = []
    r = 0
    for c in range(ncols):
        p = next((i for i in range(r, len(m)) if m[i][c] != 0), None)
        if p is None:
            continue
        m[r], m[p] = m[p], m[r]
        pv = m[r][c]
        m[r] = [x / pv for x in m[r]]
        for i in range(len(m)):
            if i != r and m[i][c] != 0:
                f = m[i][c]
                m[i] = [a - f * b for a, b in zip(m[i], m[r])]
        pivots.append(c)
        r += 1
        if r == len(m):
            break
    return m, pivots


def rank(columns):
    """Rank of the matrix whose columns are the given exponent vectors."""
    if not columns:
        return 0
    rows = [[col[i] for col in columns] for i in range(len(columns[0]))]
    return len(rref(rows)[1])


def solve_exact(columns, rhs):
    """Exponents a with Σ a_j columns_j = rhs (columns independent), or None if impossible."""
    if not columns:
        return [] if all(x == 0 for x in rhs) else None
    rows = [[col[i] for col in columns] + [rhs[i]] for i in range(len(rhs))]
    m, piv = rref(rows)
    k = len(columns)
    if k in piv:                          # a pivot in the right-hand column: inconsistent
        return None
    a = [Fraction(0)] * k
    for row, c in zip(m, piv):
        a[c] = row[k]
    return a


def nullspace(columns):
    """A basis of {x : Σ x_j columns_j = 0} (used to cross-check the groups in the tests)."""
    n = len(columns)
    if n == 0:
        return []
    rows = [[col[i] for col in columns] for i in range(len(columns[0]))]
    m, piv = rref(rows)
    basis = []
    for free in range(n):
        if free in piv:
            continue
        x = [Fraction(0)] * n
        x[free] = Fraction(1)
        for row, c in zip(m, piv):
            x[c] = -row[free]
        basis.append(x)
    return basis


# ---------------------------------------------------------------- the analysis
def _nicest(group, fixed=None):
    """Rescale a dimensionless group to its nicest form (Re = ρ v √A/μ, not μ²/(ρ² v² A))."""
    if fixed is not None:
        return group
    exps = [e for e in group.values() if e != 0]
    cands = {Fraction(1), Fraction(-1)}
    for e in exps:
        cands |= {1 / e, -1 / e}

    def score(s):
        g = [e * s for e in exps]
        den = max(x.denominator for x in g)
        return (den > 2, sum(abs(x) for x in g), sum(1 for x in g if x < 0), den, -s)
    s = min(cands, key=score)
    return {k: v * s for k, v in group.items()}


def _only_here(name, dims, names):
    """Base dimensions that `name` has and none of `names` has (for 'nothing else has mass')."""
    out = []
    for i, p in enumerate(dims[name].e):
        if p != 0 and all(dims[o].e[i] == 0 for o in names if o != name):
            out.append(BASE_NAMES[i])
    return out


def analyze(target, target_dim: Dim, inputs):
    """Buckingham Π analysis.  `inputs` is a list of (name, Dim) in the order written."""
    names = [n for n, _ in inputs]
    dims = {target: target_dim}
    for n, d in inputs:
        dims[n] = d
    if len(set(names)) != len(names):
        dup = next(n for n in names if names.count(n) > 1)
        raise AnalysisError(f"{dup} is listed twice after 'depends on'", index=1 + names.index(dup))
    if target in names:
        raise AnalysisError(f"{target} can't depend on itself", index=1 + names.index(target))
    if not names:
        raise AnalysisError(f"{target} has to depend on something: write  {target} depends on a, b, c")
    everything = [target] + names

    # repeating quantities: the inputs, in the order written, that add a new direction
    rep = []
    for n in names:
        if rank([_vec(dims[r]) for r in rep + [n]]) > len(rep):
            rep.append(n)
    r = len(rep)
    rep_cols = [_vec(dims[x]) for x in rep]

    def expo(q):
        return solve_exact(rep_cols, _vec(dims[q]))

    a_target = expo(target)
    ngroups = len(everything) - rank([_vec(dims[x]) for x in everything])
    if a_target is None:
        lonely = _only_here(target, dims, everything)
        ins = ", ".join(names)
        if lonely:
            msg = f"{target} can't be made from {ins}: {target} has {' and '.join(lonely)}, but nothing it " \
                  f"depends on has {' or '.join(lonely)}"
        else:
            msg = f"{target} ({_dim_text(target_dim)}) can't be made from any powers of {ins}"
        if ngroups == 0:
            msg += f"; so there is no dimensionless group at all ({len(everything)} quantities, " \
                   f"{len(everything)} independent dimensions)"
        raise AnalysisError(msg, hint=f"{target} must depend on something else too (a constant like G, c or ħ?)",
                            index=0)

    groups = []
    target_group = {target: Fraction(1)}
    for x, a in zip(rep, a_target):
        if a != 0:
            target_group[x] = -a
    groups.append(target_group)
    for q in names:
        if q in rep:
            continue
        a = expo(q)
        g = {q: Fraction(1)}
        for x, e in zip(rep, a):
            if e != 0:
                g[x] = -e
        groups.append(_nicest(g))
    assert len(groups) == len(everything) - r

    prefactor = {x: -e for x, e in target_group.items() if x != target}
    dropped = []
    for x in names:
        if all(g.get(x, 0) == 0 for g in groups):
            lonely = _only_here(x, dims, everything)
            if lonely:
                why = f"nothing else has {' or '.join(lonely)}"
            else:
                why = "its units can't be cancelled by the others"
            dropped.append((x, why))
    return Analysis(target, names, dims, r, rep, groups, prefactor, dropped)


# ---------------------------------------------------------------- formatting
def _dim_text(d: Dim):
    from .units import dim_name
    return dim_name(d)


def _power(name, p: Fraction):
    """name^p for an integer p ≥ 1, with superscripts."""
    if p == 1:
        return name
    return name + str(p.numerator).translate(_SUP)


def _ratio(exps, order):
    """A product of integer powers as num/den text; `order` fixes the order of names."""
    num = [_power(n, exps[n]) for n in order if exps.get(n, 0) > 0]
    den = [_power(n, -exps[n]) for n in order if exps.get(n, 0) < 0]
    top = " ".join(num) if num else "1"
    if not den:
        return top, len(num) > 1
    bottom = den[0] if len(den) == 1 else "(" + " ".join(den) + ")"
    return f"{top}/{bottom}", True


def product_text(exps, order=None):
    """Π name^exp written the Fermium way: `T √(g/L)`, `ρ v² A`, `R (ρ/(E t²))^(1/5)`.

    The text is valid Fermium, so it can be pasted into a program."""
    order = order or list(exps)
    exps = {k: Fraction(v) for k, v in exps.items() if v != 0}
    if not exps:
        return "1"
    ints = {k: v for k, v in exps.items() if v.denominator == 1}
    fracs = {k: v for k, v in exps.items() if v.denominator != 1}

    def root_of(fr):
        d = lcm(*(v.denominator for v in fr.values()))
        body, compound = _ratio({k: v * d for k, v in fr.items()}, order)
        wrap = f"({body})" if compound or body[-1] in "⁰¹²³⁴⁵⁶⁷⁸⁹" else body
        return {2: "√" + wrap, 3: "∛" + wrap}.get(d, f"({body})^(1/{d})")

    if not fracs:
        return _ratio(ints, order)[0]
    if all(v > 0 for v in fracs.values()) or all(v < 0 for v in fracs.values()):
        # the roots on one side: ρ v √A/μ, v/√(g λ), F/(μ v √A)
        def side(e):
            items = [_power(n, e[n]) for n in order if n in e and e[n].denominator == 1]
            fr = {k: v for k, v in e.items() if v.denominator != 1}
            return items + ([root_of(fr)] if fr else [])
        num = side({k: v for k, v in exps.items() if v > 0}) or ["1"]
        den = side({k: -v for k, v in exps.items() if v < 0})
        text = " ".join(num)
        if den:
            text += "/" + (f"({' '.join(den)})" if len(den) > 1 else den[0])
        return text
    # roots of both signs: one root of a ratio, integer powers around it (T √(g/L), R (ρ/(E t²))^(1/5))
    root = root_of(fracs)
    front = " ".join(_power(n, ints[n]) for n in order if n in ints and ints[n] > 0)
    text = f"{front} {root}" if front else root
    den = [_power(n, -ints[n]) for n in order if n in ints and ints[n] < 0]
    if den:
        text += "/" + (f"({' '.join(den)})" if len(den) > 1 else den[0])
    return text


def pi_name(i):
    return "Π" + str(i).translate(_SUBSCRIPT)


def report(an: Analysis, display=None, title=None):
    """The lines `analyze` prints."""
    disp = display or {}

    def D(e):
        return {disp.get(k, k): v for k, v in e.items()}
    order = [disp.get(k, k) for k in [an.target] + an.names]
    t = disp.get(an.target, an.target)
    ngr = len(an.groups)
    head = f"dimensional analysis{' of ' + title if title else ''}: {t} depends on " + \
        ", ".join(disp.get(n, n) for n in an.names)
    used = [BASE_NAMES[i] for i in range(7) if any(an.dims[x].e[i] != 0 for x in [an.target] + an.names)]
    lines = [head,
             f"  {an.n} quantities, {an.rank} independent dimension{'s' if an.rank != 1 else ''} "
             f"({'' if len(used) == an.rank else 'among '}{', '.join(used)}) → {an.n} − {an.rank} = {ngr} "
             f"dimensionless group{'s' if ngr != 1 else ''}"]
    for i, g in enumerate(an.groups, 1):
        lines.append(f"  {pi_name(i)} = {product_text(D(g), order)}")
    pre = product_text(D(an.prefactor), order)
    if not an.prefactor:        # a dimensionless target
        if ngr == 1:
            lines.append(f"  so {t} is a pure number: it can't depend on any of them")
        else:
            rest = ", ".join(pi_name(i) for i in range(2, ngr + 1))
            lines.append(f"  so {t} = f({rest})   (f is a function dimensional analysis can't give)")
    elif ngr == 1:
        lines.append(f"  so {t} ∝ {pre}   ({t} = C {pre}, with C a pure number)")
    else:
        rest = ", ".join(pi_name(i) for i in range(2, ngr + 1))
        lines.append(f"  so {t} = {pre} · f({rest})   (f is a function dimensional analysis can't give)")
    for x, why in an.dropped:
        lines.append(f"  {disp.get(x, x)} drops out: {why}")
    return lines
