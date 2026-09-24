"""Damped spring m x'' = -k x - b x', fixed-step classic RK4, dt = 1e-5 s, 0 -> 10 s.

NumPy formulation: state y = [x, v] as an ndarray and the linear right-hand
side y' = A @ y. A sequential time-stepping loop cannot be vectorized, so NumPy
adds per-call overhead on 2-element arrays; this is the honest idiomatic result.
"""
import time

import numpy as np

M = 1.0     # kg
K = 100.0   # N/m
B = 0.5     # kg/s


def rk4(y, dt, nsteps):
    A = np.array([[0.0, 1.0], [-K / M, -B / M]])
    for _ in range(nsteps):
        k1 = A @ y
        k2 = A @ (y + 0.5 * dt * k1)
        k3 = A @ (y + 0.5 * dt * k2)
        k4 = A @ (y + dt * k3)
        y = y + dt / 6 * (k1 + 2 * k2 + 2 * k3 + k4)
    return y


def main():
    dt = 1e-5
    nsteps = round(10.0 / dt)
    t0 = time.perf_counter()
    y = rk4(np.array([0.1, 0.0]), dt, nsteps)
    t = time.perf_counter() - t0
    print(f"steps {nsteps}")
    print(f"x_10s {y[0]:.10g}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
