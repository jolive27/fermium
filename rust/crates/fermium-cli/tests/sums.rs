//! The decimal-place rule for printed sums (rust/DIVERGENCES.md, "Sums of measured values"), in both back ends.
use std::process::Command;

fn run(tag: &str, src: &str, backend: &str) -> String {
    let d = std::env::temp_dir().join(format!("fermium-sums-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("prog.fm");
    std::fs::write(&f, src).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_fermium")).args(["run", "--backend", backend]).arg(&f).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn both(tag: &str, src: &str) -> String {
    let a = run(tag, src, "auto");
    assert_eq!(a, run(tag, src, "interp"), "{src}");
    a
}

#[test]
fn lengths_and_cancellations_follow_the_rule() {
    assert_eq!(both("len", "print 1.20 m + 2.0 m\nprint 10.0 m - 9.95 m\nprint 12.0 kg - 11.99 kg\n"),
               "3.2 m\n0.1 m\n0 kg\n");
}

/// Red team 11 #1: operands in °C have decimal places that can't be read off kelvin, so temperatures keep v1's
/// rule (outputs from `python3 -m fermium run`).
#[test]
fn temperature_differences_keep_v1s_rule() {
    assert_eq!(both("temp", "print 0.5 °C - 0.2 °C\nprint 25.5 °C - 20.0 °C\nprint 300.0 K - 20.0 °C\n\
                             print 293.15 K + 0.5 K\n"),
               "0.30 K\n5.50 K\n6.850 K\n293.65 K\n");
}
