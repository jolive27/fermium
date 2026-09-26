//! Calls into Python at run time (`use python`, D140): the `pycall` built-in, whose first argument is the call
//! site's index in `tables.pycalls`. The LLVM back end reaches it through its built-in callback, so both back ends
//! convert values and word errors identically (fermium_runtime::python, a port of v1's runtime/pycall.py).
use std::cell::RefCell;
use std::rc::Rc;

use fermium_runtime::python as py;

use crate::eval::{Interpreter, Printer, RunError, Value};

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn pycall(&mut self, args: &[Value]) -> Result<Value, RunError> {
        let tables = &self.module.tables;
        let id = args.first().map(Value::num).unwrap_or(-1.0);
        let Some(site) = (id >= 0.0).then(|| tables.pycalls.get(id as usize)).flatten() else {
            return self.err("calling Python failed: unknown call site");
        };
        let lists: Vec<Option<Vec<f64>>> = args[1..]
            .iter()
            .map(|a| match a {
                Value::List(l) => Some(l.borrow().clone()),
                Value::UList(l) => Some(l.borrow().iter().map(Value::num).collect()),
                Value::Arr(l) => Some(l.to_vec()),
                _ => None,
            })
            .collect();
        let pargs: Vec<py::Arg> = args[1..]
            .iter()
            .zip(&lists)
            .map(|(a, l)| match l {
                Some(xs) => py::Arg::List(xs),
                None => py::Arg::Num(a.num()),
            })
            .collect();
        let s = py::CallSite {
            module: &site.module,
            func: &site.func,
            display: &site.display,
            facs: &site.facs,
            ints: &site.ints,
            pnames: &site.pnames,
            rlist: site.rlist,
            rfac: site.rfac,
            declared: site.declared,
        };
        match py::call(&s, &pargs, &tables.py_base_dir) {
            Ok(py::PyValue::Num(x)) => Ok(Value::Num(x)),
            Ok(py::PyValue::List(xs)) => Ok(Value::List(Rc::new(RefCell::new(xs)))),
            Err(m) => self.err(m),
        }
    }
}
