//! The evaluator's dispatch for the expression kinds eval.rs doesn't handle itself: each group lives in its own
//! file (eval_core, eval_vecmat, eval_calc, eval_solve, eval_data, eval_unc).
use fermium_ir::{Expr, ExprKind};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn eval_more(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::Map { .. } => self.eval_core(e, fr),
            ExprKind::VecSet { .. } | ExprKind::VecIndex { .. } => self.eval_vecmat(e, fr),
            ExprKind::Integral { .. } | ExprKind::Sum { .. } | ExprKind::Root { .. } => self.eval_calculus(e, fr),
            ExprKind::SolEval { .. } | ExprKind::SolList { .. } | ExprKind::PdeEval { .. } => self.eval_solution(e, fr),
            ExprKind::Load(_) | ExprKind::Table(_) | ExprKind::Column(..) => self.eval_data(e, fr),
            ExprKind::Uncertain(..) => self.eval_uncertain(e, fr),
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }
}
