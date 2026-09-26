//! The checker's core: scopes, bindings, contexts, errors and the statement/expression dispatch.
//! A port of the first part of `fermium/checker.py` (class Checker and its helpers).
//!
//! The rest of the checker is split by topic into sibling modules, each an `impl Checker` block that mirrors a
//! group of methods of the Python class (same names, snake_case), so the two can be diffed:
//!
//! | module       | Python methods                                                                  |
//! |--------------|---------------------------------------------------------------------------------|
//! | `stmts`      | s_ExprStmt … s_Assert, assign_to, push_stmt, entry_assign, loops, parallel for  |
//! | `print`      | s_Print, print_items, text, fmt, fmt_components, describe_function               |
//! | `exprs`      | expr, e_Num, e_Quantity, e_Name, var_ref, undefined, e_Compare, e_If, e_Where …  |
//! | `arith`      | e_BinOp, arith, power, e_Neg, hints and significant figures, temperature warnings |
//! | `vecmat`     | vectors and matrices, indexing and slices                                        |
//! | `calls`      | e_Call, call_user, instantiate, builtin, list/map helpers                        |
//! | `calculus`   | e_Prime, e_Deriv, e_VecCalc, e_Integral, e_Sum (with fermium-sym)                 |
//! | `solve`      | s_Solve, s_Fit, s_Plot, load, table, fields of solutions (solve.py, m3solve.py)   |
//! | `systems`    | s_Units, natural units, s_Analyze                                                 |
//! | `modules`    | import, use python (importer.py, pyinterop.py)                                   |
//! | `uncertain`  | ±, propagate montecarlo, value/uncertainty/rel                                   |
use std::collections::HashMap;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty, Unifier};
use fermium_ir::{Dim, DIMLESS};
use fermium_syntax::ast as A;
use fermium_syntax::diag::{Diagnostic, Diagnostics};

use crate::units::{self, Unit};

pub type CResult<T> = Result<T, Diagnostic>;

pub type ScopeId = usize;
pub type FuncInfoId = usize;
pub type SolViewId = usize;
pub type LocalFuncId = usize;

/// A built-in constant in the root scope.
#[derive(Clone, Debug)]
pub struct ConstInfo {
    pub name: String,
    pub value: f64,
    pub unit: Unit,
    pub desc: String,
}

/// A user function (Python FuncInfo): its definition, and the monomorphic instances made so far.
#[derive(Clone, Debug)]
pub struct FuncInfo {
    pub name: String,
    pub fdef: Option<A::Stmt>,
    pub scope: ScopeId,
    /// instance key (the argument types, printed) → the IR function
    pub instances: HashMap<String, I::FuncId>,
    pub display_name: String,
    pub checked_generic: bool,
    /// a derivative: evaluate the stabilised body (A52)
    pub stable: bool,
    /// the unit system it was defined in (None: SI, callable anywhere; D60)
    pub nat: Option<crate::systems::Sys>,
    /// set by modules: the module it came from (index into Checker.mods.modules)
    pub module: Option<usize>,
    /// printed instead of name(params) for an unnamed function: d/dt (3t²), ∫ x dx
    pub anon_label: Option<String>,
    /// a derivative: (the base function, the parameter index, the order)
    pub parent: Option<(FuncInfoId, usize, u32)>,
}

impl FuncInfo {
    pub fn one_liner(&self) -> bool {
        matches!(&self.fdef, Some(A::Stmt { kind: A::StmtKind::FuncDef { body: A::FuncBody::Expr(_), .. }, .. }))
    }
}

/// A view of one component of an ODE solution: `x` after `solve x'' = … with …` (Python SolView).
#[derive(Clone, Debug)]
pub struct SolView {
    pub sol_sym: I::SymId,
    pub comp: usize,
    pub top: usize,
    pub dim: DExpr,
    pub tdim: DExpr,
    pub tname: String,
    pub name: String,
    pub n: usize,
    pub stride: usize,
    /// a complex unknown: two slots per derivative (D93)
    pub cplx: bool,
    /// display units: of this derivative, of each derivative order, of the time; significant figures
    pub hint: Option<I::Hint>,
    pub hints: Vec<Option<I::Hint>>,
    pub thint: Option<I::Hint>,
    pub sf: Option<u32>,
}

/// A one-line helper defined inside a function (D194), expanded at each call.
#[derive(Clone, Debug)]
pub struct LocalFunc {
    pub fdef: A::Stmt,
    pub scope: ScopeId,
    pub owner: String,
    pub expanding: bool,
}

/// What a name is bound to.
#[derive(Clone, Debug)]
pub enum Binding {
    Sym(I::SymId),
    Const(ConstInfo),
    Func(FuncInfoId),
    Sol(SolViewId),
    Local(LocalFuncId),
    /// an imported module (importer.py ModuleRef); index into the module table
    Module(usize),
    /// `use python numpy as np` (pyinterop.py PyModRef)
    PyModule(usize),
    /// the unknown of a PDE after its solve (m3solve.py PdeView); index into Checker::solve.pdes
    Pde(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub parent: Option<ScopeId>,
    pub names: HashMap<String, Binding>,
    pub kind: &'static str,
}

/// Who owns new locals: the main program or an IR function instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Main,
    Func(I::FuncId),
}

/// Where we are (Python Ctx): which function (or lambda) owns new locals, and its scope.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub func: Owner,
    pub scope: ScopeId,
    pub is_main: bool,
    pub lam: Option<I::LambdaId>,
    pub loop_depth: u32,
    pub branch: u32,
    /// shared with child contexts: the return types seen (index into Checker::ret_types)
    pub ret_types: usize,
    /// definite assignment: the if/loop regions we are inside, innermost last (Python ctx.regions)
    pub regions: Vec<RegionId>,
    /// the enclosing lambdas' ids, outermost first (Python walks ctx.parent for captures)
    pub lam_parents: Vec<I::LambdaId>,
}

pub type RegionId = usize;

/// An if/while/for being checked, for "x might not have a value here" (Python _enter_region).
#[derive(Clone, Debug)]
pub struct Region {
    pub kind: &'static str,
    pub line: u32,
    pub new: Vec<I::SymId>,
    pub assigned: Vec<std::collections::HashSet<I::SymId>>,
    pub parent: Option<RegionId>,
}

impl Ctx {
    pub fn child(&self, scope: ScopeId) -> Ctx {
        Ctx { scope, regions: vec![], ..self.clone() }
    }
}

/// Checker-side facts about a symbol the IR doesn't carry (Python sets them as attributes on I.Sym).
#[derive(Clone, Debug, Default)]
pub struct SymExtra {
    pub assigned: bool,
    /// the unit system it was made in (D60)
    pub nat: crate::systems::Sys,
    /// holds a temperature difference (D181)
    pub tdelta: bool,
    pub par_private: bool,
    pub region: Option<RegionId>,
    /// why the variable may have no value here, if it may not
    pub unset_msg: Option<String>,
    pub fresh_loop_var: bool,
    pub list_sf: Option<u32>,
    /// the display units of a mixed vector, one per component (Python sets a MixedHint as sym.hint)
    pub mixed_hint: Option<Vec<Option<fermium_ir::Hint>>>,
}

/// What the result of checking an expression can be: a value, or a function/solution used by name.
#[derive(Clone, Debug)]
pub enum Checked {
    Val(I::Expr),
    Func { info: FuncInfoId, name: String, param: bool },
    Sol(SolViewId),
}

#[derive(Clone, Debug, Default)]
pub struct CheckOptions {
    pub base_dir: String,
    pub repl: bool,
    pub source_name: String,
}

pub struct Checker {
    pub diags: Diagnostics,
    pub u: Unifier,
    pub opts: CheckOptions,
    pub uses_unc: bool,
    pub arena: bool,
    pub scopes: Vec<Scope>,
    pub root: ScopeId,
    pub globals: ScopeId,
    pub funcs: Vec<FuncInfo>,
    pub sols: Vec<SolView>,
    pub local_funcs: Vec<LocalFunc>,
    pub module: I::Module,
    pub extra: Vec<SymExtra>,
    pub ret_types: Vec<Vec<I::Expr>>,
    /// checker facts about each IR function instance (index = FuncId)
    pub func_extra: Vec<crate::calls::FuncExtra>,
    /// FuncInfos made for built-ins passed as functions (simpson(sin, …), D43)
    pub builtin_infos: HashMap<String, FuncInfoId>,
    /// module functions being instantiated from inside their module (D101)
    pub in_module_call: std::collections::HashSet<FuncInfoId>,
    /// errors that already carry the "this happened when calling …" note
    pub call_noted: std::collections::HashSet<String>,
    pub counter: usize,
    pub main_count: usize,
    /// names of top-level functions defined anywhere in the program, with their line (used-before-defined)
    pub future_funcs: HashMap<String, u32>,
    /// the unit system in force (SI by default; `units natural(ħ = c = 1)`, D60)
    pub nat: crate::systems::Sys,
    /// unit-system bookkeeping (systems.rs)
    pub sys: crate::systems::SysState,
    /// modules imported in this compilation (modules.rs)
    pub mods: crate::modules::ModState,
    pub positive_names: std::collections::HashSet<String>,
    pub used_consts: std::collections::HashSet<String>,
    pub warned: std::collections::HashSet<String>,
    /// the expressions being checked, outermost first (for error hints, D163); see Checker::expr_any
    pub estack: Vec<*const A::Expr>,
    pub regions: Vec<Region>,
    /// products whose left side is itself a product (Python sets `in_product` on the AST node)
    pub in_product: std::collections::HashSet<*const A::Expr>,
    /// checking the name right after a number (`3 sec`): the unit hint for undefined names (#63)
    pub after_number: bool,
    /// checking the callee of a call (undefined-name suggestions include the M3 built-ins)
    pub calling: bool,
    /// the length of each `10 °C` written out, by (line, col), for warnings pointing at it
    pub abs_at_len: HashMap<(u32, u32), u32>,
    /// every expression of the program by its id (the parser refers to nodes by id: unit_left, …)
    pub nodes: HashMap<u32, *const A::Expr>,
    /// the dimension of each print format, resolved when checking ends
    pub fmt_dims: Vec<DExpr>,
    /// the parallel for loops being checked (M5, D152): (owner, private symbols)
    pub par_stack: Vec<(Owner, Vec<I::SymId>)>,
    /// solutions of ODEs, eigenvalue problems and PDEs (solve.rs)
    pub solve: crate::solve::SolveTables,
    /// calculus: derived functions made so far (calculus.rs)
    pub calc: crate::calculus::CalcState,
}

impl Checker {
    pub fn new(opts: CheckOptions) -> Checker {
        let mut c = Checker {
            diags: Diagnostics::default(),
            u: Unifier::default(),
            arena: opts.repl,
            opts,
            uses_unc: false,
            scopes: vec![],
            root: 0,
            globals: 0,
            funcs: vec![],
            sols: vec![],
            local_funcs: vec![],
            module: I::Module::default(),
            extra: vec![],
            ret_types: vec![],
            func_extra: vec![],
            builtin_infos: HashMap::new(),
            in_module_call: Default::default(),
            call_noted: Default::default(),
            counter: 0,
            main_count: 0,
            future_funcs: HashMap::new(),
            nat: Default::default(),
            sys: Default::default(),
            mods: Default::default(),
            positive_names: Default::default(),
            used_consts: Default::default(),
            warned: Default::default(),
            estack: vec![],
            regions: vec![],
            in_product: Default::default(),
            after_number: false,
            calling: false,
            abs_at_len: HashMap::new(),
            par_stack: vec![],
            fmt_dims: vec![],
            nodes: HashMap::new(),
            solve: Default::default(),
            calc: Default::default(),
        };
        c.root = c.new_scope(None, "root");
        for k in units::constants() {
            let info = ConstInfo { name: k.name.clone(), value: k.value, unit: k.unit.clone(),
                                   desc: k.description.to_string() };
            c.scopes[c.root].names.insert(k.name.clone(), Binding::Const(info));
        }
        // the imaginary unit 𝑖 (also written as the literal 1i); use_binding gives it its complex value (D90)
        c.scopes[c.root].names.insert(
            "𝑖".into(),
            Binding::Const(ConstInfo { name: "𝑖".into(), value: f64::NAN, unit: Unit::one(),
                                       desc: "the imaginary unit, 𝑖² = -1".into() }),
        );
        c.globals = c.new_scope(Some(c.root), "global");
        c
    }

    // ============================================================ helpers
    pub fn new_scope(&mut self, parent: Option<ScopeId>, kind: &'static str) -> ScopeId {
        self.scopes.push(Scope { parent, names: HashMap::new(), kind });
        self.scopes.len() - 1
    }

    pub fn lookup(&self, scope: ScopeId, name: &str) -> Option<(Binding, ScopeId)> {
        let mut s = Some(scope);
        while let Some(id) = s {
            if let Some(b) = self.scopes[id].names.get(name) {
                return Some((b.clone(), id));
            }
            s = self.scopes[id].parent;
        }
        None
    }

    pub fn bind(&mut self, scope: ScopeId, name: &str, b: Binding) {
        self.scopes[scope].names.insert(name.to_string(), b);
    }

    pub fn fresh_name(&mut self, base: &str) -> String {
        self.counter += 1;
        format!("{base}.{}", self.counter)
    }

    /// An error at a node (Python Checker.err).
    pub fn err(&self, msg: impl Into<String>, span: A::Span, hint: Option<String>) -> Diagnostic {
        let mut d = Diagnostic::error(msg, span.line, span.col, span.length.max(1), hint);
        if span.line == 0 {
            d.line = None;
        }
        if span.col == 0 {
            d.col = None;
        }
        d
    }

    pub fn warn(&mut self, msg: impl Into<String>, span: A::Span, hint: Option<String>) {
        self.diags.warn(Diagnostic::warning(msg, span.line, span.col, span.length.max(1), hint));
    }

    pub fn desc(&self, d: &DExpr) -> String {
        if self.nat.natural() {
            return self.nat.describe(&self.u.resolve(d)); // U.namer inside a natural region (D60)
        }
        units::dim_name(&self.u.resolve(d))
    }

    pub fn unify_or(&mut self, a: &DExpr, b: &DExpr, msg: impl FnOnce(&Self) -> String, span: A::Span,
                    hint: Option<String>) -> CResult<()> {
        if !self.u.unify(a, b) {
            return Err(self.err(msg(self), span, hint));
        }
        Ok(())
    }

    /// 'a list of energy [J]', 'a 3-vector of …' for messages (types.py type_desc).
    pub fn type_desc(&self, t: &Ty) -> String {
        match t {
            Ty::Num(d) => self.desc(d),
            Ty::List(d) => format!("a list of {}", self.desc(d)),
            Ty::Complex(d) => {
                if self.u.resolve(d).is_dimensionless() {
                    "a complex number".into()
                } else {
                    format!("a complex number of {}", self.desc(d))
                }
            }
            Ty::Vec { n, dims: Some(ds), .. } => {
                format!("a {n}-vector of ({})", ds.iter().map(|d| self.desc(d)).collect::<Vec<_>>().join(", "))
            }
            Ty::Vec { n, dim, .. } => format!("a {n}-vector of {}", self.desc(dim.as_ref().unwrap())),
            Ty::Mat { r, c, dim } => format!("a {r}×{c} matrix of {}", self.desc(dim)),
            Ty::TextList => "a list of text".into(),
            Ty::ComplexList(d) => {
                if self.u.resolve(d).is_dimensionless() {
                    "a list of complex numbers".into()
                } else {
                    format!("a list of complex numbers of {}", self.desc(d))
                }
            }
            Ty::Bool => "true/false value".into(),
            Ty::Str => "text".into(),
            Ty::Sol(_) => "ODE solution".into(),
            Ty::Data(_) => "data table".into(),
            Ty::Void => "nothing".into(),
        }
    }

    /// The unit written in the program, in SI dimensions (Python resolve_unit_si).
    pub fn resolve_unit_si(&self, uexpr: &A::UnitExpr) -> CResult<Unit> {
        let mut total: Option<Unit> = None;
        let one = num_rational::Rational64::from_integer(1);
        for f in &uexpr.factors {
            let Some(mut u) = units::lookup_unit(&f.name) else {
                let sugg = match f.name.as_str() {
                    "h" => "h is Planck's constant, not the hour; for hours write hr".to_string(),
                    "t" => "for metric tons write tonne".to_string(),
                    n => match fermium_units::spelled_unit(n) {
                        Some(sym) => format!("Fermium writes units as symbols: {sym}"),
                        None => crate::convert::unit_name_suggestion(n),
                    },
                };
                let sugg = if sugg.is_empty() { "see the units list in docs/reference.md".to_string() } else { sugg };
                let n = f.name.chars().count() as u32;
                return Err(Diagnostic::error(format!("'{}' is not a unit Fermium knows", f.name), f.span.line,
                                             f.span.col, n, Some(sugg)));
            };
            if u.affine() && (uexpr.factors.len() > 1 || f.exp != one) {
                // in a compound unit (°C/min, J/(g °C)) a degree is a temperature step: K-sized, no offset
                u = Unit::new(u.name.clone(), u.dim, u.factor);
            }
            if f.exp != one {
                u = u.pow(f.exp);
            }
            total = Some(match total {
                None => u,
                Some(t) => t.mul(&u),
            });
        }
        let mut total = total.unwrap_or_else(Unit::one);
        let canon = crate::convert::canonical_unit_name(uexpr);
        if !canon.is_empty() {
            total.name = canon;
        }
        Ok(total)
    }

    /// The unit mapped into the system in force (natural units map it into powers of energy, D60).
    pub fn resolve_unit(&self, uexpr: &A::UnitExpr) -> CResult<Unit> {
        Ok(self.nat.canon_unit(&self.resolve_unit_si(uexpr)?))
    }

    // ============================================================ symbols
    pub fn owner_func(&self, ctx: &Ctx) -> Option<I::FuncId> {
        match ctx.func {
            Owner::Main => None,
            Owner::Func(f) => Some(f),
        }
    }

    /// The parallel for being checked, if new variables made here belong to its iterations.
    pub fn par_here(&self, ctx: &Ctx) -> Option<usize> {
        match self.par_stack.last() {
            Some((owner, _)) if ctx.lam.is_none() && *owner == ctx.func => Some(self.par_stack.len() - 1),
            _ => None,
        }
    }

    pub fn new_sym(&mut self, name: &str, ty: Ty, ctx: &Ctx) -> I::SymId {
        let par = self.par_here(ctx);
        let storage = if ctx.is_main && self.arena && ctx.lam.is_none() && par.is_none() {
            I::Storage::Arena
        } else {
            I::Storage::Local
        };
        let id = self.module.syms.len();
        self.module.syms.push(I::Sym { name: name.to_string(), ty, storage, func: self.owner_func(ctx), sf: None,
                                       hint: None, direct: 0, slot: None });
        self.extra.push(SymExtra { nat: self.nat.clone(), par_private: par.is_some(), ..Default::default() });
        if let Some(p) = par {
            self.par_stack[p].1.push(id);
        }
        match (ctx.lam, ctx.func) {
            (Some(l), _) => self.module.lambdas[l].locals.push(id),
            (None, Owner::Func(f)) => self.module.funcs[f].locals.push(id),
            (None, Owner::Main) => {}
        }
        id
    }

    pub fn sym(&self, id: I::SymId) -> &I::Sym {
        &self.module.syms[id]
    }

    // ============================================================ program
    pub fn check_program(&mut self, prog: &A::Program) -> CResult<I::Module> {
        self.main_count += 1;
        self.uses_unc = false;
        let ctx = Ctx { func: Owner::Main, scope: self.globals, is_main: true, lam: None, loop_depth: 0, branch: 0,
                        ret_types: self.new_ret_types(), regions: vec![], lam_parents: vec![] };
        let mut ctx = ctx;
        self.nodes.clear();
        self.index_nodes(prog);
        self.positive_names = if self.opts.repl { Default::default() } else { crate::calculus::positive_names(prog) };
        self.note_top_units(prog);
        let g = self.globals;
        self.note_program(prog, g);
        self.future_funcs = prog
            .body
            .iter()
            .filter_map(|s| match &s.kind {
                A::StmtKind::FuncDef { name, .. } => Some((name.clone(), s.span.line)),
                _ => None,
            })
            .collect();
        let main = self.block(&prog.body, &mut ctx)?;
        self.check_uncalled()?;
        self.resolve_fmts();
        self.module.main = main;
        self.module.uses_uncertainty = self.uses_unc;
        Ok(std::mem::take(&mut self.module))
    }

    fn index_nodes(&mut self, prog: &A::Program) {
        let mut tops = vec![];
        crate::walk::all_exprs_in_stmts(&prog.body, &mut tops);
        for t in tops {
            for n in t.walk() {
                self.nodes.entry(n.id).or_insert(n as *const A::Expr);
            }
        }
    }

    /// The program's expression with this id, if it is in the tree.
    pub fn node(&self, id: u32) -> Option<&A::Expr> {
        // SAFETY: the program outlives the checker's use of it (check_program borrows it throughout)
        self.nodes.get(&id).map(|p| unsafe { &**p })
    }

    pub fn new_ret_types(&mut self) -> usize {
        self.ret_types.push(vec![]);
        self.ret_types.len() - 1
    }

    pub fn block(&mut self, stmts: &[A::Stmt], ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let mut out = vec![];
        for s in stmts {
            for mut r in self.stmt(s, ctx)? {
                if r.line == 0 {
                    r.line = s.span.line;
                }
                out.push(r);
            }
        }
        Ok(out)
    }

    /// One statement → zero or more IR statements (Python dispatches on s_<Class>).
    pub fn stmt(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        use A::StmtKind as K;
        match &s.kind {
            K::ExprStmt { value: e } => self.s_expr_stmt(s, e, ctx),
            K::Assign { name, value, op } => self.s_assign(s, name, value, op, ctx),
            K::Print { items } => self.s_print(items, ctx).map(|p| vec![p]),
            K::FuncDef { .. } => self.s_funcdef(s, ctx),
            K::If { cond, then, other } => self.s_if(s, cond, then, other.as_deref(), ctx),
            K::While { cond, body } => self.s_while(s, cond, body, ctx),
            K::For { var, lo, hi, step, body, parallel } => {
                self.s_for(s, var, lo, hi, step.as_ref(), body, *parallel, ctx)
            }
            K::ForIn { var, iterable, body } => self.s_for_in(s, var, iterable, body, ctx),
            K::Return { value: v } => self.s_return(s, v.as_ref(), ctx),
            K::Break => self.s_break(s, ctx),
            K::Continue => self.s_continue(s, ctx),
            K::Assert { cond, message } => self.s_assert(s, cond, message.as_deref(), ctx),
            K::IndexAssign { .. } => self.s_index_assign(s, ctx),
            K::Solve(sv) => self.s_solve(s, sv, ctx),
            K::Analyze { .. } => self.s_analyze(s, ctx),
            K::Import { .. } => self.s_import(s, ctx),
            K::UsePython { .. } => self.s_use_python(s, ctx),
            K::Units { system, consts, body } => self.s_units(s, system, consts, body.as_deref(), ctx),
            _ => Err(self.not_ported(stmt_kind_name(&s.kind), s.span)),
        }
    }

    /// The honest error for a statement or expression the Rust checker doesn't handle yet.
    pub fn not_ported(&self, what: &str, span: A::Span) -> Diagnostic {
        self.err(format!("{what} isn't supported by this version of the Rust compiler yet"), span,
                 Some("run it with the Python implementation (legacy/) for now".into()))
    }

    /// Check the bodies of functions that were never called, so their errors still show (Python check_uncalled).
    pub fn check_uncalled(&mut self) -> CResult<()> {
        // Python walks the global names in the order they were bound: the order of definition
        let mut todo: Vec<(u32, String, FuncInfoId)> = self.scopes[self.globals]
            .names
            .iter()
            .filter_map(|(n, b)| match b {
                Binding::Func(fi) => Some((self.funcs[*fi].fdef.as_ref().map(|f| f.span.line).unwrap_or(0), n.clone(), *fi)),
                _ => None,
            })
            .collect();
        todo.sort();
        for (_, _, b) in todo {
            let f = &self.funcs[b];
            if !f.instances.is_empty() || f.checked_generic || f.module.is_some() {
                continue;
            }
            self.funcs[b].checked_generic = true;
            let Some(fdef) = self.funcs[b].fdef.clone() else { continue };
            let A::StmtKind::FuncDef { params, .. } = &fdef.kind else { continue };
            if !self.funcs[b].one_liner() && params.is_empty() {
                continue;
            }
            if !self.func_param_uses(b).is_empty() {
                continue; // takes a function: checked per call instead (D43)
            }
            let node = crate::ast_ext::mk(A::ExprKind::Name { name: self.funcs[b].name.clone() }, fdef.span);
            let node: &'static A::Expr = Box::leak(Box::new(node));
            let saved_nat = self.nat.clone();
            let fnat = self.funcs[b].nat.clone().unwrap_or_default();
            self.set_system(fnat);
            let args: Vec<Checked> =
                params.iter().map(|_| Checked::Val(ir(I::ExprKind::Const(0.0), Ty::Num(DExpr::fresh()), 0))).collect();
            let r = self.instantiate(b, args, node, false);
            let r = match r {
                Err(e) if e.message.contains("isn't defined") || e.message.contains("used before") => Ok(()),
                Err(e) if e.message.contains("needs a list") => {
                    // total(ys) = sum(ys): takes a list, so check it with lists; if that fails too (some parameters
                    // are numbers), it is checked at each call instead (D142)
                    let args: Vec<Checked> = params
                        .iter()
                        .map(|_| Checked::Val(ir(I::ExprKind::List(vec![]), Ty::List(DExpr::fresh()), 0)))
                        .collect();
                    let _ = self.instantiate(b, args, node, false);
                    Ok(())
                }
                Err(e) => Err(e),
                Ok(_) => Ok(()),
            };
            self.set_system(saved_nat);
            r?;
        }
        Ok(())
    }
}

pub fn stmt_kind_name(k: &A::StmtKind) -> &'static str {
    use A::StmtKind as K;
    match k {
        K::Assign { .. } => "assignment",
        K::IndexAssign { .. } => "setting an element",
        K::FuncDef { .. } => "a function definition",
        K::Print { .. } => "print",
        K::Plot { .. } => "plot",
        K::Solve { .. } => "solve",
        K::Fit { .. } => "fit",
        K::Analyze { .. } => "analyze",
        K::If { .. } => "if",
        K::For { .. } => "for",
        K::ForIn { .. } => "for … in",
        K::While { .. } => "while",
        K::Return { .. } => "return",
        K::Break => "break",
        K::Continue => "continue",
        K::ExprStmt { .. } => "an expression",
        K::Assert { .. } => "assert",
        K::Units { .. } => "units",
        K::Import { .. } => "import",
        K::UsePython { .. } => "use python",
        K::Propagate { .. } => "propagate montecarlo",
    }
}

/// A shorthand for building IR expressions.
pub fn ir(kind: I::ExprKind, ty: Ty, line: u32) -> I::Expr {
    I::Expr { kind, ty, sf: None, hint: None, direct: 0, line, x: None }
}

pub fn num_ty(d: Dim) -> Ty {
    Ty::Num(DExpr::of(d))
}

pub fn dimless_num() -> Ty {
    num_ty(DIMLESS)
}

/// Check a whole program (the public entry point, Python checker.check).
pub fn check(prog: &A::Program, opts: CheckOptions) -> Result<(I::Module, Diagnostics), (Diagnostic, Diagnostics)> {
    let mut c = Checker::new(opts);
    match c.check_program(prog) {
        Ok(m) => Ok((m, std::mem::take(&mut c.diags))),
        Err(e) => Err((e, std::mem::take(&mut c.diags))),
    }
}
