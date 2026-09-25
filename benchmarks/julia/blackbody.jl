# Planck spectral radiance integrated over nu in [1e11, 1e16] Hz for 1000
# temperatures in [1000, 10000] K, adaptive Gauss–Kronrod (QuadGK, order 7 = GK15), rtol = 1e-10:
# the tolerance of every Fermium integral (it has no per-integral setting), so both do the same work.
using QuadGK
using Printf

const h = 6.62607015e-34     # J s
const c = 299792458.0        # m/s
const kB = 1.380649e-23      # J/K
const sigma = 5.670374419e-8 # W m^-2 K^-4

@inline planck(nu::Float64, T::Float64) = 2h * nu^3 / c^2 / expm1(h * nu / (kB * T))

band(T::Float64; rtol=1e-10) = quadgk(nu -> planck(nu, T), 1e11, 1e16; rtol=rtol)[1]

function total(Ts)
    s = 0.0
    for T in Ts
        s += band(T)
    end
    return s
end

function main()
    Ts = range(1000.0, 10000.0; length=1000)
    total(range(1000.0, 2000.0; length=3))                # warm-up
    t0 = time_ns()
    s = total(Ts)
    ratio = band(5778.0) / (sigma * 5778.0^4 / pi)
    t = (time_ns() - t0) / 1e9
    @printf("sum_integrals %.10e\n", s)
    @printf("ratio_5778K %.12f\n", ratio)
    println("TIME_INNER ", t)
end

main()
