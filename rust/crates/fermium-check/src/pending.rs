//! Checker methods not ported yet: each returns the honest "not supported yet" error. As each group is
//! ported (see the table in checker.rs), its stubs move out of this file into the module that owns them.
use fermium_ir as I;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

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
    // ---- solutions
    pub fn sol_values(&mut self, _view: SolViewId, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("the values of an ODE solution", e.span))
    }
    pub fn sol_names(&self, _sol: usize) -> Vec<String> {
        vec![]
    }
    // ---- modules
    pub fn module_as_value(&self, _m: usize, name: &str, e: &A::Expr) -> Diagnostic {
        self.err(format!("{name} is a module, not a value"), e.span, None)
    }
    pub fn py_module_name(&self, _m: usize) -> String {
        "?".into()
    }
    pub fn module_hint(&self, _name: &str, _ctx: &Ctx) -> Option<String> {
        None
    }
}

impl Checker {
    // ---- arithmetic helpers not ported yet
    /// dx/dt written as a fraction: a derivative (calculus module).
    pub fn leibniz(&mut self, _e: &A::Expr, _ctx: &mut Ctx) -> Option<A::Expr> {
        None
    }
}

impl Checker {
    pub fn data_description(&self, _v: &I::Expr) -> String {
        "data".into()
    }
    /// The units of a function for printing it (Python function_units): needs instantiate.
    pub fn function_units(&mut self, _info: FuncInfoId) -> String {
        String::new()
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
    pub fn is_pde_name(&self, _name: &str, _ctx: &Ctx) -> bool {
        false
    }
    pub fn pde_call(&mut self, _name: &str, e: &A::Expr, _ctx: &mut Ctx, _deriv: bool) -> CResult<Checked> {
        Err(self.not_ported("a PDE solution", e.span))
    }
    pub fn err_call(&mut self, e: &A::Expr, _args: &[A::Expr], _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported("err(…)", e.span))
    }
    pub fn sol_eval(&mut self, _view: SolViewId, e: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("evaluating an ODE solution", e.span))
    }
    pub fn module_call(&mut self, _info: FuncInfoId, _args: Vec<Checked>, node: &A::Expr, _cache: bool)
                       -> CResult<I::Expr> {
        Err(self.not_ported("calling a module's function", node.span))
    }
    pub fn module_body_error(&mut self, _e: &mut Diagnostic, _info: FuncInfoId, _node: &A::Expr) {}
    /// C.stabilize: a numerically stable form of a derivative's body (calculus module).
    pub fn stabilize(&self, e: &A::Expr) -> A::Expr {
        e.clone()
    }
}

impl Checker {
    // ---- stubs called by lists.rs, owned by other modules
    /// r[k] of a vector ODE solution: a vector (A19; solutions module).
    pub fn sol_index(&mut self, _view: SolViewId, e: &A::Expr, _index: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("indexing a vector ODE solution", e.span))
    }
    /// zs[k] of a list of complex numbers (clist.index, D243).
    pub fn clist_index(&mut self, _t: I::Expr, _idx: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("indexing a list of complex numbers", e.span))
    }
    /// `d/dt (…) where a = 3` substituted into the derivative (calculus: C.inline_where); None: not that case.
    pub fn where_deriv(&mut self, e: &A::Expr, value: &A::Expr, _b: &[(String, A::Expr)], _ctx: &mut Ctx)
                       -> Option<CResult<Checked>> {
        if matches!(value.kind, A::ExprKind::Deriv { .. }) {
            return Some(Err(self.not_ported("a derivative with where", e.span)));
        }
        None
    }
}

impl Checker {
    // ---- stubs called by builtin.rs, owned by other modules
    /// The built-ins on vectors and matrices (abs, sign of a vector, trace, angle, norm, unit, hat, cross, vec, dot
    /// of vectors, transpose, det, inverse, solve_linear, eigenvalues, eigenvectors, zeros(r, c)).
    pub fn vec_builtin(&mut self, name: &str, _args: Vec<I::Expr>, e: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported(&format!("{name} of vectors and matrices"), e.span))
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
