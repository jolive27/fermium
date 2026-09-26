//! Uncertain values (±, D120–D124) end to end: the fermium binary runs each program in the tree-walker (a program
//! that uses ± never goes to LLVM) and must print what Fermium 1.5 prints. Every expected output below was produced
//! by `python3 -m fermium run` of Fermium 1.5.
use std::process::Command;

struct Out {
    stdout: String,
    stderr: String,
    ok: bool,
}

fn run(tag: &str, src: &str) -> Out {
    let dir = std::env::temp_dir().join(format!("fermium-uncertain-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.fm");
    std::fs::write(&file, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).arg("run").arg(&file).output().unwrap();
    Out { stdout: String::from_utf8_lossy(&o.stdout).into_owned(), stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
          ok: o.status.success() }
}

fn stdout_of(tag: &str, src: &str) -> String {
    let o = run(tag, src);
    assert!(o.ok, "{src}\n{}", o.stderr);
    o.stdout
}

fn error_of(tag: &str, src: &str) -> String {
    let o = run(tag, src);
    assert!(!o.ok, "{src}\n{}", o.stdout);
    o.stderr.lines().next().unwrap_or("").to_string()
}

const HOOKE: &str = "x = [0.01, 0.02, 0.03, 0.04, 0.05, 0.06] m\n\
                     F = [1.49, 2.02, 2.49, 3.03, 3.47, 4.02] N\n\
                     data = table(x = x, F = F)\n\
                     fit F = k x + F₀ to data\n";

const HOOKE_REPORT: &str = "fit F = k x + F_0   (6 data points from table(x = x, F = F))\n  \
                            k = 50.11 N/m   (standard error 0.62 N/m)\n  \
                            F_0 = 0.9993 N   (standard error 0.024 N)\n  \
                            rms residual = 0.0213 N\n";

/// In a program that uses ±, the fitted parameters carry their standard errors *and* their correlations (the
/// covariance of the fit, uncertain.correlated): the uncertainty of k·x + F₀ is far smaller than the two added
/// independently (√(0.0218² + 0.0242²) = 0.0326 N), because k and F₀ are anticorrelated.
#[test]
fn fit_parameters_are_correlated_uncertain_values() {
    let src = format!("{HOOKE}print k\nprint F₀\nF_mid = k (0.035 m) + F₀\nprint F_mid\n\
                       print uncertainty(F_mid) to 6 digits\nprint uncertainty(k (0.035 m)) to 6 digits\n\
                       print uncertainty(F₀) to 6 digits\nprint k - k\nprint value(k) to 10 digits\n\
                       print uncertainty(k) to 10 digits\nprint err(k) to 6 digits\nprint rel(F₀)\n\
                       E = ½ k (0.02 m)²\nprint E\n");
    let want = format!("{HOOKE_REPORT}50.11 ± 0.62 N/m\n0.999 ± 0.024 N\n2.753 ± 0.011 N\n0.0106272 N\n0.0217792 N\n\
                        0.0242337 N\n0 ± 0 N/m\n50.11428571 N/m\n0.6222627133 N/m\n0.622263 N/m\n0.024\n\
                        0.01002 ± 0.00012 J\n");
    assert_eq!(stdout_of("fitcorr", &src), want);
}

/// Without ± anywhere in the program the parameters stay plain numbers (v1 only makes them uncertain when the
/// module uses uncertainties), and a later fit can start from them.
#[test]
fn fit_parameters_are_plain_without_uncertainties() {
    let src = format!("{HOOKE}print k - k\nfit F = k₂ x + F₁ to data with k₂ = k, F₁ = F₀\n");
    let o = stdout_of("fitplain", &src);
    assert!(o.starts_with(&format!("{HOOKE_REPORT}0 N/m\n")), "{o}");
}

#[test]
fn fit_model_cannot_use_other_uncertain_values() {
    let src = "x = [0.01, 0.02, 0.03, 0.04, 0.05, 0.06] m\nF = [1.49, 2.02, 2.49, 3.03, 3.47, 4.02] N\n\
               data = table(x = x, F = F)\nc = 1.0 ± 0.1 N\nfit F = k x + c to data\nprint k\n";
    assert_eq!(error_of("fitmodel", src),
               "prog.fm, line 5: a fit model can't use uncertain values (±) other than the parameters being fitted; \
                write value(x) in the model");
}

/// A program that uses ± prints sums with the operands' significant figures, not the decimal-place rule: v1 runs
/// it in its interpreter, whose print_num has no such rule.
#[test]
fn sums_in_a_program_with_uncertainties_print_like_v1() {
    let src = "a = 1.23 m\nb = 4.5 m\nc = 12.345 m\nprint a + b\nprint c - a\nprint a + b - c\n\
               L = 1.0 ± 0.1 m\nprint L + a\nprint a + L - a\n";
    assert_eq!(stdout_of("sums", src), "5.73 m\n11.115 m\n-6.6150 m\n2.23 ± 0.10 m\n1.00 ± 0.10 m\n");
}

/// Vectors and matrices with uncertain components exist (v1's tuples of UFloat): their components, vdot, cross
/// and matrix products work; printing one stops with v1's message, on the line of the print.
#[test]
fn vectors_of_uncertain_values() {
    let pre = "L = 1.20 ± 0.01 m\nv = <1, 2> * L\nM = [[1, 2], [3, 4]] * L\nw = <1, 2, 3> * L\nu = <1 m, 2 m>\n\
               u[1] = L\n";
    let src = format!("{pre}print v.x\nprint v[2] + v.x\nprint (-v).y\nprint (v - v).x\nprint (v / L).x\n\
                       print dot(v, v)\nprint cross(v, v)\nprint cross(w, <1, 0, 0>).y\nprint M[1, 2]\n\
                       print (M * <1, 1>).y\nprint (M * M)[2, 2]\nprint transpose(M)[1, 2]\nprint u[1]\n");
    assert_eq!(stdout_of("vec", &src),
               "1.200 ± 0.010 m\n3.600 ± 0.030 m\n-2.400 ± 0.020 m\n0 ± 0 m\n1 ± 0\n7.20 ± 0.12 m²\n0 ± 0 m²\n\
                3.600 ± 0.030 m\n2.400 ± 0.020 m\n8.400 ± 0.070 m\n31.68 ± 0.53 m²\n3.600 ± 0.030 m\n\
                1.200 ± 0.010 m\n");
    let vec_msg = "vectors and matrices of uncertain values (±) aren't supported yet; work with the uncertain numbers \
                   one at a time, or use value(x) to drop the uncertainty";
    for (k, what) in ["v", "M", "u", "v + v", "M * <1, 1>"].iter().enumerate() {
        assert_eq!(error_of(&format!("vecerr{k}"), &format!("{pre}print {what}\n")),
                   format!("prog.fm, line 7: {vec_msg}"), "{what}");
    }
    let generic = "this operation needs a plain number, but got an uncertain value (±); write value(x) to drop the \
                   uncertainty, or put the calculation in a  propagate montecarlo  block";
    for (k, what) in ["norm(v)", "unit(v)"].iter().enumerate() {
        assert_eq!(error_of(&format!("vecgen{k}"), &format!("{pre}print {what}\n")),
                   format!("prog.fm, line 7: {generic}"), "{what}");
    }
}

/// std of uncertain values: v1's math.sqrt of a UFloat needs a plain number (sum, mean, min and max work).
#[test]
fn std_of_uncertain_values_needs_plain_numbers() {
    let pre = "x = [1.0, 2.0, 3.0] ± 0.1\n";
    assert_eq!(stdout_of("reduce", &format!("{pre}print mean(x)\nprint sum(x)\nprint max(x)\n")),
               "2.000 ± 0.058\n6.00 ± 0.17\n3.00 ± 0.10\n");
    assert_eq!(error_of("std", &format!("{pre}print std(x)\n")),
               "prog.fm, line 2: this operation needs a plain number, but got an uncertain value (±); write value(x) \
                to drop the uncertainty, or put the calculation in a  propagate montecarlo  block");
}
