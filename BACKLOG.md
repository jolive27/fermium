# BACKLOG

Tier 5 ideas and anything cut from earlier tiers. Pick the highest-value item first. The Phase 2 feature list in CLAUDE.md ("Night plan, part 2") comes before the items here.

## Bugs first (silent wrong answers; see PROGRESS "Known issues" and notes/bugs-adversarial.md)
- [x] A54: the integration variable must be in scope while the integrand is read. `∫ 1/u du`, `∫ 1/s ds` and `∫ 2/L dL` are silently wrong (u, s, L are read as units).
- [ ] A narrow peak exactly at a subdivision point gives half the integral: `∫ exp(-(x-1000)^2*100) dx from 0 to 2000` = 0.0886 (true 0.177).
- [ ] A3 leftover: a narrow peak in a huge finite range (`∫ exp(-x²) dx from -1e6 to 1e6`) gives 0.
- [ ] A56 leftover: strong singularities away from 0 (`|x - 0.3|^-0.6` and worse) are rejected. Needs QUADPACK-style ε extrapolation.
- [x] A55: `max`/`min` of an ODE solution should refine with the dense interpolant.
- [x] `2 G` / `2 h` silently mean 2 × G_Newton / 2 × Planck. Consider a warning when a constant name follows a number and a unit of that spelling exists elsewhere (gauss, hour).
- [ ] `sqrt(-1)` → NaN and `factorial(-1)` → ∞ silently: a runtime warning or error?
- [x] A23 (plot axes with mixed units), A24 (CSV traceback noise), A37 (decay below 1e-300).
- [x] A50 (push on a loaded column).

## High value
- [ ] Re-run the benchmarks; the adaptive spring disagreed with Julia at 00:09 (before the A15 error-norm change). Update RESULTS.md and the README table.
- [ ] Raise test coverage toward 95% (91% at 01:10).
- [ ] Better symbolic simplification. (Done: `d/dt (3 t^2)` and `∫ x dx` now print readable labels.)
- [ ] Better dense output for DP45: use the method's own 4th-order interpolant instead of cubic Hermite.
- [ ] Garbage collection (or reference counting) for lists.

## Features
- [ ] Matrices with units, and lists of vectors (vectors are done: D24).
- [ ] Uncertainties (§3.7): the `NumTy` flavour `{value, sigma}` with first-order propagation. `±` is already reserved in the lexer and parser.
- [ ] Browser playground (Pyodide can't run llvmlite's JIT; could use the reference interpreter, `fermium run --interp`).
- [ ] Derivatives and partial derivatives of multi-line functions.
- [ ] Stiff ODE solver (implicit method). Events and root finding in `solve` ("stop when x < 0").
- [ ] `solve` with a parameter sweep.
- [ ] `fermium build` for programs that use `plot`, `load` or `fit`.
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
