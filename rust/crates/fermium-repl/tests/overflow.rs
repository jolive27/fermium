//! Red team 13 #1: a unit power too large for 64 bits used to panic and kill the REPL (every definition lost).
//! Now it is an ordinary error and the session goes on.
#[test]
fn the_session_survives_a_unit_power_overflow() {
    let input = "x = 1 m\nz = (x^(1/4294967296))^(1/4294967296)\ny = (x^4294967296)^4294967296\nprint 2 x\n";
    let out = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut out: Vec<u8> = vec![];
            let code = fermium_repl::run(&mut fermium_repl::Piped(input.as_bytes()), false, &mut out, ".");
            assert_eq!(code, 0);
            String::from_utf8(out).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(out.matches("this unit's power is too large to track exactly").count(), 2, "{out}");
    assert!(out.contains("(m^(1/18446744073709551616))"), "{out}");
    assert!(out.contains("(m^18446744073709551616)"), "{out}");
    assert!(out.trim_end().ends_with("2 m"), "x is still defined afterwards: {out}");
}
