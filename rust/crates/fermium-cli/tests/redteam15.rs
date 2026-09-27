//! Red team 15: C++ interop and the round-14 fixes (dev-notes/REDTEAM.md, "Round 15"; DECISIONS D320-D325).
//! The C++ tests are skipped with a note when no C++ compiler is installed.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-rt15-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
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

/// `fermium <args>` in `cwd` with the cache in `cache` and the extra environment `env`: (exit code, stdout, stderr).
fn fm(args: &[&str], cwd: &Path, cache: &Path, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fermium"));
    c.args(args).current_dir(cwd).env("FERMIUM_CACHE_DIR", cache).env_remove("CXX").env_remove("CPATH")
        .env_remove("CPLUS_INCLUDE_PATH").env_remove("CXXFLAGS").env_remove("FERMIUM_BACKEND");
    for (k, v) in env {
        c.env(k, v);
    }
    let o: Output = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(),
     String::from_utf8_lossy(&o.stderr).into_owned())
}

// ---------------------------------------------------------------- #1: the wrapper cache and the program's folder

const TGAMMA: &str = "import cpp header \"cmath\":\n    std::tgamma(x) -> number\nprint tgamma(5)\n";

#[test]
fn a_planted_system_header_in_one_programs_folder_never_runs() {
    if !have_cxx() {
        return;
    }
    let d = dir("evil");
    let cache = d.join("cache");
    let (evil, good) = (d.join("evil"), d.join("good"));
    std::fs::create_dir_all(&evil).unwrap();
    std::fs::create_dir_all(&good).unwrap();
    let marker = d.join("PWNED");
    // a cstdio next to the program that includes the real one and runs code when the wrapper is loaded
    std::fs::write(evil.join("cstdio"), format!(
        "#include_next <cstdio>\n#include <cstdlib>\nnamespace {{ struct Pwn {{ Pwn() {{ std::system(\"touch {}\"); }} }} \
         pwn_instance; }}\n", marker.display())).unwrap();
    std::fs::write(evil.join("a.fm"), TGAMMA).unwrap();
    std::fs::write(good.join("b.fm"), TGAMMA).unwrap();
    // check compiles the wrapper (and loads nothing), then another program with the same import runs
    let (c, _, e) = fm(&["check", "a.fm"], &evil, &cache, &[]);
    assert_eq!(c, 0, "{e}");
    let (c, o, e) = fm(&["run", "b.fm"], &good, &cache, &[]);
    assert_eq!((c, o.as_str()), (0, "24\n"), "{e}");
    assert!(!marker.exists(), "the planted header's code ran through the cache");
    // the program's folder isn't on the include path for a system header at all
    let (c, o, e) = fm(&["run", "a.fm"], &evil, &cache, &[]);
    assert_eq!((c, o.as_str()), (0, "24\n"), "{e}");
    assert!(!marker.exists(), "a file in the program's folder stood in for <cstdio>");
    // one wrapper, with its manifest (the compiler's dependency list, system headers included)
    let files: Vec<String> = std::fs::read_dir(cache.join("cpp")).unwrap()
        .map(|f| f.unwrap().file_name().to_string_lossy().into_owned()).collect();
    let man: Vec<&String> = files.iter().filter(|f| f.ends_with(".manifest")).collect();
    assert_eq!(man.len(), 1, "{files:?}");
    let text = std::fs::read_to_string(cache.join("cpp").join(man[0])).unwrap();
    assert!(text.starts_with("fermium-cpp-manifest 2\ncompiler ") && text.contains("cmath\n"), "{text}");
    // the key is a SHA-256 (40 hex digits of it name the file)
    assert!(man[0].len() == 1 + 40 + ".manifest".len(), "{man:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_same_header_in_two_folders_makes_two_wrappers() {
    if !have_cxx() {
        return;
    }
    let d = dir("twofolders");
    let cache = d.join("cache");
    let hdr = "#pragma once\n#include <climits>\n#ifndef FLAVOR\n#define FLAVOR 1\n#endif\n\
               inline double flavor(double) { return FLAVOR; }\n";
    let prog = "import cpp header \"h.hpp\":\n    flavor(x) -> number\nprint flavor(0)\n";
    for (sub, extra) in [("a", Some("#include_next <climits>\n#define FLAVOR 7\n")), ("b", None)] {
        let p = d.join(sub);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("h.hpp"), hdr).unwrap();
        std::fs::write(p.join("p.fm"), prog).unwrap();
        if let Some(x) = extra {
            // folder a's own climits (the program's own header's folder is on the include path)
            std::fs::write(p.join("climits"), x).unwrap();
        }
    }
    let (_, o, e) = fm(&["run", "p.fm"], &d.join("a"), &cache, &[]);
    assert_eq!(o, "7\n", "{e}");
    let (_, o, e) = fm(&["run", "p.fm"], &d.join("b"), &cache, &[]);
    assert_eq!(o, "1\n", "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_changed_or_swapped_wrapper_is_not_reused() {
    if !have_cxx() {
        return;
    }
    let d = dir("tamper");
    let cache = d.join("cache");
    std::fs::write(d.join("p.fm"), TGAMMA).unwrap();
    let (_, o, e) = fm(&["run", "p.fm"], &d, &cache, &[]);
    assert_eq!(o, "24\n", "{e}");
    let so = std::fs::read_dir(cache.join("cpp")).unwrap().map(|f| f.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "so")).unwrap();
    // a wrapper whose bytes differ from the manifest's SHA-256 is made again
    let mut bytes = std::fs::read(&so).unwrap();
    bytes.push(0);
    std::fs::write(&so, &bytes).unwrap();
    let (_, o, e) = fm(&["run", "p.fm"], &d, &cache, &[]);
    assert_eq!(o, "24\n", "{e}");
    assert_ne!(std::fs::read(&so).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- #2: include paths and the compiler's identity

#[test]
fn switching_cpath_or_the_compiler_makes_the_wrapper_again() {
    if !have_cxx() {
        return;
    }
    let d = dir("cpath");
    let cache = d.join("cache");
    for (sub, k) in [("inc1", "1"), ("inc2", "2")] {
        std::fs::create_dir_all(d.join(sub)).unwrap();
        std::fs::write(d.join(sub).join("konst.hpp"), format!("#pragma once\ninline double konst(double) {{ return {k}; }}\n"))
            .unwrap();
    }
    let prog = d.join("prog");
    std::fs::create_dir_all(&prog).unwrap();
    std::fs::write(prog.join("p.fm"), "import cpp header \"konst.hpp\":\n    konst(x) -> number\nprint konst(0)\n").unwrap();
    let inc = |s: &str| d.join(s).to_string_lossy().into_owned();
    let (i1, i2) = (inc("inc1"), inc("inc2"));
    let (_, o, e) = fm(&["run", "p.fm"], &prog, &cache, &[("CPATH", &i1)]);
    assert_eq!(o, "1\n", "{e}");
    let (_, o, e) = fm(&["run", "p.fm"], &prog, &cache, &[("CPATH", &i2)]);
    assert_eq!(o, "2\n", "{e}");
    let (_, o, e) = fm(&["run", "p.fm"], &prog, &cache, &[("CPLUS_INCLUDE_PATH", &i1)]);
    assert_eq!(o, "1\n", "{e}");
    // with no include path the header isn't found: one line, with a hint
    let (c, _, e) = fm(&["run", "p.fm"], &prog, &cache, &[]);
    assert!(c != 0 && e.starts_with("p.fm, line 1: can't find the header konst.hpp\n"), "{e}");
    assert!(e.contains("hint: it isn't in the program's folder"), "{e}");
    // another compiler (a script around the real one that defines a macro): the key has $CXX, and the
    // manifest the compiler's file, so changing the script makes the wrapper again
    std::fs::write(d.join("inc1").join("konst.hpp"),
                   "#pragma once\n#ifndef K\n#define K 1\n#endif\ninline double konst(double) { return K; }\n").unwrap();
    let script = d.join("mycxx");
    let write_script = |k: &str| {
        std::fs::write(&script, format!("#!/bin/sh\nexec {} -DK={k} \"$@\"\n", cxx())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    };
    write_script("5");
    let s = script.to_string_lossy().into_owned();
    let (_, o, e) = fm(&["run", "p.fm"], &prog, &cache, &[("CPATH", &i1), ("CXX", &s)]);
    assert_eq!(o, "5\n", "{e}");
    write_script("66");
    let (_, o, e) = fm(&["run", "p.fm"], &prog, &cache, &[("CPATH", &i1), ("CXX", &s)]);
    assert_eq!(o, "66\n", "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- the cache folder is private

#[cfg(unix)]
#[test]
fn a_cache_folder_other_users_can_write_is_not_used() {
    if !have_cxx() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let d = dir("perms");
    let cache = d.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::write(d.join("p.fm"), TGAMMA).unwrap();
    let (c, o, e) = fm(&["run", "p.fm"], &d, &cache, &[]);
    assert_eq!((c, o.as_str()), (0, "24\n"), "{e}");
    assert!(e.starts_with("warning: Fermium's cache folder ") && e.contains("other users can write to it")
            && e.contains("running without a cache"), "{e}");
    assert!(std::fs::read_dir(cache.join("cpp")).map(|r| r.count()).unwrap_or(0) == 0, "the cache was written");
    // a fresh cache folder is made owner-only
    let fresh = d.join("fresh");
    let (_, o, e) = fm(&["run", "p.fm"], &d, &fresh, &[]);
    assert_eq!(o, "24\n", "{e}");
    for p in [&fresh, &fresh.join("cpp")] {
        assert_eq!(std::fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o700, "{}", p.display());
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- #6: C++ usability

#[test]
fn installed_headers_are_found_on_the_compilers_include_path() {
    if !have_cxx() {
        return;
    }
    let d = dir("mathh");
    let cache = d.join("cache");
    std::fs::write(d.join("p.fm"), "import cpp header \"math.h\":\n    cos(x) -> number\nprint cos(0)\n").unwrap();
    let (c, o, e) = fm(&["run", "p.fm"], &d, &cache, &[]);
    assert_eq!((c, o.as_str()), (0, "1\n"), "{e}");
    std::fs::write(d.join("q.fm"), "import cpp header \"nowhere/thing.hpp\":\n    f(x) -> number\nprint f(0)\n").unwrap();
    let (c, _, e) = fm(&["run", "q.fm"], &d, &cache, &[]);
    assert_eq!(c, 1);
    assert_eq!(e.lines().next().unwrap(), "q.fm, line 1: can't find the header nowhere/thing.hpp", "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_compiler_that_hangs_is_stopped() {
    if !have_cxx() || cfg!(not(unix)) {
        return;
    }
    let d = dir("hang");
    let cache = d.join("cache");
    let script = d.join("slowcxx");
    std::fs::write(&script, "#!/bin/sh\nsleep 60\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(d.join("p.fm"), TGAMMA).unwrap();
    let t = std::time::Instant::now();
    let s = script.to_string_lossy().into_owned();
    let (c, _, e) = fm(&["check", "p.fm"], &d, &cache, &[("CXX", &s), ("FERMIUM_CXX_TIMEOUT", "1")]);
    assert!(t.elapsed().as_secs() < 20, "took {:?}", t.elapsed());
    assert_eq!(c, 1, "{e}");
    assert!(e.contains("didn't finish within 1 s, so it was stopped") && e.contains("hint: set FERMIUM_CXX_TIMEOUT"),
            "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn exception_messages_are_one_line_of_plain_text() {
    if !have_cxx() {
        return;
    }
    let d = dir("exc");
    let cache = d.join("cache");
    std::fs::write(d.join("e.hpp"), "#pragma once\n#include <stdexcept>\n\
                   inline double esc(double) { throw std::runtime_error(\"line1\\nline2 \\x1b[31mRED\\x1b[0m %s %n\"); }\n")
        .unwrap();
    std::fs::write(d.join("p.fm"), "import cpp header \"e.hpp\":\n    esc(x) -> number\nprint esc(1)\n").unwrap();
    let (c, _, e) = fm(&["run", "p.fm"], &d, &cache, &[]);
    assert_eq!(c, 1);
    let first = e.lines().next().unwrap();
    assert!(first.contains("the C++ function esc threw an exception: line1 line2  [31mRED [0m %s %n"), "{e:?}");
    assert!(!e.contains('\x1b'), "{e:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_keyword_as_a_function_name_says_so() {
    let d = dir("kw");
    std::fs::write(d.join("p.fm"), "import cpp header \"cmath\":\n    phys::new(x) -> number\nprint 1\n").unwrap();
    let (c, _, e) = fm(&["check", "p.fm"], &d, &d.join("cache"), &[]);
    assert_eq!(c, 1);
    assert!(e.starts_with("p.fm, line 2: new is a C++ keyword, so phys::new can't be imported\n"), "{e}");
    assert!(e.contains("hint: operators and keywords can't be called from Fermium"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- #4, #5, #7: the round-14 fixes

/// Run `src` as prog.fm on a back end ("" = the default): (exit code, stdout, stderr).
fn run_on(tag: &str, backend: &str, src: &str) -> (i32, String, String) {
    let d = dir(tag);
    std::fs::write(d.join("prog.fm"), src).unwrap();
    let env: Vec<(&str, &str)> = if backend.is_empty() { vec![] } else { vec![("FERMIUM_BACKEND", backend)] };
    let r = fm(&["run", "prog.fm"], &d, &d.join("cache"), &env);
    let _ = std::fs::remove_dir_all(&d);
    r
}

fn run_both(tag: &str, src: &str) -> (i32, String, String) {
    let a = run_on(&format!("{tag}-interp"), "interp", src);
    let b = run_on(&format!("{tag}-llvm"), "llvm", src);
    assert_eq!(a, b, "the back ends differ on {src}");
    a
}

#[test]
fn a_declared_list_parameter_carries_through_a_wrapper() {
    let (c, o, e) = run_both("listwrap", "s(x: list) = 2\nf(y) = s(y)\nprint f([1, 2])\n\
                                          t(x: list, n) = n\ng(y, m) = t(y, m) + 1\nprint g([1, 2, 3], 1)\n\
                                          h(y) = 3 y\nprint h([1, 2])\n");
    assert_eq!(c, 0, "{e}");
    assert_eq!(o, "2\n2\n[3, 6]\n", "{e}");
}

#[test]
fn a_monte_carlo_integral_reports_the_nominal_value() {
    // ∫₀² (x if x < a) dx = a²/2 = 0.5 at a = 1 (the Monte Carlo mean is a²/2 + σ²/2 = 0.505)
    let (c, o, e) = run_on("mcint", "", "a = 1.0 ± 0.1\nJ = ∫ (if x < a then x else 0) dx from 0 to 2\nprint J\n\
                                       print value(J)\n");
    assert_eq!(c, 0, "{e}");
    let lines: Vec<&str> = o.lines().collect();
    assert_eq!(lines[0], "0.50 ± 0.10", "{o}");
    assert!((lines[1].parse::<f64>().unwrap() - 0.5).abs() < 1e-6, "{o}");
    assert!(e.contains("Monte Carlo"), "{e}");
}

#[test]
fn an_uncertain_value_that_rounds_to_zero_has_no_minus_sign() {
    let (c, o, e) = run_on("negzero", "", "x = -0.00001 ± 0.14\nprint x\ny = -0.00001 ± 0.14 m\nprint y\n\
                                         print -0.01 ± 0.14\n");
    assert_eq!(c, 0, "{e}");
    assert_eq!(o, "0.00 ± 0.14\n0.00 ± 0.14 m\n-0.01 ± 0.14\n");
}

#[test]
fn replacing_bq_by_one_per_second_or_rad_per_m_by_one_per_m_warns_with_a_fitting_hint() {
    let src = "A(r [Bq]) = r\nA(k [1/s]) = 2 k\nk(q [rad/m]) = q\nk(q [1/m]) = q\nW(e [J]) = e\nW(t [N m]) = t\n\
               H(f [Hz]) = f\nH(g [1/s]) = g\nprint 1\n";
    let (c, o, e) = run_on("respelt", "", src);
    assert_eq!((c, o.as_str()), (0, "1\n"), "{e}");
    assert_eq!(e.matches("warning").count(), 3, "Hz then 1/s is an ordinary redefinition: {e}");
    assert!(e.contains("A(k [1/s]) replaces A(r [Bq]) (line 1)"), "{e}");
    assert!(e.contains("k(q [1/m]) replaces k(q [rad/m]) (line 3)")
            && e.contains("an angle in rad counts as a plain number"), "{e}");
    assert!(e.contains("W(t [N m]) replaces W(e [J]) (line 5)"), "{e}");
    assert!(!e.contains("2π f"), "the ω = 2π f hint is for Hz and rad/s only: {e}");
}

#[test]
fn the_repl_warning_names_no_line() {
    let d = dir("repl");
    let mut child = Command::new(env!("CARGO_BIN_EXE_fermium")).current_dir(&d)
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped()).spawn().unwrap();
    {
        use std::io::Write;
        let mut i = child.stdin.take().unwrap();
        i.write_all("E(f [Hz]) = h f\nE(ω [rad/s]) = ħ ω\n".as_bytes()).unwrap();
    }
    let o = child.wait_with_output().unwrap();
    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(all.contains("E(ω [rad/s]) replaces E(f [Hz]): their units"), "{all}");
    assert!(all.contains("e.g. ω = 2π f"), "{all}");
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(unix)]
#[test]
fn the_compile_cache_is_private_too() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("jitperms");
    std::fs::write(d.join("p.fm"), "x = 0\nfor i from 1 to 10\n    x = x + i\nprint x\n").unwrap();
    // a fresh cache: jit/ is made owner-only and the program is saved there
    let fresh = d.join("fresh");
    let (c, o, e) = fm(&["run", "p.fm"], &d, &fresh, &[]);
    assert_eq!((c, o.as_str(), e.as_str()), (0, "55\n", ""));
    for p in [&fresh, &fresh.join("jit")] {
        assert_eq!(std::fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o700, "{}", p.display());
    }
    // a cache folder others can write: one warning, the program runs, nothing is read from it or saved in it
    let open = d.join("open");
    std::fs::create_dir_all(open.join("jit")).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
    let (c, o, e) = fm(&["run", "p.fm"], &d, &open, &[]);
    assert_eq!((c, o.as_str()), (0, "55\n"), "{e}");
    assert!(e.starts_with("warning: Fermium's cache folder ") && e.contains("running without a cache"), "{e}");
    assert_eq!(e.matches("warning").count(), 1, "{e}");
    assert_eq!(std::fs::read_dir(open.join("jit")).unwrap().count(), 0, "the open cache was written");
    let _ = std::fs::remove_dir_all(&d);
}
