# Solutions — Lesson 11

[Back to Lesson 11](../lesson11_dimensional_analysis.md)

## 1. Water waves

```fermium
analyze waves: v [m/s] depends on g [m/s²], λ [m]
print "λ = 100 m:", waves(g_n, 100 m) / √(2π)
```

<!-- output -->
```
dimensional analysis of waves: v depends on g, λ
  3 quantities, 2 independent dimensions (length, time) → 3 − 2 = 1 dimensionless group
  Π₁ = v/√(g λ)
  so v ∝ √(g λ)   (v = C √(g λ), with C a pure number)
  defined waves(g, λ) = √(g λ), so v = C waves(g, λ)
λ = 100 m: 12.5 m/s
```

v ∝ √(g λ): longer waves are faster. That's why the long swell from a distant storm arrives at the beach before the short, choppy waves.

## 2. Hydrogen

```fermium
analyze hydrogen: E [J] depends on m_e, e, ε₀, ħ
print hydrogen / (32 π²) in eV
```

<!-- output -->
```
dimensional analysis of hydrogen: E depends on m_e, e, ε₀, ħ
  5 quantities, 4 independent dimensions (length, mass, time, current) → 5 − 4 = 1 dimensionless group
  Π₁ = E ε₀² ħ²/(m_e e⁴)
  so E ∝ m_e e⁴/(ε₀² ħ²)   (E = C m_e e⁴/(ε₀² ħ²), with C a pure number)
  defined hydrogen = m_e e⁴/(ε₀² ħ²) = 6.88×10⁻¹⁶ J
13.6 eV
```

All four are built-in constants, so `hydrogen` is a value. With C = 1/(32π²) = 1/(2 (4π)²) it is the Rydberg energy, 13.6 eV, the energy needed to ionize hydrogen. Dimensional analysis found the whole formula except the number.

## 3. Black holes

```fermium
analyze horizon: r [m] depends on G, M [kg], c
print "Sun:", 2 horizon(M_sun) in km
```

<!-- output -->
```
dimensional analysis of horizon: r depends on G, M, c
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = r c²/(G M)
  so r ∝ G M/c²   (r = C G M/c², with C a pure number)
  defined horizon(M) = G M/c², so r = C horizon(M)
Sun: 2.95 km
```

r ∝ G M/c². Squeeze the Sun inside 3 km and light can't escape.

## 4. The speed of sound

```fermium
analyze sound: v [m/s] depends on P [Pa], ρ [kg/m³]
print sound(101 kPa, 1.20 kg/m³)
print √1.4 sound(101 kPa, 1.20 kg/m³)
```

<!-- output -->
```
dimensional analysis of sound: v depends on P, ρ
  3 quantities, 2 independent dimensions (among length, mass, time) → 3 − 2 = 1 dimensionless group
  Π₁ = v √(ρ/P)
  so v ∝ √(P/ρ)   (v = C √(P/ρ), with C a pure number)
  defined sound(P, ρ) = √(P/ρ), so v = C sound(P, ρ)
290 m/s
340 m/s
```

v ∝ √(P/ρ). With C = 1 this is Newton's formula, which gives about 290 m/s; he couldn't explain the difference from experiment. Laplace later showed that sound waves are adiabatic, which gives C = √γ = √1.4 for air, and 343 m/s.

## 5. Challenge: a sphere in honey

```fermium
analyze stokes: F [N] depends on μ [Pa s], v [m/s], r [m]
analyze sphere: F [N] depends on μ [Pa s], v [m/s], r [m], ρ [kg/m³]
```

<!-- output -->
```
dimensional analysis of stokes: F depends on μ, v, r
  4 quantities, 3 independent dimensions (length, mass, time) → 4 − 3 = 1 dimensionless group
  Π₁ = F/(μ v r)
  so F ∝ μ v r   (F = C μ v r, with C a pure number)
  defined stokes(μ, v, r) = μ v r, so F = C stokes(μ, v, r)
dimensional analysis of sphere: F depends on μ, v, r, ρ
  5 quantities, 3 independent dimensions (length, mass, time) → 5 − 3 = 2 dimensionless groups
  Π₁ = F/(μ v r)
  Π₂ = v r ρ/μ
  so F = μ v r · f(Π₂)   (f is a function dimensional analysis can't give)
  defined sphere(μ, v, r) = μ v r, so F = sphere(μ, v, r) · f(Π₂)
```

Without ρ there's one group, so F = C μ v r (Stokes' law, with C = 6π). With ρ, the second group is the Reynolds number ρ v r/μ (possibly written upside down or squared, which is the same information). Stokes' law is what you get when the Reynolds number is small.
