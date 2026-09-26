//! `parallel for` (M5, D152): a port of Checker.parallel_for, parallel_info and _par_syms from
//! `fermium/checker.py`. The iterations may set their own variables, write xs[i] of lists made before the loop,
//! and add to sums; anything else two iterations could both touch is a compile-time error.
use std::collections::{HashMap, HashSet};

use fermium_ir as I;
use fermium_ir::types::Ty;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;

const PAR_NO_BUILTIN: &[&str] = &["rand", "rand2", "randn", "randn2", "seed", "sample"];

fn banned_what(st: &I::Stmt) -> Option<&'static str> {
    Some(match &st.kind {
        I::StmtKind::Print(_) => "print",
        I::StmtKind::Plot(..) => "plot",
        I::StmtKind::Solve { .. } => "solve",
        I::StmtKind::Fit { .. } => "fit",
        I::StmtKind::Push(..) => "push",
        I::StmtKind::Clear(_) => "clear",
        I::StmtKind::Animate { .. } => "plot ... animate",
        I::StmtKind::Return(_) => "return",
        I::StmtKind::Break => "break",
        _ => return None,
    })
}

struct ParCheck<'a> {
    c: &'a Checker,
    node: A::Span,
    starts: HashMap<u32, (u32, u32)>,
    isym: I::SymId,
    private: HashSet<I::SymId>,
    owner: Owner,
}

impl ParCheck<'_> {
    fn err(&self, msg: String, line: Option<u32>, hint: Option<String>) -> Diagnostic {
        let mut e = self.c.err(msg, self.node, hint);
        if let Some(l) = line {
            if l != self.node.line {
                let (col, length) = self.starts.get(&l).copied().unwrap_or((1, 1));
                e.line = Some(l);
                e.col = Some(col);
                e.length = length;
            }
        }
        e
    }

    fn name(&self, s: I::SymId) -> &str {
        &self.c.module.syms[s].name
    }

    fn is_i(&self, ix: &I::Expr) -> bool {
        matches!(ix.kind, I::ExprKind::Var(s) if s == self.isym)
    }

    fn banned(&self, st: &I::Stmt, line: Option<u32>, who: Option<&str>) -> Result<(), Diagnostic> {
        if let Some(what) = banned_what(st) {
            let is_ret = matches!(st.kind, I::StmtKind::Return(_));
            if who.is_some() && is_ret {
                return Ok(());
            }
            if let Some(w) = who {
                return Err(self.err(format!("{w} uses {what}, so it can't be called inside a parallel for"), line, None));
            }
            let why = if matches!(st.kind, I::StmtKind::Break | I::StmtKind::Return(_)) {
                "the loop has to run to the end"
            } else {
                "the iterations run at the same time, in no fixed order"
            };
            return Err(self.err(format!("{what} can't be used inside a parallel for: {why}"), line,
                                Some("keep the values in a list with xs[i] = …, then use them after the loop".into())));
        }
        Ok(())
    }
}

/// Every variable used in an expression (lambda bodies included).
fn par_syms(c: &Checker, e: &I::Expr, out: &mut Vec<I::SymId>) {
    if let I::ExprKind::Var(s) = e.kind {
        out.push(s);
    }
    if let Some(l) = I::lambda_of(e) {
        for b in &c.module.lambdas[l].body {
            par_syms(c, b, out);
        }
    }
    for ch in I::expr_children(e) {
        par_syms(c, ch, out);
    }
}

impl Checker {
    #[allow(clippy::too_many_arguments)]
    pub fn parallel_for(&mut self, s: &A::Stmt, var: &str, lo: I::Expr, hi: I::Expr, st: Option<I::Expr>,
                        body: &[A::Stmt], ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if self.par_here(ctx).is_some() {
            return Err(self.err("a parallel for can't be inside another parallel for", s.span,
                                Some("make the inner loop a plain for; the outer loop already uses every core".into())));
        }
        if ctx.lam.is_some() {
            return Err(self.err("a parallel for can't be used here (inside an integral or equation)", s.span, None));
        }
        let before = self.scopes[ctx.scope].names.clone();
        self.par_stack.push((ctx.func, vec![]));
        // always a new variable: each iteration has its own i, which has no value after the loop
        let ld = crate::stmts::ty_dim(&lo.ty).unwrap();
        let sym = self.new_sym(var, Ty::Num(ld), ctx);
        self.bind(ctx.scope, var, Binding::Sym(sym));
        {
            let ms = &mut self.module.syms[sym];
            ms.hint = lo.hint.clone();
            ms.sf = None;
            ms.direct = 1;
        }
        self.extra[sym].assigned = true;
        ctx.loop_depth += 1;
        let reg = self.enter_region(ctx, "for", s.span.line);
        let b = self.block(body, ctx);
        self.exit_region(ctx, reg);
        ctx.loop_depth -= 1;
        let (_, private) = self.par_stack.pop().unwrap();
        let b = b?;
        for &p in &private {
            self.extra[p].region = None;
            self.extra[p].unset_msg = Some(format!("{} has no value here: it belongs to the iterations of the parallel \
                                                    for on line {}, so it isn't kept after the loop",
                                                   self.module.syms[p].name, s.span.line));
            let pname = self.module.syms[p].name.clone();
            if matches!(self.scopes[ctx.scope].names.get(&pname), Some(Binding::Sym(x)) if *x == p) {
                if let Some(old) = before.get(&pname) {
                    // the variable of that name from before the loop
                    self.scopes[ctx.scope].names.insert(pname, old.clone());
                }
            }
        }
        let info = self.parallel_info(sym, &b, &private, ctx.func, s)?;
        Ok(vec![I::Stmt { kind: I::StmtKind::For { sym, lo, hi, step: st, body: b, parallel: true,
                                                   par: Some(Box::new(info)) },
                          line: s.span.line }])
    }

    /// Check that the iterations of a parallel for can't interfere; returns what the back ends need.
    fn parallel_info(&mut self, isym: I::SymId, body: &[I::Stmt], private: &[I::SymId], owner: Owner, node: &A::Stmt)
                     -> CResult<I::ParInfo> {
        // where each statement of the body starts, so an error points at the statement (red team 5 #6)
        let mut starts: HashMap<u32, (u32, u32)> = HashMap::new();
        fn note(ss: &[A::Stmt], starts: &mut HashMap<u32, (u32, u32)>) {
            for st in ss {
                let (ln, col) = (st.span.line, st.span.col);
                if ln != 0 && col != 0 {
                    starts.entry(ln).or_insert((col, st.span.length.max(1)));
                }
                for b in crate::walk::stmt_blocks(st) {
                    note(b, starts);
                }
            }
        }
        if let A::StmtKind::For { body, .. } = &node.kind {
            note(body, &mut starts);
        }
        let mut privset: HashSet<I::SymId> = private.iter().copied().collect();
        privset.insert(isym);
        let pc = ParCheck { c: self, node: node.span, starts, isym, private: privset, owner };
        let mut reductions: Vec<I::SymId> = vec![];
        let mut written: Vec<I::SymId> = vec![];
        let mut red_ops: HashSet<*const I::Expr> = HashSet::new();

        fn stmts(pc: &ParCheck, ss: &[I::Stmt], reductions: &mut Vec<I::SymId>, written: &mut Vec<I::SymId>,
                 red_ops: &mut HashSet<*const I::Expr>) -> Result<(), Diagnostic> {
            for st in ss {
                let line = if st.line != 0 { Some(st.line) } else { None };
                pc.banned(st, line, None)?;
                match &st.kind {
                    I::StmtKind::For { par: Some(_), .. } => {
                        return Err(pc.err("a parallel for can't be inside another parallel for".into(), line, None));
                    }
                    I::StmtKind::For { sym, .. } | I::StmtKind::ForIn(sym, ..) if !pc.private.contains(sym) => {
                        return Err(pc.err(format!("{} is shared by all the iterations of this parallel for, so it can't \
                                                   be a loop variable inside it", pc.name(*sym)), line,
                                          Some("use a new name for the inner loop".into())));
                    }
                    I::StmtKind::Assign(sym, v) if !pc.private.contains(sym) => {
                        let ok = matches!(pc.c.module.syms[*sym].ty, Ty::Num(_))
                            && match &v.kind {
                                I::ExprKind::Bin(I::BinOp::Add | I::BinOp::Sub, a, b) => {
                                    matches!(a.kind, I::ExprKind::Var(x) if x == *sym) && {
                                        let mut us = vec![];
                                        par_syms(pc.c, b, &mut us);
                                        !us.contains(sym)
                                    }
                                }
                                _ => false,
                            };
                        if !ok {
                            let n = pc.name(*sym);
                            return Err(pc.err(format!("{n} is shared by all the iterations of this parallel for, so it \
                                                       can't be set inside it"), line,
                                              Some(format!("only sums are allowed:  {n} += …  (added up in a fixed \
                                                            order); or give each iteration its own new variable"))));
                        }
                        red_ops.insert(v as *const I::Expr);
                        if !reductions.contains(sym) {
                            reductions.push(*sym);
                        }
                    }
                    I::StmtKind::IndexAssign(sym, idx, _) => {
                        if pc.private.contains(sym) {
                            return Err(pc.err(format!("{} is made inside the parallel for; changing its elements there \
                                                       isn't supported", pc.name(*sym)), line,
                                              Some("build it with a formula instead".into())));
                        }
                        if !pc.is_i(idx) {
                            let (n, i) = (pc.name(*sym), pc.name(pc.isym));
                            return Err(pc.err(format!("each iteration of a parallel for may only write its own element, \
                                                       {n}[{i}]"), line,
                                              Some(format!("two iterations writing the same element would race; index \
                                                            with the loop variable {i}"))));
                        }
                        if !written.contains(sym) {
                            written.push(*sym);
                        }
                    }
                    _ => {}
                }
                for b in I::stmt_parts(st).1 {
                    stmts(pc, b, reductions, written, red_ops)?;
                }
            }
            Ok(())
        }
        stmts(&pc, body, &mut reductions, &mut written, &mut red_ops)?;
        let red_ids: HashSet<I::SymId> = reductions.iter().copied().collect();
        let w_ids: HashSet<I::SymId> = written.iter().copied().collect();
        let mut lists: Vec<I::SymId> = vec![];

        struct W<'a, 'b> {
            pc: &'a ParCheck<'b>,
            red_ids: &'a HashSet<I::SymId>,
            w_ids: &'a HashSet<I::SymId>,
            red_ops: &'a HashSet<*const I::Expr>,
            lists: Vec<I::SymId>,
        }
        impl W<'_, '_> {
            fn stmt(&mut self, st: &I::Stmt, line: u32, fstack: &mut Vec<I::FuncId>) -> Result<(), Diagnostic> {
                let line = if st.line != 0 { st.line } else { line };
                if let I::StmtKind::IndexAssign(sym, _, v) = &st.kind {
                    if self.w_ids.contains(sym) {
                        return self.expr(v, line, fstack);
                    }
                }
                let (es, bs) = I::stmt_parts(st);
                for e in es {
                    self.expr(e, line, fstack)?;
                }
                for b in bs {
                    for s in b {
                        self.stmt(s, line, fstack)?;
                    }
                }
                Ok(())
            }
            fn expr(&mut self, x: &I::Expr, line: u32, fstack: &mut Vec<I::FuncId>) -> Result<(), Diagnostic> {
                let pc = self.pc;
                let (n_i, isym) = (pc.name(pc.isym).to_string(), pc.isym);
                match &x.kind {
                    I::ExprKind::Index(l, idx) if fstack.is_empty() => {
                        if let I::ExprKind::Var(ls) = l.kind {
                            if self.w_ids.contains(&ls) {
                                if !matches!(idx.kind, I::ExprKind::Var(s) if s == isym) {
                                    let n = pc.name(ls);
                                    return Err(pc.err(format!("{n} is written by the iterations ({n}[{n_i}] = …), so \
                                                               inside the loop it can only be read as {n}[{n_i}]"),
                                                      Some(line), None));
                                }
                                return Ok(());
                            }
                        }
                    }
                    I::ExprKind::Bin(_, _, b) if self.red_ops.contains(&(x as *const I::Expr)) => {
                        return self.expr(b, line, fstack);
                    }
                    I::ExprKind::Var(sy) => {
                        let sy = *sy;
                        let n = pc.name(sy);
                        if self.red_ids.contains(&sy) {
                            return Err(pc.err(format!("the sum {n} can't be read inside the parallel for: its value is \
                                                       only known after the loop"), Some(line), None));
                        }
                        if self.w_ids.contains(&sy) {
                            return Err(pc.err(format!("{n} is written by the iterations ({n}[{n_i}] = …), so inside \
                                                       the loop it can only be read as {n}[{n_i}]"), Some(line), None));
                        }
                        let s = &pc.c.module.syms[sy];
                        let same_owner = match pc.owner {
                            Owner::Main => s.func.is_none(),
                            Owner::Func(f) => s.func == Some(f),
                        };
                        if matches!(s.ty, Ty::List(_)) && !pc.private.contains(&sy) && !self.lists.contains(&sy)
                            && (same_owner || matches!(s.storage, I::Storage::Global | I::Storage::Arena))
                        {
                            self.lists.push(sy);
                        }
                        return Ok(());
                    }
                    I::ExprKind::Builtin(name, _) if PAR_NO_BUILTIN.contains(&name.as_str()) => {
                        return Err(pc.err("random numbers can't be drawn inside a parallel for: the iterations run in \
                                           a different order each time, so the results would change from run to run"
                                              .into(), Some(line),
                                          Some("draw them before the loop, into a list".into())));
                    }
                    I::ExprKind::Call(f, _) | I::ExprKind::Map { func: f, .. } => {
                        let f = *f;
                        let fname = &pc.c.module.funcs[f].name;
                        let who = fname.split('.').next().unwrap_or(fname).to_string();
                        if fstack.contains(&f) {
                            return Err(pc.err(format!("{who} calls itself; a recursive function can't be used inside a \
                                                       parallel for"), Some(line), None));
                        }
                        let fbody = &pc.c.module.funcs[f].body;
                        fn all_banned(pc: &ParCheck, ss: &[I::Stmt], line: u32, who: &str) -> Result<(), Diagnostic> {
                            for st in ss {
                                pc.banned(st, Some(line), Some(who))?;
                            }
                            Ok(())
                        }
                        all_banned(pc, fbody, line, &who)?;
                        fstack.push(f);
                        for st in fbody {
                            self.stmt(st, line, fstack)?;
                        }
                        fstack.pop();
                    }
                    _ => {}
                }
                if let Some(l) = I::lambda_of(x) {
                    for b in &pc.c.module.lambdas[l].body {
                        self.expr(b, line, fstack)?;
                    }
                }
                for ch in I::expr_children(x) {
                    self.expr(ch, line, fstack)?;
                }
                Ok(())
            }
        }
        let mut w = W { pc: &pc, red_ids: &red_ids, w_ids: &w_ids, red_ops: &red_ops, lists: vec![] };
        let mut fstack = vec![];
        for st in body {
            w.stmt(st, node.span.line, &mut fstack)?;
        }
        lists.extend(w.lists);
        let mut pairs = vec![];
        let mut seen: HashSet<(I::SymId, I::SymId)> = HashSet::new();
        let mut texts = vec![];
        for &wv in &written {
            for &o in written.iter().chain(lists.iter()) {
                let key = (wv.min(o), wv.max(o));
                if o != wv && !seen.contains(&key) {
                    seen.insert(key);
                    texts.push((wv, o, format!("{} and {}", self.module.syms[wv].name, self.module.syms[o].name)));
                }
            }
        }
        for (wv, o, t) in texts {
            let id = self.text(&t);
            pairs.push((wv, o, id));
        }
        Ok(I::ParInfo { reductions, written, lists, alias: pairs })
    }
}
