//! The evaluator, calculus: integrals, sums, roots. A port of the compiled path of Fermium 1.5 (the oracle:
//! `e_IIntegral`, `e_ISum`, `e_IRoot` and the `fm_quad` / `fm_root` kernels in fermium/codegen_llvm.py, with the
//! messages of `Runtime.warn` / `describe_error` in fermium/runtime/core.py); the numerics are fermium-runtime's.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use fermium_ir::{Expr, ExprKind, Module};
use fermium_runtime::numerics::{err as K, quad, roots, Fail};
use fermium_units::numfmt::format_number;

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

thread_local! {
    /// fm.qzero: quiet first tries of a vector integral's components that came out exactly 0 (D110)
    static QZERO: Cell<f64> = const { Cell::new(0.0) };
    /// the fewest significant figures an integral evaluated since the last take_quad_sf() can support
    static QUAD_SF: Cell<Option<u32>> = const { Cell::new(None) };
    /// run-time warnings already shown (each distinct text once, like Runtime.warn_text)
    static WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// The significant figures an integral's result supports (spec B2, OPEN_ITEMS L-2). Summing an integrand
/// that cancels (a large oscillating part, a symmetric zero) leaves a rounding error of about ε ∫|f|, whatever
/// the quadrature's own error estimate says (that estimate is pessimistic after the B2 extrapolations, and it
/// can't see rounding). Only a result below the relative tolerance's reach, ε ∫|f| > 10⁻¹⁰ |value|, is
/// limited, to floor(log10(|value| / (ε ∫|f|))) figures, at least 1: `∫ 1e6 sin(x) + 4e-9 dx from -1 to 1`
/// prints 8×10⁻⁹, not 7.93×10⁻⁹.
pub(crate) fn meaningful_sf(value: f64, _error: f64, abs_sum: f64) -> Option<u32> {
    let u = f64::EPSILON * abs_sum;
    if value == 0.0 || !value.is_finite() || !u.is_finite() || u <= 1e-10 * value.abs() {
        return None;
    }
    Some((value.abs() / u).log10().floor().max(1.0) as u32)
}

/// Take (and clear) the figures limit recorded by the integrals evaluated since the last call.
pub(crate) fn take_quad_sf() -> Option<u32> {
    QUAD_SF.with(|q| q.replace(None))
}

fn note_quad_sf(n: Option<u32>) {
    if let Some(n) = n {
        QUAD_SF.with(|q| q.set(Some(q.get().map_or(n, |m| m.min(n)))));
    }
}

/// An integral, possibly scaled by constants (a unit factor, a sign): its printed value can be limited to the
/// figures the integral supports.
pub(crate) fn integral_shaped(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Integral { .. } => true,
        ExprKind::Neg(a) => integral_shaped(a),
        ExprKind::Bin(fermium_ir::BinOp::Mul | fermium_ir::BinOp::Div, a, b) => {
            (integral_shaped(a) && matches!(b.kind, ExprKind::Const(_)))
                || (integral_shaped(b) && matches!(a.kind, ExprKind::Const(_)))
        }
        _ => false,
    }
}

/// A printed sum or difference whose every operand carries measured precision (significant figures from a
/// written value; whole literals are exact and don't count, and neither do fitted parameters, B-F1), not asked
/// for `to N digits`.
pub(crate) fn measured_sum(e: &Expr) -> bool {
    fn leaves_measured(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Bin(fermium_ir::BinOp::Add | fermium_ir::BinOp::Sub, a, b)
                if matches!(e.ty, fermium_ir::Ty::Num(_)) => leaves_measured(a) && leaves_measured(b),
            _ => e.sf.is_some() && !fermium_ir::uses_fit_sf(e),
        }
    }
    // a temperature keeps v1's rule: its operands may be written in °C or °F, whose decimal places can't be read
    // off the value in kelvin (20.0 °C is 293.15 K; red team 11 #1), and the IR doesn't keep the written unit
    let temperature = matches!(&e.ty, fermium_ir::Ty::Num(d) if d.terms.is_empty()
                               && d.konst == fermium_units::dim::TEMPERATURE);
    matches!(e.kind, ExprKind::Bin(fermium_ir::BinOp::Add | fermium_ir::BinOp::Sub, ..))
        && matches!(e.ty, fermium_ir::Ty::Num(_))
        && !temperature
        && e.direct != 2
        && leaves_measured(e)
}

/// The decimal place (power of ten) of the last significant figure of x with sf figures.
fn last_place(x: f64, sf: u32) -> Option<i32> {
    (x != 0.0 && x.is_finite()).then(|| x.abs().log10().floor() as i32 - sf as i32 + 1)
}

/// The significant figures of x when its last meaningful decimal place is 10^place (at least 1).
pub(crate) fn decimal_rule_sf(x: f64, place: i32) -> Option<u32> {
    (x != 0.0 && x.is_finite()).then(|| (x.abs().log10().floor() as i32 - place + 1).clamp(1, 17) as u32)
}

/// A sum's value and figures when its last meaningful decimal place is 10^place: a result smaller than that
/// place (a cancellation, `12.0 kg - 11.99 kg`) is rounded to it, so it shows only the figures it has (`0 kg`:
/// zero prints as 0, as every zero does; `10.0 m - 9.95 m` is `0.1 m`), red team 10 #7; otherwise the value with decimal_rule_sf's figures.
pub(crate) fn round_to_place(x: f64, place: i32) -> (f64, Option<u32>) {
    if x == 0.0 || !x.is_finite() || x.abs().log10().floor() as i32 >= place {
        return (x, decimal_rule_sf(x, place));
    }
    let r = (x / 10f64.powi(place)).round() * 10f64.powi(place);
    if r == 0.0 {
        // zero to that place: 0.0 (place -1), 0.00 (place -2), 0 (place 0 or coarser)
        (0.0, Some((1 - place).clamp(1, 17) as u32))
    } else {
        (r, decimal_rule_sf(r, place))
    }
}

/// Show a run-time warning once per distinct text (Runtime.warn_text).
pub(crate) fn warn_text(text: &str) {
    let text = if text.starts_with("warning: ") { text.to_string() } else { format!("warning: {text}") };
    WARNED.with(|w| {
        if w.borrow_mut().insert(text.clone()) {
            fermium_runtime::vfs::stderr_line(&text); // stderr (the playground collects it)
        }
    });
}

/// Has a warning starting with this text been shown (Runtime.warn's once-per-solve rule for kind 7)?
pub(crate) fn warned_with_prefix(prefix: &str) -> bool {
    WARNED.with(|w| w.borrow().iter().any(|t| t.starts_with(prefix)))
}

/// A run-time warning at a program line (Runtime.warn): "warning: line N: …".
pub(crate) fn warn_at(line: u32, msg: &str) {
    if line > 0 {
        warn_text(&format!("warning: line {line}: {msg}"));
    } else {
        warn_text(&format!("warning: {msg}"));
    }
}

const QZERO_MSG: &str = "this integral came out as exactly 0 because the integrand was 0 at every point where it was \
                         sampled; if it is non-zero somewhere narrow (a peak in a wide range), integrate over a range \
                         that fits it";

/// Runtime.fmt_value: a number with a print format (units) if one is known, else plain SI.
pub(crate) fn fmt_value(m: &Module, v: f64, fmt: Option<usize>) -> String {
    if let Some(f) = fmt.and_then(|i| m.tables.fmts.get(i)) {
        let hint = f.hint.as_ref().map(|h| fermium_units::Unit { name: h.name.clone(), dim: h.dim, factor: h.factor,
                                                                   offset: h.offset });
        return fermium_units::quantity::format_quantity(v, &f.dim, hint.as_ref(), None, 0, true, true);
    }
    format!("{} (SI units)", format_number(v, 6, true))
}

/// Runtime.describe_error for the calculus kinds.
pub(crate) fn describe(m: &Module, f: Fail, fmt: Option<usize>) -> String {
    let (a, b) = (f.a, f.b);
    match f.kind {
        K::QUAD => format!("couldn't compute this integral numerically: it may diverge (like 1/x at 0) or oscillate \
                            without decaying (like sin(x)/x up to ∞), or the integrand is NaN or ∞ somewhere -- the \
                            estimate was {} ± {} in SI units", format_number(a, 6, true), format_number(b, 6, true)),
        K::QUAD_NAN | K::QUAD_INF => {
            let i = if b == b { b as i64 } else { -1 };
            let name = if i >= 0 && (i as usize) < m.tables.texts.len() { m.tables.texts[i as usize].clone() } else { "x".into() };
            let v = fmt_value(m, a, fmt);
            if f.kind == K::QUAD_NAN {
                format!("the integrand is NaN at {name} = {v} (0/0? ∞/∞? an overflow like exp(710)?), so this integral \
                         can't be computed; rewrite the integrand so it stays finite there, e.g. exp(x) / (exp(x) - 1)² \
                         as exp(-x) / (1 - exp(-x))², or 1 - cos(x) as 2 sin(x/2)²")
            } else {
                format!("couldn't compute this integral: the integrand is infinite at {name} = {v} (1/0? an overflow \
                         like exp(710)?), so it may blow up there (like 1/x at 0); if it shouldn't, rewrite it so it \
                         stays finite, e.g. 1 - cos(x) as 2 sin(x/2)²")
            }
        }
        K::ROOT => format!("this equation has no solution between {} and {}: the two sides never cross there (checked \
                            at 200 points)", fmt_value(m, a, fmt), fmt_value(m, b, fmt)),
        K::POLE => format!("the two sides of this equation jump past each other near {} (like tan at 90°) instead of \
                            crossing: that's not a solution; narrow the range", fmt_value(m, a, fmt)),
        K::STEP => "the step must be a non-zero number that goes from the start towards the end".into(),
        12 => format!("this for loop has no definite number of steps: it goes from {} to {} (NaN in the start, end or \
                       step)", format_number(a, 6, true), format_number(b, 6, true)),
        _ => "runtime error".into(),
    }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn builtin_calculus(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        match name {
            // before a vector integral's first tries: save and clear the count
            "qzero_mark" => Some(Ok(Value::Num(QZERO.with(|q| q.replace(0.0))))),
            // after them: every component was 0 at every sample -> warn once (D110)
            "qzero_check" => {
                let cnt = QZERO.with(|q| q.replace(args[0].num()));
                if cnt == args[1].num() {
                    warn_at(self.line, QZERO_MSG);
                }
                Some(Ok(Value::Num(0.0)))
            }
            _ => None,
        }
    }

    /// lam(x) for a scalar lambda, evaluated in the enclosing frame (its captures are there).
    pub(crate) fn call_scalar_lambda(&mut self, lam: usize, x: f64, fr: &mut Frame) -> Result<f64, RunError> {
        Ok(self.call_lambda_value(lam, Value::Num(x), fr)?.num())
    }

    /// lam(x) for a scalar lambda and any argument (an uncertain one, in v1's interpreter), with the kernel check.
    pub(crate) fn call_lambda_value(&mut self, lam: usize, x: Value, fr: &mut Frame) -> Result<Value, RunError> {
        let m: &'m Module = self.module;
        let l = &m.lambdas[lam];
        let p = l.params[0];
        let old = fr.vars.insert(p, x);
        let r = self.eval(&l.body[0], fr);
        let r = match r {
            // a callback given to a numerical kernel must return plain numbers (interp.plain_fn, D122)
            Ok(v @ (Value::Unc(_) | Value::UList(_))) => Err(crate::eval_unc::kernel_unc_error(&l.name, &v, self.line)),
            r => r,
        };
        match old {
            Some(v) => {
                fr.vars.insert(p, v);
            }
            None => {
                fr.vars.remove(&p);
            }
        }
        r
    }

    /// (value, last significant decimal place in the display unit, k = its SI factor) of a measured sum: the
    /// coarsest place of its operands (the textbook rule).
    fn sum_place(&mut self, e: &Expr, k: f64, fr: &mut Frame) -> Result<(f64, Option<i32>), RunError> {
        if let ExprKind::Bin(op @ (fermium_ir::BinOp::Add | fermium_ir::BinOp::Sub), a, b) = &e.kind {
            if matches!(e.ty, fermium_ir::Ty::Num(_)) {
                let (va, pa) = self.sum_place(a, k, fr)?;
                let (vb, pb) = self.sum_place(b, k, fr)?;
                let v = self.bin(*op, Value::Num(va), Value::Num(vb))?.num();
                let p = match (pa, pb) {
                    (Some(x), Some(y)) => Some(x.max(y)),
                    _ => None,
                };
                return Ok((v, p));
            }
        }
        let v = self.eval(e, fr)?.num();
        Ok((v, e.sf.and_then(|s| last_place(v / k, s))))
    }

    /// The value of a printed measured sum and its significant figures by the decimal-place rule (spec B2).
    pub(crate) fn sum_sf(&mut self, e: &Expr, fmt: usize, fr: &mut Frame) -> Result<(f64, Option<u32>), RunError> {
        let k = match self.module.tables.fmts.get(fmt) {
            Some(f) => {
                let hint = f.hint.as_ref().map(|h| fermium_units::Unit { name: h.name.clone(), dim: h.dim,
                                                                           factor: h.factor, offset: h.offset });
                fermium_units::display_unit(&f.dim, hint.as_ref()).factor
            }
            None => 1.0,
        };
        let (v, p) = self.sum_place(e, k, fr)?;
        let x = v / k;
        Ok(match p {
            Some(p) => {
                let (x, sf) = round_to_place(x, p);
                (x * k, sf)
            }
            None => (v, None),
        })
    }

    fn fail(&self, f: Fail, fmt: Option<usize>) -> RunError {
        RunError { message: describe(self.module, f, fmt), line: self.line, hint: None }
    }

    pub(crate) fn eval_calculus(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        let line0 = self.line;
        let r = self.eval_calculus_in(e, fr);
        match &e.kind {
            ExprKind::Integral { .. } | ExprKind::Root { .. } => crate::eval_unc::kernel_line(r, line0),
            _ => r,
        }
    }

    fn eval_calculus_in(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::Integral { lam, lo, hi, xname, xfmt, soft, atol } => {
                let (va, vb) = (self.eval(lo, fr)?, self.eval(hi, fr)?);
                let unc_limits = matches!(va, Value::Unc(_)) || matches!(vb, Value::Unc(_));
                if unc_limits {
                    // v1's quad computes its nodes from uncertain limits, so the integrand meets an uncertain x: the
                    // plain-number error, or (an integrand of x returns an uncertain value) the kernel's "an integral
                    // can't use uncertain values"; one that doesn't use x stays plain (see below)
                    let mid = self.unc_bin(fermium_ir::BinOp::Add, va.clone(), vb.clone())?;
                    let mid = self.unc_bin(fermium_ir::BinOp::Mul, mid, Value::Num(0.5))?;
                    self.call_lambda_value(*lam, mid, fr)?;
                }
                let (a, b) = (va.num(), vb.num());
                let atol = if *soft {
                    -1.0
                } else {
                    match atol {
                        Some(t) => self.eval(t, fr)?.num(),
                        None => 0.0,
                    }
                };
                let line = self.line;
                let mut error: Option<RunError> = None;
                let r = quad::quad(
                    |x| {
                        // after an error, zeros: the quadrature then ends at once (NaN can keep an infinite range
                        // subdividing for minutes); its result is dropped for the error
                        if error.is_some() {
                            return 0.0;
                        }
                        match self.call_scalar_lambda(*lam, x, fr) {
                            Ok(v) => v,
                            Err(ex) => {
                                error = Some(ex);
                                0.0
                            }
                        }
                    },
                    a,
                    b,
                    1e-10,
                    atol,
                    xname.map(|i| i as f64).unwrap_or(-1.0),
                );
                self.line = line;
                if let Some(ex) = error {
                    return Err(ex);
                }
                let r = r.map_err(|f| self.fail(f, *xfmt))?;
                note_quad_sf(meaningful_sf(r.value, r.error, r.abs_sum));
                if r.all_zero {
                    if atol < 0.0 {
                        QZERO.with(|q| q.set(q.get() + 1.0));
                    } else {
                        warn_at(line, QZERO_MSG);
                    }
                }
                if unc_limits && a != b {
                    // an integrand that doesn't use x, with uncertain limits: v1's quad arithmetic propagates them
                    // through the nodes' spacing only (the integrand's values stay plain), so the result is
                    // linear in b - a: d∫ = ∫/(b - a) (db - da)
                    use fermium_ir::BinOp::{Mul, Sub};
                    let k = crate::eval::fdiv(r.value, b - a);
                    let span = self.unc_bin(Sub, vb, va)?;
                    if let Value::Unc(u) = self.unc_bin(Mul, Value::Num(k), span)? {
                        let d = u.d.clone();
                        return Ok(Value::Unc(std::rc::Rc::new(fermium_runtime::numerics::uncertain::UFloat::new(r.value, d))));
                    }
                }
                Ok(Value::Num(r.value))
            }
            ExprKind::Sum { lam, lo, hi, step } => {
                let va = self.eval(lo, fr)?;
                let vb = self.eval(hi, fr)?;
                let vst = match step {
                    Some(s) => self.eval(s, fr)?,
                    None => Value::Num(1.0),
                };
                let (a, b, st) = (va.num(), vb.num(), vst.num());
                if st == 0.0 || st != st {
                    return Err(self.fail(Fail::new(K::STEP, st, 0.0), None));
                }
                // uncertain limits: interp.e_ISum takes math.floor of an uncertain span (a plain number is needed)
                let span = crate::eval::fdiv(b - a, st);
                if [&va, &vb, &vst].iter().any(|v| matches!(v, Value::Unc(_))) && span.is_finite() {
                    return self.err(crate::eval_unc::GENERIC);
                }
                let mut cnt = ((b - a) / st + 1e-9).floor() + 1.0;
                if cnt < 0.0 {
                    cnt = 0.0;
                }
                if cnt != cnt {
                    return Err(self.fail(Fail::new(12, a, b), None));
                }
                let n = cnt.min(2f64.powi(62)) as i64;
                if self.module.uses_uncertainty {
                    return self.unc_sum(*lam, a, st, n, fr); // the terms may be uncertain (interp.e_ISum)
                }
                let line = self.line;
                let mut acc = 0.0;
                for i in 0..n {
                    let k = a + i as f64 * st;
                    acc += self.call_scalar_lambda(*lam, k, fr)?;
                }
                self.line = line;
                Ok(Value::Num(acc))
            }
            ExprKind::Root { lam, lo, hi, scale, tfmt } => {
                let (va, vb) = (self.eval(lo, fr)?, self.eval(hi, fr)?);
                // an uncertain bracket: v1's root finder calls the function at uncertain points (the kernel check)
                let at = if matches!(va, Value::Unc(_)) {
                    Some(va.clone())
                } else if matches!(vb, Value::Unc(_)) && self.call_scalar_lambda(*lam, va.num(), fr)? != 0.0 {
                    Some(vb.clone())
                } else {
                    None
                };
                if let Some(x) = at {
                    self.call_lambda_value(*lam, x.clone(), fr)?;
                    let name = &self.module.lambdas[*lam].name;
                    return Err(crate::eval_unc::kernel_unc_error(name, &x, self.line));
                }
                let (a, b) = (va.num(), vb.num());
                let line = self.line;
                // the lambdas share the frame: evaluate through a RefCell so the scale closure can too
                let error: RefCell<Option<RunError>> = RefCell::new(None);
                let me = RefCell::new((&mut *self, &mut *fr));
                let call = |l: usize, x: f64| -> f64 {
                    if error.borrow().is_some() {
                        return f64::NAN;
                    }
                    let mut g = me.borrow_mut();
                    let (s, f) = &mut *g;
                    match s.call_scalar_lambda(l, x, f) {
                        Ok(v) => v,
                        Err(ex) => {
                            *error.borrow_mut() = Some(ex);
                            f64::NAN
                        }
                    }
                };
                let mut sc = |x: f64| call(scale.unwrap(), x);
                let r = roots::root(|x| call(*lam, x), a, b, 200,
                                    if scale.is_some() { Some(&mut sc as &mut dyn FnMut(f64) -> f64) } else { None });
                drop(me);
                self.line = line;
                if let Some(ex) = error.into_inner() {
                    return Err(ex);
                }
                let r = r.map_err(|f| self.fail(f, *tfmt))?;
                if let Some(x) = r.noise_warning {
                    warn_at(line, &format!("the two sides of this equation agree only to rounding error near {}, so \
                                            the solution found there may be meaningless (large terms cancelling?); \
                                            rewrite the equation so they cancel on paper",
                                           fmt_value(self.module, x, *tfmt)));
                }
                Ok(Value::Num(r.x))
            }
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decimal_rule_sf, last_place, meaningful_sf, round_to_place};

    #[test]
    fn cancellations_round_to_the_coarsest_place() {
        // red team 10 #7: 12.0 kg - 11.99 kg is 0.0 kg, 10.0 m - 9.95 m is 0.1 m
        let p = last_place(12.0, 3).unwrap().max(last_place(11.99, 4).unwrap());
        assert_eq!(round_to_place(12.0 - 11.99, p), (0.0, Some(2)));
        let p = last_place(10.0, 3).unwrap().max(last_place(9.95, 3).unwrap());
        let (r, sf) = round_to_place(10.0 - 9.95, p);
        assert!((r - 0.1).abs() < 1e-15 && sf == Some(1));
        assert_eq!(round_to_place(1200.0 - 1200.0 + 1.0, 2), (0.0, Some(1)));
        // not a cancellation: unchanged
        assert_eq!(round_to_place(3.2, -1), (3.2, Some(2)));
    }

    #[test]
    fn sums_keep_the_coarsest_decimal_place() {
        // 293.15 K + 0.5 K: places −2 and −1 → 293.65 to the tenths, 4 figures (293.6 K; 293.65 is 293.6499…)
        let p = last_place(293.15, 5).unwrap().max(last_place(0.5, 1).unwrap());
        assert_eq!(p, -1);
        assert_eq!(decimal_rule_sf(293.65, p), Some(4));
        // 1.20 m + 2.0 m → 3.2 m; 938.272 MeV + 2.2 MeV → 940.5 MeV; 0.1 + 0.2 → 0.3
        assert_eq!(decimal_rule_sf(3.2, last_place(1.2, 3).unwrap().max(last_place(2.0, 2).unwrap())), Some(2));
        assert_eq!(decimal_rule_sf(940.472, last_place(938.272, 6).unwrap().max(last_place(2.2, 2).unwrap())), Some(4));
        assert_eq!(decimal_rule_sf(0.30000000000000004, last_place(0.1, 1).unwrap().max(last_place(0.2, 1).unwrap())),
                   Some(1));
        // 1.00 m − 0.999 m: nothing is left at the hundredths, so one figure
        assert_eq!(decimal_rule_sf(0.0010000000000000009, -2), Some(1));
    }

    #[test]
    fn rounding_level_integrals_keep_only_their_meaningful_figures() {
        // ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1: 7.93×10⁻⁹ with ∫|f| = 9.19×10⁵ → 1 figure (8×10⁻⁹, exact 8×10⁻⁹)
        assert_eq!(meaningful_sf(7.930793799459934e-9, 5.82e-11, 919395.39), Some(1));
        // an integral that converged to its relative tolerance keeps every figure
        assert_eq!(meaningful_sf(0.3333333333333333, 5.6e-17, 0.3333333333333333), None);
        assert_eq!(meaningful_sf(1.0, 1e-11, 1.0), None);
        // a large error estimate alone (a singular integrand) doesn't limit the figures
        assert_eq!(meaningful_sf(2.7687651131, 1e-7, 2.7687651131), None);
        // a symmetric zero: rounding noise, one figure
        assert_eq!(meaningful_sf(2.7755575615628914e-17, 1.4e-17, 0.9193953882637206), Some(1));
    }
}
