"""Calling compiled Fermium from Python (M6, DECISIONS D142).

    import fermium
    mod = fermium.compile('''
    g = 9.81 m/s²
    period(L [m]) = 2π √(L / g)
    ''')
    mod.period(1.0)                    # Quantity(2.006..., 's'): a plain float is in SI units
    mod.period(fermium.Q(50, "cm"))    # any unit, converted to SI
    mod["g"]                           # Quantity(9.81, 'm/s²')

A program is checked and compiled by `compile` / `load`; its top level runs once, the first time a function is
called or a variable is read (or explicitly with `run()`).  Each call signature (the dimensions and shapes of the
arguments) is compiled once, on first use, into its own JIT entry point, as a generic Fermium function (D43) is
instantiated per call site: the units are checked by the Fermium compiler, and a unit error is a FermiumError.
"""
from __future__ import annotations

import ctypes
import os
import sys

import numpy as np

from . import ir as I
from .checker import _delta_name
from .errors import FermiumError
from .types import BoolTy, ComplexTy, ListTy, MatTy, NumTy, VecTy
from .units import DIMLESS, Unit, format_number, parse_unit_string, preferred_unit

__all__ = ["compile", "load", "Module", "Quantity", "Q", "QuantityArray"]
_PREFIX = "fmpy_"          # the hidden arena variables that carry arguments and results of calls from Python


def _unit(u):
    if isinstance(u, Unit):
        return u
    if u in (None, "", "1"):
        return Unit("1", DIMLESS, 1.0)
    try:
        return parse_unit_string(str(u))
    except Exception as ex:
        raise ValueError(f"not a unit Fermium knows: {u!r} ({ex})") from None


def _display(dim, hint=None):
    if hint is not None and getattr(hint, "dim", None) == dim and not hint.offset:
        return hint
    return preferred_unit(dim)


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
    def _si(cls, v, dim, hint=None):
        q = float.__new__(cls, v)
        q.dim = dim
        q.shown = _display(dim, hint)
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
    """A complex number with a unit (red team round 3 #6).  It *is* a Python complex, holding the value in SI units
    (both parts share one unit, D91); arithmetic on it gives plain complex numbers."""

    @classmethod
    def _si(cls, re, im, dim, hint=None):
        q = complex.__new__(cls, re, im)
        q.dim = dim
        q.shown = _display(dim, hint)
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
        """The number in its display unit (`unit`)."""
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

    def __reduce__(self):
        return (_complex_quantity, (self.value, self.unit))


def _complex_quantity(value, unit):
    u = _unit(unit)
    return ComplexQuantity._si(value.real * u.factor, value.imag * u.factor, u.dim, u)


class QuantityArray(np.ndarray):
    """A NumPy array of SI values with a unit (a Fermium list).  Arithmetic and NumPy functions on it give plain
    arrays (the unit isn't carried through Python computations); slicing keeps it."""

    def __new__(cls, values, unit=""):
        u = _unit(unit)
        obj = (np.asarray(values, dtype=float) * u.factor + u.offset).view(cls)
        obj.dim, obj.shown = u.dim, u
        return obj

    @classmethod
    def _si(cls, values, dim, hint=None):
        obj = np.asarray(values, dtype=float).view(cls)
        obj.dim, obj.shown = dim, _display(dim, hint)
        return obj

    def __array_finalize__(self, obj):
        self.dim = getattr(obj, "dim", DIMLESS)
        self.shown = getattr(obj, "shown", Unit("1", DIMLESS, 1.0))

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


# ---------------------------------------------------------------- the compiled program
def _libc():
    from .runtime.core import _libc as lc
    libc = lc()
    if not hasattr(libc, "_fm_free"):
        libc.free.argtypes = [ctypes.c_void_p]
        libc.free.restype = None
        libc._fm_free = True
    return libc


class _Worker:
    """One long-lived thread with a 512 MB stack runs the compiled code (as `fermium run` does, so deep recursion
    is caught by the stack check instead of crashing Python); reusing it keeps a call cheap."""

    def __init__(self):
        import queue
        import threading
        self.jobs, self.done = queue.SimpleQueue(), queue.SimpleQueue()
        old = threading.stack_size()
        try:
            threading.stack_size(512 << 20)
            self.thread = threading.Thread(target=self._serve, daemon=True, name="fermium-jit")
            self.thread.start()
        finally:
            threading.stack_size(old)

    def _serve(self):
        while True:
            fn = self.jobs.get()
            try:
                self.done.put((True, fn()))
            except BaseException as ex:       # re-raised in the caller's thread
                self.done.put((False, ex))

    def call(self, fn):
        self.jobs.put(fn)
        ok, r = self.done.get()
        if not ok:
            raise r
        return r


_WORKER = None


def _run_compiled(fn):
    global _WORKER
    import threading
    if _WORKER is None:
        _WORKER = _Worker()
    if threading.current_thread() is _WORKER.thread:
        return fn()
    return _WORKER.call(fn)


def _session(out, base_dir):
    from .checker import Checker
    from .driver import ReplSession

    class _Session(ReplSession):
        where = "programs compiled from Python (fermium.compile, fermium.load)"
        where_hint = "run the program with  fermium run file.fm , or pass value(x) and uncertainty(x) separately"

        def __init__(self):
            super().__init__(out=out, base_dir=base_dir)
            # a program, not a prompt: the REPL's conveniences (echoing bare expressions, redefining a variable
            # with other units) are off; only its storage model (top-level variables in an arena) is kept
            self.checker = Checker(self.diags, self.base_dir, repl=False)
            self.checker.arena = True
            self.runtime.tables = self.checker.tables

        def run_entry(self, fn):
            from .errors import FermiumRuntimeError
            rt = self.runtime
            rt.error = None
            rt.error_line = None
            code = _run_compiled(fn)
            if rt.line:
                self.out.write(" ".join(rt.line) + "\n")
                rt.line = []
            if code != 0 or rt.error:
                raise FermiumRuntimeError(rt.error or "runtime error", rt.error_line)
    return _Session()


class Module:
    """A compiled Fermium program.  `mod.f(…)` calls its function f; `mod["x"]` (or `mod.x`) reads its variable x."""

    def __init__(self, source, filename="<python>", base_dir=None, out=None, warnings=True):
        from .errors import FermiumError as _FE
        base_dir = base_dir or (os.path.dirname(os.path.abspath(filename)) if not filename.startswith("<")
                                else os.getcwd())
        self._source = source
        self._filename = filename
        self._s = _session(out or sys.stdout, base_dir)
        self._calls = {}
        self._ran = False
        try:
            self._main = self._s.compile_input(source)
        except RecursionError:
            raise _FE("this program is nested too deeply for Fermium to compile") from None
        self.warnings = [w.format(source, None) for w in self._s.diags.warnings]
        if warnings:
            for w in self.warnings:
                sys.stderr.write(w + "\n")

    # ------------------------------------------------------------ running
    def run(self):
        """Run the program's top level (prints, plots, variables).  Returns the module."""
        self._s.run_entry(self._main)
        self._ran = True
        return self

    def _ensure_ran(self):
        if not self._ran:
            self.run()

    # ------------------------------------------------------------ names
    def _binding(self, name):
        return self._s.checker.globals.names.get(name)

    @property
    def functions(self):
        from .checker import FuncInfo
        return sorted(n for n, b in self._s.checker.globals.names.items()
                      if isinstance(b, FuncInfo) and not n.startswith("__") and "'" not in n and b.fdef is not None)

    @property
    def variables(self):
        return sorted(n for n, b in self._s.checker.globals.names.items()
                      if isinstance(b, I.Sym) and not n.startswith(("__", _PREFIX)) and b.slot is not None)

    def __getitem__(self, name):
        from .checker import FuncInfo
        b = self._binding(name)
        if isinstance(b, FuncInfo):
            return Function(self, name)
        if not isinstance(b, I.Sym) or b.slot is None:
            raise KeyError(f"the program has no variable called {name}")
        self._ensure_ran()
        return self._read(b)

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        try:
            return self[name]
        except KeyError:
            raise AttributeError(f"the Fermium program has no function or variable called {name}") from None

    def __dir__(self):
        return sorted(set(super().__dir__()) | set(self.functions) | set(self.variables))

    # ------------------------------------------------------------ values in the arena
    def _addr(self, sym):
        return self._s.arena_base + 8 * sym.slot

    def _read(self, sym):
        U = self._s.checker.U
        ty = sym.ty
        if isinstance(ty, NumTy):
            return Quantity._si(self._s.arena[sym.slot], U.resolve(ty.dim), sym.hint)
        if isinstance(ty, ListTy):
            hdr = ctypes.c_uint64.from_address(self._addr(sym)).value
            if not hdr:
                return QuantityArray._si([], U.resolve(ty.dim), sym.hint)
            data = ctypes.c_uint64.from_address(hdr).value
            n = ctypes.c_int64.from_address(hdr + 8).value
            vals = np.ctypeslib.as_array((ctypes.c_double * n).from_address(data)).copy() if n > 0 else []
            return QuantityArray._si(vals, U.resolve(ty.dim), sym.hint)
        if isinstance(ty, ComplexTy):       # a complex number, not the vector <re, im> (red team round 3 #6)
            return ComplexQuantity._si(self._s.arena[sym.slot], self._s.arena[sym.slot + 1], U.resolve(ty.dim),
                                       sym.hint)
        if isinstance(ty, VecTy) and not isinstance(ty, MatTy):
            vals = [self._s.arena[sym.slot + k] for k in range(ty.n)]
            dims = [U.resolve(d) for d in ty.comp_dims()] if ty.mixed else [U.resolve(ty.dim)] * ty.n
            if len(set(dims)) == 1:
                return QuantityArray._si(vals, dims[0], sym.hint)
            return tuple(Quantity._si(v, d) for v, d in zip(vals, dims))
        if isinstance(ty, MatTy):
            vals = [self._s.arena[sym.slot + k] for k in range(ty.n)]
            return QuantityArray._si(np.array(vals).reshape(ty.r, ty.c), U.resolve(ty.dim), sym.hint)
        if isinstance(ty, BoolTy):
            return bool(ctypes.c_bool.from_address(self._addr(sym)).value)
        raise TypeError(f"{sym.name} is {ty.kind}, which can't be passed to Python yet")

    def _write_list(self, sym, values):
        libc = _libc()
        n = len(values)
        data = libc.malloc(8 * max(n, 1))
        hdr = libc.malloc(24)
        if n:
            ctypes.memmove(data, np.ascontiguousarray(values, dtype=np.float64).ctypes.data, 8 * n)
        ctypes.c_uint64.from_address(hdr).value = data
        ctypes.c_int64.from_address(hdr + 8).value = n
        ctypes.c_int64.from_address(hdr + 16).value = n
        ctypes.c_uint64.from_address(self._addr(sym)).value = hdr
        return hdr

    def __repr__(self):
        return f"<Fermium program {self._filename}: functions {', '.join(self.functions) or '(none)'}>"


def _arg_kind(x, declared_dim, fname, pname):
    """(shape, Dim, SI value(s)) of a Python argument.  A plain number is in SI units, with the dimension the
    parameter declares (dimensionless if it declares none)."""
    if isinstance(x, (bool, np.bool_)):
        raise TypeError(f"{fname}: {pname} is a bool; Fermium functions take numbers and lists of numbers")
    if isinstance(x, Quantity):
        return "n", x.dim, float(x)
    if isinstance(x, QuantityArray):
        if x.ndim != 1:
            raise TypeError(f"{fname}: {pname} must be one-dimensional (a Fermium list)")
        return "l", x.dim, np.asarray(x, dtype=float)
    if isinstance(x, (int, float, np.integer, np.floating)):
        return "n", declared_dim or DIMLESS, float(x)
    if isinstance(x, (list, tuple, np.ndarray)):
        items = list(x)
        dims = {q.dim for q in items if isinstance(q, Quantity)}
        if len(dims) > 1:
            raise TypeError(f"{fname}: the elements of {pname} have different units")
        if dims and not all(isinstance(q, Quantity) for q in items):
            raise TypeError(f"{fname}: {pname} mixes numbers with and without units")
        try:
            arr = np.asarray([float(q) for q in items], dtype=float)
        except (TypeError, ValueError):
            raise TypeError(f"{fname}: {pname} must be a list of numbers") from None
        return "l", (dims.pop() if dims else declared_dim or DIMLESS), arr
    raise TypeError(f"{fname}: {pname} is a {type(x).__name__}; Fermium functions take numbers (floats or "
                    f"fermium.Quantity) and lists of numbers")


class Function:
    """A function of a compiled program, callable from Python."""

    def __init__(self, mod, name):
        self._m = mod
        self.name = name

    def __repr__(self):
        return f"<Fermium function {self.name}>"

    def __call__(self, *args):
        m, s = self._m, self._m._s
        info = m._binding(self.name)
        params = info.fdef.params
        if len(args) != len(params):
            raise TypeError(f"{self.name} takes {len(params)} argument{'s' if len(params) != 1 else ''} "
                            f"({', '.join(p.name for p in params)}), but got {len(args)}")
        kinds = []
        for x, p in zip(args, params):
            shown = getattr(x, "shown", None)
            if shown is not None and getattr(shown, "offset", 0) and _delta_name(p.name):
                # Q(10, "°C") is 283.15 K; as a ΔT it would be 28× too big (red team round 3 #2, D181)
                what = f"{format_number(x.value)} {shown.name} is {format_number(float(x))} K" \
                    if isinstance(x, Quantity) else f"values in {shown.name} are read as kelvins from absolute zero"
                raise FermiumError(f"calling {self.name} from Python: {p.name} looks like a temperature change, "
                                   f"but a value in {shown.name} is an absolute temperature ({what})",
                                   hint="pass a change of temperature in K, like Q(10, \"K\")")
            dd = s.checker.resolve_unit(p.unit).dim if p.unit is not None else None
            kinds.append(_arg_kind(x, dd, self.name, p.name))
        key = (self.name,) + tuple((k, d) for k, d, _ in kinds)
        entry = m._calls.get(key)
        if entry is None:
            entry = self._compile(key)
            m._calls[key] = entry
        m._ensure_ran()
        fn, syms, rsym = entry
        owned = []
        for (k, _, v), sym in zip(kinds, syms):
            if k == "n":
                s.arena[sym.slot] = v
            else:
                owned.append(m._write_list(sym, v))
        try:
            s.run_entry(fn)
            return m._read(rsym)
        finally:
            libc = _libc()
            for hdr in owned:           # the argument lists Python made (the result was copied out already)
                libc.free(ctypes.c_void_p(ctypes.c_uint64.from_address(hdr).value))
                libc.free(ctypes.c_void_p(hdr))

    def _compile(self, key):
        m, s = self._m, self._m._s
        n = len(m._calls)
        tag = f"{n}_{self.name}"
        syms = []
        for k, (shape, dim) in enumerate(key[1:]):
            name = f"{_PREFIX}{tag}_a{k}"
            sym = I.Sym(name, NumTy(dim) if shape == "n" else ListTy(dim), "arena")
            sym.slot = s.next_slot
            s.next_slot += 1
            s.checker.globals.names[name] = sym
            s.known.add(name)
            syms.append(sym)
        rname = f"{_PREFIX}{tag}_r"
        text = f"{rname} = {self.name}({', '.join(sy.name for sy in syms)})\n"
        try:
            fn = s.compile_input(text)
        except FermiumError as e:
            e.message = f"calling {self.name} from Python with ({_describe(key[1:])}): {e.message}"
            e.line = None
            raise
        rsym = s.checker.globals.names[rname]
        if not isinstance(rsym.ty, (NumTy, ListTy, VecTy, MatTy, BoolTy)):
            raise TypeError(f"{self.name} returns {rsym.ty.kind}, which can't be passed to Python yet")
        return fn, syms, rsym


def _describe(key):
    from .units import dim_name
    out = []
    for shape, dim in key:
        d = "a plain number" if dim.dimensionless else dim_name(dim)
        out.append(d if shape == "n" else f"a list of {d if dim.dimensionless else dim_name(dim)}")
    return ", ".join(out)


def compile(source, filename="<python>", base_dir=None, out=None, warnings=True):  # noqa: A001
    """Check and compile a Fermium program given as text.  Raises FermiumError (with the line) on errors."""
    return Module(source, filename, base_dir=base_dir, out=out, warnings=warnings)


def load(path, out=None, warnings=True):
    """Check and compile the Fermium program in a file."""
    with open(path, encoding="utf-8") as fh:
        src = fh.read()
    return Module(src, path, out=out, warnings=warnings)
