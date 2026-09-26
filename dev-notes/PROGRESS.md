# PROGRESS — run 2 (Spec 1.5 → 2)

_Plan: `dev-notes/FERMIUM_SPEC_V1.5_V2.md`. End: Sun 2026-09-27 13:00 UTC (9 AM ET). Report: `dev-notes/RUN_REPORT.md` by 12:30 UTC Sunday._
_The first run's log is `dev-notes/PROGRESS_v1.md`._

## Status
- **Phase:** B (v2, Rust), branch `claude/v2-rust`. Phase A done: v1.5 frozen at `ccdb288` (D263).

## Done
- 22:40 UTC Fri: branch `claude/v1.5` created (D234); hourly check-in Routine set; CLAUDE.md rewritten for this plan; dev logs moved to `dev-notes/` (A9.3); README note and LICENSE name per A9.1–A9.2.

- 22:55 UTC: CI workflow (Linux on push; macOS on PRs/dispatch; concurrency cancels old runs). A0 triage: `dev-notes/OPEN_ITEMS.md` (111 rows; 22 fix in A).
- 23:25 UTC: **A1 done** (D235): the three-sentence unit rule, spacing-free; `fmt --fix`; LSP quick fix; migration (25 edits, values unchanged); Appendix 1 tests (lesson2's `a = 3 m` conflicts with the table: the rule wins, see D235). make check 3688 passed.

- 23:48 UTC: **A2 done** (D236): pure-number fractions are coefficients; every .fm program's output unchanged vs v1.
- 00:15 UTC: A3 messages (D237): natural-units hint, the g message and the g₀ leak, conversion hints, REPL terminal-command hint, uncertain-vector crash, negative literal domain errors; R1–R3/R5–R7 regression tests.

- 01:35 UTC: A3 committed; **A6 and A8.3 merged** (agent: doctor's one install line, absolute plot paths, pendulum fit curve, column-name axis labels; research inputs checked: ³He(α,γ) coefficient fixed, ⁷Li/H 5.10 → 4.36 ×10⁻¹⁰; ²⁰⁸Pb energies to 1 keV from AME2020/ENSDF). A7 part 1 (Lessons 1, 2, 2b, 3, 5, 7, 8, 9, cheat sheet).
- 01:45 UTC: red team round 8 started (independent agent).

- 02:00 UTC: A8.1/A8.2 merged (≈ with `within`, D260; typed pointer handle, D261).
- 02:30 UTC: **red team round 8** (25 findings, several silent wrong answers in the first A1/A2): all fixed or documented (D238, tests/test_redteam8.py, REDTEAM.md). **A4/A5 merged** (J m display, ½ ⇄ (1/2) in fmt, list-element figures, complex fft; D240–D244). Full suite 3951 passed. PR jolive27/fermium#2 opened (draft) for macOS CI.

- 02:40–03:35 UTC: macOS CI (first ever) found two real problems: a degenerate fit not detected by `fermium build` on macOS (D262, scale-free test in both runtimes) and a parser crash on an unclosed bracket found by the fuzz test. Both fixed; **CI green on Linux and macOS at `ccdb288`**.
- 03:40 UTC: **A10 — v1.5 frozen** at `ccdb288` (tag `v1.5` local; tag push refused with 403, so the branch `claude/v1.5-freeze` marks it; D263). CHANGES_1.5.md written; PR #2 open.

- 04:00 UTC: **B3 done**: conformance suite harvested from v1.5 (3037 programs in 22 areas, Appendix 1 included); `conformance/run --impl legacy` passes 100% (conformance/LEGACY.md).
- 04:05 UTC: **B4 started**: Cargo workspace rust/ with the spec's crates; AST ported; `fermium run --base-dir` CLI stub; LLVM 18 static libraries and lld installed (llvm-18-dev, liblld-18-dev).

- 04:10–04:40 UTC: **B4/B5 foundations**: units crate merged (51 247 parity checks with v1 printing and units, D11 exact); lexer + parser merged (**3187/3187 programs parse exactly like v1**, errors and warnings included); numerics merged (quadrature with B2 sentinels, RK4/DP45, Radau/BDF, LM fit, roots, eigen); checker core written (scopes, statements, names, arithmetic, calls, printing formats; area stubs honest); evaluator + printer; **`fermium run` works end to end in Rust**.
- 04:35 UTC: red team round 9 (conformance + early Rust): the runner was far too lenient (a fake scored 99.9 %). Fixed: strict comparison (D264), isolated runs, self-test with fakes. **First honest Rust score: 751/3037 (24.7 %)** (CONFORMANCE.md).

## In progress
- Agents (worktrees): A core expressions + built-ins; B vectors/matrices/complex/FFT; C calculus + fermium-sym; D ODE/eigen/PDE solve; numerics (special functions, FFT, RNG, PDE next); syntax (corpus widening, then fermium fmt).
- Me: merging, integration, remaining checker areas (unit systems, analyze, modules, uncertainty, RNG, parallel, data/plot/fit), red-team follow-ups.

## Next
- Merge agents as they report; re-run `conformance/run --impl rust` → CONFORMANCE.md after each merge.
- Round 9 follow-ups still open: harvest gaps (#4: notes/, FRICTION.md, REDTEAM repros, checker-only rejection tests), exclusion false positives (#5), odd-root denominators (#9), 64-bit exponent fractions (#10).
- Then B5 milestones in order; LLVM back end (inkwell) behind the Backend trait; B6/B7.

## Blocked
- (none)

## Hourly log
- 2026-09-25 22:40 UTC — run 2 started.
- 2026-09-25 23:30 UTC — A0 and A1 done; CI set up (first run red on a missing Julia package, fixed); starting A2.
- 2026-09-26 00:17 UTC — A2 done, A3 in its final test run; A4–A6, A8 with three agents; A7 (bootcamp) after they merge.
- 2026-09-26 01:50 UTC — A3, A6, A8.3 in; A7 mostly done; waiting on A4/A5 and A8.1/A8.2 agents and red team 8. CI green on Linux (A2).
- 2026-09-26 02:40 UTC — all Phase A items merged; red team 8 fixed; PR #2 open; waiting for macOS CI before the v1.5 tag.
- 2026-09-26 03:40 UTC — v1.5 frozen (CI green on both platforms); starting Phase B.
- 2026-09-26 04:10 UTC — Phase B: conformance suite ready (legacy 100%); Rust workspace up; three porting agents running.
- 2026-09-26 04:40 UTC — Rust `fermium run` works end to end; strict conformance runner; Rust at 24.7 %; four porting agents + numerics + syntax running.
