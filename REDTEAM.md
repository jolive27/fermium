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
