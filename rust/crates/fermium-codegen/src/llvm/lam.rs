//! Lambdas (integrands, summands, equations) and the calculus expressions that use them: ∫ and solve-for-x call
//! fermium-runtime's quadrature and root finder (through `rt::fm_quad` / `rt::fm_root`) with the lambda compiled
//! to a function `double(double x, env)`; Σ runs its terms in the compiled code. Semantics: eval_calc.rs.
//!
//! A lambda reads the enclosing function's variables through `env` (pointers to their slots, as a parallel for's
//! body does); its parameter and the variables its where-bindings make are its own locals.
use std::collections::HashSet;

use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::BasicValueEnum;

use fermium_ir::{expr_children, lambda_of, Expr, ExprKind, LambdaId, SymId};

use super::*;

/// The per-function state saved while another function (a lambda, a parallel for's body) is compiled.
pub(super) struct Saved<'c> {
    fnv: Option<FunctionValue<'c>>,
    entry_b: Option<Builder<'c>>,
    locals: HashMap<SymId, (PointerValue<'c>, Kind)>,
    overrides: HashMap<SymId, (PointerValue<'c>, Kind)>,
    err_bb: Option<BasicBlock<'c>>,
    loops: Vec<(BasicBlock<'c>, BasicBlock<'c>)>,
    ret_kind: Kind,
    block: Option<BasicBlock<'c>>,
    known_line: Option<u32>,
    ctx_ptr: PointerValue<'c>,
}

/// Every variable an expression uses (lambda bodies included), and every variable its where-bindings make.
pub(super) fn expr_syms(m: &Module, e: &Expr, used: &mut HashSet<SymId>, bound: &mut HashSet<SymId>) {
    match &e.kind {
        ExprKind::Var(s) | ExprKind::SolEval { sol: s, .. } | ExprKind::SolList { sol: s, .. }
        | ExprKind::PdeEval { sol: s, .. } => {
            used.insert(*s);
        }
        ExprKind::Let(binds, _) => bound.extend(binds.iter().map(|(s, _)| *s)),
        _ => {}
    }
    if let Some(l) = lambda_of(e) {
        let lam = &m.lambdas[l];
        bound.extend(lam.params.iter().copied());
        for b in &lam.body {
            expr_syms(m, b, used, bound);
        }
    }
    if let ExprKind::Root { scale: Some(l), .. } = &e.kind {
        for b in &m.lambdas[*l].body {
            expr_syms(m, b, used, bound);
        }
    }
    for c in expr_children(e) {
        expr_syms(m, c, used, bound);
    }
}

impl<'c, 'm> Gen<'c, 'm> {
    pub(super) fn save_fn(&mut self) -> Saved<'c> {
        Saved {
            fnv: self.fnv.take(),
            entry_b: self.entry_b.take(),
            locals: std::mem::take(&mut self.locals),
            overrides: std::mem::take(&mut self.overrides),
            err_bb: self.err_bb.take(),
            loops: std::mem::take(&mut self.loops),
            ret_kind: self.ret_kind,
            block: self.b.get_insert_block(),
            known_line: self.known_line,
            ctx_ptr: self.ctx_ptr,
        }
    }

    pub(super) fn restore_fn(&mut self, s: Saved<'c>) {
        self.fnv = s.fnv;
        self.entry_b = s.entry_b;
        self.locals = s.locals;
        self.overrides = s.overrides;
        self.err_bb = s.err_bb;
        self.loops = s.loops;
        self.ret_kind = s.ret_kind;
        self.b.position_at_end(s.block.unwrap());
        self.known_line = s.known_line;
        self.ctx_ptr = s.ctx_ptr;
    }

    /// Is this variable one of the current function's own (a local, or living in an override)?
    pub(super) fn is_local_here(&self, s: SymId) -> bool {
        self.overrides.contains_key(&s) || self.locals.contains_key(&s) || self.m.syms[s].func.is_some()
    }

    /// An array of pointers to the slots of `syms` in the current function.
    pub(super) fn build_env(&mut self, syms: &[SymId]) -> R<(PointerValue<'c>, Vec<Kind>)> {
        let ptrt = self.ptrt();
        let n = syms.len().max(1) as u32;
        let env = self.alloca(ptrt.array_type(n).into(), "env")?;
        let mut kinds = vec![];
        for (i, s) in syms.iter().enumerate() {
            let (p, k) = self.slot(*s)?;
            kinds.push(k);
            let at = unsafe { bl!(self.b.build_gep(ptrt.array_type(n), env, &[self.i64c(0), self.i64c(i as i64)], "e")) };
            self.st(at, p, "env")?;
        }
        Ok((env, kinds))
    }

    /// In a new function: reach `syms` through its env parameter.
    pub(super) fn bind_env(&mut self, env: PointerValue<'c>, syms: &[SymId], kinds: &[Kind]) -> R<()> {
        let ptrt = self.ptrt();
        let n = syms.len().max(1) as u32;
        for (i, (s, k)) in syms.iter().zip(kinds).enumerate() {
            let at = unsafe { bl!(self.b.build_gep(ptrt.array_type(n), env, &[self.i64c(0), self.i64c(i as i64)], "e")) };
            let p = self.ld(ptrt, at, "ep", "env")?.into_pointer_value();
            self.overrides.insert(*s, (p, *k));
        }
        Ok(())
    }

    /// Make `syms` locals of the current function (fresh slots, 0 / null).
    pub(super) fn own_locals(&mut self, syms: impl IntoIterator<Item = SymId>) -> R<()> {
        let mut v: Vec<SymId> = syms.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        for s in v {
            let k = kind_of(&self.m.syms[s].ty)?;
            if k == Kind::Void {
                continue;
            }
            let ty = self.llty(k);
            let p = self.alloca(ty, &self.m.syms[s].name.clone())?;
            let zero = match k {
                Kind::F => self.fconst(0.0).as_basic_value_enum(),
                _ => ty.const_zero(),
            };
            self.st(p, zero, "var")?;
            self.overrides.insert(s, (p, k));
        }
        Ok(())
    }

    /// A scalar lambda as a function `double(double x, ptr env)`, and its env (built in the current function).
    fn scalar_lambda(&mut self, lam: LambdaId) -> R<(FunctionValue<'c>, PointerValue<'c>)> {
        let m = self.m;
        let l = &m.lambdas[lam];
        if l.params.len() != 1 || l.body.len() != 1 {
            return Err("a lambda with other than one parameter".into());
        }
        let (mut used, mut bound) = (HashSet::new(), HashSet::new());
        expr_syms(m, &l.body[0], &mut used, &mut bound);
        bound.insert(l.params[0]);
        bound.extend(l.locals.iter().copied());
        let mut env_syms: Vec<SymId> =
            used.iter().copied().filter(|s| !bound.contains(s) && self.is_local_here(*s)).collect();
        env_syms.sort_unstable();
        let (env, kinds) = self.build_env(&env_syms)?;
        let f64t = self.f64t();
        let args: Vec<BasicMetadataTypeEnum> = vec![f64t.into(), self.ptrt().into()];
        self.par_count += 1;
        let lf = self.lm.add_function(&format!("lam{}.{}", lam, self.par_count), f64t.fn_type(&args, false),
                                      Some(Linkage::Internal));
        let saved = self.save_fn();
        let r = (|| -> R<()> {
            self.begin_fn(lf, Kind::F);
            let envp = lf.get_nth_param(1).unwrap().into_pointer_value();
            self.bind_env(envp, &env_syms, &kinds)?;
            self.own_locals(bound.iter().copied())?;
            let x = lf.get_nth_param(0).unwrap().into_float_value();
            self.store_var(l.params[0], fv(x))?;
            let v = self.expr(&l.body[0])?;
            let y = self.to_f(v)?;
            bl!(self.b.build_return(Some(&y)));
            self.finish_fn()
        })();
        self.restore_fn(saved);
        r?;
        Ok((lf, env))
    }

    /// The current line as a value (for a callback that reports errors and warnings at it).
    fn line_now(&mut self) -> R<IntValue<'c>> {
        Ok(match self.known_line {
            Some(l) => self.i32c(l as i64),
            None => self.line_val()?,
        })
    }

    pub(super) fn calculus(&mut self, e: &Expr) -> R<Val<'c>> {
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        match &e.kind {
            ExprKind::Integral { lam, lo, hi, xname, xfmt, soft, atol } => {
                let a = self.num(lo)?;
                let b = self.num(hi)?;
                let atol = if *soft {
                    self.fconst(-1.0)
                } else {
                    match atol {
                        Some(t) => self.num(t)?,
                        None => self.fconst(0.0),
                    }
                };
                let line = self.line_now()?;
                let (f, env) = self.scalar_lambda(*lam)?;
                let fp = f.as_global_value().as_pointer_value();
                let xn = self.fconst(xname.map(|i| i as f64).unwrap_or(-1.0));
                let xf = self.i64c(xfmt.map(|i| i as i64).unwrap_or(-1));
                let r = self.fcall("fm_quad", &[ctx, fp.into(), env.into(), a.into(), b.into(), atol.into(), xn.into(),
                                                xf.into(), line.into()])?;
                self.check_err()?;
                // eval_calc: the line when the integral started
                self.st(self.line_g.as_pointer_value(), line, "line")?;
                Ok(fv(r))
            }
            ExprKind::Root { lam, lo, hi, scale, tfmt } => {
                let a = self.num(lo)?;
                let b = self.num(hi)?;
                let line = self.line_now()?;
                let (f, env) = self.scalar_lambda(*lam)?;
                let (g, genv) = match scale {
                    Some(s) => {
                        let (g, genv) = self.scalar_lambda(*s)?;
                        (g.as_global_value().as_pointer_value(), genv)
                    }
                    None => (self.ptrt().const_null(), self.ptrt().const_null()),
                };
                let tf = self.i64c(tfmt.map(|i| i as i64).unwrap_or(-1));
                let fp = f.as_global_value().as_pointer_value();
                let r = self.fcall("fm_root", &[ctx, fp.into(), env.into(), g.into(), genv.into(), a.into(), b.into(),
                                                tf.into(), line.into()])?;
                self.check_err()?;
                self.st(self.line_g.as_pointer_value(), line, "line")?;
                Ok(fv(r))
            }
            ExprKind::Sum { lam, lo, hi, step } => {
                let a = self.num(lo)?;
                let b = self.num(hi)?;
                let st = match step {
                    Some(s) => self.num(s)?,
                    None => self.fconst(1.0),
                };
                // eval_calc: the count of for_count, its errors described by eval_calc::describe
                let nz = bl!(self.b.build_float_compare(FloatPredicate::ONE, st, self.fconst(0.0), "stepnz"));
                self.guard(nz, rt::E_SUM_STEP, st, self.fconst(0.0))?;
                let d = bl!(self.b.build_float_sub(b, a, "d"));
                let q = bl!(self.b.build_float_div(d, st, "q"));
                let q = bl!(self.b.build_float_add(q, self.fconst(1e-9), "q"));
                let n = self.intrinsic("llvm.floor", &[q])?;
                let n = bl!(self.b.build_float_add(n, self.fconst(1.0), "n1"));
                let neg = bl!(self.b.build_float_compare(FloatPredicate::OLT, n, self.fconst(0.0), "neg"));
                let n = bl!(self.b.build_select(neg, self.fconst(0.0), n, "n0")).into_float_value();
                let known = bl!(self.b.build_float_compare(FloatPredicate::ORD, n, n, "known"));
                self.guard(known, rt::E_SUM_RANGE, a, b)?;
                let n = self.intrinsic("llvm.minnum", &[n, self.fconst(2f64.powi(62))])?;
                let count = self.fptosi_sat(n)?;
                let line = self.line_now()?;
                let l = &self.m.lambdas[*lam];
                if l.params.len() != 1 || l.body.len() != 1 {
                    return Err("a sum with other than one variable".into());
                }
                let (p, body) = (l.params[0], &l.body[0]);
                let mut bound = HashSet::new();
                expr_syms(self.m, body, &mut HashSet::new(), &mut bound);
                // the summand's variable and where-bindings: slots of their own (so a sum inside a lambda works)
                let prev: Vec<(SymId, Option<(PointerValue, Kind)>)> =
                    std::iter::once(p).chain(bound.iter().copied()).map(|s| (s, self.overrides.get(&s).copied())).collect();
                self.own_locals(std::iter::once(p).chain(bound.iter().copied()))?;
                let acc = self.alloca(self.f64t().into(), "acc")?;
                bl!(self.b.build_store(acc, self.fconst(0.0)));
                let r = self.counted_loop(count, &[], |g, i| {
                    let fi = bl!(g.b.build_signed_int_to_float(i, g.f64t(), "fi"));
                    let x = bl!(g.b.build_float_mul(fi, st, "ist"));
                    let k = bl!(g.b.build_float_add(a, x, "k"));
                    g.store_var(p, fv(k))?;
                    let v = g.expr(body)?;
                    let y = g.to_f(v)?;
                    let s = bl!(g.b.build_load(g.f64t(), acc, "s")).into_float_value();
                    let s = bl!(g.b.build_float_add(s, y, "s"));
                    bl!(g.b.build_store(acc, s));
                    Ok(())
                });
                for (s, old) in prev {
                    match old {
                        Some(o) => self.overrides.insert(s, o),
                        None => self.overrides.remove(&s),
                    };
                }
                r?;
                self.st(self.line_g.as_pointer_value(), line, "line")?;
                self.known_line = None;
                let s = bl!(self.b.build_load(self.f64t(), acc, "sum")).into_float_value();
                Ok(fv(s))
            }
            _ => Err("this calculus expression isn't compiled".into()),
        }
    }

    fn num(&mut self, e: &Expr) -> R<FloatValue<'c>> {
        let v = self.expr(e)?;
        self.to_f(v)
    }
}
