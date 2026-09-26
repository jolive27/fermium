//! Uncertainties (D120–D124): a port of Checker.e_Uncertain, unc_part, _maybe_uncertain, s_Propagate and the
//! err(x) branch of e_Call from `fermium/checker.py`. An uncertain value has an ordinary number type: the
//! uncertainty exists only at run time.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;

use crate::arith::is_affine;
use crate::checker::*;
use crate::stmts::ty_dim;

fn kind_of(t: &Ty) -> &'static str {
    match t {
        Ty::List(_) => "a list",
        Ty::Complex(_) => "a complex number",
        Ty::Vec { .. } => "a vector",
        Ty::Mat { .. } => "a matrix",
        _ => "not a number",
    }
}

impl Checker {
    /// a ± b: a new independent error source (D120); `x ± 3%` is relative.
    pub fn e_uncertain(&mut self, e: &A::Expr, value: &A::Expr, err: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        self.uses_unc = true;
        let v = self.expr(value, ctx)?;
        self.need_numlike(&v, value, "the value before ±", false)?;
        let rel = matches!(&err.kind, A::ExprKind::Quantity { unit, .. } if unit.text.trim() == "%")
            && !v.hint.as_ref().is_some_and(|h| h.name == "%");
        let mut s = self.expr(err, ctx)?;
        self.need_numlike(&s, err, "the uncertainty after ±", false)?;
        if matches!(s.ty, Ty::List(_)) && !matches!(v.ty, Ty::List(_)) {
            return Err(self.err("a single value needs a single uncertainty after ±, not a list", err.span, None));
        }
        let (vd, sd) = (ty_dim(&v.ty).unwrap(), ty_dim(&s.ty).unwrap());
        let line = e.span.line;
        if rel {
            if !self.u.unify(&sd, &DExpr::of(DIMLESS)) {
                return Err(self.err("a relative uncertainty is a plain number, like  x ± 3%", err.span, None));
            }
        } else {
            if is_affine(&s.hint) {
                // 20.0 ± 0.5 °C: the uncertainty is a temperature difference, 0.5 K, not 273.65 K
                let off = s.hint.as_ref().unwrap().offset;
                let ty = s.ty.clone();
                s = ir(I::ExprKind::Bin(I::BinOp::Sub, Box::new(s), Box::new(ir(I::ExprKind::Const(off),
                                                                                 Ty::Num(sd.clone()), line))),
                       ty, line);
            }
            if !self.u.unify(&vd, &sd) {
                return Err(self.err(format!("the uncertainty after ± is {} but the value is {}; both need the same \
                                             units", self.desc(&sd), self.desc(&vd)), err.span,
                                    Some("write the unit once at the end, like  L = 1.20 ± 0.01 m".into())));
            }
        }
        let ty = if matches!(v.ty, Ty::List(_)) { Ty::List(vd.clone()) } else { Ty::Num(vd.clone()) };
        let v_tdelta = v.get_extra().is_some_and(|x| x.tdelta);
        let vh = v.hint.clone();
        let sh = s.hint.clone();
        let mut args = vec![v, s];
        if rel && is_affine(&vh) && !v_tdelta {
            // 20.0 °C ± 3%: 3 % of the reading as written (0.60 K), not of 293.15 K (red team round 3 #3, D182)
            args.push(ir(I::ExprKind::Const(vh.as_ref().unwrap().offset), Ty::Num(vd), line));
        }
        let mut r = ir(I::ExprKind::Builtin(if rel { "pm_rel" } else { "pm" }.into(), args), ty, line);
        let h = if vh.is_some() { vh.clone() } else if rel { None } else { sh };
        r.hint = match &h {
            Some(x) if x.offset != 0.0 && vh.as_ref() != Some(x) => None,
            _ => h,
        };
        r.sf = None;
        Ok(r)
    }

    /// value(x), uncertainty(x), rel(x) (D121): plain numbers (lists for a list).
    pub fn unc_part(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Call { args, .. } = &e.kind else { unreachable!() };
        if args.len() != 1 {
            return Err(self.err(format!("{name} takes 1 argument but was given {}", args.len()), e.span, None));
        }
        self.uses_unc = true;
        let a = self.expr(&args[0], ctx)?;
        self.need_numlike(&a, &args[0], &format!("the argument of {name}"), false)?;
        let line = e.span.line;
        if name == "rel" {
            let ty = if matches!(a.ty, Ty::List(_)) { Ty::List(DExpr::of(DIMLESS)) } else { Ty::Num(DExpr::of(DIMLESS)) };
            let mut r = ir(I::ExprKind::Builtin("unc_rel".into(), vec![a]), ty, line);
            r.sf = Some(2);
            return Ok(r);
        }
        let (ty, hint, sf) = (a.ty.clone(), a.hint.clone(), a.sf);
        let mut r = ir(I::ExprKind::Builtin(format!("unc_{name}"), vec![a]), ty, line);
        r.hint = if hint.is_none() || !is_affine(&hint) || name == "value" { hint } else { None };
        r.sf = if name == "uncertainty" { Some(2) } else { sf };
        Ok(r)
    }

    /// err(x): the standard error of a fitted parameter, or of any uncertain value in a program that uses them.
    pub fn err_call(&mut self, e: &A::Expr, args: &[A::Expr], ctx: &mut Ctx) -> CResult<Checked> {
        let a0 = if args.len() == 1 { Some(&args[0]) } else { None };
        if let Some(r) = self.fit_err(e, a0, ctx) {
            return r.map(Checked::Val);
        }
        if a0.is_some() && self.uses_unc {
            return self.builtin("uncertainty", e, ctx); // err(x) of any uncertain value (D121)
        }
        Err(self.err("err(x) gives the standard error of a parameter found by fit, like err(g) after fit T = 2π √(L/g) \
                      to data", e.span, None))
    }

    /// propagate montecarlo [N samples] + formulas (D123).
    pub fn s_propagate(&mut self, s: &A::Stmt, samples: Option<&A::Expr>, body: &[A::Stmt], ctx: &mut Ctx)
                       -> CResult<Vec<I::Stmt>> {
        self.uses_unc = true;
        let mut n = None;
        if let Some(sa) = samples {
            let v = self.expr(sa, ctx)?;
            self.need_num(&v, sa, "the number of samples")?;
            let d = ty_dim(&v.ty).unwrap();
            if !self.u.unify(&d, &DExpr::of(DIMLESS)) {
                return Err(self.err("the number of samples must be a plain number", sa.span, None));
            }
            n = Some(v);
        }
        fn check_bad(c: &Checker, stmts: &[A::Stmt]) -> CResult<()> {
            use A::StmtKind as K;
            for x in stmts {
                if matches!(x.kind, K::Print { .. } | K::Plot { .. } | K::Fit { .. } | K::FuncDef { .. }
                            | K::Propagate { .. } | K::Import { .. } | K::Units { .. } | K::Return { .. })
                {
                    return Err(c.err("only formulas (name = …) go inside  propagate montecarlo; print or plot the \
                                      results after the block", x.span, None));
                }
                for b in crate::walk::stmt_blocks(x) {
                    check_bad(c, b)?;
                }
            }
            Ok(())
        }
        check_bad(self, body)?;
        let b = self.block(body, ctx)?;
        let mut outs: Vec<I::SymId> = vec![];
        for st in &b {
            if let I::StmtKind::Assign(sym, _) = &st.kind {
                if matches!(self.module.syms[*sym].ty, Ty::Num(_)) && !outs.contains(sym) {
                    outs.push(*sym);
                    let ms = &mut self.module.syms[*sym];
                    ms.sf = None;
                    ms.direct = 0;
                }
            }
        }
        if outs.is_empty() {
            let other = b.iter().find_map(|st| match &st.kind {
                I::StmtKind::Assign(sym, _) => Some(*sym),
                _ => None,
            });
            if let Some(o) = other {
                // b = [a, 2 a]: a formula, but not for a number (red team round 3 #15)
                let name = self.module.syms[o].name.clone();
                return Err(self.err(format!("propagate montecarlo gives uncertainties to plain numbers, but {name} is \
                                             {}; give each number its own formula, like {name}1 = …, {name}2 = …",
                                            kind_of(&self.module.syms[o].ty)), s.span, None));
            }
            return Err(self.err("propagate montecarlo needs at least one formula (name = …) in its block", s.span, None));
        }
        Ok(vec![I::Stmt { kind: I::StmtKind::Propagate { n, body: b, outs }, line: s.span.line }])
    }
}
