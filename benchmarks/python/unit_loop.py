"""Tight loop: E = sum over i=1..N of 1/2 m v^2 with v = i*1e-6 m/s, m = 2 kg (plain floats)."""
import sys
import time


def kinetic(n):
    m = 2.0
    E = 0.0
    for i in range(1, n + 1):
        v = i * 1e-6
        E += 0.5 * m * v * v
    return E


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10_000_000
    t0 = time.perf_counter()
    E = kinetic(n)
    t = time.perf_counter() - t0
    print(f"E_J {E:.12e}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
