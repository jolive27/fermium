//! `analyze pendulum: T depends on L, m, g` (D70): the Buckingham Π theorem with exact rational arithmetic.
//! A port of `fermium/dimanalysis.py` (the analysis and its report) and Checker.s_Analyze.
//!
//! Method (the classical "repeating variables" form of the Π theorem):
//! 1. one column per quantity (target first), one row per base dimension;
//! 2. walk the inputs in the order written and keep each one independent of those kept (the repeating ones);
//! 3. if the target's dimension is not a combination of them, no formula can give the target: error;
//! 4. every other quantity q makes one group q · Π rep^(−a); the target is in exactly one, with exponent 1;
//! 5. the other groups are rescaled to the nicest form (Re = ρ v √A/μ rather than μ²/(ρ² v² A)).
use fermium_ir as I;
use fermium_ir::types::Ty;
use fermium_ir::Dim;
use fermium_syntax::ast as A;
use fermium_units::exact::{add_or_record, div_or_record, mul_or_record, overflow_record, sub_or_record};
use fermium_units::{Rational64 as R, BASE_NAMES};
use num_traits::{One, Signed, Zero};

use crate::ast_ext::{binop, mk, name as aname};
use crate::checker::*;

/// The analysis has no answer; the message says why in physics terms.
#[derive(Debug)]
pub struct AnalysisError {
    pub message: String,
    pub hint: Option<String>,
    /// which quantity the problem is about (0 = target), if any
    pub index: Option<usize>,
}

/// An ordered map name → exponent (Python dicts keep insertion order).
pub type Exps = Vec<(String, R)>;

fn get(e: &Exps, k: &str) -> R {
    e.iter().find(|(n, _)| n == k).map(|(_, v)| *v).unwrap_or_else(R::zero)
}

#[derive(Debug)]
pub struct Analysis {
    pub target: String,
    pub names: Vec<String>,
    pub dims: Vec<(String, Dim)>,
    pub rank: usize,
    pub repeating: Vec<String>,
    /// groups[0] holds the target
    pub groups: Vec<Exps>,
    /// target = C · Π name^exp · f(other groups)
    pub prefactor: Exps,
    pub dropped: Vec<(String, String)>,
}

impl Analysis {
    pub fn n(&self) -> usize {
        1 + self.names.len()
    }
    fn dim(&self, n: &str) -> Dim {
        dim_of(&self.dims, n)
    }
}

fn dim_of(dims: &[(String, Dim)], n: &str) -> Dim {
    // the last one written wins, like assigning into a Python dict
    dims.iter().rev().find(|(k, _)| k == n).map(|(_, d)| *d).unwrap()
}

fn vec_of(d: &Dim) -> Vec<R> {
    d.0.to_vec()
}

/// Reduced row echelon form; returns (matrix, pivot columns).
pub fn rref(rows: &[Vec<R>]) -> (Vec<Vec<R>>, Vec<usize>) {
    let mut m: Vec<Vec<R>> = rows.to_vec();
    if m.is_empty() {
        return (m, vec![]);
    }
    let ncols = m[0].len();
    let mut pivots = vec![];
    let mut r = 0;
    for c in 0..ncols {
        let Some(p) = (r..m.len()).find(|&i| !m[i][c].is_zero()) else { continue };
        m.swap(r, p);
        let pv = m[r][c];
        // checked arithmetic throughout (red team 13): an exponent that doesn't fit is recorded, not wrapped
        m[r] = m[r].iter().map(|x| div_or_record(*x, pv, None)).collect();
        for i in 0..m.len() {
            if i != r && !m[i][c].is_zero() {
                let f = m[i][c];
                let mr = m[r].clone();
                m[i] = m[i].iter().zip(&mr).map(|(a, b)| sub_or_record(*a, mul_or_record(f, *b, None), None)).collect();
            }
        }
        pivots.push(c);
        r += 1;
        if r == m.len() {
            break;
        }
    }
    (m, pivots)
}

/// Rank of the matrix whose columns are the given exponent vectors.
pub fn rank(columns: &[Vec<R>]) -> usize {
    if columns.is_empty() {
        return 0;
    }
    let rows: Vec<Vec<R>> = (0..columns[0].len()).map(|i| columns.iter().map(|c| c[i]).collect()).collect();
    rref(&rows).1.len()
}

/// Exponents a with Σ a_j columns_j = rhs (columns independent), or None if impossible.
pub fn solve_exact(columns: &[Vec<R>], rhs: &[R]) -> Option<Vec<R>> {
    if columns.is_empty() {
        return if rhs.iter().all(|x| x.is_zero()) { Some(vec![]) } else { None };
    }
    let rows: Vec<Vec<R>> = (0..rhs.len())
        .map(|i| columns.iter().map(|c| c[i]).chain(std::iter::once(rhs[i])).collect())
        .collect();
    let (m, piv) = rref(&rows);
    let k = columns.len();
    if piv.contains(&k) {
        return None; // a pivot in the right-hand column: inconsistent
    }
    let mut a = vec![R::zero(); k];
    for (row, c) in m.iter().zip(&piv) {
        a[*c] = row[k];
    }
    Some(a)
}

/// Rescale a dimensionless group to its nicest form.
fn nicest(group: Exps) -> Exps {
    let exps: Vec<R> = group.iter().map(|(_, v)| *v).filter(|v| !v.is_zero()).collect();
    let mut cands: Vec<R> = vec![R::one(), -R::one()];
    for e in &exps {
        for c in [div_or_record(R::one(), *e, None), div_or_record(-R::one(), *e, None)] {
            if !cands.contains(&c) {
                cands.push(c);
            }
        }
    }
    let score = |s: &R| {
        let g: Vec<R> = exps.iter().map(|e| mul_or_record(*e, *s, None)).collect();
        let den = g.iter().map(|x| *x.denom()).max().unwrap_or(1);
        let sum: R = g.iter().fold(R::zero(), |acc, x| add_or_record(acc, x.abs(), None));
        let neg = g.iter().filter(|x| x.is_negative()).count();
        (den > 2, sum, neg, den, -s)
    };
    let s = *cands.iter().min_by(|a, b| score(a).cmp(&score(b))).unwrap();
    group.into_iter().map(|(k, v)| (k, mul_or_record(v, s, None))).collect()
}

/// Base dimensions that `name` has and none of `names` has (for 'nothing else has mass').
fn only_here(name: &str, dims: &[(String, Dim)], names: &[String]) -> Vec<&'static str> {
    let d = dim_of(dims, name);
    let mut out = vec![];
    for i in 0..7 {
        if !d.0[i].is_zero() && names.iter().filter(|o| *o != name).all(|o| dim_of(dims, o).0[i].is_zero()) {
            out.push(BASE_NAMES[i]);
        }
    }
    out
}

/// Buckingham Π analysis. `inputs` are (name, Dim) in the order written.
pub fn analyze(target: &str, target_dim: Dim, inputs: &[(String, Dim)]) -> Result<Analysis, AnalysisError> {
    let names: Vec<String> = inputs.iter().map(|(n, _)| n.clone()).collect();
    let mut dims = vec![(target.to_string(), target_dim)];
    dims.extend(inputs.iter().cloned());
    let err = |message: String, hint: Option<String>, index: Option<usize>| AnalysisError { message, hint, index };
    if let Some(dup) = names.iter().find(|n| names.iter().filter(|m| m == n).count() > 1) {
        let i = names.iter().position(|n| n == dup).unwrap();
        return Err(err(format!("{dup} is listed twice after 'depends on'"), None, Some(1 + i)));
    }
    if let Some(i) = names.iter().position(|n| n == target) {
        return Err(err(format!("{target} can't depend on itself"), None, Some(1 + i)));
    }
    if names.is_empty() {
        return Err(err(format!("{target} has to depend on something: write  {target} depends on a, b, c"), None, None));
    }
    let mut everything = vec![target.to_string()];
    everything.extend(names.iter().cloned());
    let col = |n: &str| vec_of(&dim_of(&dims, n));

    let mut rep: Vec<String> = vec![];
    for n in &names {
        let mut cols: Vec<Vec<R>> = rep.iter().map(|r| col(r)).collect();
        cols.push(col(n));
        if rank(&cols) > rep.len() {
            rep.push(n.clone());
        }
    }
    let r = rep.len();
    let rep_cols: Vec<Vec<R>> = rep.iter().map(|x| col(x)).collect();
    let expo = |q: &str| solve_exact(&rep_cols, &col(q));

    let a_target = expo(target);
    let all_cols: Vec<Vec<R>> = everything.iter().map(|x| col(x)).collect();
    let ngroups = everything.len() - rank(&all_cols);
    let Some(a_target) = a_target else {
        let lonely = only_here(target, &dims, &everything);
        let ins = names.join(", ");
        let mut msg = if !lonely.is_empty() {
            format!("{target} can't be made from {ins}: {target} has {}, but nothing it depends on has {}",
                    lonely.join(" and "), lonely.join(" or "))
        } else {
            format!("{target} ({}) can't be made from any powers of {ins}", fermium_units::dim_name(&target_dim))
        };
        if ngroups == 0 {
            msg += &format!("; so there is no dimensionless group at all ({} quantities, {} independent dimensions)",
                            everything.len(), everything.len());
        }
        return Err(err(msg, Some(format!("{target} must depend on something else too (a constant like G, c or ħ?)")),
                       Some(0)));
    };

    let mut groups: Vec<Exps> = vec![];
    let mut target_group: Exps = vec![(target.to_string(), R::one())];
    for (x, a) in rep.iter().zip(&a_target) {
        if !a.is_zero() {
            target_group.push((x.clone(), -a));
        }
    }
    groups.push(target_group.clone());
    for q in &names {
        if rep.contains(q) {
            continue;
        }
        let a = expo(q).unwrap();
        let mut g: Exps = vec![(q.clone(), R::one())];
        for (x, e) in rep.iter().zip(&a) {
            if !e.is_zero() {
                g.push((x.clone(), -e));
            }
        }
        groups.push(nicest(g));
    }
    let prefactor: Exps = target_group.iter().filter(|(x, _)| x != target).map(|(x, e)| (x.clone(), -e)).collect();
    let mut dropped = vec![];
    for x in &names {
        if groups.iter().all(|g| get(g, x).is_zero()) {
            let lonely = only_here(x, &dims, &everything);
            let why = if !lonely.is_empty() {
                format!("nothing else has {}", lonely.join(" or "))
            } else {
                "its units can't be cancelled by the others".to_string()
            };
            dropped.push((x.clone(), why));
        }
    }
    Ok(Analysis { target: target.to_string(), names, dims, rank: r, repeating: rep, groups, prefactor, dropped })
}

// ---------------------------------------------------------------- formatting
fn sup(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'][c as usize - '0' as usize],
            '-' => '⁻',
            c => c,
        })
        .collect()
}

/// name^p for an integer p ≥ 1, with superscripts.
fn power(name: &str, p: R) -> String {
    if p == R::one() {
        return name.to_string();
    }
    format!("{name}{}", sup(&p.numer().to_string()))
}

/// A product of integer powers as num/den text; `order` fixes the order of names.
fn ratio(exps: &Exps, order: &[String]) -> (String, bool) {
    let num: Vec<String> = order.iter().filter(|n| get(exps, n).is_positive()).map(|n| power(n, get(exps, n))).collect();
    let den: Vec<String> = order.iter().filter(|n| get(exps, n).is_negative()).map(|n| power(n, -get(exps, n))).collect();
    let top = if num.is_empty() { "1".to_string() } else { num.join(" ") };
    if den.is_empty() {
        return (top, num.len() > 1);
    }
    let bottom = if den.len() == 1 { den[0].clone() } else { format!("({})", den.join(" ")) };
    (format!("{top}/{bottom}"), true)
}

fn lcm(a: i64, b: i64) -> i64 {
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    (a / gcd(a, b)).checked_mul(b).unwrap_or_else(|| {
        overflow_record(format!("a power with denominator {a}·{b}"));
        a
    })
}

fn is_int(v: &R) -> bool {
    *v.denom() == 1
}

/// Π name^exp written the Fermium way: `T √(g/L)`, `ρ v² A`, `R (ρ/(E t²))^(1/5)` (valid Fermium).
pub fn product_text(exps: &Exps, order: Option<&[String]>) -> String {
    let order: Vec<String> = match order {
        Some(o) if !o.is_empty() => o.to_vec(),
        _ => exps.iter().map(|(k, _)| k.clone()).collect(),
    };
    let exps: Exps = exps.iter().filter(|(_, v)| !v.is_zero()).cloned().collect();
    if exps.is_empty() {
        return "1".into();
    }
    let ints: Exps = exps.iter().filter(|(_, v)| is_int(v)).cloned().collect();
    let fracs: Exps = exps.iter().filter(|(_, v)| !is_int(v)).cloned().collect();
    let has = |e: &Exps, n: &str| e.iter().any(|(k, _)| k == n);
    let root_of = |fr: &Exps| -> String {
        let d = fr.iter().fold(1, |acc, (_, v)| lcm(acc, *v.denom()));
        let scaled: Exps = fr.iter().map(|(k, v)| (k.clone(), mul_or_record(*v, R::from_integer(d), None))).collect();
        let (body, compound) = ratio(&scaled, &order);
        let last = body.chars().last().unwrap();
        let wrap = if compound || "⁰¹²³⁴⁵⁶⁷⁸⁹".contains(last) { format!("({body})") } else { body.clone() };
        match d {
            2 => format!("√{wrap}"),
            3 => format!("∛{wrap}"),
            _ => format!("({body})^(1/{d})"),
        }
    };
    if fracs.is_empty() {
        return ratio(&ints, &order).0;
    }
    if fracs.iter().all(|(_, v)| v.is_positive()) || fracs.iter().all(|(_, v)| v.is_negative()) {
        // the roots on one side: ρ v √A/μ, v/√(g λ), F/(μ v √A)
        let side = |e: &Exps| -> Vec<String> {
            let mut items: Vec<String> =
                order.iter().filter(|n| has(e, n) && is_int(&get(e, n))).map(|n| power(n, get(e, n))).collect();
            let fr: Exps = e.iter().filter(|(_, v)| !is_int(v)).cloned().collect();
            if !fr.is_empty() {
                items.push(root_of(&fr));
            }
            items
        };
        let pos: Exps = exps.iter().filter(|(_, v)| v.is_positive()).cloned().collect();
        let neg: Exps = exps.iter().filter(|(_, v)| v.is_negative()).map(|(k, v)| (k.clone(), -v)).collect();
        let mut num = side(&pos);
        if num.is_empty() {
            num = vec!["1".into()];
        }
        let den = side(&neg);
        let mut text = num.join(" ");
        if !den.is_empty() {
            text += "/";
            text += &if den.len() > 1 { format!("({})", den.join(" ")) } else { den[0].clone() };
        }
        return text;
    }
    // roots of both signs: one root of a ratio, integer powers around it (T √(g/L), R (ρ/(E t²))^(1/5))
    let root = root_of(&fracs);
    let front: Vec<String> =
        order.iter().filter(|n| has(&ints, n) && get(&ints, n).is_positive()).map(|n| power(n, get(&ints, n))).collect();
    let mut text = if front.is_empty() { root } else { format!("{} {root}", front.join(" ")) };
    let den: Vec<String> =
        order.iter().filter(|n| has(&ints, n) && get(&ints, n).is_negative()).map(|n| power(n, -get(&ints, n))).collect();
    if !den.is_empty() {
        text += "/";
        text += &if den.len() > 1 { format!("({})", den.join(" ")) } else { den[0].clone() };
    }
    text
}

pub fn pi_name(i: usize) -> String {
    let sub: String = i
        .to_string()
        .chars()
        .map(|c| ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'][c as usize - '0' as usize])
        .collect();
    format!("Π{sub}")
}

/// The lines `analyze` prints.
pub fn report(an: &Analysis, title: Option<&str>) -> Vec<String> {
    let mut order = vec![an.target.clone()];
    order.extend(an.names.iter().cloned());
    let t = &an.target;
    let ngr = an.groups.len();
    let head = format!("dimensional analysis{}: {t} depends on {}",
                       title.map(|x| format!(" of {x}")).unwrap_or_default(), an.names.join(", "));
    let used: Vec<&str> =
        (0..7).filter(|&i| order.iter().any(|x| !an.dim(x).0[i].is_zero())).map(|i| BASE_NAMES[i]).collect();
    let mut lines = vec![
        head,
        format!("  {} quantities, {} independent dimension{} ({}{}) → {} − {} = {ngr} dimensionless group{}", an.n(),
                an.rank, if an.rank != 1 { "s" } else { "" }, if used.len() == an.rank { "" } else { "among " },
                used.join(", "), an.n(), an.rank, if ngr != 1 { "s" } else { "" }),
    ];
    for (i, g) in an.groups.iter().enumerate() {
        lines.push(format!("  {} = {}", pi_name(i + 1), product_text(g, Some(&order))));
    }
    let pre = product_text(&an.prefactor, Some(&order));
    let rest = (2..=ngr).map(pi_name).collect::<Vec<_>>().join(", ");
    if an.prefactor.is_empty() {
        if ngr == 1 {
            lines.push(format!("  so {t} is a pure number: it can't depend on any of them"));
        } else {
            lines.push(format!("  so {t} = f({rest})   (f is a function dimensional analysis can't give)"));
        }
    } else if ngr == 1 {
        lines.push(format!("  so {t} ∝ {pre}   ({t} = C {pre}, with C a pure number)"));
    } else {
        lines.push(format!("  so {t} = {pre} · f({rest})   (f is a function dimensional analysis can't give)"));
    }
    for (x, why) in &an.dropped {
        lines.push(format!("  {x} drops out: {why}"));
    }
    lines
}

impl Checker {
    fn text_print(&mut self, s: &str) -> I::Stmt {
        let t = self.text(s);
        I::Stmt { kind: I::StmtKind::Print(vec![I::PrintItem::Text(t)]), line: 0 }
    }

    /// `analyze pendulum: T depends on L, m, g`: Buckingham Π groups, printed; defines pendulum(L, g) (D70).
    pub fn s_analyze(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::Analyze { title, target, inputs, raw } = &s.kind else { unreachable!() };
        if !ctx.is_main || ctx.lam.is_some() || ctx.branch != 0 || ctx.loop_depth != 0 {
            return Err(self.err("analyze must be at the top level of the program (not inside a block)", s.span, None));
        }
        let shown = |n: &str| raw.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()).unwrap_or_else(|| n.to_string());
        // analyze in SI dimensions even under natural units (red team round 2 #8, D132)
        let natural = self.nat.natural();
        let mut kinds: Vec<(String, &'static str)> = vec![];
        let mut dim_of_param = |c: &mut Checker, p: &A::Param| -> CResult<Dim> {
            let show = shown(&p.name);
            if let Some(u) = &p.unit {
                kinds.push((p.name.clone(), "var"));
                return Ok(c.resolve_unit_si(u)?.dim);
            }
            match c.lookup(ctx.scope, &p.name).map(|x| x.0) {
                Some(Binding::Const(k)) => {
                    kinds.push((p.name.clone(), "const"));
                    Ok(k.unit.dim)
                }
                Some(Binding::Sym(b)) if matches!(c.module.syms[b].ty, Ty::Num(_)) => {
                    let Ty::Num(d) = c.module.syms[b].ty.clone() else { unreachable!() };
                    if !c.u.is_concrete(&d) {
                        return Err(c.err(format!("the units of {show} aren't known yet"), p.span,
                                         Some(format!("give its unit here:  {show} [m]"))));
                    }
                    if natural && c.extra[b].nat.natural() {
                        return Err(c.err(format!("{show} was computed in natural units, so its SI dimension (which \
                                                  analyze works with) isn't known"), p.span,
                                         Some(format!("give its unit here instead:  {show} [m]"))));
                    }
                    kinds.push((p.name.clone(), "var"));
                    Ok(c.u.resolve(&d))
                }
                None => Err(c.err(format!("{show} has no units yet: analyze needs to know what it is"), p.span,
                                  Some(format!("give its unit, like  {show} [m], or define it first, like  {show} = \
                                                1.0 m")))),
                Some(_) => Err(c.err(format!("{show} isn't a number with units, so it can't be analyzed"), p.span,
                                     Some(format!("give its unit instead, like  {show} [m]")))),
            }
        };
        let tdim = dim_of_param(self, target)?;
        let mut ins = vec![];
        for p in inputs {
            ins.push((shown(&p.name), dim_of_param(self, p)?));
        }
        let kind_of = |n: &str| kinds.iter().rev().find(|(k, _)| k == n).map(|(_, v)| *v).unwrap_or("");
        let mut all_params = vec![target];
        all_params.extend(inputs.iter());
        if let Some(t) = title {
            if all_params.iter().any(|p| shown(&p.name) == *t) {
                return Err(self.err(format!("the analysis can't be called {t}: that's one of its quantities"), s.span,
                                    Some(format!("pick another name, e.g.  analyze {t}_law: ..."))));
            }
        }
        let an = match analyze(&shown(&target.name), tdim, &ins) {
            Ok(a) => a,
            Err(e) => {
                let span = match e.index {
                    None => s.span,
                    Some(i) => all_params[i].span,
                };
                return Err(self.err(e.message, span, e.hint));
            }
        };
        let mut lines = report(&an, title.as_deref());
        if natural {
            lines.insert(2, format!("  (in SI dimensions: under {} those constants are pure numbers, so length, mass \
                                     and time would collapse into powers of {} and hide the groups)",
                                    self.nat.label(),
                                    if self.nat.consts().contains(&"ħ") { "energy" } else { "fewer dimensions" }));
        }
        let mut stmts: Vec<I::Stmt> = lines.iter().map(|l| self.text_print(l)).collect();
        let Some(title) = title else { return Ok(stmts) };
        if an.prefactor.is_empty() {
            return Ok(stmts);
        }
        // make the result usable: pendulum(L, g) = √(L/g), for  fit T = C pendulum(L, g) to data
        let pre = |n: &str| get(&an.prefactor, &shown(n));
        let sp = s.span;
        let params: Vec<&A::Param> =
            inputs.iter().filter(|p| !pre(&p.name).is_zero() && kind_of(&p.name) == "var").collect();
        let num = |x: i64| mk(A::ExprKind::Num { value: x as f64, sigfigs: None, digit: true }, sp);
        let mut body: Option<A::Expr> = None;
        for p in inputs {
            let e = pre(&p.name);
            if e.is_zero() {
                continue;
            }
            let mut f = aname(&p.name, sp);
            if e != R::one() {
                let k = e.abs();
                let mut ex = if *k.denom() == 1 {
                    num(*k.numer())
                } else {
                    binop("/", num(*k.numer()), num(*k.denom()), false, sp)
                };
                if e.is_negative() {
                    ex = mk(A::ExprKind::Neg { operand: Box::new(ex) }, sp);
                }
                f = binop("^", f, ex, false, sp);
            }
            body = Some(match body {
                None => f,
                Some(b) => binop("*", b, f, false, sp),
            });
        }
        let body = body.unwrap();
        let order: Vec<String> = inputs.iter().map(|p| shown(&p.name)).collect();
        let text = product_text(&an.prefactor, Some(&order));
        if !params.is_empty() {
            let fparams: Vec<A::Param> =
                params.iter().map(|p| A::Param { name: p.name.clone(), unit: p.unit.clone(), kind: None, span: p.span }).collect();
            let fdef = A::Stmt {
                kind: A::StmtKind::FuncDef { name: title.clone(), params: fparams, body: A::FuncBody::Expr(body),
                                             where_: vec![] },
                span: sp,
            };
            // the checker keeps pointers to the definitions it is given: keep this one alive with the program
            let fdef: &'static A::Stmt = Box::leak(Box::new(fdef));
            self.s_funcdef(fdef, ctx)?;
            let sig = format!("{title}({})", params.iter().map(|p| shown(&p.name)).collect::<Vec<_>>().join(", "));
            let rest = (2..=an.groups.len()).map(pi_name).collect::<Vec<_>>().join(", ");
            let law = if an.groups.len() == 1 { format!("C {sig}") } else { format!("{sig} · f({rest})") };
            let l = format!("  defined {sig} = {text}, so {} = {law}", an.target);
            stmts.push(self.text_print(&l));
            return Ok(stmts);
        }
        // only constants: a plain value, printed (planck = √(G ħ/c³) = 1.6×10⁻³⁵ m)
        let body: &'static A::Expr = Box::leak(Box::new(body));
        let v = self.expr(body, ctx)?;
        let out = self.assign_to(title, v, sp, None, ctx)?;
        stmts.push(out);
        let nm: &'static A::Expr = Box::leak(Box::new(aname(title, sp)));
        let v = self.expr(nm, ctx)?;
        let t = self.text(&format!("  defined {title} = {text} ="));
        let f = self.fmt(&v);
        stmts.push(I::Stmt { kind: I::StmtKind::Print(vec![I::PrintItem::Text(t), I::PrintItem::Num(v, f)]), line: 0 });
        Ok(stmts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fermium_units::{LENGTH, MASS, TIME};

    #[test]
    fn pendulum() {
        let g = LENGTH / TIME.powi(2);
        let an = analyze("T", TIME, &[("L".into(), LENGTH), ("m".into(), MASS), ("g".into(), g)]).unwrap();
        let lines = report(&an, Some("pendulum"));
        assert_eq!(lines[2], "  Π₁ = T √(g/L)");
        assert_eq!(lines[3], "  so T ∝ √(L/g)   (T = C √(L/g), with C a pure number)");
        assert_eq!(lines[4], "  m drops out: nothing else has mass");
    }
}
