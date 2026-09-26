//! Dense linear algebra on matrices up to 16×16: v1's `fermium/linalg.py` (up to 4×4, unrolled:
//! det by cofactors, elimination with branch-free successive swaps, Jacobi with 10 sweeps) and
//! `fermium/linalg_big.py` (5×5 to 16×16, D195: elimination with one max-pivot swap, det from the
//! pivots, Jacobi with 16 sweeps), ported operation for operation so results agree to the last bit
//! with both of v1's back ends. Matrices are flat row-major slices.

use super::{err, Fail};

/// The largest matrix v1 allows (16×16)
pub const MAX_DIM: usize = 16;
/// |a_ij − a_ji| may be at most this times the largest |entry|
pub const SYMMETRY_TOL: f64 = 1e-10;
const JACOBI_SWEEPS_SMALL: usize = 10;
const JACOBI_SWEEPS_BIG: usize = 16;

fn is_big(n: usize) -> bool {
    n > 4
}

/// (r×k)·(k×c) → r×c
pub fn matmul(a: &[f64], r: usize, k: usize, b: &[f64], c: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(r * c);
    for i in 0..r {
        for j in 0..c {
            let mut acc = a[i * k] * b[j];
            for m in 1..k {
                acc += a[i * k + m] * b[m * c + j];
            }
            out.push(acc);
        }
    }
    out
}

/// The determinant: cofactor expansion up to 4×4 (exact for whole numbers), the pivots of
/// elimination above.
pub fn det(a: &[f64], n: usize) -> f64 {
    if is_big(n) {
        return solve_big(a, n, &vec![0.0; n], 1).2;
    }
    det_small(a, n)
}

fn det_small(a: &[f64], n: usize) -> f64 {
    if n == 1 {
        return a[0];
    }
    if n == 2 {
        return a[0] * a[3] - a[1] * a[2];
    }
    let mut acc = 0.0;
    for j in 0..n {
        let minor: Vec<f64> = (1..n).flat_map(|i| (0..n).filter(move |&m| m != j).map(move |m| (i, m))).map(|(i, m)| a[i * n + m]).collect();
        let term = a[j] * det_small(&minor, n - 1);
        if j == 0 {
            acc = term;
        } else if j % 2 == 1 {
            acc -= term;
        } else {
            acc += term;
        }
    }
    acc
}

/// linalg.solve (n <= 4): (X flat n×m, pivots). The same operations in the same order as v1, on one flat
/// working array (row i at i·w) instead of a vector per row.
fn solve_small(a: &[f64], n: usize, b: &[f64], m: usize) -> (Vec<f64>, Vec<f64>) {
    let w = n + m;
    let mut rows = vec![0.0f64; n * w];
    for i in 0..n {
        rows[i * w..i * w + n].copy_from_slice(&a[i * n..i * n + n]);
        rows[i * w + n..i * w + w].copy_from_slice(&b[i * m..i * m + m]);
    }
    let mut pivots = Vec::with_capacity(n);
    for k in 0..n {
        for i in k + 1..n {
            let swap = rows[i * w + k].abs() > rows[k * w + k].abs();
            if swap {
                for j in k..w {
                    rows.swap(k * w + j, i * w + j);
                }
            }
        }
        let p = rows[k * w + k];
        pivots.push(p);
        for i in k + 1..n {
            let f = rows[i * w + k] / p;
            for j in k + 1..w {
                let v = rows[k * w + j];
                rows[i * w + j] -= f * v;
            }
        }
    }
    let mut x = vec![0.0f64; n * m];
    for i in (0..n).rev() {
        for j in 0..m {
            let mut acc = rows[i * w + n + j];
            for q in i + 1..n {
                acc -= rows[i * w + q] * x[q * m + j];
            }
            x[i * m + j] = acc / rows[i * w + i];
        }
    }
    (x, pivots)
}

/// linalg_big.solve: (X, the smallest |pivot|, det)
fn solve_big(a: &[f64], n: usize, b: &[f64], m: usize) -> (Vec<f64>, f64, f64) {
    let w = n + m;
    let mut wm = vec![0.0; n * w];
    for i in 0..n {
        for j in 0..n {
            wm[i * w + j] = a[i * n + j];
        }
        for j in 0..m {
            wm[i * w + n + j] = b[i * m + j];
        }
    }
    let mut sign = 1.0f64;
    let mut piv = vec![0.0; n];
    for k in 0..n {
        let mut best = k;
        for i in k + 1..n {
            if wm[i * w + k].abs() > wm[best * w + k].abs() {
                best = i;
            }
        }
        let p = best;
        for j in k..w {
            let (x, y) = (wm[k * w + j], wm[p * w + j]);
            wm[k * w + j] = y;
            wm[p * w + j] = x;
        }
        if p != k {
            sign = -sign;
        }
        let pk = wm[k * w + k];
        piv[k] = pk;
        for i in k + 1..n {
            let f = wm[i * w + k] / pk;
            for j in k + 1..w {
                wm[i * w + j] -= f * wm[k * w + j];
            }
        }
    }
    let mut x = vec![0.0; n * m];
    for t in 0..n {
        let i = n - 1 - t;
        for j in 0..m {
            let mut acc = wm[i * w + n + j];
            for q in i + 1..n {
                acc -= wm[i * w + q] * x[q * m + j];
            }
            x[i * m + j] = acc / wm[i * w + i];
        }
    }
    let mut acc = sign;
    let mut small = piv[0].abs();
    for &pk in &piv {
        acc *= pk;
        if pk.abs() < small {
            small = pk.abs();
        }
    }
    (x, small, acc)
}

/// A X = B (A n×n, B n×m). Err(ERR_SINGULAR) on a zero pivot, as v1.
pub fn solve(a: &[f64], n: usize, b: &[f64], m: usize) -> Result<Vec<f64>, Fail> {
    let (x, zero) = if is_big(n) {
        let (x, small, _) = solve_big(a, n, b, m);
        (x, small == 0.0)
    } else {
        let (x, piv) = solve_small(a, n, b, m);
        (x, piv.iter().any(|&p| p == 0.0))
    };
    if zero { Err(Fail::new(err::SINGULAR, 0.0, 0.0)) } else { Ok(x) }
}

/// The inverse. Err(ERR_SINGULAR) on a zero pivot.
pub fn inverse(a: &[f64], n: usize) -> Result<Vec<f64>, Fail> {
    let ident: Vec<f64> = (0..n * n).map(|ij| if ij / n == ij % n { 1.0 } else { 0.0 }).collect();
    solve(a, n, &ident, n)
}

/// v1's asymmetry test: true when every |a_ij − a_ji| <= SYMMETRY_TOL × the largest |entry|.
pub fn is_symmetric(a: &[f64], n: usize) -> bool {
    let mut scale = a[0].abs();
    for &v in &a[1..n * n] {
        if v.abs() > scale.abs() {
            scale = v.abs();
        }
    }
    let tol = scale * SYMMETRY_TOL;
    if is_big(n) {
        let mut worst = 0.0f64;
        for i in 0..n {
            for j in i + 1..n {
                let d = (a[i * n + j] - a[j * n + i]).abs();
                if worst < d {
                    worst = d;
                }
            }
        }
        return !(tol - worst < 0.0);
    }
    for i in 0..n {
        for j in i + 1..n {
            if tol - (a[i * n + j] - a[j * n + i]).abs() < 0.0 {
                return false;
            }
        }
    }
    true
}

/// Cyclic Jacobi on the symmetric row-major A (overwritten; its diagonal ends as the eigenvalues);
/// returns V (columns = eigenvectors).
fn jacobi(a: &mut [f64], n: usize, sweeps: usize) -> Vec<f64> {
    let mut v: Vec<f64> = (0..n * n).map(|ij| if ij / n == ij % n { 1.0 } else { 0.0 }).collect();
    for _ in 0..sweeps {
        for p in 0..n {
            for q in p + 1..n {
                let (apq, app, aqq) = (a[p * n + q], a[p * n + p], a[q * n + q]);
                let theta = (aqq - app) / (apq + apq);
                let sgn = if theta < 0.0 { -1.0 } else { 1.0 };
                let mut t = sgn / (theta.abs() + (theta * theta + 1.0).sqrt());
                if apq == 0.0 {
                    t = 0.0;
                }
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                a[p * n + p] = app - t * apq;
                a[q * n + q] = aqq + t * apq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for r in 0..n {
                    if r != p && r != q {
                        let (arp, arq) = (a[r * n + p], a[r * n + q]);
                        let nrp = c * arp - s * arq;
                        let nrq = s * arp + c * arq;
                        a[r * n + p] = nrp;
                        a[p * n + r] = nrp;
                        a[r * n + q] = nrq;
                        a[q * n + r] = nrq;
                    }
                    let (vrp, vrq) = (v[r * n + p], v[r * n + q]);
                    v[r * n + p] = c * vrp - s * vrq;
                    v[r * n + q] = s * vrp + c * vrq;
                }
            }
        }
    }
    v
}

/// Ascending eigenvalues (bubble-sort network, columns along), each column's largest-magnitude
/// entry made positive. `cols[j]` is column j.
fn sort_and_fix_signs(vals: &mut [f64], cols: &mut [Vec<f64>], n: usize) {
    for i in 0..n {
        for j in 0..n - 1 - i {
            if vals[j + 1] < vals[j] {
                vals.swap(j, j + 1);
                cols.swap(j, j + 1);
            }
        }
    }
    for col in cols.iter_mut() {
        let mut big = col[0];
        for &x in &col[1..n] {
            if x.abs() > big.abs() {
                big = x;
            }
        }
        if big < 0.0 {
            for x in col.iter_mut() {
                *x = -*x;
            }
        }
    }
}

fn columns(v: &[f64], n: usize) -> Vec<Vec<f64>> {
    (0..n).map(|j| (0..n).map(|r| v[r * n + j]).collect()).collect()
}

fn from_columns(cols: &[Vec<f64>], n: usize) -> Vec<f64> {
    (0..n * n).map(|ij| cols[ij % n][ij / n]).collect()
}

fn symmetrised(a: &[f64], n: usize) -> Vec<f64> {
    (0..n * n).map(|ij| {
        let (i, j) = (ij / n, ij % n);
        if i == j { a[i * n + i] } else { 0.5 * (a[i * n + j] + a[j * n + i]) }
    }).collect()
}

/// Eigenvalues (ascending) and unit eigenvectors (columns of the row-major result) of the
/// symmetrised matrix, by cyclic Jacobi (D38). Checks symmetry first (ERR_NOT_SYMMETRIC).
pub fn eigen_sym(a: &[f64], n: usize) -> Result<(Vec<f64>, Vec<f64>), Fail> {
    if !is_symmetric(a, n) {
        return Err(Fail::new(err::NOT_SYMMETRIC, 0.0, 0.0));
    }
    Ok(jacobi_eigen(a, n))
}

/// v1's `jacobi_eigen` (no symmetry check)
pub fn jacobi_eigen(a: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut s = symmetrised(a, n);
    let v = jacobi(&mut s, n, if is_big(n) { JACOBI_SWEEPS_BIG } else { JACOBI_SWEEPS_SMALL });
    let mut vals: Vec<f64> = (0..n).map(|i| s[i * n + i]).collect();
    let mut cols = columns(&v, n);
    sort_and_fix_signs(&mut vals, &mut cols, n);
    (vals, from_columns(&cols, n))
}

/// K v = λ M v for symmetric K and symmetric positive-definite M (normal modes). Checks both
/// for symmetry (ERR_NOT_SYMMETRIC) and M for positive definiteness (ERR_NOT_POSDEF).
pub fn eigen_general(k: &[f64], m: &[f64], n: usize) -> Result<(Vec<f64>, Vec<f64>), Fail> {
    if !is_symmetric(k, n) || !is_symmetric(m, n) {
        return Err(Fail::new(err::NOT_SYMMETRIC, 0.0, 0.0));
    }
    let (vals, vecs, piv) = generalized_eigen(k, m, n);
    if piv.iter().any(|&p| p <= 0.0) {
        return Err(Fail::new(err::NOT_POSDEF, 0.0, 0.0));
    }
    Ok((vals, vecs))
}

/// v1's `generalized_eigen`: (values, vectors, Cholesky pivots (small) or [smallest pivot] (big))
pub fn generalized_eigen(k: &[f64], m: &[f64], n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    // Cholesky M = L Lᵀ (the same operations in both variants)
    let mut l = vec![0.0; n * n];
    let mut piv = Vec::with_capacity(n);
    for j in 0..n {
        let mut d = m[j * n + j];
        for q in 0..j {
            d -= l[j * n + q] * l[j * n + q];
        }
        piv.push(d);
        l[j * n + j] = d.sqrt();
        for i in j + 1..n {
            let mut x = 0.5 * (m[i * n + j] + m[j * n + i]);
            for q in 0..j {
                x -= l[i * n + q] * l[j * n + q];
            }
            l[i * n + j] = x / l[j * n + j];
        }
    }
    // forward: column j of L⁻¹ (vector)
    let forward = |b: &[f64]| -> Vec<f64> {
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut acc = b[i];
            for q in 0..i {
                acc -= l[i * n + q] * y[q];
            }
            y[i] = acc / l[i * n + i];
        }
        y
    };
    let backward_t = |b: &[f64]| -> Vec<f64> {
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut acc = b[i];
            for q in i + 1..n {
                acc -= l[q * n + i] * x[q];
            }
            x[i] = acc / l[i * n + i];
        }
        x
    };
    // Y = L⁻¹ K (columns), A = L⁻¹ Yᵀ
    let ycols: Vec<Vec<f64>> = (0..n).map(|j| forward(&(0..n).map(|i| k[i * n + j]).collect::<Vec<_>>())).collect();
    let acols: Vec<Vec<f64>> = (0..n).map(|j| forward(&(0..n).map(|i| ycols[i][j]).collect::<Vec<_>>())).collect();
    let a: Vec<f64> = (0..n * n).map(|ij| acols[ij % n][ij / n]).collect();
    let normalise = |v: Vec<f64>| -> Vec<f64> {
        let mut s = v[0] * v[0];
        for &x in &v[1..] {
            s += x * x;
        }
        let norm = s.sqrt();
        v.iter().map(|x| x / norm).collect()
    };
    if !is_big(n) {
        let (vals, y) = jacobi_eigen(&a, n);
        let mut out: Vec<Vec<f64>> = (0..n).map(|j| normalise(backward_t(&(0..n).map(|i| y[i * n + j]).collect::<Vec<_>>()))).collect();
        let mut vals = vals;
        sort_and_fix_signs(&mut vals, &mut out, n);
        return (vals, from_columns(&out, n), piv);
    }
    let mut aa = a;
    let v = jacobi(&mut aa, n, JACOBI_SWEEPS_BIG);
    let mut vals: Vec<f64> = (0..n).map(|i| aa[i * n + i]).collect();
    let mut out: Vec<Vec<f64>> = (0..n).map(|j| normalise(backward_t(&(0..n).map(|i| v[i * n + j]).collect::<Vec<_>>()))).collect();
    sort_and_fix_signs(&mut vals, &mut out, n);
    let mut small = m[0];
    for &d in &piv {
        if d < small {
            small = d;
        }
    }
    (vals, from_columns(&out, n), vec![small])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_and_big() {
        let a = [4.0, 1.0, 2.0, 1.0, 3.0, 0.5, 2.0, 0.5, 5.0];
        assert_eq!(det(&a, 3), 4.0 * (15.0 - 0.25) - 1.0 * (5.0 - 1.0) + 2.0 * (0.5 - 6.0));
        let x = solve(&a, 3, &[1.0, 2.0, 3.0], 1).unwrap();
        let back = matmul(&a, 3, 3, &x, 1);
        for (b, w) in back.iter().zip([1.0, 2.0, 3.0]) {
            assert!((b - w).abs() < 1e-14);
        }
        let (vals, _) = eigen_sym(&a, 3).unwrap();
        let tr: f64 = vals.iter().sum();
        assert!((tr - 12.0).abs() < 1e-13);
        assert_eq!(solve(&[1.0, 2.0, 2.0, 4.0], 2, &[1.0, 1.0], 1).unwrap_err().kind, err::SINGULAR);
        assert_eq!(eigen_sym(&[1.0, 2.0, 3.0, 4.0], 2).unwrap_err().kind, err::NOT_SYMMETRIC);
    }
}
