"""Findings of the independent red-team review, round 5 (REDTEAM.md, "Round 5 (08:45 UTC)"): tooling and the
beginner journey (REPL, Jupyter kernel, language server, fmt, build, playground, check, bootcamp lessons 0-3).

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
"""
import io
import json
import os
import re
import subprocess
import sys

import pytest

from conftest import run, error_of
from fermium.driver import run_source

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def rt5(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 5 #{n}")


def repl(text):
    from fermium.repl import main
    out = io.StringIO()
    main(stdin=io.StringIO(text), stdout=out)
    return out.getvalue()


def run_err(src):
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    return out.getvalue().strip(), err.getvalue()


# ---- #1: a failed REPL input leaves its new variables half-defined: every later use is an internal error ------

def test_1_repl_failed_input_does_not_poison_its_variables():
    # the ± refusal (documented), then the same name used again
    out = repl("L = 1.20 +- 0.01 m\nL = 3 m\nprint L\n")
    assert "internal error" not in out
    assert "3 m" in out


def test_1_repl_compile_error_in_a_block_does_not_poison_its_variables():
    out = repl("if 1 > 0\n    ww = 1 m\n    ww2 = ww + 1 s\n\nprint ww\nww = 2 m\nprint ww\n")
    assert "can't add length [m] to time [s]" in out
    assert "internal error" not in out


# ---- #2 and #3: the Jupyter kernel --------------------------------------------------------------------------

@pytest.fixture(scope="module")
def kernel(tmp_path_factory):
    pytest.importorskip("ipykernel")
    pytest.importorskip("jupyter_client")
    from fermium.jupyter.kernel import install
    from jupyter_client.manager import start_new_kernel
    prefix = tmp_path_factory.mktemp("jup5")
    install(prefix=str(prefix))
    old = os.environ.get("JUPYTER_PATH")
    os.environ["JUPYTER_PATH"] = str(prefix / "share" / "jupyter")
    cwd = tmp_path_factory.mktemp("work5")
    km, kc = start_new_kernel(kernel_name="fermium", cwd=str(cwd))
    yield kc
    kc.stop_channels()
    km.shutdown_kernel(now=True)
    if old is None:
        os.environ.pop("JUPYTER_PATH", None)
    else:
        os.environ["JUPYTER_PATH"] = old


def execute(kc, code, reply_timeout=20):
    msg_id = kc.execute(code)
    outs = []
    while True:
        m = kc.get_iopub_msg(timeout=60)
        if m["parent_header"].get("msg_id") != msg_id:
            continue
        t = m["msg_type"]
        if t == "stream":
            outs.append((m["content"]["name"], m["content"]["text"]))
        elif t == "status" and m["content"]["execution_state"] == "idle":
            break
    return kc.get_shell_msg(timeout=reply_timeout)["content"]["status"], outs


def test_2_jupyter_cell_after_a_failed_cell_gets_a_reply(kernel):
    status, _ = execute(kernel, "a5 = 1 m\nb5 = a5 + 1 s")
    assert status == "error"
    # today: the kernel's message handler raises TypeError and never sends an execute_reply
    status, outs = execute(kernel, "print a5")
    assert status in ("ok", "error")
    assert "internal error" not in "".join(t for _, t in outs)


def test_3_jupyter_shows_run_time_warnings(kernel):
    status, outs = execute(kernel, "solve y'' = -(10/(1 s))^2 y with y(0) = 1 cm, y'(0) = 0 m/s "
                                   "for t from 0 s to 10 s step 0.1 s\nprint y(10 s)")
    assert status == "ok"
    text = "".join(t for _, t in outs)
    assert "0.252 cm" in text
    assert "too coarse" in text          # `fermium run` and the REPL show it; the notebook doesn't


PG = """
import sys, json
for m in ("llvmlite", "llvmlite.ir", "llvmlite.binding"):
    sys.modules[m] = None
sys.path[:0] = [ROOT, WEB]
import playground
r = json.loads(playground.run("print integral exp(-x^2) dx from -1e6 to 1e6", BASE))
print(json.dumps(r))
"""


def test_3_playground_shows_run_time_warnings(tmp_path):
    code = f"ROOT = {ROOT!r}\nWEB = {os.path.join(ROOT, 'web')!r}\nBASE = {str(tmp_path)!r}\n" + PG
    p = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=120)
    assert p.returncode == 0, p.stderr
    r = json.loads(p.stdout.strip().split("\n")[-1])
    assert r["stdout"] == "0\n"
    # today the warning goes to the worker's stderr (the browser console), not to the page
    assert "exactly 0" in json.dumps(r, ensure_ascii=False)


# ---- #4: d/dt x(2 s) and ∂/∂x f(1, 2) are silently 0 -------------------------------------------------------

@rt5(4)
def test_4_derivative_of_a_function_value_is_not_silently_zero():
    src = "x(t) = 3 m * t / 1 s\nprint d/dt x(2 s)"
    try:
        out, _ = run_err(src)
    except Exception as e:           # an error that suggests x'(2 s) is right too
        assert "x'(2 s)" in (str(e) + str(getattr(e, "hint", "")))
        return
    assert out == "3 m/s"            # today: "d/dt (x(2 s)) = 0"


# ---- #5: [1, 2, 3] m with your own m is silently a mass ---------------------------------------------------

@rt5(5)
def test_5_list_unit_colliding_with_a_variable_is_not_silent():
    try:
        out, err = run_err("m = 2 kg\nxs = [1, 2, 3] m\nprint xs")
    except Exception as e:
        assert "ambiguous" in str(e) or "your variable m" in str(e)
        return
    # `x = 3 m` warns in the same situation; the list silently becomes [2, 4, 6] kg
    assert "your variable m" in err


# ---- #6: parallel for errors point at column 1 --------------------------------------------------------------

@rt5(6)
def test_6_parallel_for_errors_point_at_the_statement():
    from fermium.lsp import analyze
    an = analyze("xs = zeros(3)\nparallel for i from 1 to 3\n    print i\n")
    [p] = an.problems
    assert (p.line, p.col) == (3, 5)


# ---- #7: language-server columns are code points, not UTF-16 -----------------------------------------------

@rt5(7)
def test_7_lsp_ranges_are_utf16_after_the_imaginary_unit(tmp_path):
    pytest.importorskip("pygls")
    sys.path.insert(0, os.path.join(ROOT, "tests"))
    from test_lsp import Client
    c = Client()
    try:
        rid = c.send("initialize", {"processId": None, "rootUri": None, "capabilities": {}})
        init = c.wait(lambda m: m.get("id") == rid)
        assert init["result"]["capabilities"].get("positionEncoding", "utf-16") == "utf-16"
        c.send("initialized", {}, notify=True)
        uri = (tmp_path / "a.fm").as_uri()
        src = "print 𝑖, 1 m + 1 s\n"
        c.send("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "fermium", "version": 1,
                                                         "text": src}}, notify=True)
        d = c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics")
        [diag] = d["params"]["diagnostics"]
        # 𝑖 is U+1D456: two UTF-16 code units, so `1 m + 1 s` spans characters 10..19
        assert diag["range"]["start"]["character"] == 10
        assert diag["range"]["end"]["character"] == 19
    finally:
        c.close()


# ---- #8: hover on a qualified module member ------------------------------------------------------------------

@rt5(8)
def test_8_hover_on_module_member():
    from fermium.lsp import analyze, hover_text
    src = "import mechanics\nT = mechanics.pendulum_period(1 m, 9.81 m/s^2)\n"
    h = hover_text(analyze(src), src, 1, 16)
    assert h is not None and "pendulum_period" in h


# ---- #9: the °C-in-a-product warning points at the start of the expression ----------------------------------

@rt5(9)
def test_9_celsius_product_warning_points_at_the_reading():
    from fermium.lsp import analyze
    src = "c_w = 4186 J/(kg K)\nQ = c_w * 1 kg * 10 degC\n"
    [p] = analyze(src).problems
    assert p.severity == "warning"
    assert p.col == src.split("\n")[1].index("10 degC") + 1


# ---- #10: fermium check says "no problems found" and then prints warnings ------------------------------------

def test_10_check_with_warnings_doesnt_say_no_problems(tmp_path):
    f = tmp_path / "w.fm"
    f.write_text("m = 2 kg\nx = 3 m\n")
    p = subprocess.run([sys.executable, "-m", "fermium", "check", str(f)], capture_output=True, text=True,
                       cwd=ROOT, timeout=120)
    both = p.stdout + p.stderr
    assert "warning" in both
    assert "no problems found" not in both


def test_10_check_prints_warnings_in_line_order(tmp_path):
    f = tmp_path / "w.fm"
    f.write_text("c_w = 4186 J/(kg K)\nQ = c_w * 1 kg * 10 degC\nprint Q\n"
                 "y = 2 * integral x dx from 0 to 1 - pi\nprint y\n")
    p = subprocess.run([sys.executable, "-m", "fermium", "check", str(f)], capture_output=True, text=True,
                       cwd=ROOT, timeout=120)
    lines = [int(n) for n in re.findall(r"warning: line (\d+)", p.stdout + p.stderr)]
    assert lines == sorted(lines) and len(lines) == 2


# ---- #11: :vars shows internal type names -------------------------------------------------------------------

@rt5(11)
def test_11_repl_vars_uses_physics_words():
    out = repl("z = 3 + 4i\nv = <1, 2> m/s\nM = [[1, 2], [3, 4]]\nname = \"a\"\n:vars\n")
    for internal in (": cplx", ": vec", ": mat", ": str"):
        assert internal not in out


# ---- #12: after an eigenvalue problem, `print ψ` suggests ψ = 1.0 m ------------------------------------------

@rt5(12)
def test_12_eigenstate_name_hint():
    e = error_of("solve -hbar^2/(2 m_e) * psi'' = E psi with psi(0 nm) = 0, psi(1 nm) = 0 "
                 "for x from 0 nm to 1 nm lowest 2\nprint psi")
    text = e.message + " " + str(e.hint)
    assert "ψ₁" in text or "psi_1" in text


# ---- #13: Jupyter completes only \name, not names or module members ----------------------------------------

@rt5(13)
def test_13_jupyter_completes_module_members():
    pytest.importorskip("ipykernel")
    from fermium.jupyter.kernel import FermiumKernel
    k = FermiumKernel.__new__(FermiumKernel)
    from fermium.driver import ReplSession
    k.fm = ReplSession(out=io.StringIO())
    k.fm.execute("import mechanics\n")
    code = "mechanics.spr"
    r = FermiumKernel.do_complete(k, code, len(code))
    assert "spring_period" in r["matches"]


# ---- #14, #15: bootcamp prose that disagrees with what Fermium prints ----------------------------------------

def _read(p):
    return open(os.path.join(ROOT, p), encoding="utf-8").read()


@rt5(14)
def test_14_lesson0_repl_transcript_matches_the_repl():
    text = _read("bootcamp/lesson00_setup.md")
    out = repl("print 2 m + 30 cm\nprint 1 mi in km\n")
    assert out.split() == ["2.30", "m", "1.61", "km"]
    assert "fm> print 2 m + 30 cm\n2.30 m" in text and "fm> print 1 mi in km\n1.61 km" in text


@rt5(14)
def test_14_lesson1_and_2b_prose_match_the_output():
    assert "gave `2.3 m`" not in _read("bootcamp/lesson01_numbers_units.md")     # the box says 2.30 m
    assert repl("\\theta = 30 \\deg\nprint sin(\\theta)\n").strip() == "0.500"
    assert "fm> print sin(\\theta)\n0.5\n" not in _read("bootcamp/lesson02b_symbols.md")


def test_15_lesson2_where_gotcha_is_an_error_not_a_warning():
    e = error_of("v = 3 m/s\nE = 0.5 m v^2 where m = 2 kg\nprint E")
    assert "ambiguous" in e.message
    text = _read("bootcamp/lesson02_variables_formulas.md")
    assert "means 0.5 *metres* (Fermium warns you)" not in text


# ---- #16: reference §17/§18 and the editor README -------------------------------------------------------------

@rt5(16)
def test_16_reference_tools_section_lists_the_tools():
    ref = _read("docs/reference.md")
    tools = ref[ref.index("## 17. Tools"):ref.index("## 18.")]
    for word in ("Jupyter", "fermium lsp", "fermium check", "playground"):
        assert word in tools
    grammar = ref[ref.index("## 18."):ref.index("## 19.")]
    assert grammar.count("| solve eqs [with eqs]") == 1


@rt5(16)
def test_16_doctor_checks_what_the_editor_readme_says():
    assert "check with `fermium doctor`" in _read("editors/vscode/README.md")
    src = _read("fermium/doctor.py")
    assert "pygls" in src


# ---- #17: `plot … to "f.png" title "t"` without `with` is a parse error -------------------------------------

@rt5(17)
def test_17_plot_option_after_file_name_without_with(tmp_path):
    out = run('ts = linspace(1 s, 2 s, 5)\nplot ts^2 vs ts to "a.png" title "sq"', base_dir=str(tmp_path))
    assert "a.png" in out


# ---- #18: ∂/∂x ∂/∂y f is refused although the two steps work -----------------------------------------------

@rt5(18)
def test_18_mixed_partial_derivative():
    out = run("f(x, y) = x^3 y^2\nh = partial/partial x partial/partial y f\nprint h(1, 2)")
    assert out == "12"


# ---- #19: the REPL lets a variable change its units (D13), and Lesson 2 doesn't say so -----------------------

@rt5(19)
def test_19_lesson2_says_the_repl_allows_redefinition():
    # Lesson 1 says "try everything in the REPL"; Lesson 2 says `v = 3 m/s` then `v = 5` is an error.
    # In the REPL it is silently accepted (DECISIONS D13), so the lesson's box can't be reproduced there.
    out = repl("v = 3 m/s\nv = 5\nprint v\n")
    assert out.strip() == "5"
    text = _read("bootcamp/lesson02_variables_formulas.md")
    section = text[text.index("## Variables keep their units"):text.index("## ⚠️ Gotcha: the mass")]
    assert "REPL" in section
