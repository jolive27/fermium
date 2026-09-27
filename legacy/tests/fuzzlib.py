"""Shared helpers for fuzzing: mutate real programs and check the compiler only ever
raises FermiumError (never a Python exception)."""
import glob
import os
import random
import re

from fermium.errors import FermiumError, Diagnostics
from fermium.parser import parse
from fermium.checker import Checker
from fermium.codegen_llvm import ModuleGen
from fermium.driver import finalize_tables

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

SNIPPETS = ["m", "s", "kg", "1", "0", "2.5", "π", "√", "∫", "d/dt", "'", "''", "^", "²", "(", ")", "[", "]", ",",
            "=", "==", "+", "-", "*", "/", " ", "\n", "\n    ", "x", "t", "f(x)", "dx", "from", "to", "in",
            "where", "if", "else", "for", "while", "solve", "with", "fit", "plot", "vs", "print", "load", "±",
            "°C", "eV", "e", "[m]", "|", "∞", "1e308", "end", "step", "#", '"', "µ", "ν", "½", "×10⁻³", "∂"]


def corpus():
    progs = []
    for f in glob.glob(os.path.join(ROOT, "docs", "*.md")) + glob.glob(os.path.join(ROOT, "bootcamp", "*.md")):
        progs += re.findall(r"```fermium\n(.*?)```", open(f, encoding="utf-8").read(), re.S)
    for f in glob.glob(os.path.join(ROOT, "examples", "*.fm")) + glob.glob(os.path.join(ROOT, "legacy", "tests", "programs",
                                                                                         "*.fm")):
        progs.append(open(f, encoding="utf-8").read())
    return [p for p in progs if p.strip()]


def mutate(src, rng):
    ops = rng.randint(1, 4)
    for _ in range(ops):
        k = rng.random()
        if not src:
            src = rng.choice(SNIPPETS)
            continue
        i = rng.randrange(len(src))
        if k < 0.3:
            j = min(len(src), i + rng.randint(1, 8))
            src = src[:i] + src[j:]                       # delete
        elif k < 0.6:
            src = src[:i] + rng.choice(SNIPPETS) + src[i:]  # insert
        elif k < 0.8:
            j = min(len(src), i + rng.randint(1, 20))
            src = src[:i] + src[i:j] + src[i:]            # duplicate
        else:
            j = rng.randrange(len(src))
            a, b = sorted((i, j))
            src = src[:a] + src[b:] + src[a:b]            # rotate
    return src


def compile_only(src, base_dir):
    """Parse, check and generate LLVM IR (no running: mutated loops could run forever)."""
    d = Diagnostics()
    prog = parse(src, d)
    ck = Checker(d, base_dir)
    mod = ck.check_program(prog)
    finalize_tables(mod.tables, ck.U)
    if mod.uses_unc:          # uncertainties (±) run in the interpreter, never in LLVM (D122)
        return ""
    mg = ModuleGen()
    mg.emit_main(mod.main, "fm_run")
    text = str(mg.module)
    import llvmlite.binding as llvm
    from fermium.runtime.core import init_llvm
    init_llvm()
    llvm.parse_assembly(text).verify()      # generated IR must always be valid
    return text


def fuzz(n, seed, verbose=False):
    """Return a list of (program, exception) for every non-FermiumError failure."""
    rng = random.Random(seed)
    progs = corpus()
    bad = []
    for _ in range(n):
        src = mutate(rng.choice(progs), rng)
        try:
            compile_only(src, os.path.join(ROOT, "examples"))
        except FermiumError:
            pass
        except RecursionError:
            pass
        except Exception as e:  # noqa: BLE001 -- that's the point
            bad.append((src, e))
            if verbose:
                print("----\n", src, "\n=>", type(e).__name__, e)
    return bad
