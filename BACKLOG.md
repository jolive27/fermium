# BACKLOG

Tier 5 ideas and anything cut from earlier tiers. Pick the highest-value item first. The Phase 2 feature list in CLAUDE.md ("Night plan, part 2") comes before the items here.

## Bugs first (silent wrong answers; see PROGRESS "Known issues" and dev-notes/notes/bugs-adversarial.md)
- [x] A54: the integration variable must be in scope while the integrand is read. `∫ 1/u du`, `∫ 1/s ds` and `∫ 2/L dL` are silently wrong (u, s, L are read as units).
- [ ] A narrow peak exactly at a subdivision point gives half the integral: `∫ exp(-(x-1000)^2*100) dx from 0 to 2000` = 0.0886 (true 0.177).
- [ ] A3 leftover: a narrow peak in a huge finite range (`∫ exp(-x²) dx from -1e6 to 1e6`) gives 0.
- [ ] A56 leftover: strong singularities away from 0 (`|x - 0.3|^-0.6` and worse) are rejected. Needs QUADPACK-style ε extrapolation.
- [x] A55: `max`/`min` of an ODE solution should refine with the dense interpolant.
- [x] `2 G` / `2 h` silently mean 2 × G_Newton / 2 × Planck. Consider a warning when a constant name follows a number and a unit of that spelling exists elsewhere (gauss, hour).
- [ ] `sqrt(-1)` → NaN and `factorial(-1)` → ∞ silently: a runtime warning or error?
- [x] A23 (plot axes with mixed units), A24 (CSV traceback noise), A37 (decay below 1e-300).
- [x] A50 (push on a loaded column).
- [ ] `fit` reports a wrong standard error for a parameter with a tiny SI value (found in research/level_density, C8; Fermium 1.5 does the same): `xs = [10, 20, 30, 40, 50]`, `ys = [1.3, 2.4, 3.9, 5.1, 6.2] [1/MeV]`, `k = 8 MeV`, `fit y = x / k to table(x = xs, y = ys)` gives `k = 1.273×10⁻¹² J (standard error 1.7×10⁻¹⁰ J)`, i.e. 7.95 ± 1000 MeV; the plain-number fit `fit y * 1 MeV = x / q` gives 7.948 ± 0.089. The value is right, the error (and `err(k)`) is not: probably an absolute finite-difference step in the Jacobian. Also: the report is in J, not in the unit of the starting value.

## High value
- [ ] C6 leftover (D318): blackbody is ≈ 1.5× Julia. Batch the integrand: compile a version of a pure scalar lambda that evaluates the 15 Gauss–Kronrod nodes of a panel in one call (the divisions vectorize, no per-node call and error-flag check), with fermium-runtime's quad keeping v1's adaptive algorithm and its order of sums exactly.
- [ ] C6 leftover (D313): find why removing the collector safe points from nbody's list-writing loops makes it ≈ 15 % slower, then drop them there too (it would let element-wise list loops in the main program vectorize).
- [ ] Re-run the benchmarks; the adaptive spring disagreed with Julia at 00:09 (before the A15 error-norm change). Update RESULTS.md and the README table.
- [ ] Raise test coverage toward 95% (91% at 01:10).
- [ ] Better symbolic simplification. (Done: `d/dt (3 t^2)` and `∫ x dx` now print readable labels.)
- [ ] Better dense output for DP45: use the method's own 4th-order interpolant instead of cubic Hermite.
- [ ] Garbage collection (or reference counting) for lists.

## Features
- [ ] Matrices with units, and lists of vectors (vectors are done: D24).
- [x] Uncertainties (§3.7): done in M4 (D120–D124). Open: native code, REPL/Jupyter, weighted fits, uncertain vectors.
- [ ] Browser playground (Pyodide can't run llvmlite's JIT; could use the reference interpreter, `fermium run --interp`).
- [ ] Derivatives and partial derivatives of multi-line functions.
- [ ] Stiff ODE solver (implicit method). Events and root finding in `solve` ("stop when x < 0").
- [ ] `solve` with a parameter sweep.
- [x] `fermium build` for programs that use `plot`, `load` or `fit` (D31).
- [ ] `stdlib/` as the spec lays it out (constants, units database, prelude), or record the deviation in DECISIONS.md.

## Done (moved from above)
- [x] Fuzzing the lexer and parser (`tests/test_fuzz.py`).
- [x] Property tests for units, conversions and `fmt` round-trips (`tests/test_properties.py`); cross-checks against SymPy/SciPy (`tests/test_numerics.py`, `tests/test_differential.py`).
- [x] Vectors: `<3, 4> m/s`, `|v|`, dot and cross products, vector ODEs.
- [x] AOT standalone executables (`fermium build`, D25).
- [x] `print x to 6 digits`.
- [x] `plot` with a log scale (`with log y`).

## Known traps (by design, documented)
- `2 g h` means 2 grams times h (a unit right after a number, D7). Fermium warns only if you have defined your own `g`. Write `2*g*h` or `2 g_n h`.
- Lists are never freed while a program runs (no GC). Fine for scripts, bad for very long loops that allocate.
- A bundled correctly-rounded libm (CORE-MATH or similar) so printed last digits are identical on every platform (D271); needs re-checking every golden against v1 on Linux.
- (v2.5, red team 12 #2) Warn when a spaced `/` divides by a function call of the unknown or of anything (`θ'' = -g / L sin(θ)` is -g/(L sin θ) under D8; D34's warning only covers the unknown itself): extend D34 to calls (sin, cos, exp, …). A new warning changes stderr, so it's a language change for v2.5 with conformance updates.
- (red team 12 #8) An RK4 right side that leaves its domain prints NaN silently (v1 does too): warn once, as the adaptive solvers do.

## Research track (C8) frictions (found writing research/ 12 onwards; each README's "Friction" section has the context)
- [ ] No jansky (`Jy`) and no user-defined units: radio/CMB intensities (MJy/sr) need a variable `MJy = 1e-20 W/(m² Hz)`, and a CSV header can't say `[MJy/sr]` (cmb_firas).
- [ ] `fit` has no weights and no quiet mode: a χ² fit divides both sides by the error (works, but the report's "rms residual" is then √(χ²/N)); every fit prints a report, so six fits in a loop print six (cmb_firas, mass_luminosity, supernova_hubble).
- [ ] The fit report writes a subscripted parameter `G₀` as `G_0` (cmb_firas).
- [ ] Values read from a CSV carry no significant figures: `out = 0 fm` then `out = Rs[i]` prints `3.48 fm` for 3.4776 (charge_radii); elements of a list literal print with 2 digits too (`[0.179, 1.05][k]` → `0.18`, `1.1`; mass_luminosity).
- [ ] A list filled by `push` with fm² values is plotted in m² unless the plot says `in fm²` (charge_radii).
- [ ] Plot titles: the PNG font has no en dash `–` or `⟨ ⟩` (drawn as `?`) (charge_radii).
- [ ] Error hint bug: for `1 cm³/(mol s)` with a variable `s`, the hint suggests `1 [cm³/(mol s]` (closing parenthesis lost) (gamow_window).
- [ ] `where` isn't accepted after `solve … for x from a to b` (gamow_window).
- [ ] `vs` as a function name gives only "didn't expect 'vs' here" (alpha_decay).
- [ ] Names with a combining dot (`ν̇`, `ν̈`, `ẋ`) are rejected as an unexpected character (pulsar_spindown).
- [ ] A function whose last line is `if c then a else b` "never returns a value" (pulsar_spindown).
- [ ] A negative plot axis range (`y from -22 to -9`) is refused as "not constants" (pulsar_spindown).
- [ ] `10^list` is an error ("the exponent must be a number"); element-wise exponentiation needs a loop (mass_luminosity).
- [ ] A superscript can't end a function name: `σ²(U, a, A) = …` is a parse error (level_density).
- [ ] C8 follow-up: an r-process abundance-pattern reproduction (a spec example) was not done. No machine-readable solar r-residual table could be downloaded: VizieR has Bisterzo et al. 2014 s-process fractions (J/ApJ/787/10) but no isotopic solar abundances to go with them; Goriely 1999 (J/A+A/342/881) holds only β-rate error factors; the Asplund et al. 2009 table could not be extracted reliably from the arXiv PDF. Next step: NIST isotopic compositions × a downloadable elemental solar table × the Bisterzo s-fractions, then compare the r-peaks (A ≈ 130, 195) with the N = 82, 126 waiting points from AME2020.
