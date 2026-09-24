# Escape velocity, and the work needed to escape as an integral to infinity
using Unitful, QuadGK, Printf
using Unitful: G, c

M_earth, R_earth = 5.9722e24u"kg", 6.3781e6u"m"
M_sun, R_sun = 1.98841e30u"kg", 6.957e8u"m"

v_esc(M, R) = sqrt(2G * M / R)

@printf("Earth: %.4g km/s\n", ustrip(u"km/s", v_esc(M_earth, R_earth)))
@printf("Moon: %.4g km/s\n", ustrip(u"km/s", v_esc(7.342e22u"kg", 1737.4u"km")))
@printf("Sun: %.4g km/s\n", ustrip(u"km/s", v_esc(M_sun, R_sun)))

F(r) = G * M_earth * 1u"kg" / r^2
# quadgk can't take a unitful infinite limit, so strip units for the integral
W, _ = quadgk(r -> ustrip(u"N", F(r * u"m")), ustrip(u"m", R_earth), Inf)
W_esc = W * u"J"
@printf("work per kg: %.4g MJ\n", ustrip(u"MJ", W_esc))
@printf("Schwarzschild radius of the Sun: %.4g km\n", ustrip(u"km", 2G * M_sun / c^2))
