# Lesson 8 — Integrals

Integrals are everywhere in physics: work is the integral of force, the mass of a rod is the integral of its density, the power radiated by a star is the integral of its spectrum. In Fermium, an integral is written almost exactly as on paper.

## Your first integral

∫₀³ x² dx = 9. In Fermium:

```fermium
print integral x^2 dx from 0 to 3
print ∫ x^2 dx from 0 to 3
```

<!-- output -->
```
9
9
```

The parts are:
- `integral` (or `∫`, Option + B on a Mac),
- the thing to integrate, the **integrand**: `x^2`,
- `dx`, which says that `x` is the integration variable (it can be any name: `dt`, `dr`, `dlam`),
- `from` lower limit `to` upper limit.

Fermium computes the number with a precise numerical method (adaptive Gauss–Kronrod quadrature, if you want to look it up), accurate to about 10 digits.

A few classics:

```fermium
print integral sin(x) dx from 0 to pi
print integral 1 / sqrt(x) dx from 0 to 1
print integral exp(-x^2) dx from -inf to inf
print sqrt(pi)
```

<!-- output -->
```
2
2
1.77
1.77
```

Infinite limits work: write `inf` (or `∞`). The Gaussian integral gives √π, as it should.

## Integrals with units: work done by a spring

The work done stretching a spring from 0 to 20 cm is W = ∫ F dx, with F = kx:

```fermium
k = 50 N/m
F(x) = k x
W = integral F(x) dx from 0 m to 20 cm
print W
print "check: ½ k x² =", ½ k (20 cm)^2
```

<!-- output -->
```
1 J
check: ½ k x² = 1 J
```

Fermium knows that the result has the units of the integrand **times** the units of `x`: newtons × metres = joules. The limits must have the same units as each other (here both lengths; mixing m and cm is fine).

## Example: mass of a rod

A 2 m rod gets denser toward one end: its linear density is λ(x) = 2 kg/m + (1 kg/m²) x. Its mass is M = ∫λ dx, and its centre of mass is x_cm = (∫ x λ dx) / M:

```fermium
density(x) = 2 kg/m + 1 kg/m^2 x
M = integral density(x) dx from 0 m to 2 m
x_cm = (integral x density(x) dx from 0 m to 2 m) / M
print "mass:", M
print "centre of mass:", x_cm
```

<!-- output -->
```
mass: 6 kg
centre of mass: 1.11 m
```

⚠️ Notice the **parentheses** around the second integral. Without them, `... from 0 m to 2 m / M` would mean "up to (2 m / M)", and Fermium would complain that the limits have different units.

## Integrals inside loops

The upper limit can be a variable. Here's the velocity of a car that starts from rest and whose acceleration grows with time, a(t) = (2 m/s³) t. The velocity at time T is v = ∫₀ᵀ a dt:

```fermium
a(t) = 2 m/s^3 t
for T from 0 s to 3 s step 1 s
    v = integral a(t) dt from 0 s to T
    print "at", T, "velocity is", v
```

<!-- output -->
```
at 0 s velocity is 0 m/s
at 1 s velocity is 1 m/s
at 2 s velocity is 4 m/s
at 3 s velocity is 9 m/s
```

## Example: the Sun's power

A star radiates like a **blackbody**. Planck's law gives the intensity per wavelength:

B(λ) = 2hc² / λ⁵ · 1 / (e^(hc/λk_BT) − 1)

Integrating over all wavelengths (and multiplying by π) should give the Stefan–Boltzmann law, σT⁴. Let's check it for the Sun, T = 5778 K:

```fermium
T = 5778 K
B(lam) = 2 h c^2 / lam^5 / (exp(h c / (lam k_B T)) - 1)
flux = pi * integral B(lam) dlam from 10 nm to 100 um
print "integral of Planck:", flux
print "Stefan-Boltzmann: ", sigma T^4
print "power of the Sun:", flux * 4 pi R_sun^2
```

<!-- output -->
```
integral of Planck: 6.32×10⁷ W/m²
Stefan-Boltzmann:  6.32×10⁷ W/m²
power of the Sun: 3.84×10²⁶ W
```

Planck's law and the Stefan–Boltzmann law agree, and multiplying by the Sun's surface area gives its total power, about 3.8 × 10²⁶ W. (We integrated from 10 nm to 100 μm because that's where nearly all the light is; outside that range, B is tiny.)

Remember `exp(...)`, not `e^(...)`: in Fermium, `e` is the charge of the electron. A classic mistake is `integral 1 / x dx from 1 to e`, which Fermium rejects because `e` has units of charge!

## Integrals without limits

If you leave out `from … to …`, Fermium tries to find the formula (the *antiderivative*), and gives you back a function:

```fermium
F = integral cos(x) dx
print F(pi / 2)
G = integral x^2 dx
print G(3)
```

<!-- output -->
```
1
9
```

This uses a separate maths library (SymPy), so it only works for formulas that have a neat answer. When you need a number, use limits.

## Summary

- `integral f(x) dx from a to b` (or `∫ … dx from a to b`) computes a definite integral.
- The result's units are the integrand's units × the variable's units.
- Limits can be `inf`, variables, or expressions. Put parentheses around an integral before dividing it: `(∫ … dx from a to b) / M`.
- Without limits, `integral f(x) dx` gives an antiderivative function.

## Exercises

1. **Area.** Compute ∫₀^π sin²(x) dx. What exact value do you expect?
2. **Gravitational work.** How much work is needed to lift a 1000 kg satellite from the Earth's surface (`R_earth`) to infinity? Integrate F(r) = G M_earth m / r² from R_earth to `inf`. Compare with ½ m v_esc², where v_esc = 11.2 km/s.
3. **Charging a capacitor.** The current into a capacitor is I(t) = 2 mA · exp(−t / 3 s). How much charge flows in during the first 10 s? In total (to infinity)? (Remember: `exp`, not `e^`.)
4. **Hot plate.** A 50 cm long metal bar has temperature T(x) = 300 K + (400 K/m) x. What's the average temperature, (1/L) ∫ T dx?
5. **Wien's peak.** Using the Planck function B(λ) from the lesson with T = 5778 K, find the wavelength where B is largest by looping over λ from 100 nm to 2000 nm in steps of 1 nm. Compare with Wien's law, λ_max = b_W / T (the constant `b_W` is built in).

Solutions: [solutions/lesson08.md](solutions/lesson08.md)

**Next:** [Lesson 9 — Differential equations](lesson09_differential_equations.md)
