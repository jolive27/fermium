"""M6 Python interop (DECISIONS D140-D142).

1. `use python numpy as np`: calling Python from Fermium, with the units checked at the boundary.
2. `fermium.compile(src)` / `fermium.load(path)`: calling compiled Fermium functions from Python.
"""
import io
import math
import os
import textwrap
import time

import numpy as np
import pytest
import scipy.special as sp

from conftest import run, error_of
import fermium
from fermium import Q
from fermium.errors import FermiumError, FermiumRuntimeError
from fermium.interp import run_interpreted

HELPER = '''
import numpy as np

def energy(m, v):
    return 0.5 * m * v ** 2

def fall(t):
    return 4.9 * np.asarray(t) ** 2

def total(xs):
    return float(np.sum(xs))

def grid(a, b, n):
    return np.linspace(a, b, n)

def fails(x):
    raise ValueError("x must be positive")

def gives_none(x):
    return None

def gives_matrix(x):
    return np.eye(2)

def gives_text(x):
    return "hello"

def gives_complex(x):
    return 1 + 2j

def kind(n):
    return 1.0 if isinstance(n, int) else 0.0
'''


@pytest.fixture
def helper_dir(tmp_path):
    (tmp_path / "physhelp.py").write_text(HELPER)
    return str(tmp_path)


def both(src, base_dir=None):
    """Output of the JIT and of the reference interpreter (they must agree)."""
    native = run(src, base_dir=base_dir)
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out, base_dir=base_dir)
    assert out.getvalue().strip() == native
    return native


def errors_agree(src, base_dir=None):
    """Both back ends stop with the same error; returns it."""
    with pytest.raises(FermiumError) as e1:
        run(src, base_dir=base_dir)
    with pytest.raises(FermiumError) as e2:
        run_interpreted(src, "<test>", out=io.StringIO(), base_dir=base_dir)
    assert e1.value.message == e2.value.message and e1.value.line == e2.value.line
    return e1.value


# ------------------------------------------------------------------ 1. Python from Fermium
def test_numpy_and_scipy_calls_match_python():
    out = both(textwrap.dedent("""\
        use python numpy as np
        use python scipy.special as sp
        r = 2.5 m
        print np.sinc(0.25) to 12 digits
        print sp.jv(0, r / (1 m)) to 12 digits
        print sp.gamma(4.5) to 12 digits
        print np.pi to 12 digits
        print np.sqrt(2) to 12 digits
        """)).split("\n")
    assert float(out[0]) == pytest.approx(np.sinc(0.25), rel=1e-11)
    assert float(out[1]) == pytest.approx(sp.jv(0, 2.5), rel=1e-11)
    assert float(out[2]) == pytest.approx(sp.gamma(4.5), rel=1e-11)
    assert float(out[3]) == pytest.approx(math.pi, rel=1e-11)
    assert float(out[4]) == pytest.approx(math.sqrt(2), rel=1e-11)


def test_dimensionful_argument_is_a_compile_error_that_says_how_to_fix_it():
    e = errors_agree("use python scipy.special as sp\nr = 2 m\nprint sp.jv(0, r)")
    assert e.line == 3
    assert "sp.jv is a Python function, which takes plain numbers, but argument 2 is length [m]" in e.message
    assert "divide by a unit, like  r / (1 m)" in e.hint and "jv(x1, x2 [m])" in e.hint
    e = error_of("use python numpy as np\nprint np.sqrt(4 m²)")
    assert "area" in e.message and "(1 m²)" in e.hint


def test_unit_error_inside_a_generic_function_points_at_the_call():
    e = error_of("use python numpy as np\nf(x) = np.sin(x)\nprint f(2 m)")
    assert "np.sin is a Python function" in e.message and "length" in e.message
    assert "this happened when calling f on line 3" in e.hint
    assert both("use python numpy as np\nf(x) = np.sin(x) + 1\nprint f(0.5) to 9 digits") == \
        f"{math.sin(0.5) + 1:.9g}"


def test_declared_signature_converts_units_both_ways(helper_dir):
    src = textwrap.dedent("""\
        use python physhelp as ph:
            energy(m [kg], v [km/s]) -> [J]
            fall(t [s]) -> [m]
        E = ph.energy(2 kg, 3000 m/s)
        print E
        print E in kJ to 4 digits
        print ph.energy(2000 g, 3 km/s) + 1 J
        print ph.fall([1, 2, 3] [s])
        print ph.fall(1 s) in cm to 4 digits
        """)
    out = both(src, helper_dir).split("\n")
    # v is passed in km/s (3), so Python computes 0.5 * 2 * 3² = 9, read as joules
    assert out == ["9 J", "0.009000 kJ", "10 J", "[4.90, 19.6, 44.1] m", "490.0 cm"]


def test_declared_signature_checks_the_arguments(helper_dir):
    head = "use python physhelp as ph:\n    energy(m [kg], v [m/s]) -> [J]\n"
    e = errors_agree(head + "print ph.energy(2 kg, 3 m)", helper_dir)
    assert e.message == "ph.energy expects v in m/s (declared on line 1), but got length [m]"
    e = errors_agree(head + "print ph.energy(2 kg)", helper_dir)
    assert "ph.energy takes 2 arguments (as declared in the use line on line 1), but got 1" in e.message
    # the result has the declared unit, so using it wrongly is a unit error too
    e = error_of(head + "x = ph.energy(2 kg, 3 m/s) + 1 m", helper_dir)
    assert "energy" in e.message and "length" in e.message


def test_lists_go_in_as_numpy_arrays_and_come_back(helper_dir):
    src = textwrap.dedent("""\
        use python numpy as np:
            sum(xs) -> number
            linspace(a, b, n: int) -> list
        use python physhelp as ph:
            total(xs) -> number
            grid(a [km], b [km], n: int) -> list [km]
        xs = [0, 0.5, 1, 2]
        print np.sqrt(xs) to 6 digits
        print np.sum(xs), ph.total(xs)
        print np.linspace(0, 1, 5)
        print ph.grid(0 m, 2 km, 3)
        ys = np.exp(-xs)
        print len(ys), ys[4] to 6 digits
        """)
    out = both(src, helper_dir).split("\n")
    assert out[0] == "[0, 0.707107, 1.00000, 1.41421]"
    assert out[1] == "3.50 3.50"
    assert out[2] == "[0, 0.250, 0.500, 0.750, 1.00]"
    assert out[3] == "[0, 1, 2] km"
    assert out[4] == f"4 {math.exp(-2):.6f}"


def test_int_parameters_are_passed_as_python_ints(helper_dir):
    src = "use python physhelp as ph:\n    kind(n: int) -> number\nuse python physhelp as ph2\n" \
          "print ph.kind(3), ph2.kind(3)"
    assert both(src, helper_dir) == "1 0"
    e = errors_agree("use python physhelp as ph:\n    kind(n: int) -> number\nprint ph.kind(2.5)", helper_dir)
    assert "ph.kind: n must be a whole number (it is passed as an int), not 2.5" in e.message
    e = errors_agree("use python numpy as np\nprint np.linspace(0, 1, 3)")
    assert "TypeError" in e.message and "n: int" in e.message


def test_python_errors_and_bad_results_are_runtime_errors_with_the_line(helper_dir):
    cases = {
        "ph.fails(1)": "the Python function ph.fails failed: ValueError: x must be positive",
        "ph.gives_none(1)": "the Python function ph.gives_none returned nothing (None), but Fermium needs a number",
        "ph.gives_matrix(1)": "ph.gives_matrix returned an array of shape (2, 2), but Fermium expected a number here",
        "ph.gives_text(1)": "ph.gives_text returned the text 'hello', but Fermium expected a number here",
        "ph.gives_complex(1)": "ph.gives_complex returned a complex number, but Fermium expected a real number here",
        "ph.total([1, 2])": "ph.total returned a single number, but Fermium expected a list here",
    }
    for call, msg in cases.items():
        e = errors_agree(f"use python physhelp as ph\nx = 1\nprint {call}", helper_dir)
        assert isinstance(e, FermiumRuntimeError)
        assert msg in e.message and e.line == 3, (call, e.message)


def test_compile_time_errors_for_modules_and_names():
    e = errors_agree("use python numpy as np\nprint np.sincc(1)")
    assert e.message == "the Python module numpy has no sincc" and e.hint == "did you mean np.sinc?"
    e = errors_agree("use python numpyy as np\nprint 1")
    assert e.message == "can't find the Python module numpyy" and "pip install numpyy" in e.hint
    e = error_of("use python numpy as np\nprint np.pi(2)")
    assert "np.pi is a number in Python, so it can't be called" in e.message
    e = error_of("use python numpy as np\nprint np")
    assert "np is the Python module numpy, not a value" in e.message
    e = error_of("use python numpy as np\nnp = 3")
    assert "np is the Python module numpy (line 1) and is defined again on line 2" in e.message
    e = error_of("if 1 < 2\n    use python numpy as np")
    assert "use python must be at the top level" in e.message
    e = error_of("use python scipy.special\nprint 1")
    assert "give the Python module scipy.special a short name with as" in e.message
    e = error_of("use python numpy as np:\n    sqrt(x [°C]) -> number\nprint 1")
    assert "offset" in e.message
    e = error_of("use python numpy as np\nprint d/dx np.sin(x) where x = 1")
    assert "can't differentiate np.sin(...) symbolically" in e.message and "finite difference" in e.hint
    e = error_of("use python numpy as np\nprint np.sqrt(vec(1, 2))")
    assert "np.sqrt: a Python function takes numbers and lists of numbers, but argument 1 is a vector" in e.message


def test_python_calls_in_integrals_odes_and_loops():
    src = textwrap.dedent("""\
        use python numpy as np
        use python scipy.special as sp
        print ∫ sp.erf(x) dx from 0 to 1 to 10 digits
        solve y' = -np.tanh(y) / 1 s with y(0) = 1 for t from 0 s to 2 s
        print y(2 s) to 6 digits
        s = 0
        for k from 1 to 1000
            s += np.cos(k / 1000)
        print s to 10 digits
        """)
    out = both(src).split("\n")
    from scipy.integrate import quad, solve_ivp
    assert float(out[0]) == pytest.approx(quad(sp.erf, 0, 1)[0], rel=1e-9)
    sol = solve_ivp(lambda t, y: -np.tanh(y), (0, 2), [1.0], rtol=1e-11, atol=1e-13)
    assert float(out[1]) == pytest.approx(sol.y[0, -1], rel=1e-5)
    assert float(out[2]) == pytest.approx(sum(math.cos(k / 1000) for k in range(1, 1001)), rel=1e-9)


def test_fermium_build_refuses_python_calls(tmp_path):
    from fermium.aot import build
    src = "use python numpy as np\nx = 2\nprint np.sqrt(x)\n"
    with pytest.raises(FermiumError) as ei:
        build(src, str(tmp_path / "p.fm"), str(tmp_path / "p"))
    assert ei.value.line == 3
    assert ei.value.message == ("fermium build can't compile a call into Python (np.sqrt): an executable doesn't "
                                "carry Python; use  fermium run  for this program")


def test_repl_keeps_the_python_module():
    from fermium.driver import ReplSession
    out = io.StringIO()
    s = ReplSession(out=out)
    s.execute("use python numpy as np\n")
    s.execute("x = np.sqrt(16)\n")
    s.execute("print x + np.sqrt(9)\n")
    assert out.getvalue().strip() == "7"
    s.execute("use python numpy as np:\n    hypot(a [m], b [m]) -> [m]\n")     # declared again, with units
    s.execute("print np.hypot(3 m, 4 m)\n")
    assert out.getvalue().strip().split("\n")[-1] == "5 m"


def test_fmt_keeps_python_names_after_the_dot():
    from fermium.fmt import format_source
    src = "use python scipy.special as sp\nprint sp.gamma(2) + sp.sqrt(4) + sp.pi\n"
    pretty = format_source(src, "pretty")
    assert "sp.gamma(2)" in pretty and "sp.sqrt(4)" in pretty and "sp.pi" in pretty
    assert format_source(pretty, "ascii") == src
    assert both("use python scipy.special as sp\nprint sp.γ(5)") == "24"      # an older --pretty spelling


# ------------------------------------------------------------------ 2. Fermium from Python
LIB = """\
g = 9.81 m/s²
print "top level ran"
period(L [m]) = 2π √(L / g)
double(x) = 2 x
total(ys) = sum(ys)
squares(ys) = ys²
v = vec(1, 2, 3) m/s
xs = [1, 2, 3] [m]
leibniz(n) =
    s = 0
    for k from 0 to n
        s += (-1)^k / (2 k + 1)
    return 4*s
"""


def test_calling_a_compiled_function_from_python():
    out = io.StringIO()
    mod = fermium.compile(LIB, out=out)
    assert mod.functions == ["double", "leibniz", "period", "squares", "total"]
    assert out.getvalue() == ""                      # the top level runs on first use
    T = mod.period(1.0)                              # a plain float is SI, with the declared dimension (m)
    assert out.getvalue() == "top level ran\n"
    assert isinstance(T, float) and isinstance(T, fermium.Quantity)
    assert T == pytest.approx(2 * math.pi * math.sqrt(1 / 9.81), rel=1e-14)
    assert T.unit == "s" and str(T) == "2.00607 s"
    assert mod.period(Q(50, "cm")) == pytest.approx(2 * math.pi * math.sqrt(0.5 / 9.81), rel=1e-14)
    assert mod.period(Q(1, "ft")) == pytest.approx(2 * math.pi * math.sqrt(0.3048 / 9.81), rel=1e-14)
    assert out.getvalue() == "top level ran\n"       # only once


def test_generic_functions_are_instantiated_from_the_argument_units():
    mod = fermium.compile(LIB, out=io.StringIO())
    a = mod.double(3)
    assert a == 6 and a.unit == ""
    b = mod.double(Q(3, "km"))
    assert b == 6000 and b.unit == "m" and b.to("km") == 6
    c = mod.double(Q(2, "N"))
    assert c == 4 and c.unit == "N"
    assert mod.double(4) == 8                        # the cached instance again


def test_unit_errors_are_compile_errors_raised_in_python():
    mod = fermium.compile(LIB, out=io.StringIO())
    with pytest.raises(FermiumError) as ei:
        mod.period(Q(1, "s"))
    assert ei.value.message == ("calling period from Python with (time [s]): period expects L in m (length [m]), "
                                "but got time [s]")
    with pytest.raises(TypeError, match="period takes 1 argument"):
        mod.period(1, 2)
    with pytest.raises(TypeError, match="bool"):
        mod.double(True)
    with pytest.raises(FermiumError):
        fermium.compile("x = 1 m + 1 s")


def test_lists_vectors_and_variables():
    mod = fermium.compile(LIB, out=io.StringIO())
    assert mod.total([1, 2, 3.5]) == 6.5
    xs = mod["xs"]
    assert isinstance(xs, fermium.QuantityArray) and xs.unit == "m" and list(xs) == [1, 2, 3]
    assert mod.total(xs) == 6 and mod.total(xs).unit == "m"
    sq = mod.squares(np.array([1.0, 2.0, 3.0]))
    assert list(sq) == [1, 4, 9] and sq.unit == ""
    sq = mod.squares(xs)
    assert list(sq) == [1, 4, 9] and sq.unit == "m²"
    assert list(mod.squares([])) == []
    assert (xs * 2).__class__ is np.ndarray          # Python arithmetic drops the unit (never a wrong one)
    assert mod.g == pytest.approx(9.81) and mod.g.unit == "m/s²"
    assert mod.g.to("ft/s^2") == pytest.approx(9.81 / 0.3048)
    v = mod["v"]
    assert list(v) == [1, 2, 3] and v.unit == "m/s"
    assert sorted(mod.variables) == ["g", "v", "xs"]
    with pytest.raises(KeyError):
        mod["nope"]
    with pytest.raises(AttributeError):
        mod.nope


def test_values_agree_with_a_python_implementation():
    src = textwrap.dedent("""\
        # a damped oscillator's energy, and the semi-empirical mass formula
        energy(x [m], v [m/s], m [kg], k [N/m]) = ½ m v² + ½ k x²
        semf(Z, A) = 15.75 MeV * A - 17.8 MeV * A^(2/3) - 0.711 MeV * Z (Z - 1) / A^(1/3) - 23.7 MeV * (A - 2 Z)² / A
        """)
    mod = fermium.compile(src, out=io.StringIO())
    rng = np.random.default_rng(1)
    for _ in range(20):
        x, v, m, k = rng.uniform(0.1, 3, 4)
        assert mod.energy(x, v, m, k) == pytest.approx(0.5 * m * v * v + 0.5 * k * x * x, rel=1e-14)
    MeV = 1.602176634e-13
    for Z, A in [(26, 56), (82, 208), (8, 16), (92, 238)]:
        py = 15.75 * A - 17.8 * A ** (2 / 3) - 0.711 * Z * (Z - 1) / A ** (1 / 3) - 23.7 * (A - 2 * Z) ** 2 / A
        r = mod.semf(Z, A)
        assert r.unit in ("J", "MeV") and r.to("MeV") == pytest.approx(py, rel=1e-12)
        assert float(r) == pytest.approx(py * MeV, rel=1e-12)


def test_runtime_errors_and_load_from_a_file(tmp_path):
    p = tmp_path / "lib.fm"
    p.write_text("inv(x) = [1, 2][x]\nhalf(x) = x / 2\n")
    mod = fermium.load(str(p), out=io.StringIO())
    assert mod.half(Q(3, "s")) == 1.5
    with pytest.raises(FermiumRuntimeError, match="out of range"):
        mod.inv(5)
    assert mod.half(1) == 0.5                        # still usable after an error
    assert "lib.fm" in repr(mod)


def test_runaway_recursion_called_from_python_is_a_clean_error():
    mod = fermium.compile("f(x) = x * f(x - 1)\n", out=io.StringIO())
    with pytest.raises(FermiumRuntimeError):
        mod.f(3)
    assert fermium.compile("sq(x) = x²", out=io.StringIO()).sq(3) == 9


def test_run_explicitly_and_output():
    out = io.StringIO()
    mod = fermium.compile('x = 2 m\nprint "x is", x\n', out=out).run()
    assert out.getvalue() == "x is 2 m\n"
    assert mod.x == 2 and mod.x.unit == "m"
    assert out.getvalue() == "x is 2 m\n"


def test_calling_fermium_that_calls_python(tmp_path):
    (tmp_path / "physhelp.py").write_text(HELPER)
    mod = fermium.compile("use python physhelp as ph:\n    energy(m [kg], v [m/s]) -> [J]\n"
                          "ke(m, v) = ph.energy(m, v)\n", base_dir=str(tmp_path), out=io.StringIO())
    assert mod.ke(Q(2, "kg"), Q(3, "m/s")) == 9.0 and mod.ke(Q(2, "kg"), Q(3, "m/s")).unit == "J"


def test_compiled_function_is_much_faster_than_python():
    """Speed sanity (not a benchmark: the machine is shared).  Reports the measured ratio."""
    mod = fermium.compile(LIB, out=io.StringIO())
    n = 2_000_000
    mod.leibniz(10)                                  # compile this instance first
    t0 = time.perf_counter()
    r = mod.leibniz(n)
    t_fm = time.perf_counter() - t0

    def leibniz(n):
        s = 0.0
        for k in range(n + 1):
            s += (-1) ** k / (2 * k + 1)
        return 4 * s
    t0 = time.perf_counter()
    r_py = leibniz(n)
    t_py = time.perf_counter() - t0
    assert r == pytest.approx(r_py, rel=1e-12)
    print(f"\nleibniz({n}): Fermium {t_fm * 1e3:.1f} ms, pure Python {t_py * 1e3:.1f} ms, "
          f"ratio {t_py / t_fm:.0f}x")
    assert t_fm < t_py


def test_quantity_helpers():
    q = Q(50, "cm")
    assert float(q) == 0.5 and q.unit == "cm" and q.value == 50 and q.to("m") == 0.5
    assert repr(q) == "Quantity(50.0, 'cm')" and str(q) == "50 cm"
    assert q * 2 == 1.0 and type(q * 2) is float
    with pytest.raises(ValueError):
        q.to("s")
    with pytest.raises(ValueError):
        Q(1, "furlongz")
    t = Q(20, "°C")
    assert float(t) == pytest.approx(293.15) and t.to("K") == pytest.approx(293.15)
    a = fermium.QuantityArray([1, 2], "km")
    assert list(a.to("m")) == [1000, 2000] and a.unit == "km"
    assert os.path.basename(fermium.__file__) == "__init__.py"


def test_the_reference_python_example_runs_and_prints_what_it_says(capsys):
    import re
    ref = open(os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "docs", "reference.md"),
               encoding="utf-8").read()
    section = ref.split("## Python interop", 1)[1].split("\n## ", 1)[0]
    blocks = re.findall(r"```python\n(.*?)```", section, re.S)
    assert len(blocks) == 1
    exec(compile(blocks[0], "reference.md", "exec"), {})
    lines = capsys.readouterr().out.strip().split("\n")
    assert lines == ["2.00607 s s", f"{2 * math.pi * math.sqrt(0.5 / 9.81)!r}", "6000 m", "9.81 m/s²"]
