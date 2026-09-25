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
