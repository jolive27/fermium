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
