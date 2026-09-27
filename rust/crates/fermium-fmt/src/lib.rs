//! `fermium fmt --pretty / --ascii / --fix`: a port of `fermium/fmt.py` and `cmd_fmt` of `fermium/cli.py`
//! (Fermium 1.5, the oracle).
//!
//! Works token by token, copying everything between tokens (spaces, comments) unchanged, so only spellings change,
//! never meaning. `--fix` rewrites unit/variable collisions (the A1 rule, D235) as bracketed units.

use std::collections::{HashMap, HashSet};

use fermium_syntax::lexer::{
    greek, is_vulgar_str, prepare_source, py_isalnum, py_isalpha, py_isdigit, py_isdigit_str, sup_char, tokenize,
    Kind, Token, TokValue, ExtraVal, GREEK, VULGAR_FRACS,
};
use fermium_syntax::units::lookup_unit;
use fermium_syntax::{parse_fix, parse_tokens, Diagnostic, Diagnostics, Fix};

pub mod cli;

fn kw_pretty(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "sqrt" => "√",
        "cbrt" => "∛",
        "integral" => "∫",
        "partial" => "∂",
        _ => return None,
    })
}

fn kw_ascii(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "√" => "sqrt",
        "∛" => "cbrt",
        "∫" => "integral",
        "∂" => "partial",
        _ => return None,
    })
}

fn op_pretty(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "*" => "·",
        "<=" => "≤",
        ">=" => "≥",
        "!=" => "≠",
        "+-" => "±",
        "~=" => "≈",
        _ => return None,
    })
}

fn op_ascii(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "·" => "*",
        "≤" => "<=",
        "≥" => ">=",
        "≠" => "!=",
        "±" => "+-",
        "≈" => "~=",
        "−" => "-",
        "÷" => "/",
        _ => return None,
    })
}

/// ASCII <-> pretty spellings of unit names (units.UNIT_PRETTY).
const UNIT_PRETTY: &[(&str, &str)] = &[
    ("deg", "°"), ("degC", "°C"), ("degF", "°F"), ("angstrom", "Å"), ("ohm", "Ω"), ("Msun", "M☉"), ("Rsun", "R☉"),
    ("Lsun", "L☉"),
];

/// `VULGAR_ASCII`: ½ -> (1/2).
fn vulgar_ascii(raw: &str) -> Option<String> {
    let mut it = raw.chars();
    let c = it.next()?;
    if it.next().is_some() {
        return None;
    }
    VULGAR_FRACS.iter().find(|(ch, _, _)| *ch == c).map(|(_, a, b)| format!("({a}/{b})"))
}

fn vulgar_of(a: i64, b: i64) -> Option<char> {
    VULGAR_FRACS.iter().find(|(_, x, y)| *x == a && *y == b).map(|(c, _, _)| *c)
}

fn wordy(ch: char) -> bool {
    (ch.is_ascii() && (py_isalnum(ch) || ch == '_')) || py_isalpha(ch)
}

fn sup_of(c: char) -> char {
    match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '-' => '⁻',
        '+' => '⁺',
        c => c,
    }
}

fn to_sup(n: i64) -> String {
    n.to_string().chars().map(sup_of).collect()
}

fn sub_digit(c: char) -> char {
    if c.is_ascii_digit() { char::from_u32(0x2080 + (c as u32 - '0' as u32)).unwrap() } else { c }
}

fn greek_to_ascii(letter: &str) -> Option<&'static str> {
    // {v: k for k, v in GREEK.items() if k != "inf"}: the last name of a letter wins
    let mut found = None;
    for (k, v) in GREEK {
        if *k != "inf" && *v == letter {
            found = Some(*k);
        }
    }
    found
}

fn ident_pretty(value: &str) -> String {
    let mut out: Vec<String> = vec![];
    for (i, p) in value.split('_').enumerate() {
        let p = greek(p).unwrap_or(p);
        if i > 0 && py_isdigit_str(p) && p.is_ascii() {
            let sub: String = p.chars().map(sub_digit).collect();
            let last = out.last_mut().unwrap();
            last.push_str(&sub);
            continue;
        }
        out.push(p.to_string());
    }
    out.join("_")
}

fn ident_ascii(value: &str) -> (String, bool) {
    let mut ok = true;
    let mut out = vec![];
    for p in value.split('_') {
        let mut p2 = greek_to_ascii(p).unwrap_or(p).to_string();
        if p2 == "∞" {
            p2 = "inf".into();
        }
        if !p2.is_ascii() {
            ok = false;
        }
        out.push(p2);
    }
    (out.join("_"), ok)
}

fn unit_pretty(raw: &str) -> String {
    if let Some((_, p)) = UNIT_PRETTY.iter().find(|(a, _)| *a == raw) {
        return p.to_string();
    }
    if let Some(rest) = raw.strip_prefix('u') {
        let mu = format!("μ{rest}");
        // both spellings are the same unit (the micro prefix u = μ): the factors are equal whenever both exist
        if !rest.is_empty() && lookup_unit(&mu).is_some() && raw != "u" && lookup_unit(raw).is_some() {
            return mu;
        }
    }
    raw.to_string()
}

fn unit_ascii(raw: &str) -> String {
    if let Some((a, _)) = UNIT_PRETTY.iter().find(|(_, p)| *p == raw) {
        return a.to_string();
    }
    if let Some(rest) = raw.strip_prefix('μ').or_else(|| raw.strip_prefix('µ')) {
        return format!("u{rest}");
    }
    raw.to_string()
}

fn extra_int(t: &Token, key: &str) -> Option<i64> {
    t.extra.iter().find(|(k, _)| k == key).and_then(|(_, v)| if let ExtraVal::Int(i) = v { Some(*i) } else { None })
}

fn extra_bool(t: &Token, key: &str) -> bool {
    t.extra.iter().any(|(k, v)| k == key && matches!(v, ExtraVal::Bool(true)))
}

fn warn_tok(diags: &mut Diagnostics, msg: &str, t: &Token, hint: &str) {
    diags.warn(Diagnostic {
        message: msg.into(),
        line: Some(t.line),
        col: Some(t.col),
        length: t.rawlen as u32,
        hint: Some(hint.into()),
        severity: fermium_syntax::Severity::Warning,
        fix: vec![],
    });
}

/// mode: "pretty" (ASCII -> symbols) or "ascii" (symbols -> ASCII).
pub fn format_source(source: &str, mode: &str, diags: &mut Diagnostics) -> Result<String, Diagnostic> {
    let toks = match parse_tokens(source, &[], diags) {
        Ok((_, toks)) => toks,
        Err(e) => {
            if !e.message.contains("uncertainties") {
                return Err(e);
            }
            tokenize(source, diags)?
        }
    };
    let norm: Vec<char> = prepare_source(source, None).chars().collect();
    let slice = |a: usize, b: usize| -> String { norm[a.min(norm.len())..b.min(norm.len()).max(a.min(norm.len()))].iter().collect() };
    let pretty = mode == "pretty";
    let mut out: Vec<String> = vec![];
    let mut pos = 0usize;
    let mut closers: HashMap<usize, String> = HashMap::new();
    let mut last_changed = false;
    let mut skip: HashSet<usize> = HashSet::new();
    let n = toks.len();
    let mut i = 0;
    while i < n {
        let t = &toks[i];
        if matches!(t.kind, Kind::Newline | Kind::Indent | Kind::Dedent | Kind::Eof) {
            i += 1;
            continue;
        }
        let mut gap = slice(pos, t.start);
        if pretty && gap == " " && t.kind == Kind::Name && i >= 2 && toks[i - 1].raw == "partial" && toks[i - 2].raw == "/"
        {
            gap = String::new();
        }
        out.push(gap.clone());
        pos = t.end;
        if skip.contains(&i) {
            i += 1;
            continue;
        }
        if pretty {
            if let Some(glyph) = vulgar_at(&toks, i) {
                out.push(glyph.to_string());
                pos = toks[i + 4].end;
                last_changed = true;
                i += 5;
                continue;
            }
        }
        let text: String;
        if pretty && i >= 1 && toks[i - 1].kind == Kind::Op && toks[i - 1].raw == "." && !t.ws_before {
            text = t.raw.clone();
        } else if pretty {
            text = pretty_token(&toks, i, &mut skip);
        } else if t.kind == Kind::Kw && t.s() == "nabla" {
            text = nabla_ascii(&toks, i, &mut skip);
        } else if t.kind == Kind::Name && t.s() == "𝑖" && imag_literal_ok(&toks, i) {
            *out.last_mut().unwrap() = String::new();
            text = "i".into();
        } else {
            text = ascii_token(&toks, i, &mut closers, diags);
        }
        let prev = out.iter().rev().find_map(|s| s.chars().last());
        if gap.is_empty() {
            if let (Some(p), Some(t0)) = (prev, text.chars().next()) {
                if (text != t.raw || last_changed) && wordy(p) && wordy(t0) && !(py_isdigit(p) && !py_isdigit(t0)) {
                    out.push(" ".into());
                }
            }
        }
        last_changed = text != t.raw;
        out.push(text);
        if let Some(c) = closers.remove(&i) {
            out.push(c);
        }
        i += 1;
    }
    out.push(slice(pos, norm.len()));
    Ok(out.concat())
}

/// The glyph for `(a/b)` starting at token i (`(1/2)` -> ½), or None where the glyph could read differently (D241).
fn vulgar_at(toks: &[Token], i: usize) -> Option<char> {
    if i + 4 >= toks.len() {
        return None;
    }
    let (lp, a, sl, b, rp) = (&toks[i], &toks[i + 1], &toks[i + 2], &toks[i + 3], &toks[i + 4]);
    if !(lp.kind == Kind::Op && lp.raw == "(" && a.kind == Kind::Num && py_isdigit_str(&a.raw) && sl.kind == Kind::Op
        && sl.raw == "/" && b.kind == Kind::Num && py_isdigit_str(&b.raw) && rp.kind == Kind::Op && rp.raw == ")")
    {
        return None;
    }
    let (Ok(av), Ok(bv)) = (a.raw.parse::<i64>(), b.raw.parse::<i64>()) else { return None };
    let glyph = vulgar_of(av, bv)?;
    if a.raw != av.to_string() || b.raw != bv.to_string() {
        return None;
    }
    if i >= 1 {
        let prev = &toks[i - 1];
        if prev.kind == Kind::Op && ["^", "**", "."].contains(&prev.raw.as_str()) {
            return None;
        }
        if prev.kind == Kind::Op && prev.raw == "-" && i >= 2 && toks[i - 2].kind == Kind::Op && toks[i - 2].raw == "^" {
            return None;
        }
        if !lp.ws_before
            && (matches!(prev.kind, Kind::Name | Kind::Num | Kind::Imag | Kind::Prime | Kind::Sup | Kind::Str)
                || (prev.kind == Kind::Op && [")", "]", "}", "|"].contains(&prev.raw.as_str())))
        {
            return None;
        }
    }
    if let Some(nxt) = toks.get(i + 5) {
        if !nxt.ws_before && matches!(nxt.kind, Kind::Num | Kind::Imag) {
            return None;
        }
    }
    Some(glyph)
}

fn pretty_token(toks: &[Token], i: usize, skip: &mut HashSet<usize>) -> String {
    let t = &toks[i];
    match t.kind {
        Kind::Name => {
            if t.role == "unit" {
                return unit_pretty(&t.raw);
            }
            if t.s() == "∞" && (t.raw == "inf" || t.raw == "infinity") {
                return "∞".into();
            }
            if t.raw.is_ascii() { ident_pretty(t.s()) } else { t.raw.clone() }
        }
        Kind::Kw => kw_pretty(&t.raw).map(|s| s.to_string()).unwrap_or_else(|| t.raw.clone()),
        Kind::Op => {
            if t.raw == "^" {
                let mut j = i + 1;
                let mut sign = "";
                if toks[j].kind == Kind::Op && toks[j].raw == "-" {
                    sign = "-";
                    j += 1;
                }
                let nt = &toks[j.min(toks.len() - 1)];
                let after = toks.get(j + 1);
                if nt.kind == Kind::Num
                    && py_isdigit_str(&nt.raw)
                    && !after.is_some_and(|a| a.kind == Kind::Op && a.raw == "^")
                    && !after.is_some_and(|a| a.kind == Kind::Num && !a.ws_before)
                {
                    if let Ok(v) = format!("{sign}{}", nt.raw).parse::<i64>() {
                        for k in i + 1..=j {
                            skip.insert(k);
                        }
                        return to_sup(v);
                    }
                }
                return t.raw.clone();
            }
            // `1.07 fm * A^(1/3)`: after a unit, `·` joins a following unit name into the unit (`fm·A`), where `*`
            // ends it, so the `*` stays (red team 16, D334)
            if t.raw == "*" && i >= 1 && toks[i - 1].role == "unit"
                && toks.get(i + 1).is_some_and(|n| n.kind == Kind::Name && fermium_syntax::units::is_unit_name(n.s()))
            {
                return t.raw.clone();
            }
            op_pretty(&t.raw).map(|s| s.to_string()).unwrap_or_else(|| t.raw.clone())
        }
        _ => t.raw.clone(),
    }
}

/// ∇f, ∇·F, ∇×F, ∇²f -> grad(f), div(F), curl(F), laplacian(f).
fn nabla_ascii(toks: &[Token], i: usize, skip: &mut HashSet<usize>) -> String {
    let mut j = i + 1;
    let mut word = "grad";
    let t = &toks[j];
    if t.kind == Kind::Sup && t.int() == 2 {
        word = "laplacian";
        j += 1;
    } else if t.kind == Kind::Op && t.raw == "^" && toks[j + 1].kind == Kind::Num && toks[j + 1].f() == 2.0 {
        word = "laplacian";
        j += 2;
    } else if t.kind == Kind::Op && t.s() == "*" {
        word = "div";
        j += 1;
    } else if t.kind == Kind::Op && t.raw == "×" {
        word = "curl";
        j += 1;
    }
    if toks[j].kind != Kind::Name {
        return "nabla".into();
    }
    for k in i + 1..=j {
        skip.insert(k);
    }
    let (name, ok) = if toks[j].raw.is_ascii() { (toks[j].raw.clone(), true) } else { ident_ascii(toks[j].s()) };
    format!("{word}({})", if ok { name } else { toks[j].raw.clone() })
}

/// Can `2𝑖` be written as the literal `2i`?
fn imag_literal_ok(toks: &[Token], i: usize) -> bool {
    if i < 1 || toks[i - 1].kind != Kind::Num || !toks[i - 1].raw.chars().all(|c| py_isdigit(c) || c == '.') {
        return false;
    }
    if i >= 2 && toks[i - 2].kind == Kind::Op && ["^", "**"].contains(&toks[i - 2].raw.as_str()) {
        return false;
    }
    !toks.get(i + 1).is_some_and(|n| matches!(n.kind, Kind::Name | Kind::Num))
}

fn ascii_token(toks: &[Token], i: usize, closers: &mut HashMap<usize, String>, diags: &mut Diagnostics) -> String {
    let t = &toks[i];
    match t.kind {
        Kind::Name => {
            if t.role == "unit" {
                return unit_ascii(&t.raw);
            }
            if t.raw.is_ascii() {
                return t.raw.clone();
            }
            if t.s() == "∞" {
                return "inf".into();
            }
            if t.s() == "𝑖" {
                let nxt = toks.get(i + 1);
                return if nxt.is_some_and(|n| n.kind == Kind::Name) { "(1i)".into() } else { "1i".into() };
            }
            let (text, ok) = ident_ascii(t.s());
            if !ok {
                warn_tok(diags, &format!("'{}' has no plain-ASCII spelling, so it was left as is", t.raw), t,
                         "rename it (e.g. ΔE -> Delta_E) if you need pure ASCII");
                return t.raw.clone();
            }
            // `25 ħ / √(2μ)`: after a number (or a unit) the ASCII spelling `hbar` would be read as a unit (and take
            // the `/ sqrt` into it), so the multiplication is written out (red team 16, D334)
            if i >= 1 && (toks[i - 1].kind == Kind::Num || toks[i - 1].role == "unit")
                && fermium_syntax::units::is_unit_name(&text)
            {
                return if t.ws_before { format!("* {text}") } else { format!("*{text}") };
            }
            text
        }
        Kind::Kw => {
            if t.raw == "√" || t.raw == "∛" {
                let word = kw_ascii(&t.raw).unwrap();
                let end = extra_int(t, "operand_end");
                let nxt = &toks[i + 1];
                if nxt.kind == Kind::Op && nxt.raw == "(" && extra_bool(t, "operand_paren") {
                    return word.into();
                }
                if nxt.kind == Kind::Num && is_vulgar_str(&nxt.raw) && end == Some(i as i64 + 1) {
                    return word.into();
                }
                if let Some(end) = end {
                    closers.entry(end as usize).or_default().push(')');
                    return format!("{word}(");
                }
                return word.into();
            }
            kw_ascii(&t.raw).map(|s| s.to_string()).unwrap_or_else(|| t.raw.clone())
        }
        Kind::Op => {
            if t.raw == "×" {
                warn_tok(diags, "× (cross product) has no ASCII operator, so it was left as is", t,
                         "write cross(a, b) if you need pure ASCII");
            }
            if t.raw == "ᵀ" {
                warn_tok(diags, "ᵀ (transpose) has no ASCII operator, so it was left as is", t,
                         "write transpose(M) if you need pure ASCII");
            }
            op_ascii(&t.raw).map(|s| s.to_string()).unwrap_or_else(|| t.raw.clone())
        }
        Kind::Sup => format!("^{}", t.int()),
        Kind::Num => {
            let raw = &t.raw;
            if let Some(va) = vulgar_ascii(raw) {
                if i >= 1 {
                    let prev = &toks[i - 1];
                    if !t.ws_before
                        && (matches!(prev.kind, Kind::Name | Kind::Num | Kind::Imag | Kind::Prime | Kind::Sup)
                            || (prev.kind == Kind::Op && [")", "]", "}"].contains(&prev.raw.as_str())))
                    {
                        return format!(" {va}");
                    }
                }
                return va;
            }
            if let Some((mant, ex)) = raw.split_once("×10") {
                let ex = ex.trim_start_matches('^');
                let ex: String = ex.chars().map(|c| sup_char(c).unwrap_or(c)).collect();
                return format!("{mant}e{ex}");
            }
            raw.clone()
        }
        _ => {
            let _ = TokValue::None;
            t.raw.clone()
        }
    }
}

/// `fermium fmt --fix`: rewrite every unit/variable collision (the A1 rule, D235) as a bracketed unit that keeps what
/// Fermium 1 did there. Returns (new source, number of edits, the error still left or None); an error when the
/// program can't be fixed at all.
pub fn fix_source_report(source: &str, rounds: usize) -> Result<(String, usize, Option<Diagnostic>), Diagnostic> {
    let mut source = source.to_string();
    let mut total = 0;
    for _ in 0..rounds {
        let mut d = Diagnostics::new();
        tokenize(&source, &mut d)?;
        let (fixes, err) = parse_fix(&source);
        if fixes.is_empty() {
            if let Some(e) = err {
                if total == 0 {
                    return Err(e);
                }
            }
            let left = fermium_syntax::parse(&source, &[]).err();
            return Ok((source, total, left));
        }
        let norm: Vec<char> = prepare_source(&source, None).chars().collect();
        let orig: Vec<char> = source.chars().collect();
        let mut text = if norm.len() == orig.len() { orig } else { norm };
        let mut fixes: Vec<Fix> = fixes;
        fixes.sort();
        fixes.reverse();
        for (a, b, rep) in &fixes {
            let (a, b) = ((*a).min(text.len()), (*b).min(text.len()).max((*a).min(text.len())));
            text.splice(a..b, rep.chars());
        }
        source = text.into_iter().collect();
        total += fixes.len();
    }
    Ok((source, total, None))
}

/// `fix_source`: the fixed source and the number of edits.
pub fn fix_source(source: &str) -> Result<(String, usize), Diagnostic> {
    fix_source_report(source, 20).map(|(s, n, _)| (s, n))
}
