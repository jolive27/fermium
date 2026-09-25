"""`fermium build prog.fm -o prog`: compile a program ahead of time into a standalone executable.

The program is compiled to a native object file by LLVM (the same code the JIT runs), and linked with a
small C runtime (runtime/aot_rt.c, which includes runtime/aot_data.c for load/fit/plot) plus generated
tables (fm_tables.c): print formats, texts, the CSV columns' units, fit parameters' display units and plot
labels.  Needs a C compiler/linker (`cc`; on macOS: `xcode-select --install`).

Differences from `fermium run` (DECISIONS D31): data files and plots are relative to the folder the
executable is run in (not the program's folder), and plots are written as SVG.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import csv
import tempfile
from xml.sax.saxutils import escape

import llvmlite.binding as llvm

from .checker import Checker
from .codegen_llvm import ModuleGen
from .driver import finalize_tables, optimize
from .errors import Diagnostics, FermiumError
from .parser import parse
from llvmlite import ir

from .runtime.core import display_unit, init_llvm

RT_C = os.path.join(os.path.dirname(os.path.abspath(__file__)), "runtime", "aot_rt.c")


def _c_string(s: str) -> str:
    out = []
    for b in s.encode("utf-8"):
        ch = chr(b)
        if ch in '"\\':
            out.append("\\" + ch)
        elif 32 <= b < 127:
            out.append(ch)
        else:
            out.append(f"\\{b:03o}")
    return '"' + "".join(out) + '"'


def _unit_name(u) -> str:
    return u.name if u.name not in ("", "1") else ""


def _svg_path(out: str) -> tuple[str, bool]:
    root, ext = os.path.splitext(out)
    if ext.lower() == ".svg":
        return out, False
    return root + ".svg", True


def _header_cells(full: str) -> list[str]:
    with open(full, newline="", encoding="utf-8-sig") as fh:
        return [h.strip() for h in next(csv.reader(fh))]


def tables_c(tables) -> str:
    cs = _c_string
    lines = ["#include <stdint.h>",
             "typedef struct { double factor, offset; int sf; int direct; const char *unit; } fm_fmt;",
             "const fm_fmt fm_fmts[] = {"]
    for f in tables.fmts:
        u = display_unit(f["rdim"], f["hint"])
        sf = -1 if f["sf"] is None else int(f["sf"])
        name = u.name if u.name not in ("1",) else ""
        lines.append(f"  {{{u.factor!r}, {u.offset!r}, {sf}, {int(f['direct'] or 0)}, {cs(name)}}},")
    lines.append("  {1.0, 0.0, -1, 0, \"\"}};")
    lines.append("const char *fm_texts[] = {")
    for t in tables.texts:
        lines.append(f"  {cs(t)},")
    lines.append('  ""};')
    # ---- load: each file's header (checked when the program runs) and its columns' SI conversion
    lines.append("typedef struct { const char *header; double factor, offset; } fm_colinfo;")
    lines.append("typedef struct { const char *path; int ncols; const fm_colinfo *cols; } fm_loadinfo;")
    for i, info in enumerate(tables.loads):
        heads = _header_cells(info["full"])
        cols = ", ".join(f"{{{cs(h)}, {c['unit'].factor!r}, {c['unit'].offset!r}}}"
                         for h, c in zip(heads, info["columns"]))
        lines.append(f"static const fm_colinfo fm_load{i}_cols[] = {{{cols or '{0}'}}};")
    lines.append("const fm_loadinfo fm_loads[] = {")
    for i, info in enumerate(tables.loads):
        lines.append(f"  {{{cs(info['path'])}, {len(info['columns'])}, fm_load{i}_cols}},")
    lines.append("  {0}};")
    # ---- fit: parameters' display units; the model is the compiled function fm_model_<i> (see build)
    lines.append("typedef void (*fm_model_fn)(double *, double **, int64_t, double *);")
    lines.append("typedef struct { const char *name; double factor; const char *unit; } fm_paraminfo;")
    lines.append("typedef struct { const char *text, *path; int nparams; const fm_paraminfo *params; int ncols; "
                 "const int *cols; double yfactor; const char *yunit; fm_model_fn model; } fm_fitinfo;")
    for i, info in enumerate(tables.fits):
        ps = []
        for name, dim in zip(info["params"], info["rdims"]):
            u = display_unit(dim, info.get("col_units", {}).get(dim))
            ps.append(f"{{{cs(name)}, {u.factor!r}, {cs(_unit_name(u))}}}")
        lines.append(f"static const fm_paraminfo fm_fit{i}_params[] = {{{', '.join(ps)}}};")
        lines.append(f"static const int fm_fit{i}_cols[] = {{{', '.join(str(c) for c in info['cols']) or '0'}}};")
        lines.append(f"extern void fm_model_{i}(double *, double **, int64_t, double *);")
    lines.append("const fm_fitinfo fm_fits[] = {")
    for i, info in enumerate(tables.fits):
        yu = display_unit(info["rydim"], info.get("col_units", {}).get(info["rydim"]))
        lines.append(f"  {{{cs(info['text'])}, {cs(info['path'])}, {len(info['params'])}, fm_fit{i}_params, "
                     f"{len(info['cols'])}, fm_fit{i}_cols, {yu.factor!r}, {cs(_unit_name(yu))}, fm_model_{i}}},")
    lines.append("  {0}};")
    # ---- plot: labels with units, options; the file is written as SVG
    lines.append("typedef struct { const char *legend, *ylabel, *xlabel; double yfactor, yoffset, xfactor, xoffset; "
                 "int points; } fm_seriesinfo;")
    lines.append("typedef struct { const char *svg, *shown; int renamed; const char *title; int logx, logy, equal, "
                 "nseries; const fm_seriesinfo *series; } fm_plotinfo;")
    for i, info in enumerate(tables.plots):
        ss = []
        for s in info["series"]:
            yu = display_unit(s["rydim"], s.get("yhint"))
            xu = display_unit(s["rxdim"], s.get("xhint"))
            yl = s["ylabel"] + (f" [{yu.name}]" if _unit_name(yu) else "")
            xl = s["xlabel"] + (f" [{xu.name}]" if _unit_name(xu) else "")
            ss.append(f"{{{cs(escape(s['ylabel']))}, {cs(escape(yl))}, {cs(escape(xl))}, {yu.factor!r}, "
                      f"{yu.offset!r}, {xu.factor!r}, {xu.offset!r}, {1 if s.get('points') else 0}}}")
        lines.append(f"static const fm_seriesinfo fm_plot{i}_series[] = {{{', '.join(ss) or '{0}'}}};")
    lines.append("const fm_plotinfo fm_plots[] = {")
    for i, info in enumerate(tables.plots):
        opts = info.get("options", {})
        svg, renamed = _svg_path(info["out"])
        equal = all(s["kind"] == "solxy" or s["rxdim"] == s["rydim"] and not s["rxdim"].dimensionless
                    for s in info["series"])
        lines.append(f"  {{{cs(svg)}, {cs(svg)}, {1 if renamed else 0}, {cs(escape(opts.get('title') or ''))}, "
                     f"{1 if opts.get('logx') else 0}, {1 if opts.get('logy') else 0}, {1 if equal else 0}, "
                     f"{len(info['series'])}, fm_plot{i}_series}},")
    lines.append("  {0}};")
    return "\n".join(lines) + "\n"


def add_model_wrappers(mg, tables):
    """Give each fit's compiled model a C-callable name: fm_model_<i> calls lam.<model>."""
    for i, info in enumerate(tables.fits):
        target = mg.lambda_fns[info["model"]]
        fn = ir.Function(mg.module, target.function_type, f"fm_model_{i}")
        b = ir.IRBuilder(fn.append_basic_block("entry"))
        b.call(target, list(fn.args))
        b.ret_void()


def find_cc():
    for cc in (os.environ.get("CC"), "cc", "clang", "gcc"):
        if cc and shutil.which(cc):
            return cc
    return None


def build(source: str, filename: str, output: str, diags: Diagnostics | None = None) -> str:
    diags = diags or Diagnostics()
    base_dir = os.path.dirname(os.path.abspath(filename))
    prog = parse(source, diags)
    ck = Checker(diags, base_dir)
    mod = ck.check_program(prog)
    t = mod.tables
    if t.stiff:          # the implicit solvers are SciPy's, and executables don't carry Python (D42)
        line, method = t.stiff[0]
        raise FermiumError(f"fermium build can't compile  using {method}  yet: the stiff solvers run in Python "
                           f"(SciPy); use  fermium run  for this program", line,
                           hint="or, if the equation isn't stiff, leave out  using …  to use rk45")
    for line, what in getattr(t, "python_only", []):     # eigenvalue problems and PDEs (D82, D83)
        if what.startswith("a call into Python"):          # use python (D140)
            raise FermiumError(f"fermium build can't compile {what}: an executable doesn't carry Python; use  "
                               f"fermium run  for this program", line,
                               hint="or write the function in Fermium, so it compiles into the executable")
        raise FermiumError(f"fermium build can't compile {what} yet: it runs in Python (NumPy/SciPy); use  "
                           f"fermium run  for this program", line)
    finalize_tables(t, ck.U)
    mg = ModuleGen()
    mg.emit_main(mod.main, "fm_run")
    add_model_wrappers(mg, t)
    init_llvm()
    target = llvm.Target.from_default_triple()
    tm = target.create_target_machine(cpu=llvm.get_host_cpu_name(), features=llvm.get_host_cpu_features().flatten(),
                                      opt=2, reloc="pic", codemodel="default")
    llmod = llvm.parse_assembly(str(mg.module))
    llmod.triple = tm.triple
    llmod.data_layout = str(tm.target_data)
    llmod.verify()
    optimize(llmod, tm, 2)
    obj = tm.emit_object(llmod)
    cc = find_cc()
    if cc is None:
        raise FermiumError("fermium build needs a C compiler to link the program, and none was found",
                           hint="on a Mac run  xcode-select --install ; on Linux install gcc or clang")
    with tempfile.TemporaryDirectory() as tmp:
        o = os.path.join(tmp, "prog.o")
        with open(o, "wb") as fh:
            fh.write(obj)
        tab = os.path.join(tmp, "fm_tables.c")
        with open(tab, "w", encoding="utf-8") as fh:
            fh.write(tables_c(t))
        cmd = [cc, "-O2", o, RT_C, tab, "-lm", "-lpthread", "-o", output]
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            raise FermiumError("linking the executable failed:\n" + (r.stderr.strip()[:2000] or r.stdout.strip()))
    return output
