//! Parsing unit strings like "kg m/s²" or "J/(mol K)" (port of `parse_unit_string` in fermium/units.py;
//! used for CSV headers, display units and tests -- the language parser has its own unit expressions).

use crate::db::{affine, lookup_unit, Unit, AFFINE};
use crate::dim::DIMLESS;
use num_rational::Rational64;
use std::fmt;

/// A unit string Fermium can't read.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitSyntaxError {
    pub message: String,
    /// Fermium 1.5 raised an exception other than UnitSyntaxError here (a ValueError from `int()`, an
    /// IndexError at the end of the text); `message` is then this port's own wording.
    pub python_exception: bool,
}

impl fmt::Display for UnitSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UnitSyntaxError {}

fn syn(msg: String) -> UnitSyntaxError {
    UnitSyntaxError { message: msg, python_exception: false }
}

fn exc(msg: String) -> UnitSyntaxError {
    UnitSyntaxError { message: msg, python_exception: true }
}

/// Python's `str.isspace` (Unicode White_Space plus the ASCII separators U+001C..U+001F).
pub fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.strip()`.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_isspace)
}

/// Python's `str.isalpha` for one character (letters; Rust's Alphabetic minus letter-like numbers).
pub fn py_isalpha(c: char) -> bool {
    c.is_alphabetic() && !c.is_numeric()
}

/// Python's `str.isdigit` for the characters that matter here (ASCII and superscript digits).
fn py_isdigit(c: char) -> bool {
    c.is_ascii_digit() || "⁰¹²³⁴⁵⁶⁷⁸⁹".contains(c)
}

const SUPERS: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁻";

fn sup_to_ascii(c: char) -> char {
    match c {
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
        c => c,
    }
}

fn is_unit_char(c: char) -> bool {
    py_isalpha(c) || "°Ω☉Å%_µμ".contains(c)
}

/// The tokens of a unit string (`_tokenize_unit`): names, numbers, `( ) * / · ^`, superscripts as
/// `^` + digits, anything else as `#` + the character.
pub fn tokenize_unit(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        if py_isspace(ch) {
            i += 1;
        } else if "()*/·^".contains(ch) {
            out.push(ch.to_string());
            i += 1;
        } else if SUPERS.contains(ch) {
            let mut j = i;
            while j < cs.len() && SUPERS.contains(cs[j]) {
                j += 1;
            }
            out.push("^".into());
            out.push(cs[i..j].iter().map(|&c| sup_to_ascii(c)).collect());
            i = j;
        } else if py_isdigit(ch) || (ch == '-' && out.last().is_some_and(|t| t == "^" || t == "(")) {
            let mut j = i + 1;
            while j < cs.len() && py_isdigit(cs[j]) {
                j += 1;
            }
            out.push(cs[i..j].iter().collect());
            i = j;
        } else if is_unit_char(ch) {
            let mut j = i;
            while j < cs.len() && is_unit_char(cs[j]) {
                j += 1;
            }
            out.push(cs[i..j].iter().collect());
            i = j;
        } else {
            out.push(format!("#{ch}"));
            i += 1;
        }
    }
    out
}

struct P<'a> {
    toks: Vec<String>,
    pos: usize,
    text: &'a str,
}

fn py_int(s: &str) -> Result<i64, UnitSyntaxError> {
    s.parse::<i64>().ok().filter(|v| *v != i64::MIN).ok_or_else(|| exc(format!("bad exponent '{s}' in unit")))
}

impl P<'_> {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(|s| s.as_str())
    }
    fn take(&mut self) -> Result<String, UnitSyntaxError> {
        let t = self.toks.get(self.pos).cloned().ok_or_else(|| exc(format!("incomplete unit '{}'", self.text)))?;
        self.pos += 1;
        Ok(t)
    }

    fn factor(&mut self) -> Result<Unit, UnitSyntaxError> {
        let text = self.text;
        let t = match self.peek() {
            None => return Err(syn(format!("incomplete unit '{text}'"))),
            Some(t) => t.to_string(),
        };
        let mut u = if t == "(" {
            self.take()?;
            let u = self.product()?;
            if self.peek() != Some(")") {
                return Err(syn(format!("missing ')' in unit '{text}'")));
            }
            self.take()?;
            u
        } else if t == "1" {
            self.take()?;
            Unit::new("1", DIMLESS, 1.0)
        } else if let Some(rest) = t.strip_prefix('#') {
            return Err(syn(format!("unexpected '{rest}' in unit '{text}'")));
        } else {
            self.take()?;
            match lookup_unit(&t) {
                Some(u) => u,
                None => return Err(syn(format!("unknown unit '{t}'"))),
            }
        };
        if self.peek() == Some("^") {
            self.take()?;
            let e = self.take()?;
            let p = if e == "(" {
                let num = self.take()?;
                let p = if self.peek() == Some("/") {
                    self.take()?;
                    let den = self.take()?;
                    let (n, d) = (py_int(&num)?, py_int(&den)?);
                    if d == 0 {
                        return Err(exc(format!("zero denominator in unit '{text}'")));
                    }
                    Rational64::new(n, d)
                } else {
                    Rational64::from_integer(py_int(&num)?)
                };
                self.take()?; // ')'
                p
            } else {
                Rational64::from_integer(py_int(&e)?)
            };
            u = u.pow(p);
        }
        Ok(u)
    }

    fn product(&mut self) -> Result<Unit, UnitSyntaxError> {
        let mut u = self.factor()?;
        loop {
            match self.peek() {
                Some("*") | Some("·") => {
                    self.take()?;
                    u = u.mul(&self.factor()?);
                }
                Some("/") => {
                    self.take()?;
                    u = u.div(&self.factor()?);
                }
                Some(t) if t != ")" && t != "^" => {
                    u = u.mul(&self.factor()?);
                }
                _ => return Ok(u),
            }
        }
    }
}

/// Parse a unit string ("kg m/s²", "J/(mol K)", "m^(1/2)", "°C"); the result is named `text` (stripped).
pub fn parse_unit_string(text: &str) -> Result<Unit, UnitSyntaxError> {
    let toks = tokenize_unit(text);
    let n = toks.len();
    let mut p = P { toks, pos: 0, text };
    let mut u = p.product()?;
    if p.pos != n {
        return Err(syn(format!("can't read unit '{text}'")));
    }
    if !u.affine() && AFFINE.iter().any(|a| text.contains(a.0)) && n > 1 {
        return Err(syn(format!("°C/°F can't be combined with other units in '{text}'; use K")));
    }
    let _ = affine;
    u.name = py_strip(text).to_string();
    Ok(u)
}
