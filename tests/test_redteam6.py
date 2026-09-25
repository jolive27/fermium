"""Findings of the independent red-team review, round 6 (dev-notes/REDTEAM.md, "Round 6 (10:00 UTC)"): silent wrong answers
in the changes of the last three hours (D190-D223).

Each test states the correct behaviour and is marked xfail(strict=True) until its finding is fixed:
the fix agent flips a test by deleting its xfail mark.  Each test names its finding number.
Reference values come from NumPy/SciPy or closed forms (see dev-notes/REDTEAM.md).
"""
import re

import pytest

from conftest import run


def rt6(n):
    return pytest.mark.xfail(strict=True, reason=f"red team round 6 #{n}")


def num(text):
    """The first number in a printed value, as a float (handles ×10⁻¹⁵ and the minus sign)."""
    sup = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")
    t = text.replace("−", "-")
    m = re.search(r"-?\d+(?:\.\d+)?(?:×10([⁻⁰¹²³⁴⁵⁶⁷⁸⁹]+))?", t)
    assert m, text
    base = float(re.match(r"-?\d+(?:\.\d+)?", m.group(0)).group(0))
    if m.group(1):
        base *= 10 ** int(m.group(1).translate(sup))
    return base


# ---- #1: D197 prints real entries of a computed matrix as 0 (the Minkowski metric with c²) ---------------------

def test_1_metric_with_c_squared_keeps_its_unit_diagonal():
    src = """one = 1 m²/s²
z = 0 m²/s²
g = [[-c^2, z, z, z], [z, one, z, z], [z, z, one, z], [z, z, z, one]]
h = g / c^2
print inverse(h)
"""
    out = run(src)
    # NumPy: inverse(g/c²) = diag(-1, c², c², c²); the -1 must not print as 0
    assert not out.startswith("[[0,"), out
    assert "-1" in out.split("]")[0], out


def test_1_inverse_of_a_diagonal_matrix_keeps_the_small_entry():
    out = run("M = [[1, 0], [0, 1e-15]]\nprint inverse(M)\n")
    # NumPy: [[1, 0], [0, 1e15]]
    assert out.startswith("[[1"), out


# ---- #2: D197 zeroes an entry written in the program, contradicting "entries written in the program are never
#          changed" ---------------------------------------------------------------------------------------------

def test_2_written_vector_entry_is_not_printed_as_zero():
    out = run("p = <1 AU, 1 mm, 0 m>\nprint p\n")
    # 1 mm = 6.68×10⁻¹⁵ AU, written by the user; `print p[2]` shows it, `print p` shows 0
    assert "6.68" in out, out


# ---- #3: the integral noise snap zeroes integrals that are resolvable (SciPy gets them to 10⁻³) -----------------

def test_3_small_but_resolvable_integral_is_not_snapped_to_zero():
    out = run("print ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1\n")
    # exact 8×10⁻⁹; scipy.integrate.quad gives 7.99×10⁻⁹ ± 1×10⁻⁸ (estimate); Fermium prints 0
    assert out != "0", out
    assert abs(num(out) - 8e-9) < 1e-10, out


def test_3_constant_offset_on_an_odd_integrand():
    out = run("print ∫ sin(x) + 5e-15 dx from -1 to 1\n")
    # exact 1.0×10⁻¹⁴; SciPy quad: 9.99×10⁻¹⁵
    assert out != "0", out


# ---- #4: PDE results before t0 + 10 h²/D (the D206 skipped window) are silently wrong ----------------------------

def test_4_heat_step_early_times_are_accurate_or_warned():
    src = """D = 1e-4 m²/s
solve ∂u/∂t = D * ∂²u/∂x²
    with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(1 m, t) = 0 K
    for x from 0 m to 1 m, t from 0 s to 100 s
print u(2.5 mm, 0.05 s) to 5 digits
"""
    from fermium.driver import run_source
    import io
    out, err = io.StringIO(), io.StringIO()
    run_source(src, "<t>", out=out, err=err)
    val = num(out.getvalue())
    # 80 K erfc(x / (2√(D t))) = 34.336 K; Fermium prints 40.930 K with no warning (with t to 1 s: 34.214 K)
    assert abs(val - 34.336) < 0.5 or "warning" in err.getvalue(), (val, err.getvalue())


# ---- #5: D190's inverse iteration mixes a near-degenerate pair: a symmetric double well's ground state
#          loses its parity ---------------------------------------------------------------------------------------

def test_5_double_well_ground_state_is_symmetric():
    src = """m = m_e
V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 2
print ψ₁(-1 nm) / ψ₁(1 nm) to 6 digits
print ∫ x ψ₁(x)^2 dx from -3 nm to 3 nm in nm
"""
    ratio, mean_x = run(src).split("\n")
    # parity: ψ₁(-x) = ψ₁(x) and ⟨x⟩ = 0 (`using shooting` gives 4.21×10⁴ at both points and ⟨x⟩ ≈ 10⁻⁵ nm);
    # the matrix method gives 4.24×10⁴ / 4.18×10⁴ and ⟨x⟩ = -0.0132 nm
    assert abs(num(ratio) - 1) < 1e-3, ratio
    assert abs(num(mean_x)) < 1e-4, mean_x


# ---- #6: `d/ds h(2 s)` reads `2 s` as 2 seconds (D221 derivative at a point), silently -----------------------------

def test_6_derivative_variable_named_like_a_unit_is_the_variable():
    from fermium.driver import run_source
    import io
    out, err = io.StringIO(), io.StringIO()
    from fermium.errors import FermiumError
    try:
        run_source("h(s) = s^2\nprint d/ds h(2 s)\n", "<t>", out=out, err=err)
    except FermiumError as ex:       # the fix (D231) makes it a D7 error that asks which reading is meant
        err.write(f"error (stronger than the warning asked for): {ex}")
        assert "ambiguous" in str(ex), ex
    # on paper d/ds h(2s) = 2 h'(2s) = 8s (what `d/ds h(2*s)` prints as a formula); Fermium prints `4 s`
    # (h' at 2 seconds) with no warning
    assert out.getvalue().strip() != "4 s" or "warning" in err.getvalue(), out.getvalue()


# ---- #7: `∫ 3 s^2 ds` reads `3 s^2` as 3 square seconds, with no D7 warning ---------------------------------------

def test_7_integration_variable_named_like_a_unit_warns_or_is_the_variable():
    from fermium.driver import run_source
    import io
    out, err = io.StringIO(), io.StringIO()
    run_source("print ∫ 3 s^2 ds from 0 to 1\n", "<t>", out=out, err=err)
    # ∫₀¹ 3s² ds = 1; Fermium prints `3 s²` silently (a parameter `f(s) = 3 s^2` and `Σ(2 g for g …)` do warn)
    assert out.getvalue().strip() == "1" or "warning" in err.getvalue(), out.getvalue()


def _run_both(src):
    from fermium.driver import run_source
    import io
    out, err = io.StringIO(), io.StringIO()
    try:
        run_source(src, "<t>", out=out, err=err)
    except Exception as ex:          # a parse error surfaces as an exception here
        return out.getvalue(), err.getvalue() + str(ex)
    return out.getvalue(), err.getvalue()


def test_6_derivative_variable_collision_is_an_error_that_names_both_readings():
    for src, frag in (("h(s) = s^2\nprint d/ds h(2 s)\n", "'2 s' is ambiguous: s is the variable you differentiate by"),
                      ("k(m) = m^2\nprint d/dm k(2 m)\n", "'2 m' is ambiguous: m is the variable you differentiate by"),
                      ("g(x, s) = x s\nprint ∂/∂s g(1, 2 s)\n", "'2 s' is ambiguous")):
        out, err = _run_both(src)
        assert out == "" and frag in err, (src, out, err)
        assert "2*" in err and "[" in err, err
    # the explicit forms keep working: 2*s is the formula (D221), 2 [s] the unit, and a variable that isn't
    # a unit name is unaffected
    assert run("h(s) = s^2\nprint d/ds h(2*s)\n") == "d/ds (h(2·s)) = 2 h'(2·s)"
    assert run("h(s) = s^2\nprint d/ds h(2 [s])\n") == "4 s"
    assert run("x(t) = t^2\nprint d/dt x(2 s)\n") == "4 s"


def test_7_integration_variable_m_and_ode_variable_warn():
    out, err = _run_both("print ∫ 2 m dm from 0 to 1\n")
    assert "'2 m' is the unit m, not your variable m" in err, err
    out, err = _run_both("solve y' = 3 s^2 with y(0) = 0 for s from 0 to 1\nprint y(1)\n")
    assert "'3 s^2' is the unit s^2, not your variable s" in err, err
    out, err = _run_both("solve y' = 3 s y with y(0) = 1 for s from 0 to 1\nprint y(1)\n")
    assert "'3 s' is ambiguous" in err, err
    assert run("print ∫ 3*s^2 ds from 0 to 1\n") == "1"


def test_5_nearly_degenerate_pair_without_symmetry_warns():
    # the same double well on an asymmetric range: the pair can't be symmetrised, so it is flagged (D233)
    src = """m = m_e
V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3.5 nm) = 0
    for x from -3 nm to 3.5 nm
    lowest 2
print E[2] - E[1] in eV
"""
    out, err = _run_both(src)
    assert "levels 1 and 2 are nearly degenerate" in err and "ψ₁, ψ₂ can be any mixture" in err, err
    from fermium.interp import run_interpreted
    import io
    iout, ierr = io.StringIO(), io.StringIO()
    run_interpreted(src, "<t>", out=iout, err=ierr)
    assert iout.getvalue() == out and "nearly degenerate" in ierr.getvalue(), (iout.getvalue(), ierr.getvalue())


def test_5_double_well_shooting_and_odd_state():
    base = """m = m_e
V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8
solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ
    with ψ(-3 nm) = 0, ψ(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 2{m}
print ψ₂(-1 nm) / ψ₂(1 nm) to 6 digits
print ∫ ψ₁(x) ψ₂(x) dx from -3 nm to 3 nm
"""
    for m in ("", " using shooting"):
        out, err = _run_both(base.format(m=m))
        ratio, overlap = out.strip().split("\n")
        assert abs(num(ratio) + 1) < 1e-3, out
        assert abs(num(overlap)) < 1e-6, out
        assert "degenerate" not in err, err
