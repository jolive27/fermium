//! `parallel for` (M5, D152): the body becomes a function of (env, first, end, partial sums, lo, step) that runs
//! the iterations of one block; `rt::fm_par_run` hands the blocks of `fermium_ir::par_blocks` to threads. Each
//! block's sums start from 0 and the blocks' sums are added in block order afterwards, as the tree-walker does
//! (eval_par.rs), so the numbers don't depend on the number of threads.
//!
//! The iterations' own variables (the loop variable, the sums, everything the body assigns) are locals of the
//! body function; the enclosing function's other locals are reached through `env` (pointers to their slots);
//! module variables are globals.
use std::collections::HashSet;

use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;

use fermium_ir::{expr_children, lambda_of, stmt_parts, Expr, ExprKind, ParInfo, Stmt, StmtKind, SymId};

use super::*;

/// Most blocks of a parallel for (fermium_ir::par_blocks).
const PAR_BLOCKS: u64 = 256;

fn assigned_expr(m: &Module, e: &Expr, out: &mut HashSet<SymId>) {
    if let ExprKind::Let(binds, _) = &e.kind {
        out.extend(binds.iter().map(|(s, _)| *s));
    }
    if let Some(l) = lambda_of(e) {
        for b in &m.lambdas[l].body {
            assigned_expr(m, b, out);
        }
    }
    for c in expr_children(e) {
        assigned_expr(m, c, out);
    }
}

/// Every variable a block (re)binds.
fn assigned(m: &Module, body: &[Stmt], out: &mut HashSet<SymId>) {
    for s in body {
        match &s.kind {
            StmtKind::Assign(sym, _) | StmtKind::ForIn(sym, _, _) => {
                out.insert(*sym);
            }
            StmtKind::For { sym, .. } => {
                out.insert(*sym);
            }
            _ => {}
        }
        let (es, blocks) = stmt_parts(s);
        for e in es {
            assigned_expr(m, e, out);
        }
        for b in blocks {
            assigned(m, b, out);
        }
    }
}

fn used_expr(m: &Module, e: &Expr, out: &mut HashSet<SymId>) {
    if let ExprKind::Var(s) = e.kind {
        out.insert(s);
    }
    if let Some(l) = lambda_of(e) {
        for b in &m.lambdas[l].body {
            used_expr(m, b, out);
        }
    }
    for c in expr_children(e) {
        used_expr(m, c, out);
    }
}

/// Every variable a block reads or writes.
fn used(m: &Module, body: &[Stmt], out: &mut HashSet<SymId>) {
    for s in body {
        match &s.kind {
            StmtKind::IndexAssign(sym, _, _) | StmtKind::Push(sym, _) | StmtKind::Clear(sym) => {
                out.insert(*sym);
            }
            _ => {}
        }
        let (es, blocks) = stmt_parts(s);
        for e in es {
            used_expr(m, e, out);
        }
        for b in blocks {
            used(m, b, out);
        }
    }
}

impl<'c, 'm> Gen<'c, 'm> {
    pub(super) fn parallel_for(&mut self, sym: SymId, lo: &Expr, hi: &Expr, step: Option<&Expr>, body: &[Stmt],
                               info: &ParInfo) -> R<()> {
        let m = self.m;
        let (lo, st, count) = self.range_count(lo, hi, step)?;
        // lists written as xs[i] must not be the same list as another one the loop uses
        for &(w, o, text) in &info.alias {
            let (pw, kw) = self.slot(w)?;
            let (po, ko) = self.slot(o)?;
            if kw == Kind::L && ko == Kind::L {
                let a = bl!(self.b.build_load(self.ptrt(), pw, "a")).into_pointer_value();
                let b = bl!(self.b.build_load(self.ptrt(), po, "b")).into_pointer_value();
                let same = bl!(self.b.build_int_compare(IntPredicate::EQ, a, b, "same"));
                let ok = bl!(self.b.build_not(same, "ok"));
                let (t, z) = (self.fconst(text as f64), self.fconst(0.0));
                self.guard(ok, rt::E_PAR_ALIAS, t, z)?;
            }
        }
        let mut own: HashSet<SymId> = HashSet::new();
        assigned(m, body, &mut own);
        own.insert(sym);
        own.extend(info.reductions.iter().copied());
        let mut uses = HashSet::new();
        used(m, body, &mut uses);
        // the enclosing function's locals the body reads: passed as pointers
        let mut env_syms: Vec<SymId> = uses.iter().copied()
            .filter(|s| !own.contains(s) && (self.overrides.contains_key(s) || m.syms[*s].func.is_some()))
            .collect();
        env_syms.sort_unstable();
        let ptrt = self.ptrt();
        let nenv = env_syms.len().max(1) as u32;
        let env = self.alloca(ptrt.array_type(nenv).into(), "env")?;
        let mut env_kinds = vec![];
        for (i, s) in env_syms.iter().enumerate() {
            let (p, k) = self.slot(*s)?;
            env_kinds.push(k);
            let at = unsafe { bl!(self.b.build_gep(ptrt.array_type(nenv), env, &[self.i64c(0), self.i64c(i as i64)], "e")) };
            bl!(self.b.build_store(at, p));
        }
        // the body function
        let nr = info.reductions.len();
        let (i64t, f64t) = (self.cx.i64_type(), self.f64t());
        let args: Vec<BasicMetadataTypeEnum> = vec![ptrt.into(), i64t.into(), i64t.into(), ptrt.into(), f64t.into(),
                                                    f64t.into()];
        let fty = self.cx.void_type().fn_type(&args, false);
        self.par_count += 1;
        let pf = self.lm.add_function(&format!("par{}", self.par_count), fty, Some(Linkage::Internal));
        let saved = self.save_fn();
        let r = self.par_body(pf, sym, body, info, &env_syms, &env_kinds, &own, nenv);
        self.restore_fn(saved);
        self.known_line = None;
        r?;
        // run the blocks (the recursion check is off meanwhile: the threads have stacks of their own)
        let parts = self.alloca(f64t.array_type((PAR_BLOCKS as usize * nr.max(1)) as u32).into(), "parts")?;
        let sbp = self.stackbase_g.as_pointer_value();
        let sb = bl!(self.b.build_load(i64t, sbp, "sb"));
        bl!(self.b.build_store(sbp, i64t.const_zero()));
        let fptr = pf.as_global_value().as_pointer_value();
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        self.call("fm_par_run", &[ctx, fptr.into(), env.into(), count.into(), lo.into(), st.into(), parts.into(),
                                  self.i64c(nr as i64).into()])?;
        bl!(self.b.build_store(sbp, sb));
        self.known_line = None;
        self.check_err()?;
        // the sums: each start value plus the blocks' sums in block order
        if nr > 0 {
            let lt = bl!(self.b.build_int_compare(IntPredicate::SLT, count, self.i64c(PAR_BLOCKS as i64), "lt"));
            let nb = bl!(self.b.build_select(lt, count, self.i64c(PAR_BLOCKS as i64), "nb")).into_int_value();
            let accs: Vec<PointerValue> = (0..nr).map(|_| self.alloca(f64t.into(), "acc")).collect::<R<_>>()?;
            for (k, r) in info.reductions.iter().enumerate() {
                let v = self.load_var(*r)?;
                let x = self.to_f(v)?;
                bl!(self.b.build_store(accs[k], x));
            }
            let partt = f64t.array_type((PAR_BLOCKS as usize * nr) as u32);
            self.counted_loop(nb, &[], |g, b| {
                for (k, acc) in accs.iter().enumerate() {
                    let idx = bl!(g.b.build_int_mul(b, g.i64c(nr as i64), "bi"));
                    let idx = bl!(g.b.build_int_add(idx, g.i64c(k as i64), "bk"));
                    let at = unsafe { bl!(g.b.build_gep(partt, parts, &[g.i64c(0), idx], "p")) };
                    let x = bl!(g.b.build_load(g.f64t(), at, "x")).into_float_value();
                    let a = bl!(g.b.build_load(g.f64t(), *acc, "a")).into_float_value();
                    let s = bl!(g.b.build_float_add(a, x, "s"));
                    bl!(g.b.build_store(*acc, s));
                }
                Ok(())
            })?;
            for (k, r) in info.reductions.iter().enumerate() {
                let a = bl!(self.b.build_load(f64t, accs[k], "a")).into_float_value();
                self.store_var(*r, fv(a))?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn par_body(&mut self, pf: FunctionValue<'c>, sym: SymId, body: &[Stmt], info: &ParInfo, env_syms: &[SymId],
                env_kinds: &[Kind], own: &HashSet<SymId>, nenv: u32) -> R<()> {
        self.begin_fn(pf, Kind::Void);
        let ptrt = self.ptrt();
        let env = pf.get_nth_param(0).unwrap().into_pointer_value();
        let first = pf.get_nth_param(1).unwrap().into_int_value();
        let end = pf.get_nth_param(2).unwrap().into_int_value();
        let part = pf.get_nth_param(3).unwrap().into_pointer_value();
        let lo = pf.get_nth_param(4).unwrap().into_float_value();
        let st = pf.get_nth_param(5).unwrap().into_float_value();
        for (i, (s, k)) in env_syms.iter().zip(env_kinds).enumerate() {
            let at = unsafe { bl!(self.b.build_gep(ptrt.array_type(nenv), env, &[self.i64c(0), self.i64c(i as i64)], "e")) };
            let p = bl!(self.b.build_load(ptrt, at, "ep")).into_pointer_value();
            self.overrides.insert(*s, (p, *k));
        }
        let mut own: Vec<SymId> = own.iter().copied().collect();
        own.sort_unstable();
        for s in own {
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
            bl!(self.b.build_store(p, zero));
            self.overrides.insert(s, (p, k));
        }
        // iterations first..end
        let n = bl!(self.b.build_int_sub(end, first, "n"));
        self.counted_loop(n, body, |g, j| {
            let i = bl!(g.b.build_int_add(first, j, "i"));
            let fi = bl!(g.b.build_signed_int_to_float(i, g.f64t(), "fi"));
            let x = bl!(g.b.build_float_mul(fi, st, "ist"));
            let x = bl!(g.b.build_float_add(lo, x, "x"));
            g.store_var(sym, fv(x))
        })?;
        for (k, r) in info.reductions.iter().enumerate() {
            let v = self.load_var(*r)?;
            let x = self.to_f(v)?;
            let at = unsafe { bl!(self.b.build_gep(self.f64t(), part, &[self.i64c(k as i64)], "p")) };
            bl!(self.b.build_store(at, x));
        }
        self.finish_fn()
    }
}
