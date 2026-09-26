//! fermium-syntax: lexer, parser, spans, Unicode/ASCII spellings and look-alike characters (spec §B4).
//! A port of `fermium/lexer.py` and `fermium/parser.py` from Fermium 1.5, the frozen oracle.

pub mod ast;
#[rustfmt::skip]
mod char_names;
pub mod diag;
pub mod expr;
pub mod lexer;
pub mod parser;
pub mod pyfmt;
pub mod sexpr;
#[rustfmt::skip]
pub mod tables;
pub mod unitrule;
pub mod units;

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
pub fn parse_with(source: &str, known: &[String], diags: &mut Diagnostics) -> Result<Program, Diagnostic> {
    parse_tokens(source, known, diags).map(|(p, _)| p)
}

/// Parse and also return the token list (for the formatter): `parse_tokens` of the Python parser.
pub fn parse_tokens(source: &str, known: &[String], diags: &mut Diagnostics)
                    -> Result<(Program, Vec<Token>), Diagnostic> {
    let toks = tokenize(source, diags)?;
    let mut p = parser::Parser::new(toks, std::mem::take(diags), known);
    let r = p.parse_program();
    *diags = std::mem::take(&mut p.diags);
    r.map(|prog| (prog, p.toks))
}

/// Fix mode (`fermium fmt --fix`, D235): the edits that rewrite unit/variable collisions, and the error the parse
/// stopped on, if any. A lexer error is returned as the error with no edits.
pub fn parse_fix(source: &str) -> (Vec<Fix>, Option<Diagnostic>) {
    let mut d = Diagnostics::new();
    let toks = match tokenize(source, &mut d) {
        Ok(t) => t,
        Err(e) => return (vec![], Some(e)),
    };
    let mut p = parser::Parser::new(toks, d, &[]);
    p.fix_mode = true;
    let err = p.parse_program().err();
    (p.fixes, err)
}

/// The oracle's text for a program (see rust/tools/parse_oracle.py): the tree and the tokens, or the error; then
/// the warnings.
pub fn oracle_text(source: &str) -> String {
    let mut d = Diagnostics::new();
    let mut out: Vec<String> = vec![];
    match parse_tokens(source, &[], &mut d) {
        Ok((p, toks)) => {
            out.push("TREE".into());
            out.push(sexpr::program(&p));
            out.push("TOKENS".into());
            out.extend(toks.iter().map(sexpr::token));
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
