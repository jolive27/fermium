# Astrophysics: friction log (first pass)

Problems: `01_lane_emden_n1.fm` (n = 1 against sin ξ/ξ, ξ₁, an n = 1 Sun),
`02_compact_objects.fm` (Schwarzschild radius, Sgr A* shadow, Eddington luminosity),
`03_wien_peak.fm` (peaks of B_ν and B_λ from symbolic derivatives and a root finder),
`04_mass_function.fm` (Cygnus X-1 mass function and black-hole mass).
Severity scale: blocker / wrong answer / awkward / cosmetic.

## Awkward

### A1. Singular point at the start of an ODE: the error doesn't say what happened
- **Wanted:** `solve θ'' = -2 θ'/ξ - θ with θ(0) = 1, θ'(0) = 0 for ξ from 0 to 4` (knowing it's
  singular, I wanted to see what the language says).
- **Got:** `the ODE solver needed too many steps (reached t = 0 in SI units); the equation may be
  stiff or blow up`. The real problem is 0/0 = NaN in the right-hand side at ξ = 0. The message
  also says `t` (my variable is ξ) and "SI units" (ξ has none).
- **Had to write:** start at ξ₀ = 10⁻⁴ with the series θ ≈ 1 − ξ²/6 + ξ⁴/120 (standard practice,
  and what `examples/13_lane_emden.fm` does).
- **Fix:** evaluate the RHS at the initial point; if it isn't finite, say
  "the right side of the equation is not a number at ξ = 0 (0/0?); start slightly away from the
  singular point, e.g. at ξ = 1e-4 with a series". Use the equation's own variable name.
  (Quantum Q8 hit the same message for a division by ψ(0) = 0.)

### A2. No milli- or micro-arcseconds
- **Wanted:** `print θ_shadow in μas` (EHT results are quoted in μas; parallaxes and proper motions
  in mas).
- **Got:** `'μarcsec' is not a unit Fermium knows`; `mas`/`μas`/`uas` don't exist and `arcsec`
  doesn't take prefixes.
- **Had to write:** `θ_shadow / (1e-6 arcsec)` and a "micro-arcsec" label string.
- **Fix:** make `arcsec` prefixable and add `mas`, `μas` (`uas`).

### A3. Root finding was missing at the start
- Wien's peak and the companion mass both need a root. With the new algebraic `solve` they read
  like the textbook: `solve dBν(ν, T_sun) = 0 for ν from 1e13 Hz to 1e16 Hz` (a bare `0` is
  accepted for a quantity in W m⁻² sr⁻¹ Hz⁻², which saves working out the units of a derivative),
  `solve M2³ sin(i)³ / (M1 + M2)² = f_M for M2 from 0.1 M☉ to 1000 M☉`, and
  `solve θ(ξ1) = 0 for ξ1 from 1 to 4` on an ODE solution. All agree with SciPy `brentq` to 10
  digits. Before, each was a hand-written bisection loop (as in `examples/06_blackbody.fm`).

## Cosmetic

### A4. Solar units in output
- `L_Ed(M☉) in L☉` prints `32838.696 L☉` — good. But the same number without `in` prints in W; a
  quantity computed from `M☉` might default to solar units. (Minor; `in` is easy.)

## What worked well
- `∂/∂ν B_ν` on a two-argument Planck function, called as `dBν(ν, T_sun)`, with the units carried
  through symbolically.
- `M☉`, `R☉`, `L☉`, `kpc`, `day`, `27.51°` and `∛(…)` all as a physicist writes them.
- Printing `K` for the n = 1 polytrope in m⁵/(kg s²) caught nothing wrong but confirmed the
  dimensional analysis for free.

## Second pass

Problems: `21_white_dwarfs.fm` (Lane–Emden n = 1.5 and 3, the n = 1.5 mass–radius relation, the
Chandrasekhar mass, and the full Fermi-gas structure equations for six central densities),
`22_saha_ionization.fm` (half-ionisation in a stellar atmosphere and at recombination),
`23_friedmann_age.fm` (flat ΛCDM age, lookback time, distances, onset of acceleration, and the age
again from the Friedmann ODE). Tests: `legacy/tests/test_gauntlet2_astrophysics.py`.
Severity: wrong answer / bug / awkward / cosmetic.

### Wrong answer

#### A5. The adaptive ODE solver silently accepts a far-too-large first step
- **Wanted:** the scale factor from the Friedmann equation, starting deep in the radiation era:
  `solve A' = A H(A) with A(t_i) = 1e-7 for t from t_i to 20 Gyr`, t_i = 2.4×10⁵ s.
- **Got:** A = 1.0000876 at the true age (SciPy: 1.0000000), so the "ODE age" came out 1.3 Myr
  short. `tolerance 1e-12` changes nothing.
- **Repro** (the exact solution is w = √(t / 1 s)):
  ```
  solve w' = w / (2 t)
    with w(1e-10 s) = 1e-5
    for t from 1e-10 s to 1 s
  print w(1 s)              # 6.16899   (exact: 1)
  print w(1e-8 s) / 1e-4    # 4.6828    (exact: 1)
  ```
- **Likely cause (from reading `dp45`):** the first trial step is 10⁻⁴ of the whole range (10⁻⁴ s
  here, 10⁶ times t₀). When the solution's time scale is t itself, the relative error of a step
  h ≫ t does not shrink as h is cut 5× per rejection, so after four rejections the "stalled" rule
  (accept when the error stops improving; quantum Q2) accepts a step that is still ~10³ t₀ long.
- **Had to write:** start at z = 99 (a = 0.01, t_i from the quadrature), where t_i is larger than the
  first trial step; the ODE age then agrees with the integral to 5×10⁻⁸.
- **Fix:** choose the first step from the equation (Hairer–Wanner: h₀ ≈ 0.01 ‖y₀‖/‖f(t₀, y₀)‖ with
  one Euler probe), and allow the stalled acceptance only when h is already near the floating-point
  resolution of t; otherwise stop with "could not reach the requested accuracy near t = …".

### Bug

#### A6. A NaN in the right-hand side still reads as "too many steps … in SI units" (A1, still open)
- `solve θ'' = -2 θ'/ξ - θ^1.5 … for ξ from ξ0 to 5` (Lane–Emden n = 1.5, whose θ goes negative just
  past ξ₁ = 3.654) → `the ODE solver needed too many steps (reached t = 3.61774 in SI units)`. The
  cause is θ^1.5 of a negative θ; the variable is ξ, and ξ has no units.
- **Had to write:** `max(θ, 0)^1.5`.
- **Fix:** as in A1: check the stages for NaN and say "θ^1.5 has no value for θ < 0 (at ξ ≈ 3.65)",
  in the equation's own variable and unit.

#### A7. An ODE solution made inside a function can't be passed to an algebraic `solve` (quantum Q16)
- Wanted `lane_emden(n)` returning ξ₁ (a `solve θ(x) = 0 …` on the solution) for n = 1.5 and 3:
  `__sol.3 can't be used inside this integral/equation`. Had to write the two polytropes out at top
  level, and the white-dwarf table as a top-level `for` loop instead of a function `wd(x_c)`.

### Awkward

#### A8. No "stop when" for `solve`, so the density has to be switched off by hand (#33)
- The structure equations are integrated past the surface y = 1 to a fixed r = 3×10⁷ m. Outside, y
  keeps falling (dy/dr ∝ −GM/r²) and for a massive dwarf passes y = −1, where (y² − 1)^(3/2) switches
  the density back on; the solver then stops with "the step became too small near
  t = 1.16235×10⁷ (SI units); the solution may blow up there".
- **Had to write:** `ρ_of(y) = if y > 1 then … else 0 kg/m³`, then `solve y(R) = 1 for R …`.
- **Fix:** `solve … for r from r_in until y = 1` (an event), which also gives the radius directly.

#### A9. The `a/b (c)` warning (#9) caught a real slip, but the program still ran
- `t_closed = 2 / (3 H_0 √Ω_Λ0) asinh(√(Ω_Λ0 / Ω_m))` warned "this divides by all of …" and printed
  9.77 Gyr instead of 13.79 Gyr. The warning was right and the fix was one `*`. A physicist reads
  `2/(3H₀√Ω_Λ) asinh(…)` as the product; since the checker already knows the writing is ambiguous,
  consider making this spaced-bracket form (`/ (…) f(…)`) an error that asks for `*` or parentheses.

#### A10. The RK45 "relative tolerance 10⁻⁹" is per step, not for the answer
- Over 20 Gyr the Friedmann ODE gives the time of a = a_acc to 3×10⁻⁷ at the default tolerance
  (10⁻⁹ with `tolerance 1e-12`). That is normal for local error control, but §10 of the reference
  reads as if the answer had 10⁻⁹. Document "10⁻⁹ per step; results are typically good to
  10⁻⁷–10⁻⁸; use `tolerance` for more".

### What worked well
- `M_Ch in Msun` → `1.4562974 M☉`; `D_C in Mpc`, `t_rec in yr`, `67.66 km/s/Mpc`, `0.190 m⁻³` and
  `in Gyr` all behaved exactly as written.

### What the first-pass fixes bought
- **Algebraic `solve` (#1)** on ODE solutions (ξ₁, the dwarf's surface, a(t) = a_acc, a(t) = 1), on
  a closed form with a fractional power (ρ_c from M(ρ_c) = 0.6 M☉) and on the Saha equation and the
  recombination redshift. A bare `0` on the right is accepted for ä/a in 1/s².
- **The #10 note** explained `4 s` (four seconds) in `(−s + √(s² + 4 s))/2` at once.
- **The #9 warning** (A9) caught a real precedence slip.
- Symbolic `d/dT` of a Saha fraction built from two user functions gave dx/dT to 8 digits.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#78 (C)** A function named `integral(b, T) = …` fails with "expected ')' to close '(' but found ','": `integral` is the ASCII spelling of ∫, and the error could say so
- **#79 (C)** `M in M_sun` is "'M_sun' is not a unit Fermium knows" with a generic hint: the constant is `M_sun`, the unit is `M☉`/`Msun`, and the hint could say so (and `m_sun` suggests `R_sun`, not `M_sun`)
