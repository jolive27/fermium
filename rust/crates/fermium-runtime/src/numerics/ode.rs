//! Explicit ODE solvers: v1's fixed-step RK4 (with the step-doubling check) and adaptive
//! Dormand–Prince 5(4) (Hairer–Wanner first step, D17's scale-free error norm, jump location D40,
//! Hairer's stiffness detection, `until` events located on DOPRI5's dense output, D39), ported
//! operation for operation from `fermium/interp.py` (which mirrors the compiled kernels
//! `fm_rk4`, `fm_dp45`, `fm_rk4_check` in `fermium/codegen_llvm.py`), and the stored solution
//! (t, y, y' at every step, cubic Hermite in between) with `max`/`min` of a component (`fm_sol_ext`).
//!
//! The right-hand side is `f(t, y, out)`: it writes y'(t) into `out`.

use super::{err, pymax, pymin, Fail};

/// RK45 steps before "too many steps" (the compiled kernel has the same limit)
pub const ODE_MAX_STEPS: u64 = 20_000_000;
/// step-doubling checks after a fixed-step solve (redteam #5)
pub const RK4_SAMPLES: usize = 8;
/// warn (kind 7) when the estimated relative error of a fixed-step solve is larger than this
pub const RK4_WARN: f64 = 1e-3;
/// Hairer's stiffness test starts after this many RK45 steps
pub const STIFF_AFTER: u64 = 100_000;

/// v1's warning kinds (`Runtime.warn` in fermium/runtime/core.py) raised by the solvers
pub mod warn {
    /// "this equation looks stiff: rk45 has taken N steps ..." (a = the step count)
    pub const STIFF: i64 = 2;
    /// "the step is too coarse for this equation ..." (a = the estimated relative error)
    pub const RK4_COARSE: i64 = 7;
}

/// A solved ODE: the accepted steps' times, and the state and its derivative there (flattened row
/// by row), as v1's `Sol` / `fm_sol`.
#[derive(Debug, Clone, Default)]
pub struct Sol {
    pub dim: usize,
    pub t: Vec<f64>,
    pub y: Vec<f64>,
    pub dy: Vec<f64>,
    /// run-time warnings (kind, a) raised while solving (v1's `rt.warn(kind, a, line)`)
    pub warnings: Vec<(i64, f64)>,
}

impl Sol {
    pub fn new(dim: usize) -> Self {
        Sol { dim, ..Default::default() }
    }

    pub fn n(&self) -> usize {
        self.t.len()
    }

    pub fn push(&mut self, t: f64, y: &[f64], dy: &[f64]) {
        self.t.push(t);
        self.y.extend_from_slice(y);
        self.dy.extend_from_slice(dy);
    }

    /// Component `comp` (or its derivative) at time t: v1's `Sol.eval` (cubic Hermite between
    /// steps). With `rhs` (D46), x'(t) is the right side at the interpolated state.
    pub fn eval(
        &self,
        comp: usize,
        t: f64,
        use_dy: bool,
        rhs: Option<&mut dyn FnMut(f64, &[f64], &mut [f64])>,
    ) -> Result<f64, Fail> {
        let (n, dim) = (self.n(), self.dim);
        let (tfirst, tlast) = (self.t[0], self.t[n - 1]);
        let slack = 1e-9 * (tlast - tfirst).abs();
        let (lo_t, hi_t) = (pymin(tfirst, tlast), pymax(tfirst, tlast));
        if t < lo_t - slack || t > hi_t + slack || t != t {
            return Err(Fail::new(err::SOLRANGE, t, tlast));
        }
        // times increase, or decrease for a solve towards smaller t (D39)
        let sg = if tlast >= tfirst { 1.0 } else { -1.0 };
        let (mut lo, mut hi) = (0usize, n - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.t[mid] * sg <= t * sg {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let i = lo;
        if n <= 1 {
            return Ok(if use_dy { self.dy[comp] } else { self.y[comp] });
        }
        let (ta, tb) = (self.t[i], self.t[i + 1]);
        let h = tb - ta;
        let s = (t - ta) / h;
        let s2 = s * s;
        let s3 = s2 * s;
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        let herm = |c: usize| -> (f64, f64, f64, f64, f64) {
            let (ia, ib) = (i * dim + c, (i + 1) * dim + c);
            let (ya, yb) = (self.y[ia], self.y[ib]);
            let (ma, mb) = (self.dy[ia] * h, self.dy[ib] * h);
            (ya, yb, ma, mb, h00 * ya + h10 * ma + (h01 * yb + h11 * mb))
        };
        if use_dy {
            if let Some(rhs) = rhs {
                let state: Vec<f64> = (0..dim).map(|c| herm(c).4).collect();
                let mut out = vec![0.0; dim];
                rhs(t, &state, &mut out);
                return Ok(out[comp]);
            }
        }
        let (ya, yb, ma, mb, r) = herm(comp);
        if use_dy {
            let d00 = 6.0 * s2 - 6.0 * s;
            let d10 = 3.0 * s2 - 4.0 * s + 1.0;
            let d01 = 6.0 * s - 6.0 * s2;
            let d11 = 3.0 * s2 - 2.0 * s;
            return Ok(((d00 * ya + d10 * ma) + (d01 * yb + d11 * mb)) / h);
        }
        Ok(r)
    }

    /// The largest (sg = 1) or smallest (sg = −1) value of component `comp` over the solution:
    /// v1's `sol_ext` / `fm_sol_ext` (the quintic Hermite through the best step and its neighbours,
    /// maximised by golden-section search).
    pub fn extreme(&self, comp: usize, sg: f64) -> f64 {
        let (n, dim) = (self.n(), self.dim);
        let ys: Vec<f64> = (0..n).map(|i| sg * self.y[i * dim + comp]).collect();
        let mut k = 0;
        for i in 1..n {
            if ys[i] > ys[k] {
                k = i;
            }
        }
        let best = ys[k];
        if n < 3 {
            return sg * best;
        }
        let k = k.clamp(1, n - 2);
        let ks = [k - 1, k, k + 1];
        let t = [self.t[ks[0]], self.t[ks[1]], self.t[ks[2]]];
        let yv = [ys[ks[0]], ys[ks[1]], ys[ks[2]]];
        let dv = [sg * self.dy[ks[0] * dim + comp], sg * self.dy[ks[1] * dim + comp], sg * self.dy[ks[2] * dim + comp]];
        let coef = quintic_hermite(t, yv, dv);
        let z = [t[0], t[0], t[1], t[1], t[2], t[2]];
        let poly = |x: f64| -> f64 {
            let mut acc = coef[5];
            for j in (0..5).rev() {
                acc = coef[j] + (x - z[j]) * acc;
            }
            acc
        };
        let (mut lo, mut hi) = (t[0], t[2]);
        let g = (5f64.sqrt() - 1.0) / 2.0;
        for _ in 0..80 {
            let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if poly(x1) > poly(x2) {
                hi = x2;
            } else {
                lo = x1;
            }
        }
        let pm = poly(0.5 * (lo + hi));
        sg * if pm > best || best != best { pm } else { best }
    }
}

#[inline]
fn vdiv(x: f64, y: f64) -> f64 {
    // v1's PyOps.div: x / y if y else NaN
    if y != 0.0 { x / y } else { f64::NAN }
}

/// Newton coefficients of the quintic with values y and slopes d at t0 < t1 < t2 (nodes t0, t0,
/// t1, t1, t2, t2): `fermium/numerics.py:quintic_hermite`.
pub fn quintic_hermite(t: [f64; 3], y: [f64; 3], d: [f64; 3]) -> [f64; 6] {
    let z = [t[0], t[0], t[1], t[1], t[2], t[2]];
    let mut col: Vec<f64> = vec![d[0], vdiv(y[1] - y[0], t[1] - t[0]), d[1], vdiv(y[2] - y[1], t[2] - t[1]), d[2]];
    let mut coef = [0.0; 6];
    coef[0] = y[0];
    coef[1] = col[0];
    for order in 2..6 {
        col = (0..col.len() - 1).map(|i| vdiv(col[i + 1] - col[i], z[i + order] - z[i])).collect();
        coef[order] = col[0];
    }
    coef
}

// ------------------------------------------------------------------------------ shared pieces

/// Dormand–Prince's dense output (Hairer, Nørsett & Wanner, DOPRI5): stage weights of the
/// 4th-order term
const D_DP: [f64; 7] = [
    -12715105075.0 / 11282082432.0,
    0.0,
    87487479700.0 / 32700410799.0,
    -10690763975.0 / 1880347072.0,
    701980252875.0 / 199316789632.0,
    -1453857185.0 / 822651844.0,
    69997945.0 / 29380423.0,
];

/// A step's dense output at the fraction th: the cubic Hermite through (ya, da), (yb, db) plus
/// the 4th-order term r5 (0 for the Hermite alone). v1's `_dense`.
#[inline]
fn dense(ya: f64, yb: f64, da: f64, db: f64, r5: f64, h: f64, th: f64) -> f64 {
    let r2 = yb - ya;
    let r3 = h * da - r2;
    let r4 = (r2 - h * db) - r3;
    let th1 = 1.0 - th;
    ya + th * (r2 + th1 * (r3 + th * (r4 + th1 * r5)))
}

/// True if every number in v is finite.
pub(crate) fn all_finite(v: &[f64]) -> bool {
    v.iter().all(|x| x - x == 0.0)
}

fn start<F: FnMut(f64, &[f64], &mut [f64])>(f: &mut F, t0: f64, y0: &[f64], tname: f64) -> Result<Vec<f64>, Fail> {
    let mut k0 = vec![0.0; y0.len()];
    f(t0, y0, &mut k0);
    if !all_finite(&k0) {
        return Err(Fail::new(err::ODE_NAN, t0, tname));
    }
    Ok(k0)
}

/// Which "the step became too small" error to give (D160): ERR_ODE_H when some component has
/// grown to over 10³ times both its start and the largest starting component (or isn't finite),
/// else ERR_ODE_H_FLAT. `fermium/runtime/stiff.py:step_small_kind`.
pub fn step_small_kind(y0: &[f64], y: &[f64]) -> i64 {
    let mut big = 0.0f64;
    for &v in y0 {
        big = pymax(big, v.abs());
    }
    for (&a, &v) in y0.iter().zip(y) {
        if !(v - v == 0.0) || v.abs() > 1e3 * pymax(a.abs(), big) {
            return err::ODE_H;
        }
    }
    err::ODE_H_FLAT
}

/// The absolute tolerance of each state component (D160) from the checker's (value, k) pairs:
/// the value divided k times by |t1 − t0|. `fermium/runtime/stiff.py:abs_tolerances`.
pub fn abs_tolerances(spec: &[(f64, u32)], t0: f64, t1: f64) -> Option<Vec<f64>> {
    if spec.is_empty() {
        return None;
    }
    let span = (t1 - t0).abs();
    Some(
        spec.iter()
            .map(|&(v, k)| {
                let mut v = v;
                for _ in 0..k {
                    v /= span;
                }
                v
            })
            .collect(),
    )
}

/// A sign change of g between a and c (ga, gc of opposite signs): Illinois to full precision.
/// v1's `_illinois` (and the loop in `_k_event_locate`).
pub fn illinois<G: FnMut(f64) -> f64>(mut g: G, mut a: f64, mut ga: f64, mut c: f64, mut gc: f64) -> f64 {
    let mut side = 0;
    for _ in 0..200 {
        if (c - a).abs() <= 4e-16 * pymax(a.abs(), c.abs()) {
            break;
        }
        let mut x = c - gc * (c - a) / (gc - ga);
        if !(pymin(a, c) < x && x < pymax(a, c)) {
            x = 0.5 * (a + c);
        }
        let gx = g(x);
        if gx == 0.0 || gx != gx {
            return x;
        }
        if gx * gc < 0.0 {
            a = c;
            ga = gc;
            side = 0;
        } else {
            if side == 1 {
                ga *= 0.5;
            }
            side = 1;
        }
        c = x;
        gc = gx;
    }
    c
}

/// The stop condition `until lhs = rhs` (D39): g = lhs − rhs changes sign.
pub type EventFn<'a> = &'a mut dyn FnMut(f64, &[f64]) -> f64;

struct Event<'a> {
    g: EventFn<'a>,
    sgn: f64,
}

impl<'a> Event<'a> {
    fn new(g: EventFn<'a>, t0: f64, y0: &[f64]) -> Self {
        let g0 = g(t0, y0);
        let sgn = if g0 > 0.0 { 1.0 } else if g0 < 0.0 { -1.0 } else { 0.0 };
        Event { g, sgn }
    }

    /// After a step from (t, y, k) to (tn, yn, kn): if g crossed zero, end the solution at the
    /// crossing and return true. Located on DOPRI5's continuous extension when the stages `ks`
    /// are given, else on the cubic Hermite. v1's `_Event.check`.
    #[allow(clippy::too_many_arguments)]
    fn check<F: FnMut(f64, &[f64], &mut [f64])>(
        &mut self,
        f: &mut F,
        sol: &mut Sol,
        t: f64,
        y: &[f64],
        k: &[f64],
        tn: f64,
        yn: &[f64],
        kn: &[f64],
        ks: Option<&[Vec<f64>; 7]>,
    ) -> bool {
        let gn = (self.g)(tn, yn);
        if gn != gn {
            return false;
        }
        if self.sgn == 0.0 {
            self.sgn = if gn > 0.0 { 1.0 } else if gn < 0.0 { -1.0 } else { 0.0 };
            return false;
        }
        if !(gn == 0.0 || gn * self.sgn < 0.0) {
            return false;
        }
        let n = y.len();
        let h = tn - t;
        let mut r5 = vec![0.0; n];
        if let Some(ks) = ks {
            for j in 0..n {
                let mut acc = D_DP[0] * ks[0][j];
                for m in 2..7 {
                    acc += D_DP[m] * ks[m][j];
                }
                r5[j] = h * acc;
            }
        }
        let state = |x: f64| -> Vec<f64> {
            let th = (x - t) / h;
            (0..n).map(|j| dense(y[j], yn[j], k[j], kn[j], r5[j], h, th)).collect()
        };
        let mut te = tn;
        if gn != 0.0 {
            let g = &mut self.g;
            te = illinois(|x| g(x, &state(x)), t, self.sgn, tn, gn);
        }
        let ye = if te == tn { yn.to_vec() } else { state(te) };
        let mut ke = vec![0.0; n];
        f(te, &ye, &mut ke);
        sol.push(te, &ye, &ke);
        true
    }
}

/// Options shared by the solvers.
#[derive(Debug, Clone, Copy)]
pub struct OdeOpts<'a> {
    /// relative tolerance (D17)
    pub rtol: f64,
    /// absolute tolerance per component (D160), or None
    pub atol: Option<&'a [f64]>,
    /// the text id of the independent variable's name, for errors (−1: "t")
    pub tname: f64,
    /// the text id of the `until` message, for ERR_NO_EVENT
    pub evtext: f64,
    /// the right side reads t in a condition: probe for jumps (D40)
    pub tdep: bool,
}

impl Default for OdeOpts<'_> {
    fn default() -> Self {
        OdeOpts { rtol: 1e-6, atol: None, tname: -1.0, evtext: -1.0, tdep: false }
    }
}

// ------------------------------------------------------------------------------ RK4

/// Fixed-step classical RK4 from t0 to t1 with step about h0 (the range gives the direction,
/// the step its size): v1's `rk4`. Adds the step-doubling warning (kind 7) like v1's caller.
pub fn rk4<F: FnMut(f64, &[f64], &mut [f64])>(
    mut f: F,
    y0: &[f64],
    t0: f64,
    t1: f64,
    h0: f64,
    ev: Option<EventFn<'_>>,
    opts: OdeOpts<'_>,
) -> Result<Sol, Fail> {
    let mut sol = rk4_plain(&mut f, y0, t0, t1, h0, ev, opts)?;
    let est = rk4_error(&mut f, &sol);
    if est > RK4_WARN {
        sol.warnings.push((warn::RK4_COARSE, est));
    }
    Ok(sol)
}

fn rk4_plain<F: FnMut(f64, &[f64], &mut [f64])>(
    f: &mut F,
    y0: &[f64],
    t0: f64,
    t1: f64,
    h0: f64,
    ev: Option<EventFn<'_>>,
    opts: OdeOpts<'_>,
) -> Result<Sol, Fail> {
    let span = t1 - t0;
    if !(span != 0.0) {
        return Err(Fail::new(err::ODE_RANGE, t0, opts.tname));
    }
    let ratio = (span / h0).abs();
    if ratio != ratio || ratio <= 0.0 || ratio > 1e12 {
        return Err(Fail::new(err::STEP, h0, span));
    }
    let steps = ((ratio - 1e-9).ceil() as u64).max(1);
    let h = span / steps as f64;
    let n = y0.len();
    let mut sol = Sol::new(n);
    let mut y = y0.to_vec();
    let half = h * 0.5;
    let mut k1 = start(f, t0, &y, opts.tname)?;
    let mut event = ev.map(|g| Event::new(g, t0, &y));
    let (mut k2, mut k3, mut k4, mut kn) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut tmp = vec![0.0; n];
    let mut yn = vec![0.0; n];
    for s in 0..steps {
        let t = t0 + s as f64 * h;
        sol.push(t, &y, &k1);
        for k in 0..n {
            tmp[k] = y[k] + half * k1[k];
        }
        let th = t + half;
        f(th, &tmp, &mut k2);
        for k in 0..n {
            tmp[k] = y[k] + half * k2[k];
        }
        f(th, &tmp, &mut k3);
        for k in 0..n {
            tmp[k] = y[k] + h * k3[k];
        }
        f(t + h, &tmp, &mut k4);
        let h6 = h / 6.0;
        for k in 0..n {
            yn[k] = y[k] + h6 * (k1[k] + 2.0 * (k2[k] + k3[k]) + k4[k]);
        }
        let tn = if s == steps - 1 { t1 } else { t0 + (s + 1) as f64 * h };
        f(tn, &yn, &mut kn);
        if let Some(e) = event.as_mut() {
            if e.check(f, &mut sol, t, &y, &k1, tn, &yn, &kn, None) {
                return Ok(sol);
            }
        }
        std::mem::swap(&mut y, &mut yn);
        std::mem::swap(&mut k1, &mut kn);
    }
    if event.is_some() {
        return Err(Fail::new(err::NO_EVENT, t1, opts.evtext));
    }
    sol.push(t1, &y, &k1);
    Ok(sol)
}

/// v1's `rk4_error` (`fm_rk4_check`): a cheap global error estimate of a fixed-step RK4 solution by
/// step doubling at RK4_SAMPLES evenly spaced steps (relative to D17's scale-free norm).
pub fn rk4_error<F: FnMut(f64, &[f64], &mut [f64])>(f: &mut F, sol: &Sol) -> f64 {
    let (nn, n) = (sol.n(), sol.dim);
    if nn < 3 {
        return 0.0;
    }
    let mut worst = 0.0f64;
    let mut last: i64 = -1;
    let (mut k2, mut k3, mut k4, mut tmp) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for k in 0..RK4_SAMPLES {
        let s = (k * (nn - 3)) / (RK4_SAMPLES - 1);
        if s as i64 == last {
            continue;
        }
        last = s as i64;
        let (t, tm, t2) = (sol.t[s], sol.t[s + 1], sol.t[s + 2]);
        let h = tm - t;
        if !(((t2 - tm) - h).abs() <= 1e-9 * h.abs()) {
            continue; // an event cut the last step short
        }
        let hh = t2 - t;
        let half = hh * 0.5;
        let y = &sol.y[s * n..(s + 1) * n];
        let k1 = &sol.dy[s * n..(s + 1) * n];
        for j in 0..n {
            tmp[j] = y[j] + half * k1[j];
        }
        f(t + half, &tmp, &mut k2);
        for j in 0..n {
            tmp[j] = y[j] + half * k2[j];
        }
        f(t + half, &tmp, &mut k3);
        for j in 0..n {
            tmp[j] = y[j] + hh * k3[j];
        }
        f(t + hh, &tmp, &mut k4);
        let h6 = hh / 6.0;
        for j in 0..n {
            let y2 = y[j] + h6 * (k1[j] + 2.0 * (k2[j] + k3[j]) + k4[j]);
            let yr = sol.y[(s + 2) * n + j];
            let sc = pymax(y[j].abs(), yr.abs()) + (yr - y[j]).abs();
            if sc > 0.0 {
                let e = (y2 - yr).abs() / sc;
                if e > worst {
                    worst = e;
                }
            }
        }
    }
    worst / 30.0 * (nn - 1) as f64
}

// ------------------------------------------------------------------------------ Dormand–Prince

const A_DP: [[f64; 6]; 7] = [
    [0.0; 6],
    [1.0 / 5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0, 0.0],
    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0, 0.0],
    [19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0, 0.0, 0.0],
    [9017.0 / 3168.0, -355.0 / 33.0, 46732.0 / 5247.0, 49.0 / 176.0, -5103.0 / 18656.0, 0.0],
    [35.0 / 384.0, 0.0, 500.0 / 1113.0, 125.0 / 192.0, -2187.0 / 6784.0, 11.0 / 84.0],
];
const C_DP: [f64; 7] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0, 1.0];
const E_DP: [f64; 7] =
    [71.0 / 57600.0, 0.0, -71.0 / 16695.0, 71.0 / 1920.0, -17253.0 / 339200.0, 22.0 / 525.0, -1.0 / 40.0];

/// Hairer–Wanner first step: v1's `_first_step` (`_emit_first_step`, gauntlet A5).
fn first_step<F: FnMut(f64, &[f64], &mut [f64])>(
    f: &mut F,
    t0: f64,
    y: &[f64],
    k0: &[f64],
    dirn: f64,
    aspan: f64,
    rtol: f64,
) -> f64 {
    let (mut d0, mut d1, mut cnt) = (0.0f64, 0.0f64, 0.0f64);
    for (&yj, &fj) in y.iter().zip(k0) {
        if yj.abs() > 0.0 {
            let sc = rtol * yj.abs();
            d0 += (yj.abs() / sc).powi(2);
            d1 += (fj / sc).powi(2);
            cnt += 1.0;
        }
    }
    let mut hv = aspan * 1e-4;
    if cnt > 0.0 && 0.0 < d1 && d1 < f64::INFINITY {
        let h0 = pymin(0.01 * (d0 / d1).sqrt(), aspan);
        let yt: Vec<f64> = y.iter().zip(k0).map(|(&yj, &fj)| yj + (dirn * h0) * fj).collect();
        let mut k1 = vec![0.0; y.len()];
        f(t0 + dirn * h0, &yt, &mut k1);
        let mut d2 = 0.0f64;
        for ((&yj, &fj), &gj) in y.iter().zip(k0).zip(&k1) {
            if yj.abs() > 0.0 {
                d2 += ((gj - fj) / (rtol * yj.abs())).powi(2);
            }
        }
        let dd1 = (d1 / cnt).sqrt() * rtol;
        let dd2 = ((d2 / cnt).sqrt() / h0 * rtol).sqrt();
        let m = if dd2 == dd2 { pymax(dd1, dd2) } else { dd2 };
        let h1 = if m > 0.0 { rtol.powf(0.2) / m } else { 100.0 * h0 };
        let h = pymin(pymin(100.0 * h0, h1), aspan);
        if h == h {
            hv = h;
        }
    }
    hv
}

/// Distance between two right-hand-side values, each component relative to |fa| + |fe|.
fn jdist(u: &[f64], v: &[f64], fa: &[f64], fe: &[f64]) -> f64 {
    let mut d = 0.0;
    for j in 0..u.len() {
        let w = fa[j].abs() + fe[j].abs();
        if w > 0.0 {
            d += (u[j] - v[j]).abs() / w;
        }
    }
    d
}

/// Is there a jump in f(·, y) (y held fixed) between t and tn, like `if t < 0.3 s then ...`?
/// Adjacent times (lo, hi) on the near and far side of it, or None. v1's `_find_jump` (D40).
fn find_jump<F: FnMut(f64, &[f64], &mut [f64])>(f: &mut F, t: f64, tn: f64, y: &[f64], fa: &[f64]) -> Option<(f64, f64)> {
    let n = y.len();
    let mut fe = vec![0.0; n];
    let mut fm = vec![0.0; n];
    f(tn, y, &mut fe);
    f(t + 0.5 * (tn - t), y, &mut fm);
    let dfe = jdist(fa, &fe, fa, &fe);
    if !(dfe > 1e-12) {
        return None;
    }
    let mid: Vec<f64> = (0..n).map(|j| 0.5 * (fa[j] + fe[j])).collect();
    if !(jdist(&fm, &mid, fa, &fe) > 0.4 * dfe) {
        return None; // a jump puts f(midpoint) at one end
    }
    let (mut lo, mut hi) = (t, tn);
    let (mut flo, mut fhi) = (fa.to_vec(), fe.clone());
    let mut fx = vec![0.0; n];
    for _ in 0..200 {
        let m = lo + 0.5 * (hi - lo);
        if m == lo || m == hi {
            break;
        }
        f(m, y, &mut fx);
        if jdist(&fx, fa, fa, &fe) <= jdist(&fx, &fe, fa, &fe) {
            lo = m;
            flo.copy_from_slice(&fx);
        } else {
            hi = m;
            fhi.copy_from_slice(&fx);
        }
    }
    if jdist(&flo, &fhi, fa, &fe) > 0.5 * dfe {
        return Some((lo, hi)); // most of the change happens in one rounding step
    }
    None
}

/// Hairer's stiffness detection for DOPRI5 (v1's `_stiff_test`): true once 15 checks in a row
/// find h·|λ| ≈ |h|‖k7 − k6‖/‖y7 − y6‖ above 1.8 (state[0] becomes −1: warned).
fn stiff_test(state: &mut [i64; 2], count: u64, h: f64, k6: &[f64], k7: &[f64], y6: &[f64], y7: &[f64]) -> bool {
    if count < STIFF_AFTER || !(count % 1000 == 0 || state[0] > 0) {
        return false;
    }
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for j in 0..y7.len() {
        let dk = k7[j] - k6[j];
        let dy = y7[j] - y6[j];
        num += dk * dk;
        den += dy * dy;
    }
    if den > 0.0 && h * h * num > 1.8 * 1.8 * den {
        state[1] = 0;
        state[0] += 1;
        if state[0] >= 15 {
            state[0] = -1;
            return true;
        }
    } else {
        state[1] += 1;
        if state[1] >= 6 {
            state[0] = 0;
        }
    }
    false
}

/// Adaptive Dormand–Prince 5(4) from t0 to t1 (either direction): v1's `dp45` (`fm_dp45`).
/// A stiffness warning (kind 2, a = step count) is added to `Sol::warnings`.
pub fn dp45<F: FnMut(f64, &[f64], &mut [f64])>(
    mut f: F,
    y0: &[f64],
    t0: f64,
    t1: f64,
    ev: Option<EventFn<'_>>,
    opts: OdeOpts<'_>,
) -> Result<Sol, Fail> {
    let f = &mut f;
    let n = y0.len();
    let rtol = opts.rtol;
    let zeros = vec![0.0; n];
    let atol: &[f64] = opts.atol.unwrap_or(&zeros);
    let tname = opts.tname;
    let mut y = y0.to_vec();
    let mut sol = Sol::new(n);
    let span = t1 - t0;
    if !(span != 0.0) {
        return Err(Fail::new(err::ODE_RANGE, t0, tname));
    }
    let dirn = if span > 0.0 { 1.0 } else { -1.0 };
    let aspan = span.abs();
    let mut t = t0;
    let mut count: u64 = 0;
    let mut k: [Vec<f64>; 7] = std::array::from_fn(|_| vec![0.0; n]);
    k[0] = start(f, t0, &y, tname)?;
    let mut hv = first_step(f, t0, &y, &k[0], dirn, aspan, rtol);
    sol.push(t0, &y, &k[0]);
    let mut event = ev.map(|g| Event::new(g, t0, &y));
    let (mut rej, mut first_rej) = (0u32, 0.0f64);
    let (mut has_tgt, mut tgt_lo, mut tgt_hi) = (false, 0.0f64, 0.0f64);
    let mut probed = false;
    let mut stiff = [0i64, 0i64];
    let mut tmp = vec![0.0; n];
    let mut tmp6 = vec![0.0; n];
    let mut ynew = vec![0.0; n];
    loop {
        let remaining = dirn * (t1 - t);
        if !(remaining > 1e-14 * t1.abs() && remaining > 0.0) {
            break;
        }
        count += 1;
        if count > ODE_MAX_STEPS {
            return Err(Fail::new(err::ODE_STEPS_FROM + 1 + tname as i64, t, t0));
        }
        let stop = if has_tgt { tgt_lo } else { t1 };
        let rstop = dirn * (stop - t);
        let land = hv >= rstop;
        let h = if land { rstop } else { hv };
        if h < 1e-15 * (t.abs() + aspan) {
            return Err(Fail::new(step_small_kind(y0, &y), t, tname));
        }
        let tn = if land { stop } else { t + dirn * h };
        let hs = if land { stop - t } else { dirn * h };
        for s in 1..7 {
            for j in 0..n {
                let mut acc = y[j];
                for m in 0..s {
                    if A_DP[s][m] != 0.0 {
                        acc += (hs * A_DP[s][m]) * k[m][j];
                    }
                }
                tmp[j] = acc;
            }
            if s == 6 {
                ynew.copy_from_slice(&tmp);
            } else if s == 5 {
                tmp6.copy_from_slice(&tmp);
            }
            let ts = if C_DP[s] == 1.0 { tn } else { t + hs * C_DP[s] };
            let (_, rest) = k.split_at_mut(s);
            f(ts, &tmp, &mut rest[0]);
        }
        let mut errsum = 0.0f64;
        for j in 0..n {
            let mut e = 0.0f64;
            for m in 0..7 {
                if E_DP[m] != 0.0 {
                    e += E_DP[m] * k[m][j];
                }
            }
            e *= hs;
            let sc = rtol * (pymax(y[j].abs(), ynew[j].abs()) + (ynew[j] - y[j]).abs()) + atol[j] + 5e-324;
            let r = e / sc;
            errsum += r * r;
        }
        let errn = (errsum / n as f64).sqrt();
        let fac = 0.9 * pymax(errn, 1e-10).powf(-0.2);
        let fac = pymin(5.0, pymax(0.2, fac));
        let stalled = rej >= 4 && errn >= 0.5 * first_rej;
        if errn <= 1.0 || stalled {
            if stiff[0] >= 0 && stiff_test(&mut stiff, count, hs, &k[5], &k[6], &tmp6, &ynew) {
                sol.warnings.push((warn::STIFF, count as f64));
            }
            if let Some(e) = event.as_mut() {
                let (k0, k6) = (k[0].clone(), k[6].clone());
                if e.check(f, &mut sol, t, &y, &k0, tn, &ynew, &k6, Some(&k)) {
                    return Ok(sol);
                }
            }
            t = tn;
            y.copy_from_slice(&ynew);
            let (head, tail) = k.split_at_mut(6);
            head[0].copy_from_slice(&tail[0]);
            sol.push(t, &y, &k[0]);
            hv = if stalled { h * 2.0 } else { h * fac };
            if land && has_tgt {
                // at the jump: restart on its far side
                has_tgt = false;
                t = tgt_hi;
                f(t, &y, &mut k[0]);
                sol.push(t, &y, &k[0]);
                hv = h;
            }
            rej = 0;
            probed = false;
        } else {
            if rej == 0 {
                first_rej = errn;
            }
            rej += 1;
            hv = h * pymin(fac, 1.0);
            if opts.tdep && !probed && !has_tgt {
                probed = true;
                let k0 = k[0].clone();
                if let Some((lo, hi)) = find_jump(f, t, tn, &y, &k0) {
                    hv = h;
                    if lo == t {
                        // the jump is right at t: k[0] is from the near side
                        t = hi;
                        f(t, &y, &mut k[0]);
                        sol.push(t, &y, &k[0]);
                        rej = 0;
                        probed = false;
                    } else {
                        has_tgt = true;
                        tgt_lo = lo;
                        tgt_hi = hi;
                    }
                }
            }
        }
    }
    if event.is_some() {
        return Err(Fail::new(err::NO_EVENT, t1, opts.evtext));
    }
    Ok(sol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_dp45() {
        let sol = dp45(|_t, y, o| o[0] = -y[0], &[1.0], 0.0, 2.0, None, OdeOpts { rtol: 1e-10, ..Default::default() }).unwrap();
        let v = sol.eval(0, 2.0, false, None).unwrap();
        assert!((v - (-2f64).exp()).abs() < 1e-10);
        let v = sol.eval(0, 1.234, false, None).unwrap();
        assert!((v - (-1.234f64).exp()).abs() < 1e-7);
    }

    #[test]
    fn rk4_harmonic_and_event() {
        let mut g = |_t: f64, y: &[f64]| y[0];
        let sol = rk4(|_t, y, o| { o[0] = y[1]; o[1] = -y[0]; }, &[1.0, 0.0], 0.0, 10.0, 0.001, Some(&mut g), OdeOpts::default()).unwrap();
        let te = *sol.t.last().unwrap();
        assert!((te - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{te}");
    }
}
