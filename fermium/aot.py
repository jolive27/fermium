"""`fermium build prog.fm -o prog`: compile a program ahead of time into a standalone executable.

The program is compiled to a native object file by LLVM (the same code the JIT runs), and linked with a
small C runtime (runtime/aot_rt.c) plus a generated table of print formats.  Needs a C compiler/linker
(`cc`; on macOS: `xcode-select --install`).  Plots, data files and fits need Python, so programs using
`plot`, `load` or `fit` can't be built yet.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile

import llvmlite.binding as llvm

from .checker import Checker
from .codegen_llvm import ModuleGen
from .driver import finalize_tables, optimize
from .errors import Diagnostics, FermiumError
from .parser import parse
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


def tables_c(tables) -> str:
    lines = ["#include <stdint.h>",
             "typedef struct { double factor, offset; int sf; int direct; const char *unit; } fm_fmt;",
             "const fm_fmt fm_fmts[] = {"]
    for f in tables.fmts:
        u = display_unit(f["rdim"], f["hint"])
        sf = -1 if f["sf"] is None else int(f["sf"])
        name = u.name if u.name not in ("1",) else ""
        lines.append(f"  {{{u.factor!r}, {u.offset!r}, {sf}, {1 if f['direct'] else 0}, {_c_string(name)}}},")
    lines.append("  {1.0, 0.0, -1, 0, \"\"}};")
    lines.append("const char *fm_texts[] = {")
    for t in tables.texts:
        lines.append(f"  {_c_string(t)},")
    lines.append('  ""};')
    return "\n".join(lines) + "\n"


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
    for what, items in (("plot", t.plots), ("load", t.loads), ("fit", t.fits)):
        if items:
            raise FermiumError(f"fermium build can't make an executable from a program that uses {what} yet",
                               hint="run it with  fermium run  instead")
    finalize_tables(t, ck.U)
    mg = ModuleGen()
    mg.emit_main(mod.main, "fm_run")
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
