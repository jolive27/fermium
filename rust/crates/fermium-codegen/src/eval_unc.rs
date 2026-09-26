//! The evaluator, uncertainties (D120–D124): ± values with linear propagation and exact correlations
//! (fermium-runtime's UFloat, a port of fermium/uncertain.py) and `propagate montecarlo`.
//!
//! v1 runs a program that uses uncertainties in its interpreter (fermium/interp.py), so interp.py is the reference
//! here: Python's operators propagate UFloat values through the same code as plain numbers, comparisons use the
//! nominal values, and an operation that needs a plain number stops with UncertainUse's message.
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
use std::cell::RefCell;
use std::rc::Rc;

use fermium_ir::{BinOp, CmpOp, Expr, Stmt, StmtKind};
use fermium_runtime::numerics::uncertain::{self as U, UFloat};
use fermium_units::numfmt::format_number6;

use crate::eval::{fdiv, Frame, Interpreter, Printer, RunError, Value};

/// UncertainUse's message for an operation that needs a plain number.
pub const GENERIC: &str = "this operation needs a plain number, but got an uncertain value (±); write value(x) to drop \
                           the uncertainty, or put the calculation in a  propagate montecarlo  block";

const STEP: &[&str] = &["floor", "ceil", "round", "sign"];

/// Does a value hold an uncertain number (or a list with one)?
pub fn is_unc(v: &Value) -> bool {
    matches!(v, Value::Unc(_) | Value::UList(_))
}

fn unc(u: UFloat) -> Value {
    Value::Unc(Rc::new(u))
}

/// A list of numbers and uncertain numbers: plain if none is uncertain.
pub fn make_list(items: Vec<Value>) -> Value {
    if items.iter().any(|x| matches!(x, Value::Unc(_))) {
        Value::UList(Rc::new(RefCell::new(items)))
    } else {
        Value::List(Rc::new(RefCell::new(items.iter().map(Value::num).collect())))
    }
}

/// The elements of a list as values (numbers or uncertain numbers).
pub fn list_items(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::List(l) => Some(l.borrow().iter().map(|x| Value::Num(*x)).collect()),
        Value::UList(l) => Some(l.borrow().clone()),
        _ => None,
    }
}

fn nominal(v: &Value) -> f64 {
    match v {
        Value::Unc(u) => u.v,
        v => v.num(),
    }
}

fn sigma(v: &Value) -> f64 {
    match v {
        Value::Unc(u) => u.s(),
        _ => 0.0,
    }
}

/// interp.math1 on plain numbers (the functions Python's math module gives).
pub fn interp_math1(name: &str, x: f64) -> f64 {
    match name {
        "round" => if x.is_finite() { (x.abs() + 0.5).floor().copysign(x) } else { x },
        "gamma" => crate::eval_core::cmath::tgamma(x),
        "lgamma" => crate::eval_core::cmath::lgamma(x),
        "erf" => crate::eval_core::cmath::erf(x),
        "erfc" => crate::eval_core::cmath::erfc(x),
        "asinh" => crate::eval_core::cmath::asinh(x),
        "acosh" => crate::eval_core::cmath::acosh(x),
        "atanh" => crate::eval_core::cmath::atanh(x),
        _ => crate::eval::math1(name, x),
    }
}

/// interp.powc on plain numbers (the cube root as |x|^(1/3), Python's ** for the rest).
pub fn interp_powc(x: f64, p: f64) -> f64 {
    if [2.0, 3.0, 1.0, 0.5, -1.0, -2.0, 4.0, -0.5, 1.5, -1.5].contains(&p) {
        return crate::eval::powc(x, p); // the same special cases as the compiled path
    }
    if (p - 1.0 / 3.0).abs() < 1e-15 {
        return if x.is_finite() { x.abs().powf(1.0 / 3.0).copysign(x) } else { x };
    }
    if let Some(n) = crate::eval::odd_root_numerator(p) {
        if x < 0.0 {
            let r = U::pow(-x, p);
            return if n % 2 != 0 { -r } else { r };
        }
    }
    U::pow(x, p)
}

/// One element-wise arithmetic operation, where either side may be uncertain.
fn num_op(op: BinOp, a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Unc(x), Value::Unc(y)) => unc(match op {
            BinOp::Add => x.add(y),
            BinOp::Sub => x.sub(y),
            BinOp::Mul => x.mul(y),
            BinOp::Div => x.div(y),
        }),
        (Value::Unc(x), y) => {
            let y = y.num();
            unc(match op {
                BinOp::Add => x.add_f(y),
                BinOp::Sub => x.sub_f(y),
                BinOp::Mul => x.mul_f(y),
                BinOp::Div => x.div_f(y),
            })
        }
        (x, Value::Unc(y)) => {
            let x = x.num();
            unc(match op {
                BinOp::Add => y.add_f(x),
                BinOp::Sub => y.rsub_f(x),
                BinOp::Mul => y.mul_f(x),
                BinOp::Div => y.rdiv_f(x),
            })
        }
        (x, y) => {
            let (x, y) = (x.num(), y.num());
            Value::Num(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => fdiv(x, y),
            })
        }
    }
}

fn is_list(v: &Value) -> bool {
    matches!(v, Value::List(_) | Value::UList(_))
}

/// Python's max/min on two values compared by their nominal values: the first stays unless the other is bigger
/// (smaller).
fn py_max(a: Value, b: Value, bigger: bool) -> Value {
    let (x, y) = (nominal(&a), nominal(&b));
    if (bigger && y > x) || (!bigger && y < x) { b } else { a }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    fn unc_err<T>(&self, msg: &str) -> Result<T, RunError> {
        self.err(msg.to_string())
    }

    /// a op b where a or b holds uncertain numbers (e_IBin).
    pub(crate) fn unc_bin(&self, op: BinOp, a: Value, b: Value) -> Result<Value, RunError> {
        match (list_items(&a), list_items(&b)) {
            (Some(xs), Some(ys)) => {
                if xs.len() != ys.len() {
                    return self.err(format!("these two lists have different lengths ({} and {})", xs.len(), ys.len()));
                }
                Ok(make_list(xs.iter().zip(&ys).map(|(x, y)| num_op(op, x, y)).collect()))
            }
            (Some(xs), None) => Ok(make_list(xs.iter().map(|x| num_op(op, x, &b)).collect())),
            (None, Some(ys)) => Ok(make_list(ys.iter().map(|y| num_op(op, &a, y)).collect())),
            (None, None) => match (&a, &b) {
                (Value::Unc(_) | Value::Num(_) | Value::Bool(_), Value::Unc(_) | Value::Num(_) | Value::Bool(_)) => {
                    Ok(num_op(op, &a, &b))
                }
                _ => self.unc_err(GENERIC),
            },
        }
    }

    pub(crate) fn unc_neg(&self, a: Value) -> Result<Value, RunError> {
        let neg = |v: &Value| match v {
            Value::Unc(u) => unc(u.neg()),
            v => Value::Num(-v.num()),
        };
        match list_items(&a) {
            Some(xs) => Ok(make_list(xs.iter().map(neg).collect())),
            None => Ok(neg(&a)),
        }
    }

    pub(crate) fn unc_powc(&self, a: Value, p: f64) -> Result<Value, RunError> {
        let f = |v: &Value| match v {
            Value::Unc(u) => unc(u.powc(p, interp_powc)),
            v => Value::Num(interp_powc(v.num(), p)),
        };
        match list_items(&a) {
            Some(xs) => Ok(make_list(xs.iter().map(f).collect())),
            None => Ok(f(&a)),
        }
    }

    /// a ** b (interp.fpow), a element by element for a list base.
    pub(crate) fn unc_pow(&self, a: Value, b: Value) -> Result<Value, RunError> {
        let f = |x: &Value| match (x, &b) {
            (Value::Unc(u), Value::Unc(o)) => unc(u.pow(o)),
            (Value::Unc(u), o) => unc(u.powc(o.num(), U::pow)),
            (x, Value::Unc(o)) => unc(o.rpow_f(x.num())),
            (x, o) => Value::Num(U::pow(x.num(), o.num())),
        };
        match list_items(&a) {
            Some(xs) => Ok(make_list(xs.iter().map(f).collect())),
            None => Ok(f(&a)),
        }
    }

    /// Comparisons use the nominal values.
    pub(crate) fn unc_cmp(&self, op: CmpOp, a: &Value, b: &Value) -> Result<Value, RunError> {
        if is_list(a) || is_list(b) {
            return self.unc_err(GENERIC);
        }
        let (x, y) = (nominal(a), nominal(b));
        Ok(Value::Bool(match op {
            CmpOp::Eq => x == y,
            CmpOp::Ne => x != y,
            CmpOp::Lt => x < y,
            CmpOp::Gt => x > y,
            CmpOp::Le => x <= y,
            CmpOp::Ge => x >= y,
        }))
    }

    /// name(x) for an uncertain x (uncertain.apply1 over interp.math1).
    fn apply1(&self, name: &str, v: &Value) -> Result<Value, RunError> {
        match v {
            Value::Unc(x) => {
                let r = interp_math1(name, x.v);
                if STEP.contains(&name) {
                    return Ok(Value::Num(r));
                }
                let dv = U::deriv(name, x.v, U::digamma, crate::eval_core::cmath::tgamma).unwrap_or(f64::NAN);
                Ok(unc(UFloat::new(r, U::scale(&x.d, dv))))
            }
            v => Ok(Value::Num(interp_math1(name, v.num()))),
        }
    }

    /// f(args) where some arguments are uncertain (uncertain.lift): partial derivatives given, or by central
    /// differences.
    fn lift(&self, f: &dyn Fn(&[f64]) -> f64, args: &[Value], partials: Option<&dyn Fn(&[f64]) -> Vec<f64>>) -> Value {
        let vals: Vec<f64> = args.iter().map(nominal).collect();
        let r = f(&vals);
        let ps: Vec<f64> = match partials {
            Some(p) => p(&vals),
            None => args
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    if !matches!(a, Value::Unc(_)) {
                        return 0.0;
                    }
                    let h = if vals[i] != 0.0 { 1e-6 * vals[i].abs().max(1e-300) } else { 1e-8 };
                    let (mut up, mut dn) = (vals.clone(), vals.clone());
                    up[i] += h;
                    dn[i] -= h;
                    (f(&up) - f(&dn)) / (2.0 * h)
                })
                .collect(),
        };
        let mut d: Vec<(u64, f64)> = vec![];
        for (a, p) in args.iter().zip(ps) {
            if let Value::Unc(u) = a {
                for &(k, c) in &u.d {
                    match d.iter().position(|(x, _)| *x == k) {
                        Some(i) => d[i].1 += p * c,
                        None => d.push((k, p * c)),
                    }
                }
            }
        }
        d.retain(|(_, c)| *c != 0.0 || c.is_nan());
        unc(UFloat::new(r, d))
    }

    /// A built-in called with uncertain arguments (interp.e_IBuiltin with UFloat values); GENERIC where v1 needs
    /// a plain number.
    pub(crate) fn unc_apply(&mut self, name: &str, args: &[Value]) -> Result<Value, RunError> {
        const MATH: &[&str] = &[
            "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "exp",
            "ln", "log", "log10", "log2", "erf", "erfc", "gamma", "lgamma", "expm1", "log1p", "abs", "floor", "ceil",
            "cot", "sec", "csc", "round", "sign",
        ];
        if MATH.contains(&name) {
            if let Some(xs) = list_items(&args[0]) {
                return Ok(make_list(xs.iter().map(|x| self.apply1(name, x)).collect::<Result<_, _>>()?));
            }
            return self.apply1(name, &args[0]);
        }
        if is_list(&args[0]) && !matches!(name, "len" | "sum" | "mean" | "std" | "min_list" | "max_list" | "first"
                                           | "last" | "dot" | "trapz" | "copy" | "reverse" | "sort" | "cumsum"
                                           | "diff" | "slice" | "min_ew" | "max_ew" | "text_num")
        {
            return self.unc_err(GENERIC);
        }
        use crate::eval_core::cmath;
        use fermium_runtime::numerics::special as sp;
        match name {
            "besselj" => Ok(self.lift(&|v| cmath::jn(v[0], v[1]), args, None)),
            "bessely" => Ok(self.lift(&|v| cmath::yn(v[0], v[1]), args, None)),
            "besseli" => Ok(self.lift(&|v| sp::besseli(v[0], v[1]), args, None)),
            "besselk" => Ok(self.lift(&|v| sp::besselk(v[0], v[1]), args, None)),
            "ellipk" => Ok(self.lift(&|v| sp::ellipk(v[0]), args, None)),
            "ellipe" => Ok(self.lift(&|v| sp::ellipe(v[0]), args, None)),
            "atan2" => Ok(self.lift(&|v| v[0].atan2(v[1]), args,
                                    Some(&|v| vec![fdiv(v[1], v[1] * v[1] + v[0] * v[0]),
                                                   -fdiv(v[0], v[1] * v[1] + v[0] * v[0])]))),
            "hypot" => Ok(self.lift(&|v| v[0].hypot(v[1]), args,
                                    Some(&|v| vec![fdiv(v[0], v[0].hypot(v[1])), fdiv(v[1], v[0].hypot(v[1]))]))),
            "min" | "max" => {
                let mut r = args[0].clone();
                for a in &args[1..] {
                    let (x, y) = (nominal(&r), nominal(a));
                    if (name == "min" && (y < x || x != x)) || (name == "max" && (y > x || x != x)) {
                        r = a.clone();
                    }
                }
                Ok(r)
            }
            "min_ew" | "max_ew" => {
                let lists: Vec<Vec<Value>> = args.iter().filter_map(list_items).collect();
                for l in &lists[1..] {
                    if l.len() != lists[0].len() {
                        return self.err(format!("these two lists have different lengths ({} and {})", lists[0].len(),
                                                l.len()));
                    }
                }
                let mut out = vec![];
                for k in 0..lists[0].len() {
                    let vals: Vec<Value> =
                        args.iter().map(|a| list_items(a).map(|l| l[k].clone()).unwrap_or_else(|| a.clone())).collect();
                    let mut r = vals[0].clone();
                    for v in &vals[1..] {
                        let (x, y) = (nominal(&r), nominal(v));
                        if (name == "min_ew" && (y < x || x != x)) || (name == "max_ew" && (y > x || x != x)) {
                            r = v.clone();
                        }
                    }
                    out.push(r);
                }
                Ok(make_list(out))
            }
            "clamp" => Ok(py_max(py_max(args[0].clone(), args[1].clone(), true), args[2].clone(), false)),
            "factorial" => self.apply1("gamma", &num_op(BinOp::Add, &args[0], &Value::Num(1.0))),
            "text_num" => {
                let fid = args[0].num() as usize;
                let f = crate::printer::print_fmt(&self.module.tables.fmts[fid]);
                Ok(Value::Str(match &args[1] {
                    Value::Unc(u) => fermium_units::quantity::format_uncertain(u.v, u.s(), &f.rdim, f.hint.as_ref()),
                    v => fermium_units::quantity::format_value(v.num(), &f),
                }.into()))
            }
            "approx" => {
                let (x, y, atol, rtol) = (nominal(&args[0]), nominal(&args[1]), nominal(&args[2]), nominal(&args[3]));
                let (diff, sa, sb) = ((x - y).abs(), x.abs(), y.abs());
                let tol = atol.max(rtol * sa.max(sb));
                Ok(Value::Bool(x == y || (diff <= tol && diff != f64::INFINITY)))
            }
            "len" => Ok(Value::Num(list_items(&args[0]).map(|l| l.len()).unwrap_or(0) as f64)),
            "sum" | "mean" | "std" | "min_list" | "max_list" | "first" | "last" => self.unc_reduce(name, &args[0]),
            "dot" => {
                let (a, c) = (list_items(&args[0]).unwrap_or_default(), list_items(&args[1]).unwrap_or_default());
                if a.len() != c.len() {
                    return self.err(format!("these two lists have different lengths ({} and {})", a.len(), c.len()));
                }
                let mut acc = Value::Num(0.0);
                for (x, y) in a.iter().zip(&c) {
                    acc = num_op(BinOp::Add, &acc, &num_op(BinOp::Mul, x, y));
                }
                Ok(acc)
            }
            "trapz" => {
                let (ys, xs) = (list_items(&args[0]).unwrap_or_default(), list_items(&args[1]).unwrap_or_default());
                if ys.len() != xs.len() {
                    return self.err(format!("these two lists have different lengths ({} and {})", ys.len(), xs.len()));
                }
                let mut acc = Value::Num(0.0);
                for i in 1..ys.len() {
                    let dx = num_op(BinOp::Sub, &xs[i], &xs[i - 1]);
                    let s = num_op(BinOp::Add, &ys[i], &ys[i - 1]);
                    acc = num_op(BinOp::Add, &acc, &num_op(BinOp::Mul, &Value::Num(0.5), &num_op(BinOp::Mul, &dx, &s)));
                }
                Ok(acc)
            }
            "copy" => Ok(make_list(list_items(&args[0]).unwrap_or_default())),
            "reverse" => Ok(make_list(list_items(&args[0]).unwrap_or_default().into_iter().rev().collect())),
            "sort" => {
                let mut v = list_items(&args[0]).unwrap_or_default();
                v.sort_by(|a, b| {
                    let (x, y) = (nominal(a), nominal(b));
                    let ka = (x.is_nan(), if x.is_nan() { 0.0 } else { x });
                    let kb = (y.is_nan(), if y.is_nan() { 0.0 } else { y });
                    ka.partial_cmp(&kb).unwrap()
                });
                Ok(make_list(v))
            }
            "cumsum" => {
                let mut acc = Value::Num(0.0);
                let mut out = vec![];
                for x in list_items(&args[0]).unwrap_or_default() {
                    acc = num_op(BinOp::Add, &acc, &x);
                    out.push(acc.clone());
                }
                Ok(make_list(out))
            }
            "diff" => {
                let v = list_items(&args[0]).unwrap_or_default();
                Ok(make_list(v.windows(2).map(|w| num_op(BinOp::Sub, &w[1], &w[0])).collect()))
            }
            "slice" => {
                let v = list_items(&args[0]).unwrap_or_default();
                let (lo, hi) = (args[1].num(), args[2].num());
                if hi == lo - 1.0 {
                    return Ok(make_list(vec![]));
                }
                if hi < lo - 1.0 {
                    return self.err("a slice xs[a:b] runs from a up to b, so b can't be smaller than a − 1 (xs[a:a-1] \
                                     is the empty list); to reverse a list use reverse(xs)");
                }
                let i = self.elem_index(lo, v.len())?;
                let j = self.elem_index(hi, v.len())?;
                Ok(make_list(v[i..=j].to_vec()))
            }
            _ => self.unc_err(GENERIC),
        }
    }

    /// sum, mean, std, … of a list holding uncertain numbers (interp.reduce).
    fn unc_reduce(&self, name: &str, lst: &Value) -> Result<Value, RunError> {
        let v = list_items(lst).unwrap_or_default();
        let n = v.len();
        if n < 1 && name != "sum" {
            return self.err("this list is empty");
        }
        if name == "std" && n < 2 {
            return self.err(crate::eval_core::STD_ONE_TEXT);
        }
        match name {
            "first" => return Ok(v[0].clone()),
            "last" => return Ok(v[n - 1].clone()),
            "min_list" | "max_list" => {
                let mut r = v[0].clone();
                for x in &v[1..] {
                    let (a, b) = (nominal(&r), nominal(x));
                    if (name == "min_list" && (b < a || a != a)) || (name == "max_list" && (b > a || a != a)) {
                        r = x.clone();
                    }
                }
                return Ok(r);
            }
            _ => {}
        }
        // sum_seq: left to right, starting from the first element
        let mut s = if n > 0 { v[0].clone() } else { Value::Num(0.0) };
        for x in v.iter().skip(1) {
            s = num_op(BinOp::Add, &s, x);
        }
        if name == "sum" {
            return Ok(s);
        }
        let mean = num_op(BinOp::Div, &s, &Value::Num(n as f64));
        if name == "mean" {
            return Ok(mean);
        }
        let mut acc = Value::Num(0.0);
        for x in &v {
            let d = num_op(BinOp::Sub, x, &mean);
            acc = num_op(BinOp::Add, &acc, &num_op(BinOp::Mul, &d, &d));
        }
        let q = num_op(BinOp::Div, &acc, &Value::Num(n as f64 - 1.0));
        match q {
            Value::Unc(_) => self.unc_powc(q, 0.5),
            q => Ok(Value::Num(q.num().sqrt())),
        }
    }

    pub(crate) fn builtin_uncertain(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        match name {
            "pm" | "pm_rel" => Some(self.pm_builtin(name == "pm_rel", args)),
            "unc_value" | "unc_uncertainty" | "unc_rel" => {
                let f = |x: &Value| match name {
                    "unc_value" => nominal(x),
                    "unc_uncertainty" => sigma(x),
                    _ => fdiv(sigma(x), nominal(x).abs()),
                };
                Some(Ok(match list_items(&args[0]) {
                    Some(xs) => Value::List(Rc::new(RefCell::new(xs.iter().map(f).collect()))),
                    None => Value::Num(f(&args[0])),
                }))
            }
            _ => None,
        }
    }

    fn pm_builtin(&mut self, rel: bool, args: &[Value]) -> Result<Value, RunError> {
        let off = args.get(2).map(Value::num).unwrap_or(0.0);
        if let Some(vs) = list_items(&args[0]) {
            let sgs = list_items(&args[1]).unwrap_or_else(|| vec![args[1].clone(); vs.len()]);
            if sgs.len() != vs.len() {
                return self.err(format!("these two lists have different lengths ({} and {})", vs.len(), sgs.len()));
            }
            let out = vs.iter().zip(&sgs).map(|(x, y)| self.pm(x, y, rel, off)).collect::<Result<_, _>>()?;
            return Ok(make_list(out));
        }
        self.pm(&args[0], &args[1], rel, off)
    }

    fn pm(&self, v: &Value, sg: &Value, rel: bool, off: f64) -> Result<Value, RunError> {
        let mut sg = nominal(sg); // the uncertainty of an uncertainty isn't propagated
        if rel {
            sg *= (nominal(v) - off).abs();
        }
        if sg < 0.0 || sg.is_nan() {
            return self.err(format!("an uncertainty after ± can't be negative or NaN (got {} in SI units)",
                                    py_g(sg)));
        }
        Ok(match v {
            Value::Unc(u) => unc(u.add(&UFloat::measured(0.0, sg))),
            v => unc(UFloat::measured(v.num(), sg)),
        })
    }

    /// print of a number or list that holds uncertain values: "9.81 ± 0.12 m/s²" (D121).
    pub(crate) fn unc_print(&mut self, v: &Value, fmt: usize) -> bool {
        let f = crate::printer::print_fmt(&self.module.tables.fmts[fmt]);
        let text = match v {
            Value::Unc(u) => fermium_units::quantity::format_uncertain(u.v, u.s(), &f.rdim, f.hint.as_ref()),
            Value::UList(l) => {
                let un = fermium_units::display_unit(&f.rdim, f.hint.as_ref());
                let vals: Vec<(f64, Option<f64>)> = l.borrow().iter().map(|x| match x {
                    Value::Unc(u) => (u.v, Some(u.s())),
                    x => (x.num(), None),
                }).collect();
                fermium_units::quantity::format_uncertain_list(&vals, &un)
            }
            _ => return false,
        };
        self.printer.text(&text);
        true
    }

    pub(crate) fn eval_uncertain(&mut self, _e: &Expr, _fr: &mut Frame) -> Result<Value, RunError> {
        self.err("this isn't supported by the Rust back end yet")
    }

    pub(crate) fn stmt_propagate(&mut self, s: &Stmt, _fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Propagate { .. } = &s.kind else { unreachable!() };
        self.err("propagate montecarlo isn't supported by the Rust back end yet")
    }
}

/// Python's `f"{x:g}"`.
fn py_g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    let _ = format_number6;
    let exp = format!("{:.5e}", x);
    let (m, e) = exp.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    if (-4..6).contains(&e) {
        let s = format!("{:.*}", (5 - e).max(0) as usize, x);
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    } else {
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}
