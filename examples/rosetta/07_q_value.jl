# Q-values of nuclear reactions from atomic masses (in u), results in MeV
using Unitful, Printf
using Unitful: c

n = 1.00866491595u"u"
H1 = 1.00782503223u"u"
H2 = 2.01410177812u"u"
H3 = 3.01604928199u"u"
He4 = 4.00260325413u"u"
Li7 = 7.0160034366u"u"
U238 = 238.0507884u"u"
Th234 = 234.0436014u"u"

Q(before, after) = uconvert(u"MeV", (sum(before) - sum(after)) * c^2)

@printf("D + T -> He-4 + n: %.5g MeV\n", ustrip(Q([H2, H3], [He4, n])))
@printf("p + Li-7 -> 2 He-4: %.5g MeV\n", ustrip(Q([H1, Li7], [He4, He4])))
@printf("U-238 -> Th-234 + He-4: %.5g MeV\n", ustrip(Q([U238], [Th234, He4])))
@printf("4 H -> He-4: %.5g MeV\n", ustrip(Q([H1, H1, H1, H1], [He4])))
