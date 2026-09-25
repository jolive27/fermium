"""Complex numbers (D90-D95): the literal 4i and the constant 𝑖, complex quantities with units,
arithmetic, functions, printing, ODEs with complex unknowns, complex integrals, native vs interpreter."""
import cmath
import io
import math
import re

import pytest

from conftest import run, error_of
from fermium.interp import run_interpreted

HBAR = 6.62607015e-34 / (2 * math.pi)
EV = 1.602176634e-19
ME = 9.1093837139e-31

SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")


def _real(t):
    t = t.strip()
    t = re.sub(r"×10([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)", lambda m: "e" + m.group(1).translate(SUP), t)
    return float(t.replace("∞", "inf"))


def cnum(text):
    """Parse a printed complex number: '3 + 4i', '(3 - 4i) Ω', '-1.5 + 2×10⁻³i'."""
    m = re.match(r"\s*\(?\s*(\S+) ([+-]) (\S+?)i\)?", text)
    assert m, f"not a complex number: {text!r}"
    re_, sign, im_ = m.groups()
    return complex(_real(re_), _real(im_) * (-1 if sign == "-" else 1))


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


# ------------------------------------------------------------ literals and printing
@pytest.mark.parametrize("src,want", [
    ("print 3 + 4i", "3 + 4i"),
    ("print 3 - 4i", "3 - 4i"),
    ("print 4i", "0 + 4i"),
    ("print -2.5i", "0 - 2.5i"),
    ("print 1i * 1i", "-1 + 0i"),
    ("print 𝑖^2", "-1 + 0i"),
    ("print complex(1, 2)", "1 + 2i"),
    ("print (3 + 4i) [Ω]", "(3 + 4i) Ω"),
    ("print 3 Ω + 4i Ω", "(3 + 4i) Ω"),
    ("print (3 + 4i) [Ω] in kΩ", "(0.00300 + 0.00400i) kΩ"),
    ("print (1.50 + 2.25i) [m]", "(1.50 + 2.25i) m"),
    ("print 1e3i", "0 + 1000i"),
    ("print exp(1i)", "0.540 + 0.841i"),
    ("print 0.5 + 1i", "0.50 + 1.0i"),       # 0.5 has 1 s.f.; a computed value shows at least 2
    ("print (0.5 + 1i) * 1", "0.50 + 1.0i"),
    ("z = 2 - 3i\nprint z", "2 - 3i"),
])
def test_printing(src, want):
    assert run(src) == want
    assert interp(src) == want


def test_loop_variable_i_still_works():
    """`i` stays an ordinary name: only a digit right before it makes the imaginary literal."""
    src = "s = 0\nfor i from 1 to 4\n    s += 2 i\nprint s\ni = 3\nprint 2i, 2 i"
    assert run(src).split("\n") == ["20", "0 + 2i 6"]


def test_inches_are_not_imaginary():
    assert run("print 3inch in cm") == "7.62 cm"


def test_tiny_rounding_part_prints_as_zero():
    # e^{iπ} = -1 + 1.2×10⁻¹⁶ i in floating point: the imaginary part is rounding, 10¹⁶ times smaller than |z|
    assert run("print exp(𝑖 π)") == "-1 + 0i"
    assert run("print exp(𝑖 π) + 1") != "0 + 0i"       # a genuinely small number still prints
    assert run("print |exp(𝑖 π) + 1| < 1e-15") == "true"


# ------------------------------------------------------------ arithmetic against Python
CASES = [
    ("(3 + 4i) * (1 - 2i)", (3 + 4j) * (1 - 2j)),
    ("(3 + 4i) / (1 - 2i)", (3 + 4j) / (1 - 2j)),
    ("2 / (1 + 1i)", 2 / (1 + 1j)),
    ("(1 + 2i) / 4", (1 + 2j) / 4),
    ("(1 + 2i) - 5", (1 + 2j) - 5),
    ("5 - (1 + 2i)", 5 - (1 + 2j)),
    ("-(1 + 2i)", -(1 + 2j)),
    ("(1 + 2i)^3", (1 + 2j) ** 3),
    ("(1 + 2i)^-2", (1 + 2j) ** -2),
    ("(1 + 2i)^0.5", (1 + 2j) ** 0.5),
    ("(1 + 2i)^(1 - 1i)", (1 + 2j) ** (1 - 1j)),
    ("2^(1i)", 2 ** 1j),
    ("exp(1 + 2i)", cmath.exp(1 + 2j)),
    ("ln(-1 + 0i)", cmath.log(-1)),
    ("ln(3 - 4i)", cmath.log(3 - 4j)),
    ("sqrt(-4 + 0i)", cmath.sqrt(-4)),
    ("√(3 - 4i)", cmath.sqrt(3 - 4j)),
    ("sqrt(-3 - 4i)", cmath.sqrt(-3 - 4j)),
    ("sin(1 + 2i)", cmath.sin(1 + 2j)),
    ("cos(1 + 2i)", cmath.cos(1 + 2j)),
    ("tan(0.5 + 0.5i)", cmath.tan(0.5 + 0.5j)),
    ("sinh(1 + 2i)", cmath.sinh(1 + 2j)),
    ("cosh(1 + 2i)", cmath.cosh(1 + 2j)),
    ("tanh(0.5 + 0.5i)", cmath.tanh(0.5 + 0.5j)),
    ("conj(3 + 4i)", (3 + 4j).conjugate()),
    ("polar(2, π/6)", cmath.rect(2, math.pi / 6)),
    ("cis(π/3)", cmath.rect(1, math.pi / 3)),
]


@pytest.mark.parametrize("expr,want", CASES, ids=[c[0] for c in CASES])
def test_arithmetic_matches_python(expr, want):
    for runner in (run, interp):
        got = cnum(runner(f"print {expr} to 15 digits"))
        assert abs(got - want) <= 1e-13 * max(1.0, abs(want)), (runner.__name__, got, want)


@pytest.mark.parametrize("expr,want", [
    ("re(3 - 4i)", 3), ("im(3 - 4i)", -4), ("abs(3 - 4i)", 5), ("|3 - 4i|", 5),
    ("arg(-1 + 1i)", 3 * math.pi / 4), ("arg(0 - 1i)", -math.pi / 2), ("(3 - 4i).re", 3), ("(3 - 4i).im", -4),
])
def test_real_valued_functions(expr, want):
    for runner in (run, interp):
        assert float(runner(f"print {expr} to 15 digits")) == pytest.approx(want, rel=1e-14)


def test_against_numpy_random():
    import numpy as np
    rng = np.random.default_rng(1)
    zs = rng.normal(size=6) + 1j * rng.normal(size=6)
    lines = []
    want = []
    for a, b in zip(zs[::2], zs[1::2]):
        A = f"complex({float(a.real)!r}, {float(a.imag)!r})"
        B = f"complex({float(b.real)!r}, {float(b.imag)!r})"
        lines.append(f"print ({A} * {B} + {A} / {B}) * exp({B}) to 15 digits")
        want.append((a * b + a / b) * np.exp(b))
    out = run("\n".join(lines)).split("\n")
    for line, w in zip(out, want):
        assert abs(cnum(line) - w) <= 1e-13 * abs(w)


def test_euler_identity():
    out = run("print |exp(𝑖 π) + 1| to 3 digits\nprint exp(𝑖 π/2) == 𝑖\nprint exp(𝑖 π/2) ≈ 𝑖\n"
              "print cis(π) ≈ -1 + 0i")
    lines = out.split("\n")
    assert float(lines[0].replace("×10⁻¹⁶", "e-16")) < 1e-15
    assert lines[1:] == ["false", "true", "true"]


def test_equality_and_not_equal():
    assert run("print (1 + 2i) == complex(1, 2), (1 + 2i) != (1 + 3i), (2 + 0i) == 2") == "true true true"


# ------------------------------------------------------------ units
def test_impedance_units():
    out = run("R = 3 Ω\nX = 4 Ω\nZ = R + 1i X\nprint Z\nprint |Z|\nprint re(Z), im(Z)\nprint Z + (1 + 1i) [Ω]\n"
              "V = (10 + 0i) [V]\nprint V / Z\nprint conj(Z)")
    assert out.split("\n") == ["(3 + 4i) Ω", "5 Ω", "3 Ω 4 Ω", "(4 + 5i) Ω", "(1.20 - 1.60i) A", "(3 - 4i) Ω"]


def test_wavefunction_units():
    out = run("ψ0 = 1 / √(1 nm) + 0i\nprint |ψ0|² in 1/nm to 6 digits")
    assert out == "1.00000 1/nm"
    out = run("ψ0 = (1 + 1i) / √(2 nm)\nprint ψ0 * conj(ψ0) in 1/nm")
    assert cnum(out) == pytest.approx(1 + 0j)


@pytest.mark.parametrize("src,phrase", [
    ("print (3 + 4i) [Ω] + 2 V", "can't add"),
    ("print 2 V + (3 + 4i) [Ω]", "can't add"),
    ("print (1 + 1i) < (2 + 0i)", "complex numbers can't be compared with <"),
    ("print (1 + 1i) [m] > 2 m", "complex numbers can't be compared with >"),
    ("print exp((1 + 1i) [m])", "needs a plain number"),
    ("print <1, 2> + 1i", "vector"),
    ("print dot(1 + 1i, 1 - 1i)", "complex"),
    ("print floor(1.5 + 1i)", "complex"),
    ("z = 0\nz = 1 + 1i", "complex"),
    ("print (1 + 1i).x", ".re and .im"),
    ("print (1 + 2i)[1]", "re(z)"),
    ("print complex(1 m, 2 s)", "same units"),
    ("print (20 + 1i) [°C]", "°C"),
])
def test_errors(src, phrase):
    assert phrase in str(error_of(src))


def test_accumulating_needs_complex_start_hint():
    e = error_of("z = 0\nfor k from 1 to 3\n    z += 1i\nprint z")
    assert "0i" in (e.hint or "")


# ------------------------------------------------------------ physics
def test_rlc_impedance_and_phase():
    src = ("R = 100 Ω\nL = 0.5 H\nC = 10 μF\nf = 50 Hz\nω = 2π f\n"
           "Z = R + 1i ω L + 1 / (1i ω C)\n"
           "print Z to 12 digits\nprint |Z| to 12 digits\nprint arg(Z) in ° to 12 digits\n"
           "I = (230 + 0i) [V] / Z\nprint |I| to 12 digits")
    w = 2 * math.pi * 50
    Z = 100 + 1j * w * 0.5 + 1 / (1j * w * 10e-6)
    out = run(src).split("\n")
    assert out[0].endswith(" Ω")
    assert cnum(out[0]) == pytest.approx(Z, rel=1e-11)
    assert float(out[1].split()[0]) == pytest.approx(abs(Z), rel=1e-11)
    assert float(out[2].rstrip("°")) == pytest.approx(math.degrees(cmath.phase(Z)), rel=1e-11)
    assert float(out[3].split()[0]) == pytest.approx(230 / abs(Z), rel=1e-11)
    assert out[3].endswith(" A")


def t_exact(E, V0, a):
    if E < V0:
        k = math.sqrt(2 * ME * (V0 - E)) / HBAR
        return 1 / (1 + V0 ** 2 * math.sinh(k * a) ** 2 / (4 * E * (V0 - E)))
    k = math.sqrt(2 * ME * (E - V0)) / HBAR
    return 1 / (1 + V0 ** 2 * math.sin(k * a) ** 2 / (4 * E * (E - V0)))


def test_tunneling_transmission_with_complex_amplitudes():
    """One formula for both E < V₀ and E > V₀: q = √(2m(E − V₀))/ħ is imaginary below the barrier."""
    src = ("V0 = 5.00 eV\na = 0.500 nm\n"
           "T(E) = 1 / |cos(q a) - 1i (k² + q²) / (2 k q) * sin(q a)|² where k = √(2 m_e E) / ħ, "
           "q = √(2 m_e (E - V0) + 0i) / ħ\n"
           "for E in [0.5 eV, 2 eV, 4.9 eV, 5.5 eV, 10 eV]\n    print T(E) to 12 digits")
    for runner in (run, interp):
        got = [_real(x) for x in runner(src).split("\n")]
        for E, g in zip([0.5, 2, 4.9, 5.5, 10], got):
            assert g == pytest.approx(t_exact(E * EV, 5 * EV, 0.5e-9), rel=1e-10)


@pytest.mark.parametrize("init", ["1", "1 + 0i", "complex(1, 0)"])
def test_free_particle_phase_ode(init):
    """iħ ψ' = E ψ: ψ(t) = exp(−iEt/ħ); the complex unknown is two real state slots."""
    src = (f"E = 1.0 eV\nT = 1e-14 s\nsolve 1i ħ ψ' = E ψ with ψ(0 s) = {init} for t from 0 s to T\n"
           "print ψ(T) to 12 digits\nprint |ψ(T) - exp(-1i E T / ħ)| to 3 digits\nprint |ψ(T/2)| to 12 digits")
    for runner in (run, interp):
        out = runner(src).split("\n")
        w = cmath.exp(-1j * EV * 1e-14 / HBAR)
        assert cnum(out[0]) == pytest.approx(w, abs=1e-7)
        assert _real(out[1]) < 1e-7
        assert float(out[2]) == pytest.approx(1.0, abs=1e-7)      # between steps: dense output


def test_complex_second_order_ode_matches_exponential():
    # ψ'' = -k² ψ with ψ(0) = 1, ψ'(0) = ik: ψ = e^{ikx}
    src = ("k = 2 /m\nsolve ψ'' = -k² ψ with ψ(0 m) = 1 + 0i, ψ'(0 m) = 1i k for x from 0 m to 3 m\n"
           "print ψ(3 m) to 12 digits\nprint ψ'(3 m) to 12 digits")
    out = run(src).split("\n")
    assert cnum(out[0]) == pytest.approx(cmath.exp(6j), abs=1e-7)
    assert cnum(out[1].replace(" 1/m", "")) == pytest.approx(2j * cmath.exp(6j), abs=1e-6)


def test_fresnel_integral_against_scipy():
    from scipy.special import fresnel
    S, C = fresnel(2.0)
    for runner in (run, interp):
        z = cnum(runner("print ∫ exp(1i π t² / 2) dt from 0 to 2 to 14 digits"))
        assert z.real == pytest.approx(C, abs=1e-10)
        assert z.imag == pytest.approx(S, abs=1e-10)


def test_fourier_integral_with_units():
    # ∫₀^L e^{ikx} dx = (e^{ikL} − 1)/(ik), in metres
    src = "k = 3 /m\nL = 2 m\nprint ∫ exp(1i k x) dx from 0 m to L to 12 digits"
    out = run(src)
    assert out.endswith(" m")
    w = (cmath.exp(6j) - 1) / 3j
    assert cnum(out) == pytest.approx(w, abs=1e-10)


def test_complex_sum():
    out = run("print Σ(exp(1i π k / 2) for k from 0 to 3) to 12 digits")
    assert abs(cnum(out)) < 1e-12


def test_user_functions_with_complex_values():
    out = run("f(z) = z² + 1\nprint f(1i)\nprint f(2 + 0i)\ng(x) = exp(1i x)\nprint re(g(π)) to 12 digits\n"
              "Z(ω) = R + 1i ω L where R = 1 Ω, L = 1 H\nprint Z(2 /s)")
    assert out.split("\n") == ["0 + 0i", "5 + 0i", "-1.00000000000", "(1 + 2i) Ω"]


def test_symbolic_derivative_of_complex_function():
    out = run("g(x) = exp(1i k x) where k = 2 /m\nprint g'(0 m)\nprint g''(0 m)")
    assert out.split("\n") == ["(0 + 2i) 1/m", "(-4 + 0i) 1/m²"]


def test_if_expression_and_loop_accumulation():
    src = "z = 0i\nfor k from 1 to 4\n    z = z * 1i + 1\nprint z\nw = if re(z) > 0 then z else -z\nprint w"
    out = run(src)
    assert out == interp(src)
    z = 0j
    for _ in range(4):
        z = z * 1j + 1
    assert cnum(out.split("\n")[0]) == pytest.approx(z)


def test_captured_complex_in_integral():
    src = "c = 1 + 1i\nprint ∫ c * x dx from 0 to 2"
    assert cnum(run(src)) == pytest.approx(2 + 2j)


# ------------------------------------------------------------ native vs interpreter
PROGRAMS = [
    "z = (1 + 2i) * (3 - 1i) / (2 + 0.5i)\nprint z\nprint |z|, arg(z)",
    "Z = 50 Ω + 1i (2π 1 kHz)(10 mH)\nprint Z\nprint Z in kΩ\nprint arg(Z) in °",
    "print exp(1i), ln(1i), sqrt(1i), sin(1i), cos(1i), sinh(1i), cosh(1i), tan(1i), tanh(1i)",
    "print (0 + 0i)^2, (0 + 0i)^(1 + 1i), 0i / (0 + 0i)",
    "print ∫ exp(-x²) * cis(x) dx from -∞ to ∞",
    "solve 1i ψ' = 2 ψ with ψ(0) = 1 for t from 0 to 1\nprint ψ(1), ψ(0.5)",
    "solve 1i ψ' = 2 ψ with ψ(0) = 1 for t from 0 to 1 step 0.01\nprint ψ(1)",
]


@pytest.mark.parametrize("src", PROGRAMS)
def test_native_and_interpreter_agree(src):
    assert interp(src) == run(src)


def test_hover_describes_complex():
    from fermium.lsp import analyze, hover_text
    src = "Z = (3 + 4i) [Ω]\nprint Z"
    text = hover_text(analyze(src), src, 0, 0)
    assert "complex" in text and "Ω" in text


def test_fermium_build_prints_complex(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    import subprocess
    src = ("Z = 3 Ω + 4i Ω\nprint Z, Z in kΩ, |Z|, arg(Z) in °\nprint exp(𝑖 π), (1.50 + 2.25i) [m], 0i / (0 + 0i)\n"
           "solve 1i ψ' = 2 ψ with ψ(0) = 1 for t from 0 to 1\nprint ψ(1)\n"
           "print ∫ exp(1i π t² / 2) dt from 0 to 2")
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    out = subprocess.run([exe], capture_output=True, text=True, timeout=60).stdout.strip()
    assert out == run(src)


def test_indefinite_integral_uses_the_imaginary_unit():
    # SymPy sees 𝑖 as its I, not as a real symbol
    out = run("F = ∫ exp(1i x) dx\nprint F\nprint F(π) - F(0)")
    lines = out.split("\n")
    assert "𝑖" in lines[0]
    assert cnum(lines[1]) == pytest.approx(2j)


def test_description_shows_the_literal():
    assert run("g(x) = exp(2i x)\nprint g") == "g(x) = exp(2i x)"
    assert run("g(x) = exp(1i x)\nprint g") == "g(x) = exp(𝑖 x)"


def test_standalone_imaginary_unit_splits_names():
    assert run("E = 1.0 eV\nsolve 𝑖ħ ψ' = E ψ with ψ(0 s) = 1 for t from 0 s to 1 fs\nprint |ψ(1 fs)| to 6 digits") \
        == "1.00000"


def test_fmt_ascii_spells_the_imaginary_unit():
    from fermium.fmt import format_source
    assert format_source("z = 2𝑖 + 𝑖 x + 3i\n", "ascii") == "z = 2 1i + (1i) x + 3i\n"
    assert format_source("a = 𝑖ħ\n", "ascii") == "a = (1i)hbar\n"
    assert run("x = 2\nz = 2 1i + 1i x + 3i\nprint z") == "0 + 7i"


def test_repl_keeps_complex_variables():
    from fermium.repl import main
    out = io.StringIO()
    main(stdin=io.StringIO("Z = 3 Ω + 4i Ω\nprint Z\nprint |Z|\n"), stdout=out)
    assert [ln for ln in out.getvalue().split("\n") if ln.strip()] == ["(3 + 4i) Ω", "5 Ω"]


def test_lists_of_complex_are_refused_clearly():
    assert "complex" in str(error_of("print [1i, 2]"))
    e = error_of("solve 1i ψ' = ψ with ψ(0) = 1 for t from 0 to 1\nprint max(ψ)")
    assert "lists of complex numbers aren't supported" in str(e)


def test_parts_of_a_complex_solution_are_real_solutions():
    src = ("solve 1i ψ' = 2 ψ with ψ(0) = 1 for t from 0 to 1\n"
           "print ψ.re(1) to 12 digits, ψ.im(1) to 12 digits\nprint min(ψ.im) to 12 digits\n"
           "print len(values(ψ.re)) > 3")
    for runner in (run, interp):
        out = runner(src).split("\n")
        a, b = map(float, out[0].split())
        assert a == pytest.approx(math.cos(2), abs=1e-8) and b == pytest.approx(-math.sin(2), abs=1e-8)
        assert float(out[1]) == pytest.approx(-1, abs=1e-8)
        assert out[2] == "true"


def test_complex_ode_with_radau_and_until():
    out = run("solve 1i ψ' = 2 ψ with ψ(0) = 1 for t from 0 to 1 using radau\nprint |ψ(1) - exp(-2i)| < 1e-5\n"
              "solve 1i φ' = 2 φ with φ(0) = 1 for t from 0 to 10 until re(φ) = 0\nprint φ[end] to 9 digits")
    lines = out.split("\n")
    assert lines[0] == "true"
    assert cnum(lines[1]) == pytest.approx(-1j, abs=1e-8)


def test_parameter_with_unit_takes_complex():
    unit = run("print 1 / (5 Ω)").split(" ", 1)[1]           # whatever unit a conductance is shown in
    assert run("Y(Z [Ω]) = 1 / Z\nprint Y(3 Ω + 4i Ω)") == f"(0.120 - 0.160i) {unit}"
    assert "expects Z in Ω" in str(error_of("Y(Z [Ω]) = 1 / Z\nprint Y(2 V + 1i V)"))


def test_power_delivered_to_an_impedance():
    assert run("P(V, Z) = re(V conj(V / Z)) / 2\nprint P((10 + 0i) [V], 3 Ω + 4i Ω) to 3 digits") == "6.00 W"


def test_recursive_complex_function_is_refused_not_miscompiled():
    e = error_of("f(n) = if n == 0 then 1 + 0i else 1i * f(n - 1)\nprint f(3)")
    assert "calls itself" in str(e)
