//! Vectors and matrices: a port of Checker.vec_arith, e_VecLit, vec_unify, vec_ty_map, shared_dim, mat_arith,
//! matrix_literal, zero_matrix, need_square, mat_builtin, eigen_builtin, map_entries, row_column,
//! entry_assign, the vector/matrix branches of e_Quantity, e_Index and e_Field, index_expr,
//! fixed_or_runtime_index, runtime_index, mat_index and vec_elem from `fermium/checker.py`, and the vector
//! and matrix built-ins of Checker.builtin (norm, unit, cross, dot, trace, angle, identity, …).
//!
//! Representation in the IR: a vector, matrix or complex number is an `ExprKind::Vec` of its entries
//! (row-major); built-ins that need shapes the values don't carry get them as trailing constant arguments:
//! `shuffle(m, i0, i1, …)` (picked flat indexes: transpose, rows, columns), `matmul(a, b, r, k, c)`,
//! `c.powi(z, p)` / `c.powr(z, p)`.
use num_rational::Rational64;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;

use crate::arith::minsf;
use crate::checker::*;
use crate::lists::Idx;
use crate::stmts::{ty_dim, written_code};
use crate::units::{self, Unit};

/// The largest matrix is 16×16, the longest vector 16 components (linalg_big.MAX_DIM).
pub const MAX_DIM: usize = 16;

pub type Mixed = Vec<Option<I::Hint>>;

/// The display units of a mixed vector (Python MixedHint), if the expression carries them.
pub fn mixed_of(v: &I::Expr) -> Option<Mixed> {
    v.get_extra().and_then(|x| x.mixed.clone())
}

pub fn set_mixed(r: &mut I::Expr, m: Option<Mixed>) {
    if m.is_some() || r.x.is_some() {
        r.extra().mixed = m;
    }
}

/// Python `r.hint = v.hint`, where the hint may be a MixedHint.
pub fn copy_hint(r: &mut I::Expr, v: &I::Expr) {
    r.hint = v.hint.clone();
    set_mixed(r, mixed_of(v));
}

/// Python `v.hint is not None` (a Unit or a MixedHint).
pub fn has_hint(v: &I::Expr) -> bool {
    v.hint.is_some() || mixed_of(v).is_some()
}

/// The dimension of each component (the shared one repeated, for a uniform vector; Python comp_dims).
pub fn comp_dims(t: &Ty) -> Vec<DExpr> {
    match t {
        Ty::Vec { dims: Some(ds), .. } => ds.clone(),
        Ty::Vec { n, dim: Some(d), .. } => vec![d.clone(); *n],
        Ty::Complex(d) => vec![d.clone(); 2],
        Ty::Mat { r, c, dim } => vec![dim.clone(); r * c],
        _ => vec![],
    }
}

pub fn is_mixed(t: &Ty) -> bool {
    matches!(t, Ty::Vec { dims: Some(_), .. })
}

pub fn vec_n(t: &Ty) -> usize {
    match t {
        Ty::Vec { n, .. } => *n,
        Ty::Complex(_) => 2,
        Ty::Mat { r, c, .. } => r * c,
        _ => 0,
    }
}

pub fn vec_ty(d: DExpr, n: usize) -> Ty {
    Ty::Vec { n, dim: Some(d), dims: None }
}

pub fn mat_ty(d: DExpr, r: usize, c: usize) -> Ty {
    Ty::Mat { r, c, dim: d }
}

pub fn konst(x: f64) -> I::Expr {
    ir(I::ExprKind::Const(x), dimless_num(), 0)
}

fn bin(op: I::BinOp, a: I::Expr, b: I::Expr, ty: Ty, line: u32) -> I::Expr {
    ir(I::ExprKind::Bin(op, Box::new(a), Box::new(b)), ty, line)
}

fn binop_of(op: &str) -> I::BinOp {
    match op {
        "+" => I::BinOp::Add,
        "-" => I::BinOp::Sub,
        "*" => I::BinOp::Mul,
        _ => I::BinOp::Div,
    }
}

pub fn builtin_ir(name: &str, args: Vec<I::Expr>, ty: Ty, line: u32) -> I::Expr {
    ir(I::ExprKind::Builtin(name.into(), args), ty, line)
}

/// A built-in with its figures from some arguments (Python _bi).
pub fn bi(name: &str, args: Vec<I::Expr>, ty: Ty, sfargs: &[&I::Expr], line: u32) -> I::Expr {
    let sf = minsf(sfargs);
    let mut r = builtin_ir(name, args, ty, line);
    r.sf = sf;
    r
}

/// `shuffle(m, flat indexes…)`: the entries of m at those indexes, as a vector or matrix.
pub fn shuffle(m: I::Expr, idx: &[usize], ty: Ty, line: u32) -> I::Expr {
    let mut args = vec![m];
    args.extend(idx.iter().map(|&k| konst(k as f64)));
    builtin_ir("shuffle", args, ty, line)
}

/// Flat indices that turn an r×c matrix into its c×r transpose (linalg.transpose_index).
pub fn transpose_index(r: usize, c: usize) -> Vec<usize> {
    (0..c).flat_map(|j| (0..r).map(move |i| i * c + j)).collect()
}

const SUP_FROM: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁻";
const SUP_TO: &str = "0123456789-";

fn from_sup(s: &str) -> String {
    s.chars().map(|c| match SUP_FROM.chars().position(|x| x == c) {
        Some(i) => SUP_TO.chars().nth(i).unwrap(),
        None => c,
    }).collect()
}

fn to_sup(s: &str) -> String {
    s.chars().map(|c| match SUP_TO.chars().position(|x| x == c) {
        Some(i) => SUP_FROM.chars().nth(i).unwrap(),
        None => c,
    }).collect()
}

/// The display unit u^p for a whole number p, written out: (N/m)² → N²/m², (N/m)⁻¹ → m/N; None if u isn't a
/// simple product of units (Python hint_power).
pub fn hint_power(u: &Option<I::Hint>, p: i64) -> Option<I::Hint> {
    let u = u.as_ref()?;
    if u.offset != 0.0 {
        return None;
    }
    let (num, den) = match u.name.split_once('/') {
        Some((a, b)) => (a, b),
        None => (u.name.as_str(), ""),
    };
    let mut powers: Vec<(String, i64)> = vec![];
    for (part, sign) in [(num, 1i64), (den, -1i64)] {
        for tok in part.split_whitespace() {
            if tok.contains('/') {
                return None;
            }
            // ([^\s⁰-⁹⁻^()]+)([⁰-⁹⁻]*)
            let base: String = tok.chars().take_while(|c| !SUP_FROM.contains(*c) && !"^()".contains(*c)).collect();
            let rest = &tok[base.len()..];
            if base.is_empty() || !rest.chars().all(|c| SUP_FROM.contains(c)) {
                return None;
            }
            let e: i64 = if rest.is_empty() { 1 } else { from_sup(rest).parse().ok()? };
            match powers.iter_mut().find(|(k, _)| *k == base) {
                Some((_, v)) => *v += sign * e * p,
                None => powers.push((base, sign * e * p)),
            }
        }
    }
    let top: Vec<String> = powers.iter().filter(|(_, v)| *v > 0)
        .map(|(k, v)| format!("{k}{}", if *v != 1 { to_sup(&v.to_string()) } else { String::new() })).collect();
    let bot: Vec<String> = powers.iter().filter(|(_, v)| *v < 0)
        .map(|(k, v)| format!("{k}{}", if *v != -1 { to_sup(&(-v).to_string()) } else { String::new() })).collect();
    if top.is_empty() && bot.is_empty() {
        return None;
    }
    let mut name = if top.is_empty() { "1".to_string() } else { top.join(" ") };
    if bot.len() == 1 {
        name += &format!("/{}", bot[0]);
    } else if !bot.is_empty() {
        name += &format!("/({})", bot.join(" "));
    }
    let pu = units::parse_unit_string(&name).ok()?;
    Some(crate::exprs::hint_of(&pu))
}

/// The AST nodes of a call's arguments (for errors that point at one); a |z| or √z node has one.
pub fn arg_nodes(e: &A::Expr) -> Vec<&A::Expr> {
    match &e.kind {
        A::ExprKind::Call { args, .. } => args.iter().collect(),
        A::ExprKind::Sqrt { operand, .. } | A::ExprKind::Abs { operand } | A::ExprKind::Neg { operand } => {
            vec![operand]
        }
        A::ExprKind::Field { target, .. } => vec![target],
        _ => vec![],
    }
}

pub fn arg_span(e: &A::Expr, i: usize) -> A::Span {
    arg_nodes(e).get(i).map(|a| a.span).unwrap_or(e.span)
}

impl Checker {
    // ============================================================ vectors
    pub fn vec_arith(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        let line = e.span.line;
        let va = matches!(a.ty, Ty::Vec { .. } | Ty::Complex(_));
        let vb = matches!(b.ty, Ty::Vec { .. } | Ty::Complex(_));
        if matches!(a.ty, Ty::List(_)) || matches!(b.ty, Ty::List(_)) {
            return Err(self.err("can't mix vectors and lists in arithmetic", e.span, None));
        }
        let sf = minsf(&[&a, &b]);
        let mut r;
        if op == "+" || op == "-" {
            let verb = if op == "+" { "add" } else { "subtract" };
            if !(va && vb) {
                return Err(self.err(format!("can't {verb} a vector and a single number"), e.span,
                                    Some("both sides must be vectors, e.g. <1, 2> m + <3, 4> m".into())));
            }
            let (na, nb) = (vec_n(&a.ty), vec_n(&b.ty));
            if na != nb {
                return Err(self.err(format!("can't {verb} a {na}-vector and a {nb}-vector"), e.span, None));
            }
            if is_mixed(&a.ty) || is_mixed(&b.ty) {
                if let Some(k) = self.vec_unify(&a.ty, &b.ty) {
                    let (da, db) = (self.desc(&comp_dims(&a.ty)[k]), self.desc(&comp_dims(&b.ty)[k]));
                    return Err(self.err(format!("can't {verb} these vectors: component {} is {da} on one side and \
                                                 {db} on the other", k + 1), e.span,
                                        Some("vectors add component by component, and each pair must have the same \
                                              units".into())));
                }
                let ty = if is_mixed(&a.ty) { a.ty.clone() } else { b.ty.clone() };
                let m = mixed_of(&a).or_else(|| mixed_of(&b));
                let mut r = bin(binop_of(op), a, b, ty, line);
                set_mixed(&mut r, m);
                r.sf = sf;
                return Ok(r);
            }
            let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
            if !self.u.unify(&da, &db) {
                let (sa, sb) = (self.desc(&da), self.desc(&db));
                return Err(self.err(format!("can't {verb} vectors of {sa} and {sb}"), e.span,
                                    Some("both sides of + and - must have the same units".into())));
            }
            self.warn_confusable_sum(op, &a, &b, e);
            let hint = a.hint.clone().or_else(|| b.hint.clone());
            r = bin(binop_of(op), a, b, vec_ty(da, na), line);
            r.hint = hint;
        } else if op == "*" && va && vb {
            let (na, nb) = (vec_n(&a.ty), vec_n(&b.ty));
            if na != nb {
                return Err(self.err(format!("can't take the dot product of a {na}-vector and a {nb}-vector"), e.span,
                                    None));
            }
            let da = self.shared_dim(&a, "the dot product", e)?;
            let db = self.shared_dim(&b, "the dot product", e)?;
            r = builtin_ir("vdot", vec![a, b], Ty::Num(da.mul(&db)), line);
        } else if op == "×" && va && vb {
            let (na, nb) = (vec_n(&a.ty), vec_n(&b.ty));
            if na != nb {
                return Err(self.err(format!("can't take the cross product of a {na}-vector and a {nb}-vector"),
                                    e.span, None));
            }
            if na == 4 {
                return Err(self.err("the cross product needs 3-vectors (or 2-vectors), not 4-vectors", e.span, None));
            }
            let da = self.shared_dim(&a, "the cross product", e)?;
            let db = self.shared_dim(&b, "the cross product", e)?;
            let ty = if na == 3 { vec_ty(da.mul(&db), 3) } else { Ty::Num(da.mul(&db)) };
            r = builtin_ir("cross", vec![a, b], ty, line);
        } else if op == "*" || op == "×" {
            if op == "×" {
                return Err(self.err("× between a vector and a number: use * (or a space) to scale a vector", e.span,
                                    None));
            }
            let (v, k) = if va { (&a, &b) } else { (&b, &a) };
            let kd = ty_dim(&k.ty).unwrap_or_else(DExpr::dimless);
            let ty = self.vec_ty_map(&v.ty, |d| d.mul(&kd));
            let keep = self.dimless(k);
            let (h, m) = if keep { (v.hint.clone(), mixed_of(v)) } else { (None, None) };
            r = bin(I::BinOp::Mul, a, b, ty, line);
            r.hint = h;
            set_mixed(&mut r, m);
        } else if op == "/" {
            if vb {
                return Err(self.err("can't divide by a vector", e.span, None));
            }
            let bd = ty_dim(&b.ty).unwrap_or_else(DExpr::dimless);
            let ty = self.vec_ty_map(&a.ty, |d| d.div(&bd));
            let keep = self.dimless(&b);
            let (h, m) = if keep { (a.hint.clone(), mixed_of(&a)) } else { (None, None) };
            r = bin(I::BinOp::Div, a, b, ty, line);
            r.hint = h;
            set_mixed(&mut r, m);
        } else {
            return Err(self.err(format!("unknown operator {op}"), e.span, None));
        }
        r.sf = sf;
        Ok(r)
    }

    pub fn e_vec_lit(&mut self, e: &A::Expr, items_ast: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        let mut items = vec![];
        for x in items_ast {
            items.push(self.expr(x, ctx)?);
        }
        for (it, node) in items.iter().zip(items_ast) {
            self.need_num(it, node, "a vector component")?;
        }
        let known: Vec<fermium_ir::Dim> = items
            .iter()
            .map(|it| self.u.norm(&ty_dim(&it.ty).unwrap()))
            .filter(|d| d.is_concrete())
            .map(|d| d.konst)
            .collect();
        let sf = minsf(&items.iter().collect::<Vec<_>>());
        let line = e.span.line;
        if known.iter().any(|d| *d != known[0]) {
            // components in different units, like the state vector <1 m, 2 m/s>: each keeps its own (D29)
            let dims: Vec<DExpr> = items.iter().map(|it| ty_dim(&it.ty).unwrap()).collect();
            let hints: Mixed = items.iter().map(|it| it.hint.clone()).collect();
            let n = items.len();
            let mut r = ir(I::ExprKind::Vec(items), Ty::Vec { n, dim: None, dims: Some(dims) }, line);
            if hints.iter().any(|h| h.is_some()) {
                set_mixed(&mut r, Some(hints));
            }
            r.sf = sf;
            return Ok(r);
        }
        let dim = DExpr::fresh();
        for it in &items {
            self.u.unify(&dim, &ty_dim(&it.ty).unwrap());
        }
        let hint = items.iter().find_map(|it| it.hint.clone());
        let n = items.len();
        let mut r = ir(I::ExprKind::Vec(items), vec_ty(dim, n), line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    /// Unify two vector types component by component; the index of the first mismatch, or None.
    pub fn vec_unify(&mut self, a: &Ty, b: &Ty) -> Option<usize> {
        for (k, (da, db)) in comp_dims(a).iter().zip(comp_dims(b).iter()).enumerate() {
            if !self.u.unify(da, db) {
                return Some(k);
            }
        }
        None
    }

    pub fn vec_ty_map(&self, t: &Ty, f: impl Fn(&DExpr) -> DExpr) -> Ty {
        match t {
            Ty::Vec { n, dims: Some(ds), .. } => Ty::Vec { n: *n, dim: None, dims: Some(ds.iter().map(&f).collect()) },
            Ty::Vec { n, dim: Some(d), .. } => vec_ty(f(d), *n),
            Ty::Complex(d) => Ty::Complex(f(d)),
            Ty::Mat { r, c, dim } => mat_ty(f(dim), *r, *c),
            other => other.clone(),
        }
    }

    /// The one dimension all components of vector v share (an error for <1 m, 2 m/s>).
    pub fn shared_dim(&mut self, v: &I::Expr, what: &str, node: &A::Expr) -> CResult<DExpr> {
        let Ty::Vec { dims: Some(ds), .. } = &v.ty else {
            return Ok(ty_dim(&v.ty).unwrap_or_else(DExpr::dimless));
        };
        let ds = ds.clone();
        if ds[1..].iter().all(|d| self.u.unify(&ds[0], d)) {
            return Ok(ds[0].clone());
        }
        let list = ds.iter().map(|d| self.desc(d)).collect::<Vec<_>>().join(", ");
        Err(self.err(format!("{what} needs all components of the vector in the same units, but this one has {list}"),
                     node.span, Some("a state vector like <x, v> can be added and scaled, but it has no length or \
                                      direction".into())))
    }

    // ============================================================ matrices (D29)
    pub fn mat_arith(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        let line = e.span.line;
        let ma = matches!(a.ty, Ty::Mat { .. });
        let mb = matches!(b.ty, Ty::Mat { .. });
        if matches!(a.ty, Ty::List(_)) || matches!(b.ty, Ty::List(_)) {
            return Err(self.err("can't mix matrices and lists in arithmetic", e.span, None));
        }
        let shape = |t: &Ty| match t {
            Ty::Mat { r, c, .. } => format!("{r}×{c} matrix"),
            _ => String::new(),
        };
        let rc = |t: &Ty| match t {
            Ty::Mat { r, c, .. } => (*r, *c),
            _ => (0, 0),
        };
        let sf = minsf(&[&a, &b]);
        let mut r;
        if op == "+" || op == "-" {
            let verb = if op == "+" { "add" } else { "subtract" };
            if !(ma && mb) {
                let other = if ma { &b } else { &a };
                let what = if matches!(other.ty, Ty::Vec { .. } | Ty::Complex(_)) { "vector" } else { "single number" };
                return Err(self.err(format!("can't {verb} a matrix and a {what}"), e.span,
                                    Some("both sides must be matrices of the same size".into())));
            }
            if rc(&a.ty) != rc(&b.ty) {
                return Err(self.err(format!("can't {verb} a {} and a {}", shape(&a.ty), shape(&b.ty)), e.span, None));
            }
            let (da, db) = (ty_dim(&a.ty).unwrap(), ty_dim(&b.ty).unwrap());
            if !self.u.unify(&da, &db) {
                let (sa, sb) = (self.desc(&da), self.desc(&db));
                return Err(self.err(format!("can't {verb} matrices of {sa} and {sb}"), e.span,
                                    Some("both sides of + and - must have the same units".into())));
            }
            let (rr, cc) = rc(&a.ty);
            let hint = a.hint.clone().or_else(|| b.hint.clone());
            r = bin(binop_of(op), a, b, mat_ty(da, rr, cc), line);
            r.hint = hint;
        } else if op == "×" {
            return Err(self.err("× is the cross product of vectors; multiply matrices with * or a space: A B", e.span,
                                None));
        } else if op == "*" && ma && mb {
            let ((ar, ac), (br, bc)) = (rc(&a.ty), rc(&b.ty));
            if ac != br {
                return Err(self.err(format!("can't multiply a {} times a {}: the first needs as many columns as the \
                                             second has rows", shape(&a.ty), shape(&b.ty)), e.span, None));
            }
            let d = ty_dim(&a.ty).unwrap().mul(&ty_dim(&b.ty).unwrap());
            let ty = if ar * bc == 1 { Ty::Num(d) } else { mat_ty(d, ar, bc) };
            let hint = match (&a.hint, &b.hint) {
                (Some(x), Some(y)) if x.name == y.name => hint_power(&a.hint, 2), // K K in N/m is shown in N²/m²
                _ => self.keep_hint(&a, &b),
            };
            r = builtin_ir("matmul", vec![a, b, konst(ar as f64), konst(ac as f64), konst(bc as f64)], ty, line);
            r.hint = hint;
        } else if op == "*" && ma && matches!(b.ty, Ty::Vec { .. } | Ty::Complex(_)) {
            let (ar, ac) = rc(&a.ty);
            let bn = vec_n(&b.ty);
            if ac != bn {
                return Err(self.err(format!("can't multiply a {} times a {bn}-vector: the matrix needs one column per \
                                             component", shape(&a.ty)), e.span, None));
            }
            let db = self.shared_dim(&b, "a matrix times a vector", e)?;
            let d = ty_dim(&a.ty).unwrap().mul(&db);
            let ty = if ar == 1 { Ty::Num(d) } else { vec_ty(d, ar) };
            r = builtin_ir("matmul", vec![a, b, konst(ar as f64), konst(ac as f64), konst(1.0)], ty, line);
        } else if op == "*" && mb && matches!(a.ty, Ty::Vec { .. } | Ty::Complex(_)) {
            return Err(self.err("a vector times a matrix isn't defined here; write the matrix first (M v), or use \
                                 transpose(M) v for the row-vector product", e.span, None));
        } else if op == "*" {
            let (m, k) = if ma { (&a, &b) } else { (&b, &a) };
            let (mr, mc) = rc(&m.ty);
            let d = ty_dim(&m.ty).unwrap().mul(&ty_dim(&k.ty).unwrap_or_else(DExpr::dimless));
            let hint = if self.dimless(k) { m.hint.clone() } else { None };
            r = bin(I::BinOp::Mul, a, b, mat_ty(d, mr, mc), line);
            r.hint = hint;
        } else if op == "/" {
            if mb {
                return Err(self.err("can't divide by a matrix", e.span, Some("multiply by inverse(M) instead".into())));
            }
            let (mr, mc) = rc(&a.ty);
            let d = ty_dim(&a.ty).unwrap().div(&ty_dim(&b.ty).unwrap_or_else(DExpr::dimless));
            let hint = if self.dimless(&b) { a.hint.clone() } else { None };
            r = bin(I::BinOp::Div, a, b, mat_ty(d, mr, mc), line);
            r.hint = hint;
        } else {
            return Err(self.err(format!("unknown operator {op}"), e.span, None));
        }
        r.sf = sf;
        Ok(r)
    }

    /// [[a, b], [c, d]]: a matrix, all entries in one unit, stored row by row.
    pub fn matrix_literal(&mut self, e: &A::Expr, rows_ast: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        let mut rows: Vec<Vec<I::Expr>> = vec![];
        let mut row_nodes: Vec<&[A::Expr]> = vec![];
        for row in rows_ast {
            let A::ExprKind::ListLit { items } = &row.kind else { unreachable!() };
            let mut r = vec![];
            for x in items {
                r.push(self.expr(x, ctx)?);
            }
            rows.push(r);
            row_nodes.push(items);
        }
        let ncol = rows[0].len();
        if rows.iter().any(|r| r.len() != ncol) {
            return Err(self.err("every row of a matrix needs the same number of entries", e.span, None));
        }
        let nrow = rows.len();
        if nrow > MAX_DIM || ncol > MAX_DIM || ncol == 0 || nrow * ncol < 2 {
            return Err(self.err(format!("a matrix can have 1 to {MAX_DIM} rows and 1 to {MAX_DIM} columns (at most \
                                         {MAX_DIM}×{MAX_DIM}), not {nrow}×{ncol}"), e.span, None));
        }
        let dim = DExpr::fresh();
        let mut items = vec![];
        for (row, nodes) in rows.into_iter().zip(row_nodes) {
            for (it, node) in row.into_iter().zip(nodes) {
                self.need_num(&it, node, "a matrix entry")?;
                let d = ty_dim(&it.ty).unwrap();
                if !self.u.unify(&dim, &d) {
                    return Err(self.err(format!("all entries of a matrix need the same units; this one is {} but the \
                                                 others are {}", self.desc(&d), self.desc(&dim)), node.span, None));
                }
                items.push(it);
            }
        }
        let hint = items.iter().find_map(|it| it.hint.clone());
        let sf = minsf(&items.iter().collect::<Vec<_>>());
        let direct = written_code(&items);
        let mut r = ir(I::ExprKind::Vec(items), mat_ty(dim, nrow, ncol), e.span.line);
        r.hint = hint;
        r.sf = sf;
        r.direct = direct;
        Ok(r)
    }

    /// A list literal that is a matrix ([[1, 2], [3, 4]]) or a nested list that isn't: None for a plain list.
    pub fn list_lit_matrix(&mut self, e: &A::Expr, items: &[A::Expr], ctx: &mut Ctx) -> CResult<Option<I::Expr>> {
        let is_list = |x: &A::Expr| matches!(x.kind, A::ExprKind::ListLit { .. });
        if !items.is_empty() && items.iter().all(is_list) {
            return self.matrix_literal(e, items, ctx).map(Some);
        }
        if items.iter().any(is_list) {
            return Err(self.err("a matrix is written as a list of rows, like [[1, 2], [3, 4]]; lists of lists aren't \
                                 supported otherwise", e.span, None));
        }
        Ok(None)
    }

    /// zeros(r, c): an r×c matrix of zeros whose unit comes from its first use, like a plain 0 (D195).
    pub fn zero_matrix(&mut self, args: &[I::Expr], e: &A::Expr) -> CResult<I::Expr> {
        let mut dims = vec![];
        for (i, a) in args.iter().enumerate() {
            let ok = match (&a.kind, &a.ty) {
                (I::ExprKind::Const(v), Ty::Num(d)) => {
                    *v == v.trunc() && *v >= 1.0 && *v <= MAX_DIM as f64 && self.u.unify(d, &DExpr::dimless())
                }
                _ => false,
            };
            if !ok {
                return Err(self.err(format!("zeros(r, c) makes an r×c matrix; r and c must be fixed whole numbers from \
                                             1 to {MAX_DIM}, like zeros(8, 8)"), arg_span(e, i), None));
            }
            let I::ExprKind::Const(v) = a.kind else { unreachable!() };
            dims.push(v as usize);
        }
        let (r, c) = (dims[0], dims[1]);
        if r * c < 2 {
            return Err(self.err("zeros(1, 1) would be a single number; write 0", e.span, None));
        }
        let items = (0..r * c).map(|_| konst(0.0)).collect();
        Ok(ir(I::ExprKind::Vec(items), mat_ty(DExpr::fresh(), r, c), e.span.line))
    }

    pub fn need_square(&self, m: &I::Expr, name: &str, node: &A::Expr) -> CResult<()> {
        match &m.ty {
            Ty::Mat { r, c, .. } if r != c => {
                Err(self.err(format!("{name} needs a square matrix, but this one is {r}×{c}"), node.span, None))
            }
            Ty::Mat { .. } => Ok(()),
            _ => Err(self.err(format!("{name} needs a matrix, like [[1, 2], [3, 4]]"), node.span, None)),
        }
    }

    pub fn mat_builtin(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let n = args.len();
        let k = if name == "solve_linear" { 2 } else { 1 };
        if name == "eigenvalues" || name == "eigenvectors" {
            return self.eigen_builtin(name, args, e);
        }
        if n != k {
            return Err(self.err(format!("{name} takes {k} argument{} but was given {n}", if k != 1 { "s" } else { "" }),
                                e.span, None));
        }
        let line = e.span.line;
        let m = &args[0];
        if name == "transpose" {
            let Ty::Mat { r, c, dim } = &m.ty else {
                return Err(self.err("transpose needs a matrix, like [[1, 2], [3, 4]]", e.span, None));
            };
            let (r, c, dim) = (*r, *c, dim.clone());
            let (hint, sf, direct) = (m.hint.clone(), m.sf, m.direct);
            let mut out = shuffle(args.into_iter().next().unwrap(), &transpose_index(r, c), mat_ty(dim, c, r), line);
            out.hint = hint;
            out.sf = sf;
            out.direct = direct;
            return Ok(out);
        }
        self.need_square(m, name, e)?;
        let Ty::Mat { r: mr, c: mc, dim: md } = m.ty.clone() else { unreachable!() };
        let sf = minsf(&args.iter().collect::<Vec<_>>());
        let mut r = if name == "det" {
            let hint = hint_power(&m.hint, mr as i64); // det of N/m entries is in N²/m² (2×2)
            let mut r = builtin_ir("det", args, Ty::Num(md.pow(Rational64::from_integer(mr as i64))), line);
            r.hint = hint;
            r
        } else if name == "inverse" {
            let hint = hint_power(&m.hint, -1); // and its inverse in m/N
            let mut r = builtin_ir("inverse", args, mat_ty(md.pow(Rational64::from_integer(-1)), mr, mc), line);
            r.hint = hint;
            r
        } else {
            let b = &args[1];
            if !matches!(b.ty, Ty::Vec { .. } | Ty::Complex(_)) {
                return Err(self.err("solve_linear(M, b) needs a matrix and a vector, like solve_linear(K, <1, 2> N)",
                                    e.span, None));
            }
            let bn = vec_n(&b.ty);
            if bn != mr {
                return Err(self.err(format!("solve_linear(M, b) got a {mr}×{mc} matrix and a {bn}-vector; b needs one \
                                             component per row"), e.span, None));
            }
            let db = self.shared_dim(b, "solve_linear(M, b)", e)?;
            builtin_ir("solve_linear", args, vec_ty(db.div(&md), bn), line)
        };
        r.sf = sf;
        Ok(r)
    }

    /// eigenvalues(M) / eigenvectors(M) of a symmetric matrix, and eigenvalues(K, M) / eigenvectors(K, M) for
    /// K v = λ M v (normal modes: λ = ω²). Jacobi rotations (D38).
    pub fn eigen_builtin(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let n = args.len();
        if n != 1 && n != 2 {
            return Err(self.err(format!("{name} takes a matrix, like {name}(K), or two, like {name}(K, M) for K v = λ \
                                         M v, but was given {n} arguments"), e.span, None));
        }
        for m in &args {
            self.need_square(m, name, e)?;
        }
        let Ty::Mat { r: kr, c: kc, dim: kd } = args[0].ty.clone() else { unreachable!() };
        if !(2..=MAX_DIM).contains(&kr) {
            return Err(self.err(format!("{name} needs a square matrix from 2×2 to {MAX_DIM}×{MAX_DIM}, not {kr}×{kc}"),
                                e.span, None));
        }
        if n == 2 {
            let Ty::Mat { r: mr, .. } = args[1].ty else { unreachable!() };
            if mr != kr {
                return Err(self.err(format!("{name}(K, M) needs K and M of the same size, but they are {kr}×{kr} and \
                                             {mr}×{mr}"), e.span, None));
            }
        }
        let sf = minsf(&args.iter().collect::<Vec<_>>());
        let line = e.span.line;
        let mut r = if name == "eigenvalues" {
            let dim = if n == 1 { kd } else { kd.div(&ty_dim(&args[1].ty).unwrap()) };
            let hint = if n == 1 { args[0].hint.clone() } else { None }; // eigenvalues of a matrix in N/m are in N/m
            let mut r = builtin_ir(name, args, vec_ty(dim, kr), line);
            r.hint = hint;
            r
        } else {
            builtin_ir(name, args, mat_ty(DExpr::dimless(), kr, kr), line)
        };
        r.sf = sf;
        Ok(r)
    }

    /// Apply fn to every entry of vector or matrix t (evaluated once), giving one of the same shape; with
    /// reduce = [flat indexes], the sum of those entries instead (the trace).
    pub fn map_entries(&mut self, t: I::Expr, f: Option<&dyn Fn(&mut Checker, I::Expr) -> I::Expr>,
                       reduce: Option<&[usize]>, ctx: &mut Ctx) -> I::Expr {
        let line = t.line;
        let sym = self.new_sym("·m", t.ty.clone(), ctx);
        let dims = comp_dims(&t.ty);
        let hints: Mixed = mixed_of(&t).unwrap_or_else(|| vec![t.hint.clone(); dims.len()]);
        let entry = |c: &Checker, k: usize| {
            let mut x = ir(I::ExprKind::VecElem(Box::new(c.ivar(sym, line)), k), Ty::Num(dims[k].clone()), line);
            x.hint = hints[k].clone();
            x.sf = t.sf;
            x
        };
        let mut body = if let Some(red) = reduce {
            let d = ty_dim(&t.ty).unwrap();
            let mut body = entry(self, red[0]);
            for &k in &red[1..] {
                let x = entry(self, k);
                body = bin(I::BinOp::Add, body, x, Ty::Num(d.clone()), line);
            }
            body
        } else {
            let f = f.unwrap();
            let mut items = vec![];
            for k in 0..dims.len() {
                let x = entry(self, k);
                items.push(f(self, x));
            }
            ir(I::ExprKind::Vec(items), t.ty.clone(), line)
        };
        copy_hint(&mut body, &t);
        body.sf = t.sf;
        let (h, m, sf) = (body.hint.clone(), mixed_of(&body), body.sf);
        let ty = body.ty.clone();
        let mut r = ir(I::ExprKind::Let(vec![(sym, t)], Box::new(body)), ty, line);
        r.hint = h;
        set_mixed(&mut r, m);
        r.sf = sf;
        r
    }

    /// row(M, i) and column(M, j).
    pub fn row_column(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let args = arg_nodes(e);
        if args.len() != 2 {
            return Err(self.err(format!("{name}(M, {}) takes a matrix and a number", if name == "row" { "i" } else { "j" }),
                                e.span, None));
        }
        let m = self.expr(args[0], ctx)?;
        let Ty::Mat { r: mr, c: mc, dim } = m.ty.clone() else {
            return Err(self.err(format!("{name}(M, k) needs a matrix, like [[1, 2], [3, 4]]"), args[0].span, None));
        };
        let (length, size) = if name == "row" { (mc, mr) } else { (mr, mc) };
        if !(2..=MAX_DIM).contains(&length) {
            return Err(self.err(format!("a {name} of a {mr}×{mc} matrix has {length} entr{}, so it isn't a vector; pick \
                                         an entry with M[i, j]", if length == 1 { "y" } else { "ies" }), e.span, None));
        }
        let k = self.fixed_or_runtime_index(args[1], size, ctx)?;
        let (stride, offs): (usize, Vec<usize>) =
            if name == "row" { (mc, (0..mc).collect()) } else { (1, (0..mr).map(|i| i * mc).collect()) };
        match k {
            Idx::Fixed(k) => {
                if k < 0 || k >= size as i64 {
                    return Err(self.err(format!("this matrix has {size} {name}s, so there is no {name} {}", k + 1),
                                        args[1].span, None));
                }
                let idx: Vec<usize> = offs.iter().map(|o| k as usize * stride + o).collect();
                let (hint, sf) = (m.hint.clone(), m.sf);
                let mut r = shuffle(m, &idx, vec_ty(dim, length), e.span.line);
                r.hint = hint;
                r.sf = sf;
                Ok(r)
            }
            k => self.runtime_index(m, vec![(k, size, stride)], offs, vec_ty(dim, length), e),
        }
    }

    // ============================================================ indexing




    fn checked_pieces(&self, pieces: Vec<(Idx, usize, usize)>, node: &A::Expr) -> CResult<Vec<(I::Expr, usize, usize)>> {
        let mut idxs = vec![];
        for (k, size, stride) in pieces {
            let k = match k {
                Idx::Fixed(k) => {
                    if k < 0 || k >= size as i64 {
                        return Err(self.err(format!("there is no index {} here: valid indexes are 1 to {size}", k + 1),
                                            node.span, None));
                    }
                    konst((k + 1) as f64)
                }
                Idx::Run(e) => e,
            };
            idxs.push((k, size, stride));
        }
        Ok(idxs)
    }

    pub fn mat_index(&mut self, idx_ast: &A::Expr, size: usize, what: &str, ctx: &mut Ctx) -> CResult<usize> {
        if matches!(idx_ast.kind, A::ExprKind::End) {
            return Ok(size - 1);
        }
        let mut idx = self.index_expr(idx_ast, &konst(size as f64), ctx)?;
        if let I::ExprKind::Let(_, value) = &idx.kind {
            if matches!(value.kind, I::ExprKind::Const(_)) {
                idx = (**value).clone();
            }
        }
        let I::ExprKind::Const(v) = idx.kind else {
            return Err(self.err("a matrix entry must be picked with fixed numbers, like M[1, 2]", idx_ast.span, None));
        };
        let k = v.trunc() as i64;
        if k < 1 || k > size as i64 {
            return Err(self.err(format!("this matrix has {size} {what}s, so there is no {what} {k}"), idx_ast.span,
                                None));
        }
        Ok(k as usize - 1)
    }

    pub fn vec_elem(&mut self, t: I::Expr, k: i64, node: &A::Expr) -> CResult<I::Expr> {
        let n = vec_n(&t.ty);
        if k < 0 || k >= n as i64 {
            return Err(self.err(format!("this vector has {n} components, so there is no component {}", k + 1),
                                node.span, None));
        }
        let k = k as usize;
        let d = comp_dims(&t.ty)[k].clone();
        let hint = match mixed_of(&t) {
            Some(m) => m[k].clone(),
            None => t.hint.clone(),
        };
        let sf = t.sf;
        let mut r = ir(I::ExprKind::VecElem(Box::new(t), k), Ty::Num(d), node.span.line);
        r.hint = hint;
        r.sf = sf;
        Ok(r)
    }

    /// M[i, j] (parsed as M[i][j]) when M is a matrix: None if the inner target isn't a matrix (Python e_Index's
    /// first branch; the caller then checks the target itself).
    pub fn index_matrix_entry(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Option<I::Expr>> {
        let A::ExprKind::Index { target, index: Some(index) } = &e.kind else { return Ok(None) };
        let A::ExprKind::Index { target: inner_t, index: Some(inner_i) } = &target.kind else { return Ok(None) };
        let Checked::Val(inner) = self.expr_any(inner_t, ctx)? else { return Ok(None) };
        let Ty::Mat { r, c, dim } = inner.ty.clone() else { return Ok(None) };
        let ri = self.fixed_or_runtime_index(inner_i, r, ctx)?;
        let ci = self.fixed_or_runtime_index(index, c, ctx)?;
        if matches!(ri, Idx::Run(_)) || matches!(ci, Idx::Run(_)) {
            return self.runtime_index(inner, vec![(ri, r, c), (ci, c, 1)], vec![0], Ty::Num(dim), e).map(Some);
        }
        let i = self.mat_index(inner_i, r, "row", ctx)?;
        let j = self.mat_index(index, c, "column", ctx)?;
        let (hint, sf) = (inner.hint.clone(), inner.sf);
        let mut out = ir(I::ExprKind::VecElem(Box::new(inner), i * c + j), Ty::Num(dim), e.span.line);
        out.hint = hint;
        out.sf = sf;
        Ok(Some(out))
    }

    /// t[i] for a complex number, matrix or vector t (the matching branches of Python e_Index); None otherwise.
    pub fn index_vecmat(&mut self, e: &A::Expr, t: &I::Expr, ctx: &mut Ctx) -> CResult<Option<I::Expr>> {
        let A::ExprKind::Index { index: Some(index), .. } = &e.kind else { return Ok(None) };
        match t.ty.clone() {
            Ty::Complex(_) => Err(self.err("a complex number can't be indexed with [...]", e.span,
                                           Some("its parts are re(z) and im(z) (or z.re and z.im)".into()))),
            Ty::Mat { r, c, dim } => {
                if !(2..=MAX_DIM).contains(&c) {
                    return Err(self.err(format!("a row of a {r}×{c} matrix isn't a vector; pick an entry with M[i, j]"),
                                        index.span, None));
                }
                let ri = self.fixed_or_runtime_index(index, r, ctx)?;
                if let Idx::Run(_) = ri {
                    return self.runtime_index(t.clone(), vec![(ri, r, c)], (0..c).collect(), vec_ty(dim, c), e)
                        .map(Some);
                }
                let i = self.mat_index(index, r, "row", ctx)?;
                let idx: Vec<usize> = (0..c).map(|j| i * c + j).collect();
                let mut out = shuffle(t.clone(), &idx, vec_ty(dim, c), e.span.line);
                out.hint = t.hint.clone();
                out.sf = t.sf;
                Ok(Some(out))
            }
            Ty::Vec { n, .. } => {
                let idx = if matches!(index.kind, A::ExprKind::End) {
                    konst(n as f64)
                } else {
                    self.index_expr(index, &konst(n as f64), ctx)?
                };
                if let I::ExprKind::Const(v) = idx.kind {
                    return self.vec_elem(t.clone(), v.trunc() as i64 - 1, index).map(Some);
                }
                let k = self.fixed_or_runtime_index(index, n, ctx)?;
                match k {
                    Idx::Fixed(k) => self.vec_elem(t.clone(), k, index).map(Some),
                    k => {
                        // v[i] in a loop: checked at run time (#54)
                        let d = ty_dim(&t.ty).unwrap_or_else(DExpr::dimless);
                        self.runtime_index(t.clone(), vec![(k, n, 1)], vec![0], Ty::Num(d), e).map(Some)
                    }
                }
            }
            Ty::ComplexList(d) => {
                let idx = self.index_expr(index, t, ctx)?;
                let mut r = builtin_ir("cl.get", vec![t.clone(), idx], Ty::Complex(d), e.span.line);
                r.hint = t.hint.clone();
                r.sf = None;
                Ok(Some(r))
            }
            _ => Ok(None),
        }
    }

    // ============================================================ entries, quantities, fields
    /// M[i, j] = x and v[i] = x (also +=, …): the variable gets a copy with that entry replaced, so a matrix can be
    /// filled in a loop (D195). Indexes may be known only at run time (checked then).
    pub fn entry_assign(&mut self, b: I::SymId, s: &A::Stmt, ctx: &mut Ctx) -> CResult<I::Stmt> {
        let A::StmtKind::IndexAssign { target: name, index, index2, value, op } = &s.kind else { unreachable!() };
        let name_ast = crate::ast_ext::name(name, s.span);
        let tgt = self.expr(&name_ast, ctx)?;
        let bty = self.module.syms[b].ty.clone();
        let mk_index = |t: A::Expr, i: &A::Expr| {
            crate::ast_ext::mk(A::ExprKind::Index { target: Box::new(t), index: Some(Box::new(i.clone())) }, s.span)
        };
        let is_mat = matches!(bty, Ty::Mat { .. });
        let (pieces, read) = match &bty {
            Ty::Mat { r, c, .. } => {
                let Some(index2) = index2 else {
                    return Err(self.err(format!("{name} is a matrix: set one entry at a time, like {name}[i, j] = …"),
                                        s.span, None));
                };
                let pi = self.fixed_or_runtime_index(index, *r, ctx)?;
                let pj = self.fixed_or_runtime_index(index2, *c, ctx)?;
                (vec![(pi, *r, *c), (pj, *c, 1)], mk_index(mk_index(name_ast.clone(), index), index2))
            }
            _ => {
                if index2.is_some() {
                    return Err(self.err(format!("{name} is a vector: set one component, like {name}[i] = …"), s.span,
                                        None));
                }
                if is_mixed(&bty) {
                    return Err(self.err(format!("the components of {name} have different units, so they can't be set \
                                                 one at a time; build the new vector, like {name} = <…>"), s.span,
                                        None));
                }
                let n = vec_n(&bty);
                (vec![(self.fixed_or_runtime_index(index, n, ctx)?, n, 1)], mk_index(name_ast.clone(), index))
            }
        };
        let v = if op != "=" {
            let bop: String = op.chars().next().unwrap().to_string();
            self.expr(&crate::ast_ext::binop(&bop, read, value.clone(), false, s.span), ctx)?
        } else {
            self.expr(value, ctx)?
        };
        self.need_num(&v, value, if is_mat { "a matrix entry" } else { "a vector component" })?;
        let (bd, vd) = (ty_dim(&bty).unwrap(), ty_dim(&v.ty).unwrap());
        self.unify_or(&bd, &vd, |c| format!("the entries of {name} are {}; can't put {} in it", c.desc(&bd), c.desc(&vd)),
                      value.span, None)?;
        let fake = crate::ast_ext::mk(A::ExprKind::End, s.span);
        let idxs = self.checked_pieces(pieces, &fake)?;
        let hint = if tgt.hint.is_some() { tgt.hint.clone() } else { v.hint.clone() };
        let sf = if tgt.sf.is_some() { minsf(&[&tgt, &v]) } else { v.sf };
        let mut r = ir(I::ExprKind::VecSet { v: Box::new(tgt), idxs, value: Box::new(v) }, bty, s.span.line);
        r.hint = hint;
        r.sf = sf;
        r.direct = 0;
        self.assign_to(name, r, s.span, None, ctx)
    }

    /// <3, 4> m and [[1, 2], [3, 4]] N/m: a vector or matrix times a unit (Python e_Quantity's branches).
    pub fn vec_quantity(&mut self, e: &A::Expr, value: &A::Expr, v: I::Expr, u: &Unit) -> CResult<I::Expr> {
        let line = e.span.line;
        if is_mixed(&v.ty) {
            return Err(self.err("this vector already has units (a different unit on each component)", e.span,
                                Some("write the unit on each component, like <1 m, 2 m/s>".into())));
        }
        let items_direct = |v: &I::Expr| match &v.kind {
            I::ExprKind::Vec(items) => written_code(items),
            _ => 0,
        };
        if let Ty::Mat { r, c, dim } = v.ty.clone() {
            if u.affine() {
                return Err(self.err("°C/°F can't be used for matrices", e.span, None));
            }
            if !matches!(value.kind, A::ExprKind::ListLit { .. }) {
                let vd = self.u.norm(&dim);
                if vd.is_concrete() && !vd.konst.is_dimensionless() {
                    return Err(self.err(format!("this already has units ({})", self.desc(&dim)), e.span, None));
                }
            } else if !self.u.unify(&dim, &DExpr::dimless()) {
                return Err(self.err(format!("this matrix already has units ({}); write the unit once, after the ]]",
                                            self.desc(&dim)), e.span, None));
            }
            let direct = if matches!(value.kind, A::ExprKind::ListLit { .. }) { items_direct(&v) } else { 0 };
            let sf = v.sf;
            let mut r = bin(I::BinOp::Mul, v, konst(u.factor), mat_ty(DExpr::of(u.dim), r, c), line);
            r.hint = Some(crate::exprs::hint_of(u));
            r.sf = sf;
            r.direct = direct;
            return Ok(r);
        }
        if u.affine() {
            return Err(self.err("°C/°F can't be used for vectors", e.span, None));
        }
        let dim = ty_dim(&v.ty).unwrap();
        if !matches!(value.kind, A::ExprKind::VecLit { .. }) {
            let vd = self.u.norm(&dim);
            if vd.is_concrete() && !vd.konst.is_dimensionless() {
                return Err(self.err(format!("this already has units ({})", self.desc(&dim)), e.span, None));
            }
        }
        self.u.unify(&dim, &DExpr::dimless());
        let direct = if matches!(value.kind, A::ExprKind::VecLit { .. }) { items_direct(&v) } else { 0 };
        let n = vec_n(&v.ty);
        let sf = v.sf;
        let mut r = bin(I::BinOp::Mul, v, konst(u.factor), vec_ty(DExpr::of(u.dim), n), line);
        r.hint = Some(crate::exprs::hint_of(u));
        r.sf = sf;
        r.direct = direct;
        Ok(r)
    }

    /// `.x .y .z` of a vector, `.re .im` of a complex number or of a list of them (the matching branches of
    /// Python e_Field); None for any other target (solutions, data tables).
    pub fn field_vecmat(&mut self, e: &A::Expr, name: &str, t: &I::Expr) -> CResult<Option<I::Expr>> {
        let A::ExprKind::Field { target, .. } = &e.kind else { return Ok(None) };
        let call = crate::ast_ext::mk(A::ExprKind::Call { func: Box::new(crate::ast_ext::name(name, e.span)),
                                                          args: vec![(**target).clone()] }, e.span);
        if matches!(t.ty, Ty::ComplexList(_)) && (name == "re" || name == "im") {
            return self.clist_call(name, vec![t.clone()], &call).map(Some);
        }
        if matches!(t.ty, Ty::Complex(_)) || (matches!(t.ty, Ty::Num(_)) && (name == "re" || name == "im")) {
            if name != "re" && name != "im" {
                return Err(self.err(format!("a complex number's parts are .re and .im (not .{name})"), e.span,
                                    Some("or write re(z) and im(z)".into())));
            }
            return self.cplx_builtin(name, vec![t.clone()], &call).map(Some);
        }
        if matches!(t.ty, Ty::Vec { .. }) {
            let Some(k) = ["x", "y", "z"].iter().position(|c| *c == name) else {
                return Err(self.err(format!("a vector's components are .x, .y and .z (not .{name})"), e.span, None));
            };
            return self.vec_elem(t.clone(), k as i64, e).map(Some);
        }
        Ok(None)
    }

    pub fn e_field(&mut self, e: &A::Expr, target: &A::Expr, name: &str, ctx: &mut Ctx) -> CResult<Checked> {
        if self.py_ref_of(target, ctx).is_some() {
            return Err(self.not_ported("a Python value", e.span));
        }
        let t = self.expr_any(target, ctx)?;
        let Checked::Val(t) = t else {
            return self.field_other(e, target, name, t, ctx);
        };
        if let Some(r) = self.field_vecmat(e, name, &t)? {
            return Ok(Checked::Val(r));
        }
        if matches!(t.ty, Ty::Data(_)) {
            return self.field_other(e, target, name, Checked::Val(t), ctx);
        }
        if matches!(t.ty, Ty::Num(_)) && ["x", "y", "z"].contains(&name) {
            let d = ty_dim(&t.ty).unwrap();
            return Err(self.err(format!("this is a single number ({}), not a vector, so it has no .{name}", self.desc(&d)),
                                e.span, Some("a vector is written <3, 4> m/s".into())));
        }
        Err(self.err(format!("'.{name}' only works on vectors (v.x) and data loaded from a file (data.{name})"), e.span,
                     None))
    }

    // ============================================================ built-ins
    /// The vector, matrix, complex and FFT built-ins (the matching branches of Python Checker.builtin, after the
    /// arguments are checked): Some(result), or None when `name` with these arguments isn't one of them.
    pub fn builtin_vecmat(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr, ctx: &mut Ctx)
                          -> CResult<Option<I::Expr>> {
        let n = args.len();
        let line = e.span.line;
        let is_cl = |a: &I::Expr| matches!(a.ty, Ty::ComplexList(_));
        let is_c = |a: &I::Expr| matches!(a.ty, Ty::Complex(_));
        let is_v = |a: &I::Expr| matches!(a.ty, Ty::Vec { .. } | Ty::Complex(_));
        if name == "fft" || (name == "ifft" && n == 1) || args.iter().any(is_cl)
            || (name == "complex" && n == 2 && args.iter().any(|a| matches!(a.ty, Ty::List(_)))) {
            return self.clist_call(name, args, e).map(Some); // lists of complex numbers (D243)
        }
        if crate::builtins::COMPLEX_FUNCS.contains(&name) || args.iter().any(is_c) {
            return self.cplx_builtin(name, args, e).map(Some);
        }
        let need = |c: &Checker, k: usize| -> CResult<()> {
            if n != k {
                return Err(c.err(format!("{name} takes {k} argument{} but was given {n}", if k != 1 { "s" } else { "" }),
                                 e.span, None));
            }
            Ok(())
        };
        if crate::builtins::MATH1.contains(&name) || crate::builtins::SPECIAL1.contains(&name)
            || crate::builtins::SPECIAL2.contains(&name) || name == "sqrt" || name == "cbrt" {
            return Ok(None);
        }
        if name == "abs" && n == 1 && matches!(args[0].ty, Ty::Vec { .. } | Ty::Mat { .. }) {
            let t = args.into_iter().next().unwrap();
            let f = |_c: &mut Checker, x: I::Expr| {
                let (ty, sf) = (x.ty.clone(), x.sf);
                let mut r = builtin_ir("abs", vec![x], ty, 0);
                r.sf = sf;
                r
            };
            return Ok(Some(self.map_entries(t, Some(&f), None, ctx)));
        }
        if name == "trace" {
            need(self, 1)?;
            self.need_square(&args[0], "trace", e)?;
            let Ty::Mat { r: k, .. } = args[0].ty else { unreachable!() };
            let m = args.into_iter().next().unwrap();
            let (hint, sf) = (m.hint.clone(), m.sf);
            let red: Vec<usize> = (0..k).map(|i| i * k + i).collect();
            let mut r = self.map_entries(m, None, Some(&red), ctx);
            r.hint = hint;
            r.sf = sf;
            return Ok(Some(r));
        }
        if name == "angle" {
            need(self, 2)?;
            let nodes = arg_nodes(e);
            for (x, node) in args.iter().zip(&nodes) {
                if !is_v(x) {
                    return Err(self.err("angle(a, b) needs two vectors, like angle(<1, 0> m, <1, 1> m)", node.span, None));
                }
                self.shared_dim(x, "angle(a, b)", node)?;
            }
            let (na, nc) = (vec_n(&args[0].ty), vec_n(&args[1].ty));
            if na != nc || !(na == 2 || na == 3) {
                return Err(self.err(format!("angle(a, b) needs two 2-vectors or two 3-vectors, but got a {na}-vector and a \
                                             {nc}-vector"), e.span, None));
            }
            let sfa = minsf(&args.iter().collect::<Vec<_>>());
            let sa = self.new_sym("·a", args[0].ty.clone(), ctx);
            let sc = self.new_sym("·b", args[1].ty.clone(), ctx);
            let cr = self.vec_arith("×", self.ivar(sa, line), self.ivar(sc, line), e)?;
            let crd = ty_dim(&cr.ty).unwrap();
            let crsf = cr.sf;
            let mut y = builtin_ir(if na == 3 { "norm" } else { "abs" }, vec![cr], Ty::Num(crd), line);
            y.sf = crsf;
            let x = self.vec_arith("*", self.ivar(sa, line), self.ivar(sc, line), e)?;
            let mut at = builtin_ir("atan2", vec![y, x], dimless_num(), line);
            at.sf = sfa;
            let mut args = args.into_iter();
            let (a, c) = (args.next().unwrap(), args.next().unwrap());
            let mut r = ir(I::ExprKind::Let(vec![(sa, a), (sc, c)], Box::new(at)), dimless_num(), line);
            r.sf = sfa;
            return Ok(Some(r));
        }
        if name == "sign" && n == 1 && matches!(args[0].ty, Ty::Vec { .. }) {
            // sign(v) = v/|v|, the direction; d|u|/dt = sign(u)·u' then works for vectors too (A18)
            self.shared_dim(&args[0], "sign(v)", e)?;
            let vn = vec_n(&args[0].ty);
            let sf = minsf(&args.iter().collect::<Vec<_>>());
            let mut r = builtin_ir("unit", args, vec_ty(DExpr::dimless(), vn), line);
            r.sf = sf;
            return Ok(Some(r));
        }
        if name == "norm" || name == "unit" || name == "hat" {
            need(self, 1)?;
            if !is_v(&args[0]) {
                return Err(self.err(format!("{name} needs a vector, like <3, 4> m"), arg_span(e, 0), None));
            }
            let d = self.shared_dim(&args[0], &format!("{name}(v)"), e)?;
            let sf = minsf(&args.iter().collect::<Vec<_>>());
            if name == "norm" {
                let hint = args[0].hint.clone();
                let mut r = builtin_ir("norm", args, Ty::Num(d), line);
                r.hint = hint;
                r.sf = sf;
                return Ok(Some(r));
            }
            let vn = vec_n(&args[0].ty);
            let mut r = builtin_ir("unit", args, vec_ty(DExpr::dimless(), vn), line);
            r.sf = sf;
            return Ok(Some(r));
        }
        if name == "cross" {
            need(self, 2)?;
            if !args.iter().all(is_v) {
                return Err(self.err("cross(a, b) needs two vectors", e.span, None));
            }
            let mut it = args.into_iter();
            let (a, b) = (it.next().unwrap(), it.next().unwrap());
            return self.vec_arith("×", a, b, e).map(Some);
        }
        if name == "vec" {
            if !(2..=MAX_DIM).contains(&n) {
                return Err(self.err(format!("vec(...) takes 2 to {MAX_DIM} components"), e.span, None));
            }
            let A::ExprKind::Call { args: asts, .. } = &e.kind else { unreachable!() };
            let lit = crate::ast_ext::mk(A::ExprKind::VecLit { items: asts.clone() }, e.span);
            return self.e_vec_lit(&lit, asts, ctx).map(Some);
        }
        if name == "dot" && n == 2 && args.iter().all(is_v) {
            let mut it = args.into_iter();
            let (a, b) = (it.next().unwrap(), it.next().unwrap());
            return self.vec_arith("*", a, b, e).map(Some);
        }
        if ["transpose", "det", "inverse", "solve_linear", "eigenvalues", "eigenvectors"].contains(&name) {
            return self.mat_builtin(name, args, e).map(Some);
        }
        if name == "identity" {
            need(self, 1)?;
            let k = &args[0];
            let (I::ExprKind::Const(v), Ty::Num(_)) = (&k.kind, &k.ty) else {
                return Err(self.err("identity(n) needs a fixed whole number, like identity(3)", arg_span(e, 0), None));
            };
            if *v != v.trunc() || !(2.0..=MAX_DIM as f64).contains(v) {
                return Err(self.err(format!("identity(n) needs a whole number n from 2 to {MAX_DIM} (matrices are at \
                                             most {MAX_DIM}×{MAX_DIM})"), arg_span(e, 0), None));
            }
            let m = *v as usize;
            let items = (0..m * m).map(|k| konst(if k / m == k % m { 1.0 } else { 0.0 })).collect();
            return Ok(Some(ir(I::ExprKind::Vec(items), mat_ty(DExpr::dimless(), m, m), line)));
        }
        if name == "zeros" && n == 2 {
            return self.zero_matrix(&args, e).map(Some);
        }
        if ["fft_re", "fft_im", "ifft", "amplitude_spectrum", "power_spectrum", "frequencies", "argmax", "argmin"]
            .contains(&name) {
            return self.m3_fourier(name, args, e).map(Some);
        }
        Ok(None)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpose_like_linalg() {
        assert_eq!(transpose_index(2, 3), vec![0, 3, 1, 4, 2, 5]);
    }

    #[test]
    fn hint_powers() {
        let u = crate::units::lookup_unit("N").unwrap();
        let h = Some(crate::exprs::hint_of(&u));
        assert_eq!(hint_power(&h, 2).unwrap().name, "N²");
        assert_eq!(hint_power(&h, -1).unwrap().name, "1/N");
    }
}

impl Checker {
    /// The display unit of each component of a mixed vector (Python fmt_components' hints).
    pub fn mixed_hints(&self, v: &I::Expr, n: usize) -> Vec<Option<I::Hint>> {
        mixed_of(v).unwrap_or_else(|| vec![None; n])
    }
}
