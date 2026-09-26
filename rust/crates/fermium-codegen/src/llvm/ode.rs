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

/// The largest state a fixed-step RK4 solve compiles inline (rk4_inline; beyond it: fm_ode).
const RK4_INLINE_MAX: usize = 32;

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
        let StmtKind::Solve { sol, rhs, y0, t0, t1, step, method, x, .. } = &s.kind else {
            return Err("a solve that isn't one".into());
        };
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
                if method == "rk4" && x.event.is_none() && (1..=RK4_INLINE_MAX).contains(&n) {
                    self.rk4_inline(id, f, env, y0p, &ys, a, b, h0, line)?
                } else {
                self.call("fm_ode", &[ctx, self.i64c(id).into(), self.fn_ptr(f), env.into(), evf, evenv, y0p.into(),
                                      self.i64c(n as i64).into(), a.into(), b.into(), h0.into(), line.into()])?.unwrap()
                }
            }
        };
        self.check_err()?;
        self.set_line_now(s.line)?;
        let _ = m;
        self.store_var(*sol, Val { k: Kind::H, v: Some(h) })
    }

    /// A fixed-step RK4 solve with its steps taken here, the right side called directly (so LLVM inlines it) and
    /// the state in registers: fermium-runtime's ode::rk4_plain operation for operation (the same numbers), the
    /// samples written into arrays fm_rk4_begin reserved, fm_rk4_end for the error estimate and the solution.
    #[allow(clippy::too_many_arguments)]
    fn rk4_inline(&mut self, id: i64, f: FunctionValue<'c>, env: PointerValue<'c>, y0p: PointerValue<'c>,
                  ys: &[FloatValue<'c>], t0: FloatValue<'c>, t1: FloatValue<'c>, h0: FloatValue<'c>,
                  line: IntValue<'c>) -> R<BasicValueEnum<'c>> {
        let n = ys.len();
        let (f64t, ptrt, i64t) = (self.f64t(), self.ptrt(), self.cx.i64_type());
        let arr = f64t.array_type(n as u32);
        let k1a = self.alloca(arr.into(), "k1")?;
        let planty = self.cx.struct_type(&[ptrt.into(), ptrt.into(), ptrt.into(), f64t.into(), i64t.into(), ptrt.into()],
                                         false);
        let plan = self.alloca(planty.into(), "plan")?;
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        let (fp, nn) = (self.fn_ptr(f), self.i64c(n as i64));
        self.call("fm_rk4_begin", &[ctx, self.i64c(id).into(), fp, env.into(), y0p.into(), nn.into(), t0.into(),
                                    t1.into(), h0.into(), line.into(), k1a.into(), plan.into()])?;
        self.check_err()?;
        let field = |g: &mut Self, i: u32, ty: BasicTypeEnum<'c>| -> R<BasicValueEnum<'c>> {
            let at = bl!(g.b.build_struct_gep(planty, plan, i, "pf"));
            Ok(bl!(g.b.build_load(ty, at, "p")))
        };
        let tp = field(self, 0, ptrt.into())?.into_pointer_value();
        let yp = field(self, 1, ptrt.into())?.into_pointer_value();
        let dyp = field(self, 2, ptrt.into())?.into_pointer_value();
        let h = field(self, 3, f64t.into())?.into_float_value();
        let steps = field(self, 4, i64t.into())?.into_int_value();
        let solp = field(self, 5, ptrt.into())?;
        let elem = |g: &mut Self, p: PointerValue<'c>, i: usize| -> R<PointerValue<'c>> {
            Ok(unsafe { bl!(g.b.build_gep(arr, p, &[g.i64c(0), g.i64c(i as i64)], "e")) })
        };
        let mut k1 = vec![];
        for j in 0..n {
            let at = elem(self, k1a, j)?;
            k1.push(bl!(self.b.build_load(f64t, at, "k1")).into_float_value());
        }
        let bufs: Vec<PointerValue<'c>> = ["tmp", "k2", "k3", "k4", "yn", "kn"].iter()
            .map(|nm| self.alloca(arr.into(), nm)).collect::<R<_>>()?;
        let (tmp, k2a, k3a, k4a, yna, kna) = (bufs[0], bufs[1], bufs[2], bufs[3], bufs[4], bufs[5]);
        let half = bl!(self.b.build_float_mul(h, f64t.const_float(0.5), "half"));
        let h6 = bl!(self.b.build_float_div(h, f64t.const_float(6.0), "h6"));
        let last = bl!(self.b.build_int_sub(steps, self.i64c(1), "last"));
        // a sample: t, y and y' at index i
        let sample = |g: &mut Self, i: IntValue<'c>, t: FloatValue<'c>, y: &[FloatValue<'c>], k: &[FloatValue<'c>]| -> R<()> {
            let at = unsafe { bl!(g.b.build_gep(f64t, tp, &[i], "ts")) };
            g.st(at, t, "elem")?;
            let base = bl!(g.b.build_int_mul(i, g.i64c(n as i64), "row"));
            for j in 0..n {
                let ix = bl!(g.b.build_int_add(base, g.i64c(j as i64), "ix"));
                let at = unsafe { bl!(g.b.build_gep(f64t, yp, &[ix], "ys")) };
                g.st(at, y[j], "elem")?;
                let at = unsafe { bl!(g.b.build_gep(f64t, dyp, &[ix], "dys")) };
                g.st(at, k[j], "elem")?;
            }
            Ok(())
        };
        // tmp = y + c·k, then out = f(t, tmp)
        let stage = |g: &mut Self, y: &[FloatValue<'c>], c: FloatValue<'c>, k: &[FloatValue<'c>], t: FloatValue<'c>,
                     out: PointerValue<'c>| -> R<Vec<FloatValue<'c>>> {
            for j in 0..n {
                let ck = bl!(g.b.build_float_mul(c, k[j], "ck"));
                let v = bl!(g.b.build_float_add(y[j], ck, "tmp"));
                let at = elem(g, tmp, j)?;
                bl!(g.b.build_store(at, v));
            }
            g.rk4_call(f, t, tmp, out, nn, env)?;
            let mut r = vec![];
            for j in 0..n {
                let at = elem(g, out, j)?;
                r.push(bl!(g.b.build_load(f64t, at, "k")).into_float_value());
            }
            Ok(r)
        };
        let pre = self.b.get_insert_block().unwrap();
        let (body, exit) = (self.new_bb("rk4"), self.new_bb("rk4.end"));
        bl!(self.b.build_unconditional_branch(body));
        self.b.position_at_end(body);
        self.known_line = None;
        let sphi = bl!(self.b.build_phi(i64t, "s"));
        let yphi: Vec<_> = (0..n).map(|_| self.b.build_phi(f64t, "y")).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        let kphi: Vec<_> = (0..n).map(|_| self.b.build_phi(f64t, "k1")).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        let s = sphi.as_basic_value().into_int_value();
        let y: Vec<FloatValue> = yphi.iter().map(|p| p.as_basic_value().into_float_value()).collect();
        let k1v: Vec<FloatValue> = kphi.iter().map(|p| p.as_basic_value().into_float_value()).collect();
        let sf = bl!(self.b.build_unsigned_int_to_float(s, f64t, "sf"));
        let sh = bl!(self.b.build_float_mul(sf, h, "sh"));
        let t = bl!(self.b.build_float_add(t0, sh, "t"));
        sample(self, s, t, &y, &k1v)?;
        let th = bl!(self.b.build_float_add(t, half, "th"));
        let k2 = stage(self, &y, half, &k1v, th, k2a)?;
        let k3 = stage(self, &y, half, &k2, th, k3a)?;
        let tph = bl!(self.b.build_float_add(t, h, "tph"));
        let k4 = stage(self, &y, h, &k3, tph, k4a)?;
        let mut yn = vec![];
        for j in 0..n {
            let s23 = bl!(self.b.build_float_add(k2[j], k3[j], "s23"));
            let d23 = bl!(self.b.build_float_mul(f64t.const_float(2.0), s23, "d23"));
            let a = bl!(self.b.build_float_add(k1v[j], d23, "a"));
            let a = bl!(self.b.build_float_add(a, k4[j], "a"));
            let inc = bl!(self.b.build_float_mul(h6, a, "inc"));
            let v = bl!(self.b.build_float_add(y[j], inc, "yn"));
            let at = elem(self, yna, j)?;
            bl!(self.b.build_store(at, v));
            yn.push(v);
        }
        let s1 = bl!(self.b.build_int_add(s, self.i64c(1), "s1"));
        let s1f = bl!(self.b.build_unsigned_int_to_float(s1, f64t, "s1f"));
        let s1h = bl!(self.b.build_float_mul(s1f, h, "s1h"));
        let tnext = bl!(self.b.build_float_add(t0, s1h, "tnext"));
        let is_last = bl!(self.b.build_int_compare(IntPredicate::EQ, s, last, "islast"));
        let tn = bl!(self.b.build_select(is_last, t1, tnext, "tn")).into_float_value();
        self.rk4_call(f, tn, yna, kna, nn, env)?;
        let mut kn = vec![];
        for j in 0..n {
            let at = elem(self, kna, j)?;
            kn.push(bl!(self.b.build_load(f64t, at, "kn")).into_float_value());
        }
        let more = bl!(self.b.build_int_compare(IntPredicate::ULT, s1, steps, "more"));
        let latch = self.b.get_insert_block().unwrap();
        bl!(self.b.build_conditional_branch(more, body, exit));
        sphi.add_incoming(&[(&self.i64c(0), pre), (&s1, latch)]);
        for j in 0..n {
            yphi[j].add_incoming(&[(&ys[j], pre), (&yn[j], latch)]);
            kphi[j].add_incoming(&[(&k1[j], pre), (&kn[j], latch)]);
        }
        self.b.position_at_end(exit);
        sample(self, steps, t1, &yn, &kn)?;
        let r = self.call("fm_rk4_end", &[ctx, self.i64c(id).into(), fp, env.into(), solp, steps.into(), line.into()])?.unwrap();
        Ok(r)
    }

    /// out = f(t, y) inside the compiled RK4 loop; stop if the right side stopped the program.
    fn rk4_call(&mut self, f: FunctionValue<'c>, t: FloatValue<'c>, y: PointerValue<'c>, out: PointerValue<'c>,
                n: IntValue<'c>, env: PointerValue<'c>) -> R<()> {
        bl!(self.b.build_call(f, &[t.into(), y.into(), out.into(), n.into(), env.into()], ""));
        self.check_err()
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
