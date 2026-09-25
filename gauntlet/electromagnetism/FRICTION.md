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

## Second pass

Problems: `21_crossed_fields.fm` (a proton in E ⊥ B as a 3-D vector ODE, against the cycloid),
`22_ring_disk_off_axis.fm` (a charged ring off the axis by a vector Coulomb integral and by −∇V of an
integral potential, against Jackson's elliptic-integral formulas; a charged disk as a double integral),
`23_laplace_box.fm` (Laplace's equation in a square pipe and a cube by Fourier series: V₀/4 and V₀/6
at the centres, the mean-value property, induced charge).

First-pass fixes that helped: vector integrals (#19) made Coulomb's law for the ring one line,
`∫ k λ a (P − ring(φ)) / |P − ring(φ)|^3 dφ from 0 to 2π`; the Leibniz rule (#18) made
`-∇V(P)` of `V(x, y, z) = ∫ … dφ` agree with the direct field to 8 digits; the integral-limit rule
(#8) let `∫ φ(…) dθ from 0 to 2π / (2π)` mean the average; the `3 T` hint (#10) caught
`for t from 0 s to 3 T` (T was the cyclotron period) immediately and suggested `3*T`; `solve … for z`
(D32) found the on-axis field maximum a/√2 to 8 digits. The vector ODE `M r'' = q (E + r' × B)` read
exactly like the Lorentz force law and matched the cycloid to 8 digits.

### E9. A nested integral inside a function reads garbage for the function's parameter — wrong answer

```
f(z) = ∫ (∫ z dφ from 0 to 1) ds from 0 to 1
print f(2)            # 6.9×10⁻³¹⁰ (should be 2)
g(z) = ∫ (∫ z / (s² + z²) dφ from 0 to 1) ds from 0 to 1
print g(2)            # 0, or on another run: "this integral doesn't converge ... ∞ ± NaN" with no line
```
The same happens in a multi-line function body. At the top level (`z = 2` then the same double
integral) the answer is right, and so is the single integral `∫ z / (s² + z²) ds`. In
`22_ring_disk_off_axis.fm` the on-axis field of the disk, `Ez_disk(z) = ∫ (∫ k σ s z / (s² + z²)^(3/2) dφ …) ds …`,
first printed nothing and failed with "doesn't converge (estimate 3×10⁻²⁸⁸)", and the error showed the
source text of a different line (two lines up), so it looked like the previous integral was at fault.
Workaround: a helper function for the inner integral, `Ez_ring(s, z) = ∫ … dφ …`, then
`Ez_disk(z) = ∫ Ez_ring(s, z) ds …` (correct to 10 digits).
Fix: the inner integrand's closure must capture the enclosing function's parameters (it seems to read
an uninitialised slot); a regression test with `f(z) = ∫ (∫ z dφ …) ds …`. Report the line of the
integral that failed.

### E10. A vector integral whose component is 0 by symmetry "doesn't converge" when nested — bug

```
a = 10.0 cm
P = <6.00, 0, 4.00> cm
print ∫ (∫ s (P - <s cos(φ), s sin(φ), 0 m>) / |P - <s cos(φ), s sin(φ), 0 m>|^3 dφ from 0 to 2π) ds from 0 m to a
```
Got: `this integral doesn't converge ... the estimate was 0.00297617 ± 3.7×10⁻¹³` (the y component,
which is 0 by symmetry; the inner integrals leave rounding noise ~10⁻¹³ of the x and z components, and
a relative test on a result that should be 0 cannot pass). A single-level vector integral with a zero
component (the ring) is fine. The message is misleading: nothing blows up or oscillates.
Workaround: integrate the x and z components separately.
Fix: judge convergence of a vector integral on the norm of the vector (or give each component an
absolute tolerance of 10⁻¹⁰ × the largest component); for scalars, an absolute floor relative to
∫|f|, and say "the integral is 0 to within rounding" instead of "doesn't converge".

### E11. `∇²` of `term(n, x, y)` never finishes compiling; `∇` takes every parameter as a coordinate — bug

```
term(n, x, y) = 4 V0 / (n π) * sin(n π x / a) * sinh(n π y / a) / sinh(n π)
print ∇²term          # no output after 60 s (killed)
g(n, x, y) = n² x² y
print ∇²g(2, 1 m, 1 m)   # "can't add a plain number (no units) to area [m²]"
```
A Fourier term naturally has the mode number as a parameter. `∇` silently takes the plain number n as
a third Cartesian coordinate; the unit error that follows doesn't say why, and for the real term the
compiler hangs (SymPy simplification?) instead of reporting anything.
Workaround: `(∂²/∂x² term)(n, x, y) + (∂²/∂y² term)(n, x, y)` spelled out.
Fix: require ∇'s coordinates to share one dimension and say so ("∇ takes n, x, y as coordinates, but
n is a plain number and x is a length"); allow naming the coordinates (`∇²_(x,y) term` or
`laplacian(term, x, y)`); put a time limit on SymPy simplification.

### E12. `∇²`/`∂` of a function defined by a sum (over several lines) is refused — awkward

`φ(x, y)` is a Fourier series, so it has to be a loop over several lines, and
`∇²φ` gives `∇² can only differentiate one-line functions`. Checking that a series solves Laplace's
equation is the natural test. Workaround: differentiate one term (see E11) and check the sum with the
mean-value property (an integral over a circle, which works well).
Fix: a one-line sum, `φ(x, y) = Σ(n from 1 to N step 2) term(n, x, y)` (also useful everywhere a
series appears), which the symbolic differentiator can go through term by term.

### E13. `∂/∂z V(0 cm, 0 cm, z1)` is refused while `∇V(…)` is applied — awkward

```
print -∂/∂z V(0 cm, 0 cm, z1)
```
Got: `d/dz(...) is a function; give it an argument, like d/dz(...)(x)` (it also says `d/dz` for a `∂/∂z`).
`-∇V(0.3 m, 0.4 m, 0 m)` applies the gradient to V and then calls it, so the partial derivative
behaves differently from ∇ written the same way. Had to write `-(∂/∂z V)(0 cm, 0 cm, z1)`.
Fix: parse `∂/∂z V(args)` like `∇V(args)`: (∂V/∂z)(args) when V is a function of several variables;
spell the message with the operator the user wrote.

### E14. A second partial derivative prints as `∂2term/∂x2` — cosmetic

`print ∂²/∂x² term` shows `∂2term/∂x2(n, x, y) = -4V0 n π sin(n π x/a) …`. Expected `∂²term/∂x²`.

### E15. A unit after a vector literal warns about the variable `m` — cosmetic

With `m = m_p` defined, `with r(0) = <0, 0, 0> m, …` warns `'m' right after a number is the unit m,
not your variable m` and hints `2*m`. The unit is right, but it isn't after a number and the hint
doesn't apply. Worked around by calling the proton mass `M`.
Fix: word the warning for a vector literal (`'m' after <…> is the unit`), or don't warn after `>`,
where multiplying a vector by a mass would be written `m <…>` anyway.

### E16. No elliptic integrals or Bessel functions — awkward (mild)

The off-axis ring is Jackson's textbook example of K(m) and E(m); Fermium has neither, so the closed
form lives only in the test (SciPy) and the program checks the integral against −∇ of another
integral. Fix: `ellipk`, `ellipe`, `besselj(n, x)` (see optics O4).

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#80 (C)** Display units: a conductivity ε₀ω_p²/γ prints as F/(m s), not S/m; a speed squared a²ω² as J/kg
