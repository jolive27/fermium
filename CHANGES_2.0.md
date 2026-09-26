# What changed in Fermium 2.0

Fermium 2.0 is the same language as Fermium 1.5, with a new compiler: `fermium` is now one program written in
Rust, with LLVM, the numerics, the plotting, the REPL, the language server and the Jupyter kernel inside it.
Nothing else needs to be installed: no Python, NumPy, SciPy, SymPy, matplotlib, llvmlite or C compiler.
Fermium 1.5 (Python) did its job as the reference: every Fermium 2 behaviour is checked against it.
Design details are in `DECISIONS.md` (the Rust implementation from D264 onwards; the cutover is D269) and in
[docs/architecture.md](docs/architecture.md).

## Installing

- Download `fermium-macos-arm64` or `fermium-linux-x86_64` from the release, put it on your PATH as `fermium`
  ([bootcamp Lesson 0](bootcamp/lesson00_setup.md) walks through it), and run `fermium doctor`.
- From a checkout: `make install` (`cargo install --locked --path rust/crates/fermium-cli`); building needs Rust
  and the LLVM 18 development files ([rust/BUILD.md](rust/BUILD.md)).
- `fermium --version` prints `fermium 2.0.0-dev (Rust)` until the release is tagged.

## Compatibility

Programs don't change. The conformance suite (`conformance/`: 3366 programs harvested from Fermium 1.5, namely
every test program, example, rosetta, gauntlet and research program, every code block of the docs and the
bootcamp, the benchmarks and John's Appendix 1 programs) holds each program with the output, warnings and error
Fermium 1.5 gives. The comparison is strict (D264): output exactly, except that a number with a decimal point may
differ by one unit in its last printed digit; the same error message, line and hint; the same warnings; the same
exit code. [CONFORMANCE.md](CONFORMANCE.md) has the current score (99.7 % pass or are documented divergences when
this was written) and lists every remaining failure.

Every deliberate difference is a section of [rust/DIVERGENCES.md](rust/DIVERGENCES.md), and each affected
conformance case is checked against the output the divergence describes (D265). The ones a reader is most likely
to notice:

- **v1 limitations fixed:** quadrature of narrow peaks and strong singularities, integrals at the rounding level
  printed with only their meaningful figures, sums of measured values by the decimal-place rule, PDEs right after
  a jump in the initial condition.
- **Native instead of Python libraries:** indefinite integrals and symbolic tidying without SymPy, plots drawn
  natively (SVG, PNG and animated GIF, in matplotlib's style but not pixel-identical), a tridiagonal LU instead of
  SuperLU for PDE steps. Numbers agree with 1.5 to the printed digits (bit for bit wherever the algorithms are
  ports).
- **Tools:** `fermium doctor` checks the one binary instead of Python packages; `fermium run` takes
  `--backend auto|llvm|interp`; `fermium build` links executables with the built-in lld (no C compiler on Linux;
  the macOS path is not tested yet). `fermium build` can't yet build programs that use `plot`, `fit`, `load`
  or other constructs `fermium run` hands to its interpreter: it says so, and `fermium run` runs them.
- **Python interop:** `use python` loads libpython when a program asks for it. Calling Fermium from Python is
  the module `fermium2` (rust/crates/fermium-pyapi) with Fermium 1.5's API.
- **Playground:** the browser page runs the Rust compiler as WebAssembly (3.6 MB instead of about 50 MB of
  Pyodide and SciPy).

## Speed

`fermium run` compiles a program with LLVM 18 when it compiles every construct; in mixed mode the constructs it
doesn't compile itself (plots, fits, loading data, a function applied to a list) run in the tree-walking
interpreter while the rest stays compiled, and everything else runs in the interpreter. The back end never
changes what a program prints (`rust/tools/llvm_diff.py` checks this on every conformance program). The
benchmark table in the README was measured with Fermium 1.5; the LLVM back end's first measurements are in
rust/crates/fermium-codegen/PERF.md, and the re-run against Julia and Python for 2.0 (spec §B8) replaces the
table.

## Fermium 1.5 is deprecated

Fermium 1.5 moved to [legacy/](legacy/README.md) (`legacy/fermium`, `legacy/tests`). It stays in CI for one
more phase as the conformance oracle, then goes. `python3 -m pip install -e ".[full]"` still installs it, as
`fermium-legacy` (and `python3 -m fermium`), so it never shadows the Rust `fermium`. Its output is unchanged.
