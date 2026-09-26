//! Checker methods not ported yet: each returns the honest "not supported yet" error. As each group is
//! ported (see the table in checker.rs), its stubs move out of this file into the module that owns them.
use fermium_ir as I;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;

impl Checker {
    // ---- stmts / parallel
    #[allow(clippy::too_many_arguments)]
    pub fn parallel_for(&mut self, s: &A::Stmt, _var: &str, _lo: I::Expr, _hi: I::Expr, _st: Option<I::Expr>,
                        _body: &[A::Stmt], _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("parallel for", s.span))
    }
    pub fn seed_stmt(&mut self, e: &A::Expr, _ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        Err(self.not_ported("seed", e.span))
    }
    // ---- vectors and matrices
    /// Indexing: only the vector/matrix/complex-list branches are ported (vecmat.rs); lists come with e_Index.
    pub fn e_index(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        if let Some(r) = self.index_matrix_entry(e, ctx)? {
            return Ok(r);
        }
        let A::ExprKind::Index { target, .. } = &e.kind else { unreachable!() };
        if let Checked::Val(t) = self.expr_any(target, ctx)? {
            if let Some(r) = self.index_vecmat(e, &t, ctx)? {
                return Ok(r);
            }
        }
        Err(self.not_ported("indexing", e.span))
    }
    /// Fields of ODE solutions and data tables (not ported yet).
    pub fn field_other(&mut self, e: &A::Expr, _target: &A::Expr, _name: &str, _t: Checked, _ctx: &mut Ctx)
                       -> CResult<Checked> {
        Err(self.not_ported("a field", e.span))
    }
    // ---- complex numbers
    // ---- solutions
    pub fn sol_values(&mut self, _view: SolViewId, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("the values of an ODE solution", e.span))
    }
    pub fn sol_names(&self, _sol: usize) -> Vec<String> {
        vec![]
    }
    // ---- unit systems (D60)
    pub fn natural(&self) -> bool {
        !self.nat.is_empty()
    }
    pub fn nat_label(&self) -> String {
        "SI".into()
    }
    pub fn nat_name(&self) -> String {
        "SI".into()
    }
    pub fn nat_display(&self) -> &str {
        ""
    }
    pub fn natural_const(&mut self, _c: &ConstInfo, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.not_ported("constants in natural units", e.span))
    }
    pub fn from_system(&mut self, _sym: I::SymId, r: I::Expr, _node: &A::Expr) -> CResult<I::Expr> {
        Ok(r)
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
    pub fn e_list_lit(&mut self, e: &A::Expr, items: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        if let Some(m) = self.list_lit_matrix(e, items, ctx)? {
            return Ok(m);
        }
        Err(self.not_ported("a list", e.span))
    }
    pub fn e_convert(&mut self, e: &A::Expr, _v: &A::Expr, _u: &A::UnitExpr, _ctx: &mut Ctx) -> CResult<I::Expr> {
        Err(self.not_ported("in (unit conversion)", e.span))
    }
}

impl Checker {
    // ---- arithmetic helpers not ported yet
    /// dx/dt written as a fraction: a derivative (calculus module).
    pub fn leibniz(&mut self, _e: &A::Expr, _ctx: &mut Ctx) -> Option<A::Expr> {
        None
    }
    pub fn warn_limit_division(&mut self, _e: &A::Expr, _b: &I::Expr) {}
    pub fn warn_confusable_sum(&mut self, _op: &str, _a: &I::Expr, _b: &I::Expr, _e: &A::Expr) {}
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
    /// Built-ins: only the vector/matrix/complex/FFT ones are ported (vecmat.rs builtin_vecmat, row_column).
    pub fn builtin(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        if name == "row" || name == "column" {
            return self.row_column(name, e, ctx).map(Checked::Val);
        }
        let A::ExprKind::Call { args: arg_asts, .. } = &e.kind else { unreachable!() };
        let mut args = vec![];
        for a in arg_asts {
            match self.expr_any(a, ctx)? {
                Checked::Val(v) => args.push(v),
                Checked::Sol(view) => args.push(self.sol_values(view, a)?),
                Checked::Func { info, .. } => {
                    let dn = self.funcs[info].display_name.clone();
                    return Err(self.err(format!("{dn} is a function; give it an argument"), a.span, None));
                }
            }
        }
        if let Some(r) = self.builtin_vecmat(name, args, e, ctx)? {
            return Ok(Checked::Val(r));
        }
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
    pub fn nat_contains(&self, other: &str) -> bool {
        other.is_empty() || other == self.nat
    }
    pub fn system_name(&self, sys: &str) -> String {
        sys.to_string()
    }
    pub fn system_consts(&self, sys: &str) -> String {
        sys.to_string()
    }
    /// C.stabilize: a numerically stable form of a derivative's body (calculus module).
    pub fn stabilize(&self, e: &A::Expr) -> A::Expr {
        e.clone()
    }
}
