# Friction: electromagnetism (first pass)

Problems: `01_finite_line_charge.fm`, `02_loop_biot_savart.fm`, `03_rlc_circuit.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

What went well: `E = -∇V(x, y, z)` on a closed-form potential gives the field with units V/m;
vector functions with `×` and `|…|³` make Biot–Savart readable; `solve L q'' + R q' + q / C = 0`
(a bare 0 on the right) works; `∫ q'(t)² R dt` over an ODE solution works; `μ₀`, `ε₀` and `Ω`, `μF`,
`mH` all read like the textbook.

## E1. `∇` of a function defined by an integral is impossible — blocker (for that route)

Wanted: the potential of the rod as the integral it is, then the field from it:
```
V(x, y, z) = ∫ k λ / √((x - s)^2 + y^2 + z^2) ds from -L/2 to L/2
print -∇V(0.3 m, 0.5 m, 0 m)
```
Got: `line 10: can't differentiate an integral whose integrand depends on the variable`, pointing at
the definition of V, not at the `∇` on line 12 that asked for the derivative.
Had to write: the closed-form antiderivative of the potential by hand (`k λ ln((x + L/2 + r₊)/(x − L/2 + r₋))`).

Fix: differentiate under the integral sign (Leibniz rule: d/dx ∫ f(x, s) ds = ∫ ∂f/∂x ds, plus boundary
terms when the limits depend on x). And point the error at the `∇`/`'` that triggered it.

## E2. Integrals of vectors are not supported — awkward

Wanted: `E = ∫ dE(s) ds from -L/2 to L/2` and `B = ∫ dB(φ, P) dφ from 0 to 2π` (Coulomb and
Biot–Savart are vector integrals).
Got: `the thing being integrated must be a number, but it is a 3-vector ...`.
Had to write one integral per component: `Ex = ∫ dE(s).x ds …`, `Ey = …`, `Bz = …` and rebuild the
vector by hand.

Fix: integrate each component (the same adaptive rule, with a shared error estimate) and return a
vector.

## E3. `R <cos(φ), sin(φ), 0>` is parsed as `R < cos(φ)` — awkward, bad error

Wanted: `ring(φ) = R <cos(φ), sin(φ), 0>` (scalar times vector, by juxtaposition, like `2 v`).
Got: `didn't expect ',' here`, pointing at the first comma — no hint that `<` was read as less-than.
Had to write: `R * <cos(φ), sin(φ), 0>`.

Fix: after a name/number with a space before `<` and a matching `>` later on the same line with commas
at depth 0, parse a vector literal; at the very least, when the comparison parse fails at a comma,
hint "to multiply by a vector write `R * <…>`".

## E4. `μ₀ I / (4π) dl × r` divides by dl — awkward (same trap as thermodynamics T4)

Wanted: `dB = μ₀ I / (4π) dl(φ) × (P - ring(φ)) / |P - ring(φ)|³` — the Biot–Savart law as printed.
Got: `can't divide by a vector` (because it is μ₀I / ((4π) dl)). Here the error at least fires; with
scalars the same shape silently gives a wrong formula (see thermodynamics T4).
Had to write: `μ₀ I / (4π) * dl(φ) × …`.

Fix: warn on `a / (b) c` (juxtaposition after a parenthesised divisor), as in thermodynamics T4.

## E5. An integral's upper limit swallows a following `/ …` — awkward, silent

Wanted: `print ∫ B_axis(z) dz from -∞ to ∞ / (μ₀ I)` (the ratio should be 1).
Got: `2.5132741×10⁻⁶ kg m/(s² A)` — the upper limit was read as `∞ / (μ₀ I)`, so the division was lost.
It was only obvious because the units came out as T·m instead of a plain number. With a finite limit
`∫ f(u) du from 0 to 1 / 2` silently integrates to ½.
Had to write: `(∫ B_axis(z) dz from -∞ to ∞) / (μ₀ I)`.

Fix: end the limit expression at a `/` or `*` preceded by a space (or warn when the upper limit is a
product or quotient without parentheses), or require parentheses for compound limits.

## E6. Circuit symbols collide with units: `R/(2L)`, `q/(2C)` — awkward

Wanted: `α = R / (2L)` and `U = q² / (2C)` — L (inductance) and C (capacitance) are *the* circuit names.
Got: two clear warnings (`'L' right after a number is the unit L, not your variable L`) and then a
unit error, because `2L` is 2 litres and `2C` is 2 coulombs.
Had to write: `2*L`, `2*C`.
The warnings are good, but in a program that has assigned `L = 50.0 mH`, reading `2L` as litres is
almost never what the author meant.

Fix: when a name is both a user variable with a *different* dimension than the unit and it follows a
number, prefer the variable (or make it an error with the hint, instead of a warning plus a confusing
unit error on a later line).

## E7. Indefinite integral: no line number, suspicious antiderivative — bad error message

Repro:
```
a = 0.5 m
F = ∫ 1 / √(a^2 + s^2) ds
```
Got: `SymPy returned something Fermium can't use yet: asinh(Abs(s/a))` — no file, no line, no caret.
The expression itself is also wrong for s < 0 (the antiderivative is asinh(s/|a|), which is odd in s).
The same message appears when `a` is not defined at all (the undefined name is not reported first).
And `∫ 1/√((x - s)^2 + (0.5 m)^2) ds` gives `this integral is too complicated to do symbolically`.

Fix: attach the source location; support `asinh`/`Abs` (or run SymPy with `positive=True` symbols for
quantities with units, which avoids the Abs); check names before calling SymPy.

## E8. Units in messages are shown in base SI — cosmetic

The error in E2 shows the field as `kg/(s³ A)` rather than `V/m`; E5 shows T·m as `kg m/(s² A)`.
Fix: pick a derived-unit display (V/m, N/C, T m) in diagnostics as `print` already does for simple cases.
