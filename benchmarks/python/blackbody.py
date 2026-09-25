"""Planck spectral radiance integrated over nu in [1e11, 1e16] Hz for 1000
temperatures in [1000, 10000] K. Hand-written globally adaptive Gauss-Kronrod
(7-point Gauss / 15-point Kronrod) with rtol = 1e-10 (the tolerance of every Fermium integral), same strategy as QuadGK.jl:
repeatedly bisect the segment with the largest error estimate |K15 - G7|."""
import heapq
import math
import time

H = 6.62607015e-34      # J s
C = 299792458.0         # m/s
KB = 1.380649e-23       # J/K
SIGMA = 5.670374419e-8  # W m^-2 K^-4

# Kronrod nodes (non-negative half) with Kronrod weights; Gauss weights for the
# odd-indexed nodes (which are the 7-point Gauss nodes).
XK = (0.991455371120812639, 0.949107912342758525, 0.864864423359769073,
      0.741531185599394440, 0.586087235467691130, 0.405845151377397167,
      0.207784955007898468, 0.0)
WK = (0.022935322010529225, 0.063092092629978553, 0.104790010322250184,
      0.140653259715525919, 0.169004726639267903, 0.190350578064785410,
      0.204432940075298892, 0.209482141084727828)
WG = (0.129484966168869693, 0.279705391489276668, 0.381830050505118945,
      0.417959183673469388)  # for XK[1], XK[3], XK[5], XK[7]


def gk15(f, a, b):
    c = 0.5 * (a + b)
    hw = 0.5 * (b - a)
    fc = f(c)
    ik = WK[7] * fc
    ig = WG[3] * fc
    for j in range(7):
        dx = hw * XK[j]
        s = f(c - dx) + f(c + dx)
        ik += WK[j] * s
        if j & 1:
            ig += WG[j >> 1] * s
    ik *= hw
    ig *= hw
    return ik, abs(ik - ig)


def quadgk(f, a, b, rtol=1e-10, atol=0.0, maxsegs=100_000):
    i, e = gk15(f, a, b)
    heap = [(-e, a, b, i)]
    total_i, total_e = i, e
    while total_e > max(atol, rtol * abs(total_i)) and len(heap) < maxsegs:
        neg_e, lo, hi, ival = heapq.heappop(heap)
        mid = 0.5 * (lo + hi)
        i1, e1 = gk15(f, lo, mid)
        i2, e2 = gk15(f, mid, hi)
        total_i += i1 + i2 - ival
        total_e += e1 + e2 + neg_e
        heapq.heappush(heap, (-e1, lo, mid, i1))
        heapq.heappush(heap, (-e2, mid, hi, i2))
    # Re-sum to remove accumulated round-off, as QuadGK does.
    return math.fsum(seg[3] for seg in heap), sum(-seg[0] for seg in heap)


def band(T):
    pre = 2.0 * H / (C * C)
    x = H / (KB * T)
    expm1 = math.expm1
    return quadgk(lambda nu: pre * nu ** 3 / expm1(x * nu), 1e11, 1e16)[0]


def main():
    n = 1000
    Ts = [1000.0 + (10000.0 - 1000.0) * k / (n - 1) for k in range(n)]
    t0 = time.perf_counter()
    s = 0.0
    for T in Ts:
        s += band(T)
    ratio = band(5778.0) / (SIGMA * 5778.0 ** 4 / math.pi)
    t = time.perf_counter() - t0
    print(f"sum_integrals {s:.10e}")
    print(f"ratio_5778K {ratio:.12f}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
