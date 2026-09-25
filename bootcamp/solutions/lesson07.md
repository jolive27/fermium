# Solutions — Lesson 7

[Back to Lesson 7](../lesson07_derivatives.md)

## 1. Braking car

```fermium
x(t) = 30 m/s * t - 2.5 m/s^2 * t^2
v = x'
print "velocity at 2 s:", v(2 s)
print "acceleration at 2 s:", x''(2 s)

# find when it stops, in steps of 0.01 s
t = 0 s
while v(t) > 0 m/s
    t += 0.01 s
print "stops at", t
```

<!-- output -->
```
velocity at 2 s: 20 m/s
acceleration at 2 s: -5 m/s²
stops at 6.0 s
```

By hand: v(t) = 30 m/s − (5 m/s²) t = 0 at t = 6 s.

## 2. Spring potential

```fermium
k = 200 N/m
U(x) = ½ k x^2
F(x) = -U'(x)
print F(5 cm)
print -k * 5 cm
```

<!-- output -->
```
-10 N
-10 N
```

## 3. Decay rate

```fermium
N(t) = 5000 exp(-t / 8 day)
print N'(0 day) in 1/day
print N'(8 day) in 1/day
```

<!-- output -->
```
-625 1/day
-230 1/day
```

At the start, 5000/8 = 625 nuclei per day decay; one lifetime later, e times fewer.

## 4. Lennard-Jones

```fermium
eps = 1.65e-21 J
sig = 0.34 nm
U(r) = 4 eps ((sig / r)^12 - (sig / r)^6)
F(r) = -U'(r)
for r in [0.35 nm, 0.38 nm, 0.45 nm]
    print r, F(r)
r0 = 2^(1/6) sig
print "zero force at", r0 in nm, ": F =", F(r0)
```

<!-- output -->
```
0.35 nm 6.5×10⁻¹¹ N
0.38 nm 1.4×10⁻¹² N
0.45 nm -1.0×10⁻¹¹ N
zero force at 0.38 nm : F = -6.8×10⁻²⁷ N
```

Positive force means repulsion (pushing r larger), negative means attraction. The force is zero at 2^(1/6)σ ≈ 0.38 nm (the tiny number there is rounding error, effectively zero): that's the equilibrium distance between the atoms.

## 5. Jerk

```fermium
A = 2 cm
omega = 5 rad/s
x(t) = A cos(omega t)
jerk = x'''
print jerk
print jerk(0.1 s)
```

<!-- output -->
```
jerk(t) = A ω³ sin(ω t)   [m/s³, for t in s]
1.2 m/s³
```
