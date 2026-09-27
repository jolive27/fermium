//! N-dimensional arrays with units (spec C1, DECISIONS D283): `T = fill(300 K, 50, 50)` makes a 50×50 grid of
//! temperatures; `T[i, j]` reads and `T[i, j] = …` (or `+=`) writes an entry; `size(T)`, `sum`, `mean`, `max`,
//! `min`; arithmetic entry by entry with numbers and with arrays of the same shape (units checked like numbers:
//! every entry shares one unit). Shapes are run-time facts (checked when the program runs); the rank is known now.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;

use crate::arith::minsf;
use crate::checker::*;
use crate::stmts::ty_dim;

/// The most dimensions an array can have.
pub const MAX_RANK: usize = 4;

impl Checker {
    /// `fill(value, n1, n2, …)`: an array of n1×n2×… entries, all `value` (its units are the array's). One size
    /// is a list.
    pub fn array_fill(&mut self, e: &A::Expr, args: Vec<I::Expr>, eargs: &[A::Expr]) -> CResult<I::Expr> {
        let line = e.span.line;
        if args.len() < 2 || args.len() > MAX_RANK + 1 {
            return Err(self.err(format!("fill(value, n1, n2, …) makes an array of 1 to {MAX_RANK} dimensions: give the \
                                         value, then the size along each (fill(0 K, 50, 50) is a 50×50 grid)"),
                                e.span, None));
        }
        self.need_num(&args[0], &eargs[0], "the value to fill with")?;
        for (a, node) in args[1..].iter().zip(&eargs[1..]) {
            self.need_num(a, node, "a size")?;
            let d = ty_dim(&a.ty).unwrap();
            if !self.u.unify(&d, &DExpr::of(DIMLESS)) {
                return Err(self.err(format!("a size must be a plain whole number, not {}", self.desc(&d)), node.span,
                                    None));
            }
        }
        let dim = ty_dim(&args[0].ty).unwrap();
        let hint = args[0].hint.clone();
        let rank = args.len() - 1;
        let ty = if rank == 1 { Ty::List(dim) } else { Ty::Array { rank, dim } };
        let sf = args[0].sf;
        let mut r = ir(I::ExprKind::Builtin("arr.fill".into(), args), ty, line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    /// A built-in applied to an array: size, sum, mean, max, min, abs.
    pub fn array_call(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let line = e.span.line;
        let a = &args[0];
        let Ty::Array { rank, dim } = a.ty.clone() else {
            return Err(self.err(format!("{name} can't take an array here"), e.span, None));
        };
        let dimless = || DExpr::of(DIMLESS);
        let (ty, hint) = match (name, args.len()) {
            ("size", 1) => (Ty::List(dimless()), None),
            ("size", 2) => {
                if let I::ExprKind::Const(k) = args[1].kind {
                    if !(k >= 1.0 && k <= rank as f64 && k == k.trunc()) {
                        return Err(self.err(format!("size(A, k): this array has {rank} dimensions, so k is 1 to {rank}"),
                                            e.span, None));
                    }
                }
                (Ty::Num(dimless()), None)
            }
            ("sum" | "mean" | "max" | "min", 1) => (Ty::Num(dim.clone()), a.hint.clone()),
            ("abs" | "copy", 1) => (Ty::Array { rank, dim: dim.clone() }, a.hint.clone()),
            _ => {
                return Err(self.err(format!("{name} doesn't work on arrays yet (size, sum, mean, max, min, abs and \
                                             copy do; loop over the entries for the rest)"), e.span, None));
            }
        };
        let sf = if name == "size" { None } else { a.sf };
        let mut r = ir(I::ExprKind::Builtin(format!("arr.{name}"), args), ty, line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    /// A[i, j, …] of an array variable (the parser nests the indexes: A[i][j]); None if the base isn't an array.
    pub fn array_index(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Option<I::Expr>> {
        let mut idxs: Vec<&A::Expr> = vec![];
        let mut base = e;
        while let A::ExprKind::Index { target, index: Some(index) } = &base.kind {
            idxs.push(index);
            base = target;
        }
        let A::ExprKind::Name { name } = &base.kind else { return Ok(None) };
        let Some((Binding::Sym(sym), _)) = self.lookup(ctx.scope, name) else { return Ok(None) };
        let Ty::Array { rank, dim } = self.module.syms[sym].ty.clone() else { return Ok(None) };
        idxs.reverse();
        if idxs.len() != rank {
            return Err(self.err(format!("{name} is a {rank}-dimensional array, so it takes {rank} indexes, like \
                                         {name}[{}]", ["i", "j", "k", "l"][..rank].join(", ")), e.span, None));
        }
        let arr = self.expr(base, ctx)?;
        let mut args = vec![arr.clone()];
        for ix in idxs {
            args.push(self.array_index_value(ix, ctx)?);
        }
        let mut r = ir(I::ExprKind::Builtin("arr.get".into(), args), Ty::Num(dim), e.span.line);
        r.hint = arr.hint.clone();
        r.sf = arr.sf;
        Ok(Some(r))
    }

    fn array_index_value(&mut self, ix: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        if matches!(ix.kind, A::ExprKind::Slice { .. } | A::ExprKind::End) {
            return Err(self.err("an array index is a whole number (slices and end aren't supported for arrays yet)",
                                ix.span, None));
        }
        let v = self.expr(ix, ctx)?;
        self.need_num(&v, ix, "an array index")?;
        let d = ty_dim(&v.ty).unwrap();
        if !self.u.unify(&d, &DExpr::of(DIMLESS)) {
            return Err(self.err(format!("an array index must be a plain number (1, 2, 3, ...), not {}", self.desc(&d)),
                                ix.span, None));
        }
        if let I::ExprKind::Const(x) = v.kind {
            // A[1.5, 1] (red team 14 #9b): as for a list, before the program runs
            if x != x.trunc() || !x.is_finite() {
                let shown = if x.is_nan() { "NaN".to_string() } else if x.is_infinite() { "∞".into() } else { crate::arith::py_g(x) };
                return Err(self.err(format!("an array index must be a whole number (1, 2, 3, ...), not {shown}"),
                                    ix.span, None));
            }
        }
        Ok(v)
    }

    /// A[i, j, …] = value (or +=) on an array variable: IndexAssign(A, [i, j, …], value).
    pub fn array_index_assign(&mut self, b: I::SymId, s: &A::Stmt, ctx: &mut Ctx) -> CResult<I::Stmt> {
        let A::StmtKind::IndexAssign { target, index, index2, value, op, rest } = &s.kind else { unreachable!() };
        let Ty::Array { rank, dim } = self.module.syms[b].ty.clone() else { unreachable!() };
        let mut idx_ast: Vec<&A::Expr> = vec![index];
        idx_ast.extend(index2.iter());
        idx_ast.extend(rest.iter());
        if idx_ast.len() != rank {
            return Err(self.err(format!("{target} is a {rank}-dimensional array, so it takes {rank} indexes, like \
                                         {target}[{}] = …", ["i", "j", "k", "l"][..rank].join(", ")), s.span, None));
        }
        let mut idxs = vec![];
        for ix in &idx_ast {
            idxs.push(self.array_index_value(ix, ctx)?);
        }
        let v = if op != "=" {
            // A[i, j] += x is A[i, j] = A[i, j] + x
            let mut cur = crate::ast_ext::mk(A::ExprKind::Name { name: target.clone() }, s.span);
            for ix in &idx_ast {
                cur = crate::ast_ext::mk(A::ExprKind::Index { target: Box::new(cur), index: Some(Box::new((*ix).clone())) },
                                         s.span);
            }
            let bop: String = op.chars().next().unwrap().to_string();
            let val_ast = crate::ast_ext::mk(A::ExprKind::BinOp { op: bop, left: Box::new(cur),
                                                                  right: Box::new(value.clone()), implicit: false },
                                             s.span);
            self.expr(&val_ast, ctx)?
        } else {
            self.expr(value, ctx)?
        };
        self.need_num(&v, value, "an array entry")?;
        let vd = ty_dim(&v.ty).unwrap();
        self.unify_or(&dim, &vd, |c| format!("{target} is an array of {}; can't put {} in it", c.desc(&dim), c.desc(&vd)),
                      value.span, None)?;
        let line = s.span.line;
        let idx = ir(I::ExprKind::List(idxs), Ty::List(DExpr::of(DIMLESS)), line);
        Ok(self.stmt_at(I::StmtKind::IndexAssign(b, idx, v), s))
    }

    /// Arithmetic with an array: entry by entry with a number or an array of the same rank (+ and − need the same
    /// units; the shapes are checked when the program runs).
    pub fn array_arith(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        let op = if op == "×" { "*" } else { op };
        let rank = match (&a.ty, &b.ty) {
            (Ty::Array { rank: r1, .. }, Ty::Array { rank: r2, .. }) if r1 != r2 => {
                return Err(self.err(format!("can't combine a {r1}-dimensional array with a {r2}-dimensional one"), e.span,
                                    None));
            }
            (Ty::Array { rank, .. }, Ty::Array { .. } | Ty::Num(_)) | (Ty::Num(_), Ty::Array { rank, .. }) => *rank,
            _ => {
                return Err(self.err(format!("an array can be combined with numbers and arrays, not with {}",
                                            self.type_desc(if matches!(a.ty, Ty::Array { .. }) { &b.ty } else { &a.ty })),
                                    e.span, Some("to work on each entry, loop over the indexes".into())));
            }
        };
        let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
        let (bop, dim) = match op {
            "+" | "-" => {
                if !self.u.unify(&da, &db) {
                    let (sa, sb) = (self.desc(&da), self.desc(&db));
                    let msg = if op == "+" { format!("can't add {sa} to {sb}") } else { format!("can't subtract {sb} from {sa}") };
                    return Err(self.err(msg, e.span, Some("both sides of + and - must have the same units".into())));
                }
                (if op == "+" { I::BinOp::Add } else { I::BinOp::Sub }, da)
            }
            "*" => (I::BinOp::Mul, da.mul(&db)),
            "/" => (I::BinOp::Div, da.div(&db)),
            _ => return Err(self.err(format!("unknown operator {op}"), e.span, None)),
        };
        let hint = match op {
            "+" | "-" => a.hint.clone().or_else(|| b.hint.clone()),
            "*" => self.keep_hint(&a, &b),
            _ if self.dimless(&b) => a.hint.clone(),
            _ => None,
        };
        let sf = minsf(&[&a, &b]);
        let mut r = ir(I::ExprKind::Bin(bop, Box::new(a), Box::new(b)), Ty::Array { rank, dim }, e.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }
}
