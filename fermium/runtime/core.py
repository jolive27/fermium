"""The Python half of the runtime: JIT engine setup and the callbacks compiled
code uses for things that aren't on the hot path (printing, plots, CSV files, fits).
"""
from __future__ import annotations

import ctypes
import math
import re
import os
import sys
import time

import llvmlite.binding as llvm

from ..errors import FermiumRuntimeError
from ..units import preferred_unit, format_number, Unit

_initialized = False


def init_llvm():
    global _initialized
    if not _initialized:
        llvm.initialize_native_target()
        llvm.initialize_native_asmprinter()
        _initialized = True


CB = ctypes.CFUNCTYPE
c_double, c_int64, c_void_p = ctypes.c_double, ctypes.c_int64, ctypes.c_void_p
DPTR = ctypes.POINTER(ctypes.c_double)


class SolStruct(ctypes.Structure):
    _fields_ = [("n", c_int64), ("dim", c_int64), ("cap", c_int64),
                ("t", DPTR), ("y", DPTR), ("dy", DPTR)]


def display_unit(dim, hint):
    if hint is not None and hint.dim == dim:
        return hint
    return preferred_unit(dim)


def format_quantity(v, dim, hint, sf, direct):
    u = display_unit(dim, hint)
    x = (v - u.offset) / u.factor
    if sf is None:
        s = format_number(x, 6, trim=True)
    else:
        s = format_number(x, sf if direct else max(sf, 2), trim=False)
    name = u.name
    if name in ("", "1"):
        return s
    if name in ("°", "%", "′", "″"):
        return f"{s}{name}"
    if hint is not None and u is hint and name != "c" and any(
            re.fullmatch(r"c([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+|\^-?\d+)?", tok) for tok in re.split(r"[\s/()·*]+", name)):
        si = preferred_unit(dim)
        return f"{s} {name} (= {format_number(v, 6)} {si.name})"
    return f"{s} {name}"


class Runtime:
    """Owns the callbacks; one instance per program run (or REPL session)."""

    def __init__(self, out=None, base_dir=".", show_plots=False):
        self.out = out or sys.stdout
        self.base_dir = base_dir
        self.tables = None
        self.U = None
        self.line = []
        self.error = None
        self.error_line = None
        self.datasets = {}
        self.keep = []
        self.plot_series = {}
        self.plots_saved = []
        self.engine_ref = None
        self._make_callbacks()

    # ------------------------------------------------------------ callbacks
    def _make_callbacks(self):
        rt = self

        def print_num(fid, v):
            f = rt.tables.fmts[fid]
            rt.line.append(format_quantity(v, f["rdim"], f["hint"], f["sf"], f["direct"]))

        def print_list(fid, p, n):
            f = rt.tables.fmts[fid]
            u = display_unit(f["rdim"], f["hint"])
            vals = [(p[i] - u.offset) / u.factor for i in range(n)]
            sf = f["sf"]
            if n > 12:
                shown = [format_number(x, sf or 4, trim=sf is None) for x in vals[:5]] + ["…"] + \
                        [format_number(x, sf or 4, trim=sf is None) for x in vals[-3:]]
            else:
                shown = [format_number(x, sf or 6, trim=sf is None) for x in vals]
            s = "[" + ", ".join(shown) + "]"
            if u.name not in ("", "1"):
                s += " " + u.name
            if n > 12:
                s += f"  ({n} values)"
            rt.line.append(s)

        def print_vec(fid, p, n):
            f = rt.tables.fmts[fid]
            u = display_unit(f["rdim"], f["hint"])
            sf = f["sf"]
            vals = [format_number(p[i] / u.factor, sf if f["direct"] and sf else max(sf or 6, 2) if sf else 6,
                                  trim=sf is None) for i in range(n)]
            s = "<" + ", ".join(vals) + ">"
            if u.name not in ("", "1"):
                s += " " + u.name
            rt.line.append(s)

        def print_bool(b):
            rt.line.append("true" if b else "false")

        def print_text(i):
            rt.line.append(rt.tables.texts[i])

        def print_end():
            try:
                rt.out.write(" ".join(rt.line) + "\n")
                rt.out.flush()
            except BrokenPipeError:        # e.g. `fermium run x.fm | head -1`
                try:
                    sys.stderr.close()
                finally:
                    os._exit(0)
            rt.line = []

        def error(kind, a, b, line):
            rt.error = rt.describe_error(kind, a, b)
            rt.error_line = line or None

        def plot_series(pid, idx, xp, nx, yp, ny):
            if nx != ny:
                rt.error = f"plot: the two lists have different lengths ({ny} and {nx} values)"
                return
            xs = [xp[i] for i in range(nx)]
            ys = [yp[i] for i in range(ny)]
            rt.plot_series.setdefault(pid, []).append((idx, xs, ys))

        def plot_sol(pid, idx, solp, comp, dy, comp2, dy2):
            s = ctypes.cast(solp, ctypes.POINTER(SolStruct)).contents
            ts, ys = sample_solution(s, comp, dy)
            if comp2 >= 0:
                _, xs = sample_solution(s, comp2, dy2)
            else:
                xs = ts
            rt.plot_series.setdefault(pid, []).append((idx, list(xs), list(ys)))

        def plot_done(pid):
            try:
                rt.make_plot(pid)
            except Exception as ex:  # plotting problems shouldn't crash the program
                rt.out.write(f"(plot not saved: {ex})\n")

        def load(i):
            return rt.load(i)

        def column(h, col, outp):
            arr = rt.datasets[h][col]
            outp[0] = arr.ctypes.data_as(DPTR)
            return len(arr)

        def fit(fid, h, p):
            try:
                rt.fit(fid, h, p)
            except FermiumRuntimeError as ex:
                rt.error = ex.message

        def sort(p, n):
            vals = sorted(p[i] for i in range(n))
            for i, v in enumerate(vals):
                p[i] = v

        self.callbacks = {
            "fm_print_num": CB(None, c_int64, c_double)(print_num),
            "fm_print_list": CB(None, c_int64, DPTR, c_int64)(print_list),
            "fm_print_bool": CB(None, c_int64)(print_bool),
            "fm_print_vec": CB(None, c_int64, DPTR, c_int64)(print_vec),
            "fm_print_text": CB(None, c_int64)(print_text),
            "fm_print_end": CB(None)(print_end),
            "fm_error": CB(None, c_int64, c_double, c_double, c_int64)(error),
            "fm_plot_series": CB(None, c_int64, c_int64, DPTR, c_int64, DPTR, c_int64)(plot_series),
            "fm_plot_sol": CB(None, c_int64, c_int64, c_void_p, c_int64, c_int64, c_int64, c_int64)(plot_sol),
            "fm_plot_done": CB(None, c_int64)(plot_done),
            "fm_load": CB(c_int64, c_int64)(load),
            "fm_column": CB(c_int64, c_int64, c_int64, ctypes.POINTER(DPTR))(column),
            "fm_fit": CB(None, c_int64, c_int64, DPTR)(fit),
            "fm_sort": CB(None, DPTR, c_int64)(sort),
            "fm_clock": CB(c_double)(time.perf_counter),
        }
        for name, cb in self.callbacks.items():
            llvm.add_symbol(name, ctypes.cast(cb, c_void_p).value)

    def describe_error(self, kind, a, b):
        if kind == 1:
            n = int(b)
            if a == a and a != int(a):
                return f"a list index must be a whole number (1, 2, 3, ...), not {format_number(a)}"
            if n == 0:
                return f"index {format_number(a)} is out of range: the list is empty"
            return f"index {format_number(a)} is out of range: the list has {n} element{'s' if n != 1 else ''} " \
                   f"(valid indexes are 1 to {n})"
        if kind == 2:
            return f"asked for the solution at {format_number(a)} (SI units), outside the range it was solved " \
                   f"for (it ends at {format_number(b)})"
        if kind == 3:
            return f"the ODE solver needed too many steps (reached t = {format_number(a)} in SI units); " \
                   f"the equation may be stiff or blow up"
        if kind == 4:
            return self.tables.texts[int(a)]
        if kind == 5:
            return f"these two lists have different lengths ({int(a)} and {int(b)})"
        if kind == 6:
            return "this list is empty"
        if kind == 7:
            return "the step must be a non-zero number that goes from the start towards the end"
        if kind == 8:
            return f"the ODE solver's step became too small near t = {format_number(a)} (SI units); " \
                   f"the solution may blow up there"
        return "runtime error"

    # ------------------------------------------------------------ data
    def load(self, i):
        import numpy as np
        import csv
        info = self.tables.loads[i]
        cols = info["columns"]
        rows = []
        with open(info["full"], newline="", encoding="utf-8-sig") as fh:
            r = csv.reader(fh)
            next(r)
            for ln, row in enumerate(r, start=2):
                if not row or all(not c.strip() for c in row):
                    continue
                if len(row) != len(cols):
                    self.error = f"{info['path']}, line {ln}: expected {len(cols)} values but found {len(row)}"
                    return 0
                try:
                    rows.append([float(c) for c in row])
                except ValueError:
                    self.error = f"{info['path']}, line {ln}: not a number: {row}"
                    return 0
        arr = np.array(rows, dtype=float).reshape(-1, len(cols))
        data = []
        for k, c in enumerate(cols):
            u = c["unit"]
            data.append(np.ascontiguousarray(arr[:, k] * u.factor + u.offset))
        h = len(self.datasets) + 1
        self.datasets[h] = data
        return h

    # ------------------------------------------------------------ fit
    def fit(self, fid, h, p):
        import numpy as np
        info = self.tables.fits[fid]
        data = self.datasets[h]
        addr = self.engine_ref.get_function_address("lam." + info["model"])
        model = ctypes.CFUNCTYPE(None, DPTR, ctypes.POINTER(DPTR), c_int64, DPTR)(addr)
        cols = [data[c] for c in info["cols"]]
        y = data[info["ycol"]]
        n = len(y)
        np_ = len(info["params"])
        colptrs = (DPTR * max(1, len(cols)))(*[c.ctypes.data_as(DPTR) for c in cols])
        out = np.zeros(n)

        def f(params):
            pa = (ctypes.c_double * np_)(*params)
            model(pa, colptrs, n, out.ctypes.data_as(DPTR))
            return out.copy()

        guess = [p[i] for i in range(np_)]
        guess = [g if math.isfinite(g) else math.nan for g in guess]
        from .fitting import least_squares_fit
        best, errs, rms = least_squares_fit(f, y, guess)
        for i in range(np_):
            p[i] = best[i]
        # report
        lines = [f"fit {info['text']}   ({n} data points from {info['path']})"]
        for i, name in enumerate(info["params"]):
            dim = info["rdims"][i]
            hint = info.get("col_units", {}).get(dim)   # e.g. show a time constant in the data's minutes
            u = display_unit(dim, hint)
            val = format_quantity(best[i], dim, hint, 4, False)
            if errs[i] is not None and math.isfinite(errs[i]):
                se = format_number(errs[i] / u.factor, 2, trim=False)
                unit = f" {u.name}" if u.name not in ("", "1") else ""
                lines.append(f"  {name} = {val}   (standard error {se}{unit})")
            else:
                lines.append(f"  {name} = {val}   (standard error could not be estimated)")
        yu = display_unit(info["rydim"], None)
        unit = f" {yu.name}" if yu.name not in ("", "1") else ""
        lines.append(f"  rms residual = {format_number(rms / yu.factor, 3, trim=False)}{unit}")
        self.out.write("\n".join(lines) + "\n")
        self.out.flush()

    # ------------------------------------------------------------ plots
    def make_plot(self, pid):
        info = self.tables.plots[pid]
        series = sorted(self.plot_series.pop(pid, []), key=lambda s: s[0])
        try:
            import matplotlib
            matplotlib.use("Agg")
            import matplotlib.pyplot as plt
        except ImportError:
            self.out.write("(plot skipped: matplotlib is not installed -- run: pip install matplotlib)\n")
            return
        fig, ax = plt.subplots(figsize=(7, 4.5), dpi=110)
        ylabels, xlabels = [], []
        for idx, xs, ys in series:
            s = info["series"][idx]
            yu = display_unit(s["rydim"], s.get("yhint"))
            xu = display_unit(s["rxdim"], s.get("xhint"))
            X = [(x - xu.offset) / xu.factor for x in xs]
            Y = [(y - yu.offset) / yu.factor for y in ys]
            if s.get("points"):
                ax.plot(X, Y, "o", label=s["ylabel"], markersize=5)     # measured data: markers, not lines
            else:
                ax.plot(X, Y, label=s["ylabel"], linewidth=1.8)
            ylabels.append(f"{s['ylabel']}" + (f" [{yu.name}]" if yu.name not in ("", "1") else ""))
            xlabels.append(f"{s['xlabel']}" + (f" [{xu.name}]" if xu.name not in ("", "1") else ""))
        ax.set_xlabel(xlabels[0] if xlabels else "")
        ax.set_ylabel(", ".join(dict.fromkeys(ylabels)))
        if len(series) > 1:
            ax.legend()
        ax.grid(True, alpha=0.3)
        if all(s["kind"] == "solxy" for s in info["series"]):
            ax.set_aspect("equal", adjustable="datalim")
        fig.tight_layout()
        full = info["full"]
        d = os.path.dirname(full)
        if d:
            os.makedirs(d, exist_ok=True)
        fig.savefig(full)
        plt.close(fig)
        self.plots_saved.append(full)
        self.out.write(f"plot saved to {info['out']}\n")
        self.out.flush()


def sample_solution(s: SolStruct, comp, use_dy, npts=600):
    """Dense samples of a solution component (cubic Hermite between steps)."""
    import numpy as np
    n, dim = s.n, s.dim
    t = np.ctypeslib.as_array(s.t, shape=(n,)).copy()
    y = np.ctypeslib.as_array(s.y, shape=(n * dim,)).reshape(n, dim)[:, comp].copy()
    dy = np.ctypeslib.as_array(s.dy, shape=(n * dim,)).reshape(n, dim)[:, comp].copy()
    if use_dy:
        return t, dy
    if n >= npts or n < 2:
        return t, y
    tt = np.linspace(t[0], t[-1], npts)
    i = np.clip(np.searchsorted(t, tt, side="right") - 1, 0, n - 2)
    h = t[i + 1] - t[i]
    u = (tt - t[i]) / h
    h00 = 2 * u**3 - 3 * u**2 + 1
    h10 = u**3 - 2 * u**2 + u
    h01 = -2 * u**3 + 3 * u**2
    h11 = u**3 - u**2
    yy = h00 * y[i] + h10 * h * dy[i] + h01 * y[i + 1] + h11 * h * dy[i + 1]
    return tt, yy


Unit  # re-export
