# fermium-cli: design notes

Decisions for the command-line tools (the CLI, the REPL, the language server and the Jupyter kernel), in the
style of DECISIONS.md: what, why, alternatives.

## Crates

- `fermium-cli`: the `fermium` binary: argument handling (hand-written, argparse's messages), `run`, `check`,
  `doctor`, and dispatch to the others.
- `fermium-repl`: the REPL loop, its line editor, and `Session` (v1's `driver.ReplSession`), shared with the
  Jupyter kernel. A crate of its own because the kernel runs the same session.
- `fermium-lsp`: the language server, and the small JSON reader/writer it shares with the kernel.
- `fermium-jupyter`: the kernel, with its own ZMTP (ZeroMQ wire protocol) and HMAC-SHA256.

Hooks in other crates (small, in files of their own): `fermium-check/src/api.rs` (check and keep the checker;
`type_text`; names in scope), `#[derive(Clone)]` on `Checker` (the REPL's rollback, D220),
`fermium-codegen/src/session.rs` (run inputs one after another with the globals kept; `emit_llvm`),
`fermium-syntax/src/symbols.rs` (the `\name` table of fermium/symbols.py).

## The REPL session

- **What:** one `Checker` in REPL mode lives for the whole session. After each input the checked module is
  put back into the checker (`check_program` takes it out), so the next input's symbols and functions extend
  it and ids stay valid; the tree-walker runs the new main program with the previous values
  (`ReplState`: values by symbol id, and the ODE solutions).
- **Why:** it is v1's design (one checker, an arena of globals) without a JIT per input, and it needs nothing
  new in the checker. A failed input is rolled back by restoring a clone of the checker taken before it.
- **Alternatives:** re-checking and re-running every earlier input (prints again, repeats random numbers);
  compiling each input with the LLVM back end (needs arena variables in codegen, not there yet).

## Line editing: our own small editor, no readline

- **What:** `fermium-repl/src/editor.rs`, ~300 lines: raw mode through termios (the `libc` crate, already in
  the dependency tree through LLVM's build), arrow keys, Home/End, Ctrl-A/E/K/U/W/L/C/D, history in
  `~/.fermium_history`, `\name` + Tab.
- **Why:** spec §B1 (zero runtime dependencies: no libreadline/libedit), and the REPL needs little.
- **Alternatives:** `rustyline` is pure Rust but pulls in ~10 crates (nix, unicode-width, …) for features the
  REPL doesn't use; `linefeed`/`reedline` are larger still.
- **Limits:** a line wider than the terminal is redrawn less neatly than readline; no Windows console editing
  (lines are read plainly there).

## JSON: our own reader and writer

- **What:** `fermium-lsp/src/json.rs`: a `Json` value, a parser and a compact writer (~250 lines).
- **Why:** the language server and the kernel only exchange small messages; a dependency-free crate keeps the
  build light. serde/serde_json would be acceptable (pure Rust) but bring proc-macros for little benefit.

## Jupyter: ZMTP 3.1 and HMAC-SHA256 of our own

- **What:** `fermium-jupyter`: ZMTP 3.1 over TCP with the NULL mechanism, the socket types Jupyter uses
  (ROUTER for shell/control/stdin, PUB for iopub, REP for heartbeat), and SHA-256/HMAC (FIPS 180-4, RFC 2104,
  tested with the standard vectors).
- **Why:** spec §B1: no libzmq. Jupyter's kernels only need this subset.
- **Alternatives:** the `zmq` crate (binds libzmq, a C++ library); `zeromq` (pure Rust, but async on tokio);
  `sha2`/`hmac` crates (fine, but 60 lines of SHA-256 are simpler than two more dependencies).

## How the tools are tested against v1

- CLI: messages, help texts and exit codes were compared with `python3 -m fermium ...` case by case.
- REPL: `cargo test -p fermium-repl` replays 53 scripted sessions (fixtures from
  `python3 rust/tools/repl_sessions.py`, run through v1's REPL); `python3 rust/tools/repl_pty.py` drives the
  line editor through a pseudo-terminal.
- Language server: `cargo test -p fermium-lsp` replays 4 sessions against v1's replies (fixtures from
  `python3 rust/tools/lsp_session.py --write-fixtures`; `lsp_session.py v1|BINARY` prints a server's replies).
- Jupyter: `cargo test -p fermium-jupyter` plays a client over TCP; `python3 rust/tools/jupyter_e2e.py` runs
  the kernel under the real jupyter_client.

## Not done yet

- Interrupting a running cell in Jupyter (the interrupt is acknowledged; the cell runs on). Ctrl+C in the REPL
  stops the whole session, as v1 did.
- The REPL and the kernel run inputs on the tree-walker (the LLVM back end doesn't compile arena variables).
