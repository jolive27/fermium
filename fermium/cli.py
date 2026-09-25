"""The `fermium` command."""
from __future__ import annotations

import argparse
import os
import sys
import traceback

from .errors import FermiumError


def _read(path):
    if os.path.isdir(path):
        sys.stderr.write(f"'{path}' is a folder, not a program file\n  hint: give the path of a .fm file inside it\n")
        sys.exit(2)
    try:
        with open(path, encoding="utf-8") as fh:
            return fh.read()
    except UnicodeDecodeError:
        sys.stderr.write(f"'{path}' isn't a text file Fermium can read (it must be saved as UTF-8 text)\n"
                         f"  hint: in your editor use 'Save as' with the UTF-8 encoding\n")
        sys.exit(2)
    except FileNotFoundError:
        sys.stderr.write(f"can't find the file '{path}'\n  hint: check the name, and that you're in the right "
                         f"folder (the command 'ls' lists the files here)\n")
        sys.exit(2)


def _internal(e):
    if isinstance(e, BrokenPipeError):          # `fermium run prog.fm | head`: the reader went away
        try:
            sys.stdout = open(os.devnull, "w")
        except OSError:
            pass
        return 0
    if os.environ.get("FERMIUM_DEBUG"):
        traceback.print_exc()
    sys.stderr.write(f"internal error in Fermium: {type(e).__name__}: {e}\n"
                     f"  (this is a bug in Fermium, not in your program; set FERMIUM_DEBUG=1 for details)\n")
    return 3


def cmd_run(args):
    from .driver import run_source, Program
    src = _read(args.file)
    try:
        if args.interp:
            from .interp import run_interpreted
            run_interpreted(src, args.file)
            return 0
        if args.emit_llvm:
            p = Program(src, args.file)
            print(p.llvm_ir)
            return 0
        p = run_source(src, args.file)
        if args.time:
            t = p.timings
            sys.stderr.write("time: parse {:.1f} ms, check {:.1f} ms, codegen {:.1f} ms, LLVM+JIT {:.1f} ms, "
                             "run {:.1f} ms\n".format(*(1000 * t.get(k, 0) for k in
                                                       ("parse", "check", "codegen", "llvm", "run"))))
        return 0
    except FermiumError as e:
        sys.stderr.write(e.format(src, os.path.basename(args.file)) + "\n")
        return 1
    except RecursionError:
        sys.stderr.write("this program is nested too deeply for Fermium to compile\n")
        return 1
    except Exception as e:
        return _internal(e)


def cmd_check(args):
    from .parser import parse
    from .checker import Checker
    from .errors import Diagnostics
    src = _read(args.file)
    d = Diagnostics()
    try:
        prog = parse(src, d)
        Checker(d, os.path.dirname(os.path.abspath(args.file))).check_program(prog)
    except FermiumError as e:
        sys.stderr.write(e.format(src, os.path.basename(args.file)) + "\n")
        return 1
    except Exception as e:
        return _internal(e)
    for w in d.warnings:
        sys.stderr.write(w.format(src) + "\n")
    print(f"{args.file}: no problems found (units check out)")
    return 0


def cmd_fmt(args):
    from .fmt import format_source
    from .errors import Diagnostics
    src = _read(args.file)
    mode = "ascii" if args.ascii else "pretty"
    d = Diagnostics()
    try:
        out = format_source(src, mode, d)
    except FermiumError as e:
        sys.stderr.write(e.format(src, os.path.basename(args.file)) + "\n")
        return 1
    for w in d.warnings:
        sys.stderr.write(w.format(src) + "\n")
    if args.write:
        with open(args.file, "w", encoding="utf-8") as fh:
            fh.write(out)
        print(f"rewrote {args.file} ({mode})")
    else:
        sys.stdout.write(out)
    return 0


def cmd_build(args):
    from .aot import build
    src = _read(args.file)
    out = args.output or os.path.splitext(os.path.basename(args.file))[0]
    try:
        build(src, args.file, out)
    except FermiumError as e:
        sys.stderr.write(e.format(src, os.path.basename(args.file)) + "\n")
        return 1
    except Exception as e:
        return _internal(e)
    print(f"built {out}  (run it with ./{out})" if not os.path.isabs(out) else f"built {out}")
    return 0


def cmd_doctor(args):
    from .doctor import doctor
    return doctor()


def cmd_jupyter(args):
    try:
        from .jupyter.kernel import install
    except ImportError:
        sys.stderr.write("the Jupyter kernel needs ipykernel: python3 -m pip install ipykernel jupyterlab\n")
        return 1
    where = install(user=not args.sys_prefix, prefix=sys.prefix if args.sys_prefix else None)
    print(f"installed the Fermium kernel in {where}\nstart Jupyter (jupyter lab) and pick 'Fermium' as the kernel")
    return 0


def cmd_lsp(args):
    try:
        from .lsp import serve
    except ImportError:
        sys.stderr.write("the language server needs pygls: python3 -m pip install pygls\n")
        return 1
    serve()
    return 0


def main(argv=None):
    from . import __version__
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0].endswith(".fm"):
        argv = ["run"] + argv
    p = argparse.ArgumentParser(prog="fermium", description="Fermium: a programming language for physicists.")
    p.add_argument("--version", action="version", version=f"fermium {__version__}")
    sub = p.add_subparsers(dest="cmd")
    r = sub.add_parser("run", help="run a .fm program")
    r.add_argument("file")
    r.add_argument("--time", action="store_true", help="show how long each stage took")
    r.add_argument("--emit-llvm", action="store_true", help="print the generated LLVM IR instead of running")
    r.add_argument("--interp", action="store_true",
                   help="run with the slow reference interpreter instead of compiling (for checking)")
    c = sub.add_parser("check", help="check a program's units without running it")
    c.add_argument("file")
    f = sub.add_parser("fmt", help="convert a program between ASCII and symbols")
    f.add_argument("file")
    g = f.add_mutually_exclusive_group()
    g.add_argument("--pretty", action="store_true", help="ASCII -> symbols (pi -> π, sqrt -> √, ^2 -> ²)")
    g.add_argument("--ascii", action="store_true", help="symbols -> ASCII")
    f.add_argument("-w", "--write", action="store_true", help="rewrite the file instead of printing")
    bld = sub.add_parser("build", help="compile a program into a standalone executable (needs a C compiler)")
    bld.add_argument("file")
    bld.add_argument("-o", "--output", help="name of the executable (default: the program's name)")
    sub.add_parser("doctor", help="check that Fermium is installed correctly")
    jup = sub.add_parser("jupyter", help="set up the Jupyter kernel:  fermium jupyter install")
    jup.add_argument("action", choices=["install"])
    jup.add_argument("--sys-prefix", action="store_true", help="install into this Python environment, not for the user")
    sub.add_parser("repl", help="start the interactive prompt (same as plain 'fermium')")
    sub.add_parser("lsp", help="run the language server (used by editors, speaks LSP on stdin/stdout)")
    args = p.parse_args(argv)
    if args.cmd is None or args.cmd == "repl":
        from .repl import main as repl_main
        return repl_main()
    return {"run": cmd_run, "check": cmd_check, "fmt": cmd_fmt, "doctor": cmd_doctor,
            "build": cmd_build, "jupyter": cmd_jupyter, "lsp": cmd_lsp}[args.cmd](args)


def entry():
    """Console-script entry point.  Exits with os._exit after flushing, because tearing down
    JIT engines during interpreter shutdown can crash (callback/engine destruction order)."""
    code = main()
    try:
        import atexit
        atexit._run_exitfuncs()          # e.g. save the REPL history (os._exit below skips atexit)
    except Exception:
        pass
    sys.stdout.flush()
    sys.stderr.flush()
    os._exit(code or 0)


if __name__ == "__main__":
    entry()
