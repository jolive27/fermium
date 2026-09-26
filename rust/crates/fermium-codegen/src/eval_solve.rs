//! The evaluator, ODE/eigen/PDE solutions (a port of the matching parts of fermium/interp.py and the runtime).
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
#![allow(unused_imports, unused_variables)]
use fermium_ir::{Expr, ExprKind, Stmt, StmtKind};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn eval_solution(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        self.err("this isn't supported by the Rust back end yet")
    }
    pub(crate) fn stmt_solve(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        self.err("solve, fit, plot and animate aren't supported by the Rust back end yet")
    }
}
