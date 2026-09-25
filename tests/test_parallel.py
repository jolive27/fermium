"""`parallel for` (M5, DECISIONS D152): the iterations run on several threads; sums are added up block by
block in a fixed order, so the results are the same for any number of threads, in the reference interpreter
and in `fermium build`; anything two iterations could both touch is a compile-time error."""
import io
import os
import subprocess

import pytest

from conftest import run, error_of
from fermium.interp import run_interpreted
from fermium.errors import FermiumError
from fermium import ir as I


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def run_threads(src, n, monkeypatch):
    """Run with FERMIUM_THREADS=n (read once per compiled program)."""
    monkeypatch.setenv("FERMIUM_THREADS", str(n))
    return run(src)


PI = """
n = 200000
xs = zeros(n)
total = 0
parallel for i from 1 to n
    x = (i - 0.5) / n
    y = 4 / (1 + x²)
    xs[i] = y
    total += y / n
print total to 17 digits
serial = 0
for i from 1 to n
    serial += xs[i] / n
print serial to 17 digits
print abs(total - serial) < 1e-12
print xs[1] to 17 digits, xs[n] to 17 digits
"""


def test_same_numbers_for_every_thread_count_and_the_interpreter(monkeypatch):
    ref = interp(PI)
    for t in (1, 2, 3, 4, 7):
        assert run_threads(PI, t, monkeypatch) == ref
    lines = ref.split("\n")
    assert lines[2] == "true"
    assert abs(float(lines[0]) - 3.141592653589793) < 1e-9


def test_blocks_cover_the_range_once():
    for n in (0, 1, 2, 5, 255, 256, 257, 1000, 12345):
        blocks = I.par_blocks(n)
        assert len(blocks) == min(n, I.PAR_BLOCKS)
        covered = [i for a, b in blocks for i in range(a, b)]
        assert covered == list(range(n))


SUMS = """
n = 1000
a = 10 J
b = 0 J
c = 0
parallel for i from 1 to n
    a += i * 1 J
    b -= 2 i * 1 J
    if mod(i, 2) == 0
        continue
    c += 1
print a, b, c
"""


def test_several_sums_minus_and_continue(monkeypatch):
    out = run_threads(SUMS, 4, monkeypatch)
    assert out == interp(SUMS)
    assert out.split()[0] == "500510"
    assert out.endswith(" 500")


INTEGRALS = """
B(ν, T) = 2h ν³/c² / (exp(h ν/(k_B T)) - 1)
n = 300
Ts = linspace(1000 K, 10000 K, n)
P = zeros(n) [W/m²]
total = 0 W/m²
parallel for i from 1 to n
    T = Ts[i]
    I = ∫ B(ν, T) dν from 1e11 Hz to 1e16 Hz
    P[i] = I
    total += I
print total / (1 W/m²) to 17 digits
s = 0 W/m²
for i from 1 to n
    s += P[i]
print abs(total - s) / s < 1e-13
print P[n] / (σ (10000 K)⁴ / π) to 8 digits
"""


def test_integrals_and_functions_inside(monkeypatch):
    out = run_threads(INTEGRALS, 4, monkeypatch)
    assert out == interp(INTEGRALS)
    assert out.split("\n")[1] == "true"
    assert out.split("\n")[2] == "1.0000000"


IN_FUNCTION = """
sumsq(k) =
    s = 0
    parallel for j from 1 to k
        s += j²
    s
print sumsq(1000), sumsq(1), sumsq(0)
"""


def test_inside_a_function_and_empty_ranges(monkeypatch):
    out = run_threads(IN_FUNCTION, 3, monkeypatch)
    assert out == interp(IN_FUNCTION)
    assert out.split()[1:] == ["1", "0"]


NBODY_FORCES = """
N = 60
x = zeros(N) [m]
y = zeros(N) [m]
M = zeros(N) [kg]
for k from 1 to N
    x[k] = cos(0.7 k) * k * 1 m
    y[k] = sin(1.3 k) * 1 m
    M[k] = (1 + mod(k, 3)) * 1 kg
ax = zeros(N) [m/s²]
U = 0 J
parallel for i from 1 to N
    fx = 0 m/s²
    for j from 1 to N
        if j != i
            dx = x[j] - x[i]
            dy = y[j] - y[i]
            r = √(dx² + dy²)
            fx += G M[j] dx / r³
            U -= G M[i] M[j] / r / 2
    ax[i] = fx
print U to 17 digits
print ax[1] to 17 digits, ax[N] to 17 digits
"""


def test_force_sum_matches_interpreter(monkeypatch):
    assert run_threads(NBODY_FORCES, 4, monkeypatch) == interp(NBODY_FORCES)


def test_step_and_non_integer_ranges(monkeypatch):
    src = """
s = 0 s
parallel for t from 0 s to 1 s step 0.1 s
    s += t
print s to 17 digits
q = 0
parallel for k from 10 to 1 step -3
    q += k
print q
"""
    out = run_threads(src, 4, monkeypatch)
    assert out == interp(src)
    assert out.split("\n")[1] == "22"


# ---------------------------------------------------------------- run-time errors


def test_error_in_a_worker_is_reported(monkeypatch):
    src = "xs = zeros(10)\nout = zeros(12)\nparallel for i from 1 to 12\n    out[i] = xs[i] + 1\nprint 1\n"
    for t in (1, 4):
        monkeypatch.setenv("FERMIUM_THREADS", str(t))
        e = error_of(src)
        assert "out of range" in e.message and e.line == 4
    # the program can go on running parallel loops after a failed one in another program
    assert run_threads(PI, 4, monkeypatch) == interp(PI)


def test_same_list_under_two_names_is_an_error():
    src = "xs = zeros(10)\nys = xs\nparallel for i from 1 to 10\n    xs[i] = ys[i] + 1\n"
    e = error_of(src)
    assert "xs and ys are the same list" in e.message
    with pytest.raises(FermiumError) as ei:
        interp(src)
    assert ei.value.message == e.message
    ok = "xs = zeros(10)\nys = xs * 1\nparallel for i from 1 to 10\n    xs[i] = ys[i] + 1\nprint xs[3]\n"
    assert run(ok) == "1"


# ---------------------------------------------------------------- compile-time errors


@pytest.mark.parametrize("src,msg", [
    ("s = 0\nparallel for i from 1 to 10\n    s = s * 2\n", "s is shared by all the iterations"),
    ("s = 0\nparallel for i from 1 to 10\n    s = i\n", "s is shared by all the iterations"),
    ("x = zeros(10)\nparallel for i from 1 to 9\n    x[i + 1] = 1\n", "may only write its own element, x[i]"),
    ("x = zeros(10)\nparallel for i from 2 to 10\n    x[i] = x[i - 1]\n", "can only be read as x[i]"),
    ("x = zeros(10)\nparallel for i from 1 to 10\n    x[i] = sum(x)\n", "can only be read as x[i]"),
    ("parallel for i from 1 to 3\n    print i\n", "print can't be used inside a parallel for"),
    ("parallel for i from 1 to 3\n    y = rand()\n", "random numbers can't be drawn inside a parallel for"),
    ("s = 0\nparallel for i from 1 to 3\n    s += 1\n    t = s\n", "the sum s can't be read inside"),
    ("f(n) = if n <= 1 then 1 else n f(n - 1)\ns = 0\nparallel for i from 1 to 3\n    s += f(i)\n",
     "recursive function"),
    ("parallel for i from 1 to 3\n    y = i\nprint y\n", "belongs to the iterations of the parallel for"),
    ("parallel for i from 1 to 3\n    parallel for j from 1 to 3\n        y = 1\n", "inside another parallel for"),
    ("xs = [1, 2]\nparallel for x in xs\n    y = x\n", "parallel for works with a range"),
    ("parallel for i from 1 to 3\n    if i > 1\n        break\n", "break can't be used inside a parallel for"),
    ("xs = [1.0]\nparallel for i from 1 to 3\n    push(xs, 1)\n", "can't be used inside a parallel for"),
    ("show(k) =\n    print k\n    k\ns = 0\nparallel for i from 1 to 3\n    s += show(i)\n", "show uses print"),
])
def test_races_are_compile_time_errors(src, msg):
    e = error_of(src)
    assert msg in e.message, e.message


def test_inner_loop_variable_is_private(monkeypatch):
    src = """
j = 100
tot = 0
parallel for i from 1 to 50
    for j from 1 to i
        tot += j
print tot, j
"""
    out = run_threads(src, 4, monkeypatch)
    assert out == interp(src) == "22100 100"


# ---------------------------------------------------------------- fermium build


def test_fermium_build_runs_parallel_loops(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "pi")
    build(PI, str(tmp_path / "pi.fm"), exe)
    ref = interp(PI)
    for t in ("1", "4"):
        r = subprocess.run([exe], capture_output=True, text=True, env={**os.environ, "FERMIUM_THREADS": t})
        assert r.returncode == 0, r.stderr
        assert r.stdout.strip() == ref
