# What changed in Fermium 2.0

Fermium 2.0 is the same language as Fermium 1.5, with a new compiler: `fermium` is now one program written in
Rust, with LLVM, the numerics, the plotting, the REPL, the language server and the Jupyter kernel inside it.
Nothing else needs to be installed: no Python, NumPy, SciPy, SymPy, matplotlib, llvmlite or C compiler.
Fermium 1.5 (Python) did its job as the reference: every Fermium 2 behaviour is checked against it.
Design details are in `DECISIONS.md` (the Rust implementation from D264 onwards; the cutover is D269) and in
[docs/architecture.md](docs/architecture.md).

## Installing

- The v2.0 release binaries aren't on the Releases page yet (the tag is pending, D274): until they are, build
  from a checkout with `make install`.
- Download `fermium-macos-arm64` or `fermium-linux-x86_64` from the release, put it on your PATH as `fermium`
  ([bootcamp Lesson 0](bootcamp/lesson00_setup.md) walks through it), and run `fermium doctor`.
- From a checkout: `make install` (`cargo install --locked --path rust/crates/fermium-cli`); building needs Rust
  and the LLVM 18 development files ([rust/BUILD.md](rust/BUILD.md)).
- `fermium --version` prints `fermium 2.0.0 (Rust)`.

## Compatibility

Programs don't change. The conformance suite (`conformance/`: 3366 programs harvested from Fermium 1.5, namely
every test program, example, rosetta, gauntlet and research program, every code block of the docs and the
bootcamp, the benchmarks and John's Appendix 1 programs) holds each program with the output, warnings and error
Fermium 1.5 gives. The comparison is strict (D264): output exactly, except that a number with a decimal point may
differ by one unit in its last printed digit; the same error message, line and hint; the same warnings; the same
exit code. [CONFORMANCE.md](CONFORMANCE.md) has the current score (at the cutover: 3334 of 3366 pass and the other 32
are documented divergences, so every program passes or is documented) and lists each divergence.

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
  the macOS path is not tested yet). Executables can use `plot`, `fit`, `load` and the other constructs
  `fermium run` hands to its interpreter: the executable carries the program and checks it again when it starts
  (and refuses to run if it checks differently there, e.g. after a data file changed).
- **Python interop:** `use python` loads libpython when a program asks for it. Calling Fermium from Python is
  the module `fermium2` (rust/crates/fermium-pyapi) with Fermium 1.5's API.
- **Playground:** the browser page runs the Rust compiler as WebAssembly (3.6 MB instead of about 50 MB of
  Pyodide and SciPy).

## Speed

`fermium run` compiles a program with LLVM 18 when it compiles every construct; in mixed mode the constructs it
doesn't compile itself (plots, fits, loading data, a function applied to a list) run in the tree-walking
interpreter while the rest stays compiled, and everything else runs in the interpreter. The back end never
changes what a program prints (`rust/tools/llvm_diff.py` checks this on every conformance program). The
benchmark table in the README is the Fermium 2 run against Fermium 1.5, Julia and Python (spec §B8.3), with the
details in benchmarks/RESULTS.md and rust/crates/fermium-codegen/PERF.md: whole programs are faster than 1.5
and Julia (fast start-up); compiled inner loops are level with 1.5 on four benchmarks and slower on blackbody
(1.4×), spring_adaptive (1.26×) and 4-thread forces.

## Fermium 1.5 is deprecated

Fermium 1.5 moved to [legacy/](legacy/README.md) (`legacy/fermium`, `legacy/tests`). It stays in CI for one
more phase as the conformance oracle, then goes. `python3 -m pip install -e ".[full]"` still installs it, as
`fermium-legacy` (and `python3 -m fermium`), so it never shadows the Rust `fermium`. Its output is unchanged.
