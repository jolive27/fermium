//! `fermium doctor`: check the installation and explain it in plain English (spec §B7). Fermium 2 is one
//! self-contained program, so there is nothing to install beside it: doctor says which version this is, the LLVM
//! built into it, the platform, and runs a tiny program end to end. (v1's doctor checked Python and its packages;
//! see rust/DIVERGENCES.md.)
use std::process::ExitCode;

fn ok(msg: &str) {
    println!("  ✓ {msg}");
}

fn bad(msg: &str, fix: &str) {
    println!("  ✗ {msg}\n      fix: {fix}");
}

pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" if os == "macOS" => "arm64 (Apple silicon)",
        a => a,
    };
    format!("{os} {arch}")
}

pub fn llvm_version() -> String {
    fermium_codegen::llvm::llvm_version().to_string()
}

/// Run a program's source through the whole pipeline (the default back end), returning what it printed.
pub fn run_source(src: &str) -> Result<String, String> {
    let (prog, _) = fermium_syntax::parse(src, &[]).map_err(|e| e.message)?;
    let opts = fermium_check::CheckOptions { base_dir: ".".into(), repl: false, source_name: "<doctor>".into() };
    let (module, _) = fermium_check::check(&prog, opts).map_err(|(e, _)| e.message)?;
    let mut out: Vec<u8> = vec![];
    {
        let mut printer = fermium_codegen::printer::StdPrinter::new(&module, &mut out);
        crate::run::run_module_quiet(&module, &mut printer).map_err(|e| e.message)?;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub fn doctor() -> ExitCode {
    println!("Checking your Fermium installation...\n");
    let mut problems = 0;
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "fermium".into());
    ok(&format!("Fermium {} ({exe})", env!("CARGO_PKG_VERSION")));
    ok(&format!("LLVM {} is built in (the compiler back end)", llvm_version()));
    ok(&format!("platform: {}", platform()));
    ok("nothing else is needed: no Python, C compiler or LLVM to install");
    ok("built in too: the REPL (fermium), the language server (fermium lsp) and the Jupyter kernel \
        (fermium jupyter install)");
    println!("  - fermium build (standalone executables) is not in this version yet");
    let src = "L = 1.20 m\nT = 2.21 s\nprint 4π² L / T²\n";
    let r = std::thread::Builder::new()
        .stack_size(crate::run::STACK)
        .spawn(move || run_source(src))
        .ok()
        .and_then(|h| h.join().ok())
        .unwrap_or_else(|| Err("the test program crashed".into()));
    match r {
        Ok(got) if got.trim() == "9.70 m/s²" => ok(&format!("compiled and ran a test program: g = {}", got.trim())),
        Ok(got) => {
            problems += 1;
            bad(&format!("the test program printed '{}' instead of '9.70 m/s²'", got.trim()),
                "please report this as a bug");
        }
        Err(e) => {
            problems += 1;
            bad(&format!("couldn't run a test program ({e})"), "please report this as a bug");
        }
    }
    println!();
    if problems > 0 {
        println!("{problems} problem{} found. Fix {} and run  fermium doctor  again.",
                 if problems > 1 { "s" } else { "" }, if problems > 1 { "them" } else { "it" });
        return ExitCode::from(1);
    }
    println!("Everything looks good! Try:  fermium   (then type  print 2 m + 30 cm )");
    ExitCode::SUCCESS
}
