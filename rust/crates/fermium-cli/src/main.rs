//! `fermium`: the command-line tool (spec §B4, fermium-cli).
use std::process::ExitCode;

mod run;

use fermium_syntax::{sexpr, Diagnostics};

fn usage() -> ExitCode {
    eprintln!("usage: fermium run [--base-dir DIR] [--backend llvm|interp] FILE.fm\n       fermium parse [--oracle|--tokens|--fix] FILE.fm\n       fermium fmt [--pretty|--ascii] [--fix] [-w] FILE.fm");
    ExitCode::from(2)
}

fn read(file: &str) -> Option<String> {
    match std::fs::read(file) {
        Ok(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Err(_) => {
            eprintln!("can't find the file '{file}'");
            None
        }
    }
}

fn basename(file: &str) -> &str {
    std::path::Path::new(file).file_name().and_then(|s| s.to_str()).unwrap_or(file)
}

/// `fermium parse FILE`: the tree as S-expressions; a parse error is printed as `fermium check` prints it.
/// `--oracle`: the full comparison format of rust/tools/parse_oracle.py (tree, tokens, warnings).
/// `--tokens`: only the lexer. `--fix`: fix mode (`fmt --fix`), the edits.
fn cmd_parse(args: &[String]) -> ExitCode {
    let mode = args.iter().find(|a| a.starts_with("--")).cloned().unwrap_or_default();
    let Some(file) = args.iter().find(|a| !a.starts_with("--")) else { return usage() };
    let Some(src) = read(file) else { return ExitCode::from(1) };
    match mode.as_str() {
        "--tokens" => {
            let mut d = Diagnostics::new();
            let mut out = vec![];
            match fermium_syntax::tokenize(&src, &mut d) {
                Ok(toks) => {
                    out.push("TOKENS".to_string());
                    out.extend(toks.iter().map(sexpr::token));
                }
                Err(e) => out.push(sexpr::error(&Diagnostic { fix: vec![], ..e })),
            }
            out.push("WARNINGS".into());
            out.extend(d.warnings.iter().map(sexpr::warning));
            println!("{}", out.join("\n"));
            ExitCode::SUCCESS
        }
        "--oracle" => {
            print!("{}", fermium_syntax::oracle_text(&src));
            ExitCode::SUCCESS
        }
        "--fix" => {
            print!("{}", fermium_syntax::fix_text(&src));
            ExitCode::SUCCESS
        }
        "" => match fermium_syntax::parse(&src, &[]) {
            Ok((prog, diags)) => {
                println!("{}", sexpr::program(&prog));
                let mut ws: Vec<_> = diags.warnings.iter().collect();
                ws.sort_by_key(|w| (w.line.unwrap_or(0), w.col.unwrap_or(0)));
                for w in ws {
                    eprintln!("{}", w.format(Some(&src), None));
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{}", e.format(Some(&src), Some(basename(file))));
                ExitCode::from(1)
            }
        },
        _ => usage(),
    }
}

use fermium_syntax::Diagnostic;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("fermium {} (Rust)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("parse") => cmd_parse(&args[1..]),
        Some("fmt") => ExitCode::from(fermium_fmt::cli::cmd_fmt(&args[1..])),
        Some("run") => {
            let mut file = None;
            let mut base = None;
            let mut backend = run::BackendChoice::from_env();
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                if a == "--base-dir" {
                    base = it.next().cloned();
                } else if a == "--backend" {
                    match it.next().and_then(|s| run::BackendChoice::parse(s)) {
                        Some(b) => backend = b,
                        None => return usage(),
                    }
                } else {
                    file = Some(a.clone());
                }
            }
            let Some(file) = file else { return usage() };
            run::run_file(&file, base.as_deref(), backend)
        }
        _ => usage(),
    }
}
