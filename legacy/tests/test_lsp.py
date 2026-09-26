"""The language server: diagnostics, hover with units, and completion (analysis + real LSP over stdio)."""
import json
import os
import subprocess
import sys

import pytest

from fermium.lsp import analyze, completions, hover_text

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

SRC = """L = 1.20 m
T = 2.21 s
g = 4π² L / T²
d = 15 cm
xs = [1 m, 2 m]
p(v) = (2 kg) v
solve x'' = -(9/s²) x with x(0) = 1 cm, x'(0) = 0 cm/s for t from 0 s to 5 s
y = L + T
"""


def test_error_is_a_problem_with_a_range():
    an = analyze(SRC)
    assert len(an.problems) == 1
    p = an.problems[0]
    assert (p.line, p.col, p.length, p.severity) == (8, 5, 5, "error")
    assert p.message == "can't add length [m] to time [s]"


def test_warnings_are_problems():
    an = analyze("c_w = 4186 J/(kg K)\nQ = c_w * 1 kg * 10 degC\n")
    assert [(p.line, p.severity) for p in an.problems] == [(2, "warning")]


def test_clean_program_has_no_problems():
    assert analyze("x = 3 m\nprint x\n").problems == []


@pytest.mark.parametrize("line,char,want", [
    (0, 0, "**L**: length [m]"),
    (2, 0, "**g**: acceleration [m/s²]"),
    (3, 0, "**d**: length [m], shown in cm"),
    (4, 1, "**xs**: a list of length [m]"),
    (2, 8, "**L**: length [m]"),          # a use, not only the definition
])
def test_hover_shows_units(line, char, want):
    assert hover_text(analyze(SRC), SRC, line, char) == want


def test_hover_function_solution_constant_unit():
    an = analyze(SRC)
    assert "p(v) = " in hover_text(an, SRC, 5, 0)
    assert hover_text(an, SRC, 6, 6).startswith("**x**: solution of an ODE, length [m]")
    src = "E = h * 1 Hz\ny = 3 km\n"
    an = analyze(src)
    assert hover_text(an, src, 0, 4).startswith("**h**: Planck constant")
    assert hover_text(an, src, 1, 7) == "**km**: unit of length [m], = 1000 m"


def test_backslash_completion():
    items = completions(analyze(""), "x = \\ome", 0, 8)
    assert items == [("\\omega", "ω", 4, "ω")]


def test_name_completion():
    src = SRC.replace("y = L + T\n", "") + "print gg\n"
    labels = [c[0] for c in completions(analyze(src), src, 7, 7)]
    assert "g" in labels


# ---------------------------------------------------------------- real server over stdio
pytest.importorskip("pygls")


class Client:
    def __init__(self):
        self.p = subprocess.Popen([sys.executable, "-m", "fermium.cli", "lsp"], cwd=ROOT, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
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
            line = self.p.stdout.readline().decode().strip()
            if not line:
                break
            k, v = line.split(":", 1)
            headers[k.lower()] = v.strip()
        return json.loads(self.p.stdout.read(int(headers["content-length"])))

    def wait(self, pred):
        for _ in range(50):
            m = self.read()
            if pred(m):
                return m
        raise AssertionError("no matching message")

    def close(self):
        self.send("shutdown", None)
        self.send("exit", None, notify=True)
        try:
            self.p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.p.kill()


def test_language_server_over_stdio(tmp_path):
    c = Client()
    try:
        rid = c.send("initialize", {"processId": None, "rootUri": None, "capabilities": {}})
        init = c.wait(lambda m: m.get("id") == rid)
        caps = init["result"]["capabilities"]
        assert caps["hoverProvider"] and "\\" in caps["completionProvider"]["triggerCharacters"]
        c.send("initialized", {}, notify=True)
        uri = (tmp_path / "a.fm").as_uri()
        c.send("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "fermium", "version": 1,
                                                         "text": SRC}}, notify=True)
        d = c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics")
        [diag] = d["params"]["diagnostics"]
        assert diag["range"] == {"start": {"line": 7, "character": 4}, "end": {"line": 7, "character": 9}}
        assert diag["message"].startswith("can't add length [m] to time [s]") and diag["severity"] == 1
        rid = c.send("textDocument/hover", {"textDocument": {"uri": uri}, "position": {"line": 3, "character": 0}})
        h = c.wait(lambda m: m.get("id") == rid)
        assert h["result"]["contents"]["value"] == "**d**: length [m], shown in cm"
        # fix the error: the underline goes away
        c.send("textDocument/didChange", {"textDocument": {"uri": uri, "version": 2},
                                          "contentChanges": [{"text": SRC.replace("L + T", "L + d")}]}, notify=True)
        d = c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics")
        assert d["params"]["diagnostics"] == []
        rid = c.send("textDocument/completion", {"textDocument": {"uri": uri},
                                                 "position": {"line": 0, "character": 0}})
        c.send("textDocument/didChange", {"textDocument": {"uri": uri, "version": 3},
                                          "contentChanges": [{"text": "x = \\thet"}]}, notify=True)
        rid = c.send("textDocument/completion", {"textDocument": {"uri": uri},
                                                 "position": {"line": 0, "character": 9}})
        comp = c.wait(lambda m: m.get("id") == rid)
        items = comp["result"]["items"]
        assert items[0]["label"] == "\\theta" and items[0]["textEdit"]["newText"] == "θ"
        assert items[0]["textEdit"]["range"]["start"] == {"line": 0, "character": 4}
    finally:
        c.close()


# ---- the A1 quick fix (D235): the same edit as `fermium fmt --fix` ---------------------------------------------

def test_quick_fix_for_a_unit_variable_collision():
    from fermium.lsp import quick_fixes
    src = "g = 9.81 m/s²\nprint 20 m/s/g\n"
    an = analyze(src)
    [(title, p, edits)] = quick_fixes(an, src)
    assert p.severity == "error" and "ambiguous" in p.message
    assert edits == [(1, 9, 1, 14, "[m/s/g]")] and "[m/s/g]" in title
    src = "m = 0.5 kg\nx = 0.1 m\n"
    [(_, _, edits)] = quick_fixes(analyze(src), src)
    assert edits == [(1, 8, 1, 9, "[m]")]


def test_quick_fix_over_stdio(tmp_path):
    c = Client()
    try:
        rid = c.send("initialize", {"processId": None, "rootUri": None, "capabilities": {}})
        init = c.wait(lambda m: m.get("id") == rid)
        assert init["result"]["capabilities"]["codeActionProvider"]
        c.send("initialized", {}, notify=True)
        uri = (tmp_path / "b.fm").as_uri()
        c.send("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "fermium", "version": 1,
                                                         "text": "m = 2 kg\nx = 3 m\n"}}, notify=True)
        c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics")
        rid = c.send("textDocument/codeAction", {"textDocument": {"uri": uri},
                                                 "range": {"start": {"line": 1, "character": 0},
                                                           "end": {"line": 1, "character": 7}},
                                                 "context": {"diagnostics": []}})
        r = c.wait(lambda m: m.get("id") == rid)
        [act] = r["result"]
        assert act["kind"] == "quickfix"
        [te] = act["edit"]["changes"][uri]
        assert te["newText"] == "[m]" and te["range"]["start"] == {"line": 1, "character": 6}
    finally:
        c.close()
