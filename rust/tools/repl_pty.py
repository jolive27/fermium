#!/usr/bin/env python3
"""Drive the Rust REPL through a pseudo-terminal, as a person at a keyboard would, and check the line editor:
the banner, \\name + Tab, arrow keys, history (Up, and ~/.fermium_history across sessions), Ctrl-C, a block
ended by a blank line, and Ctrl-D.

    python3 rust/tools/repl_pty.py [rust/target/fast/fermium]
"""
import os
import pty
import select
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def session(binary, home, keys):
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["HOME"] = home
        os.execv(binary, [binary])
    out = b""

    def drain(t=0.3):
        nonlocal out
        end = time.time() + t
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                try:
                    out += os.read(fd, 65536)
                except OSError:
                    return
    drain(1.0)
    for k in keys:
        os.write(fd, k)
        drain()
    drain(0.5)
    _, status = os.waitpid(pid, 0)
    return out.decode("utf-8", "replace"), os.waitstatus_to_exitcode(status)


def main():
    binary = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "rust/target/fast/fermium"))
    home = tempfile.mkdtemp(prefix="fm-home-")
    fails = 0

    def check(name, cond, got=""):
        nonlocal fails
        print(("ok    " if cond else "FAIL  ") + name + ("" if cond else f"\n      got: {got!r}"))
        fails += 0 if cond else 1

    out, code = session(binary, home, [
        b"\\ome\t = 3\r",            # Tab completes \ome to ω
        b"print \\ome\t\r",
        b"x = 2 m\r",
        b"rint yy\x7f\x7fx" + b"\x1b[D" * 6 + b"p\r",   # Backspace twice, x, Left to the start, p: print x
        b"\x1b[A\r",                  # Up: the last line again
        b"for i from 1 to 2\r", b"    print i\r", b"\r",   # a block, ended by a blank line
        b"print 99\x03",              # Ctrl-C drops the line
        b"\x04",                      # Ctrl-D leaves
    ])
    check("banner", "Fermium" in out and "Type :help" in out, out)
    check("\\name + Tab", "ω = 3" in out and "\r\n3\r\n" in out, out)
    check("arrow keys and backspace", out.count("\r\n2 m\r\n") == 2, out)
    check("a block ends with a blank line", "\r\n1\r\n2\r\n" in out, out)
    check("Ctrl-C drops the line", "99\r\n" not in out.replace("print 99", ""), out)
    check("Ctrl-D exits with 0", code == 0, code)
    hist = open(os.path.join(home, ".fermium_history"), encoding="utf-8").read().splitlines()
    check("history saved", hist[:3] == ["ω = 3", "print ω", "x = 2 m"], hist)
    out, code = session(binary, home, [b"\x1b[A" * (len(hist) - 2) + b"\r", b"\x04"])  # Up to "x = 2 m"
    check("history comes back in the next session", "fm> x = 2 m\x1b[K\r\r\n" in out, out)
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
