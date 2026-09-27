//! Seeded random numbers (D80): a port of Checker.seed_stmt, m3_random and m3_sample from `fermium/checker.py`.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;

use crate::checker::*;
use crate::stmts::ty_dim;

impl Checker {
    /// `seed(42)` on its own line.
    pub fn seed_stmt(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::ExprKind::Call { args, .. } = &e.kind else { unreachable!() };
        if args.len() != 1 {
            return Err(self.err("seed takes one whole number: seed(42)", e.span, None));
        }
        let v = self.expr(&args[0], ctx)?;
        self.need_num(&v, &args[0], "the seed")?;
        let d = ty_dim(&v.ty).unwrap();
        self.unify_or(&d, &DExpr::of(DIMLESS), |c| format!("the seed must be a plain number, not {}", c.desc(&d)),
                      args[0].span, None)?;
        let line = e.span.line;
        let call = ir(I::ExprKind::Builtin("seed".into(), vec![v]), dimless_num(), line);
        Ok(vec![I::Stmt { kind: I::StmtKind::Expr(call), line }])
    }

    /// rand(), rand(a, b) (uniform in [a, b), same units), randn(), randn(μ, σ) (normal, same units).
    pub fn m3_random(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let A::ExprKind::Call { args: eargs, .. } = &e.kind else { unreachable!() };
        let line = e.span.line;
        let n = args.len();
        if n == 0 {
            return Ok(ir(I::ExprKind::Builtin(name.into(), args), dimless_num(), line));
        }
        if n != 2 {
            let what = if name == "rand" { "rand() or rand(a, b)" } else { "randn() or randn(μ, σ)" };
            return Err(self.err(format!("{name} takes no arguments or two: {what}"), e.span, None));
        }
        for i in 0..2 {
            self.need_num(&args[i], &eargs[i], "this value")?;
        }
        let (da, db) = (ty_dim(&args[0].ty).unwrap(), ty_dim(&args[1].ty).unwrap());
        self.unify_or(&da, &db, |c| {
            if name == "rand" {
                format!("rand(a, b): a is {} but b is {}; they need the same units", c.desc(&da), c.desc(&db))
            } else {
                format!("randn(μ, σ): μ is {} but σ is {}; they need the same units", c.desc(&da), c.desc(&db))
            }
        }, e.span, None)?;
        let hint = args[0].hint.clone().or_else(|| args[1].hint.clone());
        let mut r = ir(I::ExprKind::Builtin(format!("{name}2"), args), Ty::Num(da), line);
        r.hint = hint;
        Ok(r)
    }

    /// sample(expr, N): a list of N values of expr, evaluated afresh each time (so each rand() in it draws new
    /// numbers) -- the building block of a Monte Carlo estimate (D80).
    pub fn m3_sample(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Call { args, .. } = &e.kind else { unreachable!() };
        if args.len() != 2 {
            return Err(self.err("sample takes an expression and a count: sample(randn(0 m, 1 m), 1000)", e.span, None));
        }
        let cnt = self.expr(&args[1], ctx)?;
        self.need_num(&cnt, &args[1], "the number of samples")?;
        let cd = ty_dim(&cnt.ty).unwrap();
        self.unify_or(&cd, &DExpr::of(DIMLESS), |_| "the number of samples must be a plain number".into(),
                      args[1].span, None)?;
        let lname = self.fresh_name("sample");
        self.module.lambdas.push(I::Lambda { kind: I::LambdaKind::Scalar, name: lname, params: vec![], captures: vec![],
                                             locals: vec![], body: vec![], state: vec![], col_syms: vec![],
                                             param_syms: vec![] });
        let lam = self.module.lambdas.len() - 1;
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut parents = ctx.lam_parents.clone();
        if let Some(l) = ctx.lam {
            parents.push(l);
        }
        let mut lctx = Ctx { func: ctx.func, scope, is_main: false, lam: Some(lam), loop_depth: 0, branch: 0,
                             ret_types: ctx.ret_types, regions: vec![], lam_parents: parents };
        let k = self.new_sym("__k", dimless_num(), &lctx);
        self.module.lambdas[lam].locals.retain(|s| *s != k);
        self.module.lambdas[lam].params.push(k);
        self.extra[k].assigned = true;
        let body = self.expr(&args[0], &mut lctx)?;
        self.need_num(&body, &args[0], "the thing to sample")?;
        let (hint, d) = (body.hint.clone(), ty_dim(&body.ty).unwrap());
        self.module.lambdas[lam].body = vec![body];
        let mut r = ir(I::ExprKind::Sample { lam, n: Box::new(cnt) }, Ty::List(d), e.span.line);
        r.hint = hint;
        Ok(r)
    }
}
