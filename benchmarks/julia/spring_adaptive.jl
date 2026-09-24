# Damped spring m x'' = -k x - b x', adaptive Dormand–Prince RK45, 0 -> 100 s,
# rtol = 1e-8, atol = 1e-10. Hand-written; the step-size controller and initial
# step selection follow scipy.integrate.solve_ivp(method="RK45") so accepted-step
# counts are directly comparable.
using Printf

const M = 1.0
const K = 100.0
const B = 0.5

# State is a 2-tuple (x, v); small helpers keep everything on the stack.
const S = NTuple{2,Float64}
@inline f(t::Float64, y::S) = (y[2], (-K * y[1] - B * y[2]) / M)
@inline axpy(a::Float64, x::S, y::S) = (y[1] + a * x[1], y[2] + a * x[2])
@inline rmsnorm(x::S) = sqrt((x[1]^2 + x[2]^2) / 2)

# Dormand–Prince 5(4) tableau
const C2, C3, C4, C5 = 1/5, 3/10, 4/5, 8/9
const A21 = 1/5
const A31, A32 = 3/40, 9/40
const A41, A42, A43 = 44/45, -56/15, 32/9
const A51, A52, A53, A54 = 19372/6561, -25360/2187, 64448/6561, -212/729
const A61, A62, A63, A64, A65 = 9017/3168, -355/33, 46732/5247, 49/176, -5103/18656
const B1, B3, B4, B5, B6 = 35/384, 500/1113, 125/192, -2187/6784, 11/84
const E1, E3, E4, E5, E6, E7 = -71/57600, 71/16695, -71/1920, 17253/339200, -22/525, 1/40

const SAFETY, MIN_FACTOR, MAX_FACTOR = 0.9, 0.2, 10.0

@inline lin(y::S, h::Float64, ks::NTuple{N,S}, cs::NTuple{N,Float64}) where {N} =
    (y[1] + h * sum(ntuple(i -> cs[i] * ks[i][1], Val(N))),
     y[2] + h * sum(ntuple(i -> cs[i] * ks[i][2], Val(N))))

function initial_step(t0, y0::S, f0::S, tend, rtol, atol)
    scale = (atol + abs(y0[1]) * rtol, atol + abs(y0[2]) * rtol)
    d0 = rmsnorm((y0[1] / scale[1], y0[2] / scale[2]))
    d1 = rmsnorm((f0[1] / scale[1], f0[2] / scale[2]))
    h0 = (d0 < 1e-5 || d1 < 1e-5) ? 1e-6 : 0.01 * d0 / d1
    h0 = min(h0, tend - t0)
    f1 = f(t0 + h0, axpy(h0, f0, y0))
    d2 = rmsnorm(((f1[1] - f0[1]) / scale[1], (f1[2] - f0[2]) / scale[2])) / h0
    h1 = (d1 <= 1e-15 && d2 <= 1e-15) ? max(1e-6, h0 * 1e-3) : (0.01 / max(d1, d2))^(1 / 5)
    return min(100 * h0, h1, tend - t0)
end

function dopri5(y::S, t::Float64, tend::Float64; rtol::Float64, atol::Float64)
    fy = f(t, y)
    h = initial_step(t, y, fy, tend, rtol, atol)
    naccept = 0
    while t < tend
        min_step = 10 * abs(nextfloat(t) - t)
        h = max(h, min_step)
        rejected = false
        while true
            h < min_step && error("step size too small")
            tnew = min(t + h, tend)
            h = tnew - t
            k1 = fy
            k2 = f(t + C2 * h, lin(y, h, (k1,), (A21,)))
            k3 = f(t + C3 * h, lin(y, h, (k1, k2), (A31, A32)))
            k4 = f(t + C4 * h, lin(y, h, (k1, k2, k3), (A41, A42, A43)))
            k5 = f(t + C5 * h, lin(y, h, (k1, k2, k3, k4), (A51, A52, A53, A54)))
            k6 = f(t + h, lin(y, h, (k1, k2, k3, k4, k5), (A61, A62, A63, A64, A65)))
            ynew = lin(y, h, (k1, k3, k4, k5, k6), (B1, B3, B4, B5, B6))
            fnew = f(tnew, ynew)
            err = lin((0.0, 0.0), h, (k1, k3, k4, k5, k6, fnew), (E1, E3, E4, E5, E6, E7))
            sc1 = atol + max(abs(y[1]), abs(ynew[1])) * rtol
            sc2 = atol + max(abs(y[2]), abs(ynew[2])) * rtol
            enorm = rmsnorm((err[1] / sc1, err[2] / sc2))
            if enorm < 1
                factor = enorm == 0 ? MAX_FACTOR : min(MAX_FACTOR, SAFETY * enorm^(-1 / 5))
                rejected && (factor = min(1.0, factor))
                h *= factor
                t, y, fy = tnew, ynew, fnew
                naccept += 1
                break
            else
                h *= max(MIN_FACTOR, SAFETY * enorm^(-1 / 5))
                rejected = true
            end
        end
    end
    return y, naccept
end

function main()
    dopri5((0.1, 0.0), 0.0, 1.0; rtol=1e-8, atol=1e-10)   # warm-up
    t0 = time_ns()
    y, nacc = dopri5((0.1, 0.0), 0.0, 100.0; rtol=1e-8, atol=1e-10)
    t = (time_ns() - t0) / 1e9
    @printf("x_100s %.10g\n", y[1])
    println("accepted_steps ", nacc)
    println("TIME_INNER ", t)
end

main()
