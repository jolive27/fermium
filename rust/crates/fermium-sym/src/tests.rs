//! Expected strings come from Fermium 1.5 (python3 -c "from fermium import calculus as C; …").
use fermium_syntax::ast as A;

use crate::*;

pub(crate) fn body(s: &str) -> A::Expr {
    let (p, _) = fermium_syntax::parse(&format!("f(x) = {s}"), &[]).expect("parses");
    match &p.body[0].kind {
        A::StmtKind::FuncDef { body: A::FuncBody::Expr(b), .. } => b.clone(),
        _ => panic!("not a one-line function"),
    }
}

fn dx(s: &str) -> A::Expr {
    diff(&body(s), "x", &mut Plain).unwrap()
}

#[test]
fn derivatives_print_like_v1() {
    let cases = [
        ("exp(sin(x^2))", "2x exp(sin(x²)) cos(x²)", "2 x exp(sin(x^2)) cos(x^2)"),
        ("x^3 + 2 x", "3x² + 2", "3 x^2 + 2"),
        ("sin(ω x) A", "ω A cos(ω x)", "omega A cos(omega x)"),
        ("1/(1 + x^2)", "-2x/(1 + x²)²", "-2 x/(1 + x^2)^2"),
        ("exp(x)/(exp(x) + 1)^2", "(exp(x) (exp(x) + 1)² - 2 exp(x)²·(exp(x) + 1))/(exp(x) + 1)⁴",
         "(exp(x) (exp(x) + 1)^2 - 2 exp(x)^2*(exp(x) + 1))/(exp(x) + 1)^4"),
        ("x^2 exp(-x^2)", "(2x - 2x³)·exp(-x²)", "(2 x - 2 x^3)*exp(-x^2)"),
        ("√(1 + x^2)", "x/√(1 + x²)", "x/sqrt(1 + x^2)"),
        ("ln(x)/x", "(1 - ln(x))/x²", "(1 - ln(x))/x^2"),
        ("tanh(x)", "1/cosh(x)²", "1/cosh(x)^2"),
        ("3 m/s * x^2", "6 m/s x", "6 m/s x"),
        ("x^a", "a x^(a - 1)", "a x^(a - 1)"),
    ];
    for (src, pretty, ascii) in cases {
        let d = dx(src);
        assert_eq!(to_source(&d), pretty, "{src}");
        assert_eq!(key(&d), ascii, "{src}");
    }
    let d2 = diff(&dx("exp(sin(x^2))"), "x", &mut Plain).unwrap();
    assert_eq!(to_source(&d2), "((2 + 4x² cos(x²))·cos(x²) - 4x² sin(x²))·exp(sin(x²))");
}

#[test]
fn stabilize_like_v1() {
    let d = dx("exp(x)/(exp(x) + 1)^2");
    assert_eq!(to_source(&stabilize(&d)),
               "1/(1 + 1 exp(-x))/(exp(x) + 1) + -2 (1/(1 + 1 exp(-x)))²/(exp(x) + 1)");
    assert_eq!(to_source(&stabilize(&body("sinh(x)/cosh(x)^3"))), "tanh(x)/cosh(x)²");
}

#[test]
fn isolate_and_linear_coeffs() {
    let r = isolate(&body("m*a + c*v"), &body("-k*x"), &body("a")).unwrap();
    assert_eq!(to_source(&r), "-(c v + k x)/m");
    let (cs, _) = linear_coeffs(&body("m1 a1 + m2 a2 + 3"), &body("g a1"), &[body("a1"), body("a2")]).unwrap();
    assert_eq!(cs.iter().map(to_source).collect::<Vec<_>>(), vec!["m1 - g", "m2"]);
    assert!(isolate(&body("a^2"), &body("x"), &body("a")).is_err());
}

#[test]
fn factor_common_and_source() {
    assert_eq!(to_source(&factor_common(&body("2 exp(u) - 4 x^2 exp(u)"))), "(2 - 4x²)·exp(u)");
    assert_eq!(to_source(&body("x^(2/3) + 0.5 x + 1e20 y + 4i + 2 g")), "x^(2/3) + 0.5x + 1×10²⁰y + 4i + 2 g");
}
