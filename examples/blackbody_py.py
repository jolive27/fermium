"""A small Python helper used by examples/42_python_interop.fm (Fermium's `use python`, DECISIONS D140).

Plain Python: it knows nothing about units.  The Fermium program declares the units at the boundary:
    planck(λ [m], T [K]) -> [W/m³]
so it gets λ in metres and T in kelvin, and its result is read as W/m³ (per steradian).
"""
import numpy as np

H = 6.62607015e-34      # J s
C = 299792458.0         # m/s
KB = 1.380649e-23       # J/K


def planck(lam, T):
    """Spectral radiance B_λ(T) in W/(m² sr m); works on one wavelength or a NumPy array of them."""
    lam = np.asarray(lam, dtype=float)
    return 2 * H * C ** 2 / lam ** 5 / np.expm1(H * C / (lam * KB * T))
