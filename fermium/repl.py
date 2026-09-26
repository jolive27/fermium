"""The interactive Fermium prompt (REPL) with history and \\name<TAB> symbol completion."""
from __future__ import annotations

import os
import re
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

CONTINUE_MARKERS = ("expected an indented block", "ended before", "program ended", "the line ended",
                    "solve needs a range")


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
        if "solve needs a range" in e.message:
            return True
        if any(m in e.message for m in CONTINUE_MARKERS):
            lines = text.rstrip("\n").split("\n")
            return e.line is None or e.line >= len(lines)
    return False


def opens_block(line):
    """Does this line start an indented block (if/for/while/else/solve, or `f(x) =` with nothing after)?"""
    import re
    st = line.strip()
    if re.match(r"(if|for|while|else|elif|solve)\b", st):
        return True
    return bool(re.match(r"[^\s=]+\([^)]*\)\s*=\s*(#.*)?$", st))


def describe_vars(session):
    """One line per name: numbers show their value and unit, other values say what they are in physics words
    (a complex number of …, a 2-D vector of …), and modules and functions are listed too (red team 5 #11)."""
    from . import ir as I
    from .checker import FuncInfo, SolView
    from .importer import ModuleRef
    from .lsp import _type_text
    from .pyinterop import PyModRef
    from .runtime.core import format_quantity
    from .types import NumTy
    ck = session.checker
    lines = []
    for name, b in sorted(ck.globals.names.items()):
        if name.startswith("__") or "'" in name or "_∂" in name:
            continue
        if isinstance(b, I.Sym) and isinstance(b.ty, NumTy) and b.slot is not None:
            dim = ck.U.resolve(b.ty.dim)
            v = session.arena[b.slot]
            lines.append(f"{name} = {format_quantity(v, dim, b.hint, b.sf, b.direct)}")
        elif isinstance(b, I.Sym):
            lines.append(f"{name}: {_type_text(ck, b.ty, getattr(b, 'hint', None))}")
        elif isinstance(b, FuncInfo):
            params = ", ".join(p.name for p in b.fdef.params) if b.fdef is not None else "..."
            lines.append(f"{name}({params}): function")
        elif isinstance(b, SolView):
            lines.append(f"{name}: solution of an ODE")
        elif isinstance(b, ModuleRef):
            lines.append(f"{name}: the module {b.info.name}")
        elif isinstance(b, PyModRef):
            lines.append(f"{name}: the Python module {b.module}")
    return "\n".join(lines) if lines else "(no variables yet)"


SHELL_LINE = re.compile(r"^(fermium|python3?|pip3?|cd|ls|dir|cat|open|clear|git|brew|sudo|mkdir|rm|cp|mv)"
                        r"(\s+[-\w./~\\\"\'=:]+)*$")


def session_names(session):
    """The names the session has defined (a variable called `cd` or `ls` is not a terminal command)."""
    try:
        return set(session.checker.globals.names)
    except Exception:
        return set()


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

    pending = []

    def read(prompt):
        if pending:
            return pending.pop()
        if interactive:
            return input(prompt)
        line = stdin.readline()
        if not line:
            raise EOFError
        return line.rstrip("\n")

    def peek():
        try:
            line = read("... ")
        except EOFError:
            return None
        pending.append(line)
        return line

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
            out.write(describe_vars(session) + "\n")
            continue
        if SHELL_LINE.match(s) and not re.match(r"^\S+\s*[-+*/^]?=", s) and s.split()[0] not in session_names(session):
            # `fm> fermium run ke.fm`: a terminal command typed at the Fermium prompt (spec A3.5)
            out.write(f"'{s.split()[0]}' looks like a terminal command. This is the Fermium prompt; type :quit to go "
                      f"back to the terminal first.\n")
            out.flush()
            continue
        text = line + "\n"
        block = opens_block(line) or needs_more(text, session)
        while block:
            if interactive:
                try:
                    more = read("... ")
                except EOFError:
                    break
                if not more.strip():
                    break            # an empty line ends the block (like Python)
                text += expand_all(more) + "\n"
                continue
            # reading a file/pipe: continue while lines are indented, or are 'else', or the input is unfinished
            nxt = peek()
            if nxt is None:
                break
            st = nxt.strip()
            if nxt[:1] in (" ", "\t") or st.startswith(("else", "elif")) or needs_more(text, session):
                pending.pop()
                if st:
                    text += expand_all(nxt) + "\n"
                continue
            break
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
