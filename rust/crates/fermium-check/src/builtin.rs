//! The built-in functions: a port of Checker.builtin and _bi from `fermium/checker.py` (math functions, special
//! functions, min/max, list functions, linspace/zeros/range, interp, trapz, str, to, clock). The parts that
//! belong to other areas (vectors and matrices, complex numbers, random numbers, FFT, uncertainty, solutions)
//! call their modules' methods.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use num_rational::Rational64;

use crate::arith::{minsf, py_g};
use crate::builtins::{COMPLEX_FUNCS, LIST_FUNCS, MATH1, SAME1, SPECIAL1, SPECIAL2, UNC_FUNCS};
use crate::checker::*;
use crate::stmts::ty_dim;

/// The largest matrix is 16×16, the longest vector 16 components (linalg_big.MAX_DIM).
pub const MAX_DIM: usize = 16;

fn same_class(t: &Ty, d: DExpr) -> Ty {
    if matches!(t, Ty::List(_)) { Ty::List(d) } else { Ty::Num(d) }
}

impl Checker {
    /// An IR built-in call whose figures are the fewest of `sfargs` (Python _bi).
    pub fn bi(&self, name: &str, args: Vec<I::Expr>, ty: Ty, sfargs: &[&I::Expr], line: u32) -> I::Expr {
        let sf = minsf(sfargs);
        let mut r = ir(I::ExprKind::Builtin(name.to_string(), args), ty, line);
        r.sf = sf;
        r
    }

    fn dim_of(&self, v: &I::Expr) -> DExpr {
        ty_dim(&v.ty).unwrap_or_else(DExpr::dimless)
    }

    pub fn builtin(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        self.builtin_val(name, e, ctx).map(Checked::Val)
    }

    fn builtin_val(&mut self, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        let A::ExprKind::Call { args: eargs, .. } = &e.kind else { unreachable!() };
        let line = e.span.line;
        if matches!(name, "sin" | "cos" | "tan" | "cot" | "sec" | "csc") && eargs.len() == 1 {
            if let A::ExprKind::Num { value, digit: true, .. } = eargs[0].kind {
                if value >= 10.0 && value == value.trunc() {
                    let v = value as i64;
                    self.warn(format!("{name}({v}) is the {name} of {v} radians"), e.span,
                              Some(format!("for degrees write {name}({v}°)")));
                }
            }
        }
        if matches!(name, "push" | "append") {
            return Err(self.err(format!("{name}(list, value) changes a list and doesn't give a value; write it on its \
                                         own line"), e.span, None));
        }
        if name == "to" {
            return self.builtin_to(e, eargs, ctx);
        }
        if matches!(name, "row" | "column") {
            return self.row_column(name, e, ctx);
        }
        if UNC_FUNCS.contains(&name) {
            return self.unc_part(name, e, ctx);
        }
        if name == "times" && eargs.len() == 1 {
            if let Checked::Sol(view) = self.expr_any(&eargs[0], ctx)? {
                let sol_sym = self.sols[view].sol_sym;
                let s = self.var_ref(sol_sym, ctx, e)?;
                let I::ExprKind::Var(sym) = s.kind else { unreachable!() };
                let (comp, tdim) = (self.sols[view].comp, self.sols[view].tdim.clone());
                let mut r = ir(I::ExprKind::SolList { sol: sym, comp, what: 1 }, Ty::List(tdim), line);
                r.hint = self.sols[view].thint.clone();
                return Ok(r);
            }
        }
        let mut args: Vec<I::Expr> = vec![];
        for a in eargs {
            let v = match self.expr_any(a, ctx)? {
                Checked::Sol(view) => self.sol_values(view, a)?,
                Checked::Func { info, .. } => {
                    let dn = self.funcs[info].display_name.clone();
                    return Err(self.err(format!("{dn} is a function; give it an argument"), a.span, None));
                }
                Checked::Val(v) => v,
            };
            args.push(v);
        }
        let n = args.len();
        if name == "fill" {
            return self.array_fill(e, args, eargs); // N-dimensional arrays (D283)
        }
        if args.first().is_some_and(|a| matches!(a.ty, Ty::Array { .. })) {
            return self.array_call(name, args, e);
        }
        if name == "copy" {
            return Err(self.err("copy(A) is a new array with the same entries (made with fill(value, n1, n2, …))",
                                e.span, None));
        }
        if name == "size" {
            return Err(self.err("size(A) is the shape of an array (made with fill(value, n1, n2, …)); for a list use \
                                 len(xs)", e.span, None));
        }
        if name == "str" {
            // str(x): a number as text, as print shows it (its unit and digits), for labels (D216)
            if n != 1 {
                return Err(self.err(format!("str takes 1 argument but was given {n}"), e.span, None));
            }
            let v = args.pop().unwrap();
            if matches!(v.ty, Ty::Str) {
                return Ok(v);
            }
            if !matches!(v.ty, Ty::Num(_)) {
                return Err(self.err(format!("str(x) turns a single number into text, not {}", self.type_desc(&v.ty)),
                                    eargs[0].span, None));
            }
            let fid = self.fmt(&v);
            return Ok(ir(I::ExprKind::Builtin("text_num".into(), vec![ir(I::ExprKind::Const(fid as f64), dimless_num(),
                                                                          line), v]), Ty::Str, line));
        }
        if name == "fft" || (name == "ifft" && n == 1) || args.iter().any(|a| matches!(a.ty, Ty::ComplexList(_)))
            || (name == "complex" && n == 2 && args.iter().any(|a| matches!(a.ty, Ty::List(_))))
        {
            return self.clist_call(name, args, e); // lists of complex numbers (D243)
        }
        if COMPLEX_FUNCS.contains(&name) || args.iter().any(|a| matches!(a.ty, Ty::Complex(_))) {
            return self.cplx_builtin(name, args, e);
        }
        let need = |c: &Checker, k: usize| -> CResult<()> {
            if n != k {
                return Err(c.err(format!("{name} takes {k} argument{} but was given {n}", if k != 1 { "s" } else { "" }),
                                 e.span, None));
            }
            Ok(())
        };
        let dimless = DExpr::of(DIMLESS);
        if MATH1.contains(&name) {
            need(self, 1)?;
            self.need_numlike(&args[0], &eargs[0], &format!("the argument of {name}"), false)?;
            let d = self.dim_of(&args[0]);
            if !self.u.unify(&d, &dimless) {
                let hint = if matches!(name, "ln" | "log" | "log10" | "log2") {
                    "divide by a reference value first, e.g. ln(p / (1 atm))"
                } else {
                    "the argument of sin, cos, exp, ... must be a plain number (angles in rad are plain numbers)"
                };
                return Err(self.err(format!("{name} needs a plain number, but got {}", self.desc(&d)), eargs[0].span,
                                    Some(hint.into())));
            }
            let ty = same_class(&args[0].ty, dimless);
            let a0 = args[0].clone();
            return Ok(self.bi(name, args, ty, &[&a0], line));
        }
        if matches!(name, "sqrt" | "cbrt") {
            need(self, 1)?;
            self.need_numlike(&args[0], &eargs[0], &format!("the argument of {name}"), false)?;
            let p = if name == "sqrt" { Rational64::new(1, 2) } else { Rational64::new(1, 3) };
            let a = args.pop().unwrap();
            let ty = same_class(&a.ty, self.dim_of(&a).pow(p));
            let sf = a.sf;
            let mut r = ir(I::ExprKind::PowC(Box::new(a), if name == "sqrt" { 0.5 } else { 1.0 / 3.0 }), ty, line);
            r.sf = sf;
            return Ok(r);
        }
        if SPECIAL2.contains(&name) || SPECIAL1.contains(&name) {
            let k = if SPECIAL2.contains(&name) { 2 } else { 1 };
            need(self, k)?;
            let what: &[&str] = if k == 2 { &["the order n", "x"] } else { &["m"] };
            for i in 0..k {
                self.need_num(&args[i], &eargs[i], &format!("{} in {name}", what[i]))?;
                let d = self.dim_of(&args[i]);
                if !self.u.unify(&d, &dimless) {
                    return Err(self.err(format!("{name} needs plain numbers, but {} is {}", what[i], self.desc(&d)),
                                        eargs[i].span,
                                        Some("divide by a unit or a scale first, like besselj(1, k r) with k in 1/m and \
                                              r in m".into())));
                }
            }
            if k == 2 {
                if let I::ExprKind::Const(v) = args[0].kind {
                    if v != v.trunc() {
                        return Err(self.err(format!("{name}(n, x) needs a whole-number order n, not {}", py_g(v)),
                                            eargs[0].span, None));
                    }
                }
            }
            let sfa: Vec<I::Expr> = args[k - 1..].to_vec();
            return Ok(self.bi(name, args, Ty::Num(dimless), &sfa.iter().collect::<Vec<_>>(), line));
        }
        if name == "abs" && n == 1 && matches!(args[0].ty, Ty::Vec { .. } | Ty::Mat { .. }) {
            return self.vec_builtin(name, args, e, ctx);
        }
        if matches!(name, "trace" | "angle") {
            return self.vec_builtin(name, args, e, ctx);
        }
        if SAME1.contains(&name) {
            need(self, 1)?;
            self.need_numlike(&args[0], &eargs[0], &format!("the argument of {name}"), false)?;
            let d = self.dim_of(&args[0]);
            if name != "abs" && !self.u.unify(&d, &dimless) {
                return Err(self.err(format!("{name} of {} would depend on which unit you mean", self.desc(&d)), e.span,
                                    Some(format!("divide by a unit first, e.g.  {name}(x / (1 cm)) cm"))));
            }
            let (ty, hint, a0) = (args[0].ty.clone(), args[0].hint.clone(), args[0].clone());
            let mut r = self.bi(name, args, ty, &[&a0], line);
            r.hint = hint;
            if name != "abs" {
                r.sf = None; // a whole number: print it exactly
            }
            return Ok(r);
        }
        if name == "sign" && n == 1 && matches!(args[0].ty, Ty::Vec { .. }) {
            // sign(v) = v/|v|, the direction (A18)
            return self.vec_builtin(name, args, e, ctx);
        }
        if matches!(name, "sign" | "isnan") {
            need(self, 1)?;
            self.need_numlike(&args[0], &eargs[0], &format!("the argument of {name}"), false)?;
            let ty = if name == "sign" { Ty::Num(dimless) } else { Ty::Bool };
            let a0 = args[0].clone();
            return Ok(self.bi(name, args, ty, &[&a0], line));
        }
        if name == "atan2" {
            need(self, 2)?;
            for i in 0..2 {
                self.need_num(&args[i], &eargs[i], "this value")?;
            }
            let (d0, d1) = (self.dim_of(&args[0]), self.dim_of(&args[1]));
            self.unify_or(&d0, &d1, |_| "atan2(y, x) needs y and x in the same units".into(), e.span, None)?;
            let sfa = args.clone();
            return Ok(self.bi(name, args, Ty::Num(dimless), &sfa.iter().collect::<Vec<_>>(), line));
        }
        if matches!(name, "hypot" | "mod") {
            need(self, 2)?;
            for i in 0..2 {
                self.need_num(&args[i], &eargs[i], "this value")?;
            }
            let (d0, d1) = (self.dim_of(&args[0]), self.dim_of(&args[1]));
            self.unify_or(&d0, &d1, |_| format!("{name} needs both values in the same units"), e.span, None)?;
            let (hint, sfa) = (args[0].hint.clone(), args.clone());
            let mut r = self.bi(name, args, Ty::Num(d0), &sfa.iter().collect::<Vec<_>>(), line);
            r.hint = hint;
            return Ok(r);
        }
        if name == "clamp" {
            need(self, 3)?;
            let d0 = self.dim_of(&args[0]);
            for i in 0..3 {
                self.need_num(&args[i], &eargs[i], "this value")?;
                let di = self.dim_of(&args[i]);
                self.unify_or(&d0, &di, |_| "clamp needs all values in the same units".into(), e.span, None)?;
            }
            let (hint, sfa) = (args[0].hint.clone(), args.clone());
            let mut r = self.bi(name, args, Ty::Num(d0), &sfa.iter().collect::<Vec<_>>(), line);
            r.hint = hint;
            return Ok(r);
        }
        if matches!(name, "min" | "max") {
            if n == 1 {
                if !matches!(args[0].ty, Ty::List(_)) {
                    return Err(self.err(format!("{name} of a single value needs a list"), e.span, None));
                }
                let (d, hint, a0) = (self.dim_of(&args[0]), args[0].hint.clone(), args[0].clone());
                let mut r = self.bi(&format!("{name}_list"), args, Ty::Num(d), &[&a0], line);
                r.hint = hint;
                return Ok(r);
            }
            if n < 2 {
                return Err(self.err(format!("{name} needs at least one argument"), e.span, None));
            }
            let d0 = self.dim_of(&args[0]);
            if args.iter().any(|a| matches!(a.ty, Ty::List(_))) {
                // max(xs, 1e-12): element by element, lists of one length and numbers mixed (D162)
                for i in 0..n {
                    self.need_numlike(&args[i], &eargs[i], &format!("an argument of {name}"), false)?;
                    let di = self.dim_of(&args[i]);
                    self.unify_or(&d0, &di, |c| format!("{name} needs all values in the same units (here {} and {})",
                                                        c.desc(&d0), c.desc(&di)), eargs[i].span, None)?;
                }
                let hint = args.iter().find(|a| matches!(a.ty, Ty::List(_))).and_then(|a| a.hint.clone());
                let sfa = args.clone();
                let mut r = self.bi(&format!("{name}_ew"), args, Ty::List(d0), &sfa.iter().collect::<Vec<_>>(), line);
                r.hint = hint;
                return Ok(r);
            }
            for i in 0..n {
                self.need_num(&args[i], &eargs[i], "this value")?;
                let di = self.dim_of(&args[i]);
                self.unify_or(&d0, &di, |_| format!("{name} needs all values in the same units"), eargs[i].span, None)?;
            }
            let (hint, sfa) = (args[0].hint.clone(), args.clone());
            let mut r = self.bi(name, args, Ty::Num(d0), &sfa.iter().collect::<Vec<_>>(), line);
            r.hint = hint;
            return Ok(r);
        }
        if matches!(name, "factorial" | "gamma") {
            need(self, 1)?;
            self.need_num(&args[0], &eargs[0], "this value")?;
            let d = self.dim_of(&args[0]);
            self.u.unify(&d, &dimless);
            let a0 = args[0].clone();
            return Ok(self.bi(name, args, Ty::Num(dimless), &[&a0], line));
        }
        if matches!(name, "len" | "sum" | "mean") && n == 1 {
            if let Ty::VList(el) = &args[0].ty {
                // a list of vectors or matrices (D281): its length, or the vector (matrix) sum or mean
                let (ty, hint) = if name == "len" { (Ty::Num(dimless.clone()), None) } else { ((**el).clone(), args[0].hint.clone()) };
                let mut r = self.bi(name, args, ty, &[], line);
                r.sf = None;
                r.hint = hint;
                return Ok(r);
            }
        }
        if name == "len" && n == 1 && matches!(args[0].ty, Ty::TextList) {
            let mut r = self.bi("len", args, Ty::Num(dimless), &[], line);
            r.sf = None;
            return Ok(r);
        }
        if LIST_FUNCS.contains(&name) || matches!(name, "values" | "times") {
            need(self, 1)?;
            let a = args[0].clone();
            if !matches!(a.ty, Ty::List(_)) {
                return Err(self.err(format!("{name} needs a list, but got {}", self.type_desc(&a.ty)), eargs[0].span,
                                    None));
            }
            let d = self.dim_of(&a);
            let affine = crate::arith::is_affine(&a.hint);
            if name == "len" {
                let mut r = self.bi("len", args, Ty::Num(dimless), &[&a], line);
                r.sf = None; // a count is exact
                return Ok(r);
            }
            if matches!(name, "sum" | "cumsum") && affine {
                let hn = a.hint.as_ref().unwrap().name.clone();
                return Err(self.err(format!("can't add absolute temperatures: {name} of a list in {hn} would add them \
                                             as kelvins (10 {hn} + 20 {hn} isn't 30 {hn})"), e.span,
                                    Some("mean(...) works; to add temperature changes, write them in K".into())));
            }
            if matches!(name, "sum" | "mean" | "first" | "last") {
                let mut r = self.bi(name, args, Ty::Num(d), &[&a], line);
                r.hint = a.hint.clone();
                return Ok(r);
            }
            if name == "std" {
                let mut r = self.bi(name, args, Ty::Num(d), &[&a], line);
                r.hint = if affine { None } else { a.hint.clone() }; // a spread: K, not °C
                return Ok(r);
            }
            if matches!(name, "cumsum" | "diff" | "reverse" | "sort" | "values") {
                let bname = if name == "values" { "copy" } else { name };
                let mut r = self.bi(bname, args, Ty::List(d), &[&a], line);
                r.hint = a.hint.clone();
                if matches!(name, "diff" | "cumsum") && affine {
                    r.hint = None; // differences of temperatures are shown in K
                }
                return Ok(r);
            }
            if name == "times" {
                if let I::ExprKind::SolList { sol, comp, .. } = a.kind {
                    let tdim = self.sols.iter().find(|v| v.sol_sym == sol).map(|v| v.tdim.clone())
                        .unwrap_or_else(|| DExpr::of(fermium_units::TIME));
                    return Ok(ir(I::ExprKind::SolList { sol, comp, what: 1 }, Ty::List(tdim), line));
                }
                return Err(self.err("times(...) needs an ODE solution", e.span, None));
            }
        }
        if matches!(name, "norm" | "unit" | "hat" | "cross" | "vec")
            || (name == "dot" && n == 2 && args.iter().all(|a| matches!(a.ty, Ty::Vec { .. })))
            || matches!(name, "transpose" | "det" | "inverse" | "solve_linear" | "eigenvalues" | "eigenvectors")
        {
            return self.vec_builtin(name, args, e, ctx);
        }
        if name == "identity" {
            need(self, 1)?;
            let k = match (&args[0].kind, &args[0].ty) {
                (I::ExprKind::Const(k), Ty::Num(_)) => *k,
                _ => {
                    return Err(self.err("identity(n) needs a fixed whole number, like identity(3)", eargs[0].span, None));
                }
            };
            if k != k.trunc() || !(2.0..=MAX_DIM as f64).contains(&k) {
                return Err(self.err(format!("identity(n) needs a whole number n from 2 to {MAX_DIM} (matrices are at \
                                             most {MAX_DIM}×{MAX_DIM})"), eargs[0].span, None));
            }
            let m = k as usize;
            let items = (0..m * m)
                .map(|x| ir(I::ExprKind::Const(if x / m == x % m { 1.0 } else { 0.0 }), dimless_num(), line))
                .collect();
            return Ok(ir(I::ExprKind::Vec(items), Ty::Mat { r: m, c: m, dim: dimless }, line));
        }
        if matches!(name, "trapz" | "dot") {
            need(self, 2)?;
            for i in 0..2 {
                if !matches!(args[i].ty, Ty::List(_)) {
                    let msg = if name == "trapz" { "trapz(ys, xs) needs two lists" } else { "dot(a, b) needs two lists" };
                    return Err(self.err(msg, e.span, None));
                }
            }
            let d = self.dim_of(&args[0]).mul(&self.dim_of(&args[1]));
            let sfa = args.clone();
            return Ok(self.bi(name, args, Ty::Num(d), &sfa.iter().collect::<Vec<_>>(), line));
        }
        if name == "interp" {
            need(self, 3)?;
            self.need_num(&args[0], &eargs[0], "this value")?;
            if !matches!(args[1].ty, Ty::List(_)) || !matches!(args[2].ty, Ty::List(_)) {
                return Err(self.err("interp(x, xs, ys) needs a value and two lists", e.span, None));
            }
            let (dx, dxs, dys) = (self.dim_of(&args[0]), self.dim_of(&args[1]), self.dim_of(&args[2]));
            self.unify_or(&dx, &dxs, |_| "interp: x and xs need the same units".into(), e.span, None)?;
            let sfa = args.clone();
            return Ok(self.bi(name, args, Ty::Num(dys), &sfa.iter().collect::<Vec<_>>(), line));
        }
        if name == "linspace" {
            need(self, 3)?;
            for i in 0..3 {
                self.need_num(&args[i], &eargs[i], "this value")?;
            }
            let (d0, d1, d2) = (self.dim_of(&args[0]), self.dim_of(&args[1]), self.dim_of(&args[2]));
            self.unify_or(&d0, &d1, |_| "linspace(a, b, n): a and b need the same units".into(), e.span, None)?;
            self.unify_or(&d2, &dimless, |_| "linspace(a, b, n): n must be a plain number".into(), eargs[2].span, None)?;
            let hint = args[0].hint.clone().or_else(|| args[1].hint.clone());
            let sfa = args.clone();
            let mut r = self.bi(name, args, Ty::List(d0), &sfa.iter().collect::<Vec<_>>(), line);
            r.hint = hint;
            return Ok(r);
        }
        if name == "zeros" && n == 2 {
            return self.vec_builtin(name, args, e, ctx);
        }
        if matches!(name, "zeros" | "ones") {
            need(self, 1)?;
            self.need_num(&args[0], &eargs[0], "this value")?;
            let d = self.dim_of(&args[0]);
            self.unify_or(&d, &dimless, |_| format!("{name}(n): n must be a plain number"), eargs[0].span, None)?;
            let ld = if name == "zeros" { DExpr::fresh() } else { dimless };
            let a0 = args[0].clone();
            return Ok(self.bi(name, args, Ty::List(ld), &[&a0], line));
        }
        if name == "range" {
            if n != 2 && n != 3 {
                return Err(self.err("range needs a start and an end: range(a, b) or range(a, b, step)", e.span,
                                    Some("to count, write  for i from 1 to 10".into())));
            }
            let d0 = self.dim_of(&args[0]);
            for i in 0..n {
                self.need_num(&args[i], &eargs[i], "this value")?;
                let di = self.dim_of(&args[i]);
                self.unify_or(&d0, &di, |_| "range: all values need the same units".into(), e.span, None)?;
            }
            if n == 2 {
                args.push(ir(I::ExprKind::Const(1.0), Ty::Num(d0.clone()), line));
            }
            let sfa = args.clone();
            return Ok(self.bi(name, args, Ty::List(d0), &sfa.iter().collect::<Vec<_>>(), line));
        }
        if name == "clock" {
            if n != 0 {
                return Err(self.err("clock() takes no arguments", e.span, None));
            }
            let r = self.bi(name, args, Ty::Num(DExpr::of(fermium_units::TIME)), &[], line);
            return Ok(self.seconds_here(r)); // seconds -> natural units (D60)
        }
        if matches!(name, "rand" | "randn") {
            return self.m3_random(name, args, e);
        }
        if name == "seed" {
            return Err(self.err("seed(n) is a statement on its own line, like  seed(42)", e.span, None));
        }
        if name == "sample" {
            return self.m3_sample(e, ctx);
        }
        if matches!(name, "fft_re" | "fft_im" | "ifft" | "amplitude_spectrum" | "power_spectrum" | "frequencies"
                    | "argmax" | "argmin")
        {
            return self.m3_fourier(name, args, e);
        }
        Err(self.err(format!("{name} can't be used this way"), e.span, None))
    }

    /// to(x, eV): x shown in a unit written as a name (D216).
    fn builtin_to(&mut self, e: &A::Expr, eargs: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        if eargs.len() != 2 {
            return Err(self.err("to needs a value and a unit: to(x, eV)", e.span, None));
        }
        let text = crate::source::to_source(&eargs[1]);
        let u = match crate::units::parse_unit_string(&text) {
            Ok(u) => self.nat.canon_unit(&u),
            Err(ex) => return Err(self.err(ex.to_string(), eargs[1].span, None)),
        };
        let mut v = self.expr(&eargs[0], ctx)?;
        if matches!(v.ty, Ty::Vec { dims: Some(_), .. }) {
            return Err(self.err(format!("can't show a vector with different units per component in {}", u.name),
                                e.span, Some("convert one component at a time, like to(s.x, cm)".into())));
        }
        if !matches!(v.ty, Ty::Num(_) | Ty::List(_) | Ty::Vec { .. } | Ty::Mat { .. } | Ty::Complex(_)) {
            return Err(self.err("to(x, unit) needs a number", e.span, None));
        }
        let d = self.dim_of(&v);
        if !self.u.unify(&d, &DExpr::of(u.dim)) {
            return Err(self.err(format!("can't show {} in {} ({})", self.desc(&d), u.name,
                                        self.desc(&DExpr::of(u.dim))), e.span, None));
        }
        v.hint = Some(crate::exprs::hint_of(&u));
        if matches!(v.direct, 4 | 5) {
            v.direct = 1;
        }
        Ok(v)
    }
}
