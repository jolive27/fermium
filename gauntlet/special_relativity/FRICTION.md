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

## Second pass

Problems: `21_hyperbolic_motion.fm` (an electron in a uniform E field with transverse momentum, as
vector ODEs for p, r and τ, against the exact catenary), `22_twin_paradox.fm` (a round trip with
accelerating, coasting and decelerating legs: proper time by integrating a piecewise formula and by
ODEs), `23_lorentz_matrices.fm` (boosts as 4×4 matrices: Λᵀ η Λ = η, eigenvalues e^(±φ), velocity
addition, the Wigner rotation from a polar decomposition, π⁺ → μ⁺ν kinematics boosted to the lab).

First-pass fixes that helped: `m_π` and `m_μ` as names (#14, S3); eigenvalues (#22) give e^(±φ) of a
boost to 10 digits, and `inverse`, `det`, `Mᵀ` and 4×4 products just work; vector ODEs in momentum,
`p' = q E, r' = p c² / energy(p)`, with MeV/c, MV/m, ns and m mixed freely; algebraic `solve`
(D32) on a formula with `asinh` (the 40-year trip) and on the ODE solution (the time to reach 0.98 c).
Multi-line functions returning a 4×4 matrix written over several lines read like the textbook.

### Wrong answer

#### S9. `r'(t)` of a first-order unknown is ~1000× less accurate than the solution
```
solve p' = q E, r' = p c² / energy(p), τ' = M c² / energy(p)
  with … for t from 0 ns to 10.0 ns tolerance 1e-11
print r'(5 ns)                        # <2.39927819×10⁸, 1.60062841×10⁸, 0> m/s
print p(5 ns) c² / energy(p(5 ns))    # <2.39928109×10⁸, 1.60062805×10⁸, 0> m/s
```
r(t) itself agrees with the exact solution to 10 digits, but `r'(t)` is off by 1.2×10⁻⁶ relative, a
thousand times the requested tolerance, with no warning. Near c the speed is flat, so the time at which
|r'| = 0.98 c came out 7.70284 ns instead of 7.70296 ns (1.6×10⁻⁵ relative). For a second-order
unknown (`x''`), `x'` is a state variable and is accurate (electromagnetism 21 matches to 8 digits).
Apparently r' is the derivative of the interpolant between steps.
Workaround: evaluate the right-hand side, `|p(t)| c² / energy(p(t))`.
Fix: for an unknown that appears as `r' = f(…)`, make `r'(t)` evaluate f at the interpolated state
(exact to the solver's accuracy), or interpolate r' with the same order as r (dense output of the
derivative from the RK stages).

### Awkward

#### S6. No `mrad` (or `μrad`): `rad` takes no prefixes
`print angle in mrad` gives `'mrad' is not a unit Fermium knows`. Small lab angles in particle
physics are quoted in mrad. Printed degrees instead. Fix: SI prefixes on `rad` (as done for `arcsec`, #16).

#### S7. `solve |r'(t_r)| = 0.98 c for t_r …` is read as an ODE (gauntlet #3 again)
`missing initial condition: r(start)`, though r is an existing ODE solution and the line has
`for t_r from … to …` and no `with`. Workaround: `speed_r(t) = |r'(t)|` on its own line.

#### S8. A piecewise formula can't continue on the next line without brackets
```
u(t) = if t < t1 then g t
       else if t < t1 + tc then g t1
       …
```
Got: `expected 'else' (an if-expression needs an else part) but the line ended`. Wrapping the whole
right side in `( … )` works, but the message doesn't say so, and a piecewise function is the natural
way to write "accelerate, coast, decelerate".
Fix: let an if-expression continue onto an indented line starting with `else`; at least add the hint
"to continue on the next line, put the expression in brackets".

#### S10. An acceleration that switches with `if t < …` in an ODE loses accuracy (gauntlet #11 again)
The twin trip as ODEs, `w' = a(t)` with a(t) = ±g or 0 on the legs, gives the traveller's age as
6.901531830 yr against the exact 6.901531326 yr (7×10⁻⁸) and the turning point 6×10⁻⁷ off, versus
10 digits from `∫ 1/γ(t) dt` of the same piecewise formula. The solver steps over the switches
without stopping at them. The test allows 2×10⁻⁶ for this.
Fix: as #11 (locate discontinuities in t, or accept `for t from … to … at t1, t1 + tc, …` break points).

#### S11. `abs`, `max` don't work entry by entry on vectors and matrices
To check Λᵀ η Λ = η, `max(abs(check[1]), …)` gives `the argument of abs must be a number, but it is
a 4-vector`. Used `√(|row1|² + … + |row4|²)` instead. Fix: element-wise `abs` for vectors and
matrices, `max` of a vector, and a matrix norm (`norm(M)`).

#### S12. A Lorentz matrix can't act on (t, x) with t in s and x in m
`L X` with `X = <1 s, 2 m>` is refused ("a matrix times a vector needs all components of the vector in
the same units"), so four-vectors must be written (ct, x, y, z) and (E/c, p). That is the standard
convention and the message is clear, so this is only a note: a matrix with per-entry units
(optics O7) would allow Λ with c and 1/c entries.

### Cosmetic

#### S13. Matrices print exact entries with made-up precision
`print Λ` shows `[[1.25, -0.750, 0, 0], [-0.750, 1.25, 0, 0], [0, 0, 1.00, 0], [0, 0, 0, 1.00]]`:
the identity block gets "1.00" while the zeros print as "0", and −0.75 gets three figures. It reads
as if the entries were measured. Minor.
