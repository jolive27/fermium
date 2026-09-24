# Escape velocity, and the work needed to escape as an integral to infinity
import numpy as np
from scipy.constants import G, c
from scipy.integrate import quad

M_earth, R_earth = 5.9722e24, 6.3781e6   # kg, m (IAU nominal)
M_sun, R_sun = 1.98841e30, 6.957e8       # kg, m


def v_esc(M, R):
    return np.sqrt(2 * G * M / R)


print(f"Earth: {v_esc(M_earth, R_earth) / 1e3:.4g} km/s")
print(f"Moon: {v_esc(7.342e22, 1737.4e3) / 1e3:.4g} km/s")
print(f"Sun: {v_esc(M_sun, R_sun) / 1e3:.4g} km/s")

W_esc, _ = quad(lambda r: G * M_earth * 1.0 / r**2, R_earth, np.inf)   # for 1 kg
print(f"work per kg: {W_esc / 1e6:.4g} MJ")
print(f"Schwarzschild radius of the Sun: {2 * G * M_sun / c**2 / 1e3:.4g} km")
