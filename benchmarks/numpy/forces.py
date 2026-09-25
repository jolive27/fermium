"""All-pairs gravity for N bodies (O(N^2)), vectorised with NumPy: N x N arrays of separations, row sums.
NumPy's element-wise operations run on one thread; row sums use pairwise summation, so U and the
accelerations agree with the loop versions to ~1e-12, not bit for bit."""
import sys
import time

import numpy as np

G = 6.67430e-11


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    k = np.arange(1, n + 1, dtype=float)
    x = np.cbrt(k) * np.cos(2.4 * k)
    y = np.cbrt(k) * np.sin(2.4 * k)
    z = k / n - 0.5
    M = (1 + np.arange(1, n + 1) % 7) * 1e9
    t0 = time.perf_counter()
    dx = x[None, :] - x[:, None]
    dy = y[None, :] - y[:, None]
    dz = z[None, :] - z[:, None]
    r2 = dx * dx + dy * dy + dz * dz
    np.fill_diagonal(r2, 1.0)
    r = np.sqrt(r2)
    g = G * M[None, :] / (r2 * r)
    np.fill_diagonal(g, 0.0)
    ax = (g * dx).sum(axis=1)
    az = (g * dz).sum(axis=1)
    inv = 1.0 / r
    np.fill_diagonal(inv, 0.0)
    U = -0.5 * G * (M[:, None] * M[None, :] * inv).sum()
    t = time.perf_counter() - t0
    print(f"U_J {U:.11e}")
    print(f"ax1 {ax[0]:.11e}")
    print(f"azN {az[-1]:.11e}")
    print("TIME_INNER", t)


if __name__ == "__main__":
    main()
