//! `fit`'s standard errors for parameters whose SI value is tiny (found by the C8 research track, level_density):
//! the covariance Jacobian takes a step relative to each parameter (fermium-runtime numerics/fit.rs), so
//! `k = 8 MeV` (1.3×10⁻¹² J) gets the same standard error as the same fit written with a plain number.
use std::process::Command;

fn run(src: &str, backend: &str) -> String {
    let d = std::env::temp_dir().join(format!("fermium-fit-errors-{}-{backend}", std::process::id()));
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
