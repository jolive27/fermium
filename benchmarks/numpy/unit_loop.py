"""E = sum over i=1..N of 1/2 m v^2 with v = i*1e-6 m/s, m = 2 kg, vectorized."""
import sys
import time

import numpy as np


def kinetic(n):
    m = 2.0
    v = np.arange(1, n + 1, dtype=np.float64)
    v *= 1e-6                      # in place: one 80 MB array, no temporaries
    return 0.5 * m * (v @ v)       # sum of v^2 as a dot product


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10_000_000
    t0 = time.perf_counter()
    E = kinetic(n)
    t = time.perf_counter() - t0
    print(f"E_J {E:.12e}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
