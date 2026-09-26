//! 1-D PDEs `solve ∂u/∂t = … for x from a to b, t from t0 to t1` (D83): a port of check_pde, pde_call and
//! their helpers in `fermium/m3solve.py`.
use fermium_ir as I;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;

impl Checker {
    pub fn check_pde(&mut self, s: &A::Stmt, _sv: &A::Solve, _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("a PDE", s.span))
    }

    pub fn is_pde_name(&self, name: &str, ctx: &Ctx) -> bool {
        matches!(self.lookup(ctx.scope, name), Some((Binding::Pde(_), _)))
    }

    /// A PDE solution used by its bare name (Python e_Name).
    pub fn pde_as_value(&self, p: usize, name: &str, e: &A::Expr) -> Diagnostic {
        let v = &self.solve.pdes[p];
        self.err(format!("{name} is a solution of a PDE, a function of {x} and {t}: write {name}({x}, {t}), like \
                          {name}(0.5 m, 1 s)", x = v.xname, t = v.tname), e.span, None)
    }

    pub fn pde_call(&mut self, _name: &str, e: &A::Expr, _ctx: &mut Ctx, _deriv: bool) -> CResult<Checked> {
        Err(self.not_ported("a PDE solution", e.span))
    }
}
