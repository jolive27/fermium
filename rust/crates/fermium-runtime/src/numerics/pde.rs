//! 1-D partial differential equations on a grid (D83): a port of v1's `fermium/runtime/pde.py`.
//!
//! ```text
//! solve ∂u/∂t = D ∂²u/∂x²                       heat / diffusion (order 1)
//! solve ∂²u/∂t² = c² ∂²u/∂x²                    waves (order 2)
//! solve i ħ ∂ψ/∂t = -ħ²/(2m) ∂²ψ/∂x² + V ψ      Schrödinger (complex, order 1)
//! ```
//!
//! The checker's probe is `probe(x, [u, u_x, u_xx, t, u_t, i]) -> [rhs, u0, phase0, v0, left, right]`
//! (see pde.py). Methods: the θ-method (Crank–Nicolson, the default, with 4 SDIRK2 start-up steps on
//! real equations (D131); implicit; explicit) with step doubling (D130: the default step is halved
//! until the estimate is under 1e-3 of the solution's range, a given step that is too coarse warns
//! kind 8, the 32 000-step cap warns kind 9), and the leapfrog scheme for waves. SciPy's sparse LU
//! (`splu`) is replaced by a tridiagonal LU with partial pivoting (complex), so results agree with
//! v1 to rounding.
//!
//! [`pde_solve`] returns snapshots: rows of u at the M + 1 grid points (or [Re u…, Im u…]) and ∂u/∂t.

use super::dense::C64;

pub const MAX_SNAPSHOTS: usize = 1000;
pub const RANNACHER_STEPS: usize = 4;
pub const CHECKPOINTS: usize = 8;
pub const PDE_TOL: f64 = 1e-3;
pub const PDE_MAX_STEPS: usize = 32000;
const JUMP_LAYER: f64 = 0.0;
const RHS: usize = 0;
const U0: usize = 1;
const PHASE0: usize = 2;
const V0: usize = 3;
const LEFT: usize = 4;
const RIGHT: usize = 5;

/// A PDE failure with v1's message.
#[derive(Debug, Clone, PartialEq)]
pub struct PdeFail(pub String);

/// The method of a first-order equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdeMethod {
    CrankNicolson,
    Implicit,
    Explicit,
}

/// Options of `pde_solve` (v1's keyword arguments).
#[derive(Debug, Clone, Copy)]
pub struct PdeOpts {
    pub grid: usize,
    /// 1 (heat, Schrödinger) or 2 (waves)
    pub order: u8,
    pub method: PdeMethod,
    pub step: Option<f64>,
    /// boundary kinds (left, right): 0 Dirichlet u = g, 1 Neumann ∂u/∂x = g
    pub bc: (u8, u8),
    pub is_complex: bool,
    /// the source may depend on t
    pub tdep: bool,
}

impl Default for PdeOpts {
    fn default() -> Self {
        PdeOpts { grid: 400, order: 1, method: PdeMethod::CrankNicolson, step: None, bc: (0, 0), is_complex: false, tdep: false }
    }
}

/// The solution: snapshot times, rows of u and ∂u/∂t (flattened), components per point (1 or 2),
/// the number of grid intervals, and v1's warnings (kind 8 or 9, the estimated error).
#[derive(Debug, Clone)]
pub struct PdeResult {
    pub ts: Vec<f64>,
    pub ys: Vec<f64>,
    pub dys: Vec<f64>,
    pub ncomp: usize,
    pub m: usize,
    pub warnings: Vec<(i64, f64)>,
    /// a Dirichlet boundary value differs from the initial value at that end (D206): the grid check applies
    pub jump: bool,
}

type Probe<'a> = dyn FnMut(f64, &[f64; 6]) -> [f64; 6] + 'a;

/// Python's `format(x, ".{prec}g")`
pub fn fmt_g(x: f64, prec: usize) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() { "nan".into() } else if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let p = prec.max(1);
    let s = format!("{:.*e}", p - 1, x);
    let (m, ex) = s.split_once('e').unwrap();
    let e: i32 = ex.parse().unwrap();
    let trim = |t: &str| -> String {
        if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t.to_string() }
    };
    if e < -4 || e >= p as i32 {
        format!("{}e{}{:02}", trim(m), if e < 0 { '-' } else { '+' }, e.abs())
    } else {
        trim(&format!("{:.*}", (p as i32 - 1 - e).max(0) as usize, x))
    }
}

struct P<'a, 'b> {
    f: &'a mut Probe<'b>,
    cx: bool,
}

impl P<'_, '_> {
    fn val(&mut self, x: f64, u: f64, ux: f64, uxx: f64, t: f64, ut: f64) -> C64 {
        if !self.cx {
            return C64::new((self.f)(x, &[u, ux, uxx, t, ut, 0.0])[RHS], 0.0);
        }
        let g1 = (self.f)(x, &[u, ux, uxx, t, ut, 1.0])[RHS];
        let gm = (self.f)(x, &[u, ux, uxx, t, ut, -1.0])[RHS];
        let g2 = (self.f)(x, &[u, ux, uxx, t, ut, 2.0])[RHS];
        let r = 0.5 * (g1 + gm);
        let s = 0.5 * (g1 - gm);
        let k = (g2 - r - 0.5 * s) / 1.5;
        let g = s - k;
        C64::new(r, k - g)
    }

    fn coefficients(&mut self, xs: &[f64], t: f64) -> [Vec<C64>; 5] {
        let n = xs.len();
        let mut out: [Vec<C64>; 5] = std::array::from_fn(|_| Vec::with_capacity(n));
        for &x in xs {
            let s0 = self.val(x, 0.0, 0.0, 0.0, t, 0.0);
            out[4].push(s0);
            out[2].push(self.val(x, 1.0, 0.0, 0.0, t, 0.0) - s0);
            out[1].push(self.val(x, 0.0, 1.0, 0.0, t, 0.0) - s0);
            out[0].push(self.val(x, 0.0, 0.0, 1.0, t, 0.0) - s0);
            out[3].push(self.val(x, 0.0, 0.0, 0.0, t, 1.0) - s0);
        }
        out // A, B, C, D, S
    }

    fn source(&mut self, xs: &[f64], t: f64) -> Vec<C64> {
        xs.iter().map(|&x| self.val(x, 0.0, 0.0, 0.0, t, 0.0)).collect()
    }

    fn out(&mut self, x: f64, t: f64, k: usize) -> f64 {
        (self.f)(x, &[0.0, 0.0, 0.0, t, 0.0, 0.0])[k]
    }
}

fn check_finite(arrs: &[&[C64]], xs: &[f64], what: &str) -> Result<(), PdeFail> {
    for a in arrs {
        if let Some(j) = a.iter().position(|v| !v.is_finite()) {
            return Err(PdeFail(format!("{what} is NaN or infinite at x = {} (SI units)", fmt_g(xs[j], 6))));
        }
    }
    Ok(())
}

fn average_jumps(p: &mut P<'_, '_>, xs: &[f64], h: f64, t: f64, coefs: &mut [&mut Vec<C64>; 3]) {
    let n = xs.len();
    for (ci, arr) in coefs.iter_mut().enumerate() {
        let d: Vec<f64> = arr.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        let scale = arr.iter().fold(0.0f64, |m, v| m.max(v.abs())) + 1e-300;
        for j in 0..d.len() {
            let nb = { let a = if j > 0 { d[j - 1] } else { 0.0 }; let b = if j + 1 < d.len() { d[j + 1] } else { 0.0 }; if b > a { b } else { a } };
            if !(d[j] > 1e-9 * scale && d[j] > 50.0 * nb) {
                continue;
            }
            let (left, right) = (arr[j], arr[j + 1]);
            let (mut lo, mut hi) = (xs[j], xs[j + 1]);
            let probe = |p: &mut P<'_, '_>, x: f64| -> C64 {
                let base = p.val(x, 0.0, 0.0, 0.0, t, 0.0);
                match ci {
                    0 => p.val(x, 0.0, 0.0, 1.0, t, 0.0) - base,
                    1 => p.val(x, 0.0, 1.0, 0.0, t, 0.0) - base,
                    _ => p.val(x, 1.0, 0.0, 0.0, t, 0.0) - base,
                }
            };
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if mid <= lo || mid >= hi {
                    break;
                }
                let fm = probe(p, mid);
                let fm2 = probe(p, mid);
                if (fm - left).abs() <= (fm2 - right).abs() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let xj = 0.5 * (lo + hi);
            let i = (((xj - xs[0]) / h).round_ties_even() as i64).clamp(0, n as i64 - 1) as usize;
            let frac = ((xj - (xs[i] - 0.5 * h)) / h).clamp(0.0, 1.0);
            arr[i] = left.scale(frac) + right.scale(1.0 - frac);
        }
    }
}

/// A tridiagonal complex system (sub lo[i] = A[i][i-1], diag, sup up[i] = A[i][i+1]) factored by LU
/// with partial pivoting (as LAPACK's gttrf).
struct TriLu {
    dl: Vec<C64>,
    d: Vec<C64>,
    du: Vec<C64>,
    du2: Vec<C64>,
    swap: Vec<bool>,
}

impl TriLu {
    fn new(sub: &[C64], diag: &[C64], sup: &[C64]) -> Self {
        let n = diag.len();
        let mut dl: Vec<C64> = (0..n.saturating_sub(1)).map(|i| sub[i + 1]).collect();
        let mut d = diag.to_vec();
        let mut du: Vec<C64> = (0..n.saturating_sub(1)).map(|i| sup[i]).collect();
        let mut du2 = vec![C64::default(); n.saturating_sub(2)];
        let mut swap = vec![false; n];
        for i in 0..n.saturating_sub(1) {
            if d[i].abs1() >= dl[i].abs1() {
                if d[i] != C64::default() {
                    let fact = dl[i] / d[i];
                    dl[i] = fact;
                    d[i + 1] = d[i + 1] - fact * du[i];
                }
            } else {
                let fact = d[i] / dl[i];
                d[i] = dl[i];
                dl[i] = fact;
                let temp = du[i];
                du[i] = d[i + 1];
                d[i + 1] = temp - fact * d[i + 1];
                if i + 2 < n {
                    du2[i] = du[i + 1];
                    du[i + 1] = -(fact * du[i + 1]);
                }
                swap[i] = true;
            }
        }
        TriLu { dl, d, du, du2, swap }
    }

    fn solve(&self, b: &mut [C64]) {
        let n = self.d.len();
        for i in 0..n.saturating_sub(1) {
            if !self.swap[i] {
                let v = b[i];
                b[i + 1] = b[i + 1] - self.dl[i] * v;
            } else {
                let temp = b[i];
                b[i] = b[i + 1];
                b[i + 1] = temp - self.dl[i] * b[i];
            }
        }
        let mut i = n;
        while i > 0 {
            i -= 1;
            let mut s = b[i];
            if i + 1 < n {
                s = s - self.du[i] * b[i + 1];
            }
            if i + 2 < n {
                s = s - self.du2[i] * b[i + 2];
            }
            b[i] = s / self.d[i];
        }
    }
}

/// The θ-method / SDIRK2 / leapfrog machinery over a fixed problem.
struct Solver<'p, 'a, 'b> {
    p: &'p mut P<'a, 'b>,
    xs: Vec<f64>,
    m: usize,
    h: f64,
    xa: f64,
    xb: f64,
    t0: f64,
    span: f64,
    a: Vec<C64>,
    bco: Vec<C64>,
    s: Vec<C64>,
    lo: Vec<C64>,
    di: Vec<C64>,
    up: Vec<C64>,
    bc: (u8, u8),
    dirichlet: Vec<usize>,
    src_tdep: bool,
    theta: f64,
    is_complex: bool,
    u: Vec<C64>,
    lus: Vec<(u64, TriLu)>,
    skip_t: f64,
}

type Snapshots = (Vec<f64>, Vec<Vec<f64>>, Vec<Vec<f64>>);

impl Solver<'_, '_, '_> {
    fn bval(&mut self, t: f64) -> (f64, f64) {
        (self.p.out(self.xa, t, LEFT), self.p.out(self.xb, t, RIGHT))
    }

    fn bvec(&mut self, t: f64) -> Vec<C64> {
        let mut b = if self.src_tdep { let xs = self.xs.clone(); self.p.source(&xs, t) } else { self.s.clone() };
        let (gl, gr) = self.bval(t);
        let (m, h) = (self.m, self.h);
        if self.bc.0 == 1 {
            b[0] = b[0] + (self.a[0].scale(-2.0).scale(gl) / C64::new(h, 0.0)) + self.bco[0].scale(gl);
        }
        if self.bc.1 == 1 {
            b[m] = b[m] + (self.a[m].scale(2.0).scale(gr) / C64::new(h, 0.0)) + self.bco[m].scale(gr);
        }
        for &j in &self.dirichlet {
            b[j] = C64::default();
        }
        b
    }

    fn apply_t(&self, w: &[C64]) -> Vec<C64> {
        let n = w.len();
        let mut r: Vec<C64> = (0..n).map(|j| self.di[j] * w[j]).collect();
        for j in 1..n {
            r[j] = r[j] + self.lo[j] * w[j - 1];
        }
        for j in 0..n - 1 {
            r[j] = r[j] + self.up[j] * w[j + 1];
        }
        r
    }

    fn set_dirichlet(&mut self, w: &mut [C64], t: f64) {
        let (gl, gr) = self.bval(t);
        if self.bc.0 == 0 {
            w[0] = C64::new(gl, 0.0);
        }
        if self.bc.1 == 0 {
            w[self.m] = C64::new(gr, 0.0);
        }
    }

    fn lu(&mut self, tau: f64) -> usize {
        if let Some(i) = self.lus.iter().position(|(k, _)| *k == tau.to_bits()) {
            return i;
        }
        let n = self.m + 1;
        let one = C64::new(1.0, 0.0);
        let mut sub: Vec<C64> = (0..n).map(|j| -(self.lo[j].scale(tau))).collect();
        let mut diag: Vec<C64> = (0..n).map(|j| one - self.di[j].scale(tau)).collect();
        let mut sup: Vec<C64> = (0..n).map(|j| -(self.up[j].scale(tau))).collect();
        sub[0] = C64::default();
        sup[n - 1] = C64::default();
        for &j in &self.dirichlet {
            sub[j] = C64::default();
            sup[j] = C64::default();
            diag[j] = one;
        }
        self.lus.push((tau.to_bits(), TriLu::new(&sub, &diag, &sup)));
        self.lus.len() - 1
    }

    fn set_rhs_bc(&mut self, rhs: &mut [C64], t: f64) {
        let (gl, gr) = self.bval(t);
        if self.bc.0 == 0 {
            rhs[0] = C64::new(gl, 0.0);
        }
        if self.bc.1 == 0 {
            rhs[self.m] = C64::new(gr, 0.0);
        }
    }

    fn step(&mut self, w: &[C64], t_old: f64, dt: f64, th: f64, b_old: &[C64]) -> (Vec<C64>, Vec<C64>) {
        let t = t_old + dt;
        let b_new = self.bvec(t);
        let tw = self.apply_t(w);
        let c1 = (1.0 - th) * dt;
        let c2 = th * dt;
        let mut rhs: Vec<C64> = (0..w.len()).map(|j| (w[j] + (tw[j] + b_old[j]).scale(c1)) + b_new[j].scale(c2)).collect();
        self.set_rhs_bc(&mut rhs, t);
        if th > 0.0 {
            let i = self.lu(th * dt);
            self.lus[i].1.solve(&mut rhs);
        }
        (rhs, b_new)
    }

    fn sdirk2(&mut self, w: &[C64], t_old: f64, dt: f64, _b_old: &[C64]) -> (Vec<C64>, Vec<C64>) {
        let g2 = 1.0 - 1.0 / 2f64.sqrt();
        let tau = g2 * dt;
        let li = self.lu(tau);
        let b1 = self.bvec(t_old + tau);
        let mut r1: Vec<C64> = (0..w.len()).map(|j| w[j] + b1[j].scale(tau)).collect();
        self.set_rhs_bc(&mut r1, t_old + tau);
        self.lus[li].1.solve(&mut r1);
        let k1 = r1;
        let b2 = self.bvec(t_old + dt);
        let tk = self.apply_t(&k1);
        let c = (1.0 - g2) * dt;
        let mut r2: Vec<C64> = (0..w.len()).map(|j| (w[j] + (tk[j] + b1[j]).scale(c)) + b2[j].scale(tau)).collect();
        self.set_rhs_bc(&mut r2, t_old + dt);
        let li = self.lu(tau);
        self.lus[li].1.solve(&mut r2);
        (r2, b2)
    }

    fn ut_of(&mut self, w: &[C64], t: f64) -> Vec<C64> {
        let tw = self.apply_t(w);
        let b = self.bvec(t);
        let mut d: Vec<C64> = tw.iter().zip(&b).map(|(p, q)| *p + *q).collect();
        let dir = self.dirichlet.clone();
        for j in dir {
            let e = 1e-6 * self.span.abs().max(1e-300);
            let g1 = self.bval(t + e);
            let g2 = self.bval(t - e);
            let v = (if j == 0 { g1.0 - g2.0 } else { g1.1 - g2.1 }) / (2.0 * e);
            d[j] = C64::new(v, 0.0);
        }
        d
    }

    fn row(&self, w: &[C64]) -> Vec<f64> {
        if self.is_complex {
            w.iter().map(|v| v.re).chain(w.iter().map(|v| v.im)).collect()
        } else {
            w.iter().map(|v| v.re).collect()
        }
    }

    fn keep_into(&mut self, snaps: &mut Snapshots, t: f64, w: &[C64]) -> Result<(), PdeFail> {
        if w.iter().any(|v| !v.is_finite()) {
            return Err(PdeFail(format!(
                "the solution became NaN or infinite at t = {} s (SI): the scheme is unstable or the equation blows up",
                fmt_g(t, 6)
            )));
        }
        let dw = self.ut_of(w, t);
        snaps.0.push(t);
        snaps.1.push(self.row(w));
        snaps.2.push(self.row(&dw));
        Ok(())
    }

    fn every_of(n_steps: usize) -> usize {
        n_steps.div_ceil(MAX_SNAPSHOTS).max(1)
    }

    #[allow(clippy::type_complexity)]
    fn march(&mut self, n_steps: usize, record: bool, checks: &[usize]) -> Result<(Snapshots, Vec<(usize, Vec<C64>)>), PdeFail> {
        let dt = self.span / n_steps as f64;
        let smooth = if self.theta == 0.5 && !self.is_complex { RANNACHER_STEPS } else { 0 };
        let mut w = self.u.clone();
        let mut snaps: Snapshots = (Vec::new(), Vec::new(), Vec::new());
        let mut at = Vec::new();
        let t0 = self.t0;
        if record {
            self.keep_into(&mut snaps, t0, &w.clone())?;
        }
        let mut b_old = self.bvec(t0);
        let every = Self::every_of(n_steps);
        for n in 1..=n_steps {
            let t_old = t0 + (n - 1) as f64 * dt;
            let (nw, nb) = if n <= smooth { self.sdirk2(&w, t_old, dt, &b_old) } else { let th = self.theta; self.step(&w, t_old, dt, th, &b_old) };
            w = nw;
            b_old = nb;
            let t = t0 + n as f64 * dt;
            if checks.contains(&n) {
                at.push((n, w.clone()));
            }
            if record && (n % every == 0 || n == n_steps || n < CHECKPOINTS * every) {
                self.keep_into(&mut snaps, t, &w.clone())?;
            }
        }
        Ok((snaps, at))
    }

    fn checks(n_steps: usize) -> Vec<usize> {
        let mut s: Vec<usize> = Vec::new();
        for k in 1..=CHECKPOINTS {
            s.push(((k * n_steps) / CHECKPOINTS).max(1));
            s.push(((k * n_steps) / (CHECKPOINTS * CHECKPOINTS)).max(1));
        }
        for k in 1..=n_steps.min(RANNACHER_STEPS + 2) {
            s.push(k);
        }
        s.sort_unstable();
        s.dedup();
        s
    }

    fn scale(&mut self, rows: &[Vec<f64>]) -> f64 {
        let (gl, gr) = self.bval(self.t0);
        let mut hi = rows.iter().flat_map(|r| r.iter()).fold(f64::NEG_INFINITY, |m, &v| if v > m { v } else { m });
        let mut lo = rows.iter().flat_map(|r| r.iter()).fold(f64::INFINITY, |m, &v| if v < m { v } else { m });
        hi = [hi, gl, gr].into_iter().fold(f64::NEG_INFINITY, |m, v| if v > m { v } else { m });
        lo = [lo, gl, gr].into_iter().fold(f64::INFINITY, |m, v| if v < m { v } else { m });
        if hi - lo > 0.0 {
            return hi - lo;
        }
        let m = rows.iter().flat_map(|r| r.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
        m.max(gl.abs()).max(gr.abs())
    }

    fn estimate(&self, coarse: &[(usize, Vec<C64>)], fine: &[(usize, Vec<C64>)], coarse_n: usize, fine_n: usize, scale: f64) -> f64 {
        let mut worst = 0.0f64;
        for (k, wc) in coarse {
            if *k as f64 * (self.span / coarse_n as f64) < self.skip_t {
                continue;
            }
            let key = if fine_n == 2 * coarse_n { 2 * k } else { k / 2 };
            let wf = &fine.iter().find(|(kk, _)| *kk == key).expect("checkpoint").1;
            let d = wf.iter().zip(wc).fold(0.0f64, |m, (p, q)| m.max((*p - *q).abs()));
            worst = worst.max(d);
        }
        worst / scale
    }

    fn controlled(&mut self, n_steps: usize, step: Option<f64>, warnings: &mut Vec<(i64, f64)>) -> Result<Snapshots, PdeFail> {
        let p = if self.theta == 0.5 { 2 } else { 1 };
        let pf = (1u32 << p) as f64;
        if step.is_some() {
            let ck = Self::checks(n_steps);
            let (snaps, coarse) = self.march(n_steps, true, &ck)?;
            let ck2: Vec<usize> = ck.iter().map(|k| 2 * k).collect();
            let fine = self.march(2 * n_steps, false, &ck2)?.1;
            let scale = self.scale(&snaps.1);
            if scale > 0.0 {
                let est = self.estimate(&coarse, &fine, n_steps, 2 * n_steps, scale) * pf / (pf - 1.0);
                if est > PDE_TOL {
                    warnings.push((8, est));
                }
            }
            return Ok(snaps);
        }
        let mut half = (n_steps / 2).max(1);
        let mut coarse = self.march(half, false, &Self::checks(half))?.1;
        loop {
            let ckh = Self::checks(half);
            let mut want: Vec<usize> = ckh.iter().map(|k| 2 * k).chain(Self::checks(2 * half)).collect();
            want.sort_unstable();
            want.dedup();
            let (snaps, at) = self.march(2 * half, true, &want)?;
            let scale = self.scale(&snaps.1);
            let est = if scale > 0.0 { self.estimate(&coarse, &at, half, 2 * half, scale) / (pf - 1.0) } else { 0.0 };
            if est <= PDE_TOL {
                return Ok(snaps);
            }
            if 2 * half >= PDE_MAX_STEPS {
                warnings.push((9, est));
                return Ok(snaps);
            }
            half *= 2;
            let ck = Self::checks(half);
            coarse = at.into_iter().filter(|(k, _)| ck.contains(k)).collect();
        }
    }
}

/// v1's `pde_solve`.
pub fn pde_solve(probe: &mut Probe<'_>, xa: f64, xb: f64, t0: f64, t1: f64, opts: PdeOpts) -> Result<PdeResult, PdeFail> {
    if !(xb > xa) {
        return Err(PdeFail("the range of x is empty or reversed: write  for x from a to b  with a < b".into()));
    }
    if !(t1 > t0) {
        return Err(PdeFail("the range of t is empty or reversed: a PDE is solved forward in time, from t0 to t1 > t0".into()));
    }
    let m = opts.grid;
    let xs = super::eigen::linspace(xa, xb, m + 1);
    let h = (xb - xa) / m as f64;
    let is_complex = opts.is_complex;
    let mut p = P { f: probe, cx: is_complex };
    let [mut a, mut bco, mut cco, dco, s] = p.coefficients(&xs, t0);
    check_finite(&[&a, &bco, &cco, &dco, &s], &xs, "the equation")?;
    let stride = (m / 16).max(1);
    let xs_s: Vec<f64> = xs.iter().step_by(stride).copied().collect();
    let [a1, b1, c1, d1, s1] = p.coefficients(&xs_s, t1);
    for (name, c0, cc1) in [("∂²u/∂x²", &a, &a1), ("∂u/∂x", &bco, &b1), ("u", &cco, &c1), ("∂u/∂t", &dco, &d1)] {
        let c0s: Vec<C64> = c0.iter().step_by(stride).copied().collect();
        if c0s.iter().zip(cc1.iter()).any(|(x, y)| (*x - *y).abs() > 1e-9 * (x.abs() + y.abs()) + 1e-300) {
            return Err(PdeFail(format!(
                "the coefficient of {name} changes with t; only a source term (a part without u) may depend on t for now"
            )));
        }
    }
    let src_tdep = opts.tdep && s.iter().step_by(stride).zip(&s1).any(|(x, y)| (*x - *y).abs() > 1e-12 * (y.abs() + 1e-300));
    let mut j = 0;
    while j <= m {
        let x = xs[j];
        let want = cco[j].scale(2.0) + bco[j].scale(3.0) + a[j].scale(5.0) + dco[j].scale(7.0) + s[j];
        let got = p.val(x, 2.0, 3.0, 5.0, t0, 7.0);
        let tol = 1e-8 * (cco[j].scale(2.0).abs() + bco[j].scale(3.0).abs() + a[j].scale(5.0).abs() + dco[j].scale(7.0).abs() + s[j].abs()) + 1e-300;
        if (got - want).abs() > tol {
            return Err(PdeFail("a PDE must be linear in u: each term may have one factor u, ∂u/∂x or ∂²u/∂x² (no u², no products of them)".into()));
        }
        j += (m / 7).max(1);
    }
    if opts.order == 1 && dco.iter().any(|v| *v != C64::default()) {
        return Err(PdeFail("internal: ∂u/∂t on the right of a first-order equation".into()));
    }
    if !is_complex {
        for arr in [&a, &bco, &cco, &dco, &s] {
            if arr.iter().any(|v| v.im != 0.0) {
                return Err(PdeFail("internal: complex coefficients in a real equation".into()));
            }
        }
    }
    average_jumps(&mut p, &xs, h, t0, &mut [&mut a, &mut bco, &mut cco]);
    // initial values
    let u0: Vec<f64> = xs.iter().map(|&x| p.out(x, t0, U0)).collect();
    let ph: Vec<f64> = xs.iter().map(|&x| p.out(x, t0, PHASE0)).collect();
    let mut u: Vec<C64> = if is_complex {
        u0.iter().zip(&ph).map(|(&r, &q)| C64::new(q.cos(), q.sin()).scale(r)).collect()
    } else {
        u0.iter().map(|&r| C64::new(r, 0.0)).collect()
    };
    let v: Option<Vec<C64>> = if opts.order == 2 { Some(xs.iter().map(|&x| C64::new(p.out(x, t0, V0), 0.0)).collect()) } else { None };
    {
        let mut arrs: Vec<&[C64]> = vec![&u];
        if let Some(vv) = &v {
            arrs.push(vv);
        }
        check_finite(&arrs, &xs, "the initial value")?;
    }
    let h2 = h.powf(2.0);
    let n = m + 1;
    let mut lo = vec![C64::default(); n];
    let mut di = vec![C64::default(); n];
    let mut up = vec![C64::default(); n];
    for j in 1..m {
        lo[j] = a[j] / C64::new(h2, 0.0) - bco[j] / C64::new(2.0 * h, 0.0);
        di[j] = a[j].scale(-2.0) / C64::new(h2, 0.0) + cco[j];
        up[j] = a[j] / C64::new(h2, 0.0) + bco[j] / C64::new(2.0 * h, 0.0);
    }
    if opts.bc.0 == 1 {
        di[0] = a[0].scale(-2.0) / C64::new(h2, 0.0) + cco[0];
        up[0] = a[0].scale(2.0) / C64::new(h2, 0.0);
    }
    if opts.bc.1 == 1 {
        di[m] = a[m].scale(-2.0) / C64::new(h2, 0.0) + cco[m];
        lo[m] = a[m].scale(2.0) / C64::new(h2, 0.0);
    }
    let dirichlet: Vec<usize> = [(0usize, opts.bc.0), (m, opts.bc.1)].iter().filter(|(_, k)| *k == 0).map(|(j, _)| *j).collect();
    let amax = if m > 0 { a.iter().fold(0.0f64, |mx, v| mx.max(v.abs())) } else { 0.0 };
    let span = t1 - t0;
    let mut sv = Solver {
        p: &mut p,
        xs: xs.clone(),
        m,
        h,
        xa,
        xb,
        t0,
        span,
        a: a.clone(),
        bco: bco.clone(),
        s: s.clone(),
        lo,
        di,
        up,
        bc: opts.bc,
        dirichlet,
        src_tdep,
        theta: 0.5,
        is_complex,
        u: Vec::new(),
        lus: Vec::new(),
        skip_t: 0.0,
    };
    // a jump between the initial value and a Dirichlet boundary value (D206)
    let (gl0, gr0) = sv.bval(t0);
    let mut jump = false;
    for (jj, g) in [(0usize, gl0), (m, gr0)] {
        if (jj == 0 && opts.bc.0 == 0) || (jj == m && opts.bc.1 == 0) {
            let lvl = u.iter().fold(0.0f64, |mx, v| mx.max(v.abs())).max(g.abs()).max(1e-300);
            if (u[jj] - C64::new(g, 0.0)).abs() > 1e-9 * lvl {
                jump = true;
            }
        }
    }
    sv.set_dirichlet(&mut u, t0);
    sv.u = u.clone();
    sv.skip_t = if jump && amax > 0.0 { JUMP_LAYER * h * h / amax } else { 0.0 };
    let mut warnings = Vec::new();
    let nt;
    let dt;
    if opts.order == 1 {
        let theta = match opts.method {
            PdeMethod::CrankNicolson => 0.5,
            PdeMethod::Implicit => 1.0,
            PdeMethod::Explicit => 0.0,
        };
        sv.theta = theta;
        let mut ntv = match opts.step {
            None => 1000,
            Some(st) => ((span / st - 1e-9).ceil() as i64).max(1) as usize,
        };
        if opts.step.is_none() && theta == 0.0 && amax > 0.0 {
            ntv = ntv.max((span / (0.45 * h * h / amax)).ceil() as usize);
        }
        nt = ntv;
        dt = span / nt as f64;
        if theta == 0.0 {
            if is_complex && a.iter().any(|v| v.im != 0.0) {
                return Err(PdeFail(
                    "the explicit method is unstable for the Schrödinger equation at any step; use crank_nicolson (the default)".into(),
                ));
            }
            if amax * dt / (h * h) > 0.5 + 1e-12 {
                return Err(PdeFail(format!(
                    "the explicit method is unstable with this step: it needs dt ≤ h²/(2D) = {} s (SI) on this grid; use a smaller step, or crank_nicolson",
                    fmt_g(0.5 * h * h / amax, 4)
                )));
            }
        }
    } else {
        if is_complex {
            return Err(PdeFail("a complex equation must be first order in t (like the Schrödinger equation)".into()));
        }
        let cmax = if amax > 0.0 { a.iter().fold(0.0f64, |mx, v| mx.max(v.re.abs())).sqrt() } else { 0.0 };
        if a.iter().any(|v| v.re < 0.0) {
            return Err(PdeFail(
                "this second-order equation isn't a wave equation (the coefficient of ∂²u/∂x² must be positive, c²)".into(),
            ));
        }
        nt = match opts.step {
            None => {
                if cmax > 0.0 { ((span * cmax / h - 1e-9).ceil() as i64).max(1) as usize } else { 1000 }
            }
            Some(st) => ((span / st - 1e-9).ceil() as i64).max(1) as usize,
        };
        dt = span / nt as f64;
        if cmax * dt > h * (1.0 + 1e-9) {
            return Err(PdeFail(format!(
                "the wave equation's explicit scheme is unstable with this step: it needs c dt ≤ h, dt ≤ {} s (SI) on this grid (Courant number {}); use a smaller step or leave out step",
                fmt_g(h / cmax, 4),
                fmt_g(cmax * dt / h, 3)
            )));
        }
    }
    let snaps: Snapshots = if opts.order == 1 {
        if sv.theta == 0.0 { sv.march(nt, true, &[])?.0 } else { sv.controlled(nt, opts.step, &mut warnings)? }
    } else {
        let every = nt.div_ceil(MAX_SNAPSHOTS).max(1);
        let v = v.unwrap();
        let mut snaps: Snapshots = (Vec::new(), Vec::new(), Vec::new());
        let acc = |sv: &mut Solver, w: &[C64], vel: &[C64], t: f64| -> Vec<C64> {
            let tw = sv.apply_t(w);
            let b = sv.bvec(t);
            (0..w.len()).map(|j| (tw[j] + dco[j] * vel[j]) + b[j]).collect()
        };
        let a0 = acc(&mut sv, &u, &v, t0);
        let mut u_prev = u.clone();
        let hdt = 0.5 * dt * dt;
        let mut u_cur: Vec<C64> = (0..n).map(|j| (u[j] + v[j].scale(dt)) + a0[j].scale(hdt)).collect();
        sv.set_dirichlet(&mut u_cur, t0 + dt);
        let keep = |sv: &mut Solver, snaps: &mut Snapshots, t: f64, w: &[C64], dw: &[C64]| -> Result<(), PdeFail> {
            if w.iter().any(|x| !x.is_finite()) {
                return Err(PdeFail(format!(
                    "the solution became NaN or infinite at t = {} s (SI): the scheme is unstable or the equation blows up",
                    fmt_g(t, 6)
                )));
            }
            snaps.0.push(t);
            snaps.1.push(sv.row(w));
            snaps.2.push(sv.row(dw));
            Ok(())
        };
        keep(&mut sv, &mut snaps, t0, &u, &v)?;
        let damp: Vec<C64> = dco.iter().map(|d| d.scale(dt) / C64::new(2.0, 0.0)).collect();
        let one = C64::new(1.0, 0.0);
        let dt2 = dt * dt;
        for step in 1..=nt {
            let t = t0 + step as f64 * dt;
            let tw = sv.apply_t(&u_cur);
            let b = sv.bvec(t);
            let mut u_next: Vec<C64> = (0..n)
                .map(|j| ((u_cur[j].scale(2.0) - (one + damp[j]) * u_prev[j]) + (tw[j] + b[j]).scale(dt2)) / (one - damp[j]))
                .collect();
            sv.set_dirichlet(&mut u_next, t + dt);
            if step % every == 0 || step == nt {
                let dw: Vec<C64> = (0..n).map(|j| (u_next[j] - u_prev[j]) / C64::new(2.0 * dt, 0.0)).collect();
                keep(&mut sv, &mut snaps, t, &u_cur, &dw)?;
            }
            u_prev = std::mem::replace(&mut u_cur, u_next);
        }
        snaps
    };
    let ncomp = if is_complex { 2 } else { 1 };
    Ok(PdeResult {
        ts: snaps.0,
        ys: snaps.1.into_iter().flatten().collect(),
        dys: snaps.2.into_iter().flatten().collect(),
        ncomp,
        m,
        warnings,
        jump,
    })
}

// ------------------------------------------------------------------ the grid check (Fermium 2, not in v1)
//
// v1 controls the time step of a PDE but not the grid: right after a jump (a boundary value that differs from
// the initial value), before the diffusion length √(D t) spans a few grid cells, u(x, t) between the boundary and
// the first nodes is an interpolation and can be 60 % off with no warning (OPEN_ITEMS RT7-2, spec B2). Fermium 2
// solves such a first-order equation a second time on a grid half as fine, with the same time steps, and u(x, t)
// compares the two there: for the second-order scheme the fine grid's error would be about |fine − coarse| / 3
// (near an unresolved jump it is larger). Where that is over PDE_TOL of the solution's range, the evaluator warns
// (rust/DIVERGENCES.md). Values are unchanged.

impl PdeResult {
    /// The snapshots as a stored solution (t, rows of u, rows of ∂u/∂t), as v1's SolStruct holds them.
    pub fn to_sol(&self) -> super::ode::Sol {
        let mut s = super::ode::Sol::new(self.ncomp * (self.m + 1));
        s.t = self.ts.clone();
        s.y = self.ys.clone();
        s.dy = self.dys.clone();
        s
    }

    /// The range of each component over all snapshots and grid points (max − min, or max |u| if flat).
    pub fn ranges(&self) -> Vec<f64> {
        let w = self.m + 1;
        let dim = self.ncomp * w;
        (0..self.ncomp)
            .map(|c| {
                let vals = (0..self.ts.len()).flat_map(|i| self.ys[i * dim + c * w..i * dim + (c + 1) * w].iter().copied());
                let (mut lo, mut hi, mut big) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
                for v in vals {
                    lo = lo.min(v);
                    hi = hi.max(v);
                    big = big.max(v.abs());
                }
                if hi - lo > 0.0 { hi - lo } else { big }
            })
            .collect()
    }
}

/// u(x, t) (which 0), ∂u/∂x (1) or ∂u/∂t (2) of one component of a PDE solution: cubic Lagrange interpolation
/// in x through the 4 grid points around x, the solution's Hermite interpolation in t at each (v1's
/// fm_pde_eval, operation for operation). x must be inside [xa, xb] (the caller checks).
#[allow(clippy::too_many_arguments)]
pub fn pde_eval(sol: &super::ode::Sol, xa: f64, xb: f64, m: usize, comp0: usize, x: f64, t: f64, which: u8)
                -> Result<f64, super::Fail> {
    let m = m as i64;
    let h = (xb - xa) / m as f64;
    let s = (x - xa) / h;
    let mut j = s.floor() as i64;
    j = j.max(1);
    j = j.min(m - 2);
    let r = s - j as f64;
    let r2 = r * r;
    let (rm1, rm2, rp1) = (r - 1.0, r - 2.0, r + 1.0);
    let wv = [((0.0 - r) * rm1) * rm2 / 6.0, (rp1 * rm1) * rm2 / 2.0, ((0.0 - rp1) * r) * rm2 / 2.0, (rp1 * r) * rm1 / 6.0];
    let t3 = 3.0 * r2;
    let wd = [(0.0 - ((t3 - 6.0 * r) + 2.0)) / (6.0 * h), ((t3 - 4.0 * r) - 1.0) / (2.0 * h),
              (0.0 - ((t3 - 2.0 * r) - 2.0)) / (2.0 * h), (t3 - 1.0) / (6.0 * h)];
    let base = comp0 as i64 + (j - 1);
    let mut acc = 0.0;
    for k in 0..4 {
        let w = if which == 1 { wd[k] } else { wv[k] };
        let val = sol.eval((base + k as i64) as usize, t, which == 2, None)?;
        acc += w * val;
    }
    Ok(acc)
}

/// A PDE solution and, for the grid check, the same equation on a grid half as fine with the same time steps.
#[derive(Debug, Clone)]
pub struct CheckedPde {
    pub fine: PdeResult,
    pub coarse: Option<PdeResult>,
}

/// `pde_solve`, plus the check solution on half the grid (first-order equations with an even grid of 8 or more
/// intervals; None when the check doesn't apply or the snapshots don't line up).
pub fn pde_solve_checked(probe: &mut Probe<'_>, xa: f64, xb: f64, t0: f64, t1: f64, opts: PdeOpts)
                         -> Result<CheckedPde, PdeFail> {
    let fine = pde_solve(probe, xa, xb, t0, t1, opts)?;
    let mut coarse = None;
    if fine.jump && opts.order == 1 && opts.grid % 2 == 0 && opts.grid >= 8 && fine.ts.len() >= 2 {
        // the first recorded step is step 1, so the fine solve's step is ts[1] − t0
        let n = ((t1 - t0) / (fine.ts[1] - fine.ts[0])).round().max(1.0);
        let o = PdeOpts { grid: opts.grid / 2, step: Some((t1 - t0) / n), ..opts };
        if let Ok(c) = pde_solve(probe, xa, xb, t0, t1, o) {
            let aligned = c.ts.len() == fine.ts.len()
                && c.ts.iter().zip(&fine.ts).all(|(a, b)| (a - b).abs() <= 1e-9 * (t1 - t0).abs());
            if aligned {
                coarse = Some(c);
            }
        }
    }
    Ok(CheckedPde { fine, coarse })
}

/// The grid check at (x, t) for component c: the estimated error of the fine solution's u there, relative to the
/// component's range (|fine − coarse| / 3 / range), or None without a check solution.
pub fn grid_error(fine: &super::ode::Sol, coarse: &super::ode::Sol, m: usize, xa: f64, xb: f64, c: usize, range: f64,
                  x: f64, t: f64) -> Option<f64> {
    if m < 8 || range <= 0.0 {
        return None;
    }
    let f = pde_eval(fine, xa, xb, m, c * (m + 1), x, t, 0).ok()?;
    let mc = m / 2;
    let g = pde_eval(coarse, xa, xb, mc, c * (mc + 1), x, t, 0).ok()?;
    Some((f - g).abs() / 3.0 / range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_step_very_early_time_is_accurate_or_warned() {
        // red team round 7 #2 (OPEN_ITEMS RT7-2): D = 1e-4 m²/s, u(x, 0) = 0 K, u(0, t) = 80 K, u(1 m, t) = 0 K.
        // v1 prints 55.0 K and 42.2 K where 80 K erfc(x / (2√(D t))) is 34.34 K and 38.36 K, with no warning.
        let d = 1e-4;
        let mut probe = |_x: f64, a: &[f64; 6]| [d * a[2], 0.0, 0.0, 0.0, 80.0, 0.0];
        let r = pde_solve_checked(&mut probe, 0.0, 1.0, 0.0, 100.0, PdeOpts::default()).unwrap();
        let (fine, coarse) = (r.fine.to_sol(), r.coarse.as_ref().expect("check solution").to_sol());
        let range = r.fine.ranges()[0];
        let m = r.fine.m;
        // 80 K erfc(x / (2√(D t))) from Python's math.erfc
        for (x, t, exact) in [(0.5e-3, 0.002, 34.33562403522794), (0.25e-3, 0.0025, 57.89388878654105),
                              (0.5e-3, 0.01, 57.89388878654105), (1e-3, 0.01, 38.36000977495628),
                              (0.1, 50.0, 25.38484062903312)] {
            let u = pde_eval(&fine, 0.0, 1.0, m, 0, x, t, 0).unwrap();
            let est = grid_error(&fine, &coarse, m, 0.0, 1.0, 0, range, x, t).unwrap();
            let wrong = (u - exact).abs() / range;
            // either accurate to the tolerance, or the check says so (and its estimate is not far below the error)
            assert!(wrong <= 2.0 * PDE_TOL || est > PDE_TOL, "u({x}, {t}) = {u} vs {exact}: error {wrong}, estimate {est}");
        }
        // later, once the solution has spread over many cells, it is accurate and not flagged
        let (x, t) = (0.1, 50.0);
        let u = pde_eval(&fine, 0.0, 1.0, m, 0, x, t, 0).unwrap();
        assert!((u - 25.38484062903312).abs() < 1e-3 * range);
        assert!(grid_error(&fine, &coarse, m, 0.0, 1.0, 0, range, x, t).unwrap() < PDE_TOL);
        // no jump (the boundary values match the initial value): no check solution
        let pi = std::f64::consts::PI;
        let mut smooth = |x: f64, a: &[f64; 6]| [a[2], (pi * x).sin(), 0.0, 0.0, 0.0, 0.0];
        let r = pde_solve_checked(&mut smooth, 0.0, 1.0, 0.0, 0.1, PdeOpts { grid: 100, ..Default::default() }).unwrap();
        assert!(r.coarse.is_none() && !r.fine.jump);
    }

    #[test]
    fn heat_decay_of_a_sine() {
        // u_t = u_xx on [0, 1], u = sin(πx): u(x, t) = e^{-π² t} sin(πx)
        let pi = std::f64::consts::PI;
        let mut probe = |x: f64, a: &[f64; 6]| [a[2], (pi * x).sin(), 0.0, 0.0, 0.0, 0.0];
        let r = pde_solve(&mut probe, 0.0, 1.0, 0.0, 0.1, PdeOpts { grid: 100, ..Default::default() }).unwrap();
        let last = &r.ys[r.ys.len() - 101..];
        let exact = (-pi * pi * 0.1).exp();
        assert!((last[50] - exact).abs() < 1e-3 * exact, "{} vs {exact}", last[50]);
        assert_eq!(fmt_g(0.30000000001, 6), "0.3");
        assert_eq!(fmt_g(1.5e-7, 4), "1.5e-07");
    }
}
