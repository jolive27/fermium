# N-body (Computer Language Benchmarks Game): Sun + Jupiter, Saturn, Uranus, Neptune.
# Usage: julia nbody.jl [N]   (default N = 1_000_000 steps, dt = 0.01)
using Printf

struct Vec3
    x::Float64
    y::Float64
    z::Float64
end
@inline Base.:+(a::Vec3, b::Vec3) = Vec3(a.x + b.x, a.y + b.y, a.z + b.z)
@inline Base.:-(a::Vec3, b::Vec3) = Vec3(a.x - b.x, a.y - b.y, a.z - b.z)
@inline Base.:*(s::Float64, a::Vec3) = Vec3(s * a.x, s * a.y, s * a.z)
@inline dot(a::Vec3, b::Vec3) = a.x * b.x + a.y * b.y + a.z * b.z

const SOLAR_MASS = 4 * pi^2
const DAYS_PER_YEAR = 365.24

function initial_system()
    pos = [Vec3(0.0, 0.0, 0.0),
           Vec3(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01),
           Vec3(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01),
           Vec3(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01),
           Vec3(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01)]
    vel = [Vec3(0.0, 0.0, 0.0),
           DAYS_PER_YEAR * Vec3(1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05),
           DAYS_PER_YEAR * Vec3(-2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05),
           DAYS_PER_YEAR * Vec3(2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05),
           DAYS_PER_YEAR * Vec3(2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05)]
    mass = SOLAR_MASS .* [1.0, 9.54791938424326609e-04, 2.85885980666130812e-04,
                          4.36624404335156298e-05, 5.15138902046611451e-05]
    # Offset momentum so the total momentum is zero.
    p = Vec3(0.0, 0.0, 0.0)
    for i in eachindex(vel)
        p = p + mass[i] * vel[i]
    end
    vel[1] = (-1.0 / SOLAR_MASS) * p
    return pos, vel, mass
end

function energy(pos::Vector{Vec3}, vel::Vector{Vec3}, mass::Vector{Float64})
    e = 0.0
    n = length(pos)
    for i in 1:n
        e += 0.5 * mass[i] * dot(vel[i], vel[i])
        for j in i+1:n
            d = pos[i] - pos[j]
            e -= mass[i] * mass[j] / sqrt(dot(d, d))
        end
    end
    return e
end

function advance!(pos::Vector{Vec3}, vel::Vector{Vec3}, mass::Vector{Float64}, dt::Float64, nsteps::Int)
    n = length(pos)
    for _ in 1:nsteps
        @inbounds for i in 1:n
            pi_ = pos[i]
            vi = vel[i]
            mi = mass[i]
            for j in i+1:n
                d = pi_ - pos[j]
                d2 = dot(d, d)
                mag = dt / (d2 * sqrt(d2))
                vi = vi - (mass[j] * mag) * d
                vel[j] = vel[j] + (mi * mag) * d
            end
            vel[i] = vi
        end
        @inbounds for i in 1:n
            pos[i] = pos[i] + dt * vel[i]
        end
    end
    return nothing
end

function run(nsteps::Int)
    pos, vel, mass = initial_system()
    e0 = energy(pos, vel, mass)
    advance!(pos, vel, mass, 0.01, nsteps)
    e1 = energy(pos, vel, mass)
    return e0, e1
end

function main()
    n = isempty(ARGS) ? 1_000_000 : parse(Int, ARGS[1])
    run(10)                      # warm-up: compile everything on a tiny problem
    t0 = time_ns()
    e0, e1 = run(n)
    t = (time_ns() - t0) / 1e9
    println("N ", n)
    @printf("energy_before %.9f\n", e0)
    @printf("energy_after %.9f\n", e1)
    println("TIME_INNER ", t)
end

main()
