//! Calls of C and Fortran functions at run time (`import c` / `import fortran`, C3, D275): the `ccall` built-in,
//! whose first argument is the call site's index in `tables.ccalls`. The LLVM back end calls the function directly
//! when every argument and the result are doubles, and otherwise reaches this code through its built-in callback
//! (so do executables made by `fermium build`), so the conversions and messages are the same everywhere.
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use fermium_ir::{CCallSite, CParamKind};
use fermium_runtime::cffi::{self, CArg, CRet};
use fermium_runtime::numerics::pde::fmt_g;

use crate::eval::{Interpreter, Printer, RunError, Value};

/// One argument as it arrives: a number or a list.
enum In {
    Num(f64),
    List(Vec<f64>),
}

/// The SI value → the number the function gets (÷ the declared unit's factor; exact when the factor is 1).
#[inline]
fn to_c(x: f64, fac: f64) -> f64 {
    if fac != 1.0 { x / fac } else { x }
}

/// Call site `site` once with numbers `nums` (by parameter; unused for lists and lengths) and lists `lists`.
fn call_once(site: &CCallSite, fp: *const c_void, nums: &[f64], lists: &[Option<Vec<f64>>]) -> Result<f64, String> {
    let name = &site.display;
    let n = site.params.len();
    // storage that stays put while the call runs (Fortran takes everything by reference)
    let mut dbl = vec![0f64; n];
    let mut int = vec![0i32; n];
    let mut bufs: Vec<Vec<f64>> = vec![vec![]; n];
    for (k, p) in site.params.iter().enumerate() {
        match p.kind {
            CParamKind::Num => dbl[k] = to_c(nums[k], p.fac),
            CParamKind::Int => {
                let x = to_c(nums[k], p.fac);
                if x.fract() != 0.0 || !x.is_finite() {
                    return Err(format!("{name}: {} must be a whole number (it is passed as an int), not {}", p.name,
                                       fmt_g(x, 6)));
                }
                if x.abs() > i32::MAX as f64 {
                    return Err(format!("{name}: {} is {}, too large for a C int", p.name, fmt_g(x, 6)));
                }
                int[k] = x as i32;
            }
            CParamKind::List => {
                bufs[k] = lists[k].as_ref().map(|xs| xs.iter().map(|x| to_c(*x, p.fac)).collect()).unwrap_or_default();
            }
            CParamKind::Len => {
                let len = lists[p.len_of].as_ref().map(Vec::len).unwrap_or(0);
                if len > i32::MAX as usize {
                    return Err(format!("{name}: the list {} is too long for a C int length", site.params[p.len_of].name));
                }
                int[k] = len as i32;
            }
        }
    }
    let args: Vec<CArg> = site
        .params
        .iter()
        .enumerate()
        .map(|(k, p)| match (p.kind, site.by_ref) {
            (CParamKind::Num, false) => CArg::F64(dbl[k]),
            (CParamKind::Num, true) => CArg::Ptr(&dbl[k] as *const f64 as *const c_void),
            (CParamKind::Int | CParamKind::Len, false) => CArg::I32(int[k]),
            (CParamKind::Int | CParamKind::Len, true) => CArg::Ptr(&int[k] as *const i32 as *const c_void),
            (CParamKind::List, _) => CArg::Ptr(bufs[k].as_ptr() as *const c_void),
        })
        .collect();
    let ret = if site.rint { CRet::I32 } else { CRet::F64 };
    // SAFETY: the symbol was found in the library when the program was checked, and the signature the program
    // declares is the contract (as in C, a wrong declaration is the program's error)
    let r = unsafe { cffi::call(fp, &args, ret) }?.num();
    if site.cpp {
        cpp_error(site)?;
    }
    Ok(if site.rfac != 1.0 { r * site.rfac } else { r })
}

/// After a call of a C++ wrapper (C4, D290): the exception the C++ function threw, if it threw one. The wrapper
/// catches it (an exception must not unwind into Fermium), keeps its message and returns 0;
/// `int fermium_cpp_error(char *buf, int n)` copies the message (empty: no exception) and clears it.
fn cpp_error(site: &CCallSite) -> Result<(), String> {
    let Ok(Some(ef)) = cffi::symbol(&site.lib, "fermium_cpp_error") else {
        return Err(format!("{}: the C++ wrapper {} has no fermium_cpp_error (delete it so that it is made again)",
                           site.display, site.lib));
    };
    let mut buf = [0u8; 512];
    let args = [CArg::Ptr(buf.as_mut_ptr() as *const c_void), CArg::I32(buf.len() as i32)];
    // SAFETY: the wrapper Fermium generated defines it with this signature and writes at most n bytes
    let n = unsafe { cffi::call(ef as *const c_void, &args, CRet::I32) }?.num() as usize;
    if n == 0 {
        return Ok(());
    }
    let msg = String::from_utf8_lossy(&buf[..n.min(buf.len() - 1)]).into_owned();
    Err(format!("{}: {msg}", site.display))
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn ccall(&mut self, args: &[Value]) -> Result<Value, RunError> {
        let tables = &self.module.tables;
        let id = args.first().map(Value::num).unwrap_or(-1.0);
        let Some(site) = (id >= 0.0).then(|| tables.ccalls.get(id as usize)).flatten() else {
            return self.err("calling a C function failed: unknown call site");
        };
        let fp = match cffi::symbol(&site.lib, &site.symbol) {
            Ok(Some(p)) => p as *const c_void,
            Ok(None) => return self.err(format!("the library {} has no function {} any more", site.lib, site.symbol)),
            Err(why) => return self.err(format!("can't load the library {}: {why}", site.lib)),
        };
        // the arguments by parameter (lengths are filled in)
        let mut given = args[1..].iter();
        let mut ins: Vec<Option<In>> = Vec::with_capacity(site.params.len());
        for p in &site.params {
            if p.kind == CParamKind::Len {
                ins.push(None);
                continue;
            }
            let v = given.next().ok_or_else(|| RunError { message: "calling a C function: too few arguments".into(),
                                                          line: self.line, hint: None })?;
            ins.push(Some(match v {
                Value::List(l) => In::List(l.borrow().clone()),
                Value::UList(l) => In::List(l.borrow().iter().map(Value::num).collect()),
                Value::Arr(l) => In::List(l.to_vec()),
                other => In::Num(other.num()),
            }));
        }
        let name = &site.display;
        // lists passed with one length must have that length
        if let Some(first) = site.params.iter().find(|p| p.kind == CParamKind::Len) {
            let lk = first.len_of;
            let want = match &ins[lk] {
                Some(In::List(xs)) => xs.len(),
                _ => 0,
            };
            for (k, p) in site.params.iter().enumerate() {
                if p.kind != CParamKind::List || site.params.iter().any(|q| q.kind == CParamKind::Len && q.len_of == k) {
                    continue;
                }
                if let Some(In::List(xs)) = &ins[k] {
                    if xs.len() != want {
                        let msg = format!("{name}: {} has {} element{}, but {} has {}; they are passed with one length \
                                           ({})", p.name, xs.len(), if xs.len() == 1 { "" } else { "s" },
                                          site.params[lk].name, want, first.name);
                        return self.err(msg);
                    }
                }
            }
        }
        let lists: Vec<Option<Vec<f64>>> = site
            .params
            .iter()
            .zip(&ins)
            .map(|(p, a)| match (p.kind, a) {
                (CParamKind::List, Some(In::List(xs))) => Some(xs.clone()),
                _ => None,
            })
            .collect();
        let scalar = |k: usize, i: usize| -> f64 {
            match &ins[k] {
                Some(In::Num(x)) => *x,
                Some(In::List(xs)) => xs.get(i).copied().unwrap_or(f64::NAN),
                None => 0.0,
            }
        };
        if !site.map {
            let nums: Vec<f64> = (0..site.params.len()).map(|k| scalar(k, 0)).collect();
            return match call_once(site, fp, &nums, &lists) {
                Ok(x) => Ok(Value::Num(x)),
                Err(m) => self.err(m),
            };
        }
        // a list for a number parameter: one call per element, the lists the same length
        let mut n: Option<(usize, &str)> = None;
        for (k, p) in site.params.iter().enumerate() {
            if matches!(p.kind, CParamKind::Num | CParamKind::Int) {
                if let Some(In::List(xs)) = &ins[k] {
                    match n {
                        None => n = Some((xs.len(), &p.name)),
                        Some((m, other)) if m != xs.len() => {
                            let msg = format!("{name}: the lists {other} and {} have different lengths ({m} and {})",
                                              p.name, xs.len());
                            return self.err(msg);
                        }
                        _ => {}
                    }
                }
            }
        }
        let n = n.map(|x| x.0).unwrap_or(0);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let nums: Vec<f64> = (0..site.params.len()).map(|k| scalar(k, i)).collect();
            match call_once(site, fp, &nums, &lists) {
                Ok(x) => out.push(x),
                Err(m) => return self.err(m),
            }
        }
        Ok(Value::List(Rc::new(RefCell::new(out))))
    }
}
