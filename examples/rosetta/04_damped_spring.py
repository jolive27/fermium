# A damped mass on a spring: solve m x'' = -k x - b x'
from scipy.integrate import solve_ivp

k = 50.0   # N/m
m = 0.5    # kg
b = 0.2    # kg/s


def rhs(t, y):
    x, v = y
    return [v, (-k * x - b * v) / m]


sol = solve_ivp(rhs, (0, 10), [0.10, 0.0], method="DOP853", rtol=1e-10, atol=1e-12,
                dense_output=True)

x5, _ = sol.sol(5.0)
_, v1 = sol.sol(1.0)
print(f"x(5 s) = {x5 * 100:.5g} cm")
print(f"v(1 s) = {v1 * 100:.5g} cm/s")


def E(t):
    x, v = sol.sol(t)
    return 0.5 * k * x**2 + 0.5 * m * v**2


print(f"energy left after 10 s: {E(10.0) / E(0.0):.4g}")
