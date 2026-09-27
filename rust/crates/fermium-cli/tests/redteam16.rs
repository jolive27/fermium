//! Red team 16: the round-16 fixes (dev-notes/REDTEAM.md, "Round 16"; DECISIONS D330-D339).
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-rt16-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `fermium <args>` in `cwd` with the cache in `cache` and the extra environment `env`: (exit code, stdout, stderr).
fn fm(args: &[&str], cwd: &Path, cache: &Path, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fermium"));
    c.args(args).current_dir(cwd).env("FERMIUM_CACHE_DIR", cache).env_remove("FERMIUM_BACKEND")
        .env_remove("FERMIUM_NO_CACHE");
    for (k, v) in env {
        c.env(k, v);
    }
    let o: Output = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(),
     String::from_utf8_lossy(&o.stderr).into_owned())
}

/// Run `src` as a program (no cache): (exit code, stdout, stderr).
fn run_src(tag: &str, src: &str) -> (i32, String, String) {
    let d = dir(tag);
    std::fs::write(d.join("p.fm"), src).unwrap();
    fm(&["run", "p.fm"], &d, &d.join("cache"), &[("FERMIUM_NO_CACHE", "1")])
}

// ---------------------------------------------------------------- #1: the compile cache's key (D330)

#[test]
fn a_home_relative_import_follows_home_not_the_cache() {
    let d = dir("home");
    let cache = d.join("cache");
    let (a, b, prog) = (d.join("A"), d.join("B"), d.join("prog"));
    for x in [&a, &b, &prog] {
        std::fs::create_dir_all(x).unwrap();
    }
    std::fs::write(a.join("mod.fm"), "k = 1.0\n").unwrap();
    std::fs::write(b.join("mod.fm"), "k = 2.0\n").unwrap();
    std::fs::write(prog.join("p.fm"), "import \"~/mod.fm\"\nprint mod.k\n").unwrap();
    let (c1, o1, e1) = fm(&["run", "p.fm"], &prog, &cache, &[("HOME", a.to_str().unwrap())]);
    assert_eq!(c1, 0, "{e1}");
    assert!(o1.trim() == "1" || o1.trim() == "1.0", "{o1}");
    let (c2, o2, e2) = fm(&["run", "p.fm"], &prog, &cache, &[("HOME", b.to_str().unwrap())]);
    assert_eq!(c2, 0, "{e2}");
    assert!(o2.trim() == "2" || o2.trim() == "2.0", "stale cache entry for another $HOME: {o2}");
}

#[test]
fn a_relative_base_dir_is_keyed_by_the_folder_it_names() {
    let d = dir("base");
    let cache = d.join("cache");
    let (x, y) = (d.join("X"), d.join("Y"));
    for (f, k) in [(&x, "1.0"), (&y, "2.0")] {
        std::fs::create_dir_all(f).unwrap();
        std::fs::write(f.join("m.fm"), format!("k = {k}\n")).unwrap();
    }
    // the same program text, read from the same relative name, run from two folders with `--base-dir .`
    let prog = "import m\nprint m.k\n";
    std::fs::write(x.join("p.fm"), prog).unwrap();
    std::fs::write(y.join("p.fm"), prog).unwrap();
    let (c1, o1, e1) = fm(&["run", "--base-dir", ".", "p.fm"], &x, &cache, &[]);
    assert_eq!(c1, 0, "{e1}");
    assert!(o1.trim().starts_with('1'), "{o1}");
    let (c2, o2, e2) = fm(&["run", "--base-dir", ".", "p.fm"], &y, &cache, &[]);
    assert_eq!(c2, 0, "{e2}");
    assert!(o2.trim().starts_with('2'), "stale cache entry for another folder: {o2}");
}

// ---------------------------------------------------------------- #2: (u^a)^b keeps the sign of u (D331)

#[test]
fn a_power_of_an_even_power_keeps_the_absolute_value() {
    let (c, o, e) = run_src("pow", "f(x) = (x^2)^(3/2)\ng(x) = (x^2)^(1/2)\nprint f'\nprint f'(-2)\nprint g'(-2)\nprint g'(3)\n");
    assert_eq!(c, 0, "{e}");
    let l: Vec<&str> = o.lines().collect();
    assert_eq!(l[0], "f'(x) = 3x (x²)^(1/2)", "{o}");
    assert_eq!(&l[1..], ["-12", "-1", "1"], "{o}");
}

// ---------------------------------------------------------------- #9 (REDTEAM numbering): ± only in a when (D335)

#[test]
fn an_uncertain_value_read_only_by_a_when_reset_takes_monte_carlo() {
    // first bounce at 1.428 s, the second at 1.428 + 2·0.9·14.01/9.81 = 3.999 s: y(4 s) ≈ 0.01 m nominal, and
    // |Δk|·34 m either side (a folded normal: σ ≈ 0.6·0.34 m)
    let src = "g = 9.81 m/s²\nk_e = 0.9 ± 0.01\nsolve y'' = -g\n  with y(0 s) = 10 m, y'(0 s) = 0 m/s\n  \
               for t from 0 s to 4 s\n  when y = 0 m: y' = -k_e y'\nprint y(4 s)\n";
    let (c, o, e) = run_src("whenpm", src);
    assert_eq!(c, 0, "{e}");
    assert!(e.contains("has a  when  event whose time moves with its uncertain inputs"), "{e}");
    let (v, s) = o.trim().trim_end_matches(" m").split_once(" ± ").expect("a ± result");
    let (v, s): (f64, f64) = (v.parse().unwrap(), s.parse().unwrap());
    assert!(v.abs() < 0.05 && (0.12..0.3).contains(&s), "{o}");
}

// ---------------------------------------------------------------- #3: a recursive function (D332)

#[test]
fn differentiating_a_recursive_function_is_a_one_line_error() {
    let src = "f(x) =\n    if x < 1\n        return x\n    x * f(x - 1)\nprint f'(3.5)\n";
    let (c, _, e) = run_src("rec", src);
    assert_eq!(c, 1, "{e}");
    assert!(e.contains("can't differentiate f: it calls itself"), "{e}");
    let d = dir("recchk");
    std::fs::write(d.join("p.fm"), src).unwrap();
    let (c, _, e) = fm(&["check", "p.fm"], &d, &d.join("cache"), &[]);
    assert_eq!(c, 1, "{e}");
    assert!(e.contains("calls itself"), "{e}");
}

// ---------------------------------------------------------------- #4: AD with a unit-carrying accumulator (D333)

#[test]
fn an_accumulator_that_starts_at_a_constant_takes_the_units_of_its_tangent() {
    let src = "f(r) =\n    E = 0 J\n    E = E + 5 J m / r\n    E\nprint f'(2 m)\n\
               g(v) =\n    p = 0 kg m/s\n    p = p + 2 kg v\n    p\nprint g'(3 m/s)\n\
               h(x) =\n    u = 0 J\n    if x > 1 m\n        u = u + 3 J/m * x\n    u\nprint h'(2 m)\n";
    let (c, o, e) = run_src("acc", src);
    assert_eq!(c, 0, "{e}");
    assert_eq!(o.lines().collect::<Vec<_>>(), ["-1.25 N", "2 kg", "3 J/m"], "{o}");
}
