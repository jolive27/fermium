//! The evaluator, core: the built-in functions on numbers and lists, element-by-element calls (Map, D191),
//! text (str, +), ≈ (D260), list indexing and slicing. The reference is the compiled path of Fermium 1.5
//! (fermium/codegen_llvm.py e_IBuiltin/reduce/e_IMap and the run-time messages of fermium/runtime/core.py
//! describe_error), which the conformance goldens come from.
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from builtin_core means
//! "not mine".
use std::cell::RefCell;
use std::rc::Rc;

use fermium_ir::{Expr, ExprKind};
use fermium_runtime::numerics::special;
use fermium_units::numfmt::format_number6;

use crate::eval::{fdiv, math1, Frame, Interpreter, Printer, RunError, Value};

/// The most numbers a list may hold (MAX_LIST).
pub const MAX_LIST: f64 = 1e9;
pub(crate) const STD_ONE_TEXT: &str = "std needs at least 2 values: it is the sample standard deviation, which divides \
                                       by N − 1, so one value says nothing about the spread (quote the instrument's \
                                       uncertainty for a single measurement)";
const SLICE_MSG: &str = "a slice xs[a:b] runs from a up to b, so b can't be smaller than a − 1 (xs[a:a-1] is the \
                         empty list); to reverse a list use reverse(xs)";

/// The one-argument math functions of the compiled path (MATH_INTRINSICS, MATH_LIBM, RECIPROCAL_TRIG, sign).
const MATH_NAMES: &[&str] = &[
    "sin", "cos", "exp", "ln", "log", "log10", "log2", "abs", "floor", "ceil", "round", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "erf", "erfc", "gamma", "lgamma", "expm1", "log1p",
    "cot", "sec", "csc", "sign",
];

/// describe_error kind 1: a list index (n ≥ 0) or a vector/matrix index (n < 0, -n entries) out of range.
pub fn index_message(a: f64, n: i64) -> String {
    if a.is_nan() {
        return "a list index must be a whole number (1, 2, 3, ...), not NaN".into();
    }
    let whole = a.is_infinite() || a == a.trunc();
    if n < 0 {
        if !whole {
            return format!("an index must be a whole number (1, 2, 3, ...), not {}", format_number6(a));
        }
        return format!("index {} is out of range: valid indexes here are 1 to {}{}", format_number6(a), -n,
                       if a == 0.0 { "; Fermium counts from 1" } else { "" });
    }
    if !whole {
        return format!("a list index must be a whole number (1, 2, 3, ...), not {}", format_number6(a));
    }
    if n == 0 {
        return format!("index {} is out of range: the list is empty", format_number6(a));
    }
    format!("index {} is out of range: the list has {n} element{} (valid indexes are 1 to {n}){}", format_number6(a),
            if n != 1 { "s" } else { "" },
            if a == 0.0 { "; Fermium counts from 1, so the first element is [1]" } else { "" })
}

fn new_list(v: Vec<f64>) -> Value {
    Value::List(Rc::new(RefCell::new(v)))
}

fn one_math(name: &str, x: f64) -> f64 {
    match name {
        "asinh" => cmath::asinh(x),
        "acosh" => cmath::acosh(x),
        "atanh" => cmath::atanh(x),
        "erf" => cmath::erf(x),
        "erfc" => cmath::erfc(x),
        "gamma" => cmath::tgamma(x),
        "lgamma" => cmath::lgamma(x),
        _ => math1(name, x),
    }
}

/// The C library's math functions that v1's compiled code calls (MATH_LIBM, cbrt, jn, yn), so results agree to
/// the last bit: Rust's own asinh/acosh/atanh and cbrt (compiler-builtins), and the pure-Rust ports in
/// fermium-runtime, can differ by an ulp, which shows when a value is printed to 16 digits. Elsewhere:
/// fermium-runtime's ports.
#[cfg(unix)]
pub mod cmath {
    #[link(name = "m")]
    extern "C" {
        #[link_name = "cbrt"]
        fn c_cbrt(x: f64) -> f64;
        #[link_name = "asinh"]
        fn c_asinh(x: f64) -> f64;
        #[link_name = "acosh"]
        fn c_acosh(x: f64) -> f64;
        #[link_name = "atanh"]
        fn c_atanh(x: f64) -> f64;
        #[link_name = "erf"]
        fn c_erf(x: f64) -> f64;
        #[link_name = "erfc"]
        fn c_erfc(x: f64) -> f64;
        #[link_name = "tgamma"]
        fn c_tgamma(x: f64) -> f64;
        #[link_name = "lgamma"]
        fn c_lgamma(x: f64) -> f64;
        #[link_name = "jn"]
        fn c_jn(n: i32, x: f64) -> f64;
        #[link_name = "yn"]
        fn c_yn(n: i32, x: f64) -> f64;
    }
    // SAFETY (all of them): pure C math functions of plain numbers
    /// The C library's cbrt. Rust's std links compiler-builtins' cbrt ahead of libm's, so glibc's algorithm
    /// (sysdeps/ieee754/dbl-64/s_cbrt.c) is ported instead: bit for bit the same on 200 000 random doubles.
    pub fn cbrt(x: f64) -> f64 {
        let _ = c_cbrt;
        super::glibc_cbrt(x)
    }
    pub fn asinh(x: f64) -> f64 {
        unsafe { c_asinh(x) }
    }
    pub fn acosh(x: f64) -> f64 {
        unsafe { c_acosh(x) }
    }
    pub fn atanh(x: f64) -> f64 {
        unsafe { c_atanh(x) }
    }
    pub fn erf(x: f64) -> f64 {
        unsafe { c_erf(x) }
    }
    pub fn erfc(x: f64) -> f64 {
        unsafe { c_erfc(x) }
    }
    pub fn tgamma(x: f64) -> f64 {
        unsafe { c_tgamma(x) }
    }
    pub fn lgamma(x: f64) -> f64 {
        unsafe { c_lgamma(x) }
    }
    /// besselj: NaN for an order that isn't a whole number below 2³¹ (special.ll_jn_yn)
    pub fn jn(n: f64, x: f64) -> f64 {
        if !fermium_runtime::numerics::special::order_ok(n) {
            return f64::NAN;
        }
        unsafe { c_jn(n as i32, x) }
    }
    pub fn yn(n: f64, x: f64) -> f64 {
        if !fermium_runtime::numerics::special::order_ok(n) {
            return f64::NAN;
        }
        unsafe { c_yn(n as i32, x) }
    }
}

/// glibc's cbrt (sysdeps/ieee754/dbl-64/s_cbrt.c): a polynomial first guess, one Halley step, the exponent
/// split by 3.
pub fn glibc_cbrt(x: f64) -> f64 {
    const CBRT2: f64 = 1.2599210498948731648;
    const SQR_CBRT2: f64 = 1.5874010519681994748;
    const FACTOR: [f64; 5] = [1.0 / SQR_CBRT2, 1.0 / CBRT2, 1.0, CBRT2, SQR_CBRT2];
    if x == 0.0 || !x.is_finite() {
        return x + x;
    }
    let (xm, xe) = frexp(x.abs());
    let u = 0.354895765043919860
        + ((1.50819193781584896
            + ((-2.11499494167371287
                + ((2.44693122563534430
                    + ((-1.83469277483613086 + (0.784932344976639262 - 0.145263899385486377 * xm) * xm) * xm))
                    * xm))
                * xm))
            * xm);
    let t2 = u * u * u;
    let ym = u * (t2 + 2.0 * xm) / (2.0 * t2 + xm) * FACTOR[(2 + xe % 3) as usize];
    ldexp(if x > 0.0 { ym } else { -ym }, xe / 3)
}

/// C's frexp for a finite, non-zero x: (m, e) with x = m·2^e and 0.5 ≤ |m| < 1.
fn frexp(x: f64) -> (f64, i32) {
    let (x, extra) = if x.abs() < f64::MIN_POSITIVE { (x * 2f64.powi(54), -54) } else { (x, 0) };
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i32;
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, e - 1022 + extra)
}

/// C's ldexp: x·2^e rounded once.
fn ldexp(x: f64, e: i32) -> f64 {
    if e < -1000 {
        // an exact first step keeps the only rounding in the last multiplication
        return x * 2f64.powi(-1000) * 2f64.powi(e + 1000);
    }
    if e > 1000 {
        return x * 2f64.powi(1000) * 2f64.powi(e - 1000);
    }
    x * 2f64.powi(e)
}

#[cfg(not(unix))]
pub mod cmath {
    use fermium_runtime::numerics::special;
    pub fn cbrt(x: f64) -> f64 {
        super::glibc_cbrt(x)
    }
    pub fn asinh(x: f64) -> f64 {
        x.asinh()
    }
    pub fn acosh(x: f64) -> f64 {
        x.acosh()
    }
    pub fn atanh(x: f64) -> f64 {
        x.atanh()
    }
    pub fn erf(x: f64) -> f64 {
        special::erf(x)
    }
    pub fn erfc(x: f64) -> f64 {
        special::erfc(x)
    }
    pub fn tgamma(x: f64) -> f64 {
        special::gamma(x)
    }
    pub fn lgamma(x: f64) -> f64 {
        special::lgamma(x)
    }
    pub fn jn(n: f64, x: f64) -> f64 {
        special::besselj(n, x)
    }
    pub fn yn(n: f64, x: f64) -> f64 {
        special::bessely(n, x)
    }
}

/// The seconds since the program started (time.perf_counter in v1: any fixed origin).
fn clock() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

impl<'m, P: Printer> Interpreter<'m, P> {
    fn list_arg(&self, v: &Value) -> Result<Rc<RefCell<Vec<f64>>>, RunError> {
        match v {
            Value::List(l) => Ok(l.clone()),
            _ => self.err("not yet supported by the Rust back end: a list was expected here"),
        }
    }

    fn num_arg(&self, v: &Value) -> Result<f64, RunError> {
        match v {
            Value::Num(x) => Ok(*x),
            Value::Bool(b) => Ok(f64::from(u8::from(*b))),
            _ => self.err("not yet supported by the Rust back end: a number was expected here"),
        }
    }

    /// A list length computed from a number (list_count): NaN or more than 10⁹ is an error, negative is 0.
    fn list_count(&self, nf: f64) -> Result<usize, RunError> {
        if nf.is_nan() {
            return self.err("the length of a list must be a number, not NaN");
        }
        if nf > MAX_LIST {
            return self.err(format!("not enough memory for a list of {} numbers (the most is 10⁹)", format_number6(nf)));
        }
        Ok(if nf < 0.0 { 0 } else { nf as usize })
    }

    fn len_error<T>(&self, a: usize, b: usize) -> Result<T, RunError> {
        self.err(format!("these two lists have different lengths ({a} and {b})"))
    }

    /// The 0-based position of the 1-based index idx in a list of n (elem_ptr), or the run-time error.
    pub(crate) fn elem_index(&self, idx: f64, n: usize) -> Result<usize, RunError> {
        if idx >= 1.0 && idx <= n as f64 && idx == idx.trunc() {
            return Ok(idx as usize - 1);
        }
        self.err(index_message(idx, n as i64))
    }

    pub(crate) fn builtin_core(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        let r = match name {
            _ if MATH_NAMES.contains(&name) => self.bi_math(name, args),
            "besselj" | "bessely" | "besseli" | "besselk" => (|| {
                let (n, x) = (self.num_arg(&args[0])?, self.num_arg(&args[1])?);
                Ok(Value::Num(match name {
                    "besselj" => cmath::jn(n, x),
                    "bessely" => cmath::yn(n, x),
                    "besseli" => special::besseli(n, x),
                    _ => special::besselk(n, x),
                }))
            })(),
            "ellipk" | "ellipe" => self.num_arg(&args[0]).map(|m| {
                let (k, e) = special::ellip(m);
                Value::Num(if name == "ellipk" { k } else { e })
            }),
            "isnan" => self.num_arg(&args[0]).map(|x| Value::Bool(x.is_nan())),
            "atan2" | "hypot" | "mod" => (|| {
                let (a, c) = (self.num_arg(&args[0])?, self.num_arg(&args[1])?);
                Ok(Value::Num(match name {
                    "atan2" => a.atan2(c),
                    "hypot" => a.hypot(c),
                    _ => a - c * (a / c).floor(),
                }))
            })(),
            "min" | "max" => (|| {
                // llvm.minnum / maxnum: a NaN loses to a number
                let mut r = self.num_arg(&args[0])?;
                for a in &args[1..] {
                    let x = self.num_arg(a)?;
                    r = if name == "min" { r.min(x) } else { r.max(x) };
                }
                Ok(Value::Num(r))
            })(),
            "min_ew" | "max_ew" => self.bi_minmax_ew(name, args),
            "clamp" => (|| {
                let (x, lo, hi) = (self.num_arg(&args[0])?, self.num_arg(&args[1])?, self.num_arg(&args[2])?);
                Ok(Value::Num(x.max(lo).min(hi)))
            })(),
            "factorial" => self.num_arg(&args[0]).map(|x| Value::Num(cmath::tgamma(x + 1.0))),
            "clock" => Ok(Value::Num(clock())),
            "len" => match &args[0] {
                Value::List(l) => Ok(Value::Num(l.borrow().len() as f64)),
                Value::TextList(l) => Ok(Value::Num(l.borrow().len() as f64)),
                Value::CList(l) => Ok(Value::Num(l.borrow().len() as f64)),
                _ => return None,
            },
            "sum" | "mean" | "std" | "min_list" | "max_list" | "first" | "last" => self.bi_reduce(name, &args[0]),
            "dot" if matches!(args[0], Value::List(_)) => self.bi_dot(args),
            "trapz" => self.bi_trapz(args),
            "interp" => self.bi_interp(args),
            "zeros" | "ones" => (|| {
                let n = self.list_count(self.num_arg(&args[0])?)?;
                Ok(new_list(vec![if name == "zeros" { 0.0 } else { 1.0 }; n]))
            })(),
            "linspace" => (|| {
                let (a, c) = (self.num_arg(&args[0])?, self.num_arg(&args[1])?);
                let n = self.list_count(self.num_arg(&args[2])?)?;
                let den = n as f64 - 1.0;
                let step = fdiv(c - a, den);
                Ok(new_list((0..n).map(|i| a + i as f64 * step).collect()))
            })(),
            "range" => (|| {
                let (a, c, st) = (self.num_arg(&args[0])?, self.num_arg(&args[1])?, self.num_arg(&args[2])?);
                if st == 0.0 || st.is_nan() {
                    return self.err("the step must be a non-zero number that goes from the start towards the end");
                }
                let mut cnt = (fdiv(c - a, st) + 1e-9).floor() + 1.0;
                if cnt < 0.0 {
                    cnt = 0.0;
                }
                let n = self.list_count(cnt)?;
                Ok(new_list((0..n).map(|i| a + i as f64 * st).collect()))
            })(),
            "copy" => self.list_arg(&args[0]).map(|l| new_list(l.borrow().clone())),
            "slice" => self.bi_slice(args),
            "reverse" => self.list_arg(&args[0]).map(|l| new_list(l.borrow().iter().rev().copied().collect())),
            "sort" => self.list_arg(&args[0]).map(|l| {
                // NaN last (fm_sort), a stable sort
                let mut v = l.borrow().clone();
                v.sort_by(|a, b| {
                    let ka = (a.is_nan(), if a.is_nan() { 0.0 } else { *a });
                    let kb = (b.is_nan(), if b.is_nan() { 0.0 } else { *b });
                    ka.partial_cmp(&kb).unwrap()
                });
                new_list(v)
            }),
            "cumsum" => self.list_arg(&args[0]).map(|l| {
                let mut acc = 0.0;
                new_list(l.borrow().iter().map(|x| {
                    acc += x;
                    acc
                }).collect())
            }),
            "diff" => self.list_arg(&args[0]).map(|l| {
                let v = l.borrow();
                new_list(v.windows(2).map(|w| w[1] - w[0]).collect())
            }),
            "text_concat" => match (&args[0], &args[1]) {
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}").into())),
                _ => self.err("not yet supported by the Rust back end: joining these values as text"),
            },
            "text_num" => (|| {
                let fid = self.num_arg(&args[0])? as usize;
                let v = self.num_arg(&args[1])?;
                let f = crate::printer::print_fmt(&self.module.tables.fmts[fid]);
                Ok(Value::Str(fermium_units::quantity::format_value(v, &f).into()))
            })(),
            "approx" => self.bi_approx(args),
            _ => return None,
        };
        Some(r)
    }

    fn bi_math(&self, name: &str, args: &[Value]) -> Result<Value, RunError> {
        match &args[0] {
            Value::List(l) => Ok(new_list(l.borrow().iter().map(|x| one_math(name, *x)).collect())),
            v => Ok(Value::Num(one_math(name, self.num_arg(v)?))),
        }
    }

    fn bi_minmax_ew(&self, name: &str, args: &[Value]) -> Result<Value, RunError> {
        let lists: Vec<usize> = (0..args.len()).filter(|&i| matches!(args[i], Value::List(_))).collect();
        let first = self.list_arg(&args[lists[0]])?;
        let n0 = first.borrow().len();
        for &i in &lists[1..] {
            let ni = self.list_arg(&args[i])?.borrow().len();
            if ni != n0 {
                return self.len_error(n0, ni);
            }
        }
        let mut cols: Vec<Vec<f64>> = vec![];
        for a in args {
            cols.push(match a {
                Value::List(l) => l.borrow().clone(),
                v => vec![self.num_arg(v)?],
            });
        }
        let is_list: Vec<bool> = args.iter().map(|a| matches!(a, Value::List(_))).collect();
        let out = (0..n0)
            .map(|k| {
                let at = |i: usize| if is_list[i] { cols[i][k] } else { cols[i][0] };
                let mut r = at(0);
                for i in 1..cols.len() {
                    r = if name == "min_ew" { r.min(at(i)) } else { r.max(at(i)) };
                }
                r
            })
            .collect::<Vec<f64>>();
        Ok(new_list(out))
    }

    fn bi_reduce(&self, name: &str, lst: &Value) -> Result<Value, RunError> {
        let l = self.list_arg(lst)?;
        let data = l.borrow();
        let n = data.len();
        if n < 1 && name != "sum" {
            return self.err("this list is empty");
        }
        if name == "std" && n < 2 {
            return self.err(STD_ONE_TEXT);
        }
        let r = match name {
            "first" => data[0],
            "last" => data[n - 1],
            "min_list" => data[1..].iter().fold(data[0], |a, &b| a.min(b)),
            "max_list" => data[1..].iter().fold(data[0], |a, &b| a.max(b)),
            _ => {
                let mut s = 0.0;
                for x in data.iter() {
                    s += x;
                }
                if name == "sum" {
                    s
                } else {
                    let mean = s / n as f64;
                    if name == "mean" {
                        mean
                    } else {
                        let mut acc = 0.0;
                        for x in data.iter() {
                            let d = x - mean;
                            acc += d * d;
                        }
                        (acc / (n as f64 - 1.0)).sqrt()
                    }
                }
            }
        };
        Ok(Value::Num(r))
    }

    fn bi_dot(&self, args: &[Value]) -> Result<Value, RunError> {
        let (a, c) = (self.list_arg(&args[0])?, self.list_arg(&args[1])?);
        let (a, c) = (a.borrow(), c.borrow());
        if a.len() != c.len() {
            return self.len_error(a.len(), c.len());
        }
        let mut acc = 0.0;
        for (x, y) in a.iter().zip(c.iter()) {
            acc += x * y;
        }
        Ok(Value::Num(acc))
    }

    fn bi_trapz(&self, args: &[Value]) -> Result<Value, RunError> {
        let (ys, xs) = (self.list_arg(&args[0])?, self.list_arg(&args[1])?);
        let (ys, xs) = (ys.borrow(), xs.borrow());
        let n = ys.len();
        if n != xs.len() {
            return self.len_error(n, xs.len());
        }
        let mut acc = 0.0;
        for i in 1..n {
            let dx = xs[i] - xs[i - 1];
            let s = ys[i] + ys[i - 1];
            acc += 0.5 * (dx * s);
        }
        Ok(Value::Num(acc))
    }

    fn bi_interp(&self, args: &[Value]) -> Result<Value, RunError> {
        let x = self.num_arg(&args[0])?;
        let (xs, ys) = (self.list_arg(&args[1])?, self.list_arg(&args[2])?);
        let (xs, ys) = (xs.borrow(), ys.borrow());
        let n = xs.len();
        if n != ys.len() {
            return self.len_error(n, ys.len());
        }
        if n < 2 {
            return self.err("this list is empty");
        }
        // clamp to the ends
        let mut res = ys[0];
        if x >= xs[n - 1] {
            res = ys[n - 1];
        }
        for i in 1..n {
            let (xa, xb) = (xs[i - 1], xs[i]);
            if x >= xa && x < xb {
                let s = fdiv(x - xa, xb - xa);
                res = ys[i - 1] + s * (ys[i] - ys[i - 1]);
            }
        }
        Ok(Value::Num(res))
    }

    /// xs[a:b], both ends included; xs[a:a-1] is empty (D114).
    fn bi_slice(&self, args: &[Value]) -> Result<Value, RunError> {
        let l = self.list_arg(&args[0])?;
        let (lo, hi) = (self.num_arg(&args[1])?, self.num_arg(&args[2])?);
        if hi == lo - 1.0 {
            return Ok(new_list(vec![]));
        }
        if hi < lo - 1.0 {
            return self.err(SLICE_MSG);
        }
        let v = l.borrow();
        let a = self.elem_index(lo, v.len())?;
        self.elem_index(hi, v.len())?;
        let cnt = (hi as i64 - lo as i64 + 1).max(0) as usize;
        Ok(new_list(v[a..(a + cnt).min(v.len())].to_vec()))
    }

    /// a ≈ b (Julia's isapprox, D260): numbers, or vectors by their norms.
    fn bi_approx(&self, args: &[Value]) -> Result<Value, RunError> {
        let (atol, rtol) = (self.num_arg(&args[2])?, self.num_arg(&args[3])?);
        let (same, diff, sa, sb) = match (&args[0], &args[1]) {
            (Value::Vec(a), Value::Vec(b)) => {
                let norm = |xs: &mut dyn Iterator<Item = f64>| {
                    let mut s: Option<f64> = None;
                    for x in xs {
                        s = Some(match s {
                            None => x * x,
                            Some(s) => s + x * x,
                        });
                    }
                    s.unwrap_or(0.0).sqrt()
                };
                let same = a.iter().zip(b.iter()).all(|(x, y)| x == y);
                (same, norm(&mut a.iter().zip(b.iter()).map(|(x, y)| x - y)), norm(&mut a.iter().copied()),
                 norm(&mut b.iter().copied()))
            }
            (a, b) => {
                let (x, y) = (self.num_arg(a)?, self.num_arg(b)?);
                (x == y, (x - y).abs(), x.abs(), y.abs())
            }
        };
        let tol = atol.max(rtol * sa.max(sb));
        let close = diff <= tol && diff != f64::INFINITY;
        Ok(Value::Bool(same || close))
    }

    pub(crate) fn eval_core(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::Map { func, args, list_pos } => {
                let vals = args.iter().map(|a| self.eval(a, fr)).collect::<Result<Vec<_>, _>>()?;
                // the elements as values: a list may hold uncertain numbers (D120)
                let lists: Vec<Vec<Value>> = list_pos
                    .iter()
                    .map(|&p| crate::eval_unc::list_items(&vals[p]).ok_or(()))
                    .collect::<Result<_, _>>()
                    .or_else(|_| self.err("not yet supported by the Rust back end: a list was expected here"))?;
                let n0 = lists[0].len();
                for l in &lists[1..] {
                    if l.len() != n0 {
                        return self.len_error(n0, l.len());
                    }
                }
                let mut out = Vec::with_capacity(n0);
                for i in 0..n0 {
                    let mut a2 = vals.clone();
                    for (k, &p) in list_pos.iter().enumerate() {
                        a2[p] = lists[k][i].clone();
                    }
                    let line = self.line;
                    let v = self.call(*func, a2)?;
                    self.line = line;
                    match v {
                        v @ (Value::Num(_) | Value::Unc(_)) => out.push(v),
                        v => out.push(Value::Num(self.num_arg(&v)?)),
                    }
                }
                Ok(crate::eval_unc::make_list(out))
            }
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }
}
