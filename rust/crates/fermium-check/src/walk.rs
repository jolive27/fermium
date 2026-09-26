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
        K::For { body, .. } | K::ForIn { body, .. } | K::While { body, .. } | K::Propagate { body, .. } => vec![body],
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
        K::IndexAssign { index, index2, value, .. } => {
            f(index);
            if let Some(i) = index2 {
                f(i);
            }
            f(value);
        }
        K::FuncDef { body: A::FuncBody::Expr(e), where_, .. } => {
            f(e);
            where_.iter().for_each(|(_, x)| f(x));
        }
        K::FuncDef { where_, .. } => where_.iter().for_each(|(_, x)| f(x)),
        K::Print(items) => items.iter().for_each(|x| f(x)),
        K::If { cond, .. } | K::While { cond, .. } => f(cond),
        K::For { lo, hi, step, .. } => {
            f(lo);
            f(hi);
            if let Some(s) = step {
                f(s);
            }
        }
        K::ForIn { iterable, .. } => f(iterable),
        K::Return(Some(e)) | K::Expr(e) => f(e),
        K::Assert { cond, .. } => f(cond),
        K::Solve { equations, initial, lo, hi, step, tolerance, until, absolute, extra, .. } => {
            for eq in equations.iter().chain(initial.iter()).chain(until.iter()) {
                f(&eq.lhs);
                f(&eq.rhs);
            }
            f(lo);
            f(hi);
            step.iter().chain(tolerance.iter()).for_each(|x| f(x));
            absolute.iter().flatten().for_each(|x| f(x));
            extra.iter().for_each(|(_, x)| f(x));
        }
        K::SolveAlgebraic { eq, lo, hi, .. } => {
            f(&eq.lhs);
            f(&eq.rhs);
            f(lo);
            f(hi);
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
            options.iter().for_each(|(_, x)| f(x));
        }
        K::Propagate { samples: Some(e), .. } => f(e),
        _ => {}
    }
}

/// The direct sub-expressions of an expression.
pub fn children(e: &A::Expr) -> Vec<&A::Expr> {
    use A::ExprKind as K;
    let mut v: Vec<&A::Expr> = vec![];
    match &e.kind {
        K::Quantity { value, .. } | K::Convert { value, .. } | K::Digits { value, .. } => v.push(value),
        K::BinOp { left, right, .. } | K::Logic { left, right, .. } => {
            v.push(left);
            v.push(right);
        }
        K::Compare { left, right, tol, .. } => {
            v.push(left);
            v.push(right);
            if let Some(t) = tol {
                v.push(t);
            }
        }
        K::Neg(x) | K::Not(x) | K::Abs(x) => v.push(x),
        K::Call { func, args } => {
            v.push(func);
            v.extend(args.iter());
        }
        K::Index { target, index } => {
            v.push(target);
            v.push(index);
        }
        K::Slice { lo, hi } => {
            v.extend(lo.iter().map(|b| b.as_ref()));
            v.extend(hi.iter().map(|b| b.as_ref()));
        }
        K::Field { target, .. } | K::Prime { target, .. } => v.push(target),
        K::Deriv { operand, .. } | K::Sqrt { operand, .. } => v.push(operand),
        K::Integral { integrand, lo, hi, .. } => {
            v.push(integrand);
            v.extend(lo.iter().map(|b| b.as_ref()));
            v.extend(hi.iter().map(|b| b.as_ref()));
        }
        K::Sum { body, lo, hi, step, .. } => {
            v.push(body);
            v.push(lo);
            v.push(hi);
            v.extend(step.iter().map(|b| b.as_ref()));
        }
        K::ListLit(items) | K::VecLit(items) | K::Table { items, .. } => v.extend(items.iter()),
        K::VecCalc { func, .. } => v.push(func),
        K::IfExpr { cond, then, other } => {
            v.push(cond);
            v.push(then);
            v.push(other);
        }
        K::Where { value, bindings } => {
            v.push(value);
            v.extend(bindings.iter().map(|(_, x)| x));
        }
        K::Uncertain { value, err } => {
            v.push(value);
            v.push(err);
        }
        K::Num { .. } | K::Str(_) | K::Bool(_) | K::Name(_) | K::End | K::Load(_) => {}
    }
    v
}
