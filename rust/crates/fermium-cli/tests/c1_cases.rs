//! Spec C1 (memory and data structures): the programs in rust/c-cases/c1/*.fm run with both back ends and must
//! print exactly their .json (stdout, stderr, exit), in the conformance suite's format. The LLVM back end is forced
//! (`FERMIUM_BACKEND=llvm`), so a program it can't compile fails, unless the .json says `"llvm": "auto"` (then the
//! default choice runs, which may be the tree-walker). They also run with `FERMIUM_GC_STRESS=1`, which makes the
//! compiled code collect at every safe point, so a list freed too early shows up as a wrong answer or a crash.
//! memory_*.fm are run apart: their lists must stay in bounded memory (DECISIONS D280).
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c1")
}

fn cases() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root()).unwrap().map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "fm")).collect();
    out.sort();
    out
}

/// One `"key": value` line of a case's .json (written one key per line); None when the key is absent.
fn field(json: &str, key: &str) -> Option<String> {
    let line = json.lines().find(|l| l.trim_start().starts_with(&format!("\"{key}\"")))?;
    let v = line.split_once(':').unwrap().1.trim().trim_end_matches(',');
    if !v.starts_with('"') {
        return Some(v.to_string());
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
    Some(out)
}

fn run(p: &Path, backend: &str, env: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fermium"));
    cmd.args(["run", p.file_name().unwrap().to_str().unwrap()]).current_dir(p.parent().unwrap())
       .env("FERMIUM_BACKEND", backend).env_remove("FERMIUM_GC_STRESS").env_remove("FERMIUM_GC_STATS");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

#[test]
fn c1_cases_print_their_expected_output_on_both_back_ends() {
    let cases: Vec<PathBuf> = cases().into_iter()
        .filter(|p| !p.file_name().unwrap().to_str().unwrap().starts_with("memory_")).collect();
    assert!(cases.len() >= 8);
    let mut bad = vec![];
    for p in &cases {
        let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
        let want = (field(&json, "stdout").unwrap(), field(&json, "stderr").unwrap(),
                    field(&json, "exit").unwrap().parse::<i32>().unwrap());
        let llvm = field(&json, "llvm").unwrap_or_else(|| "llvm".into());
        for (backend, env) in [("interp", vec![]), (llvm.as_str(), vec![]), (llvm.as_str(), vec![("FERMIUM_GC_STRESS", "1")])] {
            let got = run(p, backend, &env);
            if got != want {
                bad.push(format!("{} ({backend} {env:?}):\n  want {want:?}\n  got  {got:?}", p.display()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The collector's report (FERMIUM_GC_STATS=1): collections run, and the peak memory in MB (Linux).
fn gc_report(err: &str) -> (u64, Option<u64>) {
    let line = err.lines().find(|l| l.starts_with("fermium: gc: ")).unwrap_or_else(|| panic!("no gc report in {err}"));
    let runs = line["fermium: gc: ".len()..].split_whitespace().next().unwrap().parse().unwrap();
    let peak = line.split("peak memory ").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok());
    (runs, peak)
}

#[test]
fn compiled_lists_are_freed_a_million_list_loop_stays_in_bounded_memory() {
    let p = root().join("memory_loop.fm");
    let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
    let want = field(&json, "stdout").unwrap();
    let (out, err, code) = run(&p, "llvm", &[("FERMIUM_GC_STATS", "1")]);
    assert_eq!((code, out.as_str()), (0, want.as_str()), "{err}");
    let (runs, peak) = gc_report(&err);
    assert!(runs > 10, "{err}");
    if let Some(mb) = peak {
        // 10⁶ lists of 100 numbers are 800 MB if none is freed
        assert!(mb < 250, "{err}");
    }
    let (out, err, code) = run(&p, "interp", &[]);
    assert_eq!((code, out.as_str()), (0, want.as_str()), "{err}");
}
