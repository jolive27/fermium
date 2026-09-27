//! The examples in the language specification (docs/spec/*.md) stay valid (spec D1, DECISIONS D350).
//!
//! Every ```fermium block must parse with the Rust front end. Blocks marked ```fermium-error are examples of
//! rejected programs. With `FERMIUM_BIN` set to a `fermium` binary, every ```fermium block must also run without
//! an error (exit 0) and every ```fermium-error block must fail (non-zero exit); without it only parsing is
//! checked, so this test needs nothing but this crate.
use fermium_syntax::ast::{Expr, ExprKind, StmtKind};
use std::path::{Path, PathBuf};

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec")
}

/// (file, 1-based line of the fence, info string, code) for every ```fermium and ```fermium-error block.
fn blocks() -> Vec<(String, usize, String, String)> {
    blocks_with(&["fermium", "fermium-error"])
}

/// The same for the fenced blocks whose info string is one of `infos`.
fn blocks_with(infos: &[&str]) -> Vec<(String, usize, String, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(spec_dir())
        .expect("docs/spec exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    let mut out = vec![];
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let mut lines = text.lines().enumerate();
        while let Some((i, line)) = lines.next() {
            let Some(info) = line.strip_prefix("```") else { continue };
            let info = info.trim().to_string();
            let mut code = String::new();
            for (_, l) in lines.by_ref() {
                if l.starts_with("```") {
                    break;
                }
                code.push_str(l);
                code.push('\n');
            }
            if infos.contains(&info.as_str()) {
                out.push((name.clone(), i + 1, info, code));
            }
        }
    }
    out
}

/// The tree of an expression as nested prefix forms, ignoring brackets, spans and implicit-vs-explicit `*`.
fn shape(e: &Expr) -> String {
    use ExprKind as K;
    match &e.kind {
        K::Num { value, .. } => format!("{value}"),
        K::Name { name } => name.clone(),
        K::BinOp { op, left, right, .. } => format!("({op} {} {})", shape(left), shape(right)),
        K::Neg { operand } => format!("(neg {})", shape(operand)),
        K::Compare { op, left, right, .. } => format!("({op} {} {})", shape(left), shape(right)),
        K::Logic { op, left, right } => format!("({op} {} {})", shape(left), shape(right)),
        K::Not { operand } => format!("(not {})", shape(operand)),
        K::Call { func, args } => {
            format!("(call {} {})", shape(func), args.iter().map(shape).collect::<Vec<_>>().join(" "))
        }
        K::Sqrt { operand, root } => format!("(root{root} {})", shape(operand)),
        K::Uncertain { value, err } => format!("(± {} {})", shape(value), shape(err)),
        K::Convert { value, unit } => format!("(in {} {})", shape(value), unit.text),
        K::Quantity { value, unit, .. } => format!("(unit {} {})", shape(value), unit.text),
        K::Prime { target, order } => format!("(prime{order} {})", shape(target)),
        K::IfExpr { cond, then, other } => format!("(if {} {} {})", shape(cond), shape(then), shape(other)),
        other => format!("<{:?}>", std::mem::discriminant(other)),
    }
}

fn print_item_shape(src: &str) -> Result<String, String> {
    let (prog, _) = fermium_syntax::parse(&format!("print {src}\n"), &[]).map_err(|e| e.message)?;
    match &prog.body[0].kind {
        StmtKind::Print { items } if items.len() == 1 => Ok(shape(&items[0])),
        _ => Err("not one print item".into()),
    }
}

/// grammar.md's ```fermium-reads blocks: each line `A ≡ B` says that A parses to the same tree as the explicitly
/// bracketed B. This keeps the precedence table honest.
#[test]
fn spec_precedence_readings() {
    let mut n = 0;
    let mut bad = vec![];
    for (file, line, info, code) in blocks_with(&["fermium-reads"]) {
        assert_eq!(info, "fermium-reads");
        for (k, row) in code.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            let Some((a, b)) = row.split_once('≡') else {
                bad.push(format!("{file}:{}: expected 'A ≡ B'", line + 1 + k));
                continue;
            };
            let (sa, sb) = (print_item_shape(a.trim()), print_item_shape(b.trim()));
            n += 1;
            if sa.is_err() || sa != sb {
                bad.push(format!("{file}:{}: {row}\n    {sa:?}\n    {sb:?}", line + 1 + k));
            }
        }
    }
    assert!(n >= 10, "grammar.md should list precedence readings");
    assert!(bad.is_empty(), "precedence readings that don't hold:\n{}", bad.join("\n"));
}

/// grammar.md §1.6 and §1.10 list the keywords and operators; they must be the lexer's.
#[test]
fn spec_lists_the_lexers_keywords_and_operators() {
    let text = std::fs::read_to_string(spec_dir().join("grammar.md")).unwrap();
    let block_after = |heading: &str| -> String {
        let i = text.find(heading).unwrap_or_else(|| panic!("grammar.md has {heading}"));
        let rest = &text[i..];
        let s = rest.find("```text\n").unwrap() + 8;
        let e = rest[s..].find("```").unwrap();
        rest[s..s + e].to_string()
    };
    let mut kws: Vec<&str> = vec![];
    let kb = block_after("### 1.6 Keywords");
    kws.extend(kb.split_whitespace());
    let mut want: Vec<&str> = fermium_syntax::lexer::KEYWORDS.to_vec();
    kws.sort();
    want.sort();
    assert_eq!(kws, want, "grammar.md §1.6 vs lexer::KEYWORDS");
    let ob = block_after("### 1.10 Operators");
    // the quoted operators: every other piece between double quotes (no operator contains one)
    let mut ops: Vec<String> = ob.split('"').skip(1).step_by(2).map(|p| p.to_string()).collect();
    let mut want: Vec<String> = fermium_syntax::lexer::OPERATORS.iter().map(|s| s.to_string()).collect();
    ops.sort();
    want.sort();
    assert_eq!(ops, want, "grammar.md §1.10 vs lexer::OPERATORS");
}

/// grammar.md §1.7's vulgar fractions and units.md §3's prefixes are the lexer's and the unit table's.
#[test]
fn spec_lists_the_fractions_and_prefixes() {
    let grammar = std::fs::read_to_string(spec_dir().join("grammar.md")).unwrap();
    let line = grammar.lines().find(|l| l.trim_start().starts_with("vulgar    =")).expect("the vulgar rule");
    let mut got: Vec<char> = line.split('"').skip(1).step_by(2).filter_map(|p| p.chars().next()).collect();
    let mut want: Vec<char> = fermium_syntax::lexer::VULGAR_FRACS.iter().map(|v| v.0).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want, "grammar.md §1.7 vs lexer::VULGAR_FRACS");
    let units = std::fs::read_to_string(spec_dir().join("units.md")).unwrap();
    let line = units.lines().find(|l| l.starts_with("- **Prefixes:**")).expect("the prefixes line");
    let mut got: Vec<&str> = line.split('`').nth(1).unwrap().split_whitespace().collect();
    let mut want: Vec<&str> = fermium_syntax::tables::PREFIXES.to_vec();
    got.sort();
    want.sort();
    assert_eq!(got, want, "units.md §3 vs tables::PREFIXES");
}

#[test]
fn spec_has_examples() {
    let b = blocks();
    assert!(b.iter().filter(|x| x.2 == "fermium").count() >= 10, "docs/spec should have examples");
    assert!(b.iter().any(|x| x.2 == "fermium-error"), "docs/spec should show rejected programs");
}

#[test]
fn spec_examples_parse() {
    let mut bad = vec![];
    for (file, line, info, code) in blocks() {
        if info != "fermium" {
            continue;
        }
        if let Err(e) = fermium_syntax::parse(&code, &[]) {
            bad.push(format!("{file}:{line}: {} (line {:?} of the block)", e.message, e.line));
        }
    }
    assert!(bad.is_empty(), "spec examples that don't parse:\n{}", bad.join("\n"));
}

#[test]
fn spec_examples_run() {
    let Ok(bin) = std::env::var("FERMIUM_BIN") else {
        eprintln!("FERMIUM_BIN not set: the spec examples were parsed, not run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("fermium-spec-examples-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut bad = vec![];
    for (k, (file, line, info, code)) in blocks().into_iter().enumerate() {
        let path = dir.join(format!("example{k}.fm"));
        std::fs::write(&path, &code).unwrap();
        let out = std::process::Command::new(&bin).arg("run").arg(&path).current_dir(&dir).output()
            .expect("run FERMIUM_BIN");
        let ok = out.status.success();
        if info == "fermium" && !ok {
            bad.push(format!("{file}:{line}: failed:\n{}", String::from_utf8_lossy(&out.stderr)));
        } else if info == "fermium-error" && ok {
            bad.push(format!("{file}:{line}: should be rejected but ran:\n{}", String::from_utf8_lossy(&out.stdout)));
        } else if info == "fermium-error" {
            // semantics.md §5: a rejected program gets the one-line error "<file>, line N: …", not a crash
            let err = String::from_utf8_lossy(&out.stderr);
            let first = err.lines().find(|l| !l.starts_with("warning") && !l.starts_with(' ')).unwrap_or("");
            if !first.starts_with(&format!("example{k}.fm, line ")) || out.status.code() != Some(1) {
                bad.push(format!("{file}:{line}: not a one-line error with exit status 1:\n{err}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(bad.is_empty(), "spec examples:\n{}", bad.join("\n"));
}

/// README.md's cross-reference table gives each conformance area's number of cases; they must be the suite's.
#[test]
fn spec_conformance_counts() {
    let text = std::fs::read_to_string(spec_dir().join("README.md")).unwrap();
    let s = text.find("## Where the conformance suite").expect("README.md has the cross-reference");
    let sect = &text[s..s + text[s..].find("future work").unwrap()];
    let cases = spec_dir().join("../../conformance/cases");
    let count = |area: &str| -> usize {
        std::fs::read_dir(cases.join(area))
            .unwrap_or_else(|_| panic!("conformance/cases/{area} exists"))
            .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "fm"))
            .count()
    };
    let mut bad = vec![];
    let mut n = 0;
    // every "`area` (N)"
    for piece in sect.split('`').collect::<Vec<_>>().windows(2) {
        let (name, after) = (piece[0], piece[1]);
        let Some(rest) = after.strip_prefix(" (") else { continue };
        let Some(num) = rest.split(')').next().and_then(|x| x.parse::<usize>().ok()) else { continue };
        n += 1;
        let got = count(name);
        if got != num {
            bad.push(format!("{name}: README says {num}, the suite has {got}"));
        }
    }
    assert!(n >= 15, "README.md's cross-reference should give case counts");
    let areas: Vec<String> = std::fs::read_dir(&cases).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    let total: usize = areas.iter().map(|a| count(a)).sum();
    let want = format!("The suite has {total} cases in {} areas", areas.len());
    if !sect.contains(&want) {
        bad.push(format!("README.md should say: {want}"));
    }
    assert!(bad.is_empty(), "docs/spec/README.md vs conformance/cases:\n{}", bad.join("\n"));
}
