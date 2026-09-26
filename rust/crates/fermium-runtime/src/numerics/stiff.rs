//! Implicit ODE solvers for stiff equations: `solve ... using radau` and `using bdf` (D42).
//!
//! v1 stepped with SciPy's `Radau` (implicit Runge–Kutta Radau IIA of order 5, Hairer & Wanner's
//! RADAU5 design) and `BDF` (variable-order 1–5 NDF/BDF in the quasi-constant-step form of
//! Shampine & Reichelt's ode15s) and added Fermium's side in `fermium/runtime/stiff.py`: the
//! tolerance mapping, the `until` event located on the step's dense output, the error kinds and
//! the stored solution. Here both steppers are native ports of SciPy's algorithms (same
//! constants, Newton iteration, error estimates, step-size and order control, finite-difference
//! Jacobian `num_jac`, dense output), and [`stiff_solve`] ports v1's wrapper line by line.
//! The LU is our own partial-pivoting LU (LAPACK's pivot choice), so results agree with SciPy to
//! rounding-level differences that can occasionally tip a step decision; see NUMERICS.md for the
//! measured agreement.

use super::dense::C64;
use super::npblas::{self as nb, ComplexLu, RealLu};
use super::ode::{illinois, step_small_kind, EventFn, OdeOpts, Sol};
use super::{err, pymax, pymin, Fail};

const EPS: f64 = f64::EPSILON;
/// v1's MAX_STEPS for the stiff solvers
pub const MAX_STEPS: u64 = 1_000_000;

/// Which implicit method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StiffMethod {
    Radau,
    Bdf,
}

type Rhs<'a> = dyn FnMut(f64, &[f64], &mut [f64]) + 'a;

/// SciPy's norm (np.linalg.norm(x) / √n), with NumPy's dot product rounding (npblas.rs).
fn norm(x: &[f64]) -> f64 {
    super::npblas::norm(x)
}

fn call(f: &mut Rhs<'_>, t: f64, y: &[f64]) -> Vec<f64> {
    let mut o = vec![0.0; y.len()];
    f(t, y, &mut o);
    o
}

/// SciPy's `select_initial_step`.
#[allow(clippy::too_many_arguments)]
fn select_initial_step(
    f: &mut Rhs<'_>,
    t0: f64,
    y0: &[f64],
    t_bound: f64,
    f0: &[f64],
    direction: f64,
    order: f64,
    rtol: f64,
    atol: &[f64],
) -> f64 {
    let n = y0.len();
    if n == 0 {
        return f64::INFINITY;
    }
    let interval = (t_bound - t0).abs();
    if interval == 0.0 {
        return 0.0;
    }
    let scale: Vec<f64> = (0..n).map(|i| atol[i] + y0[i].abs() * rtol).collect();
    let d0 = norm(&(0..n).map(|i| y0[i] / scale[i]).collect::<Vec<_>>());
    let d1 = norm(&(0..n).map(|i| f0[i] / scale[i]).collect::<Vec<_>>());
    let h0 = if d0 < 1e-5 || d1 < 1e-5 { 1e-6 } else { 0.01 * d0 / d1 };
    let h0 = pymin(h0, interval);
    let y1: Vec<f64> = (0..n).map(|i| y0[i] + h0 * direction * f0[i]).collect();
    let f1 = call(f, t0 + h0 * direction, &y1);
    let d2 = norm(&(0..n).map(|i| (f1[i] - f0[i]) / scale[i]).collect::<Vec<_>>()) / h0;
    let h1 = if d1 <= 1e-15 && d2 <= 1e-15 {
        pymax(1e-6, h0 * 0.001)
    } else {
        (0.01 / pymax(d1, d2)).powf(1.0 / (order + 1.0))
    };
    pymin(pymin(100.0 * h0, h1), interval) // max_step = inf
}

/// SciPy's `num_jac` (dense): the forward-difference Jacobian with adaptive per-column factors.
/// Returns J row-major (J[i][j] = ∂f_i/∂y_j).
fn num_jac(f: &mut Rhs<'_>, t: f64, y: &[f64], fy: &[f64], threshold: &[f64], factor: &mut Option<Vec<f64>>) -> Vec<f64> {
    let n = y.len();
    let diff_reject = EPS.powf(0.875);
    let diff_small = EPS.powf(0.75);
    let diff_big = EPS.powf(0.25);
    let min_factor = 1000.0 * EPS;
    let mut fac = factor.clone().unwrap_or_else(|| vec![EPS.powf(0.5); n]);
    let y_scale: Vec<f64> = (0..n)
        .map(|i| {
            let sg = if fy[i] >= 0.0 { 1.0 } else { -1.0 };
            sg * pymax(threshold[i], y[i].abs())
        })
        .collect();
    let mut h: Vec<f64> = (0..n).map(|i| y[i] + fac[i] * y_scale[i] - y[i]).collect();
    for i in 0..n {
        while h[i] == 0.0 {
            fac[i] *= 10.0;
            h[i] = y[i] + fac[i] * y_scale[i] - y[i];
        }
    }
    // column j: f(t, y + h_j e_j)
    let eval_col = |f: &mut Rhs<'_>, j: usize, hj: f64| -> Vec<f64> {
        let mut yy = y.to_vec();
        yy[j] += hj;
        call(f, t, &yy)
    };
    let mut diff = vec![0.0; n * n]; // diff[i*n + j]
    let mut max_diff = vec![0.0; n];
    let mut scale = vec![0.0; n];
    let col_stats = |fnew: &[f64], j_diff: &mut dyn FnMut(usize, f64)| -> (f64, f64) {
        let mut mi = 0usize;
        let mut best = -1.0f64;
        for i in 0..n {
            let d = fnew[i] - fy[i];
            j_diff(i, d);
            if d.abs() > best {
                best = d.abs();
                mi = i;
            }
        }
        let md = (fnew[mi] - fy[mi]).abs();
        (md, pymax(fy[mi].abs(), fnew[mi].abs()))
    };
    for j in 0..n {
        let fnew = eval_col(f, j, h[j]);
        let (md, sc) = col_stats(&fnew, &mut |i, d| diff[i * n + j] = d);
        max_diff[j] = md;
        scale[j] = sc;
    }
    for j in 0..n {
        if max_diff[j] < diff_reject * scale[j] {
            let new_factor = 10.0 * fac[j];
            let h_new = y[j] + new_factor * y_scale[j] - y[j];
            let fnew = eval_col(f, j, h_new);
            let mut dnew = vec![0.0; n];
            let (md_new, sc_new) = col_stats(&fnew, &mut |i, d| dnew[i] = d);
            if max_diff[j] * sc_new < md_new * scale[j] {
                fac[j] = new_factor;
                h[j] = h_new;
                for i in 0..n {
                    diff[i * n + j] = dnew[i];
                }
                scale[j] = sc_new;
                max_diff[j] = md_new;
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            diff[i * n + j] /= h[j];
        }
    }
    for j in 0..n {
        if max_diff[j] < diff_small * scale[j] {
            fac[j] *= 10.0;
        } else if max_diff[j] > diff_big * scale[j] {
            fac[j] *= 0.1;
        }
        fac[j] = pymax(fac[j], min_factor);
    }
    *factor = Some(fac);
    diff
}

enum StepErr {
    /// SciPy's "Required step size is less than spacing between numbers" (status failed)
    TooSmall,
    /// NaN/∞ reached a matrix to factor (SciPy raises ValueError)
    NonFinite,
}

// ------------------------------------------------------------------------------ Radau IIA

struct RadauConst {
    c: [f64; 3],
    e: [f64; 3],
    mu_real: f64,
    mu_complex: C64,
    t: [[f64; 3]; 3],
    ti: [[f64; 3]; 3],
    p: [[f64; 3]; 3],
}

fn radau_const() -> RadauConst {
    let s6 = 6f64.powf(0.5);
    let mu_real = 3.0 + 3f64.powf(2.0 / 3.0) - 3f64.powf(1.0 / 3.0);
    let mu_complex = C64::new(
        3.0 + 0.5 * (3f64.powf(1.0 / 3.0) - 3f64.powf(2.0 / 3.0)),
        -(0.5 * (3f64.powf(5.0 / 6.0) + 3f64.powf(7.0 / 6.0))),
    );
    RadauConst {
        c: [(4.0 - s6) / 10.0, (4.0 + s6) / 10.0, 1.0],
        e: [(-13.0 - 7.0 * s6) / 3.0, (-13.0 + 7.0 * s6) / 3.0, -1.0 / 3.0],
        mu_real,
        mu_complex,
        t: [
            [0.09443876248897524, -0.14125529502095421, 0.03002919410514742],
            [0.25021312296533332, 0.20412935229379994, -0.38294211275726192],
            [1.0, 1.0, 0.0],
        ],
        ti: [
            [4.17871859155190428, 0.32768282076106237, 0.52337644549944951],
            [-4.17871859155190428, -0.32768282076106237, 0.47662355450055044],
            [0.50287263494578682, -2.57192694985560522, 0.59603920482822492],
        ],
        p: [
            [13.0 / 3.0 + 7.0 * s6 / 3.0, -23.0 / 3.0 - 22.0 * s6 / 3.0, 10.0 / 3.0 + 5.0 * s6],
            [13.0 / 3.0 - 7.0 * s6 / 3.0, -23.0 / 3.0 + 22.0 * s6 / 3.0, 10.0 / 3.0 - 5.0 * s6],
            [1.0 / 3.0, -8.0 / 3.0, 10.0 / 3.0],
        ],
    }
}

/// The dense output of the last Radau step: y_old + Q·[x, x², x³], x = (t − t_old)/h.
#[derive(Clone)]
struct RadauDense {
    t_old: f64,
    h: f64,
    y_old: Vec<f64>,
    q: Vec<[f64; 3]>,
}

impl RadauDense {
    /// At one time (np.dot(Q, p) with p a vector).
    fn eval(&self, t: f64) -> Vec<f64> {
        let x = (t - self.t_old) / self.h;
        let p = [x, x * x, x * x * x];
        self.y_old.iter().zip(&self.q).map(|(&y0, q)| nb::qdotp(q, &p) + y0).collect()
    }

    /// At several times at once (np.dot(Q, p) with p a matrix: a dgemm), as the step's prediction calls it.
    fn eval_many(&self, t: f64) -> Vec<f64> {
        let x = (t - self.t_old) / self.h;
        let p = [x, x * x, x * x * x];
        self.y_old.iter().zip(&self.q).map(|(&y0, q)| nb::chain3(*q, p) + y0).collect()
    }
}

struct Radau {
    k: RadauConst,
    n: usize,
    t: f64,
    y: Vec<f64>,
    f: Vec<f64>,
    t_bound: f64,
    direction: f64,
    rtol: f64,
    atol: Vec<f64>,
    h_abs: f64,
    h_abs_old: Option<f64>,
    error_norm_old: Option<f64>,
    newton_tol: f64,
    jac_factor: Option<Vec<f64>>,
    j: Vec<f64>,
    current_jac: bool,
    lu_real: Option<RealLu>,
    lu_complex: Option<ComplexLu>,
    sol: Option<RadauDense>,
    t_old: Option<f64>,
}

fn predict_factor(h_abs: f64, h_abs_old: Option<f64>, error_norm: f64, error_norm_old: Option<f64>) -> f64 {
    let multiplier = match (error_norm_old, h_abs_old) {
        (Some(eno), Some(hao)) if error_norm != 0.0 => h_abs / hao * (eno / error_norm).powf(0.25),
        _ => 1.0,
    };
    pymin(1.0, multiplier) * error_norm.powf(-0.25)
}

const RADAU_NEWTON_MAXITER: usize = 6;
const MIN_FACTOR: f64 = 0.2;
const MAX_FACTOR: f64 = 10.0;

impl Radau {
    fn new(f: &mut Rhs<'_>, t0: f64, y0: &[f64], t_bound: f64, rtol: f64, atol: Vec<f64>) -> Result<Self, StepErr> {
        let n = y0.len();
        let direction = if t_bound != t0 { (t_bound - t0).signum() } else { 1.0 };
        let rtol = if rtol < 100.0 * EPS { 100.0 * EPS } else { rtol };
        let fy = call(f, t0, y0);
        let h_abs = select_initial_step(f, t0, y0, t_bound, &fy, direction, 3.0, rtol, &atol);
        let newton_tol = pymax(10.0 * EPS / rtol, pymin(0.03, rtol.powf(0.5)));
        let mut jac_factor = None;
        let j = num_jac(f, t0, y0, &fy, &atol, &mut jac_factor);
        Ok(Radau {
            k: radau_const(),
            n,
            t: t0,
            y: y0.to_vec(),
            f: fy,
            t_bound,
            direction,
            rtol,
            atol,
            h_abs,
            h_abs_old: None,
            error_norm_old: None,
            newton_tol,
            jac_factor,
            j,
            current_jac: true,
            lu_real: None,
            lu_complex: None,
            sol: None,
            t_old: None,
        })
    }

    fn lu_real(&self, h: f64) -> Result<RealLu, StepErr> {
        let n = self.n;
        let m = self.k.mu_real / h;
        // MU_REAL / h * I - J
        let a: Vec<f64> = (0..n * n).map(|ij| if ij / n == ij % n { m - self.j[ij] } else { 0.0 - self.j[ij] }).collect();
        RealLu::new(&a, n).ok_or(StepErr::NonFinite)
    }

    fn lu_complex(&self, h: f64) -> Result<ComplexLu, StepErr> {
        let n = self.n;
        let m = C64::new(self.k.mu_complex.re / h, self.k.mu_complex.im / h);
        let a: Vec<C64> = (0..n * n)
            .map(|ij| {
                let jv = C64::new(self.j[ij], 0.0);
                if ij / n == ij % n { m - jv } else { C64::default() - jv }
            })
            .collect();
        ComplexLu::new(&a, n).ok_or(StepErr::NonFinite)
    }

    /// SciPy's `solve_collocation_system`: (converged, n_iter, Z, rate)
    #[allow(clippy::too_many_arguments)]
    fn collocation(
        &self,
        f: &mut Rhs<'_>,
        h: f64,
        z0: &[Vec<f64>; 3],
        scale: &[f64],
        lu_r: &RealLu,
        lu_c: &ComplexLu,
    ) -> (bool, usize, [Vec<f64>; 3], Option<f64>) {
        let (n, k, t, y) = (self.n, &self.k, self.t, &self.y);
        let m_real = k.mu_real / h;
        let m_complex = C64::new(k.mu_complex.re / h, k.mu_complex.im / h);
        // M.dot(X) for the 3×3 TI and T and X of shape (3, n): a dgemm (FMA chains), but for n = 1 X is a column
        // and NumPy calls dgemv 'T' instead, which sums in another order (qdotp)
        let m3 = |a: [f64; 3], x: [f64; 3]| if n == 1 { nb::qdotp(&a, &x) } else { nb::chain3(a, x) };
        // W = TI.dot(Z0)
        let mut w: [Vec<f64>; 3] =
            std::array::from_fn(|r| (0..n).map(|j| m3(k.ti[r], [z0[0][j], z0[1][j], z0[2][j]])).collect());
        let mut z = z0.clone();
        let ch = [h * k.c[0], h * k.c[1], h * k.c[2]];
        let mut dw_norm_old: Option<f64> = None;
        let mut converged = false;
        let mut rate: Option<f64> = None;
        let mut iters = 0;
        for it in 0..RADAU_NEWTON_MAXITER {
            iters = it + 1;
            let mut fs: [Vec<f64>; 3] = std::array::from_fn(|_| vec![0.0; n]);
            for i in 0..3 {
                let yy: Vec<f64> = (0..n).map(|j| y[j] + z[i][j]).collect();
                f(t + ch[i], &yy, &mut fs[i]);
            }
            if fs.iter().any(|r| r.iter().any(|v| !v.is_finite())) {
                break;
            }
            // f_real = F.T.dot(TI_REAL) - M_real * W[0]; f_complex = F.T.dot(TI_COMPLEX) - M_complex * (W[1] + 1j W[2])
            let mut f_real: Vec<f64> =
                (0..n).map(|j| nb::gemv_t3([fs[0][j], fs[1][j], fs[2][j]], &k.ti[0], n, j) - m_real * w[0][j]).collect();
            let mut f_complex: Vec<C64> = (0..n)
                .map(|j| {
                    let col = [fs[0][j], fs[1][j], fs[2][j]];
                    let s = C64::new(nb::cgemv_t3(col, &k.ti[1], n, j), nb::cgemv_t3(col, &k.ti[2], n, j));
                    s - nb::cmul(m_complex, C64::new(w[1][j], w[2][j]))
                })
                .collect();
            lu_r.solve(&mut f_real);
            lu_c.solve(&mut f_complex);
            let dw: [Vec<f64>; 3] = [f_real, f_complex.iter().map(|c| c.re).collect(), f_complex.iter().map(|c| c.im).collect()];
            let scaled: Vec<f64> = (0..3).flat_map(|r| (0..n).map(move |j| (r, j))).map(|(r, j)| dw[r][j] / scale[j]).collect();
            let dw_norm = norm(&scaled);
            if let Some(old) = dw_norm_old {
                rate = Some(dw_norm / old);
            }
            if let Some(rt) = rate {
                if rt >= 1.0 || rt.powf((RADAU_NEWTON_MAXITER - it) as f64) / (1.0 - rt) * dw_norm > self.newton_tol {
                    break;
                }
            }
            for r in 0..3 {
                for j in 0..n {
                    w[r][j] += dw[r][j];
                }
            }
            z = std::array::from_fn(|r| (0..n).map(|j| m3(k.t[r], [w[0][j], w[1][j], w[2][j]])).collect());
            if dw_norm == 0.0 || rate.map(|rt| rt / (1.0 - rt) * dw_norm < self.newton_tol).unwrap_or(false) {
                converged = true;
                break;
            }
            dw_norm_old = Some(dw_norm);
        }
        (converged, iters, z, rate)
    }

    fn step(&mut self, f: &mut Rhs<'_>) -> Result<(), StepErr> {
        let n = self.n;
        let t = self.t;
        let y = self.y.clone();
        let fy = self.f.clone();
        let rtol = self.rtol;
        let atol = self.atol.clone();
        let next = next_toward(t, self.direction * f64::INFINITY);
        let min_step = 10.0 * (next - t).abs();
        let (mut h_abs, h_abs_old, error_norm_old) = if self.h_abs < min_step {
            (min_step, None, None)
        } else {
            (self.h_abs, self.h_abs_old, self.error_norm_old)
        };
        let mut lu_r = self.lu_real.take();
        let mut lu_c = self.lu_complex.take();
        let mut current_jac = self.current_jac;
        let mut rejected = false;
        let (mut t_new, mut y_new, mut z, mut n_iter, mut rate, mut error_norm, mut safety);
        loop {
            if h_abs < min_step {
                return Err(StepErr::TooSmall);
            }
            let mut h = h_abs * self.direction;
            t_new = t + h;
            if self.direction * (t_new - self.t_bound) > 0.0 {
                t_new = self.t_bound;
            }
            h = t_new - t;
            h_abs = h.abs();
            let z0: [Vec<f64>; 3] = match &self.sol {
                None => std::array::from_fn(|_| vec![0.0; n]),
                Some(s) => std::array::from_fn(|i| {
                    let v = s.eval_many(t + h * self.k.c[i]);
                    (0..n).map(|j| v[j] - y[j]).collect()
                }),
            };
            let scale: Vec<f64> = (0..n).map(|j| atol[j] + y[j].abs() * rtol).collect();
            let mut converged = false;
            let mut res = None;
            while !converged {
                if lu_r.is_none() || lu_c.is_none() {
                    lu_r = Some(self.lu_real(h)?);
                    lu_c = Some(self.lu_complex(h)?);
                }
                let out = self.collocation(f, h, &z0, &scale, lu_r.as_ref().unwrap(), lu_c.as_ref().unwrap());
                converged = out.0;
                res = Some(out);
                if !converged {
                    if current_jac {
                        break;
                    }
                    self.j = num_jac(f, t, &y, &fy, &self.atol, &mut self.jac_factor);
                    current_jac = true;
                    lu_r = None;
                    lu_c = None;
                }
            }
            if !converged {
                h_abs *= 0.5;
                lu_r = None;
                lu_c = None;
                continue;
            }
            let (_, it, zz, rt) = res.unwrap();
            n_iter = it;
            rate = rt;
            z = zz;
            y_new = (0..n).map(|j| y[j] + z[2][j]).collect::<Vec<f64>>();
            let ze: Vec<f64> = (0..n).map(|j| nb::gemv_t3([z[0][j], z[1][j], z[2][j]], &self.k.e, n, j) / h).collect();
            let lr = lu_r.as_ref().unwrap();
            let mut error: Vec<f64> = (0..n).map(|j| fy[j] + ze[j]).collect();
            lr.solve(&mut error);
            let scale: Vec<f64> = (0..n).map(|j| atol[j] + pymax(y[j].abs(), y_new[j].abs()) * rtol).collect();
            error_norm = norm(&(0..n).map(|j| error[j] / scale[j]).collect::<Vec<_>>());
            safety = 0.9 * (2 * RADAU_NEWTON_MAXITER + 1) as f64 / (2 * RADAU_NEWTON_MAXITER + n_iter) as f64;
            if rejected && error_norm > 1.0 {
                let ye: Vec<f64> = (0..n).map(|j| y[j] + error[j]).collect();
                let fe = call(f, t, &ye);
                let mut e2: Vec<f64> = (0..n).map(|j| fe[j] + ze[j]).collect();
                lr.solve(&mut e2);
                error_norm = norm(&(0..n).map(|j| e2[j] / scale[j]).collect::<Vec<_>>());
            }
            if error_norm > 1.0 {
                let factor = predict_factor(h_abs, h_abs_old, error_norm, error_norm_old);
                h_abs *= pymax(MIN_FACTOR, safety * factor);
                lu_r = None;
                lu_c = None;
                rejected = true;
            } else {
                break;
            }
        }
        let recompute_jac = n_iter > 2 && rate.map(|r| r > 1e-3).unwrap_or(false);
        let factor = predict_factor(h_abs, h_abs_old, error_norm, error_norm_old);
        let mut factor = pymin(MAX_FACTOR, safety * factor);
        if !recompute_jac && factor < 1.2 {
            factor = 1.0;
        } else {
            lu_r = None;
            lu_c = None;
        }
        let f_new = call(f, t_new, &y_new);
        if recompute_jac {
            self.j = num_jac(f, t_new, &y_new, &f_new, &self.atol, &mut self.jac_factor);
            current_jac = true;
        } else {
            current_jac = false;
        }
        self.h_abs_old = Some(self.h_abs);
        self.error_norm_old = Some(error_norm);
        self.h_abs = h_abs * factor;
        self.t = t_new;
        self.y = y_new;
        self.f = f_new;
        self.lu_real = lu_r;
        self.lu_complex = lu_c;
        self.current_jac = current_jac;
        self.t_old = Some(t);
        let p = &self.k.p;
        let q: Vec<[f64; 3]> = (0..n)
            .map(|j| std::array::from_fn(|c| nb::chain3([z[0][j], z[1][j], z[2][j]], [p[0][c], p[1][c], p[2][c]])))
            .collect();
        self.sol = Some(RadauDense { t_old: t, h: t_new - t, y_old: y, q });
        Ok(())
    }
}

/// numpy's nextafter(t, toward)
fn next_toward(t: f64, toward: f64) -> f64 {
    if t.is_nan() || toward.is_nan() {
        return f64::NAN;
    }
    if t == toward {
        return toward;
    }
    if t == 0.0 {
        return if toward > 0.0 { f64::from_bits(1) } else { -f64::from_bits(1) };
    }
    let b = t.to_bits();
    let up = (toward > t) == (t > 0.0);
    f64::from_bits(if up { b + 1 } else { b - 1 })
}

// ------------------------------------------------------------------------------ BDF

const BDF_MAX_ORDER: usize = 5;
const BDF_NEWTON_MAXITER: usize = 4;

/// SciPy's `compute_R`
fn compute_r(order: usize, factor: f64) -> Vec<Vec<f64>> {
    let m = order + 1;
    let mut mm = vec![vec![0.0; m]; m];
    for i in 1..m {
        for j in 1..m {
            let (ii, jj) = (i as f64, j as f64);
            mm[i][j] = (ii - 1.0 - factor * jj) / ii;
        }
    }
    for j in 0..m {
        mm[0][j] = 1.0;
    }
    for i in 1..m {
        for j in 0..m {
            mm[i][j] *= mm[i - 1][j];
        }
    }
    mm
}

/// SciPy's `change_D`
fn change_d(d: &mut [Vec<f64>], order: usize, factor: f64) {
    let r = compute_r(order, factor);
    let u = compute_r(order, 1.0);
    let m = order + 1;
    // RU = R.dot(U)
    let mut ru = vec![vec![0.0; m]; m];
    for i in 0..m {
        for j in 0..m {
            let col: Vec<f64> = (0..m).map(|k| u[k][j]).collect();
            ru[i][j] = nb::gemm_entry(&r[i], &col);
        }
    }
    // D[:m] = np.dot(RU.T, D[:m]): a dgemm, or for one unknown a dgemv (RU.T.dot(d) = entries of A.T.dot(v))
    let n = d[0].len();
    let old: Vec<Vec<f64>> = d[..m].to_vec();
    for i in 0..m {
        let a: Vec<f64> = (0..m).map(|k| ru[k][i]).collect();
        for c in 0..n {
            let b: Vec<f64> = (0..m).map(|k| old[k][c]).collect();
            d[i][c] = if n == 1 { nb::gemv_t(&a, &b, m, i) } else { nb::gemm_entry(&a, &b) };
        }
    }
}

#[derive(Clone)]
struct BdfDense {
    order: usize,
    t_shift: Vec<f64>,
    denom: Vec<f64>,
    d: Vec<Vec<f64>>,
}

impl BdfDense {
    fn eval(&self, t: f64) -> Vec<f64> {
        let n = self.d[0].len();
        let mut p = vec![0.0; self.order];
        let mut acc = 1.0;
        for i in 0..self.order {
            acc *= (t - self.t_shift[i]) / self.denom[i];
            p[i] = acc;
        }
        // np.dot(D[1:].T, p) + D[0]
        (0..n)
            .map(|c| {
                let a: Vec<f64> = (0..self.order).map(|i| self.d[i + 1][c]).collect();
                nb::gemv_t(&a, &p, n, c) + self.d[0][c]
            })
            .collect()
    }
}

struct Bdf {
    n: usize,
    t: f64,
    y: Vec<f64>,
    t_bound: f64,
    direction: f64,
    rtol: f64,
    atol: Vec<f64>,
    h_abs: f64,
    newton_tol: f64,
    jac_factor: Option<Vec<f64>>,
    j: Vec<f64>,
    lu: Option<RealLu>,
    gamma: [f64; 6],
    alpha: [f64; 6],
    error_const: [f64; 6],
    d: Vec<Vec<f64>>,
    order: usize,
    n_equal_steps: usize,
    t_old: Option<f64>,
}

impl Bdf {
    fn new(f: &mut Rhs<'_>, t0: f64, y0: &[f64], t_bound: f64, rtol: f64, atol: Vec<f64>) -> Result<Self, StepErr> {
        let n = y0.len();
        let direction = if t_bound != t0 { (t_bound - t0).signum() } else { 1.0 };
        let rtol = if rtol < 100.0 * EPS { 100.0 * EPS } else { rtol };
        let fy = call(f, t0, y0);
        let h_abs = select_initial_step(f, t0, y0, t_bound, &fy, direction, 1.0, rtol, &atol);
        let newton_tol = pymax(10.0 * EPS / rtol, pymin(0.03, rtol.powf(0.5)));
        let mut jac_factor = None;
        let f0 = call(f, t0, y0);
        let j = num_jac(f, t0, y0, &f0, &atol, &mut jac_factor);
        let kappa = [0.0, -0.185, -1.0 / 9.0, -0.0823, -0.0415, 0.0];
        let mut gamma = [0.0; 6];
        let mut acc = 0.0;
        for i in 1..6 {
            acc += 1.0 / i as f64;
            gamma[i] = acc;
        }
        let alpha: [f64; 6] = std::array::from_fn(|i| (1.0 - kappa[i]) * gamma[i]);
        let error_const: [f64; 6] = std::array::from_fn(|i| kappa[i] * gamma[i] + 1.0 / (i + 1) as f64);
        let mut d = vec![vec![0.0; n]; BDF_MAX_ORDER + 3];
        d[0] = y0.to_vec();
        d[1] = (0..n).map(|i| fy[i] * h_abs * direction).collect();
        Ok(Bdf {
            n,
            t: t0,
            y: y0.to_vec(),
            t_bound,
            direction,
            rtol,
            atol,
            h_abs,
            newton_tol,
            jac_factor,
            j,
            lu: None,
            gamma,
            alpha,
            error_const,
            d,
            order: 1,
            n_equal_steps: 0,
            t_old: None,
        })
    }

    fn jac(&mut self, f: &mut Rhs<'_>, t: f64, y: &[f64]) {
        let fy = call(f, t, y);
        self.j = num_jac(f, t, y, &fy, &self.atol, &mut self.jac_factor);
    }

    fn step(&mut self, f: &mut Rhs<'_>) -> Result<(), StepErr> {
        let n = self.n;
        let t = self.t;
        let next = next_toward(t, self.direction * f64::INFINITY);
        let min_step = 10.0 * (next - t).abs();
        let mut h_abs;
        if self.h_abs < min_step {
            h_abs = min_step;
            change_d(&mut self.d, self.order, min_step / self.h_abs);
            self.n_equal_steps = 0;
        } else {
            h_abs = self.h_abs;
        }
        let atol = self.atol.clone();
        let rtol = self.rtol;
        let order = self.order;
        let mut lu = self.lu.take();
        let mut current_jac = false; // self.jac is the finite-difference wrapper, never None
        let (mut t_new, mut y_new, mut dd, mut n_iter, mut error_norm, mut scale);
        loop {
            if h_abs < min_step {
                return Err(StepErr::TooSmall);
            }
            let mut h = h_abs * self.direction;
            t_new = t + h;
            if self.direction * (t_new - self.t_bound) > 0.0 {
                t_new = self.t_bound;
                change_d(&mut self.d, order, (t_new - t).abs() / h_abs);
                self.n_equal_steps = 0;
                lu = None;
            }
            h = t_new - t;
            h_abs = h.abs();
            let mut y_predict = self.d[0].clone();
            for i in 1..=order {
                for c in 0..n {
                    y_predict[c] += self.d[i][c];
                }
            }
            scale = (0..n).map(|c| atol[c] + rtol * y_predict[c].abs()).collect::<Vec<f64>>();
            // psi = np.dot(D[1: order + 1].T, gamma[1: order + 1]) / alpha[order]
            let psi: Vec<f64> = (0..n)
                .map(|c| {
                    let a: Vec<f64> = (1..=order).map(|i| self.d[i][c]).collect();
                    nb::gemv_t(&a, &self.gamma[1..=order], n, c) / self.alpha[order]
                })
                .collect();
            let mut converged = false;
            let cc = h / self.alpha[order];
            let mut res = None;
            while !converged {
                if lu.is_none() {
                    let a: Vec<f64> =
                        (0..n * n).map(|ij| if ij / n == ij % n { 1.0 - cc * self.j[ij] } else { 0.0 - cc * self.j[ij] }).collect();
                    lu = Some(RealLu::new(&a, n).ok_or(StepErr::NonFinite)?);
                }
                let out = solve_bdf_system(f, t_new, &y_predict, cc, &psi, lu.as_ref().unwrap(), &scale, self.newton_tol);
                converged = out.0;
                res = Some(out);
                if !converged {
                    if current_jac {
                        break;
                    }
                    self.jac(f, t_new, &y_predict);
                    lu = None;
                    current_jac = true;
                }
            }
            if !converged {
                let factor = 0.5;
                h_abs *= factor;
                change_d(&mut self.d, order, factor);
                self.n_equal_steps = 0;
                lu = None;
                continue;
            }
            let (_, it, yn, d) = res.unwrap();
            n_iter = it;
            y_new = yn;
            dd = d;
            let safety = 0.9 * (2 * BDF_NEWTON_MAXITER + 1) as f64 / (2 * BDF_NEWTON_MAXITER + n_iter) as f64;
            scale = (0..n).map(|c| atol[c] + rtol * y_new[c].abs()).collect();
            let ec = self.error_const[order];
            error_norm = norm(&(0..n).map(|c| ec * dd[c] / scale[c]).collect::<Vec<_>>());
            if error_norm > 1.0 {
                let factor = pymax(MIN_FACTOR, safety * error_norm.powf(-1.0 / (order as f64 + 1.0)));
                h_abs *= factor;
                change_d(&mut self.d, order, factor);
                self.n_equal_steps = 0;
            } else {
                break;
            }
        }
        let safety = 0.9 * (2 * BDF_NEWTON_MAXITER + 1) as f64 / (2 * BDF_NEWTON_MAXITER + n_iter) as f64;
        self.n_equal_steps += 1;
        self.t_old = Some(t);
        self.t = t_new;
        self.y = y_new;
        self.h_abs = h_abs;
        self.lu = lu;
        for c in 0..n {
            self.d[order + 2][c] = dd[c] - self.d[order + 1][c];
            self.d[order + 1][c] = dd[c];
        }
        for i in (0..=order).rev() {
            for c in 0..n {
                let v = self.d[i + 1][c];
                self.d[i][c] += v;
            }
        }
        if self.n_equal_steps < order + 1 {
            return Ok(());
        }
        let error_m_norm = if order > 1 {
            let ec = self.error_const[order - 1];
            norm(&(0..n).map(|c| ec * self.d[order][c] / scale[c]).collect::<Vec<_>>())
        } else {
            f64::INFINITY
        };
        let error_p_norm = if order < BDF_MAX_ORDER {
            let ec = self.error_const[order + 1];
            norm(&(0..n).map(|c| ec * self.d[order + 2][c] / scale[c]).collect::<Vec<_>>())
        } else {
            f64::INFINITY
        };
        let norms = [error_m_norm, error_norm, error_p_norm];
        let factors: [f64; 3] = std::array::from_fn(|i| norms[i].powf(-1.0 / (order + i) as f64));
        let mut best = 0usize;
        for i in 1..3 {
            if factors[i] > factors[best] {
                best = i;
            }
        }
        let new_order = (order as i64 + best as i64 - 1) as usize;
        self.order = new_order;
        let fmax = factors[best];
        let factor = pymin(MAX_FACTOR, safety * fmax);
        self.h_abs *= factor;
        change_d(&mut self.d, new_order, factor);
        self.n_equal_steps = 0;
        self.lu = None;
        Ok(())
    }

    fn dense(&self) -> BdfDense {
        let h = self.h_abs * self.direction;
        let order = self.order;
        BdfDense {
            order,
            t_shift: (0..order).map(|i| self.t - h * i as f64).collect(),
            denom: (0..order).map(|i| h * (1 + i) as f64).collect(),
            d: self.d[..order + 1].to_vec(),
        }
    }
}

/// SciPy's `solve_bdf_system`: (converged, n_iter, y, d)
#[allow(clippy::too_many_arguments)]
fn solve_bdf_system(
    f: &mut Rhs<'_>,
    t_new: f64,
    y_predict: &[f64],
    c: f64,
    psi: &[f64],
    lu: &RealLu,
    scale: &[f64],
    tol: f64,
) -> (bool, usize, Vec<f64>, Vec<f64>) {
    let n = y_predict.len();
    let mut d = vec![0.0; n];
    let mut y = y_predict.to_vec();
    let mut dy_norm_old: Option<f64> = None;
    let mut converged = false;
    let mut iters = 0;
    for k in 0..BDF_NEWTON_MAXITER {
        iters = k + 1;
        let fy = call(f, t_new, &y);
        if fy.iter().any(|v| !v.is_finite()) {
            break;
        }
        let mut dy: Vec<f64> = (0..n).map(|i| c * fy[i] - psi[i] - d[i]).collect();
        lu.solve(&mut dy);
        let dy_norm = norm(&(0..n).map(|i| dy[i] / scale[i]).collect::<Vec<_>>());
        let rate = dy_norm_old.map(|o| dy_norm / o);
        if let Some(r) = rate {
            if r >= 1.0 || r.powf((BDF_NEWTON_MAXITER - k) as f64) / (1.0 - r) * dy_norm > tol {
                break;
            }
        }
        for i in 0..n {
            y[i] += dy[i];
            d[i] += dy[i];
        }
        if dy_norm == 0.0 || rate.map(|r| r / (1.0 - r) * dy_norm < tol).unwrap_or(false) {
            converged = true;
            break;
        }
        dy_norm_old = Some(dy_norm);
    }
    (converged, iters, y, d)
}

// ------------------------------------------------------------------------------ v1's wrapper

enum Stepper {
    Radau(Box<Radau>),
    Bdf(Box<Bdf>),
}

impl Stepper {
    fn t(&self) -> f64 {
        match self {
            Stepper::Radau(s) => s.t,
            Stepper::Bdf(s) => s.t,
        }
    }
    fn y(&self) -> &[f64] {
        match self {
            Stepper::Radau(s) => &s.y,
            Stepper::Bdf(s) => &s.y,
        }
    }
    fn t_old(&self) -> Option<f64> {
        match self {
            Stepper::Radau(s) => s.t_old,
            Stepper::Bdf(s) => s.t_old,
        }
    }
    fn set_atol(&mut self, a: Vec<f64>) {
        match self {
            Stepper::Radau(s) => s.atol = a,
            Stepper::Bdf(s) => s.atol = a,
        }
    }
    fn direction(&self) -> f64 {
        match self {
            Stepper::Radau(s) => s.direction,
            Stepper::Bdf(s) => s.direction,
        }
    }
    fn t_bound(&self) -> f64 {
        match self {
            Stepper::Radau(s) => s.t_bound,
            Stepper::Bdf(s) => s.t_bound,
        }
    }
    /// OdeSolver.step: Ok(true) when finished
    fn step(&mut self, f: &mut Rhs<'_>) -> Result<bool, StepErr> {
        if self.t() == self.t_bound() {
            return Ok(true);
        }
        match self {
            Stepper::Radau(s) => s.step(f)?,
            Stepper::Bdf(s) => s.step(f)?,
        }
        Ok(self.direction() * (self.t() - self.t_bound()) >= 0.0)
    }
    /// the dense output of the last step
    fn dense(&self) -> Box<dyn Fn(f64) -> Vec<f64>> {
        let (t, told) = (self.t(), self.t_old().unwrap_or(self.t()));
        if t == told {
            let y = self.y().to_vec();
            return Box::new(move |_| y.clone());
        }
        match self {
            Stepper::Radau(s) => {
                let d = s.sol.clone().unwrap();
                Box::new(move |x| d.eval(x))
            }
            Stepper::Bdf(s) => {
                let d = s.dense();
                Box::new(move |x| d.eval(x))
            }
        }
    }
}

/// `solve ... using radau|bdf`: y' = f(t, y) from t0 to t1 (either direction) with an implicit
/// method; v1's `stiff_solve` (fermium/runtime/stiff.py) over native steppers. Returns the
/// accepted steps with y and y' there (Radau: f at the new point from the stepper; BDF: f
/// re-evaluated, as v1), ending at the event if `ev` is given.
pub fn stiff_solve<F: FnMut(f64, &[f64], &mut [f64])>(
    mut f: F,
    y0: &[f64],
    t0: f64,
    t1: f64,
    method: StiffMethod,
    mut ev: Option<EventFn<'_>>,
    opts: OdeOpts<'_>,
) -> Result<Sol, Fail> {
    let f: &mut Rhs<'_> = &mut f;
    let n = y0.len();
    let (rtol, tname) = (opts.rtol, opts.tname);
    let span = t1 - t0;
    if !(span != 0.0) {
        return Err(Fail::new(err::ODE_RANGE, t0, tname));
    }
    let k0 = call(f, t0, y0);
    if !super::ode::all_finite(&k0) {
        return Err(Fail::new(err::ODE_NAN, t0, tname));
    }
    if !super::ode::all_finite(y0) {
        return Err(Fail::new(err::ODE_H, t0, tname));
    }
    // tolerance mapping (D17, D42): see stiff.py
    let sizes: Vec<f64> = (0..n).map(|j| pymax(y0[j].abs(), k0[j].abs() * span.abs())).collect();
    let known: Vec<f64> = sizes.iter().copied().filter(|&s| 0.0 < s && s < f64::INFINITY).collect();
    let fallback = known.iter().copied().fold(None, |m: Option<f64>, s| Some(m.map_or(s, |m| pymin(m, s)))).unwrap_or(1.0);
    let mut atol: Vec<f64> =
        sizes.iter().map(|&s| rtol * 1e-6 * if 0.0 < s && s < f64::INFINITY { s } else { fallback }).collect();
    let user = opts.atol;
    if let Some(u) = user {
        for j in 0..n {
            atol[j] = atol[j].max(u[j]);
        }
    }
    let srtol = pymax(rtol, 1e-13);
    let fail_kind = |ys: &[f64]| step_small_kind(y0, ys);
    let mut solver = match method {
        StiffMethod::Radau => Radau::new(f, t0, y0, t1, srtol, atol).map(|s| Stepper::Radau(Box::new(s))),
        StiffMethod::Bdf => Bdf::new(f, t0, y0, t1, srtol, atol).map(|s| Stepper::Bdf(Box::new(s))),
    }
    .map_err(|_| Fail::new(fail_kind(y0), t0, tname))?;
    let mut sol = Sol::new(n);
    sol.push(t0, y0, &k0);
    let (mut sgn, mut gprev) = (0.0f64, 0.0f64);
    if let Some(g) = ev.as_mut() {
        gprev = g(t0, y0);
        sgn = if gprev > 0.0 { 1.0 } else if gprev < 0.0 { -1.0 } else { 0.0 };
    }
    let mut steps: u64 = 0;
    let mut finished = false;
    while !finished {
        let told = solver.t();
        let last_y: Vec<f64> = sol.y[sol.y.len() - n..].to_vec();
        finished = match solver.step(f) {
            Ok(done) => done,
            Err(StepErr::TooSmall) | Err(StepErr::NonFinite) => return Err(Fail::new(fail_kind(&last_y), told, tname)),
        };
        steps += 1;
        if steps > MAX_STEPS {
            return Err(Fail::new(err::STIFF_STEPS_FROM + 1 + tname as i64, solver.t(), t0));
        }
        let t = solver.t();
        let y = solver.y().to_vec();
        // the floor follows the step's own change (D17)
        let mut na: Vec<f64> = (0..n).map(|j| pymax(rtol * (y[j] - last_y[j]).abs(), 1e-300)).collect();
        if let Some(u) = user {
            for j in 0..n {
                na[j] = na[j].max(u[j]);
            }
        }
        solver.set_atol(na);
        if !super::ode::all_finite(&y) {
            return Err(Fail::new(err::ODE_H, told, tname));
        }
        if let Some(g) = ev.as_mut() {
            let gn = g(t, &y);
            if gn == gn {
                if sgn == 0.0 {
                    sgn = if gn > 0.0 { 1.0 } else if gn < 0.0 { -1.0 } else { 0.0 };
                } else if gn == 0.0 || gn * sgn < 0.0 {
                    let dense = solver.dense();
                    let mut te = t;
                    if gn != 0.0 {
                        let ga = if gprev * sgn > 0.0 { gprev } else { sgn };
                        te = illinois(|x| g(x, &dense(x)), told, ga, t, gn);
                    }
                    let ye = if te == t { y.clone() } else { dense(te) };
                    let ke = call(f, te, &ye);
                    sol.push(te, &ye, &ke);
                    return Ok(sol);
                }
                gprev = gn;
            }
        }
        let dy = match &solver {
            Stepper::Radau(s) => s.f.clone(),
            Stepper::Bdf(_) => call(f, t, &y),
        };
        sol.push(t, &y, &dy);
    }
    if ev.is_some() {
        return Err(Fail::new(err::NO_EVENT, t1, opts.evtext));
    }
    Ok(sol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robertson_like_decay() {
        for m in [StiffMethod::Radau, StiffMethod::Bdf] {
            let sol = stiff_solve(
                |_t, y, o| {
                    o[0] = -1000.0 * y[0];
                    o[1] = 1000.0 * y[0] - y[1];
                },
                &[1.0, 0.0],
                0.0,
                5.0,
                m,
                None,
                OdeOpts { rtol: 1e-8, ..Default::default() },
            )
            .unwrap();
            let y1 = *sol.y.last().unwrap();
            let exact = 1000.0 / 999.0 * ((-5f64).exp() - (-5000f64).exp());
            assert!((y1 / exact - 1.0).abs() < 1e-6, "{m:?}: {y1} vs {exact}");
        }
    }
}
