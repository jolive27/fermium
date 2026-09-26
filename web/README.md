# The Fermium playground

A static page that runs Fermium in the browser: an editor with `\name` + Tab completion, Run (Ctrl+Enter /
Cmd+Enter) and Stop, warnings and errors in the usual one-line form with a caret and a hint, plots, and a menu
with every code block of the bootcamp lessons and every program in `examples/`. Nothing is sent to a server.

Since v2 (spec B5.12) it runs the Rust compiler, built to WebAssembly: `rust/crates/fermium-wasm` parses, checks
the units and runs the program with the tree-walking back end (no LLVM in the browser). It replaced the Pyodide
page of v1.5, which ran the Python implementation (about 50 MB of Python and SciPy to download).

## Build and run it locally

You need Rust with the WebAssembly target, and nothing else (no wasm-bindgen, no npm):

```
rustup target add wasm32-unknown-unknown
python3 web/build.py                  # fermium.wasm, examples, symbols -> web/gen/ (first build: a few minutes)
python3 -m http.server -d web 8000    # then open http://localhost:8000/
```

`web/build.py --wasm FILE` uses a module built elsewhere; `--no-wasm` only rewrites the JSON files. `web/gen/` is
a build output and is not committed. The module is built with
`cargo build --profile wasm --target wasm32-unknown-unknown -p fermium-wasm` in `rust/` (the `wasm` profile is
release with whole-program LTO and no symbol names).

**Size:** `fermium.wasm` is 3.6 MB (1.2 MB gzipped, as a web server sends it), measured 2026-09-26 at 90a44c4.
It loads and is ready in about 0.3 s from a local server (headless Chromium).

## How it fits together

| file | what it does |
|---|---|
| `index.html`, `playground.css` | the page |
| `playground.js` | editor, examples menu, `\name` completion, output panel; starts the worker |
| `worker.js` | a Web Worker, so a long program never freezes the page (Stop terminates it and starts a new one) |
| `fermium.js` | loads `fermium.wasm` and runs a program: the only code that talks to the module (also used by the Node tests) |
| `build.py` | builds `gen/`: `fermium.wasm`, `examples.json` (with the CSV files the examples read), `symbols.json`, `manifest.json` |
| `test/` | Node tools: `run_wasm.js` (`fermium run` on the module), `fermium-wasm` (the same, for `conformance/run --bin`), `compare_native.js` |

The module has no JS glue generator: it exports `alloc`, `dealloc`, `put_file`, `run_program`, `panic_message`
and `partial_stdout` over plain numbers and byte buffers, and imports `env.fermium_now_ms` (for `clock()`).
`run_program` returns a length-prefixed JSON object `{stdout, warnings, error, plots}` (see
`rust/crates/fermium-wasm/src/lib.rs`). Each run gets a fresh instance of the compiled module, so nothing one run
leaves behind can change the next, as with separate `fermium run` processes.

- **Files:** `load "data/pendulum.csv"` reads from an in-memory file system that the worker fills with the
  bootcamp's and examples' data files, at the same paths as in the repository (the program runs in `/bootcamp` or
  `/examples`, as the example's folder). Files a program writes (plots) are handed back to the page and shown.
- **Warnings and errors** are formatted as `fermium run` formats them, without the file name (`line 5: …`).
- **Deep recursion:** the browser's call stack is much smaller than the 512 MB thread `fermium run` uses; a
  program that recurses too deeply (beyond about 450 levels of a simple recursive function in Chromium's worker;
  800 in Node) stops with
  "this program recurses or nests too deeply for the browser playground".
- **Numbers:** the math functions (sin, exp, …) come from Rust's pure-Rust math library instead of the C library,
  so a number printed to 16–17 digits can differ in its last digits from `fermium run` on Linux
  (`rust/DIVERGENCES.md`, "The browser playground").

## Tests

```
node web/test/compare_native.js --bin rust/target/release/fermium   # every example: page vs fermium run
conformance/run --impl rust --bin web/test/fermium-wasm --out /tmp/wasm.md   # the conformance suite on the module
python3 -m pytest tests/test_playground.py                          # build, Node, and the page in headless Chromium
cargo test -p fermium-wasm                                          # the same `run`, natively (in rust/)
```
