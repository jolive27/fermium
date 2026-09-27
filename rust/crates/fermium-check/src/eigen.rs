//! Eigenvalue problems `solve -ħ²/(2m) * ψ'' + V(x) ψ = E ψ with ψ(a) = 0, ψ(b) = 0 for x from a to b lowest N`
//! (D82): a port of check_eigen and _whole in `fermium/m3solve.py`.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use fermium_syntax::ast::ExprKind as K;
use num_rational::Rational64;

use crate::checker::*;
use crate::solve::{find_derivs, ic_names, is_const, node_at, normalize_derivs, SolInfo};
use crate::stmts::ty_dim;
use crate::walk::free_names;

fn dim_of(t: &Ty) -> DExpr {
    ty_dim(t).unwrap_or_else(DExpr::fresh)
}

pub(crate) fn subscript(n: usize) -> String {
    n.to_string().chars().map(|c| char::from_u32('₀' as u32 + c.to_digit(10).unwrap()).unwrap()).collect()
}

impl Checker {
    /// A whole-number setting (lowest N, grid N): a constant from lo to hi, or the default.
    pub(crate) fn whole(&mut self, node: Option<&A::Expr>, what: &str, lo: i64, hi: i64, default: i64, ctx: &mut Ctx)
                        -> CResult<i64> {
        let Some(node) = node else { return Ok(default) };
        let v = self.expr(node, ctx)?;
        let ok = match (&v.kind, &v.ty) {
            (I::ExprKind::Const(x), Ty::Num(d)) => {
                *x == x.trunc() && (lo as f64) <= *x && *x <= hi as f64 && self.u.unify(d, &DExpr::of(DIMLESS))
            }
            _ => false,
        };
        if !ok {
            return Err(self.err(format!("{what} must be a whole number from {lo} to {hi}"), node.span, None));
        }
        Ok(is_const(&v).unwrap() as i64)
    }

    pub fn check_eigen(&mut self, s: &A::Stmt, sv: &A::Solve, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let x = sv.var.clone();
        if sv.step.is_some() || sv.tolerance.is_some() || sv.until.is_some() || sv.absolute.as_ref().is_some_and(|a| !a.is_empty()) {
            return Err(self.err("step, tolerance, absolute and until are for initial-value problems; an eigenvalue problem \
                                 (lowest N) takes  grid N  and  using matrix / using shooting", s.span, None));
        }
        if sv.equations.len() != 1 {
            return Err(self.err("an eigenvalue problem (lowest N) has one equation, like  -ħ²/(2m) * ψ'' + V(x) ψ = E ψ",
                                s.span, None));
        }
        let q0 = &sv.equations[0];
        let (ql, qr) = (normalize_derivs(&q0.lhs, &x), normalize_derivs(&q0.rhs, &x));
        let mut orders = vec![];
        let known = self.known_called(ctx, &[&ql, &qr], &ic_names(&sv.initial));
        find_derivs(&ql, &mut orders, &known);
        find_derivs(&qr, &mut orders, &known);
        if orders.len() != 1 {
            return Err(self.err("an eigenvalue problem needs one unknown function with a second derivative, like ψ''",
                                q0.span, None));
        }
        let (psi, order) = orders[0].clone();
        if order != 2 {
            return Err(self.err(format!("an eigenvalue problem needs the second derivative {psi}'' (this equation has \
                                         {psi}{})", "'".repeat(order as usize)), q0.span, None));
        }
        let method = sv.method.clone().unwrap_or_else(|| "matrix".into()).to_lowercase();
        if method != "matrix" && method != "shooting" {
            return Err(self.err(format!("unknown method '{}' for an eigenvalue problem (use matrix or shooting)",
                                        sv.method.clone().unwrap_or_default()), s.span, None));
        }
        let nstates = self.whole(sv.lowest.as_ref(), "the number of states after lowest", 1, 500, 1, ctx)? as usize;
        let grid = self.whole(sv.grid.as_ref(), "the grid (number of intervals)", 16, 200000, 2000, ctx)? as usize;

        let a = self.expr(&sv.lo, ctx)?;
        let b = self.expr(&sv.hi, ctx)?;
        self.need_num(&a, &sv.lo, "the start of the range")?;
        self.need_num(&b, &sv.hi, "the end of the range")?;
        let (da, db) = (dim_of(&a.ty), dim_of(&b.ty));
        self.unify_or(&da, &db, |c| format!("the range goes from {} to {}", c.desc(&da), c.desc(&db)), sv.lo.span, None)?;
        let xdim = da.clone();
        let psidim = DExpr::of(DIMLESS).mul(&xdim.pow(Rational64::new(-1, 2))); // normalised: ∫ψ² dx = 1

        // the boundary conditions: ψ = 0 at both ends
        let mut ends = 0;
        for ic in &sv.initial {
            let arg = match &ic.lhs.kind {
                K::Call { func, args } if args.len() == 1 && matches!(&func.kind, K::Name { name } if *name == psi) => {
                    &args[0]
                }
                _ => {
                    return Err(self.err(format!("the boundary conditions of an eigenvalue problem look like  {psi}(a) = 0, \
                                                 {psi}(b) = 0 (the ends of the range)"), ic.span, None))
                }
            };
            let v = self.expr(&ic.rhs, ctx)?;
            if is_const(&v) != Some(0.0) {
                return Err(self.err(format!("only {psi} = 0 at the ends is supported for now (a wall, or far enough out \
                                             that {psi} has died away)"), ic.rhs.span, None));
            }
            let at = self.expr(arg, ctx)?;
            self.need_num(&at, arg, "the position of the boundary condition")?;
            let dat = dim_of(&at.ty);
            self.unify_or(&dat, &xdim, |c| format!("the boundary condition is at {} but {x} is {}", c.desc(&dat),
                                                   c.desc(&xdim)), arg.span, None)?;
            if let (Some(atv), Some(av), Some(bv)) = (is_const(&at), is_const(&a), is_const(&b)) {
                let tol = 1e-12 * (av.abs() + bv.abs() + 1e-300);
                if (atv - av).abs() > tol && (atv - bv).abs() > tol {
                    return Err(self.err(format!("the boundary conditions must be at the ends of the range ({x} = start and \
                                                 {x} = end)"), arg.span, None));
                }
            }
            ends += 1;
        }
        if ends != 2 {
            return Err(self.err(format!("an eigenvalue problem needs {psi} at both ends:  with {psi}({}) = 0, {psi}({}) = 0",
                                        fermium_sym::to_source(&sv.lo), fermium_sym::to_source(&sv.hi)), s.span, None));
        }

        // the eigenvalue: the one name in the equation that isn't defined yet
        let mut called: Vec<String> = vec![];
        for side in [&ql, &qr] {
            for n in side.walk() {
                if let K::Call { func, .. } = &n.kind {
                    if let K::Name { name } = &func.kind {
                        called.push(name.clone());
                    }
                }
            }
        }
        let mut unknown: Vec<String> = vec![];
        for n in free_names(&ql).into_iter().chain(free_names(&qr)) {
            if n == psi || n == x || unknown.contains(&n) || called.contains(&n) || crate::builtins::is_builtin(&n) {
                continue;
            }
            if self.lookup(ctx.scope, &n).is_none() {
                unknown.push(n);
            }
        }
        if unknown.is_empty() {
            return Err(self.err(format!("an eigenvalue problem needs an unknown constant, like E in  … = E {psi}; every \
                                         name here already has a value (use a new name for the eigenvalue)"), q0.span,
                                None));
        }
        if unknown.len() > 1 {
            return Err(self.err(format!("this equation has {} undefined names ({}); an eigenvalue problem has exactly one \
                                         unknown constant (the eigenvalue)", unknown.len(), unknown.join(", ")), q0.span,
                                None));
        }
        let ename = unknown[0].clone();
        let edim = DExpr::fresh();

        // the right-hand side f(x, [ψ, ψ', E]) -> [ψ', ψ'', 0]
        let (lam, mut lctx) = self.new_lambda(I::LambdaKind::Ode, "eigen", ctx);
        let xsym = self.lam_sym(&x, Ty::Num(xdim.clone()), ctx);
        self.module.lambdas[lam].params = vec![xsym];
        self.bind(lctx.scope, &x, Binding::Sym(xsym));
        let s_psi = self.lam_sym(&psi, Ty::Num(psidim.clone()), ctx);
        let s_dpsi = self.lam_sym(&format!("{psi}'"), Ty::Num(psidim.div(&xdim)), ctx);
        let s_e = self.lam_sym(&ename, Ty::Num(edim.clone()), ctx);
        for (sym, nm) in [(s_psi, psi.clone()), (s_dpsi, format!("{psi}'")), (s_e, ename.clone())] {
            self.module.lambdas[lam].state.push(sym);
            self.bind(lctx.scope, &nm, Binding::Sym(sym));
        }
        let x2 = xdim.mul(&xdim);
        let topdim = psidim.div(&x2);
        let top = self.lam_sym(&format!("{psi}''"), Ty::Num(topdim.clone()), ctx);
        self.module.lambdas[lam].locals.push(top);
        self.bind(lctx.scope, &format!("{psi}''"), Binding::Sym(top));
        let lv = self.expr(&ql, &mut lctx)?;
        let rv = self.expr(&qr, &mut lctx)?;
        self.need_num(&lv, &q0.lhs, "the left side")?;
        self.need_num(&rv, &q0.rhs, "the right side")?;
        let (ld, rd) = (dim_of(&lv.ty), dim_of(&rv.ty));
        if !self.u.unify(&ld, &rd) {
            return Err(self.err(format!("the two sides of this equation don't match: left is {}, right is {}",
                                        self.desc(&ld), self.desc(&rd)), q0.span, None));
        }
        self.scopes[lctx.scope].names.remove(&format!("{psi}''"));
        let target = fermium_sym::build::prime(fermium_sym::build::name(&psi), 2);
        let iso = fermium_sym::isolate(&ql, &qr, &target)?;
        let v = self.expr(&iso, &mut lctx)?;
        let q0n = node_at(q0.span);
        self.need_num(&v, &q0n, &format!("{psi}''"))?;
        let vd = dim_of(&v.ty);
        if !self.u.unify(&vd, &topdim) {
            return Err(self.err(format!("{psi}'' works out to {} but should be {}", self.desc(&vd), self.desc(&topdim)),
                                q0.span, None));
        }
        let sn = node_at(s.span);
        let dref = self.var_ref(s_dpsi, &mut lctx, &sn)?;
        let zero = ir(I::ExprKind::Const(0.0), Ty::Num(edim.div(&xdim)), s.span.line);
        self.module.lambdas[lam].body = vec![dref, v, zero];

        // the result: ψ_1 … ψ_N as solution views (with ψ_k' and ψ_k''), and E as a list
        let names: Vec<String> = (1..=nstates).map(|k| format!("{psi}_{k}")).collect();
        self.solve.infos.push(SolInfo { names: names.clone(), t: x.clone(), slots: 3 * nstates, eigen: true, pde: false });
        let info_id = self.solve.infos.len() - 1;
        let sol_name = self.fresh_name("__eig");
        let sol_sym = self.new_sym(&sol_name, Ty::Sol(info_id), ctx);
        self.extra[sol_sym].assigned = true;
        let thint = a.hint.clone().or(b.hint.clone());
        for (k, nm) in names.iter().enumerate() {
            let pretty = format!("{psi}{}", subscript(k + 1));
            let view = SolView { sol_sym, comp: 2 * k, top: 2 * k + 1, dim: psidim.clone(), tdim: xdim.clone(),
                                 tname: x.clone(), name: pretty, n: 1, stride: 1, cplx: false, hint: None, hints: vec![],
                                 thint: thint.clone(), sf: None , list: None };
            self.sols.push(view);
            self.bind(ctx.scope, nm, Binding::Sol(self.sols.len() - 1));
        }
        let tname = self.text(&x);
        let tfmt = self.fmt_of(&xdim, thint.clone());
        let extra = I::SolveExtra { rtol: 0.0, event: None, evtext: -1, tname, tfmt, nstates, grid,
                                    eig_method: u8::from(method == "shooting"), line: s.span.line, ..Default::default() };
        let st = I::Stmt { kind: I::StmtKind::Solve { sol: sol_sym, rhs: lam, y0: vec![], t0: a, t1: b, step: None,
                                                      method: "eigen".into(), rtol: None, x: Box::new(extra) },
                           line: s.span.line };
        // E = [E₁, …, E_N]: constant columns 2N … 3N-1 of the solution, read at x = a
        let at = self.expr(&sv.lo, ctx)?;
        let sref = self.var_ref(sol_sym, ctx, &sn)?;
        let I::ExprKind::Var(sref) = sref.kind else { unreachable!() };
        let items: Vec<I::Expr> = (0..nstates)
            .map(|k| ir(I::ExprKind::SolEval { sol: sref, comp: 2 * nstates + k, t: Box::new(at.clone()), use_dy: false,
                                               tfmt }, Ty::Num(edim.clone()), s.span.line))
            .collect();
        let mut lst = ir(I::ExprKind::List(items), Ty::List(edim), s.span.line);
        lst.sf = None;
        lst.hint = None;
        let asg = self.assign_to(&ename, lst, s.span, None, ctx)?;
        Ok(vec![st, asg])
    }
}
