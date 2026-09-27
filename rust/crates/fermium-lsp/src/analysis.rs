//! What the language server knows about a program (lsp.py's plain functions: `analyze`, `quick_fixes`,
//! `word_at`, `hover_text`, `completions`), tested without an editor. Positions are 0-based lines and
//! code-point columns, as in v1; the server converts them to the protocol's UTF-16 columns.
use fermium_check::api::check_keep;
use fermium_check::checker::{Binding, CheckOptions, Checker};
use fermium_ir::types::DExpr;
use fermium_syntax::diag::{Diagnostics, Fix, Severity};

/// A problem to underline (1-based line and column, like Fermium's messages).
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub line: u32,
    pub col: u32,
    pub length: u32,
    pub message: String,
    pub error: bool,
    pub hint: Option<String>,
    /// the quick fix: (start, end, new text) source offsets in code points (D235)
    pub fix: Vec<Fix>,
}

pub struct Analysis {
    pub problems: Vec<Problem>,
    /// the checker after the longest prefix of the program that checks
    pub checker: Option<Checker>,
}

fn check(source: &str, base_dir: &str) -> Result<(Checker, Diagnostics), fermium_syntax::Diagnostic> {
    let mut d = Diagnostics::new();
    let prog = fermium_syntax::parse_with(source, &[], &mut d)?;
    let opts = CheckOptions { base_dir: base_dir.to_string(), repl: false, source_name: String::new() };
    match check_keep(&prog, opts) {
        Ok(mut ck) => {
            d.warnings.append(&mut ck.diags.warnings);
            Ok((ck, d))
        }
        Err((e, _)) => Err(e),
    }
}

/// Check a program; collect its error (if any) and warnings, and keep the symbols for hover (lsp.py `analyze`).
pub fn analyze(source: &str, base_dir: &str) -> Analysis {
    let mut an = Analysis { problems: vec![], checker: None };
    match check(source, base_dir) {
        Ok((ck, d)) => {
            an.checker = Some(ck);
            for w in d.warnings {
                an.problems.push(Problem { line: w.line.unwrap_or(1).max(1), col: w.col.unwrap_or(1).max(1),
                                           length: w.length.max(1), message: w.message, error: false,
                                           hint: w.hint, fix: vec![] });
            }
        }
        Err(e) => {
            an.problems.push(Problem { line: e.line.filter(|l| *l > 0).unwrap_or(1),
                                       col: e.col.filter(|c| *c > 0).unwrap_or(1), length: e.length,
                                       message: e.message.clone(), error: e.severity == Severity::Error,
                                       hint: e.hint.clone(), fix: e.fix.clone() });
            // hover still works for everything above the error
            let lines: Vec<&str> = source.split('\n').collect();
            let mut stop = e.line.filter(|l| *l > 0).unwrap_or(1) as usize - 1;
            while stop > 0 && an.checker.is_none() {
                match check(&(lines[..stop.min(lines.len())].join("\n") + "\n"), base_dir) {
                    Ok((ck, _)) => an.checker = Some(ck),
                    Err(_) => stop -= 1,
                }
            }
        }
    }
    an
}

/// 0-based (line, code-point column) of a source offset in code points.
fn offset_pos(source: &str, off: usize) -> (u32, u32) {
    let (mut line, mut col) = (0u32, 0u32);
    for c in source.chars().take(off) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// An edit: 0-based (line0, col0, line1, col1, new text).
pub type Edit = (u32, u32, u32, u32, String);

/// The quick fixes for the problems found (lsp.py `quick_fixes`): a unit/variable collision (the A1 rule,
/// D235) is fixed the way `fermium fmt --fix` does it.
pub fn quick_fixes<'a>(an: &'a Analysis, source: &str) -> Vec<(String, &'a Problem, Vec<Edit>)> {
    let mut out = vec![];
    for p in &an.problems {
        if p.fix.is_empty() {
            continue;
        }
        let edits: Vec<Edit> = p
            .fix
            .iter()
            .map(|(a, b, t)| {
                let ((l0, c0), (l1, c1)) = (offset_pos(source, *a), offset_pos(source, *b));
                (l0, c0, l1, c1, t.clone())
            })
            .collect();
        let what = edits.iter().map(|e| format!("'{}'", e.4)).collect::<Vec<_>>().join(" and ");
        let title = if edits.iter().any(|e| e.4.starts_with('[')) {
            format!("Fermium: write {what} (keep the unit reading)")
        } else {
            format!("Fermium: write {what} (your variable)")
        };
        out.push((title, p, edits));
    }
    out
}

// lsp.py WORD: [A-Za-z_\u0370-\u03ff\u1f00-\u1fffħ][A-Za-z0-9_\u0370-\u03ff\u1f00-\u1fffħ₀-₉]*
fn word_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || ('\u{370}'..='\u{3ff}').contains(&c) || ('\u{1f00}'..='\u{1fff}').contains(&c)
        || c == 'ħ'
}

fn word_cont(c: char) -> bool {
    word_start(c) || c.is_ascii_digit() || ('₀'..='₉').contains(&c)
}

/// The WORD matches of a line, as (start, end) code-point columns (re.finditer).
fn words(cs: &[char]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        if word_start(cs[i]) {
            let s = i;
            i += 1;
            while i < cs.len() && word_cont(cs[i]) {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

fn line_chars(source: &str, line: usize) -> Option<Vec<char>> {
    source.split('\n').nth(line).map(|l| l.chars().collect())
}

/// The identifier touching the 0-based (line, char) position (lsp.py `word_at`).
pub fn word_at(source: &str, line: usize, ch: usize) -> Option<String> {
    let cs = line_chars(source, line)?;
    words(&cs).into_iter().find(|(s, e)| *s <= ch && ch <= *e).map(|(s, e)| cs[s..e].iter().collect())
}

/// `re.search(r"(WORD)\.$", text)`: the name written just before a final dot.
fn name_before_dot(text: &[char]) -> Option<String> {
    let (&last, rest) = text.split_last()?;
    if last != '.' {
        return None;
    }
    let mut s = rest.len();
    while s > 0 && word_cont(rest[s - 1]) {
        s -= 1;
    }
    let start = (s..rest.len()).find(|&i| word_start(rest[i]))?;
    Some(rest[start..].iter().collect())
}

/// The module whose member is at the position (`mechanics.pendulum_period`), as an index into the module table.
pub fn module_of(an: &Analysis, source: &str, line: usize, ch: usize) -> Option<usize> {
    module_in(an.checker.as_ref()?, source, line, ch)
}

fn module_in(ck: &Checker, source: &str, line: usize, ch: usize) -> Option<usize> {
    let cs = line_chars(source, line)?;
    let (s, _) = words(&cs).into_iter().find(|(s, e)| *s <= ch && ch <= *e)?;
    let q = name_before_dot(&cs[..s])?;
    match ck.global(&q) {
        Some(Binding::Module(m)) => Some(m),
        _ => None,
    }
}

/// Python's `'%.10g' % x`.
pub fn fmt_g(x: f64, prec: usize) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    if !x.is_finite() {
        return if x.is_nan() { "nan".into() } else if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let e = format!("{:.*e}", prec - 1, x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= prec as i32 {
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let s = format!("{:.*}", (prec as i32 - 1 - exp).max(0) as usize, x);
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    }
}

/// Markdown for the name at the 0-based position (lsp.py `hover_text`).
pub fn hover_text(an: &mut Analysis, source: &str, line: usize, ch: usize) -> Option<String> {
    hover_in(an.checker.as_mut(), source, line, ch)
}

/// `hover_text` with any checker (the Jupyter kernel asks its session's).
pub fn hover_in(mut ck: Option<&mut Checker>, source: &str, line: usize, ch: usize) -> Option<String> {
    let name = word_at(source, line, ch)?;
    let module = ck.as_deref().and_then(|c| module_in(c, source, line, ch));
    let mut b = ck.as_deref().and_then(|c| c.global(&name));
    if let Some(m) = module {
        let ck = ck.as_deref().unwrap();
        b = ck.module_binding(m, &name);
        if b.is_none() {
            let (mname, _, _) = ck.module_names(m)?;
            return Some(format!("**{name}**: not in the module {mname}"));
        }
    }
    if let (Some(b), Some(ck)) = (b, ck.as_deref_mut()) {
        match b {
            Binding::Sym(id) => {
                let sym = &ck.module.syms[id];
                return Some(format!("**{name}**: {}", ck.type_text(&sym.ty, sym.hint.as_ref())));
            }
            Binding::Func(f) if ck.funcs[f].versions.len() > 1 => {
                // multiple dispatch (C5): at a call, the version(s) the call chose; elsewhere, every version
                let (l1, c1) = (line as u32 + 1, ch as u32 + 1);
                let mut chosen: Vec<usize> = vec![];
                for (sp, v) in &ck.dispatch_sites {
                    if sp.line == l1 && sp.col <= c1 && c1 < sp.col + sp.length.max(1) && !chosen.contains(v)
                        && ck.funcs[*v].display_name == ck.funcs[f].display_name
                    {
                        chosen.push(*v);
                    }
                }
                let n = ck.versions_of(f).len();
                if chosen.is_empty() {
                    return Some(format!("```fermium\n{}\n```\n{name} has {n} versions; each call uses the one its \
                                         arguments fit best", ck.describe_function(f)));
                }
                let text = chosen.iter().map(|&v| ck.describe_version(v)).collect::<Vec<_>>().join("\n");
                let what = if chosen.len() == 1 { "the version this call uses" } else { "the versions this call uses" };
                return Some(format!("```fermium\n{text}\n```\n{what} ({name} has {n} versions)"));
            }
            Binding::Func(f) => return Some(format!("```fermium\n{}\n```", ck.describe_function(f))),
            Binding::Module(m) => {
                let (mname, display, names) = ck.module_names(m)?;
                let more = if names.len() > 12 { ", …" } else { "" };
                let shown: Vec<&str> = names.iter().take(12).map(String::as_str).collect();
                return Some(format!("**{name}**: the module {mname} ({display}): {}{more}", shown.join(", ")));
            }
            Binding::Sol(s) => {
                let sv = &ck.sols[s];
                let what = if sv.n > 1 { "vector " } else { "" };
                return Some(format!("**{name}**: {what}solution of an ODE, {}; use {name}(t), {name}'(t), \
                                     max({name}), times({name})", ck.desc(&sv.dim)));
            }
            _ => {}
        }
    }
    if let Some(k) = fermium_units::constants().iter().rev().find(|k| k.name == name) {
        return Some(format!("**{name}**: {}  \n= {} {}", k.description, fmt_g(k.value, 10), k.unit.name));
    }
    if let Some(u) = fermium_units::lookup_unit(&name) {
        let base = fermium_units::preferred_unit(&u.dim);
        let dname = match &ck {
            Some(ck) => ck.desc(&DExpr::of(u.dim)),
            None => base.name.clone(),
        };
        let eq = if base.name != name { format!(", = {} {}", fmt_g(u.factor, 10), base.name) } else { String::new() };
        return Some(format!("**{name}**: unit of {dname}{eq}"));
    }
    if fermium_syntax::lexer::KEYWORDS.contains(&name.as_str()) {
        return Some(format!("**{name}**: keyword"));
    }
    None
}

/// A completion item: (label, text to insert, start column, detail).
pub type Item = (String, String, usize, String);

/// Completion items for the 0-based position (lsp.py `completions`).
pub fn completions(an: &mut Analysis, source: &str, line: usize, ch: usize) -> Vec<Item> {
    completions_in(an.checker.as_mut(), source, line, ch)
}

/// `completions` with any checker (the Jupyter kernel asks its session's).
pub fn completions_in(mut ck: Option<&mut Checker>, source: &str, line: usize, ch: usize) -> Vec<Item> {
    let cs = line_chars(source, line).unwrap_or_default();
    let text: Vec<char> = cs[..ch.min(cs.len())].to_vec();
    // \\([A-Za-z]*|\^-?\d?|_\d?)$
    if let Some(bs) = text.iter().rposition(|&c| c == '\\') {
        let rest: String = text[bs + 1..].iter().collect();
        let r: Vec<char> = rest.chars().collect();
        let ok = r.iter().all(|c| c.is_ascii_alphabetic())
            || (r.first() == Some(&'^') && {
                let t = if r.get(1) == Some(&'-') { &r[2..] } else { &r[1..] };
                t.len() <= 1 && t.iter().all(|c| c.is_ascii_digit())
            })
            || (r.first() == Some(&'_') && r.len() <= 2 && r[1..].iter().all(|c| c.is_ascii_digit()));
        if ok {
            let start = ch - (r.len() + 1);
            let mut items = fermium_syntax::symbols::sorted_names();
            items.sort();
            return items
                .into_iter()
                .filter(|(k, _)| k.starts_with(rest.as_str()))
                .map(|(k, v)| (format!("\\{k}"), v.to_string(), start, v.to_string()))
                .collect();
        }
    }
    // the word being typed: WORD matched at the start of the reversed text
    let mut prefix_len = 0;
    if text.last().is_some_and(|&c| word_start(c)) {
        prefix_len = 1;
        while prefix_len < text.len() && word_cont(text[text.len() - 1 - prefix_len]) {
            prefix_len += 1;
        }
    }
    let prefix: String = text[text.len() - prefix_len..].iter().collect();
    let start = ch.min(cs.len()) - prefix_len;
    let Some(ckr) = ck.as_deref() else {
        return keywords(&prefix, start);
    };
    let ck_ = ckr;
    if let Some(q) = name_before_dot(&text[..start]) {
        if let Some(Binding::Module(m)) = ck_.global(&q) {
            let (_, _, names) = ck_.module_names(m).unwrap_or_default();
            return names.into_iter().filter(|n| n.starts_with(&prefix)).map(|n| (n.clone(), n, start, String::new()))
                .collect();
        }
    }
    let names: Vec<String> = ck_
        .global_names()
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.starts_with(&prefix) && !n.starts_with("__") && !n.contains('\'') && !n.contains("_∂"))
        .collect();
    let mut out = vec![];
    for n in names {
        let detail = hover_in(ck.as_deref_mut(), &n, 0, 0).unwrap_or_default();
        out.push((n.clone(), n, start, detail));
    }
    out.extend(keywords(&prefix, start));
    out
}

fn keywords(prefix: &str, start: usize) -> Vec<Item> {
    if prefix.is_empty() {
        return vec![];
    }
    let mut kws: Vec<&str> = fermium_syntax::lexer::KEYWORDS.to_vec();
    kws.sort();
    kws.into_iter()
        .filter(|k| k.starts_with(prefix))
        .map(|k| (k.to_string(), k.to_string(), start, "keyword".to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nd = 15 cm\nxs = [1 m, 2 m]\np(v) = (2 kg) v\n\
                       solve x'' = -(9/s²) x with x(0) = 1 cm, x'(0) = 0 cm/s for t from 0 s to 5 s\ny = L + T\n";

    #[test]
    fn error_is_a_problem_with_a_range() {
        let an = analyze(SRC, ".");
        assert_eq!(an.problems.len(), 1);
        let p = &an.problems[0];
        assert_eq!((p.line, p.col, p.length, p.error), (8, 5, 5, true));
        assert_eq!(p.message, "can't add length [m] to time [s]");
    }

    #[test]
    fn warnings_and_clean_programs() {
        let an = analyze("c_w = 4186 J/(kg K)\nQ = c_w * 1 kg * 10 degC\n", ".");
        assert_eq!(an.problems.iter().map(|p| (p.line, p.error)).collect::<Vec<_>>(), vec![(2, false)]);
        assert!(analyze("x = 3 m\nprint x\n", ".").problems.is_empty());
    }

    #[test]
    fn hover_shows_units() {
        let mut an = analyze(SRC, ".");
        for (line, ch, want) in [(0, 0, "**L**: length [m]"), (2, 0, "**g**: acceleration [m/s²]"),
                                 (3, 0, "**d**: length [m], shown in cm"), (4, 1, "**xs**: a list of length [m]"),
                                 (2, 8, "**L**: length [m]")] {
            assert_eq!(hover_text(&mut an, SRC, line, ch).as_deref(), Some(want));
        }
        assert!(hover_text(&mut an, SRC, 5, 0).unwrap().contains("p(v) = "));
        assert!(hover_text(&mut an, SRC, 6, 6).unwrap().starts_with("**x**: solution of an ODE, length [m]"));
        let src = "E = h * 1 Hz\ny = 3 km\n";
        let mut an = analyze(src, ".");
        assert!(hover_text(&mut an, src, 0, 4).unwrap().starts_with("**h**: Planck constant"));
        assert_eq!(hover_text(&mut an, src, 1, 7).as_deref(), Some("**km**: unit of length [m], = 1000 m"));
    }

    #[test]
    fn hover_shows_the_version_a_call_chose() {
        // multiple dispatch (C5): at a call, the version its arguments picked; at the definition, all of them
        let src = "energy(m [kg], v [m/s]) = ½ m v²\nenergy(λ [m]) = h c / λ\nprint energy(500 nm)\n\
                   print energy(2 kg, 3 m/s)\n";
        let mut an = analyze(src, ".");
        assert!(an.problems.is_empty(), "{:?}", an.problems);
        let h = hover_text(&mut an, src, 2, 8).unwrap();
        assert!(h.contains("energy(λ) = h c/λ   [J, for λ in m]") && !h.contains("½") && h.contains("the version this call uses"), "{h}");
        let h = hover_text(&mut an, src, 3, 8).unwrap();
        assert!(h.contains("energy(m, v) = 0.5·m v²   [J, for m in kg, for v in m/s]") && !h.contains("λ"), "{h}");
        let h = hover_text(&mut an, src, 0, 1).unwrap();
        assert!(h.contains("0.5·m v²") && h.contains("h c/λ") && h.contains("energy has 2 versions"), "{h}");
    }

    #[test]
    fn completion() {
        let mut an = analyze("", ".");
        assert_eq!(completions(&mut an, "x = \\ome", 0, 8), vec![("\\omega".into(), "ω".into(), 4, "ω".into())]);
        let src = SRC.replace("y = L + T\n", "") + "print gg\n";
        let mut an = analyze(&src, ".");
        assert!(completions(&mut an, &src, 7, 7).iter().any(|c| c.0 == "g"));
    }

    #[test]
    fn quick_fix_for_a_unit_variable_collision() {
        let src = "g = 9.81 m/s²\nprint 20 m/s/g\n";
        let an = analyze(src, ".");
        let fx = quick_fixes(&an, src);
        assert_eq!(fx.len(), 1);
        assert!(fx[0].1.error && fx[0].1.message.contains("ambiguous"));
        assert_eq!(fx[0].2, vec![(1, 9, 1, 14, "[m/s/g]".to_string())]);
        assert!(fx[0].0.contains("[m/s/g]"));
        let src = "m = 0.5 kg\nx = 0.1 m\n";
        let an = analyze(src, ".");
        assert_eq!(quick_fixes(&an, src)[0].2, vec![(1, 8, 1, 9, "[m]".to_string())]);
    }

    #[test]
    fn g_format() {
        assert_eq!(fmt_g(1000.0, 10), "1000");
        assert_eq!(fmt_g(6.62607015e-34, 10), "6.62607015e-34");
        assert_eq!(fmt_g(299792458.0, 10), "299792458");
        assert_eq!(fmt_g(0.0001, 10), "0.0001");
        assert_eq!(fmt_g(1e10, 10), "1e+10");
        assert_eq!(fmt_g(1.0 / 3.0, 10), "0.3333333333");
    }
}
