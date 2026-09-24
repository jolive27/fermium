# Wien's law from dB/dλ = 0, and Stefan–Boltzmann from integrating Planck's law
using QuadGK, Printf

const h, c, k_B = 6.62607015e-34, 299792458.0, 1.380649e-23   # SI units
const σ, b_W = 5.670374419e-8, 2.897771955e-3
T = 5778.0   # K

B(λ) = 2h * c^2 / λ^5 / expm1(h * c / (λ * k_B * T))
dB(λ) = (B(λ * (1 + 1e-6)) - B(λ * (1 - 1e-6))) / (2e-6 * λ)   # central difference

function bisect(f, lo, hi; iters=60)
    for _ in 1:iters
        mid = (lo + hi) / 2
        f(mid) > 0 ? (lo = mid) : (hi = mid)
    end
    return (lo + hi) / 2
end

@printf("peak: %.5g nm\n", bisect(dB, 100e-9, 2000e-9) * 1e9)
@printf("Wien b/T: %.5g nm\n", b_W / T * 1e9)

integral, _ = quadgk(B, 0, Inf)
@printf("π ∫ B dλ = %.6g MW/m²\n", π * integral / 1e6)
@printf("σ T⁴ = %.6g MW/m²\n", σ * T^4 / 1e6)
