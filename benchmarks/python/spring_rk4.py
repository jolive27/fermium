"""Damped spring m x'' = -k x - b x', fixed-step classic RK4, dt = 1e-5 s, 0 -> 10 s."""
import time

M = 1.0     # kg
K = 100.0   # N/m
B = 0.5     # kg/s


def rk4(x, v, dt, nsteps):
    # Plain floats and locals: the fastest idiomatic pure-Python form.
    k_m = K / M
    b_m = B / M
    half = 0.5 * dt
    sixth = dt / 6.0
    for _ in range(nsteps):
        k1x = v
        k1v = -k_m * x - b_m * v
        k2x = v + half * k1v
        k2v = -k_m * (x + half * k1x) - b_m * (v + half * k1v)
        k3x = v + half * k2v
        k3v = -k_m * (x + half * k2x) - b_m * (v + half * k2v)
        k4x = v + dt * k3v
        k4v = -k_m * (x + dt * k3x) - b_m * (v + dt * k3v)
        x += sixth * (k1x + 2 * k2x + 2 * k3x + k4x)
        v += sixth * (k1v + 2 * k2v + 2 * k3v + k4v)
    return x, v


def main():
    dt = 1e-5
    nsteps = round(10.0 / dt)
    t0 = time.perf_counter()
    x, _ = rk4(0.1, 0.0, dt, nsteps)
    t = time.perf_counter() - t0
    print(f"steps {nsteps}")
    print(f"x_10s {x:.10g}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
