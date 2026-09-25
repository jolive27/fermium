# PROGRESS

_Last updated: 2026-09-24 23:40 UTC_

**How to resume:** read CLAUDE.md, DECISIONS.md, BACKLOG.md and `git log`, run `./check.sh`, then continue from **Next**.

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
  - `load` + `fit` gives g = 9.806 m/s²
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

- 00:05 **New user direction received.** Tiers 1–4 are complete. The plan is now Phases 1–4 (see CLAUDE.md, "Night plan, part 2").

## In progress
- Tier 5 hardening.

## Next
- Property tests (hypothesis) for unit algebra and conversions.
- Coverage toward 95% (in-process CLI/doctor tests).
- Possibly: AOT `fermium build` (native executable via llvmlite object code + a small C runtime).
- Re-run benchmarks on a quiet machine (~10:00 UTC) and update RESULTS.md.
- MORNING_REPORT.md at ~10:45 UTC.

## Blocked
- (none)
