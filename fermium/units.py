"""Dimensions and units.

A *dimension* is a vector of rational exponents over the 7 SI base quantities
(length, mass, time, current, temperature, amount, luminous intensity).

A *unit* is a dimension plus a scale factor (and, for °C/°F only, an offset)
that converts a value in that unit to SI base units.  All runtime values are
stored in SI base units, so after checking, units disappear from the program.

Angles: radians are dimensionless (the SI convention).  `rad` is the unit 1,
`°` / `deg` is π/180.  See DECISIONS.md.
"""
from __future__ import annotations

import math
from fractions import Fraction

BASE_SYMBOLS = ("m", "kg", "s", "A", "K", "mol", "cd")
BASE_NAMES = ("length", "mass", "time", "current", "temperature", "amount", "luminosity")

SUPERSCRIPTS = str.maketrans("0123456789-/", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻ᐟ")


class Dim:
    """An immutable vector of 7 rational exponents."""

    __slots__ = ("e",)

    def __init__(self, exps=(0, 0, 0, 0, 0, 0, 0)):
        self.e = tuple(Fraction(x) for x in exps)

    @staticmethod
    def base(i):
        v = [0] * 7
        v[i] = 1
        return Dim(v)

    def __mul__(self, o):
        return Dim(a + b for a, b in zip(self.e, o.e))

    def __truediv__(self, o):
        return Dim(a - b for a, b in zip(self.e, o.e))

    def __pow__(self, p):
        p = Fraction(p)
        return Dim(a * p for a in self.e)

    def __eq__(self, o):
        return isinstance(o, Dim) and self.e == o.e

    def __hash__(self):
        return hash(self.e)

    @property
    def dimensionless(self):
        return all(x == 0 for x in self.e)

    def __repr__(self):
        return f"Dim({format_dim(self) or '1'})"


DIMLESS = Dim()
L, M, T, I, TH, N, J = (Dim.base(i) for i in range(7))


def _fmt_exp(p: Fraction, pretty=True) -> str:
    if p == 1:
        return ""
    if not pretty:
        if p.denominator == 1:
            return f"^{p.numerator}"
        return f"^({p.numerator}/{p.denominator})"
    if p.denominator == 1:
        return str(p.numerator).translate(SUPERSCRIPTS)
    return f"^({p.numerator}/{p.denominator})"


def format_dim(d: Dim, pretty=True) -> str:
    """Format a dimension as SI base units, e.g. 'kg m/s²'."""
    order = (1, 0, 2, 3, 4, 5, 6)  # kg first, then m, s, ...
    num, den = [], []
    for i in order:
        p = d.e[i]
        if p > 0:
            num.append(BASE_SYMBOLS[i] + _fmt_exp(p, pretty))
        elif p < 0:
            den.append(BASE_SYMBOLS[i] + _fmt_exp(-p, pretty))
    return join_units(num, den)


def join_units(num, den) -> str:
    if not num and not den:
        return ""
    s = " ".join(num) if num else "1"
    if den:
        if len(den) == 1:
            s += "/" + den[0]
        else:
            s += "/(" + " ".join(den) + ")"
    return s


class Unit:
    """A unit: value_in_SI = value * factor + offset."""

    __slots__ = ("name", "dim", "factor", "offset")

    def __init__(self, name, dim, factor, offset=0.0):
        self.name = name
        self.dim = dim
        self.factor = float(factor)
        self.offset = float(offset)

    def __mul__(self, o):
        return Unit(f"{self.name} {o.name}", self.dim * o.dim, self.factor * o.factor)

    def __truediv__(self, o):
        return Unit(f"{self.name}/{o.name}", self.dim / o.dim, self.factor / o.factor)

    def __pow__(self, p):
        p = Fraction(p)
        return Unit(f"{self.name}{_fmt_exp(p)}", self.dim ** p, self.factor ** float(p))

    @property
    def affine(self):
        return self.offset != 0.0

    def __repr__(self):
        return f"Unit({self.name!r}, {format_dim(self.dim)}, {self.factor})"


# ---------------------------------------------------------------------------
# Unit database.  name -> (factor, dim, prefixable)
# Exact SI definitions where they exist.  Sources: SI Brochure (9th ed.),
# NIST SP 811, IAU 2012/2015 resolutions (au, nominal solar values).
# ---------------------------------------------------------------------------
_PI = math.pi
_c = 299792458.0
_e = 1.602176634e-19
_u = 1.66053906892e-27  # CODATA 2022 atomic mass constant
_au = 149597870700.0
_ly = _c * 365.25 * 86400
_pc = _au * 648000 / _PI

N_ = M * L / T**2
J_ = N_ * L
W_ = J_ / T
C_ = I * T
V_ = W_ / I
Pa_ = N_ / L**2

_UNITS = {
    # SI base
    "m": (1.0, L, True), "g": (1e-3, M, True), "s": (1.0, T, True), "A": (1.0, I, True),
    "K": (1.0, TH, True), "mol": (1.0, N, True), "cd": (1.0, J, True),
    # dimensionless / angles
    "rad": (1.0, DIMLESS, False), "sr": (1.0, DIMLESS, False),
    "°": (_PI / 180, DIMLESS, False), "deg": (_PI / 180, DIMLESS, False),
    "arcmin": (_PI / 10800, DIMLESS, False), "arcsec": (_PI / 648000, DIMLESS, False),
    "rev": (2 * _PI, DIMLESS, False), "percent": (0.01, DIMLESS, False), "%": (0.01, DIMLESS, False),
    # SI derived
    "Hz": (1.0, DIMLESS / T, True), "N": (1.0, N_, True), "Pa": (1.0, Pa_, True),
    "J": (1.0, J_, True), "W": (1.0, W_, True), "C": (1.0, C_, True), "V": (1.0, V_, True),
    "F": (1.0, C_ / V_, True), "Ω": (1.0, V_ / I, True), "ohm": (1.0, V_ / I, True),
    "S": (1.0, I / V_, True), "Wb": (1.0, V_ * T, True), "T": (1.0, V_ * T / L**2, True),
    "H": (1.0, V_ * T / I, True), "lm": (1.0, J, True), "lx": (1.0, J / L**2, True),
    "Bq": (1.0, DIMLESS / T, True), "Gy": (1.0, J_ / M, True), "Sv": (1.0, J_ / M, True),
    "kat": (1.0, N / T, True),
    # time
    "min": (60.0, T, False), "hr": (3600.0, T, False), "hour": (3600.0, T, False),
    "day": (86400.0, T, False), "yr": (365.25 * 86400, T, True), "year": (365.25 * 86400, T, False),
    # length
    "Å": (1e-10, L, False), "angstrom": (1e-10, L, False),
    "au": (_au, L, False), "AU": (_au, L, False), "ly": (_ly, L, True), "pc": (_pc, L, True),
    "inch": (0.0254, L, False), "in_": (0.0254, L, False), "ft": (0.3048, L, False),
    "yd": (0.9144, L, False), "mi": (1609.344, L, False), "mile": (1609.344, L, False),
    "R☉": (6.957e8, L, False), "Rsun": (6.957e8, L, False),
    "R_E": (6.3781e6, L, False), "Rearth": (6.3781e6, L, False),
    # area / volume
    "b": (1e-28, L**2, True), "barn": (1e-28, L**2, False), "ha": (1e4, L**2, False),
    "L": (1e-3, L**3, True), "l": (1e-3, L**3, False),
    # mass
    "u": (_u, M, False), "amu": (_u, M, False), "Da": (_u, M, True),
    "tonne": (1000.0, M, False), "lb": (0.45359237, M, False),
    "M☉": (1.98841e30, M, False), "Msun": (1.98841e30, M, False),
    "M_E": (5.9722e24, M, False), "Mearth": (5.9722e24, M, False),
    # energy / power
    "eV": (_e, J_, True), "erg": (1e-7, J_, False), "cal": (4.184, J_, True),
    "L☉": (3.828e26, W_, False), "Lsun": (3.828e26, W_, False),
    "hp": (745.69987158227022, W_, False),
    # force / pressure
    "dyn": (1e-5, N_, False), "lbf": (4.4482216152605, N_, False),
    "bar": (1e5, Pa_, True), "atm": (101325.0, Pa_, False), "Torr": (101325.0 / 760, Pa_, False),
    "mmHg": (133.322387415, Pa_, False), "psi": (6894.757293168361, Pa_, False),
    # EM (CGS)
    "gauss": (1e-4, V_ * T / L**2, False), "Gs": (1e-4, V_ * T / L**2, False),
    # velocity
    "c": (_c, L / T, False), "kph": (1000 / 3600, L / T, False), "mph": (0.44704, L / T, False),
    # radioactivity
    "Ci": (3.7e10, DIMLESS / T, True),
}

# Affine temperature units: SI value = x * factor + offset.
_AFFINE = {
    "°C": (1.0, 273.15), "degC": (1.0, 273.15),
    "°F": (5 / 9, 273.15 - 32 * 5 / 9), "degF": (5 / 9, 273.15 - 32 * 5 / 9),
}

PREFIXES = {
    "Q": 1e30, "R": 1e27, "Y": 1e24, "Z": 1e21, "E": 1e18, "P": 1e15, "T": 1e12, "G": 1e9,
    "M": 1e6, "k": 1e3, "h": 1e2, "da": 1e1, "d": 1e-1, "c": 1e-2, "m": 1e-3,
    "μ": 1e-6, "u": 1e-6, "n": 1e-9, "p": 1e-12, "f": 1e-15, "a": 1e-18, "z": 1e-21,
    "y": 1e-24, "r": 1e-27, "q": 1e-30,
}

# Prefixed forms we refuse because they collide with common names or other units.
_BLOCKED = {"ft", "mi", "Pa" + "", "cd", "min", "pc", "ha", "nmi", "Gs", "ms_", "dam"}

# ASCII <-> pretty spellings of unit names (used by `fermium fmt`).
UNIT_PRETTY = {"deg": "°", "degC": "°C", "degF": "°F", "angstrom": "Å", "ohm": "Ω",
               "Msun": "M☉", "Rsun": "R☉", "Lsun": "L☉"}
UNIT_ASCII = {v: k for k, v in UNIT_PRETTY.items()}


def lookup_unit(name: str):
    """Return a Unit for a single unit name (possibly prefixed), or None."""
    if name in _AFFINE:
        f, off = _AFFINE[name]
        return Unit(name, TH, f, off)
    if name in _UNITS:
        f, d, _ = _UNITS[name]
        return Unit(name, d, f)
    if name.startswith("µ"):  # micro sign -> Greek mu
        name = "μ" + name[1:]
    for p in sorted(PREFIXES, key=len, reverse=True):
        if name.startswith(p) and len(name) > len(p):
            rest = name[len(p):]
            if rest in _UNITS and _UNITS[rest][2] and name not in _BLOCKED:
                f, d, _ = _UNITS[rest]
                if rest == "g":  # kg etc.
                    return Unit(name, d, f * PREFIXES[p])
                return Unit(name, d, f * PREFIXES[p])
    return None


def is_unit_name(name: str) -> bool:
    return lookup_unit(name) is not None


# ---------------------------------------------------------------------------
# Display: choosing a nice unit for printing a value of a given dimension.
# ---------------------------------------------------------------------------
def _u(expr):
    return parse_unit_string(expr)


_PREFERRED_SPECS = [
    "m", "kg", "s", "A", "K", "mol", "cd",
    "m/s", "m/s²", "m²", "m³", "kg/m³", "kg m/s", "N", "J", "W", "Pa", "C", "V", "Ω", "F", "T", "Wb", "H",
    "N/m", "J/K", "J s", "N m²/kg²", "W/m²", "W/(m² K⁴)", "J/(kg K)", "J/(mol K)", "1/mol", "F/m", "H/m",
    "kg m²", "m²/s", "m³/(kg s²)", "V/m", "A/m", "kg/s", "J/kg", "Pa s", "C/kg", "W/m³", "C²", "C m",
    "J/T", "C/m²", "C/m³", "1/m", "1/m²", "1/m³", "kg/m²", "J/m³", "N/m²",
]
_PREFERRED = None


def preferred_unit(d: Dim):
    """A display Unit for dimension d (SI, with a named derived unit if one fits)."""
    global _PREFERRED
    if _PREFERRED is None:
        _PREFERRED = {}
        for spec in _PREFERRED_SPECS:
            u = _u(spec)
            _PREFERRED.setdefault(u.dim, Unit(spec, u.dim, 1.0))
    if d in _PREFERRED:
        return _PREFERRED[d]
    if d == DIMLESS / T:
        return Unit("1/s", d, 1.0)
    return Unit(format_dim(d), d, 1.0)


_DIM_NAMES = [
    ("length", "m"), ("mass", "kg"), ("time", "s"), ("current", "A"), ("temperature", "K"),
    ("amount of substance", "mol"), ("luminous intensity", "cd"),
    ("speed", "m/s"), ("acceleration", "m/s²"), ("area", "m²"), ("volume", "m³"), ("density", "kg/m³"),
    ("momentum", "kg m/s"), ("force", "N"), ("energy", "J"), ("power", "W"), ("pressure", "Pa"),
    ("charge", "C"), ("voltage", "V"), ("resistance", "Ω"), ("capacitance", "F"),
    ("magnetic field", "T"), ("magnetic flux", "Wb"), ("inductance", "H"), ("frequency", "1/s"),
    ("spring constant", "N/m"), ("action", "J s"), ("angular momentum", "kg m²/s"),
    ("heat capacity", "J/K"), ("electric field", "V/m"), ("moment of inertia", "kg m²"),
    ("mass flow", "kg/s"), ("specific energy", "J/kg"),
]
_DIM_NAME_MAP = None


def dim_name(d: Dim) -> str:
    """Human description: 'length [m]', 'energy [J]', 'a quantity with units [kg/s³]'."""
    global _DIM_NAME_MAP
    if _DIM_NAME_MAP is None:
        _DIM_NAME_MAP = {}
        for name, spec in _DIM_NAME_MAPPINGS():
            _DIM_NAME_MAP.setdefault(_u(spec).dim, (name, spec))
    if d.dimensionless:
        return "a plain number (no units)"
    if d in _DIM_NAME_MAP:
        name, spec = _DIM_NAME_MAP[d]
        return f"{name} [{spec}]"
    return f"a quantity with units [{preferred_unit(d).name}]"


def _DIM_NAME_MAPPINGS():
    return _DIM_NAMES


# ---------------------------------------------------------------------------
# Parsing unit strings (used for CSV headers, display units and tests).
# The language parser has its own unit-expression parser producing the same
# structure; this one works on plain strings like "kg m/s²" or "J/(mol K)".
# ---------------------------------------------------------------------------
_SUP_TO_ASCII = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")


class UnitSyntaxError(ValueError):
    pass


def parse_unit_string(text: str) -> Unit:
    toks = _tokenize_unit(text)
    pos = [0]

    def peek():
        return toks[pos[0]] if pos[0] < len(toks) else None

    def take():
        t = toks[pos[0]]
        pos[0] += 1
        return t

    def factor():
        t = peek()
        if t is None:
            raise UnitSyntaxError(f"incomplete unit '{text}'")
        if t == "(":
            take()
            u = product()
            if peek() != ")":
                raise UnitSyntaxError(f"missing ')' in unit '{text}'")
            take()
        elif t == "1":
            take()
            u = Unit("1", DIMLESS, 1.0)
        elif t[0] == "#":
            raise UnitSyntaxError(f"unexpected '{t[1:]}' in unit '{text}'")
        else:
            take()
            u = lookup_unit(t)
            if u is None:
                raise UnitSyntaxError(f"unknown unit '{t}'")
        if peek() == "^":
            take()
            e = take()
            if e == "(":
                num = take()
                if peek() == "/":
                    take()
                    den = take()
                    p = Fraction(int(num), int(den))
                else:
                    p = Fraction(int(num))
                take()  # )
            else:
                p = Fraction(int(e))
            u = u ** p
        return u

    def product():
        u = factor()
        while True:
            t = peek()
            if t in ("*", "·"):
                take()
                u = u * factor()
            elif t == "/":
                take()
                u = u / factor()
            elif t is not None and t not in (")",) and t != "^":
                u = u * factor()
            else:
                return u

    u = product()
    if pos[0] != len(toks):
        raise UnitSyntaxError(f"can't read unit '{text}'")
    if u.affine is False and any(n in text for n in _AFFINE) and len(toks) > 1:
        raise UnitSyntaxError(f"°C/°F can't be combined with other units in '{text}'; use K")
    u.name = text.strip()
    return u


def _tokenize_unit(text):
    out = []
    i = 0
    while i < len(text):
        ch = text[i]
        if ch.isspace():
            i += 1
        elif ch in "()*/·^":
            out.append(ch)
            i += 1
        elif ch in "⁰¹²³⁴⁵⁶⁷⁸⁹⁻":
            j = i
            while j < len(text) and text[j] in "⁰¹²³⁴⁵⁶⁷⁸⁹⁻":
                j += 1
            out += ["^", text[i:j].translate(_SUP_TO_ASCII)]
            i = j
        elif ch.isdigit() or (ch == "-" and out and out[-1] in ("^", "(")):
            j = i + 1
            while j < len(text) and text[j].isdigit():
                j += 1
            out.append(text[i:j])
            i = j
        elif ch.isalpha() or ch in "°Ω☉Å%_µμ":
            j = i
            while j < len(text) and (text[j].isalpha() or text[j] in "°Ω☉Å%_µμ"):
                j += 1
            out.append(text[i:j])
            i = j
        else:
            out.append("#" + ch)
            i += 1
    # affine names written with a space are not supported; fine.
    if len(out) == 1 and out[0] in _AFFINE:
        return out
    return out


# ---------------------------------------------------------------------------
# Number formatting with significant figures and pretty exponents.
# ---------------------------------------------------------------------------
def format_number(x: float, sig: int = 6, trim: bool = True) -> str:
    if x != x:
        return "NaN"
    if x in (math.inf, -math.inf):
        return "∞" if x > 0 else "-∞"
    if x == 0:
        return "0"
    if trim and x == int(x) and abs(x) < 1e7 and sig >= 6:
        return str(int(x))          # whole numbers print exactly (1048576, not 1.04858×10⁶)
    sig = max(1, min(sig, 17))
    # let Python do the decimal rounding, then read mantissa and exponent from the text
    # (no 10**exp arithmetic, which underflows for subnormal numbers)
    m, _, e = f"{x:.{sig - 1}e}".partition("e")
    exp = int(e)
    if -4 <= exp < 6:
        decimals = max(sig - 1 - exp, 0)
        s = f"{float(m + 'e' + e):.{decimals}f}"
        if trim and "." in s:
            s = s.rstrip("0").rstrip(".")
        return s
    if trim and "." in m:
        m = m.rstrip("0").rstrip(".")
    return f"{m}×10{str(exp).translate(SUPERSCRIPTS)}"
