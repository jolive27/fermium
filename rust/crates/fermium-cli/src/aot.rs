//! `fermium build prog.fm [-o prog]`: compile a program ahead of time into a standalone executable (spec B5.10,
//! v1's fermium/aot.py and cli.cmd_build). The LLVM back end compiles it to an object file for this computer;
//! lld, linked into this binary, links it with the run time this binary carries (fermium-aotrt) and the C
//! start-up files. No C compiler or system linker is needed.
use std::process::ExitCode;

use fermium_syntax::diag::Diagnostic;

// the run-time library and the C start-up files, embedded by build.rs
include!(concat!(env!("OUT_DIR"), "/aot_files.rs"));

fn error(src: &str, name: &str, message: String, line: Option<u32>, hint: Option<String>) -> ExitCode {
    let d = Diagnostic { message, line, col: None, length: 1, hint, severity: fermium_syntax::Severity::Error,
                         fix: vec![] };
    eprintln!("{}", d.format(Some(src), Some(name)));
    ExitCode::from(1)
}

/// Can this binary link executables (it carries the run time, and on Linux the C start-up files)?
pub fn available() -> bool {
    !AOTRT.is_empty() && (!cfg!(target_os = "linux") || CRT.len() == 6)
}

pub fn build_file(file: &str, output: Option<&str>) -> ExitCode {
    let src = match fermium_fmt::cli::read_program(file) {
        Ok(s) => s,
        Err(code) => return ExitCode::from(code),
    };
    let path = std::path::Path::new(file);
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let out = output.map(str::to_string).unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "a.out".into())
    });
    // `fermium build noext` and `build -o prog.fm prog.fm` replaced the source with the executable (red team 13 #7)
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(path), std::fs::canonicalize(&out)) {
        if a == b {
            return error(&src, &name, format!("fermium build would overwrite the program {out} with the executable"),
                         None, Some("choose another name for the executable with  -o NAME".into()));
        }
    }
    let fail = |e: &Diagnostic, warnings: &[Diagnostic]| {
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
    let base = path.parent().map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.is_empty())
        .unwrap_or(".".into());
    // absolute, so an executable finds the program's own Python modules (use python) in the folder it was built
    // from, wherever it is run; a relative "." made it import from whatever folder it was run in (red team 13 #6)
    let base = std::fs::canonicalize(&base).map(|p| p.to_string_lossy().into_owned()).unwrap_or(base);
    let opts = fermium_check::CheckOptions { base_dir: base, repl: false, source_name: name.clone(), no_load: false };
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
    if module.uses_uncertainty {
        // v1's message (aot.py)
        return error(&src, &name, "fermium build doesn't support uncertainties (±, propagate montecarlo) yet".into(),
                     None, Some("run the program with  fermium run  instead".into()));
    }
    if AOTRT.is_empty() {
        return error(&src, &name, "this fermium binary was built without the run time of executables, so it can't \
                                   build them".into(), None,
                     Some("run the program with  fermium run  instead".into()));
    }
    let obj = match fermium_codegen::llvm::build_object(&module, &src, &name) {
        Ok(o) => o,
        Err(why) => {
            return error(&src, &name, format!("fermium build can't compile this program yet: {why}"), None,
                         Some("use  fermium run  for this program".into()));
        }
    };
    let rt = fermium_codegen::llvm::link::RuntimeFiles { archive: AOTRT, crt: CRT.to_vec() };
    if let Err(msg) = fermium_codegen::llvm::link::link_executable(&obj, &rt, &out) {
        return error(&src, &name, format!("linking the executable failed:\n{msg}"), None, None);
    }
    if std::path::Path::new(&out).is_absolute() {
        println!("built {out}");
    } else {
        println!("built {out}  (run it with ./{out})");
    }
    ExitCode::SUCCESS
}
