# Friction: optics and waves (first pass)

Problems: `01_single_slit.fm`, `02_lensmaker.fm`, `03_water_wave_dispersion.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

What went well: the lensmaker and thin-lens equations read exactly as in Hecht; `n(λ) = A + B/λ²`
with `B = 4200 nm²` and `f'(λ_D) in mm/nm` just work (A and B as variable names don't clash with
ampere/…); `v_g = dω/dk` gives a readable symbolic formula; `σ = 0.0728 N/m` quietly replaces the
built-in Stefan–Boltzmann σ; the new `solve … for x from a to b` finds the secondary maximum and the
minimum wave speed to full precision; `for m from 0 to 10` with an `if` for the visible band is a
natural thin-film search (tried, not kept as a problem).

## O1. `solve f'(x) = 0 for x from a to b` is taken for a differential equation — awkward, bad error

Wanted (maximum of a known function, the most common use of root finding in a physics course):
```
I(θ) = (sin(β(θ)) / β(θ))²
solve I'(θ_max) = 0 for θ_max from 1.01 θ1 to 0.99 θ2
```
Got: `missing initial condition: I(start)` with a hint about `with x(0) = …` — `I` is an existing
function, not an unknown.
Had to write: `dI = I'` on its own line, then `solve dI(θ_max) = 0 for θ_max …`. Same for
`dv_p = v_p'` in the dispersion problem.

Fix: in `solve`, a primed name that is already a defined function (and is called with the solve
variable as its argument, with a `for x from a to b` range and no `with`) is a derivative, not an ODE
unknown. A cheaper fix: if every primed name is an already-defined function, treat it as an algebraic
equation.

## O2. `4 g σ / ρ` is 4 grams — awkward (well-known gotcha, good warning)

Wanted: `v_min = (4 g σ / ρ)^(1/4)`, as in the textbook.
Got: a warning (`'g' after the number means the unit g`) and a unit error in the same line.
Had to write: `4*g σ / ρ`.
The warning is clear, and the gotcha is on the cheat sheet, but it is the third time in three topics
(after `2L` and `2C` in electromagnetism E6) that a formula copied from a book breaks this way.

Fix: as electromagnetism E6 — when the program has its own `g` with a different dimension from gram,
read `4 g` as 4 × g (or refuse with the hint instead of warning and then failing on units).

## O3. Reading `in 1/m` prints `1/m` — cosmetic

Wavenumbers print as `367.08684 1/m`; `m⁻¹` works as input (`from 1 m⁻¹ to 1000 m⁻¹`) but is not
used for output. Minor.

## Second pass

Problems: `21_airy_disk.fm` (the Airy pattern with J₁ as an integral: first zero 1.2197 λ/D, FWHM,
first bright ring, encircled energy, Hubble's resolution in mas), `22_fresnel_brewster.fm` (Fresnel
coefficients, Brewster's angle by `solve r_p(θ) = 0`, R + T = 1, total internal reflection),
`23_thin_film_stack.fm` (characteristic matrices: a quarter-wave AR coating and a 16-layer dielectric
mirror, whose stop-band edges come from an algebraic solve on the trace of a period matrix).

First-pass fixes that helped: the Leibniz rule (#18) differentiates `I(x) = (2 J1(x)/x)²` through the
integral that defines J1, so `dI = I'` then `solve dI(x) = 0` finds the first bright ring (x = 5.1356)
to 10 digits; algebraic `solve` (D32) works on functions defined by integrals (the zero of J1) and on
matrix entries (the band edges); `mas` (#16) for Hubble's 57.65 mas; matrices (#22) as function
results and in loops (`M = M layer(…) layer(…)`), and matrices as function parameters
(`R_of(M) = … M[1,1] …`); the `4 nH` hint (#10) named the cause at once.

### O4. No Bessel functions — awkward

The Airy pattern is the textbook use of J₁. Fermium has no `besselj`, so J₀ and J₁ are written as
Bessel's integrals, `J1(x) = (1/π) ∫ cos(τ - x sin(τ)) dτ from 0 to π`. This works well (10 digits,
roots and derivatives included) but a student has to know the integral representation, and see O6.
Fix: `besselj(n, x)` (and `bessely`, `ellipk`, `ellipe`; see electromagnetism E16) with their
derivatives in the symbolic differentiator (J₁' = J₀ − J₁/x).

### O5. A plain-number result is shown in degrees when the argument was in degrees — wrong answer (display)

```
f(θ) = 2 cos(θ)
θ = 60°
print f(θ)           # 57.2958°   (the value is 1)
g(θ) = cos(θ) / cos(θ/2)
print g(θ)           # 33.0797°   (the value is 0.577)
```
In the Fresnel problem the transmittance `T_s = T_factor(θ) t_s(θ)²` printed as `46.78554195°` and
`R_s + T_s` as `57.29577951°` (1 rad), which reads like a failed energy balance. The number is right
underneath (angles are plain numbers), but the display unit "°" of the argument leaks into a result
that is not an angle. `r_s(θ)²` happened not to be affected.
Workaround: `T_s in 1`.
Fix: take a function result's display unit from its own expression only; the result of `cos`, `sin`,
`exp`, … and anything built from them is a plain number. Show degrees only for values that come from
an angle by +, −, or scaling (`θ/2`, `180° − θ`).

### O6. An integral over a function defined by an integral fails far out, with the wrong location — bug

```
J1(x) = (1/π) ∫ cos(τ - x sin(τ)) dτ from 0 to π
I(x) = (2 J1(x) / x)²
print ∫ I(x) x dx from 0 to ∞       # line 3: "doesn't converge ... estimate 4.1×10⁻⁵" (the answer is 2)
print ∫ I(x) x dx from 0 to 2000    # the same, estimate −6.4×10⁻⁵ for a positive integrand
print J1(20000)                     # "this integral doesn't converge ..." with no line at all
```
The inner integral fails for large x (the integrand oscillates x/π times over the range), but the error
is reported on the outer integral's line, with the inner integral's estimate, so it looks like the
outer integral diverges. For a direct call there is no line at all. The encircled-energy
normalisation (∫₀^∞ I x dx = 2) had to be taken from theory instead.
Fix: report the failing inner integral's own line and the outer variable's value ("while computing
J1 at x = 20000"); raise the subdivision limit for smooth oscillatory integrands.

### O7. Matrices can't have a different unit per entry; no complex numbers — awkward

The physical characteristic matrix acts on (E, H) and has entries [[1, Ω], [S, 1]] (the admittance
n/Z₀ appears off the diagonal). `M = [[1, 2 Ω], [3 S, 1]]` gives `all entries of a matrix need the
same units`, though vectors now allow a unit per component. And the matrix is complex
(`i sin δ / n`), which Fermium can't write (#23), so the problem stores the real form
[[cos δ, sin δ/n], [−n sin δ, cos δ]] and writes |r|² out by hand — a trick most students won't know.
Fix: per-entry units for matrices (checked so that products stay consistent, like state vectors),
and complex numbers.

### O8. `4 nH` and `4 nL` are nanohenry and nanolitre — awkward (same as #10)

`λm / (4 nH)` with refractive indices `nH`, `nL` (the usual names for high- and low-index layers)
failed with `cos needs a plain number, but got [F/s²]`. The warning and the #10 note named the cause
exactly (`to multiply by your variable write 4*nH`); renamed to `n_H`, `n_L`.

### O9. The "divides the whole integral" warning fires when that is what was meant — cosmetic

`J1(x) = ∫ cos(τ - x sin(τ)) dτ from 0 to π / π` (Bessel's integral, divided by π) warns on every
such line, although dividing the whole integral is intended and is what happens. The only way to
silence it is to rewrite as `(1/π) ∫ …` or `(∫ …) / π`. Acceptable, but three warnings for three
correct lines is noisy.
Fix: don't warn when the divisor is a constant such as π, or when the limit already ends in the
same name (`π / π`); or mention `(1/π) ∫ …` in the hint.

### O10. No `trace(M)` — cosmetic

The Bloch condition is |tr M| = 2; had to write `(period(λ)[1,1] + period(λ)[2,2]) / 2`, which also
builds the matrix twice. Fix: `trace(M)`.

### Still open from the first pass

O1 (`solve I'(θ) = 0` is read as an ODE: `missing initial condition: I(start)`) bit again in
`21_airy_disk.fm`; the `dI = I'` workaround still works.

## Third pass (graduate, files 31_… and 32_…)

Nothing new: both problems ran as first written, apart from frictions already logged.
