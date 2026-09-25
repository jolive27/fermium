# CLAUDE.md — Fermium

**Read `dev-notes/PROGRESS.md` first.** Then `dev-notes/FERMIUM_SPEC_V1.5_V2.md` (the current plan), `DECISIONS.md`,
`dev-notes/OPEN_ITEMS.md`, `BACKLOG.md` and `git log`. Run `make check`, then continue from "Next".
After any context compaction, re-read this file, `dev-notes/PROGRESS.md` and the spec before continuing.

## Mission
Fermium is a programming language for physicists, designed by John Oliver:

> Physics code that reads like physics on paper, a compiler that understands units and calculus, and speed on par with Julia.

- Units are checked at compile time and erased (zero runtime cost). Calculus is part of the language.
- Keep 100% (spec §1): notebook-like readability (Greek, subscripts, `²`, `√`, `½`, implicit multiplication),
  the unit-after-number rule, `\name` + Tab and ASCII ⇄ symbol equivalence, `fmt --pretty/--ascii`, speed,
  one-line errors with a caret and a hint, `where`, natural units and the constants library.
- Priority when goals conflict: (1) unit safety, (2) readability, (3) calculus, (4) performance.

## The plan: `dev-notes/FERMIUM_SPEC_V1.5_V2.md` (unattended run, ends Sun 2026-09-27 13:00 UTC = 9 AM ET)
Phases in order, each gated:
- **A (v1.5)**, branch `claude/v1.5`: fix what real use revealed in the Python implementation (A0–A9), then freeze (A10: `CHANGES_1.5.md`, tag `v1.5`, PR).
- **B (v2)**, branch `claude/v2-rust` from the `v1.5` tag: the compiler in Rust, zero runtime dependencies; `conformance/` suite first; cutover at B8.
- **C (v2.5)**, branch `claude/v2.5`: language growth. **D (v3)**, branch `claude/v3`: scale and ecosystem.
- Write `dev-notes/RUN_REPORT.md` 30 minutes before the end time (spec §F).

## Operating rules (spec §E)
1. Don't finish before the end time; when the list is done, add to `BACKLOG.md` and continue.
2. Commit after every working step; push at least every 30 min. Never leave a branch broken: `make check` before each commit.
3. `dev-notes/PROGRESS.md`: done / in progress / next / blocked, with a timestamped line every hour.
4. Stuck > 30 min: log under Blocked, move on, come back later.
5. Tests, CI and `CONFORMANCE.md` are the source of truth. Never weaken a test or golden output unless it was wrong (say why in the commit).
6. Honesty: never claim a feature works unless a test proves it. Label stubs as stubs. Keep benchmarks fair.
7. Subagents for parallel work; merge only through tests. **Before any full test run, check that the installed
   `fermium` points at the main checkout** (`python3 -c "import fermium; print(fermium.__file__)"`).
8. Red team with an independent subagent at least every 3 hours; log in `dev-notes/REDTEAM.md`.
9. Record design choices in `DECISIONS.md` (what, why, alternatives).
10. Never add, edit or commit John's personal files: `my_notes.md`, `hello.fm`, `me.fm`, `lesson*.fm`, `ke.fm` (top level).
11. CI minutes are limited: Linux CI on pushes; macOS CI only on pull requests or manual dispatch at milestones; concurrency cancels old runs.

## Commands
- `python3 -m pip install -e ".[full,dev]"` — install everything (`.[full]` is enough for users); exposes `fermium`.
- `make check` — lint + the whole test suite (docs and bootcamp blocks, examples, gauntlet, research). Must pass before every commit.
- `fermium run file.fm`, `fermium` (REPL), `fermium fmt --pretty|--ascii file.fm`, `fermium doctor`, `fermium build file.fm`.
