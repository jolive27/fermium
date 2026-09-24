# Damped spring m x'' = -k x - b x', fixed-step classic RK4, dt = 1e-5 s, 0 -> 10 s.
using Printf

const M = 1.0     # kg
const K = 100.0   # N/m
const B = 0.5     # kg/s

@inline accel(x::Float64, v::Float64) = (-K * x - B * v) / M

function rk4(x::Float64, v::Float64, dt::Float64, nsteps::Int)
    for _ in 1:nsteps
        k1x = v;                    k1v = accel(x, v)
        k2x = v + 0.5dt * k1v;      k2v = accel(x + 0.5dt * k1x, v + 0.5dt * k1v)
        k3x = v + 0.5dt * k2v;      k3v = accel(x + 0.5dt * k2x, v + 0.5dt * k2v)
        k4x = v + dt * k3v;         k4v = accel(x + dt * k3x, v + dt * k3v)
        x += dt / 6 * (k1x + 2k2x + 2k3x + k4x)
        v += dt / 6 * (k1v + 2k2v + 2k3v + k4v)
    end
    return x, v
end

function main()
    dt = 1e-5
    nsteps = round(Int, 10.0 / dt)
    rk4(0.1, 0.0, dt, 10)        # warm-up
    t0 = time_ns()
    x, _ = rk4(0.1, 0.0, dt, nsteps)
    t = (time_ns() - t0) / 1e9
    println("steps ", nsteps)
    @printf("x_10s %.10g\n", x)
    println("TIME_INNER ", t)
end

main()
