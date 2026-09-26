//! `fermium run`: parse → check → run (the tree-walking back end for now; LLVM next), printing warnings and
//! errors exactly as Fermium 1.5 does (driver.run_source, cli.cmd_run).
use std::process::ExitCode;

use fermium_syntax::diag::Diagnostic;

pub fn run_file(file: &str, base_dir: Option<&str>) -> ExitCode {
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("can't find the file '{file}'");
            return ExitCode::from(1);
        }
    };
    let name = std::path::Path::new(file).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let fail = |e: &Diagnostic, warnings: &[Diagnostic]| {
        // the warnings collected before the error often explain it
        for w in warnings {
            eprintln!("{}", w.format(Some(&src), None));
        }
        eprintln!("{}", e.format(Some(&src), Some(&name)));
        ExitCode::from(1)
    };
    let (prog, pdiags) = match fermium_syntax::parse(&src, &[]) {
        Ok(x) => x,
        Err(e) => return fail(&e, &[]),
    };
    let base = base_dir.map(str::to_string).unwrap_or_else(|| {
        std::path::Path::new(file).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or(".".into())
    });
    let opts = fermium_check::CheckOptions { base_dir: base, repl: false, source_name: name.clone() };
    let (module, cdiags) = match fermium_check::check(&prog, opts) {
        Ok(x) => x,
        Err((e, d)) => {
            let mut ws = pdiags.warnings.clone();
            ws.extend(d.warnings);
            return fail(&e, &ws);
        }
    };
    for w in pdiags.warnings.iter().chain(cdiags.warnings.iter()) {
        eprintln!("{}", w.format(Some(&src), None));
    }
    let stdout = std::io::stdout();
    let printer = fermium_codegen::printer::StdPrinter::new(&module, std::io::BufWriter::new(stdout.lock()));
    let mut interp = fermium_codegen::eval::Interpreter::new(&module, printer);
    let r = interp.run();
    drop(interp);
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let d = Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                                 length: 1, hint: e.hint, severity: fermium_syntax::Severity::Error, fix: vec![] };
            eprintln!("{}", d.format(Some(&src), Some(&name)));
            ExitCode::from(1)
        }
    }
}
