//! The run-time side of the LLVM back end: the context the compiled code calls back into (printing, lists,
//! texts, run-time errors, the built-ins it hands to the tree-walker), like fermium/runtime/core.py in v1.
//!
//! Every callback takes the context pointer first. A callback that fails stores the error in the context and
//! sets `Ctx::err`; the compiled code checks that flag after the call and returns up to `fm_main` (no unwinding
//! through Rust frames). Messages come from the tree-walker's own code where it has one (`list_index`, `bin`,
//! the built-ins), so both back ends say exactly the same thing.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use fermium_ir::{BinOp, Module};

use crate::eval::{self, Interpreter, Printer, RunError, Value};

/// A list of numbers as the compiled code sees it: it reads `ptr` and `len` directly (indexing is inlined).
#[repr(C)]
pub struct FmList {
    pub ptr: *mut f64,
    pub len: usize,
    pub cap: usize,
}

impl FmList {
    fn from_vec(v: Vec<f64>) -> FmList {
        let mut v = std::mem::ManuallyDrop::new(v);
        FmList { ptr: v.as_mut_ptr(), len: v.len(), cap: v.capacity() }
    }
    /// # Safety: the list must be valid.
    pub unsafe fn as_slice(&self) -> &[f64] {
        if self.len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(self.ptr, self.len)
        }
    }
    unsafe fn with_vec<R>(&mut self, f: impl FnOnce(&mut Vec<f64>) -> R) -> R {
        let mut v = Vec::from_raw_parts(self.ptr, self.len, self.cap);
        let r = f(&mut v);
        let mut v = std::mem::ManuallyDrop::new(v);
        self.ptr = v.as_mut_ptr();
        self.len = v.len();
        self.cap = v.capacity();
        r
    }
}

impl Drop for FmList {
    fn drop(&mut self) {
        unsafe { drop(Vec::from_raw_parts(self.ptr, self.len, self.cap)) }
    }
}

/// A list of texts (text ids into `Ctx::texts`).
pub struct FmTList(pub Vec<i64>);

/// A printer that prints nothing: the tree-walker the context keeps for built-ins and messages never prints.
pub struct NullPrinter;

impl Printer for NullPrinter {
    fn num(&mut self, _: usize, _: f64) {}
    fn list(&mut self, _: usize, _: &[f64]) {}
    fn vec(&mut self, _: usize, _: &[f64]) {}
    fn mixed_vec(&mut self, _: &[usize], _: &[f64]) {}
    fn mat(&mut self, _: usize, _: &[f64], _: usize, _: usize) {}
    fn complex(&mut self, _: usize, _: f64, _: f64) {}
    fn clist(&mut self, _: usize, _: &[(f64, f64)]) {}
    fn boolean(&mut self, _: bool) {}
    fn text(&mut self, _: &str) {}
    fn textlist(&mut self, _: &[Rc<str>]) {}
    fn end(&mut self) {}
}

/// The kind of a value in the compiled code (how it is passed to and from callbacks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// f64
    F,
    /// i1 (a u64 0/1 in callback slots)
    B,
    /// a text id (i64)
    S,
    /// *mut FmList
    L,
    /// *mut FmTList
    TL,
    /// n f64 (vectors, matrices row by row, complex numbers as re, im)
    V(usize),
    /// a solution made by the compiled code (an index into `Ctx::sols`, i64)
    H,
    Void,
}

/// A built-in call the compiled code hands to the tree-walker's implementation.
pub struct BuiltinSite {
    pub name: String,
    pub args: Vec<Kind>,
    pub ret: Kind,
}

// run-time error kinds (the `kind` argument of fm_error)
pub const E_PENDING: i64 = -1;
pub const E_INDEX: i64 = 1;
pub const E_ASSERT: i64 = 4;
pub const E_STEP0: i64 = 7;
pub const E_RANGE: i64 = 12;
pub const E_DEEP: i64 = 10;
pub const E_PAR_ALIAS: i64 = 40;
pub const E_SUM_STEP: i64 = 1007;
pub const E_SUM_RANGE: i64 = 1012;

/// What the compiled code runs against. `err` is first so the compiled code finds it at offset 0.
#[repr(C)]
pub struct Ctx<'m> {
    pub err: i32,
    /// the program line that called into a module's code (D185), set by the compiled code (offset 4)
    pub call_line: u32,
    pub module: &'m Module,
    printer: *mut (dyn Printer + 'm),
    pub error: Option<RunError>,
    pub texts: Vec<Rc<str>>,
    lists: Vec<*mut FmList>,
    tlists: Vec<*mut FmTList>,
    pub interp: Interpreter<'m, NullPrinter>,
    pub builtins: Vec<BuiltinSite>,
    pub mvec_fmts: Vec<Vec<usize>>,
    pub ode_sites: Vec<super::solve_rt::OdeSite>,
    pub sols: Vec<super::solve_rt::CSol>,
    /// printed measured sums (spec B2 decimal-place rule): the shape of each sum
    pub msum_sites: Vec<MNode>,
    /// the fewest figures the integrals since the last print can support (eval_calc's QUAD_SF, spec B2)
    pub quad_sf: Option<u32>,
    /// taken by callbacks that change shared state (parallel for runs iterations on several threads)
    lock: Mutex<()>,
}

impl<'m> Ctx<'m> {
    pub fn new(module: &'m Module, printer: &mut (dyn Printer + 'm)) -> Box<Ctx<'m>> {
        Box::new(Ctx {
            err: 0,
            call_line: 0,
            module,
            printer: printer as *mut _,
            error: None,
            texts: module.tables.texts.iter().map(|s| Rc::from(s.as_str())).collect(),
            lists: vec![],
            tlists: vec![],
            interp: Interpreter::new(module, NullPrinter),
            builtins: vec![],
            mvec_fmts: vec![],
            ode_sites: vec![],
            sols: vec![],
            msum_sites: vec![],
            quad_sf: None,
            lock: Mutex::new(()),
        })
    }

    /// The code generator's tables (texts it made, built-in and solve sites, …).
    pub fn set_tables(&mut self, t: super::blob::GenTables) {
        let super::blob::GenTables { texts, builtins, mvec_fmts, ode_sites, msum_sites } = t;
        self.texts = texts;
        self.builtins = builtins;
        self.mvec_fmts = mvec_fmts;
        self.ode_sites = ode_sites;
        self.msum_sites = msum_sites;
    }

    /// Run compiled code: `set_ctx` gets this context's address (the code reads it), then `main` runs; the
    /// run-time error it stopped with, if any, at its program line (D185).
    pub fn run(&mut self, set_ctx: impl FnOnce(*mut u8), main: unsafe extern "C" fn()) -> Result<(), RunError> {
        set_ctx(self as *mut Ctx as *mut u8);
        unsafe { main() };
        match self.error.take() {
            Some(e) => Err(self.locate(e)),
            None => Ok(()),
        }
    }

    fn printer(&mut self) -> &mut (dyn Printer + 'm) {
        unsafe { &mut *self.printer }
    }

    pub fn intern(&mut self, s: &str) -> i64 {
        if let Some(i) = self.texts.iter().position(|t| &**t == s) {
            return i as i64;
        }
        self.texts.push(Rc::from(s));
        self.texts.len() as i64 - 1
    }

    fn add_text(&mut self, s: Rc<str>) -> i64 {
        self.texts.push(s);
        self.texts.len() as i64 - 1
    }

    pub(super) fn fail(&mut self, e: RunError) {
        if self.error.is_none() {
            self.error = Some(e);
        }
        self.err = 1;
    }

    fn new_list(&mut self, v: Vec<f64>) -> *mut FmList {
        let p = Box::into_raw(Box::new(FmList::from_vec(v)));
        self.lists.push(p);
        p
    }

    fn new_tlist(&mut self, v: Vec<i64>) -> *mut FmTList {
        let p = Box::into_raw(Box::new(FmTList(v)));
        self.tlists.push(p);
        p
    }

    /// A value of the compiled code as the tree-walker's Value (lists are copied).
    unsafe fn to_value(&self, k: Kind, slot: u64) -> Value {
        match k {
            Kind::F => Value::Num(f64::from_bits(slot)),
            Kind::B => Value::Bool(slot != 0),
            Kind::S => Value::Str(self.texts[slot as usize].clone()),
            Kind::L => match (slot as *const FmList).as_ref() {
                Some(l) => Value::List(Rc::new(RefCell::new(l.as_slice().to_vec()))),
                None => Value::Void,
            },
            Kind::TL => match (slot as *const FmTList).as_ref() {
                Some(l) => Value::TextList(Rc::new(RefCell::new(l.0.iter().map(|&i| self.texts[i as usize].clone())
                    .collect()))),
                None => Value::Void,
            },
            Kind::V(n) => Value::Vec(Rc::new(std::slice::from_raw_parts(slot as *const f64, n).to_vec())),
            Kind::H | Kind::Void => Value::Void,
        }
    }

    /// Store a Value where the compiled code expects a value of kind `k` (`out` holds n f64 for V(n)).
    unsafe fn from_value(&mut self, k: Kind, v: Value, out: *mut u64) {
        match k {
            Kind::F => *out = v.num().to_bits(),
            Kind::B => *out = u64::from(v.truth()),
            Kind::S => {
                *out = match v {
                    Value::Str(s) => self.add_text(s) as u64,
                    _ => self.intern("") as u64,
                }
            }
            Kind::L => {
                *out = match v {
                    Value::List(l) => {
                        let v = l.borrow().clone();
                        self.new_list(v) as u64
                    }
                    _ => 0,
                }
            }
            Kind::TL => {
                *out = match v {
                    Value::TextList(l) => {
                        let ids: Vec<Rc<str>> = l.borrow().clone();
                        let ids = ids.into_iter().map(|s| self.add_text(s)).collect();
                        self.new_tlist(ids) as u64
                    }
                    _ => 0,
                }
            }
            Kind::V(n) => {
                let o = out as *mut f64;
                let src: Vec<f64> = match v {
                    Value::Vec(v) => v.to_vec(),
                    Value::Num(x) => vec![x],
                    _ => vec![],
                };
                for i in 0..n {
                    *o.add(i) = src.get(i).copied().unwrap_or(f64::NAN);
                }
            }
            Kind::H => *out = 0,
            Kind::Void => {}
        }
    }
}

impl Ctx<'_> {
    /// A run-time error's program line: in a module's code, the line that called into it, and the message says
    /// where in the module it happened (eval.rs locate, errors.py decode_line, D185).
    pub fn locate(&self, mut e: RunError) -> RunError {
        use crate::eval::{MODLINE_MAX, MODLINE_SHIFT};
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
}

impl Drop for Ctx<'_> {
    fn drop(&mut self) {
        for &p in &self.lists {
            unsafe { drop(Box::from_raw(p)) }
        }
        for &p in &self.tlists {
            unsafe { drop(Box::from_raw(p)) }
        }
    }
}

pub(super) type C<'a> = *mut Ctx<'a>;

/// Run `f` on the context holding its lock (callbacks that allocate or change shared state: parallel for runs
/// iterations on several threads).
pub(super) fn locked<'a, R>(c: C<'a>, f: impl FnOnce(&mut Ctx<'a>) -> R) -> R {
    let lock: *const Mutex<()> = unsafe { &(*c).lock };
    let _g = unsafe { (*lock).lock().unwrap_or_else(|e| e.into_inner()) };
    f(unsafe { &mut *c })
}

// ---------------------------------------------------------------- errors
#[no_mangle]
pub extern "C" fn fm_error(c: C, kind: i64, a: f64, b: f64, line: i32) {
    locked(c, |c| error(c, kind, a, b, line))
}

fn error(c: &mut Ctx, kind: i64, a: f64, b: f64, line: i32) {
    let line = line.max(0) as u32;
    c.interp.line = line;
    let e = match kind {
        E_INDEX => match c.interp.elem_index(a, b as usize) {
            Err(e) => e,
            Ok(_) => RunError { message: format!("index {a} is out of range"), line, hint: None },
        },
        E_ASSERT => {
            RunError { message: c.module.tables.texts.get(a as usize).cloned().unwrap_or_default(), line, hint: None }
        }
        // the tree-walker's for_count
        E_STEP0 => match c.interp.for_count(a, b, 0.0) {
            Err(e) => e,
            Ok(_) => RunError { message: "the step must be non-zero".into(), line, hint: None },
        },
        E_SUM_STEP | E_SUM_RANGE => {
            use fermium_runtime::numerics::{err as K, Fail};
            let f = if kind == E_SUM_STEP { Fail::new(K::STEP, a, b) } else { Fail::new(12, a, b) };
            RunError { message: crate::eval_calc::describe(c.module, f, None), line, hint: None }
        }
        E_RANGE => {
            let f = fermium_units::numfmt::format_number6;
            RunError { message: format!("this for loop has no definite number of steps: it goes from {} to {} (NaN \
                                         in the start, end or step)", f(a), f(b)), line, hint: None }
        }
        E_DEEP => {
            // eval.rs call(): the function's name as written, at the line of its definition
            let f = c.module.funcs.get(a as usize);
            let name = f.map(|f| f.display.clone()).filter(|d| !d.is_empty()).unwrap_or_else(|| "a function".into());
            let line = f.map(|f| f.def_line).filter(|&l| l != 0).unwrap_or(line);
            RunError { message: format!("{name} called itself too many times (the program ran out of stack) -- is a \
                                         base case missing, like  if n <= 0 then ...?"), line, hint: None }
        }
        E_PAR_ALIAS => {
            // eval_par.rs's message
            let t = c.module.tables.texts.get(a as usize).cloned().unwrap_or_default();
            RunError { message: format!("{t} are the same list (one was set from the other), so the iterations of \
                                         this parallel for would write and read the same numbers at the same \
                                         time; make a copy first, e.g.  ys = xs * 1"), line, hint: None }
        }
        _ => RunError { message: "runtime error".into(), line, hint: None },
    };
    c.fail(e);
}

// ---------------------------------------------------------------- printing
#[no_mangle]
pub extern "C" fn fm_print_num(c: C, fmt: i64, v: f64) {
    unsafe { (*c).printer().num(fmt as usize, v) }
}
#[no_mangle]
pub extern "C" fn fm_print_bool(c: C, b: i32) {
    unsafe { (*c).printer().boolean(b != 0) }
}
#[no_mangle]
pub extern "C" fn fm_print_text(c: C, id: i64) {
    let c = unsafe { &mut *c };
    let t = c.texts[id as usize].clone();
    c.printer().text(&t)
}
#[no_mangle]
pub extern "C" fn fm_print_list(c: C, fmt: i64, l: *const FmList) {
    let c = unsafe { &mut *c };
    if let Some(l) = unsafe { l.as_ref() } {
        let v = unsafe { l.as_slice() };
        c.printer().list(fmt as usize, v)
    }
}
#[no_mangle]
pub extern "C" fn fm_print_tlist(c: C, l: *const FmTList) {
    let c = unsafe { &mut *c };
    if let Some(l) = unsafe { l.as_ref() } {
        let v: Vec<Rc<str>> = l.0.iter().map(|&i| c.texts[i as usize].clone()).collect();
        c.printer().textlist(&v)
    }
}
#[no_mangle]
pub extern "C" fn fm_print_vec(c: C, fmt: i64, p: *const f64, n: i64) {
    let v = unsafe { std::slice::from_raw_parts(p, n as usize) };
    unsafe { (*c).printer().vec(fmt as usize, v) }
}
#[no_mangle]
pub extern "C" fn fm_print_mvec(c: C, site: i64, p: *const f64, n: i64) {
    let c = unsafe { &mut *c };
    let v = unsafe { std::slice::from_raw_parts(p, n as usize) };
    let fmts = c.mvec_fmts[site as usize].clone();
    c.printer().mixed_vec(&fmts, v)
}
#[no_mangle]
pub extern "C" fn fm_print_mat(c: C, fmt: i64, p: *const f64, r: i64, cols: i64) {
    let v = unsafe { std::slice::from_raw_parts(p, (r * cols) as usize) };
    unsafe { (*c).printer().mat(fmt as usize, v, r as usize, cols as usize) }
}
#[no_mangle]
pub extern "C" fn fm_print_complex(c: C, fmt: i64, re: f64, im: f64) {
    unsafe { (*c).printer().complex(fmt as usize, re, im) }
}
#[no_mangle]
pub extern "C" fn fm_print_end(c: C) {
    unsafe { (*c).printer().end() }
}

// ---------------------------------------------------------------- lists
/// The shape of a printed measured sum: + / − over operands with their significant figures (eval_calc sum_place).
#[derive(Clone, Debug)]
pub enum MNode {
    Leaf(Option<u32>),
    Op(bool, Box<MNode>, Box<MNode>),
}

/// The last significant decimal place of x with sf figures (eval_calc last_place).
fn last_place(x: f64, sf: u32) -> Option<i32> {
    (x != 0.0 && x.is_finite()).then(|| x.abs().log10().floor() as i32 - sf as i32 + 1)
}

fn msum_combine(n: &MNode, vals: &[f64], at: &mut usize, k: f64) -> (f64, Option<i32>) {
    match n {
        MNode::Leaf(sf) => {
            let v = vals[*at];
            *at += 1;
            (v, sf.and_then(|s| last_place(v / k, s)))
        }
        MNode::Op(add, a, b) => {
            let (va, pa) = msum_combine(a, vals, at, k);
            let (vb, pb) = msum_combine(b, vals, at, k);
            let v = if *add { va + vb } else { va - vb };
            (v, match (pa, pb) {
                (Some(x), Some(y)) => Some(x.max(y)),
                _ => None,
            })
        }
    }
}

/// Print a measured sum (its operands' values in DFS order) by the decimal-place rule (eval_calc sum_sf).
#[no_mangle]
pub extern "C" fn fm_print_msum(c: C, fmt: i64, site: i64, vals: *const f64, n: i64) {
    let c = unsafe { &mut *c };
    let vals = unsafe { std::slice::from_raw_parts(vals, n as usize) };
    let fmt = fmt as usize;
    let k = match c.module.tables.fmts.get(fmt) {
        Some(f) => {
            let hint = f.hint.as_ref().map(|h| fermium_units::Unit { name: h.name.clone(), dim: h.dim,
                                                                       factor: h.factor, offset: h.offset });
            fermium_units::display_unit(&f.dim, hint.as_ref()).factor
        }
        None => 1.0,
    };
    let node = c.msum_sites[site as usize].clone();
    let (v, p) = msum_combine(&node, vals, &mut 0, k);
    match p.map(|p| crate::eval_calc::round_to_place(v / k, p)) {
        Some((x, Some(sf))) => c.printer().num_sf(fmt, x * k, sf),
        _ => c.printer().num(fmt, v),
    }
}

/// Before printing an integral-shaped value: forget the integrals evaluated earlier (take_quad_sf).
#[no_mangle]
pub extern "C" fn fm_quad_sf_clear(c: C) {
    unsafe { (*c).quad_sf = None }
}

/// Print an integral-shaped value, capped at the figures its integrals support (eval.rs print, spec B2).
#[no_mangle]
pub extern "C" fn fm_print_num_capped(c: C, fmt: i64, v: f64) {
    let c = unsafe { &mut *c };
    match c.quad_sf.take() {
        Some(n) => c.printer().num_capped(fmt as usize, v, n),
        None => c.printer().num(fmt as usize, v),
    }
}

/// A new list holding v (for other run-time modules).
pub(super) fn fm_list_from(c: C, v: Vec<f64>) -> *mut FmList {
    locked(c, |c| c.new_list(v))
}
#[no_mangle]
pub extern "C" fn fm_list_new(c: C, cap: i64) -> *mut FmList {
    locked(c, |c| c.new_list(Vec::with_capacity(cap.max(0) as usize)))
}
#[no_mangle]
pub extern "C" fn fm_list_push(l: *mut FmList, x: f64) {
    if let Some(l) = unsafe { l.as_mut() } {
        unsafe { l.with_vec(|v| v.push(x)) }
    }
}
#[no_mangle]
pub extern "C" fn fm_list_extend(l: *mut FmList, o: *const FmList) {
    if l.is_null() || o.is_null() {
        return;
    }
    // `xs += xs` (the same list) appends a copy of itself, as the tree-walker's clone-then-extend does
    let ys: Vec<f64> = unsafe { (*o).as_slice().to_vec() };
    unsafe { (*l).with_vec(|v| v.extend(ys)) }
}
#[no_mangle]
pub extern "C" fn fm_list_clear(l: *mut FmList) {
    if let Some(l) = unsafe { l.as_mut() } {
        unsafe { l.with_vec(|v| v.clear()) }
    }
}
#[no_mangle]
pub extern "C" fn fm_list_copy(c: C, l: *const FmList) -> *mut FmList {
    let v = match unsafe { l.as_ref() } {
        Some(l) => unsafe { l.as_slice().to_vec() },
        None => vec![],
    };
    locked(c, |c| c.new_list(v))
}

fn op_of(op: i32) -> BinOp {
    match op {
        0 => BinOp::Add,
        1 => BinOp::Sub,
        2 => BinOp::Mul,
        _ => BinOp::Div,
    }
}

#[inline]
fn apply(op: BinOp, x: f64, y: f64) -> f64 {
    match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div => eval::fdiv(x, y),
    }
}

/// list ∘ list, element by element (the tree-walker's `bin`, whose message a length mismatch gets).
#[no_mangle]
pub extern "C" fn fm_list_binll(c: C, op: i32, a: *const FmList, b: *const FmList, line: i32) -> *mut FmList {
    locked(c, |c| list_binll(c, op, a, b, line))
}

fn list_binll(c: &mut Ctx, op: i32, a: *const FmList, b: *const FmList, line: i32) -> *mut FmList {
    let op = op_of(op);
    let (x, y) = unsafe { ((*a).as_slice(), (*b).as_slice()) };
    if x.len() != y.len() {
        c.interp.line = line.max(0) as u32;
        let va = Value::List(Rc::new(RefCell::new(x.to_vec())));
        let vb = Value::List(Rc::new(RefCell::new(y.to_vec())));
        let e = match c.interp.bin(op, va, vb) {
            Err(e) => e,
            Ok(_) => RunError { message: "these two lists have different lengths".into(), line: line as u32, hint: None },
        };
        c.fail(e);
        return std::ptr::null_mut();
    }
    let v = x.iter().zip(y.iter()).map(|(p, q)| apply(op, *p, *q)).collect();
    c.new_list(v)
}
/// list ∘ number (swap: number ∘ list).
#[no_mangle]
pub extern "C" fn fm_list_binls(c: C, op: i32, a: *const FmList, y: f64, swap: i32) -> *mut FmList {
    let op = op_of(op);
    let x = unsafe { (*a).as_slice() };
    let v = if swap != 0 { x.iter().map(|p| apply(op, y, *p)).collect() } else { x.iter().map(|p| apply(op, *p, y)).collect() };
    locked(c, |c| c.new_list(v))
}
#[no_mangle]
pub extern "C" fn fm_list_powc(c: C, a: *const FmList, p: f64) -> *mut FmList {
    let v = unsafe { (*a).as_slice() }.iter().map(|x| eval::powc(*x, p)).collect();
    locked(c, |c| c.new_list(v))
}
/// list ** y (eval.rs Pow on a list: powf element by element)
#[no_mangle]
pub extern "C" fn fm_list_powf(c: C, a: *const FmList, y: f64) -> *mut FmList {
    let v = unsafe { (*a).as_slice() }.iter().map(|x| x.powf(y)).collect();
    locked(c, |c| c.new_list(v))
}
#[no_mangle]
pub extern "C" fn fm_list_neg(c: C, a: *const FmList) -> *mut FmList {
    let v = unsafe { (*a).as_slice() }.iter().map(|x| -x).collect();
    locked(c, |c| c.new_list(v))
}

// ---------------------------------------------------------------- calculus (eval_calc.rs)
/// A compiled scalar lambda: f(x, env).
pub type ScalarFn = unsafe extern "C" fn(x: f64, env: *mut u8) -> f64;

const QZERO_MSG: &str = "this integral came out as exactly 0 because the integrand was 0 at every point where it was \
                         sampled; if it is non-zero somewhere narrow (a peak in a wide range), integrate over a range \
                         that fits it";

fn err_flag<'a>(c: C<'a>) -> &'a std::sync::atomic::AtomicI32 {
    unsafe { &*(c as *const std::sync::atomic::AtomicI32) }
}

fn fmt_opt(i: i64) -> Option<usize> {
    if i >= 0 {
        Some(i as usize)
    } else {
        None
    }
}

/// ∫ f from a to b (eval_calc Integral): fermium-runtime's quad; once the integrand has stopped with an error
/// it isn't called again (NaN), and that error is the one reported.
#[no_mangle]
pub extern "C" fn fm_quad(c: C, f: ScalarFn, env: *mut u8, a: f64, b: f64, atol: f64, xname: f64, xfmt: i64,
                          line: i32) -> f64 {
    use std::sync::atomic::Ordering::SeqCst;
    let flag = err_flag(c);
    let r = fermium_runtime::numerics::quad::quad(
        |x| if flag.load(SeqCst) != 0 { f64::NAN } else { unsafe { f(x, env) } },
        a, b, 1e-10, atol, xname);
    if flag.load(SeqCst) != 0 {
        return f64::NAN;
    }
    let line = line.max(0) as u32;
    locked(c, |c| match r {
        Err(fl) => {
            let e = RunError { message: crate::eval_calc::describe(c.module, fl, fmt_opt(xfmt)), line, hint: None };
            c.fail(e);
            f64::NAN
        }
        Ok(r) => {
            // eval_calc note_quad_sf
            if let Some(n) = crate::eval_calc::meaningful_sf(r.value, r.error, r.abs_sum) {
                c.quad_sf = Some(c.quad_sf.map_or(n, |m| m.min(n)));
            }
            if r.all_zero {
                if atol < 0.0 {
                    // eval_calc's QZERO += 1 (the quiet first try of a vector component, D110), through its
                    // qzero_mark / qzero_check built-ins: mark returns the count and clears it, check sets it
                    c.interp.line = line;
                    let n = c.interp.builtin("qzero_mark", vec![]).map(|v| v.num()).unwrap_or(0.0);
                    let _ = c.interp.builtin("qzero_check", vec![Value::Num(n + 1.0), Value::Num(f64::NAN)]);
                } else {
                    crate::eval_calc::warn_at(line, QZERO_MSG);
                }
            }
            r.value
        }
    })
}

/// The x in [a, b] where f(x) = 0 (eval_calc Root); g = |lhs| + |rhs| for the rounding-noise warning, or null.
#[no_mangle]
pub extern "C" fn fm_root(c: C, f: ScalarFn, fenv: *mut u8, g: Option<ScalarFn>, genv: *mut u8, a: f64, b: f64,
                          tfmt: i64, line: i32) -> f64 {
    use std::sync::atomic::Ordering::SeqCst;
    let flag = err_flag(c);
    let call = |h: ScalarFn, env: *mut u8, x: f64| if flag.load(SeqCst) != 0 { f64::NAN } else { unsafe { h(x, env) } };
    let mut sc = |x: f64| call(g.unwrap(), genv, x);
    let r = fermium_runtime::numerics::roots::root(|x| call(f, fenv, x), a, b, 200,
                                                   if g.is_some() { Some(&mut sc as &mut dyn FnMut(f64) -> f64) } else { None });
    if flag.load(SeqCst) != 0 {
        return f64::NAN;
    }
    let line = line.max(0) as u32;
    locked(c, |c| match r {
        Err(fl) => {
            let e = RunError { message: crate::eval_calc::describe(c.module, fl, fmt_opt(tfmt)), line, hint: None };
            c.fail(e);
            f64::NAN
        }
        Ok(r) => {
            if let Some(x) = r.noise_warning {
                crate::eval_calc::warn_at(line, &format!(
                    "the two sides of this equation agree only to rounding error near {}, so the solution found there \
                     may be meaningless (large terms cancelling?); rewrite the equation so they cancel on paper",
                    crate::eval_calc::fmt_value(c.module, x, fmt_opt(tfmt))));
            }
            r.x
        }
    })
}

// ---------------------------------------------------------------- parallel for (D152)
/// The compiled body of a parallel for: runs iterations [first, end) and writes the block's sums to `part`.
pub type ParBody = unsafe extern "C" fn(env: *mut u8, first: i64, end: i64, part: *mut f64, lo: f64, st: f64);

/// Stack of each worker thread (v1: PAR_STACK).
const PAR_STACK: usize = 64 << 20;

/// Threads for parallel for: FERMIUM_THREADS, else every core.
fn par_threads() -> usize {
    std::env::var("FERMIUM_THREADS").ok().and_then(|s| s.trim().parse::<usize>().ok()).filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// Run the blocks of a parallel for with n iterations on several threads; block b's sums go to part[b·nr ..].
/// If any iteration stops with an error, the loop is run again block by block on this thread, so the error
/// reported is the first one in iteration order, as the tree-walker (which runs the blocks in order) reports it.
#[no_mangle]
pub extern "C" fn fm_par_run(c: C, f: ParBody, env: *mut u8, n: i64, lo: f64, st: f64, part: *mut f64, nr: i64) {
    let blocks = fermium_ir::par_blocks(n.max(0) as usize);
    let flag = unsafe { &*(c as *const std::sync::atomic::AtomicI32) };
    let serial = |from: usize| {
        for (b, &(a, e)) in blocks.iter().enumerate().skip(from) {
            unsafe { f(env, a as i64, e as i64, part.add(b * nr as usize), lo, st) };
            if flag.load(std::sync::atomic::Ordering::SeqCst) != 0 {
                return;
            }
        }
    };
    let threads = par_threads().min(blocks.len());
    if threads <= 1 {
        return serial(0);
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (envu, partu) = (env as usize, part as usize);
    let spawned = std::thread::scope(|s| {
        let mut spawned = 0;
        for _ in 0..threads {
            let (next, blocks) = (&next, &blocks);
            spawned += std::thread::Builder::new().stack_size(PAR_STACK).spawn_scoped(s, move || loop {
                let b = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if b >= blocks.len() || flag.load(std::sync::atomic::Ordering::SeqCst) != 0 {
                    break;
                }
                let (a, e) = blocks[b];
                unsafe { f(envu as *mut u8, a as i64, e as i64, (partu as *mut f64).add(b * nr as usize), lo, st) };
            }).is_ok() as usize;
        }
        spawned
    });
    if spawned == 0 {
        return serial(0);
    }
    if flag.load(std::sync::atomic::Ordering::SeqCst) != 0 {
        locked(c, |c| {
            c.err = 0;
            c.error = None;
        });
        serial(0);
    }
}

// ---------------------------------------------------------------- lists of texts
#[no_mangle]
pub extern "C" fn fm_tlist_push(l: *mut FmTList, id: i64) {
    if let Some(l) = unsafe { l.as_mut() } {
        l.0.push(id)
    }
}
#[no_mangle]
pub extern "C" fn fm_tlist_new(c: C) -> *mut FmTList {
    locked(c, |c| c.new_tlist(vec![]))
}
#[no_mangle]
pub extern "C" fn fm_tlist_clear(l: *mut FmTList) {
    if let Some(l) = unsafe { l.as_mut() } {
        l.0.clear()
    }
}
#[no_mangle]
pub extern "C" fn fm_tlist_len(l: *const FmTList) -> i64 {
    unsafe { l.as_ref() }.map(|l| l.0.len() as i64).unwrap_or(0)
}
/// The text id at a 0-based position (the compiled code has checked the index).
#[no_mangle]
pub extern "C" fn fm_tlist_at(l: *const FmTList, k: i64) -> i64 {
    unsafe { (&(*l).0)[k as usize] }
}
#[no_mangle]
pub extern "C" fn fm_tlist_copy(c: C, l: *const FmTList) -> *mut FmTList {
    let v = unsafe { l.as_ref() }.map(|l| l.0.clone()).unwrap_or_default();
    locked(c, |c| c.new_tlist(v))
}

// ---------------------------------------------------------------- built-ins run by the tree-walker's code
/// Call built-in `site` with `args` (one slot each: f64 bits, 0/1, text id, list pointer, or a pointer to n f64);
/// the result goes to `out` (n f64 for a vector).
#[no_mangle]
pub extern "C" fn fm_builtin(c: C, site: i64, args: *const u64, out: *mut u64, line: i32) {
    locked(c, |c| {
        let (name, kinds, ret) = {
            let s = &c.builtins[site as usize];
            (s.name.clone(), s.args.clone(), s.ret)
        };
        let vals: Vec<Value> = kinds.iter().enumerate().map(|(i, k)| unsafe { c.to_value(*k, *args.add(i)) }).collect();
        c.interp.line = line.max(0) as u32;
        match c.interp.builtin(&name, vals) {
            Ok(v) if kind_matches(ret, &v) => unsafe { c.from_value(ret, v, out) },
            Ok(v) => {
                // never convert silently: the compiled code expected another kind of value
                if !matches!(ret, Kind::V(_) | Kind::Void) {
                    unsafe { *out = 0 };
                }
                let line = line.max(0) as u32;
                c.fail(RunError { message: format!("internal error in the LLVM back end: the built-in {name} gave \
                                                    {v:?} where a {ret:?} value was expected (run with --backend \
                                                    interp)"), line, hint: None })
            }
            Err(e) => {
                if !matches!(ret, Kind::V(_) | Kind::Void) {
                    unsafe { *out = 0 };
                }
                c.fail(e)
            }
        }
    })
}

/// Does a built-in's result fit the kind the compiled code expects (as the tree-walker would go on using it)?
fn kind_matches(k: Kind, v: &Value) -> bool {
    match (k, v) {
        (Kind::F, Value::Num(_) | Value::Bool(_) | Value::Void) => true,
        (Kind::B, Value::Bool(_) | Value::Num(_)) => true,
        (Kind::S, Value::Str(_)) | (Kind::L, Value::List(_)) | (Kind::TL, Value::TextList(_)) => true,
        (Kind::V(n), Value::Vec(x)) => x.len() == n,
        (Kind::Void, _) => true,
        _ => false,
    }
}

// ---------------------------------------------------------------- math: the same Rust functions eval.rs uses
macro_rules! math1 {
    ($($name:ident => $f:expr;)*) => {
        $(#[no_mangle] pub extern "C" fn $name(x: f64) -> f64 { let f: fn(f64) -> f64 = $f; f(x) })*
        /// (IR name of a built-in, the shim's symbol name, its address)
        pub fn math1_shims() -> Vec<(&'static str, &'static str, usize)> {
            vec![$((&stringify!($name)[3..], stringify!($name), $name as *const () as usize)),*]
        }
    };
}

math1! {
    fm_sin => |x| x.sin();
    fm_cos => |x| x.cos();
    fm_tan => |x| x.tan();
    fm_asin => |x| x.asin();
    fm_acos => |x| x.acos();
    fm_atan => |x| x.atan();
    fm_sinh => |x| x.sinh();
    fm_cosh => |x| x.cosh();
    fm_tanh => |x| x.tanh();
    fm_asinh => crate::eval_core::cmath::asinh;
    fm_acosh => crate::eval_core::cmath::acosh;
    fm_atanh => crate::eval_core::cmath::atanh;
    fm_erf => crate::eval_core::cmath::erf;
    fm_erfc => crate::eval_core::cmath::erfc;
    fm_gamma => crate::eval_core::cmath::tgamma;
    fm_lgamma => crate::eval_core::cmath::lgamma;
    fm_exp => |x| x.exp();
    fm_ln => |x| x.ln();
    fm_log => |x| x.ln();
    fm_log10 => |x| x.log10();
    fm_log2 => |x| x.log2();
    fm_expm1 => |x| x.exp_m1();
    fm_log1p => |x| x.ln_1p();
    fm_cot => |x| eval::fdiv(1.0, x.tan());
    fm_sec => |x| eval::fdiv(1.0, x.cos());
    fm_csc => |x| eval::fdiv(1.0, x.sin());
}

#[no_mangle]
pub extern "C" fn fm_powf(a: f64, b: f64) -> f64 {
    a.powf(b)
}
#[no_mangle]
pub extern "C" fn fm_powc(x: f64, p: f64) -> f64 {
    eval::powc(x, p)
}
#[no_mangle]
pub extern "C" fn fm_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}
#[no_mangle]
pub extern "C" fn fm_hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

#[cfg(test)]
mod tests {
    #[test]
    fn math_shims_agree_with_the_tree_walker() {
        static M: std::sync::LazyLock<fermium_ir::Module> = std::sync::LazyLock::new(Default::default);
        for (name, _, addr) in super::math1_shims() {
            let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(addr) };
            for x in [0.3, -0.7, 2.5, 1e-9] {
                let mut it = crate::eval::Interpreter::new(&M, super::NullPrinter);
                let b = it.builtin(name, vec![crate::eval::Value::Num(x)]).unwrap().num();
                let a = f(x);
                assert!(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()), "{name}({x})");
            }
        }
    }
}
