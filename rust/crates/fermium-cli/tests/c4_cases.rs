//! Phase C (v2.5) item C4, C++ interop through generated C wrappers (DECISIONS D290). The programs in
//! rust/c-cases/c4/*.fm call the small C++ library there (phys.hpp, phys.cpp), which the test builds with the
//! system's C++ compiler as libphys4.so in a copy of the folder; each program runs with the tree-walker and
//! the LLVM back end and must print exactly its .json (stdout, stderr, exit). Compiled wrappers go to a cache
//! folder of the test's own (FERMIUM_CACHE_DIR). Further tests: the cache (reused, and made again when an
//! included header changes), a header that doesn't compile, no C++ compiler, `fermium build`, fmt round trips
//! and the worked example. Every test is skipped with a note when no C++ compiler is installed.
//! `FERMIUM_BLESS=1` rewrites the .json files from the tree-walker's output (read them before committing).
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c4")
}

fn have_cxx() -> bool {
    let ok = ["c++", "g++", "clang++"]
        .iter()
        .any(|c| Command::new(c).arg("--version").output().map(|o| o.status.success()).unwrap_or(false));
    if !ok {
        eprintln!("skipped: no C++ compiler (c++, g++ or clang++) is installed");
    }
    ok
}

fn cxx() -> &'static str {
    ["c++", "g++", "clang++"]
        .into_iter()
        .find(|c| Command::new(c).arg("--version").output().map(|o| o.status.success()).unwrap_or(false))
        .unwrap()
}

fn fresh_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-c4-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn build_lib(dir: &Path, out: &str, src: &str) {
    let o = Command::new(cxx()).args(["-std=c++17", "-O2", "-shared", "-fPIC", "-o", out, src]).current_dir(dir)
        .output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// A copy of c-cases/c4 with libphys4.so built in it, once per test process.
fn cases_dir() -> Option<&'static PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if !have_cxx() {
            return None;
        }
        let d = fresh_dir("cases");
        for f in std::fs::read_dir(src_dir()).unwrap() {
            let p = f.unwrap().path();
            std::fs::copy(&p, d.join(p.file_name().unwrap())).unwrap();
        }
        build_lib(&d, "libphys4.so", "phys.cpp");
        Some(d)
    }).as_ref()
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

fn fermium(args: &[&str], dir: &Path, cache: &Path) -> (String, String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(args).current_dir(dir)
        .env("FERMIUM_CACHE_DIR", cache).env_remove("FERMIUM_BACKEND_INFO").env_remove("FERMIUM_BACKEND")
        .env_remove("CXX").env_remove("CXXFLAGS").output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

fn run(dir: &Path, file: &str, backend: &str) -> (String, String, i32) {
    fermium(&["run", "--backend", backend, file], dir, &dir.join("cache"))
}

#[test]
fn c4_cases_print_their_expected_output() {
    let Some(d) = cases_dir() else { return };
    let mut cases: Vec<String> = std::fs::read_dir(src_dir()).unwrap().map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "fm"))
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    cases.sort();
    assert!(cases.len() >= 20);
    let bless = std::env::var("FERMIUM_BLESS").is_ok_and(|v| v == "1");
    let mut bad = vec![];
    for f in &cases {
        let jp = src_dir().join(f).with_extension("json");
        if bless {
            let (o, e, c) = run(d, f, "interp");
            std::fs::write(&jp, format!("{{\n \"stdout\": \"{}\",\n \"stderr\": \"{}\",\n \"exit\": {c}\n}}\n",
                                        escape(&o), escape(&e))).unwrap();
        }
        let json = std::fs::read_to_string(&jp).unwrap_or_else(|_| panic!("{} has no .json", f));
        let want = (field(&json, "stdout"), field(&json, "stderr"), field(&json, "exit").parse::<i32>().unwrap());
        for backend in ["interp", "llvm"] {
            let got = run(d, f, backend);
            if got != want {
                bad.push(format!("{f} ({backend}):\n  want {want:?}\n  got  {got:?}"));
            }
        }
        // the checks the programs print must all hold
        if want.0.split_whitespace().any(|w| w == "false") {
            bad.push(format!("{f}: a check printed false:\n{}", want.0));
        }
        // a program that runs has no error; one that stops says why in one line with the file and line
        if want.2 != 0 && !want.1.starts_with(&format!("{f}, line ")) {
            bad.push(format!("{f}: the error doesn't start with the file and line:\n{}", want.1));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

const HEADER_A: &str = "#pragma once\n#include \"inner.hpp\"\nnamespace w { inline double scaled(double x) { return x * FACTOR; } }\n";

#[test]
fn the_wrapper_is_cached_and_made_again_when_an_included_header_changes() {
    if !have_cxx() {
        return;
    }
    let d = fresh_dir("cache");
    let cache = d.join("cache");
    std::fs::write(d.join("outer.hpp"), HEADER_A).unwrap();
    std::fs::write(d.join("inner.hpp"), "#define FACTOR 2.0\n").unwrap();
    std::fs::write(d.join("p.fm"), "import cpp header \"outer.hpp\":\n    w::scaled(x [m]) -> [m]\nprint scaled(3 m)\n")
        .unwrap();
    let (o, e, c) = fermium(&["run", "p.fm"], &d, &cache);
    assert_eq!((o.as_str(), e.as_str(), c), ("6 m\n", "", 0));
    let so: Vec<PathBuf> = std::fs::read_dir(cache.join("cpp")).unwrap().map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "so")).collect();
    assert_eq!(so.len(), 1, "{so:?}");
    let t0 = std::fs::metadata(&so[0]).unwrap().modified().unwrap();
    // a second run uses the cached wrapper, even with no compiler on the PATH
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run", "p.fm"]).current_dir(&d)
        .env("FERMIUM_CACHE_DIR", &cache).env("PATH", "").env_remove("CXX").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout), "6 m\n", "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::metadata(&so[0]).unwrap().modified().unwrap(), t0);
    // the header the program names includes inner.hpp: changing it makes the wrapper again
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(d.join("inner.hpp"), "#define FACTOR 5.0\n").unwrap();
    let (o, e, c) = fermium(&["run", "p.fm"], &d, &cache);
    assert_eq!((o.as_str(), e.as_str(), c), ("15 m\n", "", 0));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_header_that_doesnt_compile_and_a_missing_compiler_are_one_line_errors() {
    if !have_cxx() {
        return;
    }
    let d = fresh_dir("errors");
    let cache = d.join("cache");
    std::fs::write(d.join("bad.hpp"), "#pragma once\nnamespace w { double f(double x) { return x +; } }\n").unwrap();
    std::fs::write(d.join("p.fm"), "import cpp header \"bad.hpp\":\n    w::f(x) -> number\nprint f(1)\n").unwrap();
    let (o, e, c) = fermium(&["run", "p.fm"], &d, &cache);
    assert!(c != 0 && o.is_empty(), "{o}");
    let first = e.lines().next().unwrap();
    assert!(first.starts_with("p.fm, line 1: the header bad.hpp doesn't compile: bad.hpp, line 2: "), "{e}");
    assert!(e.contains("hint: the compiler's full output is in ") && e.contains(".log"), "{e}");
    // no compiler anywhere
    std::fs::write(d.join("q.fm"), "import cpp header \"cmath\":\n    std::erf(x) -> number\nprint erf(0.5)\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run", "q.fm"]).current_dir(&d)
        .env("FERMIUM_CACHE_DIR", &cache).env("PATH", "").env_remove("CXX").output().unwrap();
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.starts_with("q.fm, line 1: import cpp needs a C++ compiler, and none was found (tried c++, g++ and \
                           clang++)") && e.contains("hint: install one (like  sudo apt install g++), or set CXX"), "{e}");
    // with one it works (a <cmath> function: std::erf has float, double and long double overloads)
    let (o, e, _) = fermium(&["run", "q.fm"], &d, &cache);
    assert_eq!(o, "0.520\n", "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn fermium_build_executables_call_cpp_and_fmt_round_trips() {
    let Some(d) = cases_dir() else { return };
    let exe = d.join("greek_exe");
    let (_, e, c) = fermium(&["build", "-o", exe.to_str().unwrap(), "overloads.fm"], d, &d.join("cache"));
    assert_eq!(c, 0, "{e}");
    let o = Command::new(&exe).current_dir(std::env::temp_dir()).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let (want, _, _) = run(d, "overloads.fm", "interp");
    assert_eq!(String::from_utf8_lossy(&o.stdout), want);
    // fmt --ascii spells λ_max and E₀ in ASCII, --pretty brings them back, and both run the same
    let (ascii, e, c) = fermium(&["fmt", "--ascii", "greek.fm"], d, &d.join("cache"));
    assert_eq!(c, 0, "{e}");
    assert!(ascii.contains("    phys::lambda_max(T [K]) -> [m]\n    phys::energy(m [kg]) -> [J] as E_0\n"), "{ascii}");
    std::fs::write(d.join("greek_ascii.fm"), &ascii).unwrap();
    let (pretty, _, _) = fermium(&["fmt", "--pretty", "greek_ascii.fm"], d, &d.join("cache"));
    let orig = std::fs::read_to_string(d.join("greek.fm")).unwrap();
    assert_eq!(pretty, orig);
    assert_eq!(run(d, "greek_ascii.fm", "interp").0, run(d, "greek.fm", "interp").0);
}

#[test]
fn the_worked_example_prints_its_expected_output() {
    if !have_cxx() {
        return;
    }
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/cpp_interop");
    let d = fresh_dir("example");
    for f in ["cpp_interop.fm", "kinematics.hpp", "kinematics.cpp", "expected_output.txt"] {
        std::fs::copy(ex.join(f), d.join(f)).unwrap();
    }
    build_lib(&d, "libkinematics.so", "kinematics.cpp");
    let want = std::fs::read_to_string(d.join("expected_output.txt")).unwrap();
    for backend in ["interp", "llvm"] {
        let (o, e, c) = run(&d, "cpp_interop.fm", backend);
        assert_eq!((o.as_str(), e.as_str(), c), (want.as_str(), "", 0), "{backend}");
    }
    // the PDG's daughter momenta: 29.79 MeV/c for π⁺ → μ⁺ ν, 235.5 MeV/c for K⁺ → μ⁺ ν
    assert!(want.contains("π⁺ → μ⁺ ν  29.79 MeV/c 29.79 MeV/c") && want.contains("K⁺ → μ⁺ ν  235.5 MeV/c 235.5 MeV/c"),
            "{want}");
    assert!(want.contains("p π⁻ pair at 180°: 1115.683 MeV/c²"), "{want}");
    // a forbidden decay: the C++ exception's message
    let src = std::fs::read_to_string(d.join("cpp_interop.fm")).unwrap();
    let head: String = src.lines().take_while(|l| !l.starts_with("# masses")).map(|l| format!("{l}\n")).collect();
    std::fs::write(d.join("forbidden.fm"), format!("{head}print momentum(100 MeV/c², m_μ, 0 MeV/c²)\n")).unwrap();
    let (_, e, c) = run(&d, "forbidden.fm", "llvm");
    assert_ne!(c, 0);
    assert!(e.starts_with("forbidden.fm, line 22: momentum: the C++ function kin::TwoBody::momentum threw an exception: \
                           the decay is kinematically forbidden (M < m1 + m2)"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}
