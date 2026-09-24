# PROGRESS

_Last updated: 2026-09-24 23:20 UTC_

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

## In progress
- Test suites: parser, dimension checker (≥50 reject / ≥50 accept), fmt round-trip, REPL (agent).
- Examples (25) + gallery + rosetta (agent).
- Bootcamp (agent).
- Numeric validation against SciPy/SymPy (me).
- Fermium benchmark programs + performance work (me).

## Next
- Fix the bugs agents report in `notes/bugs-*.md`.
- Tier 3: `benchmarks/fermium/*.fm`, run `benchmarks/run.py`, optimize.
- VS Code extension (`editors/vscode`).
- README with gallery.
- MORNING_REPORT.md at ~10:45 UTC.

## Blocked
- (none)
