//! Multiple dispatch (spec §C5, DECISIONS D285): the programs in rust/c-cases/c5/*.fm run with both back ends and
//! must print exactly their .json (stdout, stderr, exit), in the conformance suite's format (regen.py writes them).
//! Also: `fermium fmt` round trips the new `x: vector [m]` parameters, a plot of a function with several versions,
//! and the REPL's redefinitions.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c5")
}

fn cases() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir())
        .unwrap()
        .map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "fm"))
        .collect();
    out.sort();
    out
}

fn field(json: &str, key: &str) -> String {
    // one "key": value per line; values are JSON strings or numbers
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

fn fermium(args: &[&str], cwd: &Path, backend: &str) -> (String, String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .args(args)
        .current_dir(cwd)
        .env("FERMIUM_BACKEND", backend)
        .output()
        .unwrap();
    (String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

#[test]
fn c5_cases_print_their_expected_output() {
    let cases = cases();
    assert!(cases.len() >= 10);
    let mut bad = vec![];
    for p in &cases {
        let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
        let want = (field(&json, "stdout"), field(&json, "stderr"), field(&json, "exit").parse::<i32>().unwrap());
        for backend in ["interp", "auto"] {
            let got = fermium(&["run", p.file_name().unwrap().to_str().unwrap()], &dir(), backend);
            if got != want {
                bad.push(format!("{} ({backend}):\n  want {want:?}\n  got  {got:?}", p.display()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn fmt_round_trips_versions_and_kinds() {
    let tmp = std::env::temp_dir().join(format!("fermium-c5-fmt-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for p in cases().into_iter().filter(|p| !p.ends_with("error_kindword.fm")) {
        // --pretty is the canonical form: --ascii then --pretty gives it back
        let (src, err, code) = fermium(&["fmt", "--pretty", p.to_str().unwrap()], &tmp, "auto");
        assert_eq!(code, 0, "{}: {err}", p.display());
        let (ascii, err, code) = fermium(&["fmt", "--ascii", p.to_str().unwrap()], &tmp, "auto");
        assert_eq!(code, 0, "{}: {err}", p.display());
        let a = tmp.join("a.fm");
        std::fs::write(&a, &ascii).unwrap();
        let (pretty, err, code) = fermium(&["fmt", "--pretty", a.to_str().unwrap()], &tmp, "auto");
        assert_eq!(code, 0, "{}: {err}", p.display());
        assert_eq!(pretty, src, "{}: --ascii then --pretty changed the program", p.display());
    }
    // the ASCII form runs and prints the same
    let p = dir().join("kinds.fm");
    let (ascii, _, _) = fermium(&["fmt", "--ascii", p.to_str().unwrap()], &tmp, "auto");
    assert!(ascii.contains("size(r: vector) = |r|"), "{ascii}");
    std::fs::write(tmp.join("kinds.fm"), &ascii).unwrap();
    let want = field(&std::fs::read_to_string(p.with_extension("json")).unwrap(), "stdout");
    assert_eq!(fermium(&["run", "kinds.fm"], &tmp, "auto").0, want);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn plot_of_a_function_with_versions_uses_the_version_for_the_range() {
    let tmp = std::env::temp_dir().join(format!("fermium-c5-plot-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("p.fm"), "U(x [m]) = ½ (4 N/m) x²\nU(t [s]) = 3 J/s * t\n\
                                       plot U vs x from 0 m to 2 m to \"u.svg\"\n\
                                       plot U vs t from 0 s to 2 s to \"t.svg\"\n").unwrap();
    for backend in ["interp", "auto"] {
        let (out, err, code) = fermium(&["run", "p.fm"], &tmp, backend);
        assert_eq!(code, 0, "{out}{err}");
        let u = std::fs::read_to_string(tmp.join("u.svg")).unwrap();
        let t = std::fs::read_to_string(tmp.join("t.svg")).unwrap();
        assert!(u.contains("<svg") && t.contains("<svg"));
        // U(x [m]) peaks at ½ (4 N/m)(2 m)² = 8 J; U(t [s]) at 6 J
        assert!(u.contains(">x [m]<") && u.contains(">U [J]<") && u.contains(">8<"), "the length version");
        assert!(t.contains(">t [s]<") && t.contains(">6<") && !t.contains(">8<"), "the time version");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn repl_adds_versions_and_replaces_same_signatures() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap()
         .write_all("f(x) = 2 x\nf(x) = 3 x\nprint f(2)\nf(x [m], y [m]) = x y\nprint f(2 m, 3 m)\nprint f(5)\n\
                     f(z: vector) = |z|\nprint f(<3, 4> m)\n".as_bytes())
         .unwrap();
    let o = child.wait_with_output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    for want in ["6", "6 m²", "15", "5 m"] {
        assert!(out.lines().any(|l| l.trim().trim_start_matches('>').trim() == want), "missing {want}:\n{out}\n{err}");
    }
}
