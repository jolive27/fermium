//! `solve` for differential equations: a port of `fermium/solve.py` (check_solve and its helpers: equations,
//! initial conditions, orders, vector and complex unknowns, `until` (D39), step / method / tolerance /
//! absolute (D160), coupled highest derivatives (D47)) and of the solution machinery of `fermium/checker.py`
//! (SolView, SolRef, sol_eval, _sol_eval_node, sol_values, fields and primes of solutions, sol_names).
//! Eigenvalue problems are in eigen.rs, PDEs in pde.rs.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use fermium_syntax::ast::ExprKind as K;
use fermium_syntax::diag::Diagnostic;
use num_rational::Rational64;

use crate::checker::*;
use crate::stmts::ty_dim;
/// The calculus helpers solve uses (fermium-sym, the port of calculus.py).
#[allow(non_snake_case)]
mod C {
    pub use fermium_sym::build::{at, is_num, mk, name, neg};
    pub use fermium_sym::{isolate, linear_coeffs, map_children};
    pub fn to_source(e: &fermium_syntax::ast::Expr, pretty: bool) -> String {
        fermium_sym::to_source_p(e, pretty)
    }
}
use crate::walk::free_names;

/// What a solution holds (Python SolTy.info).
#[derive(Clone, Debug, Default)]
pub struct SolInfo {
    pub names: Vec<String>,
    pub t: String,
    pub slots: usize,
    pub eigen: bool,
    pub pde: bool,
}

/// The name of a PDE's unknown after the solve (Python PdeView).
#[derive(Clone, Debug)]
pub struct PdeView {
    pub name: String,
    pub sol_sym: I::SymId,
    pub xa_sym: I::SymId,
    pub xb_sym: I::SymId,
    pub m: usize,
    pub ncomp: usize,
    pub udim: DExpr,
    pub xdim: DExpr,
    pub tdim: DExpr,
    pub xname: String,
    pub tname: String,
    pub uhint: Option<I::Hint>,
    pub xhint: Option<I::Hint>,
    pub thint: Option<I::Hint>,
}

/// The checker's tables for solutions.
#[derive(Clone, Debug, Default)]
pub struct SolveTables {
    pub infos: Vec<SolInfo>,
    pub pdes: Vec<PdeView>,
}

/// An error, or "check again with these unknowns complex" (Python _NeedComplex, D93).
pub(crate) enum SErr {
    D(Diagnostic),
    NeedComplex(Vec<String>),
}

impl From<Diagnostic> for SErr {
    fn from(d: Diagnostic) -> Self {
        SErr::D(d)
    }
}

type SResult<T> = Result<T, SErr>;

pub(crate) fn primes(k: usize) -> String {
    "'".repeat(k)
}

fn plural(n: usize) -> &'static str {
    if n != 1 { "s" } else { "" }
}

// ============================================================ AST helpers (solve.py)
/// Collect {name: max order} for x', x'', d/dt x in an equation side (known: functions whose derivative is
/// only applied to an argument, V'(x): values, not unknowns, D210).
pub(crate) fn find_derivs(e: &A::Expr, out: &mut Vec<(String, i64)>, known: &[String]) {
    fn put(out: &mut Vec<(String, i64)>, n: &str, k: i64) {
        match out.iter_mut().find(|x| x.0 == n) {
            Some(x) => x.1 = x.1.max(k),
            None => out.push((n.to_string(), k)),
        }
    }
    if !known.is_empty() {
        if let K::Call { func, args } = &e.kind {
            let tgt = match &func.kind {
                K::Prime { target, .. } => Some(target),
                K::Deriv { operand, .. } => Some(operand),
                _ => None,
            };
            if let Some(K::Name { name }) = tgt.map(|t| &t.kind) {
                if known.contains(name) {
                    for a in args {
                        find_derivs(a, out, known);
                    }
                    return;
                }
            }
        }
    }
    if let K::Prime { target, order } = &e.kind {
        if let K::Name { name } = &target.kind {
            put(out, name, *order);
            return;
        }
    }
    if let K::Deriv { operand, order, .. } = &e.kind {
        if let K::Name { name } = &operand.kind {
            put(out, name, *order);
            return;
        }
    }
    for c in e.children() {
        find_derivs(c, out, known);
    }
}

fn prime_uses(e: &A::Expr, called: &mut Vec<String>, bare: &mut Vec<String>) {
    if let K::Call { func, args } = &e.kind {
        let tgt = match &func.kind {
            K::Prime { target, .. } => Some(target),
            K::Deriv { operand, .. } => Some(operand),
            _ => None,
        };
        if let Some(K::Name { name }) = tgt.map(|t| &t.kind) {
            called.push(name.clone());
            for a in args {
                prime_uses(a, called, bare);
            }
            return;
        }
    }
    match &e.kind {
        K::Prime { target, .. } if target.is_name() => {
            bare.push(target.name().unwrap().to_string());
            return;
        }
        K::Deriv { operand, .. } if operand.is_name() => {
            bare.push(operand.name().unwrap().to_string());
            return;
        }
        _ => {}
    }
    for c in e.children() {
        prime_uses(c, called, bare);
    }
}

impl Checker {
    /// Defined functions (or solutions) whose derivative appears only applied to an argument (D210).
    pub(crate) fn known_called(&self, ctx: &Ctx, sides: &[&A::Expr], keep: &[String]) -> Vec<String> {
        let (mut called, mut bare) = (vec![], vec![]);
        for e in sides {
            prime_uses(e, &mut called, &mut bare);
        }
        let mut out = vec![];
        for n in called {
            if !bare.contains(&n) && !keep.contains(&n) && self.lookup(ctx.scope, &n).is_some() && !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }
}

/// Rewrite d/dt x, dx/dt, d²x/dt² and d/dt (x') as primes.
pub(crate) fn normalize_derivs(e: &A::Expr, tvar: &str) -> A::Expr {
    let e = C::map_children(e, &mut |c| normalize_derivs(c, tvar));
    if let K::Deriv { var, order, operand, .. } = &e.kind {
        if var == tvar {
            if operand.is_name() {
                return C::at(C::mk(K::Prime { target: operand.clone(), order: *order }), &e);
            }
            if let K::Prime { target, order: o2 } = &operand.kind {
                if target.is_name() {
                    return C::at(C::mk(K::Prime { target: target.clone(), order: o2 + order }), &e);
                }
            }
        }
    }
    if let K::BinOp { op, left, right, .. } = &e.kind {
        if op == "/" {
            if let (K::Name { name: l }, K::Name { name: r }) = (&left.kind, &right.kind) {
                if *r == format!("d{tvar}") && l.starts_with('d') && l.chars().count() > 1 && !left.paren {
                    let inner: String = l.chars().skip(1).collect();
                    let n = C::at(C::name(&fermium_syntax::lexer::canonical_name(&inner)), left);
                    return C::at(C::mk(K::Prime { target: Box::new(n), order: 1 }), &e);
                }
            }
        }
    }
    e
}

/// `0.5 b''` with b an unknown: read as 0.5 · b'' (M9).
pub(crate) fn unit_primes(e: &A::Expr, unknowns: &[String]) -> A::Expr {
    let e = C::map_children(e, &mut |c| unit_primes(c, unknowns));
    if let K::Prime { target, order } = &e.kind {
        if let K::Quantity { value, unit, bracket: false } = &target.kind {
            let fs = &unit.factors;
            if fs.len() == 1 && fs[0].exp == Rational64::from_integer(1) && unknowns.contains(&fs[0].name) {
                let p = C::at(C::mk(K::Prime { target: Box::new(C::at(C::name(&fs[0].name), target)), order: *order }), &e);
                return C::at(C::mk(K::BinOp { op: "*".into(), left: value.clone(), right: Box::new(p), implicit: true }),
                             &e);
            }
        }
    }
    e
}

/// The unknowns named by the initial conditions: x(0) = …, x'(0) = …
pub(crate) fn ic_names(initial: &[A::Equation]) -> Vec<String> {
    let mut out = vec![];
    for ic in initial {
        let mut f = match &ic.lhs.kind {
            K::Call { func, .. } => Some(&**func),
            _ => None,
        };
        while let Some(K::Prime { target, .. }) = f.map(|x| &x.kind) {
            f = Some(target);
        }
        if let Some(K::Name { name }) = f.map(|x| &x.kind) {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out
}

fn is_until(mut e: &A::Expr) -> bool {
    while let K::BinOp { op, left, implicit, .. } = &e.kind {
        if matches!(&left.kind, K::Name { name } if name == "until") && (op == "*" || op == "-")
            && (op == "-" || *implicit)
        {
            return true;
        }
        e = left;
    }
    false
}

fn strip_until(e: &A::Expr) -> A::Expr {
    let K::BinOp { op, left, right, implicit } = &e.kind else { return e.clone() };
    if matches!(&left.kind, K::Name { name } if name == "until") {
        return if op == "*" { (**right).clone() } else { C::at(C::neg((**right).clone()), e) };
    }
    C::at(C::mk(K::BinOp { op: op.clone(), left: Box::new(strip_until(left)), right: right.clone(), implicit: *implicit }), e)
}

fn eq(lhs: A::Expr, rhs: A::Expr, span: A::Span) -> A::Equation {
    A::Equation { lhs, rhs, span }
}

/// A name node at a span, for var_ref's error positions.
pub(crate) fn node_at(span: A::Span) -> A::Expr {
    let mut n = C::name("?");
    n.span = span;
    n
}

pub(crate) fn is_const(v: &I::Expr) -> Option<f64> {
    match (&v.kind, &v.ty) {
        (I::ExprKind::Const(x), Ty::Num(_)) => Some(*x),
        _ => None,
    }
}

pub(crate) fn vec_n(t: &Ty) -> usize {
    match t {
        Ty::Vec { n, .. } => *n,
        Ty::Complex(_) => 2,
        _ => 1,
    }
}

pub(crate) fn dim_of(t: &Ty) -> DExpr {
    ty_dim(t).unwrap_or_else(DExpr::fresh)
}

/// A real number as a complex one (imaginary part 0, same units): cplx.promote.
pub(crate) fn promote(v: I::Expr) -> I::Expr {
    if matches!(v.ty, Ty::Complex(_)) {
        return v;
    }
    let d = dim_of(&v.ty);
    let (hint, sf, direct, line) = (v.hint.clone(), v.sf, v.direct, v.line);
    let zero = ir(I::ExprKind::Const(0.0), Ty::Num(d.clone()), line);
    let mut r = ir(I::ExprKind::Vec(vec![v, zero]), Ty::Complex(d), line);
    r.hint = hint;
    r.sf = sf;
    r.direct = direct;
    r
}

impl Checker {
    // ============================================================ helpers shared by the solve forms
    /// A symbol owned by a lambda (params, state), not added to any list (Python I.Sym(…, "local", lam)).
    pub(crate) fn lam_sym(&mut self, name: &str, ty: Ty, ctx: &Ctx) -> I::SymId {
        let id = self.module.syms.len();
        self.module.syms.push(I::Sym { name: name.to_string(), ty, storage: I::Storage::Local,
                                       func: self.owner_func(ctx), sf: None, hint: None, direct: 0, slot: None });
        self.extra.push(SymExtra { nat: self.nat.clone(), assigned: true, ..Default::default() });
        id
    }

    /// A new lambda and the context to check its body in (Python Ctx(lam, Scope(ctx.scope), parent=ctx)).
    pub(crate) fn new_lambda(&mut self, kind: I::LambdaKind, base: &str, ctx: &Ctx) -> (I::LambdaId, Ctx) {
        let name = self.fresh_name(base);
        self.module.lambdas.push(I::Lambda { kind, name, params: vec![], captures: vec![], locals: vec![], body: vec![],
                                             state: vec![], col_syms: vec![], param_syms: vec![] });
        let l = self.module.lambdas.len() - 1;
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut parents = ctx.lam_parents.clone();
        if let Some(p) = ctx.lam {
            parents.push(p);
        }
        let lctx = Ctx { func: ctx.func, scope, is_main: false, lam: Some(l), loop_depth: 0, branch: 0,
                         ret_types: ctx.ret_types, regions: vec![], lam_parents: parents };
        (l, lctx)
    }

    /// The display unit of a value (Python v.hint) as a print format of the time, like ck.fmt(IConst(0)).
    pub(crate) fn fmt_of(&mut self, dim: &DExpr, hint: Option<I::Hint>) -> usize {
        let mut tf = ir(I::ExprKind::Const(0.0), Ty::Num(dim.clone()), 0);
        tf.hint = hint;
        self.fmt(&tf)
    }

    // ============================================================ solve
    pub fn s_solve(&mut self, s: &A::Stmt, sv: &A::Solve, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if sv.lowest.is_some() {
            return self.check_eigen(s, sv, ctx); // an eigenvalue problem (D82)
        }
        if sv.var2.is_some() {
            return self.check_pde(s, sv, ctx); // a PDE in x and t (D83)
        }
        let mut force: Vec<String> = vec![];
        loop {
            match self.check_ode(s, sv, ctx, &force) {
                Ok(r) => return Ok(r),
                Err(SErr::D(d)) => return Err(d),
                Err(SErr::NeedComplex(names)) => {
                    if names.iter().all(|n| force.contains(n)) {
                        return Err(self.err("this equation is complex, but its unknowns can't be made complex", s.span,
                                            None));
                    }
                    for n in names {
                        if !force.contains(&n) {
                            force.push(n);
                        }
                    }
                }
            }
        }
    }

    /// Split the stop condition (D39) off a solve: (equations, initial conditions, until).
    fn take_until(&self, sv: &A::Solve, ctx: &Ctx) -> CResult<(Vec<A::Equation>, Vec<A::Equation>, Option<A::Equation>)> {
        let mut until = sv.until.clone();
        let mut lists = [sv.equations.clone(), sv.initial.clone()];
        if self.lookup(ctx.scope, "until").is_none() {
            for lst in lists.iter_mut() {
                let mut keep = vec![];
                for q in lst.drain(..) {
                    if is_until(&q.lhs) {
                        if until.is_some() {
                            return Err(self.err("a solve can have only one stop condition (until ...)", q.span, None));
                        }
                        until = Some(eq(strip_until(&q.lhs), q.rhs.clone(), q.span));
                    } else {
                        keep.push(q);
                    }
                }
                *lst = keep;
            }
        }
        let [eqs, inits] = lists;
        Ok((eqs, inits, until))
    }

    fn check_ode(&mut self, s: &A::Stmt, sv: &A::Solve, ctx: &mut Ctx, force: &[String]) -> SResult<Vec<I::Stmt>> {
        let t = sv.var.clone();
        let (src_eqs, initial, until) = self.take_until(sv, ctx)?;
        let unknowns = ic_names(&initial);
        let src_eqs: Vec<A::Equation> = src_eqs
            .iter()
            .map(|q| eq(unit_primes(&q.lhs, &unknowns), unit_primes(&q.rhs, &unknowns), q.span))
            .collect();
        let eqs: Vec<A::Equation> =
            src_eqs.iter().map(|q| eq(normalize_derivs(&q.lhs, &t), normalize_derivs(&q.rhs, &t), q.span)).collect();
        let mut orders: Vec<(String, i64)> = vec![];
        let sides: Vec<&A::Expr> = eqs.iter().flat_map(|q| [&q.lhs, &q.rhs]).collect();
        let known = self.known_called(ctx, &sides, &unknowns);
        for q in &eqs {
            find_derivs(&q.lhs, &mut orders, &known);
            find_derivs(&q.rhs, &mut orders, &known);
        }
        let has = |orders: &Vec<(String, i64)>, n: &str| orders.iter().any(|x| x.0 == n);
        if initial.is_empty() && eqs.len() == 1 && until.is_none() && !has(&orders, &t) {
            // I'(θ) of a function or ODE solution that already exists is a value, not an unknown (#3)
            let (mut called, mut bare) = (vec![], vec![]);
            prime_uses(&eqs[0].lhs, &mut called, &mut bare);
            prime_uses(&eqs[0].rhs, &mut called, &mut bare);
            let known = orders.iter().all(|(x, _)| self.lookup(ctx.scope, x).is_some());
            if orders.is_empty() || (known && bare.is_empty()) {
                return Ok(self.check_root(s, sv, ctx)?);
            }
        }
        if let Some(u) = &until {
            if orders.is_empty() {
                return Err(self.err("until (a stop condition) is for differential equations", u.span, None).into());
            }
        }
        if orders.is_empty() {
            return Err(self
                .err("this solve has no derivatives in it, so there's no differential equation to solve", s.span,
                     Some("write e.g.  solve x' = -x / τ  with x(0) = 1 for t from 0 s to 5 s,  or for an equation:  \
                           solve x² = 2 for x from 0 to 2"
                         .into()))
                .into());
        }
        if eqs.len() != orders.len() {
            let names: Vec<&str> = orders.iter().map(|x| x.0.as_str()).collect();
            return Err(self
                .err(format!("this solve has {} equation{} for {} unknown function{} ({}); they must match", eqs.len(),
                             plural(eqs.len()), orders.len(), plural(orders.len()), names.join(", ")),
                     s.span, None)
                .into());
        }
        // time range
        let t0 = self.expr(&sv.lo, ctx)?;
        let t1 = self.expr(&sv.hi, ctx)?;
        self.need_num(&t0, &sv.lo, "the start time")?;
        self.need_num(&t1, &sv.hi, "the end time")?;
        let (d0, d1) = (dim_of(&t0.ty), dim_of(&t1.ty));
        self.unify_or(&d0, &d1, |c| format!("the range goes from {} to {}", c.desc(&d0), c.desc(&d1)), sv.lo.span, None)?;
        let tdim = d0.clone();
        let mut step = None;
        if let Some(st) = &sv.step {
            let v = self.expr(st, ctx)?;
            self.need_num(&v, st, "the step")?;
            let sd = dim_of(&v.ty);
            self.unify_or(&sd, &tdim, |c| format!("the step is {} but {t} is {}", c.desc(&sd), c.desc(&tdim)), st.span,
                          None)?;
            step = Some(v);
        }
        let method = sv.method.clone().unwrap_or_else(|| if step.is_some() { "rk4".into() } else { "rk45".into() })
            .to_lowercase();
        if !matches!(method.as_str(), "rk4" | "rk45" | "radau" | "bdf") {
            return Err(self
                .err(format!("unknown method '{}' (use rk45, rk4 with a step, or radau for stiff equations)",
                             sv.method.clone().unwrap_or_default()), s.span, None)
                .into());
        }
        if matches!(method.as_str(), "radau" | "bdf") && step.is_some() {
            return Err(self
                .err(format!("{method} chooses its own steps: remove  step …  (a fixed step is for rk4)"),
                     sv.step.as_ref().unwrap().span, None)
                .into());
        }
        if method == "rk4" && step.is_none() {
            return Err(self.err("the rk4 method needs a fixed step:  for t from 0 s to 5 s step 0.01 s", s.span, None).into());
        }

        // state layout: for each unknown x of order n: x, x', ..., x^(n-1)
        let names: Vec<String> = orders.iter().map(|x| x.0.clone()).collect();
        let ord = |x: &str| orders.iter().find(|o| o.0 == x).unwrap().1 as usize;
        let dims: Vec<DExpr> = names.iter().map(|_| DExpr::fresh()).collect();
        let di = |x: &str| names.iter().position(|n| n == x).unwrap();
        let mut layout: Vec<(String, usize)> = vec![];
        for x in &names {
            for k in 0..ord(x) {
                layout.push((x.clone(), k));
            }
        }
        let tpow = |k: usize| DExpr::of(DIMLESS).mul(&tdim.pow(Rational64::from_integer(k as i64)));

        // initial conditions
        let mut y0: Vec<((String, usize), I::Expr)> = vec![];
        let mut shape: Vec<(String, usize)> = vec![];
        let mut is_c: Vec<bool> = names.iter().map(|x| force.contains(x)).collect();
        for ic in &initial {
            let mut lhs = ic.lhs.clone();
            if let K::BinOp { op, left, right, .. } = &ic.lhs.kind {
                if op == "/" {
                    if let (K::Name { name: l }, K::Call { func, args }) = (&left.kind, &right.kind) {
                        if matches!(&func.kind, K::Name { name } if *name == format!("d{t}")) && l.starts_with('d') {
                            // dx/dt(0) = ...
                            let inner: String = l.chars().skip(1).collect();
                            let p = C::mk(K::Prime { target: Box::new(C::name(&fermium_syntax::lexer::canonical_name(&inner))),
                                                     order: 1 });
                            lhs = C::at(C::mk(K::Call { func: Box::new(p), args: args.clone() }), &ic.lhs);
                        }
                    }
                }
            }
            let (f, args) = match &lhs.kind {
                K::Call { func, args } if args.len() == 1 => (&**func, args),
                _ => {
                    return Err(self.err("initial conditions look like  x(0) = 1 m  or  x'(0) = 0 m/s", ic.span, None).into())
                }
            };
            let (mut f, mut k) = (f, 0usize);
            if let K::Prime { target, order } = &f.kind {
                k = *order as usize;
                f = target;
            }
            let x = match &f.kind {
                K::Name { name } if names.contains(name) => name.clone(),
                _ => {
                    return Err(self
                        .err(format!("this initial condition isn't for one of the unknowns ({})", names.join(", ")),
                             ic.span, None)
                        .into())
                }
            };
            if k >= ord(&x) {
                return Err(self
                    .err(format!("{x}{}(…) isn't needed: the equation for {x} is order {}", primes(k), ord(&x)), ic.span,
                         None)
                    .into());
            }
            let v = self.expr(&ic.rhs, ctx)?;
            self.need_numlike(&v, &ic.rhs, "an initial value", true)?;
            if matches!(v.ty, Ty::List(_)) {
                return Err(self
                    .err("an initial value must be a number or a vector like <1, 0> m, not a list", ic.rhs.span, None)
                    .into());
            }
            if matches!(v.ty, Ty::Mat { .. }) {
                return Err(self
                    .err("an initial value must be a number or a vector like <1, 0> m, not a matrix", ic.rhs.span, None)
                    .into());
            }
            if matches!(v.ty, Ty::Vec { dims: Some(_), .. }) {
                return Err(self
                    .err("a vector unknown needs the same units in every component (write separate unknowns for \
                          quantities in different units, like x and v)", ic.rhs.span, None)
                    .into());
            }
            if matches!(v.ty, Ty::Complex(_)) {
                is_c[di(&x)] = true;
            }
            let n_here = match &v.ty {
                Ty::Vec { n, .. } => *n,
                _ => 1,
            };
            let have = match shape.iter().find(|s| s.0 == x) {
                Some(s) => s.1,
                None => {
                    shape.push((x.clone(), n_here));
                    n_here
                }
            };
            if have != n_here {
                let what = if n_here == 1 { "number".to_string() } else { format!("{n_here}-vector") };
                return Err(self
                    .err(format!("the initial values of {x} don't match: one is a {have}-vector, another a {what}"),
                         ic.rhs.span, None)
                    .into());
            }
            let want = dims[di(&x)].div(&tpow(k));
            let vd = dim_of(&v.ty);
            if !self.u.unify(&want, &vd) {
                return Err(self
                    .err(format!("{x}{}(…) should be {} but this is {}", primes(k), self.desc(&want), self.desc(&vd)),
                         ic.rhs.span, None)
                    .into());
            }
            let at = self.expr(&args[0], ctx)?;
            self.need_num(&at, &args[0], "the time of the initial condition")?;
            let ad = dim_of(&at.ty);
            if !self.u.unify(&ad, &tdim) {
                return Err(self
                    .err(format!("the initial condition is given at {} but {t} is {}", self.desc(&ad), self.desc(&tdim)),
                         args[0].span, None)
                    .into());
            }
            if let (Some(a), Some(b)) = (is_const(&at), is_const(&t0)) {
                if (a - b).abs() > 1e-12 * (b.abs() + 1e-300) && !(a == 0.0 && b == 0.0) {
                    return Err(self
                        .err(format!("initial conditions must be at the start of the range ({t} = start)"), args[0].span,
                             None)
                        .into());
                }
            }
            match y0.iter_mut().find(|e| e.0 .0 == x && e.0 .1 == k) {
                Some(e) => e.1 = v,
                None => y0.push(((x.clone(), k), v)),
            }
        }
        let shape_of = |shape: &Vec<(String, usize)>, x: &str| shape.iter().find(|s| s.0 == x).map(|s| s.1).unwrap_or(1);
        for i in 0..y0.len() {
            let x = y0[i].0 .0.clone();
            if is_c[di(&x)] {
                if shape_of(&shape, &x) != 1 {
                    return Err(self
                        .err(format!("{x} can't be both complex and a vector (vectors of complex numbers aren't supported \
                                      yet)"), s.span, None)
                        .into());
                }
                let v = std::mem::replace(&mut y0[i].1, ir(I::ExprKind::Const(0.0), Ty::Void, 0));
                y0[i].1 = promote(v); // ψ'(0) = 0 for a complex ψ is 0 + 0i
            }
        }
        for (i, x) in names.iter().enumerate() {
            if is_c[i] {
                match shape.iter_mut().find(|s| s.0 == *x) {
                    Some(s) => s.1 = 2,
                    None => shape.push((x.clone(), 2)),
                }
            }
        }
        let find_y0 = |y0: &Vec<((String, usize), I::Expr)>, x: &str, k: usize| y0.iter().position(|e| e.0 .0 == x && e.0 .1 == k);
        let missing: Vec<String> = layout
            .iter()
            .filter(|(x, k)| find_y0(&y0, x, *k).is_none())
            .map(|(x, k)| format!("{x}{}(start)", primes(*k)))
            .collect();
        if !missing.is_empty() {
            return Err(self
                .err(format!("missing initial condition{}: {}", if missing.len() > 1 { "s" } else { "" }, missing.join(", ")),
                     s.span, Some("add them after 'with', e.g.  with x(0) = 0.1 [m], x'(0) = 0 m/s".into()))
                .into());
        }

        // right-hand side lambda
        let (lam, mut lctx) = self.new_lambda(I::LambdaKind::Ode, "ode", ctx);
        let tsym = self.lam_sym(&t, Ty::Num(tdim.clone()), ctx);
        self.module.lambdas[lam].params = vec![tsym];
        self.bind(lctx.scope, &t, Binding::Sym(tsym));
        let ty_of = |x: &str, k: usize, shape: &Vec<(String, usize)>| -> Ty {
            let d = dims[di(x)].div(&tpow(k));
            if is_c[di(x)] {
                return Ty::Complex(d);
            }
            let n = shape_of(shape, x);
            if n > 1 { Ty::Vec { n, dim: Some(d), dims: None } } else { Ty::Num(d) }
        };
        let mut state_syms: Vec<I::SymId> = vec![];
        for (x, k) in &layout {
            let nm = format!("{x}{}", primes(*k));
            let sym = self.lam_sym(&nm, ty_of(x, *k, &shape), ctx);
            self.module.lambdas[lam].state.push(sym);
            state_syms.push(sym);
            self.bind(lctx.scope, &nm, Binding::Sym(sym));
        }
        // highest derivatives, only for checking the equations as written
        for x in &names {
            let n = ord(x);
            let nm = format!("{x}{}", primes(n));
            let sym = self.lam_sym(&nm, ty_of(x, n, &shape), ctx);
            self.module.lambdas[lam].locals.push(sym);
            self.bind(lctx.scope, &nm, Binding::Sym(sym));
        }
        // conditions on the unknowns (D296): frozen during each RK45 step, their switches located by the solver
        let mut sw = crate::events::Switches::default();
        let eqs: Vec<A::Equation> = if method == "rk45" {
            let dep: std::collections::HashSet<String> = names.iter().cloned().collect();
            eqs.iter()
                .map(|q| {
                    let l = self.sw_rewrite(&q.lhs, &dep, &mut vec![], &mut sw, lam, &mut lctx, ctx, 0);
                    let r = self.sw_rewrite(&q.rhs, &dep, &mut vec![], &mut sw, lam, &mut lctx, ctx, 0);
                    eq(l, r, q.span)
                })
                .collect()
        } else {
            eqs
        };
        for q in &eqs {
            let mut lv = self.expr(&q.lhs, &mut lctx)?;
            let mut rv = self.expr(&q.rhs, &mut lctx)?;
            self.need_numlike(&lv, &q.lhs, "the left side", true)?;
            self.need_numlike(&rv, &q.rhs, "the right side", true)?;
            if matches!(lv.ty, Ty::Complex(_)) || matches!(rv.ty, Ty::Complex(_)) {
                let mut present = vec![];
                find_derivs(&q.lhs, &mut present, &[]);
                find_derivs(&q.rhs, &mut present, &[]);
                let real: Vec<String> = present
                    .iter()
                    .filter(|(x, _)| names.contains(x) && !is_c[di(x)])
                    .map(|(x, _)| x.clone())
                    .collect();
                if !real.is_empty() {
                    return Err(SErr::NeedComplex(real));
                }
                lv = promote(lv);
                rv = promote(rv);
            }
            if vec_n(&lv.ty) != vec_n(&rv.ty) {
                return Err(self
                    .err("one side of this equation is a vector and the other isn't (or they have different lengths)",
                         q.span, None)
                    .into());
            }
            let (ld, rd) = (dim_of(&lv.ty), dim_of(&rv.ty));
            if !self.u.unify(&ld, &rd) {
                return Err(self
                    .err(format!("the two sides of this equation don't match: left is {}, right is {}", self.desc(&ld),
                                 self.desc(&rd)), q.span, None)
                    .into());
            }
        }
        // assign equations to unknowns and isolate the highest derivative
        let mut assigned: Vec<(String, A::Expr)> = vec![];
        let mut tops_in: Vec<Vec<String>> = vec![];
        for q in &eqs {
            let mut present = vec![];
            find_derivs(&q.lhs, &mut present, &[]);
            find_derivs(&q.rhs, &mut present, &[]);
            tops_in.push(
                names
                    .iter()
                    .filter(|x| present.iter().any(|p| p.0 == **x && p.1 as usize == ord(x)))
                    .cloned()
                    .collect(),
            );
        }
        let coupled = if tops_in.iter().any(|ts| ts.len() > 1) {
            Some(self.mass_matrix(&eqs, &tops_in, &names, &orders, &shape, &t)?)
        } else {
            None
        };
        if coupled.is_none() {
            for q in &eqs {
                let mut present = vec![];
                find_derivs(&q.lhs, &mut present, &[]);
                find_derivs(&q.rhs, &mut present, &[]);
                let cands: Vec<&String> = names
                    .iter()
                    .filter(|x| present.iter().any(|p| p.0 == **x && p.1 as usize == ord(x)))
                    .filter(|x| !assigned.iter().any(|a| a.0 == **x))
                    .collect();
                let Some(x) = cands.first().map(|x| (*x).clone()) else {
                    return Err(self
                        .err("can't tell which unknown this equation is for", q.span,
                             Some("each equation should contain the highest derivative of one unknown, like x'' = ...".into()))
                        .into());
                };
                let target = C::mk(K::Prime { target: Box::new(C::name(&x)), order: ord(&x) as i64 });
                let iso = C::isolate(&q.lhs, &q.rhs, &target)?;
                assigned.push((x, iso));
            }
        }
        for x in &names {
            self.scopes[lctx.scope].names.remove(&format!("{x}{}", primes(ord(x))));
        }
        let tops_ir = match &coupled {
            Some(cp) => Some(self.mass_matrix_ir(cp, &names, &orders, &dims, &tdim, lam, &mut lctx, &t)?),
            None => None,
        };
        let sref = node_at(s.span);
        let mut body = vec![];
        for (x, k) in &layout {
            if *k < ord(x) - 1 {
                let nm = format!("{x}{}", primes(k + 1));
                let Some(Binding::Sym(sym)) = self.scopes[lctx.scope].names.get(&nm).cloned() else { unreachable!() };
                body.push(self.var_ref(sym, &mut lctx, &sref)?);
            } else if let Some(tops) = &tops_ir {
                body.push(tops[di(x)].clone());
            } else {
                let e = assigned.iter().find(|a| a.0 == *x).unwrap().1.clone();
                let mut v = self.expr(&e, &mut lctx)?;
                if is_c[di(x)] && matches!(v.ty, Ty::Num(_)) {
                    v = promote(v);
                } else if matches!(v.ty, Ty::Complex(_)) && !is_c[di(x)] {
                    return Err(SErr::NeedComplex(vec![x.clone()]));
                }
                let n = ord(x);
                if vec_n(&v.ty) != shape_of(&shape, x) {
                    let what = if shape_of(&shape, x) == 1 { "a number" } else { "a vector" };
                    return Err(self.err(format!("{x}{} must be {what} like {x}", primes(n)), s.span, None).into());
                }
                let want = dims[di(x)].div(&tpow(n));
                let vd = dim_of(&v.ty);
                if !self.u.unify(&vd, &want) {
                    return Err(self
                        .err(format!("{x}{} works out to {} but should be {}", primes(n), self.desc(&vd), self.desc(&want)),
                             s.span, None)
                        .into());
                }
                body.push(v);
            }
        }
        let nuser: usize = state_syms.iter().map(|&s| vec_n(&self.module.syms[s].ty)).sum();
        for _ in &sw.syms {
            body.push(ir(I::ExprKind::Const(0.0), Ty::Num(DExpr::of(DIMLESS)), s.span.line)); // a flag is constant
        }
        self.module.lambdas[lam].body = body;
        let mut event = None;
        let mut evtext: i64 = -1;
        if let Some(u) = &until {
            let (ev, txt) = self.check_until(u, &orders, lam, &mut lctx, &t)?;
            event = Some(ev);
            evtext = txt as i64;
        }
        let mut whens = vec![];
        for w in &sv.whens {
            if method != "rk45" {
                return Err(self
                    .err("when works with the adaptive solver (rk45) for now", w.span,
                         Some(format!("remove {} to use it", if step.is_some() { "step …" } else { "using …" })))
                    .into());
            }
            whens.push(self.check_when(w, &orders, &layout, &state_syms, lam, &mut lctx, &t)?);
        }
        let switch = if sw.syms.is_empty() {
            None
        } else {
            let name = self.fresh_name("conditions");
            let l = &self.module.lambdas[lam];
            let lm = I::Lambda { kind: I::LambdaKind::Ode, name, params: l.params.clone(),
                                 captures: l.captures.clone(), locals: l.locals.clone(), body: sw.gs.clone(),
                                 state: l.state.clone(), col_syms: vec![], param_syms: vec![] };
            self.module.lambdas.push(lm);
            Some(self.module.lambdas.len() - 1)
        };
        self.solve.infos.push(SolInfo { names: names.clone(), t: t.clone(), ..Default::default() });
        let info_id = self.solve.infos.len() - 1;
        let sol_name = self.fresh_name("__sol");
        let sol_sym = self.new_sym(&sol_name, Ty::Sol(info_id), ctx);
        self.extra[sol_sym].assigned = true;
        let mut base = 0;
        let sfs: Vec<u32> = y0.iter().filter_map(|(_, v)| v.sf).chain([t0.sf, t1.sf].into_iter().flatten()).collect();
        for (i, x) in names.iter().enumerate() {
            let n = ord(x);
            let w = shape_of(&shape, x);
            let hints: Vec<Option<I::Hint>> = (0..n).map(|k| y0[find_y0(&y0, x, k).unwrap()].1.hint.clone()).collect();
            let view = SolView { sol_sym, comp: base, top: base + (n - 1) * w, dim: dims[i].clone(), tdim: tdim.clone(),
                                 tname: t.clone(), name: x.clone(), n: w, stride: w, cplx: is_c[i], hint: hints[0].clone(),
                                 hints, thint: t0.hint.clone().or(t1.hint.clone()), sf: sfs.iter().min().copied() };
            self.sols.push(view);
            self.bind(ctx.scope, x, Binding::Sol(self.sols.len() - 1));
            base += n * w;
        }
        self.solve.infos[info_id].slots = base;
        let mut rtol = 1e-9;
        if let Some(tn) = &sv.tolerance {
            let tv = self.expr(tn, ctx)?;
            // a relative tolerance: a plain number between 0 and 1 (red team round 3 #13: units were accepted)
            if let Ty::Num(d) = &tv.ty {
                if !self.u.unify(d, &DExpr::of(DIMLESS)) {
                    return Err(self
                        .err(format!("the tolerance is relative, so it must be a plain number (no units) like 1e-8, but \
                                      it is {}", self.desc(d)), tn.span, None)
                        .into());
                }
            }
            let Some(val) = is_const(&tv) else {
                return Err(self.err("the tolerance must be a plain number written out, like 1e-8", tn.span, None).into());
            };
            if !(0.0 < val && val < 1.0) {
                return Err(self
                    .err(format!("the tolerance is relative, so it must be between 0 and 1 (like 1e-8), but it is {}",
                                 fermium_syntax::pyfmt::fmt_g(val)), tn.span, None)
                    .into());
            }
            rtol = val;
        }
        let atol = match &sv.absolute {
            Some(abs) if !abs.is_empty() => {
                let shapes: Vec<usize> = names.iter().map(|x| shape_of(&shape, x)).collect();
                let y0c: Vec<((String, usize), Option<f64>)> = y0.iter().map(|(k, v)| (k.clone(), is_const(v))).collect();
                Some(self.check_absolute(abs, &layout, &names, &dims, &shapes, &tdim, step.is_some(), Some(&y0c), ctx)?)
            }
            _ => None,
        };
        let tname = self.text(&t);
        let tfmt = self.fmt_of(&tdim, t0.hint.clone().or(t1.hint.clone()));
        // does the right side depend on t itself (not only through the unknowns)? (D40)
        let tdep = eqs.iter().any(|q| free_names(&q.lhs).contains(&t) || free_names(&q.rhs).contains(&t));
        let mut y0v: Vec<I::Expr> = layout.iter().map(|(x, k)| y0[find_y0(&y0, x, *k).unwrap()].1.clone()).collect();
        for _ in &sw.syms {
            // 2: evaluated as written, until the RK45 solver sets the flag from the condition at the start (so a
            // probe of the right side outside the solver, like the uncertainty kernels', sees the condition itself)
            y0v.push(ir(I::ExprKind::Const(2.0), Ty::Num(DExpr::of(DIMLESS)), s.span.line));
        }
        let atol = atol.map(|mut a| {
            a.extend(std::iter::repeat_n((0.0, 0), sw.syms.len()));
            a
        });
        let nuser = if sw.syms.is_empty() { 0 } else { nuser };
        let x = I::SolveExtra { rtol, atol, event, evtext, tname, tfmt, tdep, line: s.span.line, sw_ops: sw.ops.clone(),
                                sw_slot0: nuser, nuser, switch, whens, ..Default::default() };
        if let Some(tops) = tops_ir.as_ref() {
            // the singular-mass-matrix error shows t like the solve's other errors
            set_sing_fmt(&mut self.module.lambdas[lam].body, tfmt);
            let _ = tops;
        }
        Ok(vec![I::Stmt { kind: I::StmtKind::Solve { sol: sol_sym, rhs: lam, y0: y0v, t0, t1, step, method, rtol: None,
                                                   x: Box::new(x) },
                          line: s.span.line }])
    }

    /// `absolute a[, b …]`: absolute tolerances (D160). Per state slot: (value in SI, power of 1/|t1 − t0|).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_absolute(&mut self, abs: &[A::Expr], layout: &[(String, usize)], names: &[String], dims: &[DExpr],
                                 shapes: &[usize], tdim: &DExpr, has_step: bool,
                                 y0: Option<&[((String, usize), Option<f64>)]>, ctx: &mut Ctx) -> CResult<Vec<(f64, u32)>> {
        if has_step {
            return Err(self.err("absolute sets the error control of the adaptive solvers (rk45, radau, bdf); with  step  \
                                 the steps are fixed, so leave out one of them", abs[0].span, None));
        }
        let mut vals: Vec<(f64, DExpr, A::Span)> = vec![];
        for node in abs {
            let v = self.expr(node, ctx)?;
            let Some(mut value) = is_const(&v) else {
                return Err(self.err("an absolute tolerance must be a positive constant, like 1e-16 or 1e-9 m", node.span,
                                    None));
            };
            if let Some(h) = &v.hint {
                if h.offset != 0.0 {
                    // a tolerance is a size of error: `absolute 1e-6 °C` is a step of 10⁻⁶ K (D200)
                    value -= h.offset;
                }
            }
            if !(0.0 < value && value < f64::INFINITY) {
                return Err(self.err("an absolute tolerance must be a positive constant, like 1e-16 or 1e-9 m", node.span,
                                    None));
            }
            let vd = dim_of(&v.ty);
            for (_, w, _) in &vals {
                if self.same_dim(w, &vd) {
                    return Err(self.err(format!("two absolute tolerances in {}; give one value per unit", self.desc(&vd)),
                                        node.span, None));
                }
            }
            vals.push((value, vd, node.span));
        }
        let tpow = |k: usize| DExpr::of(DIMLESS).mul(&tdim.pow(Rational64::from_integer(k as i64)));
        let mut used = vec![false; vals.len()];
        let mut out = vec![];
        for (x, k) in layout {
            let i = names.iter().position(|n| n == x).unwrap();
            let want = dims[i].div(&tpow(*k));
            let mut hit = vals.iter().position(|(_, d, _)| self.same_dim(d, &want));
            let mut power = 0;
            if hit.is_none() && *k > 0 {
                hit = vals.iter().position(|(_, d, _)| self.same_dim(d, &dims[i]));
                power = *k as u32;
            }
            let Some(h) = hit else {
                let have: Vec<String> = vals.iter().map(|(_, d, _)| self.desc(d)).collect();
                let sym = |c: &Checker, d: &DExpr| {
                    let u = fermium_units::preferred_unit(&c.u.resolve(d));
                    if u.name != "" && u.name != "1" { format!(" {}", u.name) } else { String::new() }
                };
                let mut like: Vec<String> = vals.iter().map(|(_, d, _)| format!("1e-9{}", sym(self, d))).collect();
                like.push(format!("1e-12{}", sym(self, &want)));
                return Err(self.err(format!("no absolute tolerance for {x}{}, which is {} (the values given are in {}); add \
                                             one in its units after a comma, like  absolute {}", primes(*k),
                                            self.desc(&want), have.join(", "), like.join(", ")), abs[0].span, None));
            };
            used[h] = true;
            for _ in 0..shapes[i] {
                out.push((vals[h].0, power));
            }
        }
        for (i, (_, d, span)) in vals.iter().enumerate() {
            if !used[i] {
                return Err(self.err(format!("no unknown of this solve is in {}, so this absolute tolerance isn't used",
                                            self.desc(d)), *span, None));
            }
        }
        if let Some(y0) = y0 {
            // an absolute tolerance at least as large as the starting values switches the error control off (D201)
            for (value, vd, span) in &vals {
                let mut scale = 0.0f64;
                for (x, k) in layout {
                    let i = names.iter().position(|n| n == x).unwrap();
                    if *k != 0 || !self.same_dim(vd, &dims[i]) {
                        continue;
                    }
                    if let Some((_, Some(c))) = y0.iter().find(|e| e.0 .0 == *x && e.0 .1 == 0) {
                        if c.is_finite() && c.abs() > scale {
                            scale = c.abs();
                        }
                    }
                }
                if scale > 0.0 && *value >= scale {
                    let ratio = value / scale;
                    let times = if ratio >= 1.995 {
                        format!("{}×", crate::units::format_number(ratio, Some(3)))
                    } else {
                        "as large as".to_string()
                    };
                    let u = fermium_units::preferred_unit(&self.u.resolve(vd));
                    let sym = if u.name != "" && u.name != "1" { format!(" {}", u.name) } else { String::new() };
                    let shown = format!("{}{sym}", crate::units::format_number(scale / u.factor, Some(3)));
                    self.warn(format!("this absolute tolerance is {times} the largest starting value in its units ({shown}), \
                                       so the error control is effectively off and the result may be far off"), A::Span { length: 1, ..*span },
                              Some("an absolute tolerance is the size of error you accept; make it much smaller than the \
                                    values, e.g. 10⁻⁶ of them (check the unit: mm, not km?)".into()));
                }
            }
        }
        Ok(out)
    }

    /// Equations with several highest derivatives (D47): per equation (equation, coefficients in the order of
    /// names, the rest), with Σ_j a_ij x_j^(n) + r_i = 0.
    #[allow(clippy::type_complexity)]
    fn mass_matrix(&self, eqs: &[A::Equation], tops_in: &[Vec<String>], names: &[String], orders: &[(String, i64)],
                   shape: &[(String, usize)], t: &str) -> CResult<Vec<(A::Equation, Vec<A::Expr>, A::Expr)>> {
        let ord = |x: &str| orders.iter().find(|o| o.0 == x).unwrap().1 as usize;
        let tops: Vec<String> = names.iter().map(|x| format!("{x}{}", primes(ord(x)))).collect();
        let qi = tops_in.iter().position(|ts| ts.len() > 1).unwrap();
        let q0 = &eqs[qi];
        let both: Vec<String> = tops_in[qi].iter().map(|x| format!("{x}{}", primes(ord(x)))).collect();
        let both = both.join(" and ");
        let vecs: Vec<&String> = names.iter().filter(|x| shape.iter().any(|s| s.0 == **x && s.1 > 1)).collect();
        if let Some(v) = vecs.first() {
            return Err(self.err(format!("{both} both appear in one equation, which works only for unknowns that are \
                                         numbers, but {v} is a vector; write its components as separate unknowns"),
                                q0.span, None));
        }
        if names.len() > 4 {
            return Err(self.err(format!("{both} both appear in one equation; that works for up to 4 unknowns (this solve \
                                         has {}); solve for the highest derivatives yourself, e.g. with solve_linear",
                                        names.len()), q0.span, None));
        }
        let targets: Vec<A::Expr> =
            names.iter().map(|x| C::mk(K::Prime { target: Box::new(C::name(x)), order: ord(x) as i64 })).collect();
        let mut out = vec![];
        for q in eqs {
            let Some((coeffs, r0)) = C::linear_coeffs(&q.lhs, &q.rhs, &targets) else {
                return Err(self.err(format!("{both} both appear in one equation; that works when every equation is linear \
                                             in {} (like m1 a'' + k b'' = F, with coefficients that may depend on {t} and \
                                             the unknowns), and this one isn't", tops.join(", ")), q.span, None));
            };
            out.push((q.clone(), coeffs, r0));
        }
        for j in 0..names.len() {
            if out.iter().all(|(_, c, _)| C::is_num(&c[j], Some(0.0))) {
                return Err(self.err(format!("{} drops out of the equations (its coefficients are all 0), so they can't be \
                                             solved for it", tops[j]), q0.span, None));
            }
        }
        Ok(out)
    }

    /// The highest derivatives from M x'' = −r at each evaluation of the right side (D47).
    #[allow(clippy::too_many_arguments)]
    fn mass_matrix_ir(&mut self, coupled: &[(A::Equation, Vec<A::Expr>, A::Expr)], names: &[String],
                      orders: &[(String, i64)], dims: &[DExpr], tdim: &DExpr, lam: I::LambdaId, lctx: &mut Ctx, t: &str)
                      -> CResult<Vec<I::Expr>> {
        let ord = |x: &str| orders.iter().find(|o| o.0 == x).unwrap().1 as usize;
        let n = names.len();
        let (mut entries, mut rhs) = (vec![], vec![]);
        for (q, coeffs, r0) in coupled {
            let qn = node_at(q.span);
            for a in coeffs {
                let v = self.expr(a, lctx)?;
                self.need_num(&v, &qn, "a coefficient of a highest derivative")?;
                entries.push(v);
            }
            let v = self.expr(&C::at(C::neg(r0.clone()), &qn), lctx)?;
            self.need_num(&v, &qn, "the equation")?;
            rhs.push(v);
        }
        let line = coupled[0].0.span.line;
        let tops: Vec<String> = names.iter().map(|x| format!("{x}{}", primes(ord(x)))).collect();
        let text = self.text(&format!("the equations don't determine {} at {t} = ", tops.join(" and ")));
        let tsym = self.module.lambdas[lam].params[0];
        let tvar = ir(I::ExprKind::Var(tsym), self.module.syms[tsym].ty.clone(), line);
        let plain = DExpr::of(DIMLESS);
        let vty = Ty::Vec { n, dim: Some(plain), dims: None };
        let acc = ir(I::ExprKind::OdeLinSolve { m: entries, b: rhs, t: Box::new(tvar), text, fmt: usize::MAX }, vty.clone(),
                     line);
        let sym = self.lam_sym("__accel", vty.clone(), lctx);
        self.module.lambdas[lam].locals.push(sym);
        let tpow = |k: usize| DExpr::of(DIMLESS).mul(&tdim.pow(Rational64::from_integer(k as i64)));
        let mut out = vec![];
        for (j, x) in names.iter().enumerate() {
            let var = ir(I::ExprKind::Var(sym), vty.clone(), line);
            let e = ir(I::ExprKind::VecElem(Box::new(var), j), Ty::Num(dims[j].div(&tpow(ord(x)))), line);
            if j == 0 {
                let ty = e.ty.clone();
                out.push(ir(I::ExprKind::Let(vec![(sym, acc.clone())], Box::new(e)), ty, line));
            } else {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// `until lhs = rhs`: the event lambda and the text id of its "never happened" error (D39).
    fn check_until(&mut self, until: &A::Equation, orders: &[(String, i64)], lam: I::LambdaId, lctx: &mut Ctx, t: &str)
                   -> CResult<(I::LambdaId, usize)> {
        let mut used = vec![];
        find_derivs(&until.lhs, &mut used, &[]);
        find_derivs(&until.rhs, &mut used, &[]);
        for (x, k) in &used {
            if let Some((_, n)) = orders.iter().find(|o| o.0 == *x) {
                if k >= n {
                    let have: Vec<String> = (0..*n as usize).map(|j| format!("{x}{}", primes(j))).collect();
                    return Err(self.err(format!("the stop condition can use {} (not {x}{})", have.join(", "),
                                                primes(*k as usize)), until.span, None));
                }
            }
        }
        let left = self.expr(&until.lhs, lctx)?;
        let right = self.expr(&until.rhs, lctx)?;
        self.need_num(&left, &until.lhs, "the left side of the stop condition")?;
        self.need_num(&right, &until.rhs, "the right side of the stop condition")?;
        let (ld, rd) = (dim_of(&left.ty), dim_of(&right.ty));
        self.unify_or(&ld, &rd, |c| format!("the two sides of the stop condition don't match: left is {}, right is {}",
                                            c.desc(&ld), c.desc(&rd)), until.span, None)?;
        let un = node_at(until.span);
        let g = self.arith("-", left, right, &un)?;
        let name = self.fresh_name("until");
        let l = &self.module.lambdas[lam];
        let ev = I::Lambda { kind: I::LambdaKind::Ode, name, params: l.params.clone(), captures: l.captures.clone(),
                             locals: l.locals.clone(), body: vec![g], state: l.state.clone(), col_syms: vec![],
                             param_syms: vec![] };
        self.module.lambdas.push(ev);
        let text = format!("the stop condition (until {} = {}) never happened up to {t} = ", C::to_source(&until.lhs, true),
                           C::to_source(&until.rhs, true));
        let tid = self.text(&text);
        Ok((self.module.lambdas.len() - 1, tid))
    }

    // ============================================================ solutions as values (checker.py)
    /// x' of a solution (Python e_Prime, the SolRef part; called from calculus.rs e_prime).
    pub fn sol_prime_of(&mut self, vid: SolViewId, order: i64) -> Checked {
        let v = self.sols[vid].clone();
        let order_u = order as usize;
        let tp = DExpr::of(DIMLESS).mul(&v.tdim.pow(Rational64::from_integer(order)));
        let total = v.name.matches('\'').count() + order_u;
        let nv = SolView { sol_sym: v.sol_sym, comp: v.comp + order_u * v.stride, top: v.top, dim: v.dim.div(&tp),
                           tdim: v.tdim.clone(), tname: v.tname.clone(), name: format!("{}{}", v.name, primes(order_u)),
                           n: v.n, stride: v.stride, cplx: v.cplx, hint: v.hints.get(total).cloned().flatten(),
                           hints: v.hints.clone(), thint: v.thint.clone(), sf: v.sf };
        self.sols.push(nv);
        Checked::Sol(self.sols.len() - 1)
    }

    /// r.x, z.re, x.t / x.times, x.values of a solution (Python e_Field, the SolRef part).
    pub fn sol_field(&mut self, e: &A::Expr, vid: SolViewId, name: &str, ctx: &mut Ctx) -> CResult<Checked> {
        let v = self.sols[vid].clone();
        if (matches!(name, "x" | "y" | "z") && v.n > 1 && !v.cplx) || (matches!(name, "re" | "im") && v.cplx) {
            let k = match name {
                "x" | "re" => 0,
                "y" | "im" => 1,
                _ => 2,
            };
            if k >= v.n {
                return Err(self.err(format!("{} is a {}-vector, so it has no .{name}", v.name, v.n), e.span, None));
            }
            let nv = SolView { sol_sym: v.sol_sym, comp: v.comp + k, top: v.top + k, dim: v.dim.clone(),
                               tdim: v.tdim.clone(), tname: v.tname.clone(), name: format!("{}.{name}", v.name), n: 1,
                               stride: v.stride, cplx: false, hint: v.hint.clone(), hints: vec![], thint: v.thint.clone(),
                               sf: v.sf };
            self.sols.push(nv);
            return Ok(Checked::Sol(self.sols.len() - 1));
        }
        if matches!(name, "t" | "time" | "times") {
            let s = self.var_ref(v.sol_sym, ctx, e)?;
            let I::ExprKind::Var(sym) = s.kind else { unreachable!() };
            return Ok(Checked::Val(ir(I::ExprKind::SolList { sol: sym, comp: v.comp, what: 1 }, Ty::List(v.tdim.clone()),
                                      e.span.line)));
        }
        if matches!(name, "values" | "v") {
            return self.sol_values(vid, e).map(Checked::Val);
        }
        // the rest of e_Field (complex parts, vectors, data columns) belongs to other modules
        if matches!(name, "re" | "im") {
            return Err(self.not_ported("parts of a complex number", e.span));
        }
        if matches!(name, "x" | "y" | "z") {
            return Err(self.err(format!("'.{name}' only works on vectors (v.x) and data loaded from a file (data.{name})"),
                                e.span, None));
        }
        Err(self.err(format!("'.{name}' only works on vectors (v.x) and data loaded from a file (data.{name})"), e.span,
                     None))
    }

    /// All computed values of a solution component, as a list (Python sol_values).
    pub fn sol_values(&mut self, vid: SolViewId, e: &A::Expr) -> CResult<I::Expr> {
        let v = self.sols[vid].clone();
        if v.cplx {
            return Err(self.err(format!("{} is complex, and lists of complex numbers aren't supported yet", v.name), e.span,
                                Some(format!("use its real or imaginary part, {n}.re or {n}.im (plot {n}.re vs {t}), or \
                                              values at single times like |{n}({t})|", n = v.name, t = v.tname))));
        }
        if v.n > 1 {
            return Err(self.err(format!("{} is a vector; use its components, like {}.x", v.name, v.name), e.span, None));
        }
        let (comp, what) = if v.comp > v.top { (v.top, 2) } else { (v.comp, 0) };
        let mut r = ir(I::ExprKind::SolList { sol: v.sol_sym, comp, what }, Ty::List(v.dim.clone()), e.span.line);
        r.hint = None;
        Ok(r)
    }

    /// r[end], x[3] of a solution: an element of its values (Python e_Index, the SolRef part).
    pub fn sol_index(&mut self, vid: SolViewId, e: &A::Expr, index: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let v = self.sols[vid].clone();
        let line = e.span.line;
        let index_of = |c: &mut Checker, vals: &I::Expr, ctx: &mut Ctx| -> CResult<I::Expr> {
            if matches!(index.kind, K::End) {
                return Ok(ir(I::ExprKind::Builtin("len".into(), vec![vals.clone()]), Ty::Num(DExpr::of(DIMLESS)), line));
            }
            c.index_expr(index, vals, ctx)
        };
        if v.n > 1 {
            // r[end] of a vector solution is a vector (A19)
            let mut comps = vec![];
            for k in 0..v.n {
                let mut sub = v.clone();
                sub.comp = v.comp + k;
                sub.top = v.top + k;
                sub.n = 1;
                sub.cplx = false;
                self.sols.push(sub);
                let sid = self.sols.len() - 1;
                let vals = self.sol_values(sid, e)?;
                let idx = index_of(self, &vals, ctx)?;
                let mut r = ir(I::ExprKind::Index(Box::new(vals), Box::new(idx)), Ty::Num(v.dim.clone()), line);
                r.hint = v.hint.clone();
                comps.push(r);
            }
            let ty = if v.cplx { Ty::Complex(v.dim.clone()) } else { Ty::Vec { n: v.n, dim: Some(v.dim.clone()), dims: None } };
            let mut r = ir(I::ExprKind::Vec(comps), ty, line);
            r.hint = v.hint.clone();
            return Ok(r);
        }
        let vals = self.sol_values(vid, e)?;
        let idx = index_of(self, &vals, ctx)?;
        let mut r = ir(I::ExprKind::Index(Box::new(vals), Box::new(idx)), Ty::Num(v.dim.clone()), line);
        r.hint = None;
        Ok(r)
    }

    /// x(t) of a solution: its value (or a list of values, #62) at a time (Python sol_eval).
    pub fn sol_eval(&mut self, vid: SolViewId, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Call { args, .. } = &e.kind else { unreachable!() };
        let view = self.sols[vid].clone();
        if args.len() != 1 {
            return Err(self.err(format!("{} takes one argument ({})", view.name, view.tname), e.span, None));
        }
        let t = self.expr(&args[0], ctx)?;
        self.need_numlike(&t, &args[0], "this value", false)?;
        let td = dim_of(&t.ty);
        self.unify_or(&td, &view.tdim, |c| format!("{} is a function of {}, which is {}, not {}", view.name, view.tname,
                                                   c.desc(&view.tdim), c.desc(&td)), args[0].span, None)?;
        if matches!(t.ty, Ty::List(_)) && view.n > 1 && view.cplx {
            return Err(self.err(format!("{} is complex, so it can't be evaluated at each element of a list (lists of \
                                         complex numbers aren't supported yet)", view.name), e.span,
                                Some(format!("loop over the list, or take the real or imaginary part, like {}.re",
                                             view.name))));
        }
        if matches!(t.ty, Ty::List(_)) && view.n > 1 {
            return Err(self.err(format!("{} is a vector, so it can't be evaluated at each element of a list (lists of \
                                         vectors aren't supported yet)", view.name), e.span,
                                Some(format!("loop over the list, or take one component, like {n}.x or {n}[1]",
                                             n = view.name))));
        }
        let sol = self.var_ref(view.sol_sym, ctx, e)?;
        let I::ExprKind::Var(sol_sym) = sol.kind else { unreachable!() };
        let tfmt = self.fmt_of(&view.tdim, view.thint.clone());
        let tsf = t.sf;
        let mut r = self.sol_eval_node(&view, sol_sym, &t, e, tfmt)?;
        r.sf = match view.sf {
            Some(vs) => Some(tsf.map_or(vs, |a| a.min(vs))),
            None => tsf,
        };
        if view.hint.is_some() {
            r.hint = view.hint.clone();
        }
        Ok(r)
    }

    fn sol_eval_node(&mut self, view: &SolView, sol: I::SymId, t: &I::Expr, e: &A::Expr, tfmt: usize) -> CResult<I::Expr> {
        let line = e.span.line;
        if view.n > 1 {
            let mut comps = vec![];
            for k in 0..view.n {
                let mut sub = view.clone();
                sub.comp = view.comp + k;
                sub.top = view.top + k;
                sub.n = 1;
                sub.cplx = false;
                comps.push(self.sol_eval_node(&sub, sol, t, e, tfmt)?);
            }
            let ty = if view.cplx {
                Ty::Complex(view.dim.clone())
            } else {
                Ty::Vec { n: view.n, dim: Some(view.dim.clone()), dims: None }
            };
            return Ok(ir(I::ExprKind::Vec(comps), ty, line));
        }
        let ty = if matches!(t.ty, Ty::List(_)) { Ty::List(view.dim.clone()) } else { Ty::Num(view.dim.clone()) };
        if view.comp <= view.top {
            Ok(ir(I::ExprKind::SolEval { sol, comp: view.comp, t: Box::new(t.clone()), use_dy: false, tfmt }, ty, line))
        } else if view.comp == view.top + view.stride {
            // x'(t) from the right-hand side at the interpolated state (D46)
            Ok(ir(I::ExprKind::SolEval { sol, comp: view.top, t: Box::new(t.clone()), use_dy: true, tfmt }, ty, line))
        } else {
            Err(self.err(format!("can't take that many derivatives of the solution {}", view.name), e.span, None))
        }
    }

    /// The unknowns of a solution, for messages.
    pub fn sol_names(&self, sol: usize) -> Vec<String> {
        self.solve.infos.get(sol).map(|i| i.names.clone()).unwrap_or_default()
    }
}

/// The singular-matrix error of a coupled ODE shows t with the solve's time format.
fn set_sing_fmt(body: &mut [I::Expr], fmt: usize) {
    for e in body {
        if let I::ExprKind::Let(binds, _) = &mut e.kind {
            for (_, v) in binds.iter_mut() {
                if let I::ExprKind::OdeLinSolve { fmt: f, .. } = &mut v.kind {
                    *f = fmt;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use fermium_syntax::ast as A;

    fn parse_expr(s: &str) -> A::Expr {
        let (prog, _) = fermium_syntax::parse(&format!("zz = {s}\n"), &[]).unwrap();
        match &prog.body[0].kind {
            A::StmtKind::Assign { value, .. } => value.clone(),
            _ => panic!(),
        }
    }

    #[test]
    fn isolated_right_sides_match_python() {
        // python3 -c "from fermium import calculus as C ...; C.to_source(C.isolate(l, r, x''), pretty=False)"
        let cases = [("-ħ²/(2*m) * ψ''  + V(x) ψ", "E ψ", "ψ", "-(psi V(x) - E psi)/(-hbar^2/(2*m))"),
                     ("-ħ^2 / (2 m_e) * ψ'' + V0 ψ", "E ψ", "ψ", "-(V0 psi - E psi)/(-hbar^2/(2 m_e))"),
                     ("m x''", "-k x - b x' + F0 cos(ω t)", "x", "(-k x - b x' + F0 cos(omega t))/m")];
        for (l, r, x, want) in cases {
            let t = fermium_sym::build::prime(fermium_sym::build::name(x), 2);
            let got = fermium_sym::isolate(&parse_expr(l), &parse_expr(r), &t).unwrap();
            assert_eq!(fermium_sym::to_source_p(&got, false), want);
        }
    }
}
