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
