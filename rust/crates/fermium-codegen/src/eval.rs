//! The tree-walking back end: runs the typed IR directly (a port of `fermium/interp.py`, the reference
//! interpreter of Fermium 1.5). The LLVM back end must print exactly what this prints.
//!
//! Numerics (integrals, ODEs, roots, fits, eigenvalues, FFT) come from fermium-runtime; printing goes through
//! the [`Printer`] trait, implemented by the runtime with the units crate's formatting rules (D11).
use fermium_ir::{BinOp, CmpOp, Expr, ExprKind, Module, PrintItem, Stmt, StmtKind, SymId, Ty};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A run-time value. Units are erased: every number is in SI base units.
#[derive(Clone, Debug)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(Rc<str>),
    /// Lists are shared references, like Python lists (D26).
    List(Rc<RefCell<Vec<f64>>>),
    /// Vectors, matrices (row-major) and complex numbers (re, im).
    Vec(Rc<Vec<f64>>),
    TextList(Rc<RefCell<Vec<Rc<str>>>>),
    CList(Rc<RefCell<Vec<(f64, f64)>>>),
    /// An opaque handle (solutions, data tables) owned by the runtime.
    Handle(usize),
    Void,
}

impl Value {
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(x) => *x,
            Value::Bool(b) => f64::from(u8::from(*b)),
            _ => f64::NAN,
        }
    }
    pub fn truth(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(x) => *x != 0.0,
            _ => false,
        }
    }
}

/// Run-time line codes of module code (errors.py, D185): `(k << MODLINE_SHIFT) | line`, k − 1 the text id of the
/// module's file name.
pub const MODLINE_SHIFT: u32 = 20;
pub const MODLINE_MAX: u32 = (1 << MODLINE_SHIFT) - 1;

/// A run-time error in plain physics language (the program line it happened on).
#[derive(Clone, Debug)]
pub struct RunError {
    pub message: String,
    pub line: u32,
    pub hint: Option<String>,
}

/// Printing, implemented by the runtime (the formats are in `module.tables.fmts`).
pub trait Printer {
    fn num(&mut self, fmt: usize, v: f64);
    fn list(&mut self, fmt: usize, v: &[f64]);
    fn vec(&mut self, fmt: usize, v: &[f64]);
    fn mixed_vec(&mut self, fmts: &[usize], v: &[f64]);
    fn mat(&mut self, fmt: usize, v: &[f64], r: usize, c: usize);
    fn complex(&mut self, fmt: usize, re: f64, im: f64);
    fn clist(&mut self, fmt: usize, v: &[(f64, f64)]);
    fn boolean(&mut self, b: bool);
    fn text(&mut self, s: &str);
    fn textlist(&mut self, v: &[Rc<str>]);
    fn end(&mut self);
}

// ---------------------------------------------------------------- IEEE-style arithmetic (interp.py helpers)

/// a / b with Python's semantics made IEEE: x/0 is ±∞, 0/0 and NaN/0 are NaN.
pub fn fdiv(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        if a == 0.0 || a.is_nan() {
            return f64::NAN;
        }
        return f64::INFINITY.copysign(a) * 1f64.copysign(b);
    }
    a / b
}

/// a ** b: overflow is ∞, a complex result (negative base, fractional power) is NaN.
pub fn fpow(a: f64, b: f64) -> f64 {
    if a < 0.0 && b.fract() != 0.0 && b.is_finite() {
        return f64::NAN;
    }
    if a == 0.0 && b < 0.0 {
        return f64::INFINITY;
    }
    a.powf(b)
}

/// The numerator n of p = n/q with q odd (so x^p is real for x < 0), as `odd_root_numerator` does.
/// numerics.odd_root_numerator: for p = n/q with q odd (Fraction(p).limit_denominator(99)), n; else None.
pub fn odd_root_numerator(p: f64) -> Option<i64> {
    if !p.is_finite() || p == p.trunc() {
        return None;
    }
    let (n, q) = fermium_ir::pyfrac::limit_denominator(p, 99)?;
    if q % 2 == 0 || (n as f64 / q as f64 - p).abs() > 1e-12 * 1f64.max(p.abs()) {
        return None;
    }
    i64::try_from(n).ok()
}

/// x ** p for a compile-time constant p, exactly as `interp.powc` (and the compiled code) does it.
pub fn powc(x: f64, p: f64) -> f64 {
    if p == 2.0 {
        return x * x;
    }
    if p == 3.0 {
        return x * x * x;
    }
    if p == 1.0 {
        return x;
    }
    if p == 0.5 {
        return if x >= 0.0 { x.sqrt() } else if x.is_nan() { x } else { f64::NAN };
    }
    if p == -1.0 {
        return fdiv(1.0, x);
    }
    if p == -2.0 {
        return fdiv(1.0, x * x);
    }
    if p == 4.0 {
        let x2 = x * x;
        return x2 * x2;
    }
    if p == -0.5 {
        return if x >= 0.0 { fdiv(1.0, x.sqrt()) } else { f64::NAN };
    }
    if p == 1.5 {
        return if x >= 0.0 { x * x.sqrt() } else { f64::NAN };
    }
    if p == -1.5 {
        return if x >= 0.0 { fdiv(1.0, x * x.sqrt()) } else { f64::NAN };
    }
    if (p - 1.0 / 3.0).abs() < 1e-15 {
        return crate::eval_core::cmath::cbrt(x);
    }
    if let Some(n) = odd_root_numerator(p) {
        // x^(n/q), q odd: the real root, also for x < 0 (like cbrt)
        let r = x.abs().powf(p);
        return if n % 2 != 0 { r.copysign(x) } else { r };
    }
    x.powf(p)
}

/// The one-argument math functions (interp.math1).
pub fn math1(name: &str, x: f64) -> f64 {
    match name {
        // llvm.round, as the compiled v1.5 path (the oracle) does it: half away from zero, exact for 0.49999999999999994
        "round" => x.round(),
        "sign" => f64::from(i8::from(x > 0.0) - i8::from(x < 0.0)),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "asinh" => x.asinh(),
        "acosh" => x.acosh(),
        "atanh" => x.atanh(),
        "exp" => x.exp(),
        "ln" | "log" => x.ln(),
        "log10" => x.log10(),
        "log2" => x.log2(),
        "expm1" => x.exp_m1(),
        "log1p" => x.ln_1p(),
        "abs" => x.abs(),
        "cot" => fdiv(1.0, x.tan()),
        "sec" => fdiv(1.0, x.cos()),
        "csc" => fdiv(1.0, x.sin()),
        _ => f64::NAN,
    }
}

/// The one-argument functions `math1` computes itself (erf, gamma, … come from fermium-runtime).
pub const MATH1: &[&str] = &[
    "round", "sign", "floor", "ceil", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh",
    "acosh", "atanh", "exp", "ln", "log", "log10", "log2", "expm1", "log1p", "abs", "cot", "sec", "csc",
];

// ---------------------------------------------------------------- control flow
pub(crate) enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

/// Local variables of one call.
#[derive(Default)]
pub struct Frame {
    pub(crate) vars: HashMap<SymId, Value>,
}

thread_local! {
    /// The stack address where the program started (Interpreter::run), for the recursion check.
    static STACK_BASE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The bytes of stack a program may use before runaway recursion stops it. The default suits a main thread's
/// 8 MB stack; a driver that runs the program on a thread with a big stack raises it (v1: 400 MB of 512 MB).
pub static STACK_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(6 << 20);

/// Runs a checked module.
pub struct Interpreter<'m, P: Printer> {
    pub module: &'m Module,
    pub printer: P,
    pub(crate) globals: HashMap<SymId, Value>,
    pub(crate) line: u32,
    /// the program line that called into a module's code (errors inside it point there, D185)
    pub(crate) call_line: u32,
    /// Builtins the runtime provides (special functions, numerics), looked up by name.
    pub builtins: HashMap<String, Box<dyn Fn(&[Value]) -> Result<Value, String>>>,
    /// ODE / eigenvalue / PDE solutions and run-time warnings shown (eval_solve.rs)
    pub(crate) solve: crate::eval_solve::SolveState,
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub fn new(module: &'m Module, printer: P) -> Self {
        Interpreter { module, printer, globals: HashMap::new(), line: 0, call_line: 0, builtins: HashMap::new(),
                      solve: Default::default() }
    }

    pub(crate) fn err<T>(&self, message: impl Into<String>) -> Result<T, RunError> {
        Err(RunError { message: message.into(), line: self.line, hint: None })
    }

    pub fn run(&mut self) -> Result<(), RunError> {
        let here = 0u8;
        STACK_BASE.with(|b| b.set(&here as *const u8 as usize));
        let mut fr = Frame::default();
        let main = &self.module.main;
        let r = self.block(main, &mut fr).map_err(|e| self.locate(e));
        match r? {
            Flow::Normal | Flow::Return(_) => Ok(()),
            Flow::Break | Flow::Continue => Ok(()),
        }
    }

    pub(crate) fn get(&self, sym: SymId, fr: &Frame) -> Result<Value, RunError> {
        if let Some(v) = fr.vars.get(&sym) {
            return Ok(v.clone());
        }
        if let Some(v) = self.globals.get(&sym) {
            return Ok(v.clone());
        }
        let name = &self.module.syms[sym].name;
        self.err(format!("{name} is used before it has a value"))
    }

    pub(crate) fn set(&mut self, sym: SymId, v: Value, fr: &mut Frame) {
        if self.module.syms[sym].func.is_none() {
            self.globals.insert(sym, v);
        } else {
            fr.vars.insert(sym, v);
        }
    }

    pub(crate) fn block(&mut self, stmts: &[Stmt], fr: &mut Frame) -> Result<Flow, RunError> {
        for s in stmts {
            match self.stmt(s, fr)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    pub(crate) fn stmt(&mut self, s: &Stmt, fr: &mut Frame) -> Result<Flow, RunError> {
        if s.line != 0 {
            self.line = s.line;
        }
        match &s.kind {
            StmtKind::Assign(sym, e) => {
                let v = self.eval(e, fr)?;
                self.set(*sym, v, fr);
            }
            StmtKind::Expr(e) => {
                self.eval(e, fr)?;
            }
            StmtKind::IndexAssign(sym, idx, value) => {
                let lst = self.get(*sym, fr)?;
                let Value::List(l) = lst else {
                    return self.err("not yet supported by the Rust back end: setting an element of this value");
                };
                let i = self.eval(idx, fr)?.num();
                let n = l.borrow().len();
                let k = self.elem_index(i, n)?;
                let v = self.eval(value, fr)?.num();
                l.borrow_mut()[k] = v;
            }
            StmtKind::Push(sym, e) => {
                let v = self.eval(e, fr)?;
                match (self.get(*sym, fr)?, v) {
                    (Value::List(l), Value::Num(x)) => l.borrow_mut().push(x),
                    (Value::List(l), Value::List(ys)) => {
                        let ys = ys.borrow().clone();
                        l.borrow_mut().extend(ys)
                    }
                    (Value::TextList(l), Value::Str(t)) => l.borrow_mut().push(t),
                    (Value::CList(l), Value::Vec(z)) if z.len() == 2 => l.borrow_mut().push((z[0], z[1])),
                    _ => return self.err("not yet supported by the Rust back end: pushing this value"),
                }
            }
            StmtKind::Clear(sym) => {
                match self.get(*sym, fr)? {
                    Value::List(l) => l.borrow_mut().clear(),
                    Value::TextList(l) => l.borrow_mut().clear(),
                    Value::CList(l) => l.borrow_mut().clear(),
                    _ => return self.err("not yet supported by the Rust back end: clearing this value"),
                }
            }
            StmtKind::If(c, then, other) => {
                let branch = if self.eval(c, fr)?.truth() { then } else { other };
                return self.block(branch, fr);
            }
            StmtKind::While(c, body) => {
                while self.eval(c, fr)?.truth() {
                    match self.block(body, fr)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::For { sym, lo, hi, step, body, par, .. } => {
                // inclusive, computed as lo + i·st (so rounding never adds or drops the last value)
                let lo = self.eval(lo, fr)?.num();
                let hi = self.eval(hi, fr)?.num();
                let st = match step {
                    Some(e) => self.eval(e, fr)?.num(),
                    None => 1.0,
                };
                let n = self.for_count(lo, hi, st)?;
                if let Some(info) = par {
                    return self.parallel_for(info, *sym, lo, st, n.max(0) as usize, body, fr);
                }
                for i in 0..n {
                    self.set(*sym, Value::Num(lo + i as f64 * st), fr);
                    match self.block(body, fr)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::ForIn(sym, lst, body) => {
                let items: Vec<Value> = match self.eval(lst, fr)? {
                    Value::List(l) => l.borrow().iter().map(|x| Value::Num(*x)).collect(),
                    Value::TextList(l) => l.borrow().iter().map(|t| Value::Str(t.clone())).collect(),
                    Value::Vec(v) => v.iter().map(|x| Value::Num(*x)).collect(),
                    // for z in fft(xs): each z a complex number (D243)
                    Value::CList(l) => l.borrow().iter().map(|z| Value::Vec(Rc::new(vec![z.0, z.1]))).collect(),
                    _ => vec![],
                };
                for v in items {
                    self.set(*sym, v, fr);
                    match self.block(body, fr)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::Print(items) => self.print(items, fr)?,
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, fr)?,
                    None => Value::Void,
                };
                return Ok(Flow::Return(v));
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Assert(c, msg) => {
                if !self.eval(c, fr)?.truth() {
                    let m = self.module.tables.texts.get(*msg).cloned().unwrap_or_default();
                    return self.err(m);
                }
            }
            StmtKind::Plot(..) | StmtKind::Solve { .. } | StmtKind::Fit { .. } | StmtKind::Animate { .. } => {
                self.stmt_solve(s, fr)?
            }
            StmtKind::Propagate { .. } => self.stmt_propagate(s, fr)?,
        }
        Ok(Flow::Normal)
    }

    pub(crate) fn print(&mut self, items: &[PrintItem], fr: &mut Frame) -> Result<(), RunError> {
        for it in items {
            match it {
                PrintItem::Num(e, f) => {
                    let v = self.eval(e, fr)?.num();
                    self.printer.num(*f, v)
                }
                PrintItem::List(e, f) => {
                    if let Value::List(l) = self.eval(e, fr)? {
                        let v = l.borrow().clone();
                        self.printer.list(*f, &v)
                    }
                }
                PrintItem::Vec(e, f) => {
                    if let Value::Vec(v) = self.eval(e, fr)? {
                        self.printer.vec(*f, &v)
                    }
                }
                PrintItem::MixedVec(e, fs) => {
                    if let Value::Vec(v) = self.eval(e, fr)? {
                        self.printer.mixed_vec(fs, &v)
                    }
                }
                PrintItem::Mat(e, f) => {
                    let (r, c) = match &e.ty {
                        Ty::Mat { r, c, .. } => (*r, *c),
                        _ => (0, 0),
                    };
                    if let Value::Vec(v) = self.eval(e, fr)? {
                        self.printer.mat(*f, &v, r, c)
                    }
                }
                PrintItem::Complex(e, f) => {
                    if let Value::Vec(v) = self.eval(e, fr)? {
                        self.printer.complex(*f, v[0], v[1])
                    }
                }
                PrintItem::ComplexList(e, f) => {
                    if let Value::CList(l) = self.eval(e, fr)? {
                        let v = l.borrow().clone();
                        self.printer.clist(*f, &v)
                    }
                }
                PrintItem::Bool(e) => {
                    let b = self.eval(e, fr)?.truth();
                    self.printer.boolean(b)
                }
                PrintItem::Text(i) | PrintItem::Data(_, i) => {
                    let t = self.module.tables.texts[*i].clone();
                    self.printer.text(&t)
                }
                PrintItem::TextVar(e) => {
                    if let Value::Str(s) = self.eval(e, fr)? {
                        self.printer.text(&s)
                    }
                }
                PrintItem::TextList(e) => {
                    if let Value::TextList(l) = self.eval(e, fr)? {
                        let v = l.borrow().clone();
                        self.printer.textlist(&v)
                    }
                }
            }
        }
        self.printer.end();
        Ok(())
    }

    /// The number of iterations of `for … from lo to hi step st` (s_SFor / for_count of the compiled path): a zero
    /// or NaN step and a NaN count are errors; a range to ∞ runs 2⁶² times (until a break).
    pub(crate) fn for_count(&self, lo: f64, hi: f64, st: f64) -> Result<i64, RunError> {
        if st == 0.0 || st.is_nan() {
            return self.err("the step must be a non-zero number that goes from the start towards the end");
        }
        let mut cnt = ((hi - lo) / st + 1e-9).floor() + 1.0;
        if cnt < 0.0 {
            cnt = 0.0;
        }
        if cnt.is_nan() {
            return self.err(format!("this for loop has no definite number of steps: it goes from {} to {} (NaN in \
                                     the start, end or step)", fermium_units::numfmt::format_number6(lo),
                                    fermium_units::numfmt::format_number6(hi)));
        }
        if cnt > 2f64.powi(62) {
            cnt = 2f64.powi(62);
        }
        Ok(cnt as i64)
    }

    pub fn eval(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        if e.line != 0 {
            self.line = e.line;
        }
        Ok(match &e.kind {
            ExprKind::Const(x) => Value::Num(*x),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Str(s) => Value::Str(s.as_str().into()),
            ExprKind::Var(sym) => self.get(*sym, fr)?,
            ExprKind::Bin(op, a, b) => {
                let (va, vb) = (self.eval(a, fr)?, self.eval(b, fr)?);
                self.bin(*op, va, vb)?
            }
            ExprKind::PowC(a, p) => match self.eval(a, fr)? {
                Value::Num(x) => Value::Num(powc(x, *p)),
                Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| powc(*x, *p)).collect()))),
                _ => Value::Num(f64::NAN),
            },
            ExprKind::Pow(a, b) => {
                // llvm.pow, element by element for a list base (e_IPow)
                let (va, y) = (self.eval(a, fr)?, self.eval(b, fr)?.num());
                match va {
                    Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| x.powf(y)).collect()))),
                    Value::Num(x) => Value::Num(x.powf(y)),
                    _ => return self.err("not yet supported by the Rust back end: this power"),
                }
            }
            ExprKind::Neg(a) => match self.eval(a, fr)? {
                Value::Num(x) => Value::Num(-x),
                Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| -x).collect()))),
                Value::Vec(v) => Value::Vec(Rc::new(v.iter().map(|x| -x).collect())),
                v => v,
            },
            ExprKind::Cmp(op, a, b) => {
                let (x, y) = (self.eval(a, fr)?.num(), self.eval(b, fr)?.num());
                Value::Bool(match op {
                    CmpOp::Eq => x == y,
                    CmpOp::Ne => x != y,
                    CmpOp::Lt => x < y,
                    CmpOp::Gt => x > y,
                    CmpOp::Le => x <= y,
                    CmpOp::Ge => x >= y,
                })
            }
            ExprKind::Approx { a, b, rtol, atol } => {
                // Julia's isapprox (D260): a == b, or |a−b| ≤ max(atol, rtol·max(|a|,|b|)) and finite
                let (x, y) = (self.eval(a, fr)?.num(), self.eval(b, fr)?.num());
                let at = match atol {
                    Some(t) => self.eval(t, fr)?.num(),
                    None => 0.0,
                };
                let d = (x - y).abs();
                Value::Bool(x == y || (d.is_finite() && d <= at.max(rtol * x.abs().max(y.abs()))))
            }
            ExprKind::Logic { and, a, b } => {
                let x = self.eval(a, fr)?.truth();
                if *and && !x {
                    Value::Bool(false)
                } else if !*and && x {
                    Value::Bool(true)
                } else {
                    Value::Bool(self.eval(b, fr)?.truth())
                }
            }
            ExprKind::Not(a) => Value::Bool(!self.eval(a, fr)?.truth()),
            ExprKind::If(c, a, b) => {
                if self.eval(c, fr)?.truth() {
                    self.eval(a, fr)?
                } else {
                    self.eval(b, fr)?
                }
            }
            ExprKind::Let(binds, value) => {
                for (sym, v) in binds {
                    let x = self.eval(v, fr)?;
                    self.set(*sym, x, fr);
                }
                self.eval(value, fr)?
            }
            ExprKind::Call(f, args) => {
                let vals = args.iter().map(|a| self.eval(a, fr)).collect::<Result<Vec<_>, _>>()?;
                self.call(*f, vals)?
            }
            ExprKind::List(items) if matches!(e.ty, Ty::TextList) => {
                let mut out: Vec<Rc<str>> = Vec::with_capacity(items.len());
                for it in items {
                    match self.eval(it, fr)? {
                        Value::Str(t) => out.push(t),
                        _ => return self.err("not yet supported by the Rust back end: this list of text"),
                    }
                }
                Value::TextList(Rc::new(RefCell::new(out)))
            }
            ExprKind::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    match self.eval(it, fr)? {
                        Value::Num(x) => out.push(x),
                        Value::List(l) => out.extend(l.borrow().iter()),
                        _ => {}
                    }
                }
                Value::List(Rc::new(RefCell::new(out)))
            }
            ExprKind::Vec(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    match self.eval(it, fr)? {
                        Value::Num(x) => out.push(x),
                        Value::Vec(v) => out.extend(v.iter()),
                        _ => {}
                    }
                }
                Value::Vec(Rc::new(out))
            }
            ExprKind::VecElem(v, k) => match self.eval(v, fr)? {
                Value::Vec(v) => Value::Num(v[*k]),
                _ => Value::Num(f64::NAN),
            },
            ExprKind::Index(l, i) => {
                let lst = self.eval(l, fr)?;
                let i = self.eval(i, fr)?.num();
                match lst {
                    Value::List(l) => {
                        let n = l.borrow().len();
                        let k = self.elem_index(i, n)?;
                        Value::Num(l.borrow()[k])
                    }
                    Value::TextList(l) => {
                        let n = l.borrow().len();
                        let k = self.elem_index(i, n)?;
                        Value::Str(l.borrow()[k].clone())
                    }
                    _ => return self.err("not yet supported by the Rust back end: indexing this value"),
                }
            }
            ExprKind::Builtin(name, args) => {
                if let Some(v) = self.sol_extreme(name, args, fr)? {
                    return Ok(v); // max/min of a solution: v1's fm_sol_ext (eval_solve.rs)
                }
                let vals = args.iter().map(|a| self.eval(a, fr)).collect::<Result<Vec<_>, _>>()?;
                self.builtin(name, vals)?
            }
            _ => return self.eval_more(e, fr),
        })
    }

    pub(crate) fn bin(&self, op: BinOp, a: Value, b: Value) -> Result<Value, RunError> {
        let f = |x: f64, y: f64| match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => fdiv(x, y),
        };
        Ok(match (a, b) {
            (Value::Num(x), Value::Num(y)) => Value::Num(f(x, y)),
            (Value::List(l), Value::Num(y)) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| f(*x, y)).collect()))),
            (Value::Num(x), Value::List(l)) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|y| f(x, *y)).collect()))),
            (Value::List(a), Value::List(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                if a.len() != b.len() {
                    return self.err(format!("these two lists have different lengths ({} and {})", a.len(), b.len()));
                }
                Value::List(Rc::new(RefCell::new(a.iter().zip(b.iter()).map(|(x, y)| f(*x, *y)).collect())))
            }
            (Value::Vec(v), Value::Num(y)) => Value::Vec(Rc::new(v.iter().map(|x| f(*x, y)).collect())),
            (Value::Num(x), Value::Vec(v)) => Value::Vec(Rc::new(v.iter().map(|y| f(x, *y)).collect())),
            (Value::Vec(a), Value::Vec(b)) => Value::Vec(Rc::new(a.iter().zip(b.iter()).map(|(x, y)| f(*x, *y)).collect())),
            _ => Value::Num(f64::NAN),
        })
    }

    pub(crate) fn call(&mut self, f: usize, args: Vec<Value>) -> Result<Value, RunError> {
        let func = &self.module.funcs[f];
        // runaway recursion: stop with the compiled path's error before the stack overflows (stack_check)
        let here = 0u8;
        let sp = &here as *const u8 as usize;
        let base = STACK_BASE.with(|b| b.get());
        if base != 0 && base.saturating_sub(sp) > STACK_LIMIT.load(std::sync::atomic::Ordering::Relaxed) {
            let name = if func.display.is_empty() { "a function" } else { func.display.as_str() };
            return Err(RunError { message: format!("{name} called itself too many times (the program ran out of \
                                                    stack) -- is a base case missing, like  if n <= 0 then ...?"),
                                  line: if func.def_line != 0 { func.def_line } else { self.line }, hint: None });
        }
        let mut fr = Frame::default();
        for (p, v) in func.params.iter().zip(args) {
            fr.vars.insert(*p, v);
        }
        let body = &func.body;
        let caller_line = self.line;
        if body.first().is_some_and(|s| s.line > MODLINE_MAX) && 0 < caller_line && caller_line <= MODLINE_MAX {
            self.call_line = caller_line; // calling a module's function from the program (D185)
        }
        let r = match self.block(body, &mut fr)? {
            Flow::Return(v) => Ok(v),
            _ => Ok(Value::Void),
        };
        self.line = caller_line; // later errors in the caller are on its own line
        r
    }

    /// A run-time error's program line: in a module's code, the line that called into it, and the message says
    /// where in the module it happened (errors.py decode_line, D185).
    fn locate(&self, mut e: RunError) -> RunError {
        if e.line > MODLINE_MAX {
            let (k, ml) = ((e.line >> MODLINE_SHIFT) as usize, e.line & MODLINE_MAX);
            let texts = &self.module.tables.texts;
            let name = if 0 < k && k <= texts.len() { texts[k - 1].as_str() } else { "a module" };
            let suf = format!(" (in {name}, line {ml})");
            if !e.message.ends_with(&suf) {
                e.message += &suf;
            }
            e.line = self.call_line;
        }
        e
    }

    pub(crate) fn builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RunError> {
        if let Some(f) = self.builtins.get(name) {
            return f(&args).map_err(|m| RunError { message: m, line: self.line, hint: None });
        }
        for area in [Self::builtin_core, Self::builtin_vecmat, Self::builtin_calculus, Self::builtin_m3,
                     Self::builtin_complex, Self::builtin_data, Self::builtin_uncertain] {
            if let Some(r) = area(self, name, &args) {
                return r;
            }
        }
        let x = args.first().map(Value::num).unwrap_or(f64::NAN);
        Ok(match name {
            "sqrt" => Value::Num(powc(x, 0.5)),
            "max" => Value::Num(args.iter().map(Value::num).fold(f64::NEG_INFINITY, |a, b| if b > a || a.is_nan() { b } else { a })),
            "min" => Value::Num(args.iter().map(Value::num).fold(f64::INFINITY, |a, b| if b < a || a.is_nan() { b } else { a })),
            "len" => match &args[0] {
                Value::List(l) => Value::Num(l.borrow().len() as f64),
                Value::TextList(l) => Value::Num(l.borrow().len() as f64),
                Value::Vec(v) => Value::Num(v.len() as f64),
                _ => Value::Num(0.0),
            },
            "atan2" => Value::Num(x.atan2(args[1].num())),
            "hypot" => Value::Num(x.hypot(args[1].num())),
            _ if MATH1.contains(&name) => Value::Num(math1(name, x)),
            _ => return self.err(format!("the built-in {name} isn't supported by the Rust back end yet")),
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ieee_arithmetic_like_v1() {
        assert!(fdiv(0.0, 0.0).is_nan());
        assert_eq!(fdiv(1.0, 0.0), f64::INFINITY);
        assert_eq!(fdiv(-1.0, 0.0), f64::NEG_INFINITY);
        assert_eq!(fdiv(1.0, -0.0), f64::NEG_INFINITY);
        assert!(fpow(-8.0, 0.5).is_nan());
        assert_eq!(powc(-8.0, 1.0 / 3.0), -2.0);
        // the oracle: interp.powc(-32.0, 0.6) == -7.999999999999999
        assert_eq!(powc(-32.0, 0.6), -7.999999999999999);
        assert_eq!(math1("round", 2.5), 3.0);
        assert_eq!(math1("round", -2.5), -3.0);
        assert_eq!(math1("sign", -0.0), 0.0);
    }
}
