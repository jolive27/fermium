//! Checker methods not ported yet: each returns the honest "not supported yet" error. As each group is
//! ported (see the table in checker.rs), its stubs move out of this file into the module that owns them.
use fermium_ir as I;
use fermium_ir::types::Ty;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::units::Unit;

impl Checker {
    // ---- stmts / parallel
    pub fn seed_stmt(&mut self, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("seed", e.span))
    }
    // ---- vectors and matrices
    pub fn vec_unify(&mut self, _a: &Ty, _b: &Ty) -> Option<String> {
        Some("vectors aren't supported yet".into())
    }
    pub fn entry_assign(&mut self, _b: I::SymId, s: &A::Stmt, _ctx: &mut Ctx) -> CResult<I::Stmt> {
        Err(self.not_ported("setting a vector or matrix entry", s.span))
    }
    pub fn vec_quantity(&mut self, e: &A::Expr, _value: &A::Expr, _v: I::Expr, _u: &Unit) -> CResult<I::Expr> {
        Err(self.not_ported("a vector or matrix with units", e.span))
    }
    pub fn index_expr(&mut self, index: &A::Expr, _tgt: &I::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("indexing", index.span))
    }
    pub fn e_index(&mut self, e: &A::Expr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("indexing", e.span))
    }
    // ---- complex numbers
    pub fn cplx_quantity(&mut self, _v: I::Expr, _u: &Unit, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("a complex number with units", e.span))
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
    // ---- expressions not ported yet
    pub fn e_where(&mut self, e: &A::Expr, _v: &A::Expr, _b: &[(String, A::Expr)], _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported("where", e.span))
    }
    pub fn e_list_lit(&mut self, e: &A::Expr, _items: &[A::Expr], _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("a list", e.span))
    }
    pub fn cplx_builtin(&mut self, name: &str, _args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported(&format!("{name} of a complex number"), e.span))
    }
    pub fn clist_call(&mut self, name: &str, _args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported(&format!("{name} of a list of complex numbers"), e.span))
    }
}

impl Checker {
    // ---- arithmetic helpers not ported yet
    /// dx/dt written as a fraction: a derivative (calculus module).
    pub fn leibniz(&mut self, _e: &A::Expr, _ctx: &mut Ctx) -> Option<A::Expr> {
        None
    }
    pub fn cplx_arith(&mut self, _op: &str, _a: I::Expr, _b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("complex arithmetic", e.span))
    }
    pub fn mat_arith(&mut self, _op: &str, _a: I::Expr, _b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("matrix arithmetic", e.span))
    }
    pub fn vec_arith(&mut self, _op: &str, _a: I::Expr, _b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("vector arithmetic", e.span))
    }
    pub fn shared_dim(&mut self, _v: &I::Expr, _what: &str, node: &A::Expr) -> CResult<fermium_ir::types::DExpr> {
        Err(self.not_ported("vectors", node.span))
    }
    pub fn cplx_power(&mut self, e: &A::Expr, _a: I::Expr, _b: Option<I::Expr>, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("complex powers", e.span))
    }
    pub fn cplx_compare(&mut self, _op: I::CmpOp, _a: I::Expr, _b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("comparing complex numbers", e.span))
    }
    pub fn cplx_approx(&mut self, _a: I::Expr, _b: I::Expr, e: &A::Expr, _atol: I::Expr, _rtol: I::Expr)
                       -> CResult<I::Expr> {
        Err(self.not_ported("comparing complex numbers", e.span))
    }
}

impl Checker {
    pub fn data_description(&self, _v: &I::Expr) -> String {
        "data".into()
    }
    pub fn mixed_hints(&self, _v: &I::Expr, n: usize) -> Vec<Option<fermium_ir::Hint>> {
        vec![None; n]
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
    pub fn builtin(&mut self, name: &str, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Checked> {
        Err(self.not_ported(&format!("the built-in {name}"), e.span))
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
