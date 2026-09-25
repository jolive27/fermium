# Special relativity: friction log (first pass)

Problems: `01_muon_time_dilation.fm` (Frisch–Smith), `02_relativistic_rocket.fm` (hyperbolic
motion, Alpha Centauri trip, velocity addition), `03_threshold_energy.fm` (antiproton, pion
photoproduction and GZK thresholds from the invariant mass).
Severity scale: blocker / wrong answer / awkward / cosmetic.

## Wrong answer

### S1. `c²/g (√(1 + (gt/c)²) − 1)` silently means c² / (g (…))
- **Wanted:** the hyperbolic-motion distance as printed in every textbook,
  `x = c²/g (√(1 + (g T/c)²) − 1)`.
- **Got:** implicit multiplication binds tighter than `/`, so it is c²/(g·(√…−1)). The bracket is
  dimensionless, so the units agree either way: no error, no warning, and 2.21 ly instead of the
  right 0.42 ly for T = 1 yr.
- **Had to write:** `c² / g * (√(1 + (g T / c)²) - 1)` and `2 c / g * acosh(...)`.
- **Repro:**
  ```
  g = 9.81 m/s²
  T = 1 yr
  print c² / g (√(1 + (g T / c)²) - 1) in ly      # 2.21 ly, no warning
  ```
- **Fix:** the rule (D8: `h c / λ k_B T` = (hc)/(λ k_B T)) is fine for runs of names, but a
  parenthesised group after `/ name` is genuinely ambiguous. Warn (like the `1/2 x` warning) when
  the implicit product right after `/` ends with a parenthesised factor that is dimensionless,
  suggesting `c²/g * (…)` or `c²/(g (…))`.

### S2. Catastrophic cancellation gives a plausible, round, wrong root with no warning
- **Wanted:** the GZK threshold straight from the invariant mass,
  `solve (E_p + E_cmb)² − (p_p(E_p) c − E_cmb)² = (m_Δ c²)² for E_p from mp to 1e22 eV`.
- **Got:** `2.95000×10²⁰ eV` (right answer 2.49×10²⁰). The two squares are ~10⁴⁰ eV² and cancel to
  1 part in 10²², so the left side is rounding noise; the root finder picked a noise sign change,
  which happens to sit on a scan point (hence the suspicious round number).
- **Had to write:** the expanded invariant s = (m_p c²)² + 2 E_γ (E + pc). That's my job as a
  physicist, but the language said nothing.
- **Fix:** in the algebraic `solve`, estimate the size of each side at the root; if
  |lhs − rhs| over the bracket is below ~10⁻¹² × |lhs|, warn "the two sides agree only to rounding
  error here; the root may be meaningless (cancellation?)".

## Awkward

### S3. `m_π` (or any `x_π`) can't be a variable name
- **Wanted:** `m_π = 134.9768 MeV/c²`.
- **Got:** `can't store a value here: the left side of = must be a variable name` (pointing at `=`).
  The ASCII spelling `m_pi` works, and prints as `m_pi`. `m_ρ`, `m_Δ` are fine.
- **Repro:** `x_π = 1 kg`
- **Fix:** the lexer splits `x_π` into `x_` and the constant `π`; treat `π` after `_` as part of the
  identifier, exactly as `pi` is.

### S4. Root finding was missing at the start; now it's the nicest part of these problems
- `solve M_inv(E) = 4 m_p for E from mp to 100 GeV` (a threshold from E² = (pc)² + (mc²)²) and
  `solve x(t_mid) = d / 2 for t_mid from 0 yr to 5 yr` (a time from an ODE solution) both read
  exactly like the physics, keep units, and converge to double precision. Before the "Algebraic
  equations" commit both needed hand-written bisection.
- Remaining nit: the non-first-root behaviour (quantum Q1) would bite a threshold scan with
  several solutions.

## Cosmetic

### S5. The derived momentum prints with an SI echo that isn't wanted
- `print p_beam(E) in GeV/c` prints `6.500539721 GeV/c (= 3.47407×10⁻¹⁸ kg m/s)`. The "(= … SI)"
  echo is helpful for a mass in MeV/c², but after an explicit `in GeV/c` I asked for that unit;
  the extra text also makes the output harder to parse.
- **Fix:** skip the SI echo when the unit was requested with `in`.

## What worked well (for the record)
- `0.9952 c` as a speed, `v in c` for printing, `MeV/c²` masses, `m_p c² in GeV`, `ly`, `yr`.
- A system of three ODEs mixing yr, ly and m/s in the initial conditions, with no conversions.
- Proper time as just another unknown (`τ' = √(1 − v²/c²)`).
