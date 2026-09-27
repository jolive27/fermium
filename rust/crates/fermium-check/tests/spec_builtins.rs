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

/// semantics.md §7.1: every line `CALL ⇒ KIND [UNIT]` of the ```fermium-types block is type-checked and the
/// result's kind and dimension compared with the line (spec D1, DECISIONS D352). The lines without `⇒` are a
/// prelude. Every built-in except the statements `seed`, `push`, `append` must be called in some line.
#[test]
fn spec_builtins_types() {
    use fermium_check::checker::Binding;
    use fermium_check::types::Ty;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec/semantics.md");
    let text = std::fs::read_to_string(path).unwrap();
    let s = text.find("```fermium-types\n").expect("semantics.md has a ```fermium-types block") + 17;
    let block = &text[s..s + text[s..].find("```").unwrap()];
    let prelude: String = block.lines().filter(|l| !l.contains('⇒')).map(|l| format!("{l}\n")).collect();
    let check = |src: &str| -> Result<fermium_check::Checker, String> {
        let (prog, _) = fermium_syntax::parse(src, &[]).map_err(|e| format!("parse: {}", e.message))?;
        fermium_check::api::check_keep(&prog, fermium_check::CheckOptions::default())
            .map_err(|(e, _)| format!("check: {} (line {:?})", e.message, e.line))
    };
    let ty_of = |ck: &fermium_check::Checker, name: &str| -> Ty {
        match ck.global(name) {
            Some(Binding::Sym(id)) => ck.module.syms[id].ty.clone(),
            other => panic!("{name} is bound to {other:?}"),
        }
    };
    let mut bad = vec![];
    let mut called: Vec<String> = vec![];
    let mut n = 0;
    for line in block.lines().filter(|l| l.contains('⇒')) {
        let (calls, want) = line.split_once('⇒').unwrap();
        let want = want.trim();
        let (kind, unit) = want.rsplit_once(" [").expect("KIND [UNIT]");
        let unit = unit.strip_suffix(']').expect("KIND [UNIT]");
        for call in calls.split(';').map(str::trim) {
            n += 1;
            // the names called: identifiers followed by '('
            let mut start: Option<usize> = None;
            for (i, c) in call.char_indices() {
                if c.is_alphanumeric() || c == '_' {
                    start.get_or_insert(i);
                } else {
                    if let (Some(b), '(') = (start, c) {
                        called.push(call[b..i].to_string());
                    }
                    start = None;
                }
            }
            let want_line = if unit == "1" { "want_ = 1\n".to_string() } else { format!("want_ = 1 {unit}\n") };
            let src = format!("{prelude}res_ = {call}\n{want_line}");
            let ck = match check(&src) {
                Ok(ck) => ck,
                Err(e) => {
                    bad.push(format!("{call}: {e}"));
                    continue;
                }
            };
            let wdim = match ty_of(&ck, "want_") {
                Ty::Num(d) => ck.u.resolve(&d),
                t => panic!("want_ = 1 {unit} is {t:?}"),
            };
            let ty = ty_of(&ck, "res_");
            let (gk, gd) = match &ty {
                Ty::Num(d) => ("number", Some(d.clone())),
                Ty::Bool => ("boolean", None),
                Ty::Str => ("text", None),
                Ty::List(d) => ("list", Some(d.clone())),
                Ty::Complex(d) => ("complex", Some(d.clone())),
                Ty::ComplexList(d) => ("complex list", Some(d.clone())),
                Ty::Vec { dim: Some(d), .. } => ("vector", Some(d.clone())),
                Ty::Vec { dims: Some(ds), .. } => {
                    let r: Vec<_> = ds.iter().map(|d| ck.u.resolve(d)).collect();
                    if r.windows(2).any(|w| w[0] != w[1]) {
                        bad.push(format!("{call}: a vector of mixed dimensions"));
                        continue;
                    }
                    ("vector", Some(ds[0].clone()))
                }
                Ty::Mat { dim, .. } => ("matrix", Some(dim.clone())),
                Ty::Array { dim, .. } => ("array", Some(dim.clone())),
                t => {
                    bad.push(format!("{call}: unexpected type {t:?}"));
                    continue;
                }
            };
            let gdim = gd.map(|d| ck.u.resolve(&d)).unwrap_or(wdim);
            if gk != kind || gdim != wdim {
                bad.push(format!("{call}: semantics.md says {kind} [{unit}], the checker gives {} ({gk})",
                                 ck.type_text(&ty, None)));
            }
        }
    }
    assert!(n >= 100, "the ```fermium-types block should check every row");
    let mut missing: Vec<&str> = fermium_check::builtins::BUILTINS.iter().copied()
        .filter(|b| !["seed", "push", "append"].contains(b) && !called.iter().any(|c| c == b))
        .collect();
    missing.sort();
    assert!(missing.is_empty(), "built-ins not checked in semantics.md §7.1: {missing:?}");
    assert!(bad.is_empty(), "semantics.md §7.1 vs the checker:\n{}", bad.join("\n"));
}
