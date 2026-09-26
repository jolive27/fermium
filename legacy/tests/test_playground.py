"""The browser playground (web/): the build script, Fermium's WebAssembly module (Node), and the real page in
headless Chromium.

Since B5.12 the page runs the Rust compiler built to WebAssembly (rust/crates/fermium-wasm → web/gen/fermium.wasm)
instead of this Python implementation in Pyodide. Building the module needs Rust and its wasm32 target
(`python3 web/build.py`, a few minutes the first time), so the tests that need it skip when web/gen/fermium.wasm
isn't there; the Rust CI job builds it and runs web/test/compare_native.js (every example against `fermium run`)."""
import functools
import glob
import http.server
import json
import os
import re
import shutil
import socketserver
import subprocess
import sys
import threading

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
WEB = os.path.join(ROOT, "web")
GEN = os.path.join(WEB, "gen")
WASM = os.path.join(GEN, "fermium.wasm")


@pytest.fixture(scope="module")
def built():
    p = subprocess.run([sys.executable, os.path.join(WEB, "build.py"), "--no-wasm"], capture_output=True, text=True,
                       timeout=120)
    assert p.returncode == 0, p.stderr
    return json.load(open(os.path.join(GEN, "manifest.json")))


# ------------------------------------------------------------------ build (no browser, no Rust)
def test_build_exports_every_bootcamp_block_and_the_symbols(built):
    ex = json.load(open(os.path.join(GEN, "examples.json"), encoding="utf-8"))
    codes = [it["code"] for g in ex["groups"] for it in g["items"]]
    n_blocks = 0
    for f in glob.glob(os.path.join(ROOT, "bootcamp", "lesson*.md")):
        for m in re.finditer(r"```fermium\n(.*?)```", open(f, encoding="utf-8").read(), re.S):
            n_blocks += 1
            assert m.group(1).rstrip() + "\n" in codes, f
    assert n_blocks > 50
    files = {it.get("file") for g in ex["groups"] for it in g["items"]}
    assert {os.path.basename(f) for f in glob.glob(os.path.join(ROOT, "examples", "*.fm"))} <= files
    assert "bootcamp/data/pendulum.csv" in ex["data"]
    from fermium.symbols import LATEX
    assert json.load(open(os.path.join(GEN, "symbols.json"), encoding="utf-8")) == LATEX
    assert built["wasm"] == "fermium.wasm"
    assert not glob.glob(os.path.join(GEN, "*.whl"))      # the Pyodide wheel is gone


def test_the_page_no_longer_loads_pyodide():
    for f in ("index.html", "playground.js", "worker.js", "fermium.js"):
        assert "pyodide" not in open(os.path.join(WEB, f), encoding="utf-8").read().lower(), f


# ------------------------------------------------------------------ the WebAssembly module under Node
def _node():
    node = shutil.which("node")
    if not node:
        pytest.skip("node isn't installed")
    if not os.path.exists(WASM):
        pytest.skip("web/gen/fermium.wasm isn't built (python3 web/build.py)")
    return node


def run_wasm(tmp_path, code):
    node = _node()
    prog = tmp_path / "prog.fm"
    prog.write_text(code, encoding="utf-8")
    p = subprocess.run([node, os.path.join(WEB, "test", "run_wasm.js"), "run", str(prog)], capture_output=True,
                       text=True, timeout=120)
    return p.stdout, p.stderr, p.returncode


def test_wasm_runs_a_program(tmp_path):
    assert run_wasm(tmp_path, "print 4π² (1.20 m) / (2.21 s)²\n") == ("9.70 m/s²\n", "", 0)


def test_wasm_unit_error_in_one_line_form(tmp_path):
    out, err, code = run_wasm(tmp_path, "L = 1.20 m\nT = 2.21 s\nprint 4π² L / T + 9.8 m/s²\n")
    assert code == 1 and out == ""
    assert err.startswith("prog.fm, line 3: can't add speed [m/s] to acceleration [m/s²]\n    print 4π² L / T + 9.8 m/s²\n")
    assert "^^^" in err and "hint: both sides of + and - must have the same units" in err


def test_wasm_shows_run_time_warnings(tmp_path):
    out, err, code = run_wasm(tmp_path, "solve y'' = -(10/(1 s))^2 y with y(0) = 1 cm, y'(0) = 0 m/s "
                                        "for t from 0 s to 10 s step 0.1 s\nprint y(10 s)\n")
    assert (out, code) == ("0.252 cm\n", 0)
    assert err.startswith("warning: line 1: the step is too coarse")


def test_wasm_deep_recursion_is_a_clear_error(tmp_path):
    out, err, code = run_wasm(tmp_path, "f(n) = if n <= 0 then 0 else 1 + f(n - 1)\nprint f(100)\nprint f(1000000)\n")
    assert code == 1 and out == "100\n"
    assert "too deeply" in err or "called itself too many times" in err


# ------------------------------------------------------------------ the real page in headless Chromium
class _Quiet(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm"}

    def log_message(self, *args):
        pass


def _launch(p):
    errors = []
    for extra in ({}, {"executable_path": "/opt/pw-browsers/chromium"}):
        try:
            return p.chromium.launch(**extra)
        except Exception as e:        # browser binary not installed for this Playwright version
            errors.append(str(e).split("\n")[0])
    pytest.skip("no Chromium for Playwright: " + "; ".join(errors))


@pytest.fixture(scope="module")
def page(built):
    sync_api = pytest.importorskip("playwright.sync_api", reason="pip install playwright")
    if not os.path.exists(WASM):
        pytest.skip("web/gen/fermium.wasm isn't built (python3 web/build.py)")
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), functools.partial(_Quiet, directory=WEB))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    with sync_api.sync_playwright() as p:
        browser = _launch(p)
        pg = browser.new_page()
        pg.goto(f"http://127.0.0.1:{httpd.server_address[1]}/")
        pg.wait_for_function("['ready', 'fatal'].includes(document.body.dataset.state)", timeout=120_000)
        if pg.evaluate("document.body.dataset.state") == "fatal":
            text = pg.inner_text("#output")
            browser.close()
            httpd.shutdown()
            pytest.fail(text)
        assert "WebAssembly" in pg.inner_text("#status")
        yield pg
        browser.close()
    httpd.shutdown()


_runs = {"n": 0}


def run_in_page(pg, code=None, example=None, timeout=120_000):
    if example is not None:
        pg.select_option("#examples", label=example)
    if code is not None:
        pg.fill("#code", code)
    _runs["n"] += 1
    pg.press("#code", "Control+Enter")
    pg.wait_for_function(f"document.body.dataset.state === 'ready' && "
                         f"document.getElementById('output').dataset.done === '{_runs['n']}'", timeout=timeout)
    return pg.inner_text("#output")


def test_page_runs_a_program(page):
    assert run_in_page(page, "print 4π² (1.20 m) / (2.21 s)²\n").strip() == "9.70 m/s²"


def test_page_runs_the_welcome_example(page):
    out = run_in_page(page, example="Welcome: measure g with a pendulum")
    assert out.split("\n")[:2] == ["9.70 m/s²", "in feet: 31.8 ft/s²"]


def test_page_shows_unit_errors_in_one_line_form(page):
    out = run_in_page(page, example="A unit error")
    assert out.startswith("line 5: can't add speed [m/s] to acceleration [m/s²]")
    assert "hint: both sides of + and - must have the same units" in out
    assert page.locator("#output pre.error").count() == 1


def test_page_shows_run_time_warnings(page):
    run_in_page(page, "solve y'' = -(10/(1 s))^2 y with y(0) = 1 cm, y'(0) = 0 m/s "
                      "for t from 0 s to 10 s step 0.1 s\nprint y(10 s)\n")
    assert "too coarse" in page.inner_text("#output pre.warning")
    assert page.inner_text("#output pre.stdout") == "0.252 cm"


def test_page_lists_the_bootcamp_and_the_examples(page):
    groups = page.eval_on_selector_all("#examples optgroup", "gs => gs.map(g => g.label)")
    assert groups[0] == "Start here" and groups[-1] == "Example programs"
    assert sum(g.startswith("Lesson ") for g in groups) >= 10


def test_page_backslash_tab_completion(page):
    page.fill("#code", "")
    page.type("#code", "print 4\\pi")
    page.press("#code", "Tab")
    page.type("#code", "\\^2")
    page.press("#code", "Tab")
    page.type("#code", " (1.20 m) / (2.21 s)\\^2")
    page.press("#code", "Tab")
    assert page.input_value("#code") == "print 4π² (1.20 m) / (2.21 s)²"
    page.fill("#code", "x = \\ome")          # unique prefix: \\ome -> ω
    page.press("#code", "Tab")
    assert page.input_value("#code") == "x = ω"


def test_page_shows_plots(page):
    code = 'x = [1 s, 2 s, 3 s]\ny = [1 m, 4 m, 9 m]\nplot y vs x to "parabola.png"\n'
    out = run_in_page(page, code)
    if re.search(r"plot isn't supported by (this version of )?the Rust (back end|compiler) yet", out):
        pytest.skip("plot isn't in the Rust back end yet: " + out.split("\n")[0])
    assert "plot saved to " in out and "parabola.png" in out
    assert page.locator("#output img").count() == 1
    assert page.eval_on_selector("#output img", "img => img.naturalWidth") > 300


def test_page_stop_button_ends_an_endless_loop(page):
    page.fill("#code", "x = 1\nwhile x > 0\n    x += 1\n")
    _runs["n"] += 1
    page.click("#run")
    page.wait_for_function("document.body.dataset.state === 'running'")
    page.wait_for_timeout(500)
    page.click("#stop")
    page.wait_for_function("document.body.dataset.state === 'ready'", timeout=120_000)
    assert "stopped" in page.inner_text("#output")
    assert run_in_page(page, "print 2 + 3\n").strip() == "5"
