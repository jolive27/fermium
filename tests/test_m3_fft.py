"""M3 part 2: Fourier transforms (DECISIONS D81), checked against numpy.fft."""
import io
import math

import pytest

from conftest import run
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
    """The numbers in a printed list like [1.5, -2, 3e-05] (units after it ignored)."""
    inner = line[line.index("[") + 1:line.index("]")]
    return [num(x) for x in inner.split(",")] if inner.strip() else []


XS = [0.3, -1.2, 2.5, 0.7, -0.4, 1.9, -2.2, 0.05, 1.1, -0.8, 0.6]   # 11 samples: not a power of two


def lit(xs):
    return "[" + ", ".join(repr(float(x)) for x in xs) + "]"


@pytest.mark.parametrize("xs", [XS, XS[:8], XS[:1], XS[:2]], ids=["n11", "n8", "n1", "n2"])
def test_fft_matches_numpy(xs):
    src = (f"xs = {lit(xs)}\nprint fft_re(xs) to 15 digits\nprint fft_im(xs) to 15 digits\n"
           f"print ifft(fft_re(xs), fft_im(xs)) to 15 digits")
    re_, im_, back = both(src).splitlines()
    X = np.fft.fft(xs)
    assert floats(re_) == pytest.approx(list(X.real), abs=1e-12)
    assert floats(im_) == pytest.approx(list(X.imag), abs=1e-12)
    assert floats(back) == pytest.approx(xs, abs=1e-12)


def test_ifft_of_arbitrary_spectrum_matches_numpy():
    re_ = [1.0, 2.0, -0.5, 0.25, 3.0, -1.0]
    im_ = [0.0, 0.5, 1.5, -2.0, 0.0, 0.7]
    got = floats(both(f"print ifft({lit(re_)}, {lit(im_)}) to 15 digits"))
    assert got == pytest.approx(list(np.fft.ifft(np.array(re_) + 1j * np.array(im_)).real), abs=1e-12)


def test_amplitude_and_power_spectrum_definitions():
    xs = np.array(XS[:10])
    src = f"xs = {lit(list(xs))}\nprint amplitude_spectrum(xs) to 15 digits\nprint power_spectrum(xs, 1) to 15 digits"
    amp, pw = both(src).splitlines()
    X = np.fft.rfft(xs)
    w = np.array([1, 2, 2, 2, 2, 1.0])
    assert floats(amp) == pytest.approx(list(w * np.abs(X) / 10), rel=1e-12)
    assert floats(pw) == pytest.approx(list(w * np.abs(X) ** 2 / 10), rel=1e-12)


SIGNAL = """dt = 1 ms
n = 1000
ts = linspace(0 s, (n - 1) dt, n)
xs = zeros(n)
for i from 1 to n
    xs[i] = 3 V * sin(2π * 50 Hz * ts[i]) + 1 V * cos(2π * 120 Hz * ts[i]) + 0.5 V
A = amplitude_spectrum(xs)
f = frequencies(xs, dt)
peaks = values(A)
peaks[1] = 0 V
k = argmax(peaks)
print f[k]
print A[k] to 12 digits
print A[1] to 12 digits
print len(f), f[end]
P = power_spectrum(xs, dt)
df = f[2] - f[1]
print sum(P) df / mean(xs * xs) to 12 digits
"""


def test_sine_peak_frequency_in_hertz_and_amplitude_in_volts():
    out = both(SIGNAL).splitlines()
    assert out[0] == "50 Hz"                 # the peak, in Hz with units
    assert out[1].endswith(" V") and num(out[1]) == pytest.approx(3, rel=1e-10)     # its amplitude, in volts
    assert out[2].endswith(" V") and num(out[2]) == pytest.approx(0.5, rel=1e-10)   # the offset at 0 Hz
    assert out[3] == "501 500 Hz"            # up to the Nyquist frequency 1/(2 dt)
    assert float(out[4]) == pytest.approx(1.0, rel=1e-12)     # Parseval


def test_peak_between_bins_is_near_true_frequency():
    # 47.3 Hz sampled for 1 s: the peak is in the nearest bin (47 Hz), as numpy finds
    src = SIGNAL.replace("50 Hz", "47.3 Hz")
    out = run(src).splitlines()
    t = np.arange(1000) * 1e-3
    x = 3 * np.sin(2 * np.pi * 47.3 * t) + np.cos(2 * np.pi * 120 * t) + 0.5
    k = int(np.argmax(np.abs(np.fft.rfft(x))[1:])) + 1
    assert out[0] == f"{np.fft.rfftfreq(1000, 1e-3)[k]:g} Hz"


def test_frequencies_matches_numpy_rfftfreq():
    for n in (1, 2, 7, 8):
        got = floats(both(f"print frequencies({n}, 0.25 s) to 15 digits"))
        assert got == pytest.approx(list(np.fft.rfftfreq(n, 0.25)), rel=1e-13)


def test_units_of_the_results():
    out = both("xs = [1 m, 2 m, 3 m, 4 m]\nprint fft_re(xs)\nprint power_spectrum(xs, 1 s)\nprint frequencies(xs, 1 ms)"
               ).splitlines()
    assert out[0].endswith("m") and "[10, -2, -2, -2]" in out[0]
    assert out[1].endswith("m² s") or out[1].endswith("m²/Hz")
    assert out[2] == "[0, 250, 500] Hz"


def test_argmax_and_argmin():
    assert both("print argmax([3, 9, 2, 9]), argmin([3, 9, 2, 9])") == "2 3"


def test_errors():
    with pytest.raises(FermiumError, match="ifft\\(re, im\\): the real and imaginary parts need the same units"):
        run("x = ifft([1 m, 2 m], [1 s, 2 s])")
    with pytest.raises(FermiumError, match="must be a list"):
        run("x = fft_re(3 m)")
    with pytest.raises(FermiumError, match="power_spectrum takes 2 arguments"):
        run("x = power_spectrum([1, 2, 3])")
    with pytest.raises(FermiumError, match="different lengths"):
        run("x = ifft([1, 2, 3], [1, 2])")
    with pytest.raises(FermiumError, match="empty"):
        run("xs = zeros(0)\nprint fft_re(xs)")


def test_standalone_executable_matches_numpy(tmp_path):
    from fermium.aot import build, find_cc
    if find_cc() is None:
        pytest.skip("no C compiler")
    import subprocess
    src = (f"xs = {lit(XS)}\nprint fft_re(xs) to 12 digits\nprint fft_im(xs) to 12 digits\n"
           f"print ifft(fft_re(xs), fft_im(xs)) to 12 digits\nprint amplitude_spectrum({lit(XS[:8])}) to 12 digits")
    exe = str(tmp_path / "prog")
    build(src, str(tmp_path / "prog.fm"), exe)
    got = subprocess.run([exe], capture_output=True, text=True, timeout=60).stdout.strip().splitlines()
    X = np.fft.fft(XS)
    assert floats(got[0]) == pytest.approx(list(X.real), abs=1e-10)
    assert floats(got[1]) == pytest.approx(list(X.imag), abs=1e-10)
    assert floats(got[2]) == pytest.approx(XS, abs=1e-10)
    X8 = np.fft.rfft(XS[:8])
    assert floats(got[3]) == pytest.approx(list(np.array([1, 2, 2, 2, 1]) * np.abs(X8) / 8), abs=1e-10)
    assert math.isfinite(sum(floats(got[3])))
