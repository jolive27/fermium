"""Uncertain numbers for `5.0 ± 0.2 m` (DECISIONS D120-D124).

An uncertain value is a nominal value plus a sparse vector of *contributions*: {source id: ∂f/∂xᵢ · σᵢ},
one entry per independent error source xᵢ that it depends on.  σ_f² = Σ contribution².  This is first-order
(linear) propagation with exact correlations, the same model as Python's `uncertainties` package:

    x - x      = 0 ± 0           (both terms share one source)
    x * x      has σ = 2|x|σx    (x * y with an independent y of the same size has √2 |x| σ)

Each `±` literal (each element, for a list) and each set of fitted parameters creates new sources.  Values are
in SI units here (the checker erased the units); printing converts both parts to the display unit.

The reference interpreter (fermium/interp.py) runs programs that use uncertainties: Python's operators do
the propagation, so user functions, derivatives and sums need no special code.  Operations that need a
plain number (vectors, integrals, ODEs, ...) call float(), which raises UncertainUse with a clear message.
"""
from __future__ import annotations

import math
from itertools import count

_ids = count(1)


class UncertainUse(Exception):
    """An uncertain value reached an operation that only takes plain numbers."""

    def __init__(self, message):
        super().__init__(message)
        self.message = message


GENERIC = ("this operation needs a plain number, but got an uncertain value (±); write value(x) to drop the "
           "uncertainty, or put the calculation in a  propagate montecarlo  block")


def new_source():
    return next(_ids)


class UFloat:
    __slots__ = ("v", "d")

    def __init__(self, v, d=None):
        self.v = float(v)
        self.d = d if d is not None else {}

    # ------------------------------------------------------------ parts
    @property
    def s(self):
        """The standard deviation."""
        return math.sqrt(math.fsum(c * c for c in self.d.values()))

    @staticmethod
    def measured(v, sigma):
        """A new independent measurement v ± sigma."""
        sigma = abs(float(sigma))
        return UFloat(v, {new_source(): sigma} if sigma != 0 else {})

    def __repr__(self):
        return f"UFloat({self.v!r} ± {self.s!r})"

    def __float__(self):
        raise UncertainUse(GENERIC)

    def __index__(self):
        raise UncertainUse(GENERIC)

    def __int__(self):
        raise UncertainUse(GENERIC)

    def __round__(self, n=None):
        # reached when a vector or matrix holds uncertain values (printing rounds its components): not yet (C7)
        raise UncertainUse("vectors and matrices of uncertain values (±) aren't supported yet; work with the "
                           "uncertain numbers one at a time, or use value(x) to drop the uncertainty")

    def __hash__(self):
        return id(self)

    # ------------------------------------------------------------ arithmetic
    def _lin(self, a, other, b):
        """a·self + b·other (other: UFloat) as contributions."""
        d = {k: a * c for k, c in self.d.items()} if a != 1 else dict(self.d)
        for k, c in other.d.items():
            d[k] = _sum2(d.get(k, 0.0), b * c)
        return {k: c for k, c in d.items() if c != 0 or c != c}

    def __add__(self, o):
        if isinstance(o, UFloat):
            return UFloat(self.v + o.v, self._lin(1.0, o, 1.0))
        return UFloat(self.v + o, dict(self.d)) if _plain(o) else NotImplemented

    __radd__ = __add__

    def __sub__(self, o):
        if isinstance(o, UFloat):
            return UFloat(self.v - o.v, self._lin(1.0, o, -1.0))
        return UFloat(self.v - o, dict(self.d)) if _plain(o) else NotImplemented

    def __rsub__(self, o):
        return UFloat(o - self.v, {k: -c for k, c in self.d.items()}) if _plain(o) else NotImplemented

    def __neg__(self):
        return UFloat(-self.v, {k: -c for k, c in self.d.items()})

    def __pos__(self):
        return self

    def __mul__(self, o):
        if isinstance(o, UFloat):
            return UFloat(self.v * o.v, _scale_merge(self.d, o.v, o.d, self.v))
        if _plain(o):
            return UFloat(self.v * o, _scale(self.d, o))
        return NotImplemented

    __rmul__ = __mul__

    def __truediv__(self, o):
        if isinstance(o, UFloat):
            q = _div(self.v, o.v)
            return UFloat(q, _scale_merge(self.d, _div(1.0, o.v), o.d, -_div(q, o.v)))
        if _plain(o):
            return UFloat(_div(self.v, o), _scale(self.d, _div(1.0, o)))
        return NotImplemented

    def __rtruediv__(self, o):
        if not _plain(o):
            return NotImplemented
        q = _div(o, self.v)
        return UFloat(q, _scale(self.d, -_div(q, self.v)))

    def __pow__(self, o):
        if isinstance(o, UFloat):
            r = _pow(self.v, o.v)
            da = o.v * _pow(self.v, o.v - 1) if self.v != 0 else (0.0 if o.v > 1 else math.nan)
            db = r * math.log(self.v) if self.v > 0 else (0.0 if self.v == 0 else math.nan)
            return UFloat(r, _scale_merge(self.d, da, o.d, db))
        if _plain(o):
            return self.powc(o)
        return NotImplemented

    def __rpow__(self, o):
        if not _plain(o):
            return NotImplemented
        r = _pow(o, self.v)
        db = r * math.log(o) if o > 0 else (0.0 if o == 0 and self.v > 0 else math.nan)
        return UFloat(r, _scale(self.d, db))

    def powc(self, p, pv=None):
        """self ** p for a plain p; pv: the plain power function to use for the value (the interpreter's)."""
        f = pv or _pow
        r = f(self.v, p)
        if p == 0:
            return UFloat(r, {})
        dv = p * f(self.v, p - 1) if not (self.v == 0 and p >= 1) else (1.0 if p == 1 else 0.0)
        return UFloat(r, _scale(self.d, dv))

    def __abs__(self):
        return UFloat(abs(self.v), _scale(self.d, -1.0 if self.v < 0 else 1.0))

    # ------------------------------------------------------------ comparisons: by nominal value
    def __lt__(self, o):
        return self.v < nominal(o)

    def __le__(self, o):
        return self.v <= nominal(o)

    def __gt__(self, o):
        return self.v > nominal(o)

    def __ge__(self, o):
        return self.v >= nominal(o)

    def __eq__(self, o):
        return self.v == nominal(o)

    def __ne__(self, o):
        return self.v != nominal(o)


def _plain(o):
    return isinstance(o, (int, float)) and not isinstance(o, bool) or type(o).__name__ == "float64"


def _div(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        if a == 0 or a != a:
            return math.nan
        return math.copysign(math.inf, a) * math.copysign(1.0, b)


def _pow(a, b):
    try:
        r = a ** b
    except (OverflowError, ZeroDivisionError):
        return math.inf
    return math.nan if isinstance(r, complex) else float(r)


def _scale(d, a):
    if a == 1:
        return dict(d)
    return {k: c * a for k, c in d.items() if c * a != 0 or a != a}


NOISE = 1e-13    # a sum of two contributions this much smaller than its terms is rounding noise (D208)


def _sum2(x, y):
    """x + y for two contributions of one source; a cancellation down to rounding noise is exactly 0.
    (L/g)^(1/2)/L^(1/2) or (y/3)·3 − y leave ~10⁻¹⁸ of the input's σ, which is 0 to double precision and
    would otherwise set the printed digits: 2.0060666807106475318 ± 0.0000000000000000035 (red team round 4
    #11, D208)."""
    z = x + y
    if z != 0 and abs(z) <= NOISE * (abs(x) + abs(y)):
        return 0.0
    return z


def _scale_merge(d1, a, d2, b):
    out = {k: c * a for k, c in d1.items()}
    for k, c in d2.items():
        out[k] = _sum2(out.get(k, 0.0), c * b)
    return {k: c for k, c in out.items() if c != 0 or c != c}


def nominal(x):
    return x.v if isinstance(x, UFloat) else x


def sigma(x):
    return x.s if isinstance(x, UFloat) else 0.0


def correlation(a, b):
    """The correlation coefficient of two values (for tests and messages)."""
    if not (isinstance(a, UFloat) and isinstance(b, UFloat)):
        return 0.0
    cov = math.fsum(c * b.d.get(k, 0.0) for k, c in a.d.items())
    sa, sb = a.s, b.s
    return cov / (sa * sb) if sa and sb else 0.0


# ---------------------------------------------------------------- functions of one variable
def _dgamma(x):
    try:
        from scipy.special import digamma
        return float(digamma(x))
    except ImportError:          # pragma: no cover
        h = 1e-6 * max(1.0, abs(x))
        return (math.lgamma(x + h) - math.lgamma(x - h)) / (2 * h)


DERIV = {
    "sin": math.cos, "cos": lambda x: -math.sin(x), "tan": lambda x: 1 + math.tan(x) ** 2,
    "asin": lambda x: _div(1.0, math.sqrt(1 - x * x)) if abs(x) <= 1 else math.nan,
    "acos": lambda x: -_div(1.0, math.sqrt(1 - x * x)) if abs(x) <= 1 else math.nan,
    "atan": lambda x: 1 / (1 + x * x), "sinh": math.cosh, "cosh": math.sinh,
    "tanh": lambda x: 1 - math.tanh(x) ** 2, "asinh": lambda x: 1 / math.sqrt(x * x + 1),
    "acosh": lambda x: _div(1.0, math.sqrt(x * x - 1)) if x >= 1 else math.nan,
    "atanh": lambda x: _div(1.0, 1 - x * x), "exp": math.exp, "ln": lambda x: _div(1.0, x),
    "log": lambda x: _div(1.0, x), "log10": lambda x: _div(1.0, x * math.log(10)),
    "log2": lambda x: _div(1.0, x * math.log(2)),
    "erf": lambda x: 2 / math.sqrt(math.pi) * math.exp(-x * x),
    "erfc": lambda x: -2 / math.sqrt(math.pi) * math.exp(-x * x),
    "gamma": lambda x: math.gamma(x) * _dgamma(x), "lgamma": _dgamma, "expm1": math.exp,
    "log1p": lambda x: _div(1.0, 1 + x), "abs": lambda x: -1.0 if x < 0 else 1.0,
    # D113's reciprocal functions (red team round 3 #7): d cot = -csc², d sec = sec tan, d csc = -csc cot
    "cot": lambda x: -_div(1.0, math.sin(x) ** 2),
    "sec": lambda x: _div(math.sin(x), math.cos(x) ** 2),
    "csc": lambda x: -_div(math.cos(x), math.sin(x) ** 2),
}
STEP = {"floor", "ceil", "round", "sign"}      # piecewise constant: the result is a plain number


def apply1(name, x, plain):
    """name(x) for an uncertain x; plain(name, v) is the interpreter's function on floats."""
    r = plain(name, x.v)
    if name in STEP:
        return r
    try:
        dv = DERIV[name](x.v)
    except (ValueError, OverflowError, ZeroDivisionError):
        dv = math.nan
    return UFloat(r, _scale(x.d, dv))


def lift(f, args, partials=None):
    """f(*args) where some arguments are uncertain.  partials(*values) -> the partial derivatives, else they
    are found by central differences (for Bessel functions and the like)."""
    vals = [nominal(a) for a in args]
    r = f(*vals)
    if partials is not None:
        ps = partials(*vals)
    else:
        ps = []
        for i, a in enumerate(args):
            if not isinstance(a, UFloat):
                ps.append(0.0)
                continue
            h = 1e-6 * max(abs(vals[i]), 1e-300) if vals[i] != 0 else 1e-8
            up, dn = list(vals), list(vals)
            up[i] += h
            dn[i] -= h
            ps.append((f(*up) - f(*dn)) / (2 * h))
    d = {}
    for a, p in zip(args, ps):
        if isinstance(a, UFloat):
            for k, c in a.d.items():
                d[k] = d.get(k, 0.0) + p * c
    return UFloat(r, {k: c for k, c in d.items() if c != 0 or c != c})


def any_uncertain(xs):
    for x in xs:
        if isinstance(x, UFloat):
            return True
        if isinstance(x, (list, tuple)) and any(isinstance(y, UFloat) for y in x):
            return True
    return False


# ---------------------------------------------------------------- correlated sets (fit parameters)
def correlated(values, cov):
    """Uncertain values with the given covariance matrix (cov = V Λ Vᵀ; each eigenvector is one new source)."""
    import numpy as np
    cov = np.asarray(cov, dtype=float)
    n = len(values)
    if cov.shape != (n, n) or not np.all(np.isfinite(cov)):
        return None
    cov = 0.5 * (cov + cov.T)
    lam, vec = np.linalg.eigh(cov)
    ids = [new_source() for _ in range(n)]
    out = []
    for i in range(n):
        d = {}
        for k in range(n):
            c = float(vec[i, k] * math.sqrt(max(lam[k], 0.0)))
            if c != 0:
                d[ids[k]] = c
        out.append(UFloat(values[i], d))
    return out


# ---------------------------------------------------------------- printing (lab-report convention)
SUP = str.maketrans("0123456789-", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻")
SIG = 2          # significant figures of the uncertainty


def _round_sig(s, sig=SIG):
    """(σ rounded to `sig` significant figures, its decimal exponent): the last digit kept is 10^(e-sig+1)."""
    e = math.floor(math.log10(s))
    r = round(s, sig - 1 - e)
    if r >= 10 ** (e + 1):          # 0.0996 -> 0.10: one decade up
        e += 1
        r = round(s, sig - 1 - e)
    return r, e


def format_pm(x, s, sig=SIG):
    """'5.00 ± 0.20', '(1.234 ± 0.056)×10⁻³' (parenthesised when scientific) for a value x ± s.
    The uncertainty keeps `sig` significant figures and the value is rounded to the same decimal place.
    Returns (text, scientific)."""
    from .units import format_number
    if not (math.isfinite(x) and math.isfinite(s)):
        return f"{format_number(x)} ± {format_number(s)}", False
    if s == 0:
        # no uncertainty left (x − x, or a cancellation to rounding noise): the value by the default rule
        # of plain numbers (D11), 2.01 ± 0, and 0 ± 0 for a value that is 0 up to rounding (D208)
        from .units import format_default
        return f"{format_default(x)} ± 0", False
    r, e = _round_sig(s, sig)
    last = e - sig + 1                       # the decimal place of the last digit shown
    xr = round(x, -last)
    E = math.floor(math.log10(max(abs(xr), r)))
    if -3 < E < 5 and last <= 0:
        nd = -last
        return f"{xr:.{nd}f} ± {r:.{nd}f}", False
    nd = max(E - last, 0)                    # scientific, with one power of ten for both parts
    return f"({xr / 10 ** E:.{nd}f} ± {r / 10 ** E:.{nd}f})×10{str(E).translate(SUP)}", True


def format_uncertain(u, dim, hint, display_unit):
    """The printed form of an uncertain quantity, in its display unit: '9.81 ± 0.12 m/s²'."""
    un = display_unit(dim, hint)
    x = (u.v - un.offset) / un.factor
    s = u.s / abs(un.factor)
    text, sci = format_pm(x, s)
    name = un.name
    if name in ("", "1"):
        return text
    if name in ("°", "%", "′", "″"):
        return f"{text}{name}" if sci else f"({text}){name}"
    return f"{text} {name}"


def format_uncertain_list(vals, un):
    """[1.00 ± 0.10, 2.00 ± 0.20] m: each entry by the ± rule (plain entries with 6 digits) in one unit."""
    from .units import format_number
    parts = []
    for v in vals:
        if isinstance(v, UFloat):
            parts.append(format_pm((v.v - un.offset) / un.factor, v.s / abs(un.factor))[0])
        else:
            parts.append(format_number((v - un.offset) / un.factor, 6))
    s = "[" + ", ".join(parts) + "]"
    return s if un.name in ("", "1") else f"{s} {un.name}"
