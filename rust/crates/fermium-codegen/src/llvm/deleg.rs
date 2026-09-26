//! Compiling a call into the tree-walker for a construct the back end doesn't compile itself (mixed mode, see
//! native::delegate): the variables it reads (and those the functions it calls read), passed as an env of slot
//! pointers; the ones it sets written back.
use std::collections::HashSet;

use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use fermium_ir::serde_like::Json;
use fermium_ir::{expr_children, lambda_of, stmt_parts, Expr, ExprKind, FuncId, LambdaId, Stmt, StmtKind, SymId};

use super::*;
use crate::native::delegate::InterpSite;

/// What a delegated construct touches.
#[derive(Default)]
struct Walk {
    syms: HashSet<SymId>,
    bound: HashSet<SymId>,
    funcs: HashSet<FuncId>,
    lambdas: HashSet<LambdaId>,
    /// it reads a solution in a way the tree-walker's copy of it can't answer (x'(t), a PDE's u(x, t))
    blocked: Option<&'static str>,
}

impl Walk {
    fn expr(&mut self, m: &Module, e: &Expr) {
        match &e.kind {
            ExprKind::Var(s) => {
                self.syms.insert(*s);
            }
            ExprKind::Let(binds, _) => self.bound.extend(binds.iter().map(|(s, _)| *s)),
            ExprKind::Call(f, _) | ExprKind::Map { func: f, .. } => {
                self.func(m, *f);
            }
            ExprKind::SolEval { sol, use_dy, .. } => {
                self.syms.insert(*sol);
                if *use_dy {
                    self.blocked = Some("x'(t) of a solution");
                }
            }
            ExprKind::SolList { sol, .. } => {
                self.syms.insert(*sol);
            }
            ExprKind::PdeEval { .. } => self.blocked = Some("a PDE's solution"),
            ExprKind::Root { scale: Some(l), .. } => self.lambda(m, *l),
            _ => {}
        }
        if let Some(l) = lambda_of(e) {
            self.lambda(m, l);
        }
        for c in expr_children(e) {
            self.expr(m, c);
        }
    }

    fn lambda(&mut self, m: &Module, l: LambdaId) {
        if !self.lambdas.insert(l) {
            return;
        }
        let lam = &m.lambdas[l];
        self.bound.extend(lam.params.iter().chain(lam.locals.iter()).chain(lam.state.iter()).copied());
        for b in &lam.body {
            self.expr(m, b);
        }
    }

    /// A user function the tree-walker will run: the module variables its body reads.
    fn func(&mut self, m: &Module, f: FuncId) {
        if !self.funcs.insert(f) {
            return;
        }
        let mut inner = Walk::default();
        inner.funcs = std::mem::take(&mut self.funcs);
        inner.block(m, &m.funcs[f].body);
        self.funcs = std::mem::take(&mut inner.funcs);
        self.syms.extend(inner.syms.into_iter().filter(|s| m.syms[*s].func.is_none()));
        self.lambdas.extend(inner.lambdas);
        if self.blocked.is_none() {
            self.blocked = inner.blocked;
        }
    }

    fn block(&mut self, m: &Module, body: &[Stmt]) {
        for s in body {
            match &s.kind {
                StmtKind::Assign(sym, _) | StmtKind::ForIn(sym, _, _) => {
                    self.bound.insert(*sym);
                }
                StmtKind::For { sym, .. } => {
                    self.bound.insert(*sym);
                }
                StmtKind::IndexAssign(sym, _, _) | StmtKind::Push(sym, _) | StmtKind::Clear(sym) => {
                    self.syms.insert(*sym);
                }
                StmtKind::Solve { .. } => self.blocked = Some("a solve in a function the tree-walker runs"),
                StmtKind::Fit { model, params, .. } => {
                    self.lambda(m, *model);
                    self.syms.extend(params.iter().copied());
                }
                _ => {}
            }
            let (es, blocks) = stmt_parts(s);
            for e in es {
                self.expr(m, e);
            }
            for b in blocks {
                self.block(m, b);
            }
        }
    }
}

/// The lambdas a plot samples ("lam" of its series in the plot table).
fn plot_lambdas(j: &Json, out: &mut Vec<LambdaId>) {
    match j {
        Json::Obj(kv) => {
            for (k, v) in kv {
                if k == "lam" {
                    if let Json::Num(x) = v {
                        out.push(*x as usize);
                    }
                }
                plot_lambdas(v, out);
            }
        }
        Json::List(v) => v.iter().for_each(|x| plot_lambdas(x, out)),
        _ => {}
    }
}

impl<'c, 'm> Gen<'c, 'm> {
    /// Hand a statement to the tree-walker (plot, fit, animate).
    pub(super) fn delegate_stmt(&mut self, s: &Stmt) -> R<()> {
        let mut w = Walk::default();
        let mut writes = vec![];
        match &s.kind {
            StmtKind::Plot(pid, _) => {
                let mut ls = vec![];
                plot_lambdas(&self.m.tables.plots[*pid], &mut ls);
                for l in ls {
                    w.lambda(self.m, l);
                }
            }
            // the fitted values and their standard errors (the hidden variables err(x) reads)
            StmtKind::Fit { params, errs, .. } => {
                writes.extend(params.iter().copied());
                writes.extend(errs.iter().copied());
            }
            StmtKind::Animate { anim_id, sol, .. } => {
                w.syms.insert(*sol);
                // the grid's ends, kept in hidden variables named in the animation's table entry (eval_data)
                let anims: Vec<&Json> = self.m.tables.plots.iter()
                    .filter(|p| matches!(p, Json::Obj(kv) if kv.iter().any(|(k, v)| k == "anim" && *v == Json::Bool(true))))
                    .collect();
                if let Some(Json::Obj(kv)) = anims.get(*anim_id) {
                    for (k, v) in kv.iter() {
                        if let ("xa" | "xb", Json::Num(x)) = (k.as_str(), v) {
                            w.syms.insert(*x as usize);
                        }
                    }
                }
            }
            _ => return Err("this statement can't be handed to the tree-walker".into()),
        }
        w.block(self.m, std::slice::from_ref(s));
        self.delegate(s as *const Stmt as usize, true, w, writes, Kind::Void).map(|_| ())
    }

    /// Hand an expression to the tree-walker (loading data, a function applied to a list, …).
    pub(super) fn delegate_expr(&mut self, e: &Expr) -> R<Val<'c>> {
        let ret = kind_of(&e.ty)?;
        let mut w = Walk::default();
        w.expr(self.m, e);
        self.set_line(e.line)?;
        self.delegate(e as *const Expr as usize, false, w, vec![], ret)
    }

    fn delegate(&mut self, ptr: usize, is_stmt: bool, w: Walk, writes: Vec<SymId>, ret: Kind) -> R<Val<'c>> {
        if self.aot.is_some() {
            return Err("parts of this program run in the tree-walker (plot, fit, data, …), which an executable \
                        doesn't carry yet".into());
        }
        if let Some(why) = w.blocked {
            return Err(format!("{why} inside a construct the tree-walker runs"));
        }
        let mut syms: Vec<SymId> = w.syms.iter().copied().filter(|s| !w.bound.contains(s)).collect();
        for s in &writes {
            if !syms.contains(s) {
                syms.push(*s);
            }
        }
        syms.sort_unstable();
        let mut kinds = vec![];
        for s in &syms {
            let k = kind_of(&self.m.syms[*s].ty)?;
            if k == Kind::Void {
                return Err("a variable without a value in a construct the tree-walker runs".into());
            }
            kinds.push(k);
        }
        let (env, _) = self.build_env(&syms)?;
        self.tables.interp_sites.push(InterpSite { ptr, is_stmt, syms: syms.iter().copied().zip(kinds).collect(),
                                                   writes, ret });
        let site = self.tables.interp_sites.len() as i64 - 1;
        let outty: BasicTypeEnum = match ret {
            Kind::V(m) => self.f64t().array_type(m as u32).into(),
            _ => self.cx.i64_type().into(),
        };
        let outp = self.alloca(outty, "iout")?;
        let line = match self.known_line {
            Some(l) => self.i32c(l as i64),
            None => self.line_val()?,
        };
        let ctx: BasicValueEnum = self.ctx_ptr.into();
        let endline = self.call("fm_interp", &[ctx, self.i64c(site).into(), env.into(), outp.into(), line.into()])?
            .unwrap().into_int_value();
        self.check_err()?;
        // the tree-walker's line when it finished (eval.rs keeps it)
        self.st(self.line_g.as_pointer_value(), endline, "line")?;
        self.known_line = None;
        let i64t = self.cx.i64_type();
        Ok(match ret {
            Kind::Void => Val { k: ret, v: None },
            Kind::V(_) => Val { k: ret, v: Some(bl!(self.b.build_load(outty, outp, "r"))) },
            _ => {
                let raw = bl!(self.b.build_load(i64t, outp, "r")).into_int_value();
                let v: BasicValueEnum = match ret {
                    Kind::F => bl!(self.b.build_bit_cast(raw, self.f64t(), "f")),
                    Kind::B => bl!(self.b.build_int_compare(IntPredicate::NE, raw, i64t.const_zero(), "b")).into(),
                    Kind::L | Kind::TL => bl!(self.b.build_int_to_ptr(raw, self.ptrt(), "p")).into(),
                    _ => raw.into(),
                };
                Val { k: ret, v: Some(v) }
            }
        })
    }
}
