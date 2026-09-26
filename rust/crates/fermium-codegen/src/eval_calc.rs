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

/// Show a run-time warning once per distinct text (Runtime.warn_text).
pub(crate) fn warn_text(text: &str) {
    let text = if text.starts_with("warning: ") { text.to_string() } else { format!("warning: {text}") };
    WARNED.with(|w| {
        if w.borrow_mut().insert(text.clone()) {
            eprintln!("{text}");
        }
    });
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
        let m: &'m Module = self.module;
        let l = &m.lambdas[lam];
        let p = l.params[0];
        let old = fr.vars.insert(p, Value::Num(x));
        let r = self.eval(&l.body[0], fr);
        match old {
            Some(v) => {
                fr.vars.insert(p, v);
            }
            None => {
                fr.vars.remove(&p);
            }
        }
        Ok(r?.num())
    }

    fn fail(&self, f: Fail, fmt: Option<usize>) -> RunError {
        RunError { message: describe(self.module, f, fmt), line: self.line, hint: None }
    }

    pub(crate) fn eval_calculus(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::Integral { lam, lo, hi, xname, xfmt, soft, atol } => {
                let a = self.eval(lo, fr)?.num();
                let b = self.eval(hi, fr)?.num();
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
                        if error.is_some() {
                            return f64::NAN;
                        }
                        match self.call_scalar_lambda(*lam, x, fr) {
                            Ok(v) => v,
                            Err(ex) => {
                                error = Some(ex);
                                f64::NAN
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
                Ok(Value::Num(r.value))
            }
            ExprKind::Sum { lam, lo, hi, step } => {
                let a = self.eval(lo, fr)?.num();
                let b = self.eval(hi, fr)?.num();
                let st = match step {
                    Some(s) => self.eval(s, fr)?.num(),
                    None => 1.0,
                };
                if st == 0.0 || st != st {
                    return Err(self.fail(Fail::new(K::STEP, st, 0.0), None));
                }
                let mut cnt = ((b - a) / st + 1e-9).floor() + 1.0;
                if cnt < 0.0 {
                    cnt = 0.0;
                }
                if cnt != cnt {
                    return Err(self.fail(Fail::new(12, a, b), None));
                }
                let n = cnt.min(2f64.powi(62)) as i64;
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
                let a = self.eval(lo, fr)?.num();
                let b = self.eval(hi, fr)?.num();
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
    use super::meaningful_sf;

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
