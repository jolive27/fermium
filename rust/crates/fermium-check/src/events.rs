//! Conditions and events on the unknowns of a `solve` (spec C2): `if` conditions in the right side that depend on
//! the unknowns are frozen during each RK45 step and their switches located on the dense output (D296), and
//! `when lhs = rhs: x' = …` events change the state where they happen (D297). The solver side is in
//! fermium-runtime (ode.rs `Hooks`).
use std::collections::HashSet;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use fermium_syntax::ast::ExprKind as K;

use crate::checker::*;
use crate::solve::{dim_of, find_derivs, node_at, normalize_derivs, primes, promote, unit_primes, vec_n};
use crate::walk::free_names;

/// The conditions found in a right side: the lambda state slot flags and each condition's lhs − rhs.
#[derive(Default)]
pub(crate) struct Switches {
    pub syms: Vec<I::SymId>,
    pub gs: Vec<I::Expr>,
    pub ops: Vec<u8>,
}

fn contains_compare(e: &A::Expr) -> bool {
    matches!(&e.kind, K::Compare { .. }) || e.children().into_iter().any(contains_compare)
}

fn depends(e: &A::Expr, dep: &HashSet<String>) -> bool {
    free_names(e).iter().any(|n| dep.contains(n))
}

impl Checker {
    /// Rewrite each comparison `a op b` (op one of < > <= >=) that depends on the unknowns into a read of its
    /// branch flag μ in the state: `(μ > 1.5 and a op b) or (μ > 0.5 and μ < 1.5)` (flag 2: evaluated as written,
    /// 1: true, 0: false), with g = a − b typed in the right side's context (D296). One-line functions called with
    /// the unknowns are inlined when their body has a comparison; `where` bindings are carried into g.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sw_rewrite(&mut self, e: &A::Expr, dep: &HashSet<String>, wheres: &mut Vec<Vec<(String, A::Expr)>>,
                             sw: &mut Switches, lam: I::LambdaId, lctx: &mut Ctx, ctx: &Ctx, depth: u32) -> A::Expr {
        match &e.kind {
            K::Compare { op, left, right, tol: None }
                if ["<", ">", "<=", ">="].contains(&op.as_str()) && (depends(left, dep) || depends(right, dep)) =>
            {
                let mut g = fermium_sym::build::sub((**left).clone(), (**right).clone());
                for b in wheres.iter().rev() {
                    g = fermium_sym::build::mk(K::Where { value: Box::new(g), bindings: b.clone() });
                }
                let Ok(gv) = self.expr(&g, lctx) else { return e.clone() };
                if !matches!(gv.ty, Ty::Num(_)) {
                    return e.clone();
                }
                let j = sw.syms.len();
                let nm = format!("·branch{j}");
                let sym = self.lam_sym(&nm, Ty::Num(DExpr::of(DIMLESS)), ctx);
                self.module.lambdas[lam].state.push(sym);
                self.bind(lctx.scope, &nm, Binding::Sym(sym));
                sw.syms.push(sym);
                sw.gs.push(gv);
                sw.ops.push(match op.as_str() {
                    ">" => 0,
                    ">=" => 1,
                    "<" => 2,
                    _ => 3,
                });
                use fermium_sym::build::{mk, name, num};
                let cmp = |o: &str, v: f64| mk(K::Compare { op: o.into(), left: Box::new(name(&nm)), right: Box::new(num(v)),
                                                           tol: None });
                let live = mk(K::Logic { op: "and".into(), left: Box::new(cmp(">", 1.5)), right: Box::new(e.clone()) });
                let frozen = mk(K::Logic { op: "and".into(), left: Box::new(cmp(">", 0.5)), right: Box::new(cmp("<", 1.5)) });
                let mut out = mk(K::Logic { op: "or".into(), left: Box::new(live), right: Box::new(frozen) });
                out.span = e.span;
                out.paren = true;
                out
            }
            K::Where { value, bindings } => {
                let mut dep2 = dep.clone();
                let mut nb = vec![];
                for (n, v) in bindings {
                    let v2 = self.sw_rewrite(v, &dep2, wheres, sw, lam, lctx, ctx, depth);
                    if depends(v, &dep2) {
                        dep2.insert(n.clone());
                    }
                    nb.push((n.clone(), v2));
                }
                wheres.push(bindings.clone());
                let nv = self.sw_rewrite(value, &dep2, wheres, sw, lam, lctx, ctx, depth);
                wheres.pop();
                fermium_sym::build::with_kind(e, K::Where { value: Box::new(nv), bindings: nb })
            }
            K::Call { func, args } if depth < 8 && args.iter().any(|a| depends(a, dep)) => {
                if let K::Name { name: fname } = &func.kind {
                    if let Some((Binding::Func(b), _)) = self.lookup(lctx.scope, fname) {
                        if self.funcs[b].one_liner() {
                            if let Some(body) = self.body_expr(b) {
                                let params = self.func_params(b);
                                if contains_compare(&body) && params.len() == args.len()
                                    && params.iter().all(|p| p.unit.is_none())
                                {
                                    let map = params.iter().zip(args).map(|(p, a)| {
                                        let mut a = a.clone();
                                        a.paren = true;
                                        (p.name.clone(), a)
                                    }).collect();
                                    let mut inl = fermium_sym::subst(&body, &map);
                                    inl.paren = true;
                                    return self.sw_rewrite(&inl, dep, wheres, sw, lam, lctx, ctx, depth + 1);
                                }
                            }
                        }
                    }
                }
                fermium_sym::map_children(e, &mut |c| self.sw_rewrite(c, dep, wheres, sw, lam, lctx, ctx, depth))
            }
            _ => fermium_sym::map_children(e, &mut |c| self.sw_rewrite(c, dep, wheres, sw, lam, lctx, ctx, depth)),
        }
    }

    /// `when lhs op rhs: x = …, x' = …` (D297): its g lambda and its new-state lambda (the same state as the right
    /// side `lam`), checked like `until` (the unknowns and their derivatives below the highest, matching units).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_when(&mut self, w: &A::When, orders: &[(String, i64)], layout: &[(String, usize)],
                             state_syms: &[I::SymId], lam: I::LambdaId, lctx: &mut Ctx, t: &str)
                             -> CResult<I::WhenIr> {
        let unknowns: Vec<String> = orders.iter().map(|o| o.0.clone()).collect();
        let norm = |e: &A::Expr| normalize_derivs(&unit_primes(e, &unknowns), t);
        let (lhs, rhs) = (norm(&w.lhs), norm(&w.rhs));
        let below_top = |ck: &Checker, e: &A::Expr, what: &str| -> CResult<()> {
            let mut used = vec![];
            find_derivs(e, &mut used, &[]);
            for (x, k) in &used {
                if let Some((_, n)) = orders.iter().find(|o| o.0 == *x) {
                    if k >= n {
                        let have: Vec<String> = (0..*n as usize).map(|j| format!("{x}{}", primes(j))).collect();
                        return Err(ck.err(format!("{what} can use {} (not {x}{})", have.join(", "), primes(*k as usize)),
                                          w.span, None));
                    }
                }
            }
            Ok(())
        };
        below_top(self, &lhs, "the condition of when")?;
        below_top(self, &rhs, "the condition of when")?;
        let left = self.expr(&lhs, lctx)?;
        let right = self.expr(&rhs, lctx)?;
        self.need_num(&left, &w.lhs, "the left side of the condition of when")?;
        self.need_num(&right, &w.rhs, "the right side of the condition of when")?;
        let (ld, rd) = (dim_of(&left.ty), dim_of(&right.ty));
        self.unify_or(&ld, &rd, |c| format!("the two sides of the condition of when don't match: left is {}, right is {}",
                                            c.desc(&ld), c.desc(&rd)), w.span, None)?;
        let un = node_at(w.span);
        let g = self.arith("-", left, right, &un)?;
        // the new values, from the state just before the event
        let mut newv: Vec<Option<I::Expr>> = vec![None; layout.len()];
        for a in &w.assigns {
            let tgt = norm(&a.lhs);
            let (x, k) = match &tgt.kind {
                K::Name { name } => (name.clone(), 0usize),
                K::Prime { target, order } => match &target.kind {
                    K::Name { name } => (name.clone(), *order as usize),
                    _ => (String::new(), 0),
                },
                _ => (String::new(), 0),
            };
            let Some(slot) = layout.iter().position(|(n, kk)| *n == x && *kk == k) else {
                let what = if let Some((_, n)) = orders.iter().find(|o| o.0 == x) {
                    let have: Vec<String> = (0..*n as usize).map(|j| format!("{x}{}", primes(j))).collect();
                    format!("when can set {} (the highest derivative follows from the equation)", have.join(", "))
                } else {
                    let names: Vec<&str> = orders.iter().map(|o| o.0.as_str()).collect();
                    format!("when can set the unknowns ({}) and their derivatives below the highest", names.join(", "))
                };
                return Err(self.err(format!("can't set {} here: {what}", fermium_sym::to_source_p(&a.lhs, true)), a.span,
                                    Some("write e.g.  when y = 0 m: y' = -0.9 y'".into())));
            };
            if newv[slot].is_some() {
                return Err(self.err(format!("{x}{} is set twice in this when", primes(k)), a.span, None));
            }
            let rv = norm(&a.rhs);
            below_top(self, &rv, "the new values after when")?;
            let mut v = self.expr(&rv, lctx)?;
            let sty = self.module.syms[state_syms[slot]].ty.clone();
            if matches!(sty, Ty::Complex(_)) && matches!(v.ty, Ty::Num(_)) {
                v = promote(v);
            }
            if vec_n(&v.ty) != vec_n(&sty) || matches!(v.ty, Ty::List(_) | Ty::Mat { .. }) {
                let what = if vec_n(&sty) == 1 { "a number" } else { "a vector" };
                return Err(self.err(format!("{x}{} must be {what} like {x}{}", primes(k), primes(k)), a.rhs.span, None));
            }
            let (vd, sd) = (dim_of(&v.ty), dim_of(&sty));
            if !self.u.unify(&vd, &sd) {
                return Err(self.err(format!("{x}{} is {} but this is {}", primes(k), self.desc(&sd), self.desc(&vd)),
                                    a.rhs.span, None));
            }
            newv[slot] = Some(v);
        }
        let l = self.module.lambdas[lam].clone();
        let mut body = vec![];
        for (i, v) in newv.into_iter().enumerate() {
            body.push(match v {
                Some(v) => v,
                None => self.var_ref(state_syms[i], lctx, &un)?,
            });
        }
        for &s in &l.state[state_syms.len()..] {
            body.push(self.var_ref(s, lctx, &un)?); // the branch flags are recomputed by the solver
        }
        let l = &self.module.lambdas[lam];
        let (params, captures, locals, state) = (l.params.clone(), l.captures.clone(), l.locals.clone(), l.state.clone());
        let gname = self.fresh_name("when");
        self.module.lambdas.push(I::Lambda { kind: I::LambdaKind::Ode, name: gname, params: params.clone(),
                                             captures: captures.clone(), locals: locals.clone(), body: vec![g],
                                             state: state.clone(), col_syms: vec![], param_syms: vec![] });
        let gl = self.module.lambdas.len() - 1;
        let rname = self.fresh_name("when_reset");
        self.module.lambdas.push(I::Lambda { kind: I::LambdaKind::Ode, name: rname, params, captures, locals, body,
                                             state, col_syms: vec![], param_syms: vec![] });
        let rl = self.module.lambdas.len() - 1;
        let op = if w.op == "=" { "=".to_string() } else { w.op.clone() };
        let text = format!("the event (when {} {op} {}) happens again and again near {t} = ",
                           fermium_sym::to_source_p(&w.lhs, true), fermium_sym::to_source_p(&w.rhs, true));
        let tid = self.text(&text);
        let dir = match w.op.as_str() {
            ">" | ">=" => 1,
            "<" | "<=" => 2,
            _ => 0,
        };
        Ok(I::WhenIr { g: gl, reset: rl, dir, text: tid })
    }
}
