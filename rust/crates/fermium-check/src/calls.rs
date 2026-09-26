//! Calls: a port of Checker.e_Call, call_args, _func_param_uses, call_user, _takes_lists and instantiate from
//! `fermium/checker.py` (user functions: one instance per argument types, D43 functions as arguments, D191
//! element-by-element calls on lists). The built-ins themselves are checked in `builtin`.
use std::collections::HashMap;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::arith::minsf;
use crate::builtins::{is_builtin, LIST_FUNCS};
use crate::checker::*;
use crate::stmts::{delta_name, same_kind, ty_dim};

/// One-argument built-ins that can be passed to a function: simpson(sin, 0, π, 100) (D43).
const PASSABLE_BUILTINS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "exp", "ln", "log", "log10", "log2", "sinh", "cosh", "tanh", "asin",
    "acos", "atan", "sqrt", "cbrt", "abs",
];

/// Checker-side facts about an IR function instance (Python sets them as attributes on I.IFunc).
#[derive(Clone, Debug, Default)]
pub struct FuncExtra {
    pub display: String,
    pub ret_placeholder: Option<DExpr>,
    pub checking: bool,
    pub called_recursively: bool,
    pub ret_hint: Option<I::Hint>,
    pub def_line: u32,
}

impl Checker {
    pub fn e_call(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Call { func: f, args } = &e.kind else { unreachable!() };
        if let A::ExprKind::Field { target, .. } = &f.kind {
            if !f.paren {
                if let Some(pref) = self.py_ref_of(target, ctx) {
                    return self.python_call(pref, e, ctx); // np.sinc(x): a Python function (D140)
                }
            }
        }
        if let A::ExprKind::Deriv { operand, .. } = &f.kind {
            if let A::ExprKind::Name { name: n } = &operand.kind {
                if self.is_pde_name(n, ctx) {
                    return self.pde_call(n, e, ctx, true); // ∂u/∂x(x, t) of a PDE solution (D83)
                }
            }
        }
        if let A::ExprKind::Name { name: fname } = &f.kind {
            let b = self.lookup(ctx.scope, fname).map(|x| x.0);
            if b.is_none() && matches!(fname.as_str(), "sqrt" | "log" | "ln" | "log10" | "log2" | "factorial")
                && args.len() == 1
            {
                self.domain_check(fname, &args[0], e)?;
            }
            if b.is_none() && fname == "Σ" {
                return self.builtin("sum", e, ctx); // Σ(xs) is sum(xs)
            }
            if self.is_pde_name(fname, ctx) {
                return self.pde_call(fname, e, ctx, false); // u(x, t) of a PDE solution (D83)
            }
            if b.is_none() && (fname == "γ" || fname == "Γ") {
                return self.builtin("gamma", e, ctx); // `gamma(x)` is spelled γ after ASCII→Greek
            }
            if b.is_none() && fname == "err" {
                return self.err_call(e, args, ctx); // err(g): the standard error of a fitted parameter
            }
            if b.is_none() && matches!(fname.as_str(), "grad" | "div" | "curl" | "laplacian") && args.len() == 1
                && matches!(args[0].kind, A::ExprKind::Name { .. })
            {
                // ASCII for ∇f, ∇·F, ∇×F, ∇²f
                let kind = match fname.as_str() {
                    "grad" => "grad",
                    "div" => "div",
                    "curl" => "curl",
                    _ => "lap",
                };
                let vc = crate::ast_ext::mk(A::ExprKind::VecCalc { kind: kind.to_string(), func: Box::new(args[0].clone()) }, e.span);
                return self.expr_any(&vc, ctx);
            }
            let Some(b) = b else {
                if is_builtin(fname) {
                    return self.builtin(fname, e, ctx);
                }
                self.calling = true;
                let d = self.undefined(fname, f, ctx);
                self.calling = false;
                return Err(d);
            };
            match b {
                Binding::Func(info) => {
                    let a = self.call_args(args, ctx)?;
                    return self.call_user(info, a, e).map(Checked::Val);
                }
                Binding::Local(lf) => return self.call_local(lf, e, ctx).map(Checked::Val),
                Binding::Sol(view) => return self.sol_eval(view, e, ctx).map(Checked::Val),
                Binding::Sym(_) | Binding::Const(_) => {
                    if let Some(&later) = self.future_funcs.get(fname) {
                        if later > e.span.line && ctx.is_main {
                            return Err(self.err(format!("{fname} is defined as a function on line {later}, after this \
                                                         line"), e.span,
                                                Some(format!("move the definition of {fname}(...) above its first use"))));
                        }
                    }
                    if let Checked::Val(v) = self.use_binding(b, fname, f, ctx)? {
                        if matches!(v.ty, Ty::Num(_)) && args.len() == 1 {
                            // k(x + 1) means k × (x + 1)
                            let arg = self.expr(&args[0], ctx)?;
                            self.need_numlike(&arg, &args[0], "this value", false)?;
                            return self.arith("*", v, arg, e).map(Checked::Val);
                        }
                    }
                    return Err(self.err(format!("{fname} isn't a function, so it can't be called with ( )"), f.span,
                                        None));
                }
                _ => {}
            }
        }
        if matches!(f.kind, A::ExprKind::Prime { .. }) {
            return match self.expr_any(f, ctx)? {
                Checked::Func { info, .. } => {
                    let a = self.call_args(args, ctx)?;
                    self.call_user(info, a, e).map(Checked::Val)
                }
                Checked::Sol(view) => self.sol_eval(view, e, ctx).map(Checked::Val),
                _ => Err(self.err("only functions can be called", f.span, None)),
            };
        }
        if matches!(f.kind, A::ExprKind::Deriv { .. } | A::ExprKind::Field { .. } | A::ExprKind::VecCalc { .. }
                    | A::ExprKind::Call { .. })
        {
            match self.expr_any(f, ctx)? {
                Checked::Func { info, .. } => {
                    let a = self.call_args(args, ctx)?;
                    return self.call_user(info, a, e).map(Checked::Val);
                }
                Checked::Sol(view) => return self.sol_eval(view, e, ctx).map(Checked::Val),
                _ => {}
            }
        }
        if matches!(f.kind, A::ExprKind::Num { .. } | A::ExprKind::Quantity { .. }) || f.paren {
            let v = self.expr(f, ctx)?;
            if args.len() == 1 {
                let arg = self.expr(&args[0], ctx)?;
                self.need_numlike(&v, f, "this value", false)?;
                self.need_numlike(&arg, &args[0], "this value", false)?;
                return self.arith("*", v, arg, e).map(Checked::Val);
            }
        }
        Err(self.err("this can't be called like a function", e.span, None))
    }

    /// The arguments of a call of a user function: values, or functions bound at compile time (D43).
    pub fn call_args(&mut self, arg_asts: &[A::Expr], ctx: &mut Ctx) -> CResult<Vec<Checked>> {
        let mut out = vec![];
        for a in arg_asts {
            if let A::ExprKind::Name { name: n } = &a.kind {
                if self.lookup(ctx.scope, n).is_none() && PASSABLE_BUILTINS.contains(&n.as_str()) && is_builtin(n) {
                    let info = self.builtin_info(n);
                    out.push(Checked::Func { info, name: n.clone(), param: false });
                    continue;
                }
            }
            match self.expr_any(a, ctx)? {
                Checked::Sol(view) => {
                    let n = self.sols[view].name.clone();
                    return Err(self.err(format!("{n} is the solution of an ODE; it can't be passed to a function yet"),
                                        a.span, Some(format!("pass a value, like {n}(1 s), or values({n}) for all \
                                                              computed values"))));
                }
                v => out.push(v),
            }
        }
        Ok(out)
    }

    fn builtin_info(&mut self, name: &str) -> FuncInfoId {
        if let Some(&id) = self.builtin_infos.get(name) {
            return id;
        }
        let z = A::Span::default();
        let x = crate::ast_ext::mk(A::ExprKind::Name { name: "x".into() }, z);
        let body = crate::ast_ext::mk(A::ExprKind::Call { func: Box::new(crate::ast_ext::mk(A::ExprKind::Name { name: name.into() }, z)),
                                                    args: vec![x] }, z);
        let fd = A::Stmt { kind: A::StmtKind::FuncDef { name: name.into(),
                                                        params: vec![A::Param { name: "x".into(), unit: None, span: z }],
                                                        body: A::FuncBody::Expr(body), where_: vec![] },
                           span: z };
        self.funcs.push(FuncInfo { name: format!("builtin.{name}"), fdef: Some(fd), scope: self.root,
                                   instances: HashMap::new(), display_name: name.into(), checked_generic: false,
                                   stable: false, nat: None, module: None, anon_label: None, parent: None });
        let id = self.funcs.len() - 1;
        self.builtin_infos.insert(name.to_string(), id);
        id
    }

    /// Parameters the body uses like functions: {name: (sure, example)}. sure: p'(x), d/dx p, ∇p, p(a, b); not
    /// sure: p(x) (with a number for p, that means p × x).
    pub fn func_param_uses(&self, info: FuncInfoId) -> HashMap<String, (bool, String)> {
        let Some(A::Stmt { kind: A::StmtKind::FuncDef { params, body, where_, .. }, .. }) = &self.funcs[info].fdef
        else {
            return HashMap::new();
        };
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let mut exprs: Vec<&A::Expr> = vec![];
        match body {
            A::FuncBody::Expr(b) => crate::walk::descendants(b, &mut exprs),
            A::FuncBody::Block(stmts) => {
                let mut tops = vec![];
                crate::walk::all_exprs_in_stmts(stmts, &mut tops);
                for t in tops {
                    crate::walk::descendants(t, &mut exprs);
                }
            }
        }
        for (_, v) in where_ {
            crate::walk::descendants(v, &mut exprs);
        }
        // Python pops from the end of a stack; the "last seen wins" details only matter for p(x) vs p(a, b)
        let mut uses: HashMap<String, (bool, String)> = HashMap::new();
        for n in exprs.iter().rev() {
            match &n.kind {
                A::ExprKind::Prime { target, order } => {
                    if let A::ExprKind::Name { name: t } = &target.kind {
                        if names.contains(&t.as_str()) {
                            uses.insert(t.clone(), (true, format!("{t}{}", "'".repeat(*order as usize))));
                        }
                    }
                }
                A::ExprKind::Deriv { operand, var, .. } => {
                    if let A::ExprKind::Name { name: t } = &operand.kind {
                        if names.contains(&t.as_str()) {
                            uses.insert(t.clone(), (true, format!("d/d{var} {t}")));
                        }
                    }
                }
                A::ExprKind::VecCalc { func, .. } => {
                    if let A::ExprKind::Name { name: t } = &func.kind {
                        if names.contains(&t.as_str()) {
                            uses.insert(t.clone(), (true, format!("∇{t}")));
                        }
                    }
                }
                A::ExprKind::Call { func, args } => {
                    if let A::ExprKind::Name { name: t } = &func.kind {
                        if names.contains(&t.as_str()) {
                            let sure = args.len() != 1;
                            let a = args.iter().map(crate::source::to_source).collect::<Vec<_>>().join(", ");
                            if sure || !uses.contains_key(t) {
                                uses.insert(t.clone(), (sure, format!("{t}({a})")));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        uses
    }

    pub fn call_user(&mut self, info: FuncInfoId, args: Vec<Checked>, node: &A::Expr) -> CResult<I::Expr> {
        let nparams = self.func_params(info).len();
        if args.len() != nparams {
            let dn = &self.funcs[info].display_name;
            return Err(self.err(format!("{dn} takes {nparams} argument{} but was given {}",
                                        if nparams != 1 { "s" } else { "" }, args.len()), node.span, None));
        }
        let list_args: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, Checked::Val(v) if matches!(v.ty, Ty::List(_))))
            .map(|(i, _)| i)
            .collect();
        if !list_args.is_empty() && !self.takes_lists(info) {
            // f(xs, ys): element by element, lists of the same length (D191)
            let mut scalar_args = args.clone();
            for &j in &list_args {
                let Checked::Val(v) = &args[j] else { unreachable!() };
                let d = ty_dim(&v.ty).unwrap();
                scalar_args[j] = Checked::Val(ir(I::ExprKind::Const(0.0), Ty::Num(d), node.span.line));
            }
            let call = self.instantiate(info, scalar_args, node, true)?;
            let Ty::Num(rd) = &call.ty else {
                let what = if matches!(call.ty, Ty::Vec { .. }) { "a vector" } else { "something other than a number" };
                return Err(self.err(format!("{} returns {what}, so it can't be applied to each element of a list \
                                             (lists of vectors aren't supported yet)", self.funcs[info].display_name),
                                    node.span, Some("loop over the list and push the components into separate lists".into())));
            };
            let rd = rd.clone();
            let I::ExprKind::Call(fid, _) = call.kind else { unreachable!() };
            let rt_args: Vec<I::Expr> = args.iter().filter_map(|a| if let Checked::Val(v) = a { Some(v.clone()) } else { None }).collect();
            let pos: Vec<usize> = list_args
                .iter()
                .map(|&j| args[..j].iter().filter(|a| matches!(a, Checked::Val(_))).count())
                .collect();
            let mut r = ir(I::ExprKind::Map { func: fid, args: rt_args, list_pos: pos }, Ty::List(rd), node.span.line);
            r.sf = call.sf;
            return Ok(r);
        }
        self.instantiate(info, args, node, true)
    }

    pub fn func_params(&self, info: FuncInfoId) -> Vec<A::Param> {
        match &self.funcs[info].fdef {
            Some(A::Stmt { kind: A::StmtKind::FuncDef { params, .. }, .. }) => params.clone(),
            _ => vec![],
        }
    }

    /// Does the function use a parameter as a list (xs[i], sum(xs), for x in xs, …)?
    pub fn takes_lists(&self, info: FuncInfoId) -> bool {
        let Some(A::Stmt { kind: A::StmtKind::FuncDef { params, body, .. }, .. }) = &self.funcs[info].fdef else {
            return false;
        };
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let is_param = |e: &A::Expr| matches!(&e.kind, A::ExprKind::Name { name: n } if names.contains(&n.as_str()));
        let mut exprs: Vec<&A::Expr> = vec![];
        let stmts: Vec<A::Stmt> = match body {
            A::FuncBody::Expr(b) => vec![A::Stmt { kind: A::StmtKind::ExprStmt { value: b.clone() }, span: b.span }],
            A::FuncBody::Block(s) => s.clone(),
        };
        let mut tops = vec![];
        crate::walk::all_exprs_in_stmts(&stmts, &mut tops);
        for t in tops {
            crate::walk::descendants(t, &mut exprs);
        }
        for n in exprs {
            match &n.kind {
                A::ExprKind::Index { target, .. } if is_param(target) => return true,
                A::ExprKind::Call { func, args } => {
                    if let A::ExprKind::Name { name: fname } = &func.kind {
                        let f = fname.as_str();
                        if LIST_FUNCS.contains(&f) || matches!(f, "push" | "append" | "max" | "min" | "dot") {
                            for a in args {
                                if is_param(a) {
                                    if matches!(f, "max" | "min") && args.len() > 1 {
                                        continue;
                                    }
                                    return true;
                                } else if matches!(f, "sum" | "mean" | "std") && args.len() == 1
                                    && crate::walk::free_names(a).iter().any(|x| names.contains(&x.as_str()))
                                {
                                    return true; // sum(xs / σs²): a reduction of the list parameters (M7)
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        fn stmts_use(stmts: &[A::Stmt], names: &[&str]) -> bool {
            for s in stmts {
                match &s.kind {
                    A::StmtKind::ForIn { iterable, .. } => {
                        if matches!(&iterable.kind, A::ExprKind::Name { name: n } if names.contains(&n.as_str())) {
                            return true;
                        }
                    }
                    A::StmtKind::IndexAssign { target, .. } if names.contains(&target.as_str()) => return true, // A16
                    _ => {}
                }
                if crate::walk::stmt_blocks(s).iter().any(|b| stmts_use(b, names)) {
                    return true;
                }
            }
            false
        }
        stmts_use(&stmts, &names)
    }

    pub fn instantiate(&mut self, info: FuncInfoId, fargs: Vec<Checked>, node: &A::Expr, cache: bool)
                       -> CResult<I::Expr> {
        if self.funcs[info].module.is_some() && !self.in_module_call.contains(&info) {
            return self.module_call(info, fargs, node, cache); // a module's function (D101)
        }
        let Some(fdef) = self.funcs[info].fdef.clone() else { unreachable!() };
        let A::StmtKind::FuncDef { params, body, .. } = &fdef.kind else { unreachable!() };
        let display = self.funcs[info].display_name.clone();
        let mut keyparts = vec![];
        let mut concrete = true;
        for a in &fargs {
            match a {
                Checked::Func { info: fi, .. } => keyparts.push(format!("fn{fi}")), // one instance per function (D43)
                Checked::Val(v) => match &v.ty {
                    Ty::Num(d) | Ty::List(d) | Ty::Complex(d) => {
                        let n = self.u.norm(d);
                        if !n.is_concrete() {
                            concrete = false;
                        }
                        keyparts.push(format!("{}{:?}", v.ty.kind(), n.konst.0));
                    }
                    t => keyparts.push(t.kind().to_string()),
                },
                Checked::Sol(_) => keyparts.push("sol".into()),
            }
        }
        if let Some(fnat) = self.funcs[info].nat.clone() {
            if !self.nat_contains(&fnat) {
                return Err(self.err(format!("{display} was defined in {} units ({} = 1), so it can only be used where \
                                             those units hold", self.system_name(&fnat), self.system_consts(&fnat)),
                                    node.span, Some(format!("define {display} outside the  units  region to use it \
                                                             everywhere"))));
            }
        }
        keyparts.push(format!("units:{}", self.nat)); // checked again in each unit system it is used in (D60)
        let key = keyparts.join("|");
        let args: Vec<I::Expr> = fargs.iter().filter_map(|a| if let Checked::Val(v) = a { Some(v.clone()) } else { None }).collect();
        let line = node.span.line;
        if cache && concrete {
            if let Some(&inst) = self.funcs[info].instances.get(&key) {
                let fx = &mut self.func_extra[inst];
                if fx.checking {
                    fx.called_recursively = true; // its type is still the placeholder (a number)
                }
                let ret = self.module.funcs[inst].ret_ty.clone();
                let isf = self.module.funcs[inst].sf;
                let mut r = ir(I::ExprKind::Call(inst, args.clone()), ret, line);
                let refs: Vec<&I::Expr> = args.iter().collect();
                r.sf = match isf {
                    Some(s) => minsf(&refs).map_or(Some(s), |m| Some(m.min(s))),
                    None => minsf(&refs),
                };
                r.hint = self.func_extra[inst].ret_hint.clone();
                self.arg_hint(&mut r, &args);
                return Ok(r);
            }
        }
        for (p, a) in params.iter().zip(&fargs) {
            if let Checked::Val(v) = a {
                if delta_name(&p.name) && self.abs_temp(v) {
                    return Err(self.delta_abs_temp_error(&p.name, v, node.span));
                }
            }
        }
        let uses = self.func_param_uses(info);
        for (p, a) in params.iter().zip(&fargs) {
            let Checked::Val(v) = a else { continue };
            if let Some((sure, ex)) = uses.get(&p.name) {
                if *sure {
                    return Err(self.err(format!("{display} uses {} as a function ({ex}), but was given {}", p.name,
                                                self.type_desc(&v.ty)), node.span,
                                        Some(format!("pass a function's name, like {display}(g, ...) after g(x) = ..."))));
                }
                if matches!(v.ty, Ty::Num(_)) && node.span != fdef.span {
                    self.warn(format!("{} is a number here, so {ex} in {display} means {} × (...)", p.name, p.name),
                              A::Span { length: 1, ..node.span },
                              Some(format!("to pass a function, give its name: {display}(g, ...) after g(x) = ...; to \
                                            multiply, write {}*(...)", p.name)));
                }
            }
        }
        // the new instance: reserve its id so recursive calls can refer to it
        let placeholder = DExpr::fresh();
        let inst_name = self.fresh_name(&self.funcs[info].name.clone());
        self.module.funcs.push(I::Func { name: inst_name, params: vec![], ret_ty: Ty::Num(placeholder.clone()),
                                         body: vec![], locals: vec![], sf: None });
        let inst = self.module.funcs.len() - 1;
        self.func_extra.resize(inst + 1, FuncExtra::default());
        self.func_extra[inst] = FuncExtra { display: display.clone(), ret_placeholder: Some(placeholder.clone()),
                                            checking: true, def_line: fdef.span.line, ..Default::default() };
        if cache && concrete {
            self.funcs[info].instances.insert(key.clone(), inst);
        }
        let fscope = self.funcs[info].scope;
        let scope = self.new_scope(Some(fscope), "func");
        let mut fctx = Ctx { func: Owner::Func(inst), scope, is_main: false, lam: None, loop_depth: 0, branch: 0,
                             ret_types: self.new_ret_types(), regions: vec![], lam_parents: vec![] };
        // Python checks the parameters before its try: their errors get no call note (D101: nor a module note)
        let mut in_body = false;
        let result: CResult<()> = (|| {
            for (p, a) in params.iter().zip(&fargs) {
                match a {
                    Checked::Func { info: fi, name, .. } => {
                        if let Some(u) = &p.unit {
                            return Err(self.err(format!("{display} expects {} in {}, but got the function {name}",
                                                        p.name, u.text), node.span, None));
                        }
                        self.bind(scope, &p.name, Binding::Func(*fi)); // V(x) in the body calls the function passed in
                    }
                    Checked::Val(v) => {
                        let ty = match &v.ty {
                            Ty::Num(d) => Ty::Num(d.clone()),
                            Ty::List(d) => Ty::List(d.clone()),
                            t => t.clone(),
                        };
                        let sym = self.new_sym(&p.name, ty.clone(), &fctx);
                        self.module.funcs[inst].locals.retain(|s| *s != sym);
                        self.extra[sym].assigned = true;
                        if let Some(uexpr) = &p.unit {
                            let u = self.resolve_unit(uexpr)?;
                            let ok = match &ty {
                                Ty::Num(d) | Ty::List(d) | Ty::Complex(d) => self.u.unify(d, &DExpr::of(u.dim)),
                                _ => false,
                            };
                            if !ok {
                                return Err(self.err(format!("{display} expects {} in {} ({}), but got {}", p.name, u.name,
                                                            self.desc(&DExpr::of(u.dim)), self.type_desc(&v.ty)),
                                                    node.span, None));
                            }
                            self.module.syms[sym].hint = Some(crate::exprs::hint_of(&u));
                        }
                        self.module.funcs[inst].params.push(sym);
                        self.bind(scope, &p.name, Binding::Sym(sym));
                    }
                    Checked::Sol(_) => unreachable!(),
                }
            }
            in_body = true;
            match body {
                A::FuncBody::Expr(_) => {
                    let b = self.body_expr(info).unwrap();
                    let b = if self.funcs[info].stable { self.stabilize(&b) } else { b };
                    let v = self.expr(&b, &mut fctx)?;
                    self.ret_types[fctx.ret_types].push(v.clone());
                    self.module.funcs[inst].body = vec![I::Stmt { kind: I::StmtKind::Return(Some(v)), line: fdef.span.line }];
                }
                A::FuncBody::Block(stmts) => {
                    let mut stmts = stmts.clone();
                    if let Some(last) = stmts.last_mut() {
                        if let A::StmtKind::ExprStmt { value: v } = &last.kind {
                            last.kind = A::StmtKind::Return { value: Some(v.clone()) };
                        }
                    }
                    let b = self.block(&stmts, &mut fctx)?;
                    if !self.ret_types[fctx.ret_types].is_empty() && !always_returns(&b) {
                        return Err(self.err(format!("{display} doesn't return a value on every path (for example when \
                                                     an if is false, or a loop doesn't run)"), fdef.span,
                                            Some("make sure the function ends with a value or a return that always \
                                                  runs".into())));
                    }
                    self.module.funcs[inst].body = b;
                }
            }
            Ok(())
        })();
        self.func_extra[inst].checking = false;
        if let Err(mut e) = result {
            self.funcs[info].instances.remove(&key);
            if !in_body {
                return Err(e);
            }
            if e.line.is_none() {
                e.line = Some(node.span.line).filter(|l| *l != 0);
                e.col = Some(node.span.col).filter(|c| *c != 0);
            } else if node.span.line != 0 && e.line != Some(node.span.line) && !self.call_noted.contains(&err_key(&e)) {
                let argd = params
                    .iter()
                    .zip(&fargs)
                    .map(|(p, a)| match a {
                        Checked::Func { name, .. } => format!("{} = the function {name}", p.name),
                        Checked::Val(v) => format!("{} = {}", p.name, self.type_desc(&v.ty)),
                        Checked::Sol(_) => format!("{} = ?", p.name),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut note = format!("this happened when calling {display} on line {} (with {argd})", node.span.line);
                let num_called: Vec<&A::Param> = params
                    .iter()
                    .zip(&fargs)
                    .filter(|(p, a)| matches!(a, Checked::Val(v) if matches!(v.ty, Ty::Num(_))) && uses.contains_key(&p.name))
                    .map(|(p, _)| p)
                    .collect();
                if let Some(q) = num_called.first() {
                    note += &format!("; {} was given a number, so {} means {} × (...); to pass a function, give its name",
                                     q.name, uses[&q.name].1, q.name);
                }
                e.hint = Some(match e.hint {
                    Some(h) => format!("{h}; {note}"),
                    None => note,
                });
                self.call_noted.insert(err_key(&e));
            }
            if self.funcs[info].module.is_some() {
                self.module_body_error(&mut e, info, node);
            }
            return Err(e);
        }
        let rets = self.ret_types[fctx.ret_types].clone();
        if rets.is_empty() {
            return Err(self.err(format!("the function {display} never returns a value"), fdef.span,
                                Some("end it with the value to return, or use  return value".into())));
        }
        if rets.iter().all(|r| matches!(r.kind, I::ExprKind::Call(f, _) if f == inst)) {
            return Err(self.err(format!("{display} always calls itself, so it would never finish"), fdef.span,
                                Some("add a case that returns without calling it, like  if n <= 0 then 1 else ...".into())));
        }
        let rt = rets[0].ty.clone();
        for r in &rets[1..] {
            let bad = !same_kind(&r.ty, &rt)
                || match (&rt, &r.ty) {
                    (Ty::Num(a), Ty::Num(b)) | (Ty::List(a), Ty::List(b)) => !self.u.unify(a, b),
                    _ => false,
                };
            if bad {
                return Err(self.err(format!("{display} returns different kinds of values in different places"),
                                    fdef.span, None));
            }
        }
        if let Ty::Num(d) = &rt {
            if !self.u.unify(&placeholder, d) {
                return Err(self.err(format!("the units of {display} don't work out recursively"), fdef.span, None));
            }
        } else if self.func_extra[inst].called_recursively {
            self.funcs[info].instances.remove(&key);
            return Err(self.err(format!("{display} calls itself and returns {}; a function that calls itself must \
                                         return a single real number", self.type_desc(&rt)), fdef.span,
                                Some("write the recursion as a loop".into())));
        }
        let refs: Vec<&I::Expr> = rets.iter().collect();
        let isf = minsf(&refs);
        self.module.funcs[inst].ret_ty = rt.clone();
        self.module.funcs[inst].sf = isf;
        let ret_hint = if rets.len() == 1 { rets[0].hint.clone() } else { None };
        self.func_extra[inst].ret_hint = ret_hint.clone();
        let mut r = ir(I::ExprKind::Call(inst, args.clone()), rt, line);
        r.hint = ret_hint;
        self.arg_hint(&mut r, &args);
        let mut refs: Vec<&I::Expr> = args.iter().collect();
        let fake;
        if let Some(s) = isf {
            fake = I::Expr { sf: Some(s), ..ir(I::ExprKind::Const(0.0), Ty::Void, 0) };
            refs.push(&fake);
        }
        r.sf = minsf(&refs);
        Ok(r)
    }

}

fn err_key(e: &Diagnostic) -> String {
    format!("{}|{:?}|{:?}", e.message, e.line, e.col)
}

/// Does a statement list always end in a return (Python _always_returns)?
pub fn always_returns(stmts: &[I::Stmt]) -> bool {
    for st in stmts {
        match &st.kind {
            I::StmtKind::Return(_) => return true,
            I::StmtKind::If(_, then, other) if !other.is_empty() && always_returns(then) && always_returns(other) => {
                return true
            }
            _ => {}
        }
    }
    false
}

impl Checker {
    /// g(a, b) of a LocalFunc: `body where s = a, t = b`, the body's other names seen from g's definition (D194).
    pub fn call_local(&mut self, lf: LocalFuncId, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Call { args: arg_asts, .. } = &e.kind else { unreachable!() };
        let l = self.local_funcs[lf].clone();
        let A::StmtKind::FuncDef { name, params, body: A::FuncBody::Expr(body), .. } = &l.fdef.kind else {
            unreachable!()
        };
        if arg_asts.len() != params.len() {
            let n = params.len();
            return Err(self.err(format!("{name} takes {n} argument{} but was given {}", if n != 1 { "s" } else { "" },
                                        arg_asts.len()), e.span, None));
        }
        if l.expanding {
            return Err(self.err(format!("{name} calls itself; a function defined inside another function can't be \
                                         recursive (define it at the top level)"), e.span, None));
        }
        let mut args = vec![];
        for a in arg_asts {
            match self.expr_any(a, ctx)? {
                Checked::Val(v) => args.push(v),
                _ => {
                    return Err(self.err(format!("the arguments of {name} must be values (a function defined inside \
                                                 another function can't take a function)"), a.span, None))
                }
            }
        }
        let scope = self.new_scope(Some(l.scope), "block");
        let mut c2 = ctx.child(scope);
        let mut binds = vec![];
        for (p, v) in params.iter().zip(&args) {
            let fname = self.fresh_name(&p.name);
            let sym = self.new_sym(&fname, v.ty.clone(), &c2);
            let ms = &mut self.module.syms[sym];
            ms.sf = v.sf;
            ms.hint = v.hint.clone();
            ms.direct = v.direct;
            self.extra[sym].assigned = true;
            self.bind(scope, &p.name, Binding::Sym(sym));
            binds.push((sym, v.clone()));
        }
        self.local_funcs[lf].expanding = true;
        let r = self.expr(body, &mut c2);
        self.local_funcs[lf].expanding = false;
        let body = match r {
            Ok(b) => b,
            Err(mut ex) => {
                let key = format!("local|{}", err_key(&ex));
                if e.span.line != 0 && ex.line.is_some() && ex.line != Some(e.span.line) && !self.call_noted.contains(&key) {
                    let desc = params
                        .iter()
                        .zip(&args)
                        .map(|(p, v)| format!("{} = {}", p.name, self.type_desc(&v.ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let note = format!("this happened when calling {name} on line {} (with {desc})", e.span.line);
                    ex.hint = Some(match ex.hint {
                        Some(h) => format!("{h}; {note}"),
                        None => note,
                    });
                    self.call_noted.insert(format!("local|{}", err_key(&ex)));
                }
                return Err(ex);
            }
        };
        let (ty, hint, sf) = (body.ty.clone(), body.hint.clone(), body.sf);
        let mut r = ir(I::ExprKind::Let(binds, Box::new(body)), ty, e.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }
}
