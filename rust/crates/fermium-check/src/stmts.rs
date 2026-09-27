//! Statements: a port of Checker.s_ExprStmt … s_Assert from `fermium/checker.py` (assignment, push, loops,
//! definite assignment, return/break/continue/assert). Parallel for lives in `parallel`.
use std::collections::HashSet;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;

use crate::checker::*;

// constants whose names are also common variables (the Hubble h, an eccentricity e, G = 1 units): overriding
// one with a plain number, or a value in the constant's own units, warns once at the assignment (D213)
pub const WELL_KNOWN_CONSTANTS: &[(&str, &str)] = &[
    ("h", "Planck's constant"),
    ("c", "the speed of light"),
    ("G", "the gravitational constant"),
    ("e", "the elementary charge"),
    ("k_B", "Boltzmann's constant"),
];

/// ΔT, Δθ, δT, delta_T (which the lexer spells δ_T), dT: a name that says "a change" (D181).
pub fn delta_name(name: &str) -> bool {
    name.starts_with('Δ') || name.starts_with('δ') || name.to_lowercase().starts_with("delta")
        || matches!(name, "dT" | "dθ" | "d_T")
}

pub fn same_kind(a: &Ty, b: &Ty) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// The shared dimension of a numeric type (NumTy, ListTy, VecTy with one dimension, MatTy, complex…).
pub fn ty_dim(t: &Ty) -> Option<DExpr> {
    match t {
        Ty::Num(d) | Ty::List(d) | Ty::Complex(d) | Ty::ComplexList(d) => Some(d.clone()),
        Ty::Vec { dim: Some(d), .. } => Some(d.clone()),
        Ty::Mat { dim, .. } => Some(dim.clone()),
        Ty::VList(el) => ty_dim(el),
        Ty::Array { dim, .. } => Some(dim.clone()),
        _ => None,
    }
}

impl Checker {
    pub fn s_expr_stmt(&mut self, s: &A::Stmt, e: &A::Expr, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if let A::ExprKind::Call { func, args } = &e.kind {
            if let A::ExprKind::Name { name: fname } = &func.kind {
                let shadowed = self.lookup(ctx.scope, fname).is_some();
                match fname.as_str() {
                    "push" | "append" => return self.push_stmt(e, args, ctx).map(|x| vec![x]),
                    "clear" if !shadowed => {
                        let name_arg = match args.as_slice() {
                            [a @ A::Expr { kind: A::ExprKind::Name { .. }, .. }] => a,
                            _ => return Err(self.err("clear needs a list variable: clear(xs)", e.span, None)),
                        };
                        let lst = self.expr(name_arg, ctx)?;
                        if let (I::ExprKind::Var(sym), Ty::List(_) | Ty::TextList | Ty::VList(_) | Ty::ComplexList(_))
                            = (&lst.kind, &lst.ty)
                        {
                            return Ok(vec![self.stmt_at(I::StmtKind::Clear(*sym), s)]);
                        }
                        let A::ExprKind::Name { name: n } = &name_arg.kind else { unreachable!() };
                        return Err(self.err(format!("clear empties a list, and {n} is {}", self.type_desc(&lst.ty)),
                                            name_arg.span, None));
                    }
                    "seed" if !shadowed => return self.seed_stmt(e, ctx),
                    _ => {}
                }
            }
        }
        let v = self.expr_any(e, ctx)?;
        let v = match v {
            Checked::Val(v) => v,
            _ if ctx.is_main && self.opts.repl && ctx.lam.is_none() => {
                return Ok(vec![self.print_items(vec![v], &[e.clone()], ctx)?]);
            }
            _ => return Ok(vec![]),
        };
        if ctx.is_main && self.opts.repl && ctx.lam.is_none() {
            return Ok(vec![self.print_items(vec![Checked::Val(v)], &[e.clone()], ctx)?]);
        }
        if !matches!(v.kind, I::ExprKind::Call(..) | I::ExprKind::Map { .. }) {
            self.warn("this line computes a value but doesn't use it", s.span,
                      Some("use print to show it, or store it:  name = ...".into()));
        }
        Ok(vec![self.stmt_at(I::StmtKind::Expr(v), s)])
    }

    pub fn stmt_at(&self, kind: I::StmtKind, s: &A::Stmt) -> I::Stmt {
        I::Stmt { kind, line: s.span.line }
    }

    pub fn push_stmt(&mut self, e: &A::Expr, args: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Stmt> {
        if args.len() != 2 || !matches!(args[0].kind, A::ExprKind::Name { .. }) {
            return Err(self.err("push needs a list variable and a value: push(xs, x)", e.span, None));
        }
        let lst = self.expr(&args[0], ctx)?;
        let line = e.span.line;
        if let Some(st) = self.push_other(&lst, args, ctx)? {
            return Ok(I::Stmt { kind: st, line });
        }
        if let (I::ExprKind::Var(sym), Ty::TextList) = (&lst.kind, &lst.ty) {
            let v = self.expr(&args[1], ctx)?;
            if !matches!(v.ty, Ty::Str) {
                return Err(self.err("this list holds text, so you can only push text onto it", args[1].span, None));
            }
            return Ok(I::Stmt { kind: I::StmtKind::Push(*sym, v), line });
        }
        let (sym, ldim) = match (&lst.kind, &lst.ty) {
            (I::ExprKind::Var(sym), Ty::List(d)) => (*sym, d.clone()),
            _ => {
                return Err(self.err(format!("push needs a list as its first argument, not {}",
                                            self.type_desc(&lst.ty)), args[0].span, None))
            }
        };
        let v = self.expr(&args[1], ctx)?;
        self.need_num(&v, &args[1], "the value to push")?;
        let vd = ty_dim(&v.ty).unwrap();
        self.unify_or(&ldim, &vd, |c| format!("can't add {} to a list of {}", c.desc(&vd), c.desc(&ldim)),
                      args[1].span, None)?;
        if self.module.syms[sym].hint.is_none() {
            if let Some(h) = &v.hint {
                if h.offset == 0.0 {
                    // a list filled by push shows the pushed values' unit (MeV, not J)
                    self.module.syms[sym].hint = Some(h.clone());
                }
            }
        }
        Ok(I::Stmt { kind: I::StmtKind::Push(sym, v), line })
    }

    /// push onto a list of vectors, matrices or complex numbers, or onto a list set to [] that becomes one (and
    /// text onto []): D281. None: an ordinary list of numbers or of text.
    fn push_other(&mut self, lst: &I::Expr, args: &[A::Expr], ctx: &mut Ctx) -> CResult<Option<I::StmtKind>> {
        let I::ExprKind::Var(sym) = lst.kind else { return Ok(None) };
        let lty = self.module.syms[sym].ty.clone();
        let fresh_empty = self.extra[sym].empty_list && matches!(lty, Ty::List(_));
        if !fresh_empty && !matches!(lty, Ty::VList(_) | Ty::ComplexList(_)) {
            return Ok(None);
        }
        let v = self.expr(&args[1], ctx)?;
        let new_ty = match (&lty, &v.ty) {
            (Ty::List(_), Ty::Vec { dims: Some(_), .. }) => {
                return Err(self.err(format!("a list of vectors needs one unit for all components; this is {}",
                                            self.type_desc(&v.ty)), args[1].span, None));
            }
            (Ty::List(_), Ty::Vec { .. } | Ty::Mat { .. }) => Ty::VList(Box::new(v.ty.clone())),
            (Ty::List(_), Ty::Complex(d)) => Ty::ComplexList(d.clone()),
            (Ty::List(_), Ty::Str) => Ty::TextList,
            (Ty::List(_), _) => return Ok(None),
            (Ty::VList(el), _) => {
                let same = match (&**el, &v.ty) {
                    (Ty::Vec { n, .. }, Ty::Vec { n: m, dims: None, .. }) => n == m,
                    (Ty::Mat { r, c, .. }, Ty::Mat { r: r2, c: c2, .. }) => (r, c) == (r2, c2),
                    _ => false,
                };
                if !same {
                    let name = self.module.syms[sym].name.clone();
                    return Err(self.err(format!("{name} is {}, so you can't push {} onto it", self.type_desc(&lty),
                                                self.type_desc(&v.ty)), args[1].span, None));
                }
                let (ld, vd) = (ty_dim(el).unwrap(), ty_dim(&v.ty).unwrap());
                self.unify_or(&ld, &vd, |c| format!("can't add {} to a list of {}", c.desc(&vd), c.desc(&ld)),
                              args[1].span, None)?;
                lty.clone()
            }
            (Ty::ComplexList(d), Ty::Complex(_) | Ty::Num(_)) => {
                let vd = ty_dim(&v.ty).unwrap();
                self.unify_or(d, &vd, |c| format!("can't add {} to a list of complex numbers of {}", c.desc(&vd),
                                                  c.desc(d)), args[1].span, None)?;
                lty.clone()
            }
            (Ty::ComplexList(_), _) => {
                return Err(self.err(format!("this list holds complex numbers, so you can't push {} onto it",
                                            self.type_desc(&v.ty)), args[1].span, None));
            }
            _ => return Ok(None),
        };
        if fresh_empty {
            self.module.syms[sym].ty = new_ty;
            self.extra[sym].empty_list = false;
        }
        if self.module.syms[sym].hint.is_none() {
            if let Some(h) = &v.hint {
                if h.offset == 0.0 {
                    self.module.syms[sym].hint = Some(h.clone());
                }
            }
        }
        Ok(Some(I::StmtKind::Push(sym, v)))
    }

    pub fn s_assign(&mut self, s: &A::Stmt, name: &str, value: &A::Expr, op: &str, ctx: &mut Ctx)
                    -> CResult<Vec<I::Stmt>> {
        if op != "=" {
            let target = crate::ast_ext::mk(A::ExprKind::Name { name: name.to_string() }, s.span);
            let bop: String = op.chars().next().unwrap().to_string();
            let val_ast = crate::ast_ext::mk(A::ExprKind::BinOp { op: bop, left: Box::new(target),
                                                            right: Box::new(value.clone()), implicit: false },
                                       s.span);
            let v = self.expr(&val_ast, ctx)?;
            if !matches!(self.lookup(ctx.scope, name), Some((Binding::Sym(_), _))) {
                return Err(self.err(format!("{name} needs a value before you can use {op} on it"), s.span, None));
            }
            return self.assign_to(name, v, s.span, Some(value), ctx).map(|x| vec![x]);
        }
        if !ctx.is_main {
            if let A::ExprKind::ListLit { items } = &value.kind {
                if items.is_empty() {
                    if let Some((Binding::Sym(b), _)) = self.lookup(ctx.scope, name) {
                        let bs = &self.module.syms[b];
                        let key = format!("local-list:{name}");
                        if matches!(bs.ty, Ty::List(_) | Ty::TextList) && bs.func != self.owner_func(ctx)
                            && !self.warned.contains(&key)
                        {
                            // `xs = []` in a function makes a new list there; the program's xs is unchanged (D216)
                            self.warned.insert(key);
                            self.warn(format!("{name} = [] here makes a new list {name} inside this function; the \
                                               program's list {name} is unchanged"), s.span,
                                      Some(format!("to empty the program's list from here, write  clear({name})")));
                        }
                    }
                }
            }
        }
        match self.expr_any(value, ctx)? {
            Checked::Func { info, .. } => {
                self.bind(ctx.scope, name, Binding::Func(info));
                let dn = &self.funcs[info].display_name;
                if dn.starts_with('<') || dn.contains('\'') || dn.contains('∂') || dn.starts_with('λ')
                    || dn.starts_with("d/d") || dn.starts_with("∫d")
                {
                    self.funcs[info].display_name = name.to_string();
                    self.funcs[info].anon_label = None;
                }
                Ok(vec![])
            }
            Checked::Sol(view) => {
                self.bind(ctx.scope, name, Binding::Sol(view));
                Ok(vec![])
            }
            Checked::Val(v) => self.assign_to(name, v, s.span, Some(value), ctx).map(|x| vec![x]),
        }
    }

    pub fn assign_to(&mut self, name: &str, v: I::Expr, span: A::Span, value_ast: Option<&A::Expr>, ctx: &mut Ctx)
                     -> CResult<I::Stmt> {
        if matches!(v.ty, Ty::Void) {
            return Err(self.err("this doesn't produce a value to store", span, None));
        }
        if delta_name(name) && self.abs_temp(&v) {
            let at = value_ast.map(|e| e.span).unwrap_or(span);
            return Err(self.delta_abs_temp_error(name, &v, at));
        }
        let found = self.lookup(ctx.scope, name);
        let owned = match &found {
            Some((Binding::Sym(b), _)) => {
                let bs = &self.module.syms[*b];
                (bs.func == self.owner_func(ctx) || (bs.storage == I::Storage::Arena && ctx.is_main))
                    && match ctx.lam {
                        None => true,
                        Some(l) => self.module.lambdas[l].locals.contains(b),
                    }
            }
            _ => false,
        };
        let sym;
        if owned {
            let Some((Binding::Sym(b), _)) = found else { unreachable!() };
            if self.extra[b].nat != self.nat {
                return Err(self.err(format!("{name} was set outside this {} region, so it can't be changed here \
                                             (its units mean something different there)", self.nat_label()),
                                    span, Some(format!("use a new name for the value in {} units", self.nat_name()))));
            }
            let mut s = b;
            let sty = self.module.syms[s].ty.clone();
            let empty_lit = matches!(&v.kind, I::ExprKind::List(items) if items.is_empty());
            if empty_lit && matches!(sty, Ty::VList(_) | Ty::ComplexList(_) | Ty::TextList) {
                // ps = [] again: an empty list of the same kind (D281)
                let mut v = v;
                v.ty = sty.clone();
                self.extra[s].assigned = true;
                self.note_assign(ctx, s, false);
                return Ok(I::Stmt { kind: I::StmtKind::Assign(s, v), line: span.line });
            }
            if !empty_lit {
                self.extra[s].empty_list = false;
            }
            if !same_kind(&sty, &v.ty) {
                let mut hint = "use a different name for the new value".to_string();
                if matches!(sty, Ty::Num(_)) && matches!(v.ty, Ty::Complex(_)) {
                    hint = format!("to let {name} become complex, start it as a complex number, e.g.  {name} = 0i");
                }
                return Err(self.err(format!("{name} holds {}; it can't now hold {}", self.type_desc(&sty),
                                            self.type_desc(&v.ty)), span, Some(hint)));
            }
            match (&sty, &v.ty) {
                (Ty::Vec { n: a, .. }, Ty::Vec { n: b, .. }) if a != b => {
                    return Err(self.err(format!("{name} holds a {a}-vector; it can't now hold a {b}-vector"), span,
                                        None));
                }
                (Ty::Mat { r, c, .. }, Ty::Mat { r: r2, c: c2, .. }) if (r, c) != (r2, c2) => {
                    return Err(self.err(format!("{name} holds a {r}×{c} matrix; it can't now hold a {r2}×{c2} \
                                                 matrix"), span, None));
                }
                (Ty::Array { rank: a, .. }, Ty::Array { rank: b, .. }) if a != b => {
                    return Err(self.err(format!("{name} holds a {a}-dimensional array; it can't now hold a \
                                                 {b}-dimensional one"), span, None));
                }
                _ => {}
            }
            let mixed = matches!(sty, Ty::Vec { dims: Some(_), .. }) || matches!(v.ty, Ty::Vec { dims: Some(_), .. });
            if matches!(sty, Ty::Vec { .. }) && mixed {
                if self.vec_unify(&sty, &v.ty).is_some() {
                    return Err(self.err(format!("{name} is {}; it can't now hold {}", self.type_desc(&sty),
                                                self.type_desc(&v.ty)), span,
                                        Some("each variable keeps its units; use a new name for a different \
                                              quantity".into())));
                }
            } else if matches!(sty, Ty::Num(_) | Ty::List(_) | Ty::Vec { .. } | Ty::Mat { .. } | Ty::Array { .. }) {
                let (a, bd) = (ty_dim(&sty).unwrap(), ty_dim(&v.ty).unwrap());
                if !self.u.unify(&a, &bd) {
                    if self.opts.repl && ctx.is_main {
                        s = self.new_sym(name, v.ty.clone(), ctx);
                        self.bind(ctx.scope, name, Binding::Sym(s));
                    } else {
                        return Err(self.err(format!("{name} is {}; it can't now hold {}", self.desc(&a),
                                                    self.desc(&bd)), span,
                                            Some("each variable keeps its units; use a new name for a different \
                                                  quantity".into())));
                    }
                }
            }
            let vm = crate::vecmat::mixed_of(&v);
            if vm.is_some() && (ctx.loop_depth == 0 && ctx.branch == 0 || self.extra[s].mixed_hint.is_none()) {
                self.extra[s].mixed_hint = vm; // a MixedHint as the new hint
            }
            let fit = I::uses_fit_sf(&v);
            if ctx.loop_depth == 0 && ctx.branch == 0 {
                self.extra[s].fit_sf = fit;
            } else {
                self.extra[s].fit_sf |= fit;
            }
            let ms = &mut self.module.syms[s];
            if ctx.loop_depth == 0 && ctx.branch == 0 {
                // straight-line code: the variable now shows the new value's precision and unit
                ms.sf = v.sf;
                ms.direct = v.direct;
                if v.hint.is_some() {
                    ms.hint = v.hint.clone();
                }
            } else {
                if let Some(vsf) = v.sf {
                    ms.sf = Some(ms.sf.map_or(vsf, |x| x.min(vsf)));
                }
                ms.direct = 0;
                if v.hint.is_some() && ms.hint.is_none() {
                    ms.hint = v.hint.clone();
                }
            }
            sym = s;
        } else {
            if matches!(v.ty, Ty::Sol(_)) {
                return Err(self.err("can't store an ODE solution in a variable this way", span, None));
            }
            if let Some((Binding::Const(b), _)) = &found {
                let well_known = WELL_KNOWN_CONSTANTS.iter().find(|(n, _)| *n == name).map(|(_, w)| *w);
                if !self.opts.repl && self.used_consts.contains(name) {
                    // redefining a constant the program already used as the constant (gauntlet friction #34)
                    self.warn(format!("{name} is the built-in {}; from here on, {name} means your value", b.desc),
                              A::Span { length: 1, ..span },
                              Some(format!("pick another name if you still need the constant {name}")));
                } else if let (false, Some(what), Ty::Num(d)) = (self.opts.repl, well_known, &v.ty) {
                    let key = format!("const:{name}");
                    if self.u.is_concrete(d) && !self.warned.contains(&key) {
                        let r = self.u.resolve(d);
                        if r == DIMLESS || r == b.unit.dim {
                            // `h = 0.6736` (the Hubble h): say so once, at the assignment (D213)
                            self.warned.insert(key);
                            self.warn(format!("{name} ({what}) is now your variable: from here on, {name} means \
                                               your value"), A::Span { length: 1, ..span },
                                      Some(format!("that's fine if you don't need {what} below; otherwise pick \
                                                    another name, like {name}_0 or {name}2")));
                        }
                    }
                }
            }
            sym = self.new_sym(name, v.ty.clone(), ctx);
            self.bind(ctx.scope, name, Binding::Sym(sym));
            let ms = &mut self.module.syms[sym];
            ms.sf = v.sf;
            ms.hint = v.hint.clone();
            ms.direct = v.direct;
            self.extra[sym].tdelta = v.get_extra().is_some_and(|x| x.tdelta);
            self.extra[sym].fit_sf = I::uses_fit_sf(&v);
            self.extra[sym].mixed_hint = crate::vecmat::mixed_of(&v);
            self.extra[sym].empty_list = matches!(&v.kind, I::ExprKind::List(items) if items.is_empty());
        }
        self.extra[sym].assigned = true;
        self.note_assign(ctx, sym, !owned);
        Ok(I::Stmt { kind: I::StmtKind::Assign(sym, v), line: span.line })
    }

    pub fn s_index_assign(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::IndexAssign { target, index, index2, value, op, rest } = &s.kind else { unreachable!() };
        let found = self.lookup(ctx.scope, target);
        if let Some((Binding::Sym(b), _)) = &found {
            if matches!(self.module.syms[*b].ty, Ty::Array { .. }) {
                return self.array_index_assign(*b, s, ctx).map(|x| vec![x]);
            }
        }
        if !rest.is_empty() {
            return Err(self.err(format!("{target}[i, j, k] = … sets an entry of an array of 3 or more dimensions, but \
                                         {target} isn't one"), s.span, None));
        }
        if let Some((Binding::Sym(b), _)) = &found {
            if matches!(self.module.syms[*b].ty, Ty::Vec { .. } | Ty::Mat { .. }) {
                return self.entry_assign(*b, s, ctx).map(|x| vec![x]);
            }
        }
        if let Some((Binding::Sym(b), _)) = &found {
            if matches!(self.module.syms[*b].ty, Ty::VList(_) | Ty::ComplexList(_) | Ty::TextList) && index2.is_none() {
                return self.other_index_assign(*b, s, ctx).map(|x| vec![x]);
            }
        }
        let is_list = |c: &Checker| match &found {
            Some((Binding::Sym(b), _)) => matches!(c.module.syms[*b].ty, Ty::List(_)),
            _ => false,
        };
        if index2.is_some() {
            let what = if is_list(self) { "a list" } else { "not a matrix" };
            return Err(self.err(format!("{target}[i, j] = … sets an entry of a matrix, but {target} is {what}"),
                                s.span, None));
        }
        if !is_list(self) {
            return Err(self.err(format!("{target} isn't a list, so you can't set {target}[...]"), s.span, None));
        }
        let Some((Binding::Sym(b), _)) = found else { unreachable!() };
        let name_ast = crate::ast_ext::mk(A::ExprKind::Name { name: target.clone() }, s.span);
        let tgt = self.expr(&name_ast, ctx)?;
        let idx = self.index_expr(index, &tgt, ctx)?;
        let v = if op != "=" {
            let cur = crate::ast_ext::mk(A::ExprKind::Index { target: Box::new(name_ast.clone()),
                                                        index: Some(Box::new(index.clone())) }, s.span);
            let bop: String = op.chars().next().unwrap().to_string();
            let val_ast = crate::ast_ext::mk(A::ExprKind::BinOp { op: bop, left: Box::new(cur),
                                                            right: Box::new(value.clone()), implicit: false },
                                       s.span);
            self.expr(&val_ast, ctx)?
        } else {
            self.expr(value, ctx)?
        };
        self.need_num(&v, value, "a list element")?;
        let (bd, vd) = (ty_dim(&self.module.syms[b].ty).unwrap(), ty_dim(&v.ty).unwrap());
        self.unify_or(&bd, &vd, |c| format!("{target} is a list of {}; can't put {} in it", c.desc(&bd), c.desc(&vd)),
                      value.span, None)?;
        let I::ExprKind::Var(tsym) = tgt.kind else { unreachable!() };
        Ok(vec![self.stmt_at(I::StmtKind::IndexAssign(tsym, idx, v), s)])
    }

    /// ps[i] = <…> (or +=) on a list of vectors or matrices, zs[i] = … on a list of complex numbers (D281).
    fn other_index_assign(&mut self, b: usize, s: &A::Stmt, ctx: &mut Ctx) -> CResult<I::Stmt> {
        let A::StmtKind::IndexAssign { target, index, value, op, .. } = &s.kind else { unreachable!() };
        let name_ast = crate::ast_ext::mk(A::ExprKind::Name { name: target.clone() }, s.span);
        let tgt = self.expr(&name_ast, ctx)?;
        let idx = self.index_expr(index, &tgt, ctx)?;
        let v = if op != "=" {
            let cur = crate::ast_ext::mk(A::ExprKind::Index { target: Box::new(name_ast.clone()),
                                                        index: Some(Box::new(index.clone())) }, s.span);
            let bop: String = op.chars().next().unwrap().to_string();
            let val_ast = crate::ast_ext::mk(A::ExprKind::BinOp { op: bop, left: Box::new(cur),
                                                            right: Box::new(value.clone()), implicit: false },
                                       s.span);
            self.expr(&val_ast, ctx)?
        } else {
            self.expr(value, ctx)?
        };
        let lty = self.module.syms[b].ty.clone();
        let ok = match (&lty, &v.ty) {
            (Ty::VList(el), _) => match (&**el, &v.ty) {
                (Ty::Vec { n, .. }, Ty::Vec { n: m, dims: None, .. }) => n == m,
                (Ty::Mat { r, c, .. }, Ty::Mat { r: r2, c: c2, .. }) => (r, c) == (r2, c2),
                _ => false,
            },
            (Ty::ComplexList(_), Ty::Complex(_) | Ty::Num(_)) => true,
            (Ty::TextList, Ty::Str) => {
                let I::ExprKind::Var(tsym) = tgt.kind else { unreachable!() };
                return Ok(self.stmt_at(I::StmtKind::IndexAssign(tsym, idx, v), s));
            }
            _ => false,
        };
        if !ok {
            return Err(self.err(format!("{target} is {}; can't put {} in it", self.type_desc(&lty),
                                        self.type_desc(&v.ty)), value.span, None));
        }
        let (bd, vd) = (ty_dim(&lty).unwrap(), ty_dim(&v.ty).unwrap());
        self.unify_or(&bd, &vd, |c| format!("{target} is a list of {}; can't put {} in it", c.desc(&bd), c.desc(&vd)),
                      value.span, None)?;
        let I::ExprKind::Var(tsym) = tgt.kind else { unreachable!() };
        Ok(self.stmt_at(I::StmtKind::IndexAssign(tsym, idx, v), s))
    }

    pub fn s_funcdef(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::FuncDef { name, params, body, where_ } = &s.kind else { unreachable!() };
        if !ctx.is_main && ctx.lam.is_none() {
            return self.local_funcdef(s, ctx);
        }
        if !ctx.is_main || ctx.lam.is_some() {
            return Err(self.err("functions must be defined at the top level of the program (not inside a block)",
                                s.span, None));
        }
        let info = FuncInfo {
            name: name.clone(),
            fdef: Some(s.clone()),
            scope: ctx.scope,
            instances: Default::default(),
            display_name: name.clone(),
            checked_generic: false,
            stable: false,
            nat: self.func_nat(),
            module: None,
            anon_label: None,
            parent: None,
            eval_body: None,
            versions: vec![],
        };
        self.funcs.push(info);
        let id = self.funcs.len() - 1;
        if let Some(Binding::Func(prev)) = self.scopes[ctx.scope].names.get(name.as_str()).cloned() {
            self.add_version(prev, id); // multiple dispatch (C5, D285)
        }
        self.bind(ctx.scope, name, Binding::Func(id));
        // every where-binding in the body, at any depth: f(x) = 2 x where x = 5 s ignores the argument (A31)
        let pnames: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let mut binds: Vec<(String, A::Expr)> = where_.clone();
        crate::walk::collect_where_bindings(body, &mut binds);
        for (bname, val) in &binds {
            if pnames.contains(bname.as_str()) {
                let line = if val.span.line != 0 { val.span.line } else { s.span.line };
                self.warn(format!("'where {bname} = ...' hides the parameter {bname} of {name}, so the argument \
                                   is ignored"), A::Span { line, col: val.span.col, length: 1 },
                          Some(format!("rename the where-variable, or drop {bname} from {name}(...)")));
            }
        }
        Ok(vec![])
    }

    /// g(s) = … inside a function (D194): a helper that sees the enclosing function's names.
    pub fn local_funcdef(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::FuncDef { name, params, body, where_ } = &s.kind else { unreachable!() };
        let owner = match ctx.func {
            Owner::Func(f) => {
                let n = &self.module.funcs[f].name;
                match n.rsplit_once('.') {
                    Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() => head.into(),
                    _ => n.clone(),
                }
            }
            Owner::Main => "this function".to_string(),
        };
        if matches!(body, A::FuncBody::Block(_)) {
            return Err(self.err(format!("a function defined inside another function must fit on one line, like  \
                                         {name}(x) = …  (define a longer one at the top level)"), s.span, None));
        }
        if !where_.is_empty() {
            return Err(self.err(format!("'where' isn't supported on a function defined inside another function; \
                                         define the helper value first, then {name}(…) = …"), s.span, None));
        }
        if params.iter().any(|p| p.unit.is_some() || p.kind.is_some()) {
            return Err(self.err("a function defined inside another function takes its parameters' units from each \
                                 call; leave out the [unit]", s.span, None));
        }
        if self.scopes[ctx.scope].names.contains_key(name.as_str()) {
            return Err(self.err(format!("{name} is already defined here"), s.span, None));
        }
        self.local_funcs.push(LocalFunc { fdef: s.clone(), scope: ctx.scope, owner, expanding: false });
        let id = self.local_funcs.len() - 1;
        self.bind(ctx.scope, name, Binding::Local(id));
        Ok(vec![])
    }

    // ---- definite assignment: a variable first set inside an if/loop may have no value afterwards
    pub fn enter_region(&mut self, ctx: &mut Ctx, kind: &'static str, line: u32) -> RegionId {
        let parent = ctx.regions.last().copied();
        self.regions.push(Region { kind, line, new: vec![], assigned: vec![HashSet::new()], parent });
        let id = self.regions.len() - 1;
        ctx.regions.push(id);
        id
    }

    pub fn exit_region(&mut self, ctx: &mut Ctx, reg: RegionId) {
        ctx.regions.pop();
        let r = self.regions[reg].clone();
        let both: Option<HashSet<I::SymId>> = if r.kind == "if" && r.assigned.len() == 2 {
            Some(r.assigned[0].intersection(&r.assigned[1]).copied().collect()) // set on both sides of if/else
        } else {
            None
        };
        for &sym in &r.new {
            self.extra[sym].region = r.parent;
            if both.as_ref().is_some_and(|b| b.contains(&sym)) {
                continue;
            }
            if self.extra[sym].unset_msg.is_none() {
                let what = match r.kind {
                    "if" => "the if",
                    "while" => "the while loop",
                    _ => "the for loop",
                };
                self.extra[sym].unset_msg = Some(format!("{} might not have a value here: it is only set inside {what} \
                                                          on line {}", self.module.syms[sym].name, r.line));
            }
            if let Some(p) = r.parent {
                self.regions[p].new.push(sym);
            }
        }
        if let Some(p) = r.parent {
            if let Some(b) = both {
                self.regions[p].assigned.last_mut().unwrap().extend(b);
            }
        }
    }

    pub fn note_assign(&mut self, ctx: &Ctx, sym: I::SymId, new: bool) {
        let cur = ctx.regions.last().copied();
        if new {
            self.extra[sym].region = cur;
            if let Some(c) = cur {
                self.regions[c].new.push(sym);
            }
        } else {
            // assigning at the level where the variable lives (or outside all regions) gives it a value
            let home = self.extra[sym].region;
            if cur.is_none() || cur == home {
                self.extra[sym].unset_msg = None;
            }
        }
        if let Some(c) = cur {
            self.regions[c].assigned.last_mut().unwrap().insert(sym);
        }
    }

    pub fn s_if(&mut self, s: &A::Stmt, cond: &A::Expr, then: &[A::Stmt], other: Option<&[A::Stmt]>,
                ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let c = self.cond(cond, ctx)?;
        ctx.branch += 1;
        let reg = self.enter_region(ctx, "if", s.span.line);
        let res = (|| {
            let t = self.block(then, ctx)?;
            self.regions[reg].assigned.push(HashSet::new());
            let o = match other {
                Some(o) if !o.is_empty() => self.block(o, ctx)?,
                _ => {
                    self.regions[reg].assigned.pop();
                    vec![]
                }
            };
            Ok((t, o))
        })();
        ctx.branch -= 1;
        self.exit_region(ctx, reg);
        let (t, o) = res?;
        Ok(vec![self.stmt_at(I::StmtKind::If(c, t, o), s)])
    }

    pub fn cond(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let c = self.expr(e, ctx)?;
        if !matches!(c.ty, Ty::Bool) {
            return Err(self.err(format!("a condition must be true or false, but this is {}", self.type_desc(&c.ty)),
                                e.span, Some("compare values, e.g.  if x > 0 m".into())));
        }
        Ok(c)
    }

    pub fn s_while(&mut self, s: &A::Stmt, cond: &A::Expr, body: &[A::Stmt], ctx: &mut Ctx)
                   -> CResult<Vec<I::Stmt>> {
        let c = self.cond(cond, ctx)?;
        ctx.loop_depth += 1;
        let reg = self.enter_region(ctx, "while", s.span.line);
        let b = self.block(body, ctx);
        self.exit_region(ctx, reg);
        ctx.loop_depth -= 1;
        Ok(vec![self.stmt_at(I::StmtKind::While(c, b?), s)])
    }

    pub fn loop_var(&mut self, name: &str, ty: Ty, ctx: &mut Ctx) -> I::SymId {
        let mut found = match self.lookup(ctx.scope, name) {
            Some((Binding::Sym(b), _)) => Some(b),
            _ => None,
        };
        if let Some(b) = found {
            if self.par_here(ctx).is_some() && !self.extra[b].par_private {
                found = None; // inside a parallel for, an inner loop gets its own variable (D152)
            }
        }
        if let Some(b) = found {
            let bty = self.module.syms[b].ty.clone();
            if self.module.syms[b].func == self.owner_func(ctx) && same_kind(&bty, &ty) {
                let ok = match (&bty, &ty) {
                    (Ty::Num(a), Ty::Num(d)) => self.u.unify(a, d),
                    (Ty::Str, _) => true,
                    _ => false,
                };
                if ok {
                    if self.extra[b].unset_msg.is_some() {
                        // the variable of an earlier loop
                        self.extra[b].unset_msg = None;
                        self.extra[b].fresh_loop_var = true;
                    }
                    return b;
                }
            }
        }
        let sym = self.new_sym(name, ty, ctx);
        self.bind(ctx.scope, name, Binding::Sym(sym));
        self.extra[sym].fresh_loop_var = true;
        sym
    }

    /// A loop variable that didn't exist before the loop has no value if the loop never ran.
    pub fn after_loop(&mut self, sym: I::SymId, line: u32) {
        if self.extra[sym].fresh_loop_var {
            let x = &mut self.extra[sym];
            x.fresh_loop_var = false;
            x.region = None;
            x.unset_msg = Some(format!("{} might not have a value here: it is only set inside the for loop on line \
                                        {line}, which may not run at all", self.module.syms[sym].name));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn s_for(&mut self, s: &A::Stmt, var: &str, lo_a: &A::Expr, hi_a: &A::Expr, step: Option<&A::Expr>,
                 body: &[A::Stmt], parallel: bool, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let lo = self.expr(lo_a, ctx)?;
        let hi = self.expr(hi_a, ctx)?;
        self.need_num(&lo, lo_a, "the start of the range")?;
        self.need_num(&hi, hi_a, "the end of the range")?;
        let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
        self.unify_or(&ld, &hd, |c| format!("the range goes from {} to {}; both ends need the same units",
                                            c.desc(&ld), c.desc(&hd)), s.span, None)?;
        let st = match step {
            Some(sa) => {
                let st = self.expr(sa, ctx)?;
                self.need_num(&st, sa, "the step")?;
                let sd = ty_dim(&st.ty).unwrap();
                self.unify_or(&ld, &sd, |c| format!("the step is {} but the range is {}", c.desc(&sd), c.desc(&ld)),
                              sa.span, None)?;
                Some(st)
            }
            None => {
                if !self.u.unify(&ld, &DExpr::of(DIMLESS)) {
                    return Err(self.err(format!("this range is {}, so it needs a step with units", self.desc(&ld)),
                                        s.span, Some("add e.g.  step 0.1 s".into())));
                }
                None
            }
        };
        if parallel {
            return self.parallel_for(s, var, lo, hi, st, body, ctx);
        }
        let sym = self.loop_var(var, Ty::Num(ld), ctx);
        {
            let ms = &mut self.module.syms[sym];
            ms.hint = lo.hint.clone();
            ms.sf = None;
            ms.direct = 1; // grid values print as written (0.07 s, not 0.0700 s; D11)
        }
        self.extra[sym].assigned = true;
        ctx.loop_depth += 1;
        let reg = self.enter_region(ctx, "for", s.span.line);
        let b = self.block(body, ctx);
        self.exit_region(ctx, reg);
        ctx.loop_depth -= 1;
        let b = b?;
        self.after_loop(sym, s.span.line);
        Ok(vec![self.stmt_at(I::StmtKind::For { sym, lo, hi, step: st, body: b, parallel: false, par: None }, s)])
    }

    pub fn s_for_in(&mut self, s: &A::Stmt, var: &str, iterable: &A::Expr, body: &[A::Stmt], ctx: &mut Ctx)
                    -> CResult<Vec<I::Stmt>> {
        let lst = match self.expr_any(iterable, ctx)? {
            Checked::Sol(view) => self.sol_values(view, iterable)?,
            Checked::Val(v) => v,
            f @ Checked::Func { .. } => return Err(self.func_as_value_error(&f, iterable)),
        };
        let line = s.span.line;
        if matches!(lst.ty, Ty::TextList) {
            let sym = self.loop_var(var, Ty::Str, ctx);
            self.extra[sym].assigned = true;
            ctx.loop_depth += 1;
            let b = self.block(body, ctx);
            ctx.loop_depth -= 1;
            let b = b?;
            self.after_loop(sym, line);
            return Ok(vec![self.stmt_at(I::StmtKind::ForIn(sym, lst, b), s)]);
        }
        let sym = match &lst.ty {
            Ty::VList(el) => {
                // for r in positions: each r a vector (or matrix) (D281)
                let sym = self.loop_var(var, (**el).clone(), ctx);
                let ms = &mut self.module.syms[sym];
                ms.hint = lst.hint.clone();
                ms.sf = lst.sf;
                ms.direct = 0;
                sym
            }
            Ty::ComplexList(d) => {
                // for z in fft(xs): each z a complex number (D243)
                let sym = self.loop_var(var, Ty::Complex(d.clone()), ctx);
                let ms = &mut self.module.syms[sym];
                ms.hint = lst.hint.clone();
                ms.sf = None;
                ms.direct = 0;
                sym
            }
            Ty::List(d) => {
                let sym = self.loop_var(var, Ty::Num(d.clone()), ctx);
                self.module.syms[sym].hint = lst.hint.clone();
                // for E in [0.50 eV, 0.75 eV] keeps the elements' precision when they all share it (friction #30)
                let src = match &lst.kind {
                    I::ExprKind::Bin(I::BinOp::Mul, a, _) if matches!(a.kind, I::ExprKind::List(_)) => a.as_ref(),
                    _ => &lst, // [0.50, 0.75] eV (D192): the written list inside the unit
                };
                let items: Option<&Vec<I::Expr>> = match &src.kind {
                    I::ExprKind::List(items) => Some(items),
                    _ => None,
                };
                let sfs: HashSet<Option<u32>> = match items {
                    Some(it) => it.iter().map(|x| x.sf).collect(),
                    None => [None].into_iter().collect(),
                };
                let sf = if sfs.len() == 1 { *sfs.iter().next().unwrap() } else { None };
                // the elements of a written-out list print as written ([0, 1, 1.5]: 1.5, not 1.50; D11) ...
                let all_direct = items.is_some_and(|it| !it.is_empty() && it.iter().all(|x| x.direct != 0));
                let mut direct = u8::from(sf.is_none() && all_direct);
                // ... or as the list prints them, when their precisions differ (D242)
                let written = if direct != 0 { written_code(items.unwrap()) } else { 0 };
                let known: Vec<u32> = items.map(|it| it.iter().filter_map(|x| x.sf).collect()).unwrap_or_default();
                if written != 0 && !known.is_empty() {
                    self.extra[sym].list_sf = known.iter().min().copied();
                    direct = if written == 3 { 5 } else { 4 };
                }
                let ms = &mut self.module.syms[sym];
                ms.sf = sf;
                ms.direct = direct;
                sym
            }
            _ => {
                return Err(self.err(format!("can't loop over {}; 'for x in ...' needs a list", self.type_desc(&lst.ty)),
                                    iterable.span, Some("to count, write  for i from 1 to 10".into())));
            }
        };
        self.extra[sym].assigned = true;
        ctx.loop_depth += 1;
        let reg = self.enter_region(ctx, "for", line);
        let b = self.block(body, ctx);
        self.exit_region(ctx, reg);
        ctx.loop_depth -= 1;
        let b = b?;
        self.after_loop(sym, line);
        Ok(vec![self.stmt_at(I::StmtKind::ForIn(sym, lst, b), s)])
    }

    pub fn s_return(&mut self, s: &A::Stmt, value: Option<&A::Expr>, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if ctx.is_main {
            return Err(self.err("return can only be used inside a function", s.span, None));
        }
        let Some(value) = value else {
            return Err(self.err("return needs a value", s.span, None));
        };
        let v = match self.expr_any(value, ctx)? {
            Checked::Sol(view) => {
                // Q17 (D48)
                let (n, tn) = (self.sols[view].name.clone(), self.sols[view].tname.clone());
                return Err(self.err(format!("a function can't return the ODE solution {n} yet; return a number made \
                                             from it instead, like {n}(…), ∫ {n}({tn}) d{tn} or a root found with \
                                             solve"), value.span,
                                    Some("or solve the ODE at the top level, where its solution stays available".into())));
            }
            Checked::Func { .. } => self.expr(value, ctx)?, // the usual message for a function used as a value
            Checked::Val(v) => v,
        };
        self.ret_types[ctx.ret_types].push(v.clone());
        Ok(vec![self.stmt_at(I::StmtKind::Return(Some(v)), s)])
    }

    pub fn s_break(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if ctx.loop_depth == 0 {
            return Err(self.err("break can only be used inside a loop", s.span, None));
        }
        Ok(vec![self.stmt_at(I::StmtKind::Break, s)])
    }

    pub fn s_continue(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if ctx.loop_depth == 0 {
            return Err(self.err("continue can only be used inside a loop", s.span, None));
        }
        Ok(vec![self.stmt_at(I::StmtKind::Continue, s)])
    }

    pub fn s_assert(&mut self, s: &A::Stmt, cond: &A::Expr, message: Option<&str>, ctx: &mut Ctx)
                    -> CResult<Vec<I::Stmt>> {
        let c = self.cond(cond, ctx)?;
        let msg = match message {
            Some(m) => m.to_string(),
            None => format!("check failed: {}", crate::source::to_source(cond)),
        };
        let t = self.text(&msg);
        Ok(vec![self.stmt_at(I::StmtKind::Assert(c, t), s)])
    }

    pub fn need_num(&self, v: &I::Expr, node: &A::Expr, what: &str) -> CResult<()> {
        if !matches!(v.ty, Ty::Num(_)) {
            return Err(self.err(format!("{what} must be a number, but it is {}", self.type_desc(&v.ty)), node.span,
                                None));
        }
        Ok(())
    }

    pub fn need_numlike(&self, v: &I::Expr, node: &A::Expr, what: &str, allow_vec: bool) -> CResult<()> {
        // (a complex number is a 2-vector here, as ComplexTy is a VecTy in Python)
        if allow_vec && matches!(v.ty, Ty::Vec { .. } | Ty::Mat { .. } | Ty::Complex(_)) {
            return Ok(());
        }
        if !matches!(v.ty, Ty::Num(_) | Ty::List(_)) {
            return Err(self.err(format!("{what} must be a number, but it is {}", self.type_desc(&v.ty)), node.span,
                                None));
        }
        Ok(())
    }

    /// need_numlike for a function or solution used where a number is needed.
    pub fn need_numlike_checked(&self, v: &Checked, node: &A::Expr, what: &str) -> CResult<()> {
        match v {
            Checked::Val(x) => self.need_numlike(x, node, what, false),
            Checked::Func { name, .. } => Err(self.err(format!("{what} must be a number, but it is a function"),
                                                       node.span, Some(format!("call it with an argument, e.g. {name}(x)")))),
            Checked::Sol(view) => {
                let sv = &self.sols[*view];
                Err(self.err(format!("{what} must be a number, but it is an ODE solution"), node.span,
                             Some(format!("use {}({}) for its value at a time", sv.name, sv.tname))))
            }
        }
    }
}

/// `direct` of a list, vector or matrix written out in the program (D11, Python _written): 0 if not every
/// element is a literal; 3 if some elements are exact whole numbers and others have a stated precision; else 1.
pub fn written_code(items: &[I::Expr]) -> u8 {
    if items.is_empty() || !items.iter().all(|x| x.direct != 0) {
        return 0;
    }
    let none = items.iter().any(|x| x.sf.is_none());
    let some = items.iter().any(|x| x.sf.is_some());
    if none && some {
        3
    } else {
        1
    }
}
