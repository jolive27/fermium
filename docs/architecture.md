# Fermium 2 architecture

This is a guide for contributors to the Rust implementation of Fermium (v2). Fermium 1.5, the Python
implementation in `fermium/`, is frozen: it is the **oracle** that defines what every program prints. The Rust
code is a port that must agree with it, except where `rust/DIVERGENCES.md` records a deliberate difference.

## The pipeline

```
source text ──► fermium-syntax ──► AST ──► fermium-check ──► typed IR ──► back end ──► output
                (lexer, parser,          (names, types,     (fermium-ir)  eval.rs (tree-walker)
                 A1 unit rule)            units, calculus,                 llvm/   (LLVM 18 JIT)
                                          diagnostics)
```

1. **Parse** (`fermium-syntax`). The lexer normalises look-alike characters, counts significant figures and
   turns `\name` spellings into symbols; the parser builds the AST (`ast.rs`, the same classes and field names as
   `fermium/ast.py`) and applies the unit rule (D235–D238: which names right after a number are units). Parse
   errors and warnings are `Diagnostic`s formatted exactly like v1's.
2. **Check** (`fermium-check`). Name resolution, types, dimensions (Kennedy-style inference over rational
   exponents: `fermium-ir/src/types.rs`), function instantiation per argument types, calculus (via
   `fermium-sym`), unit systems, modules. The output is the typed IR with units erased: every number is in SI.
   Display facts (the unit the user wrote, significant figures, whether a value is a literal) travel with the
   IR for printing.
3. **Run**. `fermium-codegen/src/eval.rs` runs the IR directly (the reference back end in v2);
   `fermium-codegen/src/llvm/` compiles it with LLVM 18 (statically linked, see `rust/BUILD.md`). Both call the
   same runtime (`fermium-runtime`: numerics, plots, data) and print through the same printer
   (`fermium-codegen/src/printer.rs` over `fermium-units`' number formatting). The CLI uses LLVM when it can
   compile every construct of a program, else the tree-walker, so the back end never changes what a program
   prints (`--backend llvm|interp` forces one).

## The crates

| Crate | What it holds | Ported from |
|---|---|---|
| `fermium-syntax` | lexer, parser, AST, diagnostics, the A1/A2 rules, fix mode | `lexer.py`, `parser.py`, `ast.py`, `errors.py` |
| `fermium-units` | dimensions, the unit database (built from `fermium/selfhost/units_db.fm` by `build.rs`), CODATA constants, natural/nuclear/astro systems, number and quantity printing | `units.py`, `constants.py`, `natural.py`, printing in `runtime/core.py` |
| `fermium-check` | the checker, split by topic (see the table at the top of `checker.rs`) | `checker.py`, `solve.py`, `m3solve.py`, `cplx.py`, `clist.py`, `importer.py`, `dimanalysis.py` |
| `fermium-sym` | symbolic differentiation, simplification, `to_source`, isolate, indefinite integrals | `calculus.py` |
| `fermium-ir` | the typed IR, the boundary between the checker and the back ends | `ir.py`, `types.py` |
| `fermium-codegen` | back ends: `eval.rs` + `eval_*.rs` (tree-walker), `llvm/` (JIT), `printer.rs` | `interp.py`, `codegen_llvm.py` |
| `fermium-runtime` | numerics (quadrature, ODE, Radau/BDF, fit, roots, eigen, linalg, FFT, special functions, RNG, PDE), native SVG/PNG/GIF plots, CSV data | `runtime/*.py`, `numerics.py`, `linalg*.py`, `special.py`, `rng.py`, SciPy/NumPy/matplotlib |
| `fermium-fmt` | `fermium fmt --pretty/--ascii/--fix` | `fmt.py` |
| `fermium-cli` | the `fermium` binary: run, check, fmt, parse, doctor, repl, lsp, jupyter | `cli.py`, `driver.py`, `repl.py` |

The standard library stays Fermium source (`fermium/stdlib/*.fm`), embedded in the binary at build time.

## Conformance: the scoreboard

`conformance/cases/<area>/<id>.fm` with `<id>.json` are programs harvested from v1 (every test, example,
rosetta, gauntlet and research program, the benchmarks, John's Appendix 1, fenced blocks in the docs, notes and
red-team reports) with the output, warnings and error v1 gives. `conformance/run --impl rust` scores the Rust
binary and writes `CONFORMANCE.md`; `--impl legacy` must stay at 100 %. The comparison is strict (D264): exact
output, except that a number with a decimal point or a power of ten may differ by one unit in its last digit
when it has the same shape; the same error message, line and hint; the same warnings; the same exit code. CI
runs both and fails if the Rust score drops below `conformance/RUST_FLOOR` (a ratchet: raise it as it improves).

To regenerate the suite after changing v1 (you shouldn't: it is frozen) or the harvest sources:

```sh
FERMIUM_HARVEST=/tmp/h python3 -m pytest -q -n 4 --dist loadfile   # record the test programs
python3 conformance/harvest.py /tmp/h                               # write the cases
```

## How to port or fix a feature

1. Find the failing cases: `python3 conformance/run --impl rust --bin rust/target/fast/fermium --area <area>`
   lists every failure with its reason.
2. Read the v1 code for that feature (the checker method has the same name in snake_case; the run-time side is
   in `interp.py`, but the goldens come from the compiled path — `codegen_llvm.py` + `runtime/core.py` — so match
   that where the two differ).
3. Port it into the module that owns the topic; keep method names close to the Python so the two can be diffed.
4. Build with `cargo build --profile fast -p fermium-cli` (release speed without LTO) and re-run the area.
5. A deliberate difference from v1 (a v1 bug fixed, a Python-only behaviour) goes in `rust/DIVERGENCES.md`
   with the case ids and a test.

## Numbers must agree bit for bit

Printing shows 3 significant figures by default but up to 17 with `to N digits`, so the Rust code reproduces
v1's arithmetic exactly: IEEE helpers in `eval.rs` (`fdiv`, `powc` with real odd roots, `fpow`), the same order
of operations in the numerics (each `fermium-runtime` module documents its agreement with v1 in `NUMERICS.md`),
the C library for the libm functions v1's compiled code called, and v1's own algorithms elsewhere. `parallel
for` adds sums in fixed blocks (`par_blocks`) so results don't depend on the number of threads (D152).

## Tests

- `cargo test` in `rust/`: unit tests per crate, parity fixtures generated from v1 (`rust/tools/*_fixtures.py`:
  units and printing, numerics), parser and formatter parity tools (`rust/tools/compare_parse.py`,
  `compare_fmt.py`).
- The conformance suite (above).
- `rust/tools/llvm_diff.py`: every program the LLVM back end compiles must print exactly what the tree-walker
  prints.
