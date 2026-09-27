//! Lists, indexing and `where`: a port of Checker.e_Where, e_ListLit, e_Index (the list part), e_Slice, e_End,
//! index_expr, _with_end, fixed_or_runtime_index, runtime_index and slice_expr from `fermium/checker.py`.
//! Indexing a vector or a matrix goes to `index_vecmat` / `index_mat2` (the vectors module).
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;

use crate::arith::minsf;
use crate::checker::*;
use crate::stmts::{ty_dim, written_code};

/// An index into a vector or matrix of known size: fixed (0-based) or computed at run time (1-based, #54).
#[derive(Clone, Debug)]
pub enum Idx {
    Fixed(i64),
    Run(I::Expr),
}

/// The text of the error of a slice whose end is before its start (Python's msg_id text).
pub const SLICE_MSG: &str = "a slice xs[a:b] runs from a up to b, so b can't be smaller than a − 1 (xs[a:a-1] is the \
                             empty list); to reverse a list use reverse(xs)";

fn py_g(x: f64) -> String {
    crate::arith::py_g(x)
}

impl Checker {
    pub fn e_where(&mut self, e: &A::Expr, value: &A::Expr, bindings: &[(String, A::Expr)], ctx: &mut Ctx)
                   -> CResult<Checked> {
        if let Some(r) = self.where_deriv(e, value, bindings, ctx) {
            // g = d/dt (a t^2) where a = 3: substitute, so the derivative can still be a function of t
            return r;
        }
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut c2 = ctx.child(scope);
        let mut binds = vec![];
        for (name, val) in bindings {
            let v = self.expr(val, &mut c2)?;
            let sym = self.new_sym(name, v.ty.clone(), &c2);
            {
                let s = &mut self.module.syms[sym];
                s.sf = v.sf;
                s.hint = v.hint.clone();
                s.direct = v.direct;
            }
            self.bind(scope, name, Binding::Sym(sym));
            binds.push((sym, v));
        }
        let body = self.expr(value, &mut c2)?;
        let (hint, sf, ty) = (body.hint.clone(), body.sf, body.ty.clone());
        let mut r = ir(I::ExprKind::Let(binds, Box::new(body)), ty, e.span.line);
        r.hint = hint;
        r.sf = sf;
        r.direct = 0;
        Ok(Checked::Val(r))
    }

    pub fn e_list_lit(&mut self, e: &A::Expr, items: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        let is_list = |x: &A::Expr| matches!(x.kind, A::ExprKind::ListLit { .. });
        let is_mat = |x: &A::Expr| matches!(&x.kind, A::ExprKind::ListLit { items } if !items.is_empty()
                                            && items.iter().all(is_list));
        if !items.is_empty() && items.iter().all(is_mat) {
            // a list of matrices written out: [[[1, 0], [0, 1]], [[0, 1], [1, 0]]] (D281)
            let mut vals = vec![];
            for x in items {
                vals.push(self.expr(x, ctx)?);
            }
            return self.vlist_literal(e, vals, items);
        }
        if !items.is_empty() && items.iter().all(is_list) {
            return self.matrix_literal(e, items, ctx);
        }
        if items.iter().any(is_list) {
            return Err(self.err("a matrix is written as a list of rows, like [[1, 2], [3, 4]]; lists of lists aren't \
                                 supported otherwise", e.span, None));
        }
        let mut vals = vec![];
        for x in items {
            vals.push(self.expr(x, ctx)?);
        }
        let line = e.span.line;
        if !vals.is_empty() && vals.iter().all(|v| matches!(v.ty, Ty::Str)) {
            return Ok(ir(I::ExprKind::List(vals), Ty::TextList, line));
        }
        if vals.iter().any(|v| matches!(v.ty, Ty::Str)) {
            return Err(self.err("a list can hold numbers or text, but not both", e.span, None));
        }
        if !vals.is_empty() && vals.iter().all(|v| matches!(v.ty, Ty::Vec { .. } | Ty::Mat { .. })) {
            return self.vlist_literal(e, vals, items);
        }
        if !vals.is_empty() && vals.iter().all(|v| matches!(v.ty, Ty::Complex(_))) {
            // (a real number among them stays v1's error, "a list element must be a number …": write 2 + 0i)
            return self.clist_literal(e, vals, items);
        }
        let dim = DExpr::fresh();
        for (it, node) in vals.iter().zip(items) {
            self.need_num(it, node, "a list element")?;
            let d = ty_dim(&it.ty).unwrap();
            self.unify_or(&dim, &d, |c| format!("all elements of a list need the same units; this one is {} but \
                                                 earlier ones are {}", c.desc(&d), c.desc(&dim)), node.span, None)?;
        }
        let hint = vals.first().and_then(|v| v.hint.clone());
        let sf = if vals.is_empty() { None } else { minsf(&vals.iter().collect::<Vec<_>>()) };
        let direct = written_code(&vals);
        let mut r = ir(I::ExprKind::List(vals), Ty::List(dim), line);
        r.hint = hint;
        r.sf = sf;
        r.direct = direct;
        Ok(r)
    }

    /// A list index: a plain whole number (end = the length).
    pub fn index_expr(&mut self, idx_ast: &A::Expr, target: &I::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let line = idx_ast.span.line;
        if matches!(idx_ast.kind, A::ExprKind::End) {
            return Ok(ir(I::ExprKind::Builtin("len".into(), vec![target.clone()]), dimless_num(), line));
        }
        let idx = self.with_end(idx_ast, target, ctx)?;
        self.need_num(&idx, idx_ast, "a list index")?;
        if let I::ExprKind::Const(v) = idx.kind {
            if !v.is_finite() {
                // xs[inf], xs[0/0] (A32)
                let shown = if v.is_nan() { "NaN" } else if v > 0.0 { "∞" } else { "-∞" };
                return Err(self.err(format!("a list index must be a whole number (1, 2, 3, ...), not {shown}"),
                                    idx_ast.span, None));
            }
            if v != v.trunc() {
                return Err(self.err(format!("a list index must be a whole number (1, 2, 3, ...), not {}", py_g(v)),
                                    idx_ast.span, None));
            }
        }
        let d = ty_dim(&idx.ty).unwrap();
        if !self.u.unify(&d, &DExpr::of(DIMLESS)) {
            return Err(self.err(format!("a list index must be a plain number (1, 2, 3, ...), not {}", self.desc(&d)),
                                idx_ast.span, None));
        }
        Ok(idx)
    }

    fn with_end(&mut self, idx_ast: &A::Expr, target: &I::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        if !idx_ast.walk().iter().any(|n| matches!(n.kind, A::ExprKind::End)) {
            return self.expr(idx_ast, ctx);
        }
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut c2 = ctx.child(scope);
        let sym = self.new_sym("end", dimless_num(), &c2);
        self.bind(scope, "end", Binding::Sym(sym));
        let mut repl = idx_ast.clone();
        repl.walk_mut(&mut |n| {
            if matches!(n.kind, A::ExprKind::End) {
                n.kind = A::ExprKind::Name { name: "end".into() };
            }
        });
        let body = self.expr(&repl, &mut c2)?;
        let line = idx_ast.span.line;
        let len = ir(I::ExprKind::Builtin("len".into(), vec![target.clone()]), dimless_num(), line);
        let ty = body.ty.clone();
        Ok(ir(I::ExprKind::Let(vec![(sym, len)], Box::new(body)), ty, line))
    }

    /// An index into a vector or matrix of known size: fixed (0-based) or an IR expression (1-based, checked at
    /// run time; #54).
    pub fn fixed_or_runtime_index(&mut self, idx_ast: &A::Expr, size: usize, ctx: &mut Ctx) -> CResult<Idx> {
        if matches!(idx_ast.kind, A::ExprKind::End) {
            return Ok(Idx::Fixed(size as i64 - 1));
        }
        let n = ir(I::ExprKind::Const(size as f64), dimless_num(), idx_ast.span.line);
        let mut idx = self.index_expr(idx_ast, &n, ctx)?;
        if let I::ExprKind::Let(binds, value) = &idx.kind {
            if matches!(value.kind, I::ExprKind::Const(_)) {
                idx = (**value).clone();
            } else {
                // v[end - 1]: `end` is the size, known here
                let binds = binds.iter().map(|(s, _)| (*s, n.clone())).collect();
                let (value, ty, line) = ((**value).clone(), idx.ty.clone(), idx.line);
                idx = ir(I::ExprKind::Let(binds, Box::new(value)), ty, line);
            }
        }
        if let I::ExprKind::Const(v) = idx.kind {
            return Ok(Idx::Fixed(v as i64 - 1));
        }
        Ok(Idx::Run(idx))
    }

    /// t's entries at flat base Σ (i − 1)·stride + offs; pieces: [(index, size, stride)].
    pub fn runtime_index(&mut self, t: I::Expr, pieces: Vec<(Idx, usize, usize)>, offs: Vec<usize>, ty: Ty,
                         node: &A::Expr) -> CResult<I::Expr> {
        if matches!(t.ty, Ty::Vec { dims: Some(_), .. }) {
            return Err(self.err("this vector's components have different units, so pick one with a fixed number, \
                                 like v[1]", node.span, None));
        }
        let mut idxs = vec![];
        for (k, size, stride) in pieces {
            let k = match k {
                Idx::Fixed(k) => {
                    if !(0..size as i64).contains(&k) {
                        return Err(self.err(format!("there is no index {} here: valid indexes are 1 to {size}", k + 1),
                                            node.span, None));
                    }
                    ir(I::ExprKind::Const((k + 1) as f64), dimless_num(), node.span.line)
                }
                Idx::Run(e) => e,
            };
            idxs.push((k, size, stride));
        }
        let (hint, sf) = (t.hint.clone(), t.sf);
        let mut r = ir(I::ExprKind::VecIndex { v: Box::new(t), idxs, offs }, ty, node.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    pub fn e_slice(&mut self, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.err("a:b can only be used inside [...] to take part of a list, like xs[2:5]", e.span, None))
    }

    pub fn e_end(&mut self, e: &A::Expr) -> CResult<I::Expr> {
        Err(self.err("'end' can only be used inside [...] to mean the last element", e.span, None))
    }

    /// xs[a:b]: a new list of elements a to b, both included (1-based, D114).
    fn slice_expr(&mut self, e: &A::Expr, target: &A::Expr, lo: Option<&A::Expr>, hi: Option<&A::Expr>,
                  ctx: &mut Ctx) -> CResult<I::Expr> {
        let t = match self.expr_any(target, ctx)? {
            Checked::Sol(view) if self.sols[view].n == 1 => Checked::Val(self.sol_values(view, e)?),
            other => other,
        };
        if let Checked::Val(v) = &t {
            if matches!(v.ty, Ty::Data(_)) {
                // `fit … to data[2:5]` (red team round 4 #17, D209)
                return Err(self.err("a data table can't be sliced with [a:b]; its columns are lists, and those can be",
                                    target.span,
                                    Some("to fit some of the rows, slice the columns and make a table of them:  fit … \
                                          to table(L = data.L[2:5], T = data.T[2:5])".into())));
            }
        }
        let t = match t {
            Checked::Val(v) if matches!(v.ty, Ty::List(_)) => v,
            other => {
                let what = match &other {
                    Checked::Val(v) if matches!(v.ty, Ty::Vec { .. } | Ty::Mat { .. } | Ty::Complex(_)) => "a vector",
                    _ => "this",
                };
                return Err(self.err(format!("only lists can be sliced with [a:b], and {what} isn't a list"),
                                    target.span, Some("pick single components with v[1], v[2], ...".into())));
            }
        };
        let line = e.span.line;
        let lo = match lo {
            Some(x) => self.index_expr(x, &t, ctx)?,
            None => ir(I::ExprKind::Const(1.0), dimless_num(), line),
        };
        let hi = match hi {
            Some(x) => self.index_expr(x, &t, ctx)?,
            None => ir(I::ExprKind::Builtin("len".into(), vec![t.clone()]), dimless_num(), line),
        };
        let (hint, sf, ty) = (t.hint.clone(), t.sf, Ty::List(ty_dim(&t.ty).unwrap()));
        let mut r = ir(I::ExprKind::Builtin("slice".into(), vec![t, lo, hi]), ty, line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    pub fn e_index(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Index { target, index } = &e.kind else { unreachable!() };
        let Some(index) = index else {
            return Err(self.err("only lists can be indexed with [...]", target.span, None));
        };
        if let A::ExprKind::Slice { lo, hi } = &index.kind {
            return self.slice_expr(e, target, lo.as_deref(), hi.as_deref(), ctx);
        }
        if let Some(r) = self.array_index(e, ctx)? {
            return Ok(r); // A[i, j, …] of an array (D283)
        }
        if let A::ExprKind::Index { target: inner_t, index: Some(inner_i) } = &target.kind {
            // M[i, j] (parsed as M[i][j]) or M[i][j]
            let _ = (inner_t, inner_i);
            if let Some(r) = self.index_matrix_entry(e, ctx)? {
                return Ok(r);
            }
        }
        let t = self.expr_any(target, ctx)?;
        if let Checked::Val(v) = &t {
            match v.ty {
                Ty::Complex(_) => {
                    return Err(self.err("a complex number can't be indexed with [...]", e.span,
                                        Some("its parts are re(z) and im(z) (or z.re and z.im)".into())));
                }
                Ty::Mat { .. } | Ty::Vec { .. } => {
                    let Checked::Val(v) = t else { unreachable!() };
                    return match self.index_vecmat(e, &v, ctx)? {
                        Some(r) => Ok(r),
                        None => Err(self.not_ported("indexing", e.span)),
                    };
                }
                _ => {}
            }
        }
        let t = match t {
            Checked::Sol(view) if self.sols[view].n > 1 || self.sols[view].list.is_some() => return self.sol_index(view, e, index, ctx),
            Checked::Sol(view) => Checked::Val(self.sol_values(view, e)?),
            other => other,
        };
        let line = e.span.line;
        if let Checked::Val(v) = &t {
            if matches!(v.ty, Ty::TextList) {
                let idx = self.index_expr(index, v, ctx)?;
                return Ok(ir(I::ExprKind::Index(Box::new(v.clone()), Box::new(idx)), Ty::Str, line));
            }
            if matches!(v.ty, Ty::ComplexList(_)) {
                let idx = self.index_expr(index, v, ctx)?;
                return self.clist_index(v.clone(), idx, e);
            }
            if let Ty::VList(el) = &v.ty {
                // ps[i]: the i-th vector or matrix (D281)
                let idx = self.index_expr(index, v, ctx)?;
                let mut r = ir(I::ExprKind::Index(Box::new(v.clone()), Box::new(idx)), (**el).clone(), line);
                r.hint = v.hint.clone();
                r.sf = v.sf;
                return Ok(r);
            }
        }
        let t = match t {
            Checked::Val(v) if matches!(v.ty, Ty::List(_)) => v,
            _ => {
                return Err(self.err("only lists can be indexed with [...]", target.span,
                                    Some("to call a function use parentheses: f(x)".into())));
            }
        };
        let idx = self.index_expr(index, &t, ctx)?;
        let (hint, sf, d) = (t.hint.clone(), t.sf, ty_dim(&t.ty).unwrap());
        let mut r = ir(I::ExprKind::Index(Box::new(t), Box::new(idx)), Ty::Num(d), line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    /// [<1, 2> m, <3, 4> m] or [A, B]: a list of vectors or matrices, all the same size and units (D281).
    fn vlist_literal(&mut self, e: &A::Expr, vals: Vec<I::Expr>, items: &[A::Expr]) -> CResult<I::Expr> {
        let first = vals[0].ty.clone();
        let (n, r, c) = match &first {
            Ty::Vec { dims: Some(_), .. } => {
                return Err(self.err(format!("a list of vectors needs one unit for all components; this is {}",
                                            self.type_desc(&first)), items[0].span,
                                    Some("put each component in its own list instead".into())));
            }
            Ty::Vec { n, .. } => (*n, 0, 0),
            Ty::Mat { r, c, .. } => (0, *r, *c),
            _ => unreachable!(),
        };
        let dim = DExpr::fresh();
        for (v, node) in vals.iter().zip(items) {
            let same = match &v.ty {
                Ty::Vec { n: m, dims: None, .. } => *m == n && n > 0,
                Ty::Mat { r: r2, c: c2, .. } => (*r2, *c2) == (r, c) && n == 0,
                _ => false,
            };
            if !same {
                return Err(self.err(format!("all elements of a list must be the same kind of value: this one is {} \
                                             but the first is {}", self.type_desc(&v.ty), self.type_desc(&first)),
                                    node.span, None));
            }
            let d = ty_dim(&v.ty).unwrap();
            self.unify_or(&dim, &d, |c| format!("all elements of a list need the same units; this one is {} but \
                                                 earlier ones are {}", c.desc(&d), c.desc(&dim)), node.span, None)?;
        }
        let elem = if n > 0 { Ty::Vec { n, dim: Some(dim), dims: None } } else { Ty::Mat { r, c, dim } };
        let hint = vals.first().and_then(|v| v.hint.clone());
        let sf = minsf(&vals.iter().collect::<Vec<_>>());
        let direct = written_code(&vals);
        let mut out = ir(I::ExprKind::List(vals), Ty::VList(Box::new(elem)), e.span.line);
        out.hint = hint;
        out.sf = sf;
        out.direct = direct;
        Ok(out)
    }

    /// [1 + 2i, 3i]: a list of complex numbers sharing one unit (D281).
    fn clist_literal(&mut self, e: &A::Expr, vals: Vec<I::Expr>, items: &[A::Expr]) -> CResult<I::Expr> {
        let dim = DExpr::fresh();
        for (v, node) in vals.iter().zip(items) {
            if !matches!(v.ty, Ty::Num(_) | Ty::Complex(_)) {
                return Err(self.err(format!("a list of complex numbers can't hold {}", self.type_desc(&v.ty)),
                                    node.span, None));
            }
            let d = ty_dim(&v.ty).unwrap();
            self.unify_or(&dim, &d, |c| format!("all elements of a list need the same units; this one is {} but \
                                                 earlier ones are {}", c.desc(&d), c.desc(&dim)), node.span, None)?;
        }
        let hint = vals.iter().find_map(|v| v.hint.clone());
        let sf = minsf(&vals.iter().collect::<Vec<_>>());
        let mut out = ir(I::ExprKind::List(vals), Ty::ComplexList(dim), e.span.line);
        out.hint = hint;
        out.sf = sf;
        Ok(out)
    }
}
