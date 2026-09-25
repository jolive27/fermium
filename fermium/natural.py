"""Unit systems: SI (the default), natural units (`units natural(ħ = c = 1)`, `units nuclear`) and
display presets (`units astro`).  See DECISIONS.md D60.

How natural units work
----------------------
Setting a set S of constants to 1 (ħ, c, k_B, G, ε₀) identifies dimensions that differ by powers of
those constants.  Every SI dimension D splits uniquely as

    D = Σ aᵢ dim(Cᵢ)  +  Σ βⱼ Bⱼ          (exponent vectors, exact Fractions)

where the Bⱼ are a fixed set of *kept* base dimensions (for ħ = c = 1: energy, current, temperature,
amount, luminous intensity) chosen so that {dim(Cᵢ)} ∪ {Bⱼ} is a basis of the 7-dimensional space.
The *canonical dimension* of D is Σ βⱼ Bⱼ, and a quantity q whose SI value is v (in SI units of D) is
stored as its *canonical value*

    φ(q) = v · Π Cᵢ^(−aᵢ)          (the SI value of q / Π Cᵢ^aᵢ, a quantity of the canonical dimension)

φ is a homomorphism (a is linear in D), so ordinary unit checking on canonical dimensions is exactly
unit checking "modulo ħ and c": a mass and an energy share the canonical dimension J, while a length
(J⁻¹) and an energy (J) still differ.  Converting back to any SI unit U with dimension D_U is the
inverse map, which is unique because the split is: SI value = φ · Π Cᵢ^(aᵢ(D_U)).  The checker applies
φ to every unit and constant that appears inside a natural region, so a unit written there is just a
Unit whose dimension is canonical and whose factor already includes Π Cᵢ^(−aᵢ); the code generator,
the interpreter and `fermium build` need no changes.
"""
from __future__ import annotations

import math
from fractions import Fraction

from .units import Dim, DIMLESS, L, M, T, I, TH, N, J, Unit, dim_name, lookup_unit, _fmt_exp, join_units

_HBAR = 6.62607015e-34 / (2 * math.pi)
_C = 299792458.0
_KB = 1.380649e-23
_G = 6.67430e-11
_EPS0 = 8.8541878188e-12
ENERGY = M * L**2 / T**2

# name -> (SI value, dimension); the constants a natural system may set to 1
SETTABLE = {
    "ħ": (_HBAR, ENERGY * T),
    "c": (_C, L / T),
    "k_B": (_KB, ENERGY / TH),
    "G": (_G, L**3 / (M * T**2)),
    "ε_0": (_EPS0, (I * T)**2 / (ENERGY * L)),
}
ALIASES = {"hbar": "ħ", "kB": "k_B", "eps0": "ε_0", "ε0": "ε_0", "epsilon_0": "ε_0", "c_0": "c"}
_ORDER = ("ħ", "c", "k_B", "G", "ε_0")

def _solve(cols, target):
    """Solve Σ xₖ colsₖ = target exactly (cols: 7 Dims forming a basis)."""
    n = 7
    a = [[Fraction(cols[k].e[r]) for k in range(n)] + [Fraction(target.e[r])] for r in range(n)]
    for c in range(n):
        p = next(r for r in range(c, n) if a[r][c] != 0)
        a[c], a[p] = a[p], a[c]
        piv = a[c][c]
        a[c] = [x / piv for x in a[c]]
        for r in range(n):
            if r != c and a[r][c] != 0:
                f = a[r][c]
                a[r] = [x - f * y for x, y in zip(a[r], a[c])]
    return [a[r][n] for r in range(n)]


def _rank(dims):
    rows = [[Fraction(x) for x in d.e] for d in dims]
    rank, col = 0, 0
    while rank < len(rows) and col < 7:
        p = next((r for r in range(rank, len(rows)) if rows[r][col] != 0), None)
        if p is None:
            col += 1
            continue
        rows[rank], rows[p] = rows[p], rows[rank]
        for r in range(len(rows)):
            if r != rank and rows[r][col] != 0:
                f = rows[r][col] / rows[rank][col]
                rows[r] = [x - f * y for x, y in zip(rows[r], rows[rank])]
        rank += 1
        col += 1
    return rank


class UnitSystem:
    """SI, a natural system (some constants set to 1), or a display preset (astro)."""

    def __init__(self, name, consts=(), display=None):
        self.name = name
        self.consts = tuple(c for c in _ORDER if c in consts)
        self.display = display or name
        self.key = frozenset(self.consts)
        self.natural = bool(self.consts)
        if not self.natural:
            return
        vecs = [SETTABLE[c][1] for c in self.consts]
        if _rank(vecs) < len(vecs):
            raise ValueError("these constants can't all be 1 at once (they are not independent)")
        if "ħ" in self.consts:
            cand = [ENERGY, I, TH, N, J, L, M, T]
        else:
            cand = [L, I, TH, N, J, M, T, ENERGY]
        kept = []
        for d in cand:
            if len(vecs) + len(kept) == 7:
                break
            if _rank(vecs + kept + [d]) == len(vecs) + len(kept) + 1:
                kept.append(d)
        self.kept = kept
        self.cols = vecs + kept
        self._cache = {}

    def __repr__(self):
        return f"UnitSystem({self.name}, {self.consts})"

    def label(self):
        if not self.natural:
            return f"units {self.name}"
        return f"units {self.name} ({' = '.join(self.consts)} = 1)"

    # ------------------------------------------------------------ the split D = Σ aᵢ Cᵢ + Σ βⱼ Bⱼ
    def split(self, d: Dim):
        if d not in self._cache:
            x = _solve(self.cols, d)
            k = len(self.consts)
            self._cache[d] = (x[:k], x[k:])
        return self._cache[d]

    def canon_dim(self, d: Dim) -> Dim:
        if not self.natural:
            return d
        _, beta = self.split(d)
        out = DIMLESS
        for b, bd in zip(beta, self.kept):
            if b:
                out = out * bd ** b
        return out

    def factor(self, d: Dim) -> float:
        """Multiply an SI value of dimension d by this to get its canonical value."""
        if not self.natural:
            return 1.0
        a, _ = self.split(d)
        f = 1.0
        for ai, c in zip(a, self.consts):
            if ai:
                v = SETTABLE[c][0]
                f *= v ** (-int(ai)) if ai.denominator == 1 else v ** float(-ai)
        return f

    def invariant(self, d: Dim) -> bool:
        return not self.natural or all(x == 0 for x in self.split(d)[0])

    def canon_unit(self, u: Unit) -> Unit:
        if not self.natural or self.invariant(u.dim):
            return u
        f = self.factor(u.dim)
        return Unit(u.name, self.canon_dim(u.dim), u.factor * f, u.offset * f)

    def const_value(self, name, value, dim):
        if name in self.consts or ALIASES.get(name) in self.consts:
            return 1.0          # exactly 1, not ħ·(1/ħ) with a rounding error
        return value * self.factor(dim)

    # ------------------------------------------------------------ display
    def _base_display(self, bd, sign):
        """Display unit (Unit in canonical form) for one kept base dimension."""
        if bd == ENERGY:
            if self.display == "nuclear" and sign < 0:
                return self.canon_unit(lookup_unit("fm")), -1          # 1/MeV is shown as fm (ħc = 1)
            return lookup_unit("MeV"), 1
        if bd == L:
            return lookup_unit("m"), 1
        for d, s in ((M, "kg"), (T, "s"), (I, "A"), (TH, "K"), (N, "mol"), (J, "cd")):
            if bd == d:
                return lookup_unit(s), 1
        return None, 1

    def display_unit(self, d: Dim):
        """The unit a value of canonical dimension d is printed in when it carries no unit of its own."""
        if self.display == "astro":
            return astro_display(d)
        if not self.natural:
            return None
        _, beta = self.split(d)
        num, den, factor = [], [], 1.0
        for b, bd in zip(beta, self.kept):
            if not b:
                continue
            u, s = self._base_display(bd, b)
            p = b * s
            factor *= u.factor ** float(p)
            (num if p > 0 else den).append(u.name + _fmt_exp(abs(p)))
        if num or len(den) != 1:
            name = join_units(num, den)
        else:                                   # MeV⁻¹ rather than 1/MeV (as physicists write it)
            name = den[0][:-1] + "⁻" + den[0][-1] if den[0][-1] in "²³⁴⁵⁶⁷⁸⁹" else den[0] + "⁻¹"
        return Unit(name or "1", d, factor)

    def describe(self, d: Dim) -> str:
        """A dimension in words, for error messages inside a natural region."""
        if not self.natural:
            return dim_name(d)
        if d.dimensionless:
            return "a plain number (no units)"
        uname = self.display_unit(d).name
        if "ħ" in self.consts:
            _, beta = self.split(d)
            n = beta[0]
            if all(b == 0 for b in beta[1:]):
                words = {1: "energy or mass", -1: "length or time (1/energy)", 2: "energy²",
                         -2: "area (1/energy²)", -3: "volume (1/energy³)"}
                if n in words:
                    return f"{words[n]} [{uname}]"
                return f"energy^{n} [{uname}]"
        return f"a quantity with units [{uname}]"


SI = UnitSystem("SI")


def make_system(name, consts=None):
    """`units natural(ħ = c = 1)`, `units nuclear`, `units astro`, `units SI`."""
    if name == "SI":
        return SI
    if name == "astro":
        if consts:
            raise ValueError("units astro sets no constants to 1 (it only chooses M☉, AU and yr for printing); "
                             "for G = c = 1 write  units natural(G = c = 1)")
        return UnitSystem("astro", (), display="astro")
    if name == "nuclear":
        if consts and set(consts) != {"ħ", "c"}:
            raise ValueError("units nuclear always means ħ = c = 1 (with MeV and fm); for other constants "
                             "write  units natural(...)")
        return UnitSystem("nuclear", ("ħ", "c"), display="nuclear")
    if name == "natural":
        cs = consts or ["ħ", "c"]
        for c in cs:
            if c not in SETTABLE:
                raise ValueError(f"{c} can't be set to 1 in natural units (Fermium knows ħ, c, k_B, G and ε_0)")
        if len(set(cs)) != len(cs):
            raise ValueError("a constant is listed twice")
        return UnitSystem("natural", cs, display="natural")
    raise ValueError(f"unknown unit system '{name}' (Fermium knows natural, nuclear, astro and SI)")


def canonical_const_name(name):
    return ALIASES.get(name, name)


# ------------------------------------------------------------ astro display preset
_ASTRO_NAMED = None


def astro_display(d: Dim):
    """Astronomy display units: M☉, AU, yr (and L☉, km/s); SI's choice for anything else."""
    global _ASTRO_NAMED
    if _ASTRO_NAMED is None:
        from .units import parse_unit_string
        _ASTRO_NAMED = {}
        for spec in ("M☉", "AU", "yr", "L☉", "km/s", "M☉/yr", "AU³/yr²", "AU³/(M☉ yr²)"):
            u = parse_unit_string(spec)
            _ASTRO_NAMED[u.dim] = Unit(spec, u.dim, u.factor)
    if d in _ASTRO_NAMED:
        return _ASTRO_NAMED[d]
    if all(x == 0 for x in d.e[3:]) and 0 < sum(1 for x in d.e[:3] if x) <= 2:
        parts = [(d.e[1], "M☉", 1.98841e30), (d.e[0], "AU", 149597870700.0), (d.e[2], "yr", 365.25 * 86400)]
        num = [n + _fmt_exp(p) for p, n, _ in parts if p > 0]
        den = [n + _fmt_exp(-p) for p, n, _ in parts if p < 0]
        f = 1.0
        for p, _, v in parts:
            if p:
                f *= v ** float(p)
        return Unit(join_units(num, den), d, f)
    return None
