//! Build script: loads Fermium's unit database, which is written in Fermium (fermium/selfhost/units_db.fm,
//! moonshot M8, spec §B4), and generates the table of non-SI unit factors.
//!
//! It evaluates the small subset of Fermium that file uses -- assignments, numbers with SI base/coherent
//! units (with prefixes), implicit multiplication, `*`, `/`, superscript powers, parentheses, `π`, and
//! `print "name", expr to 17 digits` -- checking dimensions like Fermium's unit checker (every printed
//! factor must be a plain number). Each factor is then printed to 17 significant figures and read back
//! exactly as `python3 -m fermium.selfhost` does, so the table equals fermium/units_selfhosted.py bit for
//! bit (a test checks this).

#[path = "src/numfmt.rs"]
mod numfmt;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

type D = [i32; 7]; // exponents of m, kg, s, A, K, mol, cd

const ZERO: D = [0; 7];

fn dmul(a: D, b: D) -> D {
    let mut r = a;
    for i in 0..7 {
        r[i] += b[i];
    }
    r
}
fn ddiv(a: D, b: D) -> D {
    let mut r = a;
    for i in 0..7 {
        r[i] -= b[i];
    }
    r
}
fn dpow(a: D, p: i32) -> D {
    a.map(|x| x * p)
}

/// SI base and coherent derived units (factor, dimension), as in fermium/units.py.
fn si_unit(name: &str) -> Option<(f64, D, bool)> {
    let l = [1, 0, 0, 0, 0, 0, 0];
    let m = [0, 1, 0, 0, 0, 0, 0];
    let t = [0, 0, 1, 0, 0, 0, 0];
    let i = [0, 0, 0, 1, 0, 0, 0];
    let n_ = ddiv(dmul(m, l), dpow(t, 2));
    let j_ = dmul(n_, l);
    let w_ = ddiv(j_, t);
    let c_ = dmul(i, t);
    let v_ = ddiv(w_, i);
    let pa_ = ddiv(n_, dpow(l, 2));
    Some(match name {
        "m" => (1.0, l, true),
        "g" => (1e-3, m, true),
        "s" => (1.0, t, true),
        "A" => (1.0, i, true),
        "K" => (1.0, [0, 0, 0, 0, 1, 0, 0], true),
        "mol" => (1.0, [0, 0, 0, 0, 0, 1, 0], true),
        "cd" => (1.0, [0, 0, 0, 0, 0, 0, 1], true),
        "Hz" => (1.0, ddiv(ZERO, t), true),
        "N" => (1.0, n_, true),
        "Pa" => (1.0, pa_, true),
        "J" => (1.0, j_, true),
        "W" => (1.0, w_, true),
        "C" => (1.0, c_, true),
        "V" => (1.0, v_, true),
        "T" => (1.0, ddiv(dmul(v_, t), dpow(l, 2)), true),
        _ => return None,
    })
}

const PREFIXES: [(&str, f64); 20] = [
    ("da", 1e1),
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("μ", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
];

fn lookup(name: &str) -> Option<(f64, D)> {
    if let Some((f, d, _)) = si_unit(name) {
        return Some((f, d));
    }
    for (p, pf) in PREFIXES {
        if let Some(rest) = name.strip_prefix(p) {
            if let Some((f, d, true)) = si_unit(rest) {
                if !rest.is_empty() && name != "cd" && name != "Pa" {
                    return Some((f * pf, d));
                }
            }
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Str(String),
    Sup(i32),
    Op(char),
    Nl,
}

fn sup_val(c: char) -> Option<char> {
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
        _ => return None,
    })
}

fn lex(src: &str) -> Vec<(Tok, usize)> {
    let mut out = Vec::new();
    for (ln, line) in src.lines().enumerate() {
        let ln = ln + 1;
        let cs: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            if c == '#' {
                break;
            } else if c.is_whitespace() {
                i += 1;
            } else if c == '"' {
                let j = (i + 1..cs.len()).find(|&j| cs[j] == '"').unwrap_or_else(|| panic!("units_db.fm:{ln}: unclosed string"));
                out.push((Tok::Str(cs[i + 1..j].iter().collect()), ln));
                i = j + 1;
            } else if c.is_ascii_digit() || (c == '.' && i + 1 < cs.len() && cs[i + 1].is_ascii_digit()) {
                let mut j = i;
                while j < cs.len() && (cs[j].is_ascii_digit() || cs[j] == '.') {
                    j += 1;
                }
                if j < cs.len() && (cs[j] == 'e' || cs[j] == 'E') {
                    let mut k = j + 1;
                    if k < cs.len() && (cs[k] == '+' || cs[k] == '-') {
                        k += 1;
                    }
                    if k < cs.len() && cs[k].is_ascii_digit() {
                        while k < cs.len() && cs[k].is_ascii_digit() {
                            k += 1;
                        }
                        j = k;
                    }
                }
                let text: String = cs[i..j].iter().collect();
                out.push((Tok::Num(text.parse().unwrap_or_else(|_| panic!("units_db.fm:{ln}: bad number {text}"))), ln));
                i = j;
            } else if let Some(_) = sup_val(c) {
                let mut j = i;
                let mut s = String::new();
                while j < cs.len() {
                    match sup_val(cs[j]) {
                        Some(d) => s.push(d),
                        None => break,
                    }
                    j += 1;
                }
                out.push((Tok::Sup(s.parse().unwrap_or_else(|_| panic!("units_db.fm:{ln}: bad exponent"))), ln));
                i = j;
            } else if c.is_alphabetic() || c == '_' {
                let mut j = i;
                while j < cs.len() && (cs[j].is_alphanumeric() || cs[j] == '_') && sup_val(cs[j]).is_none() {
                    j += 1;
                }
                out.push((Tok::Ident(cs[i..j].iter().collect()), ln));
                i = j;
            } else if "=*/(),".contains(c) {
                out.push((Tok::Op(c), ln));
                i += 1;
            } else {
                panic!("units_db.fm:{ln}: the build-time evaluator doesn't understand '{c}'");
            }
        }
        out.push((Tok::Nl, ln));
    }
    out
}

#[derive(Clone, Copy)]
struct Q {
    v: f64,
    d: D,
}

struct Ev {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    vars: HashMap<String, Q>,
}

impl Ev {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }
    fn peek_at(&self, k: usize) -> &Tok {
        self.toks.get(self.pos + k).map(|t| &t.0).unwrap_or(&Tok::Nl)
    }
    fn line(&self) -> usize {
        self.toks[self.pos.min(self.toks.len() - 1)].1
    }
    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].0.clone();
        self.pos += 1;
        t
    }
    fn fail(&self, msg: &str) -> ! {
        panic!("units_db.fm:{}: {msg}", self.line())
    }
    fn is_unit_ident(&self, t: &Tok) -> bool {
        matches!(t, Tok::Ident(n) if !self.vars.contains_key(n) && lookup(n).is_some())
    }
    fn expect(&mut self, t: Tok) {
        let got = self.next();
        if got != t {
            self.fail(&format!("expected {t:?}, found {got:?}"));
        }
    }

    /// A unit written after a number: factors with exponents, like Fermium's parser produces
    /// (`m³/(kg s²)` is m³ kg⁻¹ s⁻²); its factor is computed as Checker.resolve_unit_si does.
    fn unit_expr(&mut self) -> (f64, D) {
        let mut factors: Vec<(String, i32)> = Vec::new();
        loop {
            let t = self.peek().clone();
            if self.is_unit_ident(&t) {
                self.pos += 1;
                let e = self.sup();
                factors.push((ident(&t), e));
            } else if t == Tok::Op('/') && self.is_unit_ident(&self.peek_at(1).clone()) {
                self.pos += 1;
                let n = ident(&self.next());
                let e = self.sup();
                factors.push((n, -e));
            } else if t == Tok::Op('/') && *self.peek_at(1) == Tok::Op('(') && self.is_unit_ident(&self.peek_at(2).clone()) {
                self.pos += 2;
                while self.is_unit_ident(&self.peek().clone()) {
                    let n = ident(&self.next());
                    let e = self.sup();
                    factors.push((n, -e));
                }
                self.expect(Tok::Op(')'));
            } else {
                break;
            }
        }
        let mut total: Option<(f64, D)> = None;
        for (n, e) in factors {
            let (mut f, mut d) = lookup(&n).unwrap();
            if e != 1 {
                f = f.powf(e as f64);
                d = dpow(d, e);
            }
            total = Some(match total {
                None => (f, d),
                Some((tf, td)) => (tf * f, dmul(td, d)),
            });
        }
        total.unwrap()
    }

    fn sup(&mut self) -> i32 {
        if let Tok::Sup(e) = *self.peek() {
            self.pos += 1;
            e
        } else {
            1
        }
    }

    fn primary(&mut self) -> Q {
        let q = match self.next() {
            Tok::Num(x) => {
                if self.is_unit_ident(&self.peek().clone()) {
                    let (f, d) = self.unit_expr();
                    Q { v: x * f, d }
                } else {
                    Q { v: x, d: ZERO }
                }
            }
            Tok::Ident(n) if n == "π" => Q { v: std::f64::consts::PI, d: ZERO },
            Tok::Ident(n) => match self.vars.get(&n) {
                Some(q) => *q,
                None => self.fail(&format!("'{n}' is not defined")),
            },
            Tok::Op('(') => {
                let q = self.expr();
                self.expect(Tok::Op(')'));
                q
            }
            t => self.fail(&format!("unexpected {t:?}")),
        };
        let e = self.sup();
        if e != 1 {
            Q { v: q.v.powf(e as f64), d: dpow(q.d, e) }
        } else {
            q
        }
    }

    fn starts_primary(&self) -> bool {
        match self.peek() {
            Tok::Num(_) | Tok::Op('(') => true,
            Tok::Ident(n) => n != "to",
            _ => false,
        }
    }

    /// Juxtaposition (implicit multiplication) binds tighter than `*` and `/`.
    fn term(&mut self) -> Q {
        let mut q = self.primary();
        while self.starts_primary() {
            let r = self.primary();
            q = Q { v: q.v * r.v, d: dmul(q.d, r.d) };
        }
        q
    }

    fn expr(&mut self) -> Q {
        let mut q = self.term();
        loop {
            match self.peek() {
                Tok::Op('*') => {
                    self.pos += 1;
                    let r = self.term();
                    q = Q { v: q.v * r.v, d: dmul(q.d, r.d) };
                }
                Tok::Op('/') => {
                    self.pos += 1;
                    let r = self.term();
                    q = Q { v: q.v / r.v, d: ddiv(q.d, r.d) };
                }
                _ => return q,
            }
        }
    }

    fn run(&mut self) -> Vec<(String, f64)> {
        let mut table = Vec::new();
        while self.pos < self.toks.len() {
            match self.peek().clone() {
                Tok::Nl => {
                    self.pos += 1;
                    continue;
                }
                Tok::Ident(k) if k == "print" => {
                    self.pos += 1;
                    let name = match self.next() {
                        Tok::Str(s) => s,
                        t => self.fail(&format!("expected the unit's name in quotes, found {t:?}")),
                    };
                    self.expect(Tok::Op(','));
                    let q = self.expr();
                    self.expect(Tok::Ident("to".into()));
                    let n = match self.next() {
                        Tok::Num(n) => n as i64,
                        t => self.fail(&format!("expected a number of digits, found {t:?}")),
                    };
                    self.expect(Tok::Ident("digits".into()));
                    if q.d != ZERO {
                        self.fail(&format!(
                            "the definition of {name} has the wrong dimension (it is not a plain number: exponents {:?})",
                            q.d
                        ));
                    }
                    // print ... to n digits (format_quantity: format_number(x, max(n, 2), trim=False)),
                    // then read back as fermium/selfhost/__main__.py's parse_number does
                    let text = numfmt::format_number(q.v, n.max(2), false);
                    table.push((name, parse_number(&text)));
                }
                Tok::Ident(n) if *self.peek_at(1) == Tok::Op('=') => {
                    self.pos += 2;
                    let q = self.expr();
                    self.vars.insert(n, q);
                }
                t => self.fail(&format!("the build-time evaluator doesn't understand {t:?}")),
            }
            if !matches!(self.peek_at(0), Tok::Nl) {
                self.fail("unexpected text after the statement");
            }
        }
        table
    }
}

fn ident(t: &Tok) -> String {
    match t {
        Tok::Ident(n) => n.clone(),
        _ => unreachable!(),
    }
}

/// fermium/selfhost/__main__.py `parse_number`: `float(mantissa) * 10.0 ** exponent` for `m×10ⁿ`.
fn parse_number(text: &str) -> f64 {
    if let Some((m, e)) = text.split_once("×10") {
        let e: String = e.chars().map(|c| sup_val(c).unwrap()).collect();
        let m: f64 = m.parse().unwrap();
        return m * 10f64.powf(e.parse::<i32>().unwrap() as f64);
    }
    text.parse().unwrap()
}

fn find_db(manifest: &Path) -> PathBuf {
    if let Ok(p) = env::var("FERMIUM_UNITS_DB") {
        return PathBuf::from(p);
    }
    let root = manifest.join("../../..");
    for cand in ["fermium/selfhost/units_db.fm", "legacy/fermium/selfhost/units_db.fm"] {
        let p = root.join(cand);
        if p.exists() {
            return p;
        }
    }
    panic!("can't find fermium/selfhost/units_db.fm (set FERMIUM_UNITS_DB to its path)");
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let db = find_db(&manifest);
    println!("cargo:rerun-if-changed={}", db.display());
    println!("cargo:rerun-if-changed=src/numfmt.rs");
    println!("cargo:rerun-if-env-changed=FERMIUM_UNITS_DB");
    let src = fs::read_to_string(&db).unwrap();
    let mut ev = Ev { toks: lex(&src), pos: 0, vars: HashMap::new() };
    let table = ev.run();
    let mut out = String::new();
    out.push_str("// GENERATED by build.rs from fermium/selfhost/units_db.fm (a Fermium program). Do not edit.\n");
    out.push_str("/// Non-SI unit name -> SI factor, as computed by the Fermium unit database.\n");
    out.push_str("pub const SELF_HOSTED: &[(&str, f64)] = &[\n");
    for (name, v) in &table {
        out.push_str(&format!("    ({name:?}, f64::from_bits({:#018x})), // {v:?}\n", v.to_bits()));
    }
    out.push_str("];\n");
    let dest = PathBuf::from(env::var("OUT_DIR").unwrap()).join("units_db.rs");
    fs::write(dest, out).unwrap();
}
