//! fermium-wasm's `run`, natively (the same code the browser runs; web/test/ runs it as WebAssembly).
use fermium_wasm::{run, Output};

fn go(src: &str) -> Output {
    run(src, env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn prints_like_fermium_run() {
    let o = go("print 4π² (1.20 m) / (2.21 s)²\n");
    assert_eq!(o, Output { stdout: "9.70 m/s²\n".into(), ..Default::default() });
}

#[test]
fn unit_errors_are_one_line_with_a_caret_and_a_hint() {
    let o = go("L = 1.20 m\nT = 2.21 s\nprint 4π² L / T + 9.8 m/s²\n");
    let e = o.error.unwrap();
    assert!(e.starts_with("line 3: can't add speed [m/s] to acceleration [m/s²]\n    print 4π² L / T + 9.8 m/s²\n    "), "{e}");
    assert!(e.contains("^^^") && e.ends_with("hint: both sides of + and - must have the same units"), "{e}");
    assert_eq!(o.stdout, "");
}

#[test]
fn a_run_time_error_keeps_what_was_printed() {
    let o = go("print 2 + 3\nx = [1, 2, 3]\nprint x[7]\n");
    assert_eq!(o.stdout, "5\n");
    assert!(o.error.unwrap().starts_with("line 3: "));
}

#[test]
fn warnings_come_back_one_entry_each() {
    let o = go("h = 2\nprint h\n");
    assert_eq!(o.stdout, "2\n");
    assert_eq!(o.warnings.len(), 1, "{:?}", o.warnings);
    assert!(o.warnings[0].starts_with("warning: line 1: h (Planck's constant) is now your variable"));
}

#[test]
fn parse_errors() {
    let o = go("x = (1 +\n");
    assert!(o.error.unwrap().starts_with("line 1: "));
}

#[test]
fn json_result() {
    let o = Output { stdout: "a\n".into(), warnings: vec!["warning: w".into()], error: None,
                     files: vec![("p.png".into(), b"PNG".to_vec())] };
    assert_eq!(o.to_json(), r#"{"stdout":"a\n","warnings":["warning: w"],"error":null,"plots":[{"name":"p.png","mime":"image/png","base64":"UE5H"}]}"#);
}
