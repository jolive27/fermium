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
    /// run-time warnings already shown (each distinct text once, like Runtime.warn_text)
    static WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
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

/// glibc's cbrt (sysdeps/ieee754/dbl-64/s_cbrt.c), which v1's compiled code calls: Rust's `f64::cbrt` is
/// correctly rounded and so differs from it in the last bit for some arguments (visible with `to 17 digits`).
pub fn cbrt(x: f64) -> f64 {
    fn frexp(x: f64) -> (f64, i32) {
        if x == 0.0 || !x.is_finite() {
            return (x, 0);
        }
        let bits = x.to_bits();
        let e = ((bits >> 52) & 0x7ff) as i32;
        if e == 0 {
            let (m, e2) = frexp(x * 2f64.powi(54));
            return (m, e2 - 54);
        }
        (f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52)), e - 1022)
    }
    fn ldexp(mut x: f64, mut e: i32) -> f64 {
        while e > 1000 {
            x *= 2f64.powi(1000);
            e -= 1000;
        }
        while e < -1000 {
            x *= 2f64.powi(-1000);
            e += 1000;
        }
        x * 2f64.powi(e)
    }
    const CBRT2: f64 = 1.2599210498948731648;
    const SQR_CBRT2: f64 = 1.5874010519681994748;
    let factor = [1.0 / SQR_CBRT2, 1.0 / CBRT2, 1.0, CBRT2, SQR_CBRT2];
    let (xm, xe) = frexp(x.abs());
    if xe == 0 && (x == 0.0 || !x.is_finite()) {
        return x + x;
    }
    let u = 0.354895765043919860
        + ((1.50819193781584896
            + ((-2.11499494167371287
                + ((2.44693122563534430
                    + ((-1.83469277483613086 + (0.784932344976639262 - 0.145263899385486377 * xm) * xm) * xm))
                    * xm))
                * xm))
            * xm);
    let t2 = u * u * u;
    let ym = u * (t2 + 2.0 * xm) / (2.0 * t2 + xm) * factor[(2 + xe % 3) as usize];
    ldexp(if x > 0.0 { ym } else { -ym }, xe / 3)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cbrt_is_glibcs() {
        // values from glibc 2.36's cbrt (python3 ctypes)
        for (x, want) in [(6.348380305380394, 1.8516304404461712), (-27.0, -3.0000000000000004), (0.001, 0.1),
                          (1e-310, 4.641588833612775e-104), (-5.5, -1.7651741676630317), (0.7, 0.8879040017426008)] {
            assert_eq!(super::cbrt(x), want, "{x}");
        }
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
