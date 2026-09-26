//! fermium-lsp: the Fermium language server (`fermium lsp`, used by the VS Code extension), a port of
//! `fermium/lsp.py`.
//!
//! - Errors and warnings are underlined as you type (the program is checked, never run).
//! - Hovering over a name shows its units: variables, functions (with the units of their result), ODE
//!   solutions, constants and units themselves.
//! - `\name` completes to a symbol (`\omega` → ω), and names in the program complete too.
//! - A unit/variable collision (the A1 rule) has a quick fix, the edit `fermium fmt --fix` makes.
//!
//! It speaks JSON-RPC over stdin/stdout (the Language Server Protocol) with its own small JSON reader and
//! writer (`json.rs`).
pub mod analysis;
pub mod json;
pub mod server;

pub use server::serve;
