//! The evaluator, random numbers and FFT (a port of the matching parts of fermium/interp.py and the runtime).
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
#![allow(unused_imports, unused_variables)]
use fermium_ir::{Expr, ExprKind, Stmt, StmtKind};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn builtin_m3(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        None
    }
}
