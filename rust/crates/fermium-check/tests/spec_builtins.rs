//! The built-in function table in the language specification (docs/spec/semantics.md §7) names exactly the
//! checker's built-ins (spec D1, DECISIONS D351).
use std::path::Path;

#[test]
fn spec_builtins_table() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec/semantics.md");
    let text = std::fs::read_to_string(path).unwrap();
    let start = text.find("## 7. Built-in functions").expect("semantics.md has §7 Built-in functions");
    let rest = &text[start..];
    let end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    let mut got: Vec<String> = vec![];
    for line in rest[..end].lines().filter(|l| l.starts_with("| `")) {
        let first = line.split(" | ").next().unwrap();
        // every other piece between backticks is a name
        got.extend(first.split('`').skip(1).step_by(2).map(|s| s.to_string()));
    }
    let mut dup = got.clone();
    dup.sort();
    dup.dedup();
    assert_eq!(dup.len(), got.len(), "a built-in is listed twice in semantics.md §7");
    let mut want: Vec<String> = fermium_check::builtins::BUILTINS.iter().map(|s| s.to_string()).collect();
    want.sort();
    got.sort();
    let missing: Vec<&String> = want.iter().filter(|n| !got.contains(n)).collect();
    let extra: Vec<&String> = got.iter().filter(|n| !want.contains(n)).collect();
    assert!(missing.is_empty() && extra.is_empty(),
            "semantics.md §7 vs builtins::BUILTINS: missing {missing:?}, not built-ins {extra:?}");
}
