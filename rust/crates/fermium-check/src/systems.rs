//! Unit systems (D60): `units natural(ħ = c = 1)`, `units nuclear`, `units astro`, `units SI`, with or
//! without a block. A port of Checker.s_Units, _set_system, resolve_unit's mapping into the system,
//! _from_system, _export, _export_example, the natural-units branch of use_binding, the system checks of
//! instantiate (nat_contains, system_name/consts) and tables.py's _natural_hint (applied in resolve_fmts).
//!
//! The system in force is a [`Sys`]: a cheap handle (`None` for SI) to a fermium-units [`UnitSystem`].
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::Dim;
use fermium_syntax::ast as A;
use fermium_units::natural::canonical_const_name;
use fermium_units::{make_system, Rational64, UnitSystem};

use crate::checker::*;
use crate::exprs::hint_of;
use crate::units::Unit;

const ORDER: [&str; 5] = ["ħ", "c", "k_B", "G", "ε_0"];

/// The unit system in force: SI (the default, `None`), a natural system, or the astro display preset.
#[derive(Clone, Debug, Default)]
pub struct Sys(Option<Rc<UnitSystem>>);

impl Sys {
    pub fn si() -> Sys {
        Sys(None)
    }

    pub fn of(s: UnitSystem) -> Sys {
        if s.name == "SI" && !s.natural {
            Sys(None)
        } else {
            Sys(Some(Rc::new(s)))
        }
    }

    /// Python `system is SI`.
    pub fn is_si(&self) -> bool {
        self.0.is_none()
    }

    /// The same as [`Sys::is_si`] (kept for code written when the system was a string, "" = SI).
    pub fn is_empty(&self) -> bool {
        self.is_si()
    }

    pub fn system(&self) -> Option<&UnitSystem> {
        self.0.as_deref()
    }

    pub fn natural(&self) -> bool {
        self.0.as_ref().is_some_and(|s| s.natural)
    }

    /// The constants set to 1, as a bit set over ħ, c, k_B, G, ε_0 (Python `system.key`).
    pub fn key(&self) -> u8 {
        let mut k = 0;
        for c in self.consts() {
            if let Some(i) = ORDER.iter().position(|o| o == c) {
                k |= 1 << i;
            }
        }
        k
    }

    /// Python `self.key <= other.key`: every constant set to 1 here is also 1 there.
    pub fn subset_of(&self, other: &Sys) -> bool {
        self.key() & !other.key() == 0
    }

    pub fn name(&self) -> &str {
        self.0.as_ref().map(|s| s.name.as_str()).unwrap_or("SI")
    }

    pub fn display(&self) -> &str {
        self.0.as_ref().map(|s| s.display.as_str()).unwrap_or("SI")
    }

    pub fn consts(&self) -> &[&'static str] {
        self.0.as_ref().map(|s| s.consts.as_slice()).unwrap_or(&[])
    }

    /// "ħ = c" (the constants set to 1, as the program would write them).
    pub fn consts_text(&self) -> String {
        self.consts().join(" = ")
    }

    pub fn label(&self) -> String {
        self.0.as_ref().map(|s| s.label()).unwrap_or_else(|| "units SI".into())
    }

    pub fn canon_dim(&self, d: &Dim) -> Dim {
        self.0.as_ref().map(|s| s.canon_dim(d)).unwrap_or(*d)
    }

    pub fn factor(&self, d: &Dim) -> f64 {
        self.0.as_ref().map(|s| s.factor(d)).unwrap_or(1.0)
    }

    pub fn canon_unit(&self, u: &Unit) -> Unit {
        match &self.0 {
            Some(s) => s.canon_unit(u),
            None => u.clone(),
        }
    }

    pub fn const_value(&self, name: &str, value: f64, dim: &Dim) -> f64 {
        self.0.as_ref().map(|s| s.const_value(name, value, dim)).unwrap_or(value)
    }

    pub fn display_unit(&self, d: &Dim) -> Option<Unit> {
        self.0.as_ref().and_then(|s| s.display_unit(d))
    }

    pub fn describe(&self, d: &Dim) -> String {
        match &self.0 {
            Some(s) => s.describe(d),
            None => fermium_units::dim_name(d),
        }
    }

    pub fn split(&self, d: &Dim) -> (Vec<Rational64>, Vec<Rational64>) {
        match &self.0 {
            Some(s) if s.natural => s.split(d),
            _ => (vec![], d.0.to_vec()),
        }
    }
}

/// Python compares systems by their key (the constants set to 1).
impl PartialEq for Sys {
    fn eq(&self, o: &Sys) -> bool {
        self.key() == o.key()
    }
}

/// The key, for instance keys (a function is checked again in each unit system it is used in).
impl fmt::Display for Sys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key())
    }
}

/// The checker's unit-system bookkeeping.
#[derive(Clone, Debug, Default)]
pub struct SysState {
    /// the `units` statements written at the top level of the program (Python _top_units)
    pub top_units: HashSet<*const A::Stmt>,
    /// the systems print formats were made in, by label (Fmt.nat), for resolve_fmts
    pub by_label: HashMap<String, Sys>,
}

/// A display hint as a unit.
pub fn unit_of_hint(h: &I::Hint) -> Unit {
    Unit { name: h.name.clone(), dim: h.dim, factor: h.factor, offset: h.offset }
}

/// tables.py _natural_hint: inside `units natural` / `nuclear` / `astro`, a value with no unit of its own (or one
/// that no longer fits) is shown in the system's units: MeV powers, fm, M☉/AU/yr (D60).
pub fn natural_hint(sys: &Sys, dim: &Dim, hint: Option<I::Hint>) -> Option<I::Hint> {
    if let Some(h) = &hint {
        if h.dim == *dim {
            return hint;
        }
    }
    match sys.display_unit(dim) {
        Some(u) => Some(hint_of(&u)),
        None => hint,
    }
}

fn with_dim(t: &Ty, d: DExpr) -> Ty {
    match t {
        Ty::Num(_) => Ty::Num(d),
        Ty::List(_) => Ty::List(d),
        Ty::Vec { n, .. } => Ty::Vec { n: *n, dim: Some(d), dims: None },
        Ty::Mat { r, c, .. } => Ty::Mat { r: *r, c: *c, dim: d },
        other => other.clone(),
    }
}

impl Checker {
    /// Python _set_system (U.namer follows: Checker::desc describes in the system in force).
    pub fn set_system(&mut self, sys: Sys) {
        if !sys.is_si() {
            self.sys.by_label.entry(sys.label()).or_insert_with(|| sys.clone());
        }
        self.nat = sys;
    }

    pub fn natural(&self) -> bool {
        self.nat.natural()
    }
    pub fn nat_label(&self) -> String {
        self.nat.label()
    }
    pub fn nat_name(&self) -> String {
        self.nat.name().to_string()
    }
    pub fn nat_display(&self) -> &str {
        self.nat.display()
    }

    /// The system a print format records (Fmt.nat): None in SI.
    pub fn fmt_nat(&self) -> Option<String> {
        if self.nat.is_si() { None } else { Some(self.nat.label()) }
    }

    /// The display hint of a print format made under the system labelled `nat` (resolve_fmts).
    pub fn fmt_natural_hint(&self, nat: &str, dim: &Dim, hint: Option<I::Hint>) -> Option<I::Hint> {
        match self.sys.by_label.get(nat) {
            Some(sys) => natural_hint(sys, dim, hint),
            None => hint,
        }
    }

    /// Can a function defined under `other` be used here (Python `other.key <= self.nat.key`)?
    pub fn nat_contains(&self, other: &Sys) -> bool {
        other.subset_of(&self.nat)
    }
    pub fn system_name(&self, sys: &Sys) -> String {
        sys.name().to_string()
    }
    pub fn system_consts(&self, sys: &Sys) -> String {
        sys.consts_text()
    }

    /// The system a function defined here belongs to (None: SI, callable anywhere).
    pub fn func_nat(&self) -> Option<Sys> {
        if self.nat.natural() { Some(self.nat.clone()) } else { None }
    }

    /// `units natural(ħ = c = 1)`: the rest of the program; `units nuclear:` + a block: just that block.
    pub fn s_units(&mut self, s: &A::Stmt, system: &str, consts: &[String], body: Option<&[A::Stmt]>, ctx: &mut Ctx)
                   -> CResult<Vec<I::Stmt>> {
        if !self.sys.top_units.contains(&(s as *const A::Stmt)) || !ctx.is_main || ctx.lam.is_some() {
            return Err(self.err("a  units  line must be at the top level of the program (not inside a function, loop \
                                 or if)", s.span, None));
        }
        let cs: Vec<&str> = consts.iter().map(|c| canonical_const_name(c)).collect();
        let sys = match make_system(system, Some(&cs)) {
            Ok(u) => Sys::of(u),
            Err(m) => return Err(self.err(m, s.span, None)),
        };
        let Some(body) = body else {
            self.set_system(sys);
            return Ok(vec![]);
        };
        let saved = self.nat.clone();
        self.set_system(sys);
        let r = self.block(body, ctx);
        self.set_system(saved);
        r
    }

    /// Note the `units` statements at the top level of a program (before checking it).
    pub fn note_top_units(&mut self, prog: &A::Program) {
        for s in &prog.body {
            if matches!(s.kind, A::StmtKind::Units { .. }) {
                self.sys.top_units.insert(s as *const A::Stmt);
            }
        }
    }

    /// A constant inside a natural region: its value in natural units, e.g. ħ = c = 1, m_e = 0.511 MeV.
    pub fn natural_const(&mut self, c: &ConstInfo, e: &A::Expr) -> CResult<I::Expr> {
        let v = self.nat.const_value(canonical_const_name(&c.name), c.value, &c.unit.dim);
        Ok(ir(I::ExprKind::Const(v), num_ty(self.nat.canon_dim(&c.unit.dim)), e.span.line))
    }

    /// A unit to suggest for taking sym out of its natural-units region: fm for 1/energy, MeV for energy...
    pub fn export_example(&self, sym: I::SymId, system: &Sys) -> String {
        let ty = &self.module.syms[sym].ty;
        let dim = match ty {
            Ty::Num(d) | Ty::List(d) | Ty::Mat { dim: d, .. } | Ty::Vec { dim: Some(d), .. } => Some(d),
            _ => None,
        };
        if let Some(d) = dim {
            let d = self.u.norm(d);
            if d.is_concrete() && system.consts().contains(&"ħ") {
                let (_, beta) = system.split(&d.konst);
                if beta[1..].iter().all(|b| *b == Rational64::from_integer(0)) {
                    let b0 = beta[0];
                    let known = [(1, "MeV  (or kg for a mass)"), (-1, "fm  (or s for a time)"), (0, "1"),
                                 (-2, "fm²  (or barn)")];
                    for (k, s) in known {
                        if b0 == Rational64::from_integer(k) {
                            return s.to_string();
                        }
                    }
                    return system.display_unit(&d.konst).map(|u| u.name).unwrap_or_default();
                }
            }
        }
        "fm  (or MeV, s, kg, ...)".into()
    }

    /// A variable set under another unit system (Python _from_system). SI values (or those of a system with fewer
    /// constants set to 1) convert uniquely into the current natural units; the other way is ambiguous (is 1/MeV
    /// a length or a time?), so it needs an explicit  x in unit.
    pub fn from_system(&mut self, sym: I::SymId, r: I::Expr, node: &A::Expr) -> CResult<I::Expr> {
        let other = self.extra[sym].nat.clone();
        if other == self.nat {
            return Ok(r);
        }
        let cur = self.nat.clone();
        let name = self.module.syms[sym].name.clone();
        if !other.subset_of(&cur) {
            let what = format!("{name} was computed in {} units ({} = 1)", other.name(), other.consts_text());
            let hint = format!("say which unit you want, like  {name} in {}", self.export_example(sym, &other));
            return Err(self.err(format!("{what}, so its units here are ambiguous"), node.span, Some(hint)));
        }
        let ty = self.module.syms[sym].ty.clone();
        let dim = match &ty {
            Ty::Num(d) | Ty::List(d) | Ty::Mat { dim: d, .. } | Ty::Vec { dim: Some(d), dims: None, .. } => d.clone(),
            Ty::Bool | Ty::Str | Ty::TextList => return Ok(r),
            _ => {
                return Err(self.err(format!("{name} was set outside this {} region and can't be used here",
                                            cur.label()), node.span, None))
            }
        };
        let d = self.u.norm(&dim);
        if !d.is_concrete() {
            return Err(self.err(format!("the units of {name} aren't known yet, so it can't be converted into {} \
                                         units here", cur.name()), node.span,
                                Some(format!("give {name} a unit where it is set"))));
        }
        let f = cur.factor(&d.konst);
        let nd = cur.canon_dim(&d.konst);
        if f == 1.0 && nd == d.konst {
            return Ok(r);
        }
        let nty = with_dim(&ty, DExpr::of(nd));
        let line = r.line;
        let (sf, direct) = (r.sf, r.direct);
        let hint = r.hint.as_ref().map(|h| hint_of(&cur.canon_unit(&unit_of_hint(h))));
        let mut out = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(r),
                                          Box::new(ir(I::ExprKind::Const(f), dimless_num(), line))), nty, line);
        out.sf = sf;
        out.direct = direct;
        out.hint = hint;
        Ok(out)
    }

    /// `x in fm` (or `2 x/y in fm`) using variables computed under a natural system that doesn't hold here
    /// (outside their region): the one place where natural units turn back into SI (Python _export). e_Convert
    /// calls this first; None means an ordinary conversion. The expression is checked in that system; the target
    /// unit fixes the SI dimension, and the split D = Σ aᵢ Cᵢ + Σ βⱼ Bⱼ gives the unique factor Π Cᵢ^aᵢ (D60).
    pub fn export(&mut self, e: &A::Expr, value: &A::Expr, unit: &A::UnitExpr, ctx: &mut Ctx)
                  -> CResult<Option<I::Expr>> {
        let mut foreign: Vec<(Sys, String)> = vec![];
        for n in crate::walk::free_names(value) {
            if let Some((Binding::Sym(s), _)) = self.lookup(ctx.scope, &n) {
                let other = self.extra[s].nat.clone();
                if !other.subset_of(&self.nat) && !foreign.iter().any(|(o, _)| *o == other) {
                    foreign.push((other, n));
                }
            }
        }
        if foreign.is_empty() {
            return Ok(None);
        }
        if foreign.len() > 1 {
            let names = foreign.iter().map(|(o, n)| format!("{n} ({} units)", o.name())).collect::<Vec<_>>();
            return Err(self.err(format!("this mixes values from different unit systems: {}", names.join(" and ")),
                                e.span, Some("convert each one on its own first, like  x_si = x in fm".into())));
        }
        let (other, name) = foreign.pop().unwrap();
        let u_si = self.resolve_unit_si(unit)?;
        let saved = self.nat.clone();
        self.set_system(other.clone());
        let r = self.expr(value, ctx);
        self.set_system(saved);
        let r = r?;
        self.need_numlike(&r, value, "the value to convert", true)?;
        if matches!(r.ty, Ty::Vec { dims: Some(_), .. }) {
            return Err(self.err(format!("can't show a vector with different units per component in {}", u_si.name),
                                e.span, None));
        }
        if u_si.affine() {
            return Err(self.err(format!("can't convert from {} units to {}; use K", other.name(), u_si.name), e.span,
                                None));
        }
        let what = if matches!(value.kind, A::ExprKind::Name { .. }) { name } else { "this".into() };
        let want = other.canon_dim(&u_si.dim);
        let rdim = crate::stmts::ty_dim(&r.ty).unwrap_or_else(DExpr::dimless);
        if !self.u.unify(&rdim, &DExpr::of(want)) {
            let have = other.describe(&self.u.resolve(&rdim));
            return Err(self.err(format!("{what} is {have} in {} units, so it can't be shown in {} ({})", other.name(),
                                        u_si.name, other.describe(&want)), e.span,
                                Some(format!("in {} units a length or time is 1/energy and a mass is an energy",
                                             other.name()))));
        }
        // SI value = canonical value / factor(D); then into the current system's representation
        let f = self.nat.factor(&u_si.dim) / other.factor(&u_si.dim);
        let d = DExpr::of(self.nat.canon_dim(&u_si.dim));
        let nty = match &r.ty {
            Ty::Vec { .. } | Ty::Mat { .. } | Ty::List(_) => with_dim(&r.ty, d),
            _ => Ty::Num(d),
        };
        let line = r.line;
        let sf = r.sf;
        let mut out = ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(r),
                                          Box::new(ir(I::ExprKind::Const(f), dimless_num(), line))), nty, line);
        out.hint = Some(hint_of(&self.nat.canon_unit(&u_si)));
        out.sf = sf;
        out.direct = 0;
        Ok(Some(out))
    }

    /// A value of `clock()` (seconds) or another SI time in the system in force (D60).
    pub fn seconds_here(&self, r: I::Expr) -> I::Expr {
        if !self.nat.natural() {
            return r;
        }
        let t = fermium_units::TIME;
        let line = r.line;
        ir(I::ExprKind::Bin(I::BinOp::Mul, Box::new(r),
                            Box::new(ir(I::ExprKind::Const(self.nat.factor(&t)), dimless_num(), line))),
           num_ty(self.nat.canon_dim(&t)), line)
    }
}
