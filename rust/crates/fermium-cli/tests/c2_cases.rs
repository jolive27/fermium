//! Phase C (v2.5) groundwork, opt-in with FERMIUM_C2=1 (spec §C2): the programs in rust/c2-cases/<area>/*.fm run
//! with both back ends and must print exactly their .json (stdout, stderr, exit), in the conformance suite's format.
//! They are kept out of conformance/ because that suite pins Fermium 1.5's behaviour; without FERMIUM_C2 the
//! v1 behaviour is unchanged (checked below).
use std::path::{Path, PathBuf};
use std::process::Command;

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c2-cases");
    let mut out = vec![];
    for area in std::fs::read_dir(&root).unwrap() {
        for f in std::fs::read_dir(area.unwrap().path()).unwrap() {
            let p = f.unwrap().path();
            if p.extension().is_some_and(|e| e == "fm") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn field(json: &str, key: &str) -> String {
    // the .json files are written by a script with one "key": value per line; values are JSON strings or numbers
    let line = json.lines().find(|l| l.trim_start().starts_with(&format!("\"{key}\""))).expect(key);
    let v = line.split_once(':').unwrap().1.trim().trim_end_matches(',');
    if !v.starts_with('"') {
        return v.to_string();
    }
    let mut out = String::new();
    let mut it = v[1..v.len() - 1].chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let h: String = it.by_ref().take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

fn run(p: &Path, backend: &str, c2: bool) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fermium"));
    cmd.args(["run", p.file_name().unwrap().to_str().unwrap()]).current_dir(p.parent().unwrap())
       .env("FERMIUM_BACKEND", backend);
    if c2 {
        cmd.env("FERMIUM_C2", "1");
    } else {
        cmd.env_remove("FERMIUM_C2");
    }
    let o = cmd.output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

#[test]
fn c2_cases_print_their_expected_output() {
    let cases = cases();
    assert!(cases.len() >= 5);
    let mut bad = vec![];
    for p in &cases {
        let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
        let want = (field(&json, "stdout"), field(&json, "stderr"), field(&json, "exit").parse::<i32>().unwrap());
        for backend in ["interp", "auto"] {
            let got = run(p, backend, true);
            if got != want {
                bad.push(format!("{} ({backend}):\n  want {want:?}\n  got  {got:?}", p.display()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn without_the_opt_in_multi_line_derivatives_stay_v1_errors() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c2-cases/autodiff/basics.fm");
    let (out, err, code) = run(&p, "interp", false);
    assert_eq!(code, 1, "{out}{err}");
    assert!(err.contains("can only differentiate one-line functions like f(x) = ..., and speed is defined over \
                          several lines"), "{err}");
}
