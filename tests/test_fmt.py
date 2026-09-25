"""`fermium fmt`: ASCII <-> symbols without changing a program's meaning.

Round-trip tests compare ASTs (ignoring positions, paren flags and UnitExpr.text, and
treating unit spellings like `um`/`μm` as the same unit) and the program's output.
"""
import io
import os
import re
import shutil
import subprocess
import sys
from dataclasses import fields, is_dataclass

import pytest

from conftest import run
from fermium import ast as A
from fermium.driver import run_source
from fermium.errors import Diagnostics, FermiumError
from fermium.fmt import format_source
from fermium.parser import parse
from fermium.units import lookup_unit

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

IGNORED = {"line", "col", "length", "paren"}


def _unit_key(name):
    u = lookup_unit(name)
    if u is None:
        return name
    return (repr(u.dim), round(u.factor, 12), u.offset)


def norm(n):
    """AST -> nested tuples, insensitive to spelling-only differences."""
    if isinstance(n, (list, tuple)):
        return tuple(norm(x) for x in n)
    if isinstance(n, float):
        return round(n, 12)
    # `½` and `(1/2)` are the same number
    if isinstance(n, A.BinOp) and n.op == "/" and n.paren and isinstance(n.left, A.Num) \
            and isinstance(n.right, A.Num) and n.left.sigfigs is None and n.right.sigfigs is None:
        return ("Num", round(n.left.value / n.right.value, 12))
    # `x⁻¹` is Num(-1) while `x^-1` is Neg(Num(1))
    if isinstance(n, A.Neg) and isinstance(n.operand, A.Num):
        return norm(A.Num(-n.operand.value, n.operand.sigfigs))
    if isinstance(n, A.Num):
        return ("Num", round(n.value, 12)) if n.sigfigs is None else ("Num", round(n.value, 12), n.sigfigs)
    if isinstance(n, A.UnitFactor):
        return ("UnitFactor", _unit_key(n.name), n.exp)
    if is_dataclass(n):
        items = []
        for f in fields(n):
            if f.name in IGNORED or (isinstance(n, A.UnitExpr) and f.name == "text"):
                continue
            items.append((f.name, norm(getattr(n, f.name))))
        return (type(n).__name__, tuple(items))
    return n


def ast_of(src):
    return norm(parse(src, Diagnostics()))


def output_of(src, base_dir):
    out = io.StringIO()
    try:
        run_source(src, "<fmt>", out=out, base_dir=base_dir, err=io.StringIO())
    except FermiumError as e:
        return out.getvalue() + "ERROR: " + e.message
    return out.getvalue()


def pretty(src):
    return format_source(src, "pretty")


def ascii_(src):
    return format_source(src, "ascii")


# ============================================================ specific conversions
@pytest.mark.parametrize("src,expected", [
    ("g = 4 pi^2 L / T^2", "g = 4 π² L / T²"),
    ("print sqrt(x) + theta * omega_0", "print √(x) + θ · ω₀"),
    ("W = integral F(x) dx from 0 m to 0.2 m", "W = ∫ F(x) dx from 0 m to 0.2 m"),
    ("x = 3 um + 2 angstrom", "x = 3 μm + 2 Å"),
    ("y = 1 Msun", "y = 1 M☉"),
    ("z = 20 degC", "z = 20 °C"),
    ("a = 30 deg", "a = 30 °"),
    ("if x <= 3 and y != 2 and z >= 1\n    print 1", "if x ≤ 3 and y ≠ 2 and z ≥ 1\n    print 1"),
    ("print x ~= y", "print x ≈ y"),
    ("x = inf", "x = ∞"),
    ("E = hbar c", "E = ħ c"),
    ("k = x^-1 + x^12", "k = x⁻¹ + x¹²"),
    ("a = epsilon_0 * mu_0", "a = ε₀ · μ₀"),
    ("v = 3 m s^-2", "v = 3 m s⁻²"),
    ("y = x^(1/3)", "y = x^(1/3)"),          # not an integer: left alone
    ("x = 1 # pi stays in comments", "x = 1 # pi stays in comments"),
    ('print "theta"', 'print "theta"'),       # strings are not touched
])
def test_pretty_conversions(src, expected):
    assert pretty(src) == expected


@pytest.mark.parametrize("src,expected", [
    ("E = ½ m v²", "E = (1/2) m v^2"),
    ("c = 3.00×10⁸ m/s", "c = 3.00e8 m/s"),
    ("x = 6.67×10⁻¹¹", "x = 6.67e-11"),
    ("x = √(2 g h)", "x = sqrt(2 g h)"),
    ("x = √x y", "x = sqrt(x) y"),
    ("x = ∛8", "x = cbrt(8)"),
    ("print 1.5 μm, 2 Å, 1 M☉, 20 °C", "print 1.5 um, 2 angstrom, 1 Msun, 20 degC"),
    ("x = ∞", "x = inf"),
    ("θ₀ = 3", "theta_0 = 3"),
    ("print a ≤ b, a ≥ b, a ≠ b, a ≈ b", "print a <= b, a >= b, a != b, a ~= b"),
    ("y = x · z", "y = x * z"),
    ("W = ∫ F(x) dx from 0 m to 1 m", "W = integral F(x) dx from 0 m to 1 m"),
    ("y = x⁻¹", "y = x^-1"),
])
def test_ascii_conversions(src, expected):
    assert ascii_(src) == expected


def test_ascii_pi_squared():
    assert ascii_("g = 4π² L / T²") in ("g = 4pi^2 L / T^2", "g = 4 pi^2 L / T^2")


def test_ascii_output_is_pure_ascii():
    src = "E = ½ m v²\nω₀ = √(k/m)\nprint ħ c in MeV fm\nW = ∫ F(x) dx from 0 m to ∞"
    assert ascii_(src).isascii()


def test_name_without_ascii_spelling_left_alone_with_warning():
    d = Diagnostics()
    out = format_source("ΔE = 3 J\nprint ΔE", "ascii", d)
    assert out == "ΔE = 3 J\nprint ΔE"
    assert any("no plain-ASCII spelling" in w.message for w in d.warnings)


def test_comments_and_blank_lines_preserved():
    src = "# pendulum\n\nL = 1.20 m   # length\n\n\nprint L\n"
    assert pretty(src) == src
    assert ascii_(src) == src


def test_indentation_preserved():
    src = "f(x) =\n    y = x^2\n    return sqrt(y)\nprint f(3)\n"
    assert pretty(src) == "f(x) =\n    y = x²\n    return √(y)\nprint f(3)\n"


def test_unit_after_variable_name_not_prettified_as_unit():
    # `deg` in brackets is the unit (prettified); `deg` as a variable stays as written
    assert pretty("deg = 3\nx = 30 [deg] + deg") == "deg = 3\nx = 30 [°] + deg"


def test_pretty_idempotent():
    src = "g = 4 pi^2 L / T^2\nprint sqrt(g) in m/s"
    once = pretty(src)
    assert pretty(once) == once


def test_ascii_idempotent():
    src = "g = 4π² L / T²\nprint √g"
    once = ascii_(src)
    assert ascii_(once) == once


def test_pretty_multi_digit_subscript_roundtrip():
    src = "x_12 = 3\nprint x_12"
    p = pretty(src)
    assert ast_of(p) == ast_of(src)
    assert run(p) == "3"
    assert ast_of(ascii_(p)) == ast_of(src)


def test_ascii_pi_before_name_keeps_meaning():
    src = "f = 3\nω = 2πf\nprint ω"
    a = ascii_(src)
    assert ast_of(a) == ast_of(src)
    assert run(a) == run(src)


def test_ascii_partial_keeps_meaning():
    src = "g(x) = x^2\nprint ∂/∂x g"
    a = ascii_(src)
    assert ast_of(a) == ast_of(src)
    assert run(a) == run(src)


def test_format_error_is_fermium_error():
    with pytest.raises(FermiumError):
        format_source("x = (", "pretty")


# ============================================================ round trips over a corpus
PRETTY_CORPUS = [
    "L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g\nprint g in ft/s²",
    "m = 2.0 kg\nv = 3.0 m/s\nE = ½ m v²\np = m v\nprint E, p",
    "k = 50 N/m\nm = 0.5 kg\nω = √(k/m)\nprint ω in rad/s",
    "A = 0.1 m\nω = 10 rad/s\nx(t) = A cos(ω t)\nv = d/dt x\na = x''\nprint v\nprint a\nprint v(0.1 s)",
    "k = 50 N/m\nF(x) = k x\nW = ∫ F(x) dx from 0 m to 0.2 m\nprint W\nprint ∫ exp(-x²) dx from -∞ to ∞",
    "print ħ c in MeV fm\nprint m_e c² in MeV\nprint k_B (300 K) in eV",
    "A = 56\nR = 1.2 [fm] A^(1/3)\nprint R, ∛A",
    "x = 3 m\nif x ≥ 2 m and x ≠ 5 m\n    print \"far\"\nelse\n    print \"near\"",
    "G_N = 6.674×10⁻¹¹ N m²/kg²\nc_light = 3.00×10⁸ m/s\nprint G_N, c_light",
    "θ₀ = 30°\nprint sin(θ₀), cos(θ₀)²  + sin(θ₀)²",
    "ε₀ = 8.854×10⁻¹² F/m\nq = 1.0 μC\nr = 2.0 cm\nprint q² / (4π ε₀ r²)",
    "d = 1.3 pc\nprint d in ly, d in AU\nprint 1 M☉ in kg\nprint 3 Å in nm",
    "T = 20 °C\nprint T in K, T in °F",
    "xs = [1 m, 2 m, 3 m]\nys = 2 xs + 1 m\nprint ys, √(sum(ys²))",
    "λ = 500 nm\nE = h c / λ\nprint E in eV",
    "f(x) = x³ - 2x\nprint f', f''\nprint f'(2)",
    "m = 0.5 kg\nk = 50 [N/m]\nb = 0.2 kg/s\nsolve m x'' = -k x - b x'\n  with x(0) = 0.1 [m], x'(0) = 0 m/s\n  for t from 0 s to 5 s\nprint x(5 s)",
    "total = 0 m\nfor i from 1 to 10\n    total += i · 1 cm\nprint total\nprint 1 ≈ 1.0000001",
    "E = ½ m v² where m = 2 kg, v = 3 m/s\nprint E\nprint |-3 m|",
]

ASCII_CORPUS = [
    "L = 1.20 m\nT = 2.21 s\ng = 4 pi^2 L / T^2\nprint g\nprint g in ft/s^2",
    "m = 2.0 kg\nv = 3.0 m/s\nE = (1/2) m v^2\nprint E",
    "k = 50 N/m\nm = 0.5 kg\nomega = sqrt(k/m)\nprint omega in rad/s",
    "A = 0.1 m\nomega = 10 rad/s\nx(t) = A cos(omega t)\nv = d/dt x\nprint v(0.1 s)",
    "print integral exp(-x^2) dx from -inf to inf\nprint sqrt(pi)",
    "print hbar c in MeV fm\nprint m_e c^2 in MeV",
    "theta_0 = 30 deg\nprint sin(theta_0)^2 + cos(theta_0)^2",
    "epsilon_0 = 8.854e-12 F/m\nq = 1.0 uC\nr = 2.0 cm\nprint q^2 / (4 pi epsilon_0 r^2)",
    "x = 3 m\nif x >= 2 m and x != 5 m\n    print \"far\"\nelse if x <= 1 m\n    print \"near\"\nelse\n    print \"mid\"",
    "d = 1.3 pc\nprint d in ly\nprint 1 Msun in kg\nprint 3 angstrom in nm\nprint 20 degC in K",
    "lambda = 500 nm\nE = h c / lambda\nprint E in eV",
    "xs = [1 m, 2 m]\nfor x in xs\n    print x^2, x^-1\nprint 1 ~= 1.0000001",
    "f(x) = x^3 - 2 x\nprint f'(2)\nprint cbrt(27 m^3)",
    "a = 2 s^-1\nsolve x' = -a x with x(0) = 1 kg for t from 0 s to 1 s\nprint x(1 s)",
]


def doc_blocks():
    text = open(os.path.join(ROOT, "docs", "reference.md"), encoding="utf-8").read()
    return [m.group(1) for m in re.finditer(r"```fermium\n(.*?)```", text, re.S)]


def _ids(prefix, corpus):
    return [f"{prefix}{i:02d}" for i in range(len(corpus))]


# Program output must not change either.  Some corpus programs print a unit spelled in the
# source (`in ft/s^2`, `in degF`, `N m^2/kg^2`): the printed unit must not depend on the
# spelling (bugs-tests B16, fixed).
def _params(prefix, corpus, output=False):
    return [pytest.param(src, id=f"{prefix}{i:02d}") for i, src in enumerate(corpus)]


@pytest.mark.parametrize("src", _params("p", PRETTY_CORPUS) + _params("doc", doc_blocks()))
def test_roundtrip_pretty_ascii_pretty_ast(src):
    a = ascii_(src)
    p = pretty(a)
    ref = ast_of(src)
    assert ast_of(a) == ref, f"ascii changed meaning:\n{a}"
    assert ast_of(p) == ref, f"pretty changed meaning:\n{p}"


@pytest.mark.parametrize("src", _params("p", PRETTY_CORPUS, True) + _params("doc", doc_blocks(), True))
def test_roundtrip_pretty_ascii_pretty_output(src, tmp_path):
    a = ascii_(src)
    p = pretty(a)
    out = output_of(src, str(tmp_path))
    assert "ERROR" not in out, out
    assert output_of(a, str(tmp_path)) == out
    assert output_of(p, str(tmp_path)) == out


@pytest.mark.parametrize("src", _params("a", ASCII_CORPUS))
def test_roundtrip_ascii_pretty_ascii_ast(src):
    p = pretty(src)
    a = ascii_(p)
    ref = ast_of(src)
    assert ast_of(p) == ref, f"pretty changed meaning:\n{p}"
    assert ast_of(a) == ref, f"ascii changed meaning:\n{a}"
    assert a == src          # ASCII written by a person comes back exactly


@pytest.mark.parametrize("src", _params("a", ASCII_CORPUS, True))
def test_roundtrip_ascii_pretty_ascii_output(src, tmp_path):
    p = pretty(src)
    a = ascii_(p)
    out = output_of(src, str(tmp_path))
    assert "ERROR" not in out, out
    assert output_of(p, str(tmp_path)) == out
    assert output_of(a, str(tmp_path)) == out


def test_unit_display_does_not_depend_on_spelling():
    assert run("g = 9.81 m/s²\nprint g in ft/s^2") == run("g = 9.81 m/s²\nprint g in ft/s²")
    assert run("print 20 °C in degF") == run("print 20 °C in °F")


def test_corpus_size():
    assert len(PRETTY_CORPUS) + len(ASCII_CORPUS) >= 20


# ============================================================ CLI
def fermium_cmd():
    exe = shutil.which("fermium")
    return [exe] if exe else [sys.executable, "-m", "fermium.cli"]


def test_cli_fmt_pretty(tmp_path):
    f = tmp_path / "asc.fm"
    f.write_text("g = 4 pi^2 L / T^2\nprint sqrt(g)\n", encoding="utf-8")
    r = subprocess.run(fermium_cmd() + ["fmt", str(f), "--pretty"],
                       capture_output=True, text=True, timeout=60)
    assert r.returncode == 0, r.stderr
    assert r.stdout == "g = 4 π² L / T²\nprint √(g)\n"
    assert f.read_text(encoding="utf-8") == "g = 4 pi^2 L / T^2\nprint sqrt(g)\n"   # not rewritten


def test_cli_fmt_ascii_write(tmp_path):
    f = tmp_path / "p.fm"
    f.write_text("E = ½ m v²\n", encoding="utf-8")
    r = subprocess.run(fermium_cmd() + ["fmt", str(f), "--ascii", "-w"],
                       capture_output=True, text=True, timeout=60)
    assert r.returncode == 0, r.stderr
    assert f.read_text(encoding="utf-8") == "E = (1/2) m v^2\n"
