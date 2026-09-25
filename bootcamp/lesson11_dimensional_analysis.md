# Lesson 11 (bonus) — Dimensional analysis

Physicists have a trick for guessing the answer to a problem before solving it: **the units have to work out**. A pendulum's period is a time. If it can only depend on the pendulum's length L, the bob's mass m and gravity g, there is exactly one way to combine them into a time: √(L/g). So T = C √(L/g) for some pure number C, without writing down a single differential equation. (Solving the equation gives C = 2π.)

This trick is called **dimensional analysis**, and the theorem behind it is the **Buckingham Π theorem**. Fermium knows the units of everything, so it can do the bookkeeping for you.

## Your first analysis: the pendulum

```fermium
analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]
```

<!-- output -->
```
dimensional analysis of pendulum: T depends on L, m, g
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = T √(g/L)
  so T ∝ √(L/g)   (T = C √(L/g), with C a pure number)
  m drops out: nothing else has mass
  defined pendulum(L, g) = √(L/g), so T = C pendulum(L, g)
```

The line reads like the sentence you'd say: "analyze the pendulum: T depends on L, m and g". The name after `analyze` (`pendulum`) is yours to choose. Each quantity gets its unit in square brackets, as in a function definition. Only the kind of unit matters: `[cm]` or `[km]` would mean the same as `[m]`.

What Fermium tells you:

- **Π₁ = T √(g/L)** is a **dimensionless group**: a combination with no units at all. Check it: √(g/L) is 1/s, times T in seconds.
- Whatever the physics is, a formula that is true in any units can only relate dimensionless groups. With one group, that means Π₁ is a constant, so **T ∝ √(L/g)** (∝ means "is proportional to").
- **m drops out**: the mass is the only quantity with kilograms in it, so nothing can cancel them. The period doesn't depend on the mass. Galileo found that by experiment; here it falls out of the units.
- The number of groups is always (number of quantities) − (number of independent dimensions): 4 − 3 = 1 here.

Dimensional analysis can't give you the pure number C. For that you need the real theory, or an experiment.

## Using the result

`analyze pendulum` also defined a function, `pendulum(L, g) = √(L/g)`, so you can use the answer right away. Here it is with the lab data from Lesson 6: fit the one unknown number C.

```fermium
data = load "data/pendulum.csv"
analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]
fit T = C pendulum(L, 9.81 m/s²) to data
print "C =", C, "and 2π =", 2π
```

<!-- output -->
```
dimensional analysis of pendulum: T depends on L, m, g
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = T √(g/L)
  so T ∝ √(L/g)   (T = C √(L/g), with C a pure number)
  m drops out: nothing else has mass
  defined pendulum(L, g) = √(L/g), so T = C pendulum(L, g)
fit T = C pendulum(L, 9.81 m/s²)   (7 data points from data/pendulum.csv)
  C = 6.269   (standard error 0.012)
  rms residual = 0.00850 s
C = 6.27 and 2π = 6.28
```

The data says C ≈ 6.27, within about one standard error of 2π. Units predicted the shape of the law; the experiment measured the number.

## Constants of nature

Built-in constants like `G`, `c` and `ħ` can go after `depends on` without brackets, since Fermium already knows their units. The same goes for variables you defined earlier in the program.

What length can you make from gravity (G), quantum mechanics (ħ) and relativity (c)? Only one: the **Planck length**, the scale where all three matter at once.

```fermium
analyze planck: ℓ [m] depends on G, ħ, c
```

<!-- output -->
```
dimensional analysis of planck: ℓ depends on G, ħ, c
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = ℓ √(c³/(G ħ))
  so ℓ ∝ √(G ħ/c³)   (ℓ = C √(G ħ/c³), with C a pure number)
  defined planck = √(G ħ/c³) = 1.62×10⁻³⁵ m
```

Because everything in the formula is a constant, `planck` is simply a number (with units), and Fermium prints it: 1.6 × 10⁻³⁵ m.

Kepler's third law comes out the same way. A planet's period T can depend on the size of its orbit a, on G, and on the Sun's mass M:

```fermium
analyze kepler: T [s] depends on a [m], G, M [kg]
print "one year:", 2π kepler(AU, M_sun) in day
```

<!-- output -->
```
dimensional analysis of kepler: T depends on a, G, M
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = T √(G M/a³)
  so T ∝ √(a³/(G M))   (T = C √(a³/(G M)), with C a pure number)
  defined kepler(a, M) = √(a³/(G M)), so T = C kepler(a, M)
one year: 365 day
```

T² ∝ a³: Kepler's third law, from units alone. `G` is a constant, so it isn't an argument of `kepler(a, M)`. With C = 2π (from Newton's laws) you get the length of the year.

## The atomic bomb

In 1950, G. I. Taylor worked out the energy of the first atomic bomb (the Trinity test of 1945), which was still secret, from photographs of its fireball that had been released to the public. The fireball's radius R depends on the energy released E, the density of the air ρ, and the time t since the explosion. Turn it around: E depends on R, ρ and t.

```fermium
analyze taylor: E [J] depends on R [m], ρ [kg/m³], t [s]
E_trinity = taylor(130 m, 1.2 kg/m³, 25 ms)
print "E ≈", E_trinity
print "  ≈", E_trinity / (4.184e12 J), "kilotons of TNT"
```

<!-- output -->
```
dimensional analysis of taylor: E depends on R, ρ, t
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = E t²/(R⁵ ρ)
  so E ∝ R⁵ ρ/t²   (E = C R⁵ ρ/t², with C a pure number)
  defined taylor(R, ρ, t) = R⁵ ρ/t², so E = C taylor(R, ρ, t)
E ≈ 7.1×10¹³ J
  ≈ 17 kilotons of TNT
```

The photos showed a radius of about 130 m after 25 ms. Taylor's C turned out to be close to 1 (his own estimate was 17 kilotons); the official figure, declassified much later, was about 20 kilotons. Notice the R⁵: a 5% error in the radius makes a 25% error in the energy, so an estimate like this is only good to a few tens of percent.

## More than one group

When there are more quantities, there can be more than one dimensionless group, and the answer contains an unknown **function** instead of an unknown number. The drag force F on an object depends on the air's density ρ, the speed v, the object's size (its area A) and the air's viscosity μ:

```fermium
analyze drag: F [N] depends on ρ [kg/m³], v [m/s], A [m²], μ [Pa s]
```

<!-- output -->
```
dimensional analysis of drag: F depends on ρ, v, A, μ
  5 quantities, 3 independent dimensions (length, mass, time) → 5 − 3 = 2 dimensionless groups
  Π₁ = F/(ρ v² A)
  Π₂ = ρ v √A/μ
  so F = ρ v² A · f(Π₂)   (f is a function dimensional analysis can't give)
  defined drag(ρ, v, A) = ρ v² A, so F = drag(ρ, v, A) · f(Π₂)
```

Π₂ is the **Reynolds number** (with √A as the size), the most important number in fluid mechanics. Its value tells you whether a flow is smooth or turbulent. The result says F = ρ v² A · f(Re): engineers write it as F = ½ C_d ρ v² A, where the drag coefficient C_d depends only on the Reynolds number. Measure C_d once in a wind tunnel for a small model, and you know it for the full-size car at the same Reynolds number.

The order you write the quantities in matters a little: Fermium builds the groups from the first ones that are independent (here ρ, v and A), so that's where the answer's prefactor ρ v² A comes from. List the quantities you want in the formula first.

A pure number counts as a quantity too. A pendulum swinging through a big angle θ₀ (Lesson 9) has two groups, so its period is √(L/g) times some function of the amplitude:

```fermium
analyze swing: T [s] depends on L [m], g [m/s²], θ₀ [1]
```

<!-- output -->
```
dimensional analysis of swing: T depends on L, g, θ₀
  4 quantities, 2 independent dimensions (length, time) → 4 − 2 = 2 dimensionless groups
  Π₁ = T √(g/L)
  Π₂ = θ₀
  so T = √(L/g) · f(Π₂)   (f is a function dimensional analysis can't give)
  defined swing(L, g) = √(L/g), so T = swing(L, g) · f(Π₂)
```

`[1]` means "a pure number" (an angle in radians counts as one). For small swings, f is 2π. Exercise 2 of Lesson 9 finds f for big ones.

## When it can't work

If the quantities you listed can't make the target's units, there's no formula, and Fermium says why:

<!-- run as oops.fm -->
```
analyze fall: v [m/s] depends on m [kg], t [s]
```

<!-- output -->
```
oops.fm, line 1: v can't be made from m, t: v has length, but nothing it depends on has length; so there is no dimensionless group at all (3 quantities, 3 independent dimensions)
    analyze fall: v [m/s] depends on m [kg], t [s]
                  ^^^^^^^
  hint: v must depend on something else too (a constant like G, c or ħ?)
```

The speed of a falling object can't depend only on its mass and the time: nothing there has metres in it. That's a sign you forgot something (here, g). Dimensional analysis is a good way to find the missing ingredient of a problem.

## How it works

Fermium writes the base units of every quantity as a column of exponents (the **dimension matrix**): for the pendulum, T is (length 0, mass 0, time 1), L is (1, 0, 0), m is (0, 1, 0) and g is (1, 0, −2). A dimensionless group Tᵃ Lᵇ mᶜ gᵈ needs every row to add up to zero, which is a set of linear equations for a, b, c, d. Fermium solves them exactly, with fractions (so √ and fifth roots come out exact), and picks the solution where T appears in exactly one group, to the power 1. That's what lets it write "T = …". The rank of the matrix (the number of independent dimensions) says how many groups there are.

## Summary

- `analyze name: T [s] depends on L [m], m [kg], g [m/s²]` finds the dimensionless groups (Buckingham Π) and what they say about T.
- Units go in brackets. Built-in constants (`G`, `c`, `ħ`, `e`, `m_e`, …) and variables you already defined need no brackets.
- One group: T ∝ (a formula). Several groups: T = (a formula) · f(the other groups).
- It defines `name(...)` (the formula, with only the non-constant quantities as arguments), ready to use in `print`, `fit` and your own formulas. With only constants, `name` is a plain value.
- Dimensional analysis can't give pure numbers like 2π. Fit them to data or solve the real equations.

## Exercises

1. **Water waves.** The speed v of waves on deep water depends on gravity g and the wavelength λ. What does dimensional analysis say? How fast is a wave with λ = 100 m, taking C = 1/√(2π) from the full theory?
2. **Hydrogen.** The energy of the hydrogen atom can only depend on the electron's mass (`m_e`), its charge (`e`), the permittivity of the vacuum (`ε₀`) and Planck's constant (`ħ`). Find the formula. The exact result has C = 1/(32π²): print E / (32π²) in eV and compare with 13.6 eV.
3. **Black holes.** What radius can you build from G, a mass M and c? Evaluate it for the Sun's mass (`M_sun`). (The real Schwarzschild radius has C = 2.)
4. **The speed of sound.** The speed of sound v in a gas depends on its pressure P and density ρ. What's the formula? For air, P = 101 kPa and ρ = 1.20 kg/m³. Compare with the measured 343 m/s (the missing C is √1.4).
5. **Challenge: a sphere in honey.** The drag on a small sphere moving slowly through a very viscous liquid depends only on the viscosity μ, the speed v and the radius r (not on the density). Show that F ∝ μ v r. Then add the density ρ and see what the second group is.

Solutions: [solutions/lesson11.md](solutions/lesson11.md)

**Back to:** [the list of lessons](README.md)
