//! `fermium`: the command-line tool (spec §B4, fermium-cli).
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage: fermium run [--base-dir DIR] FILE.fm");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("fermium {} (Rust)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("run") => {
            let mut file = None;
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                if a == "--base-dir" {
                    it.next();
                } else {
                    file = Some(a.clone());
                }
            }
            let Some(file) = file else { return usage() };
            if std::fs::read_to_string(&file).is_err() {
                eprintln!("can't find the file '{file}'");
                return ExitCode::from(1);
            }
            eprintln!("{file}, line 1: the Rust compiler can't run programs yet (Phase B in progress)");
            ExitCode::from(1)
        }
        _ => usage(),
    }
}
