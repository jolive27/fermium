//! Derivatives of multi-line functions by automatic differentiation (spec §C2, Fermium 2.5; DECISIONS D295).
//!
//! Forward mode as a source transformation: each local variable `y` that depends on the variable of
//! differentiation `x` gets a tangent `dy/dx`, assigned just before `y` is (from the values before the
//! assignment), by the chain rule over the local names:
//!
//! ```text
//! y = e      →   dy/dx = Σ_v ∂e/∂v · dv/dx     (v: x and the locals that depend on x)
//!                y = e
//! return e   →   return Σ_v ∂e/∂v · dv/dx
//! ```
//!
//! The partial derivatives ∂e/∂v are the symbolic ones of fermium-sym (`diff`), so calls to built-ins, to one-line
//! functions and (through the context) to other multi-line functions are differentiated as in one-line
//! functions. Control flow is kept as it is: `if`/`else` differentiate each branch (a piecewise function has the
//! piecewise derivative), loops run the same iterations with the tangents updated alongside, and a loop counter is
//! a constant (the number of iterations changes in steps, so it contributes nothing to the derivative). `print`
//! statements are dropped from the derivative. Lists, `solve`, `plot` and the like are refused with an error.
//! The result is again a multi-line function body, so a second derivative applies the transformation again.
use std::collections::{HashMap, HashSet};

use fermium_syntax::ast as A;

use crate::build::*;
use crate::diff::{diff, DiffContext};
use crate::simplify::simplify;
use crate::walk::{free_names, map_children, subst};

type K = A::ExprKind;
type S = A::StmtKind;

/// The body of d(f)/d(var) for a multi-line function f(params) = body.
pub fn ad_body(fname: &str, params: &[String], body: &[A::Stmt], var: &str, ctx: &mut dyn DiffContext)
               -> SymResult<Vec<A::Stmt>> {
    let mut assigns = vec![];
    collect_assigns(body, &mut assigns);
    let mut used = HashSet::new();
    collect_names(body, &mut used);
    used.extend(params.iter().cloned());
    // the locals (and parameters) that depend on var, to a fixed point
    let mut active: HashSet<String> = HashSet::from([var.to_string()]);
    loop {
        let before = active.len();
        for (n, v) in &assigns {
            if !active.contains(n) && free_names(v).iter().any(|x| active.contains(x)) {
                active.insert(n.clone());
            }
        }
        if active.len() == before {
            break;
        }
    }
    let reassigned: HashSet<&String> = assigns.iter().map(|(n, _)| n).collect();
    let mut tan: HashMap<String, Option<String>> = HashMap::new();
    let mut ordered: Vec<&String> = active.iter().collect();
    ordered.sort();
    for a in ordered {
        if a == var && !reassigned.contains(a) {
            tan.insert(a.clone(), None); // d var/d var = 1 throughout
            continue;
        }
        let mut t = format!("d{a}/d{var}");
        while used.contains(&t) {
            t.push('\'');
        }
        used.insert(t.clone());
        tan.insert(a.clone(), Some(t));
    }
    let mut ad = Ad { fname, var, tan, helpers: HashMap::new(), ctx };
    let mut out = vec![];
    // parameters that are reassigned start with their tangent: 1 for var, 0 for the others
    for p in params {
        if let Some(Some(t)) = ad.tan.get(p) {
            let v = num(if p == var { 1.0 } else { 0.0 });
            out.push(A::Stmt { kind: S::Assign { name: t.clone(), value: v, op: "=".into() },
                               span: body.first().map(|s| s.span).unwrap_or_default() });
        }
    }
    let n = body.len();
    for (i, s) in body.iter().enumerate() {
        ad.stmt(s, i + 1 == n, &mut out)?;
    }
    Ok(prune(out))
}

/// Remove assignments to names that nothing reads (the derivative doesn't need the tangents of variables that
/// don't reach the result, nor, in a second derivative, the first derivative's own values), to a fixed point.
fn prune(mut body: Vec<A::Stmt>) -> Vec<A::Stmt> {
    loop {
        let mut reads = HashSet::new();
        collect_reads(&body, &mut reads);
        let mut changed = false;
        body = prune_with(body, &reads, &mut changed);
        if !changed {
            return body;
        }
    }
}

fn expr_reads(e: &A::Expr, out: &mut HashSet<String>) {
    out.extend(free_names(e));
}

/// Names read anywhere, except an assignment's reads of its own target (`s += k` alone doesn't keep s alive).
fn collect_reads(body: &[A::Stmt], out: &mut HashSet<String>) {
    for s in body {
        match &s.kind {
            S::Assign { name, value, .. } => {
                for n in free_names(value) {
                    if &n != name {
                        out.insert(n);
                    }
                }
            }
            S::IndexAssign { target, index, value, index2, .. } => {
                out.insert(target.clone());
                expr_reads(index, out);
                expr_reads(value, out);
                if let Some(i) = index2 {
                    expr_reads(i, out);
                }
            }
            S::FuncDef { body: A::FuncBody::Expr(b), where_, .. } => {
                expr_reads(b, out);
                for (_, v) in where_ {
                    expr_reads(v, out);
                }
            }
            S::FuncDef { .. } => {}
            S::Print { items } => items.iter().for_each(|e| expr_reads(e, out)),
            S::If { cond, then, other } => {
                expr_reads(cond, out);
                collect_reads(then, out);
                if let Some(o) = other {
                    collect_reads(o, out);
                }
            }
            S::For { lo, hi, step, body, .. } => {
                expr_reads(lo, out);
                expr_reads(hi, out);
                if let Some(st) = step {
                    expr_reads(st, out);
                }
                collect_reads(body, out);
            }
            S::ForIn { iterable, body, .. } => {
                expr_reads(iterable, out);
                collect_reads(body, out);
            }
            S::While { cond, body } => {
                expr_reads(cond, out);
                collect_reads(body, out);
            }
            S::Return { value: Some(e) } | S::ExprStmt { value: e } => expr_reads(e, out),
            S::Assert { cond, .. } => expr_reads(cond, out),
            _ => {}
        }
    }
}

fn prune_with(body: Vec<A::Stmt>, reads: &HashSet<String>, changed: &mut bool) -> Vec<A::Stmt> {
    let mut out = vec![];
    for mut s in body {
        match &mut s.kind {
            S::Assign { name, .. } if !reads.contains(name.as_str()) => {
                *changed = true;
                continue;
            }
            S::If { then, other, .. } => {
                *then = prune_with(std::mem::take(then), reads, changed);
                if let Some(o) = other {
                    *o = prune_with(std::mem::take(o), reads, changed);
                    if o.is_empty() {
                        *other = None;
                    }
                }
                if then.is_empty() && other.is_none() {
                    *changed = true;
                    continue;
                }
            }
            S::For { body, .. } | S::ForIn { body, .. } | S::While { body, .. } => {
                *body = prune_with(std::mem::take(body), reads, changed);
            }
            _ => {}
        }
        out.push(s);
    }
    out
}

fn collect_assigns(body: &[A::Stmt], out: &mut Vec<(String, A::Expr)>) {
    for s in body {
        match &s.kind {
            S::Assign { name, value, op } => out.push((name.clone(), full_value(name, value, op))),
            S::If { then, other, .. } => {
                collect_assigns(then, out);
                if let Some(o) = other {
                    collect_assigns(o, out);
                }
            }
            S::For { body, .. } | S::ForIn { body, .. } | S::While { body, .. } => collect_assigns(body, out),
            _ => {}
        }
    }
}

fn collect_names(body: &[A::Stmt], out: &mut HashSet<String>) {
    for s in body {
        match &s.kind {
            S::Assign { name, .. } => {
                out.insert(name.clone());
            }
            S::FuncDef { name, .. } => {
                out.insert(name.clone());
            }
            S::For { var, body, .. } | S::ForIn { var, body, .. } => {
                out.insert(var.clone());
                collect_names(body, out);
            }
            S::If { then, other, .. } => {
                collect_names(then, out);
                if let Some(o) = other {
                    collect_names(o, out);
                }
            }
            S::While { body, .. } => collect_names(body, out),
            _ => {}
        }
    }
}

/// `y += e` is `y = y + e`.
fn full_value(name: &str, value: &A::Expr, op: &str) -> A::Expr {
    let y = || crate::build::name(name);
    match op {
        "+=" => add(y(), value.clone()),
        "-=" => sub(y(), value.clone()),
        "*=" => mul(y(), value.clone()),
        "/=" => div(y(), value.clone()),
        _ => value.clone(),
    }
}

struct Ad<'a> {
    fname: &'a str,
    var: &'a str,
    /// active name → its tangent variable (None: the variable of differentiation, whose tangent is 1)
    tan: HashMap<String, Option<String>>,
    /// one-line helpers defined in the body: name → (parameters, body); inlined before differentiating
    helpers: HashMap<String, (Vec<String>, A::Expr)>,
    ctx: &'a mut dyn DiffContext,
}

impl Ad<'_> {
    fn inline(&self, e: &A::Expr) -> A::Expr {
        if self.helpers.is_empty() {
            return e.clone();
        }
        let e = map_children(e, &mut |c| self.inline(c));
        if let K::Call { func, args } = &e.kind {
            if let Some((ps, b)) = func.name().and_then(|n| self.helpers.get(n)) {
                if ps.len() == args.len() {
                    let m: HashMap<String, A::Expr> = ps.iter().cloned().zip(args.iter().cloned()).collect();
                    return subst(b, &m);
                }
            }
        }
        e
    }

    /// Σ_v ∂e/∂v · dv/dx over the active names v in e.
    fn tangent(&mut self, e: &A::Expr) -> SymResult<A::Expr> {
        let e = self.inline(e);
        let mut seen = HashSet::new();
        let mut out: Option<A::Expr> = None;
        for v in free_names(&e) {
            if !seen.insert(v.clone()) {
                continue;
            }
            let Some(t) = self.tan.get(&v).cloned() else { continue };
            let c = diff(&e, &v, self.ctx).map_err(|mut ex| {
                if ex.line.is_none() && e.span.line != 0 {
                    ex.line = Some(e.span.line);
                    ex.col = Some(e.span.col).filter(|c| *c != 0);
                }
                ex
            })?;
            if is_num_v(&c, 0.0) {
                continue;
            }
            let term = match t {
                None => c,
                Some(t) => mul(c, name(&t)),
            };
            out = Some(match out {
                None => term,
                Some(o) => add(o, term),
            });
        }
        Ok(out.map(|o| simplify(&o)).unwrap_or_else(|| num(0.0)))
    }

    fn block(&mut self, body: &[A::Stmt]) -> SymResult<Vec<A::Stmt>> {
        let mut out = vec![];
        for s in body {
            self.stmt(s, false, &mut out)?;
        }
        Ok(out)
    }

    fn stmt(&mut self, s: &A::Stmt, last: bool, out: &mut Vec<A::Stmt>) -> SymResult<()> {
        let sp = s.span;
        let st = |kind: S| A::Stmt { kind, span: sp };
        match &s.kind {
            S::Assign { name, value, op } => {
                if let Some(t) = self.tan.get(name).cloned() {
                    let full = full_value(name, value, op);
                    let dv = self.tangent(&full)?;
                    // the variable of differentiation itself reassigned: it has a tangent variable (see ad_body)
                    let t = t.expect("reassigned names have a tangent variable");
                    out.push(st(S::Assign { name: t, value: dv, op: "=".into() }));
                }
                out.push(s.clone());
            }
            S::IndexAssign { target, index, value, index2, .. } => {
                let mut names = free_names(value);
                names.extend(free_names(index));
                if let Some(i2) = index2 {
                    names.extend(free_names(i2));
                }
                if names.iter().any(|n| self.tan.contains_key(n)) {
                    return Err(ferr(format!("can't differentiate {} automatically: it changes an element of the \
                                             list {target} with a value that depends on {}", self.fname, self.var),
                                    sp, Some("lists aren't differentiated yet; compute with numbers, or use a finite \
                                              difference, (f(x + h) - f(x - h)) / (2h)".into())));
                }
                out.push(s.clone());
            }
            S::FuncDef { name, params, body: A::FuncBody::Expr(b), where_ } => {
                let b = if where_.is_empty() {
                    b.clone()
                } else {
                    crate::walk::inline_where(&with_kind(b, K::Where { value: Box::new(b.clone()),
                                                                       bindings: where_.clone() }))
                };
                let b = self.inline(&b);
                self.helpers.insert(name.clone(), (params.iter().map(|p| p.name.clone()).collect(), b));
                out.push(s.clone());
            }
            S::Print { .. } => {} // the derivative doesn't print
            S::Assert { .. } | S::Break | S::Continue => out.push(s.clone()),
            S::If { cond, then, other } => {
                let then = self.block(then)?;
                let other = match other {
                    Some(o) => Some(self.block(o)?),
                    None => None,
                };
                out.push(st(S::If { cond: cond.clone(), then, other }));
            }
            S::For { var, lo, hi, step, body, parallel } => {
                let body = self.block(body)?;
                out.push(st(S::For { var: var.clone(), lo: lo.clone(), hi: hi.clone(), step: step.clone(), body,
                                     parallel: *parallel }));
            }
            S::ForIn { var, iterable, body } => {
                if free_names(iterable).iter().any(|n| self.tan.contains_key(n)) {
                    return Err(ferr(format!("can't differentiate {} automatically: its loop over {} depends on {}",
                                            self.fname, crate::source::to_source(iterable), self.var),
                                    sp, Some("lists aren't differentiated yet; use a finite difference, \
                                              (f(x + h) - f(x - h)) / (2h)".into())));
                }
                let body = self.block(body)?;
                out.push(st(S::ForIn { var: var.clone(), iterable: iterable.clone(), body }));
            }
            S::While { cond, body } => {
                let body = self.block(body)?;
                out.push(st(S::While { cond: cond.clone(), body }));
            }
            S::Return { value: Some(v) } => {
                let dv = self.tangent(v)?;
                out.push(st(S::Return { value: Some(dv) }));
            }
            S::ExprStmt { value } if last => {
                let dv = self.tangent(value)?;
                out.push(st(S::ExprStmt { value: dv }));
            }
            S::ExprStmt { .. } => out.push(s.clone()),
            other => {
                let what = match other {
                    S::Return { value: None } => "a return without a value",
                    S::Solve(_) => "solve",
                    S::Plot { .. } => "plot",
                    S::Fit { .. } => "fit",
                    S::Propagate { .. } => "propagate",
                    S::Units { .. } => "a units block",
                    _ => "a statement",
                };
                return Err(ferr(format!("can't differentiate {} automatically: it uses {what}", self.fname), sp,
                                Some("automatic differentiation goes through assignments, if, for and while; use a \
                                      finite difference, (f(x + h) - f(x - h)) / (2h)".into())));
            }
        }
        Ok(())
    }
}

/// The statements as Fermium source, indented by `indent` levels (for tests and for reading a derivative's body).
pub fn block_source(body: &[A::Stmt], indent: usize) -> String {
    use crate::source::to_source;
    let pad = "    ".repeat(indent);
    let mut out = String::new();
    for s in body {
        let line = match &s.kind {
            S::Assign { name, value, op } => format!("{name} {op} {}", to_source(value)),
            S::Return { value: Some(v) } => format!("return {}", to_source(v)),
            S::Return { value: None } => "return".into(),
            S::ExprStmt { value } => to_source(value),
            S::Print { items } => format!("print {}", items.iter().map(to_source).collect::<Vec<_>>().join(", ")),
            S::Break => "break".into(),
            S::Continue => "continue".into(),
            S::If { cond, then, other } => {
                let mut t = format!("if {}\n{}", to_source(cond), block_source(then, indent + 1));
                if let Some(o) = other {
                    t += &format!("{pad}else\n{}", block_source(o, indent + 1));
                }
                out += &format!("{pad}{t}");
                continue;
            }
            S::For { var, lo, hi, step, body, .. } => {
                let st = step.as_ref().map(|x| format!(" step {}", to_source(x))).unwrap_or_default();
                out += &format!("{pad}for {var} from {} to {}{st}\n{}", to_source(lo), to_source(hi),
                                block_source(body, indent + 1));
                continue;
            }
            S::ForIn { var, iterable, body } => {
                out += &format!("{pad}for {var} in {}\n{}", to_source(iterable), block_source(body, indent + 1));
                continue;
            }
            S::While { cond, body } => {
                out += &format!("{pad}while {}\n{}", to_source(cond), block_source(body, indent + 1));
                continue;
            }
            S::FuncDef { name, params, body: A::FuncBody::Expr(b), .. } => {
                let ps = params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
                format!("{name}({ps}) = {}", to_source(b))
            }
            other => format!("({})", other.class()),
        };
        out += &format!("{pad}{line}\n");
    }
    out
}
