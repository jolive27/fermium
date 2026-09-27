//! The run-time side of `solve` in the LLVM back end: ODE, eigenvalue and PDE solves with the right-hand side
//! compiled (`OdeFn`), and reading the solutions. It mirrors eval_solve.rs (the tree-walker's), which mirrors v1's
//! compiled path; the numerics are fermium-runtime's, the messages eval_solve's `describe_error` and `rt_warn`.
use fermium_ir::Module;
use fermium_runtime::numerics::{self as nx, err as E, ode, Fail};

use super::rt::{fm_list_from, locked, Ctx, FmList, Kind, C};
use crate::eval::RunError;
use crate::eval_solve::describe_error;

/// A compiled right-hand side: out[0..nout) = f(t, y) (ode_call's convention: t, then the state from y).
pub type OdeFn = unsafe extern "C" fn(t: f64, y: *const f64, out: *mut f64, nout: i64, env: *mut u8);

/// What the compiled code knows about a solve statement.
pub struct OdeSite {
    pub method: String,
    pub rtol: f64,
    pub atol: Option<Vec<(f64, u32)>>,
    pub tname: usize,
    pub evtext: i64,
    pub tdep: bool,
    pub tfmt: usize,
    /// the kinds of the env slots (for the snapshot the solution keeps, D46)
    pub env_kinds: Vec<Kind>,
    // eigenvalue problems and PDEs
    pub nstates: usize,
    pub grid: usize,
    pub eig_method: u8,
    pub order: u8,
    pub pmethod: u8,
    pub bc: (u8, u8),
    pub is_complex: bool,
    pub xname: usize,
    pub pde_line: u32,
}

/// A copy of the values the right side reads (eval_solve snapshot): the env the solution's x'(t) uses.
pub struct Snapshot {
    _data: Vec<u64>,
    ptrs: Vec<*mut u8>,
}

pub struct PdeCheck {
    coarse: ode::Sol,
    ranges: Vec<f64>,
    xname: usize,
    tname: usize,
    warned: std::cell::Cell<bool>,
}

/// A solution made by the compiled code.
pub struct CSol {
    /// the index of the tree-walker's copy of it (for plots and the other constructs the tree-walker runs)
    pub interp_h: usize,
    /// shared with the tree-walker's copy (no second copy of a million-step solution)
    pub(crate) sol: std::rc::Rc<crate::eval_solve::SolData>,
    pub rhs: Option<(OdeFn, Snapshot)>,
    pub grid: Option<(f64, f64)>,
    pub check: Option<PdeCheck>,
}

fn bytes_of(k: Kind) -> usize {
    match k {
        Kind::B => 1,
        Kind::V(n) => 8 * n,
        _ => 8,
    }
}

unsafe fn snapshot(env: *const *mut u8, kinds: &[Kind]) -> Snapshot {
    let total: usize = kinds.iter().map(|k| bytes_of(*k).div_ceil(8)).sum();
    let mut data = vec![0u64; total.max(1)];
    let mut ptrs = Vec::with_capacity(kinds.len().max(1));
    let mut at = 0;
    for (i, k) in kinds.iter().enumerate() {
        let dst = data.as_mut_ptr().add(at) as *mut u8;
        std::ptr::copy_nonoverlapping(*env.add(i), dst, bytes_of(*k));
        ptrs.push(dst);
        at += bytes_of(*k).div_ceil(8);
    }
    if ptrs.is_empty() {
        ptrs.push(std::ptr::null_mut());
    }
    Snapshot { _data: data, ptrs }
}

fn fail(c: &mut Ctx, f: Fail, fmt: usize, line: u32) {
    let e = RunError { message: describe_error(c.module, f.kind, f.a, f.b, fmt), line, hint: None };
    c.fail(e);
}

fn flag<'a>(c: C<'a>) -> &'a std::sync::atomic::AtomicI32 {
    unsafe { &*(c as *const std::sync::atomic::AtomicI32) }
}

fn stopped(c: C) -> bool {
    flag(c).load(std::sync::atomic::Ordering::SeqCst) != 0
}

/// An ODE solve (eval_solve solve_ode); the handle of the solution, or -1 after an error.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn fm_ode(c: C, site: i64, f: OdeFn, env: *mut u8, ev: Option<OdeFn>, evenv: *mut u8, y0: *const f64,
                         n: i64, t0: f64, t1: f64, h0: f64, line: i32) -> i64 {
    let line = line.max(0) as u32;
    let s = unsafe { &(&(*c).ode_sites)[site as usize] };
    let y0 = unsafe { std::slice::from_raw_parts(y0, n as usize) }.to_vec();
    let atol = s.atol.as_ref().and_then(|spec| ode::abs_tolerances(spec, t0, t1));
    let opts = ode::OdeOpts { rtol: s.rtol, atol: atol.as_deref(), tname: s.tname as f64, evtext: s.evtext as f64,
                              tdep: s.tdep };
    let rhs = |t: f64, y: &[f64], out: &mut [f64]| {
        if stopped(c) {
            out.fill(f64::NAN);
            return;
        }
        unsafe { f(t, y.as_ptr(), out.as_mut_ptr(), out.len() as i64, env) };
        if stopped(c) {
            out.fill(f64::NAN);
        }
    };
    let mut gfun = |t: f64, y: &[f64]| -> f64 {
        if stopped(c) {
            return f64::NAN;
        }
        let mut out = [0.0];
        unsafe { ev.unwrap()(t, y.as_ptr(), out.as_mut_ptr(), 1, evenv) };
        if stopped(c) {
            return f64::NAN;
        }
        out[0]
    };
    let evf: Option<ode::EventFn<'_>> = if ev.is_some() { Some(&mut gfun) } else { None };
    let mut early: Vec<(i64, f64)> = vec![];
    let r = match s.method.as_str() {
        "radau" | "bdf" => {
            let m = if s.method == "bdf" { nx::stiff::StiffMethod::Bdf } else { nx::stiff::StiffMethod::Radau };
            nx::stiff::stiff_solve(rhs, &y0, t0, t1, m, evf, opts)
        }
        "rk4" => ode::rk4(rhs, &y0, t0, t1, h0, evf, opts),
        _ => {
            let mut sv = ode::Sol::new(y0.len());
            match ode::dp45_into(rhs, &y0, t0, t1, evf, opts, &mut sv) {
                Ok(()) => Ok(sv),
                Err(fl) => {
                    early = sv.warnings;
                    Err(fl)
                }
            }
        }
    };
    if stopped(c) {
        return -1;
    }
    finish_ode(c, site, f, env, r, early, line)
}

/// The end of an ODE solve: its warnings (or its error), then the solution, kept with a snapshot of the right
/// side's env (for x'(t)) and mirrored for the tree-walker.
fn finish_ode(c: C, site: i64, f: OdeFn, env: *mut u8, r: Result<ode::Sol, Fail>, early: Vec<(i64, f64)>,
              line: u32) -> i64 {
    let s = unsafe { &(&(*c).ode_sites)[site as usize] };
    let (tfmt, kinds) = (s.tfmt, s.env_kinds.clone());
    locked(c, |c| {
        for &(kind, a) in &early {
            c.interp.rt_warn(kind, a, line);
        }
        let solv = match r {
            Ok(v) => v,
            Err(fl) => {
                fail(c, fl, tfmt, line);
                return -1;
            }
        };
        for &(kind, a) in &solv.warnings {
            c.interp.rt_warn(kind, a, line);
        }
        let snap = unsafe { snapshot(env as *const *mut u8, &kinds) };
        let (interp_h, sol) = mirror(c, solv, None);
        c.sols.push(CSol { interp_h, sol, rhs: Some((f, snap)), grid: None, check: None });
        c.sols.len() as i64 - 1
    })
}

/// The right side as fermium-runtime's solvers call it (after an error: NaN, and the solve stops).
fn rhs_of<'a>(c: C<'a>, f: OdeFn, env: *mut u8) -> impl FnMut(f64, &[f64], &mut [f64]) + 'a {
    move |t: f64, y: &[f64], out: &mut [f64]| {
        if stopped(c) {
            out.fill(f64::NAN);
            return;
        }
        unsafe { f(t, y.as_ptr(), out.as_mut_ptr(), out.len() as i64, env) };
        if stopped(c) {
            out.fill(f64::NAN);
        }
    }
}

/// Where the compiled fixed-step RK4 loop writes (fm_rk4_begin): the solution's arrays, reserved for every step.
#[repr(C)]
pub struct Rk4Plan {
    pub t: *mut f64,
    pub y: *mut f64,
    pub dy: *mut f64,
    pub h: f64,
    pub steps: i64,
    /// the solution being filled (boxed: a right side may call a function that solves another equation)
    pub sol: *mut ode::Sol,
}

/// A fixed-step RK4 solve (method rk4, no `until`) whose steps the compiled code takes itself, with the right side
/// inlined (v1 did the same): the checks and the first derivative of ode::rk4_plain, then the arrays reserved for
/// its steps + 1 samples. k1 gets f(t0, y0). 0, or -1 after an error.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn fm_rk4_begin(c: C, site: i64, f: OdeFn, env: *mut u8, y0: *const f64, n: i64, t0: f64, t1: f64,
                               h0: f64, line: i32, k1: *mut f64, plan: *mut Rk4Plan) -> i32 {
    let line = line.max(0) as u32;
    let s = unsafe { &(&(*c).ode_sites)[site as usize] };
    let n = n as usize;
    let y0 = unsafe { std::slice::from_raw_parts(y0, n) };
    let tname = s.tname as f64;
    // ode::rk4_plain's checks, in its order
    let span = t1 - t0;
    let pre = if !(span != 0.0) {
        Err(Fail::new(E::ODE_RANGE, t0, tname))
    } else {
        let ratio = (span / h0).abs();
        if ratio != ratio || ratio <= 0.0 || ratio > 1e12 {
            Err(Fail::new(E::STEP, h0, span))
        } else {
            Ok(((ratio - 1e-9).ceil() as u64).max(1))
        }
    };
    let steps = match pre {
        Ok(st) => st,
        Err(fl) => return if finish_ode(c, site, f, env, Err(fl), vec![], line) < 0 { -1 } else { 0 },
    };
    let h = span / steps as f64;
    let mut rhs = rhs_of(c, f, env);
    let mut k0 = vec![0.0; n];
    rhs(t0, y0, &mut k0);
    if stopped(c) {
        return -1;
    }
    if !k0.iter().all(|x| x - x == 0.0) {
        finish_ode(c, site, f, env, Err(Fail::new(E::ODE_NAN, t0, tname)), vec![], line);
        return -1;
    }
    unsafe { std::ptr::copy_nonoverlapping(k0.as_ptr(), k1, n) };
    let total = steps as usize + 1;
    let mut sol = ode::Sol::new(n);
    sol.t.reserve_exact(total);
    sol.y.reserve_exact(total * n);
    sol.dy.reserve_exact(total * n);
    unsafe {
        let (t, y, dy) = (sol.t.as_mut_ptr(), sol.y.as_mut_ptr(), sol.dy.as_mut_ptr());
        *plan = Rk4Plan { t, y, dy, h, steps: steps as i64, sol: Box::into_raw(Box::new(sol)) };
    }
    0
}

/// The end of a compiled RK4 solve whose loop filled every sample (sol and steps: fm_rk4_begin's plan): ode::rk4's error estimate and warning, then
/// fm_ode's end. The handle of the solution, or -1 after an error.
#[no_mangle]
pub extern "C" fn fm_rk4_end(c: C, site: i64, f: OdeFn, env: *mut u8, sol: *mut ode::Sol, steps: i64,
                             line: i32) -> i64 {
    let line = line.max(0) as u32;
    let mut sol = unsafe { *Box::from_raw(sol) };
    // the compiled loop wrote all steps + 1 samples into the arrays fm_rk4_begin reserved for them
    let total = steps as usize + 1;
    unsafe {
        sol.t.set_len(total);
        sol.y.set_len(total * sol.dim);
        sol.dy.set_len(total * sol.dim);
    }
    let mut rhs = rhs_of(c, f, env);
    let est = ode::rk4_error(&mut rhs, &sol);
    if est > ode::RK4_WARN {
        sol.warnings.push((ode::warn::RK4_COARSE, est));
    }
    if stopped(c) {
        return -1;
    }
    finish_ode(c, site, f, env, Ok(sol), vec![], line)
}

/// An eigenvalue problem (eval_solve solve_eigen): the right side at (x, [ψ, ψ', E]) gives ψ'' in out[1].
#[no_mangle]
pub extern "C" fn fm_eigen(c: C, site: i64, f: OdeFn, env: *mut u8, a: f64, b: f64, line: i32) -> i64 {
    let line = line.max(0) as u32;
    let s = unsafe { &(&(*c).ode_sites)[site as usize] };
    let method = if s.eig_method == 1 { nx::eigen::EigenMethod::Shooting } else { nx::eigen::EigenMethod::Matrix };
    let mut rhs = |xv: f64, p: f64, dp: f64, e: f64| -> f64 {
        if stopped(c) {
            return f64::NAN;
        }
        let y = [p, dp, e];
        let mut out = [0.0; 3];
        unsafe { f(xv, y.as_ptr(), out.as_mut_ptr(), 3, env) };
        if stopped(c) {
            return f64::NAN;
        }
        out[1]
    };
    let (nstates, grid, tname, tfmt) = (s.nstates, s.grid, s.tname, s.tfmt);
    let r = nx::eigen::eigen_solve(&mut rhs, a, b, nstates, grid, method);
    if stopped(c) {
        return -1;
    }
    locked(c, |c| {
        let module: &Module = c.module;
        let r = match r {
            Ok(r) => r,
            Err(ex) => {
                let msg = match ex.x {
                    None => ex.message,
                    Some(xv) => nx::eigen::singular_text(&tname_of(module, tname as f64),
                                                         &crate::eval_solve::fmt_value(module, xv, tfmt)),
                };
                c.fail(RunError { message: msg, line, hint: None });
                return -1;
            }
        };
        for &(k, rel) in &r.warnings {
            let n = k + 1;
            let sub = |n: usize| -> String {
                n.to_string().chars().map(|ch| char::from_u32('₀' as u32 + ch.to_digit(10).unwrap()).unwrap()).collect()
            };
            crate::eval_calc::warn_text(&format!("levels {n} and {} are nearly degenerate (ΔE/E = {}); their eigenfunctions ψ{}, ψ{} \
                                       can be any mixture of the two: use them only through combinations, or break the \
                                       symmetry", n + 1, fermium_units::format_number(rel, 2, true), sub(n), sub(n + 1)));
        }
        let ns = nstates;
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
        let (interp_h, sol) = mirror(c, solv, None);
        c.sols.push(CSol { interp_h, sol, rhs: None, grid: None, check: None });
        c.sols.len() as i64 - 1
    })
}

/// A PDE (eval_solve solve_pde): the right side at (x, [u, u_x, u_xx, …]) into 6 slots.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn fm_pde(c: C, site: i64, f: OdeFn, env: *mut u8, t0: f64, t1: f64, step: f64, xa: f64, xb: f64,
                         line: i32) -> i64 {
    let line = line.max(0) as u32;
    let s = unsafe { &(&(*c).ode_sites)[site as usize] };
    let method = match s.pmethod {
        1 => nx::pde::PdeMethod::Implicit,
        2 => nx::pde::PdeMethod::Explicit,
        _ => nx::pde::PdeMethod::CrankNicolson,
    };
    let opts = nx::pde::PdeOpts { grid: s.grid, order: s.order, method, step: Some(step).filter(|v| v == v), bc: s.bc,
                                  is_complex: s.is_complex, tdep: s.tdep };
    let mut rhs = |xv: f64, st: &[f64; 6]| -> [f64; 6] {
        let mut out = [f64::NAN; 6];
        if stopped(c) {
            return out;
        }
        unsafe { f(xv, st.as_ptr(), out.as_mut_ptr(), 6, env) };
        if stopped(c) {
            return [f64::NAN; 6];
        }
        out
    };
    let (xname, tname, pline) = (s.xname, s.tname, s.pde_line);
    let r = nx::pde::pde_solve_checked(&mut rhs, xa, xb, t0, t1, opts);
    if stopped(c) {
        return -1;
    }
    locked(c, |c| {
        let r = match r {
            Ok(r) => r,
            Err(nx::pde::PdeFail(m)) => {
                c.fail(RunError { message: m, line, hint: None });
                return -1;
            }
        };
        for &(kind, est) in &r.fine.warnings {
            c.interp.rt_warn(kind, est, pline);
        }
        let check = r.coarse.as_ref().map(|co| PdeCheck { coarse: co.to_sol(), ranges: r.fine.ranges(), xname, tname,
                                                           warned: std::cell::Cell::new(false) });
        let solv = r.fine.to_sol();
        let (interp_h, sol) = mirror(c, solv, Some((xa, xb)));
        c.sols.push(CSol { interp_h, sol, rhs: None, grid: Some((xa, xb)), check });
        c.sols.len() as i64 - 1
    })
}

/// The tree-walker's copy of a solution (plots and the other constructs it runs read it; they don't ask for
/// x'(t) or a PDE's grid check, which only the compiled code answers).
fn mirror(c: &mut Ctx, sol: ode::Sol, grid: Option<(f64, f64)>) -> (usize, std::rc::Rc<crate::eval_solve::SolData>) {
    let d = std::rc::Rc::new(crate::eval_solve::SolData { sol, rhs: None, grid, check: None });
    c.interp.solve.sols.push(d.clone());
    (c.interp.solve.sols.len() - 1, d)
}

fn tname_of(module: &Module, i: f64) -> String {
    let i = if i == i { i as i64 } else { -1 };
    if i >= 0 && (i as usize) < module.tables.texts.len() {
        module.tables.texts[i as usize].clone()
    } else {
        "t".into()
    }
}

/// Component comp (or its derivative) of solution h at t (eval_solve sol_at).
fn sol_at(c: C, h: i64, comp: usize, t: f64, use_dy: bool) -> Result<f64, Fail> {
    let d = unsafe { &(&(*c).sols)[h as usize] };
    match (&d.rhs, use_dy) {
        (Some((f, snap)), true) => {
            let env = snap.ptrs.as_ptr() as *mut u8;
            let mut rhs = |t: f64, y: &[f64], out: &mut [f64]| {
                unsafe { f(t, y.as_ptr(), out.as_mut_ptr(), out.len() as i64, env) };
                if stopped(c) {
                    out.fill(f64::NAN);
                }
            };
            d.sol.sol.eval(comp, t, true, Some(&mut rhs))
        }
        _ => d.sol.sol.eval(comp, t, use_dy, None),
    }
}

#[no_mangle]
pub extern "C" fn fm_sol_eval(c: C, h: i64, comp: i64, t: f64, use_dy: i32, tfmt: i64, line: i32) -> f64 {
    let r = sol_at(c, h, comp as usize, t, use_dy != 0);
    if stopped(c) {
        return f64::NAN;
    }
    match r {
        Ok(v) => v,
        Err(fl) => {
            locked(c, |c| fail(c, fl, tfmt as usize, line.max(0) as u32));
            f64::NAN
        }
    }
}

/// The same at every time of a list (a list).
#[no_mangle]
pub extern "C" fn fm_sol_eval_list(c: C, h: i64, comp: i64, ts: *const FmList, use_dy: i32, tfmt: i64, line: i32)
                                   -> *mut FmList {
    let ts: Vec<f64> = unsafe { (*ts).as_slice().to_vec() };
    let mut out = Vec::with_capacity(ts.len());
    for t in ts {
        let v = fm_sol_eval(c, h, comp, t, use_dy, tfmt, line);
        if stopped(c) {
            return std::ptr::null_mut();
        }
        out.push(v);
    }
    fm_list_from(c, out)
}

/// All samples of a component (what 0), the times (1) or the derivatives (2).
#[no_mangle]
pub extern "C" fn fm_sol_list(c: C, h: i64, comp: i64, what: i64) -> *mut FmList {
    let s = unsafe { &(&(*c).sols)[h as usize].sol.sol };
    let comp = comp as usize;
    let out: Vec<f64> = match what {
        1 => s.t.clone(),
        0 => (0..s.n()).map(|i| s.y[i * s.dim + comp]).collect(),
        _ => (0..s.n()).map(|i| s.dy[i * s.dim + comp]).collect(),
    };
    fm_list_from(c, out)
}

/// max(x) / min(x) of a solution component (v1's fm_sol_ext).
#[no_mangle]
pub extern "C" fn fm_sol_extreme(c: C, h: i64, comp: i64, sg: f64) -> f64 {
    unsafe { (&(*c).sols)[h as usize].sol.sol.extreme(comp as usize, sg) }
}

/// u(x, t) of a PDE solution and its grid check (eval_solve PdeEval).
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn fm_pde_eval(c: C, h: i64, m: i64, comp0: i64, xv: f64, tv: f64, which: i64, xfmt: i64, tfmt: i64,
                              line: i32) -> f64 {
    let line = line.max(0) as u32;
    let (m, comp0) = (m as usize, comp0 as usize);
    let d = unsafe { &(&(*c).sols)[h as usize] };
    let (xa, xb) = d.grid.unwrap_or((0.0, 1.0));
    let slack = 1e-9 * (xb - xa).abs();
    if xv < xa - slack || xv > xb + slack || xv != xv {
        let end = if xv < xa { xa } else { xb };
        locked(c, |c| fail(c, Fail::new(E::SOLRANGE, xv, end), xfmt as usize, line));
        return f64::NAN;
    }
    let v = match nx::pde::pde_eval(&d.sol.sol, xa, xb, m, comp0, xv, tv, which as u8) {
        Ok(v) => v,
        Err(fl) => {
            locked(c, |c| fail(c, fl, tfmt as usize, line));
            return f64::NAN;
        }
    };
    if let (Some(ck), 0) = (&d.check, which) {
        let cc = comp0 / (m + 1);
        let est = nx::pde::grid_error(&d.sol.sol, &ck.coarse, m, xa, xb, cc, ck.ranges[cc], xv, tv);
        if est.is_some_and(|q| q > nx::pde::PDE_TOL) && !ck.warned.get() {
            ck.warned.set(true);
            let module = unsafe { (*c).module };
            let text = |i: usize| module.tables.texts.get(i).cloned().unwrap_or_default();
            let msg = format!("the grid is too coarse for this PDE at {} = {}, {} = {}: the value there \
                               changes by {}% of the solution's range when the grid is made half as fine \
                               (a sharp front, a short wavelength, or the time right after a jump needs more \
                               grid points); raise  grid  (it is {m})",
                              text(ck.xname), crate::eval_solve::fmt_value(module, xv, xfmt as usize),
                              text(ck.tname), crate::eval_solve::fmt_value(module, tv, tfmt as usize),
                              fermium_units::format_number(3.0 * est.unwrap() * 100.0, 2, true));
            crate::eval_calc::warn_at(line, &msg);
        }
    }
    v
}

/// m·x = b for the highest derivatives of a coupled ODE (eval_solve OdeLinSolve): 1 if singular.
#[no_mangle]
pub extern "C" fn fm_odelin(n: i64, a: *const f64, b: *const f64, out: *mut f64) -> i32 {
    let n = n as usize;
    let a = unsafe { std::slice::from_raw_parts(a, n * n) };
    let b = unsafe { std::slice::from_raw_parts(b, n) };
    let (xs, piv) = solve_linear(a, n, b);
    for (i, x) in xs.iter().enumerate() {
        unsafe { *out.add(i) = *x };
    }
    i32::from(piv.iter().any(|p| *p == 0.0))
}

/// A solver error at a line with a print format (describe_error).
#[no_mangle]
pub extern "C" fn fm_solve_error(c: C, kind: i64, a: f64, b: f64, fmt: i64, line: i32) {
    locked(c, |c| fail(c, Fail::new(kind, a, b), fmt as usize, line.max(0) as u32));
}

/// eval_solve's solve_linear: Gaussian elimination with branch-free partial pivoting.
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
