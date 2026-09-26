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
    // ---- calls: pieces owned by other modules
    pub fn py_ref_of(&mut self, _target: &A::Expr, _ctx: &mut Ctx) -> Option<usize> {
        None
    }
    pub fn python_call(&mut self, _pref: usize, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported("calling Python", e.span))
    }
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
    /// err(g) of a parameter found by fit: its standard-error variable (the fit module); None: not a fitted
    /// parameter.
    pub fn fit_err(&mut self, _e: &A::Expr, _a0: Option<&A::Expr>, _ctx: &mut Ctx) -> Option<CResult<I::Expr>> {
        None
    }
}
