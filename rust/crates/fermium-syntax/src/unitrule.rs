//! Units in the parser (`fermium/parser.py`, "the unit-name rule (A1, D235)" and "units").
//!
//! 1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
//! 2. If that unit is a single name that is also one of your variables, Fermium stops and asks which you mean.
//! 3. In a compound unit the first name is always a unit; any later name that is also your variable is an error.
//!
//! Brackets are always units; a name that doesn't come right after a number is a variable; spaces never matter.
//! In fix mode (`fermium fmt --fix`, the language server's quick fix) each collision is rewritten as a bracketed unit
//! that keeps what v1 did there, and parsing goes on with v1's reading.

use std::collections::HashSet;

use num_rational::Rational64;

use crate::ast::*;
use crate::diag::{Diagnostic, Fix};
use crate::lexer::{canonical_name, Kind};
use crate::parser::{unit_words, Parser, R};
use crate::units::{is_base_unit, is_unit_name, lookup_unit, prefixes_longest_first};

/// Prefixed units a physics course uses all the time: never read as two of your variables (D203)
const COMMON_PREFIXED: &[&str] = &[
    "kg", "mg", "μg", "ug", "km", "cm", "mm", "μm", "um", "nm", "pm", "fm", "dm", "ms", "μs", "us", "ns", "ps", "fs",
    "kHz", "MHz", "GHz", "THz", "mV", "kV", "MV", "μV", "uV", "mA", "μA", "uA", "nA", "pA", "kW", "MW", "GW", "mW",
    "μW", "kJ", "MJ", "GJ", "mJ", "μJ", "keV", "MeV", "GeV", "TeV", "meV", "kPa", "MPa", "GPa", "hPa", "mL", "μL",
    "mmol", "kmol", "μmol", "mT", "μT", "uT", "nT", "μF", "uF", "nF", "pF", "mF", "mH", "μH", "uH", "kΩ", "MΩ", "mΩ",
    "kohm", "Mohm", "μC", "uC", "nC", "pC", "mC", "mK", "μK", "nK", "kN", "mN", "kBq", "MBq", "GBq", "mGy", "mSv",
    "μSv", "uSv", "mbar", "kcal", "Myr", "Gyr", "kyr", "kpc", "Mpc", "Gpc", "mrad", "μrad", "krad", "dB",
];

fn what_of(name: &str) -> String {
    unit_words(name).map(|s| s.to_string()).or_else(|| lookup_unit(name).map(|s| s.to_string()))
        .unwrap_or_else(|| "a unit".into())
}

/// `Fraction(x).limit_denominator(1000)` for a float.
pub fn limit_denominator(x: f64, max_den: i128) -> Rational64 {
    if !x.is_finite() {
        return Rational64::from_integer(0);
    }
    // the exact value of x as n/d
    let bits = x.to_bits();
    let neg = (bits >> 63) != 0;
    let exp = ((bits >> 52) & 0x7ff) as i64;
    let frac = (bits & ((1u64 << 52) - 1)) as i128;
    let (mant, e2) = if exp == 0 { (frac, -1074i64) } else { (frac | (1i128 << 52), exp - 1075) };
    if mant == 0 {
        return Rational64::from_integer(0);
    }
    let (mut n, mut d): (i128, i128);
    if e2 >= 0 {
        if e2 > 60 {
            return Rational64::from_integer(if neg { i64::MIN + 1 } else { i64::MAX });
        }
        n = mant << e2;
        d = 1;
    } else {
        let sh = -e2;
        if sh > 120 {
            // far below 1/max_den: rounds to 0 or 1/max_den; Python gives the closer one
            return Rational64::from_integer(0);
        }
        n = mant;
        d = 1i128 << sh;
        let g = gcd(n, d);
        n /= g;
        d /= g;
    }
    if neg {
        n = -n;
    }
    if d <= max_den {
        return Rational64::new(n.clamp(i64::MIN as i128 + 1, i64::MAX as i128) as i64, d as i64);
    }
    let (on, od) = (n, d);
    let (mut p0, mut q0, mut p1, mut q1) = (0i128, 1i128, 1i128, 0i128);
    let (mut nn, mut dd) = (n, d);
    loop {
        let a = nn.div_euclid(dd);
        let q2 = q0 + a * q1;
        if q2 > max_den {
            break;
        }
        (p0, q0, p1, q1) = (p1, q1, p0 + a * p1, q2);
        (nn, dd) = (dd, nn - a * dd);
    }
    let k = (max_den - q0) / q1;
    let (b1n, b1d) = (p0 + k * p1, q0 + k * q1);
    let (b2n, b2d) = (p1, q1);
    // |b2 - x| <= |b1 - x|
    // checked: with a tiny x these products overflow i128 (red team 13); then compare as floats
    let diff = |bn: i128, bd: i128, other_d: i128| -> Option<i128> {
        bn.checked_mul(od)?.checked_sub(on.checked_mul(bd)?)?.checked_abs()?.checked_mul(other_d)
    };
    let closer_b2 = match (diff(b2n, b2d, b1d), diff(b1n, b1d, b2d)) {
        (Some(d2), Some(d1)) => d2 <= d1,
        _ => {
            let x = on as f64 / od as f64;
            (b2n as f64 / b2d as f64 - x).abs() <= (b1n as f64 / b1d as f64 - x).abs()
        }
    };
    let (rn, rd) = if closer_b2 { (b2n, b2d) } else { (b1n, b1d) };
    Rational64::new(rn as i64, rd as i64)
}

/// a · b for unit exponents, None if it doesn't fit in 64 bits (red team 13).
fn exp_mul(a: Rational64, b: Rational64) -> Option<Rational64> {
    let n = (*a.numer() as i128).checked_mul(*b.numer() as i128)?;
    let d = (*a.denom() as i128).checked_mul(*b.denom() as i128)?;
    let g = gcd(n, d).max(1);
    let (n, d) = (n / g, d / g);
    let lim = i64::MAX as i128;
    (n.abs() <= lim && d <= lim).then(|| Rational64::new_raw(n as i64, d as i64))
}

/// The message and hint for a unit power too large for 64 bits (the checker's wording, fermium_units::exact).
pub fn exp_too_large(what: &str) -> (String, String) {
    (format!("this unit's power is too large to track exactly ({what})"),
     "Fermium keeps a unit's power as an exact fraction of 64-bit whole numbers; raise a plain number to the \
      power instead, and attach the unit afterwards"
         .to_string())
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Parser {
    fn whose(&self, name: &str, where_: bool) -> String {
        if where_ {
            return format!("the {name} from 'where'");
        }
        if self.deriv_vars.contains(name) {
            return format!("the variable {name} you differentiate by");
        }
        if self.solve_unknowns.contains(name) {
            return format!("the unknown {name} of this solve");
        }
        format!("your variable {name}")
    }

    fn number_before(&self, start: usize) -> String {
        if start > 0 && self.toks[start - 1].kind == Kind::Num {
            self.toks[start - 1].raw.clone()
        } else {
            "2".into()
        }
    }

    /// Record a fix-mode edit; an edit inside one already recorded (or the same) adds nothing.
    pub fn add_fix(&mut self, fix: Fix) {
        let (a, b) = (fix.0, fix.1);
        self.fixes.retain(|f| !(a <= f.0 && f.1 <= b));
        if !self.fixes.iter().any(|f| f.0 <= a && b <= f.1) {
            self.fixes.push(fix);
        }
    }

    /// The unit written by tokens a..b, with spaces only between names (`m/s/g`, `kg m`, `J/(kg K)`).
    pub fn unit_span_text(&self, a: usize, b: usize) -> String {
        let mut out = String::new();
        for j in a..=b.min(self.toks.len() - 1) {
            let tk = &self.toks[j];
            let pv = if j > a { Some(&self.toks[j - 1]) } else { None };
            let tight = pv.is_none()
                || matches!(tk.kind, Kind::Sup | Kind::Prime)
                || (tk.kind == Kind::Op && "/*^)".contains(tk.s()))
                || pv.is_some_and(|p| p.kind == Kind::Op && "/*^(-".contains(p.s()));
            if !(tight || !tk.ws_before) {
                out.push(' ');
            }
            out.push_str(&tk.raw);
        }
        out
    }

    /// The edit that writes the unit from token `first` to token `last` in brackets.
    pub fn bracket_fix(&self, first: usize, last: usize) -> Fix {
        let text = self.unit_span_text(first, last);
        (self.toks[first].start, self.toks[last].end, format!("[{text}]"))
    }

    /// The last token of a unit that starts at token j, reading every unit name (for a quick fix).
    fn unit_end_from(&self, j: usize) -> usize {
        let mut last = j;
        let mut k = j + 1;
        let n = self.toks.len();
        while k < n {
            let tk = &self.toks[k];
            if tk.kind == Kind::Sup {
                last = k;
            } else if tk.is_op("^") && k + 1 < n {
                k += 1;
                if self.toks[k].kind == Kind::Op && ["-", "("].contains(&self.toks[k].s()) {
                    let m = if self.toks[k].s() == "(" { self.match_(k) } else { Some(k + 1) };
                    k = m.unwrap_or(k);
                }
                last = k;
            } else if tk.kind == Kind::Op
                && ["/", "*"].contains(&tk.s())
                && k + 1 < n
                && self.toks[k + 1].kind == Kind::Name
                && self.unit_tok(k + 1)
            {
                k += 1;
                last = k;
            } else if tk.kind == Kind::Name
                && self.in_integrand > 0
                && tk.raw.starts_with('d')
                && tk.raw.chars().count() > 1
                && self.known.contains(&canonical_name(&tk.raw[1..]))
            {
                break;
            } else if tk.kind == Kind::Name
                && self.unit_tok(k)
                && tk.ws_before
                && !(k + 1 < n && self.toks[k + 1].is_op("(") && !self.toks[k + 1].ws_before)
            {
                last = k;
            } else {
                break;
            }
            k += 1;
        }
        last.min(n - 1)
    }

    /// Sentence 3: a later name of a compound unit (`20 m/s/g`, `2 kg m`) that is also your variable.
    /// Returns true to go on reading the unit (fix mode, where v1 did), false to stop before the name (fix mode).
    fn later_collision(&mut self, start: usize, nt: usize, old_continues: bool) -> R<bool> {
        let mut old_continues = old_continues;
        let so_far_last = self.i - 1;
        if self.fix_mode {
            let j = nt + 1;
            if j < self.toks.len() {
                let nx = &self.toks[j];
                if (nx.is_op("/") && j + 1 < self.toks.len() && self.toks[j + 1].kind == Kind::Name
                    && self.unit_tok(j + 1))
                    || nx.kind == Kind::Sup
                {
                    old_continues = true;
                }
            }
            if old_continues {
                self.fix_whole.push(start);
                return Ok(true);
            }
            let f = self.bracket_fix(start, so_far_last);
            self.add_fix(f);
            return Ok(false);
        }
        let num = self.number_before(start);
        let so_far = self.unit_span_text(start, self.i - 1);
        let full_last = self.unit_end_from(nt);
        let full = self.unit_span_text(start, full_last);
        let ntt = self.toks[nt].clone();
        let what = what_of_value(ntt.s(), &ntt.raw);
        let whose = self.whose(ntt.s(), false);
        let op = if self.kind() == Kind::Op { self.tok().s().to_string() } else { " ".into() };
        let ntext = self.unit_span_text(nt, self.factor_last(nt));
        let use_ = if op == "/" {
            format!("({num} {so_far})/{ntext}  to divide by {whose}")
        } else {
            format!("({num} {so_far}) {ntext}  for {num} {so_far} × {whose}")
        };
        let mut e = self.error(
            format!("'{num} {full}' is ambiguous: right after a number, {full} is one unit ({} is {what} there), but \
                     {} is also {whose}", ntt.raw, ntt.raw),
            Some(nt),
            Some(format!("write  {use_}, or  {num} [{full}]  for the unit")),
        );
        e.fix = vec![if old_continues { self.bracket_fix(start, full_last) } else { self.bracket_fix(start, so_far_last) }];
        Err(e)
    }

    /// Sentence 2: `2 g`, `0.1 m`, `2 T` right after a number, when that single name is also your variable.
    pub fn single_collision(&mut self, q: &Expr, _start_tok: Option<usize>, where_: bool, num: Option<String>)
                            -> R<()> {
        let ExprKind::Quantity { unit, .. } = &q.kind else { return Ok(()) };
        let f = &unit.factors[0];
        let Some(first) = self.toks.iter().position(|tk| tk.line == f.span.line && tk.col == f.span.col) else {
            return Ok(());
        };
        let last = self.factor_last(first);
        let fix = self.bracket_fix(first, last);
        if self.fix_mode {
            let j = last + 1;
            let nx = self.toks.get(j);
            let k = first as i64 - 2;
            let pv = if k >= 0 { Some(&self.toks[k as usize]) } else { None };
            let combined = nx.is_some_and(|nx| {
                matches!(nx.kind, Kind::Num | Kind::Name | Kind::Imag)
                    || (nx.kind == Kind::Op && ["*", "/", "×", "(", "|"].contains(&nx.s()))
                    || (nx.kind == Kind::Kw && ["sqrt", "cbrt", "integral"].contains(&nx.s()))
            }) || pv.is_some_and(|pv| pv.kind == Kind::Op && ["*", "/", "×"].contains(&pv.s()));
            if !combined {
                self.add_fix(fix);
            }
            return Ok(());
        }
        let num = num.filter(|s| !s.is_empty()).unwrap_or_else(|| Self::num_text_of(q, "2"));
        let ut = if unit.text.trim().is_empty() { f.name.clone() } else { unit.text.trim().to_string() };
        let what = what_of(&f.name);
        let whose = self.whose(&f.name, where_);
        let mut hint = format!("write  {num}*{ut}  for {num} × {whose}, or  {num} [{ut}]  for the unit");
        let k = first as i64 - 1;
        if k >= 2 && self.tk(k - 1).is_op("/") && self.tk(k - 2).kind == Kind::Num && self.tk(k).kind == Kind::Num {
            let (a, b) = (self.tk(k - 2).raw.clone(), self.tk(k).raw.clone());
            let frac = match (a.as_str(), b.as_str()) {
                ("1", "2") => "½".to_string(),
                ("1", "3") => "⅓".to_string(),
                ("1", "4") => "¼".to_string(),
                ("3", "4") => "¾".to_string(),
                _ => format!("({a}/{b})"),
            };
            hint = format!("write  {frac} {ut}  for {a}/{b} × {whose}, or  {a}/{b} [{ut}]  for the unit");
        }
        let mut e = self.error(
            format!("'{num} {ut}' is ambiguous: right after a number, {} is a unit ({what}), but {} is also {whose}",
                    f.name, f.name),
            Some(first),
            Some(hint),
        );
        e.fix = vec![fix];
        Err(e)
    }

    /// `[1, 2, 3] m` when m is also your variable (D192, D222, D235).
    pub fn list_collision(&mut self, open_tok: usize) -> R<()> {
        let f = self.tok().clone();
        let fi = self.i;
        let prev = self.toks[self.i - 1].clone();
        if self.fix_mode {
            self.add_fix((prev.end, f.start, "*".into()));
            return Ok(());
        }
        let mut text = self.text(open_tok, self.i);
        if text.chars().count() > 24 {
            text = "[…]".into();
        }
        let what = what_of(f.s());
        let mut e = self.error(
            format!("'{text} {0}' is ambiguous: after a list, {0} is a unit ({what}), but {0} is also your variable {0}",
                    f.raw),
            Some(fi),
            Some(format!("write  {text}*{0}  for your variable, or  {text} [{0}]  for the unit", f.raw)),
        );
        e.fix = vec![(prev.end, f.start, "*".into())];
        Err(e)
    }

    /// The rule for the names a `where` defines (`0.1 m where m = 2 kg`).
    pub fn check_where_collisions(&mut self, exprs: &[Expr], names: &HashSet<String>) -> R<()> {
        for ex in exprs {
            let nodes: Vec<Expr> = ex.walk().into_iter().cloned().collect();
            for n in &nodes {
                let ExprKind::Quantity { value, unit, bracket } = &n.kind else { continue };
                if *bracket || n.paren || !value.is_num() {
                    continue;
                }
                let fs = &unit.factors;
                if fs.len() == 1 && names.contains(&fs[0].name) {
                    self.single_collision(n, None, true, None)?;
                }
                for f in fs.iter().skip(1) {
                    if names.contains(&f.name) {
                        let first = self.toks.iter().position(|tk| tk.line == fs[0].span.line && tk.col == fs[0].span.col);
                        let ntk = self.toks.iter().position(|tk| tk.line == f.span.line && tk.col == f.span.col);
                        let (Some(first), Some(ntk)) = (first, ntk) else { continue };
                        let last = self.unit_end_from(first);
                        let fix = self.bracket_fix(first, last);
                        if self.fix_mode {
                            self.add_fix(fix);
                            break;
                        }
                        let num = Self::num_text_of(n, "2");
                        let mut e = self.error(
                            format!("'{num} {0}' is ambiguous: right after a number, {0} is one unit, but {1} is also \
                                     the {1} from 'where'", unit.text, f.name),
                            Some(ntk),
                            Some(format!("write  {num} [{}]  for the unit, or put the number and its unit in brackets \
                                          before using {}", unit.text, f.name)),
                        );
                        e.fix = vec![fix];
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// The last token of the unit factor that starts at token j: its exponent (`m²`, `m^-3`, `m^(1/2)`).
    pub fn factor_last(&self, j: usize) -> usize {
        let n = self.toks.len();
        let mut k = j + 1;
        if k < n && self.toks[k].kind == Kind::Sup {
            return k;
        }
        if k < n && self.toks[k].is_op("^") {
            k += 1;
            if self.tk(k as i64).is_op("-") {
                k += 1;
            }
            if self.tk(k as i64).is_op("(") {
                k = self.match_(k.min(n - 1)).filter(|m| *m != 0).unwrap_or(k);
            }
            return k.min(n - 1);
        }
        j
    }

    pub fn bracket_unit(&mut self) -> R<UnitExpr> {
        self.expect_op("[", None)?;
        let u = self.unit_expr(true, false)?;
        self.expect_op("]", Some("to close the unit brackets"))?;
        Ok(u)
    }

    fn unit_factor(&mut self, sign: i64, explicit: bool, factors: &mut Vec<UnitFactor>) -> R<()> {
        if self.at_op("(") {
            self.next();
            let inner = self.unit_expr(true, false)?;
            self.expect_op(")", None)?;
            let exp = self.unit_exponent()?;
            for f in inner.factors {
                let sp = Span { length: if f.span.length == 0 { 1 } else { f.span.length }, ..f.span };
                let Some(e) = exp_mul(f.exp, exp * Rational64::from_integer(sign)) else {
                    let msg = exp_too_large(&format!("{}^({}·{})", f.name, f.exp, exp));
                    return Err(Diagnostic::error(msg.0, sp.line, sp.col, sp.length, Some(msg.1)));
                };
                factors.push(UnitFactor { name: f.name, exp: e, span: sp });
            }
            return Ok(());
        }
        if self.kind() == Kind::Num && self.tok().f() == 1.0 && explicit {
            self.next();
            return Ok(());
        }
        if self.kind() != Kind::Name {
            return Err(self.err(format!("expected a unit name{}", self.found())));
        }
        let nt = self.next();
        self.toks[nt].role = "unit".into();
        let name = self.toks[nt].raw.clone();
        if !explicit {
            self.check_prefix_split(nt);
        }
        let exp = self.unit_exponent()?;
        let ntt = &self.toks[nt];
        factors.push(UnitFactor {
            name,
            exp: exp * Rational64::from_integer(sign),
            span: Span { line: ntt.line, col: ntt.col, length: ntt.rawlen as u32 },
        });
        Ok(())
    }

    fn unit_name_here(&self, k: usize) -> bool {
        let j = (self.i + k).min(self.toks.len() - 1);
        self.toks[j].kind == Kind::Name && self.unit_tok(j)
    }

    /// Parse a unit expression like `kg m/s²`.
    /// explicit=true: inside [...] or after `in` -- every name is a unit.
    /// explicit=false: right after a number -- the first name is a unit; later names are units only if they aren't
    /// variables you've defined (see DECISIONS.md).
    pub fn unit_expr(&mut self, explicit: bool, reciprocal: bool) -> R<UnitExpr> {
        let start = self.i;
        let mut factors: Vec<UnitFactor> = vec![];
        if reciprocal {
            if self.kind() == Kind::Num {
                self.next();
            }
            self.next();
            self.unit_factor(-1, explicit, &mut factors)?;
        } else {
            self.unit_factor(1, explicit, &mut factors)?;
        }
        let mut juxt_join = false;
        loop {
            let t = self.i;
            let tt = self.tok().clone();
            self.no_hour_h(t, explicit, start)?;
            if tt.is_op("/") && self.unit_name_here(1) {
                let pv = self.peek(1).s().to_string();
                if !explicit && self.known.contains(&pv) {
                    if self.later_collision(start, self.i + 1, !tt.ws_before)? {
                        self.next();
                        self.unit_factor(-1, explicit, &mut factors)?;
                        continue;
                    }
                    break;
                }
                self.next();
                self.unit_factor(-1, explicit, &mut factors)?;
            } else if tt.is_op("/")
                && self.peek(1).is_op("(")
                && (explicit || (self.unit_name_here(2) && self.bracket_all_units(self.i + 1)))
            {
                let clash: Vec<usize> = if explicit {
                    vec![]
                } else {
                    let hi = self.match_(self.i + 1).unwrap_or(self.i);
                    (self.i + 2..hi.max(self.i + 2))
                        .filter(|&m| self.toks[m].kind == Kind::Name && self.known.contains(self.toks[m].s()))
                        .collect()
                };
                if !clash.is_empty() && !self.later_collision(start, clash[0], !tt.ws_before)? {
                    break;
                }
                self.next();
                self.unit_factor(-1, explicit, &mut factors)?;
            } else if tt.is_op("/") && explicit && self.peek(1).kind == Kind::Num {
                break;
            } else if tt.is_op("*") && self.unit_name_here(1) && (explicit || tt.raw == "·") {
                // `2 N·m`: the centred dot joins units, like a space; an explicit `*` ends the unit (D235)
                let pv = self.peek(1).s().to_string();
                if !explicit && self.known.contains(&pv) && !self.later_collision(start, self.i + 1, false)? {
                    break;
                }
                self.next();
                self.unit_factor(1, explicit, &mut factors)?;
                juxt_join = true;
            } else if self.unit_name_here(0) && (explicit || !self.is_call_like()) {
                if !explicit && self.peek(1).is_op("(") && !self.peek(1).ws_before {
                    break;
                }
                if !explicit && tt.s() == "c" {
                    break; // `2 m c²`: c continues a unit only after '/' (`MeV/c²`), D235
                }
                if !explicit
                    && self.in_integrand > 0
                    && tt.raw.starts_with('d')
                    && tt.raw.chars().count() > 1
                    && self.known.contains(&canonical_name(&tt.raw[1..]))
                {
                    break; // `∫ 2 [m] dm`: the trailing dm is the differential, not decimetres
                }
                if !explicit && self.known.contains(tt.s()) && !self.later_collision(start, t, false)? {
                    break;
                }
                let mut k = self.i as i64 - 1;
                while k > 0
                    && (self.tk(k).kind == Kind::Sup || (self.tk(k).kind == Kind::Num && self.tk(k - 1).kind == Kind::Op))
                {
                    k -= 1; // step back over an exponent: `W/m² K`
                }
                if self.tk(k - 1).is_op("/") && self.tk(k).kind == Kind::Name {
                    let below = self.unit_span_text(k.max(0) as usize, self.i - 1);
                    let so_far = self.unit_span_text(start, self.i - 1);
                    let d = Diagnostic::warning(
                        format!("'{so_far} {0}' has only {below} below the line: {0} multiplies", tt.raw),
                        tt.line, tt.col, tt.rawlen as u32,
                        Some(format!("for {} below the line too, write /({below} {})", tt.raw, tt.raw)),
                    );
                    self.diags.warn(Diagnostic { length: tt.rawlen as u32, ..d });
                }
                self.unit_factor(1, explicit, &mut factors)?;
                juxt_join = true;
            } else {
                break;
            }
        }
        let utext = self.unit_text(start);
        let text = if reciprocal && utext.starts_with('/') { format!("1{utext}") } else { utext };
        let st = &self.toks[start];
        let length = (self.tk(self.i as i64 - 1).end as i64 - st.start as i64).max(1) as u32;
        let u = UnitExpr { factors, text, span: Span { line: st.line, col: st.col, length }, juxt_join: Some(juxt_join) };
        if let Some(pos) = self.fix_whole.iter().position(|&s| s == start) {
            self.fix_whole.remove(pos);
            let f = self.bracket_fix(start, self.i - 1);
            self.add_fix(f);
        }
        Ok(u)
    }

    /// `E = 1.5 kT` with your own k and T is the unit kilotesla, while `k T` was meant: warn (D203).
    fn check_prefix_split(&mut self, nt: usize) {
        let name = self.toks[nt].raw.clone();
        if is_base_unit(&name) || self.known.contains(&name) || COMMON_PREFIXED.contains(&name.as_str())
            || lookup_unit(&name).is_none()
        {
            return;
        }
        for p in prefixes_longest_first() {
            if !name.starts_with(p) {
                continue;
            }
            let rest = &name[p.len()..];
            if !rest.is_empty() && self.known.contains(p) && self.known.contains(rest) {
                if self.warned_units.contains(&name) {
                    return;
                }
                self.warned_units.insert(name.clone());
                let num = if nt > 0 && self.toks[nt - 1].kind == Kind::Num {
                    self.toks[nt - 1].raw.clone()
                } else {
                    "2".into()
                };
                let what = lookup_unit(&name).unwrap_or("");
                let t = &self.toks[nt];
                let d = Diagnostic::warning(
                    format!("'{num} {name}' is the unit {name} (the prefix {p} on the unit {rest}: a {what}), not your \
                             {p} times your {rest}"),
                    t.line, t.col, t.rawlen as u32,
                    Some(format!("write  {num} {p} {rest}  (with a space) for {num} × {p} × {rest}, or  {num} [{name}]  \
                                  if you mean the unit")),
                );
                let len = t.rawlen as u32;
                self.diags.warn(Diagnostic { length: len, ..d });
                return;
            }
        }
    }

    /// At the '(' at token j after a unit and '/': does the bracket hold only unit names (and exponents, `*`, `/`)?
    pub fn bracket_all_units(&self, j: usize) -> bool {
        let Some(k) = self.match_(j) else { return true };
        for m in j + 1..k {
            let tk = &self.toks[m];
            match tk.kind {
                Kind::Name => {
                    if !is_unit_name(&tk.raw) {
                        return false;
                    }
                }
                Kind::Op => {
                    if !["^", "/", "*", "(", ")", "-"].contains(&tk.s()) {
                        return false;
                    }
                }
                Kind::Sup | Kind::Num => {}
                _ => return false,
            }
        }
        true
    }

    /// `36 km/h` and `[km/h]`: h is Planck's constant, not the hour (D180).
    fn no_hour_h(&mut self, t: usize, explicit: bool, start: usize) -> R<()> {
        if !self.toks[t].is_op("/") {
            return Ok(());
        }
        let h = self.peek(1).clone();
        let hi = (self.i + 1).min(self.toks.len() - 1);
        if h.kind != Kind::Name || h.raw != "h" {
            return Ok(());
        }
        let after = self.peek(2);
        if after.is_op("(") && !after.ws_before {
            return Ok(());
        }
        let unit = self.unit_text(start);
        let num = if !explicit { format!("{} ", self.number_before(start)) } else { String::new() };
        if explicit {
            return Err(self.error(format!("'{unit}/h' isn't a unit: in Fermium h is Planck's constant, not the hour"),
                                  Some(hi), Some(format!("write {unit}/hr for {unit} per hour"))));
        }
        let mine = self.known.contains("h");
        let who = if mine { "your h" } else { "Planck's constant" };
        let msg = if mine {
            format!("'{num}{unit}/h': h is your variable here, and the hour is written hr")
        } else {
            format!("'{num}{unit}/h': in Fermium h is Planck's constant, not the hour, so this would divide by Planck's \
                     constant")
        };
        let e = self.error(msg, Some(hi),
                           Some(format!("write {num}{unit}/hr for {unit} per hour, or  ({num}{unit})/h  to divide by {who}")));
        if self.fix_mode && (self.toks[t].ws_before || h.ws_before) {
            let f = self.bracket_fix(start, self.i - 1);
            self.add_fix(f);
            return Ok(());
        }
        Err(e)
    }

    pub fn unit_text(&self, start: usize) -> String {
        self.text(start, self.i)
    }

    fn unit_exponent(&mut self) -> R<Rational64> {
        let start = self.i;
        let r = self.unit_exponent_raw()?;
        match r {
            Some(p) => Ok(p),
            None => {
                let text = self.text(start, self.i);
                let (msg, hint) = exp_too_large(&format!("a power of {}", text.trim_start_matches('^')));
                // underline the whole exponent, ^ to its last token
                let (a, b) = (&self.toks[start], &self.toks[self.i.saturating_sub(1).max(start)]);
                let len = if a.line == b.line { (b.col + b.rawlen as u32).saturating_sub(a.col).max(1) } else { 1 };
                Err(Diagnostic::error(msg, a.line, a.col, len, Some(hint)))
            }
        }
    }

    /// The exponent after a unit name; None if it doesn't fit in 64 bits (red team 13: `m^1e300` saturated).
    fn unit_exponent_raw(&mut self) -> R<Option<Rational64>> {
        const LIM: f64 = 9.2e18; // below i64::MAX
        if self.kind() == Kind::Sup {
            let s = self.next();
            let v = self.toks[s].int();
            return Ok((v != i64::MIN).then(|| Rational64::from_integer(v)));
        }
        if self.at_op("^") {
            self.next();
            let mut neg = 1;
            if self.at_op("-") {
                self.next();
                neg = -1;
            }
            if self.at_op("(") {
                self.next();
                let mut sgn = 1;
                if self.at_op("-") {
                    self.next();
                    sgn = -1;
                }
                let a = self.next();
                if self.toks[a].kind != Kind::Num {
                    return Err(self.error("expected a number in the unit's exponent", Some(a), None));
                }
                let av = self.toks[a].f();
                let p = if self.at_op("/") {
                    self.next();
                    let b = self.next();
                    let bt = &self.toks[b];
                    let bv = bt.f();
                    if bt.kind != Kind::Num || bv == 0.0 || av != av.trunc() || bv != bv.trunc() {
                        return Err(self.error("a unit's exponent must be a fraction of whole numbers, like m^(1/2)",
                                              Some(b), None));
                    }
                    if av.abs() > LIM || bv.abs() > LIM {
                        self.expect_op(")", None)?;
                        return Ok(None);
                    }
                    Rational64::new(av as i64, bv as i64)
                } else {
                    if av.abs() > LIM {
                        self.expect_op(")", None)?;
                        return Ok(None);
                    }
                    limit_denominator(av, 1000)
                };
                self.expect_op(")", None)?;
                return Ok(Some(p * Rational64::from_integer(sgn * neg)));
            }
            let a = self.next();
            if self.toks[a].kind != Kind::Num {
                return Err(self.error("expected a number after ^ in this unit", Some(a), None));
            }
            let av = self.toks[a].f();
            if av.abs() > LIM {
                return Ok(None);
            }
            return Ok(Some(limit_denominator(av, 1000) * Rational64::from_integer(neg)));
        }
        Ok(Some(Rational64::from_integer(1)))
    }
}

/// What a unit measures, for sentence 3 (`UNIT_WORDS.get(nt.value)` or the unit database by the raw spelling).
fn what_of_value(value: &str, raw: &str) -> String {
    unit_words(value).map(|s| s.to_string()).or_else(|| lookup_unit(raw).map(|s| s.to_string()))
        .unwrap_or_else(|| "a unit".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_den() {
        assert_eq!(limit_denominator(0.5, 1000), Rational64::new(1, 2));
        assert_eq!(limit_denominator(1.0 / 3.0, 1000), Rational64::new(1, 3));
        assert_eq!(limit_denominator(0.333, 1000), Rational64::new(333, 1000));
        assert_eq!(limit_denominator(3.14159265, 1000), Rational64::new(355, 113));
        assert_eq!(limit_denominator(-1.5, 1000), Rational64::new(-3, 2));
        assert_eq!(limit_denominator(2.0, 1000), Rational64::new(2, 1));
    }
}
