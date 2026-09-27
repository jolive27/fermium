//! Checker methods not ported yet: each returns the honest "not supported yet" error. As each group is
//! ported (see the table in checker.rs), its stubs move out of this file into the module that owns them.
use fermium_ir as I;
use fermium_syntax::ast as A;

use crate::checker::*;

impl Checker {
    // ---- stmts / parallel
    // ---- vectors and matrices
}

impl Checker {
}

impl Checker {
    // ---- calls: pieces owned by other modules (none left: Python calls are in pyinterop.rs)
}

impl Checker {
    // ---- stubs called by builtin.rs, owned by other modules
    /// The built-ins on vectors and matrices (abs, sign of a vector, trace, angle, norm, unit, hat, cross, vec, dot
    /// of vectors, transpose, det, inverse, solve_linear, eigenvalues, eigenvectors, zeros(r, c)).
    pub fn vec_builtin(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        match self.builtin_vecmat(name, args, e, ctx)? {
            Some(r) => Ok(r),
            None => Err(self.not_ported(&format!("{name} of vectors and matrices"), e.span)),
        }
    }
}

impl Checker {
}
