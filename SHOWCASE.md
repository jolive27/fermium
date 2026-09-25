# Fermium showcase

Five things Fermium does that general-purpose languages don't. Every snippet is copy-pasteable into a file
and runnable with `fermium run file.fm` (tests/test_showcase.py runs them all and checks the output shown).

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
k = 50 N/m
b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 10 cm, x'(0) = 0 m/s
  for t from 0 s to 5 s
print x(5 s)
```

`∇φ` prints the symbolic field `<-q x/(4π ε_0 (x² + y² + z²)^(3/2)), …>`; evaluated it gives V/m. The damped
spring is solved adaptively; the answer comes back in cm because that's how x(0) was written.

## 3. Natural units that are still unit-checked

```fermium
units natural(ħ = c = 1)
a0 = 1/(α m_e)
print "Bohr radius:", a0 in fm to 6 digits, "=", a0 in Å to 6 digits
m_π = 139.57039 MeV
print "range of the nuclear force:", 1/m_π in fm
```

Particle physicists write ħ = c = 1; Fermium keeps checking dimensions *modulo* ħ and c (an energy plus an
inverse energy is still an error) and converts back to SI exactly: 52917.7 fm = 0.529177 Å, and 1.4138 fm.

## 4. Dimensional analysis on demand

```fermium
analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]
```

Fermium builds the dimension matrix, finds the Buckingham Π groups with exact rational arithmetic, and
reports that T ∝ √(L/g) and that the mass drops out, and defines `pendulum(L, g)` for a fit.

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
(research/semf_ame2020/) show the magic numbers 50, 82 and 126.
