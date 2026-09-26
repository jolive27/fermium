#!/usr/bin/env python3
"""End-to-end check of the Rust Jupyter kernel with the real Jupyter client (jupyter_client, libzmq):

    python3 rust/tools/jupyter_e2e.py [rust/target/fast/fermium]

Installs the kernel spec into a temporary prefix (`fermium jupyter install --prefix DIR`), starts the kernel
as Jupyter does, and replays tests/test_jupyter.py's checks (values carry over between cells, errors, Tab
completion, is_complete) plus a few more. Prints one line per check; exits with 1 if any fails.
"""
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def main():
    binary = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "rust/target/fast/fermium"))
    from jupyter_client.manager import start_new_kernel
    prefix = tempfile.mkdtemp(prefix="fm-jup-")
    r = subprocess.run([binary, "jupyter", "install", "--prefix", prefix], capture_output=True, text=True)
    print(r.stdout.strip())
    os.environ["JUPYTER_PATH"] = os.path.join(prefix, "share", "jupyter")
    cwd = tempfile.mkdtemp(prefix="fm-work-")
    km, kc = start_new_kernel(kernel_name="fermium", cwd=cwd, startup_timeout=30)
    fails = 0

    def check(name, cond, got=None):
        nonlocal fails
        print(("ok    " if cond else "FAIL  ") + name + ("" if cond else f"   got: {got!r}"))
        fails += 0 if cond else 1

    def execute(code):
        msg_id = kc.execute(code)
        outs = []
        while True:
            m = kc.get_iopub_msg(timeout=60)
            if m["parent_header"].get("msg_id") != msg_id:
                continue
            t = m["msg_type"]
            if t == "stream":
                outs.append((m["content"]["name"], m["content"]["text"]))
            elif t == "display_data":
                outs.append(("display", m["content"]["data"]))
            elif t == "error":
                outs.append(("error", m["content"]))
            elif t == "status" and m["content"]["execution_state"] == "idle":
                break
        return kc.get_shell_msg(timeout=60)["content"]["status"], outs

    try:
        info = kc.kernel_info()
        reply = kc.get_shell_msg(timeout=10) if isinstance(info, str) else info
        check("kernel_info: language fermium", reply["content"]["language_info"]["name"] == "fermium", reply)
        got = execute("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g")
        check("values carry over between cells (1)", got == ("ok", [("stdout", "9.70 m/s²\n")]), got)
        got = execute("print g in ft/s²")
        check("values carry over between cells (2)", got == ("ok", [("stdout", "31.8 ft/s²\n")]), got)
        status, outs = execute("y = L + T")
        check("errors are one-line physics", status == "error" and outs and outs[0][0] == "stderr"
              and "can't add length [m] to time [s]" in outs[0][1], (status, outs))
        got = execute("y = 2*L\nprint y")
        check("a failed cell leaves no names behind", got == ("ok", [("stdout", "2.40 m\n")]), got)
        got = execute("x = 10 m\nx\nz = 2 * (5 °C)\nprint z")
        print("      ", got)
        check("warnings go to stderr, before the output", got[0] == "ok" and len(got[1]) == 2
              and got[1][0][0] == "stderr" and got[1][1][0] == "stdout", got)
        got = execute("f(x) =\n    y = 2 x\n    y + 1 m\nprint f(3 m)")
        check("multi-line function in a cell", got == ("ok", [("stdout", "7 m\n")]), got)
        got = execute("")
        check("an empty cell", got == ("ok", []), got)
        got = execute("xs = [1, 2]\nprint xs[5]")
        check("run-time errors", got[0] == "error" and "out of range" in got[1][-1][1], got)
        kc.complete("x = \\ome", 8)
        reply = kc.get_shell_msg(timeout=10)["content"]
        check("backslash completion", reply["matches"] == ["ω"] and reply["cursor_start"] == 4, reply)
        kc.complete("print gg", 7)
        reply = kc.get_shell_msg(timeout=10)["content"]
        check("name completion", "g" in reply["matches"] and reply["cursor_start"] == 6, reply)
        kc.is_complete("if 1 > 0")
        reply = kc.get_shell_msg(timeout=10)["content"]
        check("is_complete: an if without its body", reply["status"] == "incomplete", reply)
        kc.is_complete("x = 1")
        reply = kc.get_shell_msg(timeout=10)["content"]
        check("is_complete: a whole line", reply["status"] == "complete", reply)
    finally:
        kc.stop_channels()
        km.shutdown_kernel(now=False)
    check("the kernel stops on shutdown", not km.is_alive())
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
