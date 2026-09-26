"""Fourier transforms for Fermium's FFT built-ins (DECISIONS D81), on NumPy.

Both back ends call `spectrum`: the reference interpreter directly, compiled code through the fm_fft
callback (runtime/core.py).  `fermium build` has its own C version in aot_rt.c.

kind 0 fft_re(xs)            Re X_k, k = 0 … n-1, X_k = Σ_j x_j e^{-2πi jk/n} (NumPy's convention, unnormalised)
kind 1 fft_im(xs)            Im X_k
kind 2 amplitude_spectrum(xs) one-sided amplitudes, k = 0 … n//2: |X_k|/n, doubled except at 0 and at n/2
                              (n even), so a sine A sin(2π f t) at a bin frequency shows a peak of height A
kind 3 power_spectrum(xs, dt) one-sided power spectral density, k = 0 … n//2: |X_k|² dt/n, doubled like the
                              amplitudes; Σ P_k Δf = mean(x²) (Parseval) with Δf = 1/(n dt)
kind 4 ifft(re, im)           Re of the inverse transform (1/n Σ_k X_k e^{+2πi jk/n}), k = 0 … n-1

The complex versions (D243) return X_k as 2n doubles, (re, im) interleaved, and take a complex input the same way:
kind 5 fft(real xs), kind 6 ifft(complex X), kind 7 fft(complex X), kind 8 ifft(real xs).
"""
from __future__ import annotations


def out_len(kind: int, n: int) -> int:
    return n // 2 + 1 if kind in (2, 3) else 2 * n if kind >= 5 else n


def spectrum(kind: int, a, b=None, dt: float = 1.0) -> list:
    import numpy as np
    x = np.asarray(a, dtype=float)
    n = len(x)
    if kind >= 5:
        z = x[0::2] + 1j * x[1::2] if kind in (6, 7) else x
        Z = np.fft.fft(z) if kind in (5, 7) else np.fft.ifft(z)
        out = np.empty(2 * len(Z))
        out[0::2], out[1::2] = Z.real, Z.imag
        return [float(v) for v in out]
    if kind == 4:
        z = x + 1j * np.asarray(b, dtype=float)
        return [float(v) for v in np.fft.ifft(z).real]
    X = np.fft.fft(x)
    if kind == 0:
        return [float(v) for v in X.real]
    if kind == 1:
        return [float(v) for v in X.imag]
    half = X[: n // 2 + 1]
    w = np.full(len(half), 2.0)
    w[0] = 1.0
    if n % 2 == 0:
        w[-1] = 1.0
    if kind == 2:
        return [float(v) for v in w * np.abs(half) / n]
    return [float(v) for v in w * np.abs(half) ** 2 * dt / n]
