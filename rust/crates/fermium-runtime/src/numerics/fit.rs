//! Nonlinear least squares for `fit`: v1's `fermium/runtime/fitting.py` (initial-guess scan over
//! powers of ten and a 1-2-5 grid, several starts, covariance (JᵀJ)⁻¹·rss/dof with the D262
//! degeneracy test) over a native port of what v1 called, SciPy 1.17's
//! `least_squares(method='lm', x_scale=|p0|, xtol=ftol=gtol=1e-14, max_nfev=20000)`: MINPACK's
//! `lmder` (Levenberg–Marquardt with a trust region, QR with column pivoting, `lmpar`, `qrsolv`,
//! `enorm`) with the Jacobian by SciPy's `approx_derivative` ('2-point', step
//! √ε·sign(x)·max(1, |x|)).
//!
//! The model is given as residuals: `resid(params, out)` writes model − left side for every data
//! point (v1 fits those to zero).

const EPS: f64 = f64::EPSILON;

/// The result of a fit.
#[derive(Debug, Clone)]
pub struct FitResult {
    pub params: Vec<f64>,
    /// standard errors; None where they could not be estimated
    pub errors: Vec<Option<f64>>,
    /// √(rss / n)
    pub rms: f64,
    /// the covariance matrix (k×k, row-major), when the errors could be estimated (D124)
    pub cov: Option<Vec<f64>>,
}

/// v1's `fit_sigfigs`: digits for a fitted value, at least 4 and enough to reach the second digit
/// of its standard error (gauntlet friction #35).
pub fn fit_sigfigs(val: f64, err: Option<f64>) -> i32 {
    match err {
        Some(e) if e.is_finite() && e > 0.0 && val.is_finite() && val != 0.0 => {
            let d = val.abs().log10().floor() as i32 - e.log10().floor() as i32 + 2;
            d.clamp(4, 12)
        }
        _ => 4,
    }
}

struct Model<'a> {
    resid: &'a mut dyn FnMut(&[f64], &mut [f64]),
    n: usize,
}

impl Model<'_> {
    /// fitting._sse: the sum of squares of the residuals, inf if not finite
    fn sse(&mut self, p: &[f64]) -> f64 {
        let mut r = vec![0.0; self.n];
        (self.resid)(p, &mut r);
        let v: f64 = r.iter().map(|x| x * x).sum();
        if v.is_finite() { v } else { f64::INFINITY }
    }
    /// the residuals least_squares sees: non-finite values become 1e300
    fn fun(&mut self, p: &[f64], r: &mut [f64]) {
        (self.resid)(p, r);
        for v in r.iter_mut() {
            if !v.is_finite() {
                *v = 1e300;
            }
        }
    }
    /// scipy approx_derivative '2-point' at p (f0 = fun(p)); J row-major n×k
    fn jac(&mut self, p: &[f64], f0: &[f64]) -> Vec<f64> {
        let (n, k) = (self.n, p.len());
        let mut j = vec![0.0; n * k];
        let rstep = EPS.powf(0.5);
        let mut x1 = p.to_vec();
        let mut f1 = vec![0.0; n];
        for c in 0..k {
            let sign = if p[c] >= 0.0 { 1.0 } else { -1.0 };
            let h = rstep * sign * p[c].abs().max(1.0);
            x1.copy_from_slice(p);
            x1[c] = p[c] + h;
            self.fun(&x1, &mut f1);
            let dx = (p[c] + h) - p[c];
            for i in 0..n {
                j[i * k + c] = (f1[i] - f0[i]) / dx;
            }
        }
        j
    }
}

/// fitting._initial_guess: fill in missing guesses (None) by scanning a grid of values.
fn initial_guess(m: &mut Model<'_>, guess: &[Option<f64>], pos125: bool) -> Vec<f64> {
    let mut p: Vec<f64> = guess.iter().map(|g| g.filter(|v| v.is_finite()).unwrap_or(1.0)).collect();
    let missing: Vec<usize> = (0..guess.len()).filter(|&i| !guess[i].map(|v| v.is_finite()).unwrap_or(false)).collect();
    if missing.is_empty() {
        return p;
    }
    let mut scales = Vec::new();
    for e in -35..=35 {
        let t = 10f64.powf(e as f64);
        if pos125 {
            for mm in [1.0, 2.0, 5.0] {
                scales.push(mm * t);
            }
        } else {
            for s in [1.0, -1.0] {
                scales.push(s * t);
            }
        }
    }
    for _ in 0..2 {
        for &i in &missing {
            let (mut best, mut bestv) = (p[i], m.sse(&p));
            for &s in &scales {
                let mut q = p.clone();
                q[i] = s;
                let v = m.sse(&q);
                if v < bestv {
                    best = s;
                    bestv = v;
                }
            }
            p[i] = best;
        }
    }
    p
}

/// v1's `degenerate` (D262): is JᵀJ singular up to rounding? Normalised to unit diagonal,
/// Gaussian elimination with partial pivoting must not meet a pivot below 1e-9.
pub fn degenerate(a: &[f64], k: usize) -> bool {
    let d: Vec<f64> = (0..k).map(|i| a[i * k + i].abs().sqrt()).collect();
    if a.iter().any(|v| !v.is_finite()) || d.iter().any(|&v| v == 0.0) {
        return true;
    }
    let mut m: Vec<f64> = (0..k * k).map(|ij| a[ij] / (d[ij / k] * d[ij % k])).collect();
    for c in 0..k {
        let mut piv = c;
        for r in c + 1..k {
            if m[r * k + c].abs() > m[piv * k + c].abs() {
                piv = r;
            }
        }
        if !(m[piv * k + c].abs() >= 1e-9) {
            return true;
        }
        if piv != c {
            for j in 0..k {
                m.swap(c * k + j, piv * k + j);
            }
        }
        for r in c + 1..k {
            let f = m[r * k + c] / m[c * k + c];
            for j in c..k {
                m[r * k + j] -= f * m[c * k + j];
            }
        }
    }
    false
}

/// v1's `least_squares_fit`: fit the parameters so the residuals go to zero. `guess[i]` None
/// means no starting guess (then the grid scans and two starts, as v1).
pub fn least_squares_fit(
    resid: &mut dyn FnMut(&[f64], &mut [f64]),
    n: usize,
    guess: &[Option<f64>],
) -> Result<FitResult, String> {
    let k = guess.len();
    if n < k {
        return Err(format!("can't fit {k} parameters to only {n} data points"));
    }
    let mut m = Model { resid, n };
    let missing = guess.iter().any(|g| !g.map(|v| v.is_finite()).unwrap_or(false));
    let mut starts = vec![initial_guess(&mut m, guess, false)];
    if missing {
        starts.push(initial_guess(&mut m, guess, true));
    }
    let mut best: Option<Lm> = None;
    for p0 in starts {
        let x_scale: Vec<f64> = p0.iter().map(|&v| if v != 0.0 { v.abs() } else { 1.0 }).collect();
        let mut f0 = vec![0.0; n];
        m.fun(&p0, &mut f0);
        if f0.iter().any(|v| !v.is_finite()) {
            continue; // "Residuals are not finite in the initial point" (never after the 1e300 guard)
        }
        let diag: Vec<f64> = x_scale.iter().map(|s| 1.0 / s).collect();
        let cand = lmder(&mut m, &p0, &diag, 1e-14, 1e-14, 1e-14, 20000, 100.0);
        if best.as_ref().map(|b| cand.cost < b.cost).unwrap_or(true) {
            best = Some(cand);
        }
    }
    let res = best.ok_or_else(|| "the fit failed: the model can't be evaluated at the starting guesses".to_string())?;
    // fitting._finish
    let r = &res.fvec;
    let rss: f64 = r.iter().map(|v| v * v).sum();
    let dof = (n as i64 - k as i64).max(1) as f64;
    let mut errors = vec![None; k];
    let mut cov = None;
    let jac = m.jac(&res.x, &res.fvec);
    let mut a = vec![0.0; k * k];
    for p in 0..k {
        for q in 0..k {
            let mut s = 0.0;
            for i in 0..n {
                s += jac[i * k + p] * jac[i * k + q];
            }
            a[p * k + q] = s;
        }
    }
    if !degenerate(&a, k) {
        if let Some(inv) = invert(&a, k) {
            let c: Vec<f64> = inv.iter().map(|v| v * (rss / dof)).collect();
            for i in 0..k {
                let v = c[i * k + i];
                errors[i] = if v >= 0.0 { Some(v.sqrt()) } else { None };
            }
            cov = Some(c);
        }
    }
    Ok(FitResult { params: res.x, errors, rms: (rss / n as f64).sqrt(), cov })
}

/// The inverse of a k×k matrix (partial-pivoting LU), None if exactly singular.
fn invert(a: &[f64], k: usize) -> Option<Vec<f64>> {
    let lu = super::dense::Lu::new(a.to_vec(), k)?;
    if lu.singular {
        return None;
    }
    let mut inv = vec![0.0; k * k];
    for c in 0..k {
        let mut e = vec![0.0; k];
        e[c] = 1.0;
        lu.solve(&mut e);
        for r in 0..k {
            inv[r * k + c] = e[r];
        }
    }
    Some(inv)
}

// ------------------------------------------------------------------------------ MINPACK

struct Lm {
    x: Vec<f64>,
    fvec: Vec<f64>,
    cost: f64,
    #[allow(dead_code)]
    info: i32,
}

/// MINPACK's enorm: the Euclidean norm, computed without destructive underflow or overflow.
fn enorm(x: &[f64]) -> f64 {
    const RDWARF: f64 = 3.834e-20;
    const RGIANT: f64 = 1.304e19;
    let (mut s1, mut s2, mut s3, mut x1max, mut x3max) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let agiant = RGIANT / x.len() as f64;
    for &v in x {
        let xabs = v.abs();
        if xabs > RDWARF && xabs < agiant {
            s2 += xabs * xabs;
        } else if xabs <= RDWARF {
            if xabs > x3max {
                s3 = 1.0 + s3 * (x3max / xabs) * (x3max / xabs);
                x3max = xabs;
            } else if xabs != 0.0 {
                s3 += (xabs / x3max) * (xabs / x3max);
            }
        } else if xabs > x1max {
            s1 = 1.0 + s1 * (x1max / xabs) * (x1max / xabs);
            x1max = xabs;
        } else {
            s1 += (xabs / x1max) * (xabs / x1max);
        }
    }
    if s1 != 0.0 {
        x1max * (s1 + (s2 / x1max) / x1max).sqrt()
    } else if s2 != 0.0 {
        if s2 >= x3max {
            (s2 * (1.0 + (x3max / s2) * (x3max * s3))).sqrt()
        } else {
            (x3max * ((s2 / x3max) + (x3max * s3))).sqrt()
        }
    } else {
        x3max * s3.sqrt()
    }
}

/// MINPACK's qrfac with column pivoting on a (m×n, column-major in `a[j*m + i]`).
/// Returns (ipvt, rdiag, acnorm).
fn qrfac(m: usize, n: usize, a: &mut [f64]) -> (Vec<usize>, Vec<f64>, Vec<f64>) {
    let mut acnorm = vec![0.0; n];
    let mut rdiag = vec![0.0; n];
    let mut wa = vec![0.0; n];
    let mut ipvt: Vec<usize> = (0..n).collect();
    for j in 0..n {
        acnorm[j] = enorm(&a[j * m..j * m + m]);
        rdiag[j] = acnorm[j];
        wa[j] = rdiag[j];
    }
    for j in 0..m.min(n) {
        let mut kmax = j;
        for k in j..n {
            if rdiag[k] > rdiag[kmax] {
                kmax = k;
            }
        }
        if kmax != j {
            for i in 0..m {
                a.swap(j * m + i, kmax * m + i);
            }
            rdiag[kmax] = rdiag[j];
            wa[kmax] = wa[j];
            ipvt.swap(j, kmax);
        }
        let mut ajnorm = enorm(&a[j * m + j..j * m + m]);
        if ajnorm != 0.0 {
            if a[j * m + j] < 0.0 {
                ajnorm = -ajnorm;
            }
            for i in j..m {
                a[j * m + i] /= ajnorm;
            }
            a[j * m + j] += 1.0;
            for k in j + 1..n {
                let mut sum = 0.0;
                for i in j..m {
                    sum += a[j * m + i] * a[k * m + i];
                }
                let temp = sum / a[j * m + j];
                for i in j..m {
                    a[k * m + i] -= temp * a[j * m + i];
                }
                if rdiag[k] != 0.0 {
                    let temp = a[k * m + j] / rdiag[k];
                    rdiag[k] *= (1.0 - temp * temp).max(0.0).sqrt();
                    if 0.05 * (rdiag[k] / wa[k]) * (rdiag[k] / wa[k]) <= EPS {
                        rdiag[k] = enorm(&a[k * m + j + 1..k * m + m]);
                        wa[k] = rdiag[k];
                    }
                }
            }
        }
        rdiag[j] = -ajnorm;
    }
    (ipvt, rdiag, acnorm)
}

/// MINPACK's qrsolv. r: n×n column-major with leading dimension ldr (r[j*ldr + i]).
#[allow(clippy::too_many_arguments)]
fn qrsolv(n: usize, r: &mut [f64], ldr: usize, ipvt: &[usize], diag: &[f64], qtb: &[f64], x: &mut [f64], sdiag: &mut [f64]) {
    let mut wa = vec![0.0; n];
    for j in 0..n {
        for i in j..n {
            r[j * ldr + i] = r[i * ldr + j];
        }
        x[j] = r[j * ldr + j];
        wa[j] = qtb[j];
    }
    for j in 0..n {
        let l = ipvt[j];
        if diag[l] != 0.0 {
            for s in sdiag.iter_mut().take(n).skip(j) {
                *s = 0.0;
            }
            sdiag[j] = diag[l];
            let mut qtbpj = 0.0;
            for k in j..n {
                if sdiag[k] == 0.0 {
                    continue;
                }
                let rkk = r[k * ldr + k];
                let (sin, cos);
                if rkk.abs() < sdiag[k].abs() {
                    let cotan = rkk / sdiag[k];
                    sin = 0.5 / (0.25 + 0.25 * cotan * cotan).sqrt();
                    cos = sin * cotan;
                } else {
                    let tan = sdiag[k] / rkk;
                    cos = 0.5 / (0.25 + 0.25 * tan * tan).sqrt();
                    sin = cos * tan;
                }
                r[k * ldr + k] = cos * rkk + sin * sdiag[k];
                let temp = cos * wa[k] + sin * qtbpj;
                qtbpj = -sin * wa[k] + cos * qtbpj;
                wa[k] = temp;
                for i in k + 1..n {
                    let temp = cos * r[k * ldr + i] + sin * sdiag[i];
                    sdiag[i] = -sin * r[k * ldr + i] + cos * sdiag[i];
                    r[k * ldr + i] = temp;
                }
            }
        }
        sdiag[j] = r[j * ldr + j];
        r[j * ldr + j] = x[j];
    }
    let mut nsing = n;
    for j in 0..n {
        if sdiag[j] == 0.0 && nsing == n {
            nsing = j;
        }
        if nsing < n {
            wa[j] = 0.0;
        }
    }
    for kk in 0..nsing {
        let j = nsing - 1 - kk;
        let mut sum = 0.0;
        for i in j + 1..nsing {
            sum += r[j * ldr + i] * wa[i];
        }
        wa[j] = (wa[j] - sum) / sdiag[j];
    }
    for j in 0..n {
        x[ipvt[j]] = wa[j];
    }
}

/// MINPACK's lmpar: the Levenberg–Marquardt parameter for the trust region delta.
/// Returns the new par; x receives the step, sdiag the diagonal of S.
#[allow(clippy::too_many_arguments)]
fn lmpar(
    n: usize,
    r: &mut [f64],
    ldr: usize,
    ipvt: &[usize],
    diag: &[f64],
    qtb: &[f64],
    delta: f64,
    par0: f64,
    x: &mut [f64],
    sdiag: &mut [f64],
) -> f64 {
    let dwarf = f64::MIN_POSITIVE;
    let mut par = par0;
    let mut wa1 = vec![0.0; n];
    let mut wa2 = vec![0.0; n];
    let mut nsing = n;
    for j in 0..n {
        wa1[j] = qtb[j];
        if r[j * ldr + j] == 0.0 && nsing == n {
            nsing = j;
        }
        if nsing < n {
            wa1[j] = 0.0;
        }
    }
    for kk in 0..nsing {
        let j = nsing - 1 - kk;
        wa1[j] /= r[j * ldr + j];
        let temp = wa1[j];
        for i in 0..j {
            wa1[i] -= r[j * ldr + i] * temp;
        }
    }
    for j in 0..n {
        x[ipvt[j]] = wa1[j];
    }
    let mut iter = 0;
    for j in 0..n {
        wa2[j] = diag[j] * x[j];
    }
    let mut dxnorm = enorm(&wa2);
    let mut fp = dxnorm - delta;
    if fp <= 0.1 * delta {
        return 0.0;
    }
    let mut parl = 0.0;
    if nsing >= n {
        for j in 0..n {
            let l = ipvt[j];
            wa1[j] = diag[l] * (wa2[l] / dxnorm);
        }
        for j in 0..n {
            let mut sum = 0.0;
            for i in 0..j {
                sum += r[j * ldr + i] * wa1[i];
            }
            wa1[j] = (wa1[j] - sum) / r[j * ldr + j];
        }
        let temp = enorm(&wa1);
        parl = ((fp / delta) / temp) / temp;
    }
    for j in 0..n {
        let mut sum = 0.0;
        for i in 0..=j {
            sum += r[j * ldr + i] * qtb[i];
        }
        wa1[j] = sum / diag[ipvt[j]];
    }
    let gnorm = enorm(&wa1);
    let mut paru = gnorm / delta;
    if paru == 0.0 {
        paru = dwarf / delta.min(0.1);
    }
    par = par.max(parl);
    par = par.min(paru);
    if par == 0.0 {
        par = gnorm / dxnorm;
    }
    loop {
        iter += 1;
        if par == 0.0 {
            par = dwarf.max(0.001 * paru);
        }
        let temp = par.sqrt();
        for j in 0..n {
            wa1[j] = temp * diag[j];
        }
        qrsolv(n, r, ldr, ipvt, &wa1, qtb, x, sdiag);
        for j in 0..n {
            wa2[j] = diag[j] * x[j];
        }
        dxnorm = enorm(&wa2);
        let temp = fp;
        fp = dxnorm - delta;
        if fp.abs() <= 0.1 * delta || (parl == 0.0 && fp <= temp && temp < 0.0) || iter == 10 {
            break;
        }
        for j in 0..n {
            let l = ipvt[j];
            wa1[j] = diag[l] * (wa2[l] / dxnorm);
        }
        for j in 0..n {
            wa1[j] /= sdiag[j];
            let temp = wa1[j];
            for i in j + 1..n {
                wa1[i] -= r[j * ldr + i] * temp;
            }
        }
        let temp = enorm(&wa1);
        let parc = ((fp / delta) / temp) / temp;
        if fp > 0.0 {
            parl = parl.max(par);
        }
        if fp < 0.0 {
            paru = paru.min(par);
        }
        par = parl.max(par + parc);
    }
    if iter == 0 { 0.0 } else { par }
}

/// MINPACK's lmder (mode 2: user `diag`), with the Jacobian of `Model::jac`.
#[allow(clippy::too_many_arguments)]
fn lmder(mdl: &mut Model<'_>, x0: &[f64], diag0: &[f64], ftol: f64, xtol: f64, gtol: f64, maxfev: usize, factor: f64) -> Lm {
    let m = mdl.n;
    let n = x0.len();
    let mut x = x0.to_vec();
    let diag = diag0.to_vec();
    let mut fvec = vec![0.0; m];
    mdl.fun(&x, &mut fvec);
    let mut nfev = 1usize;
    let mut fnorm = enorm(&fvec);
    let mut par = 0.0;
    let mut iter = 1;
    let mut delta = 0.0;
    let mut xnorm = 0.0;
    let mut info;
    let mut qtf = vec![0.0; n];
    let (mut wa1, mut wa2, mut wa3) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let mut wa4 = vec![0.0; m];
    'outer: loop {
        // Jacobian, stored column-major fjac[j*m + i]
        let jr = mdl.jac(&x, &fvec);
        let mut fjac = vec![0.0; m * n];
        for i in 0..m {
            for j in 0..n {
                fjac[j * m + i] = jr[i * n + j];
            }
        }
        let (ipvt, rdiag, acnorm) = qrfac(m, n, &mut fjac);
        wa1.copy_from_slice(&rdiag);
        wa2.copy_from_slice(&acnorm);
        if iter == 1 {
            for j in 0..n {
                wa3[j] = diag[j] * x[j];
            }
            xnorm = enorm(&wa3);
            delta = factor * xnorm;
            if delta == 0.0 {
                delta = factor;
            }
        }
        wa4.copy_from_slice(&fvec);
        for j in 0..n {
            if fjac[j * m + j] != 0.0 {
                let mut sum = 0.0;
                for i in j..m {
                    sum += fjac[j * m + i] * wa4[i];
                }
                let temp = -sum / fjac[j * m + j];
                for i in j..m {
                    wa4[i] += fjac[j * m + i] * temp;
                }
            }
            fjac[j * m + j] = wa1[j];
            qtf[j] = wa4[j];
        }
        let mut gnorm = 0.0f64;
        if fnorm != 0.0 {
            for j in 0..n {
                let l = ipvt[j];
                if wa2[l] != 0.0 {
                    let mut sum = 0.0;
                    for i in 0..=j {
                        sum += fjac[j * m + i] * (qtf[i] / fnorm);
                    }
                    gnorm = gnorm.max((sum / wa2[l]).abs());
                }
            }
        }
        if gnorm <= gtol {
            info = 4;
            break 'outer;
        }
        loop {
            par = lmpar(n, &mut fjac, m, &ipvt, &diag, &qtf, delta, par, &mut wa1, &mut wa2);
            for j in 0..n {
                wa1[j] = -wa1[j];
                wa2[j] = x[j] + wa1[j];
                wa3[j] = diag[j] * wa1[j];
            }
            let pnorm = enorm(&wa3);
            if iter == 1 {
                delta = delta.min(pnorm);
            }
            mdl.fun(&wa2, &mut wa4);
            nfev += 1;
            let fnorm1 = enorm(&wa4);
            let mut actred = -1.0;
            if 0.1 * fnorm1 < fnorm {
                actred = 1.0 - (fnorm1 / fnorm) * (fnorm1 / fnorm);
            }
            for j in 0..n {
                wa3[j] = 0.0;
                let l = ipvt[j];
                let temp = wa1[l];
                for i in 0..=j {
                    wa3[i] += fjac[j * m + i] * temp;
                }
            }
            let temp1 = enorm(&wa3) / fnorm;
            let temp2 = (par.sqrt() * pnorm) / fnorm;
            let prered = temp1 * temp1 + temp2 * temp2 / 0.5;
            let dirder = -(temp1 * temp1 + temp2 * temp2);
            let ratio = if prered != 0.0 { actred / prered } else { 0.0 };
            if ratio <= 0.25 {
                let mut temp = if actred >= 0.0 { 0.5 } else { 0.5 * dirder / (dirder + 0.5 * actred) };
                if 0.1 * fnorm1 >= fnorm || temp < 0.1 {
                    temp = 0.1;
                }
                delta = temp * delta.min(pnorm / 0.1);
                par /= temp;
            } else if par == 0.0 || ratio >= 0.75 {
                delta = pnorm / 0.5;
                par *= 0.5;
            }
            if ratio >= 1e-4 {
                for j in 0..n {
                    x[j] = wa2[j];
                    wa2[j] = diag[j] * x[j];
                }
                fvec.copy_from_slice(&wa4);
                xnorm = enorm(&wa2);
                fnorm = fnorm1;
                iter += 1;
            }
            info = 0;
            if actred.abs() <= ftol && prered <= ftol && 0.5 * ratio <= 1.0 {
                info = 1;
            }
            if delta <= xtol * xnorm {
                info = 2;
            }
            if actred.abs() <= ftol && prered <= ftol && 0.5 * ratio <= 1.0 && info == 2 {
                info = 3;
            }
            if info != 0 {
                break 'outer;
            }
            if nfev >= maxfev {
                info = 5;
            }
            if actred.abs() <= EPS && prered <= EPS && 0.5 * ratio <= 1.0 {
                info = 6;
            }
            if delta <= EPS * xnorm {
                info = 7;
            }
            if gnorm <= EPS {
                info = 8;
            }
            if info != 0 {
                break 'outer;
            }
            if ratio >= 1e-4 {
                break;
            }
        }
    }
    let cost = 0.5 * fvec.iter().map(|v| v * v).sum::<f64>();
    Lm { x, fvec, cost, info }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x + 1.0 + 0.01 * (3.0 * x).sin()).collect();
        let mut r = |p: &[f64], o: &mut [f64]| {
            for i in 0..10 {
                o[i] = p[0] * xs[i] + p[1] - ys[i];
            }
        };
        let fit = least_squares_fit(&mut r, 10, &[None, None]).unwrap();
        assert!((fit.params[0] - 2.0).abs() < 1e-2 && (fit.params[1] - 1.0).abs() < 1e-2);
        assert!(fit.errors.iter().all(|e| e.is_some()));
    }

    #[test]
    fn degenerate_product() {
        // A·B only appears together: JᵀJ is singular (D262)
        let mut r = |p: &[f64], o: &mut [f64]| {
            for i in 0..5 {
                o[i] = p[0] * p[1] * i as f64 - 2.0 * i as f64;
            }
        };
        let fit = least_squares_fit(&mut r, 5, &[Some(1.0), Some(1.0)]).unwrap();
        assert!(fit.errors.iter().all(|e| e.is_none()));
    }
}
