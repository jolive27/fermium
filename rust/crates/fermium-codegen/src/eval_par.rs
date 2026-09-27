//! parallel for, run serially (D152, interp.parallel_for): block by block (fermium_ir::par_blocks, the blocks
//! the compiled code hands to its threads), each sum starting from 0 in each block and the blocks' sums added in
//! order, so the numbers are exactly those of the compiled, multi-threaded loop.
use fermium_ir::{par_blocks, ParInfo, Stmt, SymId};

use crate::eval::{Flow, Frame, Interpreter, Printer, RunError, Value};

impl<'m, P: Printer> Interpreter<'m, P> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn parallel_for(&mut self, info: &ParInfo, sym: SymId, lo: f64, st: f64, n: usize, body: &[Stmt],
                               fr: &mut Frame) -> Result<Flow, RunError> {
        for &(w, o, text) in &info.alias {
            if let (Value::List(a), Value::List(b)) = (self.get(w, fr)?, self.get(o, fr)?) {
                if std::rc::Rc::ptr_eq(&a, &b) {
                    let t = self.module.tables.texts[text].clone();
                    return self.err(format!("{t} are the same list (one was set from the other), so the iterations of \
                                             this parallel for would write and read the same numbers at the same \
                                             time; make a copy first, e.g.  ys = xs * 1"));
                }
            }
        }
        let reds = &info.reductions;
        let mut acc: Vec<f64> = reds.iter().map(|r| self.get(*r, fr).map(|v| v.num())).collect::<Result<_, _>>()?;
        for (a, b) in par_blocks(n) {
            for r in reds {
                self.set(*r, Value::Num(0.0), fr);
            }
            for i in a..b {
                self.set(sym, Value::Num(lo + i as f64 * st), fr);
                match self.block(body, fr)? {
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    _ => continue,
                }
            }
            for (k, r) in reds.iter().enumerate() {
                acc[k] += self.get(*r, fr)?.num();
            }
        }
        for (k, r) in reds.iter().enumerate() {
            self.set(*r, Value::Num(acc[k]), fr);
        }
        Ok(Flow::Normal)
    }
}
