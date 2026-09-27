//! The built-in function names (the top of `fermium/checker.py`). The checking of each built-in call
//! (Checker.builtin) is in `calls`.
pub const MATH1: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "exp", "ln", "log", "log10", "log2", "erf", "erfc", "gamma", "lgamma", "expm1", "log1p",
];
pub const SAME1: &[&str] = &["abs", "floor", "ceil", "round"];
/// (n, x): whole-number order n (#50)
pub const SPECIAL2: &[&str] = &["besselj", "bessely", "besseli", "besselk"];
/// (m): parameter m = k²
pub const SPECIAL1: &[&str] = &["ellipk", "ellipe"];
pub const LIST_FUNCS: &[&str] = &["len", "sum", "mean", "std", "first", "last", "cumsum", "diff", "reverse", "sort"];
/// parts of an uncertain value (D121)
pub const UNC_FUNCS: &[&str] = &["value", "uncertainty", "rel"];
/// seeded random numbers (D80), FFT (D81)
pub const M3_FUNCS: &[&str] = &[
    "randn", "seed", "sample", "fft", "fft_re", "fft_im", "ifft", "amplitude_spectrum", "power_spectrum",
    "frequencies", "argmax", "argmin",
];
/// re, im, conj, arg, complex, polar, cis (D90)
pub const COMPLEX_FUNCS: &[&str] = &["re", "im", "conj", "arg", "complex", "polar", "cis"];
/// built-ins that also take a complex argument
pub const COMPLEX_MATH: &[&str] = &["exp", "ln", "log", "sqrt", "sin", "cos", "tan", "sinh", "cosh", "tanh"];
const OTHER: &[&str] = &[
    "sqrt", "cbrt", "min", "max", "atan2", "hypot", "sign", "mod", "linspace", "zeros", "ones", "range", "push",
    "append", "to", "values", "times", "dot", "factorial", "clamp", "isnan", "rand", "interp", "trapz", "clock", "norm",
    "unit", "hat", "cross", "vec", "transpose", "det", "inverse", "identity", "solve_linear", "eigenvalues",
    "eigenvectors", "trace", "angle", "row", "column", "str",
];

/// Every built-in function name (Python BUILTINS), without duplicates.
pub static BUILTINS: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
    let mut v: Vec<&str> = vec![];
    for group in [MATH1, SAME1, LIST_FUNCS, OTHER, SPECIAL2, SPECIAL1, UNC_FUNCS, M3_FUNCS, COMPLEX_FUNCS] {
        for n in group {
            if !v.contains(n) {
                v.push(n);
            }
        }
    }
    v
});

pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}
