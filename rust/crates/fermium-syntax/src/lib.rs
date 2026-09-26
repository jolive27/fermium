//! fermium-syntax: lexer, parser, spans, Unicode/ASCII spellings and look-alike characters (spec §B4).
//! A port of `fermium/lexer.py` and `fermium/parser.py` from Fermium 1.5, the frozen oracle.

pub mod ast;
#[rustfmt::skip]
mod char_names;
pub mod diag;
pub mod lexer;
pub mod pyfmt;
pub mod sexpr;
#[rustfmt::skip]
pub mod tables;

pub use diag::{Diagnostic, Diagnostics, Fix, Severity};
pub use lexer::{tokenize, Token};

use ast::Program;

/// Parse a program. `known`: names already defined (the REPL). Returns the tree and the warnings, or the first
/// error (use [`parse_with`] to keep the warnings produced before an error).
pub fn parse(source: &str, known: &[String]) -> Result<(Program, Diagnostics), Diagnostic> {
    let mut d = Diagnostics::new();
    let prog = parse_with(source, known, &mut d)?;
    Ok((prog, d))
}

/// Parse a program, collecting warnings into `diags`.
pub fn parse_with(source: &str, _known: &[String], diags: &mut Diagnostics) -> Result<Program, Diagnostic> {
    let _toks = tokenize(source, diags)?;
    Err(Diagnostic::error("the Rust parser isn't written yet", 1, 1, 1, None))
}

/// Fix mode (`fermium fmt --fix`, D235): the edits that rewrite unit/variable collisions, and the error the parse
/// stopped on, if any.
pub fn parse_fix(_source: &str) -> (Vec<Fix>, Option<Diagnostic>) {
    (vec![], None)
}

/// The oracle's text for a program (see rust/tools/parse_oracle.py): the tree and the tokens, or the error; then
/// the warnings.
pub fn oracle_text(source: &str) -> String {
    let mut d = Diagnostics::new();
    let mut out: Vec<String> = vec![];
    match parse_with(source, &[], &mut d) {
        Ok(p) => {
            out.push("TREE".into());
            out.push(sexpr::program(&p));
        }
        Err(e) => out.push(sexpr::error(&e)),
    }
    out.push("WARNINGS".into());
    out.extend(d.warnings.iter().map(sexpr::warning));
    out.join("\n") + "\n"
}

/// The oracle's text for fix mode.
pub fn fix_text(source: &str) -> String {
    let (fixes, err) = parse_fix(source);
    let mut out = vec![];
    if let Some(e) = err {
        out.push(format!("ERROR {}:{}+{} {}", e.line.unwrap_or(0), e.col.unwrap_or(0), e.length,
                         pyfmt::json_str(&e.message)));
    }
    let items: Vec<String> = fixes.iter().map(|(a, b, r)| format!("({a} {b} {})", pyfmt::json_str(r))).collect();
    out.push(format!("FIXES {}", items.join(" ")));
    out.join("\n") + "\n"
}
