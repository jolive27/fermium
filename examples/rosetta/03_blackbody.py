# Wien's law from dB/dλ = 0, and Stefan–Boltzmann from integrating Planck's law
import numpy as np
from scipy.constants import h, c, k, sigma, Wien
from scipy.integrate import quad
from scipy.optimize import minimize_scalar

T = 5778.0  # K


def B(lam):
    return 2 * h * c**2 / lam**5 / np.expm1(h * c / (lam * k * T))


peak = minimize_scalar(lambda lam: -B(lam), bounds=(100e-9, 2000e-9), method="bounded",
                       options={"xatol": 1e-15})
print(f"peak: {peak.x * 1e9:.5g} nm")
print(f"Wien b/T: {Wien / T * 1e9:.5g} nm")

# quad needs numbers of order 1: integrate over λ in μm, not in m
integral, _ = quad(lambda x: B(x * 1e-6) * 1e-6, 0, np.inf)
print(f"π ∫ B dλ = {np.pi * integral / 1e6:.6g} MW/m²")
print(f"σ T⁴ = {sigma * T**4 / 1e6:.6g} MW/m²")
