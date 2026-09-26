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

#[test]
fn tidy_like_sympy() {
    let f = body("q / (4π ε_0 √(x² + y² + z²))");
    let gx = diff(&f, "x", &mut Plain).unwrap();
    assert_eq!(to_source(&tidy(&gx)), "-q x/(4π ε_0 (x² + y² + z²)^(3/2))");
    let v = body("λ / (4π ε_0 √((x - s)^2 + y^2 + z^2))");
    let dv = simplify(&d(&v, "x", &mut Plain).unwrap());
    assert_eq!(to_source(&tidy(&dv)), "λ·(s - x)/(4π ε_0 (y² + z² + (s - x)²)^(3/2))");
    let r = body("1/√(x² + y² + z²)");
    let mut ts = vec![];
    for p in ["x", "y", "z"] {
        let d1 = diff(&r, p, &mut Plain).unwrap();
        ts.push(diff(&d1, p, &mut Plain).unwrap());
    }
    let lap = simplify(&crate::build::add(crate::build::add(ts[0].clone(), ts[1].clone()), ts[2].clone()));
    assert_eq!(to_source(&tidy(&lap)), "0", "{}", to_source(&lap));
}

#[test]
fn tidy_nested_cancellation() {
    let e = body("A y z·(√(x² + y² + z²)·(1/(x² + y² + z²) + a·(a + 1/√(x² + y² + z²))) + √(x² + y² + z²)·(-1/(x² + y² + z²) + a·(-a - 1/√(x² + y² + z²))))·exp(-a √(x² + y² + z²))/(x² + y² + z²)²");
    assert_eq!(to_source(&tidy(&e)), "0");
    let e = body("G M m·(2 √(x² + y²) - 3x²/√(x² + y²) - 3y²/√(x² + y²))/(x² + y²)²");
    assert_eq!(to_source(&tidy(&e)), "-G M m/(x² + y²)^(3/2)");
    let f = body("-G M m / √(x² + y²)");
    let mut ts = vec![];
    for p in ["x", "y", "z"] {
        let d1 = diff(&f, p, &mut Plain).unwrap();
        ts.push(diff(&d1, p, &mut Plain).unwrap());
    }
    let lap = simplify(&crate::build::add(crate::build::add(ts[0].clone(), ts[1].clone()), ts[2].clone()));
    assert_eq!(to_source(&tidy(&lap)), "-G M m/(x² + y²)^(3/2)", "{:?}", lap);
}

fn integ(s: &str, var: &str, positive: &[&str]) -> Result<String, String> {
    let p: Vec<String> = positive.iter().map(|x| x.to_string()).collect();
    integrate(&body(s), var, &p).map(|e| to_source(&e)).map_err(|d| d.message)
}

#[test]
fn antiderivatives_print_like_v1() {
    // the forms v1 (SymPy) printed, from the conformance goldens
    assert_eq!(integ("x^2", "x", &[]).unwrap(), "x³/3");
    assert_eq!(integ("x", "x", &[]).unwrap(), "x²/2");
    assert_eq!(integ("1 / √(a^2 + s^2)", "s", &["a"]).unwrap(), "asinh(s/a)");
    assert_eq!(integ("|x|", "x", &[]).unwrap(), "if x <= 0 then -x²/2 else x²/2");
    assert_eq!(integ("exp(1i x)", "x", &[]).unwrap(), "-𝑖 exp(𝑖 x)");
}

#[test]
fn antiderivatives_are_right() {
    // each is checked numerically inside integrate(); here: that a formula is found at all
    for (s, v) in [("exp(-k x)", "x"), ("1/√((x - s)^2 + (0.5 m)^2)", "s"), ("1 / √(a^2 + s^2)", "s"),
                   ("csc(x)^2", "x"), ("exp(-x) sin(x)", "x"), ("k x", "x"), ("x^a", "x"), ("cos(ω t)", "t"),
                   ("x^2 exp(x)", "x"), ("x ln(x)", "x"), ("x exp(-x^2)", "x"), ("1/(x^2 - 1)", "x"),
                   ("(3x + 1)/(x^2 + 2 x + 5)", "x"), ("sin(x) cos(x)", "x"), ("1/(1 + x^2)", "x"),
                   ("exp(-a x^2)", "x"), ("tan(x)", "x"), ("x (x + 1)^2", "x"), ("cos(x)^3", "x"),
                   ("sin(2x)^5", "x"), ("ln(x)^2", "x"), ("ln(3 x)^3", "x"), ("sin(2x) sin(3x)", "x"),
                   ("cos(x) cos(4x)", "x"), ("1/(1 + cos(x))", "x"), ("1/(1 - cos(2x))", "x")] {
        assert!(integ(s, v, &["a"]).is_ok(), "{s}: {:?}", integ(s, v, &["a"]));
    }
}

#[test]
fn non_elementary_integrals_are_refused() {
    let e = integ("exp(s) / s", "s", &[]).unwrap_err();
    assert_eq!(e, "SymPy's formula for this integral uses the function Ei, which Fermium doesn't have yet: Ei(s)");
    assert!(integ("sin(x)/x", "x", &[]).unwrap_err().contains("Si"));
    assert_eq!(integ("exp(sin(x))", "x", &[]).unwrap_err(), "Fermium couldn't find a formula for this integral");
}
