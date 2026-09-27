//! Calling Fermium from Python (DECISIONS D142, spec §B5.14): the C ABI behind `python/fermium2`, a pure-Python
//! ctypes module that offers the API of Fermium 1.5's `fermium/api.py`:
//!
//! ```text
//! import fermium2 as fermium
//! mod = fermium.compile("g = 9.81 m/s²\nperiod(L [m]) = 2π √(L / g)\n")
//! mod.period(1.0)                    # Quantity(2.006…, 's'): a plain float is in SI units
//! mod.period(fermium.Q(50, "cm"))    # any unit, converted to SI
//! ```
//!
//! A program is checked like v1's `_Session` does it: one long-lived checker with the REPL's storage model (the
//! top-level variables are kept between inputs) but not its conveniences. Each call signature (the shapes and
//! dimensions of the arguments) is compiled once into its own input, `fmpy_<n>_f_r = f(fmpy_<n>_f_a0, …)`,
//! whose arguments are hidden top-level variables of exactly those types, so the Fermium checker checks the
//! units of each call (a generic function is instantiated per signature, D43). Programs run on the tree-walker,
//! on a thread with a 512 MB stack, so runaway recursion is a clean error.
//!
//! Every function returns a JSON text (freed with `fermium_free`): `{"error": {...}}` or the result. Values
//! are in SI units; a dimension travels as its seven exponents, "1,0,-2,0,0,0,0" (fractions as "1/2").
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::fmt::Write as _;

use fermium_check::checker::{Binding, CheckOptions, Checker, SymExtra};
use fermium_codegen::eval::Value;
use fermium_codegen::printer::StdPrinter;
use fermium_codegen::session::ReplState;
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::Dim;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;
use fermium_units::Unit;
use num_rational::Rational64;

const PREFIX: &str = "fmpy_";
const STACK: usize = 512 << 20;

// ---------------------------------------------------------------- JSON out
fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A float as Python's json module reads it (NaN and ±Infinity included), exactly (shortest round trip).
fn jnum(x: f64) -> String {
    if x.is_nan() {
        "NaN".into()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity".into() } else { "-Infinity".into() }
    } else {
        format!("{x:?}")
    }
}

fn jnums(xs: &[f64]) -> String {
    format!("[{}]", xs.iter().map(|x| jnum(*x)).collect::<Vec<_>>().join(","))
}

fn jopt(s: Option<&str>) -> String {
    s.map(jstr).unwrap_or_else(|| "null".into())
}

pub fn dim_key(d: &Dim) -> String {
    d.0.iter()
        .map(|r| if *r.denom() == 1 { r.numer().to_string() } else { format!("{}/{}", r.numer(), r.denom()) })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_dim_key(s: &str) -> Option<Dim> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 7 {
        return None;
    }
    let mut e = [Rational64::from_integer(0); 7];
    for (i, p) in parts.iter().enumerate() {
        e[i] = match p.split_once('/') {
            Some((n, d)) => {
                // no panic on a zero denominator, no overflow on i64::MIN (red team 13)
                let (n, d): (i64, i64) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
                if d == 0 || n == i64::MIN || d == i64::MIN {
                    return None;
                }
                Rational64::new(n, d)
            }
            None => Rational64::from_integer(p.trim().parse().ok().filter(|v: &i64| *v != i64::MIN)?),
        };
    }
    Some(Dim(e))
}

fn junit(u: &Unit) -> String {
    format!("{{\"name\":{},\"factor\":{},\"offset\":{},\"dim\":{}}}", jstr(&u.name), jnum(u.factor), jnum(u.offset),
            jstr(&dim_key(&u.dim)))
}

fn jerror(kind: &str, message: &str, line: Option<u32>, hint: Option<&str>, formatted: &str) -> String {
    format!("{{\"error\":{{\"kind\":{},\"message\":{},\"line\":{},\"hint\":{},\"formatted\":{}}}}}", jstr(kind),
            jstr(message), line.map(|l| l.to_string()).unwrap_or_else(|| "null".into()), jopt(hint), jstr(formatted))
}

fn jdiag(kind: &str, d: &Diagnostic, src: &str) -> String {
    jerror(kind, &d.message, d.line.filter(|l| *l > 0), d.hint.as_deref(), &d.format(Some(src), None))
}

/// The unit a value is shown in (api.py `_display`): its own display unit when it has this dimension, else the
/// preferred unit of the dimension.
fn display_unit(dim: &Dim, hint: Option<&I::Hint>) -> Unit {
    match hint {
        Some(h) if h.dim == *dim && h.offset == 0.0 => Unit { name: h.name.clone(), dim: h.dim, factor: h.factor,
                                                              offset: 0.0 },
        _ => fermium_units::preferred_unit(dim),
    }
}

// ---------------------------------------------------------------- a compiled program
struct CallEntry {
    module: I::Module,
    args: Vec<I::SymId>,
    result: I::SymId,
}

pub struct Program {
    checker: Checker,
    state: ReplState,
    /// every input's tree, kept alive: the checker keeps pointers to nodes it has seen
    programs: Vec<Box<A::Program>>,
    known: Vec<String>,
    main: I::Module,
    ran: bool,
    calls: HashMap<String, CallEntry>,
    warnings: Vec<String>,
}

/// Run `f` on a thread with a big stack (deep recursion in the checker or the program is caught by the
/// tree-walker's stack check instead of crashing Python).
fn on_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    fermium_codegen::eval::STACK_LIMIT.store(400 << 20, std::sync::atomic::Ordering::Relaxed);
    std::thread::scope(|s| {
        let h = std::thread::Builder::new().stack_size(STACK).spawn_scoped(s, f).expect("a thread for Fermium");
        match h.join() {
            Ok(v) => v,
            Err(p) => std::panic::resume_unwind(p),
        }
    })
}

/// The program is only touched by one thread at a time (the caller waits for the worker), so moving the
/// reference to the worker thread is sound although it holds Rc values.
struct AssertSend<T>(T);
unsafe impl<T> Send for AssertSend<T> {}

fn names_of(prog: &A::Program, known: &mut Vec<String>) {
    for s in &prog.body {
        if let A::StmtKind::Assign { name, .. } | A::StmtKind::FuncDef { name, .. } = &s.kind {
            if !known.contains(name) {
                known.push(name.clone());
            }
        }
    }
}

impl Program {
    pub fn compile(source: &str, base_dir: &str, source_name: &str) -> Result<Program, String> {
        let (prog, pdiags) = fermium_syntax::parse(source, &[]).map_err(|e| jdiag("compile", &e, source))?;
        let opts = CheckOptions { base_dir: base_dir.to_string(), repl: false, source_name: source_name.to_string(), no_load: false };
        let mut checker = Checker::new(opts);
        // a program, not a prompt: the REPL's storage model (top-level variables kept between inputs), without
        // its conveniences (echoing bare expressions, redefining a variable with other units)
        checker.arena = true;
        let prog = Box::new(prog);
        let module = checker.check_program(&prog).map_err(|e| jdiag("compile", &e, source))?;
        checker.module = module.clone();
        let mut known = vec![];
        names_of(&prog, &mut known);
        let warnings = pdiags.warnings.iter().chain(checker.diags.warnings.iter())
            .map(|w| w.format(Some(source), None)).collect();
        Ok(Program { checker, state: ReplState::new(), programs: vec![prog], known,
                     main: module, ran: false, calls: HashMap::new(), warnings })
    }

    fn run_module(state: &mut ReplState, module: &I::Module) -> (String, Result<(), fermium_codegen::eval::RunError>) {
        let mut buf: Vec<u8> = vec![];
        let r = {
            let mut printer = StdPrinter::new(module, &mut buf);
            state.run(module, &mut printer)
        };
        (String::from_utf8_lossy(&buf).into_owned(), r)
    }

    /// Run the top level once (on the first call or read, or explicitly). Ok(printed text) or Err(JSON error).
    fn ensure_ran(&mut self, force: bool) -> Result<String, (String, String)> {
        if self.ran && !force {
            return Ok(String::new());
        }
        self.ran = true;
        let (out, r) = Self::run_module(&mut self.state, &self.main);
        match r {
            Ok(()) => Ok(out),
            Err(e) => Err((out, jerror("runtime", &e.message, (e.line > 0).then_some(e.line), e.hint.as_deref(), ""))),
        }
    }

    pub fn functions(&self) -> Vec<String> {
        self.checker.global_names().into_iter()
            .filter(|(n, b)| match b {
                Binding::Func(f) => !n.starts_with("__") && !n.contains('\'') && self.checker.funcs[*f].fdef.is_some(),
                _ => false,
            })
            .map(|(n, _)| n)
            .collect()
    }

    pub fn variables(&self) -> Vec<String> {
        self.checker.global_names().into_iter()
            .filter(|(n, b)| match b {
                Binding::Sym(s) => !n.starts_with("__") && !n.starts_with(PREFIX)
                    && self.checker.module.syms.get(*s).is_some_and(|y| y.storage == I::Storage::Arena),
                _ => false,
            })
            .map(|(n, _)| n)
            .collect()
    }

    /// A value as JSON (api.py `_read`).
    fn value_json(&self, sym: I::SymId) -> String {
        let c = &self.checker;
        let Some(s) = c.module.syms.get(sym) else { return jerror("type", "no such variable", None, None, "") };
        let v = self.state.value(sym);
        let nums = |v: Option<&Value>| -> Vec<f64> {
            match v {
                Some(Value::Num(x)) => vec![*x],
                Some(Value::List(l)) => l.borrow().clone(),
                Some(Value::Vec(xs)) | Some(Value::Arr(xs)) => xs.to_vec(),
                Some(Value::UList(l)) => l.borrow().iter().map(Value::num).collect(),
                Some(other) => vec![other.num()],
                None => vec![],
            }
        };
        let num = |d: &DExpr, t: &str, vals: String, extra: String| {
            let dim = c.u.resolve(d);
            format!("{{\"t\":{},\"v\":{vals},\"dim\":{},\"unit\":{}{extra}}}", jstr(t), jstr(&dim_key(&dim)),
                    junit(&display_unit(&dim, s.hint.as_ref())))
        };
        match &s.ty {
            Ty::Num(d) => {
                let x = nums(v).first().copied().unwrap_or(f64::NAN);
                num(d, "num", jnum(x), String::new())
            }
            Ty::List(d) => num(d, "list", jnums(&nums(v)), String::new()),
            Ty::Complex(d) => {
                let p = nums(v);
                num(d, "complex", jnums(&p), String::new())
            }
            Ty::Vec { n, dim, dims } => {
                let p = nums(v);
                let ds: Vec<Dim> = match (dims, dim) {
                    (Some(ds), _) => ds.iter().map(|d| c.u.resolve(d)).collect(),
                    (None, Some(d)) => vec![c.u.resolve(d); *n],
                    _ => vec![fermium_ir::DIMLESS; *n],
                };
                let comps: Vec<String> = ds.iter().map(|d| {
                    format!("{{\"dim\":{},\"unit\":{}}}", jstr(&dim_key(d)), junit(&display_unit(d, s.hint.as_ref())))
                }).collect();
                format!("{{\"t\":\"vec\",\"v\":{},\"comps\":[{}]}}", jnums(&p), comps.join(","))
            }
            Ty::Mat { r, c: cols, dim } => num(dim, "mat", jnums(&nums(v)), format!(",\"r\":{r},\"c\":{cols}")),
            Ty::Bool => {
                let b = matches!(v, Some(Value::Bool(true)));
                format!("{{\"t\":\"bool\",\"v\":{b}}}")
            }
            other => jerror("type", &format!("{} is {}, which can't be passed to Python yet", s.name, other.kind()), None,
                            None, ""),
        }
    }

    pub fn read(&mut self, name: &str) -> String {
        let sym = match self.checker.global(name) {
            Some(Binding::Sym(s)) if self.checker.module.syms[s].storage == I::Storage::Arena => s,
            _ => return "{\"missing\":true}".into(),
        };
        let out = match self.ensure_ran(false) {
            Ok(o) => o,
            Err((o, e)) => return with_out(&e, &o),
        };
        format!("{{\"out\":{},\"value\":{}}}", jstr(&out), self.value_json(sym))
    }

    pub fn run(&mut self) -> String {
        match self.ensure_ran(true) {
            Ok(o) => format!("{{\"out\":{}}}", jstr(&o)),
            Err((o, e)) => with_out(&e, &o),
        }
    }

    /// The parameters of a function: name, declared unit (dimension), for the Python side's checks.
    pub fn params(&self, name: &str) -> String {
        let Some(Binding::Func(f)) = self.checker.global(name) else { return "{\"missing\":true}".into() };
        let Some(A::Stmt { kind: A::StmtKind::FuncDef { params, .. }, .. }) = &self.checker.funcs[f].fdef else {
            return "{\"missing\":true}".into();
        };
        let mut ps = vec![];
        for p in params {
            let unit = p.unit.as_ref().and_then(|u| self.checker.resolve_unit(u).ok());
            ps.push(format!("{{\"name\":{},\"unit\":{}}}", jstr(&p.name),
                            unit.as_ref().map(junit).unwrap_or_else(|| "null".into())));
        }
        format!("{{\"params\":[{}]}}", ps.join(","))
    }

    fn compile_call(&mut self, name: &str, kinds: &[(bool, Dim)]) -> Result<CallEntry, String> {
        let snap = (self.checker.clone(), self.known.clone());
        let tag = format!("{}_{name}", self.calls.len());
        let mut syms = vec![];
        let mut hidden = vec![];
        for (k, (is_list, dim)) in kinds.iter().enumerate() {
            let n = format!("{PREFIX}{tag}_a{k}");
            let ty = if *is_list { Ty::List(DExpr::of(*dim)) } else { Ty::Num(DExpr::of(*dim)) };
            let c = &mut self.checker;
            let id = c.module.syms.len();
            c.module.syms.push(I::Sym { name: n.clone(), ty, storage: I::Storage::Arena, func: None, sf: None,
                                        hint: None, direct: 0, slot: None });
            c.extra.push(SymExtra { assigned: true, ..Default::default() });
            let g = c.globals;
            c.bind(g, &n, Binding::Sym(id));
            syms.push(id);
            hidden.push(n);
        }
        let rname = format!("{PREFIX}{tag}_r");
        let text = format!("{rname} = {name}({})\n", hidden.join(", "));
        let mut known = self.known.clone();
        known.extend(hidden.iter().cloned());
        let fail = |this: &mut Program, d: Diagnostic| {
            this.checker = snap.0.clone();
            this.known = snap.1.clone();
            let what: Vec<String> = kinds.iter().map(|(l, d)| {
                let t = if d.is_dimensionless() { "a plain number".to_string() } else { fermium_units::dim_name(d) };
                if *l { format!("a list of {t}") } else { t }
            }).collect();
            let msg = format!("calling {name} from Python with ({}): {}", what.join(", "), d.message);
            jerror("compile", &msg, None, d.hint.as_deref(), &msg)
        };
        let prog = match fermium_syntax::parse(&text, &known) {
            Ok((p, _)) => Box::new(p),
            Err(d) => return Err(fail(self, d)),
        };
        self.checker.diags = Default::default();
        let r = self.checker.check_program(&prog);
        self.programs.push(prog);
        let module = match r {
            Ok(m) => m,
            Err(d) => return Err(fail(self, d)),
        };
        self.checker.module = module.clone();
        self.known = known;
        let Some(Binding::Sym(result)) = self.checker.global(&rname) else {
            return Err(jerror("type", &format!("{name} doesn't return a value"), None, None, ""));
        };
        let rty = &self.checker.module.syms[result].ty;
        if !matches!(rty, Ty::Num(_) | Ty::List(_) | Ty::Vec { .. } | Ty::Mat { .. } | Ty::Bool | Ty::Complex(_)) {
            return Err(jerror("type", &format!("{name} returns {}, which can't be passed to Python yet", rty.kind()),
                              None, None, ""));
        }
        Ok(CallEntry { module, args: syms, result })
    }

    /// Call function `name` with arguments (is a list, dimension, SI values).
    pub fn call(&mut self, name: &str, args: Vec<(bool, Dim, Vec<f64>)>) -> String {
        match self.checker.global(name) {
            Some(Binding::Func(_)) => {}
            _ => return "{\"missing\":true}".into(),
        }
        let key = format!("{name}|{}", args.iter().map(|(l, d, _)| format!("{}{}", if *l { "l" } else { "n" },
                                                                          dim_key(d))).collect::<Vec<_>>().join("|"));
        if !self.calls.contains_key(&key) {
            let kinds: Vec<(bool, Dim)> = args.iter().map(|(l, d, _)| (*l, *d)).collect();
            match self.compile_call(name, &kinds) {
                Ok(e) => {
                    self.calls.insert(key.clone(), e);
                }
                Err(j) => return j,
            }
        }
        let mut out = match self.ensure_ran(false) {
            Ok(o) => o,
            Err((o, e)) => return with_out(&e, &o),
        };
        let entry = &self.calls[&key];
        for ((is_list, _, vals), sym) in args.into_iter().zip(&entry.args) {
            let v = if is_list {
                Value::List(std::rc::Rc::new(std::cell::RefCell::new(vals)))
            } else {
                Value::Num(vals.first().copied().unwrap_or(f64::NAN))
            };
            self.state.set(*sym, v);
        }
        let (o, r) = Self::run_module(&mut self.state, &entry.module);
        out += &o;
        let result = entry.result;
        match r {
            Ok(()) => format!("{{\"out\":{},\"value\":{}}}", jstr(&out), self.value_json(result)),
            Err(e) => with_out(&jerror("runtime", &e.message, (e.line > 0).then_some(e.line), e.hint.as_deref(), ""),
                               &out),
        }
    }
}

/// Add the printed text to an error's JSON.
fn with_out(err: &str, out: &str) -> String {
    format!("{},\"out\":{}}}", &err[..err.len() - 1], jstr(out))
}

// ---------------------------------------------------------------- the C ABI
fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
}

fn give(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).unwrap().into_raw()
}

/// Free a text returned by any fermium_* function.
///
/// # Safety
/// `s` must come from this library and be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Check and compile a program. Returns the program (null on error); `*json` receives
/// `{"warnings": [...]}` or `{"error": {...}}`.
///
/// # Safety
/// Pointers must be valid NUL-terminated UTF-8 texts; `json` a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_compile(src: *const c_char, base_dir: *const c_char, name: *const c_char,
                                         json: *mut *mut c_char) -> *mut Program {
    let (src, base, name) = (cstr(src).to_string(), cstr(base_dir).to_string(), cstr(name).to_string());
    let r = on_big_stack(move || AssertSend(Program::compile(&src, &base, &name)));
    match r.0 {
        Ok(p) => {
            let ws: Vec<String> = p.warnings.iter().map(|w| jstr(w)).collect();
            unsafe { *json = give(format!("{{\"warnings\":[{}]}}", ws.join(","))) };
            Box::into_raw(Box::new(p))
        }
        Err(e) => {
            unsafe { *json = give(e) };
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `p` must come from fermium_compile and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_release(p: *mut Program) {
    if !p.is_null() {
        let b = unsafe { Box::from_raw(p) };
        let b = AssertSend(b);
        on_big_stack(move || drop(b));
    }
}

fn with_prog(p: *mut Program, f: impl FnOnce(&mut Program) -> String + Send) -> *mut c_char {
    if p.is_null() {
        return give(jerror("type", "no program", None, None, ""));
    }
    let prog = AssertSend(p);
    give(on_big_stack(move || {
        let prog = prog;
        f(unsafe { &mut *prog.0 })
    }))
}

/// `{"functions": [...], "variables": [...]}`
///
/// # Safety
/// `p` must come from fermium_compile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_names(p: *mut Program) -> *mut c_char {
    with_prog(p, |p| {
        let js = |v: Vec<String>| v.iter().map(|s| jstr(s)).collect::<Vec<_>>().join(",");
        format!("{{\"functions\":[{}],\"variables\":[{}]}}", js(p.functions()), js(p.variables()))
    })
}

/// Run the top level (again): `{"out": printed}` or an error.
///
/// # Safety
/// `p` must come from fermium_compile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_run(p: *mut Program) -> *mut c_char {
    with_prog(p, |p| p.run())
}

/// Read a top-level variable (running the top level first if it hasn't run).
///
/// # Safety
/// `p` must come from fermium_compile; `name` a valid text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_read(p: *mut Program, name: *const c_char) -> *mut c_char {
    let name = cstr(name).to_string();
    with_prog(p, move |p| p.read(&name))
}

/// The parameters of a function (name and declared unit).
///
/// # Safety
/// `p` must come from fermium_compile; `name` a valid text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_params(p: *mut Program, name: *const c_char) -> *mut c_char {
    let name = cstr(name).to_string();
    with_prog(p, move |p| p.params(&name))
}

/// Call a function. Argument k is a list when `lens[k] >= 0` (that many values), else a number (one value);
/// the values follow each other in `data`; `dims[k]` is its dimension key.
///
/// # Safety
/// `p` must come from fermium_compile; the arrays must hold `nargs` entries and `data` all the values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_call(p: *mut Program, name: *const c_char, nargs: i64, dims: *const *const c_char,
                                      lens: *const i64, data: *const f64) -> *mut c_char {
    let name = cstr(name).to_string();
    let mut args = vec![];
    let mut at = 0usize;
    for k in 0..nargs.max(0) as usize {
        let key = cstr(unsafe { *dims.add(k) });
        let Some(dim) = parse_dim_key(key) else {
            return give(jerror("type", &format!("bad dimension {key:?}"), None, None, ""));
        };
        let n = unsafe { *lens.add(k) };
        let count = if n < 0 { 1 } else { n as usize };
        let vals = if count == 0 { vec![] } else { unsafe { std::slice::from_raw_parts(data.add(at), count) }.to_vec() };
        at += count;
        args.push((n >= 0, dim, vals));
    }
    with_prog(p, move |p| p.call(&name, args))
}

/// A unit by name: `{"name", "factor", "offset", "dim"}` or `{"error": ...}`.
///
/// # Safety
/// `name` must be a valid text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_unit(name: *const c_char) -> *mut c_char {
    let name = cstr(name);
    give(match fermium_units::parse_unit_string(name) {
        Ok(u) => junit(&u),
        Err(e) => jerror("unit", &e.message, None, None, ""),
    })
}

/// The preferred unit of a dimension, and the dimension's name ('length [m]').
///
/// # Safety
/// `key` must be a valid text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_dim(key: *const c_char) -> *mut c_char {
    let Some(d) = parse_dim_key(cstr(key)) else { return give(jerror("type", "bad dimension", None, None, "")) };
    give(format!("{{\"unit\":{},\"name\":{}}}", junit(&fermium_units::preferred_unit(&d)),
                 jstr(&fermium_units::dim_name(&d))))
}

/// A number as Fermium prints it with `sig` significant figures (units.py format_number).
#[unsafe(no_mangle)]
pub extern "C" fn fermium_format_number(x: f64, sig: i64) -> *mut c_char {
    give(fermium_units::numfmt::format_number(x, sig, true))
}

/// A value in unit `src` passed for a parameter declared in unit `dst`: the Hz ↔ rpm warning (D95, D202), as
/// `[message, hint]`, or null.
///
/// # Safety
/// Both must be valid texts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fermium_hz_mixup(src: *const c_char, dst: *const c_char) -> *mut c_char {
    let hint = |n: &str| fermium_units::parse_unit_string(n).ok().map(|u| I::Hint { name: u.name.clone(),
                                                                                    factor: u.factor,
                                                                                    offset: u.offset, dim: u.dim });
    let (Some(s), Some(d)) = (hint(cstr(src)), hint(cstr(dst))) else { return give("null".into()) };
    give(match fermium_check::convert::hz_angle_mixup(Some(&s), &d) {
        Some((m, h)) => format!("[{},{}]", jstr(&m), jstr(&h)),
        None => "null".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_keys_round_trip() {
        let d = fermium_units::parse_unit_string("m^(1/2) kg/s²").unwrap().dim;
        assert_eq!(dim_key(&d), "1/2,1,-2,0,0,0,0");
        assert_eq!(parse_dim_key(&dim_key(&d)), Some(d));
    }

    #[test]
    fn compile_call_and_read() {
        let mut p = Program::compile("g = 9.81 m/s²\nprint \"top\"\nperiod(L [m]) = 2π √(L / g)\ndouble(x) = 2 x\n",
                                     ".", "<python>").unwrap();
        assert_eq!(p.functions(), vec!["double", "period"]);
        assert_eq!(p.variables(), vec!["g"]);
        let len = fermium_units::parse_unit_string("m").unwrap().dim;
        let r = p.call("period", vec![(false, len, vec![1.0])]);
        assert!(r.starts_with("{\"out\":\"top\\n\",\"value\":{\"t\":\"num\",\"v\":2.0060"), "{r}");
        assert!(r.contains("\"name\":\"s\""), "{r}");
        let time = fermium_units::parse_unit_string("s").unwrap().dim;
        let r = p.call("period", vec![(false, time, vec![1.0])]);
        assert!(r.contains("calling period from Python with (time [s]): period expects L in m (length [m]), but got \
                            time [s]"), "{r}");
        let r = p.call("double", vec![(true, len, vec![1.0, 2.0])]);
        assert!(r.contains("\"t\":\"list\",\"v\":[2.0,4.0]") && r.contains("\"name\":\"m\""), "{r}");
        assert!(p.read("g").contains("\"v\":9.81"));
        assert_eq!(p.read("nope"), "{\"missing\":true}");
    }
}
