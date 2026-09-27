//! Uncertain values through the numerical kernels (Fermium 2.5, spec C7; DECISIONS D276–D278): integrals and
//! ODE solutions whose inputs are uncertain. v1 stopped with "can't use uncertain values (±) yet".
//!
//! Linear propagation first, with exact derivatives: an uncertain value carries its contributions c_k = ∂x/∂z_k
//! (one per independent source k, z_k a standard normal), and the tree-walker's arithmetic on those values is
//! forward-mode differentiation. So
//! - an integrand evaluated with its uncertain inputs gives ∂f/∂z_k at each x, and ∂I/∂z_k = ∫ ∂f/∂z_k dx
//!   (differentiation under the integral sign, one quadrature per source); uncertain limits add f(b)·∂b/∂z_k −
//!   f(a)·∂a/∂z_k;
//! - an ODE's right side evaluated with the state y + Σ S_k ε_k and the uncertain parameters gives
//!   f(t, y) and J S_k + ∂f/∂z_k at once, so the sensitivities S_k = ∂y/∂z_k are solved alongside y by the same
//!   adaptive solver (the variational equations; D277).
//!
//! The linearization is then checked: each source is moved by ±1σ (z_k = ±1) and the kernel recomputed with
//! plain numbers. If the change is not close to linear there (|I₊ + I₋ − 2 I₀| / 2 > 10 % of |I₊ − I₋| / 2, and
//! above the kernel's own accuracy), the result comes from Monte Carlo instead (D278), with a warning.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use fermium_ir::{Lambda, Stmt, StmtKind, Ty};
use fermium_runtime::numerics::{ode, quad};
use fermium_runtime::numerics::uncertain::{self as U, UFloat};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};
use crate::eval_solve::SolData;
use crate::eval_unc::{fixed_begin, fixed_end, fixed_sample, is_unc_error, mc_begin, mc_sample, mc_take, mc_ufloat};

/// Samples for the Monte Carlo fallback of a kernel (as a  propagate montecarlo  block that needs one sample at
/// a time).
pub const MC_KERNEL: usize = 10_000;

/// The linearization test: the second-order change over ±1σ may be at most this fraction of the first-order one.
pub const LIN_TOL: f64 = 0.1;

/// The contribution of source k in an uncertain value (0 for a plain one).
pub(crate) fn contrib(v: &Value, k: u64) -> f64 {
    match v {
        Value::Unc(u) => u.d.iter().find(|(s, _)| *s == k).map(|(_, c)| *c).unwrap_or(0.0),
        _ => 0.0,
    }
}

pub(crate) fn nominal(v: &Value) -> f64 {
    match v {
        Value::Unc(u) => u.v,
        v => v.num(),
    }
}

/// Adds the sources of an uncertain value to a list (first appearance order).
pub(crate) fn add_sources(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Unc(u) => {
            for (k, _) in &u.d {
                if !out.contains(k) {
                    out.push(*k);
                }
            }
        }
        Value::UVec(xs) => xs.iter().for_each(|x| add_sources(x, out)),
        Value::UList(xs) => xs.borrow().iter().for_each(|x| add_sources(x, out)),
        _ => {}
    }
}

/// Is the change over ±1σ close to linear? y0 the value, (yp, ym) at z = +1 and −1; `noise` the kernel's own
/// accuracy on this scale.
pub(crate) fn linear_enough(y0: f64, yp: f64, ym: f64, noise: f64) -> bool {
    let d1 = (yp - ym) / 2.0;
    let d2 = (yp + ym - 2.0 * y0) / 2.0;
    d2.abs() <= LIN_TOL * d1.abs() || d2.abs() <= noise
}

/// The warning for a kernel that fell back to Monte Carlo.
pub(crate) fn mc_warning(what: &str, plain_needed: bool, n: usize) -> String {
    let why = if plain_needed {
        "uses an uncertain value where a plain number is needed (a loop bound, an index), which first-order \
         propagation can't follow"
    } else {
        "is too far from linear in its uncertain inputs over ±1σ for first-order propagation"
    };
    format!("{what} {why}; its uncertainty comes from Monte Carlo ({n} samples; see  propagate montecarlo  to \
             set the number)")
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// lam(x) without the kernel check (the value may be uncertain).
    pub(crate) fn call_lambda_raw(&mut self, lam: usize, x: Value, fr: &mut Frame) -> Result<Value, RunError> {
        let m: &'m fermium_ir::Module = self.module;
        let l = &m.lambdas[lam];
        let p = l.params[0];
        let old = fr.vars.insert(p, x);
        let r = self.eval(&l.body[0], fr);
        match old {
            Some(v) => {
                fr.vars.insert(p, v);
            }
            None => {
                fr.vars.remove(&p);
            }
        }
        r
    }

    /// A plain quadrature of lam from a to b (errors from the integrand come back as errors).
    fn quad_plain(&mut self, lam: usize, a: f64, b: f64, atol: f64, name: f64, raw: bool, fr: &mut Frame)
                  -> Result<Result<quad::QuadResult, fermium_runtime::numerics::Fail>, RunError> {
        let mut error: Option<RunError> = None;
        let line = self.line;
        let r = quad::quad(
            |x| {
                if error.is_some() {
                    return 0.0;
                }
                let v = if raw { self.call_lambda_raw(lam, Value::Num(x), fr) } else {
                    self.call_lambda_value(lam, Value::Num(x), fr)
                };
                match v {
                    Ok(v) => nominal(&v),
                    Err(ex) => {
                        error = Some(ex);
                        0.0
                    }
                }
            },
            a,
            b,
            1e-10,
            atol,
            name,
        );
        self.line = line;
        match error {
            Some(e) => Err(e),
            None => Ok(r),
        }
    }

    /// ∫ lam dx from va to vb where the integrand or a limit is uncertain (C7, D276): linear propagation with
    /// exact derivatives, checked at ±1σ per source, else Monte Carlo. `fail` turns a quadrature failure into the
    /// program's error.
    pub(crate) fn integral_unc(&mut self, lam: usize, va: &Value, vb: &Value, atol: f64, name: f64,
                               fail: &dyn Fn(&Self, fermium_runtime::numerics::Fail) -> RunError, fr: &mut Frame)
                               -> Result<Value, RunError> {
        let started = kernel_pm_begin();
        let r = self.integral_unc_in(lam, va, vb, atol, name, fail, fr);
        kernel_pm_end(started);
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn integral_unc_in(&mut self, lam: usize, va: &Value, vb: &Value, atol: f64, name: f64,
                       fail: &dyn Fn(&Self, fermium_runtime::numerics::Fail) -> RunError, fr: &mut Frame)
                       -> Result<Value, RunError> {
        let (a, b) = (nominal(va), nominal(vb));
        let line = self.line;
        // the value, and the sources the integrand meets
        let mut srcs: Vec<u64> = vec![];
        add_sources(va, &mut srcs);
        add_sources(vb, &mut srcs);
        let mut error: Option<RunError> = None;
        let r0 = quad::quad(
            |x| {
                if error.is_some() {
                    return 0.0;
                }
                match self.call_lambda_raw(lam, Value::Num(x), fr) {
                    Ok(v) => {
                        add_sources(&v, &mut srcs);
                        nominal(&v)
                    }
                    Err(ex) => {
                        error = Some(ex);
                        0.0
                    }
                }
            },
            a,
            b,
            1e-10,
            atol,
            name,
        );
        self.line = line;
        let linear_ok = match &error {
            // the integrand needs plain numbers somewhere (an index, a loop bound): Monte Carlo only
            Some(e) if is_unc_error(e) => false,
            Some(e) => return Err(e.clone()),
            None => true,
        };
        if linear_ok {
            let r0 = r0.map_err(|f| fail(self, f))?;
            let i0 = r0.value;
            let noise = 1e3 * r0.error.max(1e-13 * r0.abs_sum.max(i0.abs()));
            let mut d: Vec<(u64, f64)> = vec![];
            let mut ok = true;
            for &k in &srcs {
                // ∂I/∂z_k: under the integral sign, and through the limits
                let mut cmax = 0.0f64;
                for x in [a, b, 0.5 * (a + b)] {
                    if x.is_finite() {
                        if let Ok(v) = self.call_lambda_raw(lam, Value::Num(x), fr) {
                            cmax = cmax.max(contrib(&v, k).abs());
                        }
                    }
                }
                let width = if (b - a).is_finite() { (b - a).abs() } else { 1.0 };
                let katol = (1e-12 * cmax * width).max(1e-300);
                let mut err2: Option<RunError> = None;
                let rk = quad::quad(
                    |x| {
                        if err2.is_some() {
                            return 0.0;
                        }
                        match self.call_lambda_raw(lam, Value::Num(x), fr) {
                            Ok(v) => contrib(&v, k),
                            Err(ex) => {
                                err2 = Some(ex);
                                0.0
                            }
                        }
                    },
                    a,
                    b,
                    1e-10,
                    katol,
                    name,
                );
                self.line = line;
                if let Some(e) = err2 {
                    return Err(e);
                }
                let Ok(rk) = rk else {
                    ok = false;
                    break;
                };
                let mut c = rk.value;
                for (lim, sign) in [(vb, 1.0), (va, -1.0)] {
                    let ck = contrib(lim, k);
                    if ck != 0.0 {
                        let f = self.call_lambda_raw(lam, Value::Num(nominal(lim)), fr)?;
                        c += sign * nominal(&f) * ck;
                    }
                }
                if nested() {
                    if c != 0.0 {
                        d.push((k, c));
                    }
                    continue;
                }
                // the check: I at z_k = ±1 with plain numbers
                let prev = fixed_begin(2, vec![(k, vec![1.0, -1.0])]);
                let mut ends = [0.0; 2];
                let mut res = Ok(());
                for (j, e) in ends.iter_mut().enumerate() {
                    fixed_sample(j);
                    let (sa, sb) = (nominal(&mc_sample(va)), nominal(&mc_sample(vb)));
                    match self.quad_plain(lam, sa, sb, atol, name, false, fr) {
                        Ok(Ok(r)) => *e = r.value,
                        Ok(Err(_)) => *e = f64::NAN,
                        Err(ex) => {
                            res = Err(ex);
                            break;
                        }
                    }
                }
                fixed_end(prev);
                self.line = line;
                if let Err(e) = res {
                    if is_unc_error(&e) {
                        ok = false;
                        break;
                    }
                    return Err(e);
                }
                if !linear_enough(i0, ends[0], ends[1], noise) {
                    ok = false;
                    break;
                }
                if c != 0.0 {
                    d.push((k, c));
                }
            }
            if ok {
                return Ok(Value::Unc(Rc::new(UFloat::new(i0, d))));
            }
        }
        // Monte Carlo: every source drawn from the program's random numbers
        let n = MC_KERNEL;
        let zs: Vec<(u64, Vec<f64>)> =
            srcs.iter().map(|&k| (k, (0..n).map(|_| crate::eval_m3::randn_draw()).collect())).collect();
        let prev = mc_begin(n, zs);
        let mut ys = Vec::with_capacity(n);
        let mut res = Ok(());
        for j in 0..n {
            fixed_sample(j);
            let (sa, sb) = (nominal(&mc_sample(va)), nominal(&mc_sample(vb)));
            match self.quad_plain(lam, sa, sb, atol, name, false, fr) {
                Ok(Ok(r)) => ys.push(r.value),
                Ok(Err(f)) => {
                    res = Err(fail(self, f));
                    break;
                }
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        let zs = mc_take(prev);
        self.line = line;
        res?;
        crate::eval_calc::warn_at(line, &mc_warning("this integral", !linear_ok, n));
        let srcs: Vec<u64> = zs.iter().map(|(k, _)| *k).collect();
        let zr: Vec<&[f64]> = zs.iter().map(|(_, z)| &z[..]).collect();
        let _ = U::NOISE;
        Ok(Value::Unc(Rc::new(mc_ufloat(&ys, &zr, &srcs))))
    }
}

// ---------------------------------------------------------------- ODE solutions (D277)

/// Samples for the Monte Carlo fallback of an ODE solve (each is a whole solve).
pub const MC_ODE: usize = 2_000;

thread_local! {
    /// > 0 while a kernel's linear pass evaluates (an integral inside an ODE's right side): nested kernels then
    /// propagate linearly without their own ±1σ check or Monte Carlo.
    static KERNEL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// While a kernel with uncertain inputs runs: the value of each ± written inside it (by IR node), so that
    /// every evaluation of the integrand or right side sees the same measurement.
    static KERNEL_PM: RefCell<Option<HashMap<usize, Value>>> = const { RefCell::new(None) };
}

pub(crate) fn kernel_pm_active() -> bool {
    KERNEL_PM.with(|k| k.borrow().is_some())
}

pub(crate) fn kernel_pm_get(key: usize) -> Option<Value> {
    KERNEL_PM.with(|k| k.borrow().as_ref().and_then(|m| m.get(&key).cloned()))
}

pub(crate) fn kernel_pm_put(key: usize, v: Value) {
    KERNEL_PM.with(|k| {
        if let Some(m) = k.borrow_mut().as_mut() {
            m.insert(key, v);
        }
    })
}

/// Starts a kernel's ± cache (unless an enclosing kernel has one); true if this call started it.
fn kernel_pm_begin() -> bool {
    KERNEL_PM.with(|k| {
        let mut k = k.borrow_mut();
        if k.is_some() {
            return false;
        }
        *k = Some(HashMap::new());
        true
    })
}

fn kernel_pm_end(started: bool) {
    if started {
        KERNEL_PM.with(|k| *k.borrow_mut() = None);
    }
}

pub(crate) fn nested() -> bool {
    KERNEL_DEPTH.with(|d| d.get() > 0)
}

fn depth(delta: i32) {
    KERNEL_DEPTH.with(|d| d.set((d.get() as i32 + delta).max(0) as u32));
}

/// How a solution with uncertain inputs answers x(t).
pub(crate) enum UncSol {
    /// the stored solution's state is y (n components) followed by ∂y/∂z_k for each source k
    Lin { n: usize, srcs: Vec<u64> },
    /// Monte Carlo: one solution per sample of the sources' streams (the stored one is the nominal solve)
    Mc { srcs: Vec<u64>, zs: Vec<Vec<f64>>, sols: Vec<ode::Sol>,
         /// the source of the nonlinear part at each (component, t, derivative?) asked for, so that asking twice
         /// gives the same (fully correlated) value
         resid: RefCell<HashMap<(usize, u64, bool), u64>> },
}

fn merge(d: &mut Vec<(u64, f64)>, k: u64, c: f64) {
    match d.iter().position(|(s, _)| *s == k) {
        Some(i) => d[i].1 += c,
        None => d.push((k, c)),
    }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// An ODE right side at t with the given state values (numbers or uncertain numbers), flattened.
    pub(crate) fn ode_rhs_values(&mut self, lam: &Lambda, t: f64, vals: &[Value], fr: &mut Frame)
                                 -> Result<Vec<Value>, RunError> {
        let saved = self.memo_begin();
        let r = (|| {
            let module = self.module;
            if let Some(&p) = lam.params.first() {
                fr.vars.insert(p, Value::Num(t));
            }
            let mut i = 0;
            for &s in &lam.state {
                let n = crate::eval_solve::slots(&module.syms[s].ty);
                let part = &vals[i.min(vals.len())..(i + n).min(vals.len())];
                if matches!(module.syms[s].ty, Ty::Num(_)) {
                    fr.vars.insert(s, part.first().cloned().unwrap_or(Value::Num(f64::NAN)));
                } else {
                    fr.vars.insert(s, crate::eval_unc::make_vec(part.to_vec()));
                }
                i += n;
            }
            let mut out = vec![];
            for e in &lam.body {
                match self.eval(e, fr)? {
                    Value::Vec(v) => out.extend(v.iter().map(|x| Value::Num(*x))),
                    Value::UVec(v) => out.extend(v.iter().cloned()),
                    v @ Value::Unc(_) => out.push(v),
                    Value::UList(_) => return self.err(crate::eval_unc::GENERIC),
                    v => out.push(Value::Num(v.num())),
                }
            }
            Ok(out)
        })();
        self.memo_end(saved);
        r
    }

    /// The right side of the state plus its sensitivities: y and S_k (row k of n) in, f and J S_k + ∂f/∂z_k out,
    /// by evaluating the right side on uncertain numbers whose contributions are the S_k (forward-mode
    /// differentiation, D277).
    pub(crate) fn ode_call_aug(&mut self, lam: &Lambda, t: f64, y: &[f64], out: &mut [f64], n: usize, srcs: &[u64],
                               fr: &mut Frame) -> Result<(), RunError> {
        let vals: Vec<Value> = (0..n)
            .map(|i| {
                let d = srcs.iter().enumerate()
                    .filter_map(|(k, &s)| {
                        let c = y[n + k * n + i];
                        if c != 0.0 { Some((s, c)) } else { None }
                    })
                    .collect();
                Value::Unc(Rc::new(UFloat::new(y[i], d)))
            })
            .collect();
        let fv = self.ode_rhs_values(lam, t, &vals, fr)?;
        for j in 0..n.min(fv.len()) {
            out[j] = nominal(&fv[j]);
            for (k, &s) in srcs.iter().enumerate() {
                out[n + k * n + j] = contrib(&fv[j], s);
            }
        }
        Ok(())
    }

    /// solve … with uncertain starting values, start time or right side (C7, D277): the sensitivities solved
    /// alongside, checked at ±1σ per source; else Monte Carlo.
    pub(crate) fn solve_ode_unc(&mut self, s: &Stmt, y0v: &[Value], vt0: &Value, t1: f64, h0: Option<f64>,
                                fr: &mut Frame) -> Result<(), RunError> {
        let started = kernel_pm_begin();
        let r = self.solve_ode_unc_in(s, y0v, vt0, t1, h0, fr);
        kernel_pm_end(started);
        r
    }

    fn solve_ode_unc_in(&mut self, s: &Stmt, y0v: &[Value], vt0: &Value, t1: f64, h0: Option<f64>,
                        fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Solve { sol, rhs, x, .. } = &s.kind else { unreachable!() };
        let module = self.module;
        let lam = &module.lambdas[*rhs];
        let n = y0v.len();
        let t0 = nominal(vt0);
        let line = s.line;
        let mut srcs: Vec<u64> = vec![];
        y0v.iter().for_each(|v| add_sources(v, &mut srcs));
        add_sources(vt0, &mut srcs);
        depth(1);
        let probe = self.ode_rhs_values(lam, t0, y0v, fr);
        depth(-1);
        self.line = line;
        let mut linear = match probe {
            Ok(fv) => {
                fv.iter().for_each(|v| add_sources(v, &mut srcs));
                Some(fv)
            }
            Err(e) if is_unc_error(&e) => None,
            Err(e) => return Err(e),
        };
        let y0n: Vec<f64> = y0v.iter().map(nominal).collect();
        let mut plain_needed = linear.is_none();
        if linear.is_some() && srcs.is_empty() {
            // only the end time is uncertain: the solution doesn't depend on it
            let solv = self.ode_core(s, &y0n, t0, t1, h0, None, fr)?;
            let snap = self.snapshot(*rhs, fr);
            self.store_sol(*sol, SolData { sol: solv, rhs: Some((*rhs, snap)), grid: None, lens: None, check: None, unc: None },
                           fr);
            return Ok(());
        }
        if let Some(fv) = linear.take() {
            let mut yaug = y0n.clone();
            for &k in &srcs {
                for i in 0..n {
                    yaug.push(contrib(&y0v[i], k) - nominal(&fv[i]) * contrib(vt0, k));
                }
            }
            depth(1);
            let r = self.ode_core(s, &yaug, t0, t1, h0, Some(&srcs), fr);
            depth(-1);
            match r {
                Ok(aug) => {
                    if nested() || self.ode_linear_enough(s, &aug, n, &srcs, y0v, vt0, t1, h0, fr)? {
                        let snap = self.snapshot(*rhs, fr);
                        self.store_sol(*sol, SolData { sol: aug, rhs: Some((*rhs, snap)), grid: None, lens: None, check: None,
                                                       unc: Some(UncSol::Lin { n, srcs }) }, fr);
                        return Ok(());
                    }
                }
                Err(e) if is_unc_error(&e) => plain_needed = true,
                Err(e) => return Err(e),
            }
        }
        // Monte Carlo: whole solves with sampled inputs
        let _ = x;
        let nmc = MC_ODE;
        let zs: Vec<Vec<f64>> = srcs.iter().map(|_| (0..nmc).map(|_| crate::eval_m3::randn_draw()).collect()).collect();
        let prev = mc_begin(nmc, srcs.iter().copied().zip(zs).collect());
        let mut sols = Vec::with_capacity(nmc);
        let mut res = Ok(());
        for j in 0..nmc {
            fixed_sample(j);
            let ys: Vec<f64> = y0v.iter().map(|v| nominal(&mc_sample(v))).collect();
            let ts = nominal(&mc_sample(vt0));
            match self.ode_core(s, &ys, ts, t1, h0, None, fr) {
                Ok(sv) => sols.push(sv),
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        let zs = mc_take(prev);
        let srcs: Vec<u64> = zs.iter().map(|(k, _)| *k).collect();
        let zs: Vec<Vec<f64>> = zs.into_iter().map(|(_, z)| z).collect();
        res?;
        // the nominal solve: every source at its value
        let prev = fixed_begin(1, vec![]);
        let nom = self.ode_core(s, &y0n, t0, t1, h0, None, fr);
        fixed_end(prev);
        let nom = nom?;
        self.line = line;
        crate::eval_calc::warn_at(line, &mc_warning("this differential equation's solution", plain_needed, nmc));
        let snap = self.snapshot(*rhs, fr);
        self.store_sol(*sol, SolData { sol: nom, rhs: Some((*rhs, snap)), grid: None, lens: None, check: None,
                                       unc: Some(UncSol::Mc { srcs, zs, sols, resid: RefCell::new(HashMap::new()) }) },
                       fr);
        Ok(())
    }

    /// The ±1σ test of a linearized solution: each source moved by ±1σ, solved with plain numbers and compared
    /// with the nominal solution at 8 times across it, component by component.
    #[allow(clippy::too_many_arguments)]
    fn ode_linear_enough(&mut self, s: &Stmt, aug: &ode::Sol, n: usize, srcs: &[u64], y0v: &[Value], vt0: &Value,
                         t1: f64, h0: Option<f64>, fr: &mut Frame) -> Result<bool, RunError> {
        let StmtKind::Solve { x, .. } = &s.kind else { unreachable!() };
        let (ta, tb) = (aug.t[0], aug.t[aug.n() - 1]);
        let maxabs: Vec<f64> = (0..n)
            .map(|i| (0..aug.n()).map(|j| aug.y[j * aug.dim + i].abs()).fold(0.0, f64::max))
            .collect();
        for &k in srcs {
            let prev = fixed_begin(2, vec![(k, vec![1.0, -1.0])]);
            let mut ends: Vec<ode::Sol> = vec![];
            let mut res = Ok(());
            for j in 0..2 {
                fixed_sample(j);
                let ys: Vec<f64> = y0v.iter().map(|v| nominal(&mc_sample(v))).collect();
                let ts = nominal(&mc_sample(vt0));
                match self.ode_core(s, &ys, ts, t1, h0, None, fr) {
                    Ok(sv) => ends.push(sv),
                    Err(e) => {
                        res = Err(e);
                        break;
                    }
                }
            }
            fixed_end(prev);
            self.line = s.line;
            match res {
                Err(e) if is_unc_error(&e) => return Ok(false),
                Err(_) => return Ok(false), // a ±1σ input the solve can't handle: not linear there
                Ok(()) => {}
            }
            for q in 1..=8 {
                let t = ta + (tb - ta) * q as f64 / 8.0;
                for i in 0..n {
                    let (Ok(y0), Ok(yp), Ok(ym)) = (aug.eval(i, t, false, None), ends[0].eval(i, t, false, None),
                                                    ends[1].eval(i, t, false, None)) else { continue };
                    let noise = 100.0 * x.rtol.max(1e-12) * maxabs[i];
                    if !linear_enough(y0, yp, ym, noise) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    /// x(t) (or x'(t)) of a solution with uncertain inputs; t may be uncertain too.
    pub(crate) fn sol_value_unc(&mut self, d: &SolData, comp: usize, tv: &Value, use_dy: bool, fmt: usize)
                                -> Result<Value, RunError> {
        let t = nominal(tv);
        let line = self.line;
        let v0 = self.sol_at_plain(d, comp, t, use_dy, fmt)?; // the range check, and the value
        self.line = line;
        let mut dd: Vec<(u64, f64)> = vec![];
        let mut val = v0;
        match d.unc.as_ref().unwrap() {
            UncSol::Lin { n, srcs } => {
                let (n, srcs) = (*n, srcs.clone());
                let full: Vec<f64> = if use_dy {
                    self.aug_rhs_at(d, t, n, &srcs)?
                } else {
                    (0..d.sol.dim).map(|c| d.sol.eval(c, t, false, None).unwrap_or(f64::NAN)).collect()
                };
                for (k, &s) in srcs.iter().enumerate() {
                    let c = full[n + k * n + comp];
                    if c != 0.0 {
                        dd.push((s, c));
                    }
                }
            }
            UncSol::Mc { srcs, zs, sols, resid } => {
                let ys: Vec<f64> = sols.iter().map(|sv| sv.eval(comp, t, use_dy, None).unwrap_or(f64::NAN)).collect();
                let bad = ys.iter().filter(|y| !y.is_finite()).count();
                if bad > 0 {
                    return self.err(format!("{bad} of the {} Monte Carlo solutions have no finite value there (they \
                                             end earlier, or fail for some sampled inputs)", ys.len()));
                }
                let zr: Vec<&[f64]> = zs.iter().map(|z| &z[..]).collect();
                let u = mc_ufloat(&ys, &zr, srcs);
                val = u.v;
                for (s, c) in u.d {
                    if srcs.contains(&s) {
                        dd.push((s, c));
                    } else {
                        // the nonlinear part: the same source whenever this value is asked for
                        let key = (comp, t.to_bits(), use_dy);
                        let id = *resid.borrow_mut().entry(key).or_insert(s);
                        dd.push((id, c));
                    }
                }
            }
        }
        if let Value::Unc(tu) = tv {
            if use_dy {
                return self.err(crate::eval_unc::GENERIC);
            }
            // an uncertain time: the slope times its uncertainty
            let slope = d.sol.eval(comp, t, true, None).map_err(|f| self.fail_solve(f, fmt))?;
            for &(s, c) in &tu.d {
                merge(&mut dd, s, slope * c);
            }
        }
        dd.retain(|(_, c)| *c != 0.0 || c.is_nan());
        Ok(Value::Unc(Rc::new(UFloat::new(val, dd))))
    }

    /// The nominal value of a component (the range check and its error).
    fn sol_at_plain(&mut self, d: &SolData, comp: usize, t: f64, use_dy: bool, fmt: usize) -> Result<f64, RunError> {
        if use_dy {
            if let Some(UncSol::Lin { n, srcs }) = &d.unc {
                d.sol.eval(comp, t, false, None).map_err(|f| self.fail_solve(f, fmt))?;
                let (n, srcs) = (*n, srcs.clone());
                return Ok(self.aug_rhs_at(d, t, n, &srcs)?[comp]);
            }
            // x'(t) of the nominal solve: the right side with every source at its value
            let prev = fixed_begin(1, vec![]);
            let r = self.sol_at(d, comp, t, true, fmt);
            fixed_end(prev);
            return r;
        }
        d.sol.eval(comp, t, false, None).map_err(|f| self.fail_solve(f, fmt))
    }

    /// The right side of state + sensitivities at the interpolated state at t (x'(t) with its uncertainty).
    fn aug_rhs_at(&mut self, d: &SolData, t: f64, n: usize, srcs: &[u64]) -> Result<Vec<f64>, RunError> {
        let module = self.module;
        let Some((lam, snap)) = &d.rhs else { return self.err(crate::eval_unc::GENERIC) };
        let lam = &module.lambdas[*lam];
        let mut frame = Frame { vars: snap.vars.clone() };
        let mut err = None;
        let mut full = vec![f64::NAN; d.sol.dim];
        {
            let me = &mut *self;
            let err = &mut err;
            let full = &mut full;
            let mut f = |t: f64, y: &[f64], out: &mut [f64]| {
                if let Err(e) = me.ode_call_aug(lam, t, y, out, n, srcs, &mut frame) {
                    *err = Some(e);
                    out.fill(f64::NAN);
                }
                full.copy_from_slice(out);
            };
            let _ = d.sol.eval(0, t, true, Some(&mut f));
        }
        if let Some(e) = err {
            return Err(e);
        }
        Ok(full)
    }

    /// A solution's stored values (or derivatives) as a list, with their uncertainties when the sensitivities
    /// were solved alongside.
    pub(crate) fn sol_list_unc(&self, d: &SolData, comp: usize, what: u8) -> Option<Value> {
        let Some(UncSol::Lin { n, srcs }) = &d.unc else { return None };
        let s = &d.sol;
        let src = if what == 0 { &s.y } else { &s.dy };
        let items = (0..s.n())
            .map(|i| {
                let row = &src[i * s.dim..(i + 1) * s.dim];
                let dd = srcs.iter().enumerate()
                    .filter_map(|(k, &sid)| {
                        let c = row[n + k * n + comp];
                        if c != 0.0 { Some((sid, c)) } else { None }
                    })
                    .collect();
                Value::Unc(Rc::new(UFloat::new(row[comp], dd)))
            })
            .collect();
        Some(crate::eval_unc::make_list(items))
    }
}
