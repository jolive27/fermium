"""Calling Fermium 2 (the Rust compiler) from Python (DECISIONS D142): the API of Fermium 1.5's `fermium/api.py`.

    import fermium2 as fermium
    mod = fermium.compile('''
    g = 9.81 m/s²
    period(L [m]) = 2π √(L / g)
    ''')
    mod.period(1.0)                    # Quantity(2.006..., 's'): a plain float is in SI units
    mod.period(fermium.Q(50, "cm"))    # any unit, converted to SI
    mod["g"]                           # Quantity(9.81, 'm/s²')

A pure-Python module: it loads the shared library built from rust/crates/fermium-pyapi (libfermium_pyapi.so,
.dylib on macOS) with ctypes, so it needs no compiler, no Python headers and nothing but NumPy.  The library
is found in $FERMIUM_PYAPI_LIB, next to this file, or in the repository's rust/target/{release,fast,debug}.

A program is checked and compiled by `compile` / `load`; its top level runs once, the first time a function is
called or a variable is read (or explicitly with `run()`).  Each call signature (the dimensions and shapes of the
arguments) is compiled once, on first use, as a generic Fermium function is instantiated per call site (D43):
the units are checked by the Fermium compiler, and a unit error is a FermiumError.
"""
from __future__ import annotations

import ctypes
import json
import os
import sys

import numpy as np

__all__ = ["compile", "load", "Module", "Quantity", "Q", "QuantityArray", "ComplexQuantity", "FermiumError",
           "FermiumRuntimeError"]


# ---------------------------------------------------------------- the library
def _find_library():
    names = ["libfermium_pyapi.so", "libfermium_pyapi.dylib", "fermium_pyapi.dll"]
    env = os.environ.get("FERMIUM_PYAPI_LIB")
    if env:
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    cands = [os.path.join(here, n) for n in names]
    root = os.path.dirname(os.path.dirname(here))      # rust/crates/fermium-pyapi
    target = os.path.join(os.path.dirname(os.path.dirname(root)), "target")
    for profile in ("release", "fast", "debug"):
        cands += [os.path.join(target, profile, n) for n in names]
    found = [c for c in cands if os.path.exists(c)]
    if not found:
        raise ImportError("fermium2 needs the Fermium library (libfermium_pyapi): build it with  cargo build "
                          "--release -p fermium-pyapi  in rust/, or set FERMIUM_PYAPI_LIB to its path")
    return max(found, key=os.path.getmtime)          # the most recently built


_lib = ctypes.CDLL(_find_library())
_c = ctypes.c_char_p
_P = ctypes.c_void_p
for _name, _args, _res in [
    ("fermium_free", [_P], None),
    ("fermium_compile", [_c, _c, _c, ctypes.POINTER(_P)], _P),
    ("fermium_release", [_P], None),
    ("fermium_names", [_P], _P),
    ("fermium_run", [_P], _P),
    ("fermium_read", [_P, _c], _P),
    ("fermium_params", [_P, _c], _P),
    ("fermium_call", [_P, _c, ctypes.c_int64, ctypes.POINTER(_c), ctypes.POINTER(ctypes.c_int64),
                      ctypes.POINTER(ctypes.c_double)], _P),
    ("fermium_unit", [_c], _P),
    ("fermium_dim", [_c], _P),
    ("fermium_format_number", [ctypes.c_double, ctypes.c_int64], _P),
    ("fermium_hz_mixup", [_c, _c], _P),
]:
    _f = getattr(_lib, _name)
    _f.argtypes, _f.restype = _args, _res


def _take(p):
    """The text a library call returned (and free it)."""
    try:
        return ctypes.string_at(p).decode("utf-8")
    finally:
        _lib.fermium_free(p)


def _json(p):
    return json.loads(_take(p))


def _b(s):
    return str(s).encode("utf-8")


class FermiumError(Exception):
    """A Fermium compile error (a unit mistake, an undefined name, …): .message, .line, .hint."""

    def __init__(self, message, line=None, hint=None):
        super().__init__(message)
        self.message, self.line, self.hint = message, line, hint

    def __str__(self):
        where = f"line {self.line}: " if self.line else ""
        return where + self.message + (f"\n  hint: {self.hint}" if self.hint else "")


class FermiumRuntimeError(FermiumError):
    """The program stopped while running (an index out of range, runaway recursion, a failed Python call, …)."""


def _raise(err):
    cls = FermiumRuntimeError if err.get("kind") == "runtime" else FermiumError
    if err.get("kind") == "type":
        raise TypeError(err["message"])
    raise cls(err["message"], err.get("line"), err.get("hint"))


# ---------------------------------------------------------------- units
class _Unit:
    __slots__ = ("name", "factor", "offset", "dim")

    def __init__(self, name, factor, offset, dim):
        self.name, self.factor, self.offset, self.dim = name, factor, offset, dim

    @classmethod
    def of(cls, d):
        return cls(d["name"], d["factor"], d["offset"], d["dim"])


_DIMLESS = "0,0,0,0,0,0,0"
_ONE = _Unit("1", 1.0, 0.0, _DIMLESS)
_units = {}


def _unit(u):
    if isinstance(u, _Unit):
        return u
    if u in (None, "", "1"):
        return _ONE
    key = str(u)
    if key not in _units:
        r = _json(_lib.fermium_unit(_b(key)))
        if "error" in r:
            raise ValueError(f"not a unit Fermium knows: {u!r} ({r['error']['message']})")
        _units[key] = _Unit.of(r)
    return _units[key]


def _preferred(dim):
    return _Unit.of(_json(_lib.fermium_dim(_b(dim)))["unit"])


def _dim_name(dim):
    return _json(_lib.fermium_dim(_b(dim)))["name"]


def format_number(x, sig=6):
    return _take(_lib.fermium_format_number(float(x), int(sig)))


def _delta_name(name):
    """ΔT, Δθ, δT, delta_T, dT: a name that says "a change" (D181)."""
    return name.startswith(("Δ", "δ")) or name.lower().startswith("delta") or name in ("dT", "dθ", "d_T")


class Quantity(float):
    """A number with a unit.  It *is* a float, holding the value in SI units, so it works anywhere a float does
    (NumPy, math, matplotlib); arithmetic on it gives plain floats (Python doesn't check units).

        Quantity(50, "cm")      # 0.5 as a float; .unit == 'cm', .to("m") == 0.5
    """

    def __new__(cls, value, unit=""):
        u = _unit(unit)
        q = float.__new__(cls, float(value) * u.factor + u.offset)
        q.dim = u.dim
        q.shown = u
        return q

    @classmethod
    def _si(cls, v, dim, shown):
        q = float.__new__(cls, v)
        q.dim, q.shown = dim, shown
        return q

    @property
    def unit(self):
        """The unit it is shown in, like 'm/s²' ('' for a plain number)."""
        n = self.shown.name
        return "" if n in ("1", "") else n

    @property
    def si(self):
        return float(self)

    @property
    def value(self):
        """The number in its display unit (`unit`)."""
        return (float(self) - self.shown.offset) / self.shown.factor

    def to(self, unit):
        """The number in another unit of the same dimension: Q(1, 'km').to('m') == 1000.0"""
        u = _unit(unit)
        if u.dim != self.dim:
            raise ValueError(f"can't convert {self} to {unit}: different dimensions")
        return (float(self) - u.offset) / u.factor

    def __repr__(self):
        return f"Quantity({self.value!r}, {self.unit!r})"

    def __str__(self):
        s = format_number(self.value, 6)
        return f"{s} {self.unit}" if self.unit else s

    def __reduce__(self):
        return (Quantity, (self.value, self.unit))


Q = Quantity


class ComplexQuantity(complex):
    """A complex number with a unit.  It *is* a Python complex, holding the value in SI units (both parts share
    one unit); arithmetic on it gives plain complex numbers."""

    @classmethod
    def _si(cls, re, im, dim, shown):
        q = complex.__new__(cls, re, im)
        q.dim, q.shown = dim, shown
        return q

    @property
    def unit(self):
        n = self.shown.name
        return "" if n in ("1", "") else n

    @property
    def si(self):
        return complex(self)

    @property
    def value(self):
        return complex(self) / self.shown.factor

    def to(self, unit):
        u = _unit(unit)
        if u.dim != self.dim or u.offset:
            raise ValueError(f"can't convert {self} to {unit}: different dimensions")
        return complex(self) / u.factor

    def __repr__(self):
        return f"ComplexQuantity({self.value!r}, {self.unit!r})"

    def __str__(self):
        v = self.value
        sign = "-" if v.imag < 0 or (v.imag == 0 and str(v.imag).startswith("-")) else "+"
        s = f"{format_number(v.real, 6)} {sign} {format_number(abs(v.imag), 6)}i"
        return f"({s}) {self.unit}" if self.unit else s


class QuantityArray(np.ndarray):
    """A NumPy array of SI values with a unit (a Fermium list).  Arithmetic and NumPy functions on it give plain
    arrays (the unit isn't carried through Python computations); slicing keeps it."""

    def __new__(cls, values, unit=""):
        u = _unit(unit)
        obj = (np.asarray(values, dtype=float) * u.factor + u.offset).view(cls)
        obj.dim, obj.shown = u.dim, u
        return obj

    @classmethod
    def _si(cls, values, dim, shown):
        obj = np.asarray(values, dtype=float).view(cls)
        obj.dim, obj.shown = dim, shown
        return obj

    def __array_finalize__(self, obj):
        self.dim = getattr(obj, "dim", _DIMLESS)
        self.shown = getattr(obj, "shown", _ONE)

    def __array_wrap__(self, arr, context=None, return_scalar=False):
        out = np.asarray(arr).view(np.ndarray)          # computed arrays lose the unit
        return out[()] if return_scalar else out

    @property
    def unit(self):
        n = self.shown.name
        return "" if n in ("1", "") else n

    def to(self, unit):
        u = _unit(unit)
        if u.dim != self.dim:
            raise ValueError(f"can't convert a list in {self.unit or 'plain numbers'} to {unit}: different dimensions")
        return (self.view(np.ndarray) - u.offset) / u.factor

    def __repr__(self):
        return f"QuantityArray({np.array2string(self.to(self.shown), separator=', ')}, {self.unit!r})"


def _value(v):
    """A value the library returned (JSON) as Python."""
    t = v["t"]
    if t == "bool":
        return bool(v["v"])
    if t == "vec":
        dims = [c["dim"] for c in v["comps"]]
        if len(set(dims)) == 1:
            return QuantityArray._si(v["v"], dims[0], _Unit.of(v["comps"][0]["unit"]))
        return tuple(Quantity._si(x, c["dim"], _Unit.of(c["unit"])) for x, c in zip(v["v"], v["comps"]))
    shown = _Unit.of(v["unit"])
    if t == "num":
        return Quantity._si(v["v"], v["dim"], shown)
    if t == "list":
        return QuantityArray._si(v["v"], v["dim"], shown)
    if t == "complex":
        return ComplexQuantity._si(v["v"][0], v["v"][1], v["dim"], shown)
    if t == "mat":
        return QuantityArray._si(np.array(v["v"]).reshape(v["r"], v["c"]), v["dim"], shown)
    raise TypeError(f"a value of kind {t} can't be passed to Python yet")


# ---------------------------------------------------------------- the compiled program
class Module:
    """A compiled Fermium program.  `mod.f(…)` calls its function f; `mod["x"]` (or `mod.x`) reads its variable x."""

    def __init__(self, source, filename="<python>", base_dir=None, out=None, warnings=True):
        base_dir = base_dir or (os.path.dirname(os.path.abspath(filename)) if not filename.startswith("<")
                                else os.getcwd())
        self._filename = filename
        self._out = out
        self._p = None
        info = _P()
        p = _lib.fermium_compile(_b(source), _b(base_dir), _b(os.path.basename(filename)), ctypes.byref(info))
        r = _json(info.value)
        if not p:
            _raise(r["error"])
        self._p = p
        self._params = {}
        self.warnings = list(r["warnings"])
        self._show_warnings = warnings
        if warnings:
            for w in self.warnings:
                sys.stderr.write(w + "\n")

    def __del__(self):
        p = getattr(self, "_p", None)
        if p:
            self._p = None
            _lib.fermium_release(p)

    def _emit(self, r):
        out = r.get("out")
        if out:
            (self._out or sys.stdout).write(out)
        if "error" in r:
            _raise(r["error"])
        return r

    # ------------------------------------------------------------ running
    def run(self):
        """Run the program's top level (prints, plots, variables).  Returns the module."""
        self._emit(_json(_lib.fermium_run(self._p)))
        return self

    # ------------------------------------------------------------ names
    def _names(self):
        return _json(_lib.fermium_names(self._p))

    @property
    def functions(self):
        return sorted(self._names()["functions"])

    @property
    def variables(self):
        return sorted(self._names()["variables"])

    def __getitem__(self, name):
        if name in self._names()["functions"]:
            return Function(self, name)
        r = _json(_lib.fermium_read(self._p, _b(name)))
        if r.get("missing"):
            raise KeyError(f"the program has no variable called {name}")
        return _value(self._emit(r)["value"])

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        try:
            return self[name]
        except KeyError:
            raise AttributeError(f"the Fermium program has no function or variable called {name}") from None

    def __dir__(self):
        return sorted(set(super().__dir__()) | set(self.functions) | set(self.variables))

    def __repr__(self):
        return f"<Fermium program {self._filename}: functions {', '.join(self.functions) or '(none)'}>"


def _arg_kind(x, declared_dim, fname, pname):
    """(is a list, dimension, SI value(s)) of a Python argument.  A plain number is in SI units, with the dimension
    the parameter declares (dimensionless if it declares none)."""
    if isinstance(x, (bool, np.bool_)):
        raise TypeError(f"{fname}: {pname} is a bool; Fermium functions take numbers and lists of numbers")
    if isinstance(x, Quantity):
        return False, x.dim, [float(x)]
    if isinstance(x, QuantityArray):
        if x.ndim != 1:
            raise TypeError(f"{fname}: {pname} must be one-dimensional (a Fermium list)")
        return True, x.dim, np.asarray(x, dtype=float).tolist()
    if isinstance(x, (int, float, np.integer, np.floating)):
        return False, declared_dim or _DIMLESS, [float(x)]
    if isinstance(x, (list, tuple, np.ndarray)):
        items = list(x)
        dims = {q.dim for q in items if isinstance(q, Quantity)}
        if len(dims) > 1:
            raise TypeError(f"{fname}: the elements of {pname} have different units")
        if dims and not all(isinstance(q, Quantity) for q in items):
            raise TypeError(f"{fname}: {pname} mixes numbers with and without units")
        try:
            vals = [float(q) for q in items]
        except (TypeError, ValueError):
            raise TypeError(f"{fname}: {pname} must be a list of numbers") from None
        return True, (dims.pop() if dims else declared_dim or _DIMLESS), vals
    raise TypeError(f"{fname}: {pname} is a {type(x).__name__}; Fermium functions take numbers (floats or "
                    f"fermium2.Quantity) and lists of numbers")


class Function:
    """A function of a compiled program, callable from Python."""

    def __init__(self, mod, name):
        self._m = mod
        self.name = name

    def __repr__(self):
        return f"<Fermium function {self.name}>"

    def __call__(self, *args):
        m = self._m
        params = m._params.get(self.name)
        if params is None:
            params = m._params[self.name] = _json(_lib.fermium_params(m._p, _b(self.name)))["params"]
        if len(args) != len(params):
            raise TypeError(f"{self.name} takes {len(params)} argument{'s' if len(params) != 1 else ''} "
                            f"({', '.join(p['name'] for p in params)}), but got {len(args)}")
        kinds = []
        for x, p in zip(args, params):
            shown = getattr(x, "shown", None)
            if shown is not None and shown.offset and _delta_name(p["name"]):
                # Q(10, "°C") is 283.15 K; as a ΔT it would be 28× too big (D181)
                what = f"{format_number(x.value)} {shown.name} is {format_number(float(x))} K" \
                    if isinstance(x, Quantity) else f"values in {shown.name} are read as kelvins from absolute zero"
                raise FermiumError(f"calling {self.name} from Python: {p['name']} looks like a temperature change, "
                                   f"but a value in {shown.name} is an absolute temperature ({what})",
                                   hint="pass a change of temperature in K, like Q(10, \"K\")")
            pu = p["unit"]
            if pu is not None and shown is not None:
                # Q(60, "rpm") for a parameter declared [Hz] arrives as 2π Hz (rad = 1): say so (D95, D202)
                got = _json(_lib.fermium_hz_mixup(_b(shown.name), _b(pu["name"])))
                if got is not None:
                    w = (f"warning: calling {self.name} from Python: {p['name']} is declared [{pu['name']}]: "
                         f"{got[0]}\n  hint: {got[1]}")
                    m.warnings.append(w)
                    if m._show_warnings:
                        sys.stderr.write(w + "\n")
            kinds.append(_arg_kind(x, pu["dim"] if pu is not None else None, self.name, p["name"]))
        n = len(kinds)
        dims = (_c * max(n, 1))(*[_b(d) for _, d, _ in kinds])
        lens = (ctypes.c_int64 * max(n, 1))(*[len(v) if lst else -1 for lst, _, v in kinds])
        flat = [x for _, _, v in kinds for x in v]
        data = (ctypes.c_double * max(len(flat), 1))(*flat)
        r = _json(_lib.fermium_call(m._p, _b(self.name), n, dims, lens, data))
        return _value(m._emit(r)["value"])


def compile(source, filename="<python>", base_dir=None, out=None, warnings=True):  # noqa: A001
    """Check and compile a Fermium program given as text.  Raises FermiumError (with the line) on errors."""
    return Module(source, filename, base_dir=base_dir, out=out, warnings=warnings)


def load(path, out=None, warnings=True):
    """Check and compile the Fermium program in a file."""
    with open(path, encoding="utf-8") as fh:
        src = fh.read()
    return Module(src, path, out=out, warnings=warnings)
