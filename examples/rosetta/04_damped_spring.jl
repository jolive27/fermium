# A damped mass on a spring: solve m x'' = -k x - b x'
# (Normally you'd use DifferentialEquations.jl; a small RK4 keeps this dependency-free.)
using Printf

const k, m, b = 50.0, 0.5, 0.2   # N/m, kg, kg/s

rhs(t, y) = [y[2], (-k * y[1] - b * y[2]) / m]

function rk4(f, y0, t0, t1; dt=1e-4)
    n = round(Int, (t1 - t0) / dt)
    h = (t1 - t0) / n
    y, t = copy(y0), t0
    for _ in 1:n
        k1 = f(t, y)
        k2 = f(t + h / 2, y .+ h / 2 .* k1)
        k3 = f(t + h / 2, y .+ h / 2 .* k2)
        k4 = f(t + h, y .+ h .* k3)
        y = y .+ h / 6 .* (k1 .+ 2k2 .+ 2k3 .+ k4)
        t += h
    end
    return y
end

y0 = [0.10, 0.0]
@printf("x(5 s) = %.5g cm\n", rk4(rhs, y0, 0.0, 5.0)[1] * 100)
@printf("v(1 s) = %.5g cm/s\n", rk4(rhs, y0, 0.0, 1.0)[2] * 100)

E(y) = 0.5k * y[1]^2 + 0.5m * y[2]^2
@printf("energy left after 10 s: %.4g\n", E(rk4(rhs, y0, 0.0, 10.0)) / E(y0))
