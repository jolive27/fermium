# Q-values of nuclear reactions from atomic masses (in u), results in MeV
from scipy.constants import physical_constants

uc2 = physical_constants["atomic mass constant energy equivalent in MeV"][0]   # 931.494 MeV

n = 1.00866491595   # masses in u
H1 = 1.00782503223
H2 = 2.01410177812
H3 = 3.01604928199
He4 = 4.00260325413
Li7 = 7.0160034366
U238 = 238.0507884
Th234 = 234.0436014


def Q(before, after):
    return (sum(before) - sum(after)) * uc2


print(f"D + T -> He-4 + n: {Q([H2, H3], [He4, n]):.5g} MeV")
print(f"p + Li-7 -> 2 He-4: {Q([H1, Li7], [He4, He4]):.5g} MeV")
print(f"U-238 -> Th-234 + He-4: {Q([U238], [Th234, He4]):.5g} MeV")
print(f"4 H -> He-4: {Q([H1, H1, H1, H1], [He4]):.5g} MeV")
