# Semi-empirical mass formula: binding energy per nucleon, peak of the curve
import numpy as np

a_V, a_S, a_C, a_A, a_P = 15.75, 17.8, 0.711, 23.7, 11.18   # MeV


def pairing(Z, A):
    if A % 2 == 1:
        return 0.0
    return a_P / np.sqrt(A) if Z % 2 == 0 else -a_P / np.sqrt(A)


def B(Z, A):
    return (a_V * A - a_S * A ** (2 / 3) - a_C * Z * (Z - 1) / A ** (1 / 3)
            - a_A * (A - 2 * Z) ** 2 / A + pairing(Z, A))


def Z_stable(A):
    return round(A / (2 + a_C / (2 * a_A) * A ** (2 / 3)))


print(f"Fe-56: B/A = {B(26, 56) / 56:.5g} MeV")
print(f"U-238: B/A = {B(92, 238) / 238:.5g} MeV")

As = np.arange(10, 251)
BperA = np.array([B(Z_stable(A), A) / A for A in As])
i = np.argmax(BperA)
print(f"peak at A = {As[i]} with B/A = {BperA[i]:.5g} MeV")
