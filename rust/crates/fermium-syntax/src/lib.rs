//! fermium-syntax: lexer, parser, spans, Unicode/ASCII spellings and look-alike characters (spec §B4).
//! A port of `fermium/lexer.py` and `fermium/parser.py` from Fermium 1.5, the frozen oracle.

pub mod ast;
pub mod diag;

pub use diag::{Diagnostic, Diagnostics, Severity};
