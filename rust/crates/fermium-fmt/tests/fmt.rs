//! `fermium fmt`: outputs checked against Fermium 1.5's formatter (fermium/fmt.py).

use fermium_fmt::{fix_source, format_source};
use fermium_syntax::Diagnostics;

fn fmt(src: &str, mode: &str) -> String {
    format_source(src, mode, &mut Diagnostics::new()).expect("formats")
}

const ASCII: &str = "A = pi r^2\nE = 1/2 m v^-2 + sqrt(x) + (1/2) x\nprint theta_0, hbar, 5.0 +- 0.2 m, x <= y\n";
const PRETTY: &str = "A = π r²\nE = 1/2 m v⁻² + √(x) + ½ x\nprint θ₀, ħ, 5.0 ± 0.2 m, x ≤ y\n";

#[test]
fn pretty_and_back() {
    assert_eq!(fmt(ASCII, "pretty"), PRETTY);
    assert_eq!(fmt(PRETTY, "ascii"), ASCII);
    assert_eq!(fmt(ASCII, "ascii"), ASCII);
}

#[test]
fn symbols_to_ascii() {
    let src = "T = 2π√(L/g) + ½ θ₀² + ∫ x dx from 0 to 1\nF = ∇φ\nz = 2𝑖 + 6.67×10⁻¹¹ + a ≈ b\nu = 3 μm\n";
    assert_eq!(fmt(src, "pretty"), src);
    assert_eq!(
        fmt(src, "ascii"),
        "T = 2pi sqrt(L/g) + (1/2) theta_0^2 + integral x dx from 0 to 1\nF = grad(phi)\nz = 2i + 6.67e-11 + a ~= b\nu = 3 um\n"
    );
}

#[test]
fn fix_brackets_collisions() {
    assert_eq!(fix_source("m = 0.5 kg\nk = 50 N/m\nx = 0.1 m\n").unwrap(),
               ("m = 0.5 kg\nk = 50 [N/m]\nx = 0.1 [m]\n".to_string(), 2));
}
