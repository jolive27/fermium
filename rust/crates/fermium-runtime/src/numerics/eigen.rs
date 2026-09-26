//! Bound states of a linear second-order equation, `solve -ħ²/(2m) ψ'' + V ψ = E ψ with ψ(a) = 0,
//! ψ(b) = 0 for x from a to b lowest N` (D82): a port of v1's `fermium/runtime/eigen.py`, with the
//! library calls replaced natively:
//! * SciPy's `eigh_tridiagonal(select='i')` (LAPACK stebz + stein) → [`tridiag_lowest`]: Sturm-sequence
//!   bisection for the N lowest eigenvalues and inverse iteration for their vectors, both transcribed from
//!   LAPACK so the values and vectors are v1's to the last bit;
//! * `solve_banded((2, 2), …)` (LAPACK gbsv) → [`band_solve`]: banded LU with partial pivoting (dgbtf2/dgbtrs's rounding);
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

// ------------------------------------------------------------------------------ LAPACK dstebz
const ULP: f64 = 2.220446049250313e-16; // dlamch('P')
const SAFEMN: f64 = 2.2250738585072014e-308; // dlamch('S')
const FUDGE: f64 = 2.1;
const RELFAC: f64 = 2.0;

/// LAPACK's dlaebz (serial version; dstebz calls it with NB = 0): bisection on intervals `ab` with the
/// eigenvalue counts `nab`. ijob 1: counts at the ends; 2: refine every interval holding eigenvalues,
/// splitting it when both halves hold some; 3: binary search for the points where the count is `nval`.
/// Returns (the number of intervals, unconverged ones).
#[allow(clippy::too_many_arguments)]
fn laebz(ijob: u8, nitmax: usize, n: usize, mmax: usize, minp: usize, abstol: f64, reltol: f64, pivmin: f64,
         d: &[f64], e2: &[f64], nval: &mut [usize], ab: &mut [[f64; 2]], c: &mut [f64], nab: &mut [[i64; 2]])
         -> (usize, usize) {
    if ijob == 1 {
        let mut mout = 0i64;
        for ji in 0..minp {
            for jp in 0..2 {
                let mut tmp1 = d[0] - ab[ji][jp];
                if tmp1.abs() < pivmin {
                    tmp1 = -pivmin;
                }
                nab[ji][jp] = i64::from(tmp1 <= 0.0);
                for j in 1..n {
                    tmp1 = d[j] - e2[j - 1] / tmp1 - ab[ji][jp];
                    if tmp1.abs() < pivmin {
                        tmp1 = -pivmin;
                    }
                    if tmp1 <= 0.0 {
                        nab[ji][jp] += 1;
                    }
                }
            }
            mout += nab[ji][1] - nab[ji][0];
        }
        return (mout.max(0) as usize, 0);
    }
    let (mut kf, mut kl) = (0usize, minp - 1);
    if ijob == 2 {
        for ji in 0..minp {
            c[ji] = 0.5 * (ab[ji][0] + ab[ji][1]);
        }
    }
    for _ in 0..nitmax {
        let mut klnew = kl;
        for ji in kf..=kl {
            let tmp1 = c[ji];
            let mut tmp2 = d[0] - tmp1;
            let mut itmp1 = 0i64;
            if tmp2 <= pivmin {
                itmp1 = 1;
                tmp2 = tmp2.min(-pivmin);
            }
            for j in 1..n {
                tmp2 = d[j] - e2[j - 1] / tmp2 - tmp1;
                if tmp2 <= pivmin {
                    itmp1 += 1;
                    tmp2 = tmp2.min(-pivmin);
                }
            }
            if ijob <= 2 {
                itmp1 = nab[ji][1].min(nab[ji][0].max(itmp1));
                if itmp1 == nab[ji][1] {
                    ab[ji][1] = tmp1;
                } else if itmp1 == nab[ji][0] {
                    ab[ji][0] = tmp1;
                } else if klnew + 1 < mmax {
                    klnew += 1;
                    ab[klnew][1] = ab[ji][1];
                    nab[klnew][1] = nab[ji][1];
                    ab[klnew][0] = tmp1;
                    nab[klnew][0] = itmp1;
                    ab[ji][1] = tmp1;
                    nab[ji][1] = itmp1;
                } else {
                    return (kl + 1, mmax + 1);
                }
            } else {
                if itmp1 <= nval[ji] as i64 {
                    ab[ji][0] = tmp1;
                    nab[ji][0] = itmp1;
                }
                if itmp1 >= nval[ji] as i64 {
                    ab[ji][1] = tmp1;
                    nab[ji][1] = itmp1;
                }
            }
        }
        kl = klnew;
        let mut kfnew = kf;
        for ji in kf..=kl {
            let tmp1 = (ab[ji][1] - ab[ji][0]).abs();
            let tmp2 = ab[ji][1].abs().max(ab[ji][0].abs());
            if tmp1 < abstol.max(pivmin).max(reltol * tmp2) || nab[ji][0] >= nab[ji][1] {
                if ji > kfnew {
                    ab.swap(ji, kfnew);
                    nab.swap(ji, kfnew);
                    if ijob == 3 {
                        nval.swap(ji, kfnew);
                    }
                }
                kfnew += 1;
            }
        }
        kf = kfnew;
        for ji in kf..=kl {
            c[ji] = 0.5 * (ab[ji][0] + ab[ji][1]);
        }
        if kf > kl {
            break;
        }
    }
    (kl + 1, (kl + 1).saturating_sub(kf))
}

/// The `k` lowest eigenvalues of the symmetric tridiagonal (d, e): LAPACK's dstebz with RANGE = 'I',
/// IL = 1, IU = k and the given ABSTOL, transcribed (SciPy's `eigh_tridiagonal(select='i')` calls it), so the
/// bisection takes the same path and ends on the same midpoints (checked against SciPy on random matrices).
pub fn stebz_lowest(d: &[f64], e: &[f64], k: usize, abstol: f64) -> Vec<f64> {
    let n = d.len();
    let (il, iu) = (1usize, k);
    let rtoli = ULP * RELFAC;
    let mut work = vec![0.0; n];
    let mut pivmin = 1.0f64;
    let mut isplit = vec![];
    for j in 1..n {
        let tmp1 = e[j - 1].powi(2);
        if (d[j] * d[j - 1]).abs() * ULP.powi(2) + SAFEMN > tmp1 {
            isplit.push(j);
            work[j - 1] = 0.0;
        } else {
            work[j - 1] = tmp1;
            pivmin = pivmin.max(tmp1);
        }
    }
    isplit.push(n);
    pivmin *= SAFEMN;
    let (mut gu, mut gl) = (d[0], d[0]);
    let mut tmp1 = 0.0;
    for j in 0..n - 1 {
        let tmp2 = work[j].sqrt();
        gu = gu.max(d[j] + tmp1 + tmp2);
        gl = gl.min(d[j] - tmp1 - tmp2);
        tmp1 = tmp2;
    }
    gu = gu.max(d[n - 1] + tmp1);
    gl = gl.min(d[n - 1] - tmp1);
    let tnorm = gl.abs().max(gu.abs());
    gl = gl - FUDGE * tnorm * ULP * n as f64 - FUDGE * 2.0 * pivmin;
    gu = gu + FUDGE * tnorm * ULP * n as f64 + FUDGE * pivmin;
    let itmax = (((tnorm + pivmin).ln() - pivmin.ln()) / 2f64.ln()) as usize + 2;
    let atoli = if abstol > 0.0 { abstol } else { ULP * tnorm };
    let mut ab = [[gl, gu], [gl, gu]];
    let mut nab = [[-1i64, n as i64 + 1], [-1, n as i64 + 1]];
    let mut nval = [il - 1, iu];
    let mut c = [gl, gu];
    laebz(3, itmax, n, 2, 2, atoli, rtoli, pivmin, d, &work, &mut nval, &mut ab, &mut c, &mut nab);
    let (wl, wlu, wu, wul) = if nval[1] == iu {
        (ab[0][0], ab[0][1], ab[1][1], ab[1][0])
    } else {
        (ab[1][0], ab[1][1], ab[0][1], ab[0][0])
    };
    let mut w = vec![0.0; n];
    let mut m = 0usize;
    let mut iend = 0usize;
    let (mut nwl, mut nwu) = (0i64, 0i64);
    for &isp in &isplit {
        let ibegin = iend;
        iend = isp;
        let inn = iend - ibegin;
        if inn == 1 {
            if wl >= d[ibegin] - pivmin {
                nwl += 1;
            }
            if wu >= d[ibegin] - pivmin {
                nwu += 1;
            }
            if wl < d[ibegin] - pivmin && wu >= d[ibegin] - pivmin {
                w[m] = d[ibegin];
                m += 1;
            }
            continue;
        }
        let dd = &d[ibegin..iend];
        let ee = &e[ibegin..iend - 1];
        let e2 = &work[ibegin..iend - 1];
        let (mut gu, mut gl) = (dd[0], dd[0]);
        let mut tmp1 = 0.0;
        for j in 0..inn - 1 {
            let tmp2 = ee[j].abs();
            gu = gu.max(dd[j] + tmp1 + tmp2);
            gl = gl.min(dd[j] - tmp1 - tmp2);
            tmp1 = tmp2;
        }
        gu = gu.max(dd[inn - 1] + tmp1);
        gl = gl.min(dd[inn - 1] - tmp1);
        let bnorm = gl.abs().max(gu.abs());
        gl = gl - FUDGE * bnorm * ULP * inn as f64 - FUDGE * pivmin;
        gu = gu + FUDGE * bnorm * ULP * inn as f64 + FUDGE * pivmin;
        let atoli = if abstol > 0.0 { abstol } else { ULP * gl.abs().max(gu.abs()) };
        if gu < wl {
            nwl += inn as i64;
            nwu += inn as i64;
            continue;
        }
        gl = gl.max(wl);
        gu = gu.min(wu);
        if gl >= gu {
            continue;
        }
        let mut ab = vec![[0.0f64; 2]; inn];
        let mut nabb = vec![[0i64; 2]; inn];
        ab[0] = [gl, gu];
        let mut cc = vec![0.0; inn];
        let mut nv: Vec<usize> = vec![];
        let (im, _) = laebz(1, 0, inn, inn, 1, atoli, rtoli, pivmin, dd, e2, &mut nv, &mut ab, &mut cc, &mut nabb);
        nwl += nabb[0][0];
        nwu += nabb[0][1];
        let iwoff = m as i64 - nabb[0][0];
        let itmax = (((gu - gl + pivmin).ln() - pivmin.ln()) / 2f64.ln()) as usize + 2;
        let (iout, _) = laebz(2, itmax, inn, inn, 1, atoli, rtoli, pivmin, dd, e2, &mut nv, &mut ab, &mut cc, &mut nabb);
        for j in 0..iout {
            let t = 0.5 * (ab[j][0] + ab[j][1]);
            for je in (nabb[j][0] + iwoff)..(nabb[j][1] + iwoff) {
                w[je as usize] = t;
            }
        }
        m += im;
    }
    let mut idiscl = il as i64 - 1 - nwl;
    let mut idiscu = nwu - iu as i64;
    let mut w: Vec<f64> = w[..m].to_vec();
    if idiscl > 0 || idiscu > 0 {
        let mut out = vec![];
        for &v in &w {
            if v <= wlu && idiscl > 0 {
                idiscl -= 1;
            } else if v >= wul && idiscu > 0 {
                idiscu -= 1;
            } else {
                out.push(v);
            }
        }
        w = out;
    }
    w.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    w.truncate(k);
    w
}

/// The `n` lowest eigenvalues (ascending) of the symmetric tridiagonal matrix with diagonal d and
/// off-diagonal e, by bisection to rounding level, and, if asked, their unit eigenvectors by inverse
/// iteration (column k of the returned row-major m×n matrix). The native replacement of SciPy's
/// `eigh_tridiagonal(d, e, select='i', select_range=(0, n-1))`.
pub fn tridiag_lowest(d: &[f64], e: &[f64], n: usize, vectors: bool) -> (Vec<f64>, Option<Vec<f64>>) {
    let m = d.len();
    let vals = stebz_lowest(d, e, n, 4e-308);
    if !vectors {
        return (vals, None);
    }
    let z = stein(d, e, &vals);
    let mut vecs = vec![0.0; m * n];
    for (k, x) in z.iter().enumerate() {
        for i in 0..m {
            vecs[i * n + k] = x[i];
        }
    }
    (vals, Some(vecs))
}

// ------------------------------------------------------------------------------ LAPACK dstein
const EPS_E: f64 = 1.1102230246251565e-16; // dlamch('E')

/// LAPACK's dlarnv(2, …) with its seed: uniform (−1, 1) from dlaruv's generator (multiplier
/// 33952834046453 mod 2⁴⁸; the 128 multipliers of its table are the powers of it, so the stream is consecutive).
struct Larnv(u64);

impl Larnv {
    fn fill(&mut self, n: usize) -> Vec<f64> {
        const A: u64 = 33952834046453;
        const MASK: u64 = (1 << 48) - 1;
        (0..n)
            .map(|_| {
                self.0 = ((self.0 as u128 * A as u128) as u64) & MASK;
                2.0 * (self.0 as f64 / (1u64 << 48) as f64) - 1.0
            })
            .collect()
    }
}

fn idamax(x: &[f64]) -> usize {
    let mut j = 0;
    for i in 1..x.len() {
        if x[i].abs() > x[j].abs() {
            j = i;
        }
    }
    j
}

/// OpenBLAS's dnrm2 (x87 extended precision): the correctly rounded √(Σ x²), from a double-double sum.
fn nrm2_exact(x: &[f64]) -> f64 {
    let big = x.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    if big == 0.0 || !big.is_finite() {
        return big;
    }
    // scale by a power of two (exact)
    let k = big.log2().floor() as i32;
    let sc = 2f64.powi(-k);
    let (mut hi, mut lo) = (0.0f64, 0.0f64);
    for &v in x {
        let y = v * sc;
        let p = y * y;
        let pe = y.mul_add(y, -p);
        let t = hi + p;
        let bb = t - hi;
        let err = (hi - (t - bb)) + (p - bb);
        hi = t;
        lo += err + pe;
    }
    let t = hi + lo;
    lo -= t - hi;
    hi = t;
    let r = hi.sqrt();
    let resid = |c: f64| -> f64 {
        let c2 = c * c;
        let c2e = c.mul_add(c, -c2);
        ((c2 - hi) + c2e - lo).abs()
    };
    let mut best = r;
    for c in [f64::from_bits(r.to_bits() - 1), f64::from_bits(r.to_bits() + 1)] {
        if resid(c) < resid(best) {
            best = c;
        }
    }
    best * 2f64.powi(k)
}

/// LAPACK dlagtf: T − λI = P L U for the tridiagonal (a, b above, c below).
fn lagtf(a: &[f64], lam: f64, b: &[f64], c: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<bool>) {
    let n = a.len();
    let (mut a, mut b, mut c) = (a.to_vec(), b.to_vec(), c.to_vec());
    let mut d = vec![0.0; n.saturating_sub(2)];
    let mut piv = vec![false; n];
    a[0] -= lam;
    let mut scale1 = a[0].abs() + b[0].abs();
    for k in 0..n - 1 {
        a[k + 1] -= lam;
        let mut scale2 = c[k].abs() + a[k + 1].abs();
        if k + 2 < n {
            scale2 += b[k + 1].abs();
        }
        let piv1 = if a[k] == 0.0 { 0.0 } else { a[k].abs() / scale1 };
        if c[k] == 0.0 {
            scale1 = scale2;
        } else {
            let piv2 = c[k].abs() / scale2;
            if piv2 <= piv1 {
                scale1 = scale2;
                c[k] /= a[k];
                a[k + 1] -= c[k] * b[k];
            } else {
                piv[k] = true;
                let mult = a[k] / c[k];
                a[k] = c[k];
                let temp = a[k + 1];
                a[k + 1] = b[k] - mult * temp;
                if k + 2 < n {
                    d[k] = b[k + 1];
                    b[k + 1] = -mult * d[k];
                }
                b[k] = temp;
                c[k] = mult;
            }
        }
    }
    (a, b, c, d, piv)
}

/// LAPACK dlagts with JOB = −1 (perturbing tiny pivots); `tol` ≤ 0 is set on the first call, as LAPACK does.
fn lagts(a: &[f64], b: &[f64], c: &[f64], d: &[f64], piv: &[bool], y: &mut [f64], tol: &mut f64) {
    let n = a.len();
    let sfmin = f64::MIN_POSITIVE;
    let bignum = 1.0 / sfmin;
    if *tol <= 0.0 {
        let mut t = a[0].abs();
        if n > 1 {
            t = t.max(a[1].abs()).max(b[0].abs());
        }
        for k in 2..n {
            t = t.max(a[k].abs()).max(b[k - 1].abs()).max(d[k - 2].abs());
        }
        t *= EPS_E;
        *tol = if t == 0.0 { EPS_E } else { t };
    }
    for k in 1..n {
        if !piv[k - 1] {
            y[k] -= c[k - 1] * y[k - 1];
        } else {
            let temp = y[k - 1];
            y[k - 1] = y[k];
            y[k] = temp - c[k - 1] * y[k];
        }
    }
    for k in (0..n).rev() {
        let mut temp = if k + 3 <= n {
            y[k] - b[k] * y[k + 1] - d[k] * y[k + 2]
        } else if k + 2 == n {
            y[k] - b[k] * y[k + 1]
        } else {
            y[k]
        };
        let mut ak = a[k];
        let mut pert = tol.copysign(ak);
        loop {
            let absak = ak.abs();
            if absak < 1.0 {
                if absak < sfmin {
                    if absak == 0.0 || temp.abs() * sfmin > absak {
                        ak += pert;
                        pert *= 2.0;
                        continue;
                    }
                    temp *= bignum;
                    ak *= bignum;
                } else if temp.abs() > absak * bignum {
                    ak += pert;
                    pert *= 2.0;
                    continue;
                }
            }
            break;
        }
        y[k] = temp / ak;
    }
}

/// The eigenvectors of the tridiagonal (d, e) at the ascending eigenvalues `w`: LAPACK dstein transcribed
/// (inverse iteration from dlarnv's random start, modified Gram–Schmidt within clusters, OpenBLAS's ddot/axpy/
/// nrm2 as on the oracle), so SciPy's `eigh_tridiagonal` vectors are reproduced bit for bit (checked on
/// random matrices). The matrix is treated as one block (dstebz splits it only where an off-diagonal is
/// negligible, which never happens for a discretised operator).
fn stein(d: &[f64], e: &[f64], w: &[f64]) -> Vec<Vec<f64>> {
    let n = d.len();
    let mut seed = Larnv((1 << 36) + (1 << 24) + (1 << 12) + 1);
    if n == 1 {
        return w.iter().map(|_| vec![1.0]).collect();
    }
    let mut onenrm = (d[0].abs() + e[0].abs()).max(d[n - 1].abs() + e[n - 2].abs());
    for i in 1..n - 1 {
        onenrm = onenrm.max(d[i].abs() + e[i - 1].abs() + e[i].abs());
    }
    let ortol = 1e-3 * onenrm;
    let dtpcrt = (0.1 / n as f64).sqrt();
    let mut z: Vec<Vec<f64>> = Vec::with_capacity(w.len());
    let mut gpind = 0usize;
    let mut xjm = 0.0;
    for (j, &w_j) in w.iter().enumerate() {
        let mut xj = w_j;
        if j > 0 {
            let pertol = 10.0 * (ULP * xj).abs();
            if xj - xjm < pertol {
                xj = xjm + pertol;
            }
        }
        let mut x = seed.fill(n);
        let (a, b, c, dd, piv) = lagtf(d, xj, e, e);
        let mut tol = 0.0;
        let mut nrmchk = 0;
        for _its in 0..5 {
            let jmax = idamax(&x);
            let scl = n as f64 * onenrm * ULP.max(a[n - 1].abs()) / x[jmax].abs();
            x.iter_mut().for_each(|v| *v *= scl);
            lagts(&a, &b, &c, &dd, &piv, &mut x, &mut tol);
            if j > 0 {
                if (xj - xjm).abs() > ortol {
                    gpind = j;
                }
                for zi in &z[gpind..j] {
                    let ztr = -super::npblas::ddot(&x, zi);
                    for (xv, &q) in x.iter_mut().zip(zi) {
                        *xv = ztr.mul_add(q, *xv);
                    }
                }
            }
            let nrm = x[idamax(&x)].abs();
            if nrm < dtpcrt {
                continue;
            }
            nrmchk += 1;
            if nrmchk < 3 {
                continue;
            }
            break;
        }
        let mut scl = 1.0 / nrm2_exact(&x);
        if x[idamax(&x)] < 0.0 {
            scl = -scl;
        }
        x.iter_mut().for_each(|v| *v *= scl);
        z.push(x);
        xjm = xj;
    }
    z
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
    // v1's Hermite basis at the 4 Gauss points, as NumPy evaluated it (its array power isn't libm's pow)
    const BASIS: [[f64; 4]; 4] = [
        [0.9862070884609109, 0.060124997938716174, 0.013792911539089075, -0.00448606527483153],
        [0.7451614261182038, 0.1481370634132568, 0.25483857388179626, -0.07296615908748122],
        [0.2548385738817962, 0.07296615908748116, 0.7451614261182038, -0.14813706341325683],
        [0.013792911539088903, 0.004486065274831419, 0.9862070884609111, -0.060124997938716285],
    ];
    let gw = [0.3478548451374538, 0.6521451548625461, 0.6521451548625461, 0.3478548451374538];
    let cells = p.len() - 1;
    // (v * v) @ gw is a dgemv over rows of 4; np.sum of it is pairwise
    let rows: Vec<f64> = (0..cells)
        .map(|i| {
            let sq: Vec<f64> = BASIS
                .iter()
                .map(|b| {
                    let v = p[i] * b[0] + h * d[i] * b[1] + p[i + 1] * b[2] + h * d[i + 1] * b[3];
                    v * v
                })
                .collect();
            super::npblas::gemv_t(&sq, &gw, cells, i)
        })
        .collect();
    0.5 * h * super::npblas::np_sum(&rows)
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
        // LAPACK dgbtf2: the multipliers are scaled by the pivot's reciprocal (dscal), the update is a dger
        // (OpenBLAS axpy per column: a = fma(-u, l, a)); dgbtrs applies L the same way
        let r = 1.0 / at(&rows, k, k);
        let hi = (k + ku + kl).min(n - 1);
        for i in k + 1..=last {
            let lik = at(&rows, i, k) * r;
            let sk = (k as i64 - off(i)) as usize;
            rows[i][sk] = lik;
            for j in k + 1..=hi {
                let sj = j as i64 - off(i);
                if sj >= 0 && (sj as usize) < wdt {
                    let v = at(&rows, k, j);
                    rows[i][sj as usize] = (-v).mul_add(lik, rows[i][sj as usize]);
                }
            }
            let xk = x[k];
            x[i] = (-xk).mul_add(lik, x[i]);
        }
    }
    // dtbsv (upper, column-oriented): divide, then an axpy up the column
    for k in (0..n).rev() {
        x[k] /= at(&rows, k, k);
        let lo = k.saturating_sub(ku + kl);
        let xk = x[k];
        for i in lo..k {
            x[i] = (-xk).mul_add(at(&rows, i, k), x[i]);
        }
    }
    Some(x)
}

/// `np.linalg.norm` (sqrt of OpenBLAS ddot, as on the oracle).
fn norm2(x: &[f64]) -> f64 {
    super::npblas::ddot(x, x).sqrt()
}

/// `np.dot` of two vectors (OpenBLAS ddot).
fn dot(a: &[f64], b: &[f64]) -> f64 {
    super::npblas::ddot(a, b)
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
    fn stebz_stein_match_scipy() {
        // SciPy: eigh_tridiagonal(2 + linspace(0, 1, 20)**2, -ones(19), select='i', select_range=(0, 2), tol=4e-308)
        let n = 20;
        let d: Vec<f64> = (0..n).map(|i| if i == n - 1 { 1.0 } else { i as f64 * (1.0 / 19.0) }).map(|x| 2.0 + x * x).collect();
        let e = vec![-1.0; n - 1];
        let (vals, vecs) = tridiag_lowest(&d, &e, 3, true);
        assert_eq!(vals, vec![0.13205941765508392, 0.3259210797593231, 0.520473825436112]);
        let v = vecs.unwrap();
        assert_eq!((v[3 * 3], v[7 * 3 + 1], v[11 * 3 + 2]), (0.42164731837421227, 0.35339400744462435, 0.37275885942280657));
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
