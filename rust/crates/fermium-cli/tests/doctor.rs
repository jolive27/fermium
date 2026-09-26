//! `fermium doctor` (spec §B7): reports the version, the LLVM built into the binary, the platform, that nothing
//! external is needed (Python only for `use python`), and runs a test program end to end.
use std::process::Command;

fn doctor(env: &[(&str, &str)]) -> (String, i32) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fermium"));
    c.arg("doctor").env_remove("FERMIUM_PYTHON");
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().expect("fermium doctor didn't start");
    (String::from_utf8_lossy(&o.stdout).into_owned(), o.status.code().unwrap_or(-1))
}

#[test]
fn doctor_reports_version_llvm_platform_and_needs_nothing_else() {
    let (out, code) = doctor(&[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&format!("✓ Fermium {} (", env!("CARGO_PKG_VERSION"))), "{out}");
    assert!(out.contains("✓ LLVM 18."), "the embedded LLVM version: {out}");
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => "Linux",
        o => o,
    };
    assert!(out.contains(&format!("✓ platform: {os} ")), "{out}");
    assert!(out.contains("nothing else is needed: no Python, C compiler or LLVM to install"), "{out}");
    assert!(out.contains("Python is optional, only for programs that say  use python"), "{out}");
    assert!(out.contains("compiled and ran a test program: g = 9.70 m/s²"), "{out}");
    assert!(out.contains("Everything looks good!"), "{out}");
}

#[test]
fn doctor_without_python_is_still_healthy() {
    // an empty PATH (as the conformance runner uses): doctor may still find a system python3 in the usual places,
    // but either way Python is reported as optional and doctor succeeds
    let empty = std::env::temp_dir().join("fermium-doctor-empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    let (out, code) = doctor(&[("PATH", empty.to_str().unwrap())]);
    assert_eq!(code, 0, "{out}");
    let line = out.lines().find(|l| l.contains("Python is optional")).expect("a Python line");
    assert!(line.contains("found /") || line.contains("none found (that's fine)"), "{line}");
    assert!(!out.contains('✗'), "{out}");
}

#[test]
fn doctor_names_the_python_use_python_would_use() {
    let (out, code) = doctor(&[("FERMIUM_PYTHON", "/opt/some/python3")]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("use python : found /opt/some/python3"), "{out}");
}
