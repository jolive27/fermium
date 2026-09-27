//! `fit`'s standard errors for parameters whose SI value is tiny (found by the C8 research track, level_density):
//! the covariance Jacobian takes a step relative to each parameter (fermium-runtime numerics/fit.rs), so
//! `k = 8 MeV` (1.3×10⁻¹² J) gets the same standard error as the same fit written with a plain number.
use std::process::Command;

fn run(src: &str, backend: &str) -> String {
    // one folder per program and back end: the tests run in parallel
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    let d = std::env::temp_dir().join(format!("fermium-fit-errors-{}-{backend}-{:x}", std::process::id(), h.finish()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("p.fm"), src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium"))
        .args(["run", "p.fm"])
        .current_dir(&d)
        .env("FERMIUM_BACKEND", backend)
        .env("FERMIUM_NO_CACHE", "1")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn a_tiny_si_parameter_gets_the_same_standard_error_as_the_plain_number_fit() {
    let src = "xs = [10, 20, 30, 40, 50]\nys = [1.3, 2.4, 3.9, 5.1, 6.2] [1/MeV]\nk = 8 MeV\n\
               fit y = x / k to table(x = xs, y = ys)\nprint k in MeV, err(k) in MeV\n\
               q = 8\nfit y * 1 MeV = x / q to table(x = xs, y = ys)\nprint q, err(q)\n";
    for backend in ["interp", "llvm"] {
        let out = run(src, backend);
        assert!(out.contains("\n7.95 MeV 0.089 MeV\n"), "{backend}: {out}");
        assert!(out.contains("\n7.948 0.089\n") || out.contains("\n7.95 0.089\n"), "{backend}: {out}");
        assert!(out.contains("(standard error 1.4×10⁻¹⁴ J)"), "{backend}: {out}");
    }
}

#[test]
fn a_parameter_that_ends_near_zero_keeps_v1s_standard_errors() {
    // red team 16 #3: with a purely relative covariance step, b = 2×10⁻¹⁶ got NaN errors and a false warning
    let src = "xs = [1,2,3,4,5]\nys = [1.9, 4.2, 6, 7.8, 10.1]\na = 1\nb = 1\n\
               fit y = a x + b to table(x = xs, y = ys)\nprint err(a), err(b)\n";
    for backend in ["interp", "llvm"] {
        let out = run(src, backend);
        assert!(out.contains("\n0.058 0.19\n"), "{backend}: {out}");
        assert!(!out.contains("could not be estimated"), "{backend}: {out}");
    }
}

#[test]
fn tiny_si_parameters_reach_the_least_squares_optimum() {
    // red team 16 #4: a ns decay (SciPy: τ = 2.964 ± 0.042 ns) and a fm Gaussian (x0 ≈ 0, w = 1.1993 fm,
    // rms 0.0178) stopped short of the optimum when the steps were √ε·max(|p|, 1) in SI
    let decay = "ts = [0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5, 5.5, 6, 6.5, 7, 7.5, 8, 8.5, 9, 9.5, 10] ns\n\
                 ys = [4.03882, 3.33281, 2.87249, 2.41277, 2.04262, 1.73208, 1.42912, 1.23897, 1.03508, 0.95698, \
                 0.75802, 0.63047, 0.53372, 0.44287, 0.36478, 0.31852, 0.28557, 0.22849, 0.2163, 0.16258, 0.14118]\n\
                 A = 3\nτ = 2 ns\nb = -0.001\nfit y = A exp(-t/τ) + b to table(t = ts, y = ys)\nprint τ, err(τ)\n";
    let gauss = "xs = [-3, -2.5, -2, -1.5, -1, -0.5, 0, 0.5, 1, 1.5, 2, 2.5, 3] fm\n\
                 ys = [0.182862, 0.598144, 1.248182, 2.303873, 3.551470, 4.613333, 5.030000, 4.613333, 3.551470, \
                 2.303873, 1.248182, 0.598144, 0.182862]\n\
                 A = 4\nx0 = 0.1 fm\nw = 1 fm\nfit y = A exp(-(x - x0)²/(2 w²)) to table(x = xs, y = ys)\n\
                 print w, err(w)\n";
    for backend in ["interp", "llvm"] {
        let out = run(decay, backend);
        assert!(out.contains("τ = 2.964 ns   (standard error 0.042 ns)"), "{backend}: {out}");
        let out = run(gauss, backend);
        assert!(out.contains("w = 1.1993 fm   (standard error 0.0033 fm)"), "{backend}: {out}");
        assert!(out.contains("rms residual = 0.0178"), "{backend}: {out}");
    }
}
