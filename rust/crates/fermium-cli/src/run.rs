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

/// Stack of the thread programs run on (reserved address space; pages are used only as the stack grows): deep
/// recursion gets a clear error instead of a crash, from the compiled code at 400 MB used (as in v1), from the
/// tree-walker at TREE_STACK (its frames are bigger than compiled ones, so it gets more room: recursion a million
/// deep runs, as in v1's compiled code).
pub const STACK: usize = 2 << 30;
pub const TREE_STACK: usize = 1900 << 20;

/// `fermium run`'s options (cli.py cmd_run, plus the back-end choice and --base-dir of the conformance runner).
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub base_dir: Option<String>,
    pub backend: BackendChoice,
    /// --time: how long each stage took (stderr)
    pub time: bool,
    /// --emit-llvm: print the LLVM IR instead of running
    pub emit_llvm: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions { base_dir: None, backend: BackendChoice::Auto, time: false, emit_llvm: false }
    }
}

pub fn run_file(file: &str, o: RunOptions) -> ExitCode {
    let file = file.to_string();
    fermium_repl::stop_on_ctrl_c();
    fermium_codegen::eval::STACK_LIMIT.store(TREE_STACK, std::sync::atomic::Ordering::Relaxed);
    let h = std::thread::Builder::new().stack_size(STACK).spawn(move || run_file_here(&file, &o));
    match h {
        Ok(h) => h.join().unwrap_or(ExitCode::from(101)),
        Err(_) => ExitCode::from(101),
    }
}

fn run_file_here(file: &str, o: &RunOptions) -> ExitCode {
    let (base_dir, backend) = (o.base_dir.as_deref(), o.backend);
    let src = match fermium_fmt::cli::read_program(file) {
        Ok(s) => s,
        Err(code) => return ExitCode::from(code),
    };
    let t0 = std::time::Instant::now();
    let name = std::path::Path::new(file).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let fail = |e: &Diagnostic, warnings: &[Diagnostic]| {
        // the warnings collected before the error often explain it
        for w in warnings {
            eprintln!("{}", w.format(Some(&src), None));
        }
        eprintln!("{}", e.format(Some(&src), Some(&name)));
        ExitCode::from(1)
    };
    let base = base_dir.map(str::to_string).unwrap_or_else(|| {
        // `fermium run prog.fm` has an empty parent: the current folder, absolute (like v1's os.path.abspath), so
        // `use python` finds a .py file beside the program
        let dir = std::path::Path::new(file).parent().filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        std::path::absolute(dir).unwrap_or(dir.to_path_buf()).to_string_lossy().into_owned()
    });
    // the compile cache (D317): a program compiled by an earlier run runs from its saved machine code
    let cache = (!o.emit_llvm && backend != BackendChoice::Interp && !fermium_codegen::llvm::cache::off()).then(|| {
        let dir = fermium_check::cppinterop::cache_root().join("jit");
        let key = fermium_codegen::llvm::cache::key(&src, &name, &base);
        (dir, key)
    });
    if let Some((dir, key)) = &cache {
        if let Some(code) = run_cached(dir, key, &src, &name, o.time) {
            return code;
        }
    }
    let (prog, pdiags) = match fermium_syntax::parse(&src, &[]) {
        Ok(x) => x,
        Err(e) => return fail(&e, &[]),
    };
    let t_parse = t0.elapsed();
    let opts = fermium_check::CheckOptions { base_dir: base, repl: false, source_name: name.clone(), no_load: false };
    let (module, cdiags) = match fermium_check::check(&prog, opts) {
        Ok(x) => x,
        Err((e, d)) => {
            let mut ws = pdiags.warnings.clone();
            ws.extend(d.warnings);
            return fail(&e, &ws);
        }
    };
    let t_check = t0.elapsed() - t_parse;
    let mut warnings = String::new();
    for w in pdiags.warnings.iter().chain(cdiags.warnings.iter()) {
        let text = w.format(Some(&src), None);
        eprintln!("{text}");
        warnings += &text;
        warnings.push('\n');
    }
    if o.emit_llvm {
        return match fermium_codegen::session::emit_llvm(&module) {
            Ok(ir) => {
                println!("{ir}");
                ExitCode::SUCCESS
            }
            Err(why) => {
                eprintln!("fermium: the LLVM back end can't compile this program yet: {why}");
                ExitCode::from(3)
            }
        };
    }
    let t1 = std::time::Instant::now();
    let stdout = std::io::stdout();
    let mut printer = fermium_codegen::printer::StdPrinter::new(&module, std::io::BufWriter::new(stdout.lock()));
    let deps = fermium_check::modules::deps();
    let mut save = |c: fermium_codegen::llvm::Compiled| {
        if let Some((dir, key)) = &cache {
            fermium_codegen::llvm::cache::store(dir, key, &src, &deps.files, &warnings, &c.blob, &c.object);
        }
    };
    let saver: Saver = match &cache {
        Some(_) => Some((src.as_str(), name.as_str(), &mut save)),
        None => None,
    };
    let r = run_module(&module, &mut printer, backend, saver);
    if matches!(r, Err(Stop::Error(_))) {
        fermium_codegen::eval::Printer::flush_partial(&mut printer);
    }
    drop(printer);
    if o.time {
        // v1 splits the last stage into codegen, LLVM+JIT and run; here the back end's time is one number
        eprintln!("time: parse {:.1} ms, check {:.1} ms, run {:.1} ms (codegen, JIT and running)",
                  t_parse.as_secs_f64() * 1e3, t_check.as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3);
    }
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

/// Run a program from the compile cache, if it has an entry for this program whose dependencies are unchanged
/// (D317); None when it hasn't (then nothing was printed or run).
fn run_cached(dir: &std::path::Path, key: &str, src: &str, name: &str, time: bool) -> Option<ExitCode> {
    use fermium_codegen::llvm::{blob, cache};
    let t0 = std::time::Instant::now();
    let e = cache::lookup(dir, key, src)?;
    let b = blob::read(&e.blob).ok().filter(|b| !b.needs_ir())?;
    let stdout = std::io::stdout();
    let mut printer = fermium_codegen::printer::StdPrinter::new(&b.module, std::io::BufWriter::new(stdout.lock()));
    // the warnings are printed once the cached code is known to load (else the compilation prints them)
    let warnings = e.warnings.clone();
    let mut warn = move || {
        eprint!("{warnings}");
    };
    let r = match fermium_codegen::llvm::run_object(&b.module, b.tables, &e.object, &mut printer, &mut warn) {
        Ok(r) => r,
        Err(_) => return None,
    };
    if r.is_err() {
        fermium_codegen::eval::Printer::flush_partial(&mut printer);
    }
    drop(printer);
    say_backend("llvm");
    if time {
        eprintln!("time: compiled program from the cache, run {:.1} ms (loading and running)",
                  t0.elapsed().as_secs_f64() * 1e3);
    }
    Some(match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let d = Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                                 length: 1, hint: e.hint, severity: fermium_syntax::Severity::Error, fix: vec![] };
            eprintln!("{}", d.format(Some(src), Some(name)));
            ExitCode::from(1)
        }
    })
}

/// FERMIUM_BACKEND_INFO=1: say which back end ran (stderr); =PATH: write its name to that file (tests)
fn say_backend(what: &str) {
    match std::env::var("FERMIUM_BACKEND_INFO").ok().as_deref() {
        Some("1") => eprintln!("fermium: backend {what}"),
        Some(path) => {
            let _ = std::fs::write(path, what);
        }
        None => {}
    }
}

/// The compile cache's save callback for run_module (source, file name, callback).
type Saver<'a> = Option<(&'a str, &'a str, &'a mut dyn FnMut(fermium_codegen::llvm::Compiled))>;

fn run_module(module: &fermium_ir::Module, printer: &mut dyn fermium_codegen::eval::Printer, backend: BackendChoice,
              save: Saver) -> Result<(), Stop> {
    use fermium_codegen::Backend;
    let say = say_backend;
    // uncertain values (±, propagate montecarlo) run in the tree-walker, as v1 runs them in its interpreter (D122)
    let backend = if backend == BackendChoice::Auto && module.uses_uncertainty { BackendChoice::Interp } else { backend };
    if backend == BackendChoice::Interp {
        say("interp");
    }
    if backend != BackendChoice::Interp {
        // run_module rejects a module it can't compile before running anything
        match fermium_codegen::llvm::run_module_with(module, printer, save) {
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

/// Run a module with the default back end, without the FERMIUM_BACKEND_INFO report (`fermium doctor`).
pub fn run_module_quiet(module: &fermium_ir::Module, printer: &mut dyn fermium_codegen::eval::Printer)
                        -> Result<(), RunError> {
    match run_module(module, printer, BackendChoice::from_env(), None) {
        Ok(()) => Ok(()),
        Err(Stop::Error(e)) => Err(e),
        Err(Stop::Unsupported(why)) => Err(RunError { message: why, line: 0, hint: None }),
    }
}
