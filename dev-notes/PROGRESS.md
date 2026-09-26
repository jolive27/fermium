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

- 04:45–05:20 UTC: merged agent A (core expressions, `in`, `to N digits`, lists, all built-ins), B (vectors, matrices, complex, FFT), C (calculus: fermium-sym, derivatives, integrals, Σ), E (unit systems, analyze, modules + stdlib), numerics (special functions, FFT, RNG, PDE, linalg, native SVG/PNG/GIF plots, CSV data, fit reports), syntax (`fermium fmt` in Rust: 22386/22393 runs agree; the 7 misses are a file Python's fmt crashes on), `parallel for` (me). **Rust conformance 2244/3037 (73.9 %)** before the solve merge; 100 % in analyze, appendix1, control-flow, lists, natural-units, vectors-matrices. CI: Rust job with a conformance ratchet (conformance/RUST_FLOOR).

- 05:20–06:25 UTC: merged the solve agent (ode 295/316, pde 53/61, eigen 44/62; PDE-after-jump warning, B2 RT7-2), B (complex 106/106, FFT 37/37), C (derivatives 207/207, integrals 269/274, algebraic solve 44/44), A (rng 29/29, uncertainty 97/108), the tools port (CLI parity, REPL, LSP), the WASM playground (B5.12), and the LLVM back end (default Auto; 1769 compiled programs identical to the tree-walker). Documented divergences checked by the runner (D265). CI was not running (invalid workflow YAML since the Rust job): fixed. **Rust 2775/3037 (91.4 %) + 8 documented divergences**, before the RNG/uncertainty merge.

- 06:25–07:15 UTC: merged the data/plot port (data 76/79), RNG + uncertainties, the tools port (CLI parity, REPL, LSP, Jupyter kernel with native ZMTP), the WASM playground (B5.12: 142/142 examples as native, plots byte for byte, page test 14/14), Python interop both ways (B5.14: python-interop 24/24; libpython loaded only when used; fermium2 ctypes API), calculus B2 fixes (integrals at rounding level; decimal-place rule for sums, 3 cases, all improvements). Conformance suite re-harvested with the docs/notes/red-team blocks and checker-only tests: **3366 cases, legacy 3366/3366; Rust 3307/3366 (98.2 %) + 9 documented divergences (98.5 %)**. CI: legacy oracle pinned to the harvest's library versions.

## In progress
- Agents (worktrees): A core expressions + built-ins; B vectors/matrices/complex/FFT; C calculus + fermium-sym; D ODE/eigen/PDE solve; numerics (special functions, FFT, RNG, PDE next); syntax (corpus widening, then fermium fmt).
- Me: merging, integration, remaining checker areas (unit systems, analyze, modules, uncertainty, RNG, parallel, data/plot/fit), red-team follow-ups.

## Next
- Merge agents as they report; re-run `conformance/run --impl rust` → CONFORMANCE.md after each merge.
- Round 9 follow-ups still open: harvest gaps (#4: notes/, FRICTION.md, REDTEAM repros, checker-only rejection tests), exclusion false positives (#5), odd-root denominators (#9), 64-bit exponent fractions (#10).
- B2 v1 limitations still to fix natively (each with a DIVERGENCES.md entry and a test): an integral at rounding level printing too many figures (RT7-5: use the quadrature's error estimate when printing), PDE accuracy right after a jump (RT7-2), significant figures of sums (D95: the decimal-place rule needs run-time magnitudes). Memory: done (lists are reference-counted).
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
- 2026-09-26 05:20 UTC — Rust at 73.9 % (before the ODE/eigen/PDE merge); agents A–E + LLVM back end running; harvest being widened (notes, red-team, docs blocks).
- 2026-09-26 06:25 UTC — Rust 91.4 % + 8 documented; agents: data/plot (B), speed of the evaluator (A), solve precision (D), calculus B2 (C), LLVM ODE + build (LLVM), Jupyter (tools), playground wrap-up.
- 2026-09-26 07:15 UTC — Rust 98.2 % (+9 documented) on the 3366-case suite; B5.1, B5.9, B5.11–B5.14 done; B5.10 (build) and LLVM ODE in progress; red team 10 started.
- 2026-09-26 08:10 UTC — Rust 98.6 % + 27 documented (99.4 %); runner mirrors each program's folder; module call-line fix; `fermium build` (B5.10) merged: lld linked in, fermium-aotrt runtime, executables identical to `fermium run` on the sample; next PERF.md.
- 2026-09-26 08:48 UTC — Done: fit-parameter uncertainty with covariance, uncertain vectors, faster tree-walker (1.8x), calculus log forms, sums rounding on cancellations, red team 10 triaged (4 fixed, rest assigned), playground CI fix. In progress: numerics/plot failures, B7 distribution, LLVM fallback perf + PERF.md. Next: B8 cutover. Blocked: none.
- 2026-09-26 09:40 UTC — Done: B7 merged (release workflow, macOS Rust CI, Lesson 0, doctor); Radau step order fix; plot of uncertain lists; PDE rounding divergence (D268); CI golden paths fixed. In progress: verifying agent A's uncertain linear algebra and the LLVM mixed mode (26 regressions found and fixed, re-verifying); Radau timeout agent; B8 cutover agent (legacy/ move). Next: B8 gate. Blocked: none.
- 2026-09-26 10:55 UTC — Done: B8 gate met locally (3334 + 32 documented = 3366/3366); LLVM mixed mode, fermium build with plots/data, PERF.md, uncertain linear algebra, temperature sums fix (red team 11 #1), red team 11 done. In progress: CI legacy step pinned to the goldens' OpenBLAS kernel; B8 cutover agent; LLVM inner-loop speed; calculus ln|u|. Next: B8 cutover merge, tag v2.0. Blocked: none.
- 2026-09-26 11:45 UTC — Done: B8 cutover merged and pushed (v1.5 in legacy/ as fermium-legacy, Rust binary is fermium; make check green: legacy 3968 passed, Rust tests, conformance 3334 + 32 documented); CI run 105 fully green; draft PR #3 for Phase B opened (macOS CI running; FFT fixture test fixed for Apple's libm). Container restarted at ~11:30; the LLVM agent's unfinished work saved as WIP and resumed by a new agent (inner loops + B8.3 benchmarks). Next: benchmarks, tag v2.0, then Phase C. Blocked: none.
- 2026-09-26 12:45 UTC — Done: PR #3 (Phase B) green on Linux and macOS (run 127) after macOS fixes: fixture tolerances off glibc, libpython in relocated framework builds, 28 libm-sensitive cases skipped on macOS (D271), NumPy/SciPy in the macOS job. In progress: LLVM inner loops + B8.3 benchmarks (agent). Next: tag v2.0, start Phase C on claude/v2.5. Blocked: none.
- 2026-09-26 13:25 UTC — **B8 done: Fermium 2.0.** B8.3 benchmarks re-measured (inner loops level with v1 on nbody/spring_rk4/unit_loop/forces-1; blackbody 1.4×, spring_adaptive 1.26× slower; whole programs 2.9–11× faster than 1.5); make check green; version 2.0.0; tag v2.0 local (push refused, 403) and branch claude/v2.0-freeze marks it (D274); claude/v2.5 created. Phase C agents running (C3 C/Fortran interop, C1 data structures). Next: merge Phase C work into claude/v2.5, red team 12. Blocked: release binaries need the owner to push the tag (D274).
