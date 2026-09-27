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

// ---------------------------------------------------------------- #3: plot … to "<path>"

#[test]
fn plots_refuse_non_image_paths_and_write_nothing() {
    let d = dir("plot");
    std::fs::write(d.join("notes.txt"), "my notes\n").unwrap();
    std::fs::write(d.join("prog.fm"), "plot sin(x) vs x from 0 to 1 to \"notes.txt\"\n\
        plot sin(x) vs x from 0 to 1 to \"newdir/a/b.py\"\n\
        plot sin(x) vs x from 0 to 1 to \"prog.fm\"\n\
        plot sin(x) vs x from 0 to 1 to \"c.pdf\"\n\
        print 1\n").unwrap();
    let o = fermium(&["run", "prog.fm"], &d);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let refused = |e: &str| format!("(plot not saved: Format '{e}' is not supported (supported formats: gif, png, svg))");
    assert_eq!(out, format!("{}\n{}\n{}\n{}\n1\n", refused("txt"), refused("py"), refused("fm"), refused("pdf")));
    assert_eq!(std::fs::read_to_string(d.join("notes.txt")).unwrap(), "my notes\n");
    assert!(std::fs::read_to_string(d.join("prog.fm")).unwrap().starts_with("plot sin"));
    assert!(!d.join("newdir").exists(), "no folder is made for a refused plot");
    assert!(!d.join("c.pdf").exists());
}

#[test]
fn plots_write_png_svg_and_gif_as_their_extension_says() {
    let d = dir("plot-ok");
    std::fs::write(d.join("prog.fm"), "plot sin(x) vs x from 0 to 1 to \"a.PNG\"\n\
        plot sin(x) vs x from 0 to 1 to \"out/b.svg\"\n\
        plot sin(x) vs x from 0 to 1 to \"c.gif\"\n\
        plot sin(x) vs x from 0 to 1 to \"noext\"\n").unwrap();
    let o = fermium(&["run", "prog.fm"], &d);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let abs = std::fs::canonicalize(&d).unwrap();
    let saved: Vec<String> = ["a.PNG", "out/b.svg", "c.gif", "noext.png"]
        .iter()
        .map(|f| format!("plot saved to {}", abs.join(f).display()))
        .collect();
    assert_eq!(out, saved.join("\n") + "\n");
    assert_eq!(&std::fs::read(d.join("a.PNG")).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    assert!(std::fs::read_to_string(d.join("out/b.svg")).unwrap().starts_with("<?xml"));
    assert_eq!(&std::fs::read(d.join("c.gif")).unwrap()[..6], b"GIF89a");
    // like matplotlib: a path without an extension gets .png (it doesn't overwrite a file named `noext`)
    assert_eq!(&std::fs::read(d.join("noext.png")).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    assert!(!d.join("noext").exists());
}

// ---------------------------------------------------------------- #4: Unicode look-alike signs

#[test]
fn ohm_kelvin_and_angstrom_signs_read_as_v1_reads_them() {
    // U+2126 OHM SIGN, U+212A KELVIN SIGN, U+212B ANGSTROM SIGN (what Word, PDFs and some keyboards produce);
    // text in strings is kept exactly as typed. Expected output: `fermium-legacy run` (v1).
    let src = "print 5 \u{2126}\nR = 5 \u{2126}\nprint R\nprint 5 k\u{2126}\nprint 300 \u{212a}\nprint 1 \u{212b}\n\
               \u{2126}x = 2\nprint \u{2126}x\n\u{212a} = 3\nprint \u{212a}\nprint \"\u{2126} \u{212a}\"\n\
               \u{3d5} = 1 m\n\u{3c6} = 2 m\nprint \u{3d5} + \u{3c6}\n";
    let (code, out, err) = run("signs", src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "5 \u{3a9}\n5 \u{3a9}\n5 k\u{3a9}\n300 K\n1 \u{c5}\n2\n3\n\u{2126} \u{212a}\n4 m\n");
    // v1 doesn't map these either (ring above, masculine ordinal): the same errors
    let (code, _, err) = run("ring", "T = 20 \u{2da}C\n");
    assert_eq!(code, 1);
    assert!(err.starts_with("prog.fm, line 1: unexpected character '\u{2da}' (Ring Above)"), "{err}");
    let (code, _, err) = run("ordinal", "print 20 \u{ba}C\n");
    assert_eq!(code, 1);
    assert!(err.starts_with("prog.fm, line 1: '\u{ba}C' isn't a unit Fermium knows"), "{err}");
}

// ---------------------------------------------------------------- lows: build, help, deep nesting

#[test]
fn build_refuses_to_overwrite_its_own_source() {
    for (tag, file, args) in [("noext", "noext", vec!["build", "noext"]),
                              ("same", "prog.fm", vec!["build", "-o", "prog.fm", "prog.fm"]),
                              ("dotted", "prog.fm", vec!["build", "-o", "./prog.fm", "prog.fm"])] {
        let d = dir(&format!("build-{tag}"));
        std::fs::write(d.join(file), "print 1\n").unwrap();
        let o = fermium(&args, &d);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(err.contains("fermium build would overwrite the program") && err.contains("-o NAME"), "{err}");
        assert_eq!(std::fs::read_to_string(d.join(file)).unwrap(), "print 1\n", "the source is untouched");
    }
}

#[test]
fn help_describes_build_as_available() {
    let o = fermium(&["--help"], &dir("help"));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("build               compile a program into a standalone executable\n"), "{out}");
    assert!(!out.contains("not\n") && !out.contains("yet in this version"), "{out}");
}

#[test]
fn deep_nesting_keeps_v1s_detail_and_hint() {
    let src = format!("x = {}1{}\nprint x\n", "(".repeat(5000), ")".repeat(5000));
    let (code, out, err) = run("deep", &src);
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, "this program is nested too deeply for Fermium to compile (very long or deeply nested \
                     expressions)\n  hint: split the expression into several lines with names\n");
}
