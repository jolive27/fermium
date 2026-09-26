# PROGRESS — run 2 (Spec 1.5 → 2)

_Plan: `dev-notes/FERMIUM_SPEC_V1.5_V2.md`. End: Sun 2026-09-27 13:00 UTC (9 AM ET). Report: `dev-notes/RUN_REPORT.md` by 12:30 UTC Sunday._
_The first run's log is `dev-notes/PROGRESS_v1.md`._

## Status
- **Phase:** A (v1.5), branch `claude/v1.5`.

## Done
- 22:40 UTC Fri: branch `claude/v1.5` created (D234); hourly check-in Routine set; CLAUDE.md rewritten for this plan; dev logs moved to `dev-notes/` (A9.3); README note and LICENSE name per A9.1–A9.2.

- 22:55 UTC: CI workflow (Linux on push; macOS on PRs/dispatch; concurrency cancels old runs). A0 triage: `dev-notes/OPEN_ITEMS.md` (111 rows; 22 fix in A).
- 23:25 UTC: **A1 done** (D235): the three-sentence unit rule, spacing-free; `fmt --fix`; LSP quick fix; migration (25 edits, values unchanged); Appendix 1 tests (lesson2's `a = 3 m` conflicts with the table: the rule wins, see D235). make check 3688 passed.

- 23:48 UTC: **A2 done** (D236): pure-number fractions are coefficients; every .fm program's output unchanged vs v1.
- 00:15 UTC: A3 messages (D237): natural-units hint, the g message and the g₀ leak, conversion hints, REPL terminal-command hint, uncertain-vector crash, negative literal domain errors; R1–R3/R5–R7 regression tests.

## In progress
- Three agents in worktrees: A4+A5 (display, fmt ½, FFT complex), A6+A8.3 (doctor, absolute plot paths, pendulum fit curve, axis labels, research citations), A8.1+A8.2 (≈ with tolerance, typed pointer handle). I merge each through the full suite.

## Next
- A2 fraction coefficients, A3 messages, A4 display, A5 API, A6 doctor/plots, A7 bootcamp, A8 review items, then A10 freeze.

## Blocked
- (none)

## Hourly log
- 2026-09-25 22:40 UTC — run 2 started.
- 2026-09-25 23:30 UTC — A0 and A1 done; CI set up (first run red on a missing Julia package, fixed); starting A2.
- 2026-09-26 00:17 UTC — A2 done, A3 in its final test run; A4–A6, A8 with three agents; A7 (bootcamp) after they merge.
