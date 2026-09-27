# Fermium showcase

Five things Fermium does that general-purpose languages don't. Every snippet is copy-pasteable into a file
and runnable with `fermium run file.fm` (legacy/tests/test_showcase.py runs them all and checks the output shown).

## 1. The compiler knows physics units, before anything runs

```fermium
L = 1.20 m
T = 2.21 s
g = 4π² L / T²
print g
print g in ft/s²
```

Output: `9.70 m/s²` and `31.8 ft/s²`. Writing `y = L + T` stops the program before it runs:
`line 6: can't add length [m] to time [s]`. Units cost nothing at run time: they are erased before LLVM
code generation.

## 2. Calculus is part of the language: derivatives, ∇, integrals and ODEs

```fermium
q = 1 nC
φ(x, y, z) = q / (4π ε₀ √(x² + y² + z²))
print ∇φ
print -∇φ(0 m, 3 m, 4 m)

m = 0.5 kg
k = 50 [N/m]
b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 10 cm, x'(0) = 0 m/s
  for t from 0 s to 5 s
print x(5 s)
```

`∇φ` prints the symbolic field `<-q x/(4π ε_0 (x² + y² + z²)^(3/2)), …>`; evaluated it gives V/m. The damped
spring is solved adaptively; the answer comes back in cm because that's how x(0) was written.

## 3. Error bars like a lab report: correlations, and Monte Carlo when linear propagation fails

```fermium
L = 1.000 ± 0.002 m
T = 2.007 ± 0.005 s
g = 4π² L / T²
print "g =", g
x = 5.0 ± 0.2 m
print "x - x =", x - x
propagate montecarlo 100000 samples
    v = 20.0 ± 0.5 m/s
    θ = 45° ± 5°
    R = v² sin(2θ) / g_n
print "range:", R
```

Output: `g = 9.801 ± 0.053 m/s²`, `x - x = 0 ± 0 m` and `range: 40.2 ± 2.2 m`.
- Uncertainties carry their units and their correlations, so a quantity minus itself has no error.
- For the projectile range at 45°, linear propagation says 40.8 ± 2.0 m: the derivative with respect to θ is zero there, so the angle's uncertainty disappears.
- The seeded Monte Carlo sees the curvature, and its mean matches the exact value, v² e^(−2σθ²)/g = 40.2 m.

## 4. Quantum mechanics in a few lines: bound states of a finite well

```fermium
a = 0.5 nm
V(x) = if |x| < a then 0 eV else 5 eV
solve -ħ²/(2 m_e) * ψ'' + V(x) ψ = E ψ
    with ψ(-2 nm) = 0, ψ(2 nm) = 0
    for x from -2 nm to 2 nm
    lowest 3
print "finite well levels:", E in eV to 4 digits
print "probability outside the well, ground state:", 1 - ∫ ψ₁(x)² dx from -a to a
```

Output: `[0.2718, 1.077, 2.379] eV` and `0.0083`.
- The Schrödinger equation is written as on paper. `E` is the one undefined name, so Fermium treats it as the eigenvalue.
- It finds the three lowest levels and their normalised wavefunctions `ψ₁ … ψ₃`, which can be integrated, differentiated and plotted.
- The levels agree with the transcendental equations of a finite well to 10⁻⁶ (legacy/tests/test_m3_eigen.py).
- The same language does 1-D PDEs (heat, wave, time-dependent Schrödinger with animated GIFs), complex numbers (`1i ħ ψ' = E ψ`), natural units (`units natural(ħ = c = 1)`, still unit-checked) and dimensional analysis (`analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]` gives T ∝ √(L/g)).

## 5. Real data, real physics: the liquid-drop model against AME2020

```fermium
nuclei = load "research/semf_ame2020/ame2020_binding.csv"
a_V = 15 MeV
a_S = 17 MeV
a_C = 0.7 MeV
a_A = 23 MeV
a_P = 12 MeV
fit B = a_V A - a_S A^(2/3) - a_C Z (Z - 1) / A^(1/3) - a_A (A - 2Z)² / A + P a_P / √A to nuclei
print err(a_V)
```

A five-parameter fit to 2484 measured nuclear binding energies, each term unit-checked; the residuals
(research/semf_ame2020/) show the magic numbers 50, 82 and 126. research/ has eight more reproductions compared with
published numbers: TOV neutron stars, the Chandrasekhar mass, the U-238 chain, hydrogen levels, the Planck 2018 age of
the universe, Rutherford scattering by Monte Carlo, the pp/CNO crossover and a Big Bang nucleosynthesis network
(Y_p = 0.2423, D/H = 2.60×10⁻⁵ with the stiff solver, agreeing with SciPy to 3×10⁻⁶).
