"""Stiff ODEs: `solve ... using radau` / `using bdf` (gauntlet friction #25, nuclear N1; DECISIONS D42).

Every program runs compiled (LLVM, the right-hand side called back from SciPy) and in the reference
interpreter, and the two must print the same thing."""
import io
import math
import re

import pytest

from conftest import run
from fermium.driver import run_source
from fermium.errors import FermiumError
from fermium.interp import run_interpreted

scipy = pytest.importorskip("scipy")
from scipy.integrate import solve_ivp  # noqa: E402


def interp(src):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    return o.getvalue().strip()


def both(src):
    out = run(src)
    assert interp(src) == out
    return out


SUP = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹⁻", "0123456789-")


def nums(src):
    """The numbers a program prints (both ways), one per word: '2.0×10⁻¹²' -> 2e-12."""
    out = []
    for tok in both(src).split():
        out.append(float(re.sub(r"×10([⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+)", lambda m: "e" + m.group(1).translate(SUP), tok)))
    return out


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    assert native.value.line == ref.value.line
    return native.value


# ---------------------------------------------------------------- radon progeny (N1)
LN2 = math.log(2)
HALF = [3.098 * 60, 26.8 * 60, 19.9 * 60, 164.3e-6]          # Po-218, Pb-214, Bi-214, Po-214 (s)
LAM = [LN2 / h for h in HALF]


def bateman(k, t, n0=1e6):
    """N_k(t) of a chain that starts as n0 atoms of the first member (closed form)."""
    s = 0.0
    for i in range(k + 1):
        p = 1.0
        for j in range(k + 1):
            if j != i:
                p *= LAM[j] - LAM[i]
        s += math.exp(-LAM[i] * t) / p
    return n0 * math.prod(LAM[:k]) * s


RADON = """
λ1 = ln(2) / 3.098 min
λ2 = ln(2) / 26.8 min
λ3 = ln(2) / 19.9 min
λ4 = ln(2) / 164.3 μs
solve
    N1' = -λ1 N1
    N2' = λ1 N1 - λ2 N2
    N3' = λ2 N2 - λ3 N3
    N4' = λ3 N3 - λ4 N4
    with N1(0) = 1e6, N2(0) = 0, N3(0) = 0, N4(0) = 0
    for t from 0 min to 720 min using {method}
"""
TIMES = [0.5, 7, 30, 95.25, 200, 437.9, 720]      # minutes (between steps, and the end)


def radon_errors(method, extra=""):
    lines = []
    for tm in TIMES:
        for k in range(4):
            lines.append(f"print abs(N{k + 1}({tm} min) / {bateman(k, tm * 60)!r} - 1)")
    return nums(RADON.format(method=method) + "\n".join(lines) + "\nprint len(times(N1))\n" + extra)


def test_radon_chain_720_min_against_bateman():
    # the last line: Po-214 follows Bi-214 at once, so their activities are equal (secular equilibrium;
    # λ4 N4 / λ3 N3 = λ4 / (λ4 - λ3) = 1 + 10⁻⁷)
    *errs, steps, equilibrium = radon_errors("radau", "print abs(λ4 N4(300 min) / (λ3 N3(300 min)) - 1)")
    assert max(errs) < 1e-7, errs            # tolerance 1e-9 per step; interpolated between steps
    assert steps < 20_000                    # RK45 needs ~10⁷ steps (and fails at 720 min)
    assert equilibrium < 1e-6


def test_radon_chain_bdf():
    *errs, steps = radon_errors("bdf")
    assert max(errs) < 1e-4, errs            # BDF is lower order: looser, but still right
    assert steps < 20_000


# ---------------------------------------------------------------- Robertson's chemical kinetics
def robertson(t, y):
    return [-0.04 * y[0] + 1e4 * y[1] * y[2], 0.04 * y[0] - 1e4 * y[1] * y[2] - 3e7 * y[1] ** 2, 3e7 * y[1] ** 2]


@pytest.mark.parametrize("tend", [40.0, 1e5])
def test_robertson_against_scipy(tend):
    ref = solve_ivp(robertson, (0, tend), [1.0, 0.0, 0.0], method="Radau", rtol=1e-12,
                    atol=[1e-20, 1e-24, 1e-20]).y[:, -1]
    src = f"""
k1 = 0.04 / 1 s
k2 = 3e7 / 1 s
k3 = 1e4 / 1 s
solve
    A' = -k1 A + k3 B C
    B' = k1 A - k3 B C - k2 B²
    C' = k2 B²
    with A(0) = 1, B(0) = 0, C(0) = 0
    for t from 0 s to {tend!r} s using radau
print abs(A[end] / {float(ref[0])!r} - 1)
print abs(B[end] / {float(ref[1])!r} - 1)
print abs(C[end] / {float(ref[2])!r} - 1)
print abs(A[end] + B[end] + C[end] - 1)
print len(times(A))
"""
    *errs, cons, steps = nums(src)
    assert max(errs) < 1e-7, errs
    assert cons < 1e-12                      # mass conservation
    assert steps < 3000


# ---------------------------------------------------------------- Van der Pol, μ = 1000
def test_van_der_pol_relaxation_period():
    src = """
μ = 1000
solve x'' = μ (1 - x²) x' / 1 s - x / (1 s)² with x(0 s) = 2, x'(0 s) = 0 / 1 s for t from 0 s to 3300 s using radau
solve x(T1) = 0 for T1 from 0 s to 3300 s
solve x(T2) = 0 for T2 from T1 + 10 s to 3300 s
print 2 (T2 - T1) / (1 s)
print len(times(x))
"""
    period, steps = nums(src)
    asym = (3 - 2 * math.log(2)) * 1000       # the leading term; the next is O(μ^(-1/3))
    assert abs(period / asym - 1) < 0.005, period
    assert steps < 20_000


# ---------------------------------------------------------------- with until, backwards, vectors
def test_until_backwards_and_vectors():
    src = """
g = 9.81 m/s²
solve y'' = -g with y(0) = 0 m, y'(0) = 10 m/s for t from 0 s to 5 s using radau until y = 0 m
print abs(times(y)[end] / (2 * 10 [m/s] / g) - 1)
print abs(y'[end] / (-10 m/s) - 1)
solve x' = -x / 2 s with x(3 s) = 1 m for t from 3 s to 0 s using radau
print abs(x[end] / (exp(1.5) * 1 m) - 1)
print abs(x(1 s) / (exp(1) * 1 m) - 1)
print times(x)[end] / (1 s)
solve r'' = -r / (1 s)² with r(0) = <1, 0> m, r'(0) = <0, 1> m/s for t from 0 s to 2π * 1 s using radau
print |r[end] - <1, 0> m| / (1 m)
print |r(π * 1 s) - <-1, 0> m| / (1 m)
"""
    e1, e2, e3, e4, tend, e5, e6 = nums(src)
    assert e1 < 1e-9 and e2 < 1e-8
    assert e3 < 1e-8 and e4 < 1e-6
    assert tend == 0
    assert e5 < 1e-7 and e6 < 1e-6


def test_until_never_happens():
    e = both_error("solve x' = -x / 1 s with x(0) = 1 m for t from 0 s to 1 s using radau until x = 2 m")
    assert "never" in e.message or "make the range longer" in e.message


# ---------------------------------------------------------------- errors
def test_radau_with_a_step_is_refused():
    with pytest.raises(FermiumError) as e:
        run("solve x' = -x with x(0) = 1 for t from 0 to 1 step 0.1 using radau")
    assert "radau chooses its own steps" in e.value.message


def test_unknown_method_names_radau():
    with pytest.raises(FermiumError) as e:
        run("solve x' = -x with x(0) = 1 for t from 0 to 1 using euler")
    assert "radau" in e.value.message


def test_error_inside_the_right_side_reports_its_own_message():
    e = both_error("L = [1, 2]\nk = 3\nsolve x' = -L[k] x with x(0) = 1 for t from 0 to 1 using radau\nprint x(1)")
    assert "index 3 is out of range" in e.message and e.line == 3


def test_error_in_a_function_called_by_the_right_side_has_its_line():
    src = "L = [1, 2]\nf(i) = L[i]\nsolve x' = -f(3) x with x(0) = 1 for t from 0 to 1 using radau\nprint x(1)"
    e = both_error(src)
    assert "index 3 is out of range" in e.message


def test_blow_up():
    e = both_error("solve z' = z² / 1 s with z(0) = 1 for t from 0 s to 2 s using radau")   # z = 1/(1 - t)
    assert "blow up" in e.message and "t = 1.00 s" in e.message


def test_nan_at_start_and_empty_range():
    e = both_error("solve x' = 1 / x with x(0) = 0 for t from 0 to 1 using radau")
    assert "NaN or infinite" in e.message
    e = both_error("solve x' = -x with x(1) = 1 for t from 1 to 1 using radau")
    assert "empty" in e.message


def test_the_program_continues_after_a_stiff_solve():
    out = both("solve x' = -x / 1 s with x(0) = 1 for t from 0 s to 1 s using radau\n"
               "solve y' = -y / 1 s with y(0) = 1 for t from 0 s to 1 s\nprint x(1 s) ~= y(1 s)")
    assert out == "true"


def test_build_refuses_radau(tmp_path):
    pytest.importorskip("llvmlite")
    from fermium.aot import build
    p = tmp_path / "s.fm"
    p.write_text("solve x' = -x with x(0) = 1 for t from 0 to 1 using radau\nprint x(1)\n")
    with pytest.raises(FermiumError) as e:
        build(p.read_text(), str(p), str(tmp_path / "s"))
    assert "fermium build can't compile  using radau" in e.value.message and e.value.line == 1


# ---------------------------------------------------------------- rk45: stiffness warning and message
def test_rk45_warns_that_an_equation_looks_stiff(monkeypatch):
    import fermium.interp as fi
    from fermium.codegen_llvm import ModuleGen
    monkeypatch.setattr(fi, "STIFF_AFTER", 200)
    monkeypatch.setattr(ModuleGen, "STIFF_AFTER", 200)
    src = "solve y' = -1e4 (y - cos(t / 1 s)) / 1 s with y(0) = 0 for t from 0 s to 1 s\nprint y(1 s)"
    err = io.StringIO()
    p = run_source(src, "<t>", out=io.StringIO(), err=err)
    rt = run_interpreted(src, "<t>", out=io.StringIO())
    assert p.runtime.warnings == rt.warnings
    assert len(rt.warnings) == 1 and "looks stiff" in rt.warnings[0] and "using radau" in rt.warnings[0]
    # a smooth, non-stiff problem of the same length doesn't warn
    src = "solve y' = cos(1000 t / 1 s) with y(0) = 0 for t from 0 s to 10 s\nprint y(1 s)"
    assert run_interpreted(src, "<t>", out=io.StringIO()).warnings == []
    assert run_source(src, "<t>", out=io.StringIO(), err=io.StringIO()).runtime.warnings == []


def test_too_many_steps_message_suggests_radau():
    from fermium.runtime.core import Runtime
    msg = Runtime(out=io.StringIO()).describe_error(3, 15302.7, -1)
    assert "using radau" in msg
