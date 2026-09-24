"""Compile and run Fermium programs: parse -> check -> LLVM -> JIT -> run."""
from __future__ import annotations

import ctypes
import os
import sys
import time

import llvmlite.binding as llvm

from . import ir as I
from .checker import Checker
from .codegen_llvm import ModuleGen
from .errors import Diagnostics, FermiumError, FermiumRuntimeError
from .parser import parse
from .runtime.core import Runtime, init_llvm
from .types import ListTy

_TM = None


def target_machine():
    """A fresh TargetMachine each time: an execution engine takes ownership of its machine."""
    global _TM
    init_llvm()
    if _TM is None:
        _TM = (llvm.get_host_cpu_name(), llvm.get_host_cpu_features().flatten())
    target = llvm.Target.from_default_triple()
    return target.create_target_machine(cpu=_TM[0], features=_TM[1], opt=3, reloc="default",
                                        codemodel="jitdefault")


def finalize_tables(tables, U, start=0):
    """Resolve dimension expressions now that all constraints are known."""
    for f in tables.fmts:
        f["rdim"] = U.resolve(f["dim"])
    for p in tables.plots:
        for s in p["series"]:
            s["rydim"] = U.resolve(s["ydim"])
            s["rxdim"] = U.resolve(s["xdim"])
    for f in tables.fits:
        f["rdims"] = [U.resolve(d) for d in f["dims"]]
        f["rydim"] = U.resolve(f["ydim"])


def optimize(llmod, tm, level=3):
    pto = llvm.create_pipeline_tuning_options(speed_level=level)
    pb = llvm.create_pass_builder(tm, pto)
    mpm = pb.getModulePassManager()
    mpm.run(llmod, pb)


class Program:
    """A compiled program, ready to run."""

    def __init__(self, source, filename="<program>", base_dir=None, opt_level=3, out=None):
        self.source = source
        self.filename = filename
        self.base_dir = base_dir or (os.path.dirname(os.path.abspath(filename)) if filename and not
                                     filename.startswith("<") else os.getcwd())
        self.diags = Diagnostics()
        self.out = out or sys.stdout
        self.timings = {}
        t0 = time.perf_counter()
        prog = parse(source, self.diags)
        t1 = time.perf_counter()
        self.checker = Checker(self.diags, self.base_dir, repl=False)
        self.module = self.checker.check_program(prog)
        t2 = time.perf_counter()
        finalize_tables(self.module.tables, self.checker.U)
        self.runtime = Runtime(self.out, self.base_dir)
        self.runtime.tables = self.module.tables
        mg = ModuleGen()
        mg.emit_main(self.module.main, "fm_run")
        self.llvm_ir = str(mg.module)
        t3 = time.perf_counter()
        tm = target_machine()
        llmod = llvm.parse_assembly(self.llvm_ir)
        llmod.triple = tm.triple
        llmod.data_layout = str(tm.target_data)
        llmod.verify()
        if opt_level:
            optimize(llmod, tm, opt_level)
        self.engine = llvm.create_mcjit_compiler(llmod, tm)
        self.engine.finalize_object()
        self.runtime.engine_ref = self.engine
        self.entry = ctypes.CFUNCTYPE(ctypes.c_int32)(self.engine.get_function_address("fm_run"))
        t4 = time.perf_counter()
        self.timings = {"parse": t1 - t0, "check": t2 - t1, "codegen": t3 - t2, "llvm": t4 - t3}

    def run(self):
        rt = self.runtime
        rt.error = None
        rt.error_line = None
        t0 = time.perf_counter()
        code = self.entry()
        self.timings["run"] = time.perf_counter() - t0
        if rt.line:
            self.out.write(" ".join(rt.line) + "\n")
            rt.line = []
        if code != 0 or rt.error:
            raise FermiumRuntimeError(rt.error or "runtime error", rt.error_line)


def run_source(source, filename="<program>", out=None, base_dir=None, show_warnings=True, err=None):
    err = err or sys.stderr
    p = Program(source, filename, base_dir=base_dir, out=out)
    if show_warnings:
        for w in p.diags.warnings:
            err.write(w.format(source, None) + "\n")
    p.run()
    return p


class ReplSession:
    """Incremental compilation: each input becomes a new LLVM module sharing an arena of globals."""

    ARENA_SLOTS = 1 << 16

    def __init__(self, out=None, base_dir=None):
        self.out = out or sys.stdout
        self.base_dir = base_dir or os.getcwd()
        self.diags = Diagnostics()
        self.checker = Checker(self.diags, self.base_dir, repl=True)
        self.arena = (ctypes.c_double * self.ARENA_SLOTS)()
        self.arena_base = ctypes.addressof(self.arena)
        self.next_slot = 0
        self.runtime = Runtime(self.out, self.base_dir)
        self.runtime.tables = self.checker.tables
        self.engines = []
        self.known = set()
        self.count = 0

    def assign_slots(self, module):
        def visit(sym):
            if sym.storage == "arena" and sym.slot is None:
                sym.slot = self.next_slot
                self.next_slot += 3 if isinstance(sym.ty, ListTy) else 1
        for sym in module.main.locals:
            visit(sym)

    def execute(self, text):
        """Compile and run one chunk of input.  Raises FermiumError on problems."""
        self.count += 1
        self.diags.warnings.clear()
        prog = parse(text, self.diags, known=self.known)
        module = self.checker.check_program(prog, name="main")
        for s in prog.body:
            if hasattr(s, "name"):
                self.known.add(s.name)
        finalize_tables(self.checker.tables, self.checker.U)
        self.assign_slots(module)
        mg = ModuleGen(name=f"repl{self.count}", arena_base=self.arena_base)
        entry = f"fm_run_{self.count}"
        mg.emit_main(module.main, entry)
        tm = target_machine()
        llmod = llvm.parse_assembly(str(mg.module))
        llmod.triple = tm.triple
        llmod.data_layout = str(tm.target_data)
        llmod.verify()
        optimize(llmod, tm, 2)
        engine = llvm.create_mcjit_compiler(llmod, tm)
        engine.finalize_object()
        self.engines.append(engine)
        self.runtime.engine_ref = engine
        fn = ctypes.CFUNCTYPE(ctypes.c_int32)(engine.get_function_address(entry))
        self.runtime.error = None
        for w in self.diags.warnings:
            self.out.write(w.format(text) + "\n")
        code = fn()
        if self.runtime.line:
            self.out.write(" ".join(self.runtime.line) + "\n")
            self.runtime.line = []
        if code != 0 or self.runtime.error:
            raise FermiumRuntimeError(self.runtime.error or "runtime error", self.runtime.error_line)


I, FermiumError  # re-exports
