# PROGRESS

_Last updated: 2026-09-25 11:30 UTC_

**How to resume:** read CLAUDE.md, DECISIONS.md, BACKLOG.md and `git log`, run `./check.sh`, then continue from **Next**. The whole night is summarised in MORNING_REPORT.md.

## Status at a glance (11:30 UTC)
- **Tests:** 3612 passed at the last full run, plus 2 strict xfails (red team 7 #2 and #5, documented limitations). About 15 min on 4 cores.
- **Every example is checked:**
  - every ```` ```fermium ```` block in the docs, bootcamp, README and SHOWCASE runs in the tests;
  - every bootcamp output box is compared with the real output;
  - the 31 examples, 11 research reproductions and 81 gauntlet problems are all tested.
- **Moonshots:** all 8 done (M4 uncertainties only in the interpreter; M5 measured on a quiet machine: see MORNING_REPORT.md).
- **Red team:** 7 rounds, 88 findings: 83 fixed, 2 partly fixed (round 1 #1, #7), 3 documented as by design or known limitations (round 4 #16, round 7 #2 and #5).

## Phase 2 (all six items done, 02:00 UTC)
1. **Jupyter kernel:** `fermium jupyter install`; inline plots, errors, `\name` completion; `examples/notebook.ipynb` (tests/test_jupyter.py runs it with nbclient).
2. **Language server + VS Code:** `fermium lsp` (pygls): live error/warning underlines, hover shows units, `\name` completion; the extension starts it (tests/test_lsp.py drives it over stdio; the extension is tested under Node with a VS Code stand-in).
3. **Vectors and matrices with units:** matrices 1×1–4×4 (`det`, `inverse`, `solve_linear`, `Mᵀ`), 4-vectors, per-component units (`<1 m, 2 m/s>`), a 3-D orbit example (tests/test_matrices.py).
4. **∇:** `∇f`, `∇·F`, `∇×F`, `∇²f` (ASCII `grad/div/curl/laplacian`), symbolic, with units (tests/test_vector_calculus.py).
5. **Browser playground:** `web/` (Pyodide + the reference interpreter, bootcamp examples preloaded), verified in headless Chromium (tests/test_playground.py).
6. **`fermium build`:** native executables now also support `load`, `fit` and `plot` (SVG) (tests/test_aot.py).

Also new: algebraic equations `solve lhs = rhs for x from a to b` (D32).

## Gauntlet (Phases 3 and 7)
- 81 problems: 31 in pass 1, 30 in pass 2 and 20 in pass 3 (graduate level). All are checked against closed forms, SciPy or published values.
- 96 friction items in gauntlet/FRICTION.md, 86 fixed in the language.

## What works
- **Units:** checked at compile time and erased before codegen. 7 base dimensions with rational exponents, SI prefixes, physics and astronomy units, °C/°F as absolute temperatures, `in` conversions, CODATA 2022 constants.
- **Language:** variables, one-line and block functions, `if`/`else`, `for` (with units and `step`, and `to inf` with `break`), `while`, lists, text, `where`, 2-D and 3-D vectors. Variables that may be unset after an `if` or loop are compile errors.
- **Calculus:** symbolic derivatives (`x'`, `d/dt`, `dx/dt`, `d²x/dt²`, `∂/∂x`) of one-line functions and formulas; definite integrals (adaptive Gauss–Kronrod, compiled; infinite ranges at any length scale; integrable singularities at the ends and at 0); indefinite integrals through SymPy; `solve` with RK4 or adaptive DP45 (`tolerance`, `using rk4|rk45`), systems and vector unknowns.
- **Data:** `load` CSV with units, `fit` with units and standard errors (`examples/data/pendulum.csv` gives g = 9.818 m/s²), `plot` PNGs with unit labels.
- **Tools:** `fermium run/check/fmt/doctor/build`, the REPL with `\name` Tab completion and history saved in `~/.fermium_history`, the VS Code extension (highlighting and `\name` completion). `fermium doctor` reports the C compiler that `fermium build` needs.
- **Speed:** within 2× of Julia's compute time on 4 of the 5 benchmarks (benchmarks/RESULTS.md, 00:09 UTC run); whole-process times are shorter than Julia's.

## Partial (checked 11:30 UTC)
- **Uncertainties (±):** run in the reference interpreter. They don't work in `fermium build`, the REPL or Jupyter, and there are no vectors of uncertain values (D122).
- **Python-backed numerics:** eigenvalue problems, PDEs, stiff solvers (`using radau`/`bdf`) and FFT use NumPy/SciPy. `fermium build` refuses the first three.
- **Matrices:** up to 16×16, with one unit per matrix. There are no lists of vectors, and `solve` can't take a list of unknowns.
- **Derivatives, ∂ and ∇:** only of one-line functions and formulas (helpers may be nested one-liners).
- **VS Code:** the extension is tested under Node with a stand-in for the VS Code API, not in a running VS Code.
- **Browser playground:** runs the reference interpreter, which is slower than native code.

## Known issues
- **Integrals:**
  - a narrow feature that is only partly sampled can be missed or counted half, with no warning; only the all-zero case warns (D110);
  - a result at the integrand's rounding level shows more figures than are right (red team 7 #5).
- **1-D PDEs right after a jump** in the initial or boundary data are inaccurate until diffusion reaches one grid cell (red team 7 #2).
- **Strong interior singularities away from 0** (`|x - 0.3|^-0.8`) fail. This is documented, with a workaround.
- **Significant figures of sums** use the most precise operand, so `1.00 m - 0.999 m` over-claims (D95, D11).
- **Unit names that are also variables** (`2 g`, `8 K`, `2 b`) are caught by a family of rules (D7, D130, D170, D171, D180, D203, D222, D231, D232). They work, but a single unified rule would be easier to learn.
- **Memory:** there is no garbage collection; list memory is freed only when the program ends.

## In progress (11:30 UTC)
- Final full test run; MORNING_REPORT.md and SHOWCASE.md final check.

## Next
- 12:30 UTC: stop new work; final MORNING_REPORT.md; push by 13:00 UTC.
- After the run: see "What I would do next" in MORNING_REPORT.md.

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

## Moonshot status (final, 11:30 UTC)
- M1 natural units: done (D60). M2 dimensional analysis: done (D70, bootcamp lesson 11).
- M3 numerics: done (D42, D80–D83, D131, D160, D190, D233).
- M4 uncertainties: done, interpreter only (D120–D124, bootcamp lesson 12).
- M5 performance: done (D150–D152). On a quiet machine it matches or beats Julia on 3 benchmarks and is slower on 3 (MORNING_REPORT.md).
- M6 Python interop: done (D140–D142). M7 modules and stdlib: done (D100–D103). M8 self-hosting: done (unit database).

## Blocked
- (none)

## Hourly log
- 00:31 UTC. **Phase 1 (audit)**:
  - Done: the clean-clone install, `make check`, all examples and the benchmark re-run; AUDIT.md written.
  - Fixed so far: A5–A11, A15, A20–A22, A25, A27–A30 and most of the AUDIT §4 items.
  - Next: the remaining AUDIT items (docs claims, bootcamp output boxes, stale PROGRESS/BACKLOG) and the rest of the adversarial list. Then Phase 2 (Jupyter kernel).
- 01:00 UTC — Phase 1 audit fixes: quadrature rewritten (A3/A44/A51/A56), A33/A34/A37/A41/A43/A55 fixed by me; checker + calculus agents fixed A16–A19, A26, A31–A32, A35, A38, A40, A42, A45–A49, A52 (merged). 1561 tests pass, 3 xfail. Agents now on A23/A24/A54 and AUDIT §2 docs.
- 02:00 UTC — Phase 1 closed (all audit items fixed or documented; A56 off-zero singularities and the mid-range half-peak are documented limitations). Phase 2 items 1–6 all done and merged. Gauntlet first pass: 31 problems, 38 friction items, 7 fixed; parser/solve/calculus friction agents running. 2051 tests pass.
- 03:05 UTC — Gauntlet second pass merged (30 harder problems; 61 total). Friction: 59 logged, 35 fixed. Fixed E9 (nested-integral capture, wrong answer) and A5 (ODE first step, wrong answer) myself. Higher-order functions merged. 2254 tests pass.
- 04:05 UTC — Review priorities in progress: (1) complex numbers agent running; (2) unit-after-number rule revised (error when a colliding unit is combined with other factors, warning when alone; D7), bootcamp updated; (3) integral-reliability warnings queued behind the numerics agent; (4) PROGRESS Partial rewritten. Moonshots: M1 natural units and M2 dimensional analysis merged; M3 numerics and M7 modules running. Research: #1 SEMF/AME2020 done, batch 2–7 running. Red team round 1: 10 findings, fix agent running. Gauntlet friction 59 logged / 47 fixed.
- 05:00 UTC — User request done: 3 significant figures by default when precision is ambiguous (display only; D11 with refinements: whole numbers exact, literals and loop grids as written, one style per list/vector, `to N digits` on lists). ~140 older tests updated to say `to N digits` where they check accuracy. Staged merges (branch merge-agents, tests green): M7 modules + stdlib, M3 (seeded RNG, FFT, eigenvalue problems, PDEs), research reproductions 2–8, research frictions #60–#65, complex numbers (review priority 1). Found and fixed a merge crash (PDE solutions left the new rhs pointer uninitialised). Agents running: red team round 2, M4 uncertainties.
- 06:05 UTC — Merged: M4 uncertainties (±, correlations, Monte Carlo, uncertain fits; interpreter-only), M6 Python interop (use python …, fermium.compile), research #3 BBN (Y_p 0.2423, D/H 2.60e-5, ⁷Li/H 5.1e-10; SciPy agreement 3e-6), red-team round 2 (14 findings, all fixed: D130–D134), review priority 3 (zero-integral warning, D110). README feature section. 3202 tests pass. Running: M5 performance, Phase 7 graduate gauntlet, research frictions (radau atol, plot axes), red team round 3.
- 07:00 UTC — Merged: Phase 7 graduate gauntlet (20 problems, all passing; frictions #66–#81 logged, with 3 silent wrong answers now being fixed), research frictions #82–#86 (absolute ODE tolerances so the full BBN network starts at 10 MeV, plot ranges/labels/reversed axes, element-wise max/min), red team round 3 (15 findings; #9 fixed here: a power of ten when rounding would leave non-significant zeros; the rest in a fix agent). SHOWCASE updated (uncertainties, eigenvalue problems). 3277 tests pass. Running: M5 performance, gauntlet-3 bug fixes, red-team round 3 fixes.
- 07:45 UTC — Merged: M5 performance (integer loop counters, `parallel for` with deterministic sums, fair benchmarks; matches or beats Julia on 4 rows on the loaded machine, re-measure pending), red-team round 3 fixes (all 15), gauntlet-3 bug fixes (#66–#79, including 3 silent wrong answers). MORNING_REPORT draft, cheat sheet updated. 3434 tests pass. Running: red team round 4 (false positives of tonight's new rules), remaining open frictions (matrices >4×4, list fits, eigenfunction accuracy).
- 08:35 UTC — Merged: red-team round 4 display fixes (integral rounding noise is 0, SI echo figures, written whole numbers; ties documented), open frictions #51/#56/#59/#70/#75/#80/#81 (matrices to 16×16, fourth-order eigenfunctions, list fits, nested helpers), research #10 nuclear shell model (magic numbers 2, 8, 20, 28, 50, 82, 126 from Woods–Saxon + spin–orbit) and #11 recombination (z_* = 1089.6 vs Planck 1089.9). 3495 tests pass. Running: red-team round 4 fixes, shell/recombination frictions.
- 09:30 UTC — Merged: red-team round 4 fixes (all 18: D200–D209, including 3 silent wrong answers and 6 false positives from tonight's new rules), round 5 (tools: REPL/Jupyter rollback after errors, Jupyter/playground run-time warnings, d/dt f(point), LSP UTF-16 ranges, bootcamp transcripts; all 19 fixed, D220–D223). Running: shell/recombination frictions. Next: red team round 6 (~10:00), quiet-machine benchmarks (~11:30), final report (12:30).
- 10:30 UTC — Red team round 6 (final): 7 silent wrong answers in the newest code; #1–#4 were display/step conveniences added tonight, reverted (D230); #5–#7 in a fix agent. Found and killed a stray process that had used one core since 00:27; benchmarks re-measured on the quiet machine: Fermium matches/beats Julia on 3 (nbody 0.95×, spring_adaptive 0.86×, forces 0.65× with 4 threads), slower on 3 (RK4 1.93×, blackbody 1.27×, unit loop 1.54×); faster in whole-process time on all. Merged: round 5 fixes (19), research frictions #87–#96. MORNING_REPORT complete except final counts.
- 11:30 UTC — Red team round 6 #5–#7 fixed (D231–D233). Round 7 (cut short by a container restart; findings salvaged): #1 eigenpair symmetrisation on an asymmetric well, #3 Rutherford README, #4 sums lost significant figures (293.15 K + 0.5 K → 290 K) fixed; #2 (very early PDE times) and #5 (rounding-level integrals) documented. Found that the installed editable package pointed at an agent worktree since ~09:30; reinstalled. README tour comment fixed. Final report next.
- 12:30 UTC — New work stopped. MORNING_REPORT.md final (gauntlet counts per topic, top 10 frictions, benchmarks, red-team tally, weaknesses); coverage run in progress. PR description updated.
