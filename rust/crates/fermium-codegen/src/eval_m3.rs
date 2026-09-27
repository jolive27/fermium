//! The evaluator, random numbers (D80): rand, rand(a, b), randn, randn(μ, σ), seed and sample, on v1's stream
//! (fermium-runtime's rng, the same numbers as v1's JIT, `fermium build` and interpreter). A port of the compiled
//! path (fermium/codegen_m3.py `builtin`), with the messages of runtime/core.py describe_error.
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
use std::cell::RefCell;
use std::rc::Rc;

use fermium_ir::{Expr, ExprKind};
use fermium_runtime::numerics::rng::Rng;
use fermium_units::numfmt::format_number6;

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

thread_local! {
    /// The generator: a program that never calls seed starts as if it had called seed(0).
    static RNG: RefCell<Rng> = RefCell::new(Rng::default());
}

/// A standard normal from the program's stream (propagate montecarlo draws its samples here, as v1 does).
pub(crate) fn randn_draw() -> f64 {
    RNG.with(|r| r.borrow_mut().randn())
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn builtin_m3(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        let num = |v: &Value| match v {
            Value::Num(x) => Ok(*x),
            _ => Err(RunError { message: "not yet supported by the Rust back end: a number was expected here".into(),
                                line: self.line, hint: None }),
        };
        let r = match name {
            "rand" => Ok(Value::Num(RNG.with(|r| r.borrow_mut().rand()))),
            "rand2" => (|| {
                let (lo, hi) = (num(&args[0])?, num(&args[1])?);
                Ok(Value::Num(lo + (hi - lo) * RNG.with(|r| r.borrow_mut().rand())))
            })(),
            "randn" => Ok(Value::Num(RNG.with(|r| r.borrow_mut().randn()))),
            "randn2" => (|| {
                let (mu, sig) = (num(&args[0])?, num(&args[1])?);
                if sig < 0.0 {
                    return self.err("randn(μ, σ): σ is a standard deviation, so it can't be negative");
                }
                Ok(Value::Num(mu + sig * RNG.with(|r| r.borrow_mut().randn())))
            })(),
            "seed" => num(&args[0]).map(|s| {
                RNG.with(|r| r.borrow_mut().seed(s));
                Value::Num(0.0)
            }),
            _ => return None,
        };
        Some(r)
    }

    pub(crate) fn eval_m3(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        let ExprKind::Sample { lam, n } = &e.kind else {
            return self.err("this isn't supported by the Rust back end yet");
        };
        let nf = self.eval(n, fr)?.num();
        if nf < 0.0 || nf != nf.floor() {
            return self.err(format!("the number of samples must be a whole number, 0 or more, not {}",
                                    format_number6(nf)));
        }
        if nf.is_nan() {
            return self.err("the length of a list must be a number, not NaN");
        }
        if nf > crate::eval_core::MAX_LIST {
            return self.err(format!("not enough memory for a list of {} numbers (the most is 10⁹)", format_number6(nf)));
        }
        let cnt = nf as usize;
        let line = self.line;
        let mut out = Vec::with_capacity(cnt);
        for i in 0..cnt {
            out.push(self.call_scalar_lambda(*lam, (i + 1) as f64, fr)?);
            self.line = line;
        }
        Ok(Value::List(Rc::new(RefCell::new(out))))
    }
}
