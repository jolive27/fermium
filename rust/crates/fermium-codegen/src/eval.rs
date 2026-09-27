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
    /// A list of vectors or matrices (spec C1, D281), shared like a list.
    VList(Rc<RefCell<Vec<Rc<Vec<f64>>>>>),
    /// An N-dimensional array (D283): its shape and its entries, the last index fastest.
    NdArr(Rc<RefCell<NdArray>>),
    /// An opaque handle (solutions, data tables) owned by the runtime.
    Handle(usize),
    /// An uncertain number, 5.0 ± 0.2 (D120; eval_unc.rs).
    Unc(Rc<fermium_runtime::numerics::uncertain::UFloat>),
    /// A list holding uncertain numbers (and plain ones, as `Num`).
    UList(Rc<RefCell<Vec<Value>>>),
    /// A vector or matrix with uncertain components (v1's tuple of UFloat values): elementwise arithmetic,
    /// components, norm, unit, vdot, cross, det, inverse and printing work (eval_unc.rs; printing: C7).
    UVec(Rc<Vec<Value>>),
    /// All the samples of a number at once, inside `propagate montecarlo` (v1's NumPy arrays, D123).
    Arr(Rc<Vec<f64>>),
    Void,
}

impl Value {
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(x) => *x,
            // comparisons and the like use the nominal value, as v1's UFloat does
            Value::Unc(u) => u.v,
            // an operation that needs one number got all the samples: v1's vectorized Monte Carlo fails there
            Value::Arr(_) => {
                crate::eval_unc::vec_fail();
                f64::NAN
            }
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
    /// A number printed with at most `max_sf` significant figures (an integral at its rounding level, spec B2).
    fn num_capped(&mut self, fmt: usize, v: f64, _max_sf: u32) {
        self.num(fmt, v)
    }
    /// A number printed with exactly `sf` significant figures (a sum by the decimal-place rule, spec B2).
    fn num_sf(&mut self, fmt: usize, v: f64, _sf: u32) {
        self.num(fmt, v)
    }
    fn list(&mut self, fmt: usize, v: &[f64]);
    fn vec(&mut self, fmt: usize, v: &[f64]);
    fn mixed_vec(&mut self, fmts: &[usize], v: &[f64]);
    fn mat(&mut self, fmt: usize, v: &[f64], r: usize, c: usize);
    fn complex(&mut self, fmt: usize, re: f64, im: f64);
    fn clist(&mut self, fmt: usize, v: &[(f64, f64)]);
    /// A list of vectors (cols None; k components each) or of matrices (k entries, cols per row), flattened.
    fn vlist(&mut self, fmt: usize, v: &[f64], k: usize, cols: Option<usize>);
    fn boolean(&mut self, b: bool);
    fn text(&mut self, s: &str);
    fn textlist(&mut self, v: &[Rc<str>]);
    fn end(&mut self);
    /// After a run-time error in the middle of a print: end the line with the items printed so far, as v1 does
    /// (`print "a", xs[5]` prints `a`); nothing when no item was printed.
    fn flush_partial(&mut self) {}
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

/// A fast hasher for symbol ids (small integers): the variables are looked up at every use, and SipHash was a
/// third of an ODE right side's time.
#[derive(Default, Clone, Copy)]
pub struct SymHasher(u64);

impl std::hash::Hasher for SymHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(*b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    fn write_usize(&mut self, i: usize) {
        self.0 = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

/// A map keyed by symbol id.
pub type SymMap<V> = HashMap<SymId, V, std::hash::BuildHasherDefault<SymHasher>>;

/// Local variables of one call.
#[derive(Default)]
pub struct Frame {
    pub(crate) vars: crate::varmap::VarMap,
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
    pub(crate) globals: crate::varmap::GlobalMap,
    pub(crate) line: u32,
    /// the program line that called into a module's code (errors inside it point there, D185)
    pub(crate) call_line: u32,
    /// Builtins the runtime provides (special functions, numerics), looked up by name.
    pub builtins: HashMap<String, Box<dyn Fn(&[Value]) -> Result<Value, String>>>,
    /// ODE / eigenvalue / PDE solutions and run-time warnings shown (eval_solve.rs)
    pub(crate) solve: crate::eval_solve::SolveState,
    /// data sets made by load and table (eval_data.rs)
    pub(crate) data: crate::eval_data::DataState,
    /// pure calls remembered within one ODE right-hand side (eval_memo.rs, D270)
    pub(crate) memo: crate::eval_memo::Memo,
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub fn new(module: &'m Module, printer: P) -> Self {
        Interpreter { module, printer, globals: Default::default(), line: 0, call_line: 0, builtins: HashMap::new(),
                      solve: Default::default(), data: Default::default(), memo: Default::default() }
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
                if let Value::UList(l) = &lst {
                    let i = self.eval(idx, fr)?;
                    let i = self.plain(&i)?;
                    let n = l.borrow().len();
                    let k = self.elem_index(i, n)?;
                    let v = self.eval(value, fr)?;
                    l.borrow_mut()[k] = v;
                    return Ok(Flow::Normal);
                }
                if let Value::VList(l) = &lst {
                    let i = self.eval(idx, fr)?;
                    let i = self.plain(&i)?;
                    let n = l.borrow().len();
                    let k = self.elem_index(i, n)?;
                    if let Value::Vec(v) = self.eval(value, fr)? {
                        l.borrow_mut()[k] = v;
                    }
                    return Ok(Flow::Normal);
                }
                if let Value::NdArr(a) = &lst {
                    // A[i, j, …] = value (D283): the indexes come as a list
                    let ix = match self.eval(idx, fr)? {
                        Value::List(l) => l.borrow().clone(),
                        v => vec![v.num()],
                    };
                    let k = self.arr_offset(&a.borrow().shape, &ix)?;
                    let v = self.eval(value, fr)?;
                    let x = self.plain(&v)?;
                    a.borrow_mut().data[k] = x;
                    return Ok(Flow::Normal);
                }
                if let Value::TextList(l) = &lst {
                    let i = self.eval(idx, fr)?;
                    let i = self.plain(&i)?;
                    let n = l.borrow().len();
                    let k = self.elem_index(i, n)?;
                    if let Value::Str(t) = self.eval(value, fr)? {
                        l.borrow_mut()[k] = t;
                    }
                    return Ok(Flow::Normal);
                }
                if let Value::CList(l) = &lst {
                    let i = self.eval(idx, fr)?;
                    let i = self.plain(&i)?;
                    let n = l.borrow().len();
                    let k = self.elem_index(i, n)?;
                    l.borrow_mut()[k] = match self.eval(value, fr)? {
                        Value::Vec(z) if z.len() == 2 => (z[0], z[1]),
                        v => (v.num(), 0.0),
                    };
                    return Ok(Flow::Normal);
                }
                let Value::List(l) = lst else {
                    return self.err("not yet supported by the Rust back end: setting an element of this value");
                };
                let i = self.eval(idx, fr)?;
                let i = self.plain(&i)?;
                let n = l.borrow().len();
                let k = self.elem_index(i, n)?;
                let v = self.eval(value, fr)?;
                if let Value::Unc(_) = v {
                    // the list now holds an uncertain number (v1's lists hold any number)
                    let mut items: Vec<Value> = l.borrow().iter().map(|x| Value::Num(*x)).collect();
                    items[k] = v;
                    self.set(*sym, Value::UList(Rc::new(RefCell::new(items))), fr);
                    return Ok(Flow::Normal);
                }
                l.borrow_mut()[k] = v.num();
            }
            StmtKind::Push(sym, e) => {
                let v = self.eval(e, fr)?;
                let cur = self.get(*sym, fr)?;
                // `ps = []` then push(ps, <1, 2> m): the empty list becomes a list of the pushed kind (D281)
                let cur = match (&cur, &self.module.syms[*sym].ty) {
                    (Value::List(l), ty @ (Ty::VList(_) | Ty::ComplexList(_) | Ty::TextList)) if l.borrow().is_empty() => {
                        let nv = empty_of(ty);
                        self.set(*sym, nv.clone(), fr);
                        nv
                    }
                    _ => cur,
                };
                match (cur, v) {
                    (Value::VList(l), Value::Vec(v)) => l.borrow_mut().push(v),
                    (Value::CList(l), Value::Num(x)) => l.borrow_mut().push((x, 0.0)),
                    (Value::List(l), Value::Num(x)) => l.borrow_mut().push(x),
                    (Value::List(l), Value::List(ys)) => {
                        let ys = ys.borrow().clone();
                        l.borrow_mut().extend(ys)
                    }
                    (Value::TextList(l), Value::Str(t)) => l.borrow_mut().push(t),
                    (Value::CList(l), Value::Vec(z)) if z.len() == 2 => l.borrow_mut().push((z[0], z[1])),
                    (Value::UList(l), v @ (Value::Num(_) | Value::Unc(_))) => l.borrow_mut().push(v),
                    (Value::List(l), v @ Value::Unc(_)) => {
                        let mut items: Vec<Value> = l.borrow().iter().map(|x| Value::Num(*x)).collect();
                        items.push(v);
                        self.set(*sym, Value::UList(Rc::new(RefCell::new(items))), fr);
                    }
                    _ => return self.err("not yet supported by the Rust back end: pushing this value"),
                }
            }
            StmtKind::Clear(sym) => {
                match self.get(*sym, fr)? {
                    Value::List(l) => l.borrow_mut().clear(),
                    Value::UList(l) => l.borrow_mut().clear(),
                    Value::TextList(l) => l.borrow_mut().clear(),
                    Value::CList(l) => l.borrow_mut().clear(),
                    Value::VList(l) => l.borrow_mut().clear(),
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
                let lo = self.eval(lo, fr)?;
                let lo = self.plain(&lo)?;
                let hi = self.eval(hi, fr)?;
                let hi = self.plain(&hi)?;
                let st = match step {
                    Some(e) => {
                        let v = self.eval(e, fr)?;
                        self.plain(&v)?
                    }
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
                    Value::UList(l) => l.borrow().clone(),
                    Value::TextList(l) => l.borrow().iter().map(|t| Value::Str(t.clone())).collect(),
                    Value::Vec(v) => v.iter().map(|x| Value::Num(*x)).collect(),
                    // for z in fft(xs): each z a complex number (D243)
                    Value::CList(l) => l.borrow().iter().map(|z| Value::Vec(Rc::new(vec![z.0, z.1]))).collect(),
                    Value::VList(l) => l.borrow().iter().map(|v| Value::Vec(v.clone())).collect(),
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
                // v1 runs a program that uses ± in its interpreter, whose print_num has no decimal-place rule
                PrintItem::Num(e, f) if !self.module.uses_uncertainty && crate::eval_calc::measured_sum(e) => {
                    let (x, sf) = self.sum_sf(e, *f, fr)?;
                    match sf {
                        Some(n) => self.printer.num_sf(*f, x, n),
                        None => self.printer.num(*f, x),
                    }
                }
                PrintItem::Num(e, f) => {
                    crate::eval_calc::take_quad_sf();
                    let v = self.eval(e, fr)?;
                    if !self.unc_print(&v, *f) {
                        let x = v.num();
                        match crate::eval_calc::take_quad_sf() {
                            Some(n) if crate::eval_calc::integral_shaped(e) => self.printer.num_capped(*f, x, n),
                            _ => self.printer.num(*f, x),
                        }
                    }
                }
                PrintItem::List(e, f) => {
                    let v = self.eval(e, fr)?;
                    if self.unc_print(&v, *f) {
                        continue;
                    }
                    match v {
                        Value::List(l) => {
                            let v = l.borrow().clone();
                            self.printer.list(*f, &v)
                        }
                        // a variable printed as [] before a push made it another kind of list (D281)
                        Value::TextList(l) => {
                            let v = l.borrow().clone();
                            self.printer.textlist(&v)
                        }
                        Value::VList(l) if l.borrow().is_empty() => self.printer.list(*f, &[]),
                        Value::CList(l) if l.borrow().is_empty() => self.printer.list(*f, &[]),
                        _ => {}
                    }
                }
                PrintItem::Vec(e, f) => match self.eval(e, fr)? {
                    Value::Vec(v) => self.printer.vec(*f, &v),
                    Value::UVec(v) => self.unc_print_vec(&v, &[*f], None),
                    _ => {}
                },
                PrintItem::MixedVec(e, fs) => match self.eval(e, fr)? {
                    Value::Vec(v) => self.printer.mixed_vec(fs, &v),
                    Value::UVec(v) => self.unc_print_vec(&v, fs, None),
                    _ => {}
                },
                PrintItem::Mat(e, f) => {
                    let (r, c) = match &e.ty {
                        Ty::Mat { r, c, .. } => (*r, *c),
                        _ => (0, 0),
                    };
                    match self.eval(e, fr)? {
                        Value::Vec(v) => self.printer.mat(*f, &v, r, c),
                        Value::UVec(v) => self.unc_print_vec(&v, &[*f], Some((r, c))),
                        _ => {}
                    }
                }
                PrintItem::Complex(e, f) => match self.eval(e, fr)? {
                    Value::Vec(v) => self.printer.complex(*f, v[0], v[1]),
                    // format_complex takes math.hypot of the parts
                    Value::UVec(_) => return self.err(crate::eval_unc::GENERIC),
                    _ => {}
                },
                PrintItem::Array(e, f) => {
                    if let Value::NdArr(a) = self.eval(e, fr)? {
                        let s = crate::printer::format_array(self.module, *f, &a.borrow());
                        self.printer.text(&s)
                    }
                }
                PrintItem::VList(e, f) => {
                    let (k, cols) = match &e.ty {
                        Ty::VList(el) => match &**el {
                            Ty::Vec { n, .. } => (*n, None),
                            Ty::Mat { r, c, .. } => (r * c, Some(*c)),
                            _ => (1, None),
                        },
                        _ => (1, None),
                    };
                    let flat: Vec<f64> = match self.eval(e, fr)? {
                        Value::VList(l) => l.borrow().iter().flat_map(|v| v.iter().copied()).collect(),
                        _ => vec![],
                    };
                    self.printer.vlist(*f, &flat, k, cols)
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
            ExprKind::Var(sym) => {
                let v = self.get(*sym, fr)?;
                if crate::eval_unc::mc_active() {
                    crate::eval_unc::mc_sample(&v)
                } else {
                    if crate::eval_unc::is_unc(&v) {
                        crate::eval_unc_kern::kernel_seen(&v); // the sources a kernel reads (D300)
                    }
                    v
                }
            }
            ExprKind::Bin(op, a, b) => {
                let (va, vb) = (self.eval(a, fr)?, self.eval(b, fr)?);
                // (a complex number is a pair too: cplx.py's (re, im) tuple)
                if matches!(e.ty, Ty::Vec { .. } | Ty::Mat { .. } | Ty::Complex(_))
                    && (crate::eval_unc::is_unc(&va) || crate::eval_unc::is_unc(&vb))
                {
                    return self.unc_vec_bin(*op, va, vb);
                }
                self.bin(*op, va, vb)?
            }
            ExprKind::PowC(a, p) => match self.eval(a, fr)? {
                v @ (Value::Unc(_) | Value::UList(_) | Value::Arr(_)) => self.unc_powc(v, *p)?,
                // v1 runs a program with uncertainties in its interpreter: interp.powc there (D120)
                Value::Num(x) if self.module.uses_uncertainty => Value::Num(crate::eval_unc::interp_powc(x, *p)),
                Value::Num(x) => Value::Num(powc(x, *p)),
                Value::List(l) if self.module.uses_uncertainty => Value::List(Rc::new(RefCell::new(
                    l.borrow().iter().map(|x| crate::eval_unc::interp_powc(*x, *p)).collect()))),
                Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| powc(*x, *p)).collect()))),
                _ => Value::Num(f64::NAN),
            },
            ExprKind::Pow(a, b) => {
                // llvm.pow, element by element for a list base (e_IPow)
                let (va, vb) = (self.eval(a, fr)?, self.eval(b, fr)?);
                if crate::eval_unc::is_unc(&va) || crate::eval_unc::is_unc(&vb) {
                    return self.unc_pow(va, vb);
                }
                let y = vb.num();
                match va {
                    Value::List(l) if self.module.uses_uncertainty => Value::List(Rc::new(RefCell::new(
                        l.borrow().iter().map(|x| fermium_runtime::numerics::uncertain::pow(*x, y)).collect()))),
                    Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| x.powf(y)).collect()))),
                    // interp.fpow in a program with uncertainties (v1 runs those in its interpreter)
                    Value::Num(x) if self.module.uses_uncertainty => Value::Num(fermium_runtime::numerics::uncertain::pow(x, y)),
                    Value::Num(x) => Value::Num(x.powf(y)),
                    _ => return self.err("not yet supported by the Rust back end: this power"),
                }
            }
            ExprKind::Neg(a) => match self.eval(a, fr)? {
                v @ (Value::Unc(_) | Value::UList(_) | Value::Arr(_) | Value::UVec(_)) => self.unc_neg(v)?,
                Value::Num(x) => Value::Num(-x),
                Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().iter().map(|x| -x).collect()))),
                Value::Vec(v) => Value::Vec(Rc::new(v.iter().map(|x| -x).collect())),
                Value::NdArr(a) => {
                    let a = a.borrow();
                    Value::NdArr(Rc::new(RefCell::new(NdArray { shape: a.shape.clone(), data: a.data.iter().map(|x| -x).collect() })))
                }
                Value::VList(l) => Value::VList(Rc::new(RefCell::new(l.borrow().iter()
                    .map(|v| Rc::new(v.iter().map(|x| -x).collect())).collect()))),
                v => v,
            },
            ExprKind::Cmp(op, a, b) => {
                let (va, vb) = (self.eval(a, fr)?, self.eval(b, fr)?);
                if crate::eval_unc::is_unc(&va) || crate::eval_unc::is_unc(&vb) {
                    return self.unc_cmp(*op, &va, &vb);
                }
                let (x, y) = (va.num(), vb.num());
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
            ExprKind::Call(f, args) => self.call_expr(*f, args, fr)?,
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
            ExprKind::List(items) if matches!(e.ty, Ty::VList(_)) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    match self.eval(it, fr)? {
                        Value::Vec(v) => out.push(v),
                        _ => return self.err("not yet supported by the Rust back end: this list of vectors"),
                    }
                }
                Value::VList(Rc::new(RefCell::new(out)))
            }
            ExprKind::List(items) if matches!(e.ty, Ty::ComplexList(_)) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    match self.eval(it, fr)? {
                        Value::Vec(z) if z.len() == 2 => out.push((z[0], z[1])),
                        Value::Num(x) => out.push((x, 0.0)),
                        _ => return self.err("not yet supported by the Rust back end: this list of complex numbers"),
                    }
                }
                Value::CList(Rc::new(RefCell::new(out)))
            }
            ExprKind::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                let mut uout: Option<Vec<Value>> = None;
                for it in items {
                    let v = self.eval(it, fr)?;
                    if uout.is_none() && crate::eval_unc::is_unc(&v) {
                        uout = Some(out.iter().map(|x| Value::Num(*x)).collect());
                    }
                    if let Some(u) = uout.as_mut() {
                        match v {
                            Value::UList(l) => u.extend(l.borrow().iter().cloned()),
                            Value::List(l) => u.extend(l.borrow().iter().map(|x| Value::Num(*x))),
                            v @ (Value::Num(_) | Value::Unc(_)) => u.push(v),
                            _ => return self.err("not yet supported by the Rust back end: this list"),
                        }
                        continue;
                    }
                    match v {
                        Value::Num(x) => out.push(x),
                        Value::List(l) => out.extend(l.borrow().iter()),
                        _ => return self.err("not yet supported by the Rust back end: this list"),
                    }
                }
                match uout {
                    Some(u) => Value::UList(Rc::new(RefCell::new(u))),
                    None => Value::List(Rc::new(RefCell::new(out))),
                }
            }
            ExprKind::Vec(items) => {
                let mut out = Vec::with_capacity(items.len());
                let mut uout: Option<Vec<Value>> = None;
                for it in items {
                    let v = self.eval(it, fr)?;
                    // a vector or matrix with uncertain components (Fermium 2.5, C7: v1 stopped here)
                    if uout.is_none() && matches!(v, Value::Unc(_) | Value::UVec(_)) {
                        uout = Some(out.iter().map(|x| Value::Num(*x)).collect());
                    }
                    if let Some(u) = uout.as_mut() {
                        match v {
                            Value::Num(_) | Value::Unc(_) => u.push(v),
                            Value::Vec(w) => u.extend(w.iter().map(|x| Value::Num(*x))),
                            Value::UVec(w) => u.extend(w.iter().cloned()),
                            _ => return self.err("not yet supported by the Rust back end: this vector"),
                        }
                        continue;
                    }
                    match v {
                        Value::Num(x) => out.push(x),
                        Value::Vec(v) => out.extend(v.iter()),
                        _ => return self.err("not yet supported by the Rust back end: this vector"),
                    }
                }
                match uout {
                    Some(u) => crate::eval_unc::make_vec(u),
                    None => Value::Vec(Rc::new(out)),
                }
            }
            ExprKind::VecElem(v, k) => match self.eval(v, fr)? {
                Value::Vec(v) => Value::Num(v[*k]),
                Value::UVec(v) => v[*k].clone(),
                _ => Value::Num(f64::NAN),
            },
            ExprKind::Index(l, i) => {
                let lst = self.eval(l, fr)?;
                let i = self.eval(i, fr)?;
                let i = self.plain(&i)?;
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
                    Value::UList(l) => {
                        let n = l.borrow().len();
                        let k = self.elem_index(i, n)?;
                        l.borrow()[k].clone()
                    }
                    Value::VList(l) => {
                        let n = l.borrow().len();
                        let k = self.elem_index(i, n)?;
                        Value::Vec(l.borrow()[k].clone())
                    }
                    _ => return self.err("not yet supported by the Rust back end: indexing this value"),
                }
            }
            ExprKind::Builtin(name, args) => {
                // a math function of one plain number, len of a list, rand(): straight there (the same results as
                // through builtin_slice)
                use crate::eval_core::Fast;
                match (crate::eval_core::fast_builtin(name), args.len()) {
                    (Some(Fast::Math(f)), 1) => {
                        let v = self.eval(&args[0], fr)?;
                        return match v {
                            // interp's round in a program with uncertainties (v1 runs those in its interpreter)
                            Value::Num(x) if self.module.uses_uncertainty && name == "round" => {
                                Ok(Value::Num(crate::eval_unc::interp_math1(name, x)))
                            }
                            Value::Num(x) => Ok(Value::Num(f(x))),
                            v => self.builtin_slice(name, std::slice::from_ref(&v)),
                        };
                    }
                    (Some(Fast::Len), 1) => {
                        let v = self.eval(&args[0], fr)?;
                        return match &v {
                            Value::List(l) => Ok(Value::Num(l.borrow().len() as f64)),
                            _ => self.builtin_slice(name, std::slice::from_ref(&v)),
                        };
                    }
                    (Some(Fast::Rand), 0) if self.builtins.is_empty() => {
                        if let Some(r) = self.builtin_m3("rand", &[]) {
                            return r;
                        }
                    }
                    _ => {}
                }
                if let Some(v) = self.sol_extreme(name, args, fr)? {
                    return Ok(v); // max/min of a solution: v1's fm_sol_ext (eval_solve.rs)
                }
                let vals = self.eval_args(args, fr)?;
                if crate::eval_unc::mc_active() && (name == "pm" || name == "pm_rel") {
                    if crate::eval_unc_kern::kernel_pm_active() {
                        // a kernel's ±1σ test or Monte Carlo: the ± written inside it is the measurement its
                        // linear pass made (one source), sampled like any other input (red team 14 #1)
                        if let Some(v) = crate::eval_unc_kern::kernel_pm_get(e as *const Expr as usize) {
                            return Ok(crate::eval_unc::mc_sample(&v));
                        }
                    }
                    return self.mc_pm(e as *const Expr as usize, name == "pm_rel", &vals);
                }
                if (name == "pm" || name == "pm_rel") && crate::eval_unc_kern::kernel_pm_active() {
                    // a ± written inside an integrand or a right side: one measurement per kernel (C7)
                    let key = e as *const Expr as usize;
                    if let Some(v) = crate::eval_unc_kern::kernel_pm_get(key) {
                        crate::eval_unc_kern::kernel_seen(&v);
                        return Ok(v);
                    }
                    let v = self.builtin_slice(name, &vals)?;
                    crate::eval_unc_kern::kernel_pm_put(key, v.clone());
                    crate::eval_unc_kern::kernel_seen(&v);
                    return Ok(v);
                }
                let r = self.builtin_slice(name, &vals);
                crate::varmap::give_args(vals);
                r?
            }
            _ => return self.eval_more(e, fr),
        })
    }

    pub(crate) fn bin(&self, op: BinOp, a: Value, b: Value) -> Result<Value, RunError> {
        if let (Value::Num(x), Value::Num(y)) = (&a, &b) {
            // the common case first
            let (x, y) = (*x, *y);
            return Ok(Value::Num(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => fdiv(x, y),
            }));
        }
        if crate::eval_unc::is_unc(&a) || crate::eval_unc::is_unc(&b) {
            return self.unc_bin(op, a, b);
        }
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
            (Value::NdArr(a), Value::NdArr(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                if a.shape != b.shape {
                    return self.err(format!("these two arrays have different shapes ({} and {})", shape_text(&a.shape),
                                            shape_text(&b.shape)));
                }
                arr_new(a.shape.clone(), a.data.iter().zip(b.data.iter()).map(|(x, y)| f(*x, *y)).collect())
            }
            (Value::NdArr(a), Value::Num(y)) => {
                let a = a.borrow();
                arr_new(a.shape.clone(), a.data.iter().map(|x| f(*x, y)).collect())
            }
            (Value::Num(x), Value::NdArr(a)) => {
                let a = a.borrow();
                arr_new(a.shape.clone(), a.data.iter().map(|y| f(x, *y)).collect())
            }
            // a list of vectors or matrices times or over a number (D281)
            (Value::VList(l), Value::Num(y)) => Value::VList(Rc::new(RefCell::new(l.borrow().iter()
                .map(|v| Rc::new(v.iter().map(|x| f(*x, y)).collect())).collect()))),
            (Value::Num(x), Value::VList(l)) => Value::VList(Rc::new(RefCell::new(l.borrow().iter()
                .map(|v| Rc::new(v.iter().map(|y| f(x, *y)).collect())).collect()))),
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
        // room for the parameters and locals up front (no regrowing in a hot call), from the pool of frames
        let mut fr = Frame { vars: crate::varmap::take_frame(func.params.len() + func.locals.len()) };
        let mut args = args;
        for (p, v) in func.params.iter().zip(args.drain(..)) {
            fr.vars.insert(*p, v);
        }
        crate::varmap::give_args(args);
        let body = &func.body;
        let caller_line = self.line;
        if func_in_module(func) && 0 < caller_line && caller_line <= MODLINE_MAX {
            self.call_line = caller_line; // calling a module's function from the program (D185)
        }
        let r = match self.block(body, &mut fr)? {
            Flow::Return(v) => Ok(v),
            _ => Ok(Value::Void),
        };
        crate::varmap::give_frame(std::mem::take(&mut fr.vars));
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

    /// The values of a call's arguments, in order.
    #[inline]
    pub(crate) fn eval_args(&mut self, args: &[Expr], fr: &mut Frame) -> Result<Vec<Value>, RunError> {
        let mut vals = crate::varmap::take_args(args.len());
        for a in args {
            vals.push(self.eval(a, fr)?);
        }
        Ok(vals)
    }

    pub(crate) fn builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RunError> {
        self.builtin_slice(name, &args)
    }

    pub(crate) fn builtin_slice(&mut self, name: &str, args: &[Value]) -> Result<Value, RunError> {
        if name == "pycall" {
            return self.pycall(args); // a Python function (D140, eval_py.rs)
        }
        if name == "ccall" {
            return self.ccall(args); // a C or Fortran function (D275, eval_c.rs)
        }
        if !self.builtins.is_empty() {
            if let Some(f) = self.builtins.get(name) {
                return f(args).map_err(|m| RunError { message: m, line: self.line, hint: None });
            }
        }
        if !name.starts_with("pm") && !name.starts_with("unc_") && !name.starts_with("c.")
            && args.iter().any(crate::eval_unc::is_unc)
        {
            return self.unc_apply(name, args);
        }
        if name == "__sol_list" && args.len() == 6 {
            return self.sol_list_at(args);
        }
        if let Some(op) = name.strip_prefix("arr.") {
            return self.array_builtin(op, args);
        }
        for area in [Self::builtin_core, Self::builtin_vecmat, Self::builtin_calculus, Self::builtin_m3,
                     Self::builtin_complex, Self::builtin_data, Self::builtin_uncertain] {
            if let Some(r) = area(self, name, args) {
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
                Value::VList(l) => Value::Num(l.borrow().len() as f64),
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

/// Whether a function instance is a module's: its first line is a module line code (D185). A one-line function
/// `f(x) = ...` is a return statement without a line of its own, so its expression's line counts.
pub fn func_in_module(func: &fermium_ir::Func) -> bool {
    match func.body.first() {
        Some(s) if s.line != 0 => s.line > MODLINE_MAX,
        Some(Stmt { kind: StmtKind::Return(Some(e)), .. }) => e.line > MODLINE_MAX,
        _ => false,
    }
}

/// The empty value of a list type (a list variable set to [] and then pushed a vector, a complex number or text).
pub fn empty_of(ty: &Ty) -> Value {
    match ty {
        Ty::VList(_) => Value::VList(Rc::new(RefCell::new(vec![]))),
        Ty::ComplexList(_) => Value::CList(Rc::new(RefCell::new(vec![]))),
        Ty::TextList => Value::TextList(Rc::new(RefCell::new(vec![]))),
        _ => Value::List(Rc::new(RefCell::new(vec![]))),
    }
}

/// An N-dimensional array's shape and entries (row-major: the last index runs fastest; D283).
#[derive(Clone, Debug, PartialEq)]
pub struct NdArray {
    pub shape: Vec<usize>,
    pub data: Vec<f64>,
}

pub fn arr_new(shape: Vec<usize>, data: Vec<f64>) -> Value {
    Value::NdArr(Rc::new(RefCell::new(NdArray { shape, data })))
}

/// 50×50
pub fn shape_text(shape: &[usize]) -> String {
    shape.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("×")
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// The position of entry [i, j, …] (1-based, checked against the shape).
    pub(crate) fn arr_offset(&self, shape: &[usize], ix: &[f64]) -> Result<usize, RunError> {
        let mut k = 0usize;
        for (d, (&n, &i)) in shape.iter().zip(ix.iter()).enumerate() {
            if i.is_nan() || (i.is_finite() && i != i.trunc()) {
                let shown = if i.is_nan() { "NaN".to_string() } else { fermium_units::numfmt::format_number6(i) };
                return self.err(format!("an array index must be a whole number (1, 2, 3, ...), not {shown}"));
            }
            if !(i >= 1.0 && i <= n as f64 && i == i.trunc()) {
                let shown = if i == i.trunc() && i.is_finite() { format!("{}", i as i64) } else { format!("{i}") };
                return self.err(format!("index {shown} is out of range for dimension {} of this {} array (valid \
                                         indexes there are 1 to {n})", d + 1, shape_text(shape)));
            }
            k = k * n + (i as usize - 1);
        }
        Ok(k)
    }

    /// arr.fill, arr.get, arr.size, arr.sum, … (D283).
    pub(crate) fn array_builtin(&mut self, op: &str, args: &[Value]) -> Result<Value, RunError> {
        if op == "fill" {
            let x = self.plain(&args[0])?;
            let mut shape = vec![];
            for a in &args[1..] {
                let n = a.num();
                if !(n >= 0.0 && n == n.trunc()) {
                    return self.err(format!("an array size must be a whole number 0 or more, not {}", py_num(n)));
                }
                shape.push(n as usize);
            }
            let total = shape.iter().try_fold(1usize, |acc, &n| acc.checked_mul(n)).unwrap_or(usize::MAX);
            if total > 1_000_000_000 {
                return self.err(format!("not enough memory for a {} array (the most is 10⁹ entries)", shape_text(&shape)));
            }
            if shape.len() == 1 {
                return Ok(Value::List(Rc::new(RefCell::new(vec![x; total]))));
            }
            return Ok(arr_new(shape, vec![x; total]));
        }
        let Value::NdArr(a) = &args[0] else { return self.err("this array has no value") };
        if op == "set" {
            // A[i, j, …] = x from compiled code: arr.set(A, x, i, j, …)
            let ix: Vec<f64> = args[2..].iter().map(Value::num).collect();
            let k = self.arr_offset(&a.borrow().shape, &ix)?;
            let x = self.plain(&args[1])?;
            a.borrow_mut().data[k] = x;
            return Ok(Value::Void);
        }
        let a = a.borrow();
        Ok(match op {
            "get" => {
                let ix: Vec<f64> = args[1..].iter().map(Value::num).collect();
                let k = self.arr_offset(&a.shape, &ix)?;
                Value::Num(a.data[k])
            }
            "size" if args.len() == 2 => {
                let k = args[1].num();
                if !(k >= 1.0 && k <= a.shape.len() as f64 && k == k.trunc()) {
                    return self.err(format!("size(A, k): this array has {} dimensions, so k is 1 to {}", a.shape.len(),
                                            a.shape.len()));
                }
                Value::Num(a.shape[k as usize - 1] as f64)
            }
            "size" => Value::List(Rc::new(RefCell::new(a.shape.iter().map(|&n| n as f64).collect()))),
            "sum" => Value::Num(a.data.iter().sum()),
            "mean" => {
                if a.data.is_empty() {
                    return self.err("the mean of an empty array is undefined");
                }
                Value::Num(a.data.iter().sum::<f64>() / a.data.len() as f64)
            }
            "max" | "min" => {
                if a.data.is_empty() {
                    return self.err(format!("the {op} of an empty array is undefined"));
                }
                let mut r = a.data[0];
                for &x in &a.data[1..] {
                    if x.is_nan() || r.is_nan() {
                        r = f64::NAN;
                    } else if (op == "max" && x > r) || (op == "min" && x < r) {
                        r = x;
                    }
                }
                Value::Num(r)
            }
            "abs" => arr_new(a.shape.clone(), a.data.iter().map(|x| x.abs()).collect()),
            // B = A shares the array (as lists do, D26); copy(A) is a new one
            "copy" => arr_new(a.shape.clone(), a.data.clone()),
            _ => return self.err(format!("the built-in {op} isn't supported on arrays")),
        })
    }
}

fn py_num(x: f64) -> String {
    if x == x.trunc() && x.is_finite() { format!("{}", x as i64) } else { format!("{x}") }
}
