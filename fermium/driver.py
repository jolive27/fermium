"""Compile and run Fermium programs: parse -> check -> LLVM -> JIT -> run."""
from __future__ import annotations

import ctypes
import sys as _sys
import os
import sys
import time

import llvmlite.binding as llvm

from . import ir as I
from .checker import Checker
from .codegen_llvm import ModuleGen
from .errors import Diagnostics, FermiumError, FermiumRuntimeError
from .tables import finalize_tables  # noqa: F401  (re-exported: aot.py and tests import it from here)
from .parser import parse
from .runtime.core import Runtime, init_llvm

_TM = None
_sys.setrecursionlimit(max(_sys.getrecursionlimit(), 20000))


def target_machine():
    """A fresh TargetMachine each time: an execution engine takes ownership of its machine."""
    global _TM
    init_llvm()
    if _TM is None:
        _TM = (llvm.get_host_cpu_name(), llvm.get_host_cpu_features().flatten())
    target = llvm.Target.from_default_triple()
    return target.create_target_machine(cpu=_TM[0], features=_TM[1], opt=3, reloc="default",
                                        codemodel="jitdefault")


def optimize(llmod, tm, level=3):
    pto = llvm.create_pipeline_tuning_options(speed_level=level)
    pb = llvm.create_pass_builder(tm, pto)
    mpm = pb.getModulePassManager()
    mpm.run(llmod, pb)


class _CtrlC:
    """While compiled code runs, Python never gets control back, so its Ctrl+C handling can't work.
    Install a C-level SIGINT handler (a ctypes callback) that says what happened and exits."""

    def __init__(self, out):
        self.out = out
        self.prev = None
        self.cb = None

    def __enter__(self):
        import signal
        if not hasattr(signal, "SIGINT") or os.name == "nt":
            return self
        try:
            import threading
            if threading.current_thread() is not threading.main_thread():
                return self
            libc = ctypes.CDLL(None)
            HANDLER = ctypes.CFUNCTYPE(None, ctypes.c_int)

            def stop(signum):
                try:
                    _sys.stdout.flush()
                except Exception:
                    pass
                os.write(2, b"\nstopped by Ctrl+C\n")
                os._exit(130)
            self.cb = HANDLER(stop)
            self.prev = signal.getsignal(signal.SIGINT)
            libc.signal.restype = ctypes.c_void_p
            libc.signal.argtypes = [ctypes.c_int, HANDLER]
            libc.signal(signal.SIGINT, self.cb)
        except (OSError, AttributeError, ValueError):
            self.prev = None
        return self

    def __exit__(self, *exc):
        import signal
        if self.prev is not None:
            signal.signal(signal.SIGINT, self.prev)
        return False


class Program:
    """A compiled program, ready to run."""

    def __init__(self, source, filename="<program>", base_dir=None, opt_level=2, out=None):
        self.source = source
        self.filename = filename
        self.base_dir = base_dir or (os.path.dirname(os.path.abspath(filename)) if filename and not
                                     filename.startswith("<") else os.getcwd())
        self.diags = Diagnostics()
        self.out = out or sys.stdout
        self.timings = {}
        t0 = time.perf_counter()
        try:
            prog = parse(source, self.diags)
            t1 = time.perf_counter()
            self.checker = Checker(self.diags, self.base_dir, repl=False)
            self.module = self.checker.check_program(prog)
        except FermiumError as e:
            e.warnings = list(self.diags.warnings)
            raise
        t2 = time.perf_counter()
        finalize_tables(self.module.tables, self.checker.U)
        self.runtime = Runtime(self.out, self.base_dir)
        self.runtime.tables = self.module.tables
        self.interpreted = getattr(self.module, "uses_unc", False)
        if self.interpreted:
            # uncertain values (±) run in the reference interpreter, not native code (D122)
            self.llvm_ir = ""
            self.timings = {"parse": t1 - t0, "check": t2 - t1, "codegen": 0.0, "llvm": 0.0}
            return
        mg = ModuleGen(rng_addr=self.runtime.rng_addr)
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
        if self.interpreted:
            from .interp import Interpreter
            try:
                Interpreter(self.module, rt).run()
            finally:
                self.timings["run"] = time.perf_counter() - t0
                if rt.line:
                    self.out.write(" ".join(rt.line) + "\n")
                    rt.line = []
            return
        with _CtrlC(self.out):
            code = call_with_big_stack(self.entry)
        self.timings["run"] = time.perf_counter() - t0
        if rt.line:
            self.out.write(" ".join(rt.line) + "\n")
            rt.line = []
        if code != 0 or rt.error:
            raise FermiumRuntimeError(rt.error or "runtime error", rt.error_line)


def call_with_big_stack(fn):
    """Run compiled code on a thread with a 512 MB stack (deep recursion is caught at 400 MB)."""
    import threading
    result = {}

    def target():
        try:
            result["v"] = fn()
        except BaseException as e:      # re-raised in the caller's thread
            result["e"] = e
    old = threading.stack_size()
    try:
        threading.stack_size(512 << 20)
    except (ValueError, RuntimeError):
        return fn()
    try:
        th = threading.Thread(target=target)
        th.start()
    finally:
        threading.stack_size(old)
    th.join()
    if "e" in result:
        raise result["e"]
    return result["v"]


def run_source(source, filename="<program>", out=None, base_dir=None, show_warnings=True, err=None):
    err = err or sys.stderr
    try:
        p = Program(source, filename, base_dir=base_dir, out=out)
    except RecursionError:
        raise FermiumError("this program is nested too deeply for Fermium to compile (very long or deeply "
                           "nested expressions)", hint="split the expression into several lines with names")
    except FermiumError as e:
        # show the warnings collected before the error -- they often explain it
        if show_warnings and getattr(e, "warnings", None):
            for w in e.warnings:
                err.write(w.format(source, None) + "\n")
        raise
    if show_warnings:
        for w in p.diags.warnings:
            err.write(w.format(source, None) + "\n")
    p.runtime.err = err
    p.run()
    return p


class ReplSession:
    """Incremental compilation: each input becomes a new LLVM module sharing an arena of globals."""

    ARENA_SLOTS = 1 << 16
    where = "the REPL or Jupyter"       # named in "doesn't work here yet" messages
    where_hint = "save the lines in a .fm file and run it"

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
                self.next_slot += getattr(sym.ty, "n", 1)
        for sym in module.main.locals:
            visit(sym)

    def execute(self, text):
        """Compile and run one chunk of input.  Raises FermiumError on problems."""
        fn = self.compile_input(text)
        for w in self.diags.warnings:
            self.out.write(w.format(text) + "\n")
        self.run_entry(fn)

    def compile_input(self, text):
        """Compile one chunk of input into a callable entry point (without running it)."""
        self.count += 1
        self.diags.warnings.clear()
        prog = parse(text, self.diags, known=self.known)
        module = self.checker.check_program(prog, name="main")
        if module.uses_unc:
            raise FermiumError(f"uncertainties (±, propagate montecarlo) work in programs (fermium run file.fm) "
                               f"but not yet in {self.where}", hint=self.where_hint)
        for s in prog.body:
            if hasattr(s, "name"):
                self.known.add(s.name)
        finalize_tables(self.checker.tables, self.checker.U)
        self.assign_slots(module)
        mg = ModuleGen(name=f"repl{self.count}", arena_base=self.arena_base, rng_addr=self.runtime.rng_addr)
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
        return ctypes.CFUNCTYPE(ctypes.c_int32)(engine.get_function_address(entry))

    def run_entry(self, fn):
        """Run a compiled entry point; raises FermiumRuntimeError if the program stops with an error."""
        self.runtime.error = None
        self.runtime.error_line = None
        with _CtrlC(self.out):
            code = call_with_big_stack(fn)
        if self.runtime.line:
            self.out.write(" ".join(self.runtime.line) + "\n")
            self.runtime.line = []
        if code != 0 or self.runtime.error:
            raise FermiumRuntimeError(self.runtime.error or "runtime error", self.runtime.error_line)


I, FermiumError  # re-exports
