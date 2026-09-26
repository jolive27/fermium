//! Module constants: a module variable set exactly once, by a statement of the main block that runs before any user
//! function is called, to a value known now, is that value everywhere it is read (the compiled code reads it only
//! after that statement: the main block's later statements, the functions, which run only once one is called), so
//! `m = 1 kg` makes `F / m` a plain F and `N = 1000000` gives a loop a known trip count. Likewise a list set once
//! there to a list of known length, and used only by indexing, len and `for … in` (nothing can push to it, nothing
//! else holds it), keeps that length: `len(mass)` is a constant and index checks compare with it. v1 got the same
//! from LLVM's globalopt, which sees every use of its globals; here the variables' addresses also go to the run
//! time (envs of right-hand sides, integrands, parallel loops), which hides that from LLVM. The variable is still
//! stored, so everything that reads its slot (the tree-walker's constructs, a solution's snapshot) is unchanged.
use std::collections::{HashMap, HashSet};

use fermium_ir::{expr_children, lambda_of, stmt_parts, Expr, ExprKind, Module, Stmt, StmtKind, SymId};

use super::*;

/// How many statements of the module (re)bind each variable (every kind of statement that does, no default).
pub(super) fn write_counts(m: &Module) -> HashMap<SymId, usize> {
    fn walk(body: &[Stmt], c: &mut HashMap<SymId, usize>) {
        for s in body {
            let mut w = |x: SymId| *c.entry(x).or_insert(0) += 1;
            match &s.kind {
                StmtKind::Assign(x, _) | StmtKind::IndexAssign(x, _, _) | StmtKind::Push(x, _) | StmtKind::Clear(x)
                | StmtKind::ForIn(x, _, _) => w(*x),
                StmtKind::For { sym, .. } => w(*sym),
                StmtKind::Solve { sol, .. } => w(*sol),
                StmtKind::Fit { params, errs, .. } => params.iter().chain(errs).for_each(|x| w(*x)),
                StmtKind::Propagate { outs, .. } => outs.iter().for_each(|x| w(*x)),
                StmtKind::If(..) | StmtKind::While(..) | StmtKind::Print(_) | StmtKind::Plot(..) | StmtKind::Return(_)
                | StmtKind::Break | StmtKind::Continue | StmtKind::Expr(_) | StmtKind::Assert(..)
                | StmtKind::Animate { .. } => {}
            }
            let (_, blocks) = stmt_parts(s);
            for b in blocks {
                walk(b, c);
            }
        }
    }
    let mut c = HashMap::new();
    walk(&m.main, &mut c);
    for f in &m.funcs {
        walk(&f.body, &mut c);
    }
    c
}

/// Does running this statement call a user function (in its expressions, the integrands, sums and roots they
/// use, a solve's right side or event, a fit's model)?
fn calls(m: &Module, s: &Stmt) -> bool {
    fn expr(m: &Module, e: &Expr) -> bool {
        matches!(e.kind, ExprKind::Call(..) | ExprKind::Map { .. })
            || lambda_of(e).is_some_and(|l| m.lambdas[l].body.iter().any(|b| expr(m, b)))
            || expr_children(e).into_iter().any(|c| expr(m, c))
    }
    let lam = |l: usize| m.lambdas[l].body.iter().any(|b| expr(m, b));
    let own = match &s.kind {
        StmtKind::Solve { rhs, x, .. } => lam(*rhs) || x.event.is_some_and(lam),
        StmtKind::Fit { model, .. } => lam(*model),
        _ => false,
    };
    let (es, blocks) = stmt_parts(s);
    own || es.into_iter().any(|e| expr(m, e)) || blocks.into_iter().any(|b| b.iter().any(|s| calls(m, s)))
}

/// The lists whose every use is `l[i]`, `l[i] = …`, `len(l)` or `for x in l`, bound by exactly one statement
/// (an assignment): nothing but that assignment can change their length, and no other variable shares them.
fn fixed_candidates(m: &Module) -> HashSet<SymId> {
    fn expr(e: &Expr, bad: &mut HashSet<SymId>) {
        match &e.kind {
            ExprKind::Var(s) => {
                bad.insert(*s);
            }
            ExprKind::Index(l, i) => {
                if !matches!(l.kind, ExprKind::Var(_)) {
                    expr(l, bad);
                }
                expr(i, bad);
            }
            ExprKind::Builtin(name, args) if name == "len" && args.len() == 1
                && matches!(args[0].kind, ExprKind::Var(_)) => {}
            ExprKind::Let(binds, body) => {
                for (s, v) in binds {
                    bad.insert(*s);
                    expr(v, bad);
                }
                expr(body, bad);
            }
            _ => {
                for c in expr_children(e) {
                    expr(c, bad);
                }
            }
        }
    }
    fn block(body: &[Stmt], bad: &mut HashSet<SymId>, binds: &mut HashMap<SymId, usize>) {
        for s in body {
            let mut bind = |x: SymId| *binds.entry(x).or_insert(0) += 1;
            match &s.kind {
                StmtKind::Assign(x, _) => bind(*x),
                StmtKind::ForIn(x, l, _) => {
                    bind(*x);
                    // the list a `for … in` walks is copied first: reading it is all it does
                    if let ExprKind::Var(_) = l.kind {
                        let (_, blocks) = stmt_parts(s);
                        for b in blocks {
                            block(b, bad, binds);
                        }
                        continue;
                    }
                }
                StmtKind::For { sym, .. } => bind(*sym),
                StmtKind::Push(x, _) | StmtKind::Clear(x) => {
                    bad.insert(*x);
                }
                StmtKind::Solve { sol, .. } => bind(*sol),
                StmtKind::Fit { params, errs, .. } => params.iter().chain(errs).for_each(|x| bind(*x)),
                StmtKind::Propagate { outs, .. } => outs.iter().for_each(|x| bind(*x)),
                StmtKind::IndexAssign(..) | StmtKind::If(..) | StmtKind::While(..) | StmtKind::Print(_)
                | StmtKind::Plot(..) | StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Expr(_)
                | StmtKind::Assert(..) | StmtKind::Animate { .. } => {}
            }
            let (es, blocks) = stmt_parts(s);
            for e in es {
                expr(e, bad);
            }
            for b in blocks {
                block(b, bad, binds);
            }
        }
    }
    let (mut bad, mut binds) = (HashSet::new(), HashMap::new());
    block(&m.main, &mut bad, &mut binds);
    for f in &m.funcs {
        block(&f.body, &mut bad, &mut binds);
    }
    for l in &m.lambdas {
        for b in &l.body {
            expr(b, &mut bad);
        }
    }
    binds.into_iter().filter(|(s, n)| *n == 1 && !bad.contains(s)).map(|(s, _)| s).collect()
}

/// The length a list expression is known to have: a list of numbers written out, and element-by-element
/// arithmetic on one (with numbers, or with another of the same known length).
fn static_len(e: &Expr) -> Option<u64> {
    let is_list = |x: &Expr| matches!(x.ty, Ty::List(_));
    match &e.kind {
        ExprKind::List(items) if items.iter().all(|x| matches!(x.ty, Ty::Num(_))) => Some(items.len() as u64),
        ExprKind::Bin(_, a, b) => match (is_list(a), is_list(b)) {
            (true, false) => static_len(a),
            (false, true) => static_len(b),
            (true, true) => static_len(a).filter(|k| static_len(b) == Some(*k)),
            (false, false) => None,
        },
        ExprKind::Neg(a) | ExprKind::PowC(a, _) if is_list(a) => static_len(a),
        _ => None,
    }
}

/// A list expression that always makes a new list (no other variable can hold it).
fn fresh_list(e: &Expr) -> bool {
    matches!(e.ty, Ty::List(_))
        && match &e.kind {
            ExprKind::List(_) | ExprKind::Bin(..) | ExprKind::Neg(_) | ExprKind::PowC(..) => true,
            ExprKind::Builtin(name, _) => matches!(name.as_str(), "zeros" | "ones" | "linspace"),
            _ => false,
        }
}

/// The lists that own their numbers: fixed_candidates (every use indexes them, one binding) whose binding makes
/// a new list. No two of them ever share their numbers, so their elements get TBAA types of their own
/// (Gen::elem_tag): a store into one doesn't make LLVM reload another (v1's lists were separate global arrays).
pub(super) fn owned_lists(m: &Module) -> HashSet<SymId> {
    fn walk(body: &[Stmt], cand: &HashSet<SymId>, out: &mut HashSet<SymId>) {
        for s in body {
            if let StmtKind::Assign(x, e) = &s.kind {
                if cand.contains(x) && fresh_list(e) {
                    out.insert(*x);
                }
            }
            let (_, blocks) = stmt_parts(s);
            for b in blocks {
                walk(b, cand, out);
            }
        }
    }
    let cand = fixed_candidates(m);
    let mut out = HashSet::new();
    walk(&m.main, &cand, &mut out);
    for f in &m.funcs {
        walk(&f.body, &cand, &mut out);
    }
    out
}

impl<'c, 'm> Gen<'c, 'm> {
    /// The program's main block. Until it calls a user function, each module variable it sets (at its top level)
    /// that nothing else ever sets becomes a constant if its value is known now, and each list that qualifies
    /// (fixed_candidates) keeps its known length.
    pub(super) fn main_block(&mut self, main: &[Stmt]) -> R<()> {
        let m = self.m;
        let writes = write_counts(m);
        let lists = fixed_candidates(m);
        let mut called = false;
        for s in main {
            if self.terminated() {
                break;
            }
            called = called || calls(m, s);
            if !called {
                if let StmtKind::Assign(sym, e) = &s.kind {
                    self.set_line(s.line)?;
                    let v = self.expr(e)?;
                    let known = match (v.k, v.v) {
                        (Kind::F, Some(BasicValueEnum::FloatValue(f))) if f.get_constant().is_some() => Some(f),
                        _ => None,
                    };
                    self.store_var(*sym, v)?;
                    // (after the store, which makes the variable's slot)
                    let global = m.syms[*sym].func.is_none() && self.globals.contains_key(sym);
                    let slot_kind = self.globals.get(sym).map(|g| g.1);
                    if let (Some(f), true, Some(Kind::F)) = (known, global, slot_kind) {
                        if writes.get(sym) == Some(&1) {
                            self.consts.insert(*sym, f);
                        }
                    }
                    if let (true, Some(Kind::L), Some(k)) = (global && lists.contains(sym), slot_kind, static_len(e)) {
                        self.fixed.insert(*sym, k);
                    }
                    continue;
                }
            }
            self.stmt(s)?;
        }
        Ok(())
    }
}
