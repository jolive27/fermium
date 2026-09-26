//! Reading list headers once per loop: in a loop body that can't change any list's length or move its numbers
//! (no push, clear, user function call or anything else that could reach a list), the data pointer and the length
//! of each list variable it indexes are loaded before the loop. LLVM would do this itself if it could prove the
//! loads safe to move, which it can't after the index checks' early exits.
use std::collections::HashSet;

use fermium_ir::{expr_children, stmt_parts, Expr, ExprKind, Stmt, StmtKind, SymId};

use super::*;

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
            let parts = self.list_parts(l)?;
            self.hoisted.insert(s, parts);
        }
        Ok(syms)
    }
}
