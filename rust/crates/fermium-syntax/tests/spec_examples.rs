//! The examples in the language specification (docs/spec/*.md) stay valid (spec D1, DECISIONS D350).
//!
//! Every ```fermium block must parse with the Rust front end. Blocks marked ```fermium-error are examples of
//! rejected programs. With `FERMIUM_BIN` set to a `fermium` binary, every ```fermium block must also run without
//! an error (exit 0) and every ```fermium-error block must fail (non-zero exit); without it only parsing is
//! checked, so this test needs nothing but this crate.
use std::path::{Path, PathBuf};

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec")
}

/// (file, 1-based line of the fence, info string, code) for every fenced block whose info string starts with
/// `fermium`.
fn blocks() -> Vec<(String, usize, String, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(spec_dir())
        .expect("docs/spec exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    let mut out = vec![];
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let mut lines = text.lines().enumerate();
        while let Some((i, line)) = lines.next() {
            let Some(info) = line.strip_prefix("```") else { continue };
            let info = info.trim().to_string();
            let mut code = String::new();
            for (_, l) in lines.by_ref() {
                if l.starts_with("```") {
                    break;
                }
                code.push_str(l);
                code.push('\n');
            }
            if info == "fermium" || info == "fermium-error" {
                out.push((name.clone(), i + 1, info, code));
            }
        }
    }
    out
}

#[test]
fn spec_has_examples() {
    let b = blocks();
    assert!(b.iter().filter(|x| x.2 == "fermium").count() >= 10, "docs/spec should have examples");
    assert!(b.iter().any(|x| x.2 == "fermium-error"), "docs/spec should show rejected programs");
}

#[test]
fn spec_examples_parse() {
    let mut bad = vec![];
    for (file, line, info, code) in blocks() {
        if info != "fermium" {
            continue;
        }
        if let Err(e) = fermium_syntax::parse(&code, &[]) {
            bad.push(format!("{file}:{line}: {} (line {:?} of the block)", e.message, e.line));
        }
    }
    assert!(bad.is_empty(), "spec examples that don't parse:\n{}", bad.join("\n"));
}

#[test]
fn spec_examples_run() {
    let Ok(bin) = std::env::var("FERMIUM_BIN") else {
        eprintln!("FERMIUM_BIN not set: the spec examples were parsed, not run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("fermium-spec-examples-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut bad = vec![];
    for (k, (file, line, info, code)) in blocks().into_iter().enumerate() {
        let path = dir.join(format!("example{k}.fm"));
        std::fs::write(&path, &code).unwrap();
        let out = std::process::Command::new(&bin).arg("run").arg(&path).current_dir(&dir).output()
            .expect("run FERMIUM_BIN");
        let ok = out.status.success();
        if info == "fermium" && !ok {
            bad.push(format!("{file}:{line}: failed:\n{}", String::from_utf8_lossy(&out.stderr)));
        } else if info == "fermium-error" && ok {
            bad.push(format!("{file}:{line}: should be rejected but ran:\n{}", String::from_utf8_lossy(&out.stdout)));
        } else if info == "fermium-error" {
            // semantics.md §5: a rejected program gets the one-line error "<file>, line N: …", not a crash
            let err = String::from_utf8_lossy(&out.stderr);
            let first = err.lines().find(|l| !l.starts_with("warning") && !l.starts_with(' ')).unwrap_or("");
            if !first.starts_with(&format!("example{k}.fm, line ")) || out.status.code() != Some(1) {
                bad.push(format!("{file}:{line}: not a one-line error with exit status 1:\n{err}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(bad.is_empty(), "spec examples:\n{}", bad.join("\n"));
}
