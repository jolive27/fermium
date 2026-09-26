# Quantum mechanics: friction log (first pass)

Problems: `01_infinite_well.fm` (shooting, box), `02_harmonic_oscillator.fm` (shooting with parity),
`03_barrier_tunneling.fm` (transmission by integrating the Schrödinger equation).
Severity scale: blocker / wrong answer / awkward / cosmetic.

## Wrong answer

### Q1. `solve f(E) = 0 for E from a to b` returns a root that is not the first one
- **Wanted:** `solve psi_end(E) = 0 for E from 0.01 eV to 10 eV` to give the ground state (the
  docs say "finds the **first** solution after a"), then step the lower limit past each level.
- **Got:** 9.40 eV, the n = 5 level. When the two ends already have opposite signs the scan is
  skipped and regula falsi converges to whichever root it lands on.
- **Had to write:** my own upward scan in steps of 0.05 eV to bracket each level, then `solve` on
  the bracket (`01`, `02`).
- **Repro:**
  ```
  solve sin(x) = 0 for x from 1 to 10
  print x          # 9.42478 (3π); the first root after 1 is π
  ```
- **Fix:** always scan from `a` for the first sign change (the 200-point scan already exists), or
  change the docs to "a root". For shooting, the scan should be finer than the level spacing:
  let the user give `step` for algebraic solves, or scan adaptively.

### Q2. A discontinuous right-hand side silently loses accuracy (and fails at tight tolerance)
- **Wanted:** integrate the Schrödinger equation through the barrier *and* on into the free region
  with `V(x) = if x > 0 nm and x < a then V0 else 0 eV`.
- **Got:** T correct only to ~2×10⁻⁷ at the default rtol 1e-9; with `tolerance 1e-12` it did
  not improve and in places got *worse* (at 1 eV: 1×10⁻⁸ → 2.4×10⁻⁷), and at the resonance energy the solver stopped with "the ODE solver's
  step became too small near t = 5×10⁻¹⁰ (SI units); the solution may blow up there" (nothing
  blows up; that's the edge of the barrier).
- **Had to write:** stop the integration exactly at the barrier edge x = 0, with `>=`/`<=` so the
  last stage still sees V₀, and do the free region analytically.
- **Repro** (default tolerance; errors are 30× and 650× the requested 1e-9):
  ```
  r(t) = if t < 0.3 s then 1 / (1 s) else 2 / (1 s)
  solve y' = r(t) y
    with y(0 s) = 1
    for t from 0 s to 1 s
  print y(1 s) / exp(1.7) - 1                        # -3.3e-8
  solve z'' = -(r(t))^2 z
    with z(0 s) = 1, z'(0 s) = 0 / (1 s)
    for t from 0 s to 1 s
  print z(1 s) / 0.016765659634750962 - 1            # 6.5e-7
  ```
- **Fix:** the "stalled" acceptance rule (accept after 4 rejections if the error stops shrinking)
  lets a step across the jump through. Better: when the RHS contains an `if` on the independent
  variable, find the switch points (they are `t < const` comparisons) and restart the integrator
  there; or bisect the rejected step onto the discontinuity. The "may blow up" message should
  mention discontinuities too.

## Awkward

### Q3. No root finder when I started
- Before `solve lhs = rhs for x from a to b` landed (commit "Algebraic equations", merged
  mid-session) every eigenvalue needed a hand-written 50-step bisection loop, duplicated in each
  problem, because functions can't be passed to functions (Q5).
- **Now:** it works well for shooting: `solve psi_end(E, p) = 0 for E from E_lo to E_hi`
  calls a function that itself runs an ODE `solve`, converges to double precision (all three box
  levels and four oscillator levels agree with n²π²ħ²/2mL² and ħω(n+½) to 10 digits) and a bare
  `0` on the right is accepted whatever the units of ψ. Only Q1 (non-first root) and Q4 (error
  message) remain.

### Q4. Root-finder error inside a function that contains an ODE solve: wrong line, wrong units
- **Got** (from a too-wide second window in `01`):
  `line 22: this equation has no solution between 1.52123×10⁻⁹ nm and 1.60218×10⁻⁹ nm` — the
  range is an energy (9.5 eV to 10 eV), the line is the function's `return ψ(L)`, not the `solve`.
- **Repro:**
  ```
  f(k) =
      solve y' = k y
        with y(0 s) = 1 m
        for t from 0 s to 1 s
      return y(1 s)
  solve f(k) = 0.5 m for k from 1 / (1 s) to 2 / (1 s)
  # "line 5: ... no solution between 1 s and 2 s"  (should be line 6, 1/s and 2/s)
  ```
- **Fix:** the algebraic solve's error context (line, display unit) is being overwritten by the
  inner ODE solve; save and restore it around the call.

### Q5. Functions can't be passed to functions
- **Wanted:** `bisect(f, lo, hi)` or `levels(psi_end, 3)` written once.
- **Got:** `g is a function; give it an argument, like g(x)`.
- **Fix:** allow function-valued parameters (they are compile-time specialised anyway).
- **Now:** fixed (D43): `levels(psi_end, 3)`, `bisect(f, lo, hi)` and `shoot(V, E)` work; each call is
  specialised for the function passed, so units are checked per function.

### Q6. No complex numbers
- **Wanted:** ψ = e^{ikx}, `solve ψ'' = (2m/ħ²)(V − E) ψ with ψ(a) = 1, ψ'(a) = i k`, then
  `T = 1/|A|²`.
- **Had to write:** ψ = u + i w as two real equations, and A = ½[(u − w'/k) + i(w + u'/k)] worked
  out by hand (`03`).
- **Fix:** a complex type (`i` or `im`), `abs`, `conj`, `re`, `im`, and complex unknowns in `solve`.
- **Fixed (D90–D95):** `solve ψ'' = … ψ with ψ(0 nm) = 1 + 0i, ψ'(0 nm) = -1i k`, then
  `A = ½ (ψ(a) - ψ'(a) / (1i k))` and `T = 1 / |A|²` (`03` now reads like that).

### Q7. `solve` can't integrate towards smaller t, and the error talks about a step I never gave
- **Wanted:** start from the transmitted wave at x = a and integrate `for x from a to 0 nm`.
- **Got:** `the step must be a non-zero number that goes from the start towards the end` (there is
  no `step` in the program).
- **Had to write:** a new variable s = a − x (and flip the sign of every first derivative).
- **Fix:** allow a decreasing range (negate h); at least say "the range must increase".

### Q8. Implicit multiplication after `/`: `-2 m_e E / ħ² ψ` divides by ψ
- **Wanted:** `ψ'' = -2 m_e E / ħ² ψ` as on paper.
- **Got:** it means −2 m_e E / (ħ² ψ). With a dimensionless ψ the units balance either way, so the
  checker can't catch it, and the solver fails with the unrelated
  "the ODE solver needed too many steps (reached t = 0 in SI units); the equation may be stiff or
  blow up" (ψ(0) = 0, so it divides by zero at the first step).
- **Had to write:** `-(2 m_e E / ħ²) ψ`, and I gave ψ its physical units (m^(−1/2)) so the unit
  checker would catch the next slip.
- **Fix:** warn when an implicit product after `/` contains a `solve` unknown (dividing by the
  unknown is rare and usually a precedence slip); report a NaN/∞ right-hand side as such (see
  astrophysics A2).

### Q9. The natural name `m` for the mass means metres after a number
- `2 m E` and `2 m L²` read as 2 metres (Fermium warned, correctly). Every QM textbook writes
  2mE/ħ². Had to drop `m = m_e` and write `m_e` everywhere.
- **Fix:** none needed beyond the warning, but the warning could be an error when the unit reading
  makes the equation fail its unit check, and the hint could suggest `m_e` or `2*m`.

### Q10. A unit after a parenthesised expression or a bare `/`
- `(1 - p) nm^(-1/2)` → `nm isn't defined`; had to write `(1 - p) [nm^(-1/2)]`.
- `u'(0 nm) = 0 / nm` → `nm isn't defined`; had to write `0 nm^-1`.
- **Fix:** after `)` or after a plain number followed by ` / `, a known unit name that is not a
  user variable could be read as a unit (as it already is right after a number).

### Q11. Shooting needs a lot of scaffolding
- Every eigenvalue problem needs: a function wrapping the solve, an upward scan loop, a bracket
  test, a `solve` on the bracket, and a counter. A boundary-value/eigenvalue form would read like
  the physics:
  `solve -ħ²/2m ψ'' + V ψ = E ψ with ψ(0) = 0, ψ(L) = 0 for x from 0 to L, eigenvalues E (3)`.

## Cosmetic

### Q12. Confusable-name warning across unrelated functions
- `κ` in `exact()` and `k` in `transmission()` (different functions) → "'κ' and 'k' look almost
  identical". κ is the textbook name for the decay constant; I renamed `k` to `k_in` instead.
- **Fix:** only warn when both names are visible in the same scope.

### Q13. ASCII names are shown converted in hints
- `psi_end` appears as `ψ_end` in "this happened when calling ψ_end". Harmless but surprising;
  show the name as written.

### Q14. Significant figures lost in list loops
- `for E in [0.50 eV, 1.00 eV, ...]` then `print E` shows `0.5 eV`, `1 eV`: the list elements
  forget their significant figures.

## Second pass

Problems: `21_finite_well.fm` (even/odd transcendental equations, state count, leakage),
`22_quartic_oscillator.fm` (V = βx⁴ by shooting, WKB, a Gaussian variational bound and a 4×4
oscillator-basis matrix), `23_hydrogen_radial.fm` (radial shooting for l = 0, 1 against −Ry/n²,
and ⟨r⟩ for 1s and 2p). Tests: `legacy/tests/test_gauntlet2_quantum.py`.
Severity: wrong answer / bug / awkward / cosmetic.

### Wrong answer

#### Q15. `solve` over a range that brackets several roots still returns a later one (Q1, still open)
- **Wanted:** the finite-well ground state straight from the matching condition in energy:
  ```
  V0 = 20.0 eV
  a = 0.500 nm
  k(E) = √(2 m_e (E + V0)) / ħ
  κ(E) = √(-2 m_e E) / ħ
  solve k(E) tan(k(E) a) = κ(E) for E from -19.99 eV to -0.001 eV
  print E in eV       # -17.149 eV: the third level (2nd even state); the ground state is -19.682 eV
  ```
- The reference (§10) still says "It finds the **first** solution after `a`". This is the most
  natural way to write a bound-state problem, and it gives a real eigenvalue, just the wrong one,
  silently.
- **Had to write:** the dimensionless form with one window per branch of tan, (jπ, jπ + π/2) and
  (jπ + π/2, (j+1)π), in a `while` loop. The shooting problems (22, 23) still need the hand-written
  bracket scan of Q1.
- **Fix:** always run the 200-point scan from `a` (it exists for the same-sign case) and refine the
  first sign change that is not a pole; at minimum, change the docs to "a solution".

### Bug

#### Q16. An ODE solution made inside a function can't be used in `∫` or an algebraic `solve`, and the error names an internal variable
- **Repro:**
  ```
  f(k) =
      solve y' = k y
        with y(0 s) = 1
        for t from 0 s to 1 s
      solve y(T) = 2 for T from 0 s to 1 s
      return T
  print f(1 / (1 s))
  # line 5: __sol.3 can't be used inside this integral/equation (only numbers, vectors and
  #   matrices can be captured from a function)
  ```
  The same happens with `return ∫ r u(r)² dr from r0 to r_cut / ∫ u(r)² dr from r0 to r_cut` in
  `23`. At top level both work.
- **Had to write (23):** carry the integrals along as extra unknowns, `N' = u²`, `M' = r u²`, and
  return `M(r_cut)/N(r_cut)`.
- **Fix:** let the quadrature and root-finder closures capture the solution (it is a pointer, like a
  list); at least never show `__sol.N`: "the solution y can't be used inside ∫ or solve within a
  function yet".

### Awkward

#### Q17. A solved ODE can't be returned from a function, so one function does two jobs via a flag
- For ⟨r⟩ the wavefunction must come from *the very same* integration the energy was tuned on: the
  outward 1s solution grows like e^{r/a₀} away from the eigenvalue, and a second function with the
  same equation, E and initial values but two extra unknowns (so a different step sequence) gave
  ⟨r⟩_1s = 2.59 a₀ instead of 1.5 a₀. So `radial(E, l, want)` returns u(r_max) when `want == 0` and
  ⟨r⟩ otherwise.
- Both `return`s must have the same units, so ⟨r⟩ is returned divided by a₀.
- **Fix:** allow a function to return a solution, or let `solve` be named (`sol = solve …`) so that
  `sol.u`, `sol.N` can be read later.

#### Q18. Matrices stop at 4×4 and can't be filled in a loop
- `identity(5)` → "matrices are at most 4×4"; `M[i, j] = -1 eV` in a loop → "the entries of a
  matrix can't be changed one at a time". So a tight-binding chain of more than four sites, or a
  basis of more than four oscillator states in `22`, is impossible, and the 4×4 in `22` is written
  out entry by entry as `H(0, 0)`, `H(0, 2)`, ….
- **Fix:** larger matrices (Jacobi is fine up to ~50), a constructor from a function of (i, j), or
  element assignment on a local matrix.

### Cosmetic

#### Q19. "f returns different kinds of values in different places" doesn't say which kinds
- **Repro:** `f(x) =` / `if x > 0` / `return 1 m` / `return 2` → the error points at `f(x) =` only.
  Hit when `radial` returned u (a plain number) in one branch and ⟨r⟩ (a length) in the other.
- **Fix:** name both units and both `return` lines ("line 3 returns a length [m], line 4 a plain
  number").

#### Q20. `κ` and `k` in the same function still warn
- `k0 = z/a` and `κ0 = √(−2mE)/ħ` in `21` (and `k(E)`, `κ(E)`): "look almost identical". In a
  square-well problem k and κ *are* the notation. The warning now comes once (fix #17), which is
  much better; consider exempting a pair whose two names have different units.

### What the first-pass fixes bought
- **Algebraic `solve` (#1)** carried all three problems: every eigenvalue, the WKB quantisation
  `solve action(E) = 2π ħ (n + ½) for E …` with an `∫` whose limits depend on E, and dE/dα = 0.
- **Leibniz rule (#18):** `E_var(α) = ∫ (ħ²/2m) dtrial(x, α)² + V(x) trial(x, α)² dx from -∞ to ∞`,
  then `dE = d/dα E_var` and `solve dE(α) = 0 …`, worked first time and agrees with the closed form
  to 10 digits. This is the variational method as written in Griffiths.
- **`eigenvalues` (#22):** the 4×4 Hamiltonian in the oscillator basis is diagonalised to machine
  precision; its lowest eigenvalue sits between E₀ and E_var, as it must.
- List element assignment (`E_shoot[2 found + p + 1] = E`), and 10-digit agreement of shooting with
  finite differences and with SciPy.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#66 (W)** With a variable `m`, `2 m c²` is silently read as 2 × the compound unit `m c²` (metre times c², because `c` is also a unit), with no error or warning, while `2 m v` is an ambiguity error (#10). Repro: `m = 2 kg` / `print 1 J / (2 m c²)` prints `5.56×10⁻¹⁸ kg/m`. Workaround: write `m_e` or `2*m`
- **#69 (B)** `solve … lowest N` evaluates the equation at the end points, so the hydrogen radial equation with −k/r and u(0) = 0 stops with "the equation can't be evaluated at x = 0 (SI units)" (the variable is r, not x). The boundary value is known there; only interior points are needed. Workaround: an inner wall at r = 10⁻²⁰ m
- **#70 (A)** The eigenstates of `solve … lowest N` are only second-order accurate (⟨1/r⟩ of hydrogen 2s/2p is 1.5×10⁻⁵ off with the default grid) while the eigenvalues are Richardson-extrapolated to ~10⁻¹⁰; reference §20 doesn't say so. Needed `grid 32000` for 10⁻⁷
- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#75 (A)** A function can't define a helper function inside it ("functions must be defined at the top level"); the pulse shape had to move to the top level
