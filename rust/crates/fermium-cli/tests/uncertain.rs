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

/// A fitted parameter's figures don't come from a written value, so a printed sum with one keeps v1's rule (the
/// most precise operand's figures), not the decimal-place rule (B-F1); also through a variable set from it.
/// Expected outputs from `python3 -m fermium run`, except the last: written values still follow the decimal-place
/// rule (spec B2; v1 printed 3.20 m).
#[test]
fn sums_of_fit_parameters_print_like_v1() {
    let src = format!("{HOOKE}print 2 k - k\nprint k + k\na = k\nprint a + a\nprint k + 1.000000 N/m\n\
                       print F₀ + 0.1 N\nprint err(k) + err(k)\nprint 1.20 m + 2.0 m\n");
    assert_eq!(stdout_of("fitsum", &src),
               format!("{HOOKE_REPORT}50.1 N/m\n100 N/m\n100 N/m\n51.11429 N/m\n1.10 N\n1.2 N/m\n3.2 m\n"));
}

/// Where v1's interpreter needs a plain number (Python's int()/float() of a UFloat: an index, a slice, a loop
/// bound, the span of Σ) an uncertain value stops with the plain-number error; a root with an uncertain limit calls
/// the function at an uncertain point (the kernel check). Expected outputs from v1, except the integrals and ODEs
/// with uncertain inputs, which work from Fermium 2.5 (spec C7, D276–D278; v1 stopped with "can't use uncertain
/// values (±) yet"): their outputs are checked against the analytic results in the comments.
#[test]
fn uncertain_values_where_plain_numbers_are_needed() {
    let generic = "this operation needs a plain number, but got an uncertain value (±); write value(x) to drop the \
                   uncertainty, or put the calculation in a  propagate montecarlo  block";
    for (k, body) in ["print xs[i]", "print xs[i:3]", "xs[i] = 7", "for k from 1 to i\n    print k", "print v[i]",
                      "print Σ(k^2 for k from 1 to i)", "print ys[i]", "d = table(a = xs, b = ys)"]
        .iter()
        .enumerate()
    {
        let src = format!("xs = [1, 2, 3]\nys = [1, 2, 3] ± 0.1\nv = <1, 2, 3>\ni = 2.0 ± 0.1\n{body}\n");
        assert_eq!(error_of(&format!("plain{k}"), &src), format!("prog.fm, line 5: {generic}"), "{body}");
    }
    let kernel = |what: &str| format!("prog.fm, line 2: {what} can't use uncertain values (±) yet; put it inside a  \
                                       propagate montecarlo  block, or use value(x)");
    // C7: ∫₀^L x² dx = L³/3 = 0.0417, σ = L² σ_L = 0.00125
    assert_eq!(stdout_of("kint", "L = 0.5 ± 0.005\nprint ∫ x^2 dx from 0 to L\n"), "0.0417 ± 0.0013\n");
    assert_eq!(error_of("kroot", "a = 1.0 ± 0.1\nsolve x^2 = 2 for x from a to 3\nprint x\n"), kernel("solve … for x"));
    // an integrand that doesn't use x: the limits' uncertainty propagates (linear in b - a, as v1's quad)
    assert_eq!(stdout_of("kconst", "L = 0.5 ± 0.005\nprint ∫ 1 dx from 0 to L\nf(x) = if x < 1 then 1 else 2\n\
                                    print ∫ f(x) dx from L to 3\n"),
               "0.5000 ± 0.0050\n4.5000 ± 0.0090\n");
    // an ODE whose end is uncertain (v1: an error): the solution doesn't depend on the end (C7)
    assert_eq!(stdout_of("kode", "T = 1.0 ± 0.1\nsolve y' = -y with y(0) = 1 for t from 0 to T\nprint y(0.5)\n"),
               "0.61\n");
    // a solution at an uncertain time: the interpolant's slope carries t's uncertainty; y' there calls the right side
    let sol = "solve y' = -y with y(0) = 1 for t from 0 to 2\nT = 1.0 ± 0.1\nprint y(T)\nprint y([0.5, T])\n";
    assert_eq!(stdout_of("ksol", sol), "0.368 ± 0.037\n[0.606531, 0.368 ± 0.037]\n");
    assert_eq!(error_of("ksoldy", "solve y' = -y with y(0) = 1 for t from 0 to 2\nT = 1.0 ± 0.1\nprint y'(T)\n"),
               "prog.fm, line 3: a differential equation (solve) can't use uncertain values (±) yet; put the solve \
                inside a  propagate montecarlo  block, or use value(x)");
    // an uncertain integrand over an infinite range (v1: an error): π I = 6.283 ± 0.063 (C7)
    assert_eq!(stdout_of("kinf", "I = 2.0 ± 0.02\nprint ∫ I / (1 + z²) dz from -∞ to ∞\n"), "6.283 ± 0.063\n");
}

/// Complex numbers with uncertain parts (v1's (re, im) tuples of UFloat in cplx.py's kernels): arithmetic, conj
/// and whole powers propagate; printing one, |z|, arg and the math functions need plain numbers. An error from
/// inside a kernel reports the line where the outermost kernel started (interp.kernel). Expected outputs from v1.
#[test]
fn complex_numbers_with_uncertain_parts() {
    let src = "L = 0.5 ± 0.005\nZ = (1 + 1i) * L\nprint im(Z)\nprint im(Z / (2 + 1i)), re(Z / (2 + 1i))\n\
               print re(1 / Z), im((3 - 1i) / Z)\nprint re(Z^2), im(Z^3), re(Z^-2)\nprint Z == Z, Z != Z\n\
               print re(polar(L, 0.3)), im(conj(Z))\n";
    assert_eq!(stdout_of("cunc", src),
               "0.5000 ± 0.0050\n0.1000 ± 0.0010 0.3000 ± 0.0030\n1.000 ± 0.010 -4.000 ± 0.040\n\
                0 ± 0 0.2500 ± 0.0075 0 ± 0\ntrue false\n0.4777 ± 0.0048 -0.5000 ± 0.0050\n");
    let generic = "this operation needs a plain number, but got an uncertain value (±); write value(x) to drop the \
                   uncertainty, or put the calculation in a  propagate montecarlo  block";
    for (k, what) in ["Z", "|Z|", "arg(Z)", "exp(Z)", "√Z"].iter().enumerate() {
        assert_eq!(error_of(&format!("cgen{k}"), &format!("L = 0.5 ± 0.005\nZ = (1 + 1i) * L\nprint {what}\n")),
                   format!("prog.fm, line 3: {generic}"), "{what}");
    }
    // a complex ODE with an uncertain coefficient (v1: an error; C7): ψ = e^(−iEt), so at t = 1 the real part is
    // cos(1) ± sin(1)·0.01 and the imaginary part −sin(1) ± cos(1)·0.01; printing the complex value itself needs
    // plain parts, as above
    let ode = "E = 1.0 ± 0.01\nsolve 1i ψ' = E ψ\n  with ψ(0) = 1\n  for t from 0 to 1\nprint re(ψ(1)), im(ψ(1))\n";
    assert_eq!(stdout_of("code", ode), "0.5403 ± 0.0084 -0.8415 ± 0.0054\n");
    assert_eq!(error_of("code2", &ode.replace("re(ψ(1)), im(ψ(1))", "ψ(1)")), format!("prog.fm, line 5: {generic}"));
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

/// Vectors and matrices with uncertain components (v1's tuples of UFloat): their components, vdot, cross and
/// matrix products work as in v1. v1 stopped when printing one or taking norm/unit; from Fermium 2.5 those work
/// too (spec C7, D279).
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
    // |v| = √5 L; unit(v) doesn't depend on L at all
    let src = format!("{pre}print v\nprint M\nprint u\nprint v + v\nprint M * <1, 1>\nprint norm(v)\nprint unit(v)\n");
    assert_eq!(stdout_of("vecprint", &src),
               "<1.200 ± 0.010, 2.400 ± 0.020> m\n[[1.200 ± 0.010, 2.400 ± 0.020], [3.600 ± 0.030, 4.800 ± 0.040]] m\n\
                <1.200 ± 0.010, 2> m\n<2.400 ± 0.020, 4.800 ± 0.040> m\n<3.600 ± 0.030, 8.400 ± 0.070> m\n\
                2.683 ± 0.022 m\n<0.447 ± 0, 0.894 ± 0>\n");
}

/// std of uncertain values: v1's math.sqrt of a UFloat needed a plain number (sum, mean, min and max worked); from
/// Fermium 2.5 it is the spread with its uncertainty propagated (C7, D279): s = 1, ∂s/∂xᵢ = (xᵢ − x̄)/((n − 1) s),
/// so σ_s = 0.1 √(1/2).
#[test]
fn std_of_uncertain_values() {
    let pre = "x = [1.0, 2.0, 3.0] ± 0.1\n";
    assert_eq!(stdout_of("reduce", &format!("{pre}print mean(x)\nprint sum(x)\nprint max(x)\n")),
               "2.000 ± 0.058\n6.00 ± 0.17\n3.00 ± 0.10\n");
    assert_eq!(stdout_of("std", &format!("{pre}print std(x)\n")), "1.000 ± 0.071\n");
}

/// det, inverse, solve_linear, eigenvalues and eigenvectors of matrices of uncertain values: v1's interpreter
/// runs linalg.py (up to 4×4) or linalg_big.py (larger) on UFloat values, so the uncertainties propagate through
/// the same operations (B-U1). Expected outputs from `python3 -m fermium run`.
#[test]
fn linear_algebra_of_uncertain_matrices() {
    let src = "L = 1.20 ± 0.01 m\nM = [[1, 2], [3, 4]] * L\nprint det(M)\nN = [[2, 1, 0], [1, 3, 1], [0, 1, 4]] * L\n\
               print det(N)\nMi = inverse(M)\nprint Mi[1, 1]\nprint Mi[2, 1]\nx = solve_linear(M, <1 m², 2 m²>)\n\
               print x.y\ny = solve_linear(N, <1, 2, 3> * L)\nprint y[1], y[2], y[3]\n";
    assert_eq!(stdout_of("la2", src),
               "-2.880 ± 0.048 m²\n31.10 ± 0.78 m³\n-1.667 ± 0.014 1/m\n1.250 ± 0.010 1/m\n0.4167 ± 0.0035 m\n\
                0.333 ± 0 0.333 ± 0 0.667 ± 0\n");
    let big = "L = 1.20 ± 0.01\nM = [[2, 1, 0, 0, 0], [1, 3, 1, 0, 0], [0, 1, 4, 1, 0], [0, 0, 1, 5, 1], \
               [0, 0, 0, 1, 6]] * L\nprint det(M)\nMi = inverse(M)\nprint Mi[1, 1]\nprint Mi[5, 4]\n\
               x = solve_linear(M, <1, 2, 3, 4, 5>)\nprint x[1], x[5]\n";
    assert_eq!(stdout_of("la5", big),
               "1224 ± 51\n0.5098 ± 0.0042\n-0.03049 ± 0.00025\n0.2524 ± 0.0021 0.6182 ± 0.0052\n");
    let two = "a = 3.0 ± 0.1\nb = 1.0 ± 0.2\nM = [[3, 0, 0, 1], [0, 3, 1, 0], [0, 1, 3, 0], [1, 0, 0, 3]] * a + \
               [[0, 1, 0, 0], [1, 0, 0, 0], [0, 0, 0, 1], [0, 0, 1, 0]] * b\nprint det(M)\nprint inverse(M)[2, 3]\n\
               N = ([[1, 0], [0, 1]] * a + [[0, 1], [1, 0]] * b) * 1 m\nprint det(N)\nprint inverse(N)[1, 2]\n";
    assert_eq!(stdout_of("la4", two), "(5.00 ± 0.68)×10³\n-0.0438 ± 0.0018\n8.00 ± 0.72 m²\n-0.125 ± 0.033 1/m\n");
    assert_eq!(error_of("lasing", "L = 1.20 ± 0.01\nM = [[1, 2], [2, 4]] * L\nprint det(M)\nprint inverse(M)[1, 1]\n"),
               "prog.fm, line 4: this matrix is singular (its determinant is 0), so it has no inverse and M x = b has \
                no unique solution");
    // eigenvalues: a rotation that meets an uncertain entry takes math.sqrt of a UFloat (v1 needs a plain number),
    // except where no rotation is needed (equal diagonal entries)
    let eig = "a = 3.0 ± 0.1\nI5 = [[1, 0, 0, 0, 0], [0, 1, 0, 0, 0], [0, 0, 1, 0, 0], [0, 0, 0, 1, 0], \
               [0, 0, 0, 0, 1]]\nN = [[1, 0, 0], [0, 1, 0], [0, 0, 1]] * a\n\
               print eigenvalues(N)[2], eigenvalues(N, [[2, 0, 0], [0, 2, 0], [0, 0, 2]])[3]\n\
               print eigenvalues(I5 * a, I5 * 2)[5]\nprint eigenvectors(I5 * a, I5 * 2)[2, 2]\n";
    assert_eq!(stdout_of("laeig", eig), "3.00 ± 0.10 1.500 ± 0.050\n1.500 ± 0.050\n1\n");
    let generic = "this operation needs a plain number, but got an uncertain value (±); write value(x) to drop the \
                   uncertainty, or put the calculation in a  propagate montecarlo  block";
    assert_eq!(error_of("laeig2", "L = 1.20 ± 0.01\nM = [[2, 1], [1, 3]] * L\nprint eigenvalues(M)\n"),
               format!("prog.fm, line 3: {generic}"));
    assert_eq!(error_of("laeig3", "a = 3.0 ± 0.1\nprint eigenvalues([[1, 0], [0, 1]] * 2, [[1, 0], [0, 1]] * -a)\n"),
               "prog.fm, line 2: in eigenvalues(K, M) the second matrix M must be positive definite, like a mass \
                matrix (positive masses on the diagonal)");
    let asym = "L = 1.20 ± 0.01\nM = [[2, 1, 0, 0, 0], [1, 3, 1, 0, 0], [0, 1, 4, 1, 0], [0, 0, 1, 5, 1], \
                [0, 0, 0, 2, 6]] * L\nprint eigenvalues(M)\n";
    assert!(error_of("laeig4", asym).starts_with("prog.fm, line 3: eigenvalues and eigenvectors need a symmetric matrix"));
}
