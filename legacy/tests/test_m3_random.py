"""M3 part 1: seeded random numbers and Monte Carlo (DECISIONS D80).

The generator is xoshiro256** seeded by splitmix64, written twice: in Python (fermium/rng.py, used by the
reference interpreter) and in LLVM IR (fermium/codegen_m3.py, used by `fermium run` and `fermium build`).
"""
import io
import math

import pytest

from conftest import run
from fermium import rng
from fermium.driver import ReplSession
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a = run(src)
    b = interp(src)
    assert a == b
    return a


# ---------------------------------------------------------------- the generator itself
def test_splitmix64_reference_vector():
    # the first output of splitmix64 from 0 (Vigna's reference implementation)
    assert rng._splitmix(0)[1] == 0xE220A8397B1DCDAF


def test_xoshiro256starstar_reference_vector():
    # xoshiro256** from the state {1, 2, 3, 4} (Vigna's reference implementation)
    st = [1, 2, 3, 4]
    assert [rng.next_u64(st) for _ in range(4)] == [11520, 0, 1509978240, 1215971899390074240]


def test_uniform_passes_kolmogorov_smirnov():
    stats = pytest.importorskip("scipy.stats")
    st = rng.state_for(2026)
    xs = [rng.rand(st) for _ in range(100_000)]
    assert min(xs) >= 0.0 and max(xs) < 1.0
    assert stats.kstest(xs, "uniform").pvalue > 1e-3


def test_normal_passes_kolmogorov_smirnov():
    stats = pytest.importorskip("scipy.stats")
    st = rng.state_for(7)
    xs = [rng.randn(st) for _ in range(100_000)]
    assert stats.kstest(xs, "norm").pvalue > 1e-3
    np = pytest.importorskip("numpy")
    assert abs(np.mean(xs)) < 4 / math.sqrt(len(xs))
    assert abs(np.std(xs) - 1) < 0.01


# ---------------------------------------------------------------- the same numbers everywhere
SEQ = """seed(42)
for i from 1 to 5
    print rand() to 15 digits
print randn() to 15 digits
seed(42)
print rand() to 15 digits
"""


def test_same_sequence_in_jit_and_interpreter_and_reference():
    out = both(SEQ).splitlines()
    st = rng.state_for(42)
    want = [rng.rand(st) for _ in range(5)] + [rng.randn(st)]
    got = [float(x) for x in out[:6]]
    for g, w in zip(got, want):
        assert g == pytest.approx(w, rel=1e-14)
    assert out[6] == out[0]            # seed(42) again restarts the sequence


def test_different_seeds_give_different_numbers():
    a = both("seed(1)\nprint rand() to 15 digits")
    b = both("seed(2)\nprint rand() to 15 digits")
    assert a != b


def test_unseeded_program_is_reproducible_and_equals_seed_zero():
    a = both("print rand() to 15 digits, randn() to 15 digits")
    assert a == both("seed(0)\nprint rand() to 15 digits, randn() to 15 digits")
    assert a == both("print rand() to 15 digits, randn() to 15 digits")


def test_repl_keeps_one_stream_across_inputs():
    out = io.StringIO()
    s = ReplSession(out=out)
    s.execute("seed(9)")
    s.execute("a = rand()")
    s.execute("b = rand()")
    s.execute("print a to 15 digits, b to 15 digits")
    st = rng.state_for(9)
    got = [float(x) for x in out.getvalue().split()]
    assert got == pytest.approx([rng.rand(st), rng.rand(st)], rel=1e-14)


def test_standalone_executable_gives_the_same_numbers(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    import subprocess
    exe = str(tmp_path / "prog")
    build(SEQ, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60).stdout.strip()
    assert got == run(SEQ)


# ---------------------------------------------------------------- units
def test_rand_and_randn_with_units():
    out = both("seed(3)\nx = rand(2 m, 3 m)\nprint x > 2 m and x < 3 m\ny = randn(10 kg, 1 g)\n"
               "print abs(y - 10 kg) < 10 g")
    assert out.splitlines() == ["true", "true"]


def test_rand_bounds_need_matching_units():
    with pytest.raises(FermiumError, match="rand\\(a, b\\): a is .* but b is"):
        run("x = rand(1 m, 2 s)")
    with pytest.raises(FermiumError, match="randn\\(μ, σ\\)"):
        run("x = randn(1 m, 2 kg)")
    with pytest.raises(FermiumError, match="no arguments or two"):
        run("x = rand(1)")


def test_seed_must_be_plain_and_a_statement():
    with pytest.raises(FermiumError, match="plain number"):
        run("seed(4 m)")
    with pytest.raises(FermiumError, match="statement"):
        run("x = seed(4)")


# ---------------------------------------------------------------- Monte Carlo
PI = """seed(2026)
N = 200000
inside = sample(if rand()^2 + rand()^2 < 1 then 1 else 0, N)
p = mean(inside)
print 4 p to 12 digits
print 4 sqrt(p (1 - p) / N) to 12 digits
"""


def test_pi_by_monte_carlo_within_three_sigma():
    est, sigma = (float(x) for x in both(PI).splitlines())
    assert sigma == pytest.approx(4 * math.sqrt(math.pi / 4 * (1 - math.pi / 4) / 200000), rel=0.01)
    assert abs(est - math.pi) < 3 * sigma


def test_sample_draws_afresh_and_keeps_units():
    out = both("seed(5)\nxs = sample(randn(1.5 m, 0.2 m), 40000)\nprint len(xs)\n"
               "print abs(mean(xs) - 1.5 m) < 0.01 m, abs(std(xs) - 0.2 m) < 0.005 m")
    assert out.splitlines() == ["40000", "true true"]


def test_sample_inside_a_function_and_with_captured_values():
    out = both("seed(6)\nf(k) = mean(sample(k rand(), 1000))\nprint abs(f(4) - 2) < 0.2\n"
               "a = 3 s\nts = sample(a + rand(0 s, 1 s), 10)\nprint min(ts) >= 3 s, max(ts) < 4 s")
    assert out.splitlines() == ["true", "true true"]


def test_monte_carlo_integral_matches_quadrature():
    # ∫₀¹ exp(-x²) dx = √π/2 · erf(1) by averaging over uniform points, within 4σ
    src = ("seed(11)\nN = 100000\nys = sample(exp(-rand()^2), N)\nprint mean(ys) to 12 digits\n"
           "print std(ys) / sqrt(N) to 12 digits")
    m, s = (float(x) for x in both(src).splitlines())
    exact = math.sqrt(math.pi) / 2 * math.erf(1)
    assert abs(m - exact) < 4 * s


def test_sample_count_must_be_plain():
    with pytest.raises(FermiumError, match="number of samples must be a plain number"):
        run("xs = sample(rand(), 10 m)")
