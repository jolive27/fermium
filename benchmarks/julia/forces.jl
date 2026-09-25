# All-pairs gravity for N bodies (O(N²)): each body's acceleration and the total potential energy.
# Threaded over i with Threads.@threads (run with `julia -t T`; the runner uses the same T as Fermium's
# FERMIUM_THREADS), and the same loop on one thread.  Same formulas, in the same order, as forces.fm.
using Printf
using Base.Threads

const G = 6.67430e-11

function setup(N)
    x = [cbrt(k) * cos(2.4 * k) for k in 1:N]
    y = [cbrt(k) * sin(2.4 * k) for k in 1:N]
    z = [k / N - 0.5 for k in 1:N]
    M = [(1 + mod(k, 7)) * 1e9 for k in 1:N]
    return x, y, z, M
end

@inline function row(i, x, y, z, M, N)
    fx = 0.0; fy = 0.0; fz = 0.0; u = 0.0
    @inbounds for j in 1:N
        if j != i
            dx = x[j] - x[i]
            dy = y[j] - y[i]
            dz = z[j] - z[i]
            r2 = dx * dx + dy * dy + dz * dz
            r = sqrt(r2)
            g = G * M[j] / (r2 * r)
            fx += g * dx
            fy += g * dy
            fz += g * dz
            u -= 0.5 * G * M[i] * M[j] / r
        end
    end
    return fx, fy, fz, u
end

function forces_threaded!(ax, ay, az, urow, x, y, z, M, N)
    @threads for i in 1:N
        @inbounds ax[i], ay[i], az[i], urow[i] = row(i, x, y, z, M, N)
    end
    return sum(urow)
end

function forces_serial!(ax, ay, az, urow, x, y, z, M, N)
    for i in 1:N
        @inbounds ax[i], ay[i], az[i], urow[i] = row(i, x, y, z, M, N)
    end
    return sum(urow)
end

function main()
    N = isempty(ARGS) ? 2000 : parse(Int, ARGS[1])
    for f in (forces_threaded!, forces_serial!)          # warm-up: compile on a tiny problem
        x, y, z, M = setup(10)
        f(zeros(10), zeros(10), zeros(10), zeros(10), x, y, z, M, 10)
    end
    x, y, z, M = setup(N)
    ax, ay, az, urow = zeros(N), zeros(N), zeros(N), zeros(N)
    t0 = time_ns()
    U = forces_threaded!(ax, ay, az, urow, x, y, z, M, N)
    t1 = time_ns()
    bx, by, bz, vrow = zeros(N), zeros(N), zeros(N), zeros(N)
    t2 = time_ns()
    V = forces_serial!(bx, by, bz, vrow, x, y, z, M, N)
    t3 = time_ns()
    @printf("U_J %.11e\n", U)
    @printf("ax1 %.11e\n", ax[1])
    @printf("azN %.11e\n", az[N])
    println("TIME_INNER ", (t1 - t0) / 1e9)
    println("TIME_INNER_SERIAL ", (t3 - t2) / 1e9)
end

main()
