"""`fermium doctor`: check the installation and explain fixes in plain English."""
from __future__ import annotations

import io
import platform
import sys


def _ok(msg):
    print(f"  ✓ {msg}")


def _bad(msg, fix):
    print(f"  ✗ {msg}\n      fix: {fix}")


def doctor():
    print("Checking your Fermium installation...\n")
    problems = 0
    v = sys.version_info
    if v >= (3, 10):
        _ok(f"Python {v.major}.{v.minor}.{v.micro} ({platform.system()} {platform.machine()})")
    else:
        problems += 1
        _bad(f"Python {v.major}.{v.minor} is too old (Fermium needs 3.10 or newer)",
             "install a newer Python from https://www.python.org/downloads/ and reinstall Fermium with it")
    try:
        import llvmlite
        import llvmlite.binding as llvm
        _ok(f"llvmlite {llvmlite.__version__} (LLVM {'.'.join(map(str, llvm.llvm_version_info))})")
    except ImportError:
        problems += 1
        _bad("llvmlite (the compiler back end) is missing", "run:  pip install llvmlite")
    try:
        import numpy
        _ok(f"numpy {numpy.__version__}")
    except ImportError:
        problems += 1
        _bad("numpy is missing", "run:  pip install numpy")
    for mod, why in (("scipy", "needed for fit"), ("sympy", "needed for integrals without limits"),
                     ("matplotlib", "needed for plot")):
        try:
            m = __import__(mod)
            _ok(f"{mod} {m.__version__}")
        except ImportError:
            _bad(f"{mod} is not installed ({why}; everything else works)", f"run:  pip install {mod}")
    # the editor and notebook tools: optional, each needs one package (red team 5 #16)
    for mod, what in (("pygls", "the language server, fermium lsp (VS Code hover and live errors)"),
                      ("ipykernel", "the Jupyter kernel (fermium jupyter install)")):
        try:
            __import__(mod)
            try:
                from importlib.metadata import version
                ver = " " + version(mod)
            except Exception:
                ver = ""
            _ok(f"{mod}{ver}: for {what}")
        except ImportError:
            print(f"  - {mod} is not installed: only needed for {what}\n      to get it: pip install {mod}")
    # a C compiler links `fermium build` executables; nothing else needs one
    try:
        from . import aot
        cc = aot.find_cc()
    except ImportError:
        cc = None
    if cc:
        _ok(f"C compiler: {cc} (only needed for  fermium build)")
    else:
        mac = platform.system() == "Darwin"
        print("  - no C compiler found. That's fine: it's only needed for  fermium build  (standalone "
              "executables);\n      run, the REPL, fmt and check work without it.\n"
              f"      to get one: {'xcode-select --install' if mac else 'install clang or gcc'}"
              f"{'' if mac else ' (on a Mac: xcode-select --install)'}")
    # compile and run a tiny program end to end
    try:
        from .driver import run_source
        out = io.StringIO()
        run_source("L = 1.20 m\nT = 2.21 s\nprint 4π² L / T²\n", "<doctor>", out=out)
        got = out.getvalue().strip()
        if got == "9.70 m/s²":
            _ok(f"compiled and ran a test program: g = {got}")
        else:
            problems += 1
            _bad(f"the test program printed {got!r} instead of '9.70 m/s²'", "please report this as a bug")
    except Exception as e:
        problems += 1
        _bad(f"couldn't compile a test program ({type(e).__name__}: {e})",
             "reinstall with:  pip install --force-reinstall llvmlite  then run fermium doctor again")
    print()
    if problems:
        print(f"{problems} problem{'s' if problems > 1 else ''} found. Fix them and run  fermium doctor  again.")
        return 1
    print("Everything looks good! Try:  fermium   (then type  print 2 m + 30 cm )")
    return 0
