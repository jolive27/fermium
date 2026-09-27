//! Expressions, part 1: dispatch, literals, quantities, names and variables. A port of Checker.expr, e_Num,
//! e_Str, e_Bool, e_Quantity, e_Name, use_binding, var_ref, _ivar and undefined from `fermium/checker.py`.
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::stmts::ty_dim;
use crate::units::{self, Unit};

pub fn hint_of(u: &Unit) -> I::Hint {
    I::Hint { name: u.name.clone(), factor: u.factor, offset: u.offset, dim: u.dim }
}

impl Checker {
    /// Check an expression that must be a value (Python expr(e, ctx) with allow_func=False).
    pub fn expr(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<I::Expr> {
        match self.expr_any(e, ctx)? {
            Checked::Val(v) => Ok(v),
            other => Err(self.func_as_value_error(&other, e)),
        }
    }

    /// The error for a function or ODE solution used where a value is needed.
    pub fn func_as_value_error(&self, c: &Checked, e: &A::Expr) -> Diagnostic {
        match c {
            Checked::Func { info, name, param } => {
                if *param {
                    return self.err(format!("{name} is a function here; call it like {name}(x)"), e.span, None);
                }
                let dn = &self.funcs[*info].display_name;
                self.err(format!("{dn} is a function; give it an argument, like {dn}(x)"), e.span, None)
            }
            Checked::Sol(view) => {
                let v = &self.sols[*view];
                self.err(format!("{} is the solution of an ODE (a function of {}); use {}({}) for its value at a time",
                                 v.name, v.tname, v.name, v.tname), e.span,
                         Some(format!("e.g. {}(1 s), or values({}) for all computed values", v.name, v.name)))
            }
            Checked::Val(_) => unreachable!(),
        }
    }

    /// Check an expression that may also be a function or a solution (allow_func=True).
    pub fn expr_any(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        // the stack of expressions being checked (Python _estack): raw pointers, valid while each is on the
        // stack, because every expression checked outlives its own check
        self.estack.push(e as *const A::Expr);
        let r = self.expr_dispatch(e, ctx);
        self.estack.pop();
        // a unit power too large to track exactly: the error is at the innermost expression that made it (red
        // team 13), whatever else went wrong after it
        if let Some(err) = self.take_overflow(e.span) {
            return Err(err);
        }
        let mut r = r?;
        if let Checked::Val(v) = &mut r {
            v.line = e.span.line;
        }
        Ok(r)
    }

    fn expr_dispatch(&mut self, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        use A::ExprKind as K;
        let v = match &e.kind {
            K::Num { value, sigfigs, digit } => self.e_num(*value, *sigfigs, *digit),
            K::Str { value: s } => {
                let mut r = ir(I::ExprKind::Str(s.clone()), Ty::Str, e.span.line);
                let t = self.text(s);
                r.extra().text_id = Some(t);
                r
            }
            K::Bool { value: b } => ir(I::ExprKind::Bool(*b), Ty::Bool, e.span.line),
            K::Quantity { value, unit, .. } => self.e_quantity(e, value, unit, ctx)?,
            K::Name { name: n } => return self.e_name(e, n, ctx),
            K::Field { target, name } if self.module_of(target, ctx).is_some() => {
                // mechanics.pendulum_period (D100)
                let m = self.module_of(target, ctx).unwrap();
                return self.module_member(m, e, name, ctx);
            }
            K::BinOp { .. } => return self.e_binop(e, ctx),
            K::Neg { operand: x } => self.e_neg(e, x, ctx)?,
            K::Compare { .. } => self.e_compare(e, ctx)?,
            K::Logic { op, left, right } => self.e_logic(e, op == "and", left, right, ctx)?,
            K::Not { operand: x } => self.e_not(e, x, ctx)?,
            K::IfExpr { cond, then, other } => self.e_if_expr(e, cond, then, other, ctx)?,
            K::Where { value, bindings } => return self.e_where(e, value, bindings, ctx),
            K::Call { .. } => return self.e_call(e, ctx),
            K::Sqrt { operand, root } => self.e_sqrt(e, operand, *root as u32, ctx)?,
            K::Abs { operand: x } => self.e_abs(e, x, ctx)?,
            K::ListLit { items } => self.e_list_lit(e, items, ctx)?,
            K::Index { .. } => self.e_index(e, ctx)?,
            K::Slice { .. } => self.e_slice(e)?,
            K::Uncertain { value, err } => self.e_uncertain(e, value, err, ctx)?,
            K::End => self.e_end(e)?,
            K::Convert { value, unit } => self.e_convert(e, value, unit, ctx)?,
            K::Load { path } => self.e_load(e, path)?,
            K::Table { names, items } => self.e_table(e, names, items, ctx)?,
            K::Digits { value, digits } => self.e_digits(e, value, *digits as u32, ctx)?,
            K::VecLit { items } => self.e_vec_lit(e, items, ctx)?,
            K::Field { target, name } => return self.e_field(e, target, name, ctx),
            K::Prime { target, order } => return self.e_prime(e, target, *order, ctx),
            K::Deriv { .. } => return self.e_deriv(e, ctx),
            K::VecCalc { .. } => return self.e_veccalc(e, ctx),
            K::Integral { .. } => return self.e_integral(e, ctx),
            K::Sum { .. } => return self.e_sum(e, ctx),
            #[allow(unreachable_patterns)]
            _ => return Err(self.not_ported(expr_kind_name(&e.kind), e.span)),
        };
        Ok(Checked::Val(v))
    }

    pub fn e_num(&mut self, value: f64, sigfigs: Option<u32>, digit: bool) -> I::Expr {
        let mut r = if value == 0.0 && digit {
            ir(I::ExprKind::Const(0.0), Ty::Num(DExpr::fresh()), 0) // a plain 0 fits any units
        } else {
            ir(I::ExprKind::Const(value), dimless_num(), 0)
        };
        r.sf = sigfigs;
        // a digit literal prints as written; ½ and ⅓ are exact numbers like (1/2) (D241)
        r.direct = u8::from(digit);
        r
    }

    pub fn e_quantity(&mut self, e: &A::Expr, value: &A::Expr, unit: &A::UnitExpr, ctx: &mut Ctx)
                      -> CResult<I::Expr> {
        let u = self.resolve_unit(unit)?;
        let v = self.expr(value, ctx)?;
        if let Ty::VList(el) = &v.ty {
            // [<1, 2>, <3, 4>] m: the unit of every element (D281)
            let vdim = ty_dim(el).unwrap();
            let vd = self.u.norm(&vdim);
            if u.affine() || (vd.is_concrete() && !vd.konst.is_dimensionless()) {
                return Err(self.err(format!("this already has units ({}), so [{}] would multiply them",
                                            self.desc(&vdim), unit.text), e.span, None));
            }
            self.u.unify(&vdim, &DExpr::of(DIMLESS));
            let dim = DExpr::of(u.dim);
            let elem = match &**el {
                Ty::Vec { n, .. } => Ty::Vec { n: *n, dim: Some(dim), dims: None },
                Ty::Mat { r, c, .. } => Ty::Mat { r: *r, c: *c, dim },
                other => other.clone(),
            };
            let line = e.span.line;
            let (sf, direct) = (v.sf, v.direct);
            let mut r = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(v),
                                            Box::new(ir(I::ExprKind::Const(u.factor), dimless_num(), line))),
                           Ty::VList(Box::new(elem)), line);
            r.hint = Some(hint_of(&u));
            r.sf = sf;
            r.direct = direct;
            return Ok(r);
        }
        self.need_numlike(&v, value, "this value", true)?;
        let line = e.span.line;
        match &v.ty {
            Ty::Complex(_) => return self.cplx_quantity(v, &u, e),
            Ty::Vec { .. } | Ty::Mat { .. } => return self.vec_quantity(e, value, v, &u),
            _ => {}
        }
        let value_is_num = matches!(value.kind, A::ExprKind::Num { .. });
        if e.attrs.times_unit == Some(true) && matches!(v.ty, Ty::Num(_) | Ty::List(_)) && !u.affine() {
            let vd = self.u.norm(&ty_dim(&v.ty).unwrap());
            if !(vd.is_concrete() && vd.konst.is_dimensionless()) {
                // `(a + b) MeV`, `100 h km/s/Mpc` with h Planck's: times 1 unit, like `* 1 MeV` (D215)
                let dim = ty_dim(&v.ty).unwrap().mul(&DExpr::of(u.dim));
                let ty = if matches!(v.ty, Ty::Num(_)) { Ty::Num(dim) } else { Ty::List(dim) };
                let sf = v.sf;
                let mut r = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(v),
                                                Box::new(ir(I::ExprKind::Const(u.factor), dimless_num(), line))),
                               ty, line);
                r.sf = sf;
                return Ok(r);
            }
        }
        if !value_is_num {
            let vdim = ty_dim(&v.ty).unwrap();
            let vd = self.u.norm(&vdim);
            if vd.is_concrete() && !vd.konst.is_dimensionless() {
                return Err(self.err(format!("this already has units ({}), so [{}] would multiply them",
                                            self.desc(&vdim), unit.text), e.span,
                                    Some(format!("to show it in {}, write:  ... in {}", unit.text, unit.text))));
            }
            self.u.unify(&vdim, &DExpr::of(DIMLESS));
        }
        let dim = DExpr::of(u.dim);
        let ty = if matches!(v.ty, Ty::Num(_)) { Ty::Num(dim) } else { Ty::List(dim) };
        let sf = v.sf;
        let mut r = if let I::ExprKind::Const(c) = v.kind {
            ir(I::ExprKind::Const(c * u.factor + u.offset), ty.clone(), line)
        } else {
            let mut r = if u.factor != 1.0 {
                ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(v.clone()),
                                    Box::new(ir(I::ExprKind::Const(u.factor), dimless_num(), line))), ty.clone(), line)
            } else {
                v.clone()
            };
            if u.offset != 0.0 {
                r = ir(I::ExprKind::Bin(I::BinOp::Add, Box::new(r),
                                        Box::new(ir(I::ExprKind::Const(u.offset), dimless_num(), line))), ty.clone(),
                       line);
            }
            if u.factor == 1.0 && u.offset == 0.0 {
                r = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(v.clone()),
                                        Box::new(ir(I::ExprKind::Const(1.0), dimless_num(), line))), ty.clone(), line);
            }
            r
        };
        r.ty = ty;
        r.hint = Some(hint_of(&u));
        r.sf = sf;
        // ½ kg prints like (1/2) kg (D241)
        r.direct = u8::from(matches!(value.kind, A::ExprKind::Num { digit: true, .. }));
        if u.affine() {
            if let A::ExprKind::Num { value: x, .. } = value.kind {
                // `10 °C` written out: see warn_absolute_in_product (#9)
                let ex = r.extra();
                ex.abs_literal = Some((x, hint_of(&u)));
                ex.abs_at = Some((e.span.line, e.span.col));
                self.abs_at_len.insert((e.span.line, e.span.col), e.span.length);
            }
        }
        Ok(r)
    }

    pub fn e_name(&mut self, e: &A::Expr, name: &str, ctx: &mut Ctx) -> CResult<Checked> {
        let Some((b, scope)) = self.lookup(ctx.scope, name) else {
            return Err(self.undefined(name, e, ctx));
        };
        if let Binding::Func(info) = b {
            if self.scopes[scope].kind == "func" {
                // a function passed in as an argument (D43)
                return Ok(Checked::Func { info, name: name.to_string(), param: true });
            }
        }
        self.use_binding(b, name, e, ctx)
    }

    pub fn use_binding(&mut self, b: Binding, name: &str, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let line = e.span.line;
        match b {
            Binding::Sym(s) => {
                let r = self.var_ref(s, ctx, e)?;
                Ok(Checked::Val(self.from_system(s, r, e)?))
            }
            Binding::Const(c) => {
                self.used_consts.insert(name.to_string());
                if name == "∞" {
                    return Ok(Checked::Val(ir(I::ExprKind::Const(f64::INFINITY), Ty::Num(DExpr::fresh()), line)));
                }
                if name == "𝑖" && c.desc.starts_with("the imaginary unit") {
                    let parts = vec![ir(I::ExprKind::Const(0.0), dimless_num(), line),
                                     ir(I::ExprKind::Const(1.0), dimless_num(), line)];
                    return Ok(Checked::Val(ir(I::ExprKind::Vec(parts), Ty::Complex(DExpr::of(DIMLESS)), line)));
                }
                if self.natural() {
                    return self.natural_const(&c, e).map(Checked::Val);
                }
                let mut r = ir(I::ExprKind::Const(c.value), num_ty(c.unit.dim), line);
                if c.unit.name != "1" && self.nat_display() != "astro" {
                    r.hint = Some(hint_of(&c.unit));
                }
                Ok(Checked::Val(r))
            }
            Binding::Func(info) => {
                let dn = self.funcs[info].display_name.clone();
                Ok(Checked::Func { info, name: dn, param: false })
            }
            Binding::Sol(view) => {
                let sol_sym = self.sols[view].sol_sym;
                if self.extra[sol_sym].nat != self.nat {
                    return Err(self.err(format!("{name} was solved for outside this {} region, so it can't be used \
                                                 here", self.nat_label()), e.span,
                                        Some("solve the equation inside the region (or outside, and use it there)".into())));
                }
                self.var_ref(sol_sym, ctx, e)?; // marks capture/global as needed
                Ok(Checked::Sol(view))
            }
            Binding::Local(lf) => {
                let l = &self.local_funcs[lf];
                let A::StmtKind::FuncDef { params, .. } = &l.fdef.kind else { unreachable!() };
                let ps = params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
                Err(self.err(format!("{name} is a function defined inside {}; it can only be called, like {name}({ps})",
                                     l.owner), e.span,
                             Some(format!("to pass it to another function, or to differentiate or integrate it by \
                                           name, define {name} at the top level"))))
            }
            Binding::Module(m) => Err(self.module_as_value(m, name, e)),
            Binding::Pde(p) => Err(self.pde_as_value(p, name, e)),
            Binding::CFunc(c) => Err(self.c_func_as_value(c, name, e)),
            Binding::PyModule(m) => {
                let module = self.py_module_name(m);
                Err(self.err(format!("{name} is the Python module {module}, not a value; call its functions, like \
                                      {name}.f(x)"), e.span, None))
            }
        }
    }

    /// Reference a variable, marking it global or captured when used from another function (Python var_ref).
    pub fn var_ref(&mut self, sym: I::SymId, ctx: &mut Ctx, node: &A::Expr) -> CResult<I::Expr> {
        let mut msg = self.extra[sym].unset_msg.clone();
        if msg.is_some() && ctx.regions.iter().any(|r| self.regions[*r].assigned.last().unwrap().contains(&sym)) {
            msg = None; // set earlier in this same loop body / branch (gauntlet friction #4)
        }
        let same_func = self.module.syms[sym].func == self.owner_func(ctx);
        if let Some(m) = &msg {
            if ctx.lam.is_none() && same_func && self.extra[sym].par_private {
                return Err(self.err(m.clone(), node.span,
                                    Some("to keep values from the loop, write them to a list made before it (ys[i] = …) \
                                          or add them up (s += …)".into())));
            }
            if ctx.lam.is_none() && same_func {
                let where_ = m.split("inside ").nth(1).unwrap_or("").split(',').next().unwrap_or("");
                let n = &self.module.syms[sym].name;
                return Err(self.err(m.clone(), node.span, Some(format!("give {n} a value before {where_}, e.g.  {n} = 0"))));
            }
        }
        if let Some(l) = ctx.lam {
            if self.lam_owns(l, sym) {
                return Ok(self.plain_var(sym, node.span.line));
            }
            // symbol from an enclosing function
            let st = self.module.syms[sym].storage;
            if matches!(st, I::Storage::Global | I::Storage::Arena) {
                return Ok(self.ivar(sym, node.span.line));
            }
            if !self.extra[sym].par_private && self.module.syms[sym].func.is_none() {
                // private to a parallel for: passed in the env, never a shared global (D152)
                self.module.syms[sym].storage = I::Storage::Global;
                return Ok(self.ivar(sym, node.span.line));
            }
            if !self.module.lambdas[l].captures.contains(&sym) {
                // an ODE solution made in the function is passed as a pointer in the env (D48)
                if !matches!(self.module.syms[sym].ty, Ty::Num(_) | Ty::Bool | Ty::Vec { .. } | Ty::Mat { .. } | Ty::Sol(_)) {
                    let what = self.user_name(sym);
                    return Err(self.err(format!("{what} can't be used inside this integral/equation yet (only numbers, \
                                                 vectors, matrices and ODE solutions can be taken in from the function)"),
                                        node.span, None));
                }
                self.module.lambdas[l].captures.push(sym);
                // an enclosing integrand / equation must capture it too, to pass it on (gauntlet E9)
                for &pl in ctx.lam_parents.iter().rev() {
                    if self.lam_owns(pl, sym) {
                        break;
                    }
                    if !self.module.lambdas[pl].captures.contains(&sym) {
                        self.module.lambdas[pl].captures.push(sym);
                    }
                }
            }
            return Ok(self.ivar(sym, node.span.line));
        }
        if !same_func && self.module.syms[sym].storage == I::Storage::Local {
            if self.extra[sym].par_private {
                return Err(self.err(format!("{} belongs to one iteration of a parallel for and can't be used in another \
                                             function", self.module.syms[sym].name), node.span, None));
            }
            if self.module.syms[sym].func.is_none() {
                self.module.syms[sym].storage = I::Storage::Global;
            } else {
                return Err(self.err(format!("{} belongs to another function and can't be used here",
                                            self.user_name(sym)), node.span, None));
            }
        }
        Ok(self.ivar(sym, node.span.line))
    }

    fn lam_owns(&self, l: I::LambdaId, sym: I::SymId) -> bool {
        let lam = &self.module.lambdas[l];
        lam.locals.contains(&sym) || lam.params.contains(&sym) || lam.state.contains(&sym)
            || lam.param_syms.contains(&sym) || lam.col_syms.contains(&sym)
    }

    fn plain_var(&self, sym: I::SymId, line: u32) -> I::Expr {
        ir(I::ExprKind::Var(sym), self.module.syms[sym].ty.clone(), line)
    }

    /// How to name a variable in a message: an ODE solution's handle by its unknowns.
    pub fn user_name(&self, sym: I::SymId) -> String {
        let s = &self.module.syms[sym];
        match &s.ty {
            Ty::Sol(i) => {
                let names = self.sol_names(*i);
                if names.is_empty() { "this ODE solution".into() } else { format!("the solution {}", names.join(", ")) }
            }
            Ty::List(_) => format!("the list {}", s.name),
            _ => s.name.clone(),
        }
    }

    pub fn ivar(&self, sym: I::SymId, line: u32) -> I::Expr {
        let s = &self.module.syms[sym];
        let mut r = ir(I::ExprKind::Var(sym), s.ty.clone(), line);
        r.sf = s.sf;
        r.hint = s.hint.clone();
        r.direct = s.direct;
        if matches!(s.direct, 4 | 5) {
            r.extra().list_sf = self.extra[sym].list_sf; // a loop variable over a written list (D242)
        }
        if self.extra[sym].tdelta {
            r.extra().tdelta = true;
        }
        if self.extra[sym].fit_sf {
            r.extra().fit_sf = true;
        }
        if let Some(m) = &self.extra[sym].mixed_hint {
            r.extra().mixed = Some(m.clone());
        }
        r
    }

    pub fn undefined(&mut self, name: &str, e: &A::Expr, ctx: &Ctx) -> Diagnostic {
        crate::names::undefined(self, name, e, ctx)
    }

    pub fn known_names(&self, scope: ScopeId) -> std::collections::HashSet<String> {
        let mut known = std::collections::HashSet::new();
        let mut s = Some(scope);
        while let Some(id) = s {
            known.extend(self.scopes[id].names.keys().cloned());
            s = self.scopes[id].parent;
        }
        known
    }

    pub fn text(&mut self, s: &str) -> usize {
        self.module.tables.texts.push(s.to_string());
        self.module.tables.texts.len() - 1
    }
}

/// The unit a constant or value shows in, as a display hint.
pub fn unit_hint(u: &Unit) -> Option<I::Hint> {
    if u.name == "1" { None } else { Some(hint_of(u)) }
}

pub fn expr_kind_name(k: &A::ExprKind) -> &'static str {
    use A::ExprKind as K;
    match k {
        K::Num { .. } => "a number",
        K::Quantity { .. } => "a quantity",
        K::Str { .. } => "text",
        K::Bool { .. } => "true/false",
        K::Name { .. } => "a name",
        K::BinOp { .. } => "arithmetic",
        K::Neg { .. } => "negation",
        K::Compare { .. } => "a comparison",
        K::Logic { .. } => "and/or",
        K::Not { .. } => "not",
        K::Call { .. } => "a call",
        K::Index { .. } => "indexing",
        K::Slice { .. } => "a slice",
        K::End => "end",
        K::Field { .. } => "a field",
        K::Prime { .. } => "a derivative (′)",
        K::Deriv { .. } => "a derivative (d/dx)",
        K::Integral { .. } => "an integral",
        K::Sum { .. } => "Σ",
        K::Sqrt { .. } => "√",
        K::Abs { .. } => "|…|",
        K::ListLit { .. } => "a list",
        K::Table { .. } => "table(…)",
        K::VecCalc { .. } => "∇",
        K::VecLit { .. } => "a vector",
        K::IfExpr { .. } => "if … else",
        K::Convert { .. } => "in (unit conversion)",
        K::Digits { .. } => "to N digits",
        K::Load { .. } => "load",
        K::Where { .. } => "where",
        K::Uncertain { .. } => "±",
    }
}

#[allow(dead_code)]
fn _unused(_: &Unit) -> Option<units::Unit> {
    None
}
