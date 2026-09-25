# PROGRESS

_Last updated: 2026-09-25 01:10 UTC_

**How to resume:** read CLAUDE.md, DECISIONS.md, BACKLOG.md and `git log`, run `./check.sh`, then continue from **Next**.

## Status at a glance (01:10 UTC)
- **Tests:** 1753 passed, 7 skipped, 4 xfailed (strict xfails of known bugs), in about 90 s. Coverage 91%.
- Every ```` ```fermium ```` block in the docs, bootcamp and README runs in the tests, and every bootcamp output box is compared with the real output (`tests/test_bootcamp_outputs.py`).
- 25 examples, all tested; 7 rosetta programs in Fermium, Julia and Python.

## What works
- **Units:** checked at compile time and erased before codegen. 7 base dimensions with rational exponents, SI prefixes, physics and astronomy units, °C/°F as absolute temperatures, `in` conversions, CODATA 2022 constants.
- **Language:** variables, one-line and block functions, `if`/`else`, `for` (with units and `step`, and `to inf` with `break`), `while`, lists, text, `where`, 2-D and 3-D vectors. Variables that may be unset after an `if` or loop are compile errors.
- **Calculus:** symbolic derivatives (`x'`, `d/dt`, `dx/dt`, `d²x/dt²`, `∂/∂x`) of one-line functions and formulas; definite integrals (adaptive Gauss–Kronrod, compiled; infinite ranges at any length scale; integrable singularities at the ends and at 0); indefinite integrals through SymPy; `solve` with RK4 or adaptive DP45 (`tolerance`, `using rk4|rk45`), systems and vector unknowns.
- **Data:** `load` CSV with units, `fit` with units and standard errors (`examples/data/pendulum.csv` gives g = 9.818 m/s²), `plot` PNGs with unit labels.
- **Tools:** `fermium run/check/fmt/doctor/build`, the REPL with `\name` Tab completion and history saved in `~/.fermium_history`, the VS Code extension (highlighting and `\name` completion). `fermium doctor` reports the C compiler that `fermium build` needs.
- **Speed:** within 2× of Julia's compute time on 4 of the 5 benchmarks (benchmarks/RESULTS.md, 00:09 UTC run); whole-process times are shorter than Julia's.

## Partial
- **Adaptive-ODE benchmark:** results disagree with Julia and SciPy in the 00:09 RESULTS.md (Fermium took 2066 steps against 3713). The solver's error norm changed after that run (A15), so it needs a re-run.
- **Derivatives and ∂:** only of one-line functions and formulas.
- **`fermium build`:** no `plot`, `load` or `fit`; needs a C compiler.
- **Vectors:** no matrices and no lists of vectors.
- **Uncertainties:** `±` is reserved and gives a friendly error; not implemented (docs/uncertainties.md).
- **VS Code:** live errors, hover with units and `\name` completion come from the language server (`fermium lsp`, tested over stdio); the extension itself is tested under Node with a stand-in for the VS Code API, not in a running VS Code.
- **`stdlib/`:** empty. Constants and units live in `fermium/constants.py` and `fermium/units.py`.

## Known issues
Open adversarial bugs (details in notes/bugs-adversarial.md; A1, A2, A4–A55 are fixed; A3 and A56 are partly fixed):
- **Narrow peak at a subdivision point (silent wrong answer, AUDIT §4.5):** `∫ exp(-(x-1000)^2*100) dx from 0 to 2000` gives exactly half (0.0886 against 0.177). A peak at 1000.5 is right.
- **A3 (partly fixed):** a narrow peak in a huge finite range can be missed: `∫ exp(-x²) dx from -1e6 to 1e6` prints 0.
- **A56 (partly fixed):** strong interior singularities away from 0 (`|x - 0.3|^-0.8`, and even `^-0.6`) are rejected as "doesn't converge".
- **Traps by design:** `2 G` is 2 × the gravitational constant (gauss is `gauss`), `2 h` is 2 × Planck's constant (both warn), and `2 g` is 2 grams (warns if you defined your own `g`).
- `sqrt(-1)` is NaN and `factorial(-1)` is ∞, silently.
- No garbage collection: list memory is only freed when the program ends.

## Done
- 22:18 Read the spec. Wrote CLAUDE.md.
- 22:40 Julia 1.12.7 is installed in `.tools/julia`. Benchmark baselines for Julia, pure Python and NumPy are in `benchmarks/` (subagent).
- 23:00 Compiler pipeline works end to end:
  - lexer, parser and checker (dimension unification)
  - typed IR, LLVM codegen and MCJIT
  - runtime callbacks
- 23:00 The §3.1 snippet runs:
  - pendulum g = 9.70 m/s²
  - `in ft/s²`
  - ω = √(k/m)
  - symbolic `d/dt x` and `x''`
  - `∫ F(x) dx` = 1.0 J
  - damped-spring `solve` with DP45
  - `plot x vs t`
  - `load` + `fit` gives g ≈ 9.8 m/s² (9.818 m/s² with `examples/data/pendulum.csv` as of 01:10)
  - `where`
- 23:10 CLI: `run`, `check`, `fmt --pretty/--ascii`, `doctor`. REPL with `\name<TAB>`.
- 23:10 Wrote DECISIONS.md (D1–D22) and docs/reference.md. `check.sh` and Makefile in place.

- 23:00 Benchmarks: `benchmarks/fermium/*.fm` written. First full run: compute time vs Julia is 1.28× on nbody, 1.88× on RK4, 1.3× on blackbody and 1.06× on the unit loop. Fermium's wall time beats Julia's everywhere. (spring_adaptive uses a different error norm, so its step count differs; see RESULTS.md.)
- 23:05 Numeric validation tests against SymPy/SciPy. Spec §3.1 snippet tests (file + REPL).
- 23:10 VS Code extension (grammar + `\name` completion, tested with a mocked vscode module under node).
- 23:15 Fixed about 30 agent-reported bugs (see notes/bugs-*.md). Mutation fuzzer: 6000+ mutated programs, no crashes.
- 23:20 Coverage 87% (cli/doctor are only covered by subprocess tests).

- 23:25 Vectors (`<3, 4> m/s`, `|v|`, `·`, `×`, `.x`) and vector ODEs (D24).
- 23:30 All three agents finished:
  - tests: 718 tests, 18 bugs found, all fixed
  - examples: 25 programs, 20 gallery plots, rosetta page with 7 programs in 3 languages
  - bootcamp: lessons 0–10 plus a symbols lesson, cheat sheet, troubleshooting, solutions, 149 tested code blocks
- 23:35 Fixed the bootcamp's 29 reported issues, except B27 (solve continuation-line indentation) and B29 (per-element units in lists). Most important: `20 m/s / g` now divides by your g (D7).
- 23:40 README with gallery. 1066 tests passing.

- 00:05 **New user direction received.** Tiers 1–4 are complete. The plan is now Phases 1–4 (see CLAUDE.md, "Night plan, part 2"). The end time moved to 13:00 UTC (9 AM ET), and the report is due at 12:30 UTC.
- 00:05–01:00 Phase 1 audit: AUDIT.md written; most AUDIT §4 items and most of A1–A53 fixed (see the hourly log and `git log`).
- 01:10 Docs pass: README, reference, DECISIONS and the VS Code README corrected against the code; reference §19 "Known limitations"; bootcamp output boxes refreshed and now tested; `doctor` checks for a C compiler.

## In progress
- Phase 2 features (Jupyter kernel first).

## Next
- Re-run the benchmarks and update RESULTS.md and the README speed table (the adaptive row still says "spot check").
- The open bugs above, silent wrong answers first (A54, the half-peak).
- Phase 2: Jupyter kernel, language server, matrices, gradient/divergence/curl, browser playground.
- Phase 3: the textbook gauntlet.
- MORNING_REPORT.md at 12:30 UTC.

## Blocked
- (none)

## Hourly log
- 00:31 UTC. **Phase 1 (audit)**:
  - Done: the clean-clone install, `make check`, all examples and the benchmark re-run; AUDIT.md written.
  - Fixed so far: A5–A11, A15, A20–A22, A25, A27–A30 and most of the AUDIT §4 items.
  - Next: the remaining AUDIT items (docs claims, bootcamp output boxes, stale PROGRESS/BACKLOG) and the rest of the adversarial list. Then Phase 2 (Jupyter kernel).
- 01:00 UTC — Phase 1 audit fixes: quadrature rewritten (A3/A44/A51/A56), A33/A34/A37/A41/A43/A55 fixed by me; checker + calculus agents fixed A16–A19, A26, A31–A32, A35, A38, A40, A42, A45–A49, A52 (merged). 1561 tests pass, 3 xfail. Agents now on A23/A24/A54 and AUDIT §2 docs.
