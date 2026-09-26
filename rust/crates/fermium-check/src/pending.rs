//! Checker methods not ported yet: each returns the honest "not supported yet" error. As each group is
//! ported (see the table in checker.rs), its stubs move out of this file into the module that owns them.
use fermium_ir as I;
use fermium_syntax::ast as A;

use crate::checker::*;

impl Checker {
    // ---- stmts / parallel
    pub fn seed_stmt(&mut self, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("seed", e.span))
    }
    // ---- vectors and matrices
    /// Fields of ODE solutions and data tables (not ported yet).
    pub fn field_other(&mut self, e: &A::Expr, _target: &A::Expr, _name: &str, _t: Checked, _ctx: &mut Ctx)
                       -> CResult<Checked> {
        Err(self.not_ported("a field", e.span))
    }
}

impl Checker {
    pub fn data_description(&self, _v: &I::Expr) -> String {
        "data".into()
    }
}

impl Checker {
    // ---- calls: pieces owned by other modules
    pub fn py_ref_of(&mut self, _target: &A::Expr, _ctx: &mut Ctx) -> Option<usize> {
        None
    }
    pub fn python_call(&mut self, _pref: usize, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported("calling Python", e.span))
    }
    pub fn err_call(&mut self, e: &A::Expr, _args: &[A::Expr], _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported("err(…)", e.span))
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
    /// value(x), uncertainty(x), rel(x) of an uncertain value (D121).
    pub fn unc_part(&mut self, name: &str, e: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported(&format!("the built-in {name}"), e.span))
    }
    /// rand(), rand(a, b), randn(), randn(μ, σ) (D80).
    pub fn m3_random(&mut self, name: &str, _args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported(&format!("the built-in {name}"), e.span))
    }
    /// sample(dist, n) (D80).
    pub fn m3_sample(&mut self, e: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("the built-in sample", e.span))
    }
}
