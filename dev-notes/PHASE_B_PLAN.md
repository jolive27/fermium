# Phase B plan (v2: the compiler in Rust)

_Written during Phase A while agents were working; no Phase B code is written before the v1.5 tag (spec §A10 gate)._

## Facts about the machine (checked 2026-09-26)
- Rust 1.94.1 (cargo, clippy, rustfmt) installed; crates.io reachable (`cargo fetch` of inkwell 0.5 works).
- LLVM 18.1.3 is installed as a **shared** library only (`libllvm18`); the static libraries come with `llvm-18-dev`
  (apt candidate 1:18.1.3-1ubuntu1). `lld-18` is installed (binary); the lld libraries for embedding need
  `liblld-18-dev`.
- 4 cores, 15 GB RAM. Running several full Python test suites at once runs out of memory: agents must not run
  full suites concurrently.

## Order of work (spec §B3–B5)
1. **Conformance suite first** (`conformance/`), harvested from the legacy implementation:
   - A recording hook in `tests/conftest.py` (env `FERMIUM_HARVEST=dir`): every `run()` / `error_of()` call during
     a normal pytest run writes the program, its stdout, and for errors the diagnostic (kind, line, col, message,
     hint) and exit code. This harvests the ~3700 tests' programs with golden outputs from the oracle itself.
   - Plus every example, rosetta program, bootcamp block (with its output box), gauntlet problem, research
     reproduction, benchmark, and Appendix 1 (mandatory).
   - Dedupe by source hash; drop programs that depend on the machine (timings, absolute paths) or on Python-only
     features, and say so in the manifest.
   - Group by feature area from the syntax used (units, calculus/ode, integrals, eigen, pde, fft, complex,
     uncertainty, data/fit/plot, modules, parallel, cli/repl, build, lsp).
   - Runner `conformance/run --impl legacy|rust` compares stdout with documented numeric tolerances (numbers
     parsed with their display significant figures; relative 1e-9 where the program asks `to N digits`) and
     diagnostics by kind + line + key words, writes `CONFORMANCE.md`: pass/fail per program, per area, overall %.
   - Ready when legacy passes 100% (by construction, then kept honest by re-running).
2. **Cargo workspace** (`rust/`), crates as in spec §B4:
   `fermium-syntax` (lexer, parser, spans, Unicode/ASCII, look-alikes), `fermium-units` (rational dimensions,
   unit database loaded from the Fermium-source unit database at build time, CODATA), `fermium-check`,
   `fermium-sym`, `fermium-ir`, `fermium-codegen` (inkwell/LLVM 18 behind a `Backend` trait),
   `fermium-runtime` (native numerics, printing with D11 significant figures, CSV, SVG/PNG plots, uncertainties),
   `fermium-fmt`, `fermium-cli`, then `fermium-lsp`, `fermium-jupyter`.
   A tree-walking evaluator over the typed IR comes first inside `fermium-ir` (a second oracle, and the fallback
   while codegen grows); the LLVM backend then must print identically, as v1's interpreter and JIT do.
3. **Milestones** in the spec's order (syntax → units/checking → core execution → calculus → complex →
   uncertainties → data → modules/parallel/RNG → CLI/REPL → build → LSP → WASM → Jupyter → Python interop), each
   done when its conformance group passes. `CONFORMANCE.md` is the scoreboard; partial is reported as partial.

## Parallel work
- One agent per crate once the syntax and IR crates' interfaces exist (they are the contract). Agents work in
  worktrees on `claude/v2-rust`, never run the legacy full suite concurrently, and merge only through
  `cargo test` + the conformance runner.

## Honest expectations
- The Python implementation is ~25k lines built over a night and a day; a full-parity Rust port is more than one
  run (the spec says so). The goal for this run is: the conformance suite, the workspace, and as many milestone
  groups passing as can be done properly, reported by percentage.
