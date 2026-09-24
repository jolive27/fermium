# Lesson 7 — Derivatives

Velocity is the derivative of position; acceleration is the derivative of velocity; force is minus the derivative of potential energy. In most programming languages, taking a derivative is a chore. In Fermium it's one character: `'`.

This lesson assumes you've met derivatives in a calculus class. If they're rusty: the derivative dx/dt is the *rate of change* of x, the slope of the graph of x against t.

## The prime `'`

Start with a function (Lesson 3), then put a prime after its name:

```fermium
y(t) = 20 m/s * t - ½ * 9.81 m/s^2 * t^2
v = y'
a = y''
print y
print v
print a
```

<!-- output -->
```
y(t) = 20 m/s t - 0.5·9.81 m/s^2 t²   [m, for t in s]
v(t) = 20 m/s - t·9.81 m/s^2   [m/s, for t in s]
a(t) = -9.81 m/s^2   [m/s²]
```

Fermium did the calculus for you, **exactly**, the way you would on paper (it isn't an approximation). `y'` is the velocity, `y''` the acceleration. Look at the units in brackets: y is in m, so y′ is in m/s and y″ in m/s². Fermium divides by the units of `t` each time.

`v` and `a` are new functions. Call them like any other:

```fermium
y(t) = 20 m/s * t - ½ * 9.81 m/s^2 * t^2
v = y'
print "velocity at 1 s:", v(1 s)
print "velocity at 3 s:", v(3 s)
print "directly:", y'(2 s)
```

<!-- output -->
```
velocity at 1 s: 10.2 m/s
velocity at 3 s: -9.43 m/s
directly: 0.380 m/s
```

You can write `y'(2 s)` directly without giving the derivative a name.

## Leibniz notation: `d/dt`

If you prefer the notation dy/dt, you can write exactly that: `dy/dt`, or `d/dt y` (the variable after `d/d` must be the function's parameter). Both mean the same as `y'`:

```fermium
y(t) = 20 m/s * t - ½ * 9.81 m/s^2 * t^2
v = dy/dt
print v
print dy/dt(2 s)
```

<!-- output -->
```
v(t) = 20 m/s - t·9.81 m/s^2   [m/s, for t in s]
0.380 m/s
```

Second derivatives are written `d²/dt² y` (or `d^2/dt^2 y`), or simply `y''`. (The form `d²y/dt²` isn't understood yet: Fermium says `d isn't defined`. Use one of the others.) Here's an oscillation, x(t) = A cos(ωt), differentiated twice:

```fermium
A = 0.1 m
omega = 10 rad/s
x(t) = A cos(omega t)
v = d/dt x
a = d²/dt² x
print v
print a
```

<!-- output -->
```
v(t) = -A ω sin(ω t)   [m/s, for t in s]
a(t) = -A ω² cos(ω t)   [m/s², for t in s]
```

This is simple harmonic motion. Let's check the famous property a = −ω²x:

```fermium
A = 0.1 m
omega = 10 rad/s
x(t) = A cos(omega t)
a = x''
print a(0.3 s) / x(0.3 s)
print -omega^2
```

<!-- output -->
```
-100 1/s²
-100 1/s²
```

They match.

## Forces from potentials

In physics, a force is minus the slope of the potential energy: F = −dU/dx. Gravity near a planet has U(r) = −GMm/r:

```fermium
m = 1 kg
U(r) = -G M_earth m / r
F(r) = -U'(r)
print "force on 1 kg at the surface:", F(R_earth)
print "force at twice the radius:", F(2 R_earth)
```

<!-- output -->
```
force on 1 kg at the surface: -9.79845 N
force at twice the radius: -2.44961 N
```

The force is negative because it points *inward* (toward smaller r), and it's about 9.8 N at the surface, as it should be. At twice the distance it's four times weaker: the inverse-square law, which we never typed in. It came from differentiating 1/r.

## Derivatives work on lists too

Evaluate a derivative at many times at once:

```fermium
x(t) = 5 m * sin(2 t / 1 s)
ts = linspace(0 s, 1 s, 5)
print ts
print x'(ts)
```

<!-- output -->
```
[0, 0.25, 0.5, 0.75, 1] s
[10, 8.77583, 5.40302, 0.707372, -4.16147] m/s
```

## What's happening inside

You might wonder whether Fermium estimates the derivative with a small step, like (x(t+h) − x(t))/h. It doesn't: it applies the rules of differentiation (sum rule, product rule, chain rule, derivatives of sin, exp, ln, …) to your formula, just as you would. Compare with the small-step estimate:

```fermium
x(t) = 3 m/s^3 * t^3
h = 1e-6 s
print "estimate:", (x(2 s + h) - x(2 s)) / h to 9 digits
print "exact:   ", x'(2 s) to 9 digits
print x'
```

<!-- output -->
```
estimate: 36.0000180 m/s
exact:    36.0000000 m/s
x'(t) = 3 t²·3 m/s^3
```

The estimate is close but not exact; the prime is exact.

Two limits to know about:
- You can differentiate **one-line** functions (`f(x) = ...`). A multi-line function gives the error "can only differentiate one-line functions".
- The printed formula is correct but not always as tidy as you'd write it by hand.

## Partial derivatives

For a function of several variables, `∂/∂x f` (in ASCII: `partial/partial x f`) differentiates with respect to one of them, keeping the others fixed:

```fermium
f(x, y) = x^2 y + sin(y)
fx = partial/partial x f
fy = partial/partial y f
print fx
print fy
print fx(1, 2), fy(1, 2)
```

<!-- output -->
```
fx(x, y) = 2x y
fy(x, y) = x² + cos(y)
4 0.583853
```

## Summary

- `y'` and `y''` are the first and second derivatives of a one-line function `y(t) = ...`.
- `dy/dt` and `d/dt y` mean the same as `y'`; `d²/dt² y` means `y''`.
- Derivatives are exact (symbolic), and their units are worked out: m → m/s → m/s².
- A derivative is a function: `y'(2 s)`, `v = y'`, `v(ts)` on a list.
- `partial/partial x f` (or `∂/∂x f`) for partial derivatives.

## Exercises

1. **Braking car.** A car's position is x(t) = 30 m/s · t − 2.5 m/s² · t². Find its velocity and acceleration at t = 2 s. At what time does it stop? (Solve v(t) = 0 by hand, or use a loop.)
2. **Spring potential.** A spring has U(x) = ½ k x² with k = 200 N/m. Use a derivative to find the force at x = 5 cm, and check it against F = −kx.
3. **Decay rate.** A sample decays as N(t) = 5000 exp(−t / 8 day). How fast is it decaying (dN/dt) at t = 0 and at t = 8 days? Print in 1/day.
4. **Lennard-Jones.** The potential between two argon atoms is U(r) = 4ε((σ/r)¹² − (σ/r)⁶) with ε = 1.65e-21 J and σ = 0.34 nm. Compute the force F = −U′(r) at r = 0.35 nm, 0.38 nm and 0.45 nm. Where is the force zero? (Theory: r = 2^(1/6) σ.) Careful: `sigma` and `σ` are the built-in Stefan–Boltzmann constant; you can use your own `sig` instead, or overwrite it.
5. **Jerk.** The derivative of acceleration is called *jerk*. For x(t) = A cos(ωt) with A = 2 cm and ω = 5 rad/s, print the jerk function and its units.

Solutions: [solutions/lesson07.md](solutions/lesson07.md)

**Next:** [Lesson 8 — Integrals](lesson08_integrals.md)
