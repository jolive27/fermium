"""Planck spectral radiance integrated over nu in [1e11, 1e16] Hz for 1000
temperatures in [1000, 10000] K with scipy.integrate.quad (QUADPACK QAGS,
21-point Gauss-Kronrod, adaptive), epsrel = 1e-8, epsabs = 0."""
import math
import time

import numpy as np
from scipy.integrate import quad

H = 6.62607015e-34      # J s
C = 299792458.0         # m/s
KB = 1.380649e-23       # J/K
SIGMA = 5.670374419e-8  # W m^-2 K^-4
PRE = 2.0 * H / C ** 2


def planck(nu, x):
    # quad calls the integrand with Python floats, so scalar math is fastest.
    return PRE * nu ** 3 / math.expm1(x * nu)


def band(T):
    val, _ = quad(planck, 1e11, 1e16, args=(H / (KB * T),), epsabs=0.0, epsrel=1e-8, limit=200)
    return val


def main():
    Ts = np.linspace(1000.0, 10000.0, 1000)
    t0 = time.perf_counter()
    s = sum(band(T) for T in Ts.tolist())
    ratio = band(5778.0) / (SIGMA * 5778.0 ** 4 / np.pi)
    t = time.perf_counter() - t0
    print(f"sum_integrals {s:.10e}")
    print(f"ratio_5778K {ratio:.12f}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
