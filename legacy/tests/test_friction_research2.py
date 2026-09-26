"""Frictions from the BBN research reproduction (gauntlet/FRICTION.md #82-#86, source "research"; D160-D164).

Programs run compiled (LLVM) and in the reference interpreter, and the two must agree; `fermium build`
is checked where it applies."""
import io
import math
import subprocess

import pytest

from conftest import run, error_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted


def interp(src, base_dir=None):
    o = io.StringIO()
    run_interpreted(src, "<t>", out=o, base_dir=base_dir)
    return o.getvalue().strip()


def both(src, base_dir=None):
    out = run(src, base_dir)
    assert interp(src, base_dir) == out
    return out


def both_error(src):
    with pytest.raises(FermiumError) as native:
        run(src)
    with pytest.raises(FermiumError) as ref:
        interp(src)
    assert native.value.message == ref.value.message
    return native.value


def built(src, tmp_path, name="prog"):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / name)
    build(src, str(tmp_path / (name + ".fm")), exe)
    return subprocess.run([exe], capture_output=True, text=True, timeout=120, cwd=str(tmp_path))


# ---------------------------------------------------------------- #82: absolute tolerances (D160)
SHO = ("solve\n    x'' = -x / (1 s²)\n    with x(0) = 1 m, x'(0) = 0 m/s\n    for t from 0 s to 10 s {opts}\n"
       "print x(10 s) to 10 digits")


def test_66_absolute_tolerance_rk45_with_a_derivative_slot():
    # x' (m/s) borrows x's tolerance divided by the range (10 s)
    v = float(both(SHO.format(opts="tolerance 1e-8 absolute 1e-12 m")).split()[0])
    assert v == pytest.approx(math.cos(10), abs=1e-7)


def test_66_absolute_tolerance_per_unit():
    v = float(both(SHO.format(opts="tolerance 1e-8 absolute 1e-12 m, 1e-12 m/s")).split()[0])
    assert v == pytest.approx(math.cos(10), abs=1e-7)


def test_66_a_negligible_absolute_tolerance_changes_nothing():
    # the absolute term is added to the relative scale, so a negligible one leaves every step as it was
    assert both(SHO.format(opts="absolute 1e-300 m")) == both(SHO.format(opts=""))


DECAY = "solve y' = -y / (1 s) with y(0) = 1 for t from 0 s to 50 s using {m} {opts}\nprint y(50 s) to 4 digits"


@pytest.mark.parametrize("m", ["radau", "bdf"])
def test_66_absolute_tolerance_stiff(m):
    exact = math.exp(-50)
    # purely relative (D42, unchanged): the decay is followed to ~1e-9 relative accuracy
    rel = float(both(DECAY.format(m=m, opts="")).replace("×10⁻²²", "e-22"))
    assert rel == pytest.approx(exact, rel=1e-4)
    # with an absolute tolerance of 1e-16, values far below it are only accurate in absolute terms
    out = both(DECAY.format(m=m, opts="tolerance 1e-10 absolute 1e-16"))
    v = float(out.replace("×10⁻", "e-").translate(str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹", "0123456789")))
    assert abs(v - exact) < 1e-15 and abs(v - exact) > 1e-4 * exact


@pytest.mark.parametrize("opts,msg", [
    ("absolute 1e-9 s", "no absolute tolerance for x, which is length [m]"),
    ("absolute 1e-9 m, 2e-9 m", "two absolute tolerances in length [m]"),
    ("absolute 1e-9 m, 1e-9 kg", "no unknown of this solve is in mass [kg]"),
    ("absolute -1e-9 m", "positive constant"),
    ("absolute 0 m", "positive constant"),
    ("step 0.01 s absolute 1e-9 m", "with  step  the steps are fixed"),
])
def test_66_absolute_tolerance_units_are_checked(opts, msg):
    assert msg in error_of(SHO.format(opts=opts)).message


def test_66_absolute_tolerance_for_two_units_needs_both():
    src = ("solve\n    x' = v\n    v' = -x / (1 s²)\n    with x(0) = 1 m, v(0) = 0 m/s\n    for t from 0 s to 1 s "
           "absolute 1e-9 m\nprint x(1 s)")
    e = error_of(src)
    assert "no absolute tolerance for v, which is speed [m/s]" in e.message
    assert "like  absolute 1e-9 m, 1e-12 m/s" in e.message
    ok = src.replace("absolute 1e-9 m", "absolute 1e-9 m, 1e-9 m/s").replace("print x(1 s)", "print x(1 s) to 6 digits")
    assert float(both(ok).split()[0]) == pytest.approx(math.cos(1), abs=1e-6)


def test_66_absolute_is_refused_where_it_means_nothing():
    assert "absolute" in error_of("solve x^2 = 2 for x from 0 to 2 absolute 1e-9\nprint x").message


def test_66_absolute_can_still_be_a_variable():
    assert both("absolute = 3\nprint absolute + 1") == "4"


CHATTER = "solve y' = -sign(y) / 1 s with y(0) = 1 for t from 0 s to 3 s{opts}\nprint abs(y(3 s)) < 1e-5"


def test_66_a_stalled_step_that_is_not_a_blow_up_says_so():
    # y reaches 0 at t = 1 s and then chatters around it: nothing grows, the relative control stalls
    e = both_error(CHATTER.format(opts=""))
    assert "step became too small near t = 1 s" in e.message
    assert "probably not a blow-up" in e.message and "absolute 1e-16" in e.message
    assert both(CHATTER.format(opts=" absolute 1e-6")) == "true"


@pytest.mark.parametrize("src", [
    "solve z' = z² / 1 s with z(0) = 1 for t from 0 s to 2 s using radau\nprint z(2 s)",
    "solve x' = x^2 with x(0) = 1 for t from 0 to 2\nprint x(2)",
    "solve x' = (1 + x^2) / 1 s with x(0) = 0 for t from 0 s to 2 s\nprint x(2 s)",      # tan t from 0
])
def test_66_a_real_blow_up_still_says_blow_up(src):
    e = both_error(src)
    assert "may blow up" in e.message and "not a blow-up" not in e.message


def test_66_step_small_kind():
    from fermium.runtime.stiff import ERR_ODE_H, ERR_ODE_H_FLAT, step_small_kind
    assert step_small_kind([0.5, 0.5, 0.0], [0.5, 0.5, 1e-23]) == ERR_ODE_H_FLAT     # a tiny abundance
    assert step_small_kind([1.0], [1e7]) == ERR_ODE_H                                # 1/(1 - t)
    assert step_small_kind([0.0], [1e15]) == ERR_ODE_H                               # tan t from 0
    assert step_small_kind([1.0], [math.nan]) == ERR_ODE_H
    assert step_small_kind([0.0, 0.0], [0.0, 0.0]) == ERR_ODE_H_FLAT


def test_66_built_executable_matches(tmp_path):
    src = SHO.format(opts="tolerance 1e-8 absolute 1e-12 m")
    got = built(src, tmp_path)
    assert got.returncode == 0 and got.stdout.strip() == run(src)
    got = built(CHATTER.format(opts=""), tmp_path, "chatter")
    assert got.returncode == 1 and "probably not a blow-up" in got.stderr
    assert got.stderr.strip().endswith(error_of(CHATTER.format(opts="")).message)
    assert "fermium build can't compile  using radau" in str(pytest.raises(
        FermiumError, built, DECAY.format(m="radau", opts="absolute 1e-16"), tmp_path, "stiff").value)


# ---------------------------------------------------------------- #83: plot ranges, labels, reversed axes (D161)
PLOT = "T = [1, 0.5, 0.2, 0.1] [MeV]\nD = [1e-15, 1e-10, 1e-6, 1e-5]\nplot D vs T {opts} to \"p.png\""


def _axes(src, tmp_path, monkeypatch, backend):
    plt = pytest.importorskip("matplotlib.pyplot")
    figs = []
    monkeypatch.setattr(plt, "close", figs.append)
    out = backend(src, str(tmp_path))
    assert out == f"plot saved to {tmp_path / 'p.png'}" and (tmp_path / "p.png").exists()
    return figs[-1].axes[0]


@pytest.mark.parametrize("backend", [run, interp])
def test_67_ranges_labels_reversed(tmp_path, monkeypatch, backend):
    opts = ('with log, y from 1e-12 to 1e-3, x from 30 keV to 2 MeV, reversed x, xlabel "T [MeV]", '
            'ylabel "mass fraction"')
    ax = _axes(PLOT.format(opts=opts), tmp_path, monkeypatch, backend)
    assert ax.get_ylim() == pytest.approx((1e-12, 1e-3))
    assert ax.get_xlim() == pytest.approx((2, 0.03))           # MeV (the data's unit), reversed
    assert ax.get_xlabel() == "T [MeV]" and ax.get_ylabel() == "mass fraction"


@pytest.mark.parametrize("backend", [run, interp])
def test_67_label_gets_the_unit_unless_it_names_one(tmp_path, monkeypatch, backend):
    ax = _axes(PLOT.format(opts=', xlabel "temperature", reversed x'), tmp_path, monkeypatch, backend)
    assert ax.get_xlabel() == "temperature [MeV]" and ax.get_ylabel() == "D"
    assert ax.get_xlim()[0] > ax.get_xlim()[1]


def test_67_options_without_with_even_for_a_variable_named_y(tmp_path):
    src = "y = [1, 2, 3] [m]\nx = [1, 2, 3] [s]\nplot y vs x, y from 0 m to 5 m, reversed y to \"p.png\""
    assert both(src, str(tmp_path)) == f"plot saved to {tmp_path / 'p.png'}"


@pytest.mark.parametrize("opts,msg", [
    ("with y from 1 s to 2 s", "the y axis is a plain number (no units), but this end of its range is time [s]"),
    ("with x from 1 to 2", "the x axis is energy [J], but this end of its range is a plain number"),
    ("with y from 1 to 0", "add  reversed y"),
    ("with log y, y from 0 to 1", "can't start at 0"),
    ("with reversed z", "with reversed x"),
    ('with xlabel T', "axis label in quotes"),
    ("with y from D[1] to 1", "must be constants"),
])
def test_67_option_errors(opts, msg):
    assert msg in error_of(PLOT.format(opts=opts)).message


def test_67_built_svg_has_ranges_labels_and_reversed_x(tmp_path):
    import re
    src = PLOT.format(opts='with log y, y from 1e-12 to 1e-3, reversed x, xlabel "T [MeV]", ylabel "mass fraction"')
    got = built(src, tmp_path)
    assert got.returncode == 0, got.stderr
    svg = (tmp_path / "p.svg").read_text()
    assert ">T [MeV]</text>" in svg and ">mass fraction</text>" in svg
    pts = [tuple(map(float, p.split(","))) for p in re.search(r'points="([^"]*)"', svg).group(1).split()]
    xs = [p[0] for p in pts]
    assert xs == sorted(xs)             # T decreases along the data, so a reversed x axis draws it left to right
    assert ">10⁻¹²</text>" in svg or ">10⁻¹⁰</text>" in svg
    assert "10⁻¹⁵" not in svg           # the y range, not the data, sets the axis


def test_67_pde_plot_refuses_ranges():
    src = ("solve ∂u/∂t = 1e-3 m²/s * ∂²u/∂x²\n    with u(x, 0 s) = exp(-(x / 0.1 m)²), u(-1 m, t) = 0, u(1 m, t) = 0\n"
           "    for x from -1 m to 1 m, t from 0 s to 1 s\nplot u vs x, y from 0 to 1")
    assert "takes only  title" in error_of(src).message


# ---------------------------------------------------------------- #84: max/min of a list and a number (D162)
def test_68_elementwise_max_and_min():
    src = "xs = [1e-20, 0.5, 1e-3]\nys = [2, 1, 3] [m]\nprint max(xs, 1e-12), min(ys, 1.5 m), max(0 m, ys, [1, 4, 1] [m])"
    assert both(src) == "[1.0×10⁻¹², 0.50, 0.0010] [1.5, 1.0, 1.5] m [2, 4, 3] m"


def test_68_elementwise_units_and_lengths_are_checked():
    assert "max needs all values in the same units" in error_of("xs = [1, 2] [m]\nprint max(xs, 1 s)").message
    e = both_error("xs = [1, 2] [m]\nprint min(xs, [1, 2, 3] [m])")
    assert "different lengths (2 and 3)" in e.message
    assert both("print max(2, 3), min([3, 1, 2])") == "3 1"         # the old forms are unchanged


def test_68_built_executable_matches(tmp_path):
    src = "xs = [1e-20, 0.5, 1e-3]\nprint max(xs, 1e-12), min(xs, 0.1)"
    got = built(src, tmp_path)
    assert got.returncode == 0 and got.stdout.strip() == run(src)


# ---------------------------------------------------------------- #85: a unit used as a value (D163)
@pytest.mark.parametrize("src,shown", [
    ("rate = cm³/(mol s)", "cm³/(mol s)"),
    ("v = km/s\nprint v", "km/s"),
    ("k = N/m", "N/m"),
])
def test_69_a_whole_unit_as_a_value_suggests_one_of_it(src, shown):
    e = error_of(src)
    assert e.message == f"{shown} is a unit, not a value"
    assert f"1 {shown}" in e.hint


def test_69_other_unit_hints_are_unchanged():
    assert "like 1 m" in error_of("x = m").hint
    assert "x * 1 km" in error_of("x = 3\nprint x km").hint
    assert both("rate = 1 cm³/(mol s)\nprint rate") == "1 cm³/(mol s)"
    # a unit next to a variable of the same name is not a bare unit
    assert "s isn't defined" in error_of("m = 2 kg\nprint m/s").message


# ---------------------------------------------------------------- #86: a unit read in place of your variable (D164)
# Since the A1 rule (D235) the collision itself is the error, before any unit mismatch it would cause.
@pytest.mark.parametrize("src,shown", [
    ("T = 2 MeV\nE = 4/3 T + 1 MeV", "3 T"),
    ("T = 2 MeV\nx = π²/15 T⁴ + (1 J)⁴", "15 T⁴"),
    ("T = 2 MeV\nρ(T) = (4/3) T + 4/3 T\nprint ρ(T)", "3 T"),
])
def test_70_the_collision_is_the_message(src, shown):
    e = error_of(src)
    assert e.message.startswith(f"'{shown}' is ambiguous: right after a number, T is a unit (tesla), but T is also")


def test_70_the_fix_works():
    assert both("T = 2 MeV\nE = 4/3 * T + 1 MeV\nprint E to 4 digits") == "3.667 MeV"


def test_70_no_collision_no_change():
    e = error_of("x = 2 m\ny = 3 s\nprint x + y")
    assert e.message.startswith("can't add") and "read as" not in e.message
    assert both("T = 2 MeV\nprint (4/3) T to 4 digits") == "2.667 MeV"
