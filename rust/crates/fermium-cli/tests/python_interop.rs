//! `use python` end to end (D140, spec §B5.14): the fermium binary runs programs that call NumPy, SciPy and a
//! helper module of the test's own, with both back ends, which must agree. Ports tests/test_python_interop.py
//! (part 1) of Fermium 1.5; conformance/cases/python-interop has the rest. Skipped (with a note) when python3
//! with NumPy and SciPy isn't available.
use std::path::{Path, PathBuf};
use std::process::Command;

const HELPER: &str = r#"
import numpy as np

def energy(m, v):
    return 0.5 * m * v ** 2

def fall(t):
    return 4.9 * np.asarray(t) ** 2

def total(xs):
    return float(np.sum(xs))

def grid(a, b, n):
    return np.linspace(a, b, n)

def fails(x):
    raise ValueError("x must be positive")

def gives_none(x):
    return None

def gives_matrix(x):
    return np.eye(2)

def gives_text(x):
    return "hello"

def gives_complex(x):
    return 1 + 2j

def kind(n):
    return 1.0 if isinstance(n, int) else 0.0
"#;

fn have_python() -> bool {
    let ok = Command::new("python3").args(["-c", "import numpy, scipy"]).output().map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: python3 with numpy and scipy isn't available");
    }
    ok
}

fn helper_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fermium-pyinterop-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("physhelp.py"), HELPER).unwrap();
    d
}

struct Out {
    stdout: String,
    stderr: String,
    ok: bool,
}

fn run_with(src: &str, dir: &Path, backend: &str) -> Out {
    let file = dir.join("prog.fm");
    std::fs::write(&file, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .args(["run", "--backend", backend, "--base-dir"])
        .arg(dir)
        .arg(&file)
        .output()
        .unwrap();
    Out { stdout: String::from_utf8_lossy(&o.stdout).into_owned(), stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
          ok: o.status.success() }
}

/// The output of both back ends (they must agree).
fn both(src: &str, dir: &Path) -> String {
    let a = run_with(src, dir, "auto");
    let b = run_with(src, dir, "interp");
    assert!(a.ok, "{src}\n{}", a.stderr);
    assert_eq!(a.stdout, b.stdout, "{src}");
    a.stdout.trim_end().to_string()
}

/// Both back ends stop with the same error; returns it.
fn error_of(src: &str, dir: &Path) -> String {
    let a = run_with(src, dir, "auto");
    let b = run_with(src, dir, "interp");
    assert!(!a.ok && !b.ok, "{src}\n{}", a.stdout);
    assert_eq!(a.stderr, b.stderr);
    a.stderr
}

#[test]
fn declared_signature_converts_units_both_ways() {
    if !have_python() {
        return;
    }
    let d = helper_dir("sig");
    let out = both("use python physhelp as ph:\n    energy(m [kg], v [km/s]) -> [J]\n    fall(t [s]) -> [m]\n\
                    E = ph.energy(2 kg, 3000 m/s)\nprint E\nprint E in kJ to 4 digits\n\
                    print ph.energy(2000 g, 3 km/s) + 1 J\nprint ph.fall([1, 2, 3] [s])\n\
                    print ph.fall(1 s) in cm to 4 digits\n", &d);
    // v is passed in km/s (3), so Python computes 0.5 * 2 * 3² = 9, read as joules
    assert_eq!(out, "9 J\n0.009000 kJ\n10 J\n[4.90, 19.6, 44.1] m\n490.0 cm");
}

#[test]
fn declared_signature_checks_the_arguments() {
    if !have_python() {
        return;
    }
    let d = helper_dir("check");
    let head = "use python physhelp as ph:\n    energy(m [kg], v [m/s]) -> [J]\n";
    let e = error_of(&format!("{head}print ph.energy(2 kg, 3 m)"), &d);
    assert!(e.contains("line 3: ph.energy expects v in m/s (declared on line 1), but got length [m]\n"), "{e}");
    let e = error_of(&format!("{head}print ph.energy(2 kg)"), &d);
    assert!(e.contains("ph.energy takes 2 arguments (as declared in the use line on line 1), but got 1"), "{e}");
    let e = error_of(&format!("{head}x = ph.energy(2 kg, 3 m/s) + 1 m"), &d);
    assert!(e.contains("energy") && e.contains("length"), "{e}");
}

#[test]
fn lists_go_in_as_numpy_arrays_and_come_back() {
    if !have_python() {
        return;
    }
    let d = helper_dir("lists");
    let out = both("use python numpy as np:\n    sum(xs) -> number\n    linspace(a, b, n: int) -> list\n\
                    use python physhelp as ph:\n    total(xs) -> number\n    grid(a [km], b [km], n: int) -> list [km]\n\
                    xs = [0, 0.5, 1, 2]\nprint np.sqrt(xs) to 6 digits\nprint np.sum(xs), ph.total(xs)\n\
                    print np.linspace(0, 1, 5)\nprint ph.grid(0 m, 2 km, 3)\nys = np.exp(-xs)\n\
                    print len(ys), ys[4] to 6 digits\n", &d);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "[0, 0.707107, 1.00000, 1.41421]");
    assert_eq!(lines[1], "3.50 3.50");
    assert_eq!(lines[2], "[0, 0.250, 0.500, 0.750, 1.00]");
    assert_eq!(lines[3], "[0, 1, 2] km");
    assert_eq!(lines[4], format!("4 {:.6}", (-2f64).exp()));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn int_parameters_are_passed_as_python_ints() {
    if !have_python() {
        return;
    }
    let d = helper_dir("ints");
    assert_eq!(both("use python physhelp as ph:\n    kind(n: int) -> number\nuse python physhelp as ph2\n\
                     print ph.kind(3), ph2.kind(3)\n", &d), "1 0");
    let e = error_of("use python physhelp as ph:\n    kind(n: int) -> number\nprint ph.kind(2.5)", &d);
    assert!(e.contains("ph.kind: n must be a whole number (it is passed as an int), not 2.5"), "{e}");
    let e = error_of("use python physhelp as ph:\n    grid(a, b, n: int) -> list\nprint ph.grid(0, 1, [2, 2.5])", &d);
    assert!(e.contains("ph.grid: n must be a list of whole numbers (it is passed as ints)"), "{e}");
}

#[test]
fn python_errors_and_bad_results_are_runtime_errors_with_the_line() {
    if !have_python() {
        return;
    }
    let d = helper_dir("errors");
    let cases = [
        ("ph.fails(1)", "the Python function ph.fails failed: ValueError: x must be positive"),
        ("ph.gives_none(1)", "the Python function ph.gives_none returned nothing (None), but Fermium needs a number"),
        ("ph.gives_matrix(1)", "ph.gives_matrix returned an array of shape (2, 2), but Fermium expected a number here"),
        ("ph.gives_text(1)", "ph.gives_text returned the text 'hello', but Fermium expected a number here"),
        ("ph.gives_complex(1)", "ph.gives_complex returned a complex number, but Fermium expected a real number here"),
        ("ph.total([1, 2])", "ph.total returned a single number, but Fermium expected a list here"),
    ];
    for (call, msg) in cases {
        let e = error_of(&format!("use python physhelp as ph\nx = 1\nprint {call}"), &d);
        assert!(e.contains(&format!("line 3: {msg}")), "{call}: {e}");
    }
}

#[test]
fn python_calls_in_functions_loops_and_lists_agree_between_back_ends() {
    if !have_python() {
        return;
    }
    let d = helper_dir("loops");
    let out = both("use python numpy as np\nf(x) = np.sin(x) + 1\ns = 0\nfor k from 1 to 100\n    s += f(k / 100)\n\
                    print s to 12 digits\nprint np.cumsum([1, 2, 3])\nprint np.pi to 12 digits\n", &d);
    let want: f64 = (1..=100).map(|k| (k as f64 / 100.0).sin() + 1.0).sum();
    let lines: Vec<&str> = out.lines().collect();
    assert!((lines[0].parse::<f64>().unwrap() - want).abs() < 1e-9 * want, "{out}");
    assert_eq!(lines[1], "[1, 3, 6]");
    assert_eq!(lines[2], "3.14159265359");
}

#[test]
fn the_program_folder_is_on_the_python_path_and_errors_name_the_module() {
    if !have_python() {
        return;
    }
    let d = helper_dir("path");
    let e = error_of("use python physhelp as ph\nprint ph.enrgy(1, 2)", &d);
    assert!(e.contains("the Python module physhelp has no enrgy") && e.contains("did you mean ph.energy?"), "{e}");
    std::fs::write(d.join("broken_mod.py"), "raise RuntimeError('boom')\n").unwrap();
    let e = error_of("use python broken_mod as b\nprint 1", &d);
    assert!(e.contains("importing the Python module broken_mod failed: RuntimeError: boom"), "{e}");
    std::fs::write(d.join("needs_dep.py"), "import not_a_module_xyz\n").unwrap();
    let e = error_of("use python needs_dep as n\nprint 1", &d);
    assert!(e.contains("can't find the Python module needs_dep")
            && e.contains("needs_dep needs not_a_module_xyz: install it with  pip install not_a_module_xyz"), "{e}");
}

#[test]
fn a_program_without_python_never_loads_it() {
    // FERMIUM_LIBPYTHON pointing nowhere: a program that doesn't use Python runs; one that does says why it can't
    let d = helper_dir("nopython");
    let file = d.join("plain.fm");
    std::fs::write(&file, "print 2 m + 3 m\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run"]).arg(&file)
        .env("FERMIUM_LIBPYTHON", "/nonexistent/libpython3.so").output().unwrap();
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout), "5 m\n");
    std::fs::write(&file, "use python math as m\nprint m.sqrt(4)\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run"]).arg(&file)
        .env("FERMIUM_LIBPYTHON", "/nonexistent/libpython3.so").output().unwrap();
    assert!(!o.status.success());
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("line 1: use python needs Python 3, but it couldn't be loaded"), "{e}");
}

/// `cd folder; fermium run prog.fm` (a bare file name, no --base-dir): the .py file beside the program is found
/// (red team 10 #4: the empty folder name was skipped).
#[test]
fn a_bare_file_name_finds_the_module_beside_it() {
    if !have_python() {
        return;
    }
    let dir = helper_dir("bare");
    std::fs::write(dir.join("prog.fm"), "use python physhelp as ph:\n    fall(t [s]) -> [m]\nprint ph.fall(1 s)\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run", "prog.fm"]).current_dir(&dir).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
