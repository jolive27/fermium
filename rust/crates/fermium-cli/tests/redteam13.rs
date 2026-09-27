//! Red team 13: pathological inputs and student-tool edges (dev-notes/REDTEAM.md, "Round 13").
use std::path::PathBuf;
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-rt13-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fermium(args: &[&str], cwd: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fermium")).args(args).current_dir(cwd).output().unwrap()
}

/// Run `src` as prog.fm with `fermium <cmd>`: (exit code, stdout, stderr).
fn run_cmd(tag: &str, cmd: &str, src: &str) -> (i32, String, String) {
    let d = dir(tag);
    std::fs::write(d.join("prog.fm"), src).unwrap();
    let o = fermium(&[cmd, "prog.fm"], &d);
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(),
     String::from_utf8_lossy(&o.stderr).into_owned())
}

fn run(tag: &str, src: &str) -> (i32, String, String) {
    run_cmd(tag, "run", src)
}

/// The program is refused before it runs with the unit-power error, pointing at `caret_line`.
fn assert_power_error(tag: &str, src: &str, what: &str, caret_line: u32) {
    for cmd in ["run", "check"] {
        let (code, out, err) = run_cmd(&format!("{tag}-{cmd}"), cmd, src);
        assert_eq!(code, 1, "{cmd} {src}\nstdout: {out}\nstderr: {err}");
        assert!(!err.contains("panicked"), "{err}");
        assert!(out.is_empty(), "nothing may print: {out}");
        let first = err.lines().next().unwrap_or("");
        assert_eq!(first, format!("prog.fm, line {caret_line}: this unit's power is too large to track exactly ({what})"),
                   "{err}");
        assert!(err.contains("^^^"), "a caret: {err}");
        assert!(err.contains("hint: Fermium keeps a unit's power as an exact fraction"), "{err}");
    }
}

// ---------------------------------------------------------------- #1, #2: unit exponent overflow

#[test]
fn nested_square_roots_past_64_bits_are_an_error_not_a_panic() {
    let nest = |n: usize| format!("print {}1 m{}\n", "√(".repeat(n), ")".repeat(n));
    // 62 deep still fits: m^(1/2^62), as v1 prints it
    let (code, out, err) = run("sqrt62", &nest(62));
    assert_eq!((code, out.as_str()), (0, "1 m^(1/4611686018427387904)\n"), "{err}");
    assert_power_error("sqrt64", &nest(64), "m^(1/9223372036854775808)", 1);
}

#[test]
fn powers_of_powers_that_overflow_are_refused() {
    assert_power_error("tiny", "x = 1 m\nz = (x^(1/4294967296))^(1/4294967296)\nprint z\n",
                       "m^(1/18446744073709551616)", 2);
    // wrapped to m⁰ before, so `y + 1` passed the unit check and printed 2
    assert_power_error("huge", "x = 1 m\ny = (x^4294967296)^4294967296\nprint y + 1\n", "m^18446744073709551616", 2);
    assert_power_error("prod", "x = 1 m^4611686018427387904\ny = x x\n", "m^9223372036854775808", 2);
}

#[test]
fn doubling_generic_functions_overflow_at_the_definition() {
    let mut src = String::from("f0(x) = x\n");
    for i in 1..=64 {
        src += &format!("f{i}(x) = f{}(x) f{}(x)\n", i - 1, i - 1);
    }
    src += "print f64(1 m) + 1\n";
    assert_power_error("double", &src, "m^9223372036854775808", 64);
}

#[test]
fn repeated_cube_roots_keep_their_sign_or_fail() {
    let src = format!("print {}1 m{}\n", "cbrt(".repeat(40), ")".repeat(40));
    assert_power_error("cbrt", &src, "m^(1/12157665459056928801)", 1);
}

#[test]
fn written_exponents_too_large_for_64_bits() {
    // a unit literal's exponent saturated to m^9223372036854775807 before
    assert_power_error("lit", "print 1 m^1e300\n", "a power of 1e300", 1);
    assert_power_error("litpow", "a = (1 m)^99999999999999999999\nprint a\n", "a power of 100000000000000000000", 1);
    assert_power_error("litmul", "print (1 m)^(4294967296 * 4294967296)\n", "a power of 18446744073709551616", 1);
    // exponents that fit are exact
    let (code, out, err) = run("fits", "x = 1 m^1000\nprint x\nprint (1 m)^1000000000000 / (1 m)^999999999999\n");
    assert_eq!((code, out.as_str()), (0, "1 m¹⁰⁰⁰\n1 m\n"), "{err}");
}

#[test]
fn a_tiny_float_exponent_rounds_like_python() {
    // Fraction(1e-19).limit_denominator(10000) == 0 (the comparison overflowed i128 and picked 1/10000)
    let (code, out, err) = run("tiny-float", "print (1 m)^1e-19\n");
    assert_eq!((code, out.as_str()), (0, "1\n"), "{err}");
}
