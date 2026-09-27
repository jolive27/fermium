//! The checker's error table in the language specification (docs/spec/errors.md) lists exactly the message
//! templates of the checker files it covers, and each example tagged with a row is rejected by the checker with
//! that row's message (spec D1, DECISIONS D353).
use std::path::Path;

/// The checker source files errors.md covers completely (errors.md §1 says which are still to do).
const COVERED: &[&str] = &["arith.rs", "arrays.rs", "builtin.rs", "calculus.rs", "calls.rs", "checker.rs", "clist.rs",
                           "convert.rs", "cplx.rs", "data.rs", "dispatch.rs", "events.rs", "exprs.rs", "lists.rs", "modules.rs", "names.rs", "print.rs", "rng.rs", "solve.rs",
                           "stmts.rs", "uncertain.rs", "units.rs", "vecmat.rs"];

/// The Rust string literal at the start of `s` (which starts with '"'): its value and its length in bytes.
fn rust_literal(s: &str) -> (String, usize) {
    let b: Vec<char> = s.chars().collect();
    let (mut i, mut out, mut len) = (1, String::new(), 1);
    while b[i] != '"' {
        if b[i] == '\\' {
            let n = b[i + 1];
            len += 1 + n.len_utf8();
            i += 2;
            if n == '\n' {
                while matches!(b[i], ' ' | '\t' | '\n') {
                    len += 1;
                    i += 1;
                }
                continue;
            }
            match n {
                'n' => out.push('\n'),
                '"' | '\\' | '\'' => out.push(n),
                _ => {
                    out.push('\\');
                    out.push(n)
                }
            }
            continue;
        }
        out.push(b[i]);
        len += b[i].len_utf8();
        i += 1;
    }
    (out, len + 1)
}

/// A message template with each `{…}` placeholder (and each run of them) written as one `…`.
fn template(t: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in t.chars() {
        match c {
            '{' => {
                depth += 1;
                if !out.ends_with('…') {
                    out.push('…');
                }
            }
            '}' if depth > 0 => depth -= 1,
            '…' if out.ends_with('…') => {}
            _ if depth > 0 => {}
            _ => out.push(c),
        }
    }
    out
}

/// The literal (possibly inside `format!(`) at the start of `rest`, as a template.
fn first_literal(rest: &str) -> Option<String> {
    let mut rest = rest.trim_start();
    if let Some(r) = rest.strip_prefix("format!(") {
        rest = r.trim_start();
    }
    rest.starts_with('"').then(|| template(&rust_literal(rest).0))
}

fn src_of(crate_dir: &str, f: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(crate_dir).join("src").join(f);
    let s = std::fs::read_to_string(p).unwrap();
    match s.find("#[cfg(test)]") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Every error message template of the covered files: the first literal argument of each `.err(…)` and
/// `Diagnostic::error(…)`, the message of each `unify_or(…, |c| …)` closure, the literals that start a
/// `format!(` or a `{ … }` branch of a `let msg = …;` that the next lines raise with `err(msg`; plus the stored
/// "might not have a value" messages (`unset_msg = Some(format!(…))`, any checker file, raised in exprs.rs) and the unit-power overflow message
/// (fermium-units exact.rs `overflow_message`, raised in checker.rs).
fn checker_templates() -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let f = entry.unwrap().file_name().into_string().unwrap();
        let s = src_of("fermium-check", &f);
        let mut pos = 0;
        while let Some(i) = s[pos..].find("unset_msg = Some(") {
            pos += i + 17;
            out.extend(first_literal(&s[pos..]));
        }
    }
    let ex = src_of("fermium-units", "exact.rs");
    let at = ex.find("fn overflow_message").expect("fermium-units has overflow_message");
    let at = at + ex[at..].find("format!(").unwrap();
    out.extend(first_literal(&ex[at..]));
    let openers = [".err(", "Diagnostic::error(", "unify_or(", "let msg = "];
    for f in COVERED {
        let s = src_of("fermium-check", f);
        let mut pos = 0;
        while let Some((at, op)) = openers.iter().filter_map(|o| s[pos..].find(o).map(|i| (pos + i, *o))).min() {
            pos = at + op.len();
            let rest = &s[pos..];
            match op {
                "unify_or(" => {
                    // the message closure `|c| …`: its body starts after the second '|'
                    let head = &rest[..rest.len().min(200)];
                    if let Some(j) = head.find('|') {
                        if let Some(k) = head[j + 1..].find('|') {
                            out.extend(first_literal(&rest[j + 1 + k + 1..]));
                        }
                    }
                }
                "let msg = " => {
                    let end = rest.find(';').unwrap();
                    let after = &rest[end..(end + 300).min(rest.len())];
                    if !after.contains("err(msg") {
                        continue; // a warning's message
                    }
                    // the literals that start a `format!(` or a `{ … }` branch
                    for (k, _) in rest[..end].match_indices('"') {
                        let before = rest[..k].trim_end();
                        if (before.ends_with("format!(") || before.ends_with('{'))
                            && !rest[..k].ends_with('\\') {
                            out.extend(first_literal(&rest[k..]));
                        }
                    }
                }
                _ => out.extend(first_literal(rest)),
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Does `msg` match `tpl`, where each `…` in tpl stands for any text?
fn matches_template(msg: &str, tpl: &str) -> bool {
    let parts: Vec<&str> = tpl.split('…').collect();
    if parts.len() == 1 {
        return msg == tpl;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !msg.starts_with(first) || msg.len() < first.len() + last.len() || !msg[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &msg[first.len()..msg.len() - last.len()];
    for p in &parts[1..parts.len() - 1] {
        match rest.find(p) {
            Some(i) => rest = &rest[i + p.len()..],
            None => return false,
        }
    }
    true
}

#[test]
fn spec_checker_errors() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/spec/errors.md");
    let text = std::fs::read_to_string(path).unwrap();
    // the covered files named in errors.md are this test's
    let cov_line = text.lines().find(|l| l.starts_with("Covered files:")).expect("errors.md names its covered files");
    let mut named: Vec<&str> = cov_line.split('`').skip(1).step_by(2).collect();
    named.sort();
    assert_eq!(named, COVERED, "errors.md's 'Covered files:' line and COVERED in spec_errors.rs differ");
    let mut rows: Vec<(usize, String, String)> = vec![]; // (number, template, example tag)
    for line in text.lines().filter(|l| l.starts_with("| ") && l.contains('`')) {
        let cells: Vec<&str> = line.split(" | ").collect();
        let Ok(n) = cells[0].trim_start_matches("| ").parse::<usize>() else { continue };
        let tpl = line.split('`').nth(1).unwrap().replace("\\|", "|");
        let tag = cells.last().unwrap().trim_end_matches(" |").trim().to_string();
        assert_eq!(n, rows.len() + 1, "errors.md: rows are numbered 1, 2, …");
        rows.push((n, tpl, tag));
    }
    let want = checker_templates();
    assert!(want.len() >= 50, "found only {} message templates in the checker", want.len());
    let got: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let missing: Vec<&String> = want.iter().filter(|t| !got.contains(t)).collect();
    let extra: Vec<&String> = got.iter().filter(|t| !want.contains(t)).collect();
    let mut bad = vec![];
    if !missing.is_empty() || !extra.is_empty() {
        bad.push(format!("not in the table: {missing:#?}\nnot in the source: {extra:#?}"));
    }
    let mut seen = vec![];
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if l != "```fermium-error" {
            continue;
        }
        let code: String = lines.by_ref().take_while(|l| !l.starts_with("```")).map(|l| format!("{l}\n")).collect();
        let tag = code.lines().next().unwrap_or("").trim_start_matches("# ").to_string();
        let Some(row) = rows.iter().find(|r| r.2 == tag) else {
            bad.push(format!("example {tag}: no row has it in the Example column"));
            continue;
        };
        seen.push(tag.clone());
        let prog = match fermium_syntax::parse(&code, &[]) {
            Ok((p, _)) => p,
            Err(e) => {
                bad.push(format!("example {tag} doesn't parse: {}", e.message));
                continue;
            }
        };
        match fermium_check::api::check_keep(&prog, fermium_check::CheckOptions::default()) {
            Ok(_) => bad.push(format!("example {tag} checks, but should fail with: {}", row.1)),
            Err((e, _)) if !matches_template(&e.message, &row.1) => {
                bad.push(format!("example {tag} fails with\n    {}\nnot\n    {}", e.message, row.1))
            }
            Err(_) => {}
        }
    }
    for r in &rows {
        if r.2 != "—" && !seen.contains(&r.2) {
            bad.push(format!("row {} names example {} but there is none", r.0, r.2));
        }
    }
    assert!(bad.is_empty(), "errors.md:\n{}", bad.join("\n"));
}
