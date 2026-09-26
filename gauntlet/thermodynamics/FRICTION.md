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

## Second pass

Problems: `21_van_der_waals_maxwell.fm` (spinodals, the equal-area Maxwell construction with three
levels of nested `solve`, latent heat, Clausius–Clapeyron by redoing the construction at T ± 0.1 K),
`22_debye_einstein.fm` (the Debye integral against Einstein, the T³ and Dulong–Petit limits,
C = 3R/2, the entropy as ∫C/T dT of an integral), `23_photon_carnot_stirling.fm` (the photon-gas
adiabat as an ODE in V, a Carnot cycle leg by leg, Stirling with and without a regenerator).
Tests: `legacy/tests/test_gauntlet2_thermodynamics.py`.

### T7. The van der Waals `b` is the barn: `3 b`, `27 b²` (awkward, silent until later; see mechanics M10)

`V_c = 3b`, `P_c = a / (27 b²)`, `solve … for V_l from 1.01 b to …`: every one of these is barns.
`27 b²` only warned, and the error came on the *next* line (`can't show a quantity with units
[kg m/(s² mol²)] in MPa`), without the note naming `27 b` as the cause, because the note is only
added to an error on the same line. Inside a function (`from 1.01 b to 3b`) the error came from
another line entirely: `line 4: can't subtract [m³/mol] from area [m²] … (with V = area [m²])`. The
vdW `b` is universal notation. Workaround: `3*b`, `27*b²`, `1.01*b` everywhere. **Fix:** M10; and
consider not reading `b` as the barn right after a number when the program defines a variable `b`
(`barn` stays available).

### T8. A failing `solve` inside a function doesn't say which call failed (awkward)

`area(temp, P)` solves `P_vdw(V_l, temp) = P`. When the outer `solve area(T, P_s) = 0 J/mol for P_s
from 3.0 MPa to 3.9 MPa` tried a P below the spinodal pressure (no liquid root exists), the error was
`line 7: this equation has no solution between 4.30967×10⁻⁵ m³/mol and 0.00012801 m³/mol`, with no
word about which P, or that it was reached from the outer solve on line 11. **Fix:** add "while
evaluating area(T = 273 K, P = 3.0 MPa), called from the solve on line 11" to runtime errors raised
inside functions.

### T9. `from 0 to θ_D / T` divides the integral, and the error that follows doesn't say why (awkward; #8 follow-up)

`C_D(T) = 9 R (T/θ_D)³ ∫ x^4 exp(x) / (exp(x) - 1)² dx from 0 to θ_D / T`: the #8 warning fired
("the ' / ' after the upper limit divides the whole integral"), then the error `exp needs a plain
number, but got temperature [K]`, pointing at `exp(x)`. Both are right, but the error doesn't
connect them: x is a temperature *because* the limit became θ_D. Textbooks write exactly
`∫₀^{θ_D/T}`. Workaround: `to (θ_D / T)`. **Fix:** when the integration variable's units come from a
limit that the ` / ` rule shortened, say so in the error ("x has the units of the upper limit θ_D;
did you mean `to (θ_D / T)`?"); or, when dividing the whole integral fails the unit check and
dividing the limit passes it, take the latter.

### T10. exp overflow gives NaN, reported as "doesn't converge" on the wrong line (bug)

The entropy `S_D(T) = ∫ C_D(u) / u du from 0 K to T` makes the inner Debye integral's upper limit
θ_D/u huge as u → 0, where `x^4 exp(x) / (exp(x) - 1)²` is ∞/∞ = NaN for x > 709. Result:
`line 30: this integral doesn't converge … the estimate was NaN ± NaN`, and line 30 is the
`solve C_D(T_half) = 3 R / 2 …` line, not the entropy integral (known #13). Workaround: write the
integrand as `x^4 exp(-x) / (1 - exp(-x))²`, as a numerical analyst would, not as Kittel does.
**Fix:** (i) report NaN integrands as such ("the integrand is NaN at x = 7.1×10²: ∞/∞, probably exp
overflow"), on the right line; (ii) evaluate `exp(x)/(exp(x) − 1)^n` patterns in the overflow-safe
form (SymPy rewrite or a code-generation rule).

### T11. `3 V` with V the ODE's independent variable is 3 volts, with no warning or note (bug)

`solve T' = -T / (3 V) with T(1.00 L) = 3000 K for V from 1.00 L to 8.00 L` →
`the two sides of this equation don't match: left is a quantity with units [K/m³], right is a
quantity with units [s³ A K/(kg m²)]`. No warning beforehand and no "'3 V' here is 3 volts" note:
the independent variable of `solve` is not treated as a user variable by the #10 checks. The units
in the message are the only clue. Workaround: `3*V`. **Fix:** register the `for V from …` variable as
a user name for the unit/variable clash warning and the note.

### T12. The radiation constant prints in base SI (cosmetic; #29)

`a_rad = 4σ / c` prints `7.56573×10⁻¹⁶ kg/(m s² K⁴)`; books write J/(m³ K⁴). **Fix:** extend the
composite display units (J/(m³ K⁴), or prefer J/m³ over kg/(m s²) when further units follow).

### What helped from the first pass

- The algebraic `solve` (#1, T3) is the whole Maxwell construction: spinodals from `∂/∂V` of a
  two-argument function, the liquid and gas volumes from `solve`s inside a function that returns a
  vector `<V_l, V_g>`, the equal-area pressure from a `solve` on a function that itself runs three
  `solve`s and an integral, and dP_s/dT by calling all of that at T ± 0.1 K. 0.6 s in total;
  P_s/P_c = 0.64700, the textbook value, and Clapeyron holds to 5 digits.
- An ODE with the **volume** as the independent variable and the initial condition at 1.00 L
  (`T(1.00 L) = 3000 K`).
- Integrals of functions defined by integrals (∫C_D/T dT), including the 0 K end, once T10 is avoided.
- `4σ / c`, `R_gas`, and `in μJ`, `in kPa/K`, `in cm³/mol` all as expected.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#68 (W)** With a variable `m` (a mass), `n = 2.50e19 /m³` is 2.5×10¹⁹ divided by the variable m, cubed (1/kg³), with no warning (the §2 rule for `/` + space + a variable, but written with no space after `/`, as a unit). The unit error surfaces lines later, at an unrelated `in nK`. Workaround: `2.50e19 m⁻³`, or rename the mass
- **#73 (A)** Textbook coefficients `73/24 e²`, `37/96 e⁴`, `121/304 e²`, `π²/12 t²`, `π⁴/80 t⁴` read as a/(b x) (D8): 9 warnings in two problems, each fixed with brackets. The warning works, but Peters' and Sommerfeld's formulas are written this way on paper
- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#81 (C)** The warning for `2.50e19 m⁻³` (m also a variable) quotes the number as `2.5e+19`, not as written
