# CLAUDE.md — Fermium

**Read `PROGRESS.md` before doing anything.** Then `DECISIONS.md`, `BACKLOG.md`, and `git log`, run `make check`, and continue from "Next".

## Mission
Fermium is a programming language for physicists (the user is a physics undergrad, nuclear physics & astrophysics, still learning to code):

> Physics code that reads like physics on paper, a compiler that understands units and calculus, and speed on par with Julia.

- Units are built into the compiler, checked **before the program runs**, and erased (zero runtime cost).
- Calculus is part of the language: `x'`, `d/dt`, `∫ … d…`, `solve … with …`.
- Notation: `4π² L / T²`. Errors: one clear line in physics terms.
- Priority when goals conflict: (1) unit safety, (2) readability, (3) calculus, (4) performance. Correctness always beats speed.
- Architecture: Python 3 front end → typed IR → LLVM (llvmlite) JIT. Full spec: `FERMIUM_SPEC_1.md`.

## Operating rules (spec §5, §6)
1. **Not done before 9:00 AM Eastern (13:00 UTC), Fri Sep 25 2026** (the user extended the end time from 7 AM).** When Tiers 1–4b are done, work Tier 5 from `BACKLOG.md`. If the backlog is empty, review critically and add to it. Going idle early is the main failure mode.
2. **Git**: commit after every working step with a clear message; push at least every 30 min. Never commit a broken build — run `make check` before every commit.
3. **`PROGRESS.md`** is always current: done / in progress / next / blocked, with timestamps.
4. **Resuming**: read PROGRESS.md, DECISIONS.md, BACKLOG.md, `git log`, run the tests, continue from "Next".
5. **Stuck > 30 min**: write what was tried under *Blocked* in PROGRESS.md, move on, come back later.
6. **Tests are the source of truth.** Never weaken/delete a test to make it pass unless the test is wrong (explain in the commit message).
7. **Honesty**: never claim something works unless a test proves it. Mark partial features as partial. Never fudge benchmarks.
8. **Record design choices** in `DECISIONS.md` (what, why, alternatives).
9. Stay inside this project directory. Network only for installing packages/tools.
10. Write `MORNING_REPORT.md` (~6:45 AM ET, final by 7:00 AM ET) per spec §7.

## Working branch
`claude/lucid-gauss-9y1ov2` (push with `git push -u origin claude/lucid-gauss-9y1ov2`).

## Commands
- `pip install -e .` — install; exposes `fermium`.
- `make check` — lint + tests + bootcamp snippets + examples. Must pass before every commit.
- `fermium run file.fm`, `fermium` (REPL), `fermium fmt --pretty|--ascii file.fm`, `fermium doctor`.

## Night plan, part 2 (user instruction received 00:05 UTC, after Tier 4 was complete)
Keep working until **13:00 UTC** (9 AM ET; the user extended it). The phases, in order:
1. **Strict audit:** start from a clean clone. Follow bootcamp Lesson 0, then run `make check`, all examples, every bootcamp snippet and the benchmarks. Write AUDIT.md and fix everything in it.
2. **Features, in order:**
   1. Jupyter kernel and an example notebook
   2. Language server and VS Code: hover shows units, live error underlines, `\name` completion
   3. Vectors and matrices with units, and 3-D vector ODEs
   4. ∂/∂x, gradient, divergence, curl
   5. Browser playground (Pyodide)
   6. `fermium build`
3. **Textbook gauntlet** in `gauntlet/<topic>/`: 3 tested problems per topic, and `gauntlet/FRICTION.md` gets fixed in the language. Topics, in order: mechanics, oscillations, gravitation, thermodynamics, electromagnetism, optics/waves, special relativity, quantum mechanics, nuclear physics, astrophysics. After that, a second, harder pass. Add a README "Gauntlet" section with counts.
4. **Hourly rotating quality passes (~15 min):** beginner, adversarial, performance, correctness and docs, in rotation.

Other rules:
- Add a timestamped line to PROGRESS.md on the hour.
- At **12:30 UTC** stop new work and write MORNING_REPORT.md. Include the audit, Phase 2 status, gauntlet counts, the top 10 frictions and what is still weak.
