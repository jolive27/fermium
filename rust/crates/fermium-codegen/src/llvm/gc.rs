//! Memory (spec C1, DECISIONS D280): the code the collector in `rt.rs` needs. A function whose loops may make
//! lists registers its list slots on entry and unregisters them on return, and those loops poll `gc_flag` at the
//! top of each iteration (a safe point). Functions whose loops make no lists (the numeric inner loops) get neither,
//! so their code is unchanged. See the memory section of rt.rs for why this is safe.
use std::collections::HashSet;

use fermium_ir::{expr_children, lambda_of, stmt_parts, Expr, ExprKind, Module, Stmt, StmtKind};

use super::*;

/// Does evaluating this type's values make a list, text list, text or Obj value (something the collector frees)?
fn collected(ty: &Ty) -> bool {
    matches!(kind_of(ty), Ok(Kind::L | Kind::TL | Kind::Obj | Kind::S))
}

/// Which user functions may make lists when called (directly, through a lambda, or through a function they call).
pub(super) fn alloc_funcs(m: &Module) -> Vec<bool> {
    let mut out = vec![false; m.funcs.len()];
    loop {
        let mut changed = false;
        for (i, f) in m.funcs.iter().enumerate() {
            if !out[i] && block_allocs(m, &f.body, &out, &mut HashSet::new()) {
                out[i] = true;
                changed = true;
            }
        }
        if !changed {
            return out;
        }
    }
}

/// May running these statements make a list (the loops that get a safe point)?
pub(super) fn block_allocs(m: &Module, body: &[Stmt], funcs: &[bool], seen: &mut HashSet<usize>) -> bool {
    body.iter().any(|s| stmt_allocs(m, s, funcs, seen))
}

fn stmt_allocs(m: &Module, s: &Stmt, funcs: &[bool], seen: &mut HashSet<usize>) -> bool {
    match &s.kind {
        // the tree-walker runs these, and may give back new values for the variables they set
        StmtKind::Plot(..) | StmtKind::Fit { .. } | StmtKind::Animate { .. } | StmtKind::Push(..) => return true,
        StmtKind::Solve { rhs, x, .. } => {
            if lambda_allocs(m, *rhs, funcs, seen) || x.event.is_some_and(|l| lambda_allocs(m, l, funcs, seen)) {
                return true;
            }
        }
        _ => {}
    }
    let (es, blocks) = stmt_parts(s);
    es.into_iter().any(|e| expr_allocs(m, e, funcs, seen)) || blocks.into_iter().any(|b| block_allocs(m, b, funcs, seen))
}

fn lambda_allocs(m: &Module, l: usize, funcs: &[bool], seen: &mut HashSet<usize>) -> bool {
    if !seen.insert(l) {
        return false;
    }
    m.lambdas.get(l).is_some_and(|lam| lam.body.iter().any(|e| expr_allocs(m, e, funcs, seen)))
}

fn expr_allocs(m: &Module, e: &Expr, funcs: &[bool], seen: &mut HashSet<usize>) -> bool {
    // reading a list variable (for xs[i], len(xs), …) makes nothing new (D313): a loop that only reads lists
    // needs no safe point, whose call to the collector would stop LLVM from keeping variables in registers
    if matches!(e.kind, ExprKind::Var(_)) {
        return false;
    }
    if collected(&e.ty) {
        return true;
    }
    match &e.kind {
        ExprKind::Call(f, _) | ExprKind::Map { func: f, .. } if funcs.get(*f).copied().unwrap_or(true) => return true,
        _ => {}
    }
    if let Some(l) = lambda_of(e) {
        if lambda_allocs(m, l, funcs, seen) {
            return true;
        }
    }
    expr_children(e).into_iter().any(|c| expr_allocs(m, c, funcs, seen))
}

impl<'c, 'm> Gen<'c, 'm> {
    /// The kind code of a slot the collector reads (rt.rs GcFrame), if it holds a collected value.
    pub(super) fn gc_code(k: Kind) -> Option<u8> {
        match k {
            Kind::L => Some(1),
            Kind::TL => Some(2),
            Kind::Obj => Some(3),
            Kind::S => Some(4),
            _ => None,
        }
    }

    /// At the top of a loop iteration: collect if gc_flag is set and the body may make lists.
    pub(super) fn safe_point(&mut self, body: &[Stmt]) -> R<()> {
        // a loop that only reads lists needs no safe point (D313): its call to the collector would keep LLVM from
        // vectorizing it. Loops around other loops and loops that write list elements keep theirs, as before
        // (a branch per pass): measured, LLVM turns nbody's loop nest into slower code without them
        let outer_reads = || (has_loop(body) || writes_lists(body)) && reads_collected(body);
        if !self.gc_frame || !(block_allocs(self.m, body, &self.alloc_funcs, &mut HashSet::new()) || outer_reads()) {
            return Ok(());
        }
        let fp = unsafe { bl!(self.b.build_gep(self.cx.i8_type(), self.ctx_ptr, &[self.i64c(8)], "gcfp")) };
        let flag = self.ld(self.cx.i32_type(), fp, "gcflag", "flag")?.into_int_value();
        let due = bl!(self.b.build_int_compare(IntPredicate::NE, flag, self.cx.i32_type().const_zero(), "gcdue"));
        let (run, cont) = (self.new_bb("gc"), self.new_bb("gcdone"));
        self.cold_br(due, run, cont)?;
        self.b.position_at_end(run);
        self.call("fm_gc", &[self.ctx_ptr.into()])?;
        bl!(self.b.build_unconditional_branch(cont));
        self.b.position_at_end(cont);
        Ok(())
    }

    /// Before a return of a function with a frame.
    pub(super) fn gc_leave(&mut self) -> R<()> {
        if self.gc_frame {
            self.call("fm_gc_leave", &[self.ctx_ptr.into()])?;
        }
        Ok(())
    }

    /// A hidden slot the collector reads (a `for … in` loop's copy of its list).
    pub(super) fn gc_root(&mut self, v: BasicValueEnum<'c>, k: Kind) -> R<()> {
        if !self.gc_frame {
            return Ok(());
        }
        let Some(code) = Self::gc_code(k) else { return Ok(()) };
        let p = self.alloca(self.llty(k), "gcroot")?;
        let eb = self.entry_b.as_ref().unwrap();
        bl!(eb.build_store(p, self.init_value(k)));
        bl!(self.b.build_store(p, v));
        self.gc_roots.push((p, code));
        Ok(())
    }

    /// At the end of the entry block (which holds only the slots and their first values): register the frame's
    /// slots (`extra`: the module's variables, for fm_main), then go to the body.
    pub(super) fn close_entry(&mut self, entry: BasicBlock<'c>, start: BasicBlock<'c>,
                              extra: &[(PointerValue<'c>, u8)]) -> R<()> {
        self.b.position_at_end(entry);
        if self.gc_frame {
            let mut slots: Vec<(PointerValue<'c>, u8)> = extra.to_vec();
            let mut locals: Vec<_> = self.locals.iter().filter_map(|(s, (p, k))| Self::gc_code(*k).map(|c| (*s, *p, c)))
                .collect();
            locals.sort_by_key(|x| x.0);
            slots.extend(locals.into_iter().map(|(_, p, c)| (p, c)));
            slots.extend(self.gc_roots.iter().copied());
            let n = slots.len();
            let arr_t = self.ptrt().array_type(n.max(1) as u32);
            let arr = bl!(self.b.build_alloca(arr_t, "gcslots"));
            for (i, (p, _)) in slots.iter().enumerate() {
                let ep = unsafe { bl!(self.b.build_gep(arr_t, arr, &[self.i64c(0), self.i64c(i as i64)], "gcs")) };
                bl!(self.b.build_store(ep, *p));
            }
            let codes: Vec<IntValue> = slots.iter().map(|(_, c)| self.cx.i8_type().const_int(*c as u64, false)).collect();
            let kinds = self.lm.add_global(self.cx.i8_type().array_type(n.max(1) as u32), None, "fm.gckinds");
            let init = if n == 0 { self.cx.i8_type().array_type(1).const_zero() } else { self.cx.i8_type().const_array(&codes) };
            kinds.set_initializer(&init);
            kinds.set_linkage(Linkage::Internal);
            kinds.set_constant(true);
            self.call("fm_gc_enter", &[self.ctx_ptr.into(), arr.into(), kinds.as_pointer_value().into(),
                                       self.i64c(n as i64).into()])?;
        }
        bl!(self.b.build_unconditional_branch(start));
        Ok(())
    }
}

/// Does this function body have a loop that may make lists (so it needs a frame and safe points)?
/// Does a block hold a loop (at any depth)?
fn has_loop(body: &[Stmt]) -> bool {
    body.iter().any(|s| matches!(s.kind, StmtKind::While(..) | StmtKind::For { .. } | StmtKind::ForIn(..))
        || stmt_parts(s).1.into_iter().any(|b| has_loop(b)))
}

/// Does a block set list elements (xs[i] = …)?
fn writes_lists(body: &[Stmt]) -> bool {
    body.iter().any(|s| matches!(s.kind, StmtKind::IndexAssign(..)) || stmt_parts(s).1.into_iter().any(|b| writes_lists(b)))
}

/// Does a block read a variable holding a collected value (a list …)?
fn reads_collected(body: &[Stmt]) -> bool {
    fn expr(e: &Expr) -> bool {
        (matches!(e.kind, ExprKind::Var(_)) && collected(&e.ty)) || expr_children(e).into_iter().any(expr)
    }
    body.iter().any(|s| {
        let (es, blocks) = stmt_parts(s);
        es.into_iter().any(expr) || blocks.into_iter().any(|b| reads_collected(b))
    })
}

pub(super) fn has_alloc_loop(m: &Module, body: &[Stmt], funcs: &[bool]) -> bool {
    body.iter().any(|s| {
        let (_, blocks) = stmt_parts(s);
        let here = match &s.kind {
            StmtKind::While(_, b) | StmtKind::ForIn(_, _, b) | StmtKind::For { body: b, par: None, .. } => {
                block_allocs(m, b, funcs, &mut HashSet::new())
            }
            _ => false,
        };
        here || blocks.into_iter().any(|b| has_alloc_loop(m, b, funcs))
    })
}

/// A list the tree-walker holds for the compiled code (kind Obj): of vectors, matrices or complex numbers.
fn tw_list(ty: &Ty) -> bool {
    matches!(ty, Ty::VList(_) | Ty::ComplexList(_) | Ty::Array { .. })
}

/// Values the tree-walker computes whenever they appear (lists of vectors or matrices, arrays).
fn tw_value(ty: &Ty) -> bool {
    matches!(ty, Ty::VList(_) | Ty::Array { .. })
}

/// An expression the tree-walker computes for the compiled code (D281): a list of vectors, matrices or complex
/// numbers written out ([<1, 2> m, <3, 4> m], [1 + 2i, 3i]), and anything made directly from a list of vectors or
/// matrices (ps[i], len(ps), 2 ps). (Built-ins on lists of complex numbers already go through fm_builtin.)
pub(super) fn tree_walker_list(m: &Module, e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) => false,
        // built-ins on arrays, lists of vectors and list unknowns' solutions take these values as Obj arguments
        // through fm_builtin, which is much cheaper than handing the expression over (fm_interp)
        ExprKind::Builtin(name, args) if name.starts_with("arr.") || name == "__sol_list"
            || (matches!(name.as_str(), "len" | "sum" | "mean") && args.len() == 1 && matches!(args[0].ty, Ty::VList(_))) => {
            args.iter().any(|a| retyped(m, a))
        }
        ExprKind::List(_) => tw_list(&e.ty),
        _ => tw_value(&e.ty) || expr_children(e).iter().any(|c| tw_value(&c.ty) || retyped(m, c)),
    }
}

/// A variable read before it became a list of vectors, complex numbers or text (`vel = []`, then `print vel`, then
/// `push(vel, <1, 2> m/s)`): the read was checked as a list of numbers, but the slot holds the other kind (D281).
fn retyped(m: &Module, e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(s) => matches!(e.ty, Ty::List(_)) && matches!(m.syms[*s].ty, Ty::VList(_) | Ty::ComplexList(_) | Ty::TextList),
        _ => expr_children(e).iter().any(|c| retyped(m, c)),
    }
}

/// A statement the tree-walker runs for the compiled code (D281): push, xs[i] = …, clear or for … in on such a
/// list, and a print of a list of vectors or matrices.
pub(super) fn tree_walker_stmt(m: &Module, s: &Stmt) -> bool {
    let sym_tw = |x: &usize| tw_list(&m.syms[*x].ty);
    match &s.kind {
        StmtKind::Push(x, _) | StmtKind::Clear(x) => sym_tw(x),
        // (xs[i] = "text" too: the compiled code's text lists have no element store)
        StmtKind::IndexAssign(x, _, _) => sym_tw(x) || matches!(m.syms[*x].ty, Ty::TextList),
        StmtKind::ForIn(_, l, _) => tw_list(&l.ty) || retyped(m, l),
        StmtKind::Print(items) => items.iter().any(|it| {
            matches!(it, fermium_ir::PrintItem::VList(..) | fermium_ir::PrintItem::Array(..))
                || fermium_ir::print_item_expr(it).is_some_and(|e| retyped(m, e))
        }),
        _ => false,
    }
}
