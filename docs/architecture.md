# Fermium 2 architecture

This is a guide for contributors to Fermium 2, the Rust implementation in `rust/` that builds the `fermium`
binary. Fermium 1.5, the Python implementation, now lives in `legacy/fermium/` (deprecated since v2.0, DECISIONS
D269). It is frozen, and it is the **oracle** that defines what every program prints: the Rust code is a port
that must agree with it, except where `rust/DIVERGENCES.md` records a deliberate difference. `rust/BUILD.md`
explains how to build; `make build` gives `rust/target/fast/fermium`, and `make check` runs everything (below).

## The pipeline

```
source text ──► fermium-syntax ──► AST ──► fermium-check ──► typed IR ──► back end ──────────► output
                (lexer, parser,          (names, types,     (fermium-ir)  llvm/   (LLVM 18 JIT)
                 A1 unit rule)            units, calculus,                  │  mixed mode: fm_interp
                                          diagnostics)                      ▼
                                                                          eval.rs (tree-walker)
```

1. **Parse** (`fermium-syntax`). The lexer normalises look-alike characters, counts significant figures and
   turns `\name` spellings into symbols; the parser builds the AST (`ast.rs`, the same classes and field names as
   `legacy/fermium/ast.py`) and applies the unit rule (D235–D238: which names right after a number are units).
   Parse errors and warnings are `Diagnostic`s formatted exactly like v1's.
2. **Check** (`fermium-check`). Name resolution, types, dimensions (Kennedy-style inference over rational
   exponents: `fermium-ir/src/types.rs`), function instantiation per argument types, calculus (via
   `fermium-sym`), unit systems, modules. The output is the typed IR with units erased: every number is in SI.
   Display facts (the unit the user wrote, significant figures, whether a value is a literal) travel with the
   IR for printing.
3. **Run** (`fermium-cli/src/run.rs`). The LLVM back end (`fermium-codegen/src/llvm/`, LLVM 18 statically
   linked, see `rust/BUILD.md`) compiles the IR and runs it with the JIT; the tree-walker
   (`fermium-codegen/src/eval.rs` and `eval_*.rs`) runs the IR directly. Both call the same runtime
   (`fermium-runtime`: numerics, plots, data) and print through the same printer (`fermium-codegen/src/printer.rs`
   over `fermium-units`' number formatting), so the back end never changes what a program prints. The CLI's
   choice: programs with uncertain values (±) run in the tree-walker (as in v1, D122; integrals and ODEs with
   uncertain inputs, Fermium 2.5: `eval_unc_kern.rs`, D276–D278); otherwise LLVM compiles
   the program if it can, in mixed mode (next section), and the tree-walker runs whatever LLVM rejects.
   `fermium run --backend llvm|interp` or `FERMIUM_BACKEND` forces one; `FERMIUM_BACKEND_INFO=1` says which ran.

### Mixed mode: compiled code that delegates to the tree-walker

Some constructs are not compiled by the LLVM back end itself: `plot`, `fit`, `plot … animate`, `load` /
`table` / `columns`, functions applied to lists (maps), run-time vector indexes. Instead of sending the whole
program to the tree-walker (red team round 10), the code generator emits a call into the tree-walker for just
that statement or expression:

- `llvm/deleg.rs` (code generation) walks the construct for the variables it reads (including those the
  functions it calls read) and the ones it sets, records an `InterpSite` (the IR node's address, the variables
  with their kinds, the kind of the result) and emits a call to `fm_interp(ctx, site, env, out, line)` with an
  env of pointers to the variable slots.
- `llvm/delegate.rs` (run time, `fm_interp`) copies those slots into a tree-walker `Frame` as `Value`s, runs the
  node with the context's own tree-walker instance (the same printer, so output stays in order), writes the
  variables it set back into the compiled slots, and returns the line it ended on. Data sets and complex lists
  cross the boundary as opaque values; ODE solutions are mirrored for plots.
- A construct the copy can't serve (e.g. `x'(t)` of a solution, a PDE's `u(x, t)` read inside it) is rejected
  at compile time, and the program runs in the tree-walker as before.

`python3 rust/tools/llvm_diff.py --bin rust/target/fast/fermium -j 2` runs every conformance program with both
back ends and requires identical output wherever LLVM compiles the program (at the mixed-mode merge: 2524 of
2614 compiled, 2521 identical, the other 3 time out or run out of stack in the tree-walker).

### `fermium build`: executables linked with the built-in lld

`fermium build prog.fm [-o prog]` (`fermium-cli/src/aot.rs`) compiles the program with the LLVM back end into
an object file (`llvm::build_object`: generic CPU, position independent; the code reads its run-time context
from the global `fm_ctx`; `fm_blob` carries the module's tables, the code generator's tables and the source,
`llvm/blob.rs`) and links it with **lld, linked into the fermium binary** (`llvm/lld_shim.cpp`, `llvm/link.rs`)
against:

- `crates/fermium-aotrt`, the run time of executables: a static library holding the C `main` and the same
  `native::rt` callbacks, printer and numerics as the JIT (`fermium-codegen/src/native.rs` re-exports
  `llvm/rt.rs`, `blob.rs`, `solve_rt.rs` and `delegate.rs` without LLVM). `fermium-cli/build.rs` builds it with a
  nested `cargo build -p fermium-aotrt --profile aotrt` and embeds it in the binary (`FERMIUM_NO_AOTRT=1` skips
  it for faster builds);
- on Linux, the C start-up files of the build machine's glibc (embedded too) and the shared libc, libm and
  libgcc_s every glibc system has. No C compiler or system linker is used. On macOS, `ld64.lld` against the
  Command Line Tools' libSystem (written, not yet tested on a Mac).

An executable prints exactly what `fermium run --backend llvm` prints (`rust/tools/aot_diff.py` checks it on the
conformance programs). Mixed mode isn't available there yet: a program with a delegated construct (plot, fit,
data, …) is refused with a message saying so, as are programs with uncertain values.

## The crates

| Crate | What it holds | Ported from (legacy/fermium/) |
|---|---|---|
| `fermium-syntax` | lexer, parser, AST, diagnostics, the A1/A2 rules, fix mode | `lexer.py`, `parser.py`, `ast.py`, `errors.py` |
| `fermium-units` | dimensions, the unit database (built from `legacy/fermium/selfhost/units_db.fm` by `build.rs`), CODATA constants, natural/nuclear/astro systems, number and quantity printing | `units.py`, `constants.py`, `natural.py`, printing in `runtime/core.py` |
| `fermium-check` | the checker, split by topic (see the table at the top of `checker.rs`) | `checker.py`, `solve.py`, `m3solve.py`, `cplx.py`, `clist.py`, `importer.py`, `dimanalysis.py` |
| `fermium-sym` | symbolic differentiation, simplification, `to_source`, isolate, indefinite integrals | `calculus.py` (and SymPy) |
| `fermium-ir` | the typed IR, the boundary between the checker and the back ends | `ir.py`, `types.py` |
| `fermium-codegen` | back ends: `eval.rs` + `eval_*.rs` (tree-walker), `llvm/` (JIT, mixed mode, object files, lld), `native.rs` (the run time of compiled code), `printer.rs` | `interp.py`, `codegen_llvm.py`, `codegen_m3.py`, `aot.py` |
| `fermium-runtime` | numerics (quadrature, ODE, Radau/BDF, fit, roots, eigen, linalg, FFT, special functions, RNG, PDE), native SVG/PNG/GIF plots, CSV data, the file layer (`vfs`: the file system natively, in memory in the browser) | `runtime/*.py`, `numerics.py`, `linalg*.py`, `special.py`, `rng.py`, SciPy/NumPy/matplotlib |
| `fermium-fmt` | `fermium fmt --pretty/--ascii/--fix` | `fmt.py` |
| `fermium-repl` | the REPL and the session it shares with the Jupyter kernel | `repl.py`, `driver.ReplSession` |
| `fermium-lsp` | the language server (`fermium lsp`): JSON-RPC over stdin/stdout, diagnostics, hover with units, completion, the A1 quick fix | `lsp.py` (pygls) |
| `fermium-jupyter` | the Jupyter kernel (`fermium jupyter install`): Jupyter's wire protocol (ZMTP 3.1, HMAC-SHA256) implemented in the crate, so no libzmq, Python or ipykernel | `jupyter/kernel.py` |
| `fermium-cli` | the `fermium` binary: run, check, fmt, build, doctor, repl, lsp, jupyter | `cli.py`, `driver.py`, `doctor.py` |
| `fermium-aotrt` | the run time of executables made by `fermium build` (a static library embedded in the binary) | `runtime/aot_rt.c`, `aot_data.c` |
| `fermium-wasm` | the browser playground's compiler: parse → check → tree-walker as a `wasm32-unknown-unknown` module with a small C ABI (D266) | the Pyodide page |
| `fermium-pyapi` | calling Fermium from Python: a C ABI (`libfermium_pyapi`) behind the ctypes module `python/fermium2`, with v1's API (compile, load, Quantity, …) | `api.py` |

The standard library stays Fermium source (`legacy/fermium/stdlib/*.fm`), embedded in the binary at build time
(`fermium-check/build.rs`, which looks in `stdlib/`, `fermium/stdlib/` and `legacy/fermium/stdlib/`). When
legacy/ is removed after the next phase, those two inputs (the stdlib and `units_db.fm`) move to `rust/`.

### Tools around the binary

- **REPL** (`fermium`, `fermium repl`): `fermium-repl`, v1's prompt loop line for line; the session state
  (variables carried between inputs) is shared with the Jupyter kernel.
- **Language server** (`fermium lsp`): `fermium-lsp`, used by `editors/vscode` (the extension starts
  `fermium lsp`; `languageServer.command` overrides the path). Checked against v1's server with recorded
  sessions (`rust/tools/lsp_session.py`, `crates/fermium-lsp/tests/sessions/`).
- **Jupyter** (`fermium jupyter install`, then Jupyter starts `fermium jupyter kernel -f CONNECTION_FILE`):
  `fermium-jupyter`; `rust/tools/jupyter_e2e.py` runs v1's kernel tests against it through jupyter_client.
- **Playground** (`web/`): `web/build.py` builds `fermium-wasm` (`cargo build --profile wasm --target
  wasm32-unknown-unknown`) plus the examples and the `\name` table into `web/gen/`; `web/fermium.js` is the only
  JS that talks to the module, shared by the page's worker and the Node tests. `web/test/compare_native.js`
  requires every example to print in the browser module what the native `fermium run` prints (CI). Browser-only
  differences (no C library, a small stack, an in-memory file system) are in `rust/DIVERGENCES.md`.
- **Python** (`use python` and `fermium2`): the binary loads libpython with dlopen the first time a program says
  `use python` (`fermium-check/src/pyinterop.rs`, `eval_py.rs`); `fermium-pyapi` is the other direction.
- **C and Fortran** (`import c` / `import fortran`, D275): `fermium-check/src/cinterop.rs` dlopens the library
  when the program is checked, looks up every symbol and checks each call's units; a call is the IR built-in
  `ccall` over `tables.ccalls` (like `pycall`). `fermium-runtime/src/cffi.rs` calls through the C ABI without
  libffi: the arguments (doubles, ints, pointers) are split into the platform's integer and floating-point
  registers and 8-byte stack slots, and the function pointer is called as a Rust `extern "C" fn` taking all of
  them (x86-64 System V and AArch64). The tree-walker converts values in `eval_c.rs`; the LLVM back end emits a
  direct call when every argument and the result are doubles, and otherwise (and in `fermium build`
  executables, which carry the library's path) goes through the built-in callback into `eval_c.rs`.

## Conformance: the scoreboard

`conformance/cases/<area>/<id>.fm` with `<id>.json` are programs harvested from v1 (every test, example,
rosetta, gauntlet and research program, the benchmarks, John's Appendix 1, fenced blocks in the docs, notes and
red-team reports) with the output, warnings and error v1 gives. `conformance/run --impl rust` scores the Rust
binary and writes `CONFORMANCE.md` (or `--out FILE`); `--impl legacy` must stay at 100 %. The comparison is
strict (D264): exact output, except that a number with a decimal point or a power of ten may differ by one unit
in its last digit when it has the same shape; the same error message, line and hint; the same warnings; the
same exit code. A documented divergence counts only while the binary prints exactly what
`conformance/divergences/<id>.json` records (D265). `make check` and CI fail if the Rust score drops below
`conformance/RUST_FLOOR` (a ratchet: raise it as it improves).

Because the docs, bootcamp, examples, gauntlet and research programs are all in the suite, the conformance run
is what checks them against the Rust binary; the legacy pytest suite still runs them with v1. A new or changed
fenced block reaches the suite only when it is re-harvested:

```sh
FERMIUM_HARVEST=/tmp/h python3 -m pytest -q -n 4 --dist loadfile   # record the test programs (legacy/tests)
python3 conformance/harvest.py /tmp/h                               # write the cases
```

The v1 test programs moved from `tests/` to `legacy/tests/` at the cutover; the cases keep the folder names
`tests/programs[/john]` (part of their ids), and the runner and the harvest map them (D269).

## How to port or fix a feature

1. Find the failing cases: `python3 conformance/run --impl rust --bin rust/target/fast/fermium --area <area>
   --out /tmp/c.md` lists every failure with its reason.
2. Read the v1 code for that feature in `legacy/fermium/` (the checker method has the same name in snake_case;
   the run-time side is in `interp.py`, but the goldens come from the compiled path, `codegen_llvm.py` +
   `runtime/core.py`, so match that where the two differ). `python3 -m fermium run prog.fm` (or
   `fermium-legacy run`) runs the oracle.
3. Port it into the module that owns the topic; keep method names close to the Python so the two can be diffed.
4. Build with `make build` (`cargo build --profile fast -p fermium-cli`: release speed without LTO) and re-run
   the area.
5. A deliberate difference from v1 (a v1 bug fixed, a Python-only behaviour) goes in `rust/DIVERGENCES.md`
   with the case ids, a `conformance/divergences/<id>.json` per case (`conformance/add_divergence.py`), and a
   test.

## Numbers must agree bit for bit

Printing shows 3 significant figures by default but up to 17 with `to N digits`, so the Rust code reproduces
v1's arithmetic exactly: IEEE helpers in `eval.rs` (`fdiv`, `powc` with real odd roots, `fpow`), the same order
of operations in the numerics (each `fermium-runtime` module documents its agreement with v1 in `NUMERICS.md`),
the C library for the libm functions v1's compiled code called, and v1's own algorithms elsewhere. `parallel
for` adds sums in fixed blocks (`par_blocks`) so results don't depend on the number of threads (D152).

## Tests

`make check` (`check.sh`) runs, in order:

1. ruff and the legacy pytest suite (`legacy/tests`: Fermium 1.5, deprecated but kept green), which also runs
   the docs and bootcamp blocks, the examples, gauntlet and research programs with v1;
2. `cargo build --profile fast && cargo test --profile fast` in `rust/`, then `cargo test --profile fast -p
   fermium-pyapi -p fermium-wasm` (not default members): unit tests per crate, parity fixtures generated from v1
   (`rust/tools/*_fixtures.py`: units and printing, numerics), recorded REPL, LSP and Jupyter sessions;
3. the whole conformance suite against `rust/target/fast/fermium`, with `--min conformance/RUST_FLOOR`.

`FERMIUM_SKIP_RUST=1` skips 2 and 3 (it prints that it did; so does a machine without cargo). In CI the legacy
jobs run make check that way, and the `rust-linux` / `rust-macos` jobs build, test, check the playground (Linux)
and score the binary (`.github/workflows/ci.yml`; macOS only on pull requests and hand-started runs).
Beyond make check: `rust/tools/llvm_diff.py` (LLVM against the tree-walker), `rust/tools/aot_diff.py`
(executables against `fermium run`), `rust/tools/compare_parse.py` and `compare_fmt.py` (parser and formatter
against v1).
