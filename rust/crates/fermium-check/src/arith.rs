//! Expressions, part 2: arithmetic. A port of Checker.e_BinOp, text_op, arith, power, e_Neg, e_Compare, approx,
//! e_Logic, e_Not, e_IfExpr, e_Sqrt, e_Abs and the display-hint, significant-figure and temperature helpers
//! from `fermium/checker.py`. Vector, matrix and complex arithmetic are in their own modules.
use num_rational::Rational64;
use num_traits::Zero;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::stmts::{same_kind, ty_dim};
use crate::units::format_number;

// constants whose names look like units: `2 h` is 2 × Planck's constant, not 2 hours
const UNIT_LOOKALIKE_CONSTANTS: &[(&str, &str, &str, &str)] =
    &[("h", "Planck's constant h", "hours", "hr"), ("G", "the gravitational constant G", "gauss", "gauss")];

fn bin(op: I::BinOp, a: I::Expr, b: I::Expr, ty: Ty, line: u32) -> I::Expr {
    ir(I::ExprKind::Bin(op, Box::new(a), Box::new(b)), ty, line)
}

pub fn is_affine(h: &Option<I::Hint>) -> bool {
    h.as_ref().is_some_and(|h| h.offset != 0.0)
}

impl Checker {
    pub fn e_binop(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::BinOp { op, left, right, implicit } = &e.kind else { unreachable!() };
        let (op, implicit) = (op.as_str(), *implicit);
        if op == "^" {
            return self.power(e, left, right, ctx).map(Checked::Val);
        }
        if let Some(lz) = self.leibniz(e, ctx) {
            return self.expr_any(&lz, ctx);
        }
        // 2 h c² is Planck's law, not "2 hours": only a lone `2 h` warns (Python sets left.in_product)
        let in_product = self.in_product.contains(&(e as *const A::Expr));
        if matches!(op, "*" | "/") && matches!(left.kind, A::ExprKind::BinOp { .. }) {
            self.in_product.insert(left.as_ref() as *const A::Expr);
        }
        let a = if implicit { self.expr_any(left, ctx)? } else { Checked::Val(self.expr(left, ctx)?) };
        let a = match a {
            Checked::Val(v) => v,
            other => {
                if implicit && right.paren {
                    let call = crate::ast_ext::mk(A::ExprKind::Call { func: left.clone(), args: vec![(**right).clone()] },
                                            e.span);
                    return self.e_call(&call, ctx);
                }
                self.need_numlike_checked(&other, left, "this value")?;
                unreachable!()
            }
        };
        self.after_number = implicit
            && matches!(left.kind, A::ExprKind::Num { .. } | A::ExprKind::Quantity { .. })
            && matches!(right.kind, A::ExprKind::Name { .. });
        let b = self.expr(right, ctx);
        self.after_number = false;
        let b = b?;
        self.warn_limit_division(e, &b);
        if let (true, A::ExprKind::Num { value, .. }, A::ExprKind::Name { name: rn }) = (implicit, &left.kind, &right.kind) {
            if !in_product {
                if let Some((_, what, unit0, unit1)) = UNIT_LOOKALIKE_CONSTANTS.iter().find(|x| x.0 == rn) {
                    if !matches!(self.lookup(ctx.scope, rn), Some((Binding::Sym(_), _))) {
                        let n = format_number(*value, None);
                        self.warn(format!("{n} {rn} means {n} × {what}; for {unit0} write {n} {unit1}"),
                                  A::Span { length: 1, ..e.span }, None);
                    }
                }
            }
        }
        if matches!(a.ty, Ty::Str) || matches!(b.ty, Ty::Str) {
            return self.text_op(e, op, implicit, left, right, a, b).map(Checked::Val);
        }
        self.need_numlike(&a, left, "this value", true)?;
        self.need_numlike(&b, right, "this value", true)?;
        self.arith(op, a, b, e).map(Checked::Val)
    }

    /// "3p" + "1/2" joins two texts (D216); a number must be turned into text first: str(x).
    #[allow(clippy::too_many_arguments)]
    fn text_op(&mut self, e: &A::Expr, op: &str, implicit: bool, left: &A::Expr, right: &A::Expr,
               a: I::Expr, b: I::Expr) -> CResult<I::Expr> {
        if op != "+" || implicit {
            return Err(self.err("text can only be joined with +, like  \"3p\" + \"1/2\"", e.span, None));
        }
        if !(matches!(a.ty, Ty::Str) && matches!(b.ty, Ty::Str)) {
            let num = if matches!(a.ty, Ty::Str) { right } else { left };
            return Err(self.err("can't add text and a number", num.span,
                                Some(format!("turn the number into text first:  str({})", crate::source::to_source(num)))));
        }
        if let (I::ExprKind::Str(x), I::ExprKind::Str(y)) = (&a.kind, &b.kind) {
            let v = format!("{x}{y}");
            let t = self.text(&v);
            let mut r = ir(I::ExprKind::Str(v), Ty::Str, e.span.line);
            r.extra().text_id = Some(t);
            return Ok(r);
        }
        Ok(ir(I::ExprKind::Builtin("text_concat".into(), vec![a, b]), Ty::Str, e.span.line))
    }

    pub fn arith(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        if matches!(a.ty, Ty::Complex(_)) || matches!(b.ty, Ty::Complex(_)) {
            return self.cplx_arith(op, a, b, e);
        }
        if matches!(a.ty, Ty::Mat { .. }) || matches!(b.ty, Ty::Mat { .. }) {
            return self.mat_arith(op, a, b, e);
        }
        if matches!(a.ty, Ty::Vec { .. }) || matches!(b.ty, Ty::Vec { .. }) {
            return self.vec_arith(op, a, b, e);
        }
        let line = e.span.line;
        let op = if op == "×" { "*" } else { op };
        let is_list = matches!(a.ty, Ty::List(_)) || matches!(b.ty, Ty::List(_));
        let mk = |d: DExpr| if is_list { Ty::List(d) } else { Ty::Num(d) };
        let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
        let sf = if matches!(op, "+" | "-") { sumsf(&[&a, &b]) } else { minsf(&[&a, &b]) };
        let mut r = match op {
            "+" | "-" => {
                if !self.u.unify(&da, &db) {
                    let (sa, sb) = (self.desc(&da), self.desc(&db));
                    let msg = if op == "+" {
                        format!("can't add {sa} to {sb}")
                    } else {
                        format!("can't subtract {sb} from {sa}")
                    };
                    return Err(self.err(msg, e.span, Some("both sides of + and - must have the same units".into())));
                }
                self.warn_confusable_sum(op, &a, &b, e);
                let (aff_a, aff_b) = (is_affine(&a.hint), is_affine(&b.hint));
                if op == "+" && aff_a && aff_b {
                    return Err(self.err(format!("can't add two absolute temperatures ({} + {})",
                                                a.hint.as_ref().unwrap().name, b.hint.as_ref().unwrap().name),
                                        e.span, Some("to add a temperature change, write it in K, e.g. 20 °C + 5 K".into())));
                }
                let bop = if op == "+" { I::BinOp::Add } else { I::BinOp::Sub };
                if op == "-" && aff_b {
                    // the difference of two temperatures is a difference: shown in K (A47)
                    let mut r = bin(bop, a, b, mk(da), line);
                    r.extra().tdelta = true;
                    r
                } else {
                    let hint = a.hint.clone().or_else(|| b.hint.clone());
                    let mut r = bin(bop, a, b, mk(da), line);
                    r.hint = hint;
                    r
                }
            }
            "*" => {
                let hint = self.keep_hint(&a, &b);
                let mut hint = hint;
                if hint.is_some() {
                    hint = self.drop_turn_hint(hint, &a, &b, "*");
                }
                if is_affine(&hint) {
                    let k = if a.hint == hint { &b } else { &a };
                    self.warn_scaled_temperature(hint.as_ref().unwrap(), "×", k, e);
                } else {
                    self.warn_absolute_in_product(&a, &b, e);
                    self.warn_absolute_in_product(&b, &a, e);
                }
                let mut r = bin(I::BinOp::Mul, a, b, mk(da.mul(&db)), line);
                r.hint = hint;
                r
            }
            "/" => {
                let mut hint = if self.dimless(&b) && a.hint.is_some() && b.hint.is_none() { a.hint.clone() } else { None };
                if hint.is_some() {
                    hint = self.drop_turn_hint(hint, &a, &b, "/");
                }
                if is_affine(&hint) {
                    self.warn_scaled_temperature(hint.as_ref().unwrap(), "/", &b, e);
                } else {
                    self.warn_absolute_in_product(&a, &b, e); // ΔT/Δx; b / T (Wien) is absolute by nature
                }
                let mut r = bin(I::BinOp::Div, a, b, mk(da.div(&db)), line);
                r.hint = hint;
                r
            }
            _ => return Err(self.err(format!("unknown operator {op}"), e.span, None)),
        };
        r.sf = sf;
        Ok(r)
    }

    pub fn dimless(&self, v: &I::Expr) -> bool {
        match ty_dim(&v.ty) {
            Some(d) => {
                let d = self.u.norm(&d);
                d.is_concrete() && d.konst.is_dimensionless()
            }
            None => false,
        }
    }

    /// 2π f with f in Hz is an angular frequency, and ω/(2π) with ω in rad/s a frequency: don't keep showing the
    /// old unit (redteam #2).
    fn drop_turn_hint(&self, hint: Option<I::Hint>, a: &I::Expr, b: &I::Expr, op: &str) -> Option<I::Hint> {
        let src_is_a = a.hint == hint;
        let k = if src_is_a { b } else { a };
        let v = match const_value(k) {
            Some(v) if v != 0.0 => v,
            _ => return hint,
        };
        let two_pi = 2.0 * std::f64::consts::PI;
        let kind = crate::units::unit_kind(hint.as_ref().unwrap());
        let mul = op == "*";
        let up = (mul && (v / two_pi - 1.0).abs() < 1e-12) || (!mul && (v * two_pi - 1.0).abs() < 1e-12);
        let down = (!mul && (v / two_pi - 1.0).abs() < 1e-12) || (mul && (v * two_pi - 1.0).abs() < 1e-12);
        if (kind == "cycles" && up) || (kind == "angular" && down) {
            None
        } else {
            hint
        }
    }

    /// Is v an absolute temperature written in °C/°F (not a difference, D12)?
    pub fn abs_temp(&self, v: &I::Expr) -> bool {
        matches!(v.ty, Ty::Num(_) | Ty::List(_)) && is_affine(&v.hint) && !v.get_extra().is_some_and(|x| x.tdelta)
    }

    /// ΔT = 10 °C: a Δ name says "a change", °C says "a reading" (red team round 3 #2, D181).
    pub fn delta_abs_temp_error(&self, name: &str, v: &I::Expr, at: A::Span) -> Diagnostic {
        let u = v.hint.as_ref().unwrap();
        let Some((lit, _)) = v.get_extra().and_then(|x| x.abs_literal.clone()) else {
            return self.err(format!("{name} looks like a temperature change, but this value in {} is an absolute \
                                     temperature (Fermium reads {} values as absolute temperatures in K)", u.name, u.name),
                            at, Some("write a change of temperature in K, or as a difference like T2 - T1".into()));
        };
        let x = format_number(lit, None);
        let k = format_number(lit * u.factor + u.offset, None);
        let step = format_number(lit * u.factor, Some(3));
        self.err(format!("{name} looks like a temperature change, but a value in {} is an absolute temperature ({x} {} \
                          is {k} K)", u.name, u.name), at,
                 Some(format!("write a change of temperature in K ({x} {} → {step} K), or as a difference like T2 - T1",
                              u.name)))
    }

    /// `4186 J/(kg K) * 10 °C`: a °C/°F number written out and multiplied by a quantity with units enters as the
    /// absolute temperature (D181). A variable in °C is not warned about.
    fn warn_absolute_in_product(&mut self, a: &I::Expr, b: &I::Expr, e: &A::Expr) {
        let Some(x) = a.get_extra() else { return };
        let Some((lit, _)) = x.abs_literal.clone() else { return };
        if !self.abs_temp(a) || self.dimless(b) {
            return;
        }
        let u = a.hint.as_ref().unwrap();
        let (xs, k) = (format_number(lit, None), format_number(lit * u.factor + u.offset, None));
        // the `10 °C` itself, not the start of the formula (red team 5 #9)
        let (line, col) = x.abs_at.unwrap_or((e.span.line, e.span.col));
        let span = A::Span { line: if line != 0 { line } else { e.span.line }, col: if col != 0 { col } else { e.span.col },
                             length: 1 };
        let span = match self.abs_at_len.get(&(line, col)) {
            Some(l) => A::Span { length: *l, ..span },
            None => span,
        };
        self.warn(format!("{xs} {} is an absolute temperature, so it enters this formula as {k} K", u.name), span,
                  Some(format!("for a temperature change (ΔT in Q = m c ΔT) write {} K; for an absolute temperature \
                                (p V = n R T) write {k} K to make it clear", format_number(lit * u.factor, Some(3)))));
    }

    /// 2 T or T / 2 with T in °C/°F scales the absolute temperature (in K) (D12, redteam #6).
    fn warn_scaled_temperature(&mut self, u: &I::Hint, op: &str, k: &I::Expr, e: &A::Expr) {
        let f = const_value(k);
        if f == Some(1.0) {
            return; // T * 1 changes nothing
        }
        let ex = if u.name == "°C" { 20.0 } else { 68.0 };
        let kelvin = ex * u.factor + u.offset;
        let fmt = |x: f64| format_number(x, None);
        let example = match f {
            Some(f) if f != 0.0 && f.is_finite() => {
                let res = if op == "×" { kelvin * f } else { kelvin / f };
                let shown = (res - u.offset) / u.factor;
                let (fs, ks) = (fmt(f), fmt(kelvin));
                if op == "×" {
                    format!("{fs} × {} {} is {fs} × {ks} K = {} {}", fmt(ex), u.name, fmt(shown), u.name)
                } else {
                    format!("{} {} / {fs} is {ks} K / {fs} = {} {}", fmt(ex), u.name, fmt(shown), u.name)
                }
            }
            _ => format!("{} {} is {} K, and that is what gets scaled", fmt(ex), u.name, fmt(kelvin)),
        };
        let verb = if op == "×" { "multiplied" } else { "divided" };
        self.warn(format!("this scales an absolute temperature: {} values are {verb} as kelvins ({example})", u.name),
                  A::Span { length: 1, ..e.span }, Some("to scale a temperature change, write it in K".into()));
    }

    /// Scaling by a plain number keeps the unit the user wrote (2 × 3 eV = 6 eV).
    pub fn keep_hint(&self, a: &I::Expr, b: &I::Expr) -> Option<I::Hint> {
        if a.hint.is_some() && self.dimless(b) && b.hint.is_none() {
            return a.hint.clone();
        }
        if b.hint.is_some() && self.dimless(a) && a.hint.is_none() {
            return b.hint.clone();
        }
        None
    }

    /// f(E) = E; f(3 MeV) shows MeV: a result with no unit of its own takes the unit of the first argument of
    /// the same dimension (A40).
    pub fn arg_hint(&self, r: &mut I::Expr, args: &[I::Expr]) {
        let Ty::Num(rd) = &r.ty else { return };
        if r.hint.is_some() {
            return;
        }
        let d = self.u.norm(rd);
        if !d.is_concrete() || d.konst.is_dimensionless() {
            return; // 2 cos(θ) with θ in degrees is a plain number, not an angle (gauntlet O5)
        }
        for a in args {
            if let (Ty::Num(ad), Some(h)) = (&a.ty, &a.hint) {
                if h.offset == 0.0 {
                    let da = self.u.norm(ad);
                    if da.is_concrete() && da.konst == d.konst {
                        r.hint = Some(h.clone());
                        return;
                    }
                }
            }
        }
    }

    pub fn power(&mut self, e: &A::Expr, left: &A::Expr, right: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        // k(x + 1)^2 with a number k means k·(x + 1)², not (k·(x + 1))²
        if let A::ExprKind::Call { func, args } = &left.kind {
            if let (A::ExprKind::Name { name: fname }, 1, false) = (&func.kind, args.len(), left.paren) {
                if matches!(self.lookup(ctx.scope, fname), Some((Binding::Sym(_) | Binding::Const(_), _))) {
                    let inner = crate::ast_ext::mk(A::ExprKind::BinOp { op: "^".into(), left: Box::new(args[0].clone()),
                                                                  right: Box::new(right.clone()), implicit: false },
                                             e.span);
                    let prod = crate::ast_ext::mk(A::ExprKind::BinOp { op: "*".into(), left: func.clone(),
                                                                 right: Box::new(inner), implicit: true }, e.span);
                    return self.expr(&prod, ctx);
                }
            }
        }
        let pconst = const_exponent(right);
        if let A::ExprKind::Name { name: n } = &left.kind {
            if n == "e" && pconst.is_none_or(|p| p <= Rational64::zero()) {
                if matches!(self.lookup(ctx.scope, "e"), Some((Binding::Const(_), _))) {
                    return Err(self.err("e is the elementary charge (1.602×10⁻¹⁹ C) in Fermium", left.span,
                                        Some("for the exponential function write exp(x)".into())));
                }
            }
        }
        let a = self.expr(left, ctx)?;
        if matches!(a.ty, Ty::Complex(_)) {
            return self.cplx_power(e, a, None, ctx);
        }
        self.need_numlike(&a, left, "the base of a power", false)?;
        let is_list = matches!(a.ty, Ty::List(_));
        let mk = |d: DExpr| if is_list { Ty::List(d) } else { Ty::Num(d) };
        let ad = ty_dim(&a.ty).unwrap();
        let line = e.span.line;
        if let Some(p) = pconst {
            let pf = *p.numer() as f64 / *p.denom() as f64;
            let sf = a.sf;
            let hint = if a.hint.is_some() && p == Rational64::from_integer(1) { a.hint.clone() } else { None };
            let mut r = ir(I::ExprKind::PowC(Box::new(a), pf), mk(ad.pow(p)), line);
            r.sf = sf;
            r.hint = hint;
            return Ok(r);
        }
        let b = self.expr(right, ctx)?;
        if matches!(b.ty, Ty::Complex(_)) {
            return self.cplx_power(e, a, Some(b), ctx); // 2^(1i): a complex exponent
        }
        self.need_num(&b, right, "the exponent")?;
        let bd = ty_dim(&b.ty).unwrap();
        if !self.u.unify(&bd, &DExpr::of(DIMLESS)) {
            return Err(self.err(format!("an exponent must be a plain number, but this is {}", self.desc(&bd)),
                                right.span, None));
        }
        if !self.u.unify(&ad, &DExpr::of(DIMLESS)) {
            return Err(self.err(format!("can't raise {} to a power that isn't a fixed number", self.desc(&ad)), e.span,
                                Some("with units, the exponent must be a number written in the program, like x^2 or \
                                      x^(1/3)".into())));
        }
        let sf = minsf(&[&a, &b]);
        let mut r = ir(I::ExprKind::Pow(Box::new(a), Box::new(b)), mk(DExpr::of(DIMLESS)), line);
        r.sf = sf;
        Ok(r)
    }

    pub fn e_neg(&mut self, e: &A::Expr, q: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        if let A::ExprKind::Quantity { value, unit, bracket } = &q.kind {
            if let (A::ExprKind::Num { value: x, sigfigs, digit }, false) = (&value.kind, q.paren) {
                let u = self.resolve_unit(unit)?;
                if u.affine() {
                    // -40 °C is minus forty degrees, not -(313.15 K)
                    let num = crate::ast_ext::mk(A::ExprKind::Num { value: -x, sigfigs: *sigfigs, digit: *digit }, value.span);
                    let neg = crate::ast_ext::mk(A::ExprKind::Quantity { value: Box::new(num), unit: unit.clone(),
                                                                   bracket: *bracket }, e.span);
                    let A::ExprKind::Quantity { value, unit, .. } = &neg.kind else { unreachable!() };
                    return self.e_quantity(&neg, value, unit, ctx);
                }
            }
        }
        let a = self.expr(q, ctx)?;
        self.need_numlike(&a, q, "this value", true)?;
        if is_affine(&a.hint) {
            return Err(self.err(format!("can't negate an absolute temperature ({})", a.hint.as_ref().unwrap().name),
                                e.span, Some("write the negative number directly, like -5 °C, or use K".into())));
        }
        let (hint, sf, direct, ty) = (a.hint.clone(), a.sf, a.direct, a.ty.clone());
        let mut r = ir(I::ExprKind::Neg(Box::new(a)), ty, e.span.line);
        r.hint = hint;
        r.sf = sf;
        r.direct = direct;
        Ok(r)
    }

    pub fn e_compare(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Compare { op, left, right, tol } = &e.kind else { unreachable!() };
        let a = self.expr(left, ctx)?;
        let b = self.expr(right, ctx)?;
        let line = e.span.line;
        let cop = match op.as_str() {
            "==" => I::CmpOp::Eq,
            "!=" => I::CmpOp::Ne,
            "<" => I::CmpOp::Lt,
            ">" => I::CmpOp::Gt,
            "<=" => I::CmpOp::Le,
            ">=" => I::CmpOp::Ge,
            "~=" => return self.approx(e, left, right, tol.as_deref(), a, b, ctx),
            other => return Err(self.err(format!("unknown comparison {other}"), e.span, None)),
        };
        if matches!(a.ty, Ty::Bool) && matches!(b.ty, Ty::Bool) && matches!(cop, I::CmpOp::Eq | I::CmpOp::Ne) {
            return Ok(ir(I::ExprKind::Cmp(cop, Box::new(a), Box::new(b)), Ty::Bool, line));
        }
        if matches!(a.ty, Ty::Complex(_)) || matches!(b.ty, Ty::Complex(_)) {
            return self.cplx_compare(cop, a, b, e);
        }
        self.need_num(&a, left, "each side of a comparison")?;
        self.need_num(&b, right, "each side of a comparison")?;
        let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
        if !self.u.unify(&da, &db) {
            return Err(self.err(format!("can't compare {} with {}", self.desc(&da), self.desc(&db)), e.span, None));
        }
        Ok(ir(I::ExprKind::Cmp(cop, Box::new(a), Box::new(b)), Ty::Bool, line))
    }

    /// a ≈ b [within tol] (D21, D260): Julia's isapprox, |a − b| ≤ max(atol, rtol·max(|a|, |b|)).
    #[allow(clippy::too_many_arguments)]
    fn approx(&mut self, e: &A::Expr, left: &A::Expr, right: &A::Expr, tol: Option<&A::Expr>, a: I::Expr, b: I::Expr,
              ctx: &mut Ctx) -> CResult<I::Expr> {
        let c = matches!(a.ty, Ty::Complex(_)) || matches!(b.ty, Ty::Complex(_));
        let vec = !c && (matches!(a.ty, Ty::Vec { .. }) || matches!(b.ty, Ty::Vec { .. }));
        let (da, db);
        if vec {
            for (v, node) in [(&a, left), (&b, right)] {
                if !matches!(v.ty, Ty::Vec { .. }) {
                    return Err(self.err(format!("≈ compares a vector with a vector of the same length, but this is {}",
                                                self.type_desc(&v.ty)), node.span, None));
                }
            }
            let (Ty::Vec { n: na, .. }, Ty::Vec { n: nb, .. }) = (&a.ty, &b.ty) else { unreachable!() };
            if na != nb {
                return Err(self.err(format!("can't compare a {na}-vector with a {nb}-vector"), e.span, None));
            }
            da = self.shared_dim(&a, "≈", left)?;
            db = self.shared_dim(&b, "≈", right)?;
        } else if !c {
            self.need_num(&a, left, "each side of a comparison")?;
            self.need_num(&b, right, "each side of a comparison")?;
            da = ty_dim(&a.ty).unwrap();
            db = ty_dim(&b.ty).unwrap();
        } else {
            for (v, node) in [(&a, left), (&b, right)] {
                if !matches!(v.ty, Ty::Num(_) | Ty::Complex(_)) {
                    return Err(self.err("each side of a comparison must be a number", node.span, None));
                }
            }
            da = ty_dim(&a.ty).unwrap();
            db = ty_dim(&b.ty).unwrap();
        }
        if !self.u.unify(&da, &db) {
            return Err(self.err(format!("can't compare {} with {}", self.desc(&da), self.desc(&db)), e.span, None));
        }
        let line = e.span.line;
        let zero = ir(I::ExprKind::Const(0.0), Ty::Num(da.clone()), line);
        let (mut atol, mut rtol) = (zero.clone(), ir(I::ExprKind::Const(1e-6), dimless_num(), line));
        if let Some(tol) = tol {
            let t = self.expr(tol, ctx)?;
            self.need_num(&t, tol, "the tolerance after 'within'")?;
            let in_pct = [&a, &b].iter().any(|v| v.hint.as_ref().is_some_and(|h| h.name == "%"));
            let pct = matches!(&tol.kind, A::ExprKind::Quantity { unit, .. }
                               if matches!(unit.text.trim(), "%" | "percent"));
            if pct && !in_pct {
                atol = zero; // within 1%: relative
                rtol = t;
            } else {
                let td = ty_dim(&t.ty).unwrap();
                if !self.u.unify(&td, &da) {
                    return Err(self.err(format!("the tolerance after 'within' must be {}, like the values it compares, \
                                                 but it is {}", self.desc(&da), self.desc(&td)), tol.span,
                                        Some("for a relative tolerance write a percentage, like within 0.1%".into())));
                }
                atol = t;
                rtol = ir(I::ExprKind::Const(0.0), dimless_num(), line);
            }
        }
        if c {
            return self.cplx_approx(a, b, e, atol, rtol);
        }
        Ok(ir(I::ExprKind::Builtin("approx".into(), vec![a, b, atol, rtol]), Ty::Bool, line))
    }

    pub fn e_logic(&mut self, e: &A::Expr, and: bool, left: &A::Expr, right: &A::Expr, ctx: &mut Ctx)
                   -> CResult<I::Expr> {
        let a = self.cond(left, ctx)?;
        let b = self.cond(right, ctx)?;
        Ok(ir(I::ExprKind::Logic { and, a: Box::new(a), b: Box::new(b) }, Ty::Bool, e.span.line))
    }

    pub fn e_not(&mut self, e: &A::Expr, x: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let c = self.cond(x, ctx)?;
        Ok(ir(I::ExprKind::Not(Box::new(c)), Ty::Bool, e.span.line))
    }

    pub fn e_if_expr(&mut self, e: &A::Expr, cond: &A::Expr, then: &A::Expr, other: &A::Expr, ctx: &mut Ctx)
                     -> CResult<I::Expr> {
        let c = self.cond(cond, ctx)?;
        let a = self.expr(then, ctx)?;
        let b = self.expr(other, ctx)?;
        if !same_kind(&a.ty, &b.ty) {
            return Err(self.err("both branches of an if-expression must give the same kind of value", e.span, None));
        }
        match (&a.ty, &b.ty) {
            (Ty::Vec { n, .. }, Ty::Vec { n: n2, .. }) if n != n2 => {
                return Err(self.err("both branches of an if-expression must give vectors of the same length", e.span,
                                    None));
            }
            (Ty::Mat { r, c, .. }, Ty::Mat { r: r2, c: c2, .. }) if (r, c) != (r2, c2) => {
                return Err(self.err("both branches of an if-expression must give matrices of the same size", e.span,
                                    None));
            }
            _ => {}
        }
        let mixed = matches!(a.ty, Ty::Vec { dims: Some(_), .. }) || matches!(b.ty, Ty::Vec { dims: Some(_), .. });
        if matches!(a.ty, Ty::Vec { .. }) && mixed {
            if self.vec_unify(&a.ty, &b.ty).is_some() {
                return Err(self.err(format!("the two branches give {} and {}; they must match", self.type_desc(&a.ty),
                                            self.type_desc(&b.ty)), e.span, None));
            }
        } else if matches!(a.ty, Ty::Num(_) | Ty::List(_) | Ty::Vec { .. } | Ty::Mat { .. }) {
            let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
            self.unify_or(&da, &db, |c| format!("the two branches give {} and {}; they must match", c.desc(&da),
                                                c.desc(&db)), e.span, None)?;
        }
        let hint = a.hint.clone().or_else(|| b.hint.clone());
        let sf = minsf(&[&a, &b]);
        let ty = a.ty.clone();
        let mut r = ir(I::ExprKind::If(Box::new(c), Box::new(a), Box::new(b)), ty, e.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }
}

pub fn minsf(vs: &[&I::Expr]) -> Option<u32> {
    vs.iter().filter_map(|v| v.sf).min()
}

/// Significant figures of a sum or difference: those of the most precise operand (D95).
pub fn sumsf(vs: &[&I::Expr]) -> Option<u32> {
    vs.iter().filter_map(|v| v.sf).max()
}

/// The value of a constant expression built from numbers (2π, 1/2), or None (Python _const_value).
pub fn const_value(x: &I::Expr) -> Option<f64> {
    match &x.kind {
        I::ExprKind::Const(v) => Some(*v),
        I::ExprKind::Bin(op, a, b) => {
            let (a, b) = (const_value(a)?, const_value(b)?);
            match op {
                I::BinOp::Mul => Some(a * b),
                I::BinOp::Div => if b != 0.0 { Some(a / b) } else { None },
                I::BinOp::Add => Some(a + b),
                I::BinOp::Sub => Some(a - b),
            }
        }
        _ => None,
    }
}

/// Python Fraction(x).limit_denominator(max_den), as a Rational64 (None if it doesn't fit: red team 9 #10).
pub fn limit_denominator(x: f64, max_den: i64) -> Option<Rational64> {
    let (p, q) = fermium_ir::pyfrac::limit_denominator(x, max_den)?;
    Some(Rational64::new(i64::try_from(p).ok()?, i64::try_from(q).ok()?))
}

/// Evaluate a compile-time constant exponent (Python const_value): a Fraction or None.
pub fn const_exponent(e: &A::Expr) -> Option<Rational64> {
    match &e.kind {
        A::ExprKind::Num { value, .. } => limit_denominator(*value, 10000),
        A::ExprKind::Neg { operand: x } => const_exponent(x).map(|v| -v),
        A::ExprKind::BinOp { op, left, right, .. } if op != "^" => {
            let (a, b) = (const_exponent(left)?, const_exponent(right)?);
            match op.as_str() {
                "+" => Some(a + b),
                "-" => Some(a - b),
                "*" | "×" => Some(a * b),
                "/" => if b.is_zero() { None } else { Some(a / b) },
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limit_denominator_like_python() {
        // Fraction(1/3).limit_denominator(10000) == 1/3; Fraction(0.1) → 1/10; Fraction(2.5) → 5/2
        assert_eq!(limit_denominator(1.0 / 3.0, 10000), Some(Rational64::new(1, 3)));
        assert_eq!(limit_denominator(0.1, 10000), Some(Rational64::new(1, 10)));
        assert_eq!(limit_denominator(2.5, 10000), Some(Rational64::new(5, 2)));
        assert_eq!(limit_denominator(std::f64::consts::PI, 10000), Some(Rational64::new(355, 113)));
        assert_eq!(limit_denominator(-0.75, 10000), Some(Rational64::new(-3, 4)));
    }
}

/// A number as written in the source (`2.50e19`) for messages (ast.num_text); else formatted like `%g`.
pub fn num_text(n: &A::Expr) -> String {
    if let Some(r) = &n.attrs.raw {
        if !r.is_empty() {
            return r.clone();
        }
    }
    match n.kind {
        A::ExprKind::Num { value, .. } => py_g(value),
        _ => crate::source::to_source(n),
    }
}

/// Python's `f"{x:g}"`.
pub fn py_g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let exp = format!("{:.5e}", x);
    let (m, e) = exp.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    if (-4..6).contains(&e) {
        let decimals = (5 - e).max(0) as usize;
        let s = format!("{:.*}", decimals, x);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        s
    } else {
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}

impl Checker {
    /// -1, -4.0, (-2): a negative number written as a literal, or None (Python _negative_literal).
    fn negative_literal(n: &A::Expr) -> Option<f64> {
        let mut n = n.clone();
        if let A::ExprKind::Neg { operand: q } = &n.kind {
            if let A::ExprKind::Quantity { value, unit, .. } = &q.kind {
                let t = unit.text.trim();
                if matches!(value.kind, A::ExprKind::Num { .. }) && !(t.starts_with('°') || t.starts_with("deg")) {
                    n = crate::ast_ext::mk(A::ExprKind::Neg { operand: value.clone() }, n.span); // `√(-4 m²)` (red team 8 #20)
                }
            }
        }
        match &n.kind {
            A::ExprKind::Neg { operand: q } => match q.kind {
                A::ExprKind::Num { value, .. } if value != 0.0 => Some(-value),
                _ => None,
            },
            A::ExprKind::Num { value, .. } if *value < 0.0 => Some(*value),
            _ => None,
        }
    }

    /// √(-1), log(-1), factorial(-1) written with a literal: a compile-time error instead of NaN (D237).
    pub fn domain_check(&self, name: &str, arg: &A::Expr, e: &A::Expr) -> CResult<()> {
        let Some(v) = Self::negative_literal(arg) else { return Ok(()) };
        if name == "sqrt" || name == "√" {
            let is_neg = matches!(arg.kind, A::ExprKind::Neg { .. });
            let txt = if is_neg { crate::source::to_source(arg) } else { num_text(arg) };
            let simple = matches!(arg.kind, A::ExprKind::Num { .. })
                || matches!(&arg.kind, A::ExprKind::Neg { operand: q } if matches!(q.kind, A::ExprKind::Num { .. }));
            let hint = if simple {
                format!("for the complex square root write  √({txt} + 0i)  (√(-1) is 𝑖)")
            } else {
                "a square root needs a value ≥ 0".into()
            };
            return Err(self.err(format!("√ of a negative number ({txt}) isn't a real number"), e.span, Some(hint)));
        }
        if matches!(name, "log" | "ln" | "log10" | "log2") {
            return Err(self.err(format!("{name} of a negative number isn't a real number"), e.span,
                                Some("the logarithm needs a positive argument".into())));
        }
        if name == "factorial" && v == v.trunc() {
            return Err(self.err(format!("factorial of a negative whole number ({}) isn't defined", v as i64), e.span,
                                Some("factorial(n) needs n ≥ 0".into())));
        }
        Ok(())
    }

    pub fn e_sqrt(&mut self, e: &A::Expr, x: &A::Expr, root: u32, ctx: &mut Ctx) -> CResult<I::Expr> {
        if root == 2 {
            self.domain_check("sqrt", x, e)?;
        }
        let a = self.expr(x, ctx)?;
        if matches!(a.ty, Ty::Complex(_)) {
            if root != 2 {
                return Err(self.err("∛ of a complex number isn't supported; write z^(1/3) for the principal root",
                                    e.span, None));
            }
            return self.cplx_builtin("sqrt", vec![a], e);
        }
        self.need_numlike(&a, x, "the value under the root", false)?;
        let d = ty_dim(&a.ty).unwrap().pow(Rational64::new(1, root as i64));
        let ty = if matches!(a.ty, Ty::List(_)) { Ty::List(d) } else { Ty::Num(d) };
        let sf = a.sf;
        let mut r = ir(I::ExprKind::PowC(Box::new(a), if root == 2 { 0.5 } else { 1.0 / 3.0 }), ty, e.span.line);
        r.sf = sf;
        Ok(r)
    }

    pub fn e_abs(&mut self, e: &A::Expr, x: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let a = self.expr(x, ctx)?;
        if matches!(a.ty, Ty::ComplexList(_)) {
            return self.clist_call("abs", vec![a], e);
        }
        if matches!(a.ty, Ty::Complex(_)) {
            return self.cplx_builtin("abs", vec![a], e);
        }
        self.need_numlike(&a, x, "this value", true)?;
        let line = e.span.line;
        if matches!(a.ty, Ty::Vec { .. }) {
            let d = self.shared_dim(&a, "|v|", e)?;
            let (hint, sf) = (a.hint.clone(), a.sf);
            let mut r = ir(I::ExprKind::Builtin("norm".into(), vec![a]), Ty::Num(d), line);
            r.hint = hint;
            r.sf = sf;
            return Ok(r);
        }
        if matches!(a.ty, Ty::Mat { .. }) {
            return Err(self.err("the value inside |...| must be a number, but it is a matrix", e.span,
                                Some("for the determinant write det(M)".into())));
        }
        let (hint, sf, ty) = (a.hint.clone(), a.sf, a.ty.clone());
        let mut r = ir(I::ExprKind::Builtin("abs".into(), vec![a]), ty, line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }
}
