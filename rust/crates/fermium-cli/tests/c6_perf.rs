//! Spec C6 (performance): the programs in rust/c-cases/c6/*.fm print exactly their .json (Fermium 1.5's output)
//! with the tree-walker, with the LLVM back end, and with the LLVM back end with each C6 optimization switched
//! off in turn, so every optimization is shown to keep the printed numbers bit for bit (DECISIONS D310–D316).
//! Further tests: the loop vectorizer takes the loops D311/D312 are for.
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../c-cases/c6")
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
       .env("FERMIUM_BACKEND", backend).env_remove("FERMIUM_LLVM_ARGS").env_remove("FERMIUM_NO_IFCONV")
       .env_remove("FERMIUM_PREFAULT").env_remove("FERMIUM_NO_MATH_INTRINSICS").env_remove("FERMIUM_LLVM_NOSLP")
       .env_remove("FERMIUM_BACKEND_INFO").env("FERMIUM_NO_CACHE", "1");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    (String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into(), o.status.code().unwrap_or(-1))
}

#[test]
fn c6_cases_print_their_expected_output_with_and_without_each_optimization() {
    let cases = cases();
    assert!(cases.len() >= 4);
    let variants: [(&str, Vec<(&str, &str)>); 7] = [
        ("interp", vec![]),
        ("llvm", vec![]),
        ("llvm", vec![("FERMIUM_NO_IFCONV", "1")]),
        ("llvm", vec![("FERMIUM_LLVM_ARGS", "")]),
        ("llvm", vec![("FERMIUM_LLVM_NOSLP", "1")]),
        ("llvm", vec![("FERMIUM_PREFAULT", "0")]),
        ("llvm", vec![("FERMIUM_NO_MATH_INTRINSICS", "1")]),
    ];
    let mut bad = vec![];
    for p in &cases {
        let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
        let want = (field(&json, "stdout").unwrap(), field(&json, "stderr").unwrap(),
                    field(&json, "exit").unwrap().parse::<i32>().unwrap());
        for (backend, env) in &variants {
            let got = run(p, backend, env);
            if got != want {
                bad.push(format!("{} ({backend} {env:?}):\n  want {want:?}\n  got  {got:?}", p.display()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ------------------------------------------------------------------ the compile cache (D317)

/// A fresh folder for one test (under the system's temporary folder).
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-c6-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `fermium run prog.fm` in dir with the cache in dir/cache: (stdout, stderr, exit, used the cache).
fn run_c(dir: &Path, prog: &str, extra: &[(&str, &str)]) -> (String, String, i32, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fermium"));
    cmd.args(["run", prog]).current_dir(dir).env("FERMIUM_CACHE_DIR", dir.join("cache"))
       .env("FERMIUM_BACKEND_INFO", dir.join("backend")).env_remove("FERMIUM_BACKEND").env_remove("FERMIUM_NO_CACHE")
       .env_remove("FERMIUM_LLVM_TIME").env_remove("FERMIUM_LLVM_ARGS");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    // FERMIUM_LLVM_TIME's line says whether the code came from the cache (and is left out of stderr)
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    let cached = err.contains("llvm: cached code loaded");
    let err: String = err.lines().filter(|l| !l.starts_with("llvm: ")).map(|l| format!("{l}\n")).collect();
    (String::from_utf8_lossy(&o.stdout).into(), err, o.status.code().unwrap_or(-1), cached)
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir.join("cache/jit")).map(|rd| rd.flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "fmc")).collect()).unwrap_or_default()
}

#[test]
fn compile_cache_reuses_a_program_and_recompiles_when_a_module_changes() {
    let d = scratch("modules");
    std::fs::write(d.join("springs.fm"), "k_default = 40 [N/m]\nperiod(m [kg], k [N/m]) = 2π √(m/k)\n").unwrap();
    std::fs::write(d.join("prog.fm"), "import springs\nimport stats\nprint springs.period(1 kg, springs.k_default) to 6 digits\n\
                                        print stats.correlation([1, 2, 3], [1, 2, 4]) to 6 digits\n").unwrap();
    let t = [("FERMIUM_LLVM_TIME", "1")];
    let first = run_c(&d, "prog.fm", &t);
    assert_eq!((first.2, first.3), (0, false), "{first:?}");
    assert_eq!(entries(&d).len(), 1);
    let second = run_c(&d, "prog.fm", &t);
    assert!(second.3, "the second run didn't use the cache: {second:?}");
    assert_eq!((&second.0, &second.1, second.2), (&first.0, &first.1, first.2));
    assert_eq!(std::fs::read_to_string(d.join("backend")).unwrap(), "llvm");
    // a module's contents changed: compiled again, with the new value
    std::fs::write(d.join("springs.fm"), "k_default = 10 [N/m]\nperiod(m [kg], k [N/m]) = 2π √(m/k)\n").unwrap();
    let third = run_c(&d, "prog.fm", &t);
    assert!(!third.3 && third.0 != first.0, "{third:?}");
    assert!(run_c(&d, "prog.fm", &t).3);
    // a module file added to a folder the import searched (here, one shadowing the standard library's stats)
    std::fs::write(d.join("stats.fm"), "correlation(a, b) = 42\n").unwrap();
    let fourth = run_c(&d, "prog.fm", &t);
    assert!(!fourth.3 && fourth.0.contains("42"), "{fourth:?}");
    // the program's own text changed
    let prog = std::fs::read_to_string(d.join("prog.fm")).unwrap();
    std::fs::write(d.join("prog.fm"), format!("{prog}print 1\n")).unwrap();
    let fifth = run_c(&d, "prog.fm", &t);
    assert!(!fifth.3 && fifth.0.ends_with("1\n"), "{fifth:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn compile_cache_ignores_damaged_entries_and_replays_warnings_and_errors() {
    let d = scratch("damage");
    // a check-time warning, output, then a run-time error
    let src = "h = 2\nprint h\nxs = [1, 2]\nprint xs[3]\n";
    std::fs::write(d.join("p.fm"), src).unwrap();
    let t = [("FERMIUM_LLVM_TIME", "1")];
    let first = run_c(&d, "p.fm", &t);
    assert!(first.1.contains("warning") && first.2 == 1 && first.0 == "2\n", "{first:?}");
    let second = run_c(&d, "p.fm", &t);
    assert!(second.3, "{second:?}");
    assert_eq!((&second.0, &second.1, second.2), (&first.0, &first.1, first.2));
    // damage the entry in several ways: each run is still right, compiles again and repairs the entry
    let e = entries(&d).pop().unwrap();
    let good = std::fs::read(&e).unwrap();
    let mut flipped = good.clone();
    let mid = flipped.len() / 2;
    flipped[mid] ^= 0x40;
    for bad in [good[..good.len() / 3].to_vec(), flipped, b"garbage".to_vec(), vec![]] {
        std::fs::write(&e, &bad).unwrap();
        let r = run_c(&d, "p.fm", &t);
        assert!(!r.3, "a damaged entry was used");
        assert_eq!((&r.0, &r.1, r.2), (&first.0, &first.1, first.2));
        assert!(run_c(&d, "p.fm", &t).3, "the entry wasn't repaired");
    }
    // FERMIUM_NO_CACHE=1: nothing read or written
    let d2 = scratch("off");
    std::fs::write(d2.join("p.fm"), "print 3\n").unwrap();
    for _ in 0..2 {
        let r = run_c(&d2, "p.fm", &[("FERMIUM_LLVM_TIME", "1"), ("FERMIUM_NO_CACHE", "1")]);
        assert!(!r.3 && r.0 == "3\n");
    }
    assert!(entries(&d2).is_empty());
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&d2);
}

#[test]
fn compile_cache_gives_every_c6_case_the_same_output_from_the_cache() {
    let d = scratch("cases");
    for p in cases() {
        let json = std::fs::read_to_string(p.with_extension("json")).unwrap();
        let want = (field(&json, "stdout").unwrap(), field(&json, "stderr").unwrap(),
                    field(&json, "exit").unwrap().parse::<i32>().unwrap());
        let name = p.file_name().unwrap().to_str().unwrap();
        std::fs::copy(&p, d.join(name)).unwrap();
        let t = [("FERMIUM_LLVM_TIME", "1")];
        let a = run_c(&d, name, &t);
        let b = run_c(&d, name, &t);
        assert!(b.3, "{name} didn't run from the cache");
        assert_eq!((a.0.clone(), a.1.clone(), a.2), want, "{name}");
        assert_eq!((b.0, b.1, b.2), want, "{name} from the cache");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// How many loops LLVM's loop vectorizer vectorized in a program (its remarks, on stderr).
fn vectorized(name: &str, args: &str) -> usize {
    let (_, err, code) = run(&root().join(name), "llvm", &[("FERMIUM_LLVM_ARGS", args)]);
    assert_eq!(code, 0, "{err}");
    err.lines().filter(|l| l.contains("vectorized loop")).count()
}

#[test]
fn the_loop_vectorizer_takes_guarded_sums_only_with_if_conversion_and_ordered_reductions() {
    let remarks = "-force-ordered-reductions -pass-remarks=loop-vectorize";
    // ifconv.fm's `for j … if j != i … u += …; v -= …` loop, among others
    let with = vectorized("ifconv.fm", remarks);
    assert!(with >= 1, "no loop vectorized");
    // without in-order reductions none of its sums can be vectorized: on x86-64. AArch64's LLVM target enables
    // in-order (strict) floating-point reductions by default, so there the flag changes nothing (the sums are
    // still added in program order either way; the output checks above hold on both)
    if cfg!(target_arch = "x86_64") {
        assert!(vectorized("ifconv.fm", "-pass-remarks=loop-vectorize") < with);
    }
    // element-by-element loops and plain in-order sums
    assert!(vectorized("elementwise.fm", remarks) >= 3);
}
