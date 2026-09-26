//! Unit conversion and display: a port of Checker.e_Convert, _export, _warn_angle_in_hz, _warn_confusable_sum,
//! _warn_omega_in_hz, _warn_limit_division, e_Digits, and the module functions canonical_unit_name,
//! hz_angle_mixup and _unit_name_suggestion from `fermium/checker.py`.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;
use num_rational::Rational64;
use num_traits::{Signed, Zero};

use crate::checker::*;
use crate::exprs::hint_of;
use crate::stmts::ty_dim;
use crate::units::{self, format_number, unit_kind, unit_words, Unit};

const ANGLE_WORDS: &[&str] = &["rev", "rpm", "rad", "°", "deg", "arcmin", "arcsec"];

/// The display spelling of a written unit, independent of how it was typed (Python canonical_unit_name):
/// `ft/s^2`, `ft/s²` and `ft s^-2` all display as `ft/s²`; `N·m` as `N m`; `m m` as `m²`.
pub fn canonical_unit_name(uexpr: &A::UnitExpr) -> String {
    let mut order: Vec<String> = vec![];
    let mut exps: Vec<Rational64> = vec![];
    for f in &uexpr.factors {
        let mut name = fermium_units::unit_pretty(&f.name).map(str::to_string).unwrap_or_else(|| f.name.clone());
        let chars: Vec<char> = name.chars().collect();
        if (name.starts_with('u') || name.starts_with('µ')) && chars.len() > 1 {
            let rest: String = chars[1..].iter().collect();
            let mu = format!("μ{rest}");
            if let (Some(a), Some(b)) = (units::lookup_unit(&mu), units::lookup_unit(&name)) {
                if rest != "n" && a.factor == b.factor {
                    name = mu;
                }
            }
        }
        match order.iter().position(|n| *n == name) {
            Some(i) => exps[i] += f.exp,
            None => {
                order.push(name);
                exps.push(f.exp);
            }
        }
    }
    let num: Vec<String> = order.iter().zip(&exps).filter(|(_, e)| e.is_positive())
        .map(|(n, e)| format!("{n}{}", fermium_units::fmt_exp(*e, true))).collect();
    let den: Vec<String> = order.iter().zip(&exps).filter(|(_, e)| e.is_negative())
        .map(|(n, e)| format!("{n}{}", fermium_units::fmt_exp(-*e, true))).collect();
    let _ = Rational64::zero();
    fermium_units::join_units(&num, &den)
}

/// The hint for `in M_sun` and other near-miss unit names (Python _unit_name_suggestion, gauntlet #79).
pub fn unit_name_suggestion(name: &str) -> String {
    let all: Vec<&str> = fermium_units::unit_names()
        .into_iter()
        .filter(|u| !fermium_units::AFFINE.iter().any(|a| a.0 == *u))
        .collect();
    let key = crate::names::loose(name);
    let same: Vec<&str> = all.iter().copied().filter(|u| crate::names::loose(u) == key).collect();
    if !same.is_empty() {
        let pretty = same.iter().copied().find(|u| fermium_units::unit_ascii(u).is_some()).unwrap_or(same[0]);
        let ascii = fermium_units::unit_ascii(pretty);
        let spelled = match ascii {
            Some(a) if a != pretty => format!("{pretty} (ASCII {a})"),
            _ => pretty.to_string(),
        };
        if units::constant(name).is_some() {
            return format!("{name} is the constant; the unit is {spelled}: write  in {pretty}  (or divide by the \
                            constant: x / {name})");
        }
        return format!("did you mean the unit {spelled}?");
    }
    let poss: Vec<String> = all.iter().filter(|u| u.chars().count() > 1).map(|u| u.to_string()).collect();
    let close = crate::names::get_close_matches(name, &poss, 1, 0.8);
    if let Some(c) = close.first() {
        return format!("did you mean {c}? (see the units list in docs/reference.md)");
    }
    String::new()
}

/// (message, hint) when a value in src is converted to dst across Hz and rev/rpm/°/s (Python hz_angle_mixup).
pub fn hz_angle_mixup(src: Option<&I::Hint>, dst: &I::Hint) -> Option<(String, String)> {
    let src = src?;
    let (ks, kd) = (unit_kind(src), unit_kind(dst));
    if !((ks == "cycles" && kd == "angular") || (ks == "angular" && kd == "cycles")) {
        return None;
    }
    if dst.factor == 0.0 {
        return None;
    }
    let actual = src.factor / dst.factor;
    let (s, t) = (&src.name, &dst.name);
    if ks == "angular" && !unit_words(&src.name).iter().any(|w| w != "rad" && ANGLE_WORDS.contains(&w.as_str())) {
        return None; // rad/s in Hz: _warn_omega_in_hz says it
    }
    let fmt = |x: f64| format_number(x, None);
    if ks == "cycles" {
        let expected = actual * 2.0 * std::f64::consts::PI;
        let msg = format!("Hz here means rad/s (angles are plain numbers, 1 rev = 2π), so 1 {s} is {} {t}, not {} {t}{}",
                          fmt(actual), fmt(expected), if t == "rev/s" { "" } else { "; 1 Hz = 1/(2π) rev/s" });
        let hint = format!("if the value counts cycles per second, write it in rev/s instead of Hz (50 rev/s is \
                            3000 rpm), or multiply it by 2π:  2π f in {t}");
        Some((msg, hint))
    } else {
        let expected = actual / (2.0 * std::f64::consts::PI);
        let msg = format!("angles are plain numbers (1 rev = 2π), so a rate in {s} shown in {t} is an angular \
                           frequency: 1 {s} is {} {t} here, not {} {t}", fmt(actual), fmt(expected));
        let hint = format!("to count turns per second write  in rev/s ; for the frequency in cycles per second \
                            divide by 2π:  ω/(2π) in {t}");
        Some((msg, hint))
    }
}

/// `(^|[^A-Za-z])[kMGT]?Hz\b` (Python's re.search in _warn_omega_in_hz).
fn has_hz(name: &str) -> bool {
    let c: Vec<char> = name.chars().collect();
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    for i in 0..c.len() {
        if c[i] != 'H' || c.get(i + 1) != Some(&'z') {
            continue;
        }
        if c.get(i + 2).is_some_and(|&ch| word(ch)) {
            continue;
        }
        let ok_before = |j: usize| j == 0 || !c[j - 1].is_ascii_alphabetic();
        if ok_before(i) {
            return true;
        }
        if i > 0 && "kMGT".contains(c[i - 1]) && ok_before(i - 1) {
            return true;
        }
    }
    false
}

impl Checker {
    /// `x in unit`.
    pub fn e_convert(&mut self, e: &A::Expr, value: &A::Expr, unit: &A::UnitExpr, ctx: &mut Ctx) -> CResult<I::Expr> {
        if let Some(r) = self.export(e, value, unit, ctx)? {
            return Ok(r);
        }
        let mut v = self.expr(value, ctx)?;
        let is_cl = matches!(v.ty, Ty::ComplexList(_));
        if !is_cl {
            // fft(xs) in mV: a list of complex numbers converts as a whole (D243)
            self.need_numlike(&v, value, "the value to convert", true)?;
        }
        let mut u = self.resolve_unit(unit)?;
        if is_cl && u.affine() {
            return Err(self.err(format!("can't show complex numbers in {}", u.name), e.span, Some("use K".into())));
        }
        if matches!(v.ty, Ty::Vec { dims: Some(_), .. }) {
            return Err(self.err(format!("can't show a vector with different units per component in {}", u.name),
                                e.span, Some("convert one component at a time, like s.x in cm".into())));
        }
        let vd = ty_dim(&v.ty).unwrap();
        if !self.u.unify(&vd, &DExpr::of(u.dim)) {
            let mut hint = "the units you convert to must measure the same kind of quantity".to_string();
            if let Some((what, us)) = fermium_units::suggest_units(&self.u.resolve(&vd)) {
                // "h c is energy × length; try in J m or in eV nm" (spec A3.3)
                let text = crate::source::to_source(value);
                let text = if text.chars().count() <= 24 { text } else { "this".to_string() };
                let tries = us.iter().map(|x| format!("in {x}")).collect::<Vec<_>>().join("  or  ");
                hint = format!("{text} is {what}; try  {tries}");
            }
            if self.natural() {
                hint = self.natural_convert_hint();
            }
            return Err(self.err(format!("can't show {} in {} ({})", self.desc(&vd), u.name, self.desc(&DExpr::of(u.dim))),
                                e.span, Some(hint)));
        }
        let uh = hint_of(&u);
        if !self.warn_angle_in_hz(&v, &uh, e, "") {
            self.warn_omega_in_hz(value, &v, &u, e);
        }
        if u.affine() && v.get_extra().is_some_and(|x| x.tdelta) {
            // a difference of temperatures shown in °C/°F: no offset (gauntlet friction #6)
            u = Unit::new(u.name.clone(), u.dim, u.factor);
            self.warn(format!("this is a difference of two temperatures, so it is shown in {} without the offset (a \
                               change of 1 °C is 1 K)", u.name), A::Span { length: 1, ..e.span }, Some("write it  in K  to make that clear".into()));
        }
        v.hint = Some(hint_of(&u));
        v.direct = 0;
        Ok(v)
    }

    /// The hint for a failed conversion in natural units (the system's name and constants).
    fn natural_convert_hint(&self) -> String {
        let consts = self.system_consts(&self.nat);
        let name = self.nat_name();
        if self.nat.consts().contains(&"G") {
            return format!("in {name} units ({consts} = 1) a mass, a length and a time are all measured in the same \
                            unit; check the powers");
        }
        format!("in {name} units ({consts} = 1) a length or time is 1/energy and a mass is an energy; check the powers \
                 of energy")
    }

    /// Converting between Hz and rev, rpm, rad/s or °/s (either way): 1 Hz is 9.55 rpm, not 60 (D95).
    pub fn warn_angle_in_hz(&mut self, v: &I::Expr, u: &I::Hint, e: &A::Expr, where_: &str) -> bool {
        let Some((msg, hint)) = hz_angle_mixup(v.hint.as_ref(), u) else { return false };
        self.warn(format!("{where_}{msg}"), A::Span { length: 1, ..e.span }, Some(hint));
        true
    }

    /// 1 Gy + 1 Sv, 1 Bq + 1 Hz, 1 Hz + 1 rad/s: the same SI dimension, different things (redteam #2, #10).
    pub fn warn_confusable_sum(&mut self, op: &str, a: &I::Expr, b: &I::Expr, e: &A::Expr) {
        let (Some(ha), Some(hb)) = (&a.hint, &b.hint) else { return };
        let (ka, kb) = (unit_kind(ha), unit_kind(hb));
        if ka.is_empty() || kb.is_empty() || ka == kb {
            return;
        }
        let pair = |x: &str, y: &str| (ka == x && kb == y) || (ka == y && kb == x);
        let (msg, hint): (&str, &str) = if pair("cycles", "angular") {
            ("a value in {a} and one in {b}: Fermium treats rad as 1, so Hz and rad/s are the same unit and they add \
              as if 1 Hz were 1 rad/s", "if the Hz value counts cycles per second, multiply it by 2π first")
        } else if pair("Bq", "cycles") {
            ("a value in {a} and one in {b}: both are 1/s in SI, but becquerels count decays and hertz count cycles",
             "check that both mean the same thing")
        } else if pair("Bq", "angular") {
            ("a value in {a} and one in {b}: both are 1/s in SI, but becquerels count decays and {b} is an angular \
              rate", "check that both mean the same thing")
        } else if pair("Gy", "Sv") {
            ("a value in {a} and one in {b}: both are J/kg in SI, but grays measure absorbed dose and sieverts \
              equivalent dose (weighted by the radiation type)",
             "convert the absorbed dose with the radiation weighting factor first (H = w_R D)")
        } else if pair("energy", "torque") {
            ("a value in {a} and one in {b}: both are kg m²/s² in SI, but J is an energy and N m a torque",
             "check that both mean the same thing")
        } else {
            return;
        };
        let verb = if op == "+" { "adding" } else { "subtracting" };
        let msg = msg.replace("{a}", &ha.name).replace("{b}", &hb.name);
        let span = A::Span { length: 1, ..e.span };
        self.warn(format!("{verb} {msg}"), span, Some(hint.to_string()));
    }

    /// `ω in Hz` shows ω itself (rad/s and Hz are both 1/s), not ω/2π: warn (gauntlet friction #5).
    fn warn_omega_in_hz(&mut self, node: &A::Expr, v: &I::Expr, u: &Unit, e: &A::Expr) {
        if !has_hz(&u.name) {
            return;
        }
        let name = match &node.kind {
            A::ExprKind::Name { name } => Some(name.clone()),
            _ => None,
        };
        let hname = v.hint.as_ref().map(|h| h.name.clone()).unwrap_or_default();
        let omega = name.as_ref().is_some_and(|n| {
            n.starts_with('ω') || n.starts_with('Ω') || n.starts_with("omega") || n.starts_with("Omega")
        });
        if omega || hname.contains("rad") {
            let what = name.unwrap_or_else(|| "this angular frequency".into());
            let un = &u.name;
            self.warn(format!("{what} in {un} shows the angular frequency itself (Fermium treats rad as 1, so rad/s \
                               and Hz are the same unit), not the frequency {what}/2π"), A::Span { length: 1, ..e.span },
                      Some(format!("for the frequency in cycles per second write  {what}/(2π) in {un}  (or  {what} in \
                                    rad/s  to keep it angular)")));
        }
    }

    /// D34/D112: a spaced '/' right after an integral's upper limit divides the whole integral; warn only when the
    /// divisor is a plain number (Python _warn_limit_division, D205).
    pub fn warn_limit_division(&mut self, e: &A::Expr, b: &I::Expr) {
        let A::ExprKind::BinOp { op, right, .. } = &e.kind else { return };
        let Some(r) = &right.attrs.limit_div_of else { return };
        if op != "/" || !matches!(b.ty, Ty::Num(_) | Ty::List(_)) || !self.dimless(b) {
            return;
        }
        let tok = self.node(r.id).and_then(|n| n.attrs.div_info.as_ref()).and_then(|d| d.tok);
        let Some((line, col)) = tok else { return };
        self.warn("the ' / ' after the upper limit divides the whole integral, not the limit",
                  A::Span { line, col, length: 1 },
                  Some("to divide the limit, write it without spaces (to L/2) or in parentheses (to (L / 2))".into()));
    }

    /// `x to N digits`.
    pub fn e_digits(&mut self, e: &A::Expr, value: &A::Expr, digits: u32, ctx: &mut Ctx) -> CResult<I::Expr> {
        let mut v = self.expr(value, ctx)?;
        if !matches!(v.ty, Ty::Num(_) | Ty::List(_) | Ty::Vec { .. } | Ty::Mat { .. } | Ty::Complex(_) | Ty::ComplexList(_)) {
            return Err(self.err("'to N digits' only works on numbers", e.span, None));
        }
        if !(1..=17).contains(&digits) {
            return Err(self.err("the number of digits must be between 1 and 17", e.span, None));
        }
        v.sf = Some(digits);
        v.direct = 2; // asked for (2), not written as a literal (1): lists use exactly sf digits
        v.extra().no_echo = true; // no "(= … SI)" echo for a value printed to chosen digits (friction #31)
        Ok(v)
    }
}
