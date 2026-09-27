//! Red team 14: the new v2.5 features (dev-notes/REDTEAM.md, "Round 14").
use std::path::PathBuf;
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-rt14-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fermium(args: &[&str], cwd: &PathBuf, backend: &str) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fermium"));
    c.args(args).current_dir(cwd);
    if !backend.is_empty() {
        c.env("FERMIUM_BACKEND", backend);
    }
    c.output().unwrap()
}

/// Run `src` as prog.fm with `fermium <cmd>` on a back end ("" = the default): (exit code, stdout, stderr).
fn run_on(tag: &str, cmd: &str, backend: &str, src: &str) -> (i32, String, String) {
    let d = dir(tag);
    std::fs::write(d.join("prog.fm"), src).unwrap();
    let o = fermium(&[cmd, "prog.fm"], &d, backend);
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(),
     String::from_utf8_lossy(&o.stderr).into_owned())
}

fn run(tag: &str, src: &str) -> (i32, String, String) {
    run_on(tag, "run", "", src)
}

/// The same output on both back ends.
fn run_both(tag: &str, src: &str) -> (i32, String, String) {
    let a = run_on(&format!("{tag}-interp"), "run", "interp", src);
    let b = run_on(&format!("{tag}-llvm"), "run", "llvm", src);
    assert_eq!(a, b, "the back ends differ on {src}");
    a
}

/// "v ± σ" → (v, σ) for the printed uncertain numbers of a line.
fn pm(s: &str) -> (f64, f64) {
    let parts: Vec<&str> = s.split('±').map(str::trim).collect();
    assert_eq!(parts.len(), 2, "not an uncertain value: {s}");
    (parts[0].parse().unwrap(), parts[1].parse().unwrap())
}

// ---------------------------------------------------------------- #1: a jump at an uncertain parameter

#[test]
fn a_jump_at_an_uncertain_point_keeps_its_uncertainty() {
    let (code, out, err) = run("jump", "a = 1.0 ± 0.1\n\
                                        I = ∫ (if x < a then 1 else 0) dx from 0 to 2\nprint I\n\
                                        solve x' = if t < a then 1 else 0 with x(0) = 0 for t from 0 to 2\n\
                                        print x(2)\n");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "1.00 ± 0.10\n1.00 ± 0.10\n", "{err}");
    // the fallback says so (Monte Carlo), once per kernel
    assert_eq!(err.matches("Monte Carlo").count(), 2, "{err}");
}

#[test]
fn sign_and_floor_of_an_uncertain_shift_propagate() {
    // ∫ sign(a − x) dx from 0 to 2 = 2a − 2; ∫ floor(x + a) dx from 0 to 2 has slope floor(2 + a) − floor(a) = 2
    let (code, out, err) = run("signfloor", "a = 1.0 ± 0.01\nprint ∫ sign(a - x) dx from 0 to 2\n\
                                             print ∫ floor(x + a) dx from 0 to 2\n");
    assert_eq!(code, 0, "{err}");
    let l: Vec<&str> = out.lines().collect();
    let (v, s) = pm(l[0]);
    assert!(v.abs() < 1e-3 && (s - 0.020).abs() < 0.002, "{out}");
    let (v, s) = pm(l[1]);
    assert!((v - 3.0).abs() < 1e-3 && (s - 0.020).abs() < 0.002, "{out}");
}

#[test]
fn a_parameter_that_does_not_move_the_result_stays_plain_and_linear() {
    // a is read (in a comparison far from the range) but the integral doesn't depend on it: no Monte Carlo
    // (and prints a plain number, as Fermium 1.5 does)
    let (code, out, err) = run("farjump", "a = 5.0 ± 0.1\nprint ∫ (if x < a then x else 0) dx from 0 to 2\n\
                                           solve x' = if t < a then 1 else 0 with x(0) = 0 for t from 0 to 2\n\
                                           print x(2)\n");
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("Monte Carlo"), "{err}");
    assert_eq!(out, "2\n2\n");
}

// ---------------------------------------------------------------- #6: ODE Monte Carlo and turning points

#[test]
fn decay_and_pendulum_with_small_uncertainties_stay_linear() {
    let (code, out, err) = run("lin", "k = 1.0 ± 0.1\n\
                                       solve x' = -k x with x(0) = 1.0 for t from 0 to 3\n\
                                       print x(3)\n\
                                       g = 9.81 ± 0.05\nL = 1.0 ± 0.01\n\
                                       solve θ'' = -(g/L) θ with θ(0) = 0.1, θ'(0) = 0 for t from 0 to 3\n\
                                       print value(θ(2.5)), uncertainty(θ(2.5))\n");
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("Monte Carlo"), "linear propagation, no warning: {err}");
    let l: Vec<&str> = out.lines().collect();
    // e^{-3k}: value e^{-3}, σ = 3 e^{-3} 0.1
    let (v, s) = pm(l[0]);
    assert!((v - (-3.0f64).exp()).abs() < 1e-3 && (s - 0.3 * (-3.0f64).exp()).abs() < 1e-3, "{out}");
    // 0.1 cos(ω t), ω = √(g/L): σ² = (0.1 t sin(ωt) ∂ω)², ∂ω/∂g = ω/(2g) σ_g, ∂ω/∂L = −ω/(2L) σ_L
    let w = 9.81f64.sqrt();
    let t = 2.5;
    let dw = ((w / (2.0 * 9.81) * 0.05).powi(2) + (w / 2.0 * 0.01).powi(2)).sqrt();
    let vs: Vec<f64> = l[1].split_whitespace().map(|x| x.parse().unwrap()).collect();
    let (v, s) = (vs[0], vs[1]);
    assert!((v - 0.1 * (w * t).cos()).abs() < 1e-4, "{out}"); // printed to the uncertainty's figures
    assert!((s - 0.1 * t * (w * t).sin().abs() * dw).abs() < 0.1 * s, "{out}");
}

#[test]
fn a_monte_carlo_solution_reports_the_nominal_value_and_one_set_of_samples() {
    let (code, out, err) = run("mcode", "k = 1.0 ± 0.4\n\
                                         solve x' = -k x with x(0) = 1.0 for t from 0 to 3\n\
                                         print x(3)\nprint x(2) - x(1)^2\na = x(2)\nprint a - x(2)\n");
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("Monte Carlo"), "{err}");
    let l: Vec<&str> = out.lines().collect();
    // the value is the nominal solution's e^{-3} (the Monte Carlo mean would be ≈ 0.10)
    let (v, s) = pm(l[0]);
    assert!((v - 0.05).abs() < 0.006 && s > 0.1, "{out}");
    let (v, _) = pm(l[1]);
    assert!(v.abs() < 1e-3, "{out}");
    assert_eq!(l[2], "0 ± 0", "{out}");
}

// ---------------------------------------------------------------- #2: foreign functions and ±

fn cc(d: &PathBuf, src: &str, lib: &str) -> bool {
    std::fs::write(d.join("lib.c"), src).unwrap();
    match Command::new("cc").args(["-shared", "-fPIC", "-o", lib, "lib.c"]).current_dir(d).output() {
        Ok(o) if o.status.success() => true,
        _ => {
            eprintln!("note: no C compiler (cc), skipping");
            false
        }
    }
}

#[test]
fn a_c_function_refuses_an_uncertain_argument() {
    let d = dir("cunc");
    if !cc(&d, "double sq(double x) { return x * x; }\n", "libsq.so") {
        return;
    }
    let src = "import c \"libsq.so\":\n    sq(x [m]) -> [m²]\nprint sq(2 m)\nprint sq((2.0 ± 0.1) * 1 m)\n";
    std::fs::write(d.join("prog.fm"), src).unwrap();
    // (a program with ± runs on the tree-walker: auto chooses it, and the LLVM back end refuses it)
    for backend in ["interp", ""] {
        let o = fermium(&["run", "prog.fm"], &d, backend);
        let (out, err) = (String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert_eq!(o.status.code(), Some(1), "{backend}: {out} {err}");
        assert_eq!(out, "4 m²\n", "{backend}");
        assert!(err.starts_with("prog.fm, line 4: sq is a C function, which takes plain numbers, but got an uncertain \
                                 value (±)"), "{backend}: {err}");
        assert!(err.contains("hint: write value(x) to drop the uncertainty"), "{err}");
    }
    // in a Monte Carlo block the function gets plain samples
    std::fs::write(d.join("mc.fm"), "import c \"libsq.so\":\n    sq(x [m]) -> [m²]\na = (2.0 ± 0.1) * 1 m\n\
                                     propagate montecarlo\n    y = sq(a)\nprint y\n").unwrap();
    let o = fermium(&["run", "mc.fm"], &d, "");
    assert_eq!(String::from_utf8_lossy(&o.stdout), "4.01 ± 0.40 m²\n", "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn use_python_refuses_an_uncertain_argument_as_v1_does() {
    let d = dir("pyunc");
    std::fs::write(d.join("mymod.py"), "def sq(x):\n    return x * x\n").unwrap();
    std::fs::write(d.join("prog.fm"), "use python mymod as mm:\n    sq(x [m]) -> [m²]\nprint mm.sq(2 m)\n\
                                       print mm.sq((2.0 ± 0.1) * 1 m)\n").unwrap();
    let o = fermium(&["run", "prog.fm"], &d, "");
    let (out, err) = (String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    if !out.starts_with("4 m²") {
        eprintln!("note: no Python here, skipping: {err}");
        return;
    }
    assert_eq!(o.status.code(), Some(1), "{out} {err}");
    assert_eq!(out, "4 m²\n");
    assert!(err.starts_with("prog.fm, line 4: this operation needs a plain number, but got an uncertain value (±); \
                             write value(x) to drop the uncertainty"), "{err}");
}

// ---------------------------------------------------------------- #3, #7: redefinitions and versions

#[test]
fn a_later_less_specific_definition_replaces_as_in_v1() {
    let src = "force(x [m]) = 3 N/m x\nprint force(1 m)\nforce(x) = 5 N/m x\nprint force(1 m)\nprint force(2)\n\
               d(x) = 0\nd(x [m]) = 1\nprint d(2), d(2 m)\n\
               k(x: vector [m]) = 1\nk(x [m]) = 2\nprint k(<1, 2> m), k(3 m)\n";
    let (code, out, err) = run_both("replace", src);
    assert_eq!(code, 0, "{err}");
    // force: replaced (v1 prints 3 N then 5 N); d: a more specific version written later adds one; k: [m] doesn't
    // cover a vector, so both stay
    assert_eq!(out, "3 N\n5 N\n10 N/m\n0 1\n1 2\n", "{err}");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn replacing_hz_by_rad_per_s_warns() {
    let src = "E(f [Hz]) = h f\nE(ω [rad/s]) = ħ ω\nprint E(1 GHz)\ng(x [m]) = 1\ng(y [km]) = 2\nprint g(3 m)\n";
    let (code, out, err) = run("respelt", src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "1.05×10⁻²⁵ J\n2\n");
    assert!(err.starts_with("warning: line 2: E(ω [rad/s]) replaces E(f [Hz]) (line 1): their units have the same \
                             dimensions, so they can't be two versions"), "{err}");
    assert_eq!(err.matches("warning").count(), 1, "m then km is an ordinary redefinition: {err}");
}

// ---------------------------------------------------------------- #4: a declared list parameter

#[test]
fn a_declared_list_parameter_takes_the_list_whole() {
    let (code, out, err) = run_both("listkind", "s(x: list) = 2\nprint s([1, 2])\n\
                                                 t(x: list, y) = y + 1\nprint t([1, 2, 3], 1)\n\
                                                 u(x) = 2 x\nprint u([1, 2])\n");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "2\n2\n[2, 4]\n", "{err}");
}

// ---------------------------------------------------------------- #5: checking doesn't run library code

#[test]
fn check_does_not_load_the_library_but_still_finds_a_missing_function() {
    let d = dir("noload");
    let marker = d.join("PWNED");
    let lib = format!("#include <stdio.h>\n__attribute__((constructor)) static void pwn(void) {{ FILE *f = \
                       fopen(\"{}\", \"a\"); if (f) {{ fputs(\"ran\\n\", f); fclose(f); }} }}\n\
                       double foo(double x) {{ return x; }}\n", marker.display());
    if !cc(&d, &lib, "libevil.so") {
        return;
    }
    std::fs::write(d.join("ok.fm"), "import c \"libevil.so\":\n    foo(x) -> number\nprint foo(1)\n").unwrap();
    std::fs::write(d.join("bad.fm"), "import c \"libevil.so\":\n    fooo(x) -> number\nprint 1\n").unwrap();
    let o = fermium(&["check", "ok.fm"], &d, "");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = fermium(&["check", "bad.fm"], &d, "");
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("the C library libevil.so has no function fooo"),
            "{}", String::from_utf8_lossy(&o.stderr));
    std::fs::write(d.join("gone.fm"), "import c \"./libnothere.so\":\n    foo(x) -> number\nprint 1\n").unwrap();
    let o = fermium(&["check", "gone.fm"], &d, "");
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("can't load the C library ./libnothere.so"),
            "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!marker.exists(), "fermium check ran the library's constructor");
    // running loads it, as it must
    let o = fermium(&["run", "ok.fm"], &d, "");
    assert_eq!(String::from_utf8_lossy(&o.stdout), "1\n");
    assert!(marker.exists());
}

// ---------------------------------------------------------------- #8: ∂/∂y f(1, 3) with versions

#[test]
fn partial_of_a_call_considers_every_version() {
    let (code, out, err) = run_both("partial", "f(x, y) = x y²\nf(x) = 2 x\nprint ∂/∂y f(1, 3)\n\
                                                print (∂/∂y f)(1, 3)\n");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "6\n6\n", "{err}");
}

// ---------------------------------------------------------------- #9: messages and gaps

#[test]
fn kind_hints_make_sense() {
    let (code, _, err) = run("int", "f(n: int) = n\nprint f(2)\n");
    assert_eq!(code, 1);
    assert!(err.contains("hint: a Fermium function takes any number, so write just  n  (or  n: number)"), "{err}");
    let (_, _, err) = run("vectr", "g(x: vectr) = 1\nprint g(2)\n");
    assert!(err.contains("hint: did you mean  x: vector ?"), "{err}");
    let (_, _, err) = run("cvec", "import c \"libm.so.6\":\n    cos(x: vector) -> number\nprint 1\n");
    assert!(err.contains("a foreign function takes doubles, ints and arrays of doubles"), "{err}");
    assert!(!err.contains("[vector]"), "{err}");
}

#[test]
fn a_fractional_array_index_says_so() {
    for src in ["A = fill(0, 2, 2)\nprint A[1.5, 1]\n", "A = fill(0, 2, 2)\nA[1.5, 1] = 2\n"] {
        let (code, _, err) = run("arr15", src);
        assert_eq!(code, 1);
        assert!(err.starts_with("prog.fm, line 2: an array index must be a whole number (1, 2, 3, ...), not 1.5"),
                "{err}");
        assert!(err.contains("^^^"), "a caret: {err}");
    }
    // found when the program runs
    let (code, _, err) = run_both("arrrun", "A = fill(0, 2, 2)\ni = 1.5\nprint A[i, 1]\n");
    assert_eq!(code, 1);
    assert!(err.starts_with("prog.fm, line 3: an array index must be a whole number (1, 2, 3, ...), not 1.5"), "{err}");
}

#[test]
fn the_rk4_hint_uses_the_programs_range() {
    let (code, _, err) = run("rk4", "solve x' = -x with x(0 s) = 1 for t from 0 s to 10 s using rk4\nprint x(1 s)\n");
    assert_eq!(code, 1);
    assert!(err.starts_with("prog.fm, line 1: the rk4 method needs a fixed step:  for t from 0 s to 10 s step 0.01 s"),
            "{err}");
}

#[test]
fn an_empty_list_can_become_a_text_list_by_assignment() {
    let (code, out, err) = run_both("emptytext", "t = [\"a\", \"b\"]\nnames = []\nnames = t\npush(names, \"c\")\n\
                                                  print len(names), names[3]\nv = []\nv = [<1, 2> m]\nprint v\n");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "3 c\n[<1, 2>] m\n");
}
