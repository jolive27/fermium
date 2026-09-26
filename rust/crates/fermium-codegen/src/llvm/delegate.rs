//! Mixed mode: a statement or expression the LLVM back end doesn't compile itself (a plot, a fit, loading data, a
//! function applied to a list, …) is handed to the tree-walker's own code, with the variables it reads copied in
//! and the ones it sets copied back, so the rest of the program still runs compiled (red team round 10: one plot
//! used to send a whole program to the tree-walker). The tree-walker instance is the context's, printing through
//! the same printer, so what the program prints comes out in the same order.
use fermium_ir::{Expr, Stmt, SymId};

use super::rt::{locked, Ctx, FmList, Kind, C};
use crate::eval::{Frame, RunError, Value};

/// A construct run by the tree-walker: the IR node (its address in the module, which outlives the run), the
/// variables it reads or sets (with their kinds, in the order of the env the compiled code passes), the ones it
/// sets, and the kind of its value (expressions).
pub struct InterpSite {
    pub ptr: usize,
    pub is_stmt: bool,
    pub syms: Vec<(SymId, Kind)>,
    pub writes: Vec<SymId>,
    pub ret: Kind,
}

/// The value in a variable slot of kind k, as the tree-walker's Value.
unsafe fn read_slot(c: &Ctx, k: Kind, p: *const u8) -> Value {
    match k {
        Kind::F => Value::Num(*(p as *const f64)),
        Kind::B => Value::Bool(*p & 1 != 0),
        Kind::S | Kind::H | Kind::Obj => c.to_value(k, *(p as *const u64)),
        Kind::L | Kind::TL => c.to_value(k, *(p as *const *const u8) as u64),
        Kind::V(_) => c.to_value(k, p as u64),
        Kind::Void => Value::Void,
    }
}

/// Store a Value into a variable slot of kind k.
unsafe fn write_slot(c: &mut Ctx, k: Kind, p: *mut u8, v: Value) {
    match k {
        Kind::F => *(p as *mut f64) = v.num(),
        Kind::B => *p = u8::from(v.truth()),
        Kind::V(_) => c.from_value(k, v, p as *mut u64),
        Kind::L | Kind::TL => {
            let mut slot = 0u64;
            c.from_value(k, v, &mut slot);
            if slot != 0 {
                *(p as *mut *mut FmList) = slot as *mut FmList;
            }
        }
        _ => c.from_value(k, v, p as *mut u64),
    }
}

/// Run site `site` in the tree-walker; its value goes to `out` (expressions). Returns the program line the
/// tree-walker ended on (the compiled code carries on from it, as the tree-walker would).
#[no_mangle]
pub extern "C" fn fm_interp(c: C, site: i64, env: *const *mut u8, out: *mut u64, line: i32) -> i32 {
    locked(c, |c| {
        let (ptr, is_stmt, syms, writes, ret) = {
            let s = &c.interp_sites[site as usize];
            (s.ptr, s.is_stmt, s.syms.clone(), s.writes.clone(), s.ret)
        };
        let mut fr = Frame::default();
        for (i, (sym, k)) in syms.iter().enumerate() {
            let v = unsafe { read_slot(c, *k, *env.add(i)) };
            if c.module.syms[*sym].func.is_none() {
                c.interp.globals.insert(*sym, v);
            } else {
                fr.vars.insert(*sym, v);
            }
        }
        c.interp.line = line.max(0) as u32;
        let r = if is_stmt {
            c.interp.stmt(unsafe { &*(ptr as *const Stmt) }, &mut fr).map(|_| Value::Void)
        } else {
            c.interp.eval(unsafe { &*(ptr as *const Expr) }, &mut fr)
        };
        match r {
            Err(e) => c.fail(e),
            Ok(v) => {
                for w in &writes {
                    let Some(i) = syms.iter().position(|(s, _)| s == w) else { continue };
                    let nv = fr.vars.get(w).cloned().or_else(|| c.interp.globals.get(w).cloned());
                    if let Some(nv) = nv {
                        unsafe { write_slot(c, syms[i].1, *env.add(i), nv) };
                    }
                }
                if !is_stmt {
                    if super::rt::kind_matches(ret, &v) {
                        unsafe { c.from_value(ret, v, out) };
                    } else {
                        let line = c.interp.line;
                        c.fail(RunError { message: format!("internal error in the LLVM back end: the tree-walker gave \
                                                            {v:?} where a {ret:?} value was expected (run with \
                                                            --backend interp)"), line, hint: None });
                    }
                }
            }
        }
        c.interp.line as i32
    })
}

/// Print a value the compiled code holds as an opaque tree-walker Value (a list of complex numbers).
#[no_mangle]
pub extern "C" fn fm_print_obj(c: C, fmt: i64, obj: i64) {
    let c = unsafe { &mut *c };
    let v = c.obj(obj);
    if let Value::CList(l) = v {
        let items = l.borrow().clone();
        c.printer_mut().clist(fmt as usize, &items);
    }
}
