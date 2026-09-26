"""Spec 1.5 §A5.1 (D243): fft(xs) returns a list of complex numbers; fft_re/fft_im/ifft(re, im) are deprecated
aliases for one version.  Checked against numpy.fft, in the compiled path, the interpreter and `fermium build`."""
import io
import subprocess

import pytest

from conftest import run, warnings_of
from fermium.errors import FermiumError
from fermium.interp import run_interpreted
from numparse import num

np = pytest.importorskip("numpy")


def interp(src):
    out = io.StringIO()
    run_interpreted(src, "<test>", out=out)
    return out.getvalue().strip()


def both(src):
    a = run(src)
    assert interp(src) == a
    return a


def floats(line):
    inner = line[line.index("[") + 1:line.index("]")]
    return [num(x) for x in inner.split(",")] if inner.strip() else []


XS = [0.3, -1.2, 2.5, 0.7, -0.4, 1.9, -2.2, 0.05, 1.1, -0.8, 0.6]   # 11 samples: not a power of two


def lit(xs):
    return "[" + ", ".join(repr(float(x)) for x in xs) + "]"


@pytest.mark.parametrize("xs", [XS, XS[:8], XS[:1], XS[:2]], ids=["n11", "n8", "n1", "n2"])
def test_fft_matches_numpy(xs):
    src = (f"X = fft({lit(xs)})\nprint re(X) to 15 digits\nprint im(X) to 15 digits\n"
           f"print re(ifft(X)) to 15 digits\nprint im(ifft(X)) to 15 digits\nprint len(X)")
    re_, im_, back, back_im, n = both(src).splitlines()
    X = np.fft.fft(xs)
    assert floats(re_) == pytest.approx(list(X.real), abs=1e-12)
    assert floats(im_) == pytest.approx(list(X.imag), abs=1e-12)
    assert floats(back) == pytest.approx(xs, abs=1e-12)
    assert floats(back_im) == pytest.approx([0] * len(xs), abs=1e-12)
    assert n == str(len(xs))


def test_ifft_and_fft_of_complex_lists_match_numpy():
    re_ = [1.0, 2.0, -0.5, 0.25, 3.0, -1.0]
    im_ = [0.0, 0.5, 1.5, -2.0, 0.0, 0.7]
    z = np.array(re_) + 1j * np.array(im_)
    src = (f"Z = complex({lit(re_)}, {lit(im_)})\nW = ifft(Z)\nprint re(W) to 15 digits\nprint im(W) to 15 digits\n"
           f"V = fft(Z)\nprint re(V) to 15 digits\nprint im(V) to 15 digits\nU = ifft({lit(re_)})\n"
           f"print im(U) to 15 digits")
    a, b, c, d, e = both(src).splitlines()
    assert floats(a) == pytest.approx(list(np.fft.ifft(z).real), abs=1e-12)
    assert floats(b) == pytest.approx(list(np.fft.ifft(z).imag), abs=1e-12)
    assert floats(c) == pytest.approx(list(np.fft.fft(z).real), abs=1e-12)
    assert floats(d) == pytest.approx(list(np.fft.fft(z).imag), abs=1e-12)
    assert floats(e) == pytest.approx(list(np.fft.ifft(np.array(re_)).imag), abs=1e-12)


def test_elements_abs_arg_conj():
    xs = [1.0, 2.0, 3.0, 4.0]
    X = np.fft.fft(xs)
    src = ("X = fft([1, 2, 3, 4])\nprint X\nprint X[2]\nprint X[end]\nprint abs(X) to 15 digits\n"
           "print |X| to 15 digits\nprint arg(X) to 15 digits\nprint conj(X)\nprint X.re, X.im")
    out = both(src).splitlines()
    assert out[0] == "[10 + 0i, -2 + 2i, -2 + 0i, -2 - 2i]"
    assert out[1] == "-2 + 2i"
    assert out[2] == "-2 - 2i"
    assert floats(out[3]) == pytest.approx(list(np.abs(X)), rel=1e-13)
    assert out[4] == out[3]
    assert floats(out[5]) == pytest.approx(list(np.angle(X)), abs=1e-13)
    assert out[6] == "[10 + 0i, -2 - 2i, -2 + 0i, -2 + 2i]"
    assert out[7] == "[10, -2, -2, -2] [0, 2, 0, -2]"


def test_loop_over_a_complex_list():
    src = "X = fft([1, 2, 3, 4])\ntotal = 0i\nfor z in X\n    total = total + z\nprint total\nprint X[1] + X[3]"
    assert both(src).splitlines() == ["4 + 0i", "8 + 0i"]       # Σ X_k = n x_0


def test_units_follow_the_signal():
    out = both("xs = [1 V, 2 V, 3 V, 4 V]\nX = fft(xs)\nprint X\nprint X[2]\nprint abs(X)\nprint ifft(X)\n"
               "print arg(X)").splitlines()
    assert out[0] == "[10 + 0i, -2 + 2i, -2 + 0i, -2 - 2i] V"
    assert out[1] == "(-2 + 2i) V"
    assert out[2].endswith(" V")
    assert out[3] == "[1 + 0i, 2 + 0i, 3 + 0i, 4 + 0i] V"
    assert not out[4].endswith("V")                                # a phase is a plain number


def test_in_a_unit_and_to_n_digits():
    assert both("X = fft([1 V, 2 V])\nprint X in mV") == "[3000 + 0i, -1000 + 0i] mV"
    assert both("X = fft([1.5, 2, 3])\nprint X to 4 digits") == "[6.500 + 0i, -1.000 + 0.8660i, -1.000 - 0.8660i]"


def test_long_list_is_shortened_like_a_real_list():
    out = both("X = fft(linspace(1, 20, 20))\nprint X")
    assert "…" in out and out.endswith("(20 values)")
    assert out.startswith("[210 + 0i, -10.0 + 63.1i, ")


def test_a_real_signal_the_textbook_way():
    src = """dt = 1 ms
n = 1000
ts = linspace(0 s, (n - 1) dt, n)
xs = zeros(n)
for i from 1 to n
    xs[i] = 3 V * sin(2π * 50 Hz * ts[i])
X = fft(xs)
spec = abs(X) / n
print 2 spec[51] to 12 digits
"""
    assert num(both(src)) == pytest.approx(3.0, rel=1e-10)


def test_deprecated_aliases_still_work_and_warn():
    ws = warnings_of("xs = [1, 2, 3]\na = fft_re(xs)\nb = fft_im(xs)\nc = ifft(a, b)")
    assert len(ws) == 3
    assert any("fft_re(xs) is deprecated" in w and "re(fft(xs))" in w for w in ws)
    assert any("fft_im(xs) is deprecated" in w and "im(fft(xs))" in w for w in ws)
    assert any("ifft(re, im) is deprecated" in w and "ifft(complex(re, im))" in w for w in ws)
    # ... and give the same numbers as the new form
    assert both("xs = [1, 2, 3, 5]\nprint fft_re(xs), fft_im(xs)") == both("xs = [1, 2, 3, 5]\nX = fft(xs)\n"
                                                                              "print re(X), im(X)")
    assert warnings_of("X = fft([1, 2, 3])\nY = ifft(X)") == []


def test_errors():
    with pytest.raises(FermiumError, match="fft\\(xs\\): xs must be a list"):
        run("X = fft(3 m)")
    with pytest.raises(FermiumError, match="empty"):
        run("xs = zeros(0)\nprint fft(xs)")
    with pytest.raises(FermiumError, match="doesn't work on a list of complex numbers"):
        run("X = fft([1, 2])\nprint sum(X)")
    with pytest.raises(FermiumError, match="different lengths"):
        run("X = complex([1, 2, 3], [1, 2])")
    with pytest.raises(FermiumError, match="same units"):
        run("X = complex([1 m, 2 m], [1 s, 2 s])")
    with pytest.raises(FermiumError):
        run("X = fft([1, 2])\nprint X[3]")


@pytest.mark.parametrize("src", [
    "X = fft([1 V, 2 V, 3 V, 4 V])\nprint X\nprint X[2]\nprint abs(X)\nprint ifft(X)\nprint conj(X)\nprint len(X)",
    "X = fft(linspace(1, 20, 20))\nprint X\nfor z in X\n    print z",
    "X = fft([1 V, 2 V])\nprint X in mV\nY = fft([1.5, 2, 3])\nprint Y to 4 digits",
    f"X = fft({lit(XS)})\nprint re(X) to 12 digits\nprint im(ifft(fft(X))) to 12 digits\n"
    f"print complex([1, 2], [3, 4])\nprint re(ifft([1, 2, 3])) to 12 digits",
])
def test_standalone_executable_agrees(src, tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60).stdout.strip()
    want = run(src)
    if "to 12 digits" in src:
        for g, w in zip(got.splitlines(), want.splitlines()):
            if "[" in g and "i" not in g:
                assert floats(g) == pytest.approx(floats(w), abs=1e-10)
            else:
                assert g == w
    else:
        assert got == want
