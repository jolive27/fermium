//! The parser's share of the A1 unit rule (tests/test_a1_unit_rule.py, D235) and the A2 fraction coefficients
//! (tests/test_a2_fraction_coefficients.py, D236), plus other tricky rules. Messages are Fermium 1.5's.

use fermium_syntax::ast::{ExprKind, StmtKind};
use fermium_syntax::{parse, parse_fix, sexpr, Diagnostic};

fn err(src: &str) -> Diagnostic {
    match parse(src, &[]) {
        Ok((p, _)) => panic!("expected an error for {src:?}, got\n{}", sexpr::program(&p)),
        Err(e) => e,
    }
}

fn tree(src: &str) -> String {
    match parse(src, &[]) {
        Ok((p, _)) => sexpr::program(&p),
        Err(e) => panic!("{src:?} failed: {}", e.format(Some(src), None)),
    }
}

fn warnings(src: &str) -> Vec<String> {
    parse(src, &[]).expect("parses").1.warnings.iter().map(|w| w.message.clone()).collect()
}

fn hint(e: &Diagnostic) -> &str {
    e.hint.as_deref().unwrap_or("")
}

// ---- A1: the unit-name rule ------------------------------------------------------------------------------------

#[test]
fn first_name_of_a_compound_is_always_a_unit() {
    let t = tree("m = 2 kg\nv = 3 m/s\nprint v");
    assert!(t.contains(r#"(UnitFactor@2:7+1 name="m" exp=1)"#), "{t}");
    assert!(t.contains(r#"name="s" exp=-1"#));
}

#[test]
fn no_number_before_m_means_your_mass() {
    let t = tree("m = 2 kg\nv = 3 m/s\nE = ½ m v²\nprint E");
    assert!(t.contains(r#"(Name@3:7+1 name="m""#), "{t}");
}

#[test]
fn per_your_g_is_an_error_whatever_the_spacing() {
    for line in ["print 20 m/s/g", "print 20 m/s / g", "print 20 m/s /g", "print 20 m/s/ g"] {
        let e = err(&format!("g = 9.81 m/s²\n{line}"));
        assert!(e.message.contains("ambiguous") && e.message.contains('g'), "{line}: {}", e.message);
        assert!(hint(&e).contains("(20 m/s)/g") && hint(&e).contains("20 [m/s/g]"), "{line}: {}", hint(&e));
    }
}

#[test]
fn two_g_h_shows_both_fixes() {
    let e = err("g = 9.81 m/s²\nh = 2 m\nprint 2 g h");
    assert!(e.message.contains("'2 g' is ambiguous"));
    assert!(hint(&e).contains("2*g") && hint(&e).contains("2 [g]"));
    assert_eq!(e.message, "'2 g' is ambiguous: right after a number, g is a unit (grams), but g is also your variable g");
    assert_eq!((e.line, e.col, e.length), (Some(3), Some(9), 1));
}

#[test]
fn initial_condition_next_to_a_mass_is_an_error() {
    let src = "k = 50 N/m\nm = 0.5 kg\nsolve x'' = -(k/m) x with x(0) = 0.1 m, x'(0) = 0 m/s for t from 0 s to 1 s\n";
    let e = err(src);
    assert!(e.message.contains("'0.1 m' is ambiguous") && hint(&e).contains("0.1 [m]"));
}

#[test]
fn spring_constant_next_to_a_mass() {
    let e = err("m = 0.5 kg\nk = 50 N/m\nprint k");
    assert!(e.message.contains("'50 N/m' is ambiguous") && hint(&e).contains("50 [N/m]"));
}

#[test]
fn later_name_your_variable() {
    let e = err("m = 2 kg\nprint 2 kg m");
    assert!(e.message.contains("'2 kg m' is ambiguous"));
    assert!(hint(&e).contains("(2 kg) m") && hint(&e).contains("2 [kg m]"));
}

#[test]
fn friction_74_collisions() {
    for (defs, expr) in [("Ω = 3 rad/s\nt = 2 s", "2 Ω t"), ("K = 3", "8 K"), ("b = 2 m", "2 b"),
                         ("T = 2 s", "0.25 T"), ("l = 2 m", "2 l²"), ("T = 2.21 s", "2 T"), ("c = 340 m/s", "2 c")] {
        let e = err(&format!("{defs}\nprint {expr}"));
        let num = expr.split(' ').next().unwrap();
        assert!(e.message.contains("ambiguous"), "{expr}: {}", e.message);
        assert!(hint(&e).contains(&format!("{num}*")) && hint(&e).contains(&format!("{num} [")), "{}", hint(&e));
    }
}

#[test]
fn spacing_never_changes_meaning() {
    let base = tree("print 50 N/m");
    for src in ["print 50 N / m", "print 50 N /m", "print 50 N/ m"] {
        let t = tree(src);
        assert!(t.contains(r#"name="m" exp=-1"#), "{src}: {t}");
    }
    assert!(base.contains(r#"name="m" exp=-1"#));
    for src in ["print 3 J/(kg K)", "print 3 J / (kg K)"] {
        let t = tree(src);
        assert!(t.contains(r#"name="K" exp=-1"#), "{src}: {t}");
    }
}

#[test]
fn bracket_denominator_holding_your_variable_is_an_error() {
    for sp in ["", " "] {
        let e = err(&format!("m = 2 kg\nprint 9.81 kg{sp}/{sp}(m s²)"));
        assert!(e.message.contains("ambiguous") && e.message.contains('m'), "{}", e.message);
    }
}

#[test]
fn per_h_is_an_error_whatever_the_spacing() {
    for src in ["print 36 km/h", "print 36 km / h"] {
        let e = err(src);
        assert!(e.message.contains("Planck's constant, not the hour") && hint(&e).contains("km/hr"));
    }
    tree("print (2 eV)/h in THz");
}

#[test]
fn reciprocal_unit_after_a_number() {
    for src in ["print 0.5 /s", "print 0.5/s", "print 0.5 / s"] {
        let t = tree(src);
        assert!(t.contains(r#"text="1/s""#) || t.contains(r#"text="1/ s""#) || t.contains(r#"text="1/s"#), "{src}: {t}");
        assert!(t.contains("(Quantity@"), "{src}");
    }
    // your variable is divided by
    let t = tree("m = 2 kg\nprint 8 /m");
    assert!(t.contains(r#"op="/""#) && !t.contains("Quantity@2"), "{t}");
}

#[test]
fn after_a_list_a_unit_follows_as_after_a_number() {
    let e = err("m = 3\nprint [1, 2] m");
    assert!(e.message.contains("ambiguous") && hint(&e).contains("[1, 2]*m") && hint(&e).contains("[1, 2] [m]"));
    tree("m = 3\nprint [1, 2]*m");
    assert!(tree("print [1, 2] m").contains("(Quantity@1:7"));
}

#[test]
fn after_a_bracket_a_unit_needs_brackets() {
    let e = err("N = 2\nprint (N + 3) MeV");
    assert!(e.message.contains("after a bracket is read as a variable"));
    tree("N = 2\nprint (N + 3) [MeV]");
    assert!(tree("print (2 + 3) MeV").contains("#times_unit=True"));
    let e = err("m1 = 1 kg\nm2 = 2 kg\nF = (m1 + m2) g");
    assert!(hint(&e).contains("g_n"));
}

#[test]
fn where_names_are_your_variables() {
    let e = err("x = 0.1 m where m = 2 kg\nprint x");
    assert!(e.message.contains("ambiguous") && e.message.contains("where"));
    tree("ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg\nprint ω₀ in rad/s");
}

#[test]
fn integration_and_derivative_variables() {
    let e = err("print ∫ 3 s^2 ds from 0 to 1");
    assert!(e.message.contains("'3 s^2' is ambiguous"), "{}", e.message);
    let t = tree("print ∫ 2 [m] dm from 0 to 1");
    assert!(t.contains(r#"var="m""#), "{t}");
    let e = err("h(s) = s^2\nprint d/ds h(2 s)");
    assert!(e.message.contains("ambiguous") && e.message.contains("differentiate"));
}

#[test]
fn solve_unknown() {
    let e = err("w = 1/s²\nsolve u'' = -2 u w with u(0) = 2 [u], u'(0) = 0 u/s for t from 0 s to 1 s\nprint u(1 s)");
    assert!(e.message.contains("ambiguous"), "{}", e.message);
}

#[test]
fn errors_attach_a_bracket_fix() {
    let n = |s: &str| s.chars().count();
    let e = err("g = 9.81 m/s²\nprint 20 m/s/g");
    assert_eq!(e.fix, vec![(n("g = 9.81 m/s²\nprint 20 "), n("g = 9.81 m/s²\nprint 20 m/s/g"), "[m/s/g]".to_string())]);
    let e = err("g = 9.81 m/s²\nprint 20 m/s / g");
    assert_eq!(e.fix, vec![(n("g = 9.81 m/s²\nprint 20 "), n("g = 9.81 m/s²\nprint 20 m/s"), "[m/s]".to_string())]);
    let e = err("m = 2 kg\nx = 0.1 m²");
    assert_eq!(e.fix, vec![(n("m = 2 kg\nx = 0.1 "), n("m = 2 kg\nx = 0.1 m²"), "[m²]".to_string())]);
}

#[test]
fn fix_mode_rewrites_collisions() {
    let src = "m = 0.5 kg\nk = 50 N/m\nsolve x'' = -(k/m) x with x(0) = 0.1 m, x'(0) = 0 m/s for t from 0 s to 1 s\n";
    let (fixes, e) = parse_fix(src);
    assert!(e.is_none(), "{e:?}");
    let reps: Vec<&str> = fixes.iter().map(|f| f.2.as_str()).collect();
    assert!(reps.contains(&"[N/m]") && reps.contains(&"[m]"), "{reps:?}");
}

// ---- A2: fraction coefficients ---------------------------------------------------------------------------------

#[test]
fn fraction_coefficients() {
    for src in ["x = 3\nprint 73/24 x²", "t = 2\nprint π²/12 t² to 4 digits", "t = 2\nprint π⁴/80 t⁴ to 4 digits",
                "print 1/2 kg", "x = 4\nprint 1/2 x", "x = 3\nprint 2/3 x^2",
                "k = 50 N/m\nm = 0.5 kg\nprint 1/(2π) √(k/m)"] {
        let t = tree(src);
        assert!(t.contains("#coefficient=True"), "{src}: {t}");
    }
    for src in ["print h / m_e 3 m/s", "h_ = 2 m\nprint 2 m / h_ 4", "print 0.04 / 1 s", "print 1e4 / 1 s"] {
        assert!(!tree(src).contains("#coefficient"), "{src}");
    }
}

#[test]
fn no_precedence_warning_for_a_coefficient() {
    assert!(warnings("x = 3\ny = 73/24 x²\nz = 1/2 x").is_empty());
}

#[test]
fn one_half_m_v_squared_with_a_mass_asks_and_suggests_one_half() {
    let e = err("m = 2 kg\nv = 3 m/s\nprint 1/2 m v²");
    assert!(e.message.contains("'2 m' is ambiguous") && hint(&e).contains("½ m"), "{:?}", e);
}

#[test]
fn a_pure_number_after_the_denominator_asks() {
    for (src, shown) in [("r = 2 m\nprint 4/3 π r³", "4/3 π"), ("k = 50 N/m\nm = 0.5 kg\nprint 1/2π √(k/m)", "1/2 π")] {
        let e = err(src);
        assert!(e.message.contains(&format!("'{shown}' is ambiguous")) && hint(&e).contains('('), "{}", e.message);
    }
    tree("r = 2 m\nprint (4/3) π r³ to 4 digits");
    tree("r = 2 m\nprint 4/(3 π) r³ to 4 digits");
}

// ---- other rules ------------------------------------------------------------------------------------------------

#[test]
fn implicit_multiplication_binds_tighter_than_division() {
    let (p, d) = parse("h = 1\nc = 1\nλ = 1\nT = 1\nk_B = 1\nx = h c / λ k_B T", &[]).unwrap();
    let StmtKind::Assign { value, .. } = &p.body[5].kind else { panic!() };
    let ExprKind::BinOp { op, right, .. } = &value.kind else { panic!() };
    assert_eq!(op, "/");
    assert!(matches!(right.kind, ExprKind::BinOp { implicit: true, .. }));
    assert!(d.warnings.is_empty());
    let w = warnings("c = 1\ng = 2\nx = 3\ny = c²/g x");
    assert_eq!(w, vec!["this divides by all of 'g x': implicit multiplication binds tighter than '/'".to_string()]);
}

#[test]
fn lexer_details() {
    // look-alikes are replaced with a warning; strings are left alone
    let (_, d) = parse("х = 1\nprint \"х\"", &[]).unwrap();
    assert_eq!(d.warnings[0].message, "replaced look-alike character 'х' (Cyrillic Small Letter Ha) with Latin 'x'");
    assert_eq!(d.warnings.len(), 1);
    let e = err("x = 3 @ 4");
    assert_eq!(e.message, "unexpected character '@' (Commercial At)");
    let e = err("x = 1.2.3");
    assert_eq!(e.message, "this number has two decimal points: 1.2.3...");
    let t = tree("x = 6.67×10⁻¹¹\ny = 2½\nz = 4i\nw = 1.50e3");
    assert!(t.contains("value=6.67e-11 sigfigs=3"), "{t}");
    assert!(t.contains("value=2.5"));
    assert!(t.contains("#imag_literal=True"));
    assert!(t.contains("value=1500.0 sigfigs=3"));
}

#[test]
fn check_format_of_a_parse_error() {
    let src = "g = 9.81 m/s²\nprint 2 g";
    let e = err(src);
    assert_eq!(
        e.format(Some(src), Some("a.fm")),
        "a.fm, line 2: '2 g' is ambiguous: right after a number, g is a unit (grams), but g is also your variable g\n    \
         print 2 g\n            ^\n  hint: write  2*g  for 2 × your variable g, or  2 [g]  for the unit"
    );
}
