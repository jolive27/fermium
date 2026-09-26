//! `fermium run`: parse → check → run, printing warnings and errors exactly as Fermium 1.5 does
//! (driver.run_source, cli.cmd_run).
//!
//! The back end: the LLVM JIT (fermium-codegen::llvm) when it compiles every construct of the program, else the
//! tree-walker (so a construct the LLVM back end doesn't have yet never changes what a program prints).
//! `--backend llvm|interp` or `FERMIUM_BACKEND=llvm|interp` forces one; forcing llvm on a program it can't
//! compile stops with exit code 3 (the differential tests use this).
use std::process::ExitCode;

use fermium_codegen::eval::RunError;
use fermium_syntax::diag::Diagnostic;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BackendChoice {
    Auto,
    Llvm,
    Interp,
}

impl BackendChoice {
    pub fn parse(s: &str) -> Option<BackendChoice> {
        match s {
            "auto" | "" => Some(BackendChoice::Auto),
            "llvm" => Some(BackendChoice::Llvm),
            "interp" => Some(BackendChoice::Interp),
            _ => None,
        }
    }
    /// From FERMIUM_BACKEND when no --backend was given.
    pub fn from_env() -> BackendChoice {
        std::env::var("FERMIUM_BACKEND").ok().and_then(|s| BackendChoice::parse(&s)).unwrap_or(BackendChoice::Auto)
    }
}

/// Stack of the thread programs run on: deep recursion gets a clear error from the compiled code (400 MB used,
/// as in v1) instead of a crash.
const STACK: usize = 512 << 20;

pub fn run_file(file: &str, base_dir: Option<&str>, backend: BackendChoice) -> ExitCode {
    let (file, base) = (file.to_string(), base_dir.map(str::to_string));
    fermium_codegen::eval::STACK_LIMIT.store(400 << 20, std::sync::atomic::Ordering::Relaxed);
    let h = std::thread::Builder::new().stack_size(STACK).spawn(move || run_file_here(&file, base.as_deref(), backend));
    match h {
        Ok(h) => h.join().unwrap_or(ExitCode::from(101)),
        Err(_) => ExitCode::from(101),
    }
}

fn run_file_here(file: &str, base_dir: Option<&str>, backend: BackendChoice) -> ExitCode {
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
    let mut printer = fermium_codegen::printer::StdPrinter::new(&module, std::io::BufWriter::new(stdout.lock()));
    let r = run_module(&module, &mut printer, backend);
    drop(printer);
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(Stop::Unsupported(why)) => {
            eprintln!("fermium: the LLVM back end can't compile this program yet: {why}");
            ExitCode::from(3)
        }
        Err(Stop::Error(e)) => {
            let d = Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                                 length: 1, hint: e.hint, severity: fermium_syntax::Severity::Error, fix: vec![] };
            eprintln!("{}", d.format(Some(&src), Some(&name)));
            ExitCode::from(1)
        }
    }
}

enum Stop {
    Unsupported(String),
    Error(RunError),
}

fn run_module(module: &fermium_ir::Module, printer: &mut dyn fermium_codegen::eval::Printer, backend: BackendChoice)
              -> Result<(), Stop> {
    use fermium_codegen::Backend;
    // FERMIUM_BACKEND_INFO=1: say which back end ran (stderr); =PATH: write its name to that file (tests)
    let info = std::env::var("FERMIUM_BACKEND_INFO").ok();
    let say = |what: &str| match info.as_deref() {
        Some("1") => eprintln!("fermium: backend {what}"),
        Some(path) => {
            let _ = std::fs::write(path, what);
        }
        None => {}
    };
    if backend == BackendChoice::Interp {
        say("interp");
    }
    if backend != BackendChoice::Interp {
        // run_module rejects a module it can't compile before running anything
        match fermium_codegen::llvm::run_module(module, printer) {
            Ok(r) => {
                say("llvm");
                return r.map_err(Stop::Error);
            }
            Err(why) => {
                if backend == BackendChoice::Llvm {
                    return Err(Stop::Unsupported(why));
                }
                say(&format!("interp ({why})"));
            }
        }
    }
    fermium_codegen::InterpBackend.run(module, printer).map_err(Stop::Error)
}
