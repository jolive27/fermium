//! Eigenvalue problems `solve … lowest N` (D82): a port of check_eigen in `fermium/m3solve.py`.
use fermium_ir as I;
use fermium_syntax::ast as A;

use crate::checker::*;

impl Checker {
    pub fn check_eigen(&mut self, s: &A::Stmt, _sv: &A::Solve, _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("an eigenvalue problem", s.span))
    }
}
