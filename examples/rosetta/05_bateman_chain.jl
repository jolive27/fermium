# Decay chain Mo-99 -> Tc-99m -> Tc-99: a system of three ODEs
# (Normally you'd use DifferentialEquations.jl; a small RK4 keeps this dependency-free.)
using Printf

const λ_Mo = log(2) / 65.94    # 1/h
const λ_Tc = log(2) / 6.0067   # 1/h

rhs(t, N) = [-λ_Mo * N[1], λ_Mo * N[1] - λ_Tc * N[2], λ_Tc * N[2]]

function rk4(f, y0, t0, t1; dt=1e-3)
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

N0 = [1.0, 0.0, 0.0]
@printf("N_Tc(24 h) / N0 = %.5g\n", rk4(rhs, N0, 0.0, 24.0)[2])
@printf("N_99(240 h) / N0 = %.5g\n", rk4(rhs, N0, 0.0, 240.0)[3])
@printf("Tc-99m peaks at %.5g hr\n", log(λ_Tc / λ_Mo) / (λ_Tc - λ_Mo))
N200 = rk4(rhs, N0, 0.0, 200.0)
@printf("activity ratio at 200 h: %.5g\n", λ_Tc * N200[2] / (λ_Mo * N200[1]))
