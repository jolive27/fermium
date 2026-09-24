"""The interactive Fermium prompt (REPL) with history and \\name<TAB> symbol completion."""
from __future__ import annotations

import os
import sys

from .errors import FermiumError
from .symbols import complete, expand_all

BANNER = """Fermium {version} -- physics that reads like physics.  Type :help for help, :quit to leave.
Tip: type \\theta then press Tab to get θ (also \\hbar, \\int, \\sqrt, \\^2 ...)."""

HELP = """Examples:
    L = 1.20 m
    T = 2.21 s
    g = 4π² L / T²          (or ASCII: g = 4 pi^2 L / T^2)
    print g in ft/s²
    x(t) = 0.1 m cos(10 t / 1 s)
    print d/dt x
Commands:  :help   :quit   :vars
Symbols:   type \\name then Tab, e.g. \\omega -> ω, \\^2 -> ², \\int -> ∫"""

CONTINUE_MARKERS = ("expected an indented block", "ended before", "program ended", "the line ended")


def _setup_readline():
    try:
        import readline
    except ImportError:
        return None
    hist = os.path.join(os.path.expanduser("~"), ".fermium_history")
    try:
        readline.read_history_file(hist)
    except (OSError, IOError):
        pass
    import atexit

    def save():
        try:
            readline.write_history_file(hist)
        except OSError:
            pass
    atexit.register(save)
    readline.set_completer_delims(" \t\n()[]{},+-*/=<>|")

    def completer(text, state):
        opts = complete(text)
        return opts[state] if state < len(opts) else None
    readline.set_completer(completer)
    if "libedit" in (readline.__doc__ or ""):
        readline.parse_and_bind("bind ^I rl_complete")
    else:
        readline.parse_and_bind("tab: complete")
    return readline


def needs_more(text, session):
    """Is this input an unfinished block (e.g. an if without its body)?"""
    from .parser import parse
    from .errors import Diagnostics
    try:
        parse(text, Diagnostics(), known=session.known)
    except FermiumError as e:
        if any(m in e.message for m in CONTINUE_MARKERS):
            lines = text.rstrip("\n").split("\n")
            return e.line is None or e.line >= len(lines)
    return False


def main(stdin=None, stdout=None):
    from . import __version__
    from .driver import ReplSession
    stdin = stdin or sys.stdin
    out = stdout or sys.stdout
    interactive = stdin.isatty() if hasattr(stdin, "isatty") else False
    if interactive:
        _setup_readline()
        out.write(BANNER.format(version=__version__) + "\n")
    session = ReplSession(out=out)

    def read(prompt):
        if interactive:
            return input(prompt)
        line = stdin.readline()
        if not line:
            raise EOFError
        return line.rstrip("\n")

    while True:
        try:
            line = read("fm> ")
        except EOFError:
            if interactive:
                out.write("\n")
            return 0
        except KeyboardInterrupt:
            out.write("\n")
            continue
        line = expand_all(line)
        s = line.strip()
        if not s:
            continue
        if s in (":quit", ":q", "quit", "exit", ":exit"):
            return 0
        if s == ":help":
            out.write(HELP + "\n")
            continue
        if s == ":vars":
            names = sorted(k for k in session.checker.globals.names if not k.startswith("__"))
            out.write(", ".join(names) + "\n")
            continue
        text = line + "\n"
        while needs_more(text, session):
            try:
                more = read("... ")
            except EOFError:
                break
            if not more.strip():
                break
            text += expand_all(more) + "\n"
        try:
            session.execute(text)
        except FermiumError as e:
            out.write(e.format(text) + "\n")
        except Exception as e:  # never show a Python traceback for a user mistake
            if os.environ.get("FERMIUM_DEBUG"):
                raise
            out.write(f"internal error in Fermium: {type(e).__name__}: {e}\n"
                      f"  (this is a bug in Fermium, not in your program; set FERMIUM_DEBUG=1 for details)\n")
        out.flush()
