"""Vectors: <3, 4> m/s, |v|, dot (· or *), cross (×), components, vector ODEs."""
import math

import pytest

from conftest import run, error_of
from numparse import nums


def test_vector_literal_and_norm():
    assert run("v = <3, 4> m/s\nprint v\nprint |v|\nprint v.x, v.y, v[2]") == \
        "<3, 4> m/s\n5 m/s\n3 m/s 4 m/s 4 m/s"


def test_vector_of_quantities():
    assert run("w = <1 m, 2 m, 3 m>\nprint w in cm") == "<100, 200, 300> cm"


def test_dot_and_cross():
    out = run("a = <1, 2, 3> m\nb = <4, 5, 6> N\nprint a · b\nprint a × b\nprint cross(a, b)\nprint dot(a, b)")
    assert out.split("\n") == ["32 J", "<-3, 6, -3> J", "<-3, 6, -3> J", "32 J"]


def test_2d_cross_is_a_number():
    assert run("print <1, 0> m × <0, 2> N") == "2 J"


def test_scaling_and_sums():
    out = run("v = <3, 4> m/s\nm = 2 kg\np = m v\nprint p\nprint v + <1, 1> m/s\nprint -v / 2\nprint unit(v)")
    assert out.split("\n") == ["<6, 8> kg m/s", "<4, 5> m/s", "<-1.50, -2.00> m/s", "<0.600, 0.800>"]


def test_vector_functions():
    out = run("F(r) = -k r where k = 10 N/m\nprint F(<0.1, 0.2> m)\nKE(m, v) = ½ m (v · v)\nprint KE(2 kg, <3, 4> m/s)")
    assert out.split("\n") == ["<-1.0, -2.0> N", "25 J"]   # 0.1 has 1 s.f. -> 2 shown


@pytest.mark.parametrize("src,phrase", [
    ("print <1, 2> m + <1, 2> s", "can't add vectors of length [m] and time [s]"),
    ("print <1, 2> m + 3 m", "can't add a vector and a single number"),
    ("print <1, 2> + <1, 2, 3>", "2-vector and a 3-vector"),
    ("print |<1 m, 2 s>|", "needs all components of the vector in the same units"),
    ("print 3 m / <1, 2>", "can't divide by a vector"),
    ("v = <1, 2>\nprint v.z", "no component 3"),
    ("v = <1, 2>\nprint v.w", "components are .x, .y and .z"),
    ("print sin(<1, 2>)", "must be a number"),
    ("v = <1, 2> m\nv = <1, 2, 3> m", "can't now hold a 3-vector"),
])
def test_vector_errors(src, phrase):
    assert phrase in str(error_of(src))


def test_vector_ode_kepler_orbit():
    """A vector ODE: circular orbit returns to its start after one period (to 1e-6 AU)."""
    src = ("GM = G M_sun\nr0 = 1 AU\nv0 = √(GM / r0)\nT = 2π r0 / v0\n"
           "solve r'' = -GM r / |r|³\n  with r(0) = <1, 0> AU, r'(0) = <0 m/s, v0>\n  for t from 0 s to T\n"
           "print r.x(T) / (1 AU) to 12 digits\nprint r.y(T) / (1 AU) to 12 digits\n"
           "print |r'(T/2)| / v0 to 12 digits")
    x, y, s = nums(run(src))
    assert x == pytest.approx(1.0, abs=1e-6)
    assert y == pytest.approx(0.0, abs=1e-6)
    assert s == pytest.approx(1.0, abs=1e-6)


def test_vector_ode_energy_conserved_and_plot(tmp_path):
    src = ("GM = G M_sun\nsolve r'' = -GM r / |r|³\n  with r(0) = <1, 0> AU, r'(0) = <0, 35> km/s\n"
           "  for t from 0 s to 2 yr\n"
           "E(t) = ½ |r'(t)|² - GM/|r(t)|\n"
           "print (E(2 yr) - E(0 s)) / E(0 s) to 3 digits\n"
           "plot r.y in AU vs r.x in AU to \"orbit.png\"")
    out = run(src, base_dir=str(tmp_path)).split("\n")
    assert abs(nums(out[0])[0]) < 1e-7
    assert (tmp_path / "orbit.png").exists()


def test_vector_derivative_of_function():
    out = run("ω = 2 rad/s\nr(t) = <cos(ω t), sin(ω t)> m\nv = r'\nprint v(0 s)\nprint |v(1 s)|")
    assert out.split("\n") == ["<0, 2> m/s", "2 m/s"]


def test_vectors_in_repl_arena():
    import io
    from fermium.repl import main
    out = io.StringIO()
    main(stdin=io.StringIO("v = <3, 4> m/s\nw = v * 2\nprint |w|\n"), stdout=out)
    assert out.getvalue().strip() == "10 m/s"


def test_math_consistency():
    a = (0.3, -1.2, 2.5)
    b = (1.7, 0.4, -0.9)
    out = nums(run(f"a = <{a[0]}, {a[1]}, {a[2]}>\nb = <{b[0]}, {b[1]}, {b[2]}>\n"
                   "c = a × b\nprint c · a to 12 digits\nprint |c| to 12 digits"))
    assert out[0] == pytest.approx(0.0, abs=1e-12)
    na, nb = math.sqrt(sum(x * x for x in a)), math.sqrt(sum(x * x for x in b))
    dot = sum(x * y for x, y in zip(a, b))
    assert out[1] == pytest.approx(na * nb * math.sqrt(1 - (dot / (na * nb)) ** 2), rel=1e-10)
