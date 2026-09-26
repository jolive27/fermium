"""All-pairs gravity for N bodies (O(N^2)): accelerations and total potential energy, one thread (pure
Python has no shared-memory parallel loop; its GIL serialises threads).  Same formulas as forces.fm."""
import math
import sys
import time

G = 6.67430e-11


def forces(x, y, z, M, n):
    ax, ay, az = [0.0] * n, [0.0] * n, [0.0] * n
    U = 0.0
    for i in range(n):
        xi, yi, zi, Mi = x[i], y[i], z[i], M[i]
        fx = fy = fz = 0.0
        for j in range(n):
            if j != i:
                dx = x[j] - xi
                dy = y[j] - yi
                dz = z[j] - zi
                r2 = dx * dx + dy * dy + dz * dz
                r = math.sqrt(r2)
                g = G * M[j] / (r2 * r)
                fx += g * dx
                fy += g * dy
                fz += g * dz
                U -= 0.5 * G * Mi * M[j] / r
        ax[i], ay[i], az[i] = fx, fy, fz
    return U, ax, ay, az


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    x = [math.cbrt(k) * math.cos(2.4 * k) for k in range(1, n + 1)]
    y = [math.cbrt(k) * math.sin(2.4 * k) for k in range(1, n + 1)]
    z = [k / n - 0.5 for k in range(1, n + 1)]
    M = [(1 + k % 7) * 1e9 for k in range(1, n + 1)]
    t0 = time.perf_counter()
    U, ax, ay, az = forces(x, y, z, M, n)
    t = time.perf_counter() - t0
    print(f"U_J {U:.11e}")
    print(f"ax1 {ax[0]:.11e}")
    print(f"azN {az[-1]:.11e}")
    print("TIME_INNER", t)


if __name__ == "__main__":
    main()
