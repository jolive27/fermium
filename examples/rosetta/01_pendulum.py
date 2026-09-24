# Measure g with a pendulum, then find the exact period of big swings
import numpy as np
from scipy.integrate import quad

L = 1.20   # m
T = 2.21   # s
g = 4 * np.pi**2 * L / T**2
print(f"g = {g:.4g} m/s²")
print(f"g = {g / 0.3048:.4g} ft/s²")


def period(theta0):
    k2 = np.sin(theta0 / 2) ** 2
    integral, _ = quad(lambda phi: 1 / np.sqrt(1 - k2 * np.sin(phi) ** 2), 0, np.pi / 2)
    return 4 * np.sqrt(L / g) * integral


for deg in [10, 45, 90]:
    print(f"amplitude {deg}° period {period(np.radians(deg)):.5g} s")
