"""Damped spring m x'' = -k x - b x', adaptive Dormand-Prince RK45, 0 -> 100 s,
purely relative error control: rtol = 1e-6, atol = 1e-30 (matched to Fermium's
`tolerance 1e-6`, D17). Hand-written; step-size control and initial step
selection follow scipy.integrate.solve_ivp(method="RK45").
Exact solution: x(100 s) = 1.1176166148e-12 m."""
import math
import time

M = 1.0
K = 100.0
B = 0.5

C2, C3, C4, C5 = 1 / 5, 3 / 10, 4 / 5, 8 / 9
A21 = 1 / 5
A31, A32 = 3 / 40, 9 / 40
A41, A42, A43 = 44 / 45, -56 / 15, 32 / 9
A51, A52, A53, A54 = 19372 / 6561, -25360 / 2187, 64448 / 6561, -212 / 729
A61, A62, A63, A64, A65 = 9017 / 3168, -355 / 33, 46732 / 5247, 49 / 176, -5103 / 18656
B1, B3, B4, B5, B6 = 35 / 384, 500 / 1113, 125 / 192, -2187 / 6784, 11 / 84
E1, E3, E4, E5, E6, E7 = -71 / 57600, 71 / 16695, -71 / 1920, 17253 / 339200, -22 / 525, 1 / 40
SAFETY, MIN_FACTOR, MAX_FACTOR = 0.9, 0.2, 10.0


def f(t, x, v):
    return v, (-K * x - B * v) / M


def rms(a, b):
    return math.sqrt((a * a + b * b) / 2)


def initial_step(t0, x, v, fx, fv, tend, rtol, atol):
    s1 = atol + abs(x) * rtol
    s2 = atol + abs(v) * rtol
    d0 = rms(x / s1, v / s2)
    d1 = rms(fx / s1, fv / s2)
    h0 = 1e-6 if (d0 < 1e-5 or d1 < 1e-5) else 0.01 * d0 / d1
    h0 = min(h0, tend - t0)
    gx, gv = f(t0 + h0, x + h0 * fx, v + h0 * fv)
    d2 = rms((gx - fx) / s1, (gv - fv) / s2) / h0
    if d1 <= 1e-15 and d2 <= 1e-15:
        h1 = max(1e-6, h0 * 1e-3)
    else:
        h1 = (0.01 / max(d1, d2)) ** (1 / 5)
    return min(100 * h0, h1, tend - t0)


def dopri5(x, v, t, tend, rtol, atol):
    fx, fv = f(t, x, v)
    h = initial_step(t, x, v, fx, fv, tend, rtol, atol)
    naccept = 0
    while t < tend:
        min_step = 10 * abs(math.nextafter(t, math.inf) - t)
        h = max(h, min_step)
        rejected = False
        while True:
            if h < min_step:
                raise RuntimeError("step size too small")
            tnew = min(t + h, tend)
            h = tnew - t
            k1x, k1v = fx, fv
            k2x, k2v = f(t + C2 * h, x + h * (A21 * k1x), v + h * (A21 * k1v))
            k3x, k3v = f(t + C3 * h, x + h * (A31 * k1x + A32 * k2x),
                         v + h * (A31 * k1v + A32 * k2v))
            k4x, k4v = f(t + C4 * h, x + h * (A41 * k1x + A42 * k2x + A43 * k3x),
                         v + h * (A41 * k1v + A42 * k2v + A43 * k3v))
            k5x, k5v = f(t + C5 * h, x + h * (A51 * k1x + A52 * k2x + A53 * k3x + A54 * k4x),
                         v + h * (A51 * k1v + A52 * k2v + A53 * k3v + A54 * k4v))
            k6x, k6v = f(t + h, x + h * (A61 * k1x + A62 * k2x + A63 * k3x + A64 * k4x + A65 * k5x),
                         v + h * (A61 * k1v + A62 * k2v + A63 * k3v + A64 * k4v + A65 * k5v))
            xn = x + h * (B1 * k1x + B3 * k3x + B4 * k4x + B5 * k5x + B6 * k6x)
            vn = v + h * (B1 * k1v + B3 * k3v + B4 * k4v + B5 * k5v + B6 * k6v)
            fxn, fvn = f(tnew, xn, vn)
            ex = h * (E1 * k1x + E3 * k3x + E4 * k4x + E5 * k5x + E6 * k6x + E7 * fxn)
            ev = h * (E1 * k1v + E3 * k3v + E4 * k4v + E5 * k5v + E6 * k6v + E7 * fvn)
            s1 = atol + max(abs(x), abs(xn)) * rtol
            s2 = atol + max(abs(v), abs(vn)) * rtol
            enorm = rms(ex / s1, ev / s2)
            if enorm < 1:
                factor = MAX_FACTOR if enorm == 0 else min(MAX_FACTOR, SAFETY * enorm ** (-1 / 5))
                if rejected:
                    factor = min(1.0, factor)
                h *= factor
                t, x, v, fx, fv = tnew, xn, vn, fxn, fvn
                naccept += 1
                break
            h *= max(MIN_FACTOR, SAFETY * enorm ** (-1 / 5))
            rejected = True
    return x, v, naccept


def main():
    t0 = time.perf_counter()
    x, _, nacc = dopri5(0.1, 0.0, 0.0, 100.0, 1e-6, 1e-30)
    t = time.perf_counter() - t0
    print(f"x_100s {x:.10g}")
    print(f"accepted_steps {nacc}")
    print(f"TIME_INNER {t}")


if __name__ == "__main__":
    main()
