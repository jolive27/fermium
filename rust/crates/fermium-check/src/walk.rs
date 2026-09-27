//! Walking the AST (Python loops over vars(node)).
use fermium_syntax::ast as A;

/// Every where-binding in a function body, at any depth (s_FuncDef, A31).
pub fn collect_where_bindings(body: &A::FuncBody, out: &mut Vec<(String, A::Expr)>) {
    match body {
        A::FuncBody::Expr(e) => expr_wheres(e, out),
        A::FuncBody::Block(stmts) => stmts.iter().for_each(|s| stmt_wheres(s, out)),
    }
}

fn stmt_wheres(s: &A::Stmt, out: &mut Vec<(String, A::Expr)>) {
    for_each_stmt_expr(s, &mut |e| expr_wheres(e, out));
    for b in stmt_blocks(s) {
        b.iter().for_each(|x| stmt_wheres(x, out));
    }
}

fn expr_wheres(e: &A::Expr, out: &mut Vec<(String, A::Expr)>) {
    if let A::ExprKind::Where { bindings, .. } = &e.kind {
        out.extend(bindings.iter().cloned());
    }
    for c in children(e) {
        expr_wheres(c, out);
    }
}

/// The sub-statement blocks of a statement.
pub fn stmt_blocks(s: &A::Stmt) -> Vec<&Vec<A::Stmt>> {
    use A::StmtKind as K;
    match &s.kind {
        K::If { then, other, .. } => {
            let mut v = vec![then];
            if let Some(o) = other {
                v.push(o);
            }
            v
        }
        K::For { body, .. } | K::ForIn { body, .. } | K::While { body, .. } | K::Propagate { body, .. }
        | K::Sweep { body } => vec![body],
        K::FuncDef { body: A::FuncBody::Block(b), .. } => vec![b],
        K::Units { body: Some(b), .. } => vec![b],
        _ => vec![],
    }
}

/// Call f on each expression directly inside a statement (not inside its sub-blocks).
pub fn for_each_stmt_expr(s: &A::Stmt, f: &mut dyn FnMut(&A::Expr)) {
    use A::StmtKind as K;
    match &s.kind {
        K::Assign { value, .. } => f(value),
        K::IndexAssign { index, index2, value, rest, .. } => {
            f(index);
            if let Some(i) = index2 {
                f(i);
            }
            for i in rest {
                f(i);
            }
            f(value);
        }
        K::FuncDef { body: A::FuncBody::Expr(e), where_, .. } => {
            f(e);
            where_.iter().for_each(|(_, x)| f(x));
        }
        K::FuncDef { where_, .. } => where_.iter().for_each(|(_, x)| f(x)),
        K::Print { items } => items.iter().for_each(|x| f(x)),
        K::If { cond, .. } | K::While { cond, .. } => f(cond),
        K::For { lo, hi, step, .. } => {
            f(lo);
            f(hi);
            if let Some(s) = step {
                f(s);
            }
        }
        K::ForIn { iterable, .. } => f(iterable),
        K::Return { value: Some(e) } | K::ExprStmt { value: e } => f(e),
        K::Assert { cond, .. } => f(cond),
        K::Solve(sv) => {
            for eq in sv.equations.iter().chain(sv.initial.iter()).chain(sv.until.iter()) {
                f(&eq.lhs);
                f(&eq.rhs);
            }
            f(&sv.lo);
            f(&sv.hi);
            for x in [&sv.step, &sv.tolerance, &sv.lowest, &sv.grid, &sv.lo2, &sv.hi2, &sv.step2].into_iter().flatten() {
                f(x);
            }
            sv.absolute.iter().flatten().for_each(|x| f(x));
        }
        K::Fit { model, data, guesses } => {
            f(&model.lhs);
            f(&model.rhs);
            f(data);
            guesses.iter().for_each(|(_, x)| f(x));
        }
        K::Plot { series, options, .. } => {
            for p in series {
                f(&p.y);
                f(&p.x);
                p.lo.iter().chain(p.hi.iter()).for_each(|x| f(x));
            }
            for (_, o) in options {
                if let A::PlotOpt::Range(a, b) = o {
                    f(a);
                    f(b);
                }
            }
        }
        K::Propagate { samples: Some(e), .. } => f(e),
        _ => {}
    }
}

/// The direct sub-expressions of an expression (ast.children).
pub fn children(e: &A::Expr) -> Vec<&A::Expr> {
    e.children()
}

/// Names used in an expression, not counting variables bound inside (ast.free_names).
pub fn free_names(n: &A::Expr) -> Vec<String> {
    use A::ExprKind as K;
    match &n.kind {
        K::Name { name: x } => vec![x.clone()],
        K::Integral { integrand, var, lo, hi } => {
            let mut out: Vec<String> = free_names(integrand).into_iter().filter(|x| x != var).collect();
            for b in lo.iter().chain(hi.iter()) {
                out.extend(free_names(b));
            }
            out
        }
        K::Sum { body, var, lo, hi, step } => {
            let mut out: Vec<String> = free_names(body).into_iter().filter(|x| x != var).collect();
            out.extend(free_names(lo));
            out.extend(free_names(hi));
            for b in step.iter() {
                out.extend(free_names(b));
            }
            out
        }
        K::Where { value, bindings } => {
            let bound: Vec<&String> = bindings.iter().map(|(b, _)| b).collect();
            let mut out: Vec<String> = free_names(value).into_iter().filter(|x| !bound.contains(&x)).collect();
            for (_, v) in bindings {
                out.extend(free_names(v));
            }
            out
        }
        _ => children(n).into_iter().flat_map(free_names).collect(),
    }
}

/// Every expression in a statement list, recursively (statements' own expressions and their blocks).
pub fn all_exprs_in_stmts<'a>(stmts: &'a [A::Stmt], out: &mut Vec<&'a A::Expr>) {
    for s in stmts {
        for_each_stmt_expr_ref(s, out);
        for b in stmt_blocks(s) {
            all_exprs_in_stmts(b, out);
        }
    }
}

fn for_each_stmt_expr_ref<'a>(s: &'a A::Stmt, out: &mut Vec<&'a A::Expr>) {
    let mut v: Vec<*const A::Expr> = vec![];
    for_each_stmt_expr(s, &mut |e| v.push(e as *const A::Expr));
    // SAFETY: the pointers refer into `s`, which lives for 'a
    out.extend(v.into_iter().map(|p| unsafe { &*p }));
}

/// An expression and all its sub-expressions, depth first.
pub fn descendants<'a>(e: &'a A::Expr, out: &mut Vec<&'a A::Expr>) {
    out.push(e);
    for c in children(e) {
        descendants(c, out);
    }
}
