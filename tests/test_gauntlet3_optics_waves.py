"""Gauntlet third pass (graduate), optics and waves: run each problem and check it against independent answers."""
import cmath
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.optimize import brentq

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "optics_waves")

C = K.c


def run_problem(name):
    with open(os.path.join(DIR, name), encoding="utf-8") as f:
        return run(f.read(), base_dir=DIR).split("\n")


def field(line, key):
    """The number printed right after `key` on a line."""
    m = re.search(re.escape(key) + r"\s*(\S+)", line)
    assert m, f"{key!r} not in {line!r}"
    return num(m.group(1))


def cfield(line, key):
    """A complex number `a + bi` or `a - bi` printed right after `key`."""
    m = re.search(re.escape(key) + r"\s*(\S+) ([+-]) (\S+)i", line)
    assert m, f"{key!r} not in {line!r}"
    im = num(m.group(3))
    return complex(num(m.group(1)), im if m.group(2) == "+" else -im)


def lines_with(out, prefix):
    return [ln for ln in out if ln.lstrip().startswith(prefix)]


# ---------------------------------------------------------------- 31 Fabry–Pérot

def test_fabry_perot():
    out = run_problem("31_fabry_perot.fm")
    R, d = 0.9, 5e-3
    r, t = math.sqrt(R), math.sqrt(1 - R)
    F = 4 * R / (1 - R) ** 2
    airy = lambda dl: 1 / (1 + F * math.sin(dl / 2) ** 2)
    rows = lines_with(out, "(a)")
    assert len(rows) == 3
    for dl, line in zip((0, 0.1, math.pi), rows):
        # the geometric series in closed form: t² / (1 − r² e^{iδ})
        E = t * t / (1 - r * r * cmath.exp(1j * dl))
        got = cfield(line, "E_t =")
        assert abs(got - E) < 1e-9 * abs(E)
        assert field(line, "|E_t|² =") == pytest.approx(abs(E) ** 2, rel=1e-9)
        assert field(line, "Airy =") == pytest.approx(airy(dl), rel=1e-9)
    fsr = C / (2 * d)
    dh = brentq(lambda x: airy(x) - 0.5, 0, math.pi, xtol=1e-16)
    assert dh == pytest.approx(2 * math.asin(1 / math.sqrt(F)), rel=1e-12)
    fwhm = fsr * dh / math.pi
    b = lines_with(out, "(b)")[0]
    assert field(b, "FSR =") == pytest.approx(fsr / 1e9, rel=1e-7)
    assert field(b, "half-width phase =") == pytest.approx(dh, rel=1e-9)
    assert field(b, "FWHM =") == pytest.approx(fwhm / 1e6, rel=1e-7)
    c = lines_with(out, "(c)")[0]
    assert field(c, "finesse =") == pytest.approx(fsr / fwhm, rel=1e-9)
    assert field(c, "asin(1/sqrt F)) =") == pytest.approx(math.pi / (2 * math.asin(1 / math.sqrt(F))), rel=1e-9)
    assert field(c, "approx =") == pytest.approx(math.pi * math.sqrt(R) / (1 - R), rel=1e-7)
    nu = C / 632.8e-9
    assert field(out[5], "resolving power =") == pytest.approx(nu / fwhm, rel=1e-5)
    assert field(out[5], "order m =") == round(nu / fsr)


# ---------------------------------------------------------------- 32 Gaussian beam

def test_gaussian_beam():
    out = run_problem("32_gaussian_beam.fm")
    lam, w0, z, f = 1064e-9, 0.5e-3, 1.0, 0.1
    zR = math.pi * w0 ** 2 / lam
    w = w0 * math.sqrt(1 + (z / zR) ** 2)
    Rz = z * (1 + (zR / z) ** 2)
    a = lines_with(out, "(a)")[0]
    assert field(a, "z_R =") == pytest.approx(zR, rel=1e-9)
    assert field(a, "w(1 m) =") == pytest.approx(w * 1e3, rel=1e-9)
    assert field(a, "R(1 m) =") == pytest.approx(Rz, rel=1e-9)
    # (b) the ABCD law for q with the lens matrix [[1, 0], [−1/f, 1]], done with Python's complex numbers
    q = complex(z, zR)
    q2 = q / (-q / f + 1)
    s = -q2.real
    w0n = math.sqrt(lam * q2.imag / math.pi)
    b = lines_with(out, "(b)")[0]
    assert field(b, "new waist at s =") == pytest.approx(s * 1e3, rel=1e-9)
    assert field(b, "w0' =") == pytest.approx(w0n * 1e6, rel=1e-9)
    Z = z / f - 1
    assert field(out[3], "Self: s =") == pytest.approx(f * (1 + Z / (Z * Z + (zR / f) ** 2)) * 1e3, rel=1e-9)
    assert field(out[3], "w0' =") == pytest.approx(w0 / math.sqrt(Z * Z + (zR / f) ** 2) * 1e6, rel=1e-9)
    assert field(lines_with(out, "(c)")[0], "to the lens =") == pytest.approx(math.degrees(math.atan(z / zR)), rel=1e-7)
    d = lines_with(out, "(d)")[0]
    # Crank–Nicolson conserves the norm; the width is second-order accurate in dx and dz
    assert field(d, "norm =") == pytest.approx(1.0, abs=2e-7)
    assert field(d, "rms width =") == pytest.approx(w / 2 * 1e3, rel=2e-4)
    assert field(d, "w(z)/2 =") == pytest.approx(w / 2 * 1e3, rel=1e-6)
