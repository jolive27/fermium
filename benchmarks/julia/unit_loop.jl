# Tight loop: E = sum over i=1..N of 1/2 m v^2 with v = i*1e-6 m/s, m = 2 kg.
# Two versions: plain Float64 and Unitful.jl quantities (type-stable).
using Unitful
using Printf

function kinetic_plain(N::Int)
    m = 2.0
    E = 0.0
    for i in 1:N
        v = i * 1e-6
        E += 0.5 * m * v^2
    end
    return E
end

function kinetic_unitful(N::Int)
    m = 2.0u"kg"
    E = 0.0u"kg*m^2/s^2"         # concrete unit type of 0.5*m*v^2 -> type-stable accumulator
    for i in 1:N
        v = i * 1e-6u"m/s"
        E += 0.5 * m * v^2
    end
    return uconvert(u"J", E)
end

function main()
    N = isempty(ARGS) ? 10_000_000 : parse(Int, ARGS[1])
    kinetic_plain(10); kinetic_unitful(10)         # warm-up
    t0 = time_ns()
    E = kinetic_plain(N)
    t = (time_ns() - t0) / 1e9
    t0 = time_ns()
    Eu = kinetic_unitful(N)
    tu = (time_ns() - t0) / 1e9
    @printf("E_J %.12e\n", E)
    @printf("E_unitful_J %.12e\n", ustrip(u"J", Eu))
    println("TIME_INNER ", t)
    println("TIME_INNER_UNITFUL ", tu)
end

main()
