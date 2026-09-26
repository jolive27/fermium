//! The evaluator, ODE/eigen/PDE solutions: the run-time side of `solve` as the compiled path of Fermium 1.5
//! runs it (codegen_llvm.py s_SSolve, fm_sol_eval, e_ISolList; codegen_m3.py py_solve, fm_pde_eval;
//! runtime/core.py describe_error and warn, runtime/m3rt.py). The numerics are fermium-runtime's ports of v1's
//! kernels (ode.rs, stiff.rs, eigen.rs, pde.rs).
use std::cell::RefCell;
use std::rc::Rc;

use fermium_ir::{Expr, ExprKind, Lambda, Module, Stmt, StmtKind, SymId, Ty};
use fermium_runtime::numerics::{self as nx, err as E, ode, Fail};
use fermium_units::quantity::format_quantity;
use fermium_units::Unit;

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

/// A stored solution: the steps' times, the state and its derivative there (v1's SolStruct), the right-hand
/// side kept with it for x'(t) (D46) with a copy of the values it reads, and a PDE's grid ends.
pub(crate) struct SolData {
    pub sol: ode::Sol,
    pub rhs: Option<(usize, Frame)>,
    pub grid: Option<(f64, f64)>,
    /// a PDE's grid check (Fermium 2, DIVERGENCES.md): the solution on half the grid, each component's range,
    /// the names of x and t (text ids), and whether it has warned
    pub check: Option<PdeCheck>,
}

pub(crate) struct PdeCheck {
    coarse: ode::Sol,
    ranges: Vec<f64>,
    xname: usize,
    tname: usize,
    warned: std::cell::Cell<bool>,
}

/// Solutions made so far, and the run-time warnings already shown.
#[derive(Default)]
pub struct SolveState {
    pub(crate) sols: Vec<Rc<SolData>>,
}

/// v1's `fmt_value` (eval_calc's).
pub(crate) fn fmt_value(module: &Module, v: f64, fmt: usize) -> String {
    crate::eval_calc::fmt_value(module, v, Some(fmt))
}

/// Two values of one quantity with enough digits to tell them apart (D214).
fn fmt_apart(module: &Module, a: f64, b: f64, fmt: usize) -> (String, String) {
    let mut out = (String::new(), String::new());
    for sf in [3, 4, 5, 6, 8, 10, 12, 15] {
        out = match module.tables.fmts.get(fmt) {
            Some(f) => {
                let hint = f.hint.as_ref().map(|h| Unit { name: h.name.clone(), dim: h.dim, factor: h.factor,
                                                          offset: h.offset });
                (format_quantity(a, &f.dim, hint.as_ref(), Some(sf), 0, true, true),
                 format_quantity(b, &f.dim, hint.as_ref(), Some(sf), 0, true, true))
            }
            None => (format!("{} (SI units)", fermium_units::format_number(a, sf, true)),
                     format!("{} (SI units)", fermium_units::format_number(b, sf, true))),
        };
        if out.0 != out.1 {
            break;
        }
    }
    out
}

fn tname(module: &Module, i: f64) -> String {
    let i = if i == i { i as i64 } else { -1 };
    if i >= 0 && (i as usize) < module.tables.texts.len() {
        module.tables.texts[i as usize].clone()
    } else {
        "t".into()
    }
}

/// v1's `describe_error` for the solvers' error kinds.
pub(crate) fn describe_error(module: &Module, kind: i64, a: f64, b: f64, fmt: usize) -> String {
    let fv = |v: f64| fmt_value(module, v, fmt);
    if kind >= E::ODE_STEPS_FROM {
        let stiff = kind >= E::STIFF_STEPS_FROM;
        let name = tname(module, (kind - if stiff { 2_000_001 } else { 1_000_001 }) as f64);
        let (reached, start) = fmt_apart(module, a, b, fmt);
        let wh = format!("it got from {name} = {start} only to {name} = {reached}");
        if stiff {
            return format!("the stiff ODE solver needed too many steps ({wh}); the solution may blow up or oscillate \
                            very fast there");
        }
        return format!("the ODE solver needed too many steps (20 million: {wh}); if the equation is stiff (time scales \
                        far apart, like a 164 μs half-life in a chain followed for hours), add  using radau  after the \
                        range; otherwise the solution may blow up");
    }
    match kind {
        E::SOLRANGE => format!("asked for the solution at {}, outside the range it was solved for (it ends at {})", fv(a),
                               fv(b)),
        E::ODE_STEPS => format!("the ODE solver needed too many steps (reached {} = {}); if the equation is stiff (time \
                                 scales far apart, like a 164 μs half-life in a chain followed for hours), add  using \
                                 radau  after the range; otherwise the solution may blow up", tname(module, b), fv(a)),
        E::STIFF_STEPS => format!("the stiff ODE solver needed too many steps (reached {} = {}); the solution may blow up \
                                   or oscillate very fast there", tname(module, b), fv(a)),
        E::STEP => "the step must be a non-zero number that goes from the start towards the end".into(),
        E::ODE_H => format!("the ODE solver's step became too small near {} = {}; the solution may blow up there",
                            tname(module, b), fv(a)),
        E::ODE_H_FLAT => format!("the ODE solver's step became too small near {} = {}; no unknown has grown there, so this \
                                  is probably not a blow-up: the error control asks for relative accuracy on values that \
                                  are tiny or rounding noise (like an abundance of 10⁻²³); add an absolute tolerance after \
                                  the range, e.g.  tolerance 1e-9 absolute 1e-16  (in the units of the unknowns)",
                                 tname(module, b), fv(a)),
        E::ODE_NAN => {
            let v = fv(a);
            format!("the right side of the equation is NaN or infinite at {} = {v} (0/0? 1/0?); if the equation is \
                     singular there, start slightly away from {v}", tname(module, b))
        }
        E::ODE_RANGE => format!("the range of {} is empty: it starts and ends at {}", tname(module, b), fv(a)),
        E::ODE_SINGULAR => format!("{}{}: the matrix of their coefficients is singular (a zero mass or length?)",
                                   text(module, b), fv(a)),
        E::NO_EVENT => format!("{}{}; make the range longer", text(module, b), fv(a)),
        _ => "runtime error".into(),
    }
}

fn text(module: &Module, i: f64) -> String {
    module.tables.texts.get(i as usize).cloned().unwrap_or_default()
}

/// v1's `Runtime.warn` messages for the solvers' warning kinds.
fn warn_message(kind: i64, a: f64) -> String {
    let pct = |a: f64| fermium_units::format_number(a * 100.0, 2, true);
    match kind {
        2 => format!("this equation looks stiff: rk45 has taken {} steps, held small by stability rather than accuracy \
                      (time scales far apart); add  using radau  after the range for an implicit solver made for this",
                     a as i64),
        7 => format!("the step is too coarse for this equation: the estimated error is {}% of the solution's size \
                      (fixed-step RK4, checked by step doubling); use a smaller step, or drop  step  to use the adaptive \
                      solver", pct(a)),
        8 => format!("the time step is too coarse for this PDE: the estimated error is {}% of the solution's range \
                      (checked by step doubling); use a smaller step, or drop  step  to let Fermium choose it", pct(a)),
        9 => format!("this PDE's time step could not be made fine enough: with {} steps the estimated error is still {}% \
                      of the solution's range (checked by step doubling); the result may be inaccurate",
                     nx::pde::PDE_MAX_STEPS, pct(a)),
        _ => "warning".into(),
    }
}

fn slots(t: &Ty) -> usize {
    match t {
        Ty::Vec { n, .. } => *n,
        Ty::Complex(_) => 2,
        Ty::Mat { r, c, .. } => r * c,
        _ => 1,
    }
}

/// The symbols a lambda's body reads directly (and through the lambdas nested in it).
fn referenced(module: &Module, es: &[Expr], out: &mut Vec<SymId>) {
    for e in es {
        if let ExprKind::Var(s) = e.kind {
            if !out.contains(&s) {
                out.push(s);
            }
        }
        if let Some(l) = fermium_ir::lambda_of(e) {
            referenced(module, &module.lambdas[l].body, out);
        }
        let kids: Vec<Expr> = fermium_ir::expr_children(e).into_iter().cloned().collect();
        referenced(module, &kids, out);
    }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// v1's rt.warn for the solvers' kinds (eval_calc's warn_at), kind 7 once per solve.
    pub(crate) fn rt_warn(&mut self, kind: i64, a: f64, line: u32) {
        let msg = warn_message(kind, a);
        if kind == 7 {
            let at = if line > 0 { format!("line {line}: ") } else { String::new() };
            let text = format!("warning: {at}{msg}");
            if crate::eval_calc::warned_with_prefix(text.split(" is ").next().unwrap_or("")) {
                return; // once per solve, not once per loop pass
            }
        }
        crate::eval_calc::warn_at(line, &msg);
    }

    fn fail_solve(&self, f: Fail, fmt: usize) -> RunError {
        RunError { message: describe_error(self.module, f.kind, f.a, f.b, fmt), line: self.line, hint: None }
    }

    /// Evaluate an ODE-kind lambda at (t, y) into out (the compiled lambda's calling convention).
    pub(crate) fn ode_call(&mut self, lam: &Lambda, t: f64, y: &[f64], out: &mut [f64], fr: &mut Frame)
                           -> Result<(), RunError> {
        let module = self.module;
        if let Some(&p) = lam.params.first() {
            fr.vars.insert(p, Value::Num(t));
        }
        let mut i = 0;
        for &s in &lam.state {
            let n = slots(&module.syms[s].ty);
            let v = if matches!(module.syms[s].ty, Ty::Num(_)) {
                Value::Num(y[i])
            } else {
                Value::Vec(Rc::new(y[i..i + n].to_vec()))
            };
            fr.vars.insert(s, v);
            i += n;
        }
        let mut j = 0;
        for e in &lam.body {
            match self.eval(e, fr)? {
                // interp.ode_rhs (D122)
                Value::Unc(_) => return self.err("a differential equation (solve) can't use uncertain values (±) yet; \
                                                  put the solve inside a  propagate montecarlo  block, or use value(x)"),
                Value::Vec(v) => {
                    for x in v.iter() {
                        if j < out.len() {
                            out[j] = *x;
                        }
                        j += 1;
                    }
                }
                v => {
                    if j < out.len() {
                        out[j] = v.num();
                    }
                    j += 1;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn stmt_solve(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        match &s.kind {
            StmtKind::Solve { method, .. } if method == "eigen" => self.solve_eigen(s, fr),
            StmtKind::Solve { method, .. } if method == "pde" => self.solve_pde(s, fr),
            StmtKind::Solve { .. } => self.solve_ode(s, fr),
            StmtKind::Plot(..) => self.stmt_plot(s, fr),
            StmtKind::Fit { .. } => self.stmt_fit(s, fr),
            StmtKind::Animate { .. } => self.stmt_animate(s, fr),
            _ => Ok(()),
        }
    }

    fn flat(&mut self, es: &[Expr], fr: &mut Frame) -> Result<Vec<f64>, RunError> {
        let mut y0 = vec![];
        for e in es {
            match self.eval(e, fr)? {
                Value::Vec(v) => y0.extend(v.iter()),
                // interp.s_SSolve (D122)
                Value::Unc(_) => return self.err("a starting value of solve can't be uncertain (±) yet; put the solve \
                                                  inside a  propagate montecarlo  block, or use value(x)"),
                v => y0.push(v.num()),
            }
        }
        Ok(y0)
    }

    fn store_sol(&mut self, sym: SymId, data: SolData, fr: &mut Frame) {
        self.solve.sols.push(Rc::new(data));
        let h = Value::Handle(self.solve.sols.len() - 1);
        self.set(sym, h, fr);
    }

    /// A copy of what the right side reads, for x'(t) after the solve (codegen attach_rhs, D46).
    fn snapshot(&self, lam: usize, fr: &Frame) -> Frame {
        let module = self.module;
        let mut refs = vec![];
        let l = &module.lambdas[lam];
        referenced(module, &l.body, &mut refs);
        // the lambda's own variables are set at each call (a stale copy would shadow them)
        let own = |s: &SymId| l.params.contains(s) || l.state.contains(s) || l.locals.contains(s);
        let mut snap = Frame { vars: fr.vars.iter().filter(|(k, _)| !own(k)).map(|(k, v)| (*k, v.clone())).collect() };
        for s in refs.into_iter().filter(|s| !own(s)) {
            if !snap.vars.contains_key(&s) {
                if let Some(v) = self.globals.get(&s) {
                    snap.vars.insert(s, v.clone());
                }
            }
        }
        snap
    }

    fn solve_ode(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Solve { sol, rhs, y0, t0, t1, step, method, x, .. } = &s.kind else { unreachable!() };
        let module = self.module;
        let y0 = self.flat(y0, fr)?;
        let t0 = self.eval(t0, fr)?.num();
        let t1 = self.eval(t1, fr)?.num();
        let h0 = match step {
            Some(e) => Some(self.eval(e, fr)?.num()),
            None => None,
        };
        self.line = s.line;
        let lam = &module.lambdas[*rhs];
        let evlam = x.event.map(|l| &module.lambdas[l]);
        let atol = x.atol.as_ref().and_then(|spec| ode::abs_tolerances(spec, t0, t1));
        let opts = ode::OdeOpts { rtol: x.rtol, atol: atol.as_deref(), tname: x.tname as f64, evtext: x.evtext as f64,
                                  tdep: x.tdep };
        let cell = RefCell::new((&mut *self, &mut *fr, None::<RunError>));
        let f = |t: f64, y: &[f64], out: &mut [f64]| {
            let mut g = cell.borrow_mut();
            let (me, fr, err) = &mut *g;
            if err.is_some() {
                out.fill(f64::NAN);
                return;
            }
            if let Err(e) = me.ode_call(lam, t, y, out, fr) {
                *err = Some(e);
                out.fill(f64::NAN);
            }
        };
        let mut gfun = |t: f64, y: &[f64]| -> f64 {
            let mut g = cell.borrow_mut();
            let (me, fr, err) = &mut *g;
            if err.is_some() {
                return f64::NAN;
            }
            let mut out = [0.0];
            if let Err(e) = me.ode_call(evlam.unwrap(), t, y, &mut out, fr) {
                *err = Some(e);
                return f64::NAN;
            }
            out[0]
        };
        let ev: Option<ode::EventFn<'_>> = if evlam.is_some() { Some(&mut gfun) } else { None };
        let mut early: Vec<(i64, f64)> = vec![];
        let r = match method.as_str() {
            "radau" | "bdf" => {
                let m = if method == "bdf" { nx::stiff::StiffMethod::Bdf } else { nx::stiff::StiffMethod::Radau };
                nx::stiff::stiff_solve(f, &y0, t0, t1, m, ev, opts)
            }
            "rk4" => ode::rk4(f, &y0, t0, t1, h0.unwrap_or(f64::NAN), ev, opts),
            _ => {
                let mut sv = ode::Sol::new(y0.len());
                match ode::dp45_into(f, &y0, t0, t1, ev, opts, &mut sv) {
                    Ok(()) => Ok(sv),
                    Err(fl) => {
                        early = sv.warnings; // a stiffness warning before "too many steps"
                        Err(fl)
                    }
                }
            }
        };
        let (_, _, err) = cell.into_inner();
        if let Some(e) = err {
            return Err(e);
        }
        self.line = s.line;
        for &(kind, a) in &early {
            self.rt_warn(kind, a, s.line);
        }
        let solv = r.map_err(|f| self.fail_solve(f, x.tfmt))?;
        for &(kind, a) in &solv.warnings {
            self.rt_warn(kind, a, s.line);
        }
        let snap = self.snapshot(*rhs, fr);
        self.store_sol(*sol, SolData { sol: solv, rhs: Some((*rhs, snap)), grid: None, check: None }, fr);
        Ok(())
    }

    fn solve_eigen(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Solve { sol, rhs, t0, t1, x, .. } = &s.kind else { unreachable!() };
        let module = self.module;
        let a = self.eval(t0, fr)?.num();
        let b = self.eval(t1, fr)?.num();
        self.line = s.line;
        let lam = &module.lambdas[*rhs];
        let mut err: Option<RunError> = None;
        let method = if x.eig_method == 1 { nx::eigen::EigenMethod::Shooting } else { nx::eigen::EigenMethod::Matrix };
        let r = {
            let me = &mut *self;
            let err = &mut err;
            let mut f = |xv: f64, p: f64, dp: f64, e: f64| -> f64 {
                if err.is_some() {
                    return f64::NAN;
                }
                let mut out = [0.0; 3];
                if let Err(ex) = me.ode_call(lam, xv, &[p, dp, e], &mut out, fr) {
                    *err = Some(ex);
                    return f64::NAN;
                }
                out[1]
            };
            nx::eigen::eigen_solve(&mut f, a, b, x.nstates, x.grid, method)
        };
        if let Some(e) = err {
            return Err(e);
        }
        self.line = s.line;
        let r = match r {
            Ok(r) => r,
            Err(ex) => {
                let msg = match ex.x {
                    None => ex.message,
                    Some(xv) => nx::eigen::singular_text(&tname(module, x.tname as f64), &fmt_value(module, xv, x.tfmt)),
                };
                return self.err(msg);
            }
        };
        for &(k, rel) in &r.warnings {
            let n = k + 1;
            let sub = |n: usize| -> String {
                n.to_string().chars().map(|c| char::from_u32('₀' as u32 + c.to_digit(10).unwrap()).unwrap()).collect()
            };
            crate::eval_calc::warn_text(&format!("levels {n} and {} are nearly degenerate (ΔE/E = {}); their eigenfunctions ψ{}, ψ{} \
                                       can be any mixture of the two: use them only through combinations, or break the \
                                       symmetry", n + 1, fermium_units::format_number(rel, 2, true), sub(n), sub(n + 1)));
        }
        let ns = x.nstates;
        let dim = 3 * ns;
        let mut solv = ode::Sol::new(dim);
        let mut row = vec![0.0; dim];
        let mut drow = vec![0.0; dim];
        for (i, &xv) in r.xs.iter().enumerate() {
            for k in 0..ns {
                row[2 * k] = r.psi[k][i];
                row[2 * k + 1] = r.dpsi[k][i];
                drow[2 * k] = r.dpsi[k][i];
                drow[2 * k + 1] = r.ddpsi[k][i];
                row[2 * ns + k] = r.energies[k];
                drow[2 * ns + k] = 0.0;
            }
            solv.push(xv, &row, &drow);
        }
        self.store_sol(*sol, SolData { sol: solv, rhs: None, grid: None, check: None }, fr);
        Ok(())
    }

    fn solve_pde(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Solve { sol, rhs, t0, t1, step, x, .. } = &s.kind else { unreachable!() };
        let module = self.module;
        let t0 = self.eval(t0, fr)?.num();
        let t1 = self.eval(t1, fr)?.num();
        let step = match step {
            Some(e) => Some(self.eval(e, fr)?.num()),
            None => None,
        };
        let xa = self.eval(x.xa.as_ref().unwrap(), fr)?.num();
        let xb = self.eval(x.xb.as_ref().unwrap(), fr)?.num();
        self.line = s.line;
        let lam = &module.lambdas[*rhs];
        let mut err: Option<RunError> = None;
        let method = match x.pmethod {
            1 => nx::pde::PdeMethod::Implicit,
            2 => nx::pde::PdeMethod::Explicit,
            _ => nx::pde::PdeMethod::CrankNicolson,
        };
        let opts = nx::pde::PdeOpts { grid: x.grid, order: x.order, method, step: step.filter(|v| v == v), bc: x.bc,
                                      is_complex: x.is_complex, tdep: x.tdep };
        let r = {
            let me = &mut *self;
            let err = &mut err;
            let mut f = |xv: f64, st: &[f64; 6]| -> [f64; 6] {
                let mut out = [f64::NAN; 6];
                if err.is_some() {
                    return out;
                }
                if let Err(ex) = me.ode_call(lam, xv, st, &mut out, fr) {
                    *err = Some(ex);
                    return [f64::NAN; 6];
                }
                out
            };
            nx::pde::pde_solve_checked(&mut f, xa, xb, t0, t1, opts)
        };
        if let Some(e) = err {
            return Err(e);
        }
        self.line = s.line;
        let r = match r {
            Ok(r) => r,
            Err(nx::pde::PdeFail(m)) => return self.err(m),
        };
        for &(kind, est) in &r.fine.warnings {
            self.rt_warn(kind, est, x.line);
        }
        let check = r.coarse.as_ref().map(|c| PdeCheck { coarse: c.to_sol(), ranges: r.fine.ranges(), xname: x.xname,
                                                          tname: x.tname, warned: std::cell::Cell::new(false) });
        let solv = r.fine.to_sol();
        self.store_sol(*sol, SolData { sol: solv, rhs: None, grid: Some((xa, xb)), check }, fr);
        Ok(())
    }

    fn sol_data(&self, sym: SymId, fr: &Frame) -> Result<Rc<SolData>, RunError> {
        match self.get(sym, fr)? {
            Value::Handle(h) => Ok(self.solve.sols[h].clone()),
            _ => self.err("this solution has no value"),
        }
    }

    /// fm_sol_eval: component comp (or its derivative) at t.
    fn sol_at(&mut self, d: &SolData, comp: usize, t: f64, use_dy: bool, fmt: usize) -> Result<f64, RunError> {
        let module = self.module;
        let r = match (&d.rhs, use_dy) {
            (Some((lam, snap)), true) => {
                let lam = &module.lambdas[*lam];
                let mut frame = Frame { vars: snap.vars.clone() };
                let mut err = None;
                let r = {
                    let me = &mut *self;
                    let err = &mut err;
                    let mut f = |t: f64, y: &[f64], out: &mut [f64]| {
                        if let Err(e) = me.ode_call(lam, t, y, out, &mut frame) {
                            *err = Some(e);
                            out.fill(f64::NAN);
                        }
                    };
                    d.sol.eval(comp, t, true, Some(&mut f))
                };
                if let Some(e) = err {
                    return Err(e);
                }
                r
            }
            _ => d.sol.eval(comp, t, use_dy, None),
        };
        r.map_err(|f| self.fail_solve(f, fmt))
    }

    /// max(x) / min(x) of a solution component: the largest step value refined by the quintic Hermite through
    /// its neighbours (v1's fm_sol_ext), not just the largest stored value.
    pub(crate) fn sol_extreme(&mut self, name: &str, args: &[Expr], fr: &mut Frame) -> Result<Option<Value>, RunError> {
        if !matches!(name, "max_list" | "min_list") || args.len() != 1 {
            return Ok(None);
        }
        let ExprKind::SolList { sol, comp, what: 0 } = &args[0].kind else { return Ok(None) };
        let d = self.sol_data(*sol, fr)?;
        let sg = if name == "max_list" { 1.0 } else { -1.0 };
        Ok(Some(Value::Num(d.sol.extreme(*comp, sg))))
    }

    pub(crate) fn eval_solution(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::SolEval { sol, comp, t, use_dy, tfmt } => {
                let d = self.sol_data(*sol, fr)?;
                let tv = self.eval(t, fr)?;
                self.line = e.line;
                match tv {
                    Value::List(l) => {
                        let ts = l.borrow().clone();
                        let mut out = Vec::with_capacity(ts.len());
                        for t in ts {
                            out.push(self.sol_at(&d, *comp, t, *use_dy, *tfmt)?);
                        }
                        Ok(Value::List(Rc::new(RefCell::new(out))))
                    }
                    v => Ok(Value::Num(self.sol_at(&d, *comp, v.num(), *use_dy, *tfmt)?)),
                }
            }
            ExprKind::SolList { sol, comp, what } => {
                let d = self.sol_data(*sol, fr)?;
                let s = &d.sol;
                let out: Vec<f64> = match what {
                    1 => s.t.clone(),
                    0 => (0..s.n()).map(|i| s.y[i * s.dim + comp]).collect(),
                    _ => (0..s.n()).map(|i| s.dy[i * s.dim + comp]).collect(),
                };
                Ok(Value::List(Rc::new(RefCell::new(out))))
            }
            ExprKind::PdeEval { sol, m, comp0, x, t, which, xfmt, tfmt, .. } => {
                let d = self.sol_data(*sol, fr)?;
                let xv = self.eval(x, fr)?.num();
                let tv = self.eval(t, fr)?.num();
                self.line = e.line;
                let (xa, xb) = d.grid.unwrap_or((0.0, 1.0));
                let slack = 1e-9 * (xb - xa).abs();
                if xv < xa - slack || xv > xb + slack || xv != xv {
                    let end = if xv < xa { xa } else { xb };
                    return Err(self.fail_solve(Fail::new(E::SOLRANGE, xv, end), *xfmt));
                }
                self.line = e.line;
                let v = nx::pde::pde_eval(&d.sol, xa, xb, *m, *comp0, xv, tv, *which)
                    .map_err(|f| self.fail_solve(f, *tfmt))?;
                if let (Some(ck), 0) = (&d.check, *which) {
                    let c = comp0 / (m + 1);
                    let est = nx::pde::grid_error(&d.sol, &ck.coarse, *m, xa, xb, c, ck.ranges[c], xv, tv);
                    if est.is_some_and(|q| q > nx::pde::PDE_TOL) && !ck.warned.get() {
                        ck.warned.set(true);
                        let module = self.module;
                        let msg = format!("the grid is too coarse for this PDE at {} = {}, {} = {}: the value there \
                                           changes by {}% of the solution's range when the grid is made half as fine \
                                           (a sharp front, a short wavelength, or the time right after a jump needs more \
                                           grid points); raise  grid  (it is {m})",
                                          text(module, ck.xname as f64), fmt_value(module, xv, *xfmt),
                                          text(module, ck.tname as f64), fmt_value(module, tv, *tfmt),
                                          fermium_units::format_number(3.0 * est.unwrap() * 100.0, 2, true));
                        crate::eval_calc::warn_at(e.line, &msg);
                    }
                }
                Ok(Value::Num(v))
            }
            ExprKind::OdeLinSolve { m, b, t, text, fmt } => {
                let n = b.len();
                let mut a = vec![];
                for x in m {
                    a.push(self.eval(x, fr)?.num());
                }
                let mut bv = vec![];
                for x in b {
                    bv.push(self.eval(x, fr)?.num());
                }
                let (xs, piv) = solve_linear(&a, n, &bv);
                if piv.iter().any(|p| *p == 0.0) {
                    let tv = self.eval(t, fr)?.num();
                    self.line = e.line;
                    return Err(self.fail_solve(Fail::new(E::ODE_SINGULAR, tv, *text as f64), *fmt));
                }
                Ok(Value::Vec(Rc::new(xs)))
            }
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }
}

/// fermium.linalg.solve with one right-hand side: Gaussian elimination with branch-free partial pivoting.
/// Returns the solution and the pivots (a zero pivot: singular).
fn solve_linear(a: &[f64], n: usize, b: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let w = n + 1;
    let mut rows: Vec<Vec<f64>> = (0..n).map(|i| {
        let mut r: Vec<f64> = a[i * n..(i + 1) * n].to_vec();
        r.push(b[i]);
        r
    }).collect();
    let mut pivots = vec![];
    for k in 0..n {
        for i in k + 1..n {
            let swap = rows[i][k].abs() > rows[k][k].abs();
            if swap {
                for j in k..w {
                    let (x, y) = (rows[k][j], rows[i][j]);
                    rows[k][j] = y;
                    rows[i][j] = x;
                }
            }
        }
        let p = rows[k][k];
        pivots.push(p);
        for i in k + 1..n {
            let f = rows[i][k] / p;
            for j in k + 1..w {
                rows[i][j] -= f * rows[k][j];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut acc = rows[i][n];
        for q in i + 1..n {
            acc -= rows[i][q] * x[q];
        }
        x[i] = acc / rows[i][i];
    }
    (x, pivots)
}
