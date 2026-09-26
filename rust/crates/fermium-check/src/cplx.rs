//! Complex numbers (D90–D95): the checker's rules from `fermium/cplx.py` (arith, power, builtin, compare,
//! quantity). A complex value is stored like a 2-vector (real part, imaginary part), an `ExprKind::Vec` of two
//! numbers or an expression of type `Ty::Complex`; the operations are built-ins named `c.*`, evaluated by the
//! back end with the kernels of cplx.py.
use num_rational::Rational64;
use num_traits::Signed;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;

use crate::arith::{const_exponent, minsf};
use crate::checker::*;
use crate::stmts::ty_dim;
use crate::units::Unit;
use crate::vecmat::{arg_span, builtin_ir, konst};

pub fn is_c(v: &I::Expr) -> bool {
    matches!(v.ty, Ty::Complex(_))
}

/// A real number as a complex one (imaginary part 0, same units).
pub fn promote(v: I::Expr) -> I::Expr {
    if is_c(&v) {
        return v;
    }
    let d = ty_dim(&v.ty).unwrap_or_else(DExpr::dimless);
    let (hint, sf, direct, line) = (v.hint.clone(), v.sf, v.direct, v.line);
    let zero = ir(I::ExprKind::Const(0.0), Ty::Num(d.clone()), line);
    let mut r = ir(I::ExprKind::Vec(vec![v, zero]), Ty::Complex(d), line);
    r.hint = hint;
    r.sf = sf;
    r.direct = direct;
    r
}

fn cbi(name: &str, args: Vec<I::Expr>, ty: Ty, line: u32) -> I::Expr {
    let sf = minsf(&args.iter().collect::<Vec<_>>());
    let mut r = builtin_ir(name, args, ty, line);
    r.sf = sf;
    r
}

fn dim(v: &I::Expr) -> DExpr {
    ty_dim(&v.ty).unwrap_or_else(DExpr::dimless)
}

impl Checker {
    fn no_affine(&self, v: &I::Expr, e: &A::Expr) -> CResult<()> {
        if let Some(h) = &v.hint {
            if h.offset != 0.0 {
                return Err(self.err(format!("°C/°F can't be used for complex numbers ({} has an offset)", h.name),
                                    e.span, Some("write temperatures in K".into())));
            }
        }
        Ok(())
    }

    /// + - * / where at least one side is complex.
    pub fn cplx_arith(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        for v in [&a, &b] {
            if matches!(v.ty, Ty::List(_) | Ty::Mat { .. } | Ty::Vec { .. }) {
                let what = match v.ty {
                    Ty::List(_) => "a list",
                    Ty::Mat { .. } => "a matrix",
                    _ => "a vector",
                };
                return Err(self.err(format!("can't mix complex numbers and {what} in arithmetic"), e.span,
                                    Some("lists, vectors and matrices of complex numbers aren't supported yet".into())));
            }
        }
        let line = e.span.line;
        let op = if op == "×" { "*" } else { op };
        let sf = minsf(&[&a, &b]);
        let (da, db) = (dim(&a), dim(&b));
        let mut r = match op {
            "+" | "-" => {
                if !self.u.unify(&da, &db) {
                    let (sa, sb) = (self.desc(&da), self.desc(&db));
                    let msg = if op == "+" { format!("can't add {sa} to {sb}") } else { format!("can't subtract {sb} from {sa}") };
                    return Err(self.err(msg, e.span,
                                        Some("both sides of + and - must have the same units (for complex numbers \
                                              too)".into())));
                }
                self.no_affine(&a, e)?;
                self.no_affine(&b, e)?;
                let hint = a.hint.clone().or_else(|| b.hint.clone());
                let bop = if op == "+" { I::BinOp::Add } else { I::BinOp::Sub };
                let mut r = ir(I::ExprKind::Bin(bop, Box::new(promote(a)), Box::new(promote(b))), Ty::Complex(da), line);
                r.hint = hint;
                r
            }
            "*" => {
                let ty = Ty::Complex(da.mul(&db));
                let hint = self.keep_hint(&a, &b);
                let mut r = if is_c(&a) && is_c(&b) {
                    builtin_ir("c.mul", vec![a, b], ty, line)
                } else {
                    ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(a), Box::new(b)), ty, line)
                };
                r.hint = hint;
                r
            }
            "/" => {
                let ty = Ty::Complex(da.div(&db));
                let hint = if self.dimless(&b) && a.hint.is_some() && b.hint.is_none() { a.hint.clone() } else { None };
                let mut r = if is_c(&b) {
                    builtin_ir("c.div", vec![promote(a), b], ty, line)
                } else {
                    ir(I::ExprKind::Bin(I::BinOp::Div, Box::new(a), Box::new(b)), ty, line)
                };
                r.hint = hint;
                r
            }
            _ => return Err(self.err(format!("unknown operator {op}"), e.span, None)),
        };
        r.sf = sf;
        Ok(r)
    }

    /// a^p where the base a or the exponent is complex.
    pub fn cplx_power(&mut self, e: &A::Expr, a: I::Expr, b: Option<I::Expr>, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::BinOp { left, right, .. } = &e.kind else { unreachable!() };
        let line = e.span.line;
        let p = const_exponent(right);
        if let (Some(p), true) = (p, is_c(&a)) {
            let pf = *p.numer() as f64 / *p.denom() as f64;
            let d = dim(&a).pow(p);
            let (name, extra) = if p.is_integer() && p.abs() <= Rational64::from_integer(64) {
                ("c.powi", true)
            } else if p == Rational64::new(1, 2) {
                ("c.sqrt", false)
            } else {
                ("c.powr", true)
            };
            let (sf, hint) = (a.sf, a.hint.clone());
            let mut args = vec![a];
            if extra {
                args.push(konst(pf));
            }
            let mut r = builtin_ir(name, args, Ty::Complex(d), line);
            r.sf = sf;
            if p == Rational64::from_integer(1) {
                r.hint = hint;
            }
            return Ok(r);
        }
        let b = match b {
            Some(b) => b,
            None => self.expr(right, ctx)?,
        };
        self.need_numlike(&b, right, "the exponent", true)?;
        if !(is_c(&b) || matches!(b.ty, Ty::Num(_))) {
            return Err(self.err("the exponent must be a number", right.span, None));
        }
        let bd = dim(&b);
        if !self.u.unify(&bd, &DExpr::dimless()) {
            return Err(self.err(format!("an exponent must be a plain number, but this is {}", self.desc(&bd)),
                                right.span, None));
        }
        if !matches!(a.ty, Ty::Num(_) | Ty::Complex(_)) {
            return Err(self.err("the base of a complex power must be a number", left.span, None));
        }
        let ad = dim(&a);
        if !self.u.unify(&ad, &DExpr::dimless()) {
            return Err(self.err(format!("can't raise {} to a power that isn't a fixed number", self.desc(&ad)), e.span,
                                Some("with units, the exponent must be a number written in the program, like z^2".into())));
        }
        Ok(cbi("c.pow", vec![promote(a), promote(b)], Ty::Complex(DExpr::dimless()), line))
    }

    /// A call of a built-in function with a complex argument, or of re/im/conj/arg/complex/polar/cis.
    pub fn cplx_builtin(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let n = args.len();
        let line = e.span.line;
        let need = |c: &Checker, k: usize| -> CResult<()> {
            if n != k {
                return Err(c.err(format!("{name} takes {k} argument{} but was given {n}", if k != 1 { "s" } else { "" }),
                                 e.span, None));
            }
            Ok(())
        };
        for (i, a) in args.iter().enumerate() {
            if !matches!(a.ty, Ty::Num(_) | Ty::Complex(_)) {
                let got = match a.ty {
                    Ty::List(_) => "a list",
                    Ty::Vec { .. } => "a vector",
                    Ty::Mat { .. } => "a matrix",
                    _ => "this value",
                };
                return Err(self.err(format!("{name} needs a number or a complex number, but got {got}"), arg_span(e, i),
                                    Some("lists, vectors and matrices of complex numbers aren't supported yet".into())));
            }
        }
        if name == "complex" {
            need(self, 2)?;
            for (i, a) in args.iter().enumerate() {
                if is_c(a) {
                    return Err(self.err("complex(a, b) takes two real numbers: the real and imaginary parts",
                                        arg_span(e, i), None));
                }
            }
            let (d0, d1) = (dim(&args[0]), dim(&args[1]));
            if !self.u.unify(&d0, &d1) {
                return Err(self.err(format!("complex(a, b) needs both parts in the same units, but they are {} and {}",
                                            self.desc(&d0), self.desc(&d1)), e.span, None));
            }
            let hint = args[0].hint.clone().or_else(|| args[1].hint.clone());
            let sf = minsf(&args.iter().collect::<Vec<_>>());
            let mut r = ir(I::ExprKind::Vec(args), Ty::Complex(d0), line);
            r.hint = hint;
            r.sf = sf;
            return Ok(r);
        }
        if name == "polar" || name == "cis" {
            need(self, if name == "polar" { 2 } else { 1 })?;
            for (i, a) in args.iter().enumerate() {
                if is_c(a) {
                    return Err(self.err(format!("{name} takes real numbers"), arg_span(e, i), None));
                }
            }
            let th = args.last().unwrap();
            let thd = dim(th);
            if !self.u.unify(&thd, &DExpr::dimless()) {
                return Err(self.err(format!("the angle in {name} must be a plain number (radians or degrees), but it is \
                                             {}", self.desc(&thd)), arg_span(e, n - 1), None));
            }
            let mut args = args;
            let th = args.pop().unwrap();
            let rr = if name == "polar" { args.pop().unwrap() } else { konst(1.0) };
            let (rd, hint) = (dim(&rr), rr.hint.clone());
            let mut r = cbi("c.polar", vec![rr, th], Ty::Complex(rd), line);
            r.hint = hint;
            return Ok(r);
        }
        if !crate::builtins::COMPLEX_FUNCS.contains(&name) && !crate::builtins::COMPLEX_MATH.contains(&name)
            && name != "abs" {
            return Err(self.err(format!("{name} doesn't work on complex numbers"), e.span,
                                Some("complex numbers work with + - * / ^, exp, ln, sqrt, sin, cos, tan, sinh, cosh, \
                                      tanh, abs, arg, re, im and conj".into())));
        }
        need(self, 1)?;
        let z = args.into_iter().next().unwrap();
        if name == "re" || name == "im" {
            let z = promote(z);
            let (d, hint, sf) = (dim(&z), z.hint.clone(), z.sf);
            let mut r = ir(I::ExprKind::VecElem(Box::new(z), usize::from(name == "im")), Ty::Num(d), line);
            r.hint = hint;
            r.sf = sf;
            return Ok(r);
        }
        if name == "abs" {
            let (d, hint) = (dim(&z), z.hint.clone());
            let mut r = cbi("c.abs", vec![z], Ty::Num(d), line);
            r.hint = hint;
            return Ok(r);
        }
        if name == "arg" {
            return Ok(cbi("c.arg", vec![promote(z)], Ty::Num(DExpr::dimless()), line));
        }
        if name == "conj" {
            let (d, hint) = (dim(&z), z.hint.clone());
            let mut r = cbi("c.conj", vec![promote(z)], Ty::Complex(d), line);
            r.hint = hint;
            return Ok(r);
        }
        if name == "sqrt" {
            let d = dim(&z).pow(Rational64::new(1, 2));
            return Ok(cbi("c.sqrt", vec![promote(z)], Ty::Complex(d), line));
        }
        if crate::builtins::COMPLEX_MATH.contains(&name) {
            let d = dim(&z);
            if !self.u.unify(&d, &DExpr::dimless()) {
                return Err(self.err(format!("{name} needs a plain number, but got a complex number of {}", self.desc(&d)),
                                    arg_span(e, 0), Some("the argument of exp, ln, sin, ... must be a plain number".into())));
            }
            let k = if name == "log" { "ln" } else { name };
            return Ok(cbi(&format!("c.{k}"), vec![z], Ty::Complex(DExpr::dimless()), line));
        }
        Err(self.err(format!("{name} doesn't work on complex numbers"), e.span, None))
    }

    fn cplx_compare_core(&mut self, op: &str, a: I::Expr, b: I::Expr, e: &A::Expr, tols: Option<(I::Expr, I::Expr)>)
                         -> CResult<I::Expr> {
        let A::ExprKind::Compare { left, right, .. } = &e.kind else { unreachable!() };
        if matches!(op, "<" | ">" | "<=" | ">=") {
            return Err(self.err(format!("complex numbers can't be compared with {op} (they aren't ordered)"), e.span,
                                Some("compare their sizes |z| or their real parts re(z) instead".into())));
        }
        for (v, node) in [(&a, left), (&b, right)] {
            if !matches!(v.ty, Ty::Num(_) | Ty::Complex(_)) {
                return Err(self.err("each side of a comparison must be a number", node.span, None));
            }
        }
        let (da, db) = (dim(&a), dim(&b));
        if !self.u.unify(&da, &db) {
            return Err(self.err(format!("can't compare {} with {}", self.desc(&da), self.desc(&db)), e.span, None));
        }
        let name = match op {
            "==" => "c.eq",
            "!=" => "c.ne",
            _ => "c.approx",
        };
        let mut args = vec![promote(a), promote(b)];
        if let Some((atol, rtol)) = tols {
            args.push(atol);
            args.push(rtol);
        }
        Ok(builtin_ir(name, args, Ty::Bool, e.span.line))
    }

    pub fn cplx_compare(&mut self, op: I::CmpOp, a: I::Expr, b: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        let op = match op {
            I::CmpOp::Eq => "==",
            I::CmpOp::Ne => "!=",
            I::CmpOp::Lt => "<",
            I::CmpOp::Gt => ">",
            I::CmpOp::Le => "<=",
            I::CmpOp::Ge => ">=",
        };
        self.cplx_compare_core(op, a, b, e, None)
    }

    pub fn cplx_approx(&mut self, a: I::Expr, b: I::Expr, e: &A::Expr, atol: I::Expr, rtol: I::Expr)
                       -> CResult<I::Expr> {
        self.cplx_compare_core("~=", a, b, e, Some((atol, rtol)))
    }

    /// (3 + 4i) Ω: a complex number times a unit.
    pub fn cplx_quantity(&mut self, v: I::Expr, u: &Unit, e: &A::Expr) -> CResult<I::Expr> {
        if u.affine() {
            return Err(self.err("°C/°F can't be used for complex numbers", e.span,
                                Some("write temperatures in K".into())));
        }
        let vd = dim(&v);
        let n = self.u.norm(&vd);
        if n.is_concrete() && !n.konst.is_dimensionless() {
            return Err(self.err(format!("this already has units ({})", self.desc(&vd)), e.span, None));
        }
        self.u.unify(&vd, &DExpr::dimless());
        let A::ExprKind::Quantity { value, .. } = &e.kind else { unreachable!() };
        let direct = u8::from(value.attrs.imag_literal == Some(true));
        let sf = v.sf;
        let mut r = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(v), Box::new(konst(u.factor))),
                       Ty::Complex(DExpr::of(u.dim)), e.span.line);
        r.hint = Some(crate::exprs::hint_of(u));
        r.sf = sf;
        r.direct = direct;
        Ok(r)
    }

}
