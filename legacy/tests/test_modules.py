"""Modules and packages (M7, D100-D104): import, from-import, aliases, search path, fermium.toml,
clear errors for clashes and cycles, the REPL and the language server."""
import io
import os
import subprocess
import sys

import pytest

from conftest import run, error_of
from fermium.errors import FermiumError
from fermium.parser import parse
from fermium import ast as A
from fermium.modules import find_project, module_search_path, stdlib_dir

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def write(folder, name, text):
    path = os.path.join(str(folder), name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)
    return path


SPRINGS = """# springs: Hooke's law helpers
k_default = 50 N/m

# the force of a spring stretched by x
hooke(k, x) = -k x

# the energy stored in a spring
spring_energy(k, x) = k x² / 2

# the energy with the default spring constant
default_energy(x) = spring_energy(k_default, x)
"""


# ------------------------------------------------------------------ syntax
def test_parse_import_forms():
    body = parse('import mechanics\nimport "lib/springs.fm" as s\nfrom nuclear import semf_binding, Q_value\n'
                 'import astro as a\nfrom em import skin_depth as δ\n').body
    assert all(isinstance(s, A.Import) for s in body)
    assert (body[0].module, body[0].is_path, body[0].alias, body[0].names) == ("mechanics", False, None, None)
    assert (body[1].module, body[1].is_path, body[1].alias) == ("lib/springs.fm", True, "s")
    assert body[2].names == [("semf_binding", None), ("Q_value", None)]
    assert body[3].alias == "a"
    assert body[4].names == [("skin_depth", "δ")]


def test_import_and_as_stay_usable_as_names():
    # `import`, `as` are only special at the start of a statement
    assert run("as = 3 m\nprint as\n") == "3 m"


def test_import_needs_a_module_name():
    with pytest.raises(FermiumError) as ei:
        parse("from nuclear import\n")
    assert "expected" in ei.value.message


# ------------------------------------------------------------------ semantics
def test_import_qualified(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    out = run("import springs\nprint springs.hooke(10 N/m, 0.2 m)\nprint springs.k_default\n"
              "print springs.default_energy(0.2 m)\n", base_dir=str(tmp_path))
    assert out.split("\n") == ["-2.0 N", "50 N/m", "1.0 J"]


def test_import_as_alias(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    out = run("import springs as sp\nprint sp.spring_energy(100 N/m, 0.1 m)\n", base_dir=str(tmp_path))
    assert out == "0.50 J"


def test_from_import(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    out = run("from springs import hooke, k_default\nprint hooke(k_default, 2 cm)\n", base_dir=str(tmp_path))
    assert out == "-1 N"


def test_from_import_as(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    out = run("from springs import spring_energy as U\nprint U(2 N/m, 1 m)\n", base_dir=str(tmp_path))
    assert out == "1 J"


def test_import_by_path(tmp_path):
    write(tmp_path, "lib/springs.fm", SPRINGS)
    out = run('import "lib/springs.fm"\nprint springs.hooke(1 N/m, 1 m)\n', base_dir=str(tmp_path))
    assert out == "-1 N"
    out = run('import "lib/springs.fm" as s\nprint s.hooke(1 N/m, 1 m)\n', base_dir=str(tmp_path))
    assert out == "-1 N"


def test_module_function_passed_as_argument(tmp_path):
    write(tmp_path, "shapes.fm", "# a parabola\nsq(x) = x²\n")
    out = run("import shapes\nprint ∫ shapes.sq(x) dx from 0 m to 3 m to 4 digits\n", base_dir=str(tmp_path))
    assert out == "9.000 m³"


def test_derivative_of_imported_function():
    out = run("import astro as a\nprint a.wien_peak'(5000 K) / (1 m/K) to 6 digits\n"
              "from astro import wien_peak\ndw = wien_peak'\nprint dw(5000 K) / (1 m/K) to 6 digits\n")
    assert out.split("\n") == ["-1.15911×10⁻¹⁰"] * 2


def test_module_imports_a_module(tmp_path):
    write(tmp_path, "base.fm", "# twice\ntwice(x) = 2 x\n")
    write(tmp_path, "top.fm", "import base\n# four times\nquad(x) = base.twice(base.twice(x))\n")
    assert run("import top\nprint top.quad(1 m)\n", base_dir=str(tmp_path)) == "4 m"


def test_importing_twice_is_fine(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    out = run("import springs\nfrom springs import hooke\nimport springs\nprint hooke(1 N/m, 1 m)\n"
              "print springs.hooke(1 N/m, 1 m)\n", base_dir=str(tmp_path))
    assert out.split("\n") == ["-1 N", "-1 N"]


def test_module_names_do_not_leak(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("import springs\nprint hooke(1 N/m, 1 m)\n", base_dir=str(tmp_path))
    assert "hooke" in e.message and e.line == 2
    assert "springs.hooke" in (e.hint or "")


def test_module_uses_its_own_names_not_the_programs(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    # the program's own k_default and spring_energy don't change what the module means
    out = run("k_default = 1 N/m\nimport springs\nprint springs.default_energy(1 m)\n", base_dir=str(tmp_path))
    assert out == "25 J"


def test_units_are_checked_across_modules(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("import springs\nx = springs.hooke(10 N/m, 0.2 m) + 1 s\n", base_dir=str(tmp_path))
    assert e.line == 2 and "force" in e.message and "time" in e.message


def test_error_inside_module_function_points_at_the_call(tmp_path):
    write(tmp_path, "bad.fm", "# adds a length\nf(x) = x + 1 m\n")
    e = error_of("import bad\ny = 2\nprint bad.f(3 s)\n", base_dir=str(tmp_path))
    assert e.line == 3
    assert "bad.fm" in e.message and "line 2" in e.message


def test_module_params_with_units(tmp_path):
    write(tmp_path, "u.fm", "# speed\nspeed(d [m], t [s]) = d / t\n")
    e = error_of("import u\nprint u.speed(3 s, 1 s)\n", base_dir=str(tmp_path))
    assert e.line == 2 and "expects d in m" in e.message


# ------------------------------------------------------------------ errors
def test_missing_module(tmp_path):
    e = error_of("import nosuchthing\n", base_dir=str(tmp_path))
    assert e.line == 1 and "can't find a module called nosuchthing" in e.message
    assert "nosuchthing.fm" in (e.hint or "")


def test_missing_module_suggests_close_name(tmp_path):
    e = error_of("import mechanic\n", base_dir=str(tmp_path))
    assert "mechanics" in (e.hint or "")


def test_missing_name_in_module(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("from springs import hook\n", base_dir=str(tmp_path))
    assert e.line == 1 and "springs has no hook" in e.message and "hooke" in (e.hint or "")
    e = error_of("import springs\nprint springs.hook(1, 2)\n", base_dir=str(tmp_path))
    assert e.line == 2 and "springs has no hook" in e.message


def test_side_effects_not_allowed_in_modules(tmp_path):
    write(tmp_path, "noisy.fm", "x = 2 m\nprint x\n")
    e = error_of("import noisy\n", base_dir=str(tmp_path))
    assert e.line == 1 and "noisy.fm" in e.message and "line 2" in e.message
    assert "only define functions and constants" in e.message


def test_error_inside_module_constant(tmp_path):
    write(tmp_path, "broken.fm", "a = 1 m\nb = a + 1 s\n")
    e = error_of("import broken\n", base_dir=str(tmp_path))
    assert e.line == 1 and "broken.fm" in e.message and "line 2" in e.message and "can't add" in e.message


def test_syntax_error_inside_module(tmp_path):
    write(tmp_path, "syn.fm", "f(x) = (x + \n")
    e = error_of("import syn\n", base_dir=str(tmp_path))
    assert e.line == 1 and "syn.fm" in e.message


def test_circular_import(tmp_path):
    write(tmp_path, "a.fm", "import b\n# f\nf(x) = x\n")
    write(tmp_path, "b.fm", "import a\n# g\ng(x) = x\n")
    e = error_of("import a\n", base_dir=str(tmp_path))
    assert "circular import" in e.message and "a → b → a" in e.message


def test_name_clash_with_program_definition(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("from springs import hooke\nhooke(x) = x\n", base_dir=str(tmp_path))
    assert "hooke" in e.message and "springs" in e.message and "line 2" in e.message
    e = error_of("import springs\nsprings = 3\n", base_dir=str(tmp_path))
    assert "springs" in e.message


def test_name_clash_between_two_modules(tmp_path):
    write(tmp_path, "m1.fm", "# f\nf(x) = x\n")
    write(tmp_path, "m2.fm", "# f\nf(x) = 2 x\n")
    e = error_of("from m1 import f\nfrom m2 import f\n", base_dir=str(tmp_path))
    assert e.line == 2 and "f is already imported from m1" in e.message
    assert "import m2 as" in (e.hint or "") or "as" in (e.hint or "")


def test_import_inside_block_is_an_error(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("if true\n    import springs\n", base_dir=str(tmp_path))
    assert "top level" in e.message


def test_module_used_as_a_value(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    e = error_of("import springs\nprint springs\n", base_dir=str(tmp_path))
    assert "springs is a module" in e.message


def test_private_names_are_not_exported(tmp_path):
    write(tmp_path, "priv.fm", "_helper(x) = 2 x\n# uses the helper\npublic(x) = _helper(x)\n")
    assert run("import priv\nprint priv.public(1 m)\n", base_dir=str(tmp_path)) == "2 m"
    e = error_of("from priv import _helper\n", base_dir=str(tmp_path))
    assert "private" in e.message


def test_path_import_needs_alias_when_stem_is_not_a_name(tmp_path):
    write(tmp_path, "my-lib.fm", SPRINGS)
    e = error_of('import "my-lib.fm"\n', base_dir=str(tmp_path))
    assert "as" in (e.hint or "")
    assert run('import "my-lib.fm" as ml\nprint ml.hooke(1 N/m, 1 m)\n', base_dir=str(tmp_path)) == "-1 N"


# ------------------------------------------------------------------ search path and fermium.toml
def test_program_folder_wins_over_stdlib(tmp_path):
    write(tmp_path, "mechanics.fm", "# mine\nmine(x) = x\n")
    assert run("import mechanics\nprint mechanics.mine(2 m)\n", base_dir=str(tmp_path)) == "2 m"


def test_stdlib_is_found(tmp_path):
    assert os.path.isfile(os.path.join(stdlib_dir(), "mechanics.fm"))
    out = run("import mechanics\nprint mechanics.kinetic_energy(2 kg, 3 m/s)\n", base_dir=str(tmp_path))
    assert out == "9 J"


def test_fermium_toml_paths(tmp_path):
    write(tmp_path, "fermium.toml", '[project]\nname = "lab"\nversion = "0.1.0"\n\n[paths]\nmodules = ["lib"]\n')
    write(tmp_path, "lib/springs.fm", SPRINGS)
    write(tmp_path, "src/prog.fm", "import springs\nprint springs.hooke(2 N/m, 1 m)\n")
    proj = find_project(str(tmp_path / "src"))
    assert proj is not None and proj.name == "lab" and proj.version == "0.1.0"
    assert proj.module_paths == [str(tmp_path / "lib")]
    path = module_search_path(str(tmp_path / "src"))
    assert path[0] == str(tmp_path / "src") and path[1] == str(tmp_path / "lib") and path[-1] == stdlib_dir()
    assert run(open(tmp_path / "src/prog.fm").read(), base_dir=str(tmp_path / "src")) == "-2 N"


def test_fermium_toml_bad_file_is_a_clear_error(tmp_path):
    write(tmp_path, "fermium.toml", "[paths\nmodules = 3\n")
    e = error_of("import springs\n", base_dir=str(tmp_path))
    assert "fermium.toml" in e.message and e.line == 1


def test_fermium_toml_modules_must_be_a_list(tmp_path):
    write(tmp_path, "fermium.toml", '[paths]\nmodules = "lib"\n')
    with pytest.raises(FermiumError) as ei:
        find_project(str(tmp_path))
    assert "list" in ei.value.message


def test_cli_run_uses_fermium_toml(tmp_path):
    write(tmp_path, "fermium.toml", '[project]\nname = "lab"\n[paths]\nmodules = ["lib"]\n')
    write(tmp_path, "lib/springs.fm", SPRINGS)
    prog = write(tmp_path, "src/prog.fm", "import springs\nprint springs.hooke(2 N/m, 1 m)\n")
    r = subprocess.run([sys.executable, "-m", "fermium.cli", "run", prog], capture_output=True, text=True,
                       cwd=str(tmp_path), env={**os.environ, "PYTHONPATH": os.path.join(ROOT, "legacy")})
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "-2 N"


def test_cli_check_resolves_imports(tmp_path):
    write(tmp_path, "springs.fm", SPRINGS)
    prog = write(tmp_path, "prog.fm", "import springs\nprint springs.hooke(2 N/m, 1 m)\n")
    r = subprocess.run([sys.executable, "-m", "fermium.cli", "check", prog], capture_output=True, text=True,
                       env={**os.environ, "PYTHONPATH": os.path.join(ROOT, "legacy")})
    assert r.returncode == 0, r.stderr
    assert "no problems found" in r.stdout


def test_cli_error_in_module_names_the_module(tmp_path):
    write(tmp_path, "bad.fm", "# adds a length\nf(x) = x + 1 m\n")
    prog = write(tmp_path, "prog.fm", "import bad\nprint bad.f(3 s)\n")
    r = subprocess.run([sys.executable, "-m", "fermium.cli", "run", prog], capture_output=True, text=True,
                       env={**os.environ, "PYTHONPATH": os.path.join(ROOT, "legacy")})
    assert r.returncode == 1
    assert "prog.fm, line 2" in r.stderr and "bad.fm" in r.stderr


# ------------------------------------------------------------------ other engines, REPL, LSP
def test_interpreter_runs_imports(tmp_path):
    from fermium.interp import run_interpreted
    write(tmp_path, "springs.fm", SPRINGS)
    out = io.StringIO()
    run_interpreted("import springs\nprint springs.hooke(10 N/m, 0.2 m)\n", "<t>", out=out, base_dir=str(tmp_path))
    assert out.getvalue().strip() == "-2.0 N"


def test_repl_import(tmp_path, monkeypatch):
    from fermium.repl import main
    monkeypatch.chdir(tmp_path)
    write(tmp_path, "springs.fm", SPRINGS)
    out = io.StringIO()
    main(stdin=io.StringIO("import mechanics\nprint mechanics.kinetic_energy(2 kg, 1 m/s)\n"
                           "import springs\nfrom springs import hooke\nprint hooke(1 N/m, 3 m)\n"
                           "print springs.k_default\nimport springs\n"), stdout=out)
    text = out.getvalue()
    assert "1 J" in text and "-3 N" in text and "50 N/m" in text
    assert "error" not in text.lower()


def test_repl_session_import_across_inputs(tmp_path):
    from fermium.driver import ReplSession
    write(tmp_path, "springs.fm", SPRINGS)
    out = io.StringIO()
    s = ReplSession(out=out, base_dir=str(tmp_path))
    s.execute("import springs")
    s.execute("e = springs.default_energy(0.2 m)")
    s.execute("print e")
    s.execute("print springs.k_default * 2")
    assert out.getvalue().split() == ["1.0", "J", "100", "N/m"]


def test_lsp_analysis_resolves_imports_from_document_folder(tmp_path):
    from fermium.lsp import analyze, hover_text
    write(tmp_path, "springs.fm", SPRINGS)
    src = "import springs\nF = springs.hooke(10 N/m, 0.2 m)\nprint F\n"
    an = analyze(src, str(tmp_path))
    assert an.problems == []
    assert "force" in (hover_text(an, src, 1, 0) or "")
    assert "the module springs" in hover_text(an, src, 0, 9)
    from fermium.lsp import completions
    labels = [c[0] for c in completions(an, "import springs\nx = springs.ho", 1, 14)]
    assert labels == ["hooke"]
    # a missing module is an underlined error, not a crash
    an = analyze("import nothere\n", str(tmp_path))
    assert len(an.problems) == 1 and an.problems[0].line == 1 and "can't find" in an.problems[0].message
    # an error inside the module shows at the import line
    write(tmp_path, "broken.fm", "a = 1 m + 1 s\n")
    an = analyze("x = 1\nimport broken\n", str(tmp_path))
    assert an.problems[0].line == 2 and an.problems[0].severity == "error"


def test_formatter_keeps_imports():
    from fermium.fmt import format_source
    src = 'import mechanics\nfrom nuclear import semf_binding as B\nimport "lib/x.fm" as x\n'
    assert format_source(src, "pretty") == src
