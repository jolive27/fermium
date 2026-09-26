"""The browser playground (web/): the build script, the Python glue, and the real page in headless
Chromium.

The browser tests serve web/ on a local port and drive the page with Playwright.  Pyodide comes from
web/pyodide/ (downloaded once by `python3 web/build.py --local-pyodide`, which the fixture runs if it
is missing); if that download fails they try the jsdelivr CDN, and if the browser can't reach that
either they skip with the reason."""
import functools
import glob
import http.server
import json
import os
import re
import socketserver
import subprocess
import sys
import threading
import zipfile

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEB = os.path.join(ROOT, "web")
GEN = os.path.join(WEB, "gen")
sys.path.insert(0, WEB)


@pytest.fixture(scope="module")
def built():
    p = subprocess.run([sys.executable, os.path.join(WEB, "build.py")], capture_output=True, text=True, timeout=120)
    assert p.returncode == 0, p.stderr
    return json.load(open(os.path.join(GEN, "manifest.json")))


# ------------------------------------------------------------------ build + glue (no browser)
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


def test_wheel_is_a_valid_pure_python_wheel(built):
    whl = os.path.join(GEN, built["wheel"])
    with zipfile.ZipFile(whl) as z:
        names = set(z.namelist())
        assert {"fermium/interp.py", "fermium/numerics.py", "fermium/tables.py", "fermium/runtime/core.py"} <= names
        dist = [n for n in names if n.endswith(".dist-info/RECORD")][0]
        record = z.read(dist).decode().split()
        assert len(record) == len(names)
        assert b"Root-Is-Purelib: true" in z.read(dist.replace("RECORD", "WHEEL"))


GLUE = """
import sys, json
for m in ("llvmlite", "llvmlite.ir", "llvmlite.binding"):
    sys.modules[m] = None                    # as in Pyodide
sys.path[:0] = [ROOT, WEB]
import playground
out = {}
out["ok"] = json.loads(playground.run("print 4π² (1.20 m) / (2.21 s)²", BASE))
out["err"] = json.loads(playground.run("L = 1.20 m\\nT = 2.21 s\\nprint 4π² L / T + 9.8 m/s²", BASE))
out["plot"] = json.loads(playground.run('x = [1 s, 2 s, 3 s]\\ny = [1 m, 4 m, 9 m]\\nplot y vs x to "p.png"', BASE))
out["needs"] = playground.packages_needed('plot y vs x to "a.png"\\nfit y = a x')
print(json.dumps(out))
"""


def test_glue_runs_programs_without_llvmlite(tmp_path):
    pytest.importorskip("matplotlib")
    code = f"ROOT = {ROOT!r}\nWEB = {WEB!r}\nBASE = {str(tmp_path)!r}\n" + GLUE
    p = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=120)
    assert p.returncode == 0, p.stderr
    out = json.loads(p.stdout.strip().split("\n")[-1])
    assert out["ok"]["stdout"] == "9.70 m/s²\n" and out["ok"]["error"] is None
    assert out["err"]["error"].startswith("line 3: can't add speed [m/s] to acceleration [m/s²]")
    assert "hint:" in out["err"]["error"]
    assert out["plot"]["error"] is None and len(out["plot"]["plots"]) == 1
    assert out["plot"]["plots"][0]["name"] == "p.png" and len(out["plot"]["plots"][0]["png"]) > 1000
    assert out["needs"] == ["matplotlib", "scipy"]


# ------------------------------------------------------------------ the real page in headless Chromium
class _Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def _launch(p):
    kw = {}
    proxy = os.environ.get("HTTPS_PROXY") or os.environ.get("https_proxy")
    if proxy:
        kw["proxy"] = {"server": proxy, "bypass": "127.0.0.1,localhost"}
    errors = []
    for extra in ({}, {"executable_path": "/opt/pw-browsers/chromium"}):
        try:
            return p.chromium.launch(**kw, **extra)
        except Exception as e:        # browser binary not installed for this Playwright version
            errors.append(str(e).split("\n")[0])
    pytest.skip("no Chromium for Playwright: " + "; ".join(errors))


@pytest.fixture(scope="module")
def page(built):
    sync_api = pytest.importorskip("playwright.sync_api", reason="pip install playwright")
    mode = "local"
    import build
    if not os.path.exists(os.path.join(WEB, "pyodide", build.LOCAL_MARKER)):
        try:
            build.local_pyodide()
        except Exception:             # no network for Python: maybe the browser can reach the CDN
            mode = "cdn"
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), functools.partial(_Quiet, directory=WEB))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    with sync_api.sync_playwright() as p:
        browser = _launch(p)
        pg = browser.new_page()
        pg.goto(f"http://127.0.0.1:{httpd.server_address[1]}/?pyodide={mode}")
        pg.wait_for_function("['ready', 'fatal'].includes(document.body.dataset.state)", timeout=240_000)
        if pg.evaluate("document.body.dataset.state") == "fatal":
            text = pg.inner_text("#output")
            browser.close()
            httpd.shutdown()
            if mode == "cdn":
                pytest.skip("the headless browser can't load Pyodide from the CDN: " + text.split("\n")[-1])
            pytest.fail(text)
        assert pg.evaluate("document.body.dataset.pyodide") == mode
        yield pg
        browser.close()
    httpd.shutdown()


_runs = {"n": 0}


def run_in_page(pg, code=None, example=None, timeout=240_000):
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


def test_page_shows_unit_errors_in_one_line_form(page):
    out = run_in_page(page, example="A unit error")
    assert out.startswith("line 5: can't add speed [m/s] to acceleration [m/s²]")
    assert "hint: both sides of + and - must have the same units" in out
    assert page.locator("#output pre.error").count() == 1


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
    assert "plot saved to " in out and "parabola.png" in out
    assert page.locator("#output img").count() == 1
    assert page.eval_on_selector("#output img", "img => img.naturalWidth") > 300


def test_page_integral_without_limits_loads_sympy_on_demand(page):
    out = run_in_page(page, "F = integral cos(x) dx\nprint F(pi / 2)\n")
    assert out.strip() == "1"


def test_page_stop_button_ends_an_endless_loop(page):
    page.fill("#code", "x = 1\nwhile x > 0\n    x += 1\n")
    _runs["n"] += 1
    page.click("#run")
    page.wait_for_function("document.body.dataset.state === 'running'")
    page.wait_for_timeout(500)
    page.click("#stop")
    page.wait_for_function("document.body.dataset.state === 'ready'", timeout=240_000)
    assert "stopped" in page.inner_text("#output")
    assert run_in_page(page, "print 2 + 3\n").strip() == "5"
