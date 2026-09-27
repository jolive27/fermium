//! `fermium fmt --pretty` and `--ascii` change only spellings, never meaning (red team 16 #5, D334): every program of
//! research/, examples/ and rust/c-cases/ parses to the same tree after formatting as before (spans and the
//! spelling-only attributes aside), and formatting back gives the same tree again.
use fermium_fmt::format_source;
use fermium_syntax::Diagnostics;
use std::path::{Path, PathBuf};

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut out = vec![];
    fn walk(d: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth > 0 {
                walk(&p, depth - 1, out);
            } else if p.extension().is_some_and(|x| x == "fm") {
                out.push(p);
            }
        }
    }
    walk(&root.join("research"), 1, &mut out);
    walk(&root.join("examples"), 0, &mut out);
    walk(&root.join("rust/c-cases"), 2, &mut out);
    out.sort();
    out
}

/// The tree without positions: the s-expression of every statement with spans, node ids and the spelling-only
/// facts removed.
fn shape(src: &str) -> Option<String> {
    // a fraction glyph is the exact number (a/b) (D241): both sides are read with the bracket spelling
    let mut src = src.to_string();
    for (g, f) in [('½', "(1/2)"), ('⅓', "(1/3)"), ('⅔', "(2/3)"), ('¼', "(1/4)"), ('¾', "(3/4)"), ('⅕', "(1/5)"),
                   ('⅖', "(2/5)"), ('⅗', "(3/5)"), ('⅘', "(4/5)"), ('⅙', "(1/6)"), ('⅚', "(5/6)"), ('⅛', "(1/8)"),
                   ('⅜', "(3/8)"), ('⅝', "(5/8)"), ('⅞', "(7/8)"), ('⅐', "(1/7)"), ('⅑', "(1/9)"), ('⅒', "(1/10)")] {
        src = src.replace(g, &format!("{f} "));
    }
    let (p, _) = fermium_syntax::parse(&src, &[]).ok()?;
    let s = fermium_syntax::sexpr::program(&p);
    // drop the positions (`Class@line:col+length`-style markers after a node's class)
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if (c == '@' || c == '+') && i > 0 && cs[i - 1].is_ascii_alphanumeric() && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            i += 1;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == ':') {
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    // drop what only records the spelling: `#fact=…` attributes, digit/paren flags, a unit's source text; a unit
    // factor's name by what it names (`deg` and `°`, `uF` and `μF` are one unit)
    let words: Vec<String> = out
        .split_whitespace()
        .filter(|w| !w.starts_with('#') && !w.starts_with("digit=") && *w != "paren" && !w.starts_with("text=")
                    && !w.starts_with("(\"") && !w.starts_with("implicit="))
        .map(|w| {
            let w = w.trim_end_matches(')');
            match w.strip_prefix("name=\"").and_then(|r| r.strip_suffix('"')) {
                Some(n) => format!("name={}", canon_unit(n)),
                None => w.replace('²', "^2").replace('³', "^3").replace("⁻¹", "^-1"),
            }
        })
        .collect();
    Some(words.join(" "))
}

fn canon_unit(n: &str) -> String {
    n.replace('μ', "u").replace('°', "deg").replace('Ω', "ohm").replace('Å', "angstrom").replace('☉', "sun")
}

#[test]
fn pretty_and_ascii_keep_every_corpus_programs_meaning() {
    let files = corpus();
    assert!(files.len() > 20, "corpus not found: {files:?}");
    let mut bad = vec![];
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let Some(orig) = shape(&src) else { continue }; // programs that don't parse on their own (fragments)
        for mode in ["pretty", "ascii"] {
            let Ok(out) = format_source(&src, mode, &mut Diagnostics::new()) else {
                bad.push(format!("{}: fmt --{mode} failed", f.display()));
                continue;
            };
            let new = shape(&out).unwrap_or_default();
            if new != orig {
                let (a, b): (Vec<&str>, Vec<&str>) = (orig.split(' ').collect(), new.split(' ').collect());
                let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                let at = Some(format!("{}\n  vs\n{}", a[k.saturating_sub(4)..(k + 4).min(a.len())].join(" "),
                                      b[k.saturating_sub(4)..(k + 4).min(b.len())].join(" ")));
                bad.push(format!("{}: fmt --{mode} changes the tree: {}", f.display(), at.unwrap_or_default()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn a_times_before_a_unit_name_after_a_unit_stays() {
    let src = "A = 8\nr = 1.07 fm * A^(1/3)\n";
    let out = format_source(src, "pretty", &mut Diagnostics::new()).unwrap();
    assert_eq!(out, "A = 8\nr = 1.07 fm * A^(1/3)\n");
    assert_eq!(shape(&out), shape(src));
    // where nothing merges, the centred dot is still used
    let out = format_source("x = 2\ny = 3 m * x\n", "pretty", &mut Diagnostics::new()).unwrap();
    assert_eq!(out, "x = 2\ny = 3 m · x\n");
}
