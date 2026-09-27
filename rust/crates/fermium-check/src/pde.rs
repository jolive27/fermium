//! 1-D PDEs (heat, wave, Schrödinger) `solve ∂u/∂t = D ∂²u/∂x² with u(x, 0) = …, u(0, t) = 0, u(L, t) = 0 for x
//! from 0 to L, t from 0 s to T` (D83): a port of check_pde, pde_call and their helpers in `fermium/m3solve.py`.
use std::collections::HashMap;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_sym::build as B;
use fermium_syntax::ast as A;
use fermium_syntax::ast::ExprKind as K;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::solve::{is_const, node_at, PdeView, SolInfo};
use crate::stmts::ty_dim;
use crate::walk::free_names;

fn dim_of(t: &Ty) -> DExpr {
    ty_dim(t).unwrap_or_else(DExpr::fresh)
}

fn src(e: &A::Expr) -> String {
    fermium_sym::to_source(e)
}

fn pde_method(m: &str) -> Option<u8> {
    match m {
        "crank_nicolson" | "cn" | "cranknicolson" => Some(0),
        "implicit" => Some(1),
        "explicit" => Some(2),
        _ => None,
    }
}

/// ∂u/∂x → u__x1, ∂²u/∂x² → u__x2, ∂u/∂t → u__t1, ∂²u/∂t² → u__t2 (names the probe binds).
fn pde_rewrite(e: &A::Expr, u: &str, xv: &str, tv: &str, found: &mut Vec<String>) -> Result<A::Expr, String> {
    if let K::Deriv { var, order, operand, .. } = &e.kind {
        if matches!(&operand.kind, K::Name { name } if name == u) {
            if var != xv && var != tv {
                return Err(format!("∂{u}/∂{var}: {u} is a function of {xv} and {tv}"));
            }
            if *order > 2 {
                return Err(format!("derivatives of order {order} aren't supported in a PDE (up to second order)"));
            }
            let key = format!("{u}__{}{order}", if var == xv { "x" } else { "t" });
            if !found.contains(&key) {
                found.push(key.clone());
            }
            return Ok(B::at(B::name(&key), e));
        }
        if free_names(operand).iter().any(|n| n == u) {
            return Err(format!("only derivatives of {u} itself are supported (like ∂²{u}/∂{xv}²); expand ∂/∂{var} (…) by \
                                hand"));
        }
    }
    let mut failed = None;
    let r = fermium_sym::map_children(e, &mut |c| match pde_rewrite(c, u, xv, tv, found) {
        Ok(x) => x,
        Err(m) => {
            if failed.is_none() {
                failed = Some(m);
            }
            c.clone()
        }
    });
    match failed {
        Some(m) => Err(m),
        None => Ok(r),
    }
}

/// A(x) · exp(i φ(x)) → (A, φ): one factor exp(… i …) in a product / quotient; (e, None) if none.
fn split_phase(e: &A::Expr) -> (A::Expr, Option<A::Expr>) {
    if let K::Call { func, args } = &e.kind {
        if matches!(&func.kind, K::Name { name } if name == "exp") && args.len() == 1
            && free_names(&args[0]).iter().any(|n| n == "i")
        {
            let mut m = HashMap::new();
            m.insert("i".to_string(), B::at(B::num(1.0), e));
            return (B::at(B::num(1.0), e), Some(fermium_sym::subst(&args[0], &m)));
        }
    }
    if let K::BinOp { op, left, right, .. } = &e.kind {
        if op == "*" {
            let (a, pa) = split_phase(left);
            if pa.is_some() {
                return (B::at(B::binop("*", a, (**right).clone(), false), e), pa);
            }
            let (b, pb) = split_phase(right);
            if pb.is_some() {
                return (B::at(B::binop("*", (**left).clone(), b, false), e), pb);
            }
        }
        if op == "/" {
            let (a, pa) = split_phase(left);
            if pa.is_some() {
                return (B::at(B::binop("/", a, (**right).clone(), false), e), pa);
            }
        }
    }
    (e.clone(), None)
}

struct Conds {
    ic: Option<A::Equation>,
    v0: Option<A::Equation>,
    /// per end ("a", "b"): (0 value / 1 flux, the condition)
    bcs: Vec<(char, u8, A::Equation)>,
}

impl Checker {
    /// Sort the with-conditions: the initial value, the initial velocity (waves) and the boundary values.
    #[allow(clippy::too_many_arguments)]
    fn pde_conditions(&mut self, sv: &A::Solve, initial: &[A::Equation], ctx: &mut Ctx, u: &str, xv: &str, tv: &str,
                      xa: &I::Expr, xb: &I::Expr, t0: &I::Expr, t1: &I::Expr, tdim: &DExpr, xdim: &DExpr)
                      -> CResult<Conds> {
        let mut c = Conds { ic: None, v0: None, bcs: vec![] };
        for cond in initial {
            let f = &cond.lhs;
            let mut deriv: Option<String> = None;
            let mut ok = false;
            if let K::Call { func, .. } = &f.kind {
                match &func.kind {
                    K::Deriv { var, order: 1, operand, .. } if matches!(&operand.kind, K::Name { name } if name == u) => {
                        deriv = Some(var.clone());
                        ok = true;
                    }
                    K::Name { name } if name == u => ok = true,
                    _ => {}
                }
            }
            let args = match &f.kind {
                K::Call { args, .. } if ok && args.len() == 2 => args,
                _ => {
                    return Err(self.err(format!("the conditions of a PDE look like  {u}({xv}, 0 s) = …  (initial) and  \
                                                 {u}(0 m, {tv}) = …  or  ∂{u}/∂{xv}(0 m, {tv}) = …  (boundaries)"),
                                        cond.span, None))
                }
            };
            let (ax, at) = (&args[0], &args[1]);
            let is_name = |e: &A::Expr, n: &str| matches!(&e.kind, K::Name { name } if name == n);
            if is_name(ax, xv) && !is_name(at, tv) {
                if deriv.as_deref() == Some(tv) {
                    if c.v0.is_some() {
                        return Err(self.err(format!("∂{u}/∂{tv} at the start is given twice"), cond.span, None));
                    }
                    c.v0 = Some(cond.clone());
                } else if deriv.is_none() {
                    if c.ic.is_some() {
                        return Err(self.err(format!("the initial value of {u} is given twice"), cond.span, None));
                    }
                    c.ic = Some(cond.clone());
                } else {
                    return Err(self.err(format!("an initial condition gives {u}({xv}, start) (and for a wave \
                                                 ∂{u}/∂{tv}({xv}, start))"), cond.span, None));
                }
                let tv_at = self.expr(at, ctx)?;
                self.need_num(&tv_at, at, "the time of the initial condition")?;
                let d = dim_of(&tv_at.ty);
                self.unify_or(&d, tdim, |c| format!("the initial condition is at {} but {tv} is {}", c.desc(&d),
                                                    c.desc(tdim)), at.span, None)?;
                if let (Some(a), Some(b)) = (is_const(&tv_at), is_const(t0)) {
                    let scale = b.abs() + is_const(t1).map_or(0.0, f64::abs) + 1e-300;
                    if (a - b).abs() > 1e-12 * scale {
                        return Err(self.err(format!("the initial condition must be at the start of the {tv} range"),
                                            at.span, None));
                    }
                }
            } else if is_name(at, tv) {
                if !(deriv.is_none() || deriv.as_deref() == Some(xv)) {
                    return Err(self.err(format!("a boundary condition gives {u} or ∂{u}/∂{xv} at an end, as a function of \
                                                 {tv}"), cond.span, None));
                }
                let pos = self.expr(ax, ctx)?;
                self.need_num(&pos, ax, "the position of the boundary condition")?;
                let d = dim_of(&pos.ty);
                self.unify_or(&d, xdim, |c| format!("the boundary condition is at {} but {xv} is {}", c.desc(&d),
                                                    c.desc(xdim)), ax.span, None)?;
                let s = src(ax);
                let side = if let (Some(p), Some(a), Some(b)) = (is_const(&pos), is_const(xa), is_const(xb)) {
                    let tol = 1e-12 * (a.abs() + b.abs() + 1e-300);
                    if (p - a).abs() <= tol {
                        Some('a')
                    } else if (p - b).abs() <= tol {
                        Some('b')
                    } else {
                        None
                    }
                } else if s == src(&sv.lo) {
                    Some('a')
                } else if s == src(&sv.hi) {
                    Some('b')
                } else {
                    None
                };
                let Some(side) = side else {
                    return Err(self.err(format!("boundary conditions are at the ends of the range ({xv} = {} or {})",
                                                src(&sv.lo), src(&sv.hi)), ax.span, None));
                };
                if c.bcs.iter().any(|b| b.0 == side) {
                    return Err(self.err(format!("two boundary conditions at the same end ({xv} = {s})"), cond.span, None));
                }
                c.bcs.push((side, u8::from(deriv.is_some()), cond.clone()));
            } else {
                return Err(self.err(format!("write the initial condition as  {u}({xv}, {}) = …  and the boundary \
                                             conditions as  {u}({}, {tv}) = …", src(sv.lo2.as_ref().unwrap()),
                                            src(&sv.lo)), cond.span, None));
            }
        }
        Ok(c)
    }

    /// The imaginary unit of a Schrödinger PDE is 𝑖 (D90), or a bare `i` when the program has no `i` (D184).
    fn pde_imaginary_unit(&self, sv: &A::Solve, ctx: &Ctx) -> CResult<(Vec<A::Equation>, Vec<A::Equation>)> {
        let mut names = vec![];
        for q in sv.equations.iter().chain(sv.initial.iter()) {
            names.extend(free_names(&q.lhs));
            names.extend(free_names(&q.rhs));
        }
        let uses_imag = names.iter().any(|n| n == "𝑖");
        let uses_i = names.iter().any(|n| n == "i");
        if uses_i && self.lookup(ctx.scope, "i").is_some() {
            return Err(self.err("in this PDE, i is your own variable i, not the imaginary unit", sv.equations[0].span,
                                Some("write the imaginary unit as 𝑖 (Tab \\imag) or 1i, like  𝑖 ħ ∂ψ/∂t = …; or rename \
                                      your variable".into())));
        }
        if !uses_imag {
            return Ok((sv.equations.clone(), sv.initial.clone()));
        }
        let mut m = HashMap::new();
        m.insert("𝑖".to_string(), B::name("i"));
        let conv = |qs: &[A::Equation]| -> Vec<A::Equation> {
            qs.iter()
                .map(|q| A::Equation { lhs: fermium_sym::subst(&q.lhs, &m), rhs: fermium_sym::subst(&q.rhs, &m), span: q.span })
                .collect()
        };
        Ok((conv(&sv.equations), conv(&sv.initial)))
    }

    pub fn check_pde(&mut self, s: &A::Stmt, sv: &A::Solve, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let (equations, initial) = self.pde_imaginary_unit(sv, ctx)?;
        let (xv, tv) = (sv.var.clone(), sv.var2.clone().unwrap());
        if let Some(st) = &sv.step {
            return Err(self.err(format!("in a PDE the time step goes after the time range:  {tv} from … to … step …"),
                                st.span, None));
        }
        if sv.tolerance.is_some() || sv.until.is_some() || sv.lowest.is_some()
            || sv.absolute.as_ref().is_some_and(|a| !a.is_empty())
        {
            return Err(self.err("tolerance, absolute, until and lowest aren't used in a PDE; it takes  step,  grid N  and  \
                                 using crank_nicolson / implicit / explicit", s.span, None));
        }
        if equations.len() != 1 {
            return Err(self.err("a PDE solve has one equation, like  ∂u/∂t = D * ∂²u/∂x²", s.span, None));
        }
        let q0 = &equations[0];
        let mut unknowns: Vec<String> = vec![];
        for side in [&q0.lhs, &q0.rhs] {
            for n in side.walk() {
                if let K::Deriv { var, operand, .. } = &n.kind {
                    if let K::Name { name } = &operand.kind {
                        if (*var == xv || *var == tv) && !unknowns.contains(name) && self.lookup(ctx.scope, name).is_none() {
                            unknowns.push(name.clone());
                        }
                    }
                }
            }
        }
        if unknowns.len() != 1 {
            let found = if unknowns.is_empty() { String::new() } else { format!(" (found {})", unknowns.join(", ")) };
            return Err(self.err(format!("a PDE needs one unknown function with partial derivatives, like ∂u/∂{tv} and \
                                         ∂²u/∂{xv}²{found}"), q0.span, None));
        }
        let u = unknowns[0].clone();
        let mut found = vec![];
        let lhs = pde_rewrite(&q0.lhs, &u, &xv, &tv, &mut found).map_err(|m| self.err(m, q0.span, None))?;
        let rhs = pde_rewrite(&q0.rhs, &u, &xv, &tv, &mut found).map_err(|m| self.err(m, q0.span, None))?;
        let has = |k: &str| found.iter().any(|f| f == k);
        let order: u8 = if has(&format!("{u}__t2")) {
            2
        } else if has(&format!("{u}__t1")) {
            1
        } else {
            0
        };
        if order == 0 {
            return Err(self.err(format!("a PDE needs a time derivative ∂{u}/∂{tv} (or ∂²{u}/∂{tv}²)"), q0.span, None));
        }
        let method = sv.method.clone().unwrap_or_else(|| "crank_nicolson".into()).to_lowercase();
        let Some(pmethod) = pde_method(&method) else {
            return Err(self.err(format!("unknown method '{}' for a PDE (use crank_nicolson, implicit or explicit)",
                                        sv.method.clone().unwrap_or_default()), s.span, None));
        };
        if order == 2 && sv.method.is_some() {
            return Err(self.err("the wave equation (second order in t) is solved with its own explicit scheme; leave out \
                                 using …", s.span, None));
        }
        let grid = self.whole(sv.grid.as_ref(), "the grid (number of intervals in x)", 4, 100000, 400, ctx)? as usize;
        let mut names_eq = free_names(&lhs);
        names_eq.extend(free_names(&rhs));
        let is_complex = names_eq.iter().any(|n| n == "i");
        if is_complex && order == 2 {
            return Err(self.err("a complex equation (with i) must be first order in t, like the Schrödinger equation",
                                q0.span, None));
        }

        // ranges
        let xa = self.expr(&sv.lo, ctx)?;
        let xb = self.expr(&sv.hi, ctx)?;
        self.need_num(&xa, &sv.lo, &format!("the start of the {xv} range"))?;
        self.need_num(&xb, &sv.hi, &format!("the end of the {xv} range"))?;
        let (da, db) = (dim_of(&xa.ty), dim_of(&xb.ty));
        self.unify_or(&da, &db, |c| format!("the {xv} range goes from {} to {}", c.desc(&da), c.desc(&db)), sv.lo.span,
                      None)?;
        let (lo2, hi2) = (sv.lo2.as_ref().unwrap(), sv.hi2.as_ref().unwrap());
        let t0 = self.expr(lo2, ctx)?;
        let t1 = self.expr(hi2, ctx)?;
        self.need_num(&t0, lo2, &format!("the start of the {tv} range"))?;
        self.need_num(&t1, hi2, &format!("the end of the {tv} range"))?;
        let (d0, d1) = (dim_of(&t0.ty), dim_of(&t1.ty));
        self.unify_or(&d0, &d1, |c| format!("the {tv} range goes from {} to {}", c.desc(&d0), c.desc(&d1)), lo2.span,
                      None)?;
        let (xdim, tdim) = (da.clone(), d0.clone());
        let mut step = None;
        if let Some(st) = &sv.step2 {
            let v = self.expr(st, ctx)?;
            self.need_num(&v, st, "the time step")?;
            let sd = dim_of(&v.ty);
            self.unify_or(&sd, &tdim, |c| format!("the step is {} but {tv} is {}", c.desc(&sd), c.desc(&tdim)), st.span,
                          None)?;
            step = Some(v);
        }
        let udim = DExpr::fresh();

        let conds = self.pde_conditions(sv, &initial, ctx, &u, &xv, &tv, &xa, &xb, &t0, &t1, &tdim, &xdim)?;
        let Some(ic) = conds.ic.clone() else {
            return Err(self.err(format!("missing the initial condition:  with {u}({xv}, {}) = …", src(lo2)), s.span, None));
        };
        if order == 2 && conds.v0.is_none() {
            return Err(self.err(format!("a wave equation also needs the initial velocity:  ∂{u}/∂{tv}({xv}, {}) = …  (0 for \
                                         a string released at rest)", src(lo2)), s.span, None));
        }
        if order == 1 {
            if let Some(v0) = &conds.v0 {
                return Err(self.err(format!("∂{u}/∂{tv} at the start is only given for a wave equation (second order in \
                                             {tv})"), v0.span, None));
            }
        }
        let missing: Vec<char> = ['a', 'b'].into_iter().filter(|e| !conds.bcs.iter().any(|b| b.0 == *e)).collect();
        if !missing.is_empty() {
            let wh: Vec<String> = missing.iter().map(|e| src(if *e == 'a' { &sv.lo } else { &sv.hi })).collect();
            return Err(self.err(format!("missing a boundary condition at {xv} = {}: give {u}(…, {tv}) = … (fixed value) or \
                                         ∂{u}/∂{xv}(…, {tv}) = … (flux; 0 for an insulated end)", wh.join(" and ")), s.span,
                                None));
        }

        // the probe f(x, [u, u_x, u_xx, t, u_t, i]) -> [rhs, u0, phase0, v0, left, right]
        let (lam, mut lctx) = self.new_lambda(I::LambdaKind::Ode, "pde", ctx);
        let xsym = self.lam_sym(&xv, Ty::Num(xdim.clone()), ctx);
        self.module.lambdas[lam].params = vec![xsym];
        self.bind(lctx.scope, &xv, Binding::Sym(xsym));
        let ut_name = if order == 2 { format!("{u}__t1") } else { "__ut_unused".into() };
        let x2 = xdim.mul(&xdim);
        let state = [(u.clone(), udim.clone()), (format!("{u}__x1"), udim.div(&xdim)), (format!("{u}__x2"), udim.div(&x2)),
                     (tv.clone(), tdim.clone()), (ut_name, udim.div(&tdim)),
                     ((if is_complex { "i" } else { "__i_unused" }).to_string(), DExpr::of(DIMLESS))];
        for (nm, d) in &state {
            let sym = self.lam_sym(nm, Ty::Num(d.clone()), ctx);
            self.module.lambdas[lam].state.push(sym);
            self.bind(lctx.scope, nm, Binding::Sym(sym));
        }
        let top_name = format!("{u}__t{order}");
        let tpow = if order == 2 { tdim.mul(&tdim) } else { tdim.clone() };
        let topdim = udim.div(&tpow);
        let top = self.lam_sym(&top_name, Ty::Num(topdim.clone()), ctx);
        self.module.lambdas[lam].locals.push(top);
        self.bind(lctx.scope, &top_name, Binding::Sym(top));

        // the initial value fixes u's units
        let (ic_rhs, phase) = if is_complex { split_phase(&ic.rhs) } else { (ic.rhs.clone(), None) };
        for side in std::iter::once(&ic_rhs).chain(phase.iter()) {
            if let Some(bad) = free_names(side).into_iter().find(|n| *n == u || n == "i") {
                let extra = if bad == "i" {
                    format!("; a complex initial value is written  A({xv}) exp(i φ({xv}))")
                } else {
                    String::new()
                };
                return Err(self.err(format!("the initial value can't use {bad}{extra}"), ic.rhs.span, None));
            }
        }
        let uv0 = self.expr(&ic_rhs, &mut lctx)?;
        self.need_num(&uv0, &ic.rhs, "the initial value")?;
        let d = dim_of(&uv0.ty);
        self.unify_or(&d, &udim, |_| "the initial value's units don't fit".into(), ic.rhs.span, None)?;
        let mut pv = ir(I::ExprKind::Const(0.0), Ty::Num(DExpr::of(DIMLESS)), s.span.line);
        if let Some(ph) = &phase {
            pv = self.expr(ph, &mut lctx)?;
            self.need_num(&pv, &ic.rhs, "the phase in exp(i …)")?;
            let d = dim_of(&pv.ty);
            self.unify_or(&d, &DExpr::of(DIMLESS), |c| format!("the phase in exp(i …) must be a plain number, not {}",
                                                               c.desc(&d)), ic.rhs.span, None)?;
        }

        let lv = self.expr(&lhs, &mut lctx)?;
        let rv = self.expr(&rhs, &mut lctx)?;
        self.need_num(&lv, &q0.lhs, "the left side")?;
        self.need_num(&rv, &q0.rhs, "the right side")?;
        let (ld, rd) = (dim_of(&lv.ty), dim_of(&rv.ty));
        if !self.u.unify(&ld, &rd) {
            return Err(self.err(format!("the two sides of this equation don't match: left is {}, right is {}",
                                        self.desc(&ld), self.desc(&rd)), q0.span, None));
        }
        self.scopes[lctx.scope].names.remove(&top_name);
        let sup = if order == 2 { "²" } else { "" };
        let iso = fermium_sym::isolate(&lhs, &rhs, &B::name(&top_name)).map_err(|_| {
            self.err(format!("can't solve this equation for ∂{sup}{u}/∂{tv}{sup}: it must appear linearly (like ∂u/∂t = D * \
                              ∂²u/∂x²)"), q0.span, None)
        })?;
        let body0 = self.expr(&iso, &mut lctx)?;
        let bd = dim_of(&body0.ty);
        if !self.u.unify(&bd, &topdim) {
            return Err(self.err(format!("∂{sup}{u}/∂{tv}{sup} works out to {} but should be {}", self.desc(&bd),
                                        self.desc(&topdim)), q0.span, None));
        }

        let mut vv = ir(I::ExprKind::Const(0.0), Ty::Num(udim.div(&tdim)), s.span.line);
        if let Some(v0) = &conds.v0 {
            if free_names(&v0.rhs).contains(&u) {
                return Err(self.err(format!("the initial velocity can't use {u}"), v0.rhs.span, None));
            }
            vv = self.expr(&v0.rhs, &mut lctx)?;
            self.need_num(&vv, &v0.rhs, "the initial velocity")?;
            let d = dim_of(&vv.ty);
            let want = udim.div(&tdim);
            self.unify_or(&d, &want, |c| format!("∂{u}/∂{tv} at the start should be {}, not {}", c.desc(&want), c.desc(&d)),
                          v0.rhs.span, None)?;
        }
        let mut bvals = vec![];
        let mut bc = (0u8, 0u8);
        for end in ['a', 'b'] {
            let (_, kind, cond) = conds.bcs.iter().find(|b| b.0 == end).unwrap().clone();
            if end == 'a' {
                bc.0 = kind;
            } else {
                bc.1 = kind;
            }
            if free_names(&cond.rhs).contains(&u) {
                return Err(self.err(format!("a boundary value can't use {u} itself"), cond.rhs.span, None));
            }
            let bv = self.expr(&cond.rhs, &mut lctx)?;
            self.need_num(&bv, &cond.rhs, "the boundary value")?;
            let want = if kind == 0 { udim.clone() } else { udim.div(&xdim) };
            let what = if kind == 0 { u.clone() } else { format!("∂{u}/∂{xv}") };
            let d = dim_of(&bv.ty);
            self.unify_or(&d, &want, |c| format!("{what} at the boundary should be {}, not {}", c.desc(&want), c.desc(&d)),
                          cond.rhs.span, None)?;
            bvals.push(bv);
        }
        let uhint = uv0.hint.clone();
        let bb = bvals.pop().unwrap();
        let ba = bvals.pop().unwrap();
        self.module.lambdas[lam].body = vec![body0, uv0, pv, vv, ba, bb];

        // results: the solution, and the ends of the grid kept in hidden variables
        let mut out = vec![];
        self.solve.infos.push(SolInfo { names: vec![u.clone()], t: tv.clone(), slots: 0, eigen: false, pde: true });
        let info_id = self.solve.infos.len() - 1;
        let sol_name = self.fresh_name("__pde");
        let sol_sym = self.new_sym(&sol_name, Ty::Sol(info_id), ctx);
        self.extra[sol_sym].assigned = true;
        let (xhint, thint) = (xa.hint.clone().or(xb.hint.clone()), t0.hint.clone().or(t1.hint.clone()));
        let mut ends = vec![];
        for val in [xa, xb] {
            let nm = self.fresh_name("__pdex");
            let sym = self.new_sym(&nm, Ty::Num(xdim.clone()), ctx);
            self.extra[sym].assigned = true;
            out.push(I::Stmt { kind: I::StmtKind::Assign(sym, val), line: s.span.line });
            ends.push(sym);
        }
        let sn = node_at(s.span);
        let xa_ref = self.var_ref(ends[0], ctx, &sn)?;
        let xb_ref = self.var_ref(ends[1], ctx, &sn)?;
        let mut tdep_names = names_eq.clone();
        for (_, _, cond) in &conds.bcs {
            tdep_names.extend(free_names(&cond.rhs));
        }
        let tname = self.text(&tv);
        let xname = self.text(&xv);
        let tfmt = self.fmt_of(&tdim, thint.clone());
        let extra = I::SolveExtra { rtol: 0.0, event: None, evtext: -1, tname, xname, tfmt, tdep: tdep_names.contains(&tv), grid,
                                    xa: Some(xa_ref), xb: Some(xb_ref), order, pmethod, bc, is_complex,
                                    line: s.span.line, ..Default::default() };
        out.push(I::Stmt { kind: I::StmtKind::Solve { sol: sol_sym, rhs: lam, y0: vec![], t0, t1, step,
                                                      method: "pde".into(), rtol: None, x: Box::new(extra) },
                           line: s.span.line });
        self.solve.pdes.push(PdeView { name: u.clone(), sol_sym, xa_sym: ends[0], xb_sym: ends[1], m: grid,
                                       ncomp: if is_complex { 2 } else { 1 }, udim, xdim, tdim, xname: xv, tname: tv,
                                       uhint, xhint, thint });
        self.bind(ctx.scope, &u, Binding::Pde(self.solve.pdes.len() - 1));
        Ok(out)
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

    /// u(x, t), ∂u/∂x(x, t), ∂u/∂t(x, t) of a PDE solution; a complex one gives a complex number.
    pub fn pde_call(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx, deriv: bool) -> CResult<Checked> {
        let Some((Binding::Pde(p), _)) = self.lookup(ctx.scope, name) else { unreachable!() };
        let view = self.solve.pdes[p].clone();
        let K::Call { func, args } = &e.kind else { unreachable!() };
        let nm = &view.name;
        let mut which = 0u8;
        let mut d = view.udim.clone();
        if deriv {
            let K::Deriv { var, order, .. } = &func.kind else { unreachable!() };
            if *order != 1 || (*var != view.xname && *var != view.tname) {
                return Err(self.err(format!("of a PDE solution, ∂{nm}/∂{} and ∂{nm}/∂{} can be evaluated", view.xname,
                                            view.tname), e.span, None));
            }
            which = if *var == view.xname { 1 } else { 2 };
            d = view.udim.div(if which == 1 { &view.xdim } else { &view.tdim });
        }
        if args.len() != 2 {
            return Err(self.err(format!("{nm} is a function of {x} and {t}: write {nm}({x}, {t})", x = view.xname,
                                        t = view.tname), e.span, None));
        }
        let x = self.expr(&args[0], ctx)?;
        let t = self.expr(&args[1], ctx)?;
        self.need_num(&x, &args[0], &view.xname)?;
        self.need_num(&t, &args[1], &view.tname)?;
        let (dx, dt) = (dim_of(&x.ty), dim_of(&t.ty));
        self.unify_or(&dx, &view.xdim, |c| format!("{nm}'s first argument is {}, {}, not {}", view.xname,
                                                   c.desc(&view.xdim), c.desc(&dx)), args[0].span, None)?;
        self.unify_or(&dt, &view.tdim, |c| format!("{nm}'s second argument is {}, {}, not {}", view.tname,
                                                   c.desc(&view.tdim), c.desc(&dt)), args[1].span, None)?;
        let xfmt = self.fmt_of(&view.xdim, view.xhint.clone());
        let tfmt = self.fmt_of(&view.tdim, view.thint.clone());
        let sref = self.var_ref(view.sol_sym, ctx, e)?;
        let I::ExprKind::Var(sol) = sref.kind else { unreachable!() };
        self.var_ref(view.xa_sym, ctx, e)?;
        self.var_ref(view.xb_sym, ctx, e)?;
        let line = e.span.line;
        let mut parts = vec![];
        for c in 0..view.ncomp {
            let mut n = ir(I::ExprKind::PdeEval { sol, xa: 0.0, xb: 0.0, m: view.m, comp0: c * (view.m + 1),
                                                  x: Box::new(x.clone()), t: Box::new(t.clone()), which, xfmt, tfmt },
                           Ty::Num(d.clone()), line);
            n.sf = None;
            n.hint = if which == 0 { view.uhint.clone() } else { None };
            parts.push(n);
        }
        if view.ncomp == 1 {
            return Ok(Checked::Val(parts.pop().unwrap()));
        }
        let hint = parts[0].hint.clone();
        let mut r = ir(I::ExprKind::Vec(parts), Ty::Complex(d), line); // a complex number (D91)
        r.sf = None;
        r.hint = hint;
        Ok(Checked::Val(r))
    }
}
