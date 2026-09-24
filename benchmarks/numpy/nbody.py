"""N-body (Computer Language Benchmarks Game) with NumPy arrays.

Positions/velocities are (5, 3) arrays; each step computes all pair
interactions at once by broadcasting (the standard NumPy formulation).
With only 5 bodies, per-call NumPy overhead dominates -- which is the honest
result for this problem size.
Usage: python nbody.py [N]   (default N = 1_000_000 steps; dt = 0.01)
"""
import sys
import time

import numpy as np

SOLAR_MASS = 4 * np.pi ** 2
DAYS_PER_YEAR = 365.24


def initial_system():
    pos = np.array([
        [0.0, 0.0, 0.0],
        [4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01],
        [8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01],
        [1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01],
        [1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01],
    ])
    vel = DAYS_PER_YEAR * np.array([
        [0.0, 0.0, 0.0],
        [1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05],
        [-2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05],
        [2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05],
        [2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05],
    ])
    mass = SOLAR_MASS * np.array([1.0, 9.54791938424326609e-04, 2.85885980666130812e-04,
                                  4.36624404335156298e-05, 5.15138902046611451e-05])
    vel[0] = -(mass[:, None] * vel).sum(axis=0) / SOLAR_MASS
    return pos, vel, mass


def energy(pos, vel, mass, i, j):
    kinetic = 0.5 * np.sum(mass * np.sum(vel * vel, axis=1))
    d = pos[i] - pos[j]
    potential = np.sum(mass[i] * mass[j] / np.sqrt(np.sum(d * d, axis=1)))
    return kinetic - potential


def advance(pos, vel, mass, dt, nsteps):
    # All-pairs acceleration from the (5, 5, 3) displacement tensor; the
    # self-interaction is removed by setting r^2 = inf on the diagonal.
    for _ in range(nsteps):
        d = pos[:, None, :] - pos[None, :, :]      # d[i, j] = r_i - r_j
        r2 = np.einsum("ijk,ijk->ij", d, d)
        np.fill_diagonal(r2, np.inf)
        w = mass / (r2 * np.sqrt(r2))              # m_j / |r_i - r_j|^3
        vel -= dt * np.einsum("ijk,ij->ik", d, w)
        pos += dt * vel


def main():
    nsteps = int(sys.argv[1]) if len(sys.argv) > 1 else 1_000_000
    t0 = time.perf_counter()
    pos, vel, mass = initial_system()
    i, j = np.triu_indices(len(mass), k=1)
    e0 = energy(pos, vel, mass, i, j)
    advance(pos, vel, mass, 0.01, nsteps)
    e1 = energy(pos, vel, mass, i, j)
    t = time.perf_counter() - t0
    print(f"N {nsteps}")
    print(f"energy_before {e0:.9f}")
    print(f"energy_after {e1:.9f}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
