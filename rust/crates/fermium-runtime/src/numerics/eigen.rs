//! Bound states of a linear second-order equation, `solve -ħ²/(2m) ψ'' + V ψ = E ψ with ψ(a) = 0,
//! ψ(b) = 0 for x from a to b lowest N` (D82): a port of v1's `fermium/runtime/eigen.py`, with the
//! library calls replaced natively:
//! * SciPy's `eigh_tridiagonal(select='i')` (LAPACK stebz + stein) → [`tridiag_lowest`]: Sturm-sequence
//!   bisection for the N lowest eigenvalues and inverse iteration for their vectors;
//! * `solve_banded((2, 2), …)` (LAPACK gbsv) → [`band_solve`]: banded LU with partial pivoting;
//! * `brentq` → [`super::roots::brentq`]; the 2-column SVD of the degenerate-pair fix → the 2×2
//!   eigenproblem of its Gram matrix.
//!
//! The equation is given as its right side ψ'' = rhs(x, ψ, ψ', E) (the checker's lambda).

use super::roots::brentq;

/// An eigenvalue-problem failure: v1's message, and a singular point (SI) when there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct EigenFail {
    pub message: String,
    pub x: Option<f64>,
}

impl EigenFail {
    fn new(m: impl Into<String>) -> Self {
        EigenFail { message: m.into(), x: None }
    }
}

/// v1's `singular_text`
pub fn singular_text(name: &str, where_: &str) -> String {
    format!("the equation can't be evaluated at {name} = {where_}: NaN or infinite; move the range's end away from a singular point")
}

/// The solution of an eigenvalue problem.
#[derive(Debug, Clone)]
pub struct EigenResult {
    /// the grid (2·grid intervals)
    pub xs: Vec<f64>,
    /// per state: ψ, ψ', ψ'' at the grid points (normalised, first lobe positive)
    pub psi: Vec<Vec<f64>>,
    pub dpsi: Vec<Vec<f64>>,
    pub ddpsi: Vec<Vec<f64>>,
    pub energies: Vec<f64>,
    /// nearly degenerate pairs that could not be separated by symmetry (v1's warn(k, rel), D233)
    pub warnings: Vec<(usize, f64)>,
}

/// levels closer than this (relative to the largest |E|) count as nearly degenerate (D233)
pub const DEGENERATE_REL: f64 = 1e-8;

type Rhs<'a> = dyn FnMut(f64, f64, f64, f64) -> f64 + 'a;

fn coefficients(rhs: &mut Rhs<'_>, xs: &[f64]) -> Result<(Vec<f64>, Vec<f64>), EigenFail> {
    let n = xs.len();
    let mut alpha = vec![0.0; n];
    let mut w = vec![0.0; n];
    let (mut worst_beta, mut worst_lin) = (0.0f64, 0.0f64);
    let h = if n > 1 { (xs[1] - xs[0]).abs() } else { 1.0 };
    for i in 1..n.saturating_sub(1) {
        let x = xs[i];
        let a0 = rhs(x, 1.0, 0.0, 0.0);
        let a1 = rhs(x, 1.0, 0.0, 1.0);
        alpha[i] = a0;
        w[i] = a0 - a1;
        if i % 97 == 1 || i == n - 2 {
            let b0 = rhs(x, 0.0, 1.0, 0.0);
            let b1 = rhs(x, 0.0, 1.0, 1.0);
            let c = rhs(x, 2.0, 0.0, 3.0);
            worst_lin = worst_lin.max((c - (2.0 * a0 - 6.0 * w[i])).abs() / ((2.0 * a0).abs() + (6.0 * w[i]).abs() + 1e-300));
            worst_beta = worst_beta.max((b0.abs() + b1.abs()) * h);
        }
    }
    if n > 2 {
        alpha[0] = alpha[1];
        w[0] = w[1];
        alpha[n - 1] = alpha[n - 2];
        w[n - 1] = w[n - 2];
    }
    if let Some(bad) = (0..n).find(|&i| !(alpha[i].is_finite() && w[i].is_finite())) {
        let mut e = EigenFail::new(singular_text("x", &format!("{} (SI units)", fmt_g(xs[bad]))));
        e.x = Some(xs[bad]);
        return Err(e);
    }
    if worst_lin > 1e-6 {
        return Err(EigenFail::new(
            "an eigenvalue problem must be linear: every term has one factor ψ, ψ' or ψ'' (like -ħ²/(2m) ψ'' + V(x) ψ = E ψ)",
        ));
    }
    if worst_beta > 1e-9 {
        return Err(EigenFail::new(
            "a term with ψ' isn't supported in eigenvalue problems yet: write the equation with ψ'' and ψ only (for a radial equation use u = r R, which removes the R' term)",
        ));
    }
    let all_pos = w.iter().all(|&v| v > 0.0);
    let all_neg = w.iter().all(|&v| v < 0.0);
    if !all_pos && !all_neg {
        return Err(EigenFail::new(
            "the eigenvalue must appear as E ψ with a coefficient of the same sign everywhere (like -ħ²/(2m) ψ'' + V(x) ψ = E ψ)",
        ));
    }
    if all_neg {
        return Err(EigenFail::new(
            "this equation has no lowest eigenvalues (they go down without end): check the sign of the ψ'' term; the standard form is -ħ²/(2m) ψ'' + V(x) ψ = E ψ",
        ));
    }
    Ok((alpha, w))
}

/// Python's `f"{x:g}"`
fn fmt_g(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return format!("{x}");
    }
    let e = x.abs().log10().floor() as i32;
    if (-4..6).contains(&e) {
        let s = format!("{:.*}", (5 - e).max(0) as usize, x);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        s
    } else {
        let s = format!("{:.5e}", x);
        let (m, ex) = s.split_once('e').unwrap();
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        let exn: i32 = ex.parse().unwrap();
        format!("{m}e{}{:02}", if exn < 0 { '-' } else { '+' }, exn.abs())
    }
}

type Jump = (f64, (f64, f64), (f64, f64));

fn find_jumps(rhs: &mut Rhs<'_>, xs: &[f64], alpha: &[f64], w: &[f64]) -> Vec<Jump> {
    let mut out: Vec<Jump> = Vec::new();
    for arr in [alpha, w] {
        let d: Vec<f64> = arr.windows(2).map(|p| (p[1] - p[0]).abs()).collect();
        let scale = arr.iter().fold(0.0f64, |m, v| m.max(v.abs())) + 1e-300;
        for j in 0..d.len() {
            let nb = pymax2(if j > 0 { d[j - 1] } else { 0.0 }, if j + 1 < d.len() { d[j + 1] } else { 0.0 });
            if d[j] > 1e-9 * scale && d[j] > 50.0 * nb && !out.iter().any(|(xj, _, _)| xs[j] <= *xj && *xj <= xs[j + 1]) {
                let (mut lo, mut hi) = (xs[j], xs[j + 1]);
                let left = (alpha[j], w[j]);
                let right = (alpha[j + 1], w[j + 1]);
                for _ in 0..80 {
                    let mid = 0.5 * (lo + hi);
                    if mid <= lo || mid >= hi {
                        break;
                    }
                    let a0 = rhs(mid, 1.0, 0.0, 0.0);
                    let l = (a0 - left.0).abs() + (a0 - rhs(mid, 1.0, 0.0, 1.0) - left.1).abs();
                    let r = (a0 - right.0).abs() + (a0 - rhs(mid, 1.0, 0.0, 1.0) - right.1).abs();
                    if l <= r {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                out.push((0.5 * (lo + hi), left, right));
            }
        }
    }
    out
}

#[inline]
fn pymax2(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

fn on_grid(alpha: &[f64], w: &[f64], xs: &[f64], jumps: &[Jump], stride: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a: Vec<f64> = alpha.iter().step_by(stride).copied().collect();
    let mut q: Vec<f64> = w.iter().step_by(stride).copied().collect();
    let x: Vec<f64> = xs.iter().step_by(stride).copied().collect();
    let h = x[1] - x[0];
    for (xj, left, right) in jumps {
        let i = ((xj - x[0]) / h).round_ties_even() as i64;
        let i = i.clamp(0, x.len() as i64 - 1) as usize;
        let f = ((xj - (x[i] - 0.5 * h)) / h).clamp(0.0, 1.0);
        a[i] = f * left.0 + (1.0 - f) * right.0;
        q[i] = f * left.1 + (1.0 - f) * right.1;
    }
    (a, q)
}

// ------------------------------------------------------------------------------ tridiagonal

/// The number of eigenvalues of the symmetric tridiagonal (d, e) below x (Sturm sequence).
fn sturm_count(d: &[f64], e2: &[f64], x: f64, pivmin: f64) -> usize {
    let mut count = 0;
    let mut q = d[0] - x;
    if q.abs() < pivmin {
        q = -pivmin;
    }
    if q < 0.0 {
        count += 1;
    }
    for i in 1..d.len() {
        q = d[i] - x - e2[i - 1] / q;
        if q.abs() < pivmin {
            q = -pivmin;
        }
        if q < 0.0 {
            count += 1;
        }
    }
    count
}

/// The `n` lowest eigenvalues (ascending) of the symmetric tridiagonal matrix with diagonal d and
/// off-diagonal e, by bisection to rounding level, and, if asked, their unit eigenvectors by inverse
/// iteration (column k of the returned row-major m×n matrix). The native replacement of SciPy's
/// `eigh_tridiagonal(d, e, select='i', select_range=(0, n-1))`.
pub fn tridiag_lowest(d: &[f64], e: &[f64], n: usize, vectors: bool) -> (Vec<f64>, Option<Vec<f64>>) {
    let m = d.len();
    let e2: Vec<f64> = e.iter().map(|v| v * v).collect();
    let mut tnorm = 0.0f64;
    let (mut gl, mut gu) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..m {
        let r = if i > 0 { e[i - 1].abs() } else { 0.0 } + if i + 1 < m { e[i].abs() } else { 0.0 };
        gl = gl.min(d[i] - r);
        gu = gu.max(d[i] + r);
        tnorm = tnorm.max(d[i].abs() + r);
    }
    let pivmin = f64::MIN_POSITIVE * e2.iter().fold(1.0f64, |a, &b| a.max(b));
    let width = (gu - gl).max(tnorm * f64::EPSILON);
    gl -= 2.0 * f64::EPSILON * tnorm * m as f64 + 2.0 * pivmin + 1e-300 * width;
    gu += 2.0 * f64::EPSILON * tnorm * m as f64 + 2.0 * pivmin;
    let mut vals = Vec::with_capacity(n);
    let mut lo_bound = gl;
    for k in 0..n {
        // the (k+1)-th eigenvalue: count(lo) <= k < count(hi)
        let (mut lo, mut hi) = (lo_bound, gu);
        for _ in 0..2000 {
            let mid = 0.5 * (lo + hi);
            if mid <= lo || mid >= hi {
                break;
            }
            if (hi - lo) <= 2.0 * f64::EPSILON * lo.abs().max(hi.abs()) {
                break;
            }
            if sturm_count(d, &e2, mid, pivmin) > k {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let v = 0.5 * (lo + hi);
        vals.push(v);
        lo_bound = lo;
    }
    if !vectors {
        return (vals, None);
    }
    let mut vecs = vec![0.0; m * n];
    let mut done: Vec<Vec<f64>> = Vec::with_capacity(n);
    let ortol = 1e-3 * tnorm;
    for k in 0..n {
        let lam = vals[k];
        // cluster: earlier eigenvalues within ortol get Gram–Schmidt (as LAPACK's stein)
        let cluster: Vec<usize> = (0..k).filter(|&j| (vals[j] - lam).abs() <= ortol).collect();
        let x = inverse_iteration(d, e, lam, tnorm, k, &cluster.iter().map(|&j| &done[j][..]).collect::<Vec<_>>());
        for i in 0..m {
            vecs[i * n + k] = x[i];
        }
        done.push(x);
    }
    (vals, Some(vecs))
}

/// Inverse iteration for the eigenvector of (d, e) at lam (tridiagonal LU with partial pivoting as
/// LAPACK's gttrf/gttrs), orthogonalised against the vectors in `orth`.
fn inverse_iteration(d: &[f64], e: &[f64], lam: f64, tnorm: f64, seed: usize, orth: &[&[f64]]) -> Vec<f64> {
    let n = d.len();
    let tiny = f64::EPSILON * tnorm.max(f64::MIN_POSITIVE);
    let mut dd: Vec<f64> = d.iter().map(|&v| v - lam).collect();
    let mut dl: Vec<f64> = e.to_vec();
    let mut du: Vec<f64> = e.to_vec();
    let mut du2 = vec![0.0; n.saturating_sub(2)];
    let mut swap = vec![false; n];
    for i in 0..n.saturating_sub(1) {
        if dd[i].abs() >= dl[i].abs() {
            if dd[i] != 0.0 {
                let fact = dl[i] / dd[i];
                dl[i] = fact;
                dd[i + 1] -= fact * du[i];
            }
        } else {
            let fact = dd[i] / dl[i];
            dd[i] = dl[i];
            dl[i] = fact;
            let temp = du[i];
            du[i] = dd[i + 1];
            dd[i + 1] = temp - fact * dd[i + 1];
            if i + 2 < n {
                du2[i] = du[i + 1];
                du[i + 1] = -fact * du[i + 1];
            }
            swap[i] = true;
        }
    }
    for v in dd.iter_mut() {
        if *v == 0.0 {
            *v = tiny;
        }
    }
    let solve = |b: &mut Vec<f64>| {
        for i in 0..n.saturating_sub(1) {
            if !swap[i] {
                let v = b[i];
                b[i + 1] -= dl[i] * v;
            } else {
                let temp = b[i];
                b[i] = b[i + 1];
                b[i + 1] = temp - dl[i] * b[i];
            }
        }
        let mut i = n;
        while i > 0 {
            i -= 1;
            let mut s = b[i];
            if i + 1 < n {
                s -= du[i] * b[i + 1];
            }
            if i + 2 < n {
                s -= du2[i] * b[i + 2];
            }
            b[i] = s / dd[i];
        }
    };
    // a deterministic "random" start
    let mut x: Vec<f64> = (0..n).map(|i| ((i as f64 + 1.0) * 0.618033988749895 + seed as f64 * 0.414213562).fract() - 0.5).collect();
    let orthonormalise = |x: &mut Vec<f64>| {
        for v in orth {
            let p: f64 = x.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
            for i in 0..n {
                x[i] -= p * v[i];
            }
        }
        let nrm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        if nrm > 0.0 && nrm.is_finite() {
            for v in x.iter_mut() {
                *v /= nrm;
            }
        }
    };
    for _ in 0..4 {
        orthonormalise(&mut x);
        solve(&mut x);
    }
    orthonormalise(&mut x);
    x
}

fn matrix(alpha: &[f64], w: &[f64], h: f64, nstates: usize, vectors: bool) -> Result<(Vec<f64>, Option<Vec<f64>>), EigenFail> {
    let ai = &alpha[1..alpha.len() - 1];
    let wi = &w[1..w.len() - 1];
    let m = ai.len();
    if m < nstates + 2 {
        return Err(EigenFail::new(format!("the grid is too coarse for {nstates} states")));
    }
    let d: Vec<f64> = (0..m).map(|i| (2.0 / (h * h) + ai[i]) / wi[i]).collect();
    let e: Vec<f64> = (0..m - 1).map(|i| -1.0 / (h * h * (wi[i] * wi[i + 1]).sqrt())).collect();
    let (vals, vecs) = tridiag_lowest(&d, &e, nstates, vectors);
    let vecs = vecs.map(|mut v| {
        for i in 0..m {
            let s = wi[i].sqrt();
            for k in 0..nstates {
                v[i * nstates + k] /= s;
            }
        }
        v
    });
    Ok((vals, vecs))
}

// ------------------------------------------------------------------------------ Numerov

fn start_limit(f1: f64, f2: f64, h: f64) -> f64 {
    2.0 * f1 * h - f2 * 2.0 * h
}

/// ψ'' = f ψ from one end (ψ = 0, next point h): values (rescaled) and sign changes.
fn numerov(f: &[f64], h: f64, rev: bool) -> (Vec<f64>, usize) {
    let n = f.len();
    let order: Vec<usize> = if rev { (0..n).rev().collect() } else { (0..n).collect() };
    let c = h * h / 12.0;
    let hh = h * h;
    let mut ps = vec![0.0; n];
    let i1 = order[1];
    ps[i1] = h;
    let y_prev = if n > 2 { -c * start_limit(f[i1], f[order[2]], h) } else { 0.0 };
    let mut y = (1.0 - c * f[i1]) * h;
    let mut d = y - y_prev;
    let mut nodes = 0;
    for j in 2..n {
        let (i1, i2) = (order[j - 1], order[j]);
        d += hh * f[i1] * ps[i1];
        y += d;
        let den = 1.0 - c * f[i2];
        let v = if den.is_finite() && den != 0.0 { y / den } else { y };
        ps[i2] = v;
        if v.abs() > 1e150 {
            for &k in &order[..=j] {
                ps[k] *= 1e-150;
            }
            y *= 1e-150;
            d *= 1e-150;
        }
        if (ps[i2] < 0.0) != (ps[i1] < 0.0) && ps[i1] != 0.0 && ps[i2] != 0.0 {
            nodes += 1;
        }
    }
    (ps, nodes)
}

fn numerov_end(al: &[f64], wl: &[f64], e: f64, h: f64) -> (f64, usize) {
    let c = h * h / 12.0;
    let hh = h * h;
    let f: Vec<f64> = al.iter().zip(wl).map(|(a, q)| a - q * e).collect();
    let mut p = h;
    let mut y = (1.0 - c * f[1]) * h;
    let mut d = if f.len() > 2 { y + c * start_limit(f[1], f[2], h) } else { y };
    let mut nodes = 0;
    let last = f.len() - 1;
    for i in 2..=last {
        d += hh * f[i - 1] * p;
        y += d;
        let mut p2 = if i == last { y } else { y / (1.0 - c * f[i]) };
        if p2.abs() > 1e150 {
            p2 *= 1e-150;
            y *= 1e-150;
            d *= 1e-150;
        }
        if p2 != 0.0 && p != 0.0 && (p2 < 0.0) != (p < 0.0) {
            nodes += 1;
        }
        p = p2;
    }
    (p, nodes)
}

fn shoot(alpha: &[f64], w: &[f64], h: f64, nstates: usize) -> Result<Vec<f64>, EigenFail> {
    let mut cache: std::collections::HashMap<u64, (f64, usize)> = std::collections::HashMap::new();
    let mut run = |e: f64| -> (f64, usize) { *cache.entry(e.to_bits()).or_insert_with(|| numerov_end(alpha, w, e, h)) };
    let lo = alpha.iter().zip(w).map(|(a, q)| a / q).fold(f64::INFINITY, |m, v| if v < m { v } else { m });
    let span = h * (alpha.len() - 1) as f64;
    let wmax = w.iter().fold(f64::NEG_INFINITY, |m, &v| if v > m { v } else { m });
    let step = (std::f64::consts::PI / span).powf(2.0) / wmax;
    let mut hi = lo + step;
    let mut ok = false;
    for _ in 0..200 {
        if run(hi).1 >= nstates {
            ok = true;
            break;
        }
        hi = lo + (hi - lo) * 2.0;
    }
    if !ok {
        return Err(EigenFail::new("the shooting method couldn't bracket the states"));
    }
    let mut out = Vec::new();
    let mut a = lo;
    for k in 0..nstates {
        let (mut p, mut q) = (a, hi);
        while run(q).1 > k + 1 && q - p > 1e-15 * q.abs() {
            let mid = 0.5 * (p + q);
            if run(mid).1 > k {
                q = mid;
            } else {
                p = mid;
            }
        }
        while run(p).1 < k && q - p > 1e-15 * q.abs() {
            let mid = 0.5 * (p + q);
            if run(mid).1 < k + 1 {
                p = mid;
            } else {
                q = mid;
            }
        }
        let (fp, fq) = (run(p).0, run(q).0);
        let e = if fp == 0.0 {
            p
        } else if fq == 0.0 {
            q
        } else if (fp < 0.0) == (fq < 0.0) {
            return Err(EigenFail::new(format!("the shooting method lost state {}; try using matrix, or a finer grid", k + 1)));
        } else {
            brentq(|e| run(e).0, p, q, 1e-300, 1e-15, 400)
                .map_err(|_| EigenFail::new(format!("the shooting method lost state {}; try using matrix, or a finer grid", k + 1)))?
        };
        out.push(e);
        a = e;
    }
    Ok(out)
}

fn shoot_vector(alpha: &[f64], w: &[f64], h: f64, e: f64) -> Vec<f64> {
    let n = alpha.len();
    let f: Vec<f64> = alpha.iter().zip(w).map(|(a, q)| a - q * e).collect();
    let (left, _) = numerov(&f, h, false);
    let (right, _) = numerov(&f, h, true);
    let allowed: Vec<usize> = (1..n - 1).filter(|&i| f[i] < 0.0).map(|i| i - 1).collect();
    let mut m = if let Some(&l) = allowed.last() { l + 1 } else { n / 2 };
    let lo = (m as i64 - (n / 4) as i64).max(1) as usize;
    let seg_max = left[lo..=m].iter().fold(0.0f64, |a, v| a.max(v.abs())) + 1e-300;
    if let Some(g) = (lo..=m).rev().find(|&i| left[i].abs() / seg_max > 0.3) {
        m = g;
    }
    if right[m] == 0.0 || left[m] == 0.0 {
        return left;
    }
    let mut psi = left.clone();
    let r = left[m] / right[m];
    for i in m..n {
        psi[i] = right[i] * r;
    }
    psi
}

fn derivative4(psi: &[f64], h: f64) -> Vec<f64> {
    let n = psi.len();
    let mut d = vec![0.0; n];
    if n < 7 {
        // np.gradient(psi, h, edge_order=2)
        for i in 1..n - 1 {
            d[i] = (psi[i + 1] - psi[i - 1]) / (2.0 * h);
        }
        d[0] = (-1.5 * psi[0] + 2.0 * psi[1] - 0.5 * psi[2]) / h;
        d[n - 1] = (0.5 * psi[n - 3] - 2.0 * psi[n - 2] + 1.5 * psi[n - 1]) / h;
        return d;
    }
    for i in 2..n - 2 {
        d[i] = (psi[i - 2] - 8.0 * psi[i - 1] + 8.0 * psi[i + 1] - psi[i + 2]) / (12.0 * h);
    }
    let p = psi;
    d[0] = (-25.0 * p[0] + 48.0 * p[1] - 36.0 * p[2] + 16.0 * p[3] - 3.0 * p[4]) / (12.0 * h);
    d[1] = (-3.0 * p[0] - 10.0 * p[1] + 18.0 * p[2] - 6.0 * p[3] + p[4]) / (12.0 * h);
    d[n - 1] = (25.0 * p[n - 1] - 48.0 * p[n - 2] + 36.0 * p[n - 3] - 16.0 * p[n - 4] + 3.0 * p[n - 5]) / (12.0 * h);
    d[n - 2] = (3.0 * p[n - 1] + 10.0 * p[n - 2] - 18.0 * p[n - 3] + 6.0 * p[n - 4] - p[n - 5]) / (12.0 * h);
    d
}

fn hermite_norm2(p: &[f64], d: &[f64], h: f64) -> f64 {
    let g = [-0.8611363115940526, -0.3399810435848563, 0.3399810435848563, 0.8611363115940526];
    let gw = [0.3478548451374538, 0.6521451548625461, 0.6521451548625461, 0.3478548451374538];
    let mut basis = [[0.0f64; 4]; 4];
    for (k, &gk) in g.iter().enumerate() {
        let t: f64 = 0.5 * (gk + 1.0);
        let (t2, t3) = (t * t, t.powf(3.0));
        basis[k] = [2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, -2.0 * t3 + 3.0 * t2, t3 - t2];
    }
    let mut s = 0.0;
    for i in 0..p.len() - 1 {
        let mut row = 0.0;
        for k in 0..4 {
            let b = basis[k];
            let v = p[i] * b[0] + h * d[i] * b[1] + p[i + 1] * b[2] + h * d[i + 1] * b[3];
            row += v * v * gw[k];
        }
        s += row;
    }
    0.5 * h * s
}

fn finish(h: f64, psi: &[f64], f: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let dpsi = derivative4(psi, h);
    let nrm = hermite_norm2(psi, &dpsi, h).sqrt();
    let mut p: Vec<f64> = psi.iter().map(|v| v / nrm).collect();
    let mut dp: Vec<f64> = dpsi.iter().map(|v| v / nrm).collect();
    let mx = p.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    if let Some(i) = p.iter().position(|v| v.abs() > 1e-3 * mx) {
        if p[i] < 0.0 {
            p.iter_mut().for_each(|v| *v = -*v);
            dp.iter_mut().for_each(|v| *v = -*v);
        }
    }
    let ddp: Vec<f64> = f.iter().zip(&p).map(|(a, b)| a * b).collect();
    (p, dp, ddp)
}

/// Solve the banded system A x = b (A given by `get(i, j)` for |i − j| within the bands) by LU
/// with partial pivoting: the native `solve_banded((kl, ku), …)`. None if singular.
pub fn band_solve(n: usize, kl: usize, ku: usize, get: &dyn Fn(usize, usize) -> f64, b: &[f64]) -> Option<Vec<f64>> {
    // rows stored densely over columns [i - kl, i + ku + kl] (fill-in from pivoting)
    let wdt = 2 * kl + ku + 1;
    let off = |i: usize| i as i64 - kl as i64; // column of slot 0 in row i
    let mut rows: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..wdt)
                .map(|s| {
                    let j = off(i) + s as i64;
                    if j >= 0 && (j as usize) < n && (j - i as i64).abs() as usize <= kl.max(ku) && j - (i as i64) <= ku as i64 && (i as i64) - j <= kl as i64 {
                        get(i, j as usize)
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();
    let mut x = b.to_vec();
    let at = |rows: &Vec<Vec<f64>>, i: usize, j: usize| -> f64 {
        let s = j as i64 - off(i);
        if s < 0 || s as usize >= wdt { 0.0 } else { rows[i][s as usize] }
    };
    for k in 0..n {
        // pivot among rows k..=k+kl
        let last = (k + kl).min(n - 1);
        let mut p = k;
        let mut best = at(&rows, k, k).abs();
        for i in k + 1..=last {
            let v = at(&rows, i, k).abs();
            if v > best {
                best = v;
                p = i;
            }
        }
        if best == 0.0 || !best.is_finite() {
            return None;
        }
        if p != k {
            // swap rows k and p over columns k..=k+ku+kl
            let hi = (k + ku + kl).min(n - 1);
            for j in k..=hi {
                let (a, bb) = (at(&rows, k, j), at(&rows, p, j));
                let sk = (j as i64 - off(k)) as usize;
                let sp = j as i64 - off(p);
                rows[k][sk] = bb;
                if sp >= 0 && (sp as usize) < wdt {
                    rows[p][sp as usize] = a;
                }
            }
            x.swap(k, p);
        }
        let piv = at(&rows, k, k);
        let hi = (k + ku + kl).min(n - 1);
        for i in k + 1..=last {
            let lik = at(&rows, i, k) / piv;
            if lik == 0.0 {
                continue;
            }
            for j in k..=hi {
                let sj = j as i64 - off(i);
                if sj >= 0 && (sj as usize) < wdt {
                    let v = at(&rows, k, j);
                    rows[i][sj as usize] -= lik * v;
                }
            }
            let xk = x[k];
            x[i] -= lik * xk;
        }
    }
    for k in (0..n).rev() {
        let hi = (k + ku + kl).min(n - 1);
        let mut s = x[k];
        for j in k + 1..=hi {
            s -= at(&rows, k, j) * x[j];
        }
        x[k] = s / at(&rows, k, k);
    }
    Some(x)
}

fn norm2(x: &[f64]) -> f64 {
    x.iter().map(|v| v * v).sum::<f64>().sqrt()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Numerov's eigenvector nearest E by inverse iteration from `start` (v1's `_numerov_vector`).
fn numerov_vector(alpha: &[f64], w: &[f64], h: f64, e: f64, start: &[f64]) -> Option<(Vec<f64>, f64)> {
    let ai = &alpha[1..alpha.len() - 1];
    let wi = &w[1..w.len() - 1];
    let m = ai.len();
    if m < 6 {
        return None;
    }
    let hh = 1.0 / (h * h);
    let coef: Vec<f64> = (0..m).map(|i| ai[i] - e * wi[i]).collect();
    // A[i][j] for |i-j| <= 2
    let get = |i: usize, j: usize| -> f64 {
        let mut v = 0.0;
        if i == j {
            v = 2.0 * hh + 10.0 * coef[i] / 12.0;
        } else if j == i + 1 {
            v = -hh + coef[j] / 12.0;
        } else if i == j + 1 {
            v = -hh + coef[j] / 12.0;
        }
        if i == 0 {
            if j == 0 {
                v += 3.0 * coef[0] / 12.0;
            } else if j == 1 {
                v += -3.0 * coef[1] / 12.0;
            } else if j == 2 {
                v += coef[2] / 12.0;
            }
        }
        if i == m - 1 {
            if j == m - 1 {
                v += 3.0 * coef[m - 1] / 12.0;
            } else if j == m - 2 {
                v += -3.0 * coef[m - 2] / 12.0;
            } else if j == m - 3 {
                v += coef[m - 3] / 12.0;
            }
        }
        v
    };
    let times = |c: &[f64], x: &[f64]| -> Vec<f64> {
        let mut y: Vec<f64> = (0..m).map(|i| 10.0 * c[i] * x[i] / 12.0).collect();
        for i in 0..m - 1 {
            y[i] += c[i + 1] * x[i + 1] / 12.0;
        }
        for i in 1..m {
            y[i] += c[i - 1] * x[i - 1] / 12.0;
        }
        y[0] += (3.0 * c[0] * x[0] - 3.0 * c[1] * x[1] + c[2] * x[2]) / 12.0;
        y[m - 1] += (3.0 * c[m - 1] * x[m - 1] - 3.0 * c[m - 2] * x[m - 2] + c[m - 3] * x[m - 3]) / 12.0;
        y
    };
    let sn = norm2(start);
    let mut x: Vec<f64> = start.iter().map(|v| v / sn).collect();
    let mut lam = e;
    let mut change = f64::INFINITY;
    for _ in 0..6 {
        let rhs = times(wi, &x);
        let mut y = band_solve(m, 2, 2, &get, &rhs)?;
        if y.iter().any(|v| !v.is_finite()) {
            return None;
        }
        lam = e + 1.0 / dot(&x, &y);
        let ny = norm2(&y);
        y.iter_mut().for_each(|v| *v /= ny);
        if dot(&y, &x) < 0.0 {
            y.iter_mut().for_each(|v| *v = -*v);
        }
        change = y.iter().zip(&x).fold(0.0f64, |a, (p, q)| a.max((p - q).abs()));
        x = y;
        if change < 1e-14 {
            break;
        }
    }
    let st: Vec<f64> = start.iter().map(|v| v / sn).collect();
    if change > 1e-10 || dot(&x, &st).abs() < 0.9 {
        return None;
    }
    Some((x, lam))
}

fn numerov_extrapolate(raw_alpha: &[f64], raw_w: &[f64], xs: &[f64], h: f64, e: f64, psi: &[f64], lam_fine: f64) -> Option<f64> {
    let mut lams = vec![lam_fine];
    for stride in [2usize, 4] {
        let (a, q) = on_grid(raw_alpha, raw_w, xs, &[], stride);
        // psi[stride:-stride:stride]
        let n = psi.len();
        let start: Vec<f64> = (stride..n - stride).step_by(stride).map(|i| psi[i]).collect();
        let r = numerov_vector(&a, &q, stride as f64 * h, e, &start)?;
        lams.push(r.1);
    }
    let (lf, lm, lc) = (lams[0], lams[1], lams[2]);
    let (d1, d2) = (lm - lc, lf - lm);
    if d2 == 0.0 {
        return Some(lf);
    }
    let ratio = d1 / d2;
    if 12.0 < ratio && ratio < 40.0 { Some(lf + d2 / (ratio - 1.0)) } else { None }
}

fn sign_changes(p: &[f64]) -> usize {
    let mx = p.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    let big: Vec<f64> = p.iter().copied().filter(|v| v.abs() > 1e-6 * mx).collect();
    big.windows(2).filter(|w| w[0].is_sign_negative() != w[1].is_sign_negative()).count()
}

fn symmetric(raw_alpha: &[f64], raw_w: &[f64]) -> bool {
    for c in [raw_alpha, raw_w] {
        if c.iter().any(|v| !v.is_finite()) {
            return false;
        }
        let mx = c.iter().fold(0.0f64, |a, v| a.max(v.abs()));
        let floor = 1e-14 * mx.max(1e-300);
        let n = c.len();
        for i in 0..n {
            let (a, m) = (c[i], c[n - 1 - i]);
            if (a - m).abs() > 1e-12 * a.abs().max(m.abs()) + floor {
                return false;
            }
        }
    }
    true
}

/// The right singular vector for the smallest singular value of the m×2 matrix [u v].
fn smallest_right_singular(u: &[f64], v: &[f64]) -> [f64; 2] {
    let (a, b, c) = (dot(u, u), dot(u, v), dot(v, v));
    // eigenvector of [[a, b], [b, c]] for the smaller eigenvalue
    let tr = 0.5 * (a + c);
    let disc = (0.25 * (a - c) * (a - c) + b * b).sqrt();
    let lmin = tr - disc;
    let (x, y) = if b.abs() > 0.0 {
        if (a - lmin).abs() > (c - lmin).abs() { (-b, a - lmin) } else { (c - lmin, -b) }
    } else if a <= c {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let n = (x * x + y * y).sqrt();
    [x / n, y / n]
}

fn near_degenerate(energies: &[f64], vecs: &mut [Vec<f64>], raw_alpha: &[f64], raw_w: &[f64], warnings: &mut Vec<(usize, f64)>) {
    let n = energies.len();
    if n < 2 {
        return;
    }
    let mut scale = energies.iter().fold(0.0f64, |a, e| a.max(e.abs()));
    if scale == 0.0 {
        scale = 1.0;
    }
    let mut sym: Option<bool> = None;
    let mut k = 0;
    while k + 1 < n {
        let split = (energies[k + 1] - energies[k]).abs();
        if split > DEGENERATE_REL * scale {
            k += 1;
            continue;
        }
        let s = *sym.get_or_insert_with(|| symmetric(raw_alpha, raw_w));
        if !s {
            warnings.push((k, split / scale));
            k += 2;
            continue;
        }
        let (p, q) = (vecs[k].clone(), vecs[k + 1].clone());
        let len = p.len();
        let mut combos: Vec<Vec<f64>> = Vec::new();
        for parity in [1.0, -1.0] {
            let o1: Vec<f64> = (0..len).map(|i| p[i] - parity * p[len - 1 - i]).collect();
            let o2: Vec<f64> = (0..len).map(|i| q[i] - parity * q[len - 1 - i]).collect();
            let c = smallest_right_singular(&o1, &o2);
            let v: Vec<f64> = (0..len).map(|i| c[0] * p[i] + c[1] * q[i]).collect();
            let v: Vec<f64> = (0..len).map(|i| 0.5 * (v[i] + parity * v[len - 1 - i])).collect();
            let nv = norm2(&v);
            if nv == 0.0 {
                break;
            }
            let np_ = norm2(&p);
            combos.push(v.iter().map(|x| x * (np_ / nv)).collect());
        }
        if combos.len() == 2 {
            combos.sort_by_key(|c| sign_changes(c));
            vecs[k + 1] = combos.pop().unwrap();
            vecs[k] = combos.pop().unwrap();
        }
        k += 2;
    }
}

/// numpy's linspace(a, b, num)
pub fn linspace(a: f64, b: f64, num: usize) -> Vec<f64> {
    let div = (num - 1) as f64;
    let step = (b - a) / div;
    let mut y: Vec<f64> = (0..num).map(|i| if step == 0.0 { i as f64 / div * (b - a) + a } else { i as f64 * step + a }).collect();
    if num > 1 {
        y[num - 1] = b;
    }
    y
}

/// Which method finds the eigenvalues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EigenMethod {
    Matrix,
    Shooting,
}

/// v1's `eigen_solve`: the `nstates` lowest states of ψ'' = rhs(x, ψ, ψ', E) with ψ(a) = ψ(b) = 0
/// on a grid of 2·grid intervals.
pub fn eigen_solve(
    rhs: &mut dyn FnMut(f64, f64, f64, f64) -> f64,
    a: f64,
    b: f64,
    nstates: usize,
    grid: usize,
    method: EigenMethod,
) -> Result<EigenResult, EigenFail> {
    if !(b > a) {
        return Err(EigenFail::new("the range of x is empty or reversed: an eigenvalue problem needs from a to b with a < b"));
    }
    let mut mm = grid;
    if mm < 8 {
        return Err(EigenFail::new("the grid needs at least 8 intervals"));
    }
    mm += mm % 2;
    let xs = linspace(a, b, 2 * mm + 1);
    let h2 = (b - a) / (2 * mm) as f64;
    let (raw_alpha, raw_w) = coefficients(rhs, &xs)?;
    let jumps = find_jumps(rhs, &xs, &raw_alpha, &raw_w);
    let (alpha, w) = on_grid(&raw_alpha, &raw_w, &xs, &jumps, 1);
    let mut energies: Vec<f64>;
    let mut vecs: Vec<Vec<f64>>;
    match method {
        EigenMethod::Shooting => {
            energies = shoot(&alpha, &w, h2, nstates)?;
            vecs = energies.iter().map(|&e| shoot_vector(&alpha, &w, h2, e)).collect();
        }
        EigenMethod::Matrix => {
            let (e_fine, v) = matrix(&alpha, &w, h2, nstates, true)?;
            let v = v.unwrap();
            let (a2, w2) = on_grid(&raw_alpha, &raw_w, &xs, &jumps, 2);
            let (e_mid, _) = matrix(&a2, &w2, 2.0 * h2, nstates, false)?;
            let (a4, w4) = on_grid(&raw_alpha, &raw_w, &xs, &jumps, 4);
            let (e_coarse, _) = matrix(&a4, &w4, 4.0 * h2, nstates, false)?;
            energies = Vec::new();
            for k in 0..nstates {
                let (ef, em, ec) = (e_fine[k], e_mid[k], e_coarse[k]);
                let (d1, d2) = (em - ec, ef - em);
                let ratio = if d2 != 0.0 { d1 / d2 } else { f64::INFINITY };
                energies.push(if 3.0 < ratio && ratio < 5.0 { (4.0 * ef - em) / 3.0 } else { ef });
            }
            let m = xs.len() - 2;
            vecs = Vec::new();
            for k in 0..nstates {
                let mut psi = vec![0.0; xs.len()];
                let col: Vec<f64> = (0..m).map(|i| v[i * nstates + k]).collect();
                let num = if jumps.is_empty() {
                    let start: Vec<f64> = (0..m).map(|i| col[i] * w[i + 1].sqrt()).collect();
                    numerov_vector(&alpha, &w, h2, energies[k], &start)
                } else {
                    None
                };
                match num {
                    None => psi[1..=m].copy_from_slice(&col),
                    Some((vec, lam)) => {
                        psi[1..=m].copy_from_slice(&vec);
                        if let Some(better) = numerov_extrapolate(&raw_alpha, &raw_w, &xs, h2, energies[k], &psi, lam) {
                            energies[k] = better;
                        }
                    }
                }
                vecs.push(psi);
            }
        }
    }
    let mut warnings = Vec::new();
    near_degenerate(&energies, &mut vecs, &raw_alpha, &raw_w, &mut warnings);
    let (mut psi_out, mut dpsi_out, mut dd_out) = (Vec::new(), Vec::new(), Vec::new());
    for (k, &e) in energies.iter().enumerate() {
        let f: Vec<f64> = raw_alpha.iter().zip(&raw_w).map(|(a, q)| a - q * e).collect();
        let (p, dp, ddp) = finish(h2, &vecs[k], &f);
        psi_out.push(p);
        dpsi_out.push(dp);
        dd_out.push(ddp);
    }
    Ok(EigenResult { xs, psi: psi_out, dpsi: dpsi_out, ddpsi: dd_out, energies, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tridiagonal_laplacian() {
        // -D² on n points: eigenvalues 2 - 2 cos(kπ/(n+1))
        let n = 50;
        let d = vec![2.0; n];
        let e = vec![-1.0; n - 1];
        let (vals, vecs) = tridiag_lowest(&d, &e, 3, true);
        for (k, v) in vals.iter().enumerate() {
            let exact = 2.0 - 2.0 * ((k + 1) as f64 * std::f64::consts::PI / (n + 1) as f64).cos();
            assert!((v - exact).abs() < 1e-14, "{v} {exact}");
        }
        let vecs = vecs.unwrap();
        // orthonormal
        let col = |k: usize| (0..n).map(|i| vecs[i * 3 + k]).collect::<Vec<f64>>();
        assert!((dot(&col(0), &col(0)) - 1.0).abs() < 1e-12);
        assert!(dot(&col(0), &col(1)).abs() < 1e-12);
    }

    #[test]
    fn band_solver() {
        let n = 7;
        let get = |i: usize, j: usize| if i == j { 4.0 } else if (i as i64 - j as i64).abs() == 1 { 1.0 } else if (i as i64 - j as i64).abs() == 2 { 0.5 } else { 0.0 };
        let x0: Vec<f64> = (0..n).map(|i| i as f64 + 1.0).collect();
        let b: Vec<f64> = (0..n).map(|i| (0..n).map(|j| get(i, j) * x0[j]).sum()).collect();
        let x = band_solve(n, 2, 2, &get, &b).unwrap();
        for i in 0..n {
            assert!((x[i] - x0[i]).abs() < 1e-13);
        }
    }

    #[test]
    fn harmonic_oscillator() {
        // -ψ''/2 + x²/2 ψ = E ψ  →  ψ'' = x² ψ - 2 E ψ; E = n + 1/2
        let mut rhs = |x: f64, p: f64, _dp: f64, e: f64| (x * x - 2.0 * e) * p;
        for m in [EigenMethod::Matrix, EigenMethod::Shooting] {
            let r = eigen_solve(&mut rhs, -10.0, 10.0, 3, 2000, m).unwrap();
            for (k, e) in r.energies.iter().enumerate() {
                assert!((e - (k as f64 + 0.5)).abs() < 1e-9, "{m:?} {k}: {e}");
            }
        }
    }
}
