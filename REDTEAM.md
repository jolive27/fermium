# Red team

An independent reviewer tries to break Fermium: wrong numbers, silent unit mistakes, misleading docs and
unfair benchmarks. Each finding is logged with a repro, what was done, and the test that proves the fix.
Tests are in `tests/test_redteam.py` unless noted.

## Round 1 (03:35 UTC)

### 1. Integrals over infinite ranges miss narrow peaks far from the start: partly handled by the main agent
- **Repro:** a narrow peak far from the lower limit (like a 1 μm wide Gaussian at 1 m, integrated from 0 to ∞) can be missed by the length-scale scan, and the result is too small with no warning. docs/reference.md §9 claimed infinite limits work "whatever the physical scale".
- **Done here:** the docs are precise now. §9 says the scan finds the scale of decays and of peaks near the start (or near 0), that a narrow peak far from the start can still be missed, and how to split the range. The §19 note that "infinite ranges don't have this problem" is corrected too.
- **Not done here:** the warning for narrow peaks is the main agent's work, and the quadrature kernel (`fm_quad*`, GK15) belongs to the numerics agent.
- **Test:** `test_1_reference_doesnt_promise_every_scale`.

### 2. Hz, rad/s, rev and rpm give wrong numbers silently
- **Repro:** `print 1 Hz in rpm` printed 9.5493 rpm (physicists expect 60). `f = 50 Hz; print f in rev/min` printed 477 rev/min, and `1 Hz in rev/s` printed 0.159 rev/s. None of these warned. `1 Hz + 1 rad/s` added silently. The old warning text ("1 rev/min is 2π/60 = 0.105 Hz") was shown for any angle unit, even when rpm wasn't involved.
- **Fix:** any conversion between a value written or shown in Hz and rev, rpm, rad/s or °/s now warns, in either direction, with the numbers for that case: "Hz here means rad/s (angles are plain numbers, 1 rev = 2π), so 1 Hz is 9.5493 rpm, not 60 rpm; 1 Hz = 1/(2π) rev/s". The hint says to write the value in rev/s, or to multiply by 2π. The other direction reads "1 rpm is 0.10472 Hz here, not 0.0166667 Hz". Adding or subtracting a Hz value and a rad/s (or rpm) value warns. `2π f` and `ω/(2π)` drop the old display unit, because they *are* the conversion. The tags are a light "cycles vs angular" label read from the unit the user wrote (DECISIONS D95; D27 revised). The numbers are unchanged. Only the warnings are new.
- **Tests:** `test_2_hz_to_turns_warns_with_the_right_numbers`, `test_2_turns_to_hz_warns_tailored`, `test_2_rad_per_s_in_hz_keeps_the_omega_warning`, `test_2_no_warning_for_safe_conversions`, `test_2_adding_hz_and_rad_per_s_warns`. `tests/test_regressions_check.py::test_rev_per_min_in_hz_warns` now checks the tailored text instead of "2π/60".

### 3. The adaptive-RK45 benchmark was unfair, and the README overstated it
- **Repro:** the README row "Damped spring, adaptive RK45, same accuracy: ~1.1× Julia (spot check)" couldn't be reproduced. benchmarks/fermium/spring_adaptive.fm said Fermium's tolerance 1e-10 matched the accuracy of Julia's rtol 1e-8 + atol 1e-10. It didn't: RESULTS.md itself showed x(100 s) = −2.3e-9 m (Fermium) against −1.95e-10 m (Julia), where the exact value is 1.1176e-12 m. Fermium's DP45 control is purely relative (D17) and Julia's was mixed, so the settings weren't comparable.
- **Fix:** all four versions now use pure-relative control at rtol 1e-6: Fermium `tolerance 1e-6`, Julia/Python `rtol=1e-6, atol=1e-30`, SciPy `rtol=1e-6, atol=1e-30`. The comments state the accuracy measured against the exact solution x(100 s) = 1.1176166148e-12 m. Fermium gives 1.11735e-12 m (relative error 2.4e-4, 4297 steps), and Julia/Python/SciPy give 1.11745e-12 m (relative error 1.5e-4, 4903 steps). That is comparable accuracy, not identical, because the step controllers differ. The runner's cross-language tolerance for x_100s is now 1e-3, which matches that accuracy (it was 1e-6 at the atol noise floor). The benchmark was re-run with `benchmarks/run.py --benchmarks spring_adaptive -r 5` three times while other agents loaded the machine. Fermium/Julia compute time came out at 0.90×, 0.80× and 0.83×, so the median is **0.83×**. Per step that is about 0.95×, since Fermium takes 4297 steps to Julia's 4903. Pure Python came out at 36.9×, 35.8× and 20.1× Julia, median ~36×. The README row gives 0.83× and ~36× and no longer says "spot check". Only the spring_adaptive rows of RESULTS.md and benchmarks/README.md were replaced, with a note. The runner rewrites the whole file, so the other rows were restored from the last full run. The README's pure-Python multipliers now match RESULTS.md (blackbody ~20.5×, unit loop ~86×, n-body ~58×). The "Julia spends that time JIT-compiling" sentence now says it is start-up, package loading, the warm-up call and JIT.
- **Tests:** `test_3_readme_adaptive_row_is_not_a_spot_check`, `test_3_readme_python_multipliers_match_results`.

### 4. `solve` returned a pole as a root
- **Repro:** `solve 1/(x - 1.5) = 2 for x from 1 to 2` printed 1.5 (the root is 2). `solve 1/(x - 1.5) = 0 for x from 1 to 2` printed 1.5, although there is no root. A scan point landed exactly on the pole, where f = ∞, and ∞ × (negative) < 0 counted as a sign change.
- **Fix:** in `fm_root` (codegen_llvm.py) and `interp.root`, mirrored step for step, a scan point with f = ±∞ is skipped: no sign change is taken across it. If the scan then finds no crossing and the finite values on the two sides of that point have opposite signs, the result is the existing "jump past each other" error at that point. A NaN at an Illinois iterate is the same error, instead of being returned as the root. The existing |f(root)| check against the bracket is now meaningful, because the bracket values are finite.
- **Tests:** `test_4_scan_point_on_a_pole_is_skipped`, `test_4_pole_without_root_is_an_error`, `test_4_even_pole_is_no_solution_and_nan_domains_still_work` (JIT and interpreter agree in each).

### 5. Fixed-step RK4 gave confidently wrong answers
- **Repro:** `solve x'' = -(10/(1 s))² x with x(0) = 1 m, x'(0) = 0 m/s for t from 0 s to 10 s step 0.1 s` printed 0.2515 m with no warning. The true value is cos(100) = 0.8623 m.
- **Fix:** after every RK4 solve, a cheap step-doubling check runs (`fm_rk4_check` in the JIT and `fermium build`, `interp.rk4_error` in the interpreter). At 8 evenly spaced stored steps it takes one RK4 step of 2h and compares it with the stored point two steps later. The difference is 30 times the local error, and multiplying by the number of steps estimates the global error relative to the solution's size. That costs 24 extra evaluations of the right-hand side. Above 10⁻³ a run-time warning appears, once per `solve` line: "the step is too coarse for this equation: the estimated error is 80% of the solution's size (fixed-step RK4, checked by step doubling); use a smaller step, or drop step to use the adaptive solver". It uses `fm_warn` kind 7 in core.py and aot_rt.c. None of the examples or benchmarks warn.
- **Tests:** `test_5_coarse_rk4_step_warns`, `test_5_fine_rk4_step_is_quiet`, `test_5_warns_once_in_a_loop`, `test_aot_runtime_messages`.

### 6. °C inconsistencies
- **Repro:** `sum([10 °C, 20 °C])` printed 303.15 °C without a word, while `10 °C + 20 °C` is an error. `T1 / 2` of a °C value was silent, while `2 T1` warned. The `*` warning always said "2 × 20 °C is 313.15 °C", even for `T1 * 1`.
- **Fix:** `sum` and `cumsum` of a list in °C/°F are errors ("can't add absolute temperatures …"; the hint says `mean` works and that changes should be written in K). `T / k` warns like `k T`. The warning's example uses the actual factor when it is a constant ("3 × 20 °C is 3 × 293.15 K = 606.3 °C"; "20 °C / 2 is 293.15 K / 2 = -126.575 °C"), a generic text when the factor is a variable, and nothing for a factor of exactly 1.
- **Tests:** `test_6_sum_of_celsius_is_an_error`, `test_6_dividing_celsius_warns_and_scaling_text_uses_the_factor`.

### 7. Misleading significant figures
- **Repro:** examples/03_damped_spring.fm printed `exact: 3.5 cm` next to `numerical: 3.52006 cm`, because the exact formula's `10.0 cm` has 3 significant figures and the result was rounded to 2.
- **Fix:** the example prints the exact value `to 6 digits` (`exact: 3.52006 cm`). A stray pasted comment in its header is fixed too.
- **Not changed:** carrying decimal places through + and − (`1.00 m - 0.999 m` prints 0.00100 m, which claims 3 figures). Significant figures are a compile-time property of each expression, but the decimal place of a difference depends on the result's magnitude, which is only known at run time. The print format would have to be computed at run time in the JIT, the interpreter and the C runtime, and every bootcamp box would change. Logged in DECISIONS D95 as not changed. It is a candidate for the uncertainties work (M4), where ± makes precision explicit.
- **Test:** `test_7_damped_spring_exact_value_has_six_digits`.

### 8. "this integral doesn't converge" for integrals that converge
- **Repro:** `print ∫ sin(x)/x dx from 0 to ∞` (= π/2) stopped with "this integral doesn't converge".
- **Fix:** the message is now "couldn't compute this integral numerically: it may diverge (like 1/x at 0) or oscillate without decaying (like sin(x)/x up to ∞), or the integrand is NaN or ∞ somewhere", in core.py (JIT and interpreter) and aot_rt.c. Tests that matched the old text were updated: test_adversarial.py, test_friction_solve.py, test_regressions_misc.py. docs/reference.md §9 and §19 were updated to match.
- **Tests:** `test_8_integral_message_doesnt_claim_divergence`, `test_aot_runtime_messages`.

### 10. Nits
- **σ and b_W were labelled exact but truncated** (5.670374419e-8 and 2.897771955e-3). They are now computed from their exact definitions in constants.py: σ = 2π⁵k⁴/(15h³c²), and b_W = hc/(k x), where x solves x = 5(1 − e⁻ˣ) (Newton's method, to full precision). **Test:** `test_10_sigma_and_wien_from_exact_definitions`.
- **No `kWh`:** there is now a prefixable `Wh` (3600 J), so `kWh`, `MWh` and so on work. **Test:** `test_10_kwh`.
- **`1 Gy + 1 Sv` and `1 Bq + 1 Hz` added silently** (the same SI dimension). Adding or subtracting values whose display units are a known confusable pair now warns: Gy/Sv, Bq/Hz, Bq/rad/s, J/N m (torque), and Hz/rad/s from #2 (D95). **Tests:** `test_10_confusable_units_warn_when_added`, `test_10_same_named_units_dont_warn`.

(The reviewer's list had no item 9.)

## Round 2 (05:00 UTC)

Reviewer: an independent subagent, on merge-agents at 889c57f (modules/stdlib D100–D103, seeded RNG/FFT/eigenvalue
problems/PDEs D80–D83, natural units D60, `analyze` D70, the revised unit-after-number rule D7). Every finding below
was reproduced with `fermium run` (JIT), and where noted with `--interp` and `fermium build`. Each has an
`xfail(strict=True)` test in `tests/test_redteam2.py` named `test_<N>_…`. Delete the mark when the finding is fixed.

**What held up (no findings despite targeted probing):**
- Natural units: conversions through ħc, c² and k_B, G = c = 1, Planck units, regions, and the "computed in natural units" errors.
- `analyze` in SI: the Reynolds number, Taylor's blast wave, the Planck length, the hydrogen energy scale, and fits with the defined function.
- The eigenvalue solver:
  - the finite well matches the transcendental equation to 1e-10;
  - the infinite well, harmonic oscillator and double well are right, and the linear potential matches the Airy zero;
  - the matrix and shooting methods agree.
- RNG: xoshiro256** matches the reference algorithm, moments over 10⁶ draws are right, and JIT = interp = build.
- FFT against NumPy: odd and even n, Nyquist doubling, Parseval.
- PDEs (heat, wave, TDSE) against closed forms, with *smooth, compatible* data.
- Unit checking of lists, branches, `rand`, `linspace`, `ifft` and PDE boundary values.
- Module errors.

### 1. `2 c` is the speed of light even when `c` is your own variable (wrong-answer, silent). Status: fixed
- **Fix:** the parser no longer exempts `c` from the D7 rule, so with your own c, `2 c * t` is the "ambiguous" error and `2 c` alone warns; `3 c` without your own c is still the speed of light (D130).
- **Repro:**
  ```
  c = 340 m/s          # speed of sound
  t = 2 s
  d = 2 c * t
  print d in m
  ```
- **Expected:** 1360 m, or the D7 error "'2 c' is ambiguous … write 2*c or 2 [c]". That error is what `2 g * h`, `2 N * x` and `0.5 m v²` give.
- **Actual:** `1.19917×10⁹ m`, with no error and no warning. `print 2 c` alone prints `2 c` without the D7 warning that `2 N`, `2 V` and `2 u` get.
- **Where:** fermium/parser.py, in `_colliding_unit` (line ~1204) and `_warn_bare_unit` (~1233, ~1236). Both skip the name `c` (`f.name != "c"`), probably so the built-in constant doesn't trigger them. The exemption should apply only while `c` is still the built-in constant.
- **Tests:** `test_1_two_c_times_t_with_your_own_c_is_not_the_speed_of_light`, `test_1_two_c_alone_with_your_own_c_warns`.

### 2. Crank–Nicolson with a coarse time step is silently wrong, even in sign (wrong-answer). Status: fixed
- **Fix:** step doubling checks CN/implicit: the default step is halved until the estimated error is under 0.1 % of the solution's largest value (up to 32 000 steps), and a step of your own that is too coarse warns, in the JIT and `--interp` (D131).
- **Repro:**
  ```
  L = 1 m
  D = 0.01 m²/s
  solve ∂u/∂t = D * ∂²u/∂x²
      with u(x, 0 s) = 2 K * sin(π x / L), u(0 m, t) = 0 K, u(L, t) = 0 K
      for x from 0 m to L, t from 0 s to 1000 s step 100 s
  print u(0.5 m, 1000 s) to 6 digits
  ```
- **Expected:** 2 K exp(−Dπ²t/L²) = 2.7×10⁻⁴³ K, or a warning like the RK4 step-doubling one (round 1 #5).
- **Actual:** `0.0328237 K`, with no warning.
  - Why: with r = Dπ²Δt/(2L²) = 4.9, CN's amplification factor for this mode is (1 − r)/(1 + r) = −0.66. So the "decay" alternates in sign and is 10⁴¹ times too slow.
  - Another case: D = 1 m²/s, u₀ = 1 K, t to 0.5 s with step 0.05 s gives u(0.5 m) = 0.00643 K, where the exact value is 0.00916 K (30 % off).
  - The reference says CN is "stable for any step". That is true, but it reads as "accurate for any step".
- **Where:** fermium/runtime/pde.py, `pde_solve`. There is no accuracy check on the time step. A step-doubling estimate, as for RK4, would catch this.
- **Test:** `test_2_coarse_crank_nicolson_step_is_right_or_warns`.

### 3. Crank–Nicolson keeps grid-scale wiggles from incompatible initial and boundary data, with default settings (wrong-answer). Status: fixed
- **Fix:** Rannacher-style start-up: CN's first 4 steps on real equations use L-stable, second-order SDIRK2, which damps the grid-scale modes the jump excites; both repros now match the analytic answers (D131).
- **Repro A (Neumann, the flux is wrong):**
  ```
  L = 1 m
  D = 1 m²/s
  solve ∂u/∂t = D * ∂²u/∂x²
      with u(x, 0 s) = 0 K, ∂u/∂x(0 m, t) = -1 K/m, u(L, t) = 0 K
      for x from 0 m to L, t from 0 s to 5 s
  print ∂u/∂x(0 m, 5 s) to 6 digits
  ```
  - **Expected:** −1 K/m, the imposed slope. By t = 5 s the solution is the steady line 1 K − x·1 K/m to within 10⁻⁵.
  - **Actual:** `−0.747629 K/m`. With `grid 4000` it is `−0.0100420 K/m`, and u(0 m) gets *worse* on the finer grid (0.99982 K becomes 0.99937 K).
  - Nearby points give −0.9938, −1.0210 and −0.9996 K/m: a sawtooth.
  - With initial data that already has slope −1 K/m the result is exact. So the cause is the start-up, not the boundary formula.
- **Repro B (Dirichlet, a wall that doesn't match u₀):** the same equation with `u(x, 0 s) = 1 K, u(0 m, t) = 0 K, u(L, t) = 0 K`, `t from 0 s to 2 s`, and `print u(0.005 m, 2 s)`.
  - **Expected:** 5.4×10⁻¹¹ K, from the Fourier series. The peak at L/2 is 3.4×10⁻⁹ K, and Fermium gets that right.
  - **Actual:** `−0.00265119 K`, and `0.00239863 K` at 2.5 mm. This ±0.0025 K sawtooth at the walls is 10⁶ times the real solution, and it would dominate `plot u vs x`.
- **Why:** CN's amplification factor for the stiffest grid modes is ≈ −1 (Δt D/h² is 800 at the defaults and 80 000 with grid 4000). So the jump between u₀ and the boundary condition never decays. The usual fix is a few backward-Euler (L-stable) start-up steps, known as Rannacher smoothing.
- **Where:** fermium/runtime/pde.py, `pde_solve` (the time stepping).
- **Tests:** `test_3_neumann_slope_at_the_boundary_is_the_one_imposed`, `test_3_step_initial_data_leaves_no_wiggle_at_the_wall`.

### 4. `std` of a single value is 0 (wrong-answer). Status: fixed
- **Fix:** `std` of one value is now the run-time error "std needs at least 2 values: it is the sample standard deviation, which divides by N − 1 …" in the JIT, the interpreter and `fermium build` (so `stats.standard_error([5 m])` too) (D133).
- **Repro:** `print std([5 m])`, and `import stats` then `print stats.standard_error([5 m])`.
- **Expected:** an error ("std needs at least 2 values") or NaN. The reference says `std` is the sample standard deviation (it divides by N − 1), which is 0/0 for one value. NumPy's `std(ddof=1)` gives nan.
- **Actual:** `0 m` for both, so one measurement is reported with zero uncertainty. The JIT, the interpreter and build agree.
- **Where:** fermium/interp.py, ~1682 `n - 1 if n >= 2 else 1` (and ~1629), mirrored in the JIT and the C runtime.
- **Test:** `test_4_std_of_a_single_value_is_not_zero` (2 cases).

### 5. stdlib `em.cyclotron_frequency` gives wrong numbers in rev/s and rpm (wrong-answer; only the JIT warns). Status: fixed
- **Fix:** `em.cyclotron_frequency` now returns the turning rate shown in rev/s (value |q|B/m, since 1 rev = 2π), so `in rev/s` and `in rpm` are right; `in Hz` gets the D95 warning; the module header and docs/stdlib.md explain the convention; test_stdlib checks it in rev/s (D134).
- **Repro:**
  ```
  import em
  f = em.cyclotron_frequency(e, 1 T, m_p)
  print f in rev/s to 6 digits
  ```
- **Expected:** eB/(2π m_p) = 1.52452×10⁷ turns per second. The module header says "_frequency … are in cycles per second".
- **Actual:** `2.42635×10⁶ rev/s`, which is 2π too small. `in rpm` gives 1.45581×10⁸ rpm instead of 9.14711×10⁸.
  - Cause: the function ends in `in Hz`, and Fermium's Hz is rad/s (D27/D95). So the value is tagged as an angular frequency.
  - The JIT warns, but the hint ("write it in rev/s instead of Hz … or multiply by 2π") asks for a change inside the stdlib that the user can't make. `--interp` prints the wrong number silently (#6).
  - `cyclotron_angular_frequency(...) in rev/s` is right.
- **Where:** fermium/stdlib/em.fm (`cyclotron_frequency … in Hz`). `skin_depth(… f [Hz] …)` takes the same kind of value. A likely fix is to return `in rev/s`, or to document the convention in docs/stdlib.md.
- **Test:** `test_5_cyclotron_frequency_in_rev_per_s`.

### 6. `fermium run --interp` never shows compile-time warnings (JIT/interp difference). Status: fixed
- **Fix:** `run_interpreted(..., show_warnings=True)`, used by `fermium run --interp`, writes the checker's warnings to stderr before running (and before a compile error), like `driver.run_source`; run-time warnings already went to stderr.
- **Repro:** a file with `f = 50 Hz` and `print f in rpm`, run with `fermium run file.fm` and with `fermium run --interp file.fm`.
- **Expected:** both print the D95 warning "Hz here means rad/s …, so 1 Hz is 9.5493 rpm, not 60 rpm".
- **Actual:** the JIT prints it. `--interp` prints only `477.465 rpm`.
  - The same is true of every checker warning: `'2 u' is the unit u, not your variable u`, "implicit multiplication binds tighter than '/'", °C scaling, and confusable units.
  - The playground passes a Diagnostics object and is fine.
- **Where:** fermium/cli.py, ~47–49. It calls `run_interpreted(src, args.file)` without `diags` and never prints `diags.warnings`. Compare `driver.run_source`, which prints them.
- **Test:** `test_6_interp_cli_shows_the_same_warnings_as_the_jit`.

### 7. A qualified module call can't be differentiated (docs/message). Status: fixed
- **Fix:** the differentiator resolves `mod.f(…)` to the module's function (bound in the caller's scope under a private name), so `T'`, `f'` and `d/dL mechanics.pendulum_period(…)` work like the `from … import` form.
- **Repro:**
  ```
  import mechanics
  T(L) = mechanics.pendulum_period(L, 9.81 m/s²)
  print T'(1 m)
  ```
  `f(T) = 2 astro.wien_peak(T)` then `f'(5000 K)` fails the same way, and so does `print d/dL mechanics.pendulum_period(L, 9.81 m/s²)`.
- **Expected:** 1.00303 s/m, which is what `from mechanics import pendulum_period` gives. The Modules section of the reference says imported functions "can be passed to other functions, integrated and differentiated".
- **Actual:** `line 2: can't differentiate this expression symbolically`, with the caret at the start of the right-hand side. Nothing says that the `mechanics.` form is the problem.
- **Where:** fermium/calculus.py, ~468. The symbolic differentiator doesn't resolve a qualified call `mod.f(…)`. It handles only the bare-name form and `mod.f'`.
- **Test:** `test_7_qualified_module_calls_can_be_differentiated` (2 cases).

### 8. `analyze` after `units natural` silently works modulo ħ and c, and says "length, mass, time" (message). Status: fixed
- **Fix:** `analyze` in a natural-units region works in SI dimensions (bracketed units read as SI, constants by their SI dimensions) and prints a line saying why; a variable computed in natural units asks for a bracketed unit; `pendulum(1 m, 9.81 m/s²)` works (D132).
- **Repro:** `units natural(ħ = c = 1)`, then `analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]`.
- **Expected:** either the SI analysis, or an explicit note that ħ = c = 1 leaves one dimension (energy).
- **Actual:** `4 quantities, 1 independent dimension (among length, mass, time) → 3 dimensionless groups`, then `so T = L · f(Π₂, Π₃)` and `defined pendulum(L) = L`.
  - The text names three dimensions and says only one is independent.
  - ħ and c enter the analysis without being listed.
  - `pendulum(1 m, 9.81 m/s²)` is then an arity error.
  - The reference section on `analyze` doesn't mention natural units.
- **Where:** fermium/dimanalysis.py, ~315. The dimension list uses SI names, while the rank is computed in the natural-unit dimension space.
- **Test:** `test_8_analyze_in_natural_units_says_so`.

### 9. stdlib `nuclear.bateman_daughter` is NaN for equal half-lives (wrong-answer: NaN, no error). Status: fixed
- **Fix:** `bateman_daughter` uses t e^(−λp t)(1 − e^(−δt))/δt with `expm1` when |δt| ≤ 1 (δt = (λd − λp) t), giving the limit N₀λt e^(−λt) for equal half-lives and full precision for nearly equal ones; the original form is kept for |δt| > 1 (no overflow).
- **Repro:** `from nuclear import bateman_daughter`, then `print bateman_daughter(1000, 10 s, 10 s, 5 s)`.
- **Expected:** the λ_d → λ_p limit N₀ λ t e^(−λt) = 245.066, or an error saying the formula needs different half-lives.
- **Actual:** `NaN`, from 0/0 in `N0 λp / (λd - λp) * (…)`. Nearly equal half-lives also lose digits to cancellation.
- **Where:** fermium/stdlib/nuclear.fm, `bateman_daughter`.
- **Test:** `test_9_bateman_daughter_equal_half_lives`.

### 10. Differentiating through a multi-line function: the error has no line and no caret (message). Status: fixed
- **Fix:** the error now points at the call of the multi-line function (`line 4`, caret at `g(t)`), with a hint (write it on one line with `where`, or use a finite difference); errors without a position inside a derivative get the position of the `'`.
- **Repro:**
  ```
  g(t) =
      a = 2 s
      t^2 / a
  f(t) = g(t) + 1 s
  print f'(5 s)
  ```
- **Expected:** `file.fm, line 5: …` with a caret, as `print g'(5 s)` gets ("line 4: can only differentiate one-line functions …").
- **Actual:** the bare text `can't differentiate through g: it's defined over several lines`, with no file, line or source line. The same happens with the stdlib (`f(t) = bateman_daughter(…)`).
- **Where:** fermium/checker.py, ~3259. It raises `FermiumError(...)` without a line or column.
- **Test:** `test_10_multiline_function_error_has_a_line`.

### 11. `using shooting` or `using explicit` on its own line gives "expected '=' in this equation" (message). Status: fixed
- **Fix:** `using …` / `method …` is accepted on a line of its own among a solve's clauses, for ODEs, eigenvalue problems and PDEs; giving it twice is an error; docs/reference.md §20 says so.
- **Repro:** an eigenvalue `solve` with its options on separate lines, `lowest 3` and then `using shooting`. Or a PDE with `using explicit` on its own line after the `for` line.
- **Expected:** it works, as `grid 1000` on its own line does, or an error that says `using` goes on the `for` or `lowest` line.
- **Actual:** `line 5: expected '=' in this equation but the line ended`, with the caret after `using shooting`. docs/reference.md §20 says only "`using matrix` … and `using shooting`, after `lowest N`".
- **Where:** fermium/parser.py, in the handling of `solve` option lines. `grid` is accepted on its own line; `using` isn't.
- **Test:** `test_11_using_on_its_own_line` (2 cases).

### 12. Eigenvalue problem in r: the singular-point error talks about x (message). Status: fixed
- **Fix:** the singular-point error names the problem's variable and uses its display unit (`at r = 0 nm`), in the JIT (two new fm_eigen arguments: the name's text id and format) and the interpreter.
- **Repro:** the textbook radial hydrogen problem: `V(r) = -e²/(4π ε₀ r)`, then `solve -ħ²/(2*m_e) * u'' + V(r) u = E u with u(0 nm) = 0, u(5 nm) = 0 for r from 0 nm to 5 nm lowest 3`.
- **Expected:** "can't be evaluated at r = 0 …", ideally in the user's units (nm).
- **Actual:** `the equation can't be evaluated at x = 0 (SI units): NaN or infinite; move the range's end away from a singular point`.
- **Where:** fermium/runtime/eigen.py, ~52, hard-codes `x`.
- **Test:** `test_12_singular_point_error_names_the_right_variable`.

### 13. Assigning to a module constant: the hint suggests `solve … for nuclear` (message). Status: fixed
- **Fix:** assigning to `module.name` says "can't change nuclear.a_V: a module's names can't be changed from outside it" with the hint to make a copy; other `a.b = …` targets get a hint about storing in a name of their own.
- **Repro:** `import nuclear`, then `nuclear.a_V = 16 MeV`.
- **Expected:** something like "a module's constants can't be changed; copy it: a_V = 16 MeV".
- **Actual:** `can't store a value in nuclear.a_V: the left side of = must be a variable name`, with the hint `to solve nuclear.a_V = … for nuclear, write  solve nuclear.a_V = … for nuclear from nuclear_min to nuclear_max`.
- **Where:** fermium/parser.py, ~326. The hint for an assignment target treats `nuclear` as an unknown.
- **Test:** `test_13_assigning_to_a_module_constant_has_a_sensible_hint`.

### 14. `sample` and `randn` accept nonsense arguments silently (message). Status: fixed
- **Fix:** `sample(expr, N)` needs N to be a whole number ≥ 0 and `randn(μ, σ)` needs σ ≥ 0: run-time errors 25/26 in the JIT, the interpreter and the C runtime of `fermium build` (D133).
- **Repro:**
  - `print len(sample(rand(), 2.5))` prints `2`.
  - `print sample(rand(), -1)` prints `[]`.
  - `print randn(1 m, -1 m)` prints `1.01896 m`.
- **Expected:** errors. The number of samples must be a whole number ≥ 0 (or ≥ 1), and σ must not be negative.
- **Where:** the `sample` and `randn` builtins (fermium/codegen_m3.py `builtin`, with mirrors in the interpreter and the C runtime).
- **Test:** `test_14_bad_sample_counts_and_negative_sigma_are_errors` (3 cases).

**Not reported,** by instruction or because they are already documented:
- the `∂u/∂t(x, t)` segfault, fixed on merge-agents in 889c57f;
- digit counts;
- a decay 1 fm wide integrated over 1 m giving 0 (§19, "narrow peak in a huge finite range");
- FFT results in `fermium build` differing from NumPy by 10⁻¹⁶.
