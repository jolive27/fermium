"""Tests of fermium2 (calling Fermium 2 from Python, D142): the tests of Fermium 1.5's fermium/api.py
(tests/test_python_interop.py part 2, and the API cases of tests/test_redteam3.py and test_redteam4.py), run
against the Rust library.  Plain asserts: `python3 test_fermium2.py` runs them all (so does pytest).
The Rust test crates/fermium-pyapi/tests/python.rs runs this file with FERMIUM_PYAPI_LIB set.
"""
import io
import math
import os
import sys
import tempfile
import time

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fermium2 as fermium                      # noqa: E402
from fermium2 import Q, FermiumError, FermiumRuntimeError   # noqa: E402


def approx(a, b, rel=1e-12):
    return abs(a - b) <= rel * max(abs(a), abs(b), 1e-300)


def raises(exc, fn, match=None):
    try:
        fn()
    except exc as e:
        if match is not None:
            assert match in str(e), (match, str(e))
        return e
    raise AssertionError(f"{exc.__name__} not raised")


HELPER = '''
import numpy as np

def energy(m, v):
    return 0.5 * m * v ** 2
'''

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
    assert approx(T, 2 * math.pi * math.sqrt(1 / 9.81), rel=1e-14)
    assert T.unit == "s" and str(T) == "2.00607 s"
    assert approx(mod.period(Q(50, "cm")), 2 * math.pi * math.sqrt(0.5 / 9.81), rel=1e-14)
    assert approx(mod.period(Q(1, "ft")), 2 * math.pi * math.sqrt(0.3048 / 9.81), rel=1e-14)
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
    e = raises(FermiumError, lambda: mod.period(Q(1, "s")))
    assert e.message == ("calling period from Python with (time [s]): period expects L in m (length [m]), "
                         "but got time [s]"), e.message
    raises(TypeError, lambda: mod.period(1, 2), "period takes 1 argument")
    raises(TypeError, lambda: mod.double(True), "bool")
    raises(FermiumError, lambda: fermium.compile("x = 1 m + 1 s"))
    assert mod.period(2.0) > 0                       # still usable after a failed call signature


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
    assert approx(mod.g, 9.81) and mod.g.unit == "m/s²"
    assert approx(mod.g.to("ft/s^2"), 9.81 / 0.3048)
    v = mod["v"]
    assert list(v) == [1, 2, 3] and v.unit == "m/s"
    assert sorted(mod.variables) == ["g", "v", "xs"]
    raises(KeyError, lambda: mod["nope"])
    raises(AttributeError, lambda: mod.nope)


def test_values_agree_with_a_python_implementation():
    src = ("energy(x [m], v [m/s], m [kg], k [N/m]) = ½ m v² + ½ k x²\n"
           "semf(Z, A) = 15.75 MeV * A - 17.8 MeV * A^(2/3) - 0.711 MeV * Z (Z - 1) / A^(1/3) - 23.7 MeV * "
           "(A - 2 Z)² / A\n")
    mod = fermium.compile(src, out=io.StringIO())
    rng = np.random.default_rng(1)
    for _ in range(20):
        x, v, m, k = rng.uniform(0.1, 3, 4)
        assert approx(mod.energy(x, v, m, k), 0.5 * m * v * v + 0.5 * k * x * x, rel=1e-14)
    MeV = 1.602176634e-13
    for Z, A in [(26, 56), (82, 208), (8, 16), (92, 238)]:
        py = 15.75 * A - 17.8 * A ** (2 / 3) - 0.711 * Z * (Z - 1) / A ** (1 / 3) - 23.7 * (A - 2 * Z) ** 2 / A
        r = mod.semf(Z, A)
        assert r.unit in ("J", "MeV") and approx(r.to("MeV"), py, rel=1e-12)
        assert approx(float(r), py * MeV, rel=1e-12)


def test_runtime_errors_and_load_from_a_file():
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "lib.fm")
        with open(p, "w") as fh:
            fh.write("inv(x) = [1, 2][x]\nhalf(x) = x / 2\n")
        mod = fermium.load(p, out=io.StringIO())
        assert mod.half(Q(3, "s")) == 1.5
        raises(FermiumRuntimeError, lambda: mod.inv(5), "out of range")
        assert mod.half(1) == 0.5                    # still usable after an error
        assert "lib.fm" in repr(mod)


def test_runaway_recursion_called_from_python_is_a_clean_error():
    mod = fermium.compile("f(x) = x * f(x - 1)\n", out=io.StringIO())
    raises(FermiumRuntimeError, lambda: mod.f(3))
    assert fermium.compile("sq(x) = x²", out=io.StringIO()).sq(3) == 9


def test_run_explicitly_and_output():
    out = io.StringIO()
    mod = fermium.compile('x = 2 m\nprint "x is", x\n', out=out).run()
    assert out.getvalue() == "x is 2 m\n"
    assert mod.x == 2 and mod.x.unit == "m"
    assert out.getvalue() == "x is 2 m\n"


def test_calling_fermium_that_calls_python():
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "physhelp.py"), "w") as fh:
            fh.write(HELPER)
        mod = fermium.compile("use python physhelp as ph:\n    energy(m [kg], v [m/s]) -> [J]\n"
                              "ke(m, v) = ph.energy(m, v)\n", base_dir=d, out=io.StringIO())
        r = mod.ke(Q(2, "kg"), Q(3, "m/s"))
        assert r == 9.0 and r.unit == "J"


def test_compiled_function_speed_is_reported():
    """Speed report, not an assertion: fermium2 runs programs on the tree-walker, which is about as fast as pure
    Python on this loop; v1's API compiled each call with LLVM (~10x faster).  See rust/DIVERGENCES.md."""
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
    assert approx(r, r_py, rel=1e-12)
    print(f"  leibniz({n}): Fermium 2 (tree-walker) {t_fm * 1e3:.1f} ms, pure Python {t_py * 1e3:.1f} ms, "
          f"ratio {t_py / t_fm:.1f}x")


def test_quantity_helpers():
    q = Q(50, "cm")
    assert float(q) == 0.5 and q.unit == "cm" and q.value == 50 and q.to("m") == 0.5
    assert repr(q) == "Quantity(50.0, 'cm')" and str(q) == "50 cm"
    assert q * 2 == 1.0 and type(q * 2) is float
    raises(ValueError, lambda: q.to("s"))
    raises(ValueError, lambda: Q(1, "furlongz"))
    t = Q(20, "°C")
    assert approx(float(t), 293.15) and approx(t.to("K"), 293.15)
    a = fermium.QuantityArray([1, 2], "km")
    assert list(a.to("m")) == [1000, 2000] and a.unit == "km"


def test_compiled_complex_results():
    mod = fermium.compile("z = 3 + 4i\nf(x) = x + 1i\nw(x [Ω]) = x + 2i Ω\nv = <3, 4>\n")
    z = mod["z"]
    assert isinstance(z, complex) and z == 3 + 4j and abs(z) == 5.0 and z.unit == ""
    assert mod.f(2) == 2 + 1j
    w = mod.w(3)
    assert w == 3 + 2j and w.unit == "Ω" and str(w) == "(3 + 2i) Ω"
    assert abs(w.to("mΩ") - (3000 + 2000j)) < 1e-9
    assert list(mod["v"]) == [3.0, 4.0]                   # vectors are still arrays


def test_python_celsius_to_a_delta_parameter_is_refused():
    mod = fermium.compile("heat(m [kg], ΔT [K]) = m 4186 J/(kg K) ΔT\nP(T [K]) = 1 mol R_gas T / (1 m³)\n")
    assert approx(float(mod.heat(1, Q(10, "K"))), 41860)
    assert approx(float(mod.P(Q(20, "°C"))), 8.314462618 * 293.15, rel=1e-6)
    e = raises(FermiumError, lambda: mod.heat(1, Q(10, "°C")))
    assert "temperature change" in e.message and "283.15 K" in e.message


def test_hz_parameter_given_rpm_warns():
    m = fermium.compile("f(freq [Hz]) = freq\ng(w [rpm]) = w\n", warnings=False)
    assert approx(float(m.f(Q(60, "rpm"))), 2 * math.pi, rel=1e-12)
    assert any("Hz" in w and "rpm" in w for w in m.warnings)
    n = len(m.warnings)
    m.f(Q(60, "Hz"))                                       # Hz for Hz: nothing to say
    assert len(m.warnings) == n
    m.g(Q(1, "Hz"))
    assert any("9.5493 rpm, not 60 rpm" in w for w in m.warnings[n:]), m.warnings[n:]


def test_compile_errors_carry_the_line_and_hint():
    e = raises(FermiumError, lambda: fermium.compile("x = 2 m\ny = x + 3 s\n"))
    assert e.line == 2 and "can't add" in e.message and e.hint


if __name__ == "__main__":
    failed = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok     {name}")
            except Exception as ex:                 # report every failure, then exit non-zero
                failed += 1
                import traceback
                print(f"FAILED {name}: {type(ex).__name__}: {ex}")
                traceback.print_exc()
    print("all passed" if not failed else f"{failed} failed")
    sys.exit(1 if failed else 0)
