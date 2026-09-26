# Semi-empirical mass formula: binding energy per nucleon, peak of the curve
using Printf

const a_V, a_S, a_C, a_A, a_P = 15.75, 17.8, 0.711, 23.7, 11.18   # MeV

pairing(Z, A) = isodd(A) ? 0.0 : (iseven(Z) ? a_P / √A : -a_P / √A)
B(Z, A) = a_V * A - a_S * A^(2 / 3) - a_C * Z * (Z - 1) / A^(1 / 3) - a_A * (A - 2Z)^2 / A + pairing(Z, A)
Z_stable(A) = round(Int, A / (2 + a_C / (2a_A) * A^(2 / 3)))

@printf("Fe-56: B/A = %.5g MeV\n", B(26, 56) / 56)
@printf("U-238: B/A = %.5g MeV\n", B(92, 238) / 238)

best, i = findmax(A -> B(Z_stable(A), A) / A, 10:250)
@printf("peak at A = %d with B/A = %.5g MeV\n", (10:250)[i], best)
