"""Gauntlet third pass (graduate), electromagnetism: run each problem and check it against independent answers."""
import cmath
import math
import os
import re

import pytest
from scipy import constants as K
from scipy.optimize import brentq
from scipy.special import jn_zeros, jnp_zeros

from conftest import run
from numparse import num

DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gauntlet", "electromagnetism")

C, EPS0, MU0, HBAR, EV = K.c, K.epsilon_0, K.mu_0, K.hbar, K.e


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


# ---------------------------------------------------------------- 31 waveguides

def test_waveguide_modes():
    out = run_problem("31_waveguide_modes.fm")
    a, b = 22.86e-3, 10.16e-3
    fc = lambda m, n: C / 2 * math.hypot(m / a, n / b)
    A = lines_with(out, "(a)")[0]
    for key, (m, n) in (("TE10 =", (1, 0)), ("TE20 =", (2, 0)), ("TE01 =", (0, 1)), ("TE11 =", (1, 1))):
        assert field(A, key) == pytest.approx(fc(m, n) / 1e9, rel=1e-7)
    # Pozar Table / WR-90 data: TE10 cutoff 6.557 GHz
    assert field(A, "TE10 =") == pytest.approx(6.557, abs=1e-3)

    f = 10e9
    root = math.sqrt(1 - (fc(1, 0) / f) ** 2)
    B = lines_with(out, "(b)")[0]
    assert field(B, "lambda_g =") == pytest.approx(C / f / root * 1e3, rel=1e-7)
    assert field(B, "v_p/c =") == pytest.approx(1 / root, rel=1e-7)
    assert field(B, "v_g/c =") == pytest.approx(root, rel=1e-7)
    assert field(B, "v_p v_g / c² =") == pytest.approx(1.0, rel=1e-12)

    Cl = lines_with(out, "(c)")[0]
    # the matrix eigenvalue solver is Richardson-extrapolated: ~1e-10
    assert field(Cl, "k_c =") == pytest.approx(math.pi / a, rel=1e-9)
    assert field(Cl, "pi/a =") == pytest.approx(math.pi / a, rel=1e-9)
    assert field(Cl, "second / first =") == pytest.approx(4.0, rel=1e-7)

    # (d) Pozar eq. 3.96 evaluated independently
    sigma = 5.8e7
    w = 2 * math.pi * f
    k = w / C
    beta = math.sqrt(k * k - (math.pi / a) ** 2)
    eta = math.sqrt(MU0 / EPS0)
    Rs = math.sqrt(w * MU0 / (2 * sigma))
    alpha = Rs * (2 * b * math.pi ** 2 + a ** 3 * k * k) / (a ** 3 * b * beta * k * eta)
    D = lines_with(out, "(d)")[0]
    assert field(D, "R_s =") == pytest.approx(Rs, rel=1e-5)
    assert field(D, "alpha_c =") == pytest.approx(alpha, rel=1e-5)
    assert field(D, "1/m  =") == pytest.approx(20 * math.log10(math.e) * alpha, rel=1e-5)
    # the textbook figure for copper WR-90 at 10 GHz is about 0.11 dB/m
    assert field(D, "1/m  =") == pytest.approx(0.11, abs=0.005)

    E = lines_with(out, "(e)")[0]
    x11, x01 = jnp_zeros(1, 1)[0], jn_zeros(0, 1)[0]
    assert field(E, "x'11 =") == pytest.approx(x11, rel=1e-9)
    assert field(E, "x01 =") == pytest.approx(x01, rel=1e-9)
    assert field(E, "TE11 =") == pytest.approx(C * x11 / (2 * math.pi * 0.01) / 1e9, rel=1e-7)
    assert field(E, "TM01 =") == pytest.approx(C * x01 / (2 * math.pi * 0.01) / 1e9, rel=1e-7)


# ---------------------------------------------------------------- 32 Drude metal

def test_drude_metal_optics():
    out = run_problem("32_drude_metal_optics.fm")
    wp, g = 15.0 * EV / HBAR, 0.1 * EV / HBAR
    eps = lambda w: 1 - wp ** 2 / (w * w + 1j * g * w)
    N = lambda w: cmath.sqrt(eps(w))
    R = lambda w: abs((1 - N(w)) / (1 + N(w))) ** 2
    rows = lines_with(out, "(a)")
    assert len(rows) == 5
    for E, line in zip((1, 10, 14, 15, 20), rows):
        w = E * EV / HBAR
        e_got, n_got = cfield(line, "eps ="), cfield(line, "N =")
        # 8 digits per part; near ω_p the real part of ε is a small difference, so compare
        # each part with a tolerance relative to |ε|
        assert abs(e_got - eps(w)) < 1e-7 * abs(eps(w))
        assert abs(n_got - N(w)) < 1e-7 * abs(N(w))
        assert N(w).imag >= 0
        assert field(line, "R =") == pytest.approx(R(w), rel=1e-9)
    w_half = brentq(lambda w: R(w) - 0.5, 10 * EV / HBAR, 20 * EV / HBAR, xtol=1e-3)
    assert field(lines_with(out, "(b)")[0], "hbar w =") == pytest.approx(HBAR * w_half / EV, rel=1e-9)
    w1, w2 = EV / HBAR, 0.01 * EV / HBAR
    c = lines_with(out, "(c)")[0]
    assert field(c, "at 1 eV =") == pytest.approx(C / (w1 * N(w1).imag) * 1e9, rel=1e-7)
    assert field(c, "at 0.01 eV =") == pytest.approx(C / (w2 * N(w2).imag) * 1e9, rel=1e-7)
    # far below γ the field skin depth is the classical √(2/(μ0 σ0 ω)) (to O(ω/γ))
    s0 = EPS0 * wp ** 2 / g
    assert field(out[7], "sigma_0 =") == pytest.approx(s0, rel=1e-5)
    assert field(c, "at 0.01 eV =") == pytest.approx(math.sqrt(2 / (MU0 * s0 * w2)) * 1e9, rel=0.1)
    d = lines_with(out, "(d)")[0]
    assert field(d, "R(0.01 eV) =") == pytest.approx(R(w2), rel=1e-9)
    assert field(d, "Hagen-Rubens =") == pytest.approx(1 - math.sqrt(8 * EPS0 * w2 / s0), rel=1e-9)
