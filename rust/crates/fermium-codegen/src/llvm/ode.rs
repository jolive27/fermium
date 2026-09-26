//! `solve` in the LLVM back end: the right-hand side (an ODE-kind lambda) compiled to a function
//! `void(double t, double* y, double* out, i64 nout, env)` that fermium-runtime's solvers call through
//! `solve_rt`; reading solutions (x(t), x'(t), lists of samples, PDE values). Semantics: eval_solve.rs.
use std::collections::HashSet;

use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::BasicValueEnum;

use fermium_ir::{Expr, ExprKind, LambdaId, Stmt, StmtKind, SymId};

use super::lam::expr_syms;
use super::*;
use crate::llvm::solve_rt::OdeSite;

fn slots(t: &Ty) -> usize {
    match t {
        Ty::Vec { n, .. } => *n,
        Ty::Complex(_) => 2,
        Ty::Mat { r, c, .. } => r * c,
        _ => 1,
    }
}

impl<'c, 'm> Gen<'c, 'm> {
    /// An ODE-kind lambda as a function, its env (built here) and the env's kinds. The env holds every variable
    /// the right side reads that isn't its own, module variables included, so the solution can keep a copy of
    /// them (eval_solve snapshot, D46).
    fn ode_lambda(&mut self, lam: LambdaId) -> R<(FunctionValue<'c>, PointerValue<'c>, Vec<Kind>)> {
        let m = self.m;
        let l = &m.lambdas[lam];
        let (mut used, mut bound) = (HashSet::new(), HashSet::new());
        for b in &l.body {
            expr_syms(m, b, &mut used, &mut bound);
        }
        bound.extend(l.params.iter().copied());
        bound.extend(l.state.iter().copied());
        bound.extend(l.locals.iter().copied());
        let mut env_syms: Vec<SymId> = used.iter().copied().filter(|s| !bound.contains(s)).collect();
        env_syms.sort_unstable();
        let (env, kinds) = self.build_env(&env_syms)?;
        let (f64t, ptrt, i64t) = (self.f64t(), self.ptrt(), self.cx.i64_type());
        let args: Vec<BasicMetadataTypeEnum> = vec![f64t.into(), ptrt.into(), ptrt.into(), i64t.into(), ptrt.into()];
        self.par_count += 1;
        let lf = self.lm.add_function(&format!("rhs{}.{}", lam, self.par_count),
                                      self.cx.void_type().fn_type(&args, false), Some(Linkage::Internal));
        let saved = self.save_fn();
        let r = (|| -> R<()> {
            self.begin_fn(lf, Kind::Void);
            let t = lf.get_nth_param(0).unwrap().into_float_value();
            let y = lf.get_nth_param(1).unwrap().into_pointer_value();
            let out = lf.get_nth_param(2).unwrap().into_pointer_value();
            let nout = lf.get_nth_param(3).unwrap().into_int_value();
            let envp = lf.get_nth_param(4).unwrap().into_pointer_value();
            self.bind_env(envp, &env_syms, &kinds)?;
            self.own_locals(bound.iter().copied())?;
            if let Some(&p) = l.params.first() {
                self.store_var(p, fv(t))?;
            }
            let mut i = 0usize;
            for &s in &l.state {
                let n = slots(&m.syms[s].ty);
                let mut xs = vec![];
                for k in 0..n {
                    let at = unsafe { bl!(self.b.build_gep(self.f64t(), y, &[self.i64c((i + k) as i64)], "y")) };
                    xs.push(bl!(self.b.build_load(self.f64t(), at, "yi")).into_float_value());
                }
                let v = if matches!(m.syms[s].ty, Ty::Num(_)) { fv(xs[0]) } else { self.make_vec(&xs)? };
                self.store_var(s, v)?;
                i += n;
            }
            let mut j = 0i64;
            for e in &l.body {
                let v = self.expr(e)?;
                let xs = match v.k {
                    Kind::V(_) => self.vec_elems(v)?,
                    _ => vec![self.to_f(v)?],
                };
                for x in xs {
                    // eval_solve ode_call: only the slots the caller asked for
                    let fits = bl!(self.b.build_int_compare(IntPredicate::SLT, self.i64c(j), nout, "fits"));
                    let (st, next) = (self.new_bb("out"), self.new_bb("next"));
                    bl!(self.b.build_conditional_branch(fits, st, next));
                    self.b.position_at_end(st);
                    let at = unsafe { bl!(self.b.build_gep(self.f64t(), out, &[self.i64c(j)], "o")) };
                    bl!(self.b.build_store(at, x));
                    bl!(self.b.build_unconditional_branch(next));
                    self.b.position_at_end(next);
                    j += 1;
                }
            }
            self.finish_fn()
        })();
        self.restore_fn(saved);
        r?;
        Ok((lf, env, kinds))
    }

    fn fn_ptr(&self, f: FunctionValue<'c>) -> BasicValueEnum<'c> {
        f.as_global_value().as_pointer_value().into()
    }

    pub(super) fn solve_stmt(&mut self, s: &Stmt) -> R<()> {
        let StmtKind::Solve { sol, rhs, y0, t0, t1, step, method, x, .. } = &s.kind else { unreachable!() };
        let m = self.m;
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        let site = OdeSite {
            method: method.clone(), rtol: x.rtol, atol: x.atol.clone(), tname: x.tname, evtext: x.evtext, tdep: x.tdep,
            tfmt: x.tfmt, env_kinds: vec![], nstates: x.nstates, grid: x.grid, eig_method: x.eig_method, order: x.order,
            pmethod: x.pmethod, bc: x.bc, is_complex: x.is_complex, xname: x.xname, pde_line: x.line,
        };
        let h = match method.as_str() {
            "eigen" => {
                let a = self.num_of(t0)?;
                let b = self.num_of(t1)?;
                self.set_line_now(s.line)?;
                let (f, env, kinds) = self.ode_lambda(*rhs)?;
                let id = self.add_site(OdeSite { env_kinds: kinds, ..site });
                let line = self.i32c(s.line as i64);
                self.call("fm_eigen", &[ctx, self.i64c(id).into(), self.fn_ptr(f), env.into(), a.into(), b.into(),
                                        line.into()])?.unwrap()
            }
            "pde" => {
                let a = self.num_of(t0)?;
                let b = self.num_of(t1)?;
                let st = match step {
                    Some(e) => self.num_of(e)?,
                    None => self.nan(),
                };
                let xa = self.num_of(x.xa.as_ref().ok_or("a PDE without its x range")?)?;
                let xb = self.num_of(x.xb.as_ref().ok_or("a PDE without its x range")?)?;
                self.set_line_now(s.line)?;
                let (f, env, kinds) = self.ode_lambda(*rhs)?;
                let id = self.add_site(OdeSite { env_kinds: kinds, ..site });
                let line = self.i32c(s.line as i64);
                self.call("fm_pde", &[ctx, self.i64c(id).into(), self.fn_ptr(f), env.into(), a.into(), b.into(),
                                      st.into(), xa.into(), xb.into(), line.into()])?.unwrap()
            }
            _ => {
                // y0 flattened (eval_solve flat), then t0, t1 and the step
                let mut ys = vec![];
                for e in y0 {
                    let v = self.expr(e)?;
                    match v.k {
                        Kind::V(_) => ys.extend(self.vec_elems(v)?),
                        _ => ys.push(self.to_f(v)?),
                    }
                }
                let a = self.num_of(t0)?;
                let b = self.num_of(t1)?;
                let h0 = match step {
                    Some(e) => self.num_of(e)?,
                    None => self.nan(),
                };
                self.set_line_now(s.line)?;
                let n = ys.len();
                let arr = self.f64t().array_type(n.max(1) as u32);
                let y0p = self.alloca(arr.into(), "y0")?;
                for (i, v) in ys.iter().enumerate() {
                    let at = unsafe { bl!(self.b.build_gep(arr, y0p, &[self.i64c(0), self.i64c(i as i64)], "y0i")) };
                    bl!(self.b.build_store(at, *v));
                }
                let (f, env, kinds) = self.ode_lambda(*rhs)?;
                let (evf, evenv) = match x.event {
                    Some(l) => {
                        let (g, genv, _) = self.ode_lambda(l)?;
                        (self.fn_ptr(g), genv.into())
                    }
                    None => (self.ptrt().const_null().into(), self.ptrt().const_null().into()),
                };
                let id = self.add_site(OdeSite { env_kinds: kinds, ..site });
                let line = self.i32c(s.line as i64);
                self.call("fm_ode", &[ctx, self.i64c(id).into(), self.fn_ptr(f), env.into(), evf, evenv, y0p.into(),
                                      self.i64c(n as i64).into(), a.into(), b.into(), h0.into(), line.into()])?.unwrap()
            }
        };
        self.check_err()?;
        self.set_line_now(s.line)?;
        let _ = m;
        self.store_var(*sol, Val { k: Kind::H, v: Some(h) })
    }

    fn add_site(&mut self, s: OdeSite) -> i64 {
        self.tables.ode_sites.push(s);
        self.tables.ode_sites.len() as i64 - 1
    }

    fn num_of(&mut self, e: &Expr) -> R<FloatValue<'c>> {
        let v = self.expr(e)?;
        self.to_f(v)
    }

    /// Set the run-time line (always, even if it is thought to be known: a callback follows).
    fn set_line_now(&mut self, line: u32) -> R<()> {
        if line != 0 {
            self.st(self.line_g.as_pointer_value(), self.i32c(line as i64), "line")?;
            self.known_line = Some(line);
        }
        Ok(())
    }

    fn handle_of(&mut self, sym: SymId) -> R<IntValue<'c>> {
        let v = self.load_var(sym)?;
        if v.k != Kind::H {
            return Err("a solution that isn't one".into());
        }
        Ok(v.v.unwrap().into_int_value())
    }

    pub(super) fn solution_expr(&mut self, e: &Expr) -> R<Val<'c>> {
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        match &e.kind {
            ExprKind::SolEval { sol, comp, t, use_dy, tfmt } => {
                let h = self.handle_of(*sol)?;
                let tv = self.expr(t)?;
                self.set_line_now(e.line)?;
                let line = self.line_val()?;
                let (c, u, f) = (self.i64c(*comp as i64), self.i32c(i64::from(*use_dy)), self.i64c(*tfmt as i64));
                let r = match tv.k {
                    Kind::L => Val { k: Kind::L, v: self.call("fm_sol_eval_list", &[ctx, h.into(), c.into(),
                                                                                    tv.v.unwrap(), u.into(), f.into(),
                                                                                    line.into()])? },
                    _ => {
                        let t = self.to_f(tv)?;
                        fv(self.fcall("fm_sol_eval", &[ctx, h.into(), c.into(), t.into(), u.into(), f.into(),
                                                       line.into()])?)
                    }
                };
                self.check_err()?;
                Ok(r)
            }
            ExprKind::SolList { sol, comp, what } => {
                let h = self.handle_of(*sol)?;
                let r = self.call("fm_sol_list", &[ctx, h.into(), self.i64c(*comp as i64).into(),
                                                   self.i64c(i64::from(*what)).into()])?;
                Ok(Val { k: Kind::L, v: r })
            }
            ExprKind::PdeEval { sol, m, comp0, x, t, which, xfmt, tfmt, .. } => {
                let h = self.handle_of(*sol)?;
                let xv = self.num_of(x)?;
                let tv = self.num_of(t)?;
                self.set_line_now(e.line)?;
                let line = self.line_val()?;
                let r = self.fcall("fm_pde_eval", &[ctx, h.into(), self.i64c(*m as i64).into(),
                                                    self.i64c(*comp0 as i64).into(), xv.into(), tv.into(),
                                                    self.i64c(i64::from(*which)).into(), self.i64c(*xfmt as i64).into(),
                                                    self.i64c(*tfmt as i64).into(), line.into()])?;
                self.check_err()?;
                Ok(fv(r))
            }
            ExprKind::OdeLinSolve { m, b, t, text, fmt } => {
                let n = b.len();
                let mut av = vec![];
                for x in m {
                    av.push(self.num_of(x)?);
                }
                let mut bv = vec![];
                for x in b {
                    bv.push(self.num_of(x)?);
                }
                let at = self.f64t().array_type((n * n).max(1) as u32);
                let bt = self.f64t().array_type(n.max(1) as u32);
                let (ap, bp, op) = (self.alloca(at.into(), "a")?, self.alloca(bt.into(), "b")?, self.alloca(bt.into(), "x")?);
                for (i, v) in av.iter().enumerate() {
                    let p = unsafe { bl!(self.b.build_gep(at, ap, &[self.i64c(0), self.i64c(i as i64)], "ai")) };
                    bl!(self.b.build_store(p, *v));
                }
                for (i, v) in bv.iter().enumerate() {
                    let p = unsafe { bl!(self.b.build_gep(bt, bp, &[self.i64c(0), self.i64c(i as i64)], "bi")) };
                    bl!(self.b.build_store(p, *v));
                }
                let sing = self.call("fm_odelin", &[self.i64c(n as i64).into(), ap.into(), bp.into(), op.into()])?
                    .unwrap().into_int_value();
                let bad = bl!(self.b.build_int_compare(IntPredicate::NE, sing, self.i32c(0), "sing"));
                let (fail, ok) = (self.new_bb("singular"), self.new_bb("solved"));
                self.cold_br(bad, fail, ok)?;
                // singular: the time is computed only now (eval_solve), then the error at the expression's line
                self.goto(fail);
                let tv = self.num_of(t)?;
                self.set_line_now(e.line)?;
                let line = self.line_val()?;
                self.call("fm_solve_error", &[ctx, self.i64c(fermium_runtime::numerics::err::ODE_SINGULAR).into(),
                                              tv.into(), self.fconst(*text as f64).into(),
                                              self.i64c(*fmt as i64).into(), line.into()])?;
                bl!(self.b.build_unconditional_branch(self.err_bb.unwrap()));
                self.goto(ok);
                let r = bl!(self.b.build_load(bt, op, "x"));
                let mut xs = vec![];
                for i in 0..n {
                    xs.push(bl!(self.b.build_extract_value(r.into_array_value(), i as u32, "xi")).into_float_value());
                }
                self.make_vec(&xs)
            }
            _ => Err("this solution expression isn't compiled".into()),
        }
    }
}
