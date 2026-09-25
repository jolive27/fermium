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
