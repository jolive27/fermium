# Friction log: oscillations (first pass)

Problems: `01_driven_resonance.fm`, `02_large_pendulum.fm`, `03_coupled_oscillators.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

## O1. `ω in Hz` silently gives the wrong number (wrong answer)

```
m = 0.2 kg
k = 80 N/m
ω0 = √(k / m)
print ω0 in Hz          # prints "20 Hz"; the oscillator's frequency is 3.18 Hz
```

Because radians are plain numbers, rad/s and Hz are the same unit to the checker, so converting
an angular frequency to Hz is accepted and off by 2π, with no warning. (The `rpm` case already
warns, see DECISIONS D27; this more common case does not.) Every oscillations student writes this
at some point. **Fix:** track "angular" as a pseudo-dimension on values computed as √(k/m),
from `rad/s`, `2π f`, etc., and warn on `in Hz` (and on `ω in rad/s` never); at minimum, warn
whenever a value whose formula contains no 2π is converted from rad/s to Hz or back.

## O2. Angular frequencies print as `1/s` (cosmetic)

`ω0 = √(k/m)` prints `20.0000 1/s`, and `γ = b/m` prints `2.00000 1/s`. In the same run the
half-power points found by `solve … for ω from 0 rad/s to …` print in `rad/s`, so one line of
output reads `half-power points: 18.9222 rad/s 20.9273 1/s` (the second root's range started at
`ω_r`, a `1/s` value). **Fix:** a display heuristic: a value named ω/Ω/omega (or computed as
√(k/m)) prints in rad/s; the root of `solve … for x from a to b` prints in the unit of `b` if `a`
has none — or better, in the unit the user wrote anywhere in the range.

## O3. The k/κ look-alike warning fires on every use (awkward)

Taylor calls the coupling spring κ next to the wall springs k. Fermium warns
`'κ' and 'k' look almost identical but are different names` — **15 times** for one program, once
per use, drowning the actual output. I renamed κ to `k_c`. A one-time warning at the first
definition would be useful; 15 copies are noise, and they push beginners to avoid standard
notation. **Fix:** report each confusable pair once per program (at the second name's definition),
and consider exempting pairs that are standard physics notation and visibly different in most
fonts (k/κ, v/ν is a real hazard, k/κ much less so).

## O4. A mass called `m` triggers a warning on `x(0) = 0 m` (cosmetic)

`m = 0.200 kg` then `with x(0) = 0 m` prints
`warning: 'm' right after a number is the unit m, not your variable m`. Here the unit is obviously
intended (an initial position). The warning is correct in general (`0.5 m v²`), but in an initial
condition of a length unknown, or when the number is 0, the unit reading is the only one that
type-checks as a length... it fires anyway. **Fix:** suppress the warning when the unit reading
has the expected dimension of the context (an initial condition for a length, the right side of
a comparison with a length) and the variable reading would not.

## O5. A prime inside an algebraic `solve` is read as an ODE (awkward, confusing error)

Also hit in mechanics (see mechanics M7). `solve r'(t_apex).y = 0 m/s for t_apex from …` fails with
`missing initial condition: r(start)`. The same idea shows up for oscillators: "when is the
velocity zero?" `solve x'(t1) = 0 m/s for t1 from …`. Workaround: define `v(τ) = x'(τ)` first.
**Fix:** when the equation has no `with` and the primed name is an existing ODE solution called at
the unknown, treat it as an algebraic equation.

## O6. No eigenvalues / matrices for normal modes (awkward)

For unequal masses I wanted `ω² = eigenvalues(M⁻¹ K)`. Matrices don't exist yet, so I wrote the
2×2 characteristic polynomial by hand and found its two roots with `solve char(ω) = 0 for ω from …`,
choosing brackets by knowing the roots interlace the uncoupled frequencies. That only works for
2 degrees of freedom and needs physics insight to bracket. **Fix:** matrices with units
(`K = [[k + k_c, -k_c], [-k_c, k + k_c]] N/m`) and `eigen(K, M)` returning ω² and mode shapes.

## O7. Measuring an amplitude from an ODE solution (awkward, minor)

To get the steady-state amplitude I used `√(x² + (x'/ω)²)` at the end time (exact for a sinusoid).
The obvious beginner approach, `max(values(x))`, samples the solver's own steps and so only
approximates the peak; `values(x)` from `t_end − 2π/ω` onward isn't expressible without a list
filter. **Fix:** `max(x for t from a to b)` on an ODE solution using its dense output, or
`values(x, from: a, to: b)`.

## O8. `print` spacing around parentheses (cosmetic)

`print "width:", w, " (γ =", γ, ")"` prints `width: 2.00504 rad/s  (γ = 2.00 1/s )` — the
separator space is always inserted, so a closing parenthesis cannot be attached. **Fix:** string
interpolation, e.g. `print "width {w} (γ = {γ})"`, or `print …; ` joining without spaces.

## O9. The integral error has no line number (cosmetic)

`K(k) = ∫ 1/√(1 − k² sin(φ)²) dφ from 0 to π/2` then `print K(1)` (a genuinely divergent case)
stops with `this integral doesn't converge: … the estimate was ∞ ± ∞ in SI units` and no line
or caret, unlike compile errors. With several integrals in a program you have to guess which one.
**Fix:** attach the source location of the ∫ to the runtime error.

## What worked well

- The singular energy integral `∫ dθ / √(cos θ − cos θ₀) from 0 to θ₀` is right to 8 digits even
  at θ₀ = 179°. So is the elliptic-integral form. No `ellipk` needed.
- `solve` inside a `for` loop, with the driving frequency changing each pass, and the new
  algebraic `solve θ(t_q) = 0 for t_q from …` on the ODE result, both just work.
- `mod(ω t − atan2(…), 2π)` and `δ in deg` read like the textbook.
- A zero on the right of `solve char(ω) = 0 for …` is accepted whatever the units of `char`.

## Second pass

Problems: `21_three_mass_chain.fm` (normal modes by `eigenvalues(K, M)`, modal projection, energy
per mode), `22_parametric_resonance.fm` (Mathieu equation: the Floquet trace from two ODEs inside a
function, exact band edges by an algebraic `solve` on that function), `23_duffing_oscillator.fm`
(exact period integral, Lindstedt–Poincaré, driven steady state against harmonic balance).
Tests: `legacy/tests/test_gauntlet2_oscillations.py`.

### O10. A vector can't be indexed by a loop variable (awkward)

```
ω2 = eigenvalues(K, M)
for n from 1 to 3
    print √(ω2[n]), 2 √(k / m) sin(n π / 8)
```

→ `a vector's component must be picked with a fixed number, like v[1], or v.x`. The natural "compare
mode n with the formula" loop has to be unrolled into three lines; the three mode shapes have to be
picked out one entry at a time: `v1 = <W[1, 1], W[2, 1], W[3, 1]>`. **Fix:** allow a runtime index
into a vector whose components share one unit (with a bounds check, like lists); and add
`column(M, j)` (or `M[:, j]`) so that a mode shape is one expression.

### O11. `2 m m2` is two metres × m × m2, and a number is printed anyway (wrong answer with a warning; see mechanics M10)

In the symmetric-mode quadratic `√((sym ± √(sym² − 8 k² m m2)) / (2 m m2))` the `2 m` is 2 metres.
Only a warning; the line printed `3.90879 kg^(1/2)/(m^(1/2) s)` instead of `7.81758 1/s`, because
nothing forced a unit check. Someone skimming the output could miss the odd unit. Fixed in the
program with `2*m`. **Fix:** as M10; also, a `print` of a value with fractional powers of base units
could warn ("did a unit sneak in?").

### O12. Round-off shown next to 3-figure numbers in matrices (cosmetic)

`print eigenvectors(K, M)` shows `[[0.500, 0.707, -0.500], [0.707, -8.76×10⁻¹⁷, 0.707], …]`: the
exact zero of the middle mode. The M-orthogonality check prints `4.16×10⁻¹⁷ kg`, which is fine for
a check. **Fix:** in a printed matrix or vector, show entries below ~1e-12 of the largest entry as 0.

### O13. `√` of a negative number is a silent NaN (awkward)

`growth(γ)` uses `√(D² − 4)`; off resonance (|D| < 2) it would print `NaN` with no reason given
(`x = -1.0`, `print √(x² - 4)` → `NaN`; `ln(-0.5)` → `NaN`). For a physicist a NaN in a table means
a debugging session. **Fix:** the first time a NaN is made, a warning "√ of a negative number (−3)
on line N"; or print `NaN (from √ of a negative number, line N)`.

### O14. Angular frequencies mix `1/s` and `rad/s` on one line (cosmetic; #29)

`2π / T_exact(A)` prints `5.029896 1/s` next to `ω_pert1(A)` = `5.030000 rad/s` on the same line,
because ω₀ was written in rad/s. `√(k/m)` is `1/s`. Same quantity, two spellings.
**Fix:** show 1/s as rad/s for a value computed as 2π/T or √(k/m)… or accept that `in rad/s` has to
be written each time.

### Physics note (not a language problem)

The Lindstedt–Poincaré second-order coefficient in terms of the **turning point** A is −21/256, not
the often-quoted −15/256 (that one is for the amplitude of the fundamental harmonic). The test
caught it: the exact period integral disagreed with −15/256 at order ε², and agrees with −21/256 to
order ε³.

### What helped from the first pass

- `eigenvalues(K, M)` / `eigenvectors(K, M)` (#22): the chain problem is linear algebra that reads
  like Goldstein, with units (K in N/m, M in kg, ω² in 1/s²), and `½ x0 · (K x0)` for ½xᵀKx.
- `solve` twice inside a multi-line function, and then an algebraic `solve D(γ) = -2 for γ …` on
  that function (hundreds of ODE solves): worked the first time, 0.4 s for the whole program.
- `solve … for` inside a `for A in [...]` loop over amplitudes, overwriting the solution each pass.
- The integrable 1/√ end-point blow-up of the exact Duffing period integral: right to 7 digits.
- Bare `0` in a matrix literal whose other entries have units (`[[2k, -k, 0], …]`), `identity(3) m`.
- The #10 warning at least fired for O11; the M8 fix ("`2 m` next to your variable m") is visible.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#72 (A)** `α = 0.300 /(m s²)` is refused with "m isn't defined", though `0 /s` and `<0, 0> /s` take the unit. Workaround: `0.300 [1/(m s²)]` or `0.300 m⁻¹ s⁻²`
