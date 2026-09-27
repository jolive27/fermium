//! Phase C (v2.5) item C2, calculus reach (DECISIONS D295-D299): the programs in rust/c-cases/c2/*.fm run with
//! both back-end settings (`interp` and `auto`) and must print exactly their .json (stdout, stderr, exit), in the
//! conformance suite's format. The programs check the physics themselves against analytic results (the `true`
//! lines): derivatives of multi-line functions by automatic differentiation, events in `solve` located on the
//! dense output, the simplifier's printed derivatives and parameter sweeps over `solve`.
//! `FERMIUM_BLESS=1` rewrites the .json files from the tree-walker's output (read them before committing).
use std::path::{Path, PathBuf};
use std::process::Command;

/// the sweep program writes SVG files next to itself: the two tests that run it take turns
static FILES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn remove_svgs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c2");
    for f in std::fs::read_dir(&dir).unwrap() {
        let p = f.unwrap().path();
        if p.extension().is_some_and(|e| e == "svg") {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c2");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "fm"))
        .collect();
    out.sort();
    out
}

fn unescape(v: &str) -> String {
    let mut out = String::new();
    let mut it = v.chars();
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

fn field(json: &str, key: &str) -> String {
    // one "key": value per line; values are JSON strings or numbers
    let line = json.lines().find(|l| l.trim_start().starts_with(&format!("\"{key}\""))).expect(key);
    let v = line.split_once(':').unwrap().1.trim().trim_end_matches(',');
    if !v.starts_with('"') {
        return v.to_string();
    }
    unescape(&v[1..v.len() - 1])
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn run(p: &Path, backend: &str) -> (String, String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .args(["run", p.file_name().unwrap().to_str().unwrap()])
        .current_dir(p.parent().unwrap())
        .env("FERMIUM_BACKEND", backend)
        .env_remove("FERMIUM_BACKEND_INFO")
        .output()
        .unwrap();
    // "plot saved to <the case's folder>/x.svg": the folder differs from checkout to checkout
    let dir = format!("{}/", p.parent().unwrap().canonicalize().unwrap().display());
    let out = String::from_utf8_lossy(&o.stdout).replace(&dir, "");
    (out, String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

#[test]
fn c2_cases_print_their_expected_output() {
    let cases = cases();
    assert!(cases.len() >= 5);
    let bless = std::env::var("FERMIUM_BLESS").is_ok_and(|v| v == "1");
    let _files = FILES.lock().unwrap_or_else(|e| e.into_inner());
    let mut bad = vec![];
    for p in &cases {
        let jp = p.with_extension("json");
        if bless {
            let (o, e, c) = run(p, "interp");
            std::fs::write(&jp, format!("{{\n \"stdout\": \"{}\",\n \"stderr\": \"{}\",\n \"exit\": {c}\n}}\n",
                                        escape(&o), escape(&e))).unwrap();
        }
        let json = std::fs::read_to_string(&jp).unwrap();
        let want = (field(&json, "stdout"), field(&json, "stderr"), field(&json, "exit").parse::<i32>().unwrap());
        for backend in ["interp", "auto"] {
            let got = run(p, backend);
            if got != want {
                bad.push(format!("{} ({backend}):\n  want {want:?}\n  got  {got:?}", p.display()));
            }
        }
        // the checks the programs print must all hold
        if want.0.split_whitespace().any(|w| w == "false") {
            bad.push(format!("{}: a check printed false:\n{}", p.display(), want.0));
        }
    }
    remove_svgs();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn a_sweep_draws_one_curve_per_value_in_one_figure() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c2/sweep_spring.fm");
    let _files = FILES.lock().unwrap_or_else(|e| e.into_inner());
    for backend in ["interp", "auto"] {
        let (out, err, code) = run(&p, backend);
        assert_eq!(code, 0, "{out}{err}");
        assert_eq!(out.matches("plot saved to sweep_spring.svg").count(), 1, "{out}");
        let dir = p.parent().unwrap();
        let svg = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let (spring, pend, decay) = (svg("sweep_spring.svg"), svg("sweep_pendulum.svg"), svg("sweep_decay.svg"));
        for f in ["sweep_spring.svg", "sweep_pendulum.svg", "sweep_decay.svg"] {
            let _ = std::fs::remove_file(dir.join(f));
        }
        for k in ["k = 1 N/m", "k = 2 N/m", "k = 4 N/m"] {
            assert!(spring.contains(k), "{k} not in the legend ({backend})");
        }
        for l in ["L = 0.500 m", "L = 1 m", "L = 1.50 m", "L = 2 m"] {
            assert!(pend.contains(l), "{l} not in the legend ({backend})");
        }
        for c in ["u, λ = 0.100 1/s", "w, λ = 0.400 1/s"] {
            assert!(decay.contains(c), "{c} not in the legend ({backend})");
        }
    }
}
