//! Scripted REPL sessions compared with Fermium 1.5's REPL (fixtures written by rust/tools/repl_sessions.py
//! from fermium/repl.py, run non-interactively as `fermium repl < session`). A session listed in
//! known_differences.txt (with the reason) may differ; every other must print exactly what v1 prints.
use std::path::Path;

fn run_session(input: &str, dir: &str) -> String {
    let (input, dir) = (input.to_string(), dir.to_string());
    fermium_codegen::eval::STACK_LIMIT.store(400 << 20, std::sync::atomic::Ordering::Relaxed);
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            let mut out: Vec<u8> = vec![];
            let code = fermium_repl::run(&mut fermium_repl::Piped(input.as_bytes()), false, &mut out, &dir);
            assert_eq!(code, 0);
            String::from_utf8(out).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn sessions_match_v1() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let known: Vec<String> = std::fs::read_to_string(dir.join("known_differences.txt"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    let sdir = dir.join("sessions");
    let mut names: Vec<_> = std::fs::read_dir(&sdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".in"))
        .collect();
    names.sort();
    let (mut pass, mut bad) = (0, vec![]);
    for n in &names {
        let stem = n.trim_end_matches(".in");
        let input = std::fs::read_to_string(sdir.join(n)).unwrap();
        let want = std::fs::read_to_string(sdir.join(format!("{stem}.out"))).unwrap();
        let got = run_session(&input, sdir.to_str().unwrap());
        if got == want {
            pass += 1;
        } else if !known.iter().any(|k| k == stem) {
            bad.push(format!("--- session {stem}\n{input}--- v1:\n{want}--- rust:\n{got}"));
        }
    }
    eprintln!("{pass} of {} sessions print exactly what v1 prints", names.len());
    assert!(bad.is_empty(), "{} sessions differ from v1:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn the_repl_keeps_the_python_module() {
    // v1 tests/test_python_interop.py::test_repl_keeps_the_python_module (skipped without python3 + NumPy)
    let ok = std::process::Command::new("python3").args(["-c", "import numpy"]).output()
        .map(|o| o.status.success()).unwrap_or(false);
    if !ok {
        eprintln!("skipped: python3 with numpy isn't available");
        return;
    }
    let mut s = fermium_repl::Session::new(".");
    let mut out: Vec<u8> = vec![];
    for input in ["use python numpy as np\n", "x = np.sqrt(16)\n", "print x + np.sqrt(9)\n",
                  "use python numpy as np:\n    hypot(a [m], b [m]) -> [m]\n", "print np.hypot(3 m, 4 m)\n"] {
        s.execute(input, &mut out, None).unwrap_or_else(|e| panic!("{input}: {}", e.message));
    }
    assert_eq!(String::from_utf8(out).unwrap(), "7\n5 m\n");
}
