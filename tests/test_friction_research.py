"""Frictions found while writing the research reproductions (gauntlet/FRICTION.md #60 onwards, source "research").

Programs run compiled (LLVM) and in the reference interpreter, and the two must agree."""
import io
import math

import pytest

from conftest import run, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o)
    return o.getvalue().strip()


def both(src):
    out = run(src)
    assert interp(src) == out
    return out


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    return native.value


# ---------------------------------------------------------------- #60: tolerance and using in any order
DECAY = "solve x' = -x / (1 s)\n  with x(0) = 1 m\n  for t from 0 s to 1 s {opts}\nprint x(1 s) to 9 digits"


@pytest.mark.parametrize("opts", [
    "tolerance 1e-11 using radau",
    "using radau tolerance 1e-11",
    "tolerance 1e-11 using bdf",
    "using rk45 tolerance 1e-11",
    "method rk45 tolerance 1e-11",
    "tolerance 1e-11 using rk45",
])
def test_60_tolerance_and_using_in_either_order(opts):
    tol = 1e-4 if "bdf" in opts else 1e-9
    v = float(both(DECAY.format(opts=opts)).split()[0])
    assert abs(v - 0.367879441) < tol


def test_60_until_combines_with_tolerance_and_using():
    src = ("solve y'' = -9.81 m/s²\n  with y(0 s) = 0 m, y'(0 s) = 10 m/s\n"
           "  for t from 0 s to 5 s until y = 0 m tolerance 1e-11 using rk45\nprint times(y)[end] to 6 digits")
    src2 = src.replace("until y = 0 m tolerance 1e-11 using rk45", "using rk45 tolerance 1e-11 until y = 0 m")
    a = both(src)
    assert abs(float(a.split()[0]) - 2 * 10 / 9.81) < 1e-5
    assert a == both(src2)


def test_60_tolerance_given_twice_is_an_error():
    e = both_error(DECAY.format(opts="tolerance 1e-11 using radau tolerance 1e-9"))
    assert "given twice" in e.message


def test_60_method_name_error_lists_the_methods():
    e = both_error(DECAY.format(opts="tolerance 1e-11 using 3"))
    assert "radau" in e.message


# ---------------------------------------------------------------- #61: bracketed divisor after an upper limit
@pytest.mark.parametrize("tail", ["1 / (1 + z)", "1 / √(4 z)", "1 / |2 z|"])
def test_61_bracketed_divisor_after_upper_limit_warns(tail):
    src = f"z = 1\nprint ∫ 1 da from 0 to {tail} to 4 digits"
    assert both(src) == "0.5000"          # the whole integral (1) divided by 2
    assert any("divides the whole integral" in w for w in warnings_of(src))


def test_61_plain_divisor_still_warns():
    assert any("divides the whole integral" in w for w in warnings_of("print ∫ x dx from 0 to 1 / 2"))


@pytest.mark.parametrize("src", [
    "z = 1\nprint ∫ 1 da from 0 to 1/(1 + z) to 4 digits",
    "z = 1\nprint ∫ 1 da from 0 to (1 / (1 + z)) to 4 digits",
    "z = 1\nprint (∫ 1 da from 0 to 1) / (1 + z) to 4 digits",
])
def test_61_unambiguous_forms_do_not_warn(src):
    assert both(src) == "0.5000"
    assert not any("divides the whole integral" in w for w in warnings_of(src))


def test_61_no_warning_after_an_infinite_limit():
    src = ("I = 2.0 A\nB_axis(z) = μ₀ I (1 m)² / (2 ((1 m)² + z²)^(3/2))\n"
           "print ∫ B_axis(z) dz from -∞ m to ∞ m / (μ₀ I) to 6 digits")
    assert both(src) == "1.00000"
    assert not any("divides the whole integral" in w for w in warnings_of(src))


def test_61_lower_limit_keeps_its_division():
    src = "z = 1\nprint ∫ 1 da from 1 / (1 + z) to 1 to 4 digits"
    assert both(src) == "0.5000"
    assert not any("divides" in w for w in warnings_of(src))


# ---------------------------------------------------------------- #62: an ODE solution applied to a list
SOL = "solve x' = -x / (1 s)\n  with x(0 s) = 2 m\n  for t from 0 s to 3 s\n"


def test_62_solution_maps_over_a_list_of_times():
    out = both(SOL + "ts = [0 s, 1 s, 2 s]\nprint x(ts) to 5 digits\nprint x'(ts) to 5 digits")
    assert out.split("\n") == ["[2.0000, 0.73576, 0.27067] m", "[-2.0000, -0.73576, -0.27067] m/s"]


def test_62_mapped_solution_matches_pointwise_calls_and_converts():
    src = SOL + ("ts = linspace(0 s, 3 s, 7)\nys = x(ts)\nprint len(ys), sum(ys) to 8 digits\n"
                 "s = 0 m\nfor t in ts\n    s = s + x(t)\nprint s to 8 digits\nprint x([1 s]) in cm to 4 digits")
    lines = both(src).split("\n")
    assert lines[0].split()[0] == "7"
    assert lines[0].split(" ", 1)[1] == lines[1]
    assert lines[2] == "[73.58] cm"


def test_62_list_of_times_needs_time_units():
    e = both_error(SOL + "print x([1, 2])")
    assert "function of t" in e.message


def test_62_list_outside_the_range_is_the_usual_error():
    e = both_error(SOL + "print x([1 s, 5 s])")
    assert "outside the range" in e.message


def test_62_vector_solution_over_a_list_is_a_clear_error():
    src = ("solve r'' = <0, -9.81> m/s²\n  with r(0 s) = <0, 0> m, r'(0 s) = <1, 1> m/s\n  for t from 0 s to 1 s\n"
           "print r([0.5 s])")
    e = both_error(src)
    assert "lists of vectors" in e.message


# ---------------------------------------------------------------- #63: cot, sec, csc
def test_63_reciprocal_trig_values():
    out = both("print cot(1) to 10 digits, sec(1) to 10 digits, csc(1) to 10 digits, cot(30°) to 10 digits")
    want = [1 / math.tan(1), 1 / math.cos(1), 1 / math.sin(1), math.sqrt(3)]
    assert [float(v) for v in out.split()] == pytest.approx(want, rel=1e-9)


def test_63_reciprocal_trig_on_lists_and_at_poles():
    assert both("print cot([1, 2]) to 4 digits") == "[0.6421, -0.4577]"
    assert both("print csc(0), cot(0)") == "∞ ∞"


def test_63_reciprocal_trig_calculus():
    src = ("f(x) = cot(x) + sec(x) + csc(x)\nprint f'(0.7) to 8 digits\n"
           "print ∫ cot(x) dx from 1 to 1.5 to 8 digits\ng = ∫ csc(x)² dx\nprint g(1) to 8 digits")
    lines = both(src).split("\n")
    x = 0.7
    fp = -1 / math.sin(x) ** 2 + math.tan(x) / math.cos(x) - math.cos(x) / math.sin(x) ** 2
    assert float(lines[0]) == pytest.approx(fp, rel=1e-7)
    assert float(lines[1]) == pytest.approx(math.log(math.sin(1.5) / math.sin(1)), rel=1e-7)
    assert float(lines[2]) == pytest.approx(-1 / math.tan(1), rel=1e-7)


def test_63_reciprocal_trig_needs_plain_numbers():
    e = both_error("print cot(2 m)")
    assert "plain number" in e.message


def test_63_reference_lists_the_functions():
    import pathlib
    ref = (pathlib.Path(__file__).parent.parent / "docs" / "reference.md").read_text()
    row = next(line for line in ref.splitlines() if line.startswith("| `sin cos tan"))
    for name in ("cot", "sec", "csc", "asinh", "acosh", "atanh"):
        assert f" {name} " in row


# ---------------------------------------------------------------- #64: list slices
def test_64_slices_are_one_based_and_inclusive():
    out = both("xs = [10, 20, 30, 40, 50] [m]\nprint xs[2:4]\nprint xs[:2], xs[4:], xs[2:end-1]\nprint xs[3:3]")
    assert out.split("\n") == ["[20, 30, 40] m", "[10, 20] m [40, 50] m [20, 30, 40] m", "[30] m"]


def test_64_empty_slice_and_slice_of_a_solution():
    src = ("xs = [1, 2, 3]\nk = 3\nprint len(xs[k+1:end]), len(xs[2:1])\n"
           "solve x' = -x / (1 s)\n  with x(0 s) = 1 m\n  for t from 0 s to 1 s step 0.25 s\n"
           "print x[2:3] to 4 digits\nf(v) = sum(v[2:end])\nprint f([1, 2, 3])")
    assert both(src).split("\n") == ["0 0", "[0.7788, 0.6065] m", "5"]


def test_64_slice_is_a_copy():
    assert both("xs = [1, 2, 3]\nys = xs[1:2]\nys[1] = 9\nprint xs, ys") == "[1, 2, 3] [9, 2]"


@pytest.mark.parametrize("src, words", [
    ("xs = [1, 2, 3]\nprint xs[3:1]", "reverse(xs)"),
    ("xs = [1, 2, 3]\nprint xs[0:2]", "counts from 1"),
    ("xs = [1, 2, 3]\nprint xs[2:9]", "out of range"),
    ("xs = [1, 2, 3]\ni = 1.5\nprint xs[i:2]", "whole number"),
])
def test_64_bad_slices_are_run_time_errors(src, words):
    assert words in both_error(src).message


@pytest.mark.parametrize("src, words", [
    ("v = <1, 2, 3>\nprint v[1:2]", "only lists can be sliced"),
    ("xs = [1, 2, 3]\nxs[1:2] = 5", "not assigned to"),
    ("xs = [1, 2, 3]\nprint xs[1 m:2]", "plain number"),
])
def test_64_slice_misuse_is_a_compile_error(src, words):
    assert words in both_error(src).message


def test_64_formatter_keeps_slices():
    from fermium.fmt import format_source
    src = "xs = [1, 2, 3]\nprint xs[2:end], xs[:2]\n"
    assert "xs[2:end], xs[:2]" in format_source(src, "pretty")


# ---------------------------------------------------------------- #65: plot options without `with`
def plot_of(src):
    from fermium.errors import Diagnostics
    from fermium.parser import parse
    return parse(src, Diagnostics()).body[-1]


@pytest.mark.parametrize("tail, opts, n", [
    (', title "T"', {"title": "T"}, 1),
    (' title "T"', {"title": "T"}, 1),
    (', title "T", log y', {"title": "T", "logy": True}, 1),
    (', xs vs xs, points', {"points": True}, 2),
    (' with title "T"', {"title": "T"}, 1),
])
def test_65_plot_options_without_with(tail, opts, n):
    p = plot_of("xs = [1, 2]\nplot xs vs xs" + tail)
    assert p.options == opts and len(p.series) == n


def test_65_a_variable_named_title_is_still_a_series():
    p = plot_of("xs = [1, 2]\ntitle = [3, 4]\nplot xs vs xs, title vs xs")
    assert p.options == {} and len(p.series) == 2


def test_65_title_before_vs_has_a_clear_error():
    with pytest.raises(FermiumError) as e:
        plot_of('xs = [1, 2]\nplot xs "T"')
    assert "title goes after the series" in e.value.message


def test_65_plot_with_bare_title_runs(tmp_path):
    out = run('xs = [1, 2, 3] [m]\nplot xs vs xs, title "sq" to "p.png"', base_dir=str(tmp_path))
    assert out == "plot saved to p.png" and (tmp_path / "p.png").exists()


# ---------------------------------------------------------------- fermium build: the new code paths
def test_built_executable_matches_run_for_research_fixes(tmp_path):
    import subprocess
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    src = (SOL + "print x([0 s, 1 s]) to 5 digits\nxs = [10, 20, 30, 40] [m]\nprint xs[2:3], xs[:1], len(xs[5:4])\n"
           "print cot(1) to 8 digits, sec(1) to 8 digits, csc(1) to 8 digits\n"
           "solve y' = -y / (1 s)\n  with y(0) = 1 m\n  for t from 0 s to 1 s using rk45 tolerance 1e-11\n"
           "print y(1 s) to 9 digits\nprint xs[3:1]")
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=120)
    assert got.returncode == 1 and "reverse(xs)" in got.stderr
    with pytest.raises(FermiumError):
        run(src)
    assert got.stdout.strip() == run(src.rsplit("\n", 1)[0])


def test_63_spelled_seconds_keeps_its_hint():
    e = both_error("t = 3 sec\nprint t")
    assert "isn't a unit" in e.message and "symbols: s" in (e.hint or "")
    assert both("x = 0.5\nprint 2 sec(x) to 6 digits") == f"{2 / math.cos(0.5):.5f}"
