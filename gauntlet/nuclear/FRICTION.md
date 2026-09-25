# Nuclear physics: friction log (first pass)

Problems: `01_radon_progeny.fm` (four-member Bateman chain, peak time), `02_q_values_semf.fm`
(Q-values, separation energy, SEMF vs masses), `03_radiometric_dating.fm` (C-14 age, Rb–Sr
isochron fit from `data/rb_sr_isochron.csv`).
Severity scale: blocker / wrong answer / awkward / cosmetic.

## Awkward

### N1. Stiff decay chains: ~10⁷ explicit steps, and a longer window fails outright
- **Wanted:** the radon progeny chain Po-218 (3.1 min) → Pb-214 (26.8 min) → Bi-214 (19.9 min) →
  Po-214 (164 μs) over a few hours, as `solve N1' = …, N4' = λ3 N3 − λ4 N4 … for t from 0 min to
  720 min`.
- **Got:** over 120 min it works but takes 9.2 million RK45 steps (1.6 s, all stored for
  interpolation). Over 720 min: `the ODE solver needed too many steps (reached t = 15302.7 in SI
  units); the equation may be stiff or blow up`. The 164 μs member forces h ≲ 1 ms for stability
  although nothing changes on that time scale.
- **Had to write:** the 120 min window. The alternative is to drop Po-214 and assume secular
  equilibrium by hand, which is what the problem is supposed to show.
- **Fix:** an implicit method (`using radau` / `using bdf`), or automatic stiffness detection
  (step size pinned at the stability limit with tiny error estimates) that switches to it. Linear
  constant-coefficient systems like Bateman chains could even use the matrix exponential.
  Also: say "t = 255 min" (in the range's unit) rather than "15302.7 in SI units".
- **Status:** fixed (gauntlet #25, D42): `using radau` solves the 720 min window in ~8 000 steps
  (01_radon_progeny.fm now uses it), and RK45 warns when a long solve looks stiff.

### N2. `solve N3'(t_max) = 0 for t_max …` is treated as a differential equation
- **Wanted:** the time of peak Bi-214 activity as the root of dN₃/dt on the existing solution.
- **Got:** `missing initial condition: N3(start)` with a hint about `with x(0) = …`.
- **Had to write:** a helper `slope3(tt) = N3'(tt)` and `solve slope3(t_max) = 0 for t_max …`.
- **Fix:** when `N3` is already a solved function and the equation has no `with`, a prime applied
  to it and then *called* (`N3'(t_max)`) is a value, not an unknown derivative: treat the solve as
  algebraic in `t_max`.

### N3. `15.3 / min / g` for "15.3 per minute per gram" fails
- **Wanted:** `A_modern = 15.3 / min / g` and `A_sample = 32.0 / min` (how the problem reads).
- **Got:** `min is a built-in function; call it with arguments like min(x)`.
- **Had to write:** `15.3 min⁻¹ g⁻¹`, `32.0 min⁻¹`, `1.3972e-11 yr⁻¹`.
- **Fix:** after a plain number, ` / name` where `name` is a unit and not a user variable could be
  read as a unit (`min` the function is never divided by). `15.3/min/g` without spaces does work
  (prints `15.3 1/(min g)`), but a beginner writes the spaces.

### N4. No uncertainties from `fit` to carry forward
- The isochron fit reports `slope = 0.06563 (standard error 1.1×10⁻⁵)` but the error isn't
  available as a value, so the age can't be quoted as 4.5497 ± 0.0008 Gyr without redoing the
  propagation by hand. `±` is reserved but not implemented.
- **Fix:** expose `slope.err` (or make fitted parameters uncertain numbers once `±` exists).

### N5. No table of nuclear masses
- Every Q-value problem starts by typing 10-digit AME2020 masses by hand (9 of them in `02`).
- **Fix:** a built-in `mass("U-238")` / `M("Pb-208")` from AME2020 (and maybe half-lives from
  NUBASE), returned in `u`. Units already make the rest clean: `(M_H2 + M_H3 − M_He4 − M_n) c² in
  MeV` just works.

## Cosmetic

### N6. The fit report rounds the value more coarsely than its standard error
- `ratio0 = 0.6990 (standard error 1.9×10⁻⁵)`: the value is shown to 4 figures while its error is
  in the 5th decimal. Standard practice is to show the value to the error's last digit:
  `0.69900 ± 0.00002`.
- **Fix:** choose the value's digits from the standard error.

## What worked well
- `u` for atomic masses and `c² in MeV` for Q-values; `Bq`, `min`, `μs`, `Gyr` all as expected.
- `load` of a CSV with dimensionless columns (header without brackets) and a linear `fit` that
  reaches the least-squares optimum to 8 digits.
- `mod(Z, 2) == 0` pairing term, multi-line functions with `else if`.

## Second pass

Problems: `21_woods_saxon.fm` (neutron levels in a Woods–Saxon well with spin–orbit, by shooting),
`22_alpha_gamow.fm` (Gamow half-lives of nine α emitters from `data/alpha_emitters.csv`, Krane's
closed form, a Geiger–Nuttall fit), `23_point_kinetics.fm` (six-group point kinetics after a
10-cent step, the inhour equation, the one-group estimate).
Tests: `tests/test_gauntlet2_nuclear.py`.
Severity: wrong answer / bug / awkward / cosmetic.

### Bug

#### N7. An integral's upper limit written as a fraction is cut at the `/`, and the hint points the wrong way
- **Wanted:** the Gamow integral up to the classical turning point, as written in Krane:
  ```
  G1(Z, Q) = ∫ √(2 m_α (2 Z e² / (4π ε₀ r) - Q)) / ħ dr from r_0 to 2 Z e² / (4π ε₀ Q)
  print G1(90, 4 MeV)
  # line 1: the limits of this integral are length [m] and a quantity with units [C²]
  #   hint: if you divide or multiply the integral by something, put the integral in
  #   parentheses: (∫ ... dx from a to b) / M
  ```
- This is fix #8 (a spaced `/` ends the upper limit) doing what it says, and the unit check caught
  it, so nothing wrong was computed. But the hint suggests bracketing the *integral*, while the fix
  here is to bracket the *limit*: `to (2 Z e² / (4π ε₀ Q))`.
- **Fix:** when the limits' units disagree and re-attaching the `/ …` to the limit makes them
  agree, say so: "did you mean `to (2 Z e² / (4π ε₀ Q))`?".

### Awkward

#### N8. `A u` (mass number times the atomic mass unit) is "u isn't defined"
- `μ = m_α A_d u / (m_α + A_d u)` → `u isn't defined` (hint: units go right after a number). Had to
  write `A_d [u]`. M ≈ A u is the most common expression in nuclear physics.
- **Fix:** the hint is clear, so this is safe as it is; optionally, a unit name directly after a
  *variable*, when no variable of that name exists, could be read as the unit with a warning.

#### N9. A function can't be applied element-wise over two lists; `fit` only takes `load`ed data
- `G1(data.Z, data.Q)` → "can't apply a function element-wise over two lists at once". Had to loop
  over `i from 1 to len(data.Z)`.
- `fit ys = a xs + b to xs` → "the data must come from load". So the Geiger–Nuttall law can be
  fitted to the measured half-lives in the CSV, but not to the half-lives the program just
  *computed* (the natural second half of the problem: does Gamow's model reproduce the
  Geiger–Nuttall slope?).
- **Fix:** zip semantics for equal-length lists; a `table(x = xs, y = ys)` constructor usable in
  `fit … to`.

#### N10. Six delayed-neutron groups means seven hand-written equations and six initial conditions
- Unknowns can't be lists, so `C1' … C6'` and `C1(0 s) … C6(0 s)` are written out one by one (with
  `λs[1] C1 + … + λs[6] C6` in the n equation). The inhour equation, on the other hand, is one line
  of list arithmetic: `inhour(ω) = ω Λ + sum(βs ω / (ω + λs))`.
- **Fix:** list- or vector-valued unknowns of any length: `C' = βs n / Λ - λs C`,
  `with C(0 s) = βs / (λs Λ)`.

#### N11. A list literal can't take a unit after the bracket
- `λs = [0.0124, 0.0305, 0.111, 0.301, 1.14, 3.01] s⁻¹` → "s isn't defined", although
  `<3, 4> m/s` and `[[2, -1], [-1, 2]] N/m` both work. Had to repeat `s⁻¹` six times.
- **Fix:** accept a unit after `]` for lists, as for vectors and matrices.

#### N12. `2 l + 1` is two litres plus one
- `for twoj in [2 l + 1, 2 l - 1]` → a warning, then "can't add volume [m³] to a plain number";
  `2l + 1` too. Had to write `2*l + 1`. The error's note (#10) explained it immediately; still,
  2l + 1 is on every page of a nuclear-structure text. (Astrophysics `22` hit the same with `4 s`.)

#### N13. No chained comparisons
- `E_1s < E_1p3 < E_1p1 < …` for a level ordering → "chained comparisons like a < b < c aren't
  supported" (clear, with the `and` rewrite). Minor.

### Cosmetic

#### N14. The derivative of a function of r in fm is shown in 1/m
- `f(r) = 1/(1 + exp((r − R)/a))` with `R`, `a` in fm; `print f'(R)` → `-3.84615×10¹⁴ 1/m`. The
  natural display is `-0.384615 1/fm` (per unit of the argument as written), in the way `solve`
  shows a root in the range's unit.

### What the first-pass fixes bought
- **Algebraic `solve` (#1)** for every level and for the inhour root (monotonic for ω > 0, so the
  first-root problem #2 didn't bite here).
- **The fit report (#35)** now reads `a1 = 1.480 (standard error 0.043)`.
- **The #10 note** for `2 l`.
- Symbolic `f'` of the Woods–Saxon shape inside the spin–orbit potential, `load` of a CSV with a
  unit column, and an `∫` with limits computed inside a multi-line function all worked first time.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#71 (B)** `d/dT N` where `N` calls a multi-line function `F` fails with "can't differentiate through F: it's defined over several lines" with no line number or caret (a direct `d/dx F` has both). Workaround: make F one line with `where`
