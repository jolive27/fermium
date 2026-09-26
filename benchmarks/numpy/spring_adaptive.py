"""Damped spring m x'' = -k x - b x', scipy.integrate.solve_ivp(method="RK45")
(Dormand-Prince 5(4)), 0 -> 100 s, purely relative error control: rtol = 1e-6,
atol = 1e-30 (matched to Fermium's `tolerance 1e-6`, D17).
Exact solution: x(100 s) = 1.1176166148e-12 m."""
import time

from scipy.integrate import solve_ivp

M = 1.0
K = 100.0
B = 0.5


def rhs(t, y):
    x, v = y
    return [v, (-K * x - B * v) / M]


def main():
    t0 = time.perf_counter()
    sol = solve_ivp(rhs, (0.0, 100.0), [0.1, 0.0], method="RK45", rtol=1e-6, atol=1e-30)
    t = time.perf_counter() - t0
    assert sol.success, sol.message
    print(f"x_100s {sol.y[0, -1]:.10g}")
    print(f"accepted_steps {len(sol.t) - 1}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
