//! `import c` / `import fortran` end to end (spec C3, DECISIONS D275): the fermium binary calls functions in
//! shared libraries the test builds with `cc` and `gfortran`, with both back ends (which must agree) and in an
//! executable made by `fermium build`. C stand-ins with Fortran's conventions (arguments by reference, the
//! trailing underscore) test the Fortran side even where gfortran is missing; the tests that need a compiler
//! are skipped with a note when it isn't installed.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const C_LIB: &str = r#"
#include <math.h>
double kinetic_energy(double m, double v) { return 0.5 * m * v * v; }
double ident(double x) { return x; }
int twice(int n) { return 2 * n; }
int negate(int n) { return -n; }
double scaled(double x, int n) { return x * n; }
double sum_sq(const double *x, int n) { double s = 0; for (int i = 0; i < n; i++) s += x[i] * x[i]; return s; }
double dot(const double *x, const double *y, int n) { double s = 0; for (int i = 0; i < n; i++) s += x[i] * y[i]; return s; }
double first_of(const double *x, int n) { return n > 0 ? x[0] : -1.0; }
double delta_e(double x) { return x + 0.5; }
double v_0(double x) { return 3 * x; }
double mix16(double a0, int i0, double a1, int i1, double a2, int i2, double a3, int i3,
             double a4, int i4, double a5, int i5, double a6, int i6, double a7, int i7) {
    double f[8] = {a0, a1, a2, a3, a4, a5, a6, a7};
    int k[8] = {i0, i1, i2, i3, i4, i5, i6, i7};
    double s = 0;
    for (int j = 0; j < 8; j++) s += f[j] * (j + 1) + k[j] * 1000.0 * (j + 1);
    return s;
}
double many16(double a0, double a1, double a2, double a3, double a4, double a5, double a6, double a7,
              double a8, double a9, double a10, double a11, double a12, double a13, double a14, double a15) {
    double v[16] = {a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15};
    double s = 0;
    for (int j = 0; j < 16; j++) s += v[j] * (j + 1);
    return s;
}
static int calls = 0;
double counted(double x) { calls++; return x + calls; }
/* C stand-ins with Fortran's conventions */
double fstand_(const double *x, const int *n) { return *x * *n; }
double cstyle(const double *x) { return 2 * *x; }
double renamed_symbol(const double *x) { return *x + 100; }
int fnext_(const int *n) { return *n + 1; }
double fsum_(const double *x, const int *n) { double s = 0; for (int i = 0; i < *n; i++) s += x[i]; return s; }
"#;

const F_LIB: &str = r#"
real(8) function scale(x, n)
    implicit none
    real(8), intent(in) :: x
    integer, intent(in) :: n
    scale = x * n
end function scale

real(8) function total(x, n)
    implicit none
    integer, intent(in) :: n
    real(8), intent(in) :: x(n)
    total = sum(x)
end function total

real(8) function cname(x) bind(C)
    implicit none
    real(8), intent(in) :: x
    cname = 2 * x
end function cname

real(8) function other(x) bind(C, name="my_Sym")
    implicit none
    real(8), intent(in) :: x
    other = x - 1
end function other

integer function next_int(n)
    implicit none
    integer, intent(in) :: n
    next_int = n + 1
end function next_int
"#;

fn have(cmd: &str) -> bool {
    let ok = Command::new(cmd).arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !ok {
        eprintln!("skipped: {cmd} isn't installed");
    }
    ok
}

/// A folder with libphys.so (C) and, when gfortran is there, libf.so (Fortran), built once per test process.
fn libs() -> Option<&'static PathBuf> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        if !have("cc") {
            return None;
        }
        let d = std::env::temp_dir().join(format!("fermium-cinterop-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("phys.c"), C_LIB).unwrap();
        let o = Command::new("cc").args(["-O1", "-shared", "-fPIC", "-o"]).arg(d.join("libphys.so"))
            .arg(d.join("phys.c")).arg("-lm").output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        if have("gfortran") {
            std::fs::write(d.join("f.f90"), F_LIB).unwrap();
            let o = Command::new("gfortran").args(["-O1", "-shared", "-fPIC", "-o"]).arg(d.join("libf.so"))
                .arg(d.join("f.f90")).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        Some(d)
    }).as_ref()
}

fn have_fortran_lib() -> Option<&'static PathBuf> {
    let d = libs()?;
    if d.join("libf.so").exists() { Some(d) } else { None }
}

struct Out {
    stdout: String,
    stderr: String,
    ok: bool,
}

fn fermium(args: &[&str], file: &Path, dir: &Path) -> Out {
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(args).arg(file).current_dir(dir)
        .env("FERMIUM_BACKEND_INFO", "1").output().unwrap();
    Out { stdout: String::from_utf8_lossy(&o.stdout).into_owned(), stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
          ok: o.status.success() }
}

fn run_with(src: &str, dir: &Path, backend: &str) -> Out {
    // a file per call, so tests running in parallel in one folder don't overwrite each other's programs
    static K: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let k = K.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let file = dir.join(format!("prog{k}.fm"));
    std::fs::write(&file, src).unwrap();
    let mut o = fermium(&["run", "--backend", backend], &file, dir);
    let _ = std::fs::remove_file(&file);
    // FERMIUM_BACKEND_INFO: which back end ran (the LLVM back end must have compiled these programs)
    let info = o.stderr.lines().find(|l| l.starts_with("fermium: backend")).map(str::to_string);
    if o.ok && backend == "llvm" {
        assert_eq!(info.as_deref(), Some("fermium: backend llvm"), "{src}\n{}", o.stderr);
    }
    // (messages name the file: call it prog.fm whatever its number)
    let name = format!("prog{k}.fm");
    o.stderr = o.stderr.lines().filter(|l| !l.starts_with("fermium: backend")).map(|l| format!("{l}\n")).collect::<String>()
        .replace(&name, "prog.fm");
    o
}

/// The output of both back ends (they must agree).
fn both(src: &str, dir: &Path) -> String {
    let a = run_with(src, dir, "llvm");
    let b = run_with(src, dir, "interp");
    assert!(a.ok, "{src}\n{}", a.stderr);
    assert!(b.ok, "{src}\n{}", b.stderr);
    assert_eq!(a.stdout, b.stdout, "{src}");
    a.stdout.trim_end().to_string()
}

/// Both back ends stop with the same error; returns it.
fn error_of(src: &str, dir: &Path) -> String {
    let a = run_with(src, dir, "llvm");
    let b = run_with(src, dir, "interp");
    assert!(!a.ok && !b.ok, "{src}\n{}", a.stdout);
    assert_eq!(a.stderr, b.stderr, "{src}");
    a.stderr
}

const HEAD: &str = "import c \"libphys.so\":\n    kinetic_energy(m [kg], v [km/s]) -> [J]\n    ident(x [km]) -> [m]\n";

#[test]
fn units_are_converted_both_ways() {
    let Some(d) = libs() else { return };
    let out = both(&format!("{HEAD}E = kinetic_energy(2 kg, 3000 m/s)\nprint E\nprint E in kJ to 4 digits\n\
                             print kinetic_energy(2000 g, 3 km/s) + 1 J\nprint ident(1500 m)\n\
                             print ident(2 km) in cm\n"), d);
    // v is passed in km/s (3), so C computes 0.5 * 2 * 3² = 9, read as joules; ident gets 1.5 (km), read as m
    // (a result prints in its declared unit when that unit is a plain SI one, like any value with a unit hint)
    assert_eq!(out, "9 J\n0.009000 kJ\n10 J\n1.50 m\n200 cm");
}

#[test]
fn int_parameters_and_results() {
    let Some(d) = libs() else { return };
    let src = "import c \"libphys.so\":\n    twice(n: int) -> int\n    negate(n: int) -> int\n    scaled(x [m], n: int) -> [m]\n\
               print twice(21), negate(7), twice(-4)\nprint scaled(1.5 m, 4)\nk = 10\nprint twice(k / 2)\n";
    assert_eq!(both(src, d), "42 -7 -8\n6 m\n10");
    let e = error_of("import c \"libphys.so\":\n    twice(n: int) -> int\nx = 2.5\nprint twice(x)", d);
    assert!(e.contains("line 4: twice: n must be a whole number (it is passed as an int), not 2.5"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    twice(n: int) -> int\nprint twice(3 m)", d);
    assert!(e.contains("line 3: twice expects n to be a whole number (declared as int on line 2), but got length [m]"),
            "{e}");
}

#[test]
fn sixteen_interleaved_arguments() {
    let Some(d) = libs() else { return };
    let mut decl = vec![];
    let mut call = vec![];
    for k in 0..8 {
        decl.push(format!("a{k} [m]"));
        decl.push(format!("i{k}: int"));
        call.push(format!("{} cm", 50 + 100 * k)); // 0.5 m, 1.5 m, ...
        call.push(format!("{}", k + 1));
    }
    let want_mix: f64 = (0..8).map(|j| (0.5 + j as f64) * (j + 1) as f64 + (j + 1) as f64 * 1000.0 * (j + 1) as f64).sum();
    // many16 gets milliseconds (1000, 2000, …) and its result is read as seconds
    let want_many: f64 = (0..16).map(|j| 1000.0 * (j + 1) as f64 * (j + 1) as f64).sum();
    let src = format!("import c \"libphys.so\":\n    mix16({}) -> number\n    many16({}) -> [s]\n\
                       print mix16({}) == {want_mix}\nprint many16({}) == {want_many} s\n",
                      decl.join(", "), (0..16).map(|k| format!("x{k} [ms]")).collect::<Vec<_>>().join(", "),
                      call.join(", "), (0..16).map(|k| format!("{} s", k + 1)).collect::<Vec<_>>().join(", "));
    assert_eq!(both(&src, d), "true\ntrue");
    let e = error_of(&format!("import c \"libphys.so\":\n    many16({}) -> number\n",
                              (0..17).map(|k| format!("x{k}")).collect::<Vec<_>>().join(", ")), d);
    assert!(e.contains("many16 has 17 parameters; a C function can take at most 16 here"), "{e}");
}

#[test]
fn arrays_with_their_length() {
    let Some(d) = libs() else { return };
    let src = "import c \"libphys.so\":\n    sum_sq(x: list [m], n: len(x)) -> [m²]\n    dot(x: list [m], y: list [N], n: len(x)) -> [J]\n\
               \x20   first_of(x: list [km], n: len(x)) -> [km]\n\
               print sum_sq([1 m, 2 m, 300 cm])\nprint dot([1, 2] m, [3, 4] N)\nprint first_of([2500 m, 1 m])\n\
               xs = linspace(0 m, 1 m, 101)\nprint sum_sq(xs) to 6 digits\nprint first_of([] [m])\n";
    assert_eq!(both(src, d), "14 m²\n11 J\n2.50 km\n33.8350 m²\n-1 km");
    let e = error_of("import c \"libphys.so\":\n    dot(x: list [m], y: list [N], n: len(x)) -> [J]\n\
                      print dot([1, 2] m, [1, 2, 3] N)", d);
    assert!(e.contains("line 3: dot: y has 3 elements, but x has 2; they are passed with one length (n)"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    sum_sq(x: list [m]) -> [m²]\n", d);
    assert!(e.contains("sum_sq takes the list x, so it needs its length too") && e.contains("n: len(x)"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    sum_sq(x [m], n: len(x)) -> [m²]\n", d);
    assert!(e.contains("sum_sq: x isn't a list, so len(x) can't be passed"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    sum_sq(x: list [m], n: len(y)) -> [m²]\n", d);
    assert!(e.contains("sum_sq: len(y) needs a list parameter named y"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    sum_sq(x: list [m], n: len(x)) -> [m²]\nprint sum_sq(3 m)", d);
    assert!(e.contains("line 3: sum_sq expects x to be a list (declared on line 2), but got a single number"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    sum_sq(x: list [m], n: len(x)) -> [m²]\nprint sum_sq([1, 2] s)", d);
    assert!(e.contains("line 3: sum_sq expects x in m (declared on line 2), but got time [s]"), "{e}");
}

#[test]
fn a_list_for_a_number_parameter_maps_elementwise() {
    let Some(d) = libs() else { return };
    let src = format!("{HEAD}    scaled(x [m], n: int) -> [m]\nprint kinetic_energy(2 kg, [1, 2, 3] km/s)\n\
                       print scaled([1, 2] m, [3, 4])\nprint scaled([1, 2] m, 10)\n");
    assert_eq!(both(&src, d), "[1, 4, 9] J\n[3, 8] m\n[10, 20] m");
    let e = error_of(&format!("{HEAD}    scaled(x [m], n: int) -> [m]\nprint scaled([1, 2] m, [3, 4, 5])"), d);
    assert!(e.contains("scaled: the lists x and n have different lengths (2 and 3)"), "{e}");
    let e = error_of(&format!("{HEAD}    scaled(x [m], n: int) -> [m]\nprint scaled([1, 2] m, [3, 4.5])"), d);
    assert!(e.contains("scaled: n must be a whole number (it is passed as an int), not 4.5"), "{e}");
}

#[test]
fn unit_errors_are_compile_time_errors_with_a_caret_and_a_hint() {
    let Some(d) = libs() else { return };
    let e = error_of(&format!("{HEAD}print \"never printed\"\nprint kinetic_energy(2 kg, 3 s)"), d);
    assert_eq!(e, "prog.fm, line 5: kinetic_energy expects v in km/s (declared on line 2), but got time [s]\n    \
                   print kinetic_energy(2 kg, 3 s)\n                               ^^^\n  hint: pass a speed, like  1 km/s\n");
    // through a generic function: checked per call
    let e = error_of(&format!("{HEAD}f(x) = kinetic_energy(1 kg, x)\nprint f(3 m/s)\nprint f(2 s)"), d);
    assert!(e.contains("line 4: kinetic_energy expects v in km/s (declared on line 2), but got time [s]")
            && e.contains("this happened when calling f on line 6 (with x = time [s])"), "{e}");
    let e = error_of(&format!("{HEAD}x = kinetic_energy(2 kg, 3 km/s) + 1 m"), d);
    assert!(e.contains("line 4:") && e.contains("length"), "{e}");
    let e = error_of(&format!("{HEAD}print kinetic_energy(1 kg)"), d);
    assert!(e.contains("kinetic_energy takes 2 arguments (as declared in the import on line 2), but got 1"), "{e}");
    let e = error_of(&format!("{HEAD}y = kinetic_energy"), d);
    assert!(e.contains("kinetic_energy is a C function from libphys.so; it can only be called, like \
                        kinetic_energy(m, v)"), "{e}");
    let e = error_of(&format!("{HEAD}kinetic_energy = 3"), d);
    assert!(e.contains("kinetic_energy is the C function from libphys.so (line 1) and is defined again on line 4"), "{e}");
    // the spec's  m: kg  spelling points to the bracket form
    let e = error_of("import c \"libphys.so\":\n    kinetic_energy(m: kg, v [km/s]) -> [J]\n", d);
    assert!(e.contains("a parameter can be marked  : int  (a whole number), : list  or  : len(x), not : kg")
            && e.contains("hint: give a unit in brackets instead:  m [kg]"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    kinetic_energy(m [kg], v [km/s])\n", d);
    assert!(e.contains("a C function's signature needs its result after ->"), "{e}");
    let e = error_of("if 1 > 0\n    import c \"libphys.so\":\n        twice(n: int) -> int\n", d);
    assert!(e.contains("import c must be at the top level of the program"), "{e}");
}

#[test]
fn a_missing_library_or_symbol_is_a_compile_error_with_a_hint() {
    let Some(d) = libs() else { return };
    let e = error_of("import c \"libnothing.so\":\n    f(x) -> number\nprint 1", d);
    assert!(e.contains("line 1: can't load the C library libnothing.so")
            && e.contains("hint: build it next to the program first, like  cc -shared -fPIC -o libnothing.so nothing.c"),
            "{e}");
    let e = error_of("import fortran \"libnothing.so\":\n    f(x) -> number\nprint 1", d);
    assert!(e.contains("can't load the Fortran library libnothing.so") && e.contains("gfortran -shared -fPIC"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    kinetic_energyy(m [kg], v [m/s]) -> [J]\nprint 1", d);
    assert!(e.contains("line 2: the C library libphys.so has no function kinetic_energyy")
            && e.contains("hint: check the spelling (nm -D libphys.so lists the functions it defines)"), "{e}");
    // the other Fortran spelling exists: say so
    let e = error_of("import c \"libphys.so\":\n    fstand(x, n: int) -> number\n", d);
    assert!(e.contains("the library has fstand_, a Fortran compiler's spelling: use  import fortran \"libphys.so\""),
            "{e}");
    let e = error_of("import fortran \"libphys.so\":\n    cstyle(x) -> number\n", d);
    assert!(e.contains("no function cstyle (symbol cstyle_)")
            && e.contains("the library has cstyle without the trailing underscore (a bind(C) function): add  bind(C)"),
            "{e}");
    let e = error_of("import fortran \"libphys.so\":\n    fstand(x, n: int) -> number bind(C)\n", d);
    assert!(e.contains("the library has fstand_, Fortran's default spelling: remove  bind(C"), "{e}");
    let e = error_of("import c \"libphys.so\":\n    twice(n: int) -> int bind(C)\n", d);
    assert!(e.contains("bind(C) is for Fortran functions"), "{e}");
}

#[test]
fn fortran_conventions_with_c_stand_ins() {
    let Some(d) = libs() else { return };
    let src = "import fortran \"libphys.so\":\n    fstand(x [cm], n: int) -> [cm]\n    cstyle(x) -> number bind(C)\n\
               \x20   renamed(x) -> number bind(C, name=\"renamed_symbol\")\n    fnext(n: int) -> int\n\
               \x20   fsum(x: list [g], n: len(x)) -> [g]\n\
               print fstand(1.5 m, 2), cstyle(4), renamed(1), fnext(41)\nprint fsum([1, 2, 3.5] kg)\n\
               print fstand([1, 2] m, 3)\n";
    assert_eq!(both(src, d), "300 cm 8 101 42\n6500 g\n[300, 600] cm");
}

#[test]
fn real_fortran_through_gfortran() {
    let Some(d) = have_fortran_lib() else { return };
    let src = "import fortran \"libf.so\":\n    scale(x [km], n: int) -> [km]\n    total(x: list [m], n: len(x)) -> [m]\n\
               \x20   cname(x) -> number bind(C)\n    other(x) -> number bind(C, name=\"my_Sym\")\n    next_int(n: int) -> int\n\
               print scale(1500 m, 4)\nprint total([1, 2, 3.5] m)\nprint cname(21), other(1), next_int(-5)\n\
               Scale(x) = scale(x, 2)\nprint Scale(3 km)\n";
    assert_eq!(both(src, d), "6 km\n6.50 m\n42 0 -4\n6 km");
    // Fortran symbols are lowercase: SCALE in the program is scale_ in the library
    assert_eq!(both("import fortran \"libf.so\":\n    SCALE(x, n: int) -> number\nprint SCALE(2, 3)\n", d), "6");
}

#[test]
fn greek_and_subscript_names_map_to_ascii_symbols_and_fmt_round_trips() {
    let Some(d) = libs() else { return };
    let src = "import c \"libphys.so\":\n    delta_e(x [m]) -> [m]\n    v_0(x) -> number\nprint delta_e(1 m), v_0(2)\n";
    assert_eq!(both(src, d), "1.50 m 6");
    let file = d.join("greek.fm");
    std::fs::write(&file, src).unwrap();
    let p = fermium(&["fmt", "--pretty"], &file, d);
    assert!(p.ok, "{}", p.stderr);
    assert!(p.stdout.contains("    δ_e(x [m]) -> [m]\n    v₀(x) -> number\n") && p.stdout.contains("print δ_e(1 m), v₀(2)"),
            "{}", p.stdout);
    assert_eq!(both(&p.stdout, d), "1.50 m 6"); // the pretty spelling calls the same symbols
    std::fs::write(&file, &p.stdout).unwrap();
    let a = fermium(&["fmt", "--ascii"], &file, d);
    assert_eq!(a.stdout, src);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn keywords_cant_be_function_names() {
    let Some(d) = libs() else { return };
    let e = error_of("import c \"libphys.so\":\n    solve(x) -> number\n", d);
    assert!(e.contains("solve is a Fermium keyword, so it can't be the name of a C function")
            && e.contains("small C wrapper"), "{e}");
    let e = error_of("import fortran \"libphys.so\":\n    for(x) -> number\n", d);
    assert!(e.contains("for is a Fermium keyword, so it can't be the name of a Fortran function")
            && e.contains("bind(C, name=\"for_\")"), "{e}");
}

#[test]
fn calls_in_loops_functions_integrals_and_odes_agree_between_back_ends() {
    let Some(d) = libs() else { return };
    let src = format!("{HEAD}    counted(x) -> number\ntot = 0 J\nfor k from 1 to 1000\n    tot += kinetic_energy(1 kg, k * 1 m/s)\n\
                       print tot to 12 digits\nKE(v) = kinetic_energy(2 kg, v)\n\
                       print ∫ KE(v) dv from 0 m/s to 3 m/s to 10 digits\n\
                       solve x' = ident(1 km) / (1 m) with x(0) = 0 for t from 0 to 2\nprint x(2) to 10 digits\n\
                       print counted(0), counted(0), counted(10)\n");
    let out = both(&src, d);
    let want: f64 = (1..=1000).map(|k| 0.5 * (k as f64 / 1000.0).powi(2)).sum();
    let lines: Vec<&str> = out.lines().collect();
    assert!((lines[0].trim_end_matches(" J").parse::<f64>().unwrap() - want).abs() < 1e-9 * want, "{out}");
    assert_eq!(lines[1], "9.000000000×10⁻⁶ W m"); // ∫ v²/10⁶ dv from 0 to 3 (J m/s)
    assert_eq!(lines[2], "2.000000000");
    assert_eq!(lines[3], "1 2 13"); // a function with side effects is called once per call, in order
}

#[test]
fn fermium_build_executables_call_the_library() {
    let Some(d) = libs() else { return };
    let src = format!("{HEAD}    twice(n: int) -> int\n    sum_sq(x: list [m], n: len(x)) -> [m²]\n\
                       print kinetic_energy(2 kg, 3000 m/s), ident(1500 m), twice(21)\nprint sum_sq([1 m, 2 m, 300 cm])\n\
                       print kinetic_energy(1 kg, [1, 2] km/s)\n");
    let file = d.join("built.fm");
    std::fs::write(&file, &src).unwrap();
    let exe = d.join("built_exe");
    let b = fermium(&["build", "-o", exe.to_str().unwrap()], &file, d);
    assert!(b.ok, "{}", b.stderr);
    let o = Command::new(&exe).current_dir(std::env::temp_dir()).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let want = both(&src, d);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim_end(), want);
    assert_eq!(want, "9 J 1.50 m 42\n14 m²\n[0.500, 2.00] J");
}

#[test]
fn the_worked_example_prints_its_expected_output() {
    let (Some(_), true) = (libs(), have("gfortran")) else { return };
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/c_interop");
    let d = std::env::temp_dir().join(format!("fermium-cinterop-example-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    for f in ["c_interop.fm", "nuclear.f90", "stellar.c", "expected_output.txt"] {
        std::fs::copy(ex.join(f), d.join(f)).unwrap();
    }
    let o = Command::new("gfortran").args(["-O2", "-shared", "-fPIC", "-o", "libnuclear.so", "nuclear.f90"])
        .current_dir(&d).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("cc").args(["-O2", "-shared", "-fPIC", "-o", "libstellar.so", "stellar.c", "-lm"])
        .current_dir(&d).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let want = std::fs::read_to_string(d.join("expected_output.txt")).unwrap();
    let src = std::fs::read_to_string(d.join("c_interop.fm")).unwrap();
    assert_eq!(both(&src, &d), want.trim_end());
    // the Gamow peak of p + p at 15.7 MK is about 6 keV, and the Fortran SEMF agrees with the stdlib's
    assert!(want.contains("Gamow peak E0 = 6.09 keV"), "{want}");
    assert!(want.contains("Fe-56 495.38 MeV 495.38 MeV 492.26 MeV"), "{want}");
    let _ = std::fs::remove_dir_all(&d);
}
