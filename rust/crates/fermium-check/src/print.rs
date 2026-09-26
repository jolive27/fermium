//! Printing: a port of Checker.s_Print, print_items, fmt, fmt_components and describe_function.
use fermium_ir as I;
use fermium_ir::types::Ty;
use fermium_syntax::ast as A;

use crate::checker::*;

impl Checker {
    pub fn s_print(&mut self, items: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Stmt> {
        let mut vals = vec![];
        for it in items {
            vals.push(self.expr_any(it, ctx)?);
        }
        self.print_items(vals, items, ctx)
    }

    pub fn print_items(&mut self, vals: Vec<Checked>, asts: &[A::Expr], _ctx: &mut Ctx) -> CResult<I::Stmt> {
        let mut items = vec![];
        let line = asts.first().map(|a| a.span.line).unwrap_or(0);
        for (v, a) in vals.into_iter().zip(asts) {
            let item = match v {
                Checked::Func { info, .. } => {
                    let d = self.describe_function(info);
                    I::PrintItem::Text(self.text(&d))
                }
                Checked::Sol(view) => {
                    let sv = &self.sols[view];
                    let t = format!("{}({}): solution of an ODE (use {}({}) for a value, or plot {} vs {})", sv.name,
                                    sv.tname, sv.name, sv.tname, sv.name, sv.tname);
                    I::PrintItem::Text(self.text(&t))
                }
                Checked::Val(v) => match &v.ty {
                    Ty::Num(_) => {
                        let f = self.fmt(&v);
                        I::PrintItem::Num(v, f)
                    }
                    Ty::List(_) => {
                        let f = self.fmt(&v);
                        I::PrintItem::List(v, f)
                    }
                    Ty::Complex(_) => {
                        let f = self.fmt(&v);
                        I::PrintItem::Complex(v, f)
                    }
                    Ty::Vec { dims: Some(_), .. } => {
                        let fs = self.fmt_components(&v);
                        I::PrintItem::MixedVec(v, fs)
                    }
                    Ty::Vec { .. } => {
                        let f = self.fmt(&v);
                        I::PrintItem::Vec(v, f)
                    }
                    Ty::Mat { .. } => {
                        let f = self.fmt(&v);
                        I::PrintItem::Mat(v, f)
                    }
                    Ty::TextList => I::PrintItem::TextList(v),
                    Ty::ComplexList(_) => {
                        let f = self.fmt(&v);
                        I::PrintItem::ComplexList(v, f)
                    }
                    Ty::Bool => I::PrintItem::Bool(v),
                    Ty::Str => match &v.kind {
                        I::ExprKind::Str(s) => {
                            let s = s.clone();
                            I::PrintItem::Text(self.text(&s))
                        }
                        _ => I::PrintItem::TextVar(v),
                    },
                    Ty::Data(_) => {
                        let t = self.data_description(&v);
                        let t = self.text(&t);
                        I::PrintItem::Data(v, t)
                    }
                    _ => return Err(self.err("can't print this", a.span, None)),
                },
            };
            items.push(item);
        }
        Ok(I::Stmt { kind: I::StmtKind::Print(items), line })
    }

    /// A print format for a value (Python fmt): its dimension (resolved when checking ends), display unit,
    /// significant figures and how it was written.
    pub fn fmt(&mut self, v: &I::Expr) -> usize {
        let dim = crate::stmts::ty_dim(&v.ty).unwrap_or_else(fermium_ir::types::DExpr::dimless);
        let echo = !v.get_extra().is_some_and(|x| x.no_echo) && !self.natural();
        let nat = if self.nat.is_empty() { None } else { Some(self.nat.clone()) };
        let (sf, direct) = if matches!(v.direct, 4 | 5) && matches!(v.ty, Ty::Num(_)) && v.sf.is_none() {
            // a loop variable over a written list (D242)
            let ls = v.get_extra().and_then(|x| x.list_sf);
            (ls, if ls.is_some() { v.direct } else { 1 })
        } else {
            (v.sf, v.direct)
        };
        self.module.tables.fmts.push(I::Fmt { dim: fermium_ir::DIMLESS, hint: v.hint.clone(), sf, direct, echo, nat });
        self.fmt_dims.push(dim);
        self.module.tables.fmts.len() - 1
    }

    /// One print format per component of a mixed vector (consecutive ids).
    pub fn fmt_components(&mut self, v: &I::Expr) -> Vec<usize> {
        let Ty::Vec { dims: Some(dims), .. } = &v.ty else { unreachable!() };
        let hints = self.mixed_hints(v, dims.len());
        let nat = if self.nat.is_empty() { None } else { Some(self.nat.clone()) };
        let mut out = vec![];
        for (d, h) in dims.clone().into_iter().zip(hints) {
            self.module.tables.fmts.push(I::Fmt { dim: fermium_ir::DIMLESS, hint: h, sf: v.sf, direct: v.direct,
                                                  echo: true, nat: nat.clone() });
            self.fmt_dims.push(d);
            out.push(self.module.tables.fmts.len() - 1);
        }
        out
    }

    /// Resolve the dimensions of every print format once checking is done (unknowns default to plain numbers).
    pub fn resolve_fmts(&mut self) {
        for (f, d) in self.module.tables.fmts.iter_mut().zip(&self.fmt_dims) {
            f.dim = self.u.resolve(d);
        }
    }

    pub fn describe_function(&mut self, info: FuncInfoId) -> String {
        let f = &self.funcs[info];
        let Some(A::Stmt { kind: A::StmtKind::FuncDef { params, body, .. }, .. }) = &f.fdef else {
            return format!("{}: a function", f.display_name);
        };
        let ps = params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        match body {
            A::FuncBody::Expr(b) => {
                let src = crate::source::to_source(&self.body_expr(info).unwrap_or_else(|| b.clone()));
                let f = &self.funcs[info];
                let mut s = match &f.anon_label {
                    Some(l) => format!("{l} = {src}"),
                    None => format!("{}({ps}) = {src}", f.display_name),
                };
                let units = self.function_units(info);
                if !units.is_empty() {
                    s += &format!("   [{units}]");
                }
                s
            }
            A::FuncBody::Block(_) => format!("{}({ps}): a function defined over several lines", f.display_name),
        }
    }

    /// The body of a one-line function, with its where-bindings (Python FuncInfo.body_expr).
    pub fn body_expr(&self, info: FuncInfoId) -> Option<A::Expr> {
        let Some(A::Stmt { kind: A::StmtKind::FuncDef { body: A::FuncBody::Expr(b), where_, .. }, span }) =
            &self.funcs[info].fdef
        else {
            return None;
        };
        if where_.is_empty() {
            return Some(b.clone());
        }
        Some(crate::ast_ext::mk(A::ExprKind::Where { value: Box::new(b.clone()), bindings: where_.clone() }, *span))
    }
}
