//! `fermium check FILE`: check a program's units without running it (cli.py `cmd_check`).
use std::process::ExitCode;

pub fn cmd_check(file: &str) -> ExitCode {
    let src = match fermium_fmt::cli::read_program(file) {
        Ok(s) => s,
        Err(code) => return ExitCode::from(code),
    };
    let name = std::path::Path::new(file).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (prog, pdiags) = match fermium_syntax::parse(&src, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{}", e.format(Some(&src), Some(&name)));
            return ExitCode::from(1);
        }
    };
    // imports are found next to the program (cli.py: the folder of the file's absolute path)
    let abs = std::path::absolute(file).unwrap_or_else(|_| file.into());
    let base = abs.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or(".".into());
    let opts = fermium_check::CheckOptions { base_dir: base, repl: false, source_name: name.clone(), no_load: true };
    let cdiags = match fermium_check::check(&prog, opts) {
        Ok((_, d)) => d,
        Err((e, _)) => {
            eprintln!("{}", e.format(Some(&src), Some(&name)));
            return ExitCode::from(1);
        }
    };
    let mut ws: Vec<_> = pdiags.warnings.iter().chain(cdiags.warnings.iter()).collect();
    ws.sort_by_key(|w| (w.line.unwrap_or(0), w.col.unwrap_or(0)));
    for w in &ws {
        eprintln!("{}", w.format(Some(&src), None));
    }
    let n = ws.len();
    if n > 0 {
        println!("{file}: units check out, {n} warning{} (read {} above)", if n != 1 { "s" } else { "" },
                 if n != 1 { "them" } else { "it" });
    } else {
        println!("{file}: no problems found (units check out)");
    }
    ExitCode::SUCCESS
}
