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

## Round 3 (06:30 UTC)

Reviewer: an independent subagent, on claude/lucid-gauss-9y1ov2 at de66686. The review covered uncertainties
(D120–D124), Python interop (D140–D142), complex numbers (D90–D94), the round 2 fixes (D130–D134), default printing
(D11), the zero-integral warning (D110), slices, cot and solve options (D111–D115), and modules. Each finding was
reproduced with the JIT (`driver.run_source`). Where noted it was also reproduced with the interpreter
(`interp.run_interpreted`) and `fermium build` (`aot.build`). Numbers were checked against closed forms, NumPy/SciPy
and the `uncertainties` package. Each finding has an `xfail(strict=True)` test in `tests/test_redteam3.py` named
`test_<N>_…`. Delete the mark when the finding is fixed.

**What held up (no findings despite targeted probing):**
- **Linear propagation** agrees with first-order theory in every case tried:
  - functions: `x^x`, `atan2`, `acos` near 1, `abs` at 0, `sqrt`, `∛`;
  - correlations: `x^2 - x x = 0 ± 0`, `L - L`;
  - sums, `Σ`, loops with `+=`, and `f'(a)` at an uncertain point;
  - lists: `cumsum`, `diff`, `sort`, slices, `mean(xs²)`, `interp`;
  - `in` conversions (°C → °F, km → mi), and natural, nuclear and astro units.
- **Fits with `±`:** A, τ and B come back with their covariance. `A + B`, `τ ln 2` and `A/τ` match
  `uncertainties.correlated_values` (built from SciPy's `curve_fit` covariance) to the printed digits.
- **`propagate montecarlo`:**
  - distributions: `exp(N(1, 0.5))` (lognormal: mean 3.08, sd 1.64), `|N(0,1)|` (0.80 ± 0.60) and `max(L, 1 m)`;
  - the projectile range at 45°;
  - correlations through the block;
  - the 2-sample floor, and non-finite samples.
- **Complex numbers:**
  - every function against `cmath`, including `z^100` and `z^1.5` with units;
  - a driven RLC circuit's steady-state current (the ODE against V/Z) and `𝑖ħψ' = Eψ`;
  - `until z.re = 0 m`, backward ranges, complex integrals and sums, and natural units;
  - the JIT, the interpreter and `fermium build` print the same, including NaN, ∞ and tiny parts.
- **Printing:** a 750-line random differential test of default significant figures, `to N digits` and units. The
  JIT, the interpreter and `fermium build` agree on every line.
- **Slices, trig functions and solve options:**
  - slices: empty, reversed, out of range, with `end`, of solutions, of slices, and in functions;
  - `cot`, `sec` and `csc` of plain numbers and lists;
  - solve options in either order on one line, and `using` or `until` on their own lines.
- **Python interop:**
  - units are checked at the boundary: declared, undeclared, and per call for generic functions;
  - lists are copies: a Python function that mutates its argument doesn't change the Fermium list;
  - wrong return kinds and int parameters are handled;
  - Python works in ODE right-hand sides, integrands, `solve … for x` and `propagate montecarlo`.
- **`fermium.compile`:** SI floats, `Q(…)` in other units, a new instance of a generic function for each new unit,
  and list or array arguments all work. Mixed-unit lists are refused.
- **PDEs:** the heat equation matches closed forms at the default step, with a decay term, time-dependent walls
  and advection. The coarse-step warning fires. The TDSE keeps its norm at 1.00.
- **stdlib:** `stats.linear_*` matches `numpy.polyfit`, SEMF(56, 26) = 495 MeV, and `hydrogen_level` is right.
  `box_energy` works in natural units, and `mechanics`, `nuclear` and `astro` accept uncertain arguments.

### 1. `36 km/h` is 36 km divided by Planck's constant, silently (wrong-answer). Status: fixed (an error with the hint `36 km/hr` (h stays Planck's constant; `[km/h]` and `in km/h` say so too); D180)
- **Repro:** `v = 36 km/h` then `print v`.
- **Expected:** 36 km/h (10 m/s). An error is also acceptable: `h` is not a unit, and the unit list says "for
  hours write hr".
- **Actual:** `5.43×10³⁷ s/(kg m)` with no warning. The unit chain after the number ends at `km`, so `/h` divides by
  the constant h.
  - `print 100 km/h in m/s` fails with "can't show a quantity with units [s/(kg m)] in m/s", which doesn't mention h.
  - `f(v [km/h]) = v` is a parse error ("expected ']'").
  - km/h is the most common speed unit in everyday problems.
- **Likely location:** the parser's unit-after-number rule (D7), and `checker.resolve_unit_si`. The checker already
  tells people that `h` isn't hours (hint "for hours write hr"), but only inside brackets.

### 2. `m c ΔT` with ΔT in °C silently uses the absolute temperature (wrong-answer). Status: fixed (a °C value in a Δ-named variable or parameter (also from Python) is an error; a °C number written into a product with units warns; D181)
- **Repro:** `print 1 kg * 4186 J/(kg K) * 10 °C`. The same happens in three other forms:
  - with `ΔT = 10 °C`;
  - with a function `heat(m [kg], ΔT [K]) = m 4186 J/(kg K) ΔT` called as `heat(1 kg, 10 °C)`;
  - from Python, as `fermium.compile(…).heat(1, Q(10, "°C"))`.
- **Expected:** 41 860 J, or the warning that `2 * (20 °C)` already gets ("this scales an absolute temperature").
- **Actual:** `1.19×10⁶ J` (10 °C is read as 283.15 K), with no warning in any of the four forms. D12 says to use K
  for differences, but Q = m c ΔT is the textbook formula, and the warning only fires when the result stays in °C.
- **Likely location:** the °C scaling warning, around line 1755 of checker.py. It should also fire when a °C value
  is multiplied into a product whose unit isn't a temperature, and when a °C argument is passed to a `[K]`
  parameter.

### 3. `20.0 °C ± 3%` is ±8.8 °C (wrong-answer). Status: fixed (3 % of the reading as written (±0.60 °C); D182)
- **Repro:** `t = 20.0 °C ± 3%` then `print t`.
- **Expected:** ±0.60 °C (3 % of the reading), or an error or warning that a relative uncertainty of a °C value is
  ambiguous.
- **Actual:** `20.0 ± 8.8 °C`, silently: 3 % of 293.15 K. §21 says the uncertainty of a temperature is a difference,
  and `20.0 ± 0.5 °C` is right, but the relative form uses the absolute value.
- **Likely location:** `pm_rel` (checker.py:2179, and `pm` in interp.py).

### 4. Crank–Nicolson step control misses the early transient (wrong-answer). Status: fixed (step doubling also compares the first steps and the first 1/8 of the run, and the early steps are kept as snapshots: repro B is 1.130 K (exact 1.131 K), repro A warns; D183)
- **Repro A:** `solve ∂u/∂t = D * ∂²u/∂x²` with:
  - D = 1 m²/s and L = 1 m;
  - `u(x, 0 s) = 1 K * sin(20 π x / L)`, with both walls at 0 K;
  - `for x from 0 m to L, t from 0 s to 100 s`.

  Then `print u(0.525 m, 0.1 s)`.
- **Expected:** ≈ 0 (the exact value is 3.5×10⁻¹⁷² K), or the "too coarse" warning.
- **Actual:** `-0.0120 K`: 1.2 % of the peak, with the wrong sign, and no warning.
- **Repro B:** two modes, `sin(πx) + sin(20πx)`, over 1 s:
  - `u(0.525 m, 0.5 ms)` is 0.864 K; the exact value is 1.131 K (24 % off);
  - at 1 ms it is 0.851 K; the exact value is 1.006 K.
- **Why:** the step-doubling check (D131) compares the two runs only at steps n/8, 2n/8, …, n (`_checks` in
  fermium/runtime/pde.py). By then the fast mode has decayed in both runs. The steps before n/8 are never checked.
  Those are the SDIRK2 start-up steps and the first CN steps, whose amplification factor for λΔt ≈ 4 is −0.14
  instead of e⁻⁴ = 0.02.
- **Likely location:** `_checks` and `CHECKPOINTS` in fermium/runtime/pde.py. The check should include early steps:
  the first few, and the times a user will evaluate.

### 5. The zero-integral warning misses vector integrands (wrong-answer). Status: fixed (a vector integral whose components were all 0 at every sample warns once, in the JIT, the interpreter and `fermium build` (D110))
- **Repro:** `print ∫ <exp(-x²), 0> dx from -1e6 to 1e6`.
- **Expected:** `<1.77, 0>`, or D110's warning. The scalar `∫ exp(-x²) dx from -1e6 to 1e6` warns.
- **Actual:** `<0, 0>`, silently, in the JIT, the interpreter and `fermium build`. A complex integrand does warn.
- **Likely location:** the vector path of `fm_quad` (D44's "quiet first try" per component) in codegen_llvm.py,
  interp.quad and aot_rt.c.

### 6. `fermium.compile` returns a complex number as a real 2-array (wrong-answer). Status: fixed (complex results come back as `fermium.ComplexQuantity`, a Python complex with `.unit`)
- **Repro:** `mod = fermium.compile("z = 3 + 4i\nf(x) = x + 1i\nw(x [Ω]) = x + 2i Ω\n")`, then `mod["z"]`,
  `mod.f(2)` and `mod.w(3)`.
- **Expected:** a Python complex (or a complex NumPy value) with its unit, or a clear error. The Python interop
  section of the reference says results are numbers, lists, vectors, matrices or booleans.
- **Actual:** `QuantityArray([3., 4.], '')`, `QuantityArray([2., 1.], '')` and `QuantityArray([3., 2.], 'Ω')`. These
  are the same as the vector <3, 4>, so NumPy's `abs(mod["z"])` gives `[3, 4]`, not 5.
- **Likely location:** result conversion in fermium/api.py. `ComplexTy` is a subclass of `VecTy` (D91).

### 7. `cot`, `sec`, `csc` of an uncertain value crash with a Python traceback (crash). Status: fixed (derivatives of cot, sec and csc added, and a test that every math function has one)
- **Repro:** `x = 1.0 ± 0.1` then `print cot(x)` (also `sec` and `csc`).
- **Expected:** `0.64 ± 0.14` (σ = csc²(1)·0.1), `1.85 ± 0.29` and `1.188 ± 0.076`.
- **Actual:** `KeyError: 'cot'` from fermium/uncertain.py:262 (`DERIV[name]`): a traceback instead of a message.
  D113 added these functions after D120 wrote the derivative table.
- **Likely location:** `DERIV` in fermium/uncertain.py.

### 8. A negative measurement with a unit can't be written (message). Status: fixed (the unit after σ is shared through a minus sign on either number)
- **Repro:** `q = -5.0 ± 0.2 m` then `print q`. Also `x = 5.0 ± -0.2 m`.
- **Expected:** `-5.00 ± 0.20 m`, as `(-5.0 ± 0.2) m` and `-5.0 ± 0.2` already give. For the second, "an
  uncertainty can't be negative", as the plain `5.0 ± -0.2` already says.
- **Actual:** both stop with "the uncertainty after ± is length [m] but the value is a plain number (no units); both
  need the same units". That is wrong about what was written, and negative charges and velocities are common.
- **Likely location:** the parser's ± rule. The unit after σ is shared only when the value is a bare number, and
  the unary minus makes it not bare.

### 9. Rounded large numbers print with trailing zeros that aren't significant (misleading display). Status: fixed (fixed in the main session: power of ten when rounding leaves 2+ non-significant zeros; D11)
- **Repro:**

  | Program | Prints | Note |
  |---|---|---|
  | `print 1000000 / 3` | `333000` | |
  | `x = 123456.7 m`, then `print x / 1.0` | `120000 m` | |
  | `print 1.5 * 12345.0` | `19000` | true value 18 517.5 |
  | `print 2999.85 * 1 MeV to 1 digits` | `3000 MeV` | |
  | `print complex(123456.7, 0.001)` | `120000 + 0.0010i` | |
  | the TDSE example's `ψ(0 nm, 0 fs)` | `<20000, …>` | |

- **Expected:** a form that shows the precision: `3.33×10⁵`, `1.2×10⁵ m`, `1.9×10⁴`, `3×10³ MeV`. `1234567.0` with
  3 figures already prints `1.23×10⁶`, and D11 keeps trailing zeros elsewhere so that `0.500` means 3 figures.
- **Actual:** below 10⁶ the output is positional, so a 2- or 3-figure result looks like an exact whole number. D11's
  rule "whole numbers below 10⁷ print exactly" makes this worse: `333000` reads as a count.
- **Likely location:** `units.format_number` and its mirrors (printing in core.py, and aot_rt.c). They should switch
  to ×10ⁿ when the rounding position is left of the units digit.

### 10. The Schrödinger PDE rejects the complex constant 𝑖 (complex × PDE). Status: fixed (𝑖 (and 1i) work in a PDE; a bare i is an error when the program has its own i; ψ(x, t) is a complex number; §20 updated; D184)
- **Repro:** the §20 TDSE example with `𝑖` instead of `i`. `𝑖` is the complex constant of D90 (Tab `\imag`):
  `solve 𝑖 ħ ∂ψ/∂t = … with ψ(x, 0 fs) = … exp(𝑖 k0 x), …`.
- **Expected:** the same solution as with `i`.
- **Actual:** "the initial value must be a number, but it is a complex number of a quantity with units
  [1/m^(1/2)]". Related problems:
  - Only a bare `i` works in a PDE. There it is always the imaginary unit: even when the program defines `i = 5`, the
    PDE silently ignores the variable. D90 says a bare `i` is always an ordinary name.
  - §20 still says "Fermium has no complex numbers" (in its section on Fourier transforms).
  - §20 also says ψ(x, t) is a 2-vector read with `.x`/`.y`, not `.re`/`.im`.
- **Likely location:** the initial-value type check in the PDE front end (fermium/solve.py and checker.py),
  fermium/runtime/pde.py, and docs/reference.md §20.

### 11. A run-time error inside a module gives the module's line as the program's line (message). Status: fixed (the error names the module and its line, at the calling line (JIT, interpreter, build); D185)
- **Repro:** a 4-line program: `import stats`, `x = 1`, `y = 2`, `print stats.standard_error([5 m])`.
- **Expected:** "line 4" (the call), or the module and its line (`stats.fm, line 6`), as compile-time errors in
  modules already give.
- **Actual:** `line 6: std needs at least 2 values …` in the JIT, the interpreter and `fermium build`. Line 6 is in
  stats.fm, but the message names no file. The same happens with:
  - an uncertain list: `stats.standard_error([1.0, 2.0, 3.0] ± 0.1)` → "line 6: this operation needs a plain
    number";
  - `astro.comoving_distance(1.0 ± 0.1, …)` → "line 35".
- **Likely location:** run-time line tracking in the interpreter (`self.line`) and in codegen. Both take the line
  from the module's nodes without remapping it.

### 12. `fermium build` repeats the zero-integral warning on every call (JIT/build difference). Status: fixed (the executable prints each warning text once, like the JIT)
- **Repro:** `f(a) = ∫ exp(-(x - a)²) dx from -1e6 to 1e6`, then `for k from 1 to 3` / `print f(k)`. Also
  `print ∫ exp(-x²) * (1 + 1i) dx from -1e6 to 1e6` (once per part).
- **Expected:** once per line, as D110 says and as the JIT and the interpreter do.
- **Actual:** the executable prints the warning 3 times (and twice for the complex integral).
- **Likely location:** the once-per-line bookkeeping of `fm_warn` kind 3 in aot_rt.c.

### 13. The `tolerance` option accepts units, and can't go on its own line (unit-safety / parse). Status: fixed (`tolerance` must be a plain number between 0 and 1 (separate messages), and can go on its own line)
- **Repro A:** `solve x' = -x / (1 s) with x(0 s) = 1 m for t from 0 s to 1 s tolerance 1e-6 m` (or `0.001 s`).
  - **Expected:** an error, because the tolerance is a relative plain number (§10).
  - **Actual:** accepted silently. Also, `tolerance 2` is refused with "must be a plain number like 1e-8", although
    2 is a plain number. The real problem there is the range 0–1.
- **Repro B:** a solve with `until y = 0 m` on its own line, followed by `tolerance 1e-12` on its own line.
  - **Expected:** the same as on one line. D111 says the options can come in any order, and `using bdf` and `until`
    work on their own lines.
  - **Actual:** "expected '=' in this equation but the line ended".
- **Likely location:** fermium/solve.py:443, which checks for `IConst` and the range but not the dimension; and the
  parser's handling of option lines after `for`.

### 14. Complex numbers are called vectors in error messages (message). Status: fixed ("a complex number", with hints for re(z)/im(z) and `.re`)
- **Repro:**
  - `use python numpy as np`, then `print np.abs(1 + 2i)` → "a Python function takes numbers and lists of numbers,
    but argument 1 is a vector", with the hint "pass the components one at a time, like v.x";
  - `solve z' = 1i z with z(0) = 1 for t from 0 to 1`, then `print z([0, 0.5])` → "z is a vector, so it can't be
    evaluated at each element of a list".
- **Expected:** "a complex number", and a hint with `re(z)`/`im(z)` (or `z.re`).
- **Likely location:** fermium/pyinterop.py:161 and checker.py:2546 test for `VecTy` before `ComplexTy`.

### 15. Nits in messages. Status: fixed (all three: no doubled word; "b is a list"; fermium.compile names itself, and the interop section states the limit)
- **Doubled word:** `use python numpy as np`, `units natural(ħ = c = 1)`, then `print np.sin(E / (1 MeV))` →
  "Python functions can't be called inside  units units natural (ħ = c = 1)  yet" (pyinterop.py:142).
- **Wrong reason:** a `propagate montecarlo` block with `b = [a, 2 a]` in it → "needs at least one formula
  (name = …) in its block". That line is a formula; the real problem is the list (checker.py:3386).
- **Wrong tool named:** `fermium.compile("L = 1.20 ± 0.01 m …")` → "uncertainties … work in programs
  (fermium run file.fm) but not yet in the REPL or Jupyter". The caller used neither (driver.py:247), and the Python
  interop section of the reference doesn't mention the limitation.

**Not reported** (checked, and either by design or already documented):
- `2 c` with a parameter or loop variable named c warns (D130).
- `1i^k` in a loop gives rounding noise like `-1.84×10⁻¹⁶ + …`.
- `floor` and `round` of an uncertain value drop σ.
- `1/x` in Monte Carlo with x ~ N(0, 1) gives a heavy-tailed result, as it should.
- `std` of an uncertain list is a clear error (D122).
- °C in a Python signature is refused.

## Round 4 (07:30 UTC)

Reviewer: an independent subagent, on claude/lucid-gauss-9y1ov2 at 6479b5d (round 3 fixes, D180–D185). Focus: (A) false
positives from tonight's new rules (D7/D130/D170/D171 unit after number, D180 km/h, D181 °C, D174 prefixes, D173/D112
limit warnings, D110 zero integrals, D131/D183 PDE step control, `tolerance`/`absolute`, D11 printing), from about 60
ordinary textbook programs across mechanics, E&M, thermo, optics, QM, nuclear and astro; (B) silent wrong answers in
feature interactions and printing. Every finding was reproduced with the JIT, and where noted with the interpreter and
`fermium build`. Each has an `xfail(strict=True, reason="red team round 4 #N")` test in `tests/test_redteam4.py` named
`test_<N>_…`. Delete the mark when the finding is fixed.

**What held up (no findings despite targeted probing):**
- Random differential testing of printing: 40 random programs of 60 prints each (magnitudes from 5e-324 to 1.8e308,
  negative numbers, 999.5/9.995/0.0009995 rounding boundaries, ±∞, NaN, lists, vectors, `in`, `to N digits`), and 12
  random programs with integrals, ODEs (RK45 and RK4), roots, Σ, derivatives, slices and °C conversions. JIT,
  interpreter and `fermium build` printed identical text in every case, and agreed on run-time warnings.
- 999.5 → `1.0×10³`, 9.996 → `10.0`, −9.996 → `-10.0`, `1e-320` → `1.0×10⁻³²⁰`, `-0.0` → `0`, `[1.0, ∞, -∞, NaN]`.
- `km/hr`, `kph`, `mph`; `2 °C/min`, `J/(g °C)`, `dT = 2 °C/min`, `ΔT = T2 - T1`, weighted means of °C readings,
  `linspace` and `for` over °C, `ΔT` from a list difference, `20.0 °C ± 0.5 °C`, `68.0 ± 1.0 °F in K`.
- Uncertainties through stdlib modules (`mechanics.pendulum_period`, `nuclear.activity`, `stats.linear_slope`)
  keep correlations: `T - T2` is `0 ± 0`, and so are `xs[1] - xs[1]` and slice sums.
- A TDSE free packet against the analytic spreading (0.25 % at the peak, within the stated 0.1 % of the maximum); a
  stationary state's phase `exp(-i E₁ t/ħ)`; the zero-integral warning doesn't fire on vectors with one zero component.
- `absolute` with units, with the wrong units (a clear error), and negative (an error); the RLC impedance with `1i`.

### 1. `absolute 1e-6 °C` is read as the absolute temperature 274.15 K (wrong-answer). Status: fixed (D200): an absolute tolerance in °C/°F is a temperature step; `absolute 1e-6 °C` gives 23.4851 °C, as K does.
- **Repro:** `k = 0.1 1/min`, then
  `solve T' = -k (T - 20 °C) with T(0 min) = 90 °C for t from 0 min to 30 min tolerance 1e-10 absolute 1e-6 °C`,
  `print T(30 min) in °C to 6 digits`.
- **Expected:** 23.4851 °C (20 + 70 e⁻³), as with `absolute 1e-6 K`; or an error asking for the tolerance in K.
- **Actual:** 26.6536 °C, with no warning (24.1 °C with the default tolerance and `absolute 0.001 °C`). The JIT, the
  interpreter and `fermium build` agree. An absolute tolerance is a size of error, so it is a temperature *step*;
  D12 reads a °C value as an absolute temperature, 274.15 K, which switches the error control off.
- **Likely location:** the `absolute` option's constant folding in fermium/solve.py (the value is converted to SI
  with the °C offset). `± 0.5 °C` already reads °C as a difference (D120); `absolute` should do the same, or refuse
  °C/°F.

### 2. Hz ↔ rpm at the Python boundary converts silently (wrong-answer). Status: fixed (D202): `use python` and `fermium.compile` calls give the D95 Hz ↔ rev/rpm warning when the argument and the declared unit are in different families (the value is converted by the rad = 1 rule, as `in` does).
- **Repro:** `use python numpy as np:` / `    positive(f [Hz]) -> [Hz]` / `print np.positive(60 rpm)`.
  Also `fermium.compile("f(freq [Hz]) = freq").f(Q(60, "rpm"))` → `6.28319 Hz`, and a function declared
  `(w [rpm]) -> [rpm]` given `1 Hz` → `9.55 rpm`.
- **Expected:** round 1 #2's warning ("Hz here means rad/s … 1 Hz is 9.5493 rpm, not 60 rpm"), since Python
  receives the number in the declared unit.
- **Actual:** `6.28 Hz` and no warning: the Python function receives 2π instead of 1. `print 60 rpm in Hz` in
  Fermium does warn.
- **Likely location:** fermium/pyinterop.py (argument and result conversion to the declared unit) and
  fermium/api.py (`Q(…)` arguments to `fermium.compile` functions); neither consults the D95 cycles/angular tags.

### 3. `1.5 kT` with your own k and T is 1.5 kilotesla, printed as "1.5 kT" (wrong-answer, silent). Status: fixed (D203): a prefixed unit that spells two of your variables (`1.5 kT`) warns and suggests `1.5 k T`; units every course uses (`nm`, `kg`, `mV`, …) are exempt.
- **Repro:** `k = 1.38e-23 J/K`, `T = 300 K`, `E = 1.5 kT`, `print E`.
- **Expected:** a warning or an error: `kT` is the unit kilotesla, but k and T are both your variables (write `k T`).
- **Actual:** prints `1.5 kT` with no diagnostic. The output looks exactly like the intended formula. D7 only checks
  whether the *unit's name* is one of your variables, not whether a prefixed unit spells two of them (`kT`; `mV`
  with your m and V; `nA`, `pT`, …). Gotcha 4 ("`LT` is one name") doesn't help, because here it isn't a name.
- **Likely location:** the unit-after-number rule in fermium/parser.py (`unit_expr`): a check "prefix and base are
  both your variables" next to the D7 collision check.

### 4. `60 s / (m c_w)` with your own m is a parse error (false-positive). Status: fixed (D204): `/ (…)` after a unit continues it only if every name in the bracket is a unit and (with a spaced `/`) none is your variable; `P 60 s / (m c_w)` is 14.3 K.
- **Repro:** `P = 1000 W`, `m = 1 kg`, `c_w = 4186 J/(kg K)`, `print P 60 s / (m c_w)` (ΔT = P t / (m c)).
- **Expected:** 14.3 K (60 000 J / 4186 J/K).
- **Actual:** `expected ')' but found 'c_w'`. Also `10 m / (s τ)`, `1000 J / (kg cw)` and `5 s / (ohm Cv)` give
  "expected ')'": a bracket after a unit and a spaced `/`, whose first name is a unit, is taken as a unit denominator
  (D171) before checking that the whole bracket is units. With `mass` instead of `m` it works.
- **Likely location:** fermium/parser.py, the D171 `/(units)` look-ahead (`_bracket_is_unit` or the `unit_expr`
  continuation after `/`): it should look at every name in the bracket, and otherwise leave the `/` as a division.

### 5. `∫ x * -2 dx from 0 to 1` says the dx is missing (false-positive). Status: fixed: the `dx` is found inside a negated factor (`∫ x * -2 dx` is −1).
- **Repro:** `print ∫ x * -2 dx from 0 to 1`.
- **Expected:** `-1`.
- **Actual:** `this integral is missing its 'dx' (the variable to integrate over)`. `∫ (-2) * x dx` works.
- **Likely location:** fermium/parser.py (~line 2155, the integral's `dx` search): the unary minus's operand is
  parsed so that `dx` is consumed, or the integrand stops before it.

### 6. The D173 limit warning fires when the other reading is a unit error (false-positive warning). Status: fixed (D205): the limit warnings are decided in the checker; `to T - t0` with an integrand in m/s is quiet, and a limit that is a unit error suggests `(… to T) - x0`.
- **Repro:** `v = 3 m/s`, `T = 10 s`, `t0 = 2 s`, `print ∫ v dt from 0 s to T - t0`.
- **Expected:** `24 m` with no warning. The alternative reading `(∫ … to T) - t0` is metres minus seconds, so only
  the limit reading is possible. The same holds for `∫ k λ / x dx from a to L - a` (volts minus metres).
- **Actual:** `24 m` and "the ' - t0' is part of the upper limit … if that's what you meant, write to (T - t0)".
  `∫_a^{L-a}` and `∫_0^{T-t0}` are far more common in textbooks than the δ = 2∫… − π form D173 was made for.
- **Likely location:** the D173 warning is emitted in the parser. It should be decided in the checker, and only
  when the integral and the added term have the same dimension.

### 7. A spaced `/` after the upper limit is refused even when only the limit reading has the right units, and the hint points the wrong way (false-positive/message). Status: fixed (D205): the error now says the ' / ' divides the whole integral, with the hint `to (E / (2 P0))`; the D112 warning fires only when the divisor is a plain number.
- **Repro:** `E = 3600 J`, `P0 = 2 W`, `print ∫ P0 dt from 0 s to E / (2 P0)` (also `to E / P0`).
- **Expected:** 3600 J (the limit is E/(2 P0) = 900 s, a time), or an error that suggests `to (E / (2 P0))`.
- **Actual:** the D112 warning, then the error "the limits of this integral are time [s] and energy [J]" with the
  hint "put the integral in parentheses: (∫ ... dx from a to b) / M", which is the reading the units rule out.
- **Likely location:** D34/D112 in the parser, and the limit-units error at fermium/checker.py:288. When the
  whole-integral reading fails on the limit's units and `(limit / divisor)` has the variable's units, say so.

### 8. PDE step control warns on the textbook step-change problem although the answer is accurate (false-positive warning). Status: fixed (D206): step doubling measures the error against the solution's range and skips the sub-grid layer of an initial/boundary jump; 65.8435 K with no warning.
- **Repro:** `L = 1 m`, `D = 1e-4 m^2/s`,
  `solve ∂u/∂t = D * ∂²u/∂x² with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(L, t) = 0 K for x from 0 m to L, t from 0 s to 1000 s`,
  `print u(0.1 m, 1000 s) to 6 digits`. The same happens with 20 °C / 100 °C, or 293.15 K / 373.15 K.
- **Expected:** 65.8436 K (Fourier series; erfc gives 65.8451 K) and no warning.
- **Actual:** 65.8435 K (right to 10⁻⁶) with "this PDE's time step could not be made fine enough: with 32000 steps
  the estimated error is still 0.8% of the solution's largest value … the result may be inaccurate". With the °C or K
  offsets it says 0.17 %: the absolute estimate is the same, so whether it warns depends on where zero is. It takes
  about 3 s.
- **Likely location:** D183's start-of-run comparison in fermium/runtime/pde.py. A jump between the initial value
  and a boundary value is a non-smooth start that step doubling can't resolve in the first steps, and it doesn't
  affect later times. D131's SDIRK2 start-up already handles it.

### 9. The hint for `8.5e28 m^-3` next to your own m suggests `8.5e+28 [m]` (message). Status: fixed (D207): the warning quotes `8.5e28 m^-3` as written, hint `8.5e28 [m^-3] (or [1/m³])`.
- **Repro:** `m = 9.11e-31 kg`, `n = 8.5e28 m^-3`, `print n` (an electron density next to the electron mass).
- **Expected:** the hint `write 8.5e28 [m^-3] (or [1/m³]) to say so`, with the number as written.
- **Actual:** "'8.5e+28 m' is the unit m, not your variable m", with the hint "write 8.5e+28 [m] to say so".
  Following the hint gives a length, and the number is reformatted with `e+`.
- **Likely location:** the D7 lone-unit warning in fermium/parser.py. It quotes only the first unit factor and
  formats the number with `repr`.

### 10. One token gets a warning ("is the unit") and then an error ("is ambiguous") (message). Status: fixed (D207): the ambiguity error drops the parser's earlier warnings for the same quantity.
- **Repro:** `m = 1.2 kg`, `print 2.2 * 5000 m`. Also `E1 = π^2 ħ^2 / (2 m L^2)` with your own m and L: a warning
  "reading 'L' as your variable L, not the unit L … write [m L]", then the error "'2 m' is ambiguous".
- **Expected:** the error alone.
- **Actual:** "warning: line 2: '5000 m' is the unit m, not your variable m … for 5000 × your variable write 5000*m",
  followed by "'5000 m' is ambiguous". The two messages contradict each other about the same token.
- **Likely location:** fermium/parser.py. The lone-unit warning is emitted before the enclosing product is seen; it
  should be deferred (or dropped) when the D7 rule 5 error follows.

### 11. An uncertainty that is only rounding noise sets the printed digits (misleading display). Status: fixed (D208): a cancellation to rounding noise leaves σ = 0, and σ = 0 prints the value by D11: `2.01 ± 0 s/m^(1/2)`, `0 ± 0 m`.
- **Repro:** `L = 1.000 ± 0.010 m`, `g = 9.81 m/s^2`, `T = 2 π sqrt(L / g)`, `print T / sqrt(L)`; and
  `y = 1.000 ± 0.010 m`, `print (y / 3) * 3 - y`.
- **Expected:** `2.01 ± 0 s/m^(1/2)` (or `2.006 ± 0`), and `0 ± 0 m`, as `x - x` already prints.
- **Actual:** `2.0060666807106475318 ± 0.0000000000000000035 s/m^(1/2)` and `(0.0 ± 1.7)×10⁻¹⁸ m`. The linear
  propagation leaves σ ≈ 10⁻¹⁸ (rounding in ∂f/∂x), and "values are shown to the second digit of σ" then prints 20
  digits.
- **Likely location:** fermium/uncertain.py (a σ below ~10⁻¹⁴ of the value, or of the inputs' σ, should count as
  0) and the ± formatter in fermium/units.py.

### 12. Quadrature and vector rounding noise prints as a 3-figure result (misleading display). Status: fixed in the main session (D11, D197): integral noise below 50ε·∫|f| prints 0, vector noise too.
- **Repro:**
  - `print ∫ sin(x) dx from -π to π` → `3.19×10⁻¹⁶`;
  - `f(x) = <cos(x), sin(x), 0>`, `print ∫ f(x) dx from 0 to π` → `<1.67×10⁻¹⁶, 2.00, 0>`;
  - the on-axis Biot–Savart loop `∫ dB(φ) dφ from 0 to 2 π` → `<-2.4×10⁻²², -8.1×10⁻²³, 4.5×10⁻⁶> T`;
  - a symmetric line charge's Eₓ → `3.6×10⁻¹⁵ V/m`.
- **Expected:** 0 (and `<0, 2, 0>`, `<0, 0, 4.5×10⁻⁶> T`). Complex numbers already drop such noise
  (`exp(1i π)` prints `-1 + 0i`); vectors and integrals don't.
- **Actual:** a number shown with 3 significant figures, none of which is significant. Students read
  `3.6×10⁻¹⁵ V/m` as a (tiny) physical field.
- **Likely location:** `fm_quad` knows ∫|f| (`fm.qabs`, D110), and a result below ~10⁻¹³ × ∫|f| is 0 to the
  quadrature's accuracy. For vectors, the component-wise formatter could drop components below ~10⁻¹⁴ × |v|, as
  the complex formatter does.

### 13. `2 kg c²` shows its SI value with 6 significant figures (display). Status: fixed in the main session (D11): the SI echo uses the value's own significant figures, `(= 1.80×10¹⁷ J)`.
- **Repro:** `print 2 kg c^2`; also `print 1 MeV/c^2` → `1 MeV/c² (= 1.78266×10⁻³⁰ kg)`.
- **Expected:** `(= 1.80×10¹⁷ J)`, the D11 default of 3 figures. `print 2 * 1 kg * c^2` gives `1.80×10¹⁷ J`.
- **Actual:** `2 kg c² (= 1.79751×10¹⁷ J)`. The parenthetical still uses the old 6-figure rule.
- **Likely location:** the unit-with-constant display in fermium/units.py (and its mirrors in core.py and aot_rt.c).

### 14. Lists pad exact integers and 1-figure literals to the longest element, and `10000000` prints as `1×10⁷` (display). Status: fixed in the main session (D11): written whole numbers print as written, in lists too (`[1.2345, 2]`, `90`, `10000000`).
- **Repro:**
  - `print [1.2345, 2]` → `[1.2345, 2.0000]`;
  - `print [5.018245e9, -6934.574, 9e1]` → `[5.018245×10⁹, -6934.574, 90.00000]`;
  - `print <7.1094e-12, 39> N` → `<7.1094×10⁻¹², 39.000> N`;
  - `print 10000000` → `1×10⁷`, while `print 123456789` prints as written (`1.23456789×10⁸`).
- **Expected:** each element as written when the list is written out (D11): `[1.2345, 2]`, `90`, `39`, and
  `10000000` (or `1.00×10⁷`).
- **Actual:** the "as written" style gives every element the figure count of the most precise one, so an exact 2
  claims 5 figures and `9e1` (1 figure) claims 7. The JIT, the interpreter and `fermium build` agree.
- **Likely location:** the list "as written" branch of `format_default` in fermium/units.py (and `fmt_default` in
  aot_rt.c), and the literal-as-written path for a whole number ≥ 10⁷.

### 15. The bootcamp still says `0.5 m v^2` means metres with a warning, and that `1/2 m v^2` warns (docs). Status: fixed: lesson02 (lines 183, 215, 244), CHEATSHEET gotcha 3 and TROUBLESHOOTING's contents now describe the error.
- **Where:**
  - bootcamp/lesson02_variables_formulas.md:215: "`0.5 m v^2 where m = 2 kg` means 0.5 *metres* (Fermium warns
    you)". It is the error "'0.5 m' is ambiguous: … also the m from 'where'".
  - lesson02:244 (the summary): "`0.5 m v^2` uses *metres*".
  - lesson02:183 and CHEATSHEET gotcha 3: "`1/2 m v^2` means 1/(2mv²) … Fermium warns about this". With a variable
    m it is now the error "'2 m' is ambiguous". TROUBLESHOOTING #11 shows the warning, but with `mass`.
  - TROUBLESHOOTING.md:26: the table of contents says "10. warning: 'm' after the number means the unit m", but the
    section shows the error.
- **Expected:** the prose says what the output boxes show (the boxes are right).

### 16. Exact ties round half to even: `0.125` → `0.12`, `2.5e6 × 1.3` → `3.2×10⁶` (display/docs). Status: documented in the main session (D11: ties round half to even, as C's printf, NumPy and Julia do); the test now asserts the documented behaviour.
- **Repro:** `print 0.125 * 1.0`, `print 2.5e6 * 1.3`, `print [0.125, 0.375] * 1.0` → `[0.12, 0.38]`.
- **Expected:** either school rounding (`0.13`, `3.3×10⁶`), which is what a student's calculator and textbook give,
  or a sentence in reference §4 saying that ties go to the even digit (ISO 80000-1's rule B).
- **Actual:** half-to-even (Python's `format`), undocumented. Students checking a lab value by hand see a different
  last digit.
- **Likely location:** `format_number` in fermium/units.py, `fmt_default` in aot_rt.c, and docs/reference.md §4.

### 17. Slicing a data table: the error calls it "not a list" and suggests vector components (message). Status: fixed (D209): slicing a table is an error about tables, whose hint shows `fit … to table(L = data.L[2:5], T = data.T[2:5])`.
- **Repro:** `data = load "pendulum.csv"`, `fit T = 2 π sqrt(L / g) to data[2:5]`.
- **Expected:** a message about tables (fit a subset by slicing the columns, or "fitting part of a table isn't
  supported yet").
- **Actual:** "only lists can be sliced with [a:b], and this isn't a list", with the hint "pick single components
  with v[1], v[2], ...", which is the hint for a vector.
- **Likely location:** the slice check in fermium/checker.py (its hint assumes `VecTy`).

### 18. An absolute tolerance larger than the solution is accepted silently (wrong-answer, user-set). Status: fixed (D201): an absolute tolerance at least as large as the largest starting value in its units warns.
- **Repro:** `solve x'' = -x / (1 s)^2 with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 10 s absolute 1 km`,
  `print x(10 s)`.
- **Expected:** a warning: the absolute tolerance is 1000× the largest initial value, so the error control is off.
  (`tolerance` outside (0, 1) is already refused.)
- **Actual:** `89.2 m` (the exact value is −0.839 m) with no warning. A mistyped unit (km for mm) gives a silently
  wrong run.
- **Likely location:** the `absolute` checks in fermium/solve.py (D160). Compare each value with the unknown's
  initial value (and with its range, when that is known at compile time).

**Not reported** (checked, and either by design or already documented):
- `h = 10 m`, `U = 12 V`, `T = 300 K` and `Q = 5 C` next to your own m, V, K and C warn (D7 rule 5). `2 m v`, `2 g h`,
  `0.5 m v^2`, `2 m c^2`, `8.5e28 /m^3` and `0.1 /s` with your own m or s are errors (D7, D170, D171).
- `double_speed(c) = 2 c` warns and means the speed of light (D130).
- `n R 25 °C` and `k_B * 27 °C` warn that the reading enters as an absolute temperature (D181).
- `x = 1/2 m` is 0.500 1/m (reference §3 says so, without a warning).
- `tolerance r` with a variable, and `tolerance 10^-8`, are refused ("a plain number written out"). `tolerance` next
  to `step` is ignored, as §10 says.
- Declaring `np.sin(x [deg])` passes degrees to a function that takes radians: the user's declaration is wrong.
- `[20 °C, 30 °C] in K` prints `[293, 303] K` (3 figures of 293.15), and `1.5 km in m` prints `1.5×10³ m` (D11).

## Round 5 (08:45 UTC)

Reviewer: an independent subagent, on claude/lucid-gauss-9y1ov2 at 7c7fc24. The focus was tooling and the beginner
journey with tonight's new features: the REPL, the Jupyter kernel, the language server, `fermium fmt`, `fermium
build`, the playground glue (web/playground.py, run without llvmlite, as in Pyodide), `fermium doctor` and `fermium
check`, and bootcamp lessons 0–3 typed as a beginner would. Each finding has an `xfail(strict=True)` test in
`tests/test_redteam5.py` named `test_<N>_…`. Delete the mark when the finding is fixed. (REDTEAM.md had no "Round 4"
section when this round started.)

**What held up (no findings despite targeted probing):**
- **`fermium fmt --pretty` / `--ascii`** round trips on programs with ±/+-, 𝑖/1i (including `𝑖ħ ψ'`, `2π𝑖`, `k𝑖`),
  ∂/partial (including `∂²/∂x²`), slices, `parallel for`, `use python` with declared signatures (Python attributes
  like `np.pi` and `sc.alpha` are left alone), `import … as`, `from … import`, `absolute`, `until`, plot options,
  `± 3%` on °C, and uncertain lists: each output parsed and printed exactly the same results.
- **`fermium build`** gave output identical to `fermium run` for complex numbers, stdlib modules, `parallel for`
  (sums too), slices, `table(…)` fits, `analyze`, natural and nuclear units, matrices with element assignment,
  3-D vector ODEs, `absolute`, `polar`, FFT spectra and plots with `reversed x`, `xlabel`, axis ranges and `log y`.
  It refused ±, `use python` and eigenvalue problems with clear one-line messages.
- **Playground glue:** stdlib imports, complex numbers, `parallel for`, ±, `use python` (numpy is preloaded, other
  packages are fetched on the retry), natural units, `analyze`, `table(…)` fits and seeded random numbers all work
  without llvmlite; a missing `import "file.fm"` is a clear error.
- **The REPL and Jupyter** (apart from #1–#3): complex numbers, stdlib imports, `use python`, `parallel for`,
  multi-line `solve … with … for` blocks, function redefinition, inline plots, `to N digits`, and recovery after
  unit errors and run-time errors. The ± refusal message is clear.
- **Language server:** diagnostics positions for D7, D170, D171, D180, D181 (the Δ-name error), D173 and D112
  warnings; hover on complex values, uncertain values, slices, `parallel for` variables, `from`-imported functions and
  module names; completion of `\pm`, `\imag` and of members after `mechanics.`.
- `fermium doctor` (including the C-compiler line) and the non-editable install: a wheel built from pyproject.toml
  contains the stdlib modules, and `import mechanics`, `parallel for` and `fermium build` work from it.

### 1. A failed REPL input leaves its new variables half-defined; every later use is an internal error (crash). Status: fixed (D220): the REPL session rolls the checker's scope back on any failed input, so the name can be used and defined again
- **Repro (REPL):**
  ```
  fm> L = 1.20 +- 0.01 m      → the documented "uncertainties … not yet in the REPL" message
  fm> L = 3 m                 → internal error in Fermium: TypeError: unsupported operand type(s) for *: 'int' and 'NoneType'
  fm> print L                 → the same internal error
  ```
  The same happens after any compile error in a multi-line input that created a variable before the error
  (`if 1 > 0` / `    ww = 1 m` / `    ww2 = ww + 1 s`, then `print ww`). The name can't be used or re-assigned for the
  rest of the session. The checker keeps the new symbol, but codegen never gave it an arena slot
  (`codegen_llvm.slot`: `8 * sym.slot` with `slot = None`).
- **Tests:** `test_1_repl_failed_input_does_not_poison_its_variables`, `test_1_repl_compile_error_in_a_block_does_not_poison_its_variables`.

### 2. Jupyter: after that, the cell gets no reply at all (crash). Status: fixed (D220): `do_execute` catches every exception and always replies (status error, one-line message)
- **Repro:** cell 1 `a5 = 1 m` / `b5 = a5 + 1 s` (the unit error, fine); cell 2 `print a5`. The kernel's message
  handler raises the TypeError from #1. `FermiumKernel.do_execute` only catches `FermiumError`, so no
  `execute_reply` is sent: the notebook shows no output and no error for the cell, and the kernel log shows a Python
  traceback. The same happens after the ± refusal (`L = 1.20 +- 0.01 m`, then `print L`). The REPL at least prints
  "internal error in Fermium".
- **Test:** `test_2_jupyter_cell_after_a_failed_cell_gets_a_reply`.

### 3. Jupyter and the playground drop run-time warnings, so wrong answers look fine (wrong-answer, tool-mismatch). Status: fixed (D223): Jupyter streams run-time warnings on stderr in order with the output; the playground adds them to `warnings`
- **Repro:** `solve y'' = -(10/(1 s))^2 y with y(0) = 1 cm, y'(0) = 0 m/s for t from 0 s to 10 s step 0.1 s` then
  `print y(10 s)` prints `0.252 cm` (the true value is 0.862 cm). `fermium run` and the REPL add *the step is too
  coarse for this equation: the estimated error is 80% …*. A notebook cell shows only `0.252 cm`. Likewise
  `print ∫ exp(-x^2) dx from -1e6 to 1e6` shows `0` with no *came out as exactly 0* warning. In the playground,
  `playground.run` returns `"warnings": []` for both: the run-time warnings go to the process's stderr (the
  browser console). Compile-time warnings do reach both tools (in Jupyter on stdout rather than stderr).
- **Tests:** `test_3_jupyter_shows_run_time_warnings`, `test_3_playground_shows_run_time_warnings`.

### 4. `d/dt x(2 s)` and `∂/∂x f(1, 2)` are silently 0 (wrong-answer). Status: fixed (D221): `d/dt x(2 s)` is `x'(2 s)` = 3 m/s and `∂/∂x f(1, 2)` = 12; a variable that isn't a parameter is an error; ∂ and ² print correctly
- **Repro:** `x(t) = 3 m * t / 1 s` / `print d/dt x(2 s)` prints `d/dt (x(2 s)) = 0`; `print x'(2 s)` prints
  `3 m/s`. With `f(x, y) = x^3 y^2`, `print ∂/∂x f(1, 2)` prints `d/dx (f(1, 2)) = 0` (and names the partial
  derivative "d/dx"). A beginner reading "the derivative of x at 2 s" gets a formula equal to 0 with no units and
  no warning. Differentiating an expression that doesn't depend on the variable (a function called at constant
  arguments) should be an error or a warning that suggests `x'(2 s)` / `(∂/∂x f)(1, 2)`.
- **Test:** `test_4_derivative_of_a_function_value_is_not_silently_zero`.

### 5. `[1, 2, 3] m` with your own `m` is silently a list of masses (wrong-answer, silent). Status: fixed (D222): standing alone it warns (the product reading of D192 stays), with other factors it is an error
- **Repro:** `m = 2 kg` / `xs = [1, 2, 3] m` / `print xs` prints `[2, 4, 6] kg` with no warning. The same program's
  `x0 = 1 m` warns *'1 m' is the unit m, not your variable m*, and `2 m v` is an error. D192 chose the product
  reading on purpose (as for matrices, D29), but it is the opposite of the D7 rule for a number, and silent. The
  unit check catches it only if the list later meets a length. `[[1, 0], [0, 1]] m` is the same.
- **Test:** `test_5_list_unit_colliding_with_a_variable_is_not_silent`.

### 6. Every error inside a `parallel for` points at column 1 (tool-mismatch, message). Status: fixed (D223): the caret and the LSP range are at the statement
- **Repro:** `xs = zeros(3)` / `parallel for i from 1 to 3` / `    print i`: the caret is under the indentation, not
  under `print`, and the language server underlines one character at column 1. The same for *total is shared by all
  the iterations*, *random numbers can't be drawn*, and *may only write its own element*
  (`checker.parallel_info.err` sets `col, length = 1, 1` for every statement in the body).
- **Test:** `test_6_parallel_for_errors_point_at_the_statement`.

### 7. Language-server ranges are code points, not UTF-16, so they are off after 𝑖 (tool-mismatch). Status: fixed (D223): the server converts code points to UTF-16 and back (diagnostics, hover, completion)
- **Repro:** over stdio, `print 𝑖, 1 m + 1 s`: the server announces `positionEncoding: utf-16` but publishes the
  error at characters 9–18. 𝑖 (U+1D456) is two UTF-16 units, so VS Code underlines `, 1 m + 1 ` instead of
  `1 m + 1 s`. Hover and completion positions coming in are also read as code points. Only astral characters are
  affected, and 𝑖 is the one Fermium teaches.
- **Test:** `test_7_lsp_ranges_are_utf16_after_the_imaginary_unit`.

### 8. No hover on a module member (tool-mismatch). Status: fixed (D223): hover on `module.member` shows the member's formula and units
- **Repro:** `import mechanics` / `T = mechanics.pendulum_period(1 m, 9.81 m/s^2)`: hovering over `pendulum_period`
  gives nothing (hovering over `mechanics` lists the module). The same for `nuc.semf_binding` after
  `import nuclear as nuc`. After `from astro import schwarzschild_radius` the hover shows the formula and units, so
  only the qualified form is missing.
- **Test:** `test_8_hover_on_module_member`.

### 9. The °C-in-a-product warning (D181) points at the start of the expression (message). Status: fixed (D223): the warning points at `10 degC`
- **Repro:** `Q = c_w * 1 kg * 10 degC`: the warning *10 °C is an absolute temperature …* has its caret (and the
  editor's underline) under `c_w`, not under `10 degC`. In a longer formula the reader has to hunt for the reading.
- **Test:** `test_9_celsius_product_warning_points_at_the_reading`.

### 10. `fermium check` says "no problems found" and then prints warnings, out of order (message). Status: fixed (D223): the warnings print in line order, then *units check out, N warnings (read them above)*. (The line-order test already passed on main.)
- **Repro:** `m = 2 kg` / `x = 3 m` → `w.fm: no problems found (units check out)`, followed by the warning *'3 m' is
  the unit m, not your variable m*. With a D181 warning on line 2 and a D173 warning on line 4, the line 4 warning
  prints first. The summary line should count the warnings (or come after them), and they should be in line order.
- **Tests:** `test_10_check_with_warnings_doesnt_say_no_problems`, `test_10_check_prints_warnings_in_line_order`.

### 11. The REPL's `:vars` shows internal type names (message). Status: fixed (D223): `:vars` says *a complex number of …*, *a 2-D vector of speed [m/s]*, *text*, and lists modules, functions with their parameters and `use python` modules
- **Repro:** after `z = 3 + 4i`, `v = <1, 2> m/s`, `M = [[1, 2], [3, 4]]`, `name = "a"`, `:vars` prints `z: cplx`,
  `v: vec`, `M: mat`, `name: str`: internal names, and no values or units (the docstring says "numbers show their value
  and unit"). Imported modules and `use python` names aren't listed.
- **Test:** `test_11_repl_vars_uses_physics_words`.

### 12. After an eigenvalue problem, `print ψ` suggests `ψ = 1.0 m` (message). Status: fixed (D223): *ψ isn't defined: the eigenvalue problem's states are ψ₁, ψ₂*, hint with the ASCII spelling
- **Repro:** `solve -hbar^2/(2 m_e) * psi'' = E psi with psi(0 nm) = 0, psi(1 nm) = 0 for x from 0 nm to 1 nm lowest 2`
  then `print psi` → *ψ isn't defined*, hint *give it a value first, e.g. ψ = 1.0 m*. The states are called `ψ₁ … ψ_N`
  (§20), and the message should say so.
- **Test:** `test_12_eigenstate_name_hint`.

### 13. The Jupyter kernel completes only `\name`, not names or module members (tool-mismatch). Status: fixed (D223): Tab completes names, keywords and module members (the language server's completion); the kernel docstring and reference §17 say so
- **Repro:** after `import mechanics`, Tab after `mechanics.spr` in a notebook gives no matches (`do_complete` returns
  `[]` unless the text after the last backslash is a symbol name). The language server completes the same position
  with `spring_period`, and the README lists "Jupyter kernel" among the tools without saying completion is symbols
  only.
- **Test:** `test_13_jupyter_completes_module_members`.

### 14. Bootcamp prose and transcripts disagree with the printed output (docs). Status: fixed: Lesson 0's transcript shows `2.30 m` and `1.61 km` (with a note on `to 6 digits`), the doctor sample has the pygls, ipykernel and C-compiler lines; Lesson 1 says `2.30 m`; Lesson 2b's transcript shows `0.500`
- **Lesson 0, Step 6:** the REPL transcript shows `fm> print 2 m + 30 cm` → `2.3 m` and `print 1 mi in km` →
  `1.60934 km`; the REPL prints `2.30 m` and `1.61 km` (3 significant figures by default). Step 5's `fermium doctor`
  sample also lacks the new *C compiler* line.
- **Lesson 1:** "`2 m + 30 cm` gave `2.3 m`" sits right under a box that says `2.30 m`.
- **Lesson 2b, Way 1:** the REPL session shows `print sin(\theta)` → `0.5`; it prints `0.500`.
  (The output boxes are regenerated by bootcamp/update_outputs.py; hand-written transcripts and prose are not.)
- **Tests:** `test_14_lesson0_repl_transcript_matches_the_repl`, `test_14_lesson1_and_2b_prose_match_the_output`.

### 15. Lesson 2 says `0.5 m v^2 where m = 2 kg` "means 0.5 metres (Fermium warns you)"; it is an error (docs). Status: fixed (already on main by the round-4 fix: Lesson 2 and the summary say it is an error); the test passes and is now a normal test
- **Repro:** `E = 0.5 m v^2 where m = 2 kg` stops with *'0.5 m' is ambiguous: … but m is also the m from 'where'*.
  The summary's "Gotchas: `0.5 m v^2` uses *metres*" is outdated the same way (D7 rule 5 made it an error).
- **Test:** `test_15_lesson2_where_gotcha_is_an_error_not_a_warning`.

### 16. The reference's Tools section omits most tools; the grammar lists `solve` twice; `fermium doctor` doesn't check pygls (docs). Status: fixed: §17 covers `fermium check`, the Jupyter kernel, `fermium lsp`, VS Code and the playground; §18 has one `solve`/`if`/`for` line; `fermium doctor` reports pygls and ipykernel
- docs/reference.md §17 *Tools* covers the REPL, fmt, doctor, build and VS Code ("syntax highlighting and `\name`
  completion"), but not the Jupyter kernel (`fermium jupyter install`), the language server (`fermium lsp`: hover,
  live errors), `fermium check` or the browser playground.
- §18 *Grammar summary* has the `solve …`, `if …` and `for …` lines twice (one copy with `absolute a, …` and one
  without, one `for` with `[parallel]` and one without), which reads as two different syntaxes.
- editors/vscode/README.md says to install pygls and "(check with `fermium doctor`)", but `fermium doctor` doesn't
  mention pygls (or ipykernel for the Jupyter kernel).
- **Tests:** `test_16_reference_tools_section_lists_the_tools`, `test_16_doctor_checks_what_the_editor_readme_says`.

### 17. `plot … to "a.png" title "sq"` is a parse error (message, parse). Status: fixed (D223): options may follow the file name without `with` (reference §11 says so)
- **Repro:** `plot ts^2 vs ts to "a.png" title "sq"` → *didn't expect 'title' here*. `plot ts^2 vs ts title "sq" to
  "a.png"`, `plot ts^2 vs ts, title "sq" to "a.png"` and `plot ts^2 vs ts to "a.png" with title "sq"` all work.
  §11 says `with` may be left out after the last series, and the file name is the natural thing to write right after
  the series.
- **Test:** `test_17_plot_option_after_file_name_without_with`.

### 18. `∂/∂x ∂/∂y f` is refused although the two steps work (message). Status: fixed (D221): `∂/∂x ∂/∂y f` works (h(1, 2) = 12)
- **Repro:** `f(x, y) = x^3 y^2` / `h = ∂/∂x ∂/∂y f` → *can't differentiate this derivative expression
  symbolically*. `fy = ∂/∂y f` / `h = ∂/∂x fy` gives `h(1, 2) = 12`. Mixed partials (∂²f/∂x∂y) are common in
  thermodynamics (Maxwell relations). If they stay unsupported, the message should say how to write them in two steps.
- **Test:** `test_18_mixed_partial_derivative`.

### 19. The REPL lets a variable change its units, and Lesson 2 doesn't say so (docs). Status: fixed: Lesson 2's *Variables keep their units* has a note that the REPL (and Jupyter) allow redefinition on purpose (D13)
- **Repro:** Lesson 1 says "You can try everything in the REPL". Lesson 2's box shows `v = 3 m/s` / `v = 5` as the
  error *v is speed [m/s]; it can't now hold a plain number*. In the REPL, the same two lines are accepted silently,
  and `print v` shows `5`. This is deliberate (D13: the REPL allows redefinition), but the lesson's "Variables keep
  their units" section should say so. Otherwise the beginner can't reproduce the box and concludes the check doesn't
  work.
- **Test:** `test_19_lesson2_says_the_repl_allows_redefinition`.

**Nits** (fixed ones have tests named `test_nit_…` in tests/test_redteam5.py):
- `fermium fmt --ascii` writes `2 + 1𝑖` as `2 + 1 1i` and `2𝑖` as `2 1i` (in other places `3(1i)`); it is correct but
  reads oddly (`2i` would do). *Fixed: `2𝑖` → `2i` after a plain number.*
- After `from mechanics import spring_period, nothere` fails, `spring_period` is imported anyway in the REPL. *Fixed by D220's rollback.*
- `hover` on a `table(…)` variable says only "data" (no columns or units); `fermium.selfhost` is not in pyproject's
  `packages`, so `python -m fermium.selfhost` is missing from a non-editable install. *Both fixed: the hover lists the columns and their units; pyproject packages `fermium.selfhost` and its .fm file.*
- `3𝑖 V` is *V isn't defined*: a unit can't follow an imaginary literal. The hint says what to write instead. *Won't fix: a unit after an imaginary literal would need a new literal rule, and the hint already gives `3𝑖 * 1 V`-style spelling.*

## Round 6 (10:00 UTC)

Reviewer: an independent subagent, on claude/lucid-gauss-9y1ov2 at 94c7a70. The focus was **silent wrong answers
only** (a plausible number, no error, no warning) in the changes of the last three hours (D190–D223), each checked
against NumPy/SciPy or a closed form. Each finding has an `xfail(strict=True)` test in `tests/test_redteam6.py` named
`test_<N>_…`. Delete the mark when the finding is fixed.

**What held up (no findings despite targeted probing):**
- **Matrices to 16×16 (D195):** a random 12×12 `det`, `solve_linear`, `inverse` entry, symmetric eigenvalues,
  generalised eigenvalues `eigenvalues(S, M)` (12 digits vs NumPy/SciPy `eigh`) and all 12 eigenvectors (columns,
  4×10⁻¹⁵ up to sign); the 6×6 Hilbert matrix's `det` (5.37×10⁻¹⁸) and eigenvalues; non-symmetric matrices filled at
  run time are refused, as are exactly singular ones; `M[i, j] =` with `cm`/`m` entries, `+=`/`-=` with km/m, a
  non-whole index, and value semantics through a function argument and `B = A`.
- **Fourth-order eigenfunctions (D190):** infinite well E₁, E₃, ψ₁, ψ₂, ψ₃′ and ψ₂″ at off-grid points to 10
  digits against √(2/L) sin(nπx/L); `V'(x)` and `d/dx V(x)` inside an eigen equation (x V′ added to an oscillator
  gives √3 ħω(n − ½) to 10 digits).
- **`f(xs, ys)`, `table(…)` fits (D191–D193):** pendulum g, a straight line, an exponential with t in ms (τ, N₀ and
  standard errors equal SciPy `curve_fit`); cm/mm columns; °C columns with `T - T0`; multi-line and integral-valued
  functions over two lists; number arguments shared; slices.
- **Nested helpers (D194):** late binding of outer variables, shadowed parameter names, helpers calling helpers,
  `∫ q(x) dx` inside h(x) using h's x, and loops that update a captured variable.
- **`d/dt f(point)` (D221):** `d/dx f(2)`, `f(a + 1)`, `∂/∂x g(1, 2)`, `∂/∂x g(1, x)` with x defined (the formula
  reading, 75), `d/dt x(2 s)` and `d²/dt² x(2 s)` of an ODE solution, products and sums after it.
- **D210:** `x'(t)` and `d/dt x(t)` of an earlier ODE solution and `f'(t)` of a function inside a new solve (e−1,
  e·1, 2e⁻¹ exact); `-V'(y)` and `-d/dq W(z)` against SciPy `solve_ivp`.
- **Units after brackets and lists (D192, D215, D222):** SEMF-style `(…) MeV`, `2 (N - Z) MeV`, `(3 + 1) keV / 2`,
  `(4) m / (2) s`, `100 (2 + 3) cm`, `[1, 2] m/s`, `[2, 4] cm^-1`, `[3, 4] / 2 s`.
- **`absolute` in °C/°F (D200):** Newton's cooling to 6 digits with `absolute 1e-6 °C` and `°F` (rk45 and radau).
- **`str()` and `+` on texts (D216):** units, significant figures, `±`, `in cm`, loops building text.
- **README and SHOWCASE numbers,** recomputed independently: 9.70 m/s², 31.8 ft/s², 10 rad/s, 1 J, x(5 s) =
  3.520064 cm (SciPy), −∇φ(0, 3, 4 m) = ⟨0, 0.216, 0.288⟩ V/m, g = 9.801 ± 0.053, range 40.2 m (Monte Carlo mean
  E[v²] e^(−2σθ²)/g = 40.20 m), finite-well levels 0.2718/1.077/2.379 eV (transcendental equations) and the ground
  state's 0.0083 outside (closed form 0.008272). (README's comment `# 3.52006 cm` doesn't match the printed `3.52 cm`;
  cosmetic.)

### 1. D197 prints real entries of a computed matrix as 0: the Minkowski metric, and an inverse (silent). Status: fixed (main session): D197 noise-zeroing disabled (D230)
- **Repro:** `one = 1 m²/s²`, `z = 0 m²/s²`, `g = [[-c^2, z, z, z], [z, one, z, z], [z, z, one, z], [z, z, z, one]]`
  → `print g` is `[[-8.99×10¹⁶, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]] m²/s²` (a rank-1 matrix);
  `print g / c^2` is `[[-1, 0, 0, 0], [0, 0, 0, 0], …]`; `print inverse(g / c^2)` is `[[0, 0, 0, 0], [0, 8.99×10¹⁶,
  0, 0], …]` (the −1 is gone). Also `print inverse([[1, 0], [0, 1e-15]])` → `[[0, 0], [0, 1.00×10¹⁵]]`, and
  `print 2 [[1e15, 1], [1, 2]]` → `[[2.00×10¹⁵, 0], [0, 0]]`.
- **Reference:** NumPy: diag(−c², 1, 1, 1); inverse diag(−1, c², c², c²); [[1, 0], [0, 10¹⁵]]; [[2×10¹⁵, 2], [2, 4]].
  These entries are exact, not rounding noise: the 10⁻¹⁴-of-the-largest rule can't tell a genuinely small entry from
  noise when a matrix mixes scales (c² next to 1 is the textbook metric in SI).
- **Tests:** `test_1_metric_with_c_squared_keeps_its_unit_diagonal`, `test_1_inverse_of_a_diagonal_matrix_keeps_the_small_entry`.

### 2. D197 zeroes an entry written in the program, contrary to its own rule (silent). Status: fixed (main session): D197 noise-zeroing disabled (D230)
- **Repro:** `p = <1 AU, 1 mm, 0 m>` / `print p` → `<1, 0, 0> AU`; `print p[2]` → `6.68×10⁻¹⁵ AU`; `print 1 p` and
  `print <1 AU, 0 m, 0 m> + <0 m, 1 mm, 0 m>` the same. JIT and `--interp` agree.
- **Reference:** `<1, 6.68×10⁻¹⁵, 0> AU`. D197 and reference §Printing say "entries written in the program … are
  never changed"; here a written 1 mm prints as 0.
- **Test:** `test_2_written_vector_entry_is_not_printed_as_zero`.

### 3. The integral noise snap zeroes integrals that are resolvable (silent). Status: fixed (main session): the integral noise snap removed (D230)
- **Repro:** `print ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1` → `0`; `print ∫ sin(x) + 5e-15 dx from -1 to 1` → `0`
  (also `+ 1e-15` and `∫ x exp(-x^2) + 1e-16 dx from -5 to 5`). `interp.py:905` (and the LLVM mirror) replace any
  result |r| ≤ 50 ε ∫|f| (1.1×10⁻¹⁴ ∫|f|) by 0.
- **Reference:** exact 8×10⁻⁹ and 1.0×10⁻¹⁴; SciPy `quad` returns 7.99×10⁻⁹ and 9.99×10⁻¹⁵ (0.1 % off). The
  threshold is ~50× the rounding error these integrals actually have, so a small net charge or dipole on top of a
  large antisymmetric part becomes an exact 0 with no warning. (Printing it with a "compatible with 0" note, or a
  threshold near the actual error estimate, would keep the D44 intent.)
- **Tests:** `test_3_small_but_resolvable_integral_is_not_snapped_to_zero`, `test_3_constant_offset_on_an_odd_integrand`.

### 4. PDE values inside D206's skipped window are wrong by up to 19 %, with no warning (silent). Status: fixed (main session): the PDE start-up skip removed (D230)
- **Repro:** `D = 1e-4 m²/s`; `solve ∂u/∂t = D * ∂²u/∂x² with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(1 m, t) = 0 K
  for x from 0 m to 1 m, t from 0 s to 100 s`; `print u(2.5 mm, 0.05 s), u(5 mm, 0.2 s)` → `40.930 K 35.023 K`.
- **Reference:** 80 K erfc(x / 2√(Dt)) = 34.336 K and 34.336 K (semi-infinite rod; the far end is 1 m away). The
  same program with `t from 0 s to 1 s` gives 34.214 K and 34.253 K, so the error is the time step (1000 steps of
  0.1 s), not the grid. D206 skips the step-doubling check before t0 + 10 h²/D (0.625 s here with h = 2.5 mm), and
  every value asked for inside that window is unchecked: these are far outside the promised 0.1 % of the range. At least a warning when
  u is evaluated at a time inside (or near) the unchecked window would make it visible.
- **Test:** `test_4_heat_step_early_times_are_accurate_or_warned`.

### 5. D190's inverse iteration mixes a near-degenerate pair: a symmetric double well's ground state has no parity (silent). Status: fixed (D233)
- **Repro:** `V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8`; `solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ with ψ(-3 nm) = 0,
  ψ(3 nm) = 0 for x from -3 nm to 3 nm lowest 2` (m = m_e) → `ψ₁(-1 nm) = 4.24×10⁴`, `ψ₁(1 nm) = 4.18×10⁴ 1/m^(1/2)`,
  ⟨x⟩₁ = ∫ x ψ₁² dx = −1.32×10⁻¹¹ m, ⟨x⟩₂ = +1.32×10⁻¹¹ m (the tunnelling splitting is 5×10⁻¹¹ eV). With
  `lowest 2 using shooting`: 4.21×10⁴ at both points, ⟨x⟩ ≈ 10⁻¹⁴ m. With a barrier half as high (`* 4`), the matrix
  method is symmetric too.
- **Reference:** a non-degenerate 1-D bound state of an even potential has definite parity: ψ₁ even, ⟨x⟩ = 0; an
  independent parity-restricted finite-difference solve gives |ψ₁(±1 nm)| = 4.210×10⁴ m^(−1/2). The Numerov
  refinement is shifted by the FD eigenvalue, which is within the splitting of the other state, so it converges to a
  mixture; ammonia-inversion-style problems get a 1.4 % asymmetric ground state and a spurious dipole.
- **Test:** `test_5_double_well_ground_state_is_symmetric`.
- **Fix:** two levels closer than 10⁻⁸ of the largest |E| are nearly degenerate. When the equation is symmetric about the middle of the range at every grid point, the pair is replaced by its even and odd combinations (fewer nodes first): ψ₁(−1 nm)/ψ₁(1 nm) = 1.00000 and ⟨x⟩₁ = 0 by both methods, ψ₂ odd. Otherwise (e.g. the same well on −3 nm … 3.5 nm) the run warns *levels 1 and 2 are nearly degenerate (ΔE/E = 3.8×10⁻¹¹); their eigenfunctions ψ₁, ψ₂ can be any mixture …* (JIT and interpreter). The original test passes unchanged; new tests cover the odd state, shooting, and the warning.

### 6. `d/ds h(2 s)` is h′ at 2 seconds, not the derivative in s (silent). Status: fixed (D231)
- **Repro:** `h(s) = s^2` / `print d/ds h(2 s)` → `4 s`, no warning. `print d/ds h(2*s)` → `d/ds (h(2·s)) = 2 h'(2·s)`,
  and `d/dt f(3 t)` gives the formula too. `k(m) = m^2` / `d/dm k(2 m)` → `4 m`. (`u = 1` first, `d/du f(2 u)` does
  warn.)
- **Reference:** on paper d/ds h(2s) = 2 h′(2s) = 8s. D221 reads `2 s` as a constant (2 seconds) because the variable of
  `d/ds` doesn't count as "your variable" for D7, unlike D211's rule for the unknowns of a solve and function
  parameters (`f(s) = 3 s^2` warns).
- **Test:** `test_6_derivative_variable_named_like_a_unit_is_the_variable`.
- **Fix:** the variable of `d/dv`/`∂/∂v` is your variable in its operand, and `2 s` right after a number there is an error: *'2 s' is ambiguous: s is the variable you differentiate by, but right after a number s is the unit seconds* (hint: `2*s` or `2 [s]`); `d/dm k(2 m)` and `∂/∂s g(1, 2 s)` too. The test now also accepts that error (it only allowed a warning; the error is stronger: no number is printed).

### 7. `∫ 3 s^2 ds` integrates 3 square seconds, with no warning (silent; older than D190). Status: fixed (D232)
- **Repro:** `print ∫ 3 s^2 ds from 0 to 1` → `3 s²`; `print ∫ 2 m dm from 0 to 1` → `2 m`. No warning, while the
  same `3 s^2` as a function parameter (`f(s) = 3 s^2`) and `Σ(2 g for g from 1 to 3)` warn (D7).
- **Reference:** ∫₀¹ 3s² ds = 1 and ∫₀¹ 2m dm = 1. The integration variable should count as your variable for the D7
  rules (the unit reading with a warning, as elsewhere), the same fix as #6.
- **Test:** `test_7_integration_variable_named_like_a_unit_warns_or_is_the_variable`.
- **Fix:** the integrand is checked again after its `ds` is split off (the unit reader had taken `s^2 ds` as one compound unit): `∫ 3 s^2 ds` and `∫ 2 m dm` warn *'3 s^2' is the unit s^2, not your variable s*, as `f(s) = 3 s^2` does; in a product it is D7's error. ODEs had the same hole (`solve y' = 3 s^2 … for s from 0 to 1`): a solve's independent variables now count as your variables in its equations. `Σ` already warned.

**Not findings (noted):**
- A kinked potential (`V = F |x − 0.0123 nm|`, Airy levels) gives E₁ to 1.8×10⁻⁶ at the default grid (0.3428147156 vs
  0.3428153442 eV; `grid 8000` 5×10⁻⁹, shooting 7×10⁻⁷): the second-order finite-difference value is kept, as D190
  says when the extrapolation ratio isn't steady; reference §eigen could list kinks next to jumps (~10⁻⁶).
- `(2 + 3) h` is 5 × Planck's constant with no warning, while `2 h` warns (for hours write 2 hr); the unit `J s` shows.
- `print d/dt h(0.5)^2` prints `d/dt (h(0.5)²) = 0` (the formula reading, shown).
- `table(T = Ts)` with Ts in °C reports the column as `T [K]` and `print d.T` in K (D193 says a column keeps its
  list's display unit); the values are right.

## Round 7 (11:15 UTC)

Reviewer: an independent subagent, on claude/lucid-gauss-9y1ov2 at 6480b72 (after the round 6 fixes D230–D233).
Focus: silent wrong answers introduced by the latest fixes, the D11 printing rules on physics outputs, and a final
pass over the 11 research programs. Each finding has an `xfail(strict=True)` test in `tests/test_redteam7.py` named
`test_<N>_…`; delete the mark when the finding is fixed. JIT and `--interp` agree on every repro below.

**What held up (no findings despite targeted probing):**
- **D231/D232 (derivative, integration and ODE variables):** `∫ 9.81 m/s² dt from 0 s to 2 s` = 19.6 m/s,
  `∫ 3 m/s ds from 0 s to 2 s` = 6 m, `∫ 3 N/m * s ds from 0 m to 2 m` = 6 J, `∫ 3 m/s^2 s ds from 0 s to 4 s` = 24 m
  (variable reading, warned), `∫ n R T / V dV from 1 L to 2 L` = nRT ln 2, `∫ Q/(4π ε₀ r²) dr` to ∞;
  `solve x'' = -9.81 m/s² … for t` (−19.6 m at 2 s), `solve x' = 5 m/s … for s from 0 s to 2 s` (10 m),
  `solve y'' = -4 1/s^2 * y … for s` (cos 2), `u' = 1 m/s + 2 m/s^2 * s … for s` (6 m); `x(0 s)` in the conditions of
  a solve over s is fine; nothing leaks after the solve/derivative/integral (`print 3 s` afterwards is 3 s).
  `d/ds (3 m/s * s)` = 3 m/s; `d/dt (5 t^2)` = 10t; `d/ds (3 m/s s^2)` warns and gives 6 m/s s.
- **D233 closed forms:** infinite well E₁ and E₃/E₁ = 9 to 9 digits, ψ₁(L/2) = √(2/L), ⟨ψ₁|ψ₃⟩ = 6×10⁻¹²; a tilted,
  shifted oscillator (E = ħω/2 − F²/(2mω²) + F x₀ to 9 digits, ⟨x⟩ = 0.20475 nm exact); a step well (0.1923223 and
  0.5617812 eV, exact matching condition agrees to 7 digits); the symmetric double well by both methods (parity,
  norm 1, first lobe positive, ⟨ψ₂|ψ₃⟩ = 5×10⁻¹⁸); a symmetric quadruple well (splittings 10⁻⁹ relative: all four
  states match the SciPy tight-binding pattern 0.66/1.6/1.6/0.66, …).
- **D230:** vectors/matrices show honest noise (`M * inverse(M)`, `<1 AU, 1 mm, 0 m>`), the wave equation matches
  d'Alembert to 5 digits at 0.01 s and later, `∫ exp(-x²) cos(10x) dx` over ℝ = 2.46×10⁻¹¹ (exact √π e⁻²⁵).
- **D11:** 9.9996 × 1.00 → 10.0, 99999.7 × 1.00 → 1.00×10⁵, 2/3 × 10⁶ → 6.67×10⁵, lists `[0.667, 66.7, 6670]`,
  negative values, `1 AU in m`, `c`, exact literals (`299792458 m/s`, `6.02214076×10²³`).
- **Research:** 10 of 11 programs print their README headline numbers (BBN Y_p 0.242340, D/H 2.59588×10⁻⁵; t₀
  13.791 Gyr, z_eq 3419; hydrogen 9.1×10⁻⁹, Lyman α 121.5684 nm; Lane–Emden ξ₁ and M_Ch 5.825; TOV 0.7102 M☉ at
  9.161 km; pp/CNO 17.79 and 18.06 MK; recombination z_* 1089.61; SEMF a_V 15.414, rms 3.31 MeV, 13.3 MeV at
  Z = 50, N = 82; shell model gaps at 2, 8, 20, 28, 50, 82, 126; U-238 Rn-222 99 % at 25.40 d). Rutherford: see #3.

### 1. D233 forces parity on a slightly asymmetric double well: the ground state is put in both wells (silent). Status: fixed (main session): `_symmetric` compares mirrored grid values point by point, relative to their own size (a 3e-9 eV tilt at a well's bottom is an asymmetry)
- **Repro:** `m = m_e`, `V(x) = 2 eV * ((x / 1 nm)^2 - 1)^2 * 8 + 3e-9 eV * x / 1 nm` (a 3 V/m field on the round 6
  double well), `solve -ħ²/(2*m) * ψ'' + V(x) ψ = E ψ with ψ(-3 nm) = 0, ψ(3 nm) = 0 for x from -3 nm to 3 nm
  lowest 2` → `ψ₁(-1 nm)/ψ₁(1 nm)` = 1.00000, `ψ₂(-1 nm)/ψ₂(1 nm)` = −1.00000, ⟨x⟩₁ = 1.6×10⁻¹⁶ nm, no warning.
  E₂ − E₁ = 5.88×10⁻⁹ eV (right).
- **Reference:** the wells differ by δ ≈ 6×10⁻⁹ eV, 100× the tunnelling splitting Δ ≈ 5×10⁻¹¹ eV, so the states are
  localised: SciPy `eigh_tridiagonal` on the same equation (6000 intervals) gives ψ₁(−1 nm)/ψ₁(1 nm) = 199,
  ⟨x⟩₁ = −0.981 nm, ψ₂(−1 nm)/ψ₂(1 nm) = −0.0050, E₂ − E₁ = 5.87×10⁻⁹ eV. `_symmetric` accepts a relative
  asymmetry of 10⁻¹⁰ of the **largest** coefficient, which the walls make ~1000 eV (tolerance ~10⁻⁷ eV, more than the
  whole tilt), while the pair counts as degenerate at 10⁻⁸ E ≈ 1.5×10⁻⁸ eV. So any asymmetry between Δ and ~10⁻⁸ E is
  silently symmetrised away: the Stark effect of an ammonia-like inversion doublet comes out with no dipole.
- **Test:** `test_1_tilted_double_well_ground_state_is_localised`.

### 2. The heat equation after a jump: very early times are still 60 % wrong with no warning (silent). Status: open (documented limitation, reference §19): before √(Dt) reaches one grid cell the PDE value is an interpolation; refine `grid` for very early times
- **Repro:** `D = 1e-4 m²/s`, `solve ∂u/∂t = D * ∂²u/∂x² with u(x, 0 s) = 0 K, u(0 m, t) = 80 K, u(1 m, t) = 0 K
  for x from 0 m to 1 m, t from 0 s to 100 s` → `u(0.5 mm, 0.002 s)` = 55.0 K, `u(0.25 mm, 0.0025 s)` = 67.0 K,
  `u(0.5 mm, 0.01 s)` = 59.1 K, `u(1 mm, 0.01 s)` = 42.2 K; no warning.
- **Reference:** 80 K erfc(x / (2√(D t))) = 34.34 K, 57.89 K, 57.89 K, 38.36 K. D230 set the D206 skip to 0, which
  fixed the 0.05 s point of round 6 #4 (34.24 vs 34.34 K), but before the diffusion length √(Dt) reaches a grid cell
  (h = 2.5 mm) the value is an interpolation between the boundary and the first node. The reference says the step is
  refined until the error is under 0.1 % of the range; this is 26 % of the range, and the spatial error isn't
  estimated or warned about. (A smooth 1 cm Gaussian is 0.25 % off at 0.1–1 s, printed to 5 digits: same cause.)
- **Test:** `test_2_heat_step_very_early_time_is_accurate_or_warned`.

### 3. research/rutherford_mc/README.md quotes numbers the program no longer prints (docs). Status: fixed (main session): README re-measured with the seeded generator (χ² 38.2, backward fraction 0.001913)
- **Repro:** `cd research/rutherford_mc && fermium run rutherford.fm` → χ² = 38.2 (README and research/README.md:
  35.6), backward fraction 0.00191255 (README 0.001920, "1.4σ"), 150° row 0.980 with 936 α (README 1.04, 997 α),
  and every table row differs. The run is reproducible (38.2 twice).
- **Reference:** the README says "`rand()` has no seed function … the compiled program calls drand48 without seeding
  it … every `fermium run` happens to give the same numbers (the ones below)"; since M3 (commit c32a62f) `rand()` is
  xoshiro256** with `seed(n)` and an implicit `seed(0)` (reference §20), so the numbers and the "no seed" weakness
  listed under Limitations are stale. The physics conclusion (χ² ≈ dof) still holds.
- **Test:** `test_3_rutherford_readme_matches_the_program`.

### 4. D11 applies "fewest significant figures" to +: `293.15 K + 0.5 K` prints `290 K` (silent). Status: fixed (main session): a sum or difference keeps the significant figures of its most precise operand (293.15 K + 0.5 K = 293.65 K; D11)
- **Repro:** `T = 293.15 K` / `print T + 0.5 K` → `290 K`; `m_p = 938.272 MeV` / `print m_p + 2.2 MeV` → `940 MeV`;
  `L0 = 2.000 m` / `print L0 + 0.5 mm` → `2.0 m` (and `L0 + α L0 ΔT` for thermal expansion prints `2.0 m`).
- **Reference:** 293.65 K, 940.472 MeV, 2.0005 m; by the decimal-place rule for sums, 293.7 K, 940.5 MeV, 2.000 m.
  A 1-figure correction reduces a 5-figure value to 2 figures, and the printed 290 K is 3.65 K from the value.
  Round 1 logged the opposite direction (`1.00 m - 0.999 m` over-claims) as not changed (D95) because decimal places
  need the magnitude at run time; this direction is a visibly wrong number on ordinary physics (a temperature plus a
  small change, a mass plus a binding energy). A compile-time fix is possible: for `+`/`−`, use the sig figs of the
  operand with the most (or skip the rule and print 3 figures), which never prints a value outside its precision.
- **Test:** `test_4_adding_a_small_correction_keeps_the_precise_value`.

### 5. An integral dominated by rounding error prints 3 figures of which 1 is right, with no warning (silent). Status: open (documented limitation, reference §19): a result at the integrand's rounding level shows more figures than are right; no warning yet
- **Repro:** `print ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1` → `7.93×10⁻⁹` (the D230 example; the snap is gone).
- **Reference:** exactly 8×10⁻⁹ (the sine part cancels). SciPy `quad` gives 7.96×10⁻⁹ with an error estimate of
  1.0×10⁻⁸ and an IntegrationWarning about round-off. The result is at the integrand's rounding level (10⁶ × ε × 2 ≈
  4×10⁻¹⁰ per node), so either the printed digits should be limited to the error estimate or a warning should say the
  value is rounding-limited. Round 6's test only asks for 10⁻¹⁰ absolute, which 7.93 passes.
- **Test:** `test_5_rounding_dominated_integral_is_right_to_its_printed_digits_or_warned`.

**Not findings (noted):**
- `∫ ψ₁(x) (-ħ²/(2m) ψ₁''(x) + V(x) ψ₁(x)) dx` (⟨H⟩ of the quadruple-well ground state) stops with "couldn't compute
  this integral numerically … the estimate was 6.26812×10⁻¹⁹ ± 1.6×10⁻²⁶" — an error on a converged value (relative
  error 3×10⁻⁸), not a silent answer.
- `print |[3.0 m/s, 4.0 m/s]|` prints `[3.0, 4.0] m/s`: |…| of a list is element-wise, as `abs` is; a vector
  (`<3, 4> m/s`) gives 5 m/s. A physicist may write the list form expecting the length.
- `recombination.fm` prints the warning "h (Planck's constant) is now your variable" on every run (it means h = H₀/100).

## Round 8 (run 2, 2026-09-26 01:45 UTC): the v1.5 rules

Independent reviewer on branch claude/v1.5 at 0c9dcdc: D235 (the unit rule), D236 (fraction coefficients), D237
(messages), D250–D255 (plots, research inputs), Lessons 1/2/2b and the cheat sheet. 25 findings; fixes in D238,
regression tests in tests/test_redteam8.py.

| # | Finding | Severity | Status |
|---|---|---|---|
| 1 | `1 / 0.5 s`, `2π / 0.5 s`, `0.693 / 5730 yr` became times (D236) | silent wrong | fixed: coefficient only for whole-number fractions |
| 2 | `1/2 [kg]` differed from `1/2 kg` | silent wrong | fixed |
| 3 | `fmt --fix` wrote `1/2 [m] v²`, `2 [m] c²` (Fermium 1 had stopped there) | silent wrong | fixed: left and reported |
| 4 | `3 m / 2 / s` became 1.5 m·s | silent wrong | fixed |
| 5 | `10⁻² m` bypassed the rule | silent wrong | fixed: asks |
| 6 | `<1, 0> m` bypassed the rule | silent wrong | fixed: asks |
| 7 | a where-binding didn't see earlier bindings | silent wrong | fixed |
| 8 | `*` bypassed sentence 3 | silent wrong | fixed: `*` never continues a unit |
| 9 | `(m1 + m2) g` was grams when no g is defined | silent wrong | fixed: brackets needed after a bracket |
| 10 | D236 depended on how the denominator is written; `1/2len √(F/μ)` | silent wrong | partly: π powers consistent; `1/2L` documented (write `1/(2L)`) |
| 11 | `0.5 /s` divides by your own s | by design | documented in D238 |
| 12 | `J/kg K` is J·K/kg | silent wrong (unchanged since v1) | warns now |
| 13 | `fmt --fix` gave `2 [kg] m/s` | misleading | fixed: whole unit |
| 14 | geometrized units got the natural-units hint | misleading | fixed |
| 15 | integral hint suggested `3 [m/s ds]` | misleading | fixed |
| 16 | conversion hints named odd products | misleading | fixed: named kinds first, none for plain numbers |
| 17 | `km/h` with your own h spoke of Planck's constant | misleading | fixed |
| 18 | `180/π deg` message | misleading | documented (write `(180/π) [deg]`) |
| 19 | D237 hint claims; `tmep` lost its suggestion | misleading | fixed (and D237 text corrected) |
| 20 | `√(-4 m²)` not caught | misleading | fixed |
| 21 | hint dropped `²`; `[1, 2] m/s` with a mass; `N×m`; "c is speed" | nit | fixed (`×` ends a unit like `*`) |
| 22 | cheat sheet solve with `x(0) = 0.1 m` next to `m` | doc | fixed |
| 23 | BBN README ⁷Li sentence and old `2/π² T⁴` note | doc | fixed |
| 24 | ²⁰⁸Pb table −8.05 vs printed −8.04 | doc | fixed |
| 25 | Lesson 2 overstated the rule | doc | fixed by #5, #6, #9 |

## Round 9 (run 2, 2026-09-26 04:30 UTC): Phase B groundwork (conformance suite + early Rust)

Independent reviewer on branch claude/v2-rust at 2939b2e: `conformance/run`, `conformance/harvest.py`,
`conformance/MANIFEST.md`, the `FERMIUM_HARVEST` hook in `tests/conftest.py`, and the Rust crates `fermium-ir`
(types.rs, the Unifier), `fermium-codegen/src/eval.rs`, `fermium-units` (numfmt) and `fermium-check` (arith.rs).
`cargo test --workspace` passes (13 tests, plus the 51 247-check units parity fixture). The Rust CLI stub scores
0/3037, which is honest. The scoreboard itself is the problem: a deliberately wrong implementation scores 99.9 %.
Nothing here is fixed yet; the status of each finding is "open".

### 1. A wrong implementation scores 3034/3037 (99.9 %) (high). Status: open
- **Repro:** a fake `--bin` script (`mutant`, 30 lines of Python) reads the golden `<id>.json` next to the program
  and prints it back with every number changed: each integer +1, the last decimal digit of each decimal +1,
  `1.5×10⁻⁹` rewritten as `1.5e-9`. It prints every warning without its caret and hint, and every error message
  with its words in reverse order. `conformance/run --impl rust --bin ./mutant --out /tmp/M.md` gives
  **3034 of 3037 (99.9 %)**, 100 % in 20 of the 22 areas. The only 3 failures are subnormal numbers
  (`9.35762×10⁻³¹⁴`), where `10.0 ** -324` underflows the tolerance to 0. A second mutant adds a digit to every
  number (`3` → `3.4`, `2.21` → `2.214`) and scores 3035/3037.
- **Why it passes:** `same_token` accepts a difference of one unit in the expected token's last digit, so for an
  integer (`4`, `100`, a count, a loop index, a list length) any value ±1 passes. 2924 of the 24 820 stdout tokens
  are such integers. The got token's precision and notation are never compared, so `3.1` matches `3.14159`,
  `1.00` matches `1`, and `1500` matches `1.5×10³`. Significant figures (D11) and the `×10ⁿ` style are core
  language behaviour, and none of it is scored. `parse_num` strips `[](),<>;:`, so `[1, 2, 3] m` matches
  `1 2 3 m` and `<3, 4>` matches `(3; 4)`. `similar()` is a 60 % Jaccard over sets of words, so word order and
  negation are ignored: "expected a length but got a time" matches "expected a time but got a length", and
  "x is not a length" matches "x is a length". Whitespace is ignored (`split()`), so table alignment is not scored.
- **Fix:** compare text exactly by default. Allow a numeric tolerance only where the numerics are expected to
  differ (quadrature, ODE, PDE, eigen, fit, fft, rng), and only when the got token has the same digit count and
  notation as the expected one. Compare integers exactly. Keep the brackets. Compare error messages exactly, or by
  an ordered key phrase stored per case. Add `conformance/selftest` with these mutants and require them to score
  below a few percent, and report "exact" and "within tolerance" counts separately in CONFORMANCE.md.

### 2. Diagnostics: column, caret, hint, kind and exit code are never compared (high). Status: open
- **Repro:** the mutant in #1 drops every caret and hint and still passes. The 773 expected errors include 414
  hints, and `col`, `hint` and `kind` are stored in each case's JSON but `judge()` never reads them. `case["exit"]`
  is never read either: any non-zero exit counts as "an error", so a Rust panic (exit 101) is as good as exit 1.
  Also, when the implementation's error has no `line N:`, `judge()` skips the line check: the probe
  `judge(case, out, "", {"message": <expected message>, "line": None})` passes for an error expected on line 3.
  `run_binary` takes `msg[0]` (the first matching line), although its comment says "the last".
- **Why it matters:** spec §B3 requires "expected diagnostics (kind, line and column, key content), an exit code",
  and "one-line errors with a caret and a hint" is on the keep-100 % list (spec §1).
- **Fix:** compare the exit code, the line and the column (fail when the implementation gives no line), the kind,
  and the caret line. Compare the hint by key phrase. Parse the whole error block, not only the `file, line N:`
  line.

### 3. The golden output sits next to the program, and nothing stops the binary from reading it or calling Python (medium). Status: open
- **Repro:** the mutant in #1 opens `path[:-3] + ".json"`. A "Rust" binary that runs
  `python3 -m fermium run` would also score 100 %, although spec §B1 requires zero runtime dependencies.
- **Fix:** copy each `.fm` into a fresh temporary directory, with only its data files, before running it. Run the
  binary with a minimal environment (`env -i PATH=/usr/bin:/bin`, no `PYTHONPATH`), and in CI on a runner image
  without python3 on PATH, or under a seccomp or `unshare` sandbox that denies `execve`.

### 4. Harvest gaps against spec §B3: notes/, FRICTION.md, REDTEAM.md repros, fmt, REPL, CLI and rejection-only checks (medium). Status: open
- **What is harvested:** only programs that go through `run_source` while pytest runs (3030 cases), plus the
  benchmarks (7). The glob list (examples, rosetta, gauntlet, research, tests/programs, john, benchmarks) adds
  nothing new, because tests already run those files and dedup keeps the tests copy. Bootcamp, docs and README
  ```fermium blocks *are* covered (all 236 are in the suite, via test_docs).
- **Missing sources** (fenced blocks counted with a fence parser; "covered" means every non-comment line of the
  block appears in a single case):
  - `dev-notes/notes/*.md` (5 files of bug repros, e.g. `xs = [1, 2, 3] / ys = xs / push(ys, 4)`): 123 fenced
    blocks, 32 covered. About 40 more appear in tests/*.py with edited comments, and the remaining ~50 are complete
    programs with no guaranteed case.
  - `gauntlet/**/FRICTION.md` (11 files): 30 fenced blocks, 7 covered. About 500 inline code spans, 197 of them
    found verbatim in the suite.
  - `dev-notes/REDTEAM.md`: 6 fenced blocks (5 covered). 85 `**Repro:**` lines hold 130 code-like inline spans, and
    ~23 of those are not in the suite verbatim (e.g. `f = 50 Hz; print f in rev/min`,
    `f(a) = ∫ exp(-(x - a)²) dx from -1e6 to 1e6`, `print [0.125, 0.375] * 1.0`).
  - rejection tests that only run the checker: `warnings_of()` (68 calls) and `check_only()` (2) in conftest call
    `Checker` directly, so the hook never sees them.
  - `fmt`: `format_source` has 45 calls in 12 test files, and the suite has no fmt case. B5 milestone 1 ("Syntax,
    including fmt round-trips") therefore has no conformance group.
  - REPL/Jupyter (`ReplSession`, 12 uses), the CLI, `fermium build` and `fermium check` (64 subprocess calls in
    39 test files). Milestones 9 and 10 have no conformance group.
  - programs where the oracle itself raises a non-FermiumError: the hook doesn't record them, and `rerun_clean`
    drops them silently (MANIFEST's "crashed in legacy: 0" counts only the globbed files).
- **Fix:** harvest every ```fermium block, and every untagged block that parses, from notes/, FRICTION.md and
  REDTEAM.md, plus the `Repro:` spans joined on ` / `. Add case kinds `fmt` (input → `--pretty`/`--ascii` output
  and the round-trip), `check` (warnings and errors without running), `repl` (a transcript) and `cli`
  (argv → stdout, stderr, exit code). Record the checker-only paths in the hook, and count legacy crashes in
  MANIFEST.

### 5. The "machine-dependent" and "temporary directory" exclusions hide real behaviour (medium). Status: open
- **Repro:** `NONDETERMINISTIC = \bclock\(\)|\bTIME\b|\btime:\s`, matched anywhere in the source, comments included:
  - `gauntlet/quantum/32_rabi_ramsey.fm` is excluded because a *comment* says "to the same time: the
    counter-rotating terms". This is a false positive; the program is deterministic.
  - `research/shell_model_magic_numbers/shell.fm` (a whole research reproduction) is excluded for its one
    `print "time:", clock() - t_start` line.
  - Neither is in `conformance/cases`. The benchmarks already show the right mechanism: `drop_prefixes`.
  - "temporary directory with files: 141": every test program that `load`s a CSV, `import`s a module or `plot`s
    into `tmp_path` is dropped. That removes most of the data and modules features from the suite, and a Rust
    `load`/`import` is scored on only 28 `load "` cases.
- Separately, the 53 successful plot cases compare stdout only (usually empty), and no PNG or SVG is checked. A
  `plot` that writes nothing passes (B5 milestone 7). Running the suite also writes PNGs into the repo directories
  (e.g. `tests/programs/x_vs_t.png`).
- **Fix:** match NONDETERMINISTIC on code with comments stripped, and use `drop_prefixes: ["time:"]` for shell.fm.
  In the hook, snapshot the files in `base_dir` into `conformance/fixtures/<id>/` and run the case there. For
  plots, check that the file exists and is a valid PNG/SVG of the right size, and compare the axis labels and
  units in the SVG text.

### 6. eval.rs ports interp.py, but the oracle that scores it is the compiled path, and the two disagree (medium). Status: open
- **Repro (Python, both 1.5 back ends):** `x = 0.49999999999999994` / `print round(x)` gives `0` from
  `run_source` (LLVM `llvm.round`) and `1` from `run_interpreted`. `x = -10` / `print x^309` gives `-∞` compiled
  and `∞` interpreted. The Rust probe (a scratch crate calling the pub helpers) gives
  `math1("round", 0.49999999999999994) = 1` and `math1("round", 4503599627370497.0) = 4503599627370498`: the
  interp.py formula `floor(|x| + 0.5)` is off for 0.5 − ½ulp and for odd integers ≥ 2⁵², and the compiled oracle
  is right in both cases. On the other hand `fpow(-10, 309) = -inf` matches the compiled path and not interp.py.
  Julia's `round` is ties-to-even, Python 1.5 rounds half away from zero, and the eval.rs test pins
  `round(2.5) = 3`, which is correct for Fermium.
- **Fix:** name `run_source` (compiled) as the reference in eval.rs's header. Use `x.round()` (Rust rounds half
  away from zero exactly, like `llvm.round`). Add a differential fixture over the IEEE helpers (edge values:
  ±0, ½ − ½ulp, 2⁵²+1, subnormals, ±∞, NaN, overflow of negative bases) generated from the compiled path. Log in
  DIVERGENCES.md where interp.py and the compiled path differ.

### 7. eval.rs `for` loops: a NaN or ∞ bound or a NaN step runs 0 times silently, and the step-0 message differs (medium). Status: open
- **Repro (oracle):** `x = 0/0` / `for i from 1 to x` stops with "this for loop has no definite number of steps:
  it goes from 1 to NaN …". `for i from 1 to 3 step x` (x NaN) stops with "the step must be a non-zero number
  that goes from the start towards the end". `for i from 1 to 1/0` with a `break` after 4 passes prints `4` (the
  oracle runs 2⁶² iterations for +∞). eval.rs (`StmtKind::For`) checks only `st == 0.0`, and
  `n = floor(span + 1e-9)` that is not finite gives 0 iterations, so all three run 0 times with no error. Its
  step-0 message ("the step of this for loop is 0, so it would never end") shares few words with the oracle's.
- **Fix:** port `s_SFor` exactly: error on NaN step or NaN span, 2⁶² iterations for +∞, and the oracle's messages.

### 8. eval.rs returns NaN silently for unported built-ins and value shapes (medium). Status: open
- **Repro (by reading, with the oracle's output):**
  - the checker lowers `max(xs)` / `min(xs)` to the built-ins `max_list` / `min_list`, which eval.rs sends to
    `math1`, and `math1`'s `_ => NaN` returns NaN. The probe gives `math1("max_list", 1) = NaN`; the oracle
    prints `5` for `xs = [1, 5, 3]` / `print max(xs)`.
  - `erf`, `erfc`, `gamma`, `lgamma` also return NaN unless the runtime registers them (probe:
    `math1("erf", 0.5) = NaN`; the oracle prints 0.52).
  - `max(x, x)` with x NaN folds from −∞ and returns `-∞` (min returns `∞`); the oracle returns `NaN NaN`.
  - `max(xs, 1e-12)` (element by element, D162) collapses the list to NaN.
  - `ExprKind::Pow` calls `.num()`, so `xs^k` with a list base gives a scalar NaN; the oracle prints `[1, 25, 9]`.
  - `ExprKind::Neg` of a complex list returns the list unchanged (`v => v`): −z = z.
  - `IndexAssign`, `Push` and `Clear` on an unexpected shape do nothing.
  - `print` of an unexpected shape prints an empty line.

  `Value::num()` turns every non-number into NaN.
- **Fix:** every fall-through should return `RunError("not yet supported by the Rust back end: …")`, as the
  unsupported statements already do. A stub must stop, not print NaN (rule 6). Implement `max_list`/`min_list`
  and the NaN-propagating fold (`r = a if a > r or r != r`).

### 9. `odd_root_numerator` is not the oracle's algorithm (low). Status: open
- **Repro:** Python uses `Fraction(p).limit_denominator(99)` with an odd denominator and a relative tolerance.
  Rust tries only q ∈ {3, 5, …, 15}. The Rust probe gives `powc(-2, 1/17) = NaN` and `powc(-2, 2/25) = NaN`;
  `x = -2` / `print x^(1/17)` prints `-1.04` and `x^(2/25)` prints `1.06` in the oracle.
- **Fix:** port the continued-fraction `limit_denominator(99)` (arith.rs already has one), and add fixtures for
  every n/q with q odd ≤ 99.

### 10. Rational64 can overflow where Python's Fraction cannot (low). Status: open
- `Dim::pow`, `DExpr::pow`/`mul` and `const_exponent` (a·b, a/b) use `Ratio<i64>`: overflow panics in debug builds
  and wraps silently in release builds (overflow-checks are off in `[profile.release]`). Python's Fraction is
  unbounded. It is reachable with chained constant exponents, e.g. `((x^(1/9973))^(1/9967))^(1/9949)…`, and in
  `limit_denominator`, where `exp > 60` returns None and a value ≥ 2⁶³ is cast with `as i64` and wraps.
- **Fix:** use checked arithmetic, and on overflow give the checker's "this exponent is too complicated" error,
  or use `Ratio<i128>` as a backstop. Set `overflow-checks = true` in release, at least for fermium-units and
  fermium-check.

### 11. Small runner issues (low). Status: open
- `parse_num` raises OverflowError for `×10⁴⁰⁰`-style tokens (it computes `10.0 ** 400`), and the case is counted
  as a "crash". Subnormal expectations can only match exactly (see #1).
- `CONFORMANCE.md` lists the first 20 failures per area; spec §B3 asks for pass/fail for each program.
- The legacy score (3037/3037) comes from the same in-process `run_source`, in a reused worker process, that
  wrote the goldens. It shows only determinism, not the CLI path the Rust binary is scored through. Score legacy
  also through a `fermium run --base-dir` subprocess (the Python CLI has no `--base-dir`), and run each case in a
  fresh process at least once, to rule out state carried from one case to the next (units.py keeps module-level
  caches such as `_PREFERRED`).
- Six success cases expect empty stdout, so a binary that prints nothing and exits 0 passes them
  (e.g. `units-and-printing/c1a48190d6c0`, a list of assignments). Add a `print` or compare the warnings.

**Not findings (noted):**
- Number formatting in fermium-units matches CPython on ties: a probe of `{:.Ne}`/`{:.N}` over 0.125, 2.5, 0.375,
  1e23, 5e-324, 12.5, 1234.5, −2.5 (p = 0–3) gave 0 differences from `f"{x:.Ne}"`/`f"{x:.Nf}"`.
  `round_text` uses `round_ties_even`, like Python's `round()`.
- The Unifier (types.rs) is a faithful port of types.py's (same variable choice, `|c| == 1` first, then the
  oldest), and `limit_denominator` matches Python's tie rule.
- `fdiv` (x/0 = ±∞ by the sign of 0, 0/0 = NaN), NaN comparisons, `sign(NaN) = 0` and `fpow` with 0 to a
  negative power (+∞, including −0) match the oracle.

**Status of round 9 (04:40 UTC):** #1, #2, #3 fixed (D264: strict comparison, hint/line/exit code compared, isolated
runs with an empty PATH, self-test with fake implementations in tests/test_conformance_suite.py); #11 partly (every
failure listed; first vs last error line fixed). #6 fixed (round = llvm.round, the compiled oracle). #8 partly
(unknown built-ins are errors now; the rest is assigned to the core-expressions port). #7 assigned to the core port.
#4, #5, #9, #10 open (PROGRESS.md "Next").

## Round 10 (run 2, 2026-09-26 08:05 UTC): the Rust implementation (v2) against v1

Independent reviewer on branch claude/v2-rust. I started at cfea465, and the branch moved to 5ade949 while I
worked (LLVM ODE/eigen/PDE, decimal-place sums and rounding-level integrals in the LLVM back end, evaluator
speed-ups). Every finding below was re-checked on a binary I built from 5ade949
(`CARGO_TARGET_DIR=/tmp/redteam10-target cargo build --profile fast -p fermium-cli`). The oracle is
`python3 -m fermium run`. Scratch programs are in /tmp/claude-0/redteam10/ (p/: my programs; bc/: mutated bootcamp
and gauntlet blocks; ex/: examples; rs/: research; out/: outputs).

**What I ran.** I compared stdout, stderr and exit code byte for byte on:
- 203 new programs of my own: units and conversions, natural/nuclear/astro units, significant figures, lists,
  vectors, matrices, eigenvalues, complex numbers, derivatives, ∇, definite and indefinite integrals, Σ, algebraic
  solve, ODEs (until, backwards ranges, rk4, radau, bdf, vector and complex unknowns), bound states, PDEs, FFT,
  RNG, load/fit/plot on small CSVs, ±, Monte Carlo, parallel for, where, modules/stdlib/import by path,
  analyze, and error cases (wrong units, typos, index out of range, division by zero, singular matrix, bad CSV,
  missing module, recursion depth);
- 91 bootcamp and gauntlet blocks with their numbers changed by 13 % or a unit removed;
- the 31 examples and the 11 research programs.

I also compared the three back ends (auto, `--backend interp`, `--backend llvm`) on all 203 programs, ran
`fermium check`, `fmt --pretty/--ascii/--fix` (all programs plus examples) and a piped REPL session against v1, and
ran `ldd`, `doctor` and `strace`.

**Result.** The port is very faithful. All 91 mutated blocks gave the same bytes as v1, as did 30 of the 31
examples and every `check`/`fmt` output apart from the documented integral and caret cases. My own programs
differed from v1 in 27 cases:
- 16 are the documented indefinite-integral formula divergence (c05, c06, ii01–ii18; rust/DIVERGENCES.md). Two of
  them (ii16, ii18) are a real bug (#3).
- 1 is the documented rounding-level integral (i01).
- 3 are the documented decimal-place rule for sums (s01, s02, s04; see #7).
- 7 are findings: d06, d08 and d09 (#1), fr3 (#5), pd02, w01 and w02 (#6).

Outside my own programs, `examples/42_python_interop.fm` differs (#4) and `research/bbn_network` times out (#2).

Other checks:
- **Dependencies:** `ldd` lists only libc, libm, libgcc_s and the vdso. `strace` shows no libpython and no exec for
  ordinary programs, including indefinite integrals, radau, eigen, PDE, load/fit and plots. libpython is opened
  only for `use python`.
- **Back ends:** auto, interp and llvm now agree on every program the LLVM back end compiles. At cfea465 they
  did not: `--backend llvm` printed `9.80 m/s²` for `g = 4π² L / T²` with ±, and `0` for a loop sum of ± values,
  and the rounding-level integral rule was interp-only. That was fixed in 715c4c4 and 5a00b16, and `--backend llvm`
  now refuses ± ("uncertain values (±) aren't compiled").
- **Startup:** `fermium check`/`run` on a small program takes about 25 ms.

### 1. Fit parameters lose their uncertainty when the program uses ± (high). Status: open
- In v1, a program that uses ± anywhere gets its `fit` parameters as correlated uncertain values (reference §21,
  D124). The Rust binary prints plain numbers and drops the fit's standard error from everything computed later.
  The result is a silently understated uncertainty, which breaks a lab report.
- **Repro** (`spring.csv`: `x [cm], F [N]`, the rows 1.0,0.52 / 2.0,0.98 / 3.0,1.55 / 4.0,2.03 / 5.0,2.47):
  ```
  data = load "spring.csv"
  fit F = k x to data
  x0 = 2.0 ± 0.1 cm
  print k
  print k x0
  ```
  v1: `50.18 ± 0.47 N/m` / `1.004 ± 0.051 N`. Rust: `50.2 N/m` / `1.004 ± 0.050 N`. Both print the same fit
  summary lines above these.
- With decay.csv and `print τ ln(2), τ + dt` (dt = 1.0 ± 0.1 s), v1 prints `13.901 ± 0.037 s 21.05 ± 0.11 s` and
  Rust prints `13.9 s 21.05 ± 0.10 s`.
- `fermium-runtime/src/numerics/uncertain.rs::correlated` exists "for the parameters found by fit" but nothing
  calls it. The only suite case with fit + ± (uncertainty/5e93f6a90ca0) fails earlier, on a plot error, so the
  suite never reaches this.
- **Fix:** when `module.uses_uncertainty`, have fit store `uncertain::correlated(params, cov)`. Add conformance
  cases for fit followed by arithmetic with ±.

### 2. A `plot` (or any construct LLVM can't compile) moves the whole program to the tree-walker: 20–30× slower than v1 (high, performance). Status: open
- **Repro:**
  ```
  s = 0.0
  for i from 1 to 20000000
      s += 1/i^2
  print s
  plot [1, 2] vs [1, 2] to "pp.svg"
  ```
  Rust takes 3.9 s and v1 takes 0.77 s. Without the plot line, Rust takes 0.14 s.
- `research/rutherford_mc`, whose 2×10⁷-sample loop ends in one plot, takes 92–106 s in Rust against 3.2–4.3 s in
  v1 (under load, but consistently about 25× slower).
- `research/bbn_network` (radau, with plots) did not finish in 900 s on 5ade949 (600 s on cfea465) against 45 s in
  v1: both runs timed out.
- `research/hydrogen_levels` took 15 s against 2.3 s.
- The suite's 4 time-outs are the same programs. Three of them are filed under "uncertainty" only because a
  string contains ± ("35 ± 8.4 expected").
- The cause: `run.rs` picks one back end for the whole program. Plot, data, `±`, a function applied to a list,
  setting a vector component and similar constructs each make the program fall back to the tree-walker.
- **Fix:** compile per statement or per function, so that plot/data/± statements call into the runtime from
  LLVM code. At least compile the hot loops and functions and interpret only the unsupported statements, or
  compile `plot`/`load` as runtime calls, which are cheap. Spec §E/§B8: v2 must not be slower than v1.

### 3. `∫ 1/(a² − x²) dx` and `∫ 1/(x² − a²) dx` give `atanh`, which is NaN outside |x| < a (medium). Status: open
- **Repro:**
  ```
  F = ∫ 1/(x² - 1) dx
  print F(2), F(0.5), F(3) - F(2), ∫ 1/(x² - 1) dx from 2 to 3
  ```
  v1: `-0.549 NaN 0.203 0.203`. Rust: `NaN -0.55 NaN 0.203` (the formula is `-atanh(x)`).
- `G = ∫ 1/(4 - x²) dx` gives `atanh(x/2)/2`, so `G(3) - G(2.5)` is NaN (v1: -0.15).
- The 12-point derivative check passes because atanh's derivative is right wherever it is defined.
- Neither formula is valid for every x, but the x > a side (a resonance denominator, a potential above its
  threshold) is the common one in physics, and it is exactly where the Rust formula now fails.
- **Fix:** return `ln(|x - a|/|x + a|)/(2a)`, or v1's two-log form, which Fermium can print with `abs`. Make the
  verification sample points on both sides of every real root of the denominator.

### 4. `use python` can't find the program's own .py file when the program is run by a bare file name (medium). Status: open
- **Repro:** `cd examples && fermium run 42_python_interop.fm` fails with
  `line 9: can't find the Python module blackbody_py` (hint: "put blackbody_py.py in the program's folder"),
  although the file is there. `fermium run ./42_python_interop.fm` and an absolute path work. v1 works in all
  three cases.
- A two-line `mylib.py` next to `py1.fm` fails the same way.
- **Cause:** `fermium-runtime/src/python.rs:107` has `if base_dir and base_dir not in sys.path`. For `file.fm`,
  base_dir is `""`, which is falsy, so the folder is never added.
- The conformance runner passes absolute paths, so the suite can't see this.
- **Fix:** canonicalize the base directory in the CLI, or use `"."` when it is empty. Add a CLI test that runs a
  `use python` program by bare file name from its own folder.

### 5. The tree-walker's recursion limit is lower than v1's, and a `plot` line decides which limit applies (medium). Status: open
- **Repro:**
  ```
  f(n) = if n <= 0 then 0 else 1 + f(n - 1)
  print f(1000000)
  plot [1, 2] vs [1, 2] to "fr3.svg"
  ```
  v1 prints `1000000` and saves the plot. Rust stops with "f called itself too many times (the program ran out of
  stack)", already at n = 3×10⁵. Without the plot line (LLVM), Rust prints `1000000`, and `f(10000000)` works too.
- llvm_diff lists this case (functions/8c5765b57c68) as "llvm-better". It is the program's other statements, not
  the recursion, that decide whether a program runs.
- The REPL, the Jupyter kernel and fermium2 (pyapi) always use the tree-walker.
- **Fix:** give the tree-walker's thread a larger stack (v1's compiled code has the C stack; 512 MB is already
  used for the program thread, so the frames are probably too big), or make the evaluator iterative for calls.
  Fixing #2 also removes the dependence on unrelated statements.

### 6. The caret of the "is now your variable" warning spans the whole line; the suite doesn't compare carets (low). Status: open
- **Repro:** `c = 3` / `print c`. v1 puts `^` under `c`. Rust puts `^^^^^` under the whole assignment. The same
  happens for `G = 6.674e-11 m³/(kg s²)` (24 carets) and in `fermium check`.
- This is the only caret difference I found (error carets matched in every case), but nothing would catch others.
- Round 9 #2 was closed with hint, line and exit code compared, but `judge()` still ignores the column and caret
  of errors, and the source line, caret and hint of warnings. Spec §B3 asks for "kind, line and column".
- **Fix:** anchor the warning at the name's span. Compare the caret line (column and width) of errors and warnings
  in `conformance/run`.

### 7. The decimal-place rule for sums: cancellation prints unjustified figures; temperatures keep the old rule (low). Status: open
This is the documented divergence, and it is mostly an improvement: `1.23 m + 4.5678 m` → `5.80 m` (v1:
`5.7978 m`), `1.10 + 2.2 + 3.333` → `6.6`. But:
- `12.0 kg - 11.99 kg` prints `0.01 kg` (v1: `0.01000 kg`), and `10.0 m - 9.95 m` prints `0.05 m`. By the textbook
  rule the result is known only to 0.1, so it is `0.0 kg` / `0.1 m`. The "at least one figure" floor gives a
  cancellation more precision than its operands have, as does the recorded divergence 47680448a9da,
  `W − (Q_h + Q_c) = -7×10⁻¹⁵ μJ` with operands known to 10⁻⁴ μJ.
- `100.0 °C - 0.5 K` still prints `99.50 °C` (v1's rule), where the new rule gives `99.5 °C`.
- **Fix:** when the coarsest decimal place is above the result's leading digit, print the result rounded to that
  place (`0.0 kg`), or keep v1's output for that case. Apply the rule to temperature differences too. Record both
  in DIVERGENCES.md.

### 8. Documentation that isn't true (low). Status: open
- **REPL and ±:** rust/DIVERGENCES.md "The REPL" says "`±` is refused by the checker as in `fermium run`". In
  fact the Rust REPL accepts ± (`L = 1.20 ± 0.01 m` then `L * 2` → `2.400 ± 0.020 m`), which v1's REPL refused.
  This is the B5.6 improvement, but it is undocumented and the sentence says the opposite.
- **CONFORMANCE.md is stale (07:15, 4e3e0eb):**
  - 12 of the 27 recorded divergences (09fe8fafb686, 173e2d6f8f97, 47680448a9da, 664786f7f30f, 67451d15fe04,
    89747a9eb10a, 9bef8c6f60d5, a1b56d5b320c, a504c61d2cbc, ca02b7679e45, e7ffc7d1ff73, eaf1d936eb6c) appear
    nowhere in it.
  - At that commit the default (LLVM) path printed v1's output for them, so DIVERGENCES.md described behaviour
    the default binary didn't have. `∫ sin(x) dx from -1 to 1` printed `2.78×10⁻¹⁷` by default and `3×10⁻¹⁷`
    only with `--backend interp`.
  - That is fixed in 715c4c4, but the scoreboard still counts those cases as passes. Regenerate it on HEAD.
- **"The back end never changes what a program prints"** (docs/architecture.md) was false at cfea465 (see
  "Other checks"). It is true now as far as I tested, apart from #5.
- **Python interop:** reference §"Python interop in Fermium 2" says `use python` works "exactly as described",
  which #4 contradicts.

### 9. Speed of the compiled paths against v1 (low; measured under a load average of 8–19 from other agents). Status: open
TIME_INNER from benchmarks/fermium, 3 runs each, v1 against Rust (default):

| Benchmark | v1 | Rust | Rust / v1 |
|---|---|---|---|
| blackbody (1000 integrals) | 0.0025–0.0032 s | 0.008–0.016 s | about 3–5× |
| forces | 0.020–0.029 s | 0.063–0.082 s | about 3× |
| spring_rk4 | 0.066–0.12 s | 0.13–0.24 s | about 2× |
| nbody | 0.09–0.22 s | 0.20–0.26 s | about 1.5× |

unit_loop and spring_adaptive are at parity. Whole-program wall time favours Rust, because it starts in 25 ms
against v1's 0.4 s.
- **Fix:** profile the quadrature call path (blackbody: probably per-call allocation or a non-inlined integrand
  thunk) and the vector code in forces/nbody. Re-measure on an idle machine before B8.

**Not findings (checked):**
- The documented divergences I spot-checked are right physically:
  - ∫ of a 1 nm Gaussian over ±1 m = 2.51×10⁻⁹ m (σ√2π);
  - ∫₀¹ |x − 0.3|^−0.8 = 8.5858;
  - ∫₋₁² |x|^−0.9 = 20.718;
  - ∫ exp(−x²) from −10⁶ to 10⁶ = 1.77.
  v1's `0`, `0 m` and "couldn't compute" were wrong.
- `∫ sec(x) dx`: Rust's `ln(tan(x) + sec(x))` is right on |x| < π/2, where v1's `ln(1 + sin)/2 - ln(-1 + sin)/2` is
  NaN for every real x. An improvement, not a regression.
- These matched v1 byte for byte:
  - seeded RNG and Monte Carlo (`propagate montecarlo`, sample);
  - parallel for sums (reproducible and equal to v1 to 15 digits);
  - FFT, bound states (matrix and shooting), PDE heat/wave, radau/bdf;
  - analyze, stdlib imports, import by path, private names, bad CSVs, missing modules;
  - all the error cases I tried.
- `fermium doctor` is honest: it says `fermium build` isn't in this version (B5.10 is still open).

**Round 10 status (08:50 UTC):**
- #1 (fit uncertainty lost with ±): assigned to the uncertainty agent.
- #2 (one uncompiled construct sends the whole program to the tree-walker): assigned to the LLVM agent as its priority (compile plot/load in LLVM, or a mixed mode).
- #3 (atanh antiderivative NaN for |x| > a): assigned to the calculus agent.
- #4: fixed. The program folder is absolute even for a bare file name; test `a_bare_file_name_finds_the_module_beside_it`.
- #5: partly fixed. The tree-walker now has 1.9 GB of stack, about 5×10⁵ calls deep instead of 3×10⁵; the rest is documented in rust/DIVERGENCES.md. Smaller frames are open.
- #6 (warning caret): assigned to agent A; the runner comparing columns is open.
- #7 (sums rule on cancellations): open, next.
- #8: DIVERGENCES.md is corrected (the REPL accepts ±, `fermium build` exists). CONFORMANCE.md will be regenerated after the next full run. The reference's `use python` line is true after #4.
- #9 (compiled inner loops slower than v1): assigned to the LLVM agent, to re-measure on a quiet machine.

## Round 11 (run 2, 2026-09-26 10:30 UTC): Rust v2 against v1, after the mixed-mode LLVM back end

Independent reviewer. I started on a binary built from 64e088d. The branch moved to d393edb while I worked
(mixed-mode LLVM, integrator forms, conformance regenerated), so I rebuilt from d393edb
(`CARGO_TARGET_DIR=/tmp/claude-0/rt11-target FERMIUM_NO_AOTRT=1 cargo build --profile fast -p fermium-cli`) and
re-ran everything. Every finding below holds at d393edb. Oracle: `python3 -m fermium run` from the same tree.
Programs are in /tmp/claude-0/redteam11/p/ and outputs in /tmp/claude-0/redteam11/out/{v1,auto,interp,llvm}/.

**What I ran.**
- 361 new programs. Areas: units and conversions (including °C/°F offsets), natural, nuclear and astro units,
  constants, sig figs, sums and cancellations, `to N digits`, derivatives, ∇, definite integrals, 60 indefinite
  integrals (including the x² − a² families), Σ, ODEs (rk45, rk4, radau, bdf, until, backwards, complex, vector),
  PDEs, bound states, FFT, RNG, parallel for, fit with and without ±, ± propagation (including correlated fit
  parameters and det/inverse/solve_linear of uncertain matrices), vectors, matrices, complex numbers, lists,
  stdlib and user modules, and 12 mixed-mode programs (plot/load/fit/list functions inside loops, with variables
  set before and after).
- About 55 of the 361 are error programs: unit mismatch, undefined names, bad index, wrong arity, missing
  file/module/column, private name, bad slice, etc.
- Each program ran with `--backend auto`, `--backend interp` and `--backend llvm`. I also ran the conformance
  suite, the benchmarks and `doctor`, piped a REPL session, and read the docs.

**Result.** The port is very faithful. 328 of 361 programs are byte for byte identical to v1. The 33 that differ:
- 12: the documented decimal-place rule for sums (g02 g03 g05 g07 g10 g11 s01 s02 s04 s12 s13 u40). All are
  right by the textbook rule.
- 7: the documented indefinite-integral formula divergence, with equivalent formulas (i09 i25 j11 j12 j13 j18
  j27).
- 2: v1 bugs that Rust fixes. g25 is `∫ sec`. In j01, `∫ √x ln(x)` gives the right formula, where v1 refused
  it with a "meijerg" error.
- 5: the documented coverage divergence (j06 j07 j23 j28 g23). See #5.
- 1: both implementations are wrong, on different sides of the pole (g27, #2).
- 5: a Rust bug (g08 g09 g31 u03 mx11, #1).

Other results:
- **Back ends:** `interp` and `llvm` gave the same output as `auto` on every program, except where llvm
  refused with exit 3 (47 programs: ±, and at 64e088d also data and vector-component programs). The one other
  exception is r5.fm, a recursion 10⁶ deep plus a plot. It now works in auto/llvm (mixed mode) and runs out of
  stack in interp, as rust/DIVERGENCES.md "The command-line tool" says.
- **Panics:** none. I searched every output for "panicked" and "RUST_BACKTRACE".

### 1. A difference of two °C or °F temperatures prints the wrong number (high). Status: open
- The decimal-place rule takes each operand's last significant place from its absolute SI value. For
  `20.0 °C`, that value is 293.15 K with 3 significant figures, so the place is 1 K instead of 0.1 K. The
  difference is then rounded to whole kelvins.
- This hits the most common measured subtraction in a teaching lab: ΔT from two thermometer readings.
- Both back ends (and `interp`) print these:

| Program | Rust | v1 | Right |
|---|---|---|---|
| `print 0.5 °C - 0.2 °C` | `0 K` | `0.30 K` | 0.3 K |
| `print 25.5 °C - 20.0 °C` | `6 K` | `5.50 K` | 5.5 K |
| `print (25.5 °C - 20.0 °C) in mK` | `6×10³ mK` | `5500 mK` | 5500 mK |
| `print 98.6 °F - 32.0 °F` | `37 K` | `37.0 K` | 37.0 K |
| `print 300.0 K - 20.0 °C` | `7 K` | `6.850 K` | 6.9 K |
| `print 25.5 °C - 20.0 °C + 0.1 K` | `6 K` | `5.60 K` | 5.6 K |

- In a calorimetry program (g09.fm), `T1 = 21.3 °C`, `T2 = 23.8 °C`, `print T2 - T1` prints `2 K`. The same
  value stored first (`ΔT = T2 - T1`, `print ΔT`) prints `2.50 K`.
- Variables work the same way: `T = 20.0 °C` raised by 1.5 K three times, then `print T - 20.0 °C`, prints `4 K`
  where the answer is 4.5 K (mx11.fm).
- Other units are fine: `1.0 hr - 30.0 min` → `0.5 hr`, `3.0 mi + 1.00 km` → `3.6 mi`, `1.5 km + 20.5 m` →
  `1.5 km` are all right by the rule. So are uncertain temperatures: `(23.8 ± 0.2 °C) - (21.3 ± 0.2 °C)` →
  `2.50 ± 0.28 K`.
- **Cause:** `fermium-codegen/src/eval_calc.rs`, `sum_place` calls `last_place(v / k, s)` with v in absolute
  kelvins.
- **Fix:** take the place from the value in the unit it was written in. With the offset removed, the place of
  `20.0 °C` is 0.1, and 0.1 °C is 0.1 K. Until then, keep v1's rule when any operand has an offset unit.
- **Tests:** add conformance cases for °C − °C, °F − °F, K − °C and a stored-variable operand. The suite has no
  printed difference of two offset temperatures, which is why the 4-case measurement in DIVERGENCES.md didn't
  catch this.
- **Repro:** p/g08.fm, g09.fm, g31.fm (last line), t01.fm, t03.fm, mx11.fm.

### 2. Antiderivatives use ln(u), not ln|u|, so F is NaN on one side of every real pole (medium; both implementations). Status: open
- `∫ 1/(9 - x²) dx` gives `-ln(-3 + x)/6 + ln(3 + x)/6` in both. So `F(1) - F(0)` is NaN on |x| < 3, the side
  where 1/(a² − x²) is usually used (i04, g13).
- `∫ 1/(1 - x²)` has `F(0.5)` = NaN (i22).
- `∫ 1/(x - 5)` has `F(2) - F(1)` = NaN (g28).
- `∫ 3/(x² - 5x + 6)` is NaN between the roots (g29).
- `∫ 1/(2 - x)`: Rust gives `-ln(2 - x)`, which is NaN for x > 2. v1 gives `-ln(-2 + x)`, which is NaN for x < 2
  (g27). This is the one output difference in this family, and neither is right on both sides.
- Round 10 #3 was fixed by adopting v1's form (i03 now matches v1). That fixes x > a and leaves |x| < a broken
  in both. The 12-point derivative check evidently passes because NaN samples are skipped.
- **Fix:** use `ln(abs(u))` for a linear or quadratic u with real roots. The derivative is the same, and Fermium
  already prints `abs`. At least make the check test both sides of every real root.

### 3. `∫ 1/(x² - a²) dx` with a dimensional constant gives a function that can't be called (low–medium; both). Status: open
- **Repro:** `a = 2 m`, `F = ∫ 1/(x² - a²) dx`, then `print F(3 m) - F(2.5 m)`. Both stop with
  `ln needs a plain number, but got length [m]`, because the formula is `ln(x - a)/(2a) - ln(x + a)/(2a)` (i10).
- The definite integral works.
- **Fix:** combine the logs into `ln((x - a)/(x + a))/(2a)`, or divide each argument by a, when the constant has
  units.

### 4. The decimal-place rule for sums: where it applies is fragile (low). Status: open
- The rule applies only to a sum that is the whole print item:
  - `print 1.20 m + 2.0 m` → `3.2 m`;
  - but `-(1.20 m + 2.0 m)` → `-3.20 m`, `(1.20 m + 2.0 m) in cm` → `320 cm`, `2 (1.20 m + 2.0 m)` → `6.40 m`,
    `str(…)` → `3.20 m`, and `[1.20 m + 2.0 m, 1 m]` → `[3.20, 1.00] m` (g01, g31).
- DIVERGENCES.md says the place is found "in the unit the result prints in", which suggests `in cm` is covered.
- `100.0 °C - 0.5 K` still prints `99.50 °C`: the second half of round 10 #7 is still open. Meanwhile
  `20.5 °C + 1.25 K` prints `21.8 °C`.
- An exact whole operand turns the rule off: `1500 m + 2.0 m` → `1.5×10³ m`, and `100 m + 0.5 m` → `100 m`.
  The added amount disappears (t02). v1 does the same, but the new rule could print 1502.0 m and 100.5 m.
- **Fix:** apply the rule through negation, unit conversion and scaling by exact numbers. Otherwise document
  exactly where it applies.

### 5. Antiderivative coverage: textbook integrals v1 could do are refused, and some forms are redundant (low). Status: open
- **Refused.** v1 gives a formula and Rust says "Fermium couldn't find a formula for this integral" for:
  - `∫ x³ ln(x)² dx` (j06);
  - `∫ x asin(x) dx` (j07);
  - `∫ 1/(x² √(x² + 1)) dx` (j23);
  - `∫ 1/cosh(x) dx` (j28);
  - `∫ 1/(x √(x² - 1)) dx` (g23). v1 refuses this one too, with a different message.
- DIVERGENCES.md describes the gaps as mixtures, products of several transcendental functions and symbolic
  coefficients. These are single-function textbook cases: add rules, or name them in the doc.
- **Redundant forms:**
  - `∫ (x + 1)/(x² - 1) dx` → `ln(-1 + x)/2 + ln(-1 + x²)/2 - ln(1 + x)/2` (v1: `ln(-1 + x)`);
  - `∫ x/(x² - 4x + 3)` → `ln(3 + x² - 4x)/2 - ln(-1 + x) + ln(-3 + x)`.
  - They are equivalent, but not what a physicist would write. Cancel common factors before partial fractions.

### 6. Speed of compiled inner loops against v1 (low; indicative only). Status: open (round 10 #9)
- **Conditions:** load average 19–23, `fast` profile (release without LTO), TIME_INNER, 3 interleaved runs.
- **Results:**

| Benchmark | Rust | v1 |
|---|---|---|
| nbody | 0.27–0.31 s | 0.14–0.16 s |
| spring_rk4 | 0.19–0.29 s | 0.068–0.081 s |
| forces | 0.06–0.11 s | 0.035–0.054 s |
| blackbody | 0.009–0.035 s | 0.0025–0.0028 s |

- README and benchmarks/ make no speed claims for v2, so nothing is unfair today. Re-measure a release build on
  a quiet machine before any v2 number is published.
- **Round 10 #2 is fixed** by mixed mode. The 2×10⁷-iteration loop plus `plot` takes 0.12 s (was 3.9–5.1 s;
  v1 1.0 s), and plus `load` 0.09 s. A ± program is still interpreted, but a 2×10⁶-iteration loop in one takes
  0.70 s against v1's 11.1 s.

### 7. Documentation (low). Status: open
- **Lesson 0 (`bootcamp/lesson00_setup.md`, this branch)** tells beginners to download `fermium-macos-arm64` /
  `fermium-linux-x86_64` from the Releases page. The repository has no releases yet (`list_releases` is empty),
  and the binary calls itself `2.0.0-dev`. Until the first release, say it is coming and point to the Python
  version (or building from source). Otherwise Step 2 is a dead end for a student.
- **The conformance judge still ignores carets and columns** (the second half of round 10 #6). My 55 error
  programs matched v1's carets byte for byte anyway.

**Checked and fine:**
- **CONFORMANCE.md reproduces exactly.** A fresh `conformance/run --impl rust` on d393edb gives 3334/3366 + 32
  documented = 3366/3366, with the same per-area table and the same divergence list.
- **Round 10 fixes verified:**
  - #1: fit parameters carry correlated uncertainties. `k x0` → `1.004 ± 0.051 N`; `k (3 cm) + F0` with a
    correlated slope and intercept → `1.510 ± 0.017 N`, the textbook σ at the mean x.
  - #2 (mixed mode) is fixed.
  - #3: matches v1 now; see #2 above for what remains.
  - #4: `use python` by a bare file name works.
  - #5: documented.
  - #6: the warning caret is under the name.
  - #7: `12.0 kg - 11.99 kg` → `0 kg`, `10.0 m - 9.95 m` → `0.1 m`.
- **Uncertain matrices (B-U1)** match v1 and are right:
  - `det([[1, 2], [3, 4]] L)` = `-2.880 ± 0.048 m²`;
  - `det(I a)` = `4.00 ± 0.40`, and `det(M) - a²` = `0 ± 0`;
  - independent a and b in diag(a, b) give det `6.00 ± 0.63` and `solve_linear` `0.333 ± 0.011` / `0.500 ± 0.050`;
  - solving a stiffness matrix k·K₀ gives `0.0667 ± 0.0033 m`.
- **Physics spot checks:**
  - the 1 nm box ground state is 0.376 eV;
  - Newton cooling after 30 min is 23.5 °C;
  - radau and bdf stiff solves are right;
  - PDE heat decay matches exp(−Dπ²t/L²) to 5 digits;
  - the Kepler orbit closes to 1.0000 AU;
  - FFT, seeded RNG, parallel for, the stdlib modules and natural-unit conversions (ħc, 1/m_e, GeV⁻² → pb) are
    right.
- **Error messages:** all the error programs have v1's message, line, caret and hint.
  - That includes errors inside a function reported at the definition with "when calling f on line 3", errors
    in solve blocks and in module imports.
  - Runtime errors (index out of range, singular matrix, slices) have no caret or hint, as in v1.
- **Doctor and REPL:** `doctor` is honest (build unavailable without AOTRT; Python optional). The REPL answers
  Lesson 0's examples exactly (`2.30 m`, `1.61 km`, `1.60934 km`).

**Round 11 status (10:45 UTC):**
- #1 (temperature differences): fixed in the sums rule. A sum whose result is a temperature keeps v1's rule, so both back ends print `0.30 K` again.
- #2, #3, #5 (ln|u|, dimensioned log argument, refused integrals): assigned to the calculus agent.
- #4 (sums rule fragile): open; the `100.0 °C - 0.5 K` case is covered by the #1 fix.
- #6 (speed): the LLVM agent re-measures on a quiet machine for PERF.md.
- #7: the Releases page gets its first release at the B8 v2.0 tag. Comparing carets and columns in the runner is open.

## Round 12 (2026-09-26 13:10 UTC): the v2.0 release as a user meets it, benchmarks, the newest compiled code

Independent reviewer. Binary: `claude/v2.0-freeze` (f61b8e6) built with
`CARGO_TARGET_DIR=/tmp/claude-0/rt12-target FERMIUM_NO_AOTRT=1 cargo build --release -p fermium-cli`
(`fermium 2.0.0 (Rust)`; no `fermium build` in this build). Oracle: `fermium-legacy` (checked:
`import fermium` → /home/user/fermium/legacy/fermium). Programs: /tmp/claude-0/redteam12/{loops,phys}/, outputs
in /tmp/claude-0/redteam12/{out,out2}/. Load average 5–7.7 the whole time (other agents).

**High: none.** The compiled-code changes of D273 held up against everything I threw at them (below).

**Medium**
- **#1 The release binaries don't exist, but the docs send users to them without a caveat.** README "Install", CHANGES_2.0 "Installing"
  (first bullet), and bootcamp Lesson 0 Steps 2–3 ("Open the Releases page … click the file for your computer")
  all present the download as the way in; nothing says the release hasn't been published yet (D274: the tag push
  was refused). D274 says "CHANGES_2.0 and the README say how to build from source meanwhile". They do list the
  build route, but only as an alternative. A beginner following Lesson 0 finds an empty Releases page. This repeats
  round 11 #7. Fix: a one-line note in all three ("the first release is pending; until then build from the
  source, below") until the owner pushes the tag.
- **#2 The textbook pendulum, written as on paper, is silently a different equation (v1 and v2 alike).**
  `θ'' = -g / L sin(θ)` is `-g/(L sin θ)` under D8. D34 warns only when the denominator factor *is* an unknown,
  not when it's a function of one. So `phys/q02_pend_small.fm` (θ₀ = 1) and `phys/p11_pendulum_large.fm`
  fail with a misleading "step became too small … not a blow-up … add an absolute tolerance" (the equation
  really is singular at θ = 0). The silent case is worse: `phys/q06_force_over_mass.fm`, `a(t) = F0 / M cos(t / 1 s)`,
  prints a(1 s) = 3.70 m/s² (= F0/(M cos 1)) with no warning. `-(g / L) * sin(θ)` gives the right 1.60 (SciPy 1.6022).
  Suggest extending D34's warning to a denominator factor that is a call of sin/cos/exp/… (or any call whose
  argument holds an unknown or the independent variable) after a spaced `/`. `h c / λ k_B T` has no call, so it
  stays unwarned.

**Low**
- **#3 CHANGES_2.0 "Speed" is stale.** It says "The benchmark table in the README was measured with Fermium 1.5 …
  the re-run … replaces the table", but the README table is now the 26 Sep Fermium 2 run.
- **#4 Stale SymPy mentions in user docs:**
  - docs/reference.md:465 ("hand 𝑖 to SymPy");
  - :544 ("tidied by SymPy when it is installed");
  - :577 and :590 ("done symbolically with SymPy", "When SymPy can't give a usable formula");
  - bootcamp/lesson08_integrals.md:151 ("This uses a separate maths library (SymPy)");
  - docs/stdlib.md:6 cites `tests/test_stdlib.py` (now `legacy/tests/`).
  The kept-for-conformance error "SymPy's formula for this integral uses the function Ei …" also names a library
  that v2 doesn't use. That is documented, but worth rewording once the oracle goes.
- **#5 ∫ sin(x) cos(x) dx is not v1's function** (`phys/s02_indef_trig.fm`, `phys/s41_indef_value.fm`). v2 gives
  `-cos(2x)/4`, v1 gives `sin(x)²/2`, so F(1) is 0.104 against 0.354 (a constant ¼ apart; F(b) − F(a) agrees).
  DIVERGENCES says formulas may be written differently "with symbolic constants … the values agree". Here
  there are no symbolic constants and F's values differ. Either match SymPy (substitute u = sin x first), or say in
  DIVERGENCES that F can differ from v1's by a constant.
- **#6 Ambiguous ratios in the README speed table.** `1.2× Fermium 1.5` (adaptive RK45) and the bold
  `**1.6× Fermium 1.5**` (forces, 4 threads) are both *slower* (696 vs 592 µs; 11.0 vs 6.74 ms). Elsewhere bold
  plus "(slower)" marks a loss, so a reader can take 1.6× for a speed-up. Write "1.6× slower than Fermium 1.5".
- **#7 `make install` bypasses rust/.cargo/config.toml.** `cargo install --path rust/crates/fermium-cli` runs
  from the repository root, and cargo reads `.cargo/config.toml` from the working directory. So the
  `-fuse-ld=lld` flag isn't applied, and the link uses the default linker, which BUILD.md calls several times
  slower. I didn't check whether it fails. Fix: `cd rust && cargo install --locked --path crates/fermium-cli`.
- **#8 (v1-shared) An RK4 right side that goes out of its domain gives NaN silently.** In `loops/r01_rk4_error_mid.fm`
  (`x' = -√x`, past x = 0), `x(3 s)` prints `NaN m` with no warning on any back end.
- **#9 (nit, fair to Julia) blackbody.jl's timed region includes the extra `ratio` integral** (1001 integrals
  against Fermium's 1000). That handicaps Julia by about 0.1 %.

**What checked out**
- **Compiled code (D273), 71 adversarial programs** (`loops/v*`, `w*`, `x*`, `y*`, `r*`): `--backend llvm`,
  `--backend interp` and `fermium-legacy run` were byte for byte identical on every one (stdout, stderr, exit
  code). They covered:
  - indexes out of range only on the last, the first or some middle iterations (`x[6 - i]`, `x[4 - i]`,
    `x[i + 1]` with step 2, two lists of different lengths in one body, descending loops to 0, negative ranges);
  - lists that grow or are cleared inside the loop, or in a function called from it;
  - a list rebound, pushed through an alias (`b = a`), or mutated by a function;
  - "constants" reassigned later at module level, inside a loop, by a `for N` loop, by a solve unknown of the
    same name, or in an if/else; a function reading a name before it is defined;
  - a loop variable or an outer loop variable reassigned inside a versioned body; loop bounds changed in the
    body;
  - fractional, zero, negative and too-large steps, NaN bounds, `to inf` with and without `break`, trip counts
    near 2⁵³ and past 2³¹; integer products past 2⁶³ (no wraparound); integer comparisons against fractions;
  - `parallel for` with an out-of-range index and with negative steps;
  - RK4 right sides that fail mid-solve (index error, √ of a negative, log, 1/(1 − t)), a solve inside a
    function used in a solve's right side, solves in loops, backwards RK4, a step that doesn't divide the range
    or exceeds it, vector RK4, and a right side reading a list that changes between solves;
  - mixed mode (plot and list-function calls next to compiled loops).
- **Benchmarks reproduced** (release build against fermium-legacy, 7 interleaved runs, medians, load 7.0–7.7,
  so busier than RESULTS.md's 1.4–2.9). Inner v2/v1: nbody 1.03, spring_rk4 0.98, blackbody 1.46,
  spring_adaptive 1.20. Wall: v2 27–140 ms against v1 297–443 ms. That agrees with PERF.md (1.06, 0.98, 1.4,
  1.26) and RESULTS.md. Like for like: spring_rk4 has the same dt and 10⁶ steps (Fermium's trajectory storage
  and step check are disclosed); blackbody has the same rtol 10⁻¹⁰; Julia's warm-up is excluded from Inner and
  said so. The README's wall ranges (24–133 ms, 0.8–2.5 s, 0.16–0.41 s, 18 ms startup) match RESULTS.md. No
  cherry-picking found: the slower rows (spring_rk4 1.89×, blackbody 1.50× Julia, forces 1 thread) are shown.
- **User-facing tools and docs:**
  - `fermium --version` → `fermium 2.0.0 (Rust)`, as Lesson 0 and CHANGES say.
  - `doctor` is honest: it says `fermium build` isn't in this binary and that Python is optional.
  - Both README program blocks print their commented results (9.70 m/s², 31.8 ft/s², 10 rad/s, v(t), 1 J,
    3.52 cm).
  - `fmt --pretty`, `check`, `use python math`, SVG plots and the REPL work.
  - The conformance numbers (3334 + 32 = 3366) agree across CHANGES_2.0 and CONFORMANCE.md.
  - legacy/README is accurate (`fermium-legacy --version` → `fermium 0.1.0`, `fermium-legacy doctor` works).
  - pyproject only installs `fermium-legacy`. I found no doc telling a user to pip-install `fermium` as the
    language.
- **Physics, 106 programs** (`phys/p01–p64`, `s01–s41`, `q*`): identical to v1 except the documented
  decimal-place sum rule (`0.1 + 0.2` → `0.3`, v1 `0.30`) and #5. The results match physics:
  - Bohr radius 52.9 pm, 13.6 eV, ħc 197 MeV fm, escape speed 11.19 km/s;
  - box levels 0.376/1.50/3.38 eV, harmonic oscillator E/ħω = 0.500/1.50/2.50;
  - Monte Carlo g = 9.87 ± 0.22 m/s², correlated ± (x²/x = 2.00 ± 0.10, x − x = 0 ± 0);
  - complex ODE = exp(−2i), radau/bdf stiff solves, the Bateman two-member chain;
  - 1 AU orbit = 365 d, Compton 2.43 pm, R∞ = m_e c α²/2h, 1/α = 137, Hα 656.46 nm, ∇, curl and ∇² of test
    fields, Carnot 0.400, Doppler 1100 Hz;
  - every unit-error program has v1's message, caret and hint.
- **Seen but not new (v1 does the same):** the display mixes `1.0` and `1` in one list (`[1.0, 50, 3.0]`);
  `r × F` prints in J rather than N m; `semf_binding(A, Z)` doesn't reject Z > A (the README's own SEMF example
  writes B(Z, A)).
- **Not tested:** `fermium build` (the binary had no AOTRT), the 3.6 MB wasm claim, macOS.

**Round 12 status (13:55 UTC):**
- #1: fixed. README, CHANGES_2.0 and Lesson 0 say the release isn't published yet and point to building from source.
- #3, #4, #6: fixed. The CHANGES speed section is updated, the SymPy mentions in reference.md and lesson 8 are corrected, stdlib.md's test path is corrected, and the README ratios now say "slower".
- #5: documented in DIVERGENCES (antiderivatives differ by a constant; F(b) − F(a) agrees).
- #7: fixed. `make install` runs from rust/.
- #2, #8: added to BACKLOG for v2.5 (a new warning is a language change).
- #9: noted (Julia's blackbody times 1001 integrals against 1000, 0.1 % in Fermium's favour); it doesn't change any ratio's first two digits.

## Round 13 (2026-09-27 01:25 UTC): student tools, modules/stdlib, pathological inputs, security basics

Independent reviewer. Binary: `origin/claude/v2.5` (16e73ac), built with
`CARGO_TARGET_DIR=/tmp/claude-0/rt13-target cargo build --release -p fermium-cli` (`fermium 2.0.0 (Rust)`, with
`fermium build`). Oracle: `fermium-legacy`. Programs and outputs: /tmp/claude-0/redteam13/{mod,rob,fmt,repl,lsp,jup,build,sec,out}/.
Rounds 10–12 covered physics, back-end agreement, loop optimisations and docs; this round covers the tools, modules,
robustness and security.

**High**
- **#1 Panic (exit 101, Rust backtrace) when a unit's rational power overflows 64 bits.** Reproducers:
  `print √(√(…√(1 m)…))` nested 64 deep (rob/pk1.fm; 62 deep prints `1 m^(1/4611686018427387904)`), or
  `x = 1 m` / `z = (x^(1/4294967296))^(1/4294967296)` (rob/pe_s4.fm). This prints
  `thread '<unnamed>' panicked at …num-rational-0.4.2/src/lib.rs:137:13: denominator == 0` with the backtrace
  `num_rational::Ratio<T>::reduce ← fermium_ir::types::DExpr::pow ← fermium_check::arith::…::power`.
  - Affected: `fermium run`, `fermium check` and the REPL. The REPL session dies and every definition is lost.
  - Survives it: the language server (it publishes "internal error in Fermium: panic: denominator == 0") and
    Jupyter (the cell errors and the kernel lives).
  - v1 (Python Fractions) prints `1 m^(1/18446744073709551616)`.
  - Contrived input, but the brief asked for any panic to be reported as high.

**Medium**
- **#2 The same 64-bit unit powers wrap silently, which gets past the unit check** (the same root cause as #1:
  unchecked i64 arithmetic in `DExpr` and `Ratio<i64>`).
  - `x = 1 m` / `y = (x^4294967296)^4294967296` / `print y + 1` prints **2** with no error (rob/pe_s3.fm): 2⁶⁴
    wraps to 0, so y is a plain number.
  - Doubling generic functions `f_i(x) = f_{i-1}(x) f_{i-1}(x)`:
    - `f63(1 m)` shows `1/m⁻⁹²²³³⁷²⁰³⁶⁸⁵⁴⁷⁷⁵⁸⁰⁸`;
    - `f64(1 m) + 1` passes the check and prints `1` (rob/pe64.fm).
  - `cbrt` applied 40 times gives `1 1/m^(1/6289078614652622815)`, with the wrong sign (rob/pk9.fm).
  - `print 1 m^1e300` saturates to `m⁹²²³³⁷²⁰³⁶⁸⁵⁴⁷⁷⁵⁸⁰⁷` (pk7.fm).
  - v1 is exact in every case, e.g. `can't add … [m¹⁸⁴⁴⁶⁷⁴⁴⁰⁷³⁷⁰⁹⁵⁵¹⁶¹⁶] to a plain number`.
  - Fix for #1 and #2: use checked_mul and checked_add in the dimension and exponent code, and on overflow give a
    one-line error ("this power of a unit is too large for Fermium").
  - Separately, `(1 m)^1e-19` is `m^(1/10000)` in v2 and dimensionless `1` in v1. Both are wrong: an exponent
    that isn't a simple fraction should be an error.
- **#3 `plot … to "<path>"` overwrites any file with PNG bytes, whatever its extension, and creates folders.**
  - sec/prog/p.fm: `plot sin(x) vs x from 0 to 1 to "../victim/notes.txt"` replaced a text file with a PNG, and
    `to "/abs/newdir/a/b/c.svg"` created the folders.
  - v1 refuses non-image extensions: *plot not saved: Format 'txt' is not supported*.
  - In v2 a typo or a downloaded program can clobber `prog.fm`, a `.py` or a dotfile. vfs.rs `write` does
    `create_dir_all` and then `fs::write`.
  - v1 and v2 both accept `../` and absolute paths.
  - Suggest: accept only .png/.svg/.gif (v1's rule, and frame folders for animations), and perhaps warn when the
    path leaves the program's folder.
- **#4 The Unicode singletons OHM SIGN U+2126, KELVIN SIGN U+212A and ANGSTROM SIGN U+212B are rejected; v1 accepts
  them** (v1 NFC-normalises; these three have canonical decompositions).
  - `R = 5 Ω` (U+2126, what Word, PDFs and some keyboards produce) gives *'Ω' isn't a unit Fermium knows*.
  - `print 5 kΩ`, `print 300 K` (Kelvin sign), `print 1 Å` (Angstrom sign) and `Ωx = 2` / `print Ωx` also fail.
    v1 prints `5 Ω`, `300 K`, `1 Å` and `2`.
  - Reproducers: rob/ohm1–3.fm, r19, n_kelvin_sign, n_angstrom_sign, n_ohm_var, n_kelvin_var.
  - Not in DIVERGENCES, and no conformance case has these characters.
  - The micro sign µ (U+00B5) does work.
  - Fix: NFC-normalise the source (or add the three to the look-alike table).
- **#5 A Jupyter interrupt doesn't stop a running cell, and the user docs don't say so.**
  - `for i from 1 to 10^11` in a cell, then `km.interrupt_kernel()`: no shell reply within 30 s, and later cells
    stay stuck behind it (jup/drive2.py).
  - DIVERGENCES (lines 388–390) documents this.
  - But Lesson 0:228, Lesson 4:173, TROUBLESHOOTING:456 and reference:1198 all tell the student that Ctrl+C stops
    a stuck program. In a notebook the only way out is to restart the kernel.
  - Related (v1-shared): in the REPL, Ctrl+C during a running line exits the whole REPL (exit 130,
    `stopped by Ctrl+C`), which loses all definitions. TROUBLESHOOTING:456 says "you get the prompt back".

**Low**
- **#6 Docs that disagree with the binary:**
  - reference.md:926–927 says `fermium build` *refuses a program that uses Python*.
    - v2 builds build/b_py.fm and 42_python_interop.fm, and the executables run.
    - The executable finds `blackbody_py.py` in the build-time folder, stored as an absolute path, not next to the
      executable or in the working folder. A planted module in the working folder was *not* loaded (good).
    - If that folder moves, the executable prints its first results and then fails at line 24 with
      *No module named 'blackbody_py'*. The start-up re-check doesn't notice.
  - reference.md:1250 says uncertainties don't work in the REPL or Jupyter. In v2 they do
    (`L = 1.20 ± 0.01 m` / `print L - L` → `0 ± 0 m`; v1 refuses). This is an undocumented divergence.
  - reference.md:1252 says `fermium build` writes plots as SVG. The executables write PNG, exactly as `run` does.
  - `fermium --help` says build is *"(not yet in this version)"*, while `doctor` says it works (main.rs:25).
- **#7 `fermium build` can overwrite the program's source (v1-shared).**
  - `fermium build noext` (a source file without `.fm`) replaces the source with the ELF executable.
  - So does `fermium build -o prog.fm prog.fm`. There is no warning in either case.
  - Suggest refusing when the output path is the input path.
- **#8 Language-server hover after the first error (v1-shared).**
  - Checking stops at the first error, so a later `c = 3 kg` hovers as *speed of light in vacuum*.
  - Names with subscripts (`E₀`) never hover, even when there's no error.
  - Only one diagnostic is published at a time.
- **#9 The REPL and Jupyter overflow the stack earlier than `run` does.**
  - `f(n) = if n <= 0 then 0 else 1 + f(n - 1)`; `f(100000)` fails in the REPL and in Jupyter with *ran out of
    stack* (80 000 works).
  - `fermium run` and v1's REPL give 100000.
  - Not in DIVERGENCES.
- **#10 The "nested too deeply" error lost v1's detail and hint** (parser.rs:167).
  - v1: *…to compile (very long or deeply nested expressions)*, hint *split the expression into several lines with
    names*. v2 prints only the first half.
  - Seen with 5000 parentheses, 10 000 unary minuses, 2000 nested lists, and 3000 nested calls.
  - Not in DIVERGENCES; no conformance case covers it.
- **#11 An error inside a stdlib function gives a different, ambiguous hint.** `stats.linear_slope_error([1, 2, 3], 5)`:
  - v2 appends a second "this happened when calling stats.linear_slope_error on line 2".
  - That puts "line 29" (of stats.fm) and "line 2" (of the program) side by side with nothing saying which file
    each is in.
  - v1 gives one clause. Undocumented.
- **#12 Compile time grows faster than linear under `--backend auto`** (v1 is worse in both cases).
  - A list literal of 20 000 numbers takes 3.4 s; 200 000 takes over 40 s (killed). The interpreter needs
    0.05 s and `check` 0.09 s for 20 000 (rob/sz*.fm).
  - A chain of 800 one-line functions (`h_i(x) = h_{i-1}(x) + 1`) takes 46 s.
  - Suggest giving huge literals to the interpreter.
- **#13 `linspace(0, 1, 10^9)` is within the 10⁹ cap, but the process is OOM-killed** (exit 137, no message, 15 GB
  machine).
- **#14 (local multi-user) The build's temporary folder has a predictable name.** link.rs uses
  `/tmp/fermium-build-<pid>`; `create_dir_all` accepts a folder that already exists, and `fs::write` follows
  symlinks. Use a randomly named folder created exclusively.
- **#15 v1-shared input and UX nits:**
  - `ϕ = 1 m` / `φ = 2 m` / `print ϕ + φ` prints `4 m`: ϕ is silently folded into φ. Cyrillic `а` gets a warning.
  - `˚C` (macOS Option-K), `ºC`, `℃`, `𝜋` and `ℓ` aren't mapped. `𝑥` isn't folded to x.
  - REPL `:foo` gives *didn't expect ':' here* instead of "unknown command".
  - Mixed tab and space indentation gives *didn't expect '' here*.
  - `2^10000` prints `∞` with no warning.
  - `import mechanics as m`, then `1 m`, says *m is also your variable m* (m is a module).
- **#16 v1-shared stdlib output:**
  - `nuclear.semf_binding_per_nucleon(56, 26)` prints `1.417×10⁻¹² J`, not ~8.85 MeV.
  - `astro.luminosity_distance` prints metres (2.0×10²⁶ m), while `comoving_distance` prints Mpc.
  - `stats.weighted_mean([1, 2] m, [0.1, 0.2] s)` accepts σ in other units than x.
- **#17 fmt (v1-shared, cosmetic):**
  - pretty→ascii→pretty isn't the identity on 7 examples: `√A`→`√(A)`, `ε_0`→`ε₀`, `∇φ(…)`→`grad(φ)(…)`,
    `∂T/∂t`→`∂ T/∂t`. All still run identically.
  - `--ascii` leaves `kΩ` without a warning; it warns for `θ0`, `ξ0` and `dφ`.
  - fmt converts CRLF to LF and drops the BOM, but `--fix` keeps the BOM.
- **#18 Info (security):**
  - `load` errors echo the file's lines: as root, `load "/etc/shadow"` printed a shadow entry. `check` and the
    language server read load headers too.
  - `fermium check` runs a `use python` module's top-level code (verified with a
    marker file; fmt doesn't). This is documented under "Trust", but worth a line in the editor docs.
  - A `fermium.toml` in any parent folder can silently replace a stdlib module. Modules can't contain
    `use python` (verified), so this is wrong physics, not code execution.
  - Executables embed the absolute build path and the program source.
  - Nothing else runs shell commands: `Command::new` is only `python3` (to find libpython and for the Jupyter
    install's sys.prefix) and `xcrun` on macOS.

**What checked out**
- **stdlib, 67 functions × {right units, wrong units}** (mod/g*.fm, b*.fm): v2 is byte for byte v1 on 133 of 134
  runs; the one exception is #11.
  - Wrong units give v1's messages.
  - Values checked by hand: range 40.8 m, T(60°) 2.15 s, r_s(M☉) 2.95 km, Wien 502 nm, L☉ 3.83×10²⁶ W,
    B(⁵⁶Fe) 495.4 MeV, Q(²³⁸U α) 4.27 MeV, box 0.376 eV, H n=2 −3.40 eV, He⁺ −54.4 eV,
    v_esc 11.19 km/s, remaining 886.
  - `1 h` in `bateman_daughter` warns that h is Planck's constant.
- **Local modules** (mod/loc, mod/m2; 30 programs, all identical to v1):
  - import by name and by path (`"lib/my-springs.fm" as sp`) and nested relative imports;
  - fermium.toml `[paths]`;
  - cycles of 2 and 3 modules and a self-import;
  - a module that prints, `_private` names, name clashes (define vs import, import vs import, a module name
    reassigned), duplicate imports and misspelled modules (with a close-name hint);
  - `/etc/passwd` imported as a module fails cleanly;
  - a unit error inside a module points at the module line;
  - module functions differentiated.
- **REPL** (piped sessions, repl/s1–s3): definitions, errors and recovery, `:vars`, `:help`, `\omega` `\int` `\^2`
  `\sqrt` `\hbar` `\times` expansion, multi-line `for`/`if`, CRLF and BOM input, a 50 000-term line, invalid UTF-8.
  Byte for byte v1 (except that ± works, #6).
- **fmt**: `--pretty`, `--ascii` and `--fix` on all 31 examples plus 116 bootcamp programs (conformance cases whose
  origin is `bootcamp/`), in their own folders:
  - pretty and ascii are idempotent on every one;
  - every formatted program runs identically (stdout and exit code; the only stderr differences are the echoed
    source line);
  - `--fix` is idempotent and keeps behaviour;
  - fmt refuses input it can't parse.
- **Language server** (lsp/drive.py):
  - initialize and capabilities;
  - an error on open is underlined with the right UTF-16 range, also after astral characters;
  - hover gives units (`g`: acceleration [m/s²], `ω`: frequency [1/s], shown in rad/s);
  - `\ome` completes to ω with a textEdit;
  - fixing the error clears the diagnostics;
  - out-of-range positions, unknown URIs, an unknown method (−32601), bad JSON (−32700), incremental edits with
    ranges inside a surrogate pair, past the end, reversed or negative, 20 000 nested parentheses, and a
    shutdown/exit sequence all leave the server alive. v1 answered "Invalid Params" to one of these.
- **Jupyter** (`fermium jupyter install` into a scratch JUPYTER_DATA_DIR, driven with jupyter_client 8.10):
  - cells with results, unit errors (status error, then the kernel recovers), `import`, and ± all work;
  - plots come back as image/png or image/svg+xml display_data;
  - empty and comment-only cells are fine;
  - completion `\ome` gives ω, and is_complete works;
  - the kernel survives the panic in #1.
- **`fermium build`**, 15 programs: examples 01, 03, 06, 08, 13, 17, 18, 26, 28, 40, 41 and 42, a run-time index
  error, modules, and `use python`.
  - Each executable's output (stdout, stderr and exit code, run from the program's folder) is identical to
    `fermium run`.
  - It refuses ± and programs with unit errors, with clear messages.
- **Robustness** (rob/r01–r45, p01–p19): no hang or crash except #1, #12 and #13.
  - Handled cleanly:
    - empty, blank-only and comment-only files;
    - CRLF, lone CR, BOM, BOM+CRLF, tabs;
    - a 200 000-character line and a 100 000-character name (v1 crashes: *LLVM IR parsing error*);
    - 50 000 lines, 1000-deep nested `if`, 5000 functions;
    - 100 000-deep recursion; infinite self-recursion is refused at compile time, and mutual infinite
      recursion gives *ran out of stack* (v1 hangs);
    - a 40th derivative, 1e400, a null byte, invalid UTF-8, zero-width space, NBSP, fullwidth digits,
      combining accents (clear "unexpected character" messages);
    - an RTL override in a comment;
    - `zeros(10^15)` and `linspace(…, 10^12)` refused with a message.
  - Slow but finite: symbolic 6th and 7th derivatives of `exp(sin x cos x x^x)` take 9 s and 60 s (v1 39 s for
    the 6th). The values are right (mpmath: 129.2, 715.6).
- **Security:**
  - `import "../../etc/passwd"` and absolute imports only parse the file;
  - a module can't `use python`;
  - `use python` puts only the program's folder on sys.path (bridge `_import`);
  - built executables don't pick up modules from the working folder;
  - the Jupyter kernel's temporary stderr file has a random name;
  - `fermium check` writes no files.
- **Not tested:** the wasm playground, macOS, VS Code itself (only the server).


**Round 13 status (01:35 UTC):** every finding is assigned to one fix agent (branch claude/v2.5-rt13):
- #1–#2: overflow-checked unit exponents become a one-line error (no panic, no silent wrap).
- #3: plot paths limited to image formats, as in v1.
- #4: v1's Unicode normalisation table ported.
- #5: Jupyter interrupt handled, or the docs corrected.
- Lows: stale docs, `--help`, build refusing to overwrite its own source, the "nested too deeply" hint, a secure temp folder.

## Round 14 (2026-09-27 03:25 UTC): the new v2.5 features (C1, C3, C5, C7)

Independent reviewer. Binary: `origin/claude/v2.5` (d65f724: C3, C7, C5, red-team-13 fixes and C1 merged; C4 not yet).
Oracle: `fermium-legacy`. Programs are in the session scratchpad (rt14/). Nothing was fixed by the reviewer.
Machine note: memory was ~95 % used (parallel agents' test runs, no swap); legacy pytest workers were killed
("node down") in two make check runs at 02:34 and 03:25. That was the environment, not a test failure. After this,
at most two agents run at once, and the legacy suite runs with fewer workers.

**High**
- **#1 A jump at an uncertain parameter silently loses the uncertainty (C7).** `a = 1.0 ± 0.1`,
  `∫ (if x < a then 1 else 0) dx from 0 to 2` prints `1`; it should be 1.00 ± 0.10. The same happens for `sign(a - x)`,
  `floor(x + a)` and `solve x' = if t < a then 1 else 0`. Differentiating under the integral sign gives 0
  almost everywhere. The ±1σ test (D278) sees d₂ ≈ 0 and d₁ ≈ 0 and calls it linear, but never compares the
  linear prediction with the ±1σ difference itself.
- **#2 C, Fortran and `use python` functions silently drop ± from their arguments.** `sq((2.0 ± 0.1) * 1 m)` prints
  `4 m²`. For `use python` this is a regression: v1 errors with "needs a plain number, but got an uncertain value".
- **#3 A v1-valid program prints a different number (C5).** `force(x [m]) = 3 N/m x`, then a later `force(x) = 5 N/m x`:
  v1 prints 3 N then 5 N, but v2.5 keeps choosing the more specific annotated version and prints 3 N twice. This
  breaks "programs that ran under 2.0 print the same".

**Medium**
- **#4** A `: list` parameter only works if the body uses list operations: `s(x: list) = 2`, `s([1, 2])` →
  "s expects x to be a list, but got a plain number". Cause: `call_user` maps over the elements using
  `takes_lists(body)` and ignores the declared kind.
- **#5 (security)** `fermium check` and the language server dlopen the library named in `import c` while checking,
  so its constructor runs just from opening a cloned .fm file in the editor.
- **#6** The Monte Carlo fallback for ODEs gives a separate "nonlinear rest" source per (component, time) and reports
  the MC mean, not f(nominal). So `x(2) - x(1)^2` for decay is 0.029 ± 0.099, not 0. The fallback also triggers for
  plain decay (k ± 10 %) and every pendulum (the test at a turning point, where d₁ ≈ 0).
- **#7** `E(f [Hz]) = h f` and then `E(ω [rad/s]) = ħ ω`: the same dimension, so the second silently replaces the
  first, and `E(1 GHz)` is wrong by 2π. This is v1's rule, but the new docs invite such overloads. It needs at least
  a warning.

**Low**
- **#8** `∂/∂y f(1, 3)` looks only at the last version of f; `(∂/∂y f)(1, 3)` works.
- **#9** Messages and gaps:
  - a. `n: int` and `x: vector` give nonsense "[int]" / "[vector]" hints.
  - b. An array index of 1.5 says "out of range", not "must be a whole number", and has no caret.
  - c. Arrays of ± values aren't supported.
  - d. A list-unknown solve with ± isn't in the CHANGES "still errors" list.
  - e. The rk4 missing-step hint uses the wrong end time.
  - f. `names = []` followed by `names = t` is refused.

**Held up:** ± through vectors, matrices, lists, integrals (including infinite and reversed limits) and ODEs, with
correlations kept (v − v = 0) and σ matching the analytic values; `propagate montecarlo` around solve; the dispatch
examples, kinds, generic re-dispatch, modules and the REPL; the LLVM GC under FERMIUM_GC_STRESS=1 (always matched
the tree-walker); list-unknown solves (Bateman to 1e-5, circular orbit); N-d array errors; C and Fortran unit
conversion, int checks, stack-spilled arguments, Fortran mangling, SEMF, `fermium build` with C, and shell
metacharacters in library paths (no injection). No Rust panics.

Status: fixes in progress (agent on claude/v2.5-rt14).
