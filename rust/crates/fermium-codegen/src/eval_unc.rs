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

/// Does a value hold an uncertain number (or a list with one, or Monte Carlo samples)?
pub fn is_unc(v: &Value) -> bool {
    matches!(v, Value::Unc(_) | Value::UList(_) | Value::Arr(_) | Value::UVec(_))
}

/// UFloat.__round__'s message: printing a vector or matrix whose components are uncertain.
pub const VEC_UNC: &str = "vectors and matrices of uncertain values (±) aren't supported yet; work with the uncertain \
                           numbers one at a time, or use value(x) to drop the uncertainty";

/// A vector's components as values (numbers or uncertain numbers).
pub fn vec_items(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Vec(x) => Some(x.iter().map(|x| Value::Num(*x)).collect()),
        Value::UVec(x) => Some(x.to_vec()),
        _ => None,
    }
}

/// A vector from its components: plain when none is uncertain (v1's tuple).
pub fn make_vec(items: Vec<Value>) -> Value {
    if items.iter().any(|x| matches!(x, Value::Unc(_))) {
        Value::UVec(Rc::new(items))
    } else {
        Value::Vec(Rc::new(items.iter().map(Value::num).collect()))
    }
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

/// interp.powc on a NumPy array of samples (x ** p with NumPy's fast paths, np.cbrt, real odd roots).
fn np_powc(a: &[f64], p: f64) -> Vec<f64> {
    let npow = |x: f64, p: f64| -> f64 {
        // ndarray ** scalar: NumPy's fast_scalar_power for these exponents
        if p == 2.0 {
            x * x
        } else if p == 0.5 {
            x.sqrt()
        } else if p == -1.0 {
            1.0 / x
        } else if p == 1.0 {
            x
        } else if p == 0.0 {
            1.0
        } else {
            x.powf(p)
        }
    };
    if p == p.trunc() {
        return a.iter().map(|&x| npow(x, p)).collect();
    }
    if (p - 1.0 / 3.0).abs() < 1e-15 {
        return a.iter().map(|&x| crate::eval_core::glibc_cbrt(x)).collect();
    }
    let n = crate::eval::odd_root_numerator(p);
    a.iter()
        .map(|&x| {
            let r = npow(x.abs(), p);
            match n {
                None => if x < 0.0 { f64::NAN } else { r },
                Some(n) => if x < 0.0 { if n % 2 != 0 { -r } else { r } } else { r },
            }
        })
        .collect()
}

/// One element-wise arithmetic operation, where either side may be uncertain.
pub(crate) fn num_op(op: BinOp, a: &Value, b: &Value) -> Value {
    let f = |x: f64, y: f64| match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div => fdiv(x, y),
    };
    match (a, b) {
        // Monte Carlo samples, element by element (NumPy)
        (Value::Arr(x), Value::Arr(y)) => return Value::Arr(Rc::new(x.iter().zip(y.iter()).map(|(p, q)| f(*p, *q)).collect())),
        (Value::Arr(x), Value::Num(_) | Value::Bool(_)) => {
            let y = b.num();
            return Value::Arr(Rc::new(x.iter().map(|p| f(*p, y)).collect()));
        }
        (Value::Num(_) | Value::Bool(_), Value::Arr(y)) => {
            let x = a.num();
            return Value::Arr(Rc::new(y.iter().map(|q| f(x, *q)).collect()));
        }
        (Value::Arr(_), _) | (_, Value::Arr(_)) => {
            vec_fail();
            return Value::Num(f64::NAN);
        }
        _ => {}
    }
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

    /// A number where v1 needs a plain one (an index, a loop bound: Python's int()/float() of a UFloat raises).
    #[inline]
    pub(crate) fn plain(&self, v: &Value) -> Result<f64, RunError> {
        match v {
            Value::Num(x) => Ok(*x),
            Value::Unc(_) | Value::UList(_) | Value::UVec(_) => self.unc_err(GENERIC),
            v => Ok(v.num()),
        }
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
                (Value::Unc(_) | Value::Num(_) | Value::Bool(_) | Value::Arr(_),
                 Value::Unc(_) | Value::Num(_) | Value::Bool(_) | Value::Arr(_)) => {
                    Ok(num_op(op, &a, &b))
                }
                _ => self.unc_err(GENERIC),
            },
        }
    }

    pub(crate) fn unc_neg(&self, a: Value) -> Result<Value, RunError> {
        let neg = |v: &Value| match v {
            Value::Unc(u) => unc(u.neg()),
            Value::Arr(a) => Value::Arr(Rc::new(a.iter().map(|x| -x).collect())),
            v => Value::Num(-v.num()),
        };
        if let Value::UVec(xs) = &a {
            return Ok(make_vec(xs.iter().map(neg).collect())); // e_INeg on a tuple
        }
        match list_items(&a) {
            Some(xs) => Ok(make_list(xs.iter().map(neg).collect())),
            None => Ok(neg(&a)),
        }
    }

    /// a op b whose result is a vector or matrix with an uncertain operand (e_IBin with VecTy/MatTy: component by
    /// component, a number used for every component).
    pub(crate) fn unc_vec_bin(&self, op: BinOp, a: Value, b: Value) -> Result<Value, RunError> {
        let (xs, ys) = (vec_items(&a), vec_items(&b));
        let ok = |v: &Value, items: &Option<Vec<Value>>| match items {
            Some(_) => true,
            None => matches!(v, Value::Num(_) | Value::Unc(_) | Value::Bool(_)),
        };
        if !ok(&a, &xs) || !ok(&b, &ys) || (xs.is_none() && ys.is_none()) {
            return self.unc_bin(op, a, b);
        }
        let n = xs.as_ref().or(ys.as_ref()).map(|v| v.len()).unwrap_or(0);
        let av = xs.unwrap_or_else(|| vec![a.clone(); n]);
        let bv = ys.unwrap_or_else(|| vec![b.clone(); n]);
        Ok(make_vec(av.iter().zip(&bv).map(|(x, y)| num_op(op, x, y)).collect()))
    }

    pub(crate) fn unc_powc(&self, a: Value, p: f64) -> Result<Value, RunError> {
        let f = |v: &Value| match v {
            Value::Unc(u) => unc(u.powc(p, interp_powc)),
            Value::Arr(a) => Value::Arr(Rc::new(np_powc(a, p))),
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
            // np.power on samples
            (Value::Arr(a), Value::Arr(o)) => Value::Arr(Rc::new(a.iter().zip(o.iter()).map(|(p, q)| p.powf(*q)).collect())),
            (Value::Arr(a), Value::Num(q)) => Value::Arr(Rc::new(a.iter().map(|p| p.powf(*q)).collect())),
            (Value::Num(p), Value::Arr(o)) => Value::Arr(Rc::new(o.iter().map(|q| p.powf(*q)).collect())),
            (Value::Arr(_), _) | (_, Value::Arr(_)) => {
                vec_fail();
                Value::Num(f64::NAN)
            }
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
        if matches!(a, Value::Arr(_)) || matches!(b, Value::Arr(_)) {
            vec_fail(); // an array of true/false can't decide an if
            return Ok(Value::Bool(false));
        }
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
            Value::Arr(a) => {
                // interp.math1 on a NumPy array: NumPy's function, except the ones it doesn't vectorize
                const VEC: &[&str] = &["sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh",
                                       "acosh", "atanh", "exp", "ln", "log", "log10", "log2", "expm1", "log1p", "abs",
                                       "floor", "ceil", "sign"];
                if !VEC.contains(&name) {
                    vec_fail();
                    return Ok(Value::Num(f64::NAN));
                }
                Ok(Value::Arr(Rc::new(a.iter().map(|x| interp_math1(name, *x)).collect())))
            }
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
        if name == "shuffle" {
            // a transpose or a row/column picked out (matrix_op on a tuple of UFloat values)
            let src = vec_items(&args[0]).unwrap_or_default();
            return Ok(make_vec(args[1..].iter().map(|k| src[k.num() as usize].clone()).collect()));
        }
        if name == "matmul" {
            // linalg.matmul with FloatOps on UFloat values: the same order of operations
            let (a, b) = match (vec_items(&args[0]), vec_items(&args[1])) {
                (Some(a), Some(b)) => (a, b),
                _ => return self.unc_err(GENERIC),
            };
            let (r, k, c) = (args[2].num() as usize, args[3].num() as usize, args[4].num() as usize);
            let mut out = Vec::with_capacity(r * c);
            for i in 0..r {
                for j in 0..c {
                    let mut acc = num_op(BinOp::Mul, &a[i * k], &b[j]);
                    for m in 1..k {
                        acc = num_op(BinOp::Add, &acc, &num_op(BinOp::Mul, &a[i * k + m], &b[m * c + j]));
                    }
                    out.push(acc);
                }
            }
            return Ok(if out.len() == 1 { out.pop().unwrap() } else { make_vec(out) });
        }
        if matches!(name, "det" | "inverse" | "solve_linear" | "eigenvalues" | "eigenvectors") {
            // matrix_op: linalg.py / linalg_big.py with FloatOps on UFloat values (B-U1)
            use crate::eval_unc_la::LaOut;
            use fermium_runtime::numerics::{err, Fail};
            let mats: Option<Vec<Vec<Value>>> = args.iter().map(vec_items).collect();
            let Some(mats) = mats else { return self.unc_err(GENERIC) };
            return match crate::eval_unc_la::unc_matrix_op(name, &mats) {
                Some(LaOut::Val(v)) => Ok(v),
                Some(LaOut::Singular) => Err(self.fail_kind(Fail::new(err::SINGULAR, 0.0, 0.0))),
                Some(LaOut::NotSymmetric) => Err(self.fail_kind(Fail::new(err::NOT_SYMMETRIC, 0.0, 0.0))),
                Some(LaOut::NotPosDef) => Err(self.fail_kind(Fail::new(err::NOT_POSDEF, 0.0, 0.0))),
                Some(LaOut::Generic) | None => self.unc_err(GENERIC),
            };
        }
        if matches!(name, "vdot" | "cross") {
            // sum_seq of the products / the cross product of the components (e_IBuiltin on tuples)
            let (a, b) = match (vec_items(&args[0]), vec_items(&args[1])) {
                (Some(a), Some(b)) => (a, b),
                _ => return self.unc_err(GENERIC),
            };
            let mul = |x: &Value, y: &Value| num_op(BinOp::Mul, x, y);
            let sub = |x: Value, y: Value| num_op(BinOp::Sub, &x, &y);
            if name == "vdot" {
                let mut acc = mul(&a[0], &b[0]);
                for k in 1..a.len().min(b.len()) {
                    acc = num_op(BinOp::Add, &acc, &mul(&a[k], &b[k]));
                }
                return Ok(acc);
            }
            if a.len() == 2 {
                return Ok(sub(mul(&a[0], &b[1]), mul(&a[1], &b[0])));
            }
            return Ok(make_vec(vec![sub(mul(&a[1], &b[2]), mul(&a[2], &b[1])),
                                    sub(mul(&a[2], &b[0]), mul(&a[0], &b[2])),
                                    sub(mul(&a[0], &b[1]), mul(&a[1], &b[0]))]));
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
                let (lo, hi) = (self.plain(&args[1])?, self.plain(&args[2])?);
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
            // math.sqrt of a UFloat calls float(): UncertainUse (v1 has no uncertain std)
            Value::Unc(_) => self.unc_err(GENERIC),
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

    /// Σ over terms that may be uncertain (interp.e_ISum: acc = acc + f(lo + i·st)).
    pub(crate) fn unc_sum(&mut self, lam: usize, a: f64, st: f64, n: i64, fr: &mut Frame) -> Result<Value, RunError> {
        let m: &'m fermium_ir::Module = self.module;
        let l = &m.lambdas[lam];
        let p = l.params[0];
        let line = self.line;
        let old = fr.vars.get(&p).cloned();
        let mut acc = Value::Num(0.0);
        let mut res = Ok(());
        for i in 0..n {
            fr.vars.insert(p, Value::Num(a + i as f64 * st));
            match self.eval(&l.body[0], fr) {
                Ok(v) => acc = num_op(BinOp::Add, &acc, &v),
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        match old {
            Some(v) => {
                fr.vars.insert(p, v);
            }
            None => {
                fr.vars.remove(&p);
            }
        }
        res?;
        self.line = line;
        Ok(acc)
    }

    pub(crate) fn eval_uncertain(&mut self, _e: &Expr, _fr: &mut Frame) -> Result<Value, RunError> {
        self.err("this isn't supported by the Rust back end yet")
    }

}

// ---------------------------------------------------------------- propagate montecarlo (D123)

/// A Monte Carlo sample stream: an uncertainty source of the inputs, or a ± written inside the block (its IR node
/// and list element).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    Src(u64),
    Pm(usize, usize),
}

/// The sampler of interp._Sampler: all samples at once (v1's NumPy arrays) or the k-th one.
struct Mc {
    /// None: vectorized; Some(k): the k-th sample
    k: Option<usize>,
    /// samples per stream (drawn at a stream's first use)
    n: usize,
    zs: Vec<(Key, Vec<f64>)>,
}

thread_local! {
    static MC: RefCell<Option<Mc>> = const { RefCell::new(None) };
    static VEC_FAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a propagate montecarlo block running?
pub fn mc_active() -> bool {
    MC.with(|m| m.borrow().is_some())
}

/// The vectorized Monte Carlo attempt reached something NumPy arrays can't do: fall back to one sample at a time.
pub fn vec_fail() {
    VEC_FAIL.with(|f| f.set(true));
}

fn failed() -> bool {
    VEC_FAIL.with(|f| f.get())
}

/// The samples of one stream (drawn from the program's random numbers at its first use).
fn z_of(m: &mut Mc, key: Key) -> usize {
    if let Some(i) = m.zs.iter().position(|(k, _)| *k == key) {
        return i;
    }
    let v: Vec<f64> = (0..m.n).map(|_| crate::eval_m3::randn_draw()).collect();
    m.zs.push((key, v));
    m.zs.len() - 1
}

/// A variable's value inside the block: uncertain numbers become samples (_Sampler.sample).
pub fn mc_sample(v: &Value) -> Value {
    MC.with(|m| {
        let mut g = m.borrow_mut();
        let Some(m) = g.as_mut() else { return v.clone() };
        sample_in(m, v)
    })
}

fn sample_in(m: &mut Mc, v: &Value) -> Value {
    match v {
        Value::Unc(u) => match m.k {
            None => {
                let mut out = vec![u.v; m.n];
                for &(key, c) in &u.d {
                    let i = z_of(m, Key::Src(key));
                    let z = &m.zs[i].1;
                    for (o, zk) in out.iter_mut().zip(z) {
                        *o += c * zk;
                    }
                }
                Value::Arr(Rc::new(out))
            }
            Some(k) => {
                let mut parts = vec![];
                for &(key, c) in &u.d {
                    let i = z_of(m, Key::Src(key));
                    parts.push(c * m.zs[i].1[k]);
                }
                Value::Num(u.v + U::fsum(parts))
            }
        },
        Value::UList(l) => {
            if m.k.is_none() {
                vec_fail(); // a list of arrays
                return v.clone();
            }
            let items: Vec<Value> = l.borrow().iter().map(|x| sample_in(m, x)).collect();
            Value::List(Rc::new(RefCell::new(items.iter().map(Value::num).collect())))
        }
        v => v.clone(),
    }
}

/// numpy's pairwise summation of a contiguous float64 array (pairwise_sum_DOUBLE), as np.add.reduce uses it: the
/// first element, plus the pairwise sum of the rest.
pub fn np_sum(a: &[f64]) -> f64 {
    fn pw(a: &[f64]) -> f64 {
        let n = a.len();
        if n < 8 {
            let mut res = 0.0;
            for x in a {
                res += x;
            }
            res
        } else if n <= 128 {
            let mut r = [0.0f64; 8];
            r.copy_from_slice(&a[..8]);
            let mut i = 8;
            while i < n - (n % 8) {
                for j in 0..8 {
                    r[j] += a[i + j];
                }
                i += 8;
            }
            let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
            while i < n {
                res += a[i];
                i += 1;
            }
            res
        } else {
            let mut n2 = n / 2;
            n2 -= n2 % 8;
            pw(&a[..n2]) + pw(&a[n2..])
        }
    }
    if a.is_empty() {
        return 0.0;
    }
    a[0] + pw(&a[1..])
}

/// The least-squares solution of Zc β ≈ y (numpy.linalg.lstsq) by Householder QR; Zc is n×k, column-major.
fn lstsq(cols: &[Vec<f64>], y: &[f64]) -> Vec<f64> {
    let k = cols.len();
    let n = y.len();
    let mut a: Vec<Vec<f64>> = cols.to_vec();
    let mut b = y.to_vec();
    let mut diag = vec![0.0; k];
    for j in 0..k {
        let norm = a[j][j..].iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm == 0.0 {
            diag[j] = 0.0;
            continue;
        }
        let alpha = if a[j][j] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = a[j][j..].to_vec();
        v[0] -= alpha;
        let vn = v.iter().map(|x| x * x).sum::<f64>();
        if vn == 0.0 {
            diag[j] = alpha;
            continue;
        }
        for c in a.iter_mut().skip(j) {
            let dot: f64 = v.iter().zip(&c[j..]).map(|(p, q)| p * q).sum();
            let f = 2.0 * dot / vn;
            for (x, vi) in c[j..].iter_mut().zip(&v) {
                *x -= f * vi;
            }
        }
        let dot: f64 = v.iter().zip(&b[j..]).map(|(p, q)| p * q).sum();
        let f = 2.0 * dot / vn;
        for (x, vi) in b[j..].iter_mut().zip(&v) {
            *x -= f * vi;
        }
        diag[j] = a[j][j];
    }
    let _ = n;
    let mut beta = vec![0.0; k];
    for j in (0..k).rev() {
        if diag[j] == 0.0 {
            beta[j] = 0.0;
            continue;
        }
        let mut s = b[j];
        for (c, bc) in a.iter().zip(&beta).skip(j + 1) {
            s -= c[j] * bc;
        }
        beta[j] = s / diag[j];
    }
    beta
}

fn walk_assigned(stmts: &[Stmt], out: &mut Vec<fermium_ir::SymId>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Assign(sym, _) => {
                if !out.contains(sym) {
                    out.push(*sym);
                }
            }
            StmtKind::If(_, a, b) => {
                walk_assigned(a, out);
                walk_assigned(b, out);
            }
            StmtKind::While(_, b) | StmtKind::For { body: b, .. } | StmtKind::ForIn(_, _, b) => walk_assigned(b, out),
            StmtKind::Propagate { body, .. } => walk_assigned(body, out),
            _ => {}
        }
    }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// a ± σ inside the block: a new stream, the same one for every sample of this ± and list element
    /// (_Sampler.new, through interp.pm).
    pub(crate) fn mc_pm(&mut self, id: usize, rel: bool, args: &[Value]) -> Result<Value, RunError> {
        let off = args.get(2).map(Value::num).unwrap_or(0.0);
        let one = |me: &Self, i: usize, v: &Value, sg: &Value| -> Result<Value, RunError> {
            let mut sgv = match sg {
                Value::Unc(u) => Value::Num(u.v),
                s => s.clone(),
            };
            if rel {
                sgv = num_op(BinOp::Mul, &me.unc_abs(num_op(BinOp::Sub, &Value::Num(nominal_or(v)), &Value::Num(off))),
                             &sgv);
                if let Value::Unc(_) = v {
                    // U.nominal of an uncertain v, as a plain number: done above
                }
            }
            let sgf = match &sgv {
                Value::Arr(_) => {
                    vec_fail(); // a sampled uncertainty
                    return Ok(Value::Num(f64::NAN));
                }
                s => s.num(),
            };
            if sgf < 0.0 || sgf.is_nan() {
                return me.err(format!("an uncertainty after ± can't be negative or NaN (got {} in SI units)", py_g(sgf)));
            }
            MC.with(|m| {
                let mut g = m.borrow_mut();
                let m = g.as_mut().unwrap();
                let zi = z_of(m, Key::Pm(id, i));
                let v = sample_in(m, v);
                let z = &m.zs[zi].1;
                Ok(match (m.k, v) {
                    (None, Value::Arr(a)) => Value::Arr(Rc::new(a.iter().zip(z).map(|(x, zk)| x + sgf * zk).collect())),
                    (None, v) => {
                        let x = v.num();
                        Value::Arr(Rc::new(z.iter().map(|zk| x + sgf * zk).collect()))
                    }
                    (Some(k), v) => Value::Num(v.num() + sgf * z[k]),
                })
            })
        };
        if let Some(vs) = list_items(&args[0]) {
            let sgs = list_items(&args[1]).unwrap_or_else(|| vec![args[1].clone(); vs.len()]);
            if sgs.len() != vs.len() {
                return self.err(format!("these two lists have different lengths ({} and {})", vs.len(), sgs.len()));
            }
            let out: Vec<Value> = vs.iter().zip(&sgs).enumerate().map(|(i, (x, y))| one(self, i, x, y))
                .collect::<Result<_, _>>()?;
            if out.iter().any(|x| matches!(x, Value::Arr(_))) {
                vec_fail(); // a list of arrays
                return Ok(Value::Num(f64::NAN));
            }
            return Ok(make_list(out));
        }
        one(self, 0, &args[0], &args[1])
    }

    fn unc_abs(&self, v: Value) -> Value {
        match v {
            Value::Num(x) => Value::Num(x.abs()),
            Value::Arr(a) => Value::Arr(Rc::new(a.iter().map(|x| x.abs()).collect())),
            v => v,
        }
    }

    /// propagate montecarlo (D123): the block runs with every uncertain input replaced by samples, first all at
    /// once (v1's NumPy arrays), else one sample at a time; the outputs become mean ± standard deviation, linked to
    /// the input sources by regression so later formulas keep the correlations (interp.s_SPropagate).
    pub(crate) fn stmt_propagate(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        const MC_DEFAULT: usize = 100_000;
        const MC_DEFAULT_SLOW: usize = 10_000;
        let StmtKind::Propagate { n: ne, body, outs } = &s.kind else { unreachable!() };
        let given = ne.is_some();
        let n = match ne {
            Some(e) => {
                let x = self.eval(e, fr)?.num();
                if x.is_nan() {
                    return self.err("the length of a list must be a number, not NaN");
                }
                if x > crate::eval_core::MAX_LIST {
                    return self.err(format!("not enough memory for a list of {} numbers (the most is 10⁹)",
                                            format_number6(x)));
                }
                if x < 0.0 { 0 } else { x as usize }
            }
            None => MC_DEFAULT,
        };
        if n < 2 {
            return self.err("propagate montecarlo needs at least 2 samples");
        }
        let mut assigned = vec![];
        walk_assigned(body, &mut assigned);
        let snap: Vec<(fermium_ir::SymId, Option<Value>)> =
            assigned.iter().map(|&sym| (sym, self.get(sym, fr).ok())).collect();
        let restore = |me: &mut Self, fr: &mut Frame| {
            for (sym, v) in &snap {
                if let Some(v) = v {
                    me.set(*sym, v.clone(), fr);
                }
            }
        };
        let line0 = self.line;
        // all the samples at once
        restore(self, fr);
        VEC_FAIL.with(|f| f.set(false));
        MC.with(|m| *m.borrow_mut() = Some(Mc { k: None, n, zs: vec![] }));
        let r = self.block(body, fr);
        let mut results: Option<Vec<Vec<f64>>> = None;
        if r.is_ok() && !failed() {
            let mut rs = vec![];
            for &sym in outs {
                match self.get(sym, fr) {
                    Ok(Value::Arr(a)) if !failed() => rs.push(a.to_vec()),
                    Ok(Value::Num(x)) => rs.push(vec![x; n]),
                    _ => {
                        vec_fail();
                        break;
                    }
                }
            }
            if !failed() {
                results = Some(rs);
            }
        }
        let zs = MC.with(|m| m.borrow_mut().take()).map(|m| m.zs).unwrap_or_default();
        VEC_FAIL.with(|f| f.set(false));
        self.line = line0;
        let mut zs = zs;
        let mut n = n;
        let results = match results {
            Some(r) => r,
            None => {
                // one sample at a time (the streams drawn so far are kept)
                let m = if given { n } else { MC_DEFAULT_SLOW };
                let mut cols: Vec<Vec<f64>> = vec![vec![]; outs.len()];
                let mut err = None;
                for k in 0..m {
                    restore(self, fr);
                    MC.with(|mc| *mc.borrow_mut() = Some(Mc { k: Some(k), n, zs: std::mem::take(&mut zs) }));
                    let r = self.block(body, fr);
                    zs = MC.with(|mc| mc.borrow_mut().take()).map(|mc| mc.zs).unwrap_or_default();
                    if let Err(e) = r {
                        err = Some(e);
                        break;
                    }
                    for (c, &sym) in cols.iter_mut().zip(outs) {
                        let v = self.get(sym, fr)?;
                        c.push(match v {
                            Value::Unc(_) => f64::NAN,
                            v => v.num(),
                        });
                    }
                }
                VEC_FAIL.with(|f| f.set(false));
                if let Some(e) = err {
                    return Err(e);
                }
                n = m;
                cols
            }
        };
        // link the ± inside the block to new sources, shared by all the outputs
        let src: Vec<u64> = zs.iter().map(|(k, _)| match k {
            Key::Src(id) => *id,
            Key::Pm(..) => U::new_source(),
        }).collect();
        for (&sym, y) in outs.iter().zip(&results) {
            let bad = y.iter().filter(|x| !x.is_finite()).count();
            if bad > 0 {
                let name = self.module.syms[sym].name.clone();
                return Err(RunError { message: format!("propagate montecarlo: {bad} of the {n} samples of {name} aren't \
                                                        finite numbers (the formula fails for some sampled inputs)"),
                                      line: line0, hint: None });
            }
            let nf = n as f64;
            let ymean = np_sum(y) / nf;
            let mut mean = ymean;
            let dev: Vec<f64> = y.iter().map(|x| x - ymean).collect();
            let var = np_sum(&dev.iter().map(|x| x * x).collect::<Vec<_>>()) / (nf - 1.0);
            if !(var > 0.0) {
                self.set(sym, unc(UFloat::new(mean, vec![])), fr);
                continue;
            }
            let dy: Vec<f64> = y.iter().map(|x| x - ymean).collect();
            let mut d: Vec<(u64, f64)> = vec![];
            let mut r = dy.clone();
            if !zs.is_empty() {
                let zmeans: Vec<f64> = zs.iter().map(|(_, z)| np_sum(&z[..n]) / nf).collect();
                let zc: Vec<Vec<f64>> = zs.iter().zip(&zmeans).map(|((_, z), m)| z[..n].iter().map(|x| x - m).collect())
                    .collect();
                let beta = lstsq(&zc, &dy);
                for (i, ri) in r.iter_mut().enumerate() {
                    let mut s = 0.0;
                    for (c, b) in zc.iter().zip(&beta) {
                        s += c[i] * b;
                    }
                    *ri = dy[i] - s;
                }
                // the value: the fitted linear model at z = 0 (the inputs' true values)
                let mut corr = 0.0;
                for (m, b) in zmeans.iter().zip(&beta) {
                    corr += m * b;
                }
                mean -= corr;
                d = src.iter().zip(&beta).filter(|(_, b)| **b != 0.0).map(|(k, b)| (*k, *b)).collect();
            }
            let resid = r.iter().map(|x| x * x).sum::<f64>() / (nf - 1.0);
            if resid > 1e-20 * var {
                d.push((U::new_source(), resid.sqrt())); // the nonlinear part: its own source
            }
            self.set(sym, unc(UFloat::new(mean, d)), fr);
        }
        Ok(())
    }
}

fn nominal_or(v: &Value) -> f64 {
    match v {
        Value::Unc(u) => u.v,
        v => v.num(),
    }
}

/// The error for a numerical kernel's callback that returned an uncertain value (interp.plain_fn).
pub fn kernel_unc_error(lambda_name: &str, v: &Value, line: u32) -> RunError {
    let what = if lambda_name.starts_with("integrand") {
        "an integral"
    } else if lambda_name.starts_with("root") {
        "solve … for x"
    } else {
        return RunError { message: GENERIC.into(), line, hint: None };
    };
    let _ = v;
    RunError { message: format!("{what} can't use uncertain values (±) yet; put it inside a  propagate montecarlo  \
                                 block, or use value(x)"), line, hint: None }
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
