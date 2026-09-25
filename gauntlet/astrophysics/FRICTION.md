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
