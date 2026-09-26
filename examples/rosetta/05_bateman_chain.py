# Decay chain Mo-99 -> Tc-99m -> Tc-99: a system of three ODEs
import numpy as np
from scipy.integrate import solve_ivp

lam_Mo = np.log(2) / 65.94    # 1/h
lam_Tc = np.log(2) / 6.0067   # 1/h


def rhs(t, N):
    N_Mo, N_Tc, N_99 = N
    return [-lam_Mo * N_Mo, lam_Mo * N_Mo - lam_Tc * N_Tc, lam_Tc * N_Tc]


sol = solve_ivp(rhs, (0, 240), [1.0, 0.0, 0.0], rtol=1e-10, atol=1e-14, dense_output=True)

print(f"N_Tc(24 h) / N0 = {sol.sol(24)[1]:.5g}")
print(f"N_99(240 h) / N0 = {sol.sol(240)[2]:.5g}")
print(f"Tc-99m peaks at {np.log(lam_Tc / lam_Mo) / (lam_Tc - lam_Mo):.5g} hr")
N200 = sol.sol(200)
print(f"activity ratio at 200 h: {lam_Tc * N200[1] / (lam_Mo * N200[0]):.5g}")
