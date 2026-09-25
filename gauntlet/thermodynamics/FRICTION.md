# Friction: thermodynamics (first pass)

Problems: `01_otto_cycle.fm`, `02_newton_cooling.fm`, `03_maxwell_boltzmann.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

What went well: `27 °C` works directly in `p V = n R T` (°C is an absolute temperature), `T1 r^(γ−1)`
with T₁ in °C gives the right absolute temperature, `T0 - T_room` is a difference in K, `°C + °C` is
refused with a helpful hint, integrals to ∞ of the Maxwell distribution are exact to 10 digits,
`f'` of a many-parameter function differentiates with respect to its argument, and comparing a
quantity with a bare `0` works (`f'(v) > 0`).

## T1. A temperature difference shown `in °C` is treated as an absolute temperature — wrong answer

Wanted:
```
T0 = 90.0 °C
print (T0 - 20 °C) in °C      # a physicist expects "70 °C" (a 70-degree difference)
```
Got `-203 °C`, silently: the difference is 70 K, and `in °C` converts 70 K as an absolute
temperature (70 K = −203.15 °C). Same for `print (70 K) in °C`, which is correct for an absolute
70 K but there is no way to tell the two apart. Had to write `in K` for every difference.

Fix: track "temperature difference" separately from "absolute temperature" in the checker (the
result of `T − T` is a difference; `+`/`−` of a difference and an absolute is absolute). Then
`ΔT in °C` can print `70 °C` (or `70 K`), and `ΔT in °C` for a *difference* at least warns:
"this is a temperature difference; in °C it is 70 degrees, not −203 °C".

## T2. `°C` can't appear in compound units — awkward

Wanted: `rate in °C/min`, `c = 4.186 J/(g °C)` (the way many textbooks and lab manuals write
specific heats and cooling rates).
Had to write: `K/min`, `J/(g K)`.
The error is good (`°C can't be combined with other units (it has an offset)`, hint: use K). But in a
compound unit °C can only mean a degree-size step, where it equals K, so it could simply be allowed.

Fix: inside a compound unit (anything with `/` or a product), read `°C` as K (and `°F` as 5/9 K).

## T3. No root finder in the first draft — awkward (resolved during the pass)

Wanted: "when does the coffee reach 50 °C?" Had to write a 9-line bisection `while` loop on the ODE
solution. The coordinator added `solve lhs = rhs for x from a to b` mid-pass; the problem now reads
`solve T(t_drink) = T_drink for t_drink from 0 min to 60 min`, which works on an ODE solution and gives
full precision. Resolved.

## T4. `a/(b) (c)` silently means a/((b)(c)) — awkward (a trap for textbook formulas)

Wanted: `Q_in = n R_gas/(γ - 1) (T3 - T2)`, i.e. n C_V ΔT with C_V = R/(γ−1).
Got: R_gas / ((γ−1)(T3−T2)) because implicit multiplication binds tighter than `/`. No warning; the
program printed `0.00046 kg m²/(s² K²)` for the heat. It was only caught because the units looked
wrong. (The rule is documented — `h c / λ k_B T` — but the documented `1/2 x` warning does not fire for
`z / 2 x` or `z / (4π) x` either.)
Had to write: `C_V = R_gas / (γ - 1)` on its own line, then `n C_V (T3 - T2)`.

Fix: warn when a parenthesised group follows `/ (…)` by juxtaposition, i.e. `a / (b) (c)` and
`a / (b) c`: "this divides by (b)·(c); write `a / (b) * c` if you meant to multiply by c". The same
electromagnetism trap: `μ₀ I / (4π) dl × r`.

## T5. `2 T` with T in °C prints in °C — cosmetic

`print 2 T0` with `T0 = 90.0 °C` prints `453 °C` (2 × 363.15 K). Physically consistent with
"°C is absolute", but the display is surprising; for scaling an absolute temperature, K is the less
misleading display unit.

Fix: show products/quotients of an absolute °C temperature in K.

## T6. `1/s` is shown as `1/s` — cosmetic

`print k` shows `0.00112157 1/s`; `in 1/min` works. `s⁻¹` or `/s` would read better; minor.
