//! Calculus: a port of Checker.e_Prime, derived_info, _diffctx, e_Deriv, e_VecCalc, _vector_components,
//! e_Integral, _same_dim, _warn_sum_in_limit, _hint_sum_after_integral, _limit_division_error,
//! _warn_limit_division, e_Sum, vector_integral, _vector_integral_retry, indefinite_integral, _leibniz and
//! function_units from `fermium/checker.py`. The symbolic work is in fermium-sym.
use std::collections::{HashMap, HashSet};

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_sym as C;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;
use num_rational::Rational64;

use crate::arith::minsf;
use crate::ast_ext::mk;
use crate::builtins::is_builtin;
use crate::checker::*;
use crate::stmts::ty_dim;

/// Checker state for calculus (Python keeps it on FuncInfo.derived).
#[derive(Clone, Debug, Default)]
pub struct CalcState {
    /// (function, "i,order" or "veccalc,kind") → the derived function
    pub derived: HashMap<(FuncInfoId, String), FuncInfoId>,
}

fn sup(n: i64) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            c => c,
        })
        .collect()
}

fn at(mut e: A::Expr, span: A::Span) -> A::Expr {
    e.span = span;
    e
}

fn fdef_parts(s: &A::Stmt) -> (&str, &Vec<A::Param>, &A::FuncBody) {
    let A::StmtKind::FuncDef { name, params, body, .. } = &s.kind else { unreachable!() };
    (name, params, body)
}

fn new_fdef(name: &str, params: Vec<A::Param>, body: A::Expr, span: A::Span) -> A::Stmt {
    A::Stmt { kind: A::StmtKind::FuncDef { name: name.to_string(), params, body: A::FuncBody::Expr(body), where_: vec![] },
              span }
}

fn param(n: &str) -> A::Param {
    A::Param { name: n.to_string(), unit: None, span: A::Span::default() }
}

/// The differentiator's view of names in a scope (Python Checker._diffctx).
struct DC<'a> {
    ck: &'a mut Checker,
    scope: ScopeId,
}

impl C::DiffContext for DC<'_> {
    fn user_function(&mut self, fname: &str) -> C::SymResult<Option<(Vec<String>, A::Expr)>> {
        if let Some((Binding::Func(b), _)) = self.ck.lookup(self.scope, fname) {
            if !self.ck.funcs[b].one_liner() {
                return Err(C::build::ferr0(
                    format!("can't differentiate through {fname}: it's defined over several lines"),
                    Some(format!("symbolic derivatives need one-line functions: write {fname} on one line (with  \
                                  where  for helper names), or use a finite difference, (f(x + h) - f(x - h)) / (2h)")),
                ));
            }
            let params = self.ck.func_params(b).iter().map(|p| p.name.clone()).collect();
            return Ok(Some((params, self.ck.body_expr(b).unwrap())));
        }
        Ok(None)
    }

    fn derived_function(&mut self, fname: &str, i: usize, order: i64) -> C::SymResult<String> {
        let Some((Binding::Func(b), sc)) = self.ck.lookup(self.scope, fname) else { unreachable!() };
        let span = self.ck.funcs[b].fdef.as_ref().map(|f| f.span).unwrap_or_default();
        let d = self.ck.derived_info(b, i, order, span)?;
        let nm = self.ck.funcs[d].name.clone();
        self.ck.bind(sc, &nm, Binding::Func(d));
        Ok(nm)
    }

    fn is_solution(&mut self, fname: &str) -> bool {
        matches!(self.ck.lookup(self.scope, fname), Some((Binding::Sol(_), _)))
    }

    /// `mechanics.pendulum_period` in a formula being differentiated (red team round 2 #7): the module's function,
    /// bound here under a private name (not exported, can't clash with a user name) so it and its derivatives can be
    /// called from this scope like `from … import` names.
    fn field_function(&mut self, field: &A::Expr) -> Option<String> {
        let A::ExprKind::Field { target, name } = &field.kind else { return None };
        let m = self.ck.module_of_in(target, self.scope)?;
        if name.starts_with('_') {
            return None;
        }
        let mscope = self.ck.mods.modules[m].scope;
        let Some(Binding::Func(b)) = self.ck.scopes[mscope].names.get(name).cloned() else { return None };
        let key = format!("_{}", crate::source::to_source(field));
        self.ck.scopes[self.scope].names.entry(key.clone()).or_insert(Binding::Func(b));
        Some(key)
    }
}

impl Checker {
    fn func_ref(&self, info: FuncInfoId) -> Checked {
        Checked::Func { info, name: self.funcs[info].display_name.clone(), param: false }
    }

    fn new_func_info(&mut self, name: String, fdef: A::Stmt, scope: ScopeId) -> FuncInfoId {
        self.funcs.push(FuncInfo { name: name.clone(), fdef: Some(fdef), scope, instances: HashMap::new(),
                                   display_name: name, checked_generic: false, stable: false, nat: None, module: None,
                                   anon_label: None, parent: None });
        self.funcs.len() - 1
    }

    fn diffctx_diff(&mut self, body: &A::Expr, var: &str, scope: ScopeId) -> CResult<A::Expr> {
        let mut dc = DC { ck: self, scope };
        C::diff(body, var, &mut dc)
    }

    // ------------------------------------------------------------ e_Prime
    pub fn e_prime(&mut self, e: &A::Expr, target: &A::Expr, order: i64, ctx: &mut Ctx) -> CResult<Checked> {
        // inside an ODE right-hand side, x' is a state variable
        if let Some(n) = target.name() {
            let key = format!("{n}{}", "'".repeat(order.max(0) as usize));
            if let Some((Binding::Sym(s), _)) = self.lookup(ctx.scope, &key) {
                return self.var_ref(s, ctx, e).map(Checked::Val);
            }
        }
        let t = self.expr_any(target, ctx)?;
        match t {
            Checked::Func { info, .. } => {
                if self.func_params(info).len() != 1 {
                    let dn = self.funcs[info].display_name.clone();
                    return Err(self.err(format!("{dn}' is ambiguous: {dn} has several parameters"), e.span,
                                        Some("use ∂/∂x to say which one".into())));
                }
                let d = self.derived_info(info, 0, order, e.span)?;
                Ok(self.func_ref(d))
            }
            Checked::Sol(view) => {
                let v = self.sols[view].clone();
                let p = Rational64::from_integer(order);
                let nv = SolView { comp: v.comp + order as usize * v.stride, dim: v.dim.div(&v.tdim.pow(p)),
                                   name: format!("{}{}", v.name, "'".repeat(order as usize)), ..v.clone() };
                self.sols.push(nv);
                Ok(Checked::Sol(self.sols.len() - 1))
            }
            Checked::Val(_) => Err(self.err("' (prime) means a derivative; it only works on functions and ODE solutions",
                                            e.span, Some("to differentiate a formula write d/dt (formula)".into()))),
        }
    }

    /// The function ∂ᵒʳᵈᵉʳf/∂(param i)ᵒʳᵈᵉʳ, made once (Python derived_info).
    pub fn derived_info(&mut self, info: FuncInfoId, i: usize, order: i64, node: A::Span) -> CResult<FuncInfoId> {
        let key = (info, format!("{i},{order}"));
        if let Some(&d) = self.calc.derived.get(&key) {
            return Ok(d);
        }
        let dn = self.funcs[info].display_name.clone();
        if !self.funcs[info].one_liner() {
            return Err(self.err(format!("can only differentiate one-line functions like f(x) = ..., and {dn} is \
                                         defined over several lines"), node, None));
        }
        let fdef = self.funcs[info].fdef.clone().unwrap();
        let (_, params, _) = fdef_parts(&fdef);
        let pname = params[i].name.clone();
        let mut body = self.body_expr(info).unwrap();
        let scope = self.funcs[info].scope;
        for _ in 0..order {
            match self.diffctx_diff(&body, &pname, scope) {
                Ok(b) => body = b,
                Err(mut ex) => {
                    if ex.line.is_none() && node.line != 0 {
                        ex.line = Some(node.line);
                        ex.col = Some(node.col).filter(|c| *c != 0);
                    } else if node.line != 0 && ex.line != Some(node.line)
                        && !ex.hint.as_deref().unwrap_or("").contains(&format!("line {}", node.line))
                    {
                        // `d/dT N` where N calls a multi-line F: say where the derivative was asked for too (#71)
                        let add = format!("(needed for the derivative of {dn} on line {})", node.line);
                        ex.hint = Some(match ex.hint {
                            Some(h) if !h.is_empty() => format!("{h}; {add}"),
                            _ => add,
                        });
                    }
                    return Err(ex);
                }
            }
        }
        let one = params.len() == 1;
        let suffix = if one { "'".repeat(order as usize) } else { format!("_∂{pname}").repeat(order as usize) };
        let pretty = if one {
            format!("{dn}{}", "'".repeat(order as usize))
        } else if order == 1 {
            format!("∂{dn}/∂{pname}")
        } else {
            format!("∂{}{dn}/∂{pname}{}", sup(order), sup(order))
        };
        let nm = format!("{}{suffix}", self.funcs[info].name);
        let fd = new_fdef(&nm, params.clone(), body, fdef.span);
        let d = self.new_func_info(nm, fd, scope);
        let parent = match self.funcs[info].parent {
            None => (info, i, order as u32),
            Some((p, _, o)) => (p, i, order as u32 + o),
        };
        let nat = self.funcs[info].nat.clone();
        let f = &mut self.funcs[d];
        f.nat = nat;
        f.display_name = pretty;
        f.stable = true;
        f.parent = Some(parent);
        self.calc.derived.insert(key, d);
        Ok(d)
    }

    // ------------------------------------------------------------ e_Deriv
    pub fn e_deriv(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Deriv { var, order, operand: op, partial } = &e.kind else { unreachable!() };
        let (var, order, partial) = (var.as_str(), *order, *partial);
        let mut inner: Option<FuncInfoId> = None;
        if let A::ExprKind::Deriv { operand: o2, .. } = &op.kind {
            if matches!(o2.kind, A::ExprKind::Name { .. } | A::ExprKind::Deriv { .. }) {
                // ∂/∂x ∂/∂y f: differentiate the function ∂f/∂y again (mixed partials, red team 5 #18)
                if let Checked::Func { info, .. } = self.expr_any(op, ctx)? {
                    inner = Some(info);
                }
            }
        }
        if op.is_name() || inner.is_some() {
            let b = match inner {
                Some(i) => Some(Binding::Func(i)),
                None => self.lookup(ctx.scope, op.name().unwrap()).map(|x| x.0),
            };
            match b {
                Some(Binding::Func(b)) => {
                    let params: Vec<String> = self.func_params(b).iter().map(|p| p.name.clone()).collect();
                    let i = if let Some(i) = params.iter().position(|p| p == var) {
                        i
                    } else if params.len() == 1 && !partial {
                        0
                    } else {
                        return Err(self.err(format!("{} has no parameter called {var}", self.funcs[b].display_name),
                                            e.span, None));
                    };
                    let d = self.derived_info(b, i, order, e.span)?;
                    return Ok(self.func_ref(d));
                }
                Some(Binding::Sol(_)) => {
                    return self.e_prime(e, op, order, ctx);
                }
                _ => {}
            }
        }
        if let A::ExprKind::Call { func: of, args: oargs } = &op.kind {
            if let Some(fname) = of.name() {
                if !oargs.iter().any(|a| C::depends_on(a, var)) {
                    // d/dt x(2 s): read as "the derivative of x at 2 s", (d/dt x)(2 s) (red team 5 #4, D221)
                    let b = self.lookup(ctx.scope, fname).map(|x| x.0);
                    let inner_e = match b {
                        Some(Binding::Func(b)) => {
                            let params: Vec<String> = self.func_params(b).iter().map(|p| p.name.clone()).collect();
                            if !params.iter().any(|p| p == var) && !(params.len() == 1 && !partial) {
                                let ops = if partial { "∂/∂" } else { "d/d" };
                                let argsrc = oargs.iter().map(C::to_source).collect::<Vec<_>>().join(", ");
                                return Err(self.err(
                                    format!("{fname} has no parameter called {var}, so {ops}{var} {} would be 0",
                                            C::to_source(op)),
                                    e.span,
                                    Some(format!("{fname}'s parameters are {}; differentiate with respect to one of \
                                                  them, e.g. ({ops}{} {fname})({argsrc})", params.join(", "),
                                                 params[0])),
                                ));
                            }
                            Some(at(mk(A::ExprKind::Deriv { var: var.to_string(), order, operand: of.clone(), partial },
                                       e.span), e.span))
                        }
                        Some(Binding::Sol(_)) => {
                            Some(mk(A::ExprKind::Prime { target: of.clone(), order }, e.span))
                        }
                        _ => None,
                    };
                    if let Some(ie) = inner_e {
                        let mut call = (**op).clone();
                        call.kind = A::ExprKind::Call { func: Box::new(ie), args: oargs.clone() };
                        return self.expr_any(&call, ctx);
                    }
                }
            }
        }
        // derivative of a formula
        let bound = self.lookup(ctx.scope, var).map(|x| x.0);
        let mut body = (**op).clone();
        for _ in 0..order {
            body = self.diffctx_diff(&body, var, ctx.scope)?;
        }
        if let Some(Binding::Sym(_)) = bound {
            return self.expr_any(&C::stabilize(&body), ctx);
        }
        // d/dt (formula in t) with t not defined: a new function of t; `d/dr f(r, R)` with R not defined either:
        // a function of (r, R), ∂/∂r with R kept as a parameter (D212)
        let mut params = vec![var.to_string()];
        if let A::ExprKind::Call { func: of, args: oargs } = &op.kind {
            let mut loose: Vec<String> = vec![];
            for n in C::free_names(op) {
                if n != var && !is_builtin(&n) && self.lookup(ctx.scope, &n).is_none() && !loose.contains(&n) {
                    loose.push(n);
                }
            }
            let argn: Vec<String> = oargs.iter().filter_map(|a| a.name().map(str::to_string)).collect();
            if !loose.is_empty() && argn.len() == oargs.len() && argn.iter().any(|a| a == var)
                && loose.iter().all(|l| argn.contains(l))
            {
                let mut ps: Vec<String> = vec![];
                for n in argn {
                    if (n == var || loose.contains(&n)) && !ps.contains(&n) {
                        ps.push(n);
                    }
                }
                params = ps;
            } else if !loose.is_empty() {
                let fnm = C::to_source(of);
                return Err(self.err(
                    format!("d/d{var} {}: {} {}n't defined, so this can't be a function of {var} alone", C::to_source(op),
                            loose.join(", "), if loose.len() == 1 { "is" } else { "are" }),
                    e.span,
                    Some(format!("for the partial derivative with the other arguments kept as parameters, write \
                                  ∂/∂{var} {fnm}  (a function of the same arguments as {fnm})")),
                ));
            }
        }
        let fd = new_fdef(&format!("λ{}", self.counter), params.iter().map(|p| param(p)).collect(), body, e.span);
        let scope = if ctx.is_main { ctx.scope } else { self.globals };
        let fname = self.fresh_name("deriv");
        let info = self.new_func_info(fname, fd, scope);
        let dd = if partial { "∂" } else { "d" };
        let s = if order == 1 { String::new() } else { sup(order) };
        let label = format!("{dd}{s}/{dd}{var}{s} ({})", C::to_source(op));
        let nat = if self.nat.is_empty() { None } else { Some(self.nat.clone()) };
        let f = &mut self.funcs[info];
        f.nat = nat;
        f.display_name = format!("{dd}/{dd}{var}(...)");
        f.anon_label = Some(label);
        f.stable = true;
        Ok(self.func_ref(info))
    }

    // ------------------------------------------------------------ ∇
    pub fn e_veccalc(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::VecCalc { kind, func } = &e.kind else { unreachable!() };
        let kind = kind.as_str();
        let name = C::to_source(func);
        let sym = match kind {
            "grad" => "∇",
            "div" => "∇·",
            "curl" => "∇×",
            _ => "∇²",
        };
        let b = match func.name().and_then(|n| self.lookup(ctx.scope, n)) {
            Some((Binding::Func(b), _)) => b,
            _ => {
                return Err(self.err(format!("{sym} works on a function of the coordinates, like φ(x, y, z) = ..., and \
                                             {name} isn't a function"), func.span, None))
            }
        };
        self.vec_calc_info(b, kind, sym, &name, func.span).map(|i| self.func_ref(i))
    }

    fn vec_calc_info(&mut self, b: FuncInfoId, kind: &str, sym: &str, name: &str, span: A::Span)
                     -> CResult<FuncInfoId> {
        let key = (b, format!("veccalc,{kind}"));
        if let Some(&d) = self.calc.derived.get(&key) {
            return Ok(d);
        }
        if !self.funcs[b].one_liner() {
            return Err(self.err(format!("{sym} can only differentiate one-line functions like φ(x, y, z) = ..., and \
                                         {name} is defined over several lines"), span, None));
        }
        let fdef = self.funcs[b].fdef.clone().unwrap();
        let (_, fparams, _) = fdef_parts(&fdef);
        let allp: Vec<String> = fparams.iter().map(|p| p.name.clone()).collect();
        // with parameters named x, y, z those are the coordinates; others (a mode number n) are held fixed (#47)
        let xyz: Vec<String> = allp.iter().filter(|p| matches!(p.as_str(), "x" | "y" | "z")).cloned().collect();
        let params = if xyz.len() >= 2 { xyz } else { allp.clone() };
        if !(2..=3).contains(&params.len()) || (kind == "curl" && params.len() != 3) {
            let need = if kind == "curl" { "3 coordinates, like B(x, y, z)" } else { "2 or 3 coordinates, like φ(x, y, z)" };
            return Err(self.err(format!("{sym}{name} needs a function of {need}; {name} has {}", params.len()), span,
                                if allp.len() > 3 {
                                    Some("name the coordinate parameters x, y (and z); other parameters are then held \
                                          fixed".into())
                                } else {
                                    None
                                }));
        }
        let scope = self.funcs[b].scope;
        let body = self.body_expr(b).unwrap();
        let addall = |ts: Vec<A::Expr>| -> A::Expr {
            let mut out = ts[0].clone();
            for t in &ts[1..] {
                out = C::build::binop("+", out, t.clone(), false);
            }
            C::simplify(&out)
        };
        let mut new = match kind {
            "grad" => {
                let mut items = vec![];
                for p in &params {
                    items.push(self.diffctx_diff(&body, p, scope)?);
                }
                C::build::veclit(items)
            }
            "lap" => {
                let mut ts = vec![];
                for p in &params {
                    let d1 = self.diffctx_diff(&body, p, scope)?;
                    ts.push(self.diffctx_diff(&d1, p, scope)?);
                }
                addall(ts)
            }
            _ => {
                let Some(comps) = self.vector_components(&C::inline_where(&body), scope, 0)? else {
                    return Err(self.err(format!("{sym}{name} needs {name} to be a vector formula, like {name}(x, y, z) = \
                                                 <-y, x, 0> T"), span, None));
                };
                if comps.len() != params.len() {
                    return Err(self.err(format!("{name} has {} components but {} coordinates; {sym} needs them to match",
                                                comps.len(), params.len()), span, None));
                }
                if kind == "div" {
                    let mut ts = vec![];
                    for (c, p) in comps.iter().zip(&params) {
                        ts.push(self.diffctx_diff(c, p, scope)?);
                    }
                    addall(ts)
                } else {
                    let (x, y, z) = (&params[0], &params[1], &params[2]);
                    let (fx, fy, fz) = (&comps[0], &comps[1], &comps[2]);
                    let mut items = vec![];
                    for (a, av, bb, bv) in [(fz, y, fy, z), (fx, z, fz, x), (fy, x, fx, y)] {
                        let da = self.diffctx_diff(a, av, scope)?;
                        let db = self.diffctx_diff(bb, bv, scope)?;
                        items.push(C::simplify(&C::build::sub(da, db)));
                    }
                    C::build::veclit(items)
                }
            }
        };
        if let A::ExprKind::VecLit { items } = &mut new.kind {
            // a component that differentiates to 0 fits the others' units
            for c in items.iter_mut() {
                *c = C::tidy(c);
                if c.num_value() == Some(0.0) {
                    *c = mk(A::ExprKind::Num { value: 0.0, sigfigs: None, digit: true }, A::Span::default());
                }
            }
        } else {
            new = C::tidy(&new);
        }
        let nm = format!("{}_{kind}", self.funcs[b].name);
        let fd = new_fdef(&nm, fparams.clone(), new, fdef.span);
        let info = self.new_func_info(nm.clone(), fd, scope);
        let dn = self.funcs[b].display_name.clone();
        let nat = self.funcs[b].nat.clone();
        let f = &mut self.funcs[info];
        f.nat = nat;
        f.display_name = format!("{sym}{dn}");
        f.stable = true;
        self.calc.derived.insert(key, info);
        self.bind(scope, &nm, Binding::Func(info));
        Ok(info)
    }

    /// The component formulas of a vector-valued formula, or None if it isn't one (Python _vector_components).
    fn vector_components(&mut self, body: &A::Expr, scope: ScopeId, depth: u32) -> CResult<Option<Vec<A::Expr>>> {
        use A::ExprKind as K;
        if depth > 20 {
            return Ok(None);
        }
        Ok(match &body.kind {
            K::Where { .. } => return self.vector_components(&C::inline_where(body), scope, depth + 1),
            K::VecLit { items } => Some(items.clone()),
            K::Call { func, args } if func.name() == Some("vec") => Some(args.clone()),
            K::Quantity { value, unit, bracket } => {
                match self.vector_components(value, scope, depth + 1)? {
                    Some(inner) => {
                        let one = mk(K::Quantity { value: Box::new(C::build::num(1.0)), unit: unit.clone(),
                                                   bracket: *bracket }, A::Span::default());
                        Some(inner.into_iter().map(|c| C::build::binop("*", c, one.clone(), false)).collect())
                    }
                    None => None,
                }
            }
            K::Neg { operand } => self.vector_components(operand, scope, depth + 1)?
                .map(|v| v.into_iter().map(C::build::neg).collect()),
            K::BinOp { op, left, right, .. } => {
                let l = self.vector_components(left, scope, depth + 1)?;
                let r = self.vector_components(right, scope, depth + 1)?;
                match (op.as_str(), l, r) {
                    ("+" | "-", Some(l), Some(r)) if l.len() == r.len() => {
                        Some(l.into_iter().zip(r).map(|(a, b)| C::build::binop(op, a, b, false)).collect())
                    }
                    ("*", None, Some(r)) => {
                        Some(r.into_iter().map(|c| C::build::binop("*", (**left).clone(), c, false)).collect())
                    }
                    ("*", Some(l), None) => {
                        Some(l.into_iter().map(|c| C::build::binop("*", c, (**right).clone(), false)).collect())
                    }
                    ("/", Some(l), None) => {
                        Some(l.into_iter().map(|c| C::build::binop("/", c, (**right).clone(), false)).collect())
                    }
                    _ => None,
                }
            }
            K::Call { func, args } => {
                let info = match &func.kind {
                    K::Name { name } => match self.lookup(scope, name) {
                        Some((Binding::Func(b), _)) if self.funcs[b].one_liner() => Some(b),
                        _ => None,
                    },
                    K::VecCalc { kind, func: g } => {
                        let sym = match kind.as_str() {
                            "grad" => "∇",
                            "div" => "∇·",
                            "curl" => "∇×",
                            _ => "∇²",
                        };
                        let gname = C::to_source(g);
                        match g.name().and_then(|n| self.lookup(scope, n)) {
                            Some((Binding::Func(b), _)) => Some(self.vec_calc_info(b, kind, sym, &gname, g.span)?),
                            _ => {
                                return Err(self.err(format!("{sym} works on a function of the coordinates, like \
                                                             φ(x, y, z) = ..., and {gname} isn't a function"), g.span,
                                                    None))
                            }
                        }
                    }
                    _ => None,
                };
                let Some(info) = info else { return Ok(None) };
                let ps = self.func_params(info);
                if ps.len() != args.len() {
                    return Ok(None);
                }
                let fb = C::inline_where(&self.body_expr(info).unwrap());
                match self.vector_components(&fb, scope, depth + 1)? {
                    None => None,
                    Some(inner) => {
                        let m: HashMap<String, A::Expr> =
                            ps.iter().zip(args).map(|(p, a)| (p.name.clone(), a.clone())).collect();
                        Some(inner.iter().map(|c| C::subst(c, &m)).collect())
                    }
                }
            }
            _ => None,
        })
    }

    // ------------------------------------------------------------ integrals
    fn scalar_lambda(&mut self, base: &str, var: &str, dim: DExpr, ctx: &Ctx) -> (I::LambdaId, Ctx, I::SymId) {
        let lname = self.fresh_name(base);
        self.module.lambdas.push(I::Lambda { kind: I::LambdaKind::Scalar, name: lname, params: vec![], captures: vec![],
                                             locals: vec![], body: vec![], state: vec![], col_syms: vec![],
                                             param_syms: vec![] });
        let lam = self.module.lambdas.len() - 1;
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut parents = ctx.lam_parents.clone();
        if let Some(l) = ctx.lam {
            parents.push(l);
        }
        let lctx = Ctx { func: ctx.func, scope, is_main: false, lam: Some(lam), loop_depth: 0, branch: 0,
                         ret_types: ctx.ret_types, regions: vec![], lam_parents: parents };
        let xs = self.new_sym(var, Ty::Num(dim), &lctx);
        self.module.lambdas[lam].locals.retain(|s| *s != xs);
        self.module.lambdas[lam].params.push(xs);
        self.extra[xs].assigned = true;
        self.bind(scope, var, Binding::Sym(xs));
        (lam, lctx, xs)
    }

    pub fn same_dim(&self, a: &DExpr, b: &DExpr) -> bool {
        let d = self.u.norm(&a.div(b));
        d.is_concrete() && d.konst.is_dimensionless()
    }

    pub fn e_integral(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Integral { integrand, var, lo: Some(lo_e), hi } = &e.kind else {
            return self.indefinite_integral(e, ctx);
        };
        let hi_e = hi.as_ref().unwrap();
        let lo = self.expr(lo_e, ctx)?;
        let hi = match self.expr(hi_e, ctx) {
            Ok(h) => h,
            Err(mut ex) => {
                self.hint_sum_after_integral(e, &lo, &mut ex, ctx);
                return Err(ex);
            }
        };
        self.need_num(&lo, lo_e, "the lower limit")?;
        self.need_num(&hi, hi_e, "the upper limit")?;
        self.limit_division_error(e, &lo, &hi, ctx)?;
        let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
        let hint = if [&**lo_e, &**hi_e].iter().any(|n| n.name() == Some("e")) {
            "e is the elementary charge in Fermium; for Euler's number write exp(1)"
        } else {
            "if you divide or multiply the integral by something, put the integral in parentheses: (∫ ... dx from a to \
             b) / M"
        };
        self.unify_or(&ld, &hd, |s| format!("the limits of this integral are {} and {}; they need the same units",
                                             s.desc(&ld), s.desc(&hd)), e.span, Some(hint.into()))?;
        let (lam, mut lctx, _) = self.scalar_lambda("integrand", var, ld.clone(), ctx);
        let body = self.expr(integrand, &mut lctx)?;
        match &body.ty {
            Ty::Complex(_) => {
                // a complex integrand: one integral for each part (D93)
                return self.component_integral(integrand, ctx, &|part| {
                    mk(A::ExprKind::Integral { integrand: Box::new(part), var: var.clone(), lo: Some(lo_e.clone()),
                                               hi: Some(hi_e.clone()) }, e.span)
                });
            }
            Ty::Vec { n, .. } => {
                let n = *n;
                return self.vector_integral(e, n, ctx).map(Checked::Val);
            }
            _ => {}
        }
        self.need_num(&body, integrand, "the thing being integrated")?;
        let bd = ty_dim(&body.ty).unwrap();
        let sf = minsf(&[&lo, &hi, &body]);
        self.warn_sum_in_limit(e, &bd);
        let xname = self.text(var);
        let mut xf = ir(I::ExprKind::Const(0.0), Ty::Num(ld.clone()), 0);
        xf.hint = lo.hint.clone().or_else(|| hi.hint.clone());
        let xfmt = self.fmt(&xf);
        self.module.lambdas[lam].body = vec![body];
        let mut r = ir(I::ExprKind::Integral { lam, lo: Box::new(lo), hi: Box::new(hi), xname: Some(xname),
                                               xfmt: Some(xfmt), soft: false, atol: None },
                       Ty::Num(bd.mul(&ld)), e.span.line);
        r.sf = sf;
        Ok(Checked::Val(r))
    }

    /// `2 ∫ x dx from 0 to 1 - π` goes up to 1 - π (D173): warn when the other reading also has consistent units.
    fn warn_sum_in_limit(&mut self, e: &A::Expr, body_dim: &DExpr) {
        let Some(Some(info)) = &e.attrs.sum_info else { return };
        if !self.same_dim(body_dim, &DExpr::of(DIMLESS)) {
            return;
        }
        let verb = if info.op == "+" { "add" } else { "subtract" };
        let (line, col) = info.tok;
        self.warn(format!("the ' {}' is part of the upper limit: this integral goes up to {}", info.rest, info.limit),
                  A::Span { line, col, length: 1 },
                  Some(format!("if that's what you meant, write  to ({});  to {verb} it after integrating, write  (… to \
                                {}) {}", info.limit, info.head, info.rest)));
    }

    /// The upper limit `T - x0` is a unit error, but `(∫ … to T) - x0` might be meant: say so (D205).
    fn hint_sum_after_integral(&mut self, e: &A::Expr, lo: &I::Expr, ex: &mut Diagnostic, ctx: &mut Ctx) {
        let Some(Some(info)) = &e.attrs.sum_info else { return };
        let (Some(hn), Some(rn)) = (&info.head_node, &info.rest_node) else { return };
        let (Some(hp), Some(rp)) = (self.node(hn.id).map(|x| x as *const A::Expr), self.node(rn.id).map(|x| x as *const A::Expr))
        else {
            return;
        };
        // SAFETY: nodes of the program outlive the check
        let (hnode, rnode) = unsafe { (&*hp, &*rp) };
        let Ok(head) = self.expr(hnode, ctx) else { return };
        let Ok(rest) = self.expr(rnode, ctx) else { return };
        let (Ty::Num(hd), Ty::Num(rd), Some(ld)) = (&head.ty, &rest.ty, ty_dim(&lo.ty)) else { return };
        if self.same_dim(hd, &ld) && !self.same_dim(rd, &ld) {
            let verb = if info.op == "+" { "add" } else { "subtract" };
            ex.hint = Some(format!("the ' {}' is part of the upper limit here; to {verb} it after integrating, write  (… \
                                    to {}) {}", info.rest, info.head, info.rest));
        }
    }

    /// `∫ P0 dt from 0 s to E / (2 P0)`: the spaced '/' divides the whole integral (D34, D205).
    fn limit_division_error(&mut self, e: &A::Expr, lo: &I::Expr, hi: &I::Expr, ctx: &mut Ctx) -> CResult<()> {
        let Some(info) = &e.attrs.div_info else { return Ok(()) };
        let Some(dvr) = &info.divisor else { return Ok(()) };
        let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
        if self.same_dim(&ld, &hd) {
            return Ok(());
        }
        let Some(dp) = self.node(dvr.id).map(|x| x as *const A::Expr) else { return Ok(()) };
        // SAFETY: nodes of the program outlive the check
        let dnode = unsafe { &*dp };
        let Ok(dv) = self.expr(dnode, ctx) else { return Ok(()) };
        let Ty::Num(dd) = &dv.ty else { return Ok(()) };
        if self.same_dim(&hd.div(dd), &ld) {
            let limit = format!("{} / {}", info.hi_text.clone().unwrap_or_default(), info.div_text.clone().unwrap_or_default());
            return Err(self.err(format!("the limits of this integral are {} and {}: the ' / ' after the upper limit \
                                         divides the whole integral, not the limit", self.desc(&ld), self.desc(&hd)),
                                e.span, Some(format!("to divide the limit, write  to ({limit})"))));
        }
        Ok(())
    }

    /// Σ(body for k from a to b step s) (#49, D51).
    pub fn e_sum(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Sum { body: body_e, var, lo: lo_e, hi: hi_e, step } = &e.kind else { unreachable!() };
        let lo = self.expr(lo_e, ctx)?;
        let hi = self.expr(hi_e, ctx)?;
        self.need_num(&lo, lo_e, "the start of the sum")?;
        self.need_num(&hi, hi_e, "the end of the sum")?;
        let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
        self.unify_or(&ld, &hd, |s| format!("this sum runs from {} to {}; the start and end need the same units",
                                             s.desc(&ld), s.desc(&hd)), e.span, None)?;
        let st = match step {
            Some(se) => {
                let st = self.expr(se, ctx)?;
                self.need_num(&st, se, "the step of the sum")?;
                let sd = ty_dim(&st.ty).unwrap();
                self.unify_or(&ld, &sd, |_| "the step of a sum needs the same units as its start".into(), se.span, None)?;
                st
            }
            None => {
                if !self.u.unify(&ld, &DExpr::of(DIMLESS)) {
                    let hn = lo.hint.as_ref().map(|h| h.name.clone()).unwrap_or_else(|| "m".into());
                    return Err(self.err(format!("this sum runs over {}, so it needs a step, like Σ(… for {var} from a to \
                                                 b step 1 {hn})", self.desc(&ld)), e.span, None));
                }
                ir(I::ExprKind::Const(1.0), Ty::Num(ld.clone()), 0)
            }
        };
        if let I::ExprKind::Const(v) = hi.kind {
            if v.is_infinite() {
                return Err(self.err("a sum needs a finite number of terms", hi_e.span,
                                    Some(format!("sum up to a large fixed number, like Σ(… for {var} from 1 to 1000)"))));
            }
        }
        let (lam, mut lctx, ks) = self.scalar_lambda("term", var, ld.clone(), ctx);
        self.module.syms[ks].sf = None; // a count is exact
        let body = self.expr(body_e, &mut lctx)?;
        match &body.ty {
            Ty::Complex(_) => {
                return self.component_integral(body_e, ctx, &|part| {
                    mk(A::ExprKind::Sum { body: Box::new(part), var: var.clone(), lo: lo_e.clone(), hi: hi_e.clone(),
                                          step: step.clone() }, e.span)
                });
            }
            Ty::Vec { n, .. } => {
                let mut comps = vec![];
                for k in 0..*n {
                    let idx = mk(A::ExprKind::Index { target: body_e.clone(),
                                                      index: Some(Box::new(at(C::build::num((k + 1) as f64), e.span))) },
                                 body_e.span);
                    comps.push(mk(A::ExprKind::Sum { body: Box::new(idx), var: var.clone(), lo: lo_e.clone(),
                                                     hi: hi_e.clone(), step: step.clone() }, e.span));
                }
                return self.expr_any(&mk(A::ExprKind::VecLit { items: comps }, e.span), ctx);
            }
            _ => {}
        }
        self.need_num(&body, body_e, "each term of a sum")?;
        let (hint, sf) = (body.hint.clone(), body.sf);
        let bd = ty_dim(&body.ty).unwrap();
        self.module.lambdas[lam].body = vec![body];
        let mut r = ir(I::ExprKind::Sum { lam, lo: Box::new(lo), hi: Box::new(hi), step: Some(Box::new(st)) },
                       Ty::Num(bd), e.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(Checked::Val(r))
    }

    /// ∫ f dx for a complex integrand: ∫ re(f) dx + i ∫ im(f) dx (D93; Python cplx.component_integral).
    fn component_integral(&mut self, f: &A::Expr, ctx: &mut Ctx, make: &dyn Fn(A::Expr) -> A::Expr)
                          -> CResult<Checked> {
        let mut parts = vec![];
        for part in ["re", "im"] {
            let fld = mk(A::ExprKind::Field { target: Box::new(f.clone()), name: part.into() }, f.span);
            parts.push(self.expr(&make(fld), ctx)?);
        }
        let (d0, d1) = (ty_dim(&parts[0].ty).unwrap(), ty_dim(&parts[1].ty).unwrap());
        self.u.unify(&d0, &d1);
        let hint = parts[0].hint.clone();
        let sf = minsf(&[&parts[0], &parts[1]]);
        let line = parts[0].line;
        let mut r = ir(I::ExprKind::Vec(parts), Ty::Complex(d0), line);
        r.hint = hint;
        r.sf = sf;
        Ok(Checked::Val(r))
    }

    /// ∫ <f, g, h> ds from a to b = <∫ f ds, ∫ g ds, ∫ h ds> (D35).
    fn vector_integral(&mut self, e: &A::Expr, n: usize, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Integral { integrand, var, lo, hi } = &e.kind else { unreachable!() };
        let mut comps = vec![];
        for k in 0..n {
            let idx = mk(A::ExprKind::Index { target: integrand.clone(),
                                              index: Some(Box::new(at(C::build::num((k + 1) as f64), e.span))) },
                         integrand.span);
            comps.push(mk(A::ExprKind::Integral { integrand: Box::new(idx), var: var.clone(), lo: lo.clone(),
                                                  hi: hi.clone() }, e.span));
        }
        let r = self.expr(&mk(A::ExprKind::VecLit { items: comps }, e.span), ctx)?;
        self.vector_integral_retry(r, ctx)
    }

    /// Each component is tried quietly first; one that failed is computed again with an absolute tolerance of
    /// 10⁻¹⁰ × Σ|other components| (D44), and one warning if every component was 0 at every sample (D110).
    fn vector_integral_retry(&mut self, r: I::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let I::ExprKind::Vec(items) = &r.kind else { return Ok(r) };
        let mixed = matches!(r.ty, Ty::Vec { dims: Some(_), .. });
        if mixed || !items.iter().all(|it| matches!(it.kind, I::ExprKind::Integral { .. })) {
            return Ok(r);
        }
        let line = r.line;
        let dl = || Ty::Num(DExpr::of(DIMLESS));
        let mut binds = vec![];
        let mname = self.fresh_name("__vint");
        let mark = self.new_sym(&mname, dl(), ctx);
        self.extra[mark].assigned = true;
        binds.push((mark, ir(I::ExprKind::Builtin("qzero_mark".into(), vec![]), dl(), line)));
        let mut total: Option<I::Expr> = None;
        let mut syms = vec![];
        for it in items {
            let sname = self.fresh_name("__vint");
            let sym = self.new_sym(&sname, it.ty.clone(), ctx);
            self.extra[sym].assigned = true;
            let mut first = it.clone();
            if let I::ExprKind::Integral { soft, atol, .. } = &mut first.kind {
                *soft = true;
                *atol = None;
            }
            binds.push((sym, first));
            syms.push(sym);
            let v = ir(I::ExprKind::Var(sym), it.ty.clone(), line);
            let size = ir(I::ExprKind::If(Box::new(ir(I::ExprKind::Cmp(I::CmpOp::Eq, Box::new(v.clone()), Box::new(v.clone())),
                                                     Ty::Bool, line)),
                                          Box::new(ir(I::ExprKind::Builtin("abs".into(), vec![v]), it.ty.clone(), line)),
                                          Box::new(ir(I::ExprKind::Const(0.0), it.ty.clone(), line))),
                          it.ty.clone(), line);
            total = Some(match total {
                None => size,
                Some(t) => ir(I::ExprKind::Bin(I::BinOp::Add, Box::new(t), Box::new(size)), it.ty.clone(), line),
            });
        }
        let cname = self.fresh_name("__vint");
        let check = self.new_sym(&cname, dl(), ctx);
        self.extra[check].assigned = true;
        let nitems = items.len() as f64;
        binds.push((check, ir(I::ExprKind::Builtin("qzero_check".into(),
                                                   vec![ir(I::ExprKind::Var(mark), dl(), line),
                                                        ir(I::ExprKind::Const(nitems), dl(), line)]), dl(), line)));
        let total = total.unwrap();
        let mut out_items = vec![];
        for (sym, it) in syms.iter().zip(items) {
            let mut it = it.clone();
            let t = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(ir(I::ExprKind::Const(1e-10), dl(), line)),
                                        Box::new(total.clone())), it.ty.clone(), line);
            if let I::ExprKind::Integral { atol, .. } = &mut it.kind {
                *atol = Some(Box::new(t));
            }
            let v = ir(I::ExprKind::Var(*sym), it.ty.clone(), line);
            let ty = it.ty.clone();
            out_items.push(ir(I::ExprKind::If(Box::new(ir(I::ExprKind::Cmp(I::CmpOp::Eq, Box::new(v.clone()),
                                                                           Box::new(v.clone())), Ty::Bool, line)),
                                              Box::new(v), Box::new(it)), ty, line));
        }
        let mut out = ir(I::ExprKind::Let(binds, Box::new(ir(I::ExprKind::Vec(out_items), r.ty.clone(), line))),
                         r.ty.clone(), line);
        out.hint = r.hint.clone();
        out.sf = r.sf;
        Ok(out)
    }

    /// ∫ f dx without limits: an antiderivative, found natively (v1 asked SymPy; rust/DIVERGENCES.md).
    fn indefinite_integral(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Integral { integrand, var, .. } = &e.kind else { unreachable!() };
        let inlined = self.inline_calls(integrand, ctx.scope);
        let integrand2 = C::inline_where(&inlined);
        // undefined names first; and which names are safe to assume > 0 (D37)
        let mut positive: Vec<String> = vec![];
        for node in name_uses(&integrand2) {
            let n = node.name().unwrap();
            if n == var || n == "π" {
                continue;
            }
            match self.lookup(ctx.scope, n) {
                None => {
                    let at_node = if node.span.line != 0 { node } else { e };
                    return Err(self.undefined(n, at_node, ctx));
                }
                Some((Binding::Const(c), _)) => {
                    if n != "∞" && c.value > 0.0 && !positive.iter().any(|p| p == n) {
                        positive.push(n.to_string());
                    }
                }
                Some((Binding::Sym(_), sc)) => {
                    if sc == self.globals && self.positive_names.contains(n) && !positive.iter().any(|p| p == n) {
                        positive.push(n.to_string());
                    }
                }
                _ => {}
            }
        }
        let body = match C::integrate(&integrand2, var, &positive) {
            Ok(b) => b,
            Err(mut ex) => {
                if ex.line.is_none() {
                    ex.line = Some(e.span.line).filter(|l| *l != 0);
                    ex.col = Some(e.span.col).filter(|c| *c != 0);
                    ex.length = 1;
                }
                return Err(ex);
            }
        };
        let fd = new_fdef(&format!("∫d{var}"), vec![param(var)], body, e.span);
        let scope = if ctx.is_main { ctx.scope } else { self.globals };
        let fname = self.fresh_name("antideriv");
        let info = self.new_func_info(fname, fd, scope);
        let nat = if self.nat.is_empty() { None } else { Some(self.nat.clone()) };
        let f = &mut self.funcs[info];
        f.nat = nat;
        f.display_name = format!("∫d{var}");
        f.anon_label = Some(format!("∫ {} d{var}", C::to_source(integrand)));
        Ok(self.func_ref(info))
    }

    /// Inline calls to one-line user functions so the integrator sees a plain formula.
    fn inline_calls(&mut self, n: &A::Expr, scope: ScopeId) -> A::Expr {
        let n = C::map_children(n, &mut |c| self.inline_calls(c, scope));
        if let A::ExprKind::Call { func, args } = &n.kind {
            if let Some(fname) = func.name() {
                if let Some((Binding::Func(b), _)) = self.lookup(scope, fname) {
                    if self.funcs[b].one_liner() {
                        let m: HashMap<String, A::Expr> = self
                            .func_params(b)
                            .iter()
                            .zip(args)
                            .map(|(p, a)| (p.name.clone(), a.clone()))
                            .collect();
                        return C::subst(&C::inline_where(&self.body_expr(b).unwrap()), &m);
                    }
                }
            }
        }
        n
    }

    /// dx/dt written as a fraction: a derivative when dx and dt aren't variables but x is a function.
    pub fn leibniz(&mut self, e: &A::Expr, ctx: &mut Ctx) -> Option<A::Expr> {
        let A::ExprKind::BinOp { op, left, right, .. } = &e.kind else { return None };
        let (mut rt, mut args) = (&**right, None);
        if let A::ExprKind::Call { func, args: a } = &right.kind {
            if func.is_name() {
                rt = func;
                args = Some(a.clone());
            }
        }
        if op != "/" || left.paren {
            return None;
        }
        let (Some(ln), Some(rn)) = (left.name(), rt.name()) else { return None };
        if ln.chars().count() > 1 && rn.chars().count() > 1 && ln.starts_with('d') && rn.starts_with('d')
            && self.lookup(ctx.scope, ln).is_none() && self.lookup(ctx.scope, rn).is_none()
        {
            let x = crate::names::canonical_name(&ln[1..]);
            let t = crate::names::canonical_name(&rn[1..]);
            if matches!(self.lookup(ctx.scope, &x), Some((Binding::Func(_) | Binding::Sol(_), _))) {
                let d = mk(A::ExprKind::Deriv { var: t, order: 1, operand: Box::new(at(C::build::name(&x), left.span)),
                                                partial: false }, e.span);
                return Some(match args {
                    Some(a) => mk(A::ExprKind::Call { func: Box::new(d), args: a }, e.span),
                    None => d,
                });
            }
        }
        None
    }

    /// `solve lhs = rhs for x from a to b` (D32; Python solve.check_root): the first x in [a, b] where the two
    /// sides are equal, stored in x.
    pub fn check_root(&mut self, s: &A::Stmt, sv: &A::Solve, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        if sv.step.is_some() || sv.method.is_some() || sv.tolerance.is_some()
            || sv.absolute.as_ref().is_some_and(|a| !a.is_empty())
        {
            return Err(self.err("step, tolerance, absolute and using are for differential equations; an equation is \
                                 solved to full precision", s.span, None));
        }
        let x = sv.var.as_str();
        let lo = self.expr(&sv.lo, ctx)?;
        let hi = self.expr(&sv.hi, ctx)?;
        self.need_num(&lo, &sv.lo, "the start of the search range")?;
        self.need_num(&hi, &sv.hi, "the end of the search range")?;
        let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
        self.unify_or(&ld, &hd, |c| format!("the search range goes from {} to {}; both ends need the same units",
                                             c.desc(&ld), c.desc(&hd)), sv.lo.span, None)?;
        let (lam, mut lctx, _) = self.scalar_lambda("root", x, ld.clone(), ctx);
        let q = &sv.equations[0];
        let left = self.expr(&q.lhs, &mut lctx)?;
        let right = self.expr(&q.rhs, &mut lctx)?;
        self.need_num(&left, &q.lhs, "the left side")?;
        self.need_num(&right, &q.rhs, "the right side")?;
        let (a, b) = (ty_dim(&left.ty).unwrap(), ty_dim(&right.ty).unwrap());
        self.unify_or(&a, &b, |c| format!("the two sides of this equation don't match: left is {}, right is {}",
                                           c.desc(&a), c.desc(&b)), q.span, None)?;
        let qnode = mk(A::ExprKind::Name { name: String::new() }, q.span);
        let body = self.arith("-", left.clone(), right.clone(), &qnode)?;
        self.module.lambdas[lam].body = vec![body];
        // the size of the terms added up on the two sides (|A| + |B| + |C| for A - B = C), to notice a root in
        // rounding noise (#36); it shares the equation's argument and env
        let sdim = Ty::Num(a.clone());
        fn terms(e: &I::Expr, sdim: &Ty) -> I::Expr {
            match &e.kind {
                I::ExprKind::Bin(op @ (I::BinOp::Add | I::BinOp::Sub), x, y) if matches!(e.ty, Ty::Num(_)) => {
                    let _ = op;
                    ir(I::ExprKind::Bin(I::BinOp::Add, Box::new(terms(x, sdim)), Box::new(terms(y, sdim))), sdim.clone(),
                       e.line)
                }
                I::ExprKind::Neg(x) => terms(x, sdim),
                _ => ir(I::ExprKind::Builtin("abs".into(), vec![e.clone()]), e.ty.clone(), e.line),
            }
        }
        let sbody = ir(I::ExprKind::Bin(I::BinOp::Add, Box::new(terms(&left, &sdim)), Box::new(terms(&right, &sdim))),
                       sdim.clone(), q.span.line);
        let mut slam = self.module.lambdas[lam].clone();
        slam.name = self.fresh_name("rootscale");
        slam.body = vec![sbody];
        self.module.lambdas.push(slam);
        let sl = self.module.lambdas.len() - 1;
        let hint = lo.hint.clone().or_else(|| hi.hint.clone());
        let mut tf = ir(I::ExprKind::Const(0.0), Ty::Num(ld.clone()), 0);
        tf.hint = hint.clone();
        let tfmt = self.fmt(&tf);
        let mut r = ir(I::ExprKind::Root { lam, lo: Box::new(lo), hi: Box::new(hi), scale: Some(sl), tfmt: Some(tfmt) },
                       Ty::Num(ld), s.span.line);
        r.hint = hint;
        r.sf = None;
        Ok(vec![self.assign_to(x, r, s.span, None, ctx)?])
    }

    /// `g = d/dt (a t^2) where a = 3`: substitute, so the derivative can still be a function of t (e_Where).
    pub fn where_deriv(&mut self, e: &A::Expr, value: &A::Expr, b: &[(String, A::Expr)], ctx: &mut Ctx)
                       -> Option<CResult<Checked>> {
        let A::ExprKind::Deriv { var, .. } = &value.kind else { return None };
        if b.iter().any(|(n, v)| n == var || C::depends_on(v, var)) {
            return None;
        }
        Some(self.expr_any(&C::inline_where(e), ctx))
    }

    /// C.stabilize: a numerically stable form of a derivative's body (A52).
    pub fn stabilize(&self, e: &A::Expr) -> A::Expr {
        C::stabilize(e)
    }

    /// The units of a function for printing it (Python function_units).
    pub fn function_units(&mut self, info: FuncInfoId) -> String {
        let fresh_args = |n: usize| -> Vec<Checked> {
            (0..n).map(|_| Checked::Val(ir(I::ExprKind::Const(1.0), Ty::Num(DExpr::fresh()), 0))).collect()
        };
        let dims = |args: &[Checked]| -> Vec<DExpr> {
            args.iter().map(|a| match a {
                Checked::Val(v) => ty_dim(&v.ty).unwrap(),
                _ => DExpr::of(DIMLESS),
            }).collect()
        };
        let pref = |d: &fermium_ir::Dim| -> String { fermium_units::preferred_unit(d).name };
        if let Some((base, i, order)) = self.funcs[info].parent {
            let params = self.func_params(base);
            let args = fresh_args(params.len());
            let ds = dims(&args);
            let span = self.funcs[base].fdef.as_ref().map(|f| f.span).unwrap_or_default();
            let node = mk(A::ExprKind::Name { name: String::new() }, span);
            if let Ok(call) = self.instantiate(base, args, &node, false) {
                if let Ty::Num(cd) = &call.ty {
                    let res = self.u.norm(&cd.div(&ds[i].pow(Rational64::from_integer(order as i64))));
                    let pds: Vec<DExpr> = ds.iter().map(|d| self.u.norm(d)).collect();
                    if res.is_concrete() && pds.iter().all(|d| d.is_concrete()) {
                        if res.konst.is_dimensionless() && pds.iter().all(|d| d.konst.is_dimensionless()) {
                            return String::new();
                        }
                        let mut parts = vec![non_empty(pref(&res.konst), "no units")];
                        for (p, d) in params.iter().zip(&pds) {
                            parts.push(format!("for {} in {}", p.name, non_empty(pref(&d.konst), "plain numbers")));
                        }
                        return parts.join(", ");
                    }
                }
            }
        }
        let params = self.func_params(info);
        let args = fresh_args(params.len());
        let ds = dims(&args);
        let span = self.funcs[info].fdef.as_ref().map(|f| f.span).unwrap_or_default();
        let node = mk(A::ExprKind::Name { name: String::new() }, span);
        let Ok(call) = self.instantiate(info, args, &node, false) else { return String::new() };
        let Ty::Num(cd) = &call.ty else { return String::new() };
        let res = self.u.norm(cd);
        if !res.is_concrete() {
            return String::new();
        }
        let mut parts = vec![non_empty(pref(&res.konst), "no units")];
        let mut all_plain = res.konst.is_dimensionless();
        for (p, d) in params.iter().zip(&ds) {
            let d = self.u.norm(d);
            if d.is_concrete() {
                all_plain = all_plain && d.konst.is_dimensionless();
                parts.push(format!("for {} in {}", p.name, non_empty(pref(&d.konst), "plain numbers")));
            }
        }
        if all_plain {
            return String::new();
        }
        parts.join(", ")
    }
}

fn non_empty(s: String, dflt: &str) -> String {
    if s.is_empty() { dflt.to_string() } else { s }
}

/// The Name nodes an expression reads (not function names being called, nor bound variables) (Python _name_uses).
pub fn name_uses(e: &A::Expr) -> Vec<&A::Expr> {
    fn walk<'a>(n: &'a A::Expr, bound: &HashSet<String>, out: &mut Vec<&'a A::Expr>) {
        use A::ExprKind as K;
        match &n.kind {
            K::Name { name } => {
                if !bound.contains(name) {
                    out.push(n);
                }
            }
            K::Call { func, args } => {
                if !func.is_name() {
                    walk(func, bound, out);
                }
                for a in args {
                    walk(a, bound, out);
                }
            }
            K::Integral { integrand, var, lo, hi } => {
                let mut b = bound.clone();
                b.insert(var.clone());
                walk(integrand, &b, out);
                for x in lo.iter().chain(hi.iter()) {
                    walk(x, bound, out);
                }
            }
            K::Sum { body, var, lo, hi, step } => {
                let mut b = bound.clone();
                b.insert(var.clone());
                walk(body, &b, out);
                walk(lo, bound, out);
                walk(hi, bound, out);
                for x in step.iter() {
                    walk(x, bound, out);
                }
            }
            _ => {
                for c in n.children() {
                    walk(c, bound, out);
                }
            }
        }
    }
    let mut out = vec![];
    walk(e, &HashSet::new(), &mut out);
    out
}

fn positive_literal(v: &A::Expr) -> bool {
    use A::ExprKind as K;
    match &v.kind {
        K::Num { value, .. } => *value > 0.0,
        K::Quantity { value, .. } => positive_literal(value),
        K::BinOp { op, left, right, .. } if matches!(op.as_str(), "*" | "/" | "^") => {
            if op == "^" && !right.is_num() {
                return false;
            }
            positive_literal(left) && (op == "^" || positive_literal(right))
        }
        K::Sqrt { operand, .. } => positive_literal(operand),
        _ => false,
    }
}

/// Program variables safe to assume positive in a symbolic integral (D37; Python _positive_names).
pub fn positive_names(prog: &A::Program) -> HashSet<String> {
    let mut ok = HashSet::new();
    let mut bad = HashSet::new();
    fn all_names(s: &A::Stmt, out: &mut HashSet<String>) {
        let mut exprs = vec![];
        crate::walk::all_exprs_in_stmts(std::slice::from_ref(s), &mut exprs);
        for e in exprs {
            for n in e.walk() {
                if let Some(x) = n.name() {
                    out.insert(x.to_string());
                }
                match &n.kind {
                    A::ExprKind::Integral { var, .. } | A::ExprKind::Sum { var, .. } => {
                        out.insert(var.clone());
                    }
                    A::ExprKind::Field { name, .. } | A::ExprKind::Deriv { var: name, .. } => {
                        out.insert(name.clone());
                    }
                    _ => {}
                }
            }
        }
        match &s.kind {
            A::StmtKind::Solve(sv) => {
                out.insert(sv.var.clone());
                if let Some(v) = &sv.var2 {
                    out.insert(v.clone());
                }
            }
            A::StmtKind::Fit { guesses, .. } => {
                for (g, _) in guesses {
                    out.insert(g.clone());
                }
            }
            _ => {}
        }
    }
    fn walk(stmts: &[A::Stmt], ok: &mut HashSet<String>, bad: &mut HashSet<String>) {
        for s in stmts {
            match &s.kind {
                A::StmtKind::Assign { name, value, op } => {
                    if op == "=" && positive_literal(value) {
                        ok.insert(name.clone());
                    } else {
                        bad.insert(name.clone());
                    }
                }
                A::StmtKind::For { var, .. } | A::StmtKind::ForIn { var, .. } => {
                    bad.insert(var.clone());
                }
                A::StmtKind::IndexAssign { target, .. } => {
                    bad.insert(target.clone());
                }
                A::StmtKind::FuncDef { params, .. } => {
                    for p in params {
                        bad.insert(p.name.clone());
                    }
                }
                A::StmtKind::Solve(_) | A::StmtKind::Fit { .. } => all_names(s, bad),
                _ => {}
            }
            for b in crate::walk::stmt_blocks(s) {
                walk(b, ok, bad);
            }
        }
    }
    walk(&prog.body, &mut ok, &mut bad);
    ok.retain(|n| !bad.contains(n));
    ok
}
