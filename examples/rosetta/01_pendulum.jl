# Measure g with a pendulum, then find the exact period of big swings
using Unitful, QuadGK, Printf

L = 1.20u"m"
T = 2.21u"s"
g = 4π^2 * L / T^2
@printf("g = %.4g m/s²\n", ustrip(u"m/s^2", g))
@printf("g = %.4g ft/s²\n", ustrip(u"ft/s^2", g))

function period(θ0)
    integral, _ = quadgk(φ -> 1 / sqrt(1 - sin(θ0 / 2)^2 * sin(φ)^2), 0, π / 2)
    return 4 * sqrt(L / g) * integral
end

for θ0 in [10u"°", 45u"°", 90u"°"]
    @printf("amplitude %d° period %.5g s\n", ustrip(u"°", θ0), ustrip(u"s", period(θ0)))
end
