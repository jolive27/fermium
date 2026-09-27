//! The evaluator, complex numbers, lists of them and the FFT built-ins: the kernels of fermium/cplx.py
//! (run_kernel with LLVM's ops: libm and IEEE division, as the compiled v1.5 path), fermium/clist.py's
//! ll_builtin and codegen_m3.py's fft / frequencies / argmax, over fermium-runtime's FFT (v1's spectral.py).
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
use std::cell::RefCell;
use std::rc::Rc;

use fermium_runtime::numerics::fft::spectrum;
use fermium_runtime::numerics::{err, Fail};

use crate::eval::{Interpreter, Printer, RunError, Value};
use crate::eval_vecmat::approx_core;

type C = (f64, f64);

fn k_mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// Smith's algorithm: no overflow for large |b|.
fn k_div(a: C, b: C) -> C {
    let big = b.1.abs() < b.0.abs();
    let r1 = b.1 / b.0;
    let d1 = b.0 + b.1 * r1;
    let x1 = (a.0 + a.1 * r1) / d1;
    let y1 = (a.1 - a.0 * r1) / d1;
    let r2 = b.0 / b.1;
    let d2 = b.0 * r2 + b.1;
    let x2 = (a.0 * r2 + a.1) / d2;
    let y2 = (a.1 * r2 - a.0) / d2;
    if big { (x1, y1) } else { (x2, y2) }
}

fn k_exp(a: C) -> C {
    let m = a.0.exp();
    (m * a.1.cos(), m * a.1.sin())
}

fn k_ln(a: C) -> C {
    (a.0.hypot(a.1).ln(), a.1.atan2(a.0))
}

/// √z without cancellation: t = √((|x| + |z|)/2), then the other part is y/(2t).
fn k_sqrt(a: C) -> C {
    let h = a.0.hypot(a.1);
    let t = ((a.0.abs() + h) * 0.5).sqrt();
    let zero = t == 0.0;
    let s = if zero { 1.0 } else { t + t };
    let q = a.1 / s;
    let neg = a.0 < 0.0;
    let x = if neg { q.abs() } else { t };
    let y = if neg { t.copysign(a.1) } else { q };
    (if zero { 0.0 } else { x }, if zero { a.1 } else { y })
}

fn k_sin(a: C) -> C {
    (a.0.sin() * a.1.cosh(), a.0.cos() * a.1.sinh())
}

fn k_cos(a: C) -> C {
    (a.0.cos() * a.1.cosh(), -(a.0.sin() * a.1.sinh()))
}

fn k_sinh(a: C) -> C {
    (a.0.sinh() * a.1.cos(), a.0.cosh() * a.1.sin())
}

fn k_cosh(a: C) -> C {
    (a.0.cosh() * a.1.cos(), a.0.sinh() * a.1.sin())
}

/// z^n for a whole number n (|n| <= 64): repeated squaring, then 1/z^|n| for n < 0.
fn k_powi(a: C, n: i64) -> C {
    let mut m = n.unsigned_abs();
    let mut result: Option<C> = None;
    let mut base = a;
    while m != 0 {
        if m & 1 == 1 {
            result = Some(match result {
                None => base,
                Some(r) => k_mul(r, base),
            });
        }
        m >>= 1;
        if m != 0 {
            base = k_mul(base, base);
        }
    }
    let Some(r) = result else { return (1.0, 0.0) };
    if n < 0 { k_div((1.0, 0.0), r) } else { r }
}

/// z^p for a real constant p: |z|^p (cos pθ, sin pθ), the principal value.
fn k_powr(a: C, p: f64) -> C {
    let m = a.0.hypot(a.1).powf(p);
    let th = a.1.atan2(a.0) * p;
    (m * th.cos(), m * th.sin())
}

/// z^w = exp(w ln z); 0^w is 0 (and 0^0 is 1).
fn k_pow(a: C, b: C) -> C {
    let w = k_exp(k_mul(b, k_ln(a)));
    let zero = a.0 == 0.0 && a.1 == 0.0;
    let wzero = b.0 == 0.0 && b.1 == 0.0;
    let x = if zero { if wzero { 1.0 } else { 0.0 } } else { w.0 };
    let y = if zero { 0.0 } else { w.1 };
    (x, y)
}

/// The complex version of ≈: moduli in place of absolute values (D21, D260).
fn k_approx(a: C, b: C, atol: f64, rtol: f64) -> bool {
    let same = a.0 == b.0 && a.1 == b.1;
    let diff = (a.0 - b.0).hypot(a.1 - b.1);
    approx_core(same, diff, a.0.hypot(a.1), b.0.hypot(b.1), atol, rtol)
}

fn cval(v: &Value) -> Option<C> {
    match v {
        Value::Vec(x) if x.len() == 2 => Some((x[0], x[1])),
        _ => None,
    }
}

fn cnum(z: C) -> Value {
    Value::Vec(Rc::new(vec![z.0, z.1]))
}

fn list(x: Vec<f64>) -> Value {
    Value::List(Rc::new(RefCell::new(x)))
}

fn clist(x: Vec<C>) -> Value {
    Value::CList(Rc::new(RefCell::new(x)))
}

fn pairs(flat: &[f64]) -> Vec<C> {
    flat.chunks(2).map(|p| (p[0], p[1])).collect()
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn builtin_complex(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        if let Some(k) = name.strip_prefix("c.") {
            return self.complex_kernel(k, args);
        }
        if let Some(k) = name.strip_prefix("cl.") {
            return Some(self.clist_builtin(k, args));
        }
        if name == "len" {
            if let Value::CList(l) = &args[0] {
                return Some(Ok(Value::Num(l.borrow().len() as f64)));
            }
            return None;
        }
        match name {
            "fft_re" | "fft_im" | "amplitude_spectrum" | "power_spectrum" | "ifft" | "frequencies" | "argmax"
            | "argmin" => Some(self.fourier(name, args)),
            _ => None,
        }
    }

    fn complex_kernel(&mut self, k: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        if args.iter().any(crate::eval_unc::is_unc) {
            return Some(self.unc_complex_kernel(k, args));
        }
        let z = cval(&args[0]);
        let zc = z.unwrap_or((f64::NAN, f64::NAN));
        let arg_c = |i: usize| cval(&args[i]).unwrap_or((f64::NAN, f64::NAN));
        Some(Ok(match k {
            "mul" => cnum(k_mul(zc, arg_c(1))),
            "div" => cnum(k_div(zc, arg_c(1))),
            "exp" => cnum(k_exp(zc)),
            "ln" => cnum(k_ln(zc)),
            "sqrt" => cnum(k_sqrt(zc)),
            "sin" => cnum(k_sin(zc)),
            "cos" => cnum(k_cos(zc)),
            "sinh" => cnum(k_sinh(zc)),
            "cosh" => cnum(k_cosh(zc)),
            "tan" => cnum(k_div(k_sin(zc), k_cos(zc))),
            "tanh" => cnum(k_div(k_sinh(zc), k_cosh(zc))),
            "abs" => Value::Num(match z {
                Some(z) => z.0.hypot(z.1),
                None => args[0].num().abs(),
            }),
            "arg" => Value::Num(zc.1.atan2(zc.0)),
            "conj" => cnum((zc.0, -zc.1)),
            "polar" => {
                let (r, th) = (args[0].num(), args[1].num());
                cnum((r * th.cos(), r * th.sin()))
            }
            "powi" => cnum(k_powi(zc, args[1].num() as i64)),
            "powr" => cnum(k_powr(zc, args[1].num())),
            "pow" => cnum(k_pow(zc, arg_c(1))),
            "eq" => {
                let b = arg_c(1);
                Value::Bool(zc.0 == b.0 && zc.1 == b.1)
            }
            "ne" => {
                let b = arg_c(1);
                Value::Bool(!(zc.0 == b.0 && zc.1 == b.1))
            }
            "approx" => Value::Bool(k_approx(zc, arg_c(1), args[2].num(), args[3].num())),
            _ => return None,
        }))
    }

    /// run_kernel with PyCOps on (re, im) pairs holding uncertain parts (v1 runs such a program in its
    /// interpreter): the arithmetic kernels propagate the uncertainties; the ones that call a math function on a
    /// part (exp, hypot, atan2, ...) need plain numbers.
    fn unc_complex_kernel(&mut self, k: &str, args: &[Value]) -> Result<Value, RunError> {
        use crate::eval_unc::{make_vec, num_op, vec_items, GENERIC};
        use crate::eval_unc_la::{abs, nom};
        use fermium_ir::BinOp::{Add, Div, Mul, Sub};
        type V = (Value, Value);
        let pair = |v: &Value| -> Option<V> {
            vec_items(v).filter(|x| x.len() == 2).map(|x| (x[0].clone(), x[1].clone()))
        };
        let op = |o, a: &Value, b: &Value| num_op(o, a, b);
        let mul = |a: &V, b: &V| -> V {
            (op(Sub, &op(Mul, &a.0, &b.0), &op(Mul, &a.1, &b.1)), op(Add, &op(Mul, &a.0, &b.1), &op(Mul, &a.1, &b.0)))
        };
        let div = |a: &V, b: &V| -> V {
            let big = nom(&abs(&b.1)) < nom(&abs(&b.0));
            let r1 = op(Div, &b.1, &b.0);
            let d1 = op(Add, &b.0, &op(Mul, &b.1, &r1));
            let x1 = op(Div, &op(Add, &a.0, &op(Mul, &a.1, &r1)), &d1);
            let y1 = op(Div, &op(Sub, &a.1, &op(Mul, &a.0, &r1)), &d1);
            let r2 = op(Div, &b.0, &b.1);
            let d2 = op(Add, &op(Mul, &b.0, &r2), &b.1);
            let x2 = op(Div, &op(Add, &op(Mul, &a.0, &r2), &a.1), &d2);
            let y2 = op(Div, &op(Sub, &op(Mul, &a.1, &r2), &a.0), &d2);
            if big { (x1, y1) } else { (x2, y2) }
        };
        let out = |z: V| make_vec(vec![z.0, z.1]);
        let generic = || self.err(GENERIC);
        let (Some(z), b) = (pair(&args[0]), args.get(1).and_then(pair)) else {
            return match k {
                // a real number: fabs is Python's abs, which UFloat has
                "abs" => Ok(abs(&args[0])),
                // polar(r, θ): only cos θ and sin θ need plain numbers
                "polar" if !matches!(args[1], Value::Unc(_)) => {
                    let th = args[1].num();
                    Ok(out((op(Mul, &args[0], &Value::Num(th.cos())), op(Mul, &args[0], &Value::Num(th.sin())))))
                }
                _ => generic(),
            };
        };
        match (k, b) {
            ("mul", Some(b)) => Ok(out(mul(&z, &b))),
            ("div", Some(b)) => Ok(out(div(&z, &b))),
            ("conj", _) => {
                let im = match &z.1 {
                    Value::Unc(u) => Value::Unc(Rc::new(u.neg())),
                    v => Value::Num(-v.num()),
                };
                Ok(out((z.0.clone(), im)))
            }
            ("eq", Some(b)) => Ok(Value::Bool(nom(&z.0) == nom(&b.0) && nom(&z.1) == nom(&b.1))),
            ("ne", Some(b)) => Ok(Value::Bool(!(nom(&z.0) == nom(&b.0) && nom(&z.1) == nom(&b.1)))),
            ("powi", _) => {
                let n = args[1].num() as i64;
                let mut m = n.unsigned_abs();
                let mut result: Option<V> = None;
                let mut base = z;
                while m != 0 {
                    if m & 1 == 1 {
                        result = Some(match result {
                            None => base.clone(),
                            Some(r) => mul(&r, &base),
                        });
                    }
                    m >>= 1;
                    if m != 0 {
                        base = mul(&base, &base);
                    }
                }
                let one = (Value::Num(1.0), Value::Num(0.0));
                Ok(out(match result {
                    None => one,
                    Some(r) if n < 0 => div(&one, &r),
                    Some(r) => r,
                }))
            }
            _ => generic(),
        }
    }

    fn clist_builtin(&mut self, k: &str, args: &[Value]) -> Result<Value, RunError> {
        match k {
            "fft" | "ifft" => {
                let (flat, cplx): (Vec<f64>, bool) = match &args[0] {
                    Value::CList(l) => (l.borrow().iter().flat_map(|z| [z.0, z.1]).collect(), true),
                    Value::List(l) => (l.borrow().clone(), false),
                    _ => (vec![], false),
                };
                if flat.is_empty() {
                    return Err(self.fail_kind(Fail::new(6, 0.0, 0.0)));
                }
                // fm_fft kinds: 5 fft(real), 6 ifft(complex), 7 fft(complex), 8 ifft(real)
                let kind = match (k, cplx) {
                    ("fft", false) => 5,
                    ("ifft", true) => 6,
                    ("fft", true) => 7,
                    _ => 8,
                };
                Ok(clist(pairs(&spectrum(kind, &flat, None, 1.0))))
            }
            "make" => {
                let (Value::List(re), Value::List(im)) = (&args[0], &args[1]) else { return Ok(Value::Void) };
                let (re, im) = (re.borrow(), im.borrow());
                if re.len() != im.len() {
                    return Err(self.fail_kind(Fail::new(5, re.len() as f64, im.len() as f64)));
                }
                Ok(clist(re.iter().zip(im.iter()).map(|(x, y)| (*x, *y)).collect()))
            }
            "get" => {
                let Value::CList(l) = &args[0] else { return Ok(Value::Void) };
                let l = l.borrow();
                let n = l.len();
                let idx = args[1].num();
                let inside = idx >= 1.0 && idx <= n as f64;
                let i = if inside { idx as i64 } else { 1 };
                if !inside || i as f64 != idx {
                    return Err(self.fail_kind(Fail::new(err::INDEX, idx, n as f64)));
                }
                Ok(cnum(l[i as usize - 1]))
            }
            _ => {
                let Value::CList(l) = &args[0] else { return Ok(Value::Void) };
                let l = l.borrow();
                Ok(match k {
                    "re" => list(l.iter().map(|z| z.0).collect()),
                    "im" => list(l.iter().map(|z| z.1).collect()),
                    "abs" => list(l.iter().map(|z| z.0.hypot(z.1)).collect()),
                    "arg" => list(l.iter().map(|z| z.1.atan2(z.0)).collect()),
                    "conj" => clist(l.iter().map(|z| (z.0, -0.0 - z.1)).collect()),
                    _ => return self.err(format!("cl.{k} isn't supported by the Rust back end yet")),
                })
            }
        }
    }

    /// A list length computed from a number (FuncGen.list_count): NaN or more than 10⁹ is an error, negative 0.
    fn list_count_c(&self, nf: f64) -> Result<usize, RunError> {
        if nf.is_nan() || nf > 1e9 {
            return Err(self.fail_kind(Fail::new(err::SIZE, nf, 0.0)));
        }
        Ok(if nf < 0.0 { 0 } else { nf as usize })
    }

    /// fft_re, fft_im, amplitude_spectrum, power_spectrum, ifft(re, im), frequencies, argmax, argmin
    /// (codegen_m3.py).
    fn fourier(&mut self, name: &str, args: &[Value]) -> Result<Value, RunError> {
        let lst = |v: &Value| match v {
            Value::List(l) => l.borrow().clone(),
            _ => vec![],
        };
        match name {
            "frequencies" => {
                let n = self.list_count_c(args[0].num())?;
                let dt = args[1].num();
                if n < 1 {
                    return Err(self.fail_kind(Fail::new(6, 0.0, 0.0)));
                }
                let m = n / 2 + 1;
                let span = n as f64 * dt;
                Ok(list((0..m).map(|i| i as f64 / span).collect()))
            }
            "argmax" | "argmin" => {
                let xs = lst(&args[0]);
                if xs.is_empty() {
                    return Err(self.fail_kind(Fail::new(6, 0.0, 0.0)));
                }
                let mut best = 0usize;
                for i in 1..xs.len() {
                    let (x, y) = (xs[i], xs[best]);
                    let better = if name == "argmax" { x > y } else { x < y } || (y.is_nan() && !x.is_nan());
                    if better {
                        best = i;
                    }
                }
                Ok(Value::Num((best + 1) as f64))
            }
            _ => {
                let a = lst(&args[0]);
                if a.is_empty() {
                    return Err(self.fail_kind(Fail::new(6, 0.0, 0.0)));
                }
                let kind = match name {
                    "fft_re" => 0,
                    "fft_im" => 1,
                    "amplitude_spectrum" => 2,
                    "power_spectrum" => 3,
                    _ => 4,
                };
                let mut other = None;
                let mut dt = 1.0;
                if name == "ifft" {
                    let b = lst(&args[1]);
                    if b.len() != a.len() {
                        return Err(self.fail_kind(Fail::new(5, a.len() as f64, b.len() as f64)));
                    }
                    other = Some(b);
                }
                if name == "power_spectrum" {
                    dt = args[1].num();
                }
                Ok(list(spectrum(kind, &a, other.as_deref(), dt)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernels_like_cplx_py() {
        // (3 + 4i) / (1 - 2i) = -1 + 2i
        let q = k_div((3.0, 4.0), (1.0, -2.0));
        assert!((q.0 + 1.0).abs() < 1e-15 && (q.1 - 2.0).abs() < 1e-15);
        assert_eq!(k_sqrt((-4.0, 0.0)), (0.0, 2.0));
        assert_eq!(k_powi((0.0, 1.0), 2), (-1.0, 0.0));
        assert_eq!(k_pow((0.0, 0.0), (0.0, 0.0)), (1.0, 0.0));
    }
}
