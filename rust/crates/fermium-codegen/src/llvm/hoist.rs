//! Reading list headers once per loop: in a loop body that can't change any list's length or move its numbers
//! (no push, clear, user function call or anything else that could reach a list), the data pointer and the length
//! of each list variable it indexes are loaded before the loop. LLVM would do this itself if it could prove the
//! loads safe to move, which it can't after the index checks' early exits.
use std::collections::HashSet;

use fermium_ir::{expr_children, lambda_of, stmt_parts, Expr, ExprKind, Stmt, StmtKind, SymId};

use super::*;

/// A small block of plain statements (compiled twice by versioned_loop): assignments, ifs, prints, asserts, no
/// loop, call, integrand, sum, root or solve.
fn straight_block(body: &[Stmt], n: &mut usize) -> bool {
    fn plain(e: &Expr) -> bool {
        lambda_of(e).is_none() && !matches!(e.kind, ExprKind::Call(..) | ExprKind::Map { .. })
            && expr_children(e).into_iter().all(plain)
    }
    body.iter().all(|s| {
        *n += 1;
        let ok = match &s.kind {
            StmtKind::Assign(..) | StmtKind::IndexAssign(..) | StmtKind::Expr(_) | StmtKind::Assert(..)
            | StmtKind::Print(_) | StmtKind::Break | StmtKind::Continue | StmtKind::If(..) => true,
            StmtKind::Push(..) | StmtKind::Clear(_) | StmtKind::While(..) | StmtKind::For { .. }
            | StmtKind::ForIn(..) | StmtKind::Plot(..) | StmtKind::Solve { .. } | StmtKind::Fit { .. }
            | StmtKind::Return(_) | StmtKind::Animate { .. } | StmtKind::Propagate { .. } => false,
        };
        let (es, blocks) = stmt_parts(s);
        ok && *n <= 64 && es.into_iter().all(plain) && blocks.into_iter().all(|b| straight_block(b, n))
    })
}

/// The (list, variable) pairs of `list[variable]` in a block (reads and assignments).
fn index_pairs(body: &[Stmt], out: &mut HashSet<(SymId, SymId)>) {
    fn expr(e: &Expr, out: &mut HashSet<(SymId, SymId)>) {
        if let ExprKind::Index(l, i) = &e.kind {
            if let (ExprKind::Var(ls), ExprKind::Var(is)) = (&l.kind, &i.kind) {
                out.insert((*ls, *is));
            }
        }
        for c in expr_children(e) {
            expr(c, out);
        }
    }
    for s in body {
        if let StmtKind::IndexAssign(ls, i, _) = &s.kind {
            if let ExprKind::Var(is) = i.kind {
                out.insert((*ls, is));
            }
        }
        let (es, blocks) = stmt_parts(s);
        for e in es {
            expr(e, out);
        }
        for b in blocks {
            index_pairs(b, out);
        }
    }
}

/// Can this block change a list's header (its length, or where its numbers are)?
fn may_change_lists(body: &[Stmt]) -> bool {
    fn expr(e: &Expr) -> bool {
        // a user function may push to a list; everything else only reads lists or makes new ones
        matches!(e.kind, ExprKind::Call(..) | ExprKind::Map { .. }) || expr_children(e).into_iter().any(expr)
    }
    body.iter().any(|s| {
        matches!(s.kind, StmtKind::Push(..) | StmtKind::Clear(_) | StmtKind::Solve { .. } | StmtKind::Fit { .. }
                         | StmtKind::Plot(..) | StmtKind::Propagate { .. } | StmtKind::Animate { .. })
            || {
                let (es, blocks) = stmt_parts(s);
                es.into_iter().any(expr) || blocks.into_iter().any(|b| may_change_lists(b))
            }
    })
}

/// The list variables a block indexes (xs[i] and xs[i] = …), and the variables it rebinds.
fn indexed(body: &[Stmt], idx: &mut HashSet<SymId>, bound: &mut HashSet<SymId>) {
    fn expr(e: &Expr, idx: &mut HashSet<SymId>, bound: &mut HashSet<SymId>) {
        match &e.kind {
            ExprKind::Index(l, _) => {
                if let ExprKind::Var(s) = l.kind {
                    idx.insert(s);
                }
            }
            ExprKind::Let(binds, _) => bound.extend(binds.iter().map(|(s, _)| *s)),
            _ => {}
        }
        for c in expr_children(e) {
            expr(c, idx, bound);
        }
    }
    for s in body {
        match &s.kind {
            StmtKind::IndexAssign(sym, _, _) => {
                idx.insert(*sym);
            }
            StmtKind::Assign(sym, _) | StmtKind::ForIn(sym, _, _) => {
                bound.insert(*sym);
            }
            StmtKind::For { sym, .. } => {
                bound.insert(*sym);
            }
            _ => {}
        }
        let (es, blocks) = stmt_parts(s);
        for e in es {
            expr(e, idx, bound);
        }
        for b in blocks {
            indexed(b, idx, bound);
        }
    }
}

impl<'c, 'm> Gen<'c, 'm> {
    /// Load the headers of the lists `body` indexes (when that is safe); the variables newly hoisted.
    /// An integer loop (`for sym from lo to hi step st`, sym = lo_i + i·st_i). When its body is small and straight
    /// (no inner loop, call, integrand or solve) and indexes hoisted lists by integer loop variables, the loop is
    /// compiled twice: before it, one test checks every such index against its list for all the values the
    /// variables take (the loop variable's first and last, the outer ones' current); if all are inside, the copy
    /// without those index checks runs, else the checked copy, so which error a program stops with never changes
    /// (v1's lists had lengths known at compile time, which removed the checks the same way).
    pub(super) fn versioned_loop(&mut self, sym: SymId, lo_i: IntValue<'c>, st_i: IntValue<'c>, count: IntValue<'c>,
                                 body: &[Stmt]) -> R<()> {
        let set = move |g: &mut Self, i: IntValue<'c>| -> R<()> {
            let im = bl!(g.b.build_int_mul(i, st_i, "ist"));
            let iv = bl!(g.b.build_int_add(lo_i, im, "iv"));
            g.int_vars.insert(sym, iv);
            let x = bl!(g.b.build_signed_int_to_float(iv, g.f64t(), "x"));
            g.store_var(sym, fv(x))
        };
        let mut pairs = HashSet::new();
        let straight = straight_block(body, &mut 0);
        if straight {
            index_pairs(body, &mut pairs);
        }
        let outer: HashSet<SymId> = self.int_vars.keys().copied().collect();
        let pairs: Vec<(SymId, SymId)> = {
            let mut v: Vec<_> = pairs.into_iter()
                .filter(|(l, v)| self.hoisted.contains_key(l) && (*v == sym || outer.contains(v))
                        && !self.proven.contains(&(*l, *v)))
                .collect();
            v.sort_unstable();
            v
        };
        if pairs.is_empty() {
            return self.counted_loop_in(count, body, set);
        }
        // the test: 0 < count ≤ 2³¹ (no overflow below), and every (list, variable) index inside
        let i64t = self.cx.i64_type();
        let pos = bl!(self.b.build_int_compare(IntPredicate::SGT, count, i64t.const_zero(), "cpos"));
        let small = bl!(self.b.build_int_compare(IntPredicate::SLE, count, self.i64c(1 << 31), "csmall"));
        let mut ok = bl!(self.b.build_and(pos, small, "ok"));
        let cm1 = bl!(self.b.build_int_sub(count, self.i64c(1), "cm1"));
        let span = bl!(self.b.build_int_mul(cm1, st_i, "span"));
        let last = bl!(self.b.build_int_add(lo_i, span, "last"));
        for (l, v) in &pairs {
            let len = self.hoisted[l].1;
            let ends = if *v == sym { vec![lo_i, last] } else { vec![self.int_vars[v]] };
            for e in ends {
                let k0 = bl!(self.b.build_int_sub(e, self.i64c(1), "k0"));
                let inside = bl!(self.b.build_int_compare(IntPredicate::ULT, k0, len, "inside"));
                ok = bl!(self.b.build_and(ok, inside, "ok"));
            }
        }
        let (fast, slow, end) = (self.new_bb("fast"), self.new_bb("checked"), self.new_bb("vend"));
        bl!(self.b.build_conditional_branch(ok, fast, slow));
        let saved = std::mem::take(&mut self.proven);
        self.proven = saved.iter().copied().chain(pairs.iter().copied()).collect();
        self.goto(fast);
        let r = self.counted_loop_in(count, body, set);
        self.proven = saved;
        r?;
        if !self.terminated() {
            bl!(self.b.build_unconditional_branch(end));
        }
        self.goto(slow);
        self.counted_loop_in(count, body, set)?;
        if !self.terminated() {
            bl!(self.b.build_unconditional_branch(end));
        }
        self.goto(end);
        Ok(())
    }

    pub(super) fn hoist_lists(&mut self, body: &[Stmt]) -> R<Vec<SymId>> {
        if may_change_lists(body) {
            return Ok(vec![]);
        }
        let (mut idx, mut bound) = (HashSet::new(), HashSet::new());
        indexed(body, &mut idx, &mut bound);
        let mut syms: Vec<SymId> = idx.into_iter().filter(|s| !bound.contains(s) && !self.hoisted.contains_key(s))
            .filter(|s| kind_of(&self.m.syms[*s].ty).ok() == Some(Kind::L)).collect();
        syms.sort_unstable();
        for &s in &syms {
            let (p, _) = self.slot(s)?;
            let l = self.ld_list(p)?;
            let mut parts = self.list_parts(l)?;
            if let Some(&k) = self.fixed.get(&s) {
                parts.1 = self.i64c(k as i64);
            }
            self.hoisted.insert(s, parts);
        }
        Ok(syms)
    }
}

/// A function body that calls no user function, directly or through a lambda (an integrand, a sum, a solve's
/// right side …), and has no statement the tree-walker runs: it can't recurse (D314).
pub(super) fn is_leaf(m: &Module, body: &[Stmt]) -> bool {
    fn expr(m: &Module, e: &Expr) -> bool {
        !matches!(e.kind, ExprKind::Call(..) | ExprKind::Map { .. }) && lambda_of(e).is_none()
            && !matches!(e.kind, ExprKind::Root { .. } | ExprKind::Sample { .. } | ExprKind::Load(_) | ExprKind::Table(_))
            && expr_children(e).into_iter().all(|c| expr(m, c))
    }
    body.iter().all(|s| {
        matches!(s.kind, StmtKind::Assign(..) | StmtKind::IndexAssign(..) | StmtKind::Expr(_) | StmtKind::Assert(..)
                         | StmtKind::Print(_) | StmtKind::Break | StmtKind::Continue | StmtKind::If(..)
                         | StmtKind::While(..) | StmtKind::For { par: None, .. } | StmtKind::ForIn(..)
                         | StmtKind::Return(_) | StmtKind::Push(..) | StmtKind::Clear(_))
            && !gc::tree_walker_stmt(m, s)
            && {
                let (es, blocks) = stmt_parts(s);
                es.into_iter().all(|e| expr(m, e)) && blocks.into_iter().all(|b| is_leaf(m, b))
            }
    })
}

/// Scratch variables (D312): the variables whose every read, anywhere in the program, comes after an assignment
/// to them earlier in the same run of statements (the same block, or a block around it), with no loop or `if`
/// in between that could have assigned them too. Such a variable's value never outlives the block that set it,
/// so an if-converted branch (if_converted) may store its value even when the condition is false: nothing reads
/// it before it is set again. Everything else a statement reads (a solve's right side, a lambda, a function's
/// module variables) counts as a read where it stands; a construct not walked here makes every variable it
/// mentions a non-scratch one.
pub(super) fn scratch_vars(m: &Module) -> HashSet<SymId> {
    fn reads(m: &Module, e: &Expr, defined: &HashSet<SymId>, carried: &mut HashSet<SymId>) {
        let (mut used, mut bound) = (HashSet::new(), HashSet::new());
        super::lam::expr_syms(m, e, &mut used, &mut bound);
        carried.extend(used.into_iter().filter(|s| !defined.contains(s)));
    }
    fn lambda(m: &Module, l: fermium_ir::LambdaId, defined: &HashSet<SymId>, carried: &mut HashSet<SymId>) {
        for b in &m.lambdas[l].body {
            reads(m, b, defined, carried);
        }
    }
    fn assigned(body: &[Stmt], out: &mut HashSet<SymId>) {
        for s in body {
            match &s.kind {
                StmtKind::Assign(sym, _) | StmtKind::ForIn(sym, _, _) | StmtKind::For { sym, .. } => {
                    out.insert(*sym);
                }
                StmtKind::Solve { sol, .. } => {
                    out.insert(*sol);
                }
                StmtKind::Fit { params, errs, .. } => out.extend(params.iter().chain(errs.iter()).copied()),
                StmtKind::Propagate { outs, .. } => out.extend(outs.iter().copied()),
                _ => {}
            }
            for b in stmt_parts(s).1 {
                assigned(b, out);
            }
        }
    }
    fn walk(m: &Module, body: &[Stmt], defined: &mut HashSet<SymId>, carried: &mut HashSet<SymId>) {
        for s in body {
            let (es, blocks) = stmt_parts(s);
            for e in &es {
                reads(m, e, defined, carried);
            }
            match &s.kind {
                StmtKind::Assign(sym, _) => {
                    defined.insert(*sym);
                }
                StmtKind::If(..) | StmtKind::While(..) | StmtKind::For { .. } | StmtKind::ForIn(..)
                | StmtKind::Propagate { .. } => {
                    // a loop body starts with nothing set (the previous pass may have set anything); an if's
                    // branches start with what is set before it
                    let is_if = matches!(s.kind, StmtKind::If(..));
                    for b in &blocks {
                        let mut inner = if is_if { defined.clone() } else { HashSet::new() };
                        match &s.kind {
                            StmtKind::For { sym, .. } | StmtKind::ForIn(sym, _, _) => {
                                inner.insert(*sym);
                            }
                            _ => {}
                        }
                        walk(m, b, &mut inner, carried);
                    }
                    // after it, what it may have set is no longer known to be set here
                    let mut set = HashSet::new();
                    assigned(std::slice::from_ref(s), &mut set);
                    defined.retain(|x| !set.contains(x));
                }
                StmtKind::Solve { sol, rhs, x, .. } => {
                    lambda(m, *rhs, defined, carried);
                    if let Some(ev) = x.event {
                        lambda(m, ev, defined, carried);
                    }
                    defined.remove(sol);
                }
                StmtKind::Fit { model, params, errs, .. } => {
                    lambda(m, *model, defined, carried);
                    for p in params.iter().chain(errs.iter()) {
                        defined.remove(p);
                    }
                }
                StmtKind::Animate { sol, .. } => {
                    carried.insert(*sol);
                }
                _ => {}
            }
        }
    }
    let mut carried = HashSet::new();
    walk(m, &m.main, &mut HashSet::new(), &mut carried);
    for f in &m.funcs {
        let mut d: HashSet<SymId> = f.params.iter().copied().collect();
        walk(m, &f.body, &mut d, &mut carried);
    }
    (0..m.syms.len()).filter(|s| !carried.contains(s)).collect()
}

/// The math built-ins that are pure functions of one number and never stop the program (safe to evaluate when
/// their result is thrown away: if_converted).
const PURE1: &[&str] = &["sqrt", "abs", "floor", "ceil", "round", "exp", "sin", "cos", "tan", "atan", "sinh", "cosh",
                         "tanh", "asinh", "expm1"];

impl<'c, 'm> Gen<'c, 'm> {
    /// Can e be evaluated whatever the condition of the `if` around it (if_converted)? Numbers only: constants,
    /// numeric variables, + − × ÷, constant powers, a few pure built-ins and list reads whose index check was
    /// proven before the loop (versioned_loop). Nothing that can stop the program or have an effect.
    fn speculable(&self, e: &Expr) -> bool {
        if !matches!(e.ty, Ty::Num(_)) {
            return false;
        }
        match &e.kind {
            ExprKind::Const(_) => true,
            ExprKind::Var(s) => kind_of(&self.m.syms[*s].ty).ok() == Some(Kind::F),
            ExprKind::Bin(_, a, b) => self.speculable(a) && self.speculable(b),
            ExprKind::PowC(a, _) | ExprKind::Neg(a) => self.speculable(a),
            ExprKind::Builtin(name, args) => {
                args.len() == 1 && PURE1.contains(&name.as_str()) && self.speculable(&args[0])
            }
            ExprKind::Index(l, i) => match (&l.kind, &i.kind) {
                (ExprKind::Var(ls), ExprKind::Var(is)) => {
                    self.proven.contains(&(*ls, *is)) && self.hoisted.contains_key(ls)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Is this branch of an `if` a few assignments of speculable numbers to numeric variables (if_converted)?
    fn convertible_block(&self, body: &[Stmt]) -> bool {
        body.len() <= 24 && body.iter().all(|s| match &s.kind {
            StmtKind::Assign(sym, e) => {
                kind_of(&self.m.syms[*sym].ty).ok() == Some(Kind::F) && !self.int_vars.contains_key(sym)
                    && !self.consts.contains_key(sym) && self.speculable(e)
            }
            _ => false,
        })
    }

    /// `if c` inside the copy of a loop whose index checks were proven (versioned_loop), when its branches only
    /// assign speculable numbers to numeric variables: compiled without a branch (D312). Each branch's values are
    /// computed whatever c is and kept with a select, so the loop body is one straight block that the loop
    /// vectorizer can take. A sum `v = v + e` (`v += e`) becomes v + (c ? e : −0) and `v = v − e` becomes
    /// v − (c ? e : +0): adding −0 or subtracting +0 gives v back exactly (±0 and NaN included), and the sum stays
    /// an in-order reduction LLVM recognizes (force-ordered-reductions, D311). The results are bit for bit the
    /// branchy code's: nothing computed under a false condition is kept, and floating point doesn't trap.
    /// Ok(false): not convertible (nothing compiled).
    pub(super) fn if_converted(&mut self, c: &Expr, then: &[Stmt], other: &[Stmt]) -> R<bool> {
        if self.proven.is_empty() || std::env::var_os("FERMIUM_NO_IFCONV").is_some() || then.is_empty()
            || !self.convertible_block(then) || !self.convertible_block(other) {
            return Ok(false);
        }
        let cv = self.expr(c)?;
        let t = self.truth(cv)?;
        let nt = bl!(self.b.build_not(t, "nc"));
        let reads_of = |g: &Self, body: &[Stmt]| -> HashSet<SymId> {
            let (mut used, mut bound) = (HashSet::new(), HashSet::new());
            for s in body {
                for e in stmt_parts(s).0 {
                    super::lam::expr_syms(g.m, e, &mut used, &mut bound);
                }
            }
            used
        };
        for (cond, body, rest) in [(t, then, other), (nt, other, then)] {
            let read_elsewhere = reads_of(self, rest);
            for s in body {
                let StmtKind::Assign(sym, e) = &s.kind else { return Err("not an assignment".into()) };
                if self.scratch.contains(sym) && !read_elsewhere.contains(sym) {
                    // a scratch variable: nothing reads it before it is set again, so no select is needed
                    let nv = self.expr(e)?;
                    self.store_var(*sym, nv)?;
                    continue;
                }
                let old = self.load_var(*sym)?;
                let old = self.to_f(old)?;
                let sum = match &e.kind {
                    ExprKind::Bin(op @ (BinOp::Add | BinOp::Sub), a, b)
                        if matches!(a.kind, ExprKind::Var(v) if v == *sym) => Some((*op, b)),
                    _ => None,
                };
                let v = match sum {
                    Some((op, b)) => {
                        let bv = self.expr(b)?;
                        let mut bv = self.to_f(bv)?;
                        if op == BinOp::Sub {
                            // v − e is v + (−e) exactly (IEEE subtraction is the addition of the negation); LLVM
                            // vectorizes only in-order sums made of fadd
                            bv = bl!(self.b.build_float_neg(bv, "neg"));
                        }
                        let zero = self.fconst(-0.0);
                        let term = bl!(self.b.build_select(cond, bv, zero, "term")).into_float_value();
                        self.arith(BinOp::Add, old, term)?
                    }
                    None => {
                        let nv = self.expr(e)?;
                        let nv = self.to_f(nv)?;
                        bl!(self.b.build_select(cond, nv, old, "sel")).into_float_value()
                    }
                };
                self.store_var(*sym, fv(v))?;
            }
        }
        Ok(true)
    }
}
