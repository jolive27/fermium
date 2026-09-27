#!/usr/bin/env python3
"""Play scripted sessions against a language server and print every message it sends back, one JSON per line
(keys sorted), so two servers can be compared:

    python3 rust/tools/lsp_session.py v1                       # python3 -m fermium lsp (Fermium 1.5, pygls)
    python3 rust/tools/lsp_session.py rust/target/fast/fermium # the Rust server
    python3 rust/tools/lsp_session.py --write-fixtures         # sessions + v1's replies for cargo test

The initialize reply is left out (pygls advertises more capabilities than v1 uses); every other reply and
notification is compared exactly.
"""
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIX = os.path.join(ROOT, "rust", "crates", "fermium-lsp", "tests", "sessions")

SRC = """L = 1.20 m
T = 2.21 s
g = 4π² L / T²
d = 15 cm
xs = [1 m, 2 m]
p(v) = (2 kg) v
solve x'' = -(9/s²) x with x(0) = 1 cm, x'(0) = 0 cm/s for t from 0 s to 5 s
y = L + T
"""
URI = "file:///tmp/fermium-lsp-test/a.fm"


def pos(line, ch):
    return {"line": line, "character": ch}


def doc(uri=URI):
    return {"uri": uri}


def sessions():
    """Each session: a list of (method, params, is_notification)."""
    s = []
    # the test_lsp.py session: diagnostics, hover, a fix, completion
    s.append([
        ("textDocument/didOpen", {"textDocument": {"uri": URI, "languageId": "fermium", "version": 1, "text": SRC}}, 1),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(3, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(0, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(2, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(4, 1)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(2, 8)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(5, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(6, 6)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(6, 60)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(7, 2)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(7, 30)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 2},
                                    "contentChanges": [{"text": SRC.replace("L + T", "L + d")}]}, 1),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(0, 0)}, 0),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(7, 5)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 3},
                                    "contentChanges": [{"text": "x = \\thet"}]}, 1),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(0, 9)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 4},
                                    "contentChanges": [{"text": "x = \\^"}]}, 1),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(0, 6)}, 0),
    ])
    # the quick fix
    s.append([
        ("textDocument/didOpen", {"textDocument": {"uri": URI, "languageId": "fermium", "version": 1,
                                                   "text": "m = 2 kg\nx = 3 m\n"}}, 1),
        ("textDocument/codeAction", {"textDocument": doc(), "range": {"start": pos(1, 0), "end": pos(1, 7)},
                                     "context": {"diagnostics": []}}, 0),
        ("textDocument/codeAction", {"textDocument": doc(), "range": {"start": pos(0, 0), "end": pos(0, 7)},
                                     "context": {"diagnostics": []}}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 2},
                                    "contentChanges": [{"text": "g = 9.81 m/s²\nprint 20 m/s/g\n"}]}, 1),
        ("textDocument/codeAction", {"textDocument": doc(), "range": {"start": pos(1, 0), "end": pos(1, 3)},
                                     "context": {"diagnostics": []}}, 0),
    ])
    # warnings, constants, units, keywords, astral characters (UTF-16 columns), incremental edits
    s.append([
        ("textDocument/didOpen", {"textDocument": {"uri": URI, "languageId": "fermium", "version": 1,
                                                   "text": "c_w = 4186 J/(kg K)\nQ = c_w * 1 kg * 10 degC\n"
                                                           "E = h * 1 Hz\ny = 3 km\nz = 2𝑖 + 3\nw = z + 1 m\n"}}, 1),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(2, 4)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(3, 7)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(3, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(1, 17)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(0, 7)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 2},
                                    "contentChanges": [{"range": {"start": pos(5, 8), "end": pos(5, 11)},
                                                        "text": "1"}]}, 1),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(5, 0)}, 0),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(3, 1)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 3},
                                    "contentChanges": [{"range": {"start": pos(6, 0), "end": pos(6, 0)},
                                                        "text": "whi"}]}, 1),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(6, 3)}, 0),
        ("textDocument/didSave", {"textDocument": doc()}, 1),
    ])
    # a parse error, and functions, vectors, modules
    s.append([
        ("textDocument/didOpen", {"textDocument": {"uri": URI, "languageId": "fermium", "version": 1,
                                                   "text": "import mechanics\nv = [1, 2, 3] m/s\n"
                                                           "f(x) = 3 x² / 1 s\nk = mechanics.\nq = (1 +\n"}}, 1),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(1, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(2, 0)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(0, 9)}, 0),
        ("textDocument/completion", {"textDocument": doc(), "position": pos(3, 14)}, 0),
        ("textDocument/didChange", {"textDocument": {"uri": URI, "version": 2},
                                    "contentChanges": [{"text": "import mechanics\nT = mechanics.pendulum_period(1 m)\n"
                                                                "print T\n"}]}, 1),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(1, 16)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(1, 5)}, 0),
        ("textDocument/hover", {"textDocument": doc(), "position": pos(1, 0)}, 0),
    ])
    return s


class Client:
    def __init__(self, cmd):
        self.p = subprocess.Popen(cmd, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL)
        self.id = 0

    def send(self, method, params, notify=False):
        msg = {"jsonrpc": "2.0", "method": method, "params": params}
        if not notify:
            self.id += 1
            msg["id"] = self.id
        body = json.dumps(msg).encode()
        self.p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.p.stdin.flush()
        return None if notify else self.id

    def read(self):
        headers = {}
        while True:
            line = self.p.stdout.readline().decode()
            if not line:
                raise EOFError
            line = line.strip()
            if not line:
                break
            k, v = line.split(":", 1)
            headers[k.lower()] = v.strip()
        return json.loads(self.p.stdout.read(int(headers["content-length"])))


def play(cmd, session):
    """The messages the server sends for one session (after initialize), as JSON lines."""
    c = Client(cmd)
    out = []
    try:
        rid = c.send("initialize", {"processId": None, "rootUri": None, "capabilities": {}})
        while c.read().get("id") != rid:
            pass
        c.send("initialized", {}, notify=True)
        for method, params, notify in session:
            if notify:
                c.send(method, params, notify=True)
                if method in ("textDocument/didOpen", "textDocument/didChange", "textDocument/didSave"):
                    out.append(c.read())
            else:
                rid = c.send(method, params)
                while True:
                    m = c.read()
                    out.append(m)
                    if m.get("id") == rid:
                        break
        rid = c.send("shutdown", None)
        while True:
            m = c.read()
            out.append(m)
            if m.get("id") == rid:
                break
        c.send("exit", None, notify=True)
        out.append({"exit code": c.p.wait(timeout=10)})
    finally:
        if c.p.poll() is None:
            c.p.kill()
    return [json.dumps(m, sort_keys=True, ensure_ascii=False) for m in out]


def main():
    arg = sys.argv[1] if len(sys.argv) > 1 else "v1"
    if arg == "--write-fixtures":
        os.makedirs(FIX, exist_ok=True)
        for i, s in enumerate(sessions()):
            with open(os.path.join(FIX, f"{i:03d}.in.json"), "w", encoding="utf-8") as fh:
                json.dump([[m, p, bool(n)] for m, p, n in s], fh, ensure_ascii=False, indent=0)
            with open(os.path.join(FIX, f"{i:03d}.out.jsonl"), "w", encoding="utf-8") as fh:
                fh.write("\n".join(play([sys.executable, "-m", "fermium", "lsp"], s)) + "\n")
        print(f"wrote {len(sessions())} sessions to {os.path.relpath(FIX, ROOT)}")
        return
    cmd = [sys.executable, "-m", "fermium", "lsp"] if arg == "v1" else [os.path.abspath(arg), "lsp"]
    for i, s in enumerate(sessions()):
        print(f"# session {i}")
        print("\n".join(play(cmd, s)))


if __name__ == "__main__":
    main()
