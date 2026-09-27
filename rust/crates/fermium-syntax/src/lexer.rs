//! Lexer: source text -> tokens. A port of `fermium/lexer.py` (Fermium 1.5, the oracle).
//!
//! Handles Unicode/ASCII equivalence (θ == theta), subscripts (ε₀ == ε_0), superscript exponents (x² == x^2),
//! look-alike characters, significant indentation, and scientific notation written the physics way (6.67×10⁻¹¹).
//! Offsets and columns count code points, as Python's do.

use crate::diag::{Diagnostic, Diagnostics};
use crate::pyfmt::py_int;
use crate::tables;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Num,
    Imag,
    Name,
    Kw,
    Str,
    Op,
    Sup,
    Prime,
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Num => "NUM",
            Kind::Imag => "IMAG",
            Kind::Name => "NAME",
            Kind::Kw => "KW",
            Kind::Str => "STR",
            Kind::Op => "OP",
            Kind::Sup => "SUP",
            Kind::Prime => "PRIME",
            Kind::Newline => "NEWLINE",
            Kind::Indent => "INDENT",
            Kind::Dedent => "DEDENT",
            Kind::Eof => "EOF",
        }
    }
}

/// A token's canonical value: a float (NUM, IMAG), a whole number (SUP, PRIME, INDENT, DEDENT), text (the
/// canonical name, keyword, operator or string), or nothing (EOF).
#[derive(Clone, Debug, PartialEq)]
pub enum TokValue {
    None,
    Num(f64),
    Int(i64),
    Str(String),
}

/// A value in a token's `extra` (the parser records facts for the formatter there).
#[derive(Clone, Debug, PartialEq)]
pub enum ExtraVal {
    Int(i64),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub value: TokValue,
    /// Exact source text (of the normalised source).
    pub raw: String,
    pub line: u32,
    pub col: u32,
    /// Offsets in the normalised source, in code points.
    pub start: usize,
    pub end: usize,
    pub ws_before: bool,
    /// For NUM: significant figures, None = exact.
    pub sigfigs: Option<u32>,
    /// NUM written with decimal digits (units may follow).
    pub digit: bool,
    /// Set by the parser (e.g. "unit"), used by fmt.
    pub role: String,
    pub extra: Vec<(String, ExtraVal)>,
    /// In a solve: a unit name written with a prime is the unknown (D211).
    pub unknown_prime: bool,
    /// Length of `raw` in code points.
    pub rawlen: usize,
}

impl Token {
    /// The text value ("" when the value isn't text).
    pub fn s(&self) -> &str {
        match &self.value {
            TokValue::Str(s) => s,
            _ => "",
        }
    }

    /// The numeric value (floats and whole numbers), NaN for text.
    pub fn f(&self) -> f64 {
        match &self.value {
            TokValue::Num(x) => *x,
            TokValue::Int(i) => *i as f64,
            _ => f64::NAN,
        }
    }

    pub fn int(&self) -> i64 {
        match &self.value {
            TokValue::Int(i) => *i,
            TokValue::Num(x) => *x as i64,
            _ => 0,
        }
    }

    pub fn is_op(&self, v: &str) -> bool {
        self.kind == Kind::Op && self.s() == v
    }

    pub fn is_kw(&self, v: &str) -> bool {
        self.kind == Kind::Kw && self.s() == v
    }

    pub fn is_name(&self, v: &str) -> bool {
        self.kind == Kind::Name && self.s() == v
    }

    pub fn set_extra(&mut self, key: &str, v: ExtraVal) {
        if let Some(e) = self.extra.iter_mut().find(|(k, _)| k == key) {
            e.1 = v;
        } else {
            self.extra.push((key.to_string(), v));
        }
    }
}

pub const KEYWORDS: &[&str] = &[
    "if", "else", "elif", "then", "for", "from", "to", "step", "in", "while", "return", "break", "continue", "print",
    "plot", "vs", "solve", "with", "fit", "load", "and", "or", "not", "where", "true", "false", "integral", "partial",
    "sqrt", "cbrt", "assert", "nabla",
];

/// Symbols that are spelled differently but mean the same keyword.
fn keyword_alias(c: char) -> Option<&'static str> {
    Some(match c {
        '∫' => "integral",
        '∂' => "partial",
        '√' => "sqrt",
        '∛' => "cbrt",
        '∇' => "nabla",
        _ => return None,
    })
}

pub const GREEK: &[(&str, &str)] = &[
    ("alpha", "α"), ("beta", "β"), ("gamma", "γ"), ("delta", "δ"), ("epsilon", "ε"), ("zeta", "ζ"), ("eta", "η"),
    ("theta", "θ"), ("iota", "ι"), ("kappa", "κ"), ("lambda", "λ"), ("mu", "μ"), ("nu", "ν"), ("xi", "ξ"),
    ("pi", "π"), ("rho", "ρ"), ("sigma", "σ"), ("tau", "τ"), ("upsilon", "υ"), ("phi", "φ"), ("chi", "χ"),
    ("psi", "ψ"), ("omega", "ω"), ("Gamma", "Γ"), ("Delta", "Δ"), ("Theta", "Θ"), ("Lambda", "Λ"), ("Xi", "Ξ"),
    ("Pi", "Π"), ("Sigma", "Σ"), ("Upsilon", "Υ"), ("Phi", "Φ"), ("Psi", "Ψ"), ("Omega", "Ω"), ("hbar", "ħ"),
    ("infinity", "∞"), ("inf", "∞"),
];

/// The Greek letter an ASCII name stands for (`theta` -> `θ`).
pub fn greek(name: &str) -> Option<&'static str> {
    GREEK.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Characters that are the same letter written with a different code point (v1's NORMALIZE_CHARS). The
/// three signs are written as escapes: an editor or tool that NFC-normalises this file turns the characters
/// into their targets, which is how the port lost them (red team 13 #4: `5 \u{2126}` was 'isn't a unit').
fn normalize_char(c: char) -> Option<char> {
    Some(match c {
        'µ' => 'μ',  // micro sign -> Greek mu
        '\u{2126}' => '\u{3a9}', // OHM SIGN -> Greek capital omega
        '\u{212a}' => 'K',        // KELVIN SIGN -> K
        '\u{212b}' => '\u{c5}',  // ANGSTROM SIGN -> Å (A with ring above)
        'ϵ' => 'ε',
        'ϕ' => 'φ',
        'ϑ' => 'θ',
        'ℏ' => 'ħ',
        _ => return None,
    })
}

/// Characters that look identical to Latin letters; these are replaced (with a warning): (char, Latin, name).
pub const LOOKALIKE_REPLACE: &[(char, char, &str)] = &[
    ('Α', 'A', "Greek Capital Letter Alpha"), ('Β', 'B', "Greek Capital Letter Beta"),
    ('Ε', 'E', "Greek Capital Letter Epsilon"), ('Ζ', 'Z', "Greek Capital Letter Zeta"),
    ('Η', 'H', "Greek Capital Letter Eta"), ('Ι', 'I', "Greek Capital Letter Iota"),
    ('Κ', 'K', "Greek Capital Letter Kappa"), ('Μ', 'M', "Greek Capital Letter Mu"),
    ('Ν', 'N', "Greek Capital Letter Nu"), ('Ο', 'O', "Greek Capital Letter Omicron"),
    ('Ρ', 'P', "Greek Capital Letter Rho"), ('Τ', 'T', "Greek Capital Letter Tau"),
    ('Υ', 'Y', "Greek Capital Letter Upsilon"), ('Χ', 'X', "Greek Capital Letter Chi"),
    ('а', 'a', "Cyrillic Small Letter A"), ('е', 'e', "Cyrillic Small Letter Ie"),
    ('о', 'o', "Cyrillic Small Letter O"), ('р', 'p', "Cyrillic Small Letter Er"),
    ('с', 'c', "Cyrillic Small Letter Es"), ('у', 'y', "Cyrillic Small Letter U"),
    ('х', 'x', "Cyrillic Small Letter Ha"), ('і', 'i', "Cyrillic Small Letter Byelorussian-Ukrainian I"),
    ('А', 'A', "Cyrillic Capital Letter A"), ('В', 'B', "Cyrillic Capital Letter Ve"),
    ('Е', 'E', "Cyrillic Capital Letter Ie"), ('К', 'K', "Cyrillic Capital Letter Ka"),
    ('М', 'M', "Cyrillic Capital Letter Em"), ('Н', 'H', "Cyrillic Capital Letter En"),
    ('О', 'O', "Cyrillic Capital Letter O"), ('Р', 'P', "Cyrillic Capital Letter Er"),
    ('С', 'C', "Cyrillic Capital Letter Es"), ('Т', 'T', "Cyrillic Capital Letter Te"),
    ('Х', 'X', "Cyrillic Capital Letter Ha"),
];

/// Letters that look like others but are legitimately different in physics (ν vs v).
fn skeleton_char(c: char) -> char {
    match c {
        'ν' => 'v',
        'ο' => 'o',
        'ρ' => 'p',
        'ι' => 'i',
        'κ' => 'k',
        'χ' => 'x',
        'ϰ' => 'k',
        'ɡ' => 'g',
        'ⅼ' => 'l',
        'ı' => 'i',
        c => c,
    }
}

fn punct_normalize(c: char) -> Option<char> {
    Some(match c {
        '−' => '-',
        '–' => '-',
        '÷' => '/',
        '′' => '\'',
        '“' => '"',
        '”' => '"',
        '’' => '\'',
        '⋅' => '·',
        '∙' => '·',
        '∗' => '*',
        _ => return None,
    })
}

pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Superscript characters and the ASCII they stand for.
pub fn sup_char(c: char) -> Option<char> {
    Some(match c {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁻' => '-',
        '⁺' => '+',
        _ => return None,
    })
}

pub fn sub_char(c: char) -> Option<char> {
    let d = c as u32;
    if (0x2080..=0x2089).contains(&d) {
        Some(char::from_u32('0' as u32 + d - 0x2080).unwrap())
    } else {
        None
    }
}

/// Every Unicode vulgar fraction (spec A4.2).
pub const VULGAR_FRACS: &[(char, i64, i64)] = &[
    ('½', 1, 2), ('⅓', 1, 3), ('⅔', 2, 3), ('¼', 1, 4), ('¾', 3, 4), ('⅕', 1, 5), ('⅖', 2, 5), ('⅗', 3, 5),
    ('⅘', 4, 5), ('⅙', 1, 6), ('⅚', 5, 6), ('⅐', 1, 7), ('⅛', 1, 8), ('⅜', 3, 8), ('⅝', 5, 8), ('⅞', 7, 8),
    ('⅑', 1, 9), ('⅒', 1, 10),
];

pub fn vulgar(c: char) -> Option<f64> {
    VULGAR_FRACS.iter().find(|(ch, _, _)| *ch == c).map(|(_, a, b)| *a as f64 / *b as f64)
}

/// Is this text one vulgar fraction character (Python `raw in VULGAR`)?
pub fn is_vulgar_str(s: &str) -> bool {
    let mut it = s.chars();
    matches!((it.next(), it.next()), (Some(c), None) if vulgar(c).is_some())
}

/// Multi-char operators, longest first.
pub const OPERATORS: &[&str] = &[
    "+-", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "~=", "+", "-", "*", "/", "^", "(", ")", "[", "]", "{",
    "}", ",", "=", "<", ">", ".", ":", "·", "×", "≤", "≥", "≠", "±", "≈", "|", ";", "ᵀ",
];

fn op_canon(op: &str) -> &str {
    match op {
        "·" => "*",
        "≤" => "<=",
        "≥" => ">=",
        "≠" => "!=",
        "±" => "+-",
        "≈" => "~=",
        o => o,
    }
}

fn special_standalone(c: char) -> bool {
    matches!(c, 'π' | '∞' | '𝑖')
}

fn in_ranges(c: char, rr: &[(u32, u32)]) -> bool {
    let x = c as u32;
    rr.binary_search_by(|&(a, b)| {
        if x < a {
            std::cmp::Ordering::Greater
        } else if x > b {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// Python's `str.isalpha()` for one character.
pub fn py_isalpha(c: char) -> bool {
    in_ranges(c, tables::PY_ALPHA)
}

/// Python's `str.isalnum()` for one character.
pub fn py_isalnum(c: char) -> bool {
    in_ranges(c, tables::PY_ALNUM)
}

/// Python's `str.isdigit()` for one character (superscripts count).
pub fn py_isdigit(c: char) -> bool {
    in_ranges(c, tables::PY_DIGIT)
}

/// Python's `s.isdigit()` for a string: not empty, every character a digit.
pub fn py_isdigit_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_isdigit)
}

/// `unicodedata.name(ch, "unknown character").title()` for a character that isn't a letter.
pub fn char_name_title(c: char) -> String {
    let x = c as u32;
    match crate::char_names::CHAR_NAMES.binary_search_by(|(k, _)| k.cmp(&x)) {
        Ok(i) => crate::char_names::CHAR_NAMES[i].1.to_string(),
        Err(_) => "Unknown Character".to_string(),
    }
}

/// Canonical spelling of an identifier: Greek names -> letters, subscripts -> _N.
pub fn canonical_name(raw: &str) -> String {
    raw.split('_').map(|p| greek(p).unwrap_or(p)).collect::<Vec<_>>().join("_")
}

fn count_sigfigs(mantissa: &str) -> Option<u32> {
    if !mantissa.contains('.') {
        return None;
    }
    let digits: String = mantissa.chars().filter(|c| *c != '.' && *c != '_').collect();
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some(1);
    }
    Some(digits.chars().count() as u32)
}

/// Replace look-alike characters (never inside strings). Keeps the length identical.
pub fn normalize_source(src: &str, diags: Option<&mut Diagnostics>) -> String {
    let mut diags = diags;
    let mut out = String::with_capacity(src.len());
    let (mut line, mut col) = (1u32, 1u32);
    let mut in_str = false;
    for ch in src.chars() {
        let mut ch = ch;
        if ch == '"' {
            in_str = !in_str;
        } else if ch == '\n' {
            in_str = false;
        }
        if in_str && ch != '"' {
            out.push(ch);
            col += 1;
            continue;
        }
        if let Some(c) = normalize_char(ch) {
            ch = c;
        } else if let Some(c) = punct_normalize(ch) {
            ch = c;
        } else if let Some(&(_, latin, name)) = LOOKALIKE_REPLACE.iter().find(|(c, _, _)| *c == ch) {
            if let Some(d) = diags.as_deref_mut() {
                d.warn_at(format!("replaced look-alike character '{ch}' ({name}) with Latin '{latin}'"), line, col, 1,
                          None);
            }
            ch = latin;
        }
        out.push(ch);
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    out
}

fn is_ident_start(c: char) -> bool {
    (py_isalpha(c) && c != 'ᵀ') || matches!(c, '_' | '°' | '%') || c == 'ħ'
}

pub fn is_ident_char(c: char) -> bool {
    (py_isalnum(c) && sup_char(c).is_none() && vulgar(c).is_none() && c != 'ᵀ')
        || matches!(c, '_' | '☉')
        || sub_char(c).is_some()
}

pub struct Lexer<'d> {
    pub diags: &'d mut Diagnostics,
    /// The normalised source.
    pub src: Vec<char>,
    pub pos: usize,
    pub line: u32,
    pub col: u32,
    pub tokens: Vec<Token>,
    paren: i64,
    indents: Vec<i64>,
}

/// The source as the lexer sees it: newlines normalised, BOM removed, ″ spelled '', look-alikes replaced.
pub fn prepare_source(source: &str, diags: Option<&mut Diagnostics>) -> String {
    let src = source.replace("\r\n", "\n").replace('\r', "\n");
    let src = src.trim_start_matches('\u{feff}');
    let src = src.replace('″', "''");
    normalize_source(&src, diags)
}

impl<'d> Lexer<'d> {
    pub fn new(source: &str, diags: &'d mut Diagnostics) -> Self {
        let src = prepare_source(source, Some(diags));
        Lexer { diags, src: src.chars().collect(), pos: 0, line: 1, col: 1, tokens: vec![], paren: 0, indents: vec![0] }
    }

    fn error(&self, msg: impl Into<String>, hint: Option<&str>) -> Diagnostic {
        Diagnostic::error(msg, self.line, self.col, 1, hint.map(|s| s.to_string()))
    }

    fn adv(&mut self, n: usize) {
        for _ in 0..n {
            if self.pos < self.src.len() {
                if self.src[self.pos] == '\n' {
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.col += 1;
                }
                self.pos += 1;
            }
        }
    }

    /// The character k ahead, or None at the end (Python's "").
    fn peek(&self, k: usize) -> Option<char> {
        self.src.get(self.pos + k).copied()
    }

    fn text(&self, a: usize, b: usize) -> String {
        self.src[a.min(self.src.len())..b.min(self.src.len())].iter().collect()
    }

    fn starts_with(&self, p: usize, s: &str) -> bool {
        let mut q = p;
        for c in s.chars() {
            if self.src.get(q) != Some(&c) {
                return false;
            }
            q += 1;
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, kind: Kind, value: TokValue, start: usize, line: u32, col: u32, ws: bool, sigfigs: Option<u32>,
           digit: bool) {
        let raw = self.text(start, self.pos);
        let rawlen = self.pos.saturating_sub(start);
        self.tokens.push(Token { kind, value, raw, line, col, start, end: self.pos, ws_before: ws, sigfigs, digit,
                                 role: String::new(), extra: vec![], unknown_prime: false, rawlen });
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut at_line_start = true;
        let mut ws = false;
        while self.pos < self.src.len() {
            let ch = self.src[self.pos];
            if at_line_start {
                at_line_start = false;
                let mut width = 0i64;
                let mut p = self.pos;
                while p < self.src.len() && (self.src[p] == ' ' || self.src[p] == '\t') {
                    width += if self.src[p] == '\t' { 4 } else { 1 };
                    p += 1;
                }
                let rest = if p < self.src.len() { self.src[p] } else { '\n' };
                if rest == '\n' || rest == '#' || self.paren > 0 {
                    self.adv(p - self.pos);
                    ws = true;
                    continue;
                }
                self.adv(p - self.pos);
                if width > *self.indents.last().unwrap() && self.else_continues() {
                    // `u(t) = if t < t1 then a` + an indented `else ...` line: one expression (#53)
                    self.tokens.pop();
                    ws = true;
                    continue;
                }
                if width > *self.indents.last().unwrap() {
                    self.indents.push(width);
                    self.add(Kind::Indent, TokValue::Int(width), self.pos, self.line, self.col, true, None, false);
                } else {
                    while width < *self.indents.last().unwrap() {
                        self.indents.pop();
                        self.add(Kind::Dedent, TokValue::Int(width), self.pos, self.line, self.col, true, None, false);
                    }
                    if width > *self.indents.last().unwrap() && self.continuation_word() {
                        self.indents.push(width);
                        self.add(Kind::Indent, TokValue::Int(width), self.pos, self.line, self.col, true, None, false);
                    } else if width != *self.indents.last().unwrap() {
                        return Err(self.error("this line's indentation doesn't match any block above it",
                                              Some("line up the start of the line with the lines above it")));
                    }
                }
                ws = true;
                continue;
            }
            if ch == ' ' || ch == '\t' {
                self.adv(1);
                ws = true;
                continue;
            }
            if ch == '\\' && self.peek(1) == Some('\n') {
                self.adv(2);
                ws = true;
                continue;
            }
            if ch == '#' {
                while self.pos < self.src.len() && self.peek(0) != Some('\n') {
                    self.adv(1);
                }
                continue;
            }
            if ch == '\n' {
                if self.paren == 0 && !self.continues() {
                    if self.tokens.last().is_some_and(|t| t.kind != Kind::Newline) {
                        self.add(Kind::Newline, TokValue::Str("\n".into()), self.pos, self.line, self.col, ws, None,
                                 false);
                    }
                    self.adv(1);
                    at_line_start = true;
                } else {
                    self.adv(1);
                }
                ws = true;
                continue;
            }
            let (line, col, start) = (self.line, self.col, self.pos);
            if is_digit(ch) || (ch == '.' && self.peek(1).is_some_and(is_digit)) {
                self.number(ws)?;
            } else if let Some(v) = vulgar(ch) {
                self.adv(1);
                self.add(Kind::Num, TokValue::Num(v), start, line, col, ws, None, false);
            } else if ch == '"' {
                self.string(ws)?;
            } else if sup_char(ch).is_some() {
                self.superscript(ws)?;
            } else if ch == '\'' {
                let mut n = 0;
                while self.peek(0) == Some('\'') {
                    self.adv(1);
                    n += 1;
                }
                self.add(Kind::Prime, TokValue::Int(n), start, line, col, ws, None, false);
            } else if special_standalone(ch) {
                self.adv(1);
                self.add(Kind::Name, TokValue::Str(ch.to_string()), start, line, col, ws, None, false);
            } else if let Some(kw) = keyword_alias(ch) {
                self.adv(1);
                self.add(Kind::Kw, TokValue::Str(kw.into()), start, line, col, ws, None, false);
            } else if is_ident_start(ch) {
                self.ident(ws);
            } else {
                let mut found = false;
                for op in OPERATORS {
                    if self.starts_with(self.pos, op) {
                        self.adv(op.chars().count());
                        let canon = op_canon(op);
                        if "([{".contains(canon) {
                            self.paren += 1;
                        } else if ")]}".contains(canon) {
                            self.paren = (self.paren - 1).max(0);
                        }
                        self.add(Kind::Op, TokValue::Str(canon.into()), start, line, col, ws, None, false);
                        found = true;
                        break;
                    }
                }
                if !found {
                    let name = char_name_title(ch);
                    return Err(self.error(format!("unexpected character '{ch}' ({name})"),
                                          Some("remove it, or check the cheat sheet for the symbols Fermium understands")));
                }
            }
            ws = false;
        }
        if self.tokens.last().is_some_and(|t| t.kind != Kind::Newline) {
            self.add(Kind::Newline, TokValue::Str("\n".into()), self.pos, self.line, self.col, true, None, false);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.add(Kind::Dedent, TokValue::Int(0), self.pos, self.line, self.col, true, None, false);
        }
        self.add(Kind::Eof, TokValue::None, self.pos, self.line, self.col, true, None, false);
        self.check_lookalikes();
        Ok(self.tokens)
    }

    fn continuation_word(&self) -> bool {
        let rest = self.text(self.pos, self.pos + 5);
        rest.starts_with("with ") || rest.starts_with("for ") || rest.starts_with("with\t") || rest.starts_with("for\t")
    }

    /// An indented line starting with `else` continues an if-expression begun on the line above.
    fn else_continues(&self) -> bool {
        let rest: Vec<char> = self.src[self.pos..(self.pos + 5).min(self.src.len())].to_vec();
        let starts = rest.len() >= 4 && rest[..4] == ['e', 'l', 's', 'e'];
        if !(starts && (rest.len() < 5 || !(py_isalnum(rest[4]) || rest[4] == '_'))) {
            return false;
        }
        if self.tokens.last().is_none_or(|t| t.kind != Kind::Newline) {
            return false;
        }
        let mut k = self.tokens.len() as i64 - 2;
        while k >= 0 && !matches!(self.tokens[k as usize].kind, Kind::Newline | Kind::Indent | Kind::Dedent) {
            if self.tokens[k as usize].is_kw("then") {
                return true;
            }
            k -= 1;
        }
        false
    }

    /// A line ending in a binary operator or comma continues on the next line.
    fn continues(&self) -> bool {
        match self.tokens.last() {
            None => false,
            Some(t) => {
                t.kind == Kind::Op
                    && ["+", "-", "*", "/", "^", ",", "+-", "==", "<=", ">=", "!=", "×"].contains(&t.s())
            }
        }
    }

    fn number(&mut self, ws: bool) -> Result<(), Diagnostic> {
        let (line, col, start) = (self.line, self.col, self.pos);
        let mut p = self.pos;
        let n = self.src.len();
        let dig = |p: usize, s: &Vec<char>| p < s.len() && is_digit(s[p]);
        while p < n && (is_digit(self.src[p]) || (self.src[p] == '_' && dig(p + 1, &self.src))) {
            p += 1;
        }
        if p < n && self.src[p] == '.' && dig(p + 1, &self.src) {
            p += 1;
            while p < n && (is_digit(self.src[p]) || self.src[p] == '_') {
                p += 1;
            }
        } else if p < n
            && self.src[p] == '.'
            && !(p + 1 < n && (py_isalpha(self.src[p + 1]) || self.src[p + 1] == '.'))
        {
            p += 1; // "100." trailing point
        }
        let mantissa = self.text(start, p);
        let mut exp: i64 = 0;
        if p < n && (self.src[p] == 'e' || self.src[p] == 'E') {
            let mut q = p + 1;
            if q < n && (self.src[q] == '+' || self.src[q] == '-') {
                q += 1;
            }
            if dig(q, &self.src) {
                while dig(q, &self.src) {
                    q += 1;
                }
                exp = py_int(&self.text(p + 1, q)).unwrap_or(0);
                p = q;
            }
        } else if self.starts_with(p, "×10") {
            let q = p + 3;
            if q < n && sup_char(self.src[q]).is_some() {
                let mut r = q;
                while r < n && sup_char(self.src[r]).is_some() {
                    r += 1;
                }
                let text: String = self.src[q..r].iter().map(|c| sup_char(*c).unwrap()).collect();
                match py_int(&text) {
                    Some(v) => exp = v,
                    None => {
                        return Err(Diagnostic::error("can't read the power of ten in this number", line, col,
                                                     (r - start) as u32, None))
                    }
                }
                p = r;
            } else if q < n && self.src[q] == '^' {
                let mut r = q + 1;
                if r < n && (self.src[r] == '+' || self.src[r] == '-') {
                    r += 1;
                }
                while dig(r, &self.src) {
                    r += 1;
                }
                match py_int(&self.text(q + 1, r)) {
                    Some(v) => exp = v,
                    None => {
                        return Err(Diagnostic::error("can't read the power of ten in this number (write e.g. 3×10^8)",
                                                     line, col, (r - start) as u32, None))
                    }
                }
                p = r;
            }
        }
        self.adv(p - self.pos);
        let clean: String = mantissa.chars().filter(|c| *c != '_').collect();
        let mut value: f64 = if exp != 0 {
            let e = exp.clamp(-100000, 100000);
            format!("{clean}e{e}").parse().unwrap_or(0.0)
        } else {
            clean.parse().unwrap_or(0.0)
        };
        if exp == 0 && !clean.contains('.') && p < n {
            if let Some(v) = vulgar(self.src[p]) {
                // 2½ = 2.5 (a mixed number)
                value += v;
                p += 1;
                self.adv(1);
            }
        }
        if value == f64::INFINITY {
            return Err(Diagnostic::error(
                format!("the number {} is too large (bigger than about 1.8×10³⁰⁸)", self.text(start, p)),
                line, col, (p - start) as u32, None));
        }
        if p < n && self.src[p] == '.' && dig(p + 1, &self.src) {
            return Err(Diagnostic::error(format!("this number has two decimal points: {}...", self.text(start, p + 2)),
                                         line, col, (p - start + 2) as u32, None));
        }
        if p < n && self.src[p] == 'i' && !(p + 1 < n && is_ident_char(self.src[p + 1])) {
            self.adv(1);
            self.add(Kind::Imag, TokValue::Num(value), start, line, col, ws, count_sigfigs(&mantissa), true);
            return Ok(());
        }
        self.add(Kind::Num, TokValue::Num(value), start, line, col, ws, count_sigfigs(&mantissa), true);
        Ok(())
    }

    fn string(&mut self, ws: bool) -> Result<(), Diagnostic> {
        let (line, col, start) = (self.line, self.col, self.pos);
        self.adv(1);
        let mut buf = String::new();
        while self.pos < self.src.len() && self.peek(0) != Some('"') {
            if self.peek(0) == Some('\n') {
                return Err(Diagnostic::error("this text (string) is missing its closing quote \"", line, col, 1, None));
            }
            buf.push(self.src[self.pos]);
            self.adv(1);
        }
        if self.pos >= self.src.len() {
            return Err(Diagnostic::error("this text (string) is missing its closing quote \"", line, col, 1, None));
        }
        self.adv(1);
        self.add(Kind::Str, TokValue::Str(buf), start, line, col, ws, None, false);
        Ok(())
    }

    fn superscript(&mut self, ws: bool) -> Result<(), Diagnostic> {
        let (line, col, start) = (self.line, self.col, self.pos);
        let mut buf = String::new();
        while let Some(c) = self.peek(0).and_then(sup_char) {
            buf.push(c);
            self.adv(1);
        }
        match py_int(&buf) {
            Some(v) => {
                self.add(Kind::Sup, TokValue::Int(v), start, line, col, ws, None, false);
                Ok(())
            }
            None => Err(Diagnostic::error(
                format!("can't read the superscript exponent '{}'", self.text(start, self.pos)), line, col, 1, None)),
        }
    }

    fn ident(&mut self, ws: bool) {
        let (line, col, start) = (self.line, self.col, self.pos);
        if self.peek(0) == Some('%') {
            self.adv(1);
            self.add(Kind::Name, TokValue::Str("%".into()), start, line, col, ws, None, false);
            return;
        }
        if self.peek(0) == Some('°') {
            self.adv(1);
            if matches!(self.peek(0), Some('C') | Some('F')) && !is_ident_char(self.peek(1).unwrap_or(' ')) {
                self.adv(1);
            }
            let t = self.text(start, self.pos);
            self.add(Kind::Name, TokValue::Str(t), start, line, col, ws, None, false);
            return;
        }
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if special_standalone(c) {
                if self.src[self.pos - 1] == '_' {
                    // R_∞, m_π: a symbol as a subscript is part of the name
                    self.adv(1);
                    continue;
                }
                break;
            }
            if is_ident_char(c) {
                self.adv(1);
            } else {
                break;
            }
        }
        let raw: Vec<char> = self.src[start..self.pos].to_vec();
        // subscript digits -> _N
        let mut text = String::new();
        let mut k = 0;
        while k < raw.len() {
            if sub_char(raw[k]).is_some() {
                text.push('_');
                while k < raw.len() {
                    match sub_char(raw[k]) {
                        Some(d) => text.push(d),
                        None => break,
                    }
                    k += 1;
                }
            } else {
                text.push(raw[k]);
                k += 1;
            }
        }
        let text = text.replace("__", "_");
        if KEYWORDS.contains(&text.as_str()) {
            self.add(Kind::Kw, TokValue::Str(text), start, line, col, ws, None, false);
            return;
        }
        self.add(Kind::Name, TokValue::Str(canonical_name(&text)), start, line, col, ws, None, false);
    }

    fn check_lookalikes(&mut self) {
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut warned: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        let mut out = vec![];
        for t in &self.tokens {
            if t.kind != Kind::Name {
                continue;
            }
            let v = t.s().to_string();
            let sk: String = v.chars().map(skeleton_char).collect();
            let other = seen.entry(sk).or_insert_with(|| v.clone()).clone();
            if other != v && !warned.contains(&(other.clone(), v.clone())) {
                warned.insert((other.clone(), v.clone()));
                out.push(Diagnostic::warning(
                    format!("'{v}' and '{other}' look almost identical but are different names"), t.line, t.col,
                    t.rawlen as u32, Some("rename one of them so they can't be confused (e.g. nu vs v)".into())));
            }
        }
        for d in out {
            self.diags.warn(d);
        }
    }
}

/// Tokenize a program; look-alike warnings go to `diags`.
pub fn tokenize(source: &str, diags: &mut Diagnostics) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source, diags).tokenize()
}
