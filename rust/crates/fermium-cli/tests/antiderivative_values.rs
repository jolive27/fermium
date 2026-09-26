//! Antiderivatives evaluate ln|u| (red team round 11 #2, #3; rust/DIVERGENCES.md "Indefinite integrals"): F is
//! real on both sides of each pole and F(b) − F(a) equals the definite integral, also with dimensioned constants.
//! The printed formula stays v1's. Both back ends.
use std::process::Command;

fn run(src: &str, backend: &str) -> String {
    let dir = std::env::temp_dir().join(format!("fermium-antideriv-{}-{backend}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("p.fm");
    std::fs::write(&f, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run", "p.fm"]).current_dir(&dir)
        .env("FERMIUM_BACKEND", backend).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn antiderivatives_are_real_on_both_sides_of_each_pole() {
    let cases = [
        ("F = ∫ 1/(9 - x²) dx\nprint F\nprint F(1) - F(0), F(5) - F(4)\nprint ∫ 1/(9 - x²) dx from 0 to 1\n",
         "F(x) = -ln(-3 + x)/6 + ln(3 + x)/6\n0.116 -0.0933\n0.116\n"),
        ("F = ∫ 1/(x - 5) dx\nprint F(2) - F(1), F(7) - F(6)\nprint ∫ 1/(x - 5) dx from 1 to 2\n",
         "-0.288 0.693\n-0.288\n"),
        ("F = ∫ 3/(x² - 5x + 6) dx\nprint F(2.5) - F(2.2)\nprint ∫ 3/(x² - 5x + 6) dx from 2.2 to 2.5\n",
         "-4.2\n-4.2\n"),
        ("F = ∫ 1/(2 - x) dx\nprint F(1) - F(0), F(4) - F(3)\n", "0.693 -0.693\n"),
        ("a = 2 m\nF = ∫ 1/(x² - a²) dx\nprint F\nprint F(3 m) - F(2.5 m), F(1 m) - F(0 m)\n\
          print ∫ 1/(x² - a²) dx from 0 m to 1 m\n",
         "F(x) = (-ln(x + a) + ln(x - a))/(2a)   [1/m, for x in m]\n0.15 1/m -0.275 1/m\n-0.275 1/m\n"),
        ("a = 2 m\nF = ∫ 1/(x - a) dx\nprint F(1 m) - F(0 m)\n", "-0.693\n"),
    ];
    for (src, want) in cases {
        for backend in ["interp", "auto"] {
            assert_eq!(run(src, backend), want, "{src} ({backend})");
        }
    }
}
