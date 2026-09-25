"""Fixes for the remaining open friction rows (gauntlet/FRICTION.md #70, #56, #75, #51, #80, #81, #59;
DECISIONS D190–D199).

Every program runs on both back ends (the LLVM JIT and the reference interpreter), which must agree, and
`fermium build` either prints the same or refuses with a clear error."""
import io
import os
import subprocess

import pytest

from conftest import run
from fermium.aot import build, find_cc
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
from numparse import num

needs_cc = pytest.mark.skipif(find_cc() is None, reason="no C compiler")


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a = run(src)
    assert interp(src) == a
    return a


def both_error(src):
    with pytest.raises(FermiumError) as e1:
        run(src)
    with pytest.raises(FermiumError) as e2:
        interp(src)
    assert e1.value.message == e2.value.message
    return e1.value


def built(src, tmp_path):
    exe = os.path.join(str(tmp_path), "prog")
    build(src, os.path.join(str(tmp_path), "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(tmp_path))
    assert got.returncode == 0, got.stderr
    return got.stdout.strip()


# ---- #70: the eigenfunctions of `solve … lowest N` are fourth order (Numerov), D190 ---------------------------

HYDROGEN = """k = e² / (4π ε_0)
r_max = 4 nm
solve -ħ²/(2*m_e) * ψ'' - k / r * ψ = E ψ
    with ψ(0 nm) = 0, ψ(r_max) = 0
    for r from 0 nm to r_max
    lowest 2{grid}
solve -ħ²/(2*m_e) * φ'' + (ħ² / (m_e r²) - k / r) φ = W φ
    with φ(0 nm) = 0, φ(r_max) = 0
    for r from 0 nm to r_max
    lowest 1{grid}
print (∫ ψ₂(r)² / r dr from 0 nm to r_max) a_0 * 4 - 1 to 4 digits
print (∫ φ₁(r)² / r dr from 0 nm to r_max) a_0 * 4 - 1 to 4 digits
print (∫ ψ₂(r)² / r² dr from 0 nm to r_max) a_0² * 4 - 1 to 4 digits
print (∫ φ₁(r)² / r² dr from 0 nm to r_max) a_0² * 12 - 1 to 4 digits
print (∫ ψ₂(r)² dr from 0 nm to r_max) - 1 to 4 digits
print E[1] / (-m_e k² / (2 ħ²)) - 1 to 4 digits
print E[2] / (-m_e k² / (8 ħ²)) - 1 to 4 digits
print W[1] / (-m_e k² / (8 ħ²)) - 1 to 4 digits
print ψ₂'(0 nm)² / (4π) * a_0³ * 8π - 1 to 4 digits
"""


def test_hydrogen_expectation_values_at_the_default_grid():
    """Griffiths 6.55/6.56: ⟨1/r⟩ = 1/(n² a0) (both 2s and 2p), ⟨1/r²⟩ = 1/((l + ½) n³ a0²), |ψ200(0)|² =
    1/(8π a0³).  Before D190 ⟨1/r⟩ was 1.5×10⁻⁵ off at the default grid (the finite-difference vectors)."""
    v = [num(x) for x in both(HYDROGEN.format(grid="")).splitlines()]
    assert abs(v[0]) < 2e-9 and abs(v[1]) < 2e-9          # ⟨1/r⟩ 2s, 2p (were 1.5×10⁻⁵)
    assert abs(v[2]) < 2e-8 and abs(v[3]) < 2e-8          # ⟨1/r²⟩
    assert abs(v[4]) < 1e-10                               # ∫ψ² = 1 for the interpolant ψ(r) itself
    assert abs(v[5]) < 1e-9 and abs(v[6]) < 1e-9 and abs(v[7]) < 1e-10   # the energies (1s was 6×10⁻⁸)
    assert abs(v[8]) < 1e-6                                # ψ'(0): fourth-order differences


def test_hydrogen_eigenfunctions_converge_at_fourth_order():
    coarse = [num(x) for x in run(HYDROGEN.format(grid="\n    grid 1000")).splitlines()]
    fine = [num(x) for x in run(HYDROGEN.format(grid="\n    grid 2000")).splitlines()]
    assert abs(fine[1]) < abs(coarse[1]) / 10          # 2p ⟨1/r⟩: 16× per halving (was 4×)
    assert abs(fine[0]) < abs(coarse[0]) / 10          # 2s ⟨1/r⟩


OSCILLATOR = """m = m_e
ħω = 1 eV
ω = ħω / ħ
V(x) = m ω² x² / 2
solve -ħ²/(2*m) * u'' + V(x) u = F u
    with u(-3 nm) = 0, u(3 nm) = 0
    for x from -3 nm to 3 nm
    lowest 3{method}
print (∫ u₁(x)² x² dx from -3 nm to 3 nm) / (0.5 ħ / (m ω)) - 1 to 4 digits
print (∫ u₂(x)² x² dx from -3 nm to 3 nm) / (1.5 ħ / (m ω)) - 1 to 4 digits
print (∫ u₃(x)² x² dx from -3 nm to 3 nm) / (2.5 ħ / (m ω)) - 1 to 4 digits
for n from 1 to 3
    print F[n] / ((n - 0.5) ħω) - 1 to 4 digits
print ∫ u₁(x) u₃(x) dx from -3 nm to 3 nm to 4 digits
"""


@pytest.mark.parametrize("method", ["", "\n    using shooting"])
def test_oscillator_x_squared_at_the_default_grid(method):
    """⟨x²⟩ = (n + ½) ħ/(mω) (n from 0): 4×10⁻⁶–10⁻⁵ off before D190, now ~10⁻¹⁰."""
    v = [num(x) for x in both(OSCILLATOR.format(method=method)).splitlines()]
    for d in v[:3]:
        assert abs(d) < 1e-9
    for d in v[3:6]:
        assert abs(d) < 1e-10
    assert abs(v[6]) < 1e-9                                # orthogonal


def test_finite_well_keeps_its_vectors():
    """A jump in V (a finite well): Numerov is no better than O(h²) there, so the vectors stay the finite-
    difference ones (and are still normalised)."""
    src = """V0 = 10 eV
a = 0.5 nm
V(x) = if abs(x) < a then 0 eV else V0
solve -ħ²/(2m_e) * ψ'' + V(x) ψ = E ψ
    with ψ(-2 nm) = 0, ψ(2 nm) = 0
    for x from -2 nm to 2 nm
    lowest 2
print (∫ ψ₁(x)² dx from -2 nm to 2 nm) - 1 to 3 digits
print ∫ ψ₁(x) ψ₂(x) dx from -2 nm to 2 nm to 3 digits
"""
    v = [num(x) for x in both(src).splitlines()]
    assert abs(v[0]) < 1e-6
    assert abs(v[1]) < 1e-6


# ---- #56: lists: f(xs, ys), [1, 2, 3] m, table(x = xs, y = ys) for fit (D191–D193) ----------------------------

def test_function_over_two_lists():
    assert both("f(a, b) = a * b\nxs = [1.0, 2.0, 3.0] m\nys = [4.0, 5.0, 6.0] N\nprint f(xs, ys)") == \
        "[4, 10, 18] J"


def test_function_over_the_same_list_twice_and_a_number():
    src = "f(a, b, c) = a + b * c\nxs = [1, 2, 3] m\nprint f(xs, xs, 2)\nprint f(1 m, xs, 3)"
    assert both(src).splitlines() == ["[3, 6, 9] m", "[4, 7, 10] m"]


def test_function_over_lists_of_different_lengths_is_a_runtime_error():
    src = "f(a, b) = a + b\nprint f([1, 2, 3] m, [1, 2] m)"
    for runner in (run, interp):
        with pytest.raises(Exception) as e:
            runner(src)
        assert "different lengths (3 and 2)" in str(e.value)


def test_function_over_two_lists_checks_units():
    e = both_error("f(a, b) = a + b\nprint f([1, 2] m, [1, 2] s)")
    assert "can't add" in e.message


def test_unit_after_a_list_literal():
    out = both("xs = [1, 2, 3] m\nprint xs\nprint [0.5, 1.5] km in m to 4 digits\nprint sum([1, 2] kg)")
    assert out.splitlines() == ["[1, 2, 3] m", "[500.0, 1500] m", "3 kg"]


def test_unit_after_a_list_literal_leaves_your_variable_alone():
    # m is a variable here, so [1, 2] m is still [1, 2] × m (as for matrices, D29)
    assert both("m = 3\nprint [1, 2] m") == "[3, 6]"


PENDULUM = """L = [0.20, 0.40, 0.60, 0.80, 1.00] m
T = [0.8973, 1.2690, 1.5543, 1.7947, 2.0066] s
fit T = 2π √(L / g) to table(L = L, T = T)
print g to 4 digits
"""


def test_fit_to_a_table_of_lists():
    out = both(PENDULUM)
    assert "fit T = 2π √(L/g)   (5 data points from table(L = L, T = T))" in out
    assert num(out.splitlines()[-1]) == pytest.approx(9.80, rel=2e-3)


def test_table_value_columns_and_print():
    src = """xs = [1, 2, 3, 4] s
ys = [2.1, 3.9, 6.2, 7.8] m
data = table(t = xs, x = ys)
print data
print data.x
fit x = v t + x0 to data
print v to 3 digits
"""
    out = both(src).splitlines()
    assert out[0] == "data from table(t = xs, x = ys): columns t [s], x [m]"
    assert out[1] == "[2.10, 3.90, 6.20, 7.80] m"
    assert out[-1] == "1.94 m/s"


def test_table_columns_of_different_lengths():
    src = "d = table(x = [1, 2, 3] m, y = [1, 2] s)\nprint d.x"
    for runner in (run, interp):
        with pytest.raises(Exception) as e:
            runner(src)
        assert "the columns of this table have different lengths (3 and 2)" in str(e.value)


def test_table_errors():
    assert "must be a list of numbers" in both_error("d = table(x = 3 m)").message
    assert "appears twice" in both_error("d = table(x = [1], x = [2])").message
    e = both_error("fit y = a x to 3")
    assert "load \"file.csv\" or table(x = xs, y = ys)" in e.message


@needs_cc
def test_build_fit_to_a_table_and_two_list_map(tmp_path):
    src = PENDULUM + "f(a, b) = a / b\nprint f(L, T)\n"
    assert built(src, tmp_path) == run(src)


# ---- #75: one-line helper functions inside a function (D194) ----------------------------------------------------

NESTED = """f(t, τ) =
    g(s) = exp(-s / τ)
    return g(t) + g(2 t)
print f(1 s, 2 s) to 6 digits

area(r) =
    circle(ρ) = π ρ²
    ring(a, b) = circle(b) - circle(a)
    return ring(r, 2 r)
print area(1 m) to 4 digits
print area(2) to 4 digits

pulse(t, Ω0, τ) =
    envelope(u) = Ω0 exp(-(u / τ)²)
    return ∫ envelope(u) du from -5 τ to t
print pulse(0 s, 1 /s, 2 s) to 6 digits

h(x) =
    g(s) = s * x
    return ∫ g(x) dx from 0 to 1
print h(3) to 6 digits
"""


def test_nested_helper_functions():
    assert both(NESTED).splitlines() == ["0.974410", "9.425 m²", "37.70", "1.77245", "1.50000"]


@needs_cc
def test_build_nested_helper_functions(tmp_path):
    assert built(NESTED, tmp_path) == run(NESTED)


def test_nested_helper_sees_later_values_and_solves():
    src = """decay(N0, τ) =
    N(t) = N0 exp(-t / τ)
    solve x' = -x / τ with x(0 s) = N0 for t from 0 s to 3 τ
    return (x(τ) - N(τ)) / N0
print abs(decay(1000, 2 s)) < 1e-6
"""
    assert both(src) == "true"


@pytest.mark.parametrize("src,msg", [
    ("f(x) =\n    g(s) = g(s) + 1\n    return g(x)\nprint f(1)", "g calls itself"),
    ("f(x) =\n    g(s) = s + x\n    return g\nprint f(1)", "g is a function defined inside f; it can only be called"),
    ("f(x) =\n    g(s) = s + x\n    return g(1 m)\nprint f(1 s)", "can't add"),
    ("f(x) =\n    g(s) = s + x\n    return g(1, 2)\nprint f(1)", "g takes 1 argument but was given 2"),
    ("f(x) =\n    g(s) =\n        return s\n    return g(1)\nprint f(1)", "must fit on one line"),
])
def test_nested_helper_errors(src, msg):
    e = both_error(src)
    assert msg in e.message


def test_nested_helper_error_says_which_call():
    e = both_error("f(x) =\n    g(s) = s + x\n    return g(1 m)\nprint f(1 s)")
    assert "this happened when calling g on line 3 (with s = length [m])" in e.hint
