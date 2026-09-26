//! The rounding of the NumPy / SciPy operations v1's stiff solvers use, as they run on the oracle's
//! platform (NumPy 2 with scipy-openblas 0.3.31, SkylakeX kernels: FMA, AVX-512).
//!
//! v1 steps `using radau` with SciPy's Radau, whose Newton iteration and error estimate involve cancellation:
//! a rounding difference of 10⁻¹⁶ in a dot product or an LU shows up as 10⁻⁸ in the error norm, and after
//! thousands of steps as a different step count (21542a0d1762: 9983 vs 10018 steps). So the port reproduces
//! each operation's evaluation order and fused multiply-adds. Every model here was found by comparing with
//! NumPy/SciPy on random inputs (the probes are described in NUMERICS.md); "exact" below means bit-identical
//! on all probes. Outside the ranges marked exact the model is the closest found.
//!
//! - `x.dot(y)` (ddot): FMA chain from 0 for n < 16; for n ≥ 16 the AVX-512/AVX2 accumulators. Exact, all n.
//! - `A.T.dot(v)` with A (3, n), v (3,) (dgemv): per column a·v; n ≤ 3 an FMA chain, else the first
//!   4⌊n/4⌋ columns fma(a0,v0, a1 v1) + a2 v2 and the rest a chain. Exact for n ≤ 13.
//! - the same with v complex (NumPy casts to complex: zgemv): n = 1 a chain, n ≤ 3 plain sums, else the first
//!   4⌊n/4⌋ columns fma(a1,v1, a0 v0) + a2 v2 and the rest plain. Exact for n ≤ 13.
//! - 3×3 by 3×n products and Q (n, 3) by (3, 3) (dgemm): FMA chains. Exact for n ≤ 8.
//! - Q (n, 3) · p (3,) (dgemv 'T', contiguous rows): fma(q2,p2, fma(q0,p0, q1 p1)). Exact for n ≤ 8.
//! - complex × complex array (NumPy's SIMD loop): re = fma(ar, br, −(ai bi)), im = fma(ar, bi, ai br).
//! - `lu_factor` real (OpenBLAS dgetf2, left-looking): exact for n ≤ 5.
//! - `lu_solve` real (dgetrs): exact for n ≤ 6.
//! - `lu_factor` complex (zgetf2): exact for n ≤ 6. `lu_solve` complex (zgetrs): exact for n ≤ 4.
use super::dense::C64;

#[inline]
fn fma(a: f64, b: f64, c: f64) -> f64 {
    a.mul_add(b, c)
}

/// NumPy's `x.dot(y)` for 1-D float64 arrays (OpenBLAS SkylakeX ddot).
pub fn ddot(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len();
    let n1 = n & !15;
    let mut dot = 0.0;
    if n1 > 0 {
        let mut i = 0;
        let mut acc5 = [[0.0f64; 8]; 4];
        let n32 = n1 & !31;
        while i < n32 {
            for (a, acc) in acc5.iter_mut().enumerate() {
                for (l, v) in acc.iter_mut().enumerate() {
                    *v = fma(x[i + 8 * a + l], y[i + 8 * a + l], *v);
                }
            }
            i += 32;
        }
        let mut acc = [[0.0f64; 4]; 4];
        for a in 0..4 {
            for l in 0..4 {
                acc[a][l] = acc5[a][l] + acc5[a][l + 4];
            }
        }
        while i < n1 {
            for (a, ac) in acc.iter_mut().enumerate() {
                for (l, v) in ac.iter_mut().enumerate() {
                    *v = fma(x[i + 4 * a + l], y[i + 4 * a + l], *v);
                }
            }
            i += 16;
        }
        let a0: Vec<f64> = (0..4).map(|l| ((acc[0][l] + acc[1][l]) + acc[2][l]) + acc[3][l]).collect();
        dot = (a0[0] + a0[2]) + (a0[1] + a0[3]);
    }
    for i in n1..n {
        dot = fma(y[i], x[i], dot);
    }
    dot
}

/// SciPy's `norm(x) = np.linalg.norm(x) / x.size ** 0.5`.
pub fn norm(x: &[f64]) -> f64 {
    ddot(x, x).sqrt() / (x.len() as f64).powf(0.5)
}

/// Column j of `A.T.dot(v)` for A of shape (3, n): a = A[:, j].
#[inline]
pub fn gemv_t3(a: [f64; 3], v: &[f64; 3], n: usize, j: usize) -> f64 {
    gemv_t(&a, v, n, j)
}

fn chain(a: &[f64], v: &[f64]) -> f64 {
    let mut s = a[0] * v[0];
    for i in 1..a.len() {
        s = fma(a[i], v[i], s);
    }
    s
}

/// Entry j of `A.T.dot(v)` for a C-contiguous A of shape (k, n) (OpenBLAS dgemv): a = A[:, j], v of length k.
/// n = 1 and the rows past the last block of 4 are an FMA chain; rows in blocks of 4 take the columns in
/// chunks of 4, 2 and 1 (fma(a3,v3, fma(a2,v2, fma(a0,v0, a1 v1))), fma(a0,v0, a1 v1), a0 v0), summed in order;
/// 2 ≤ n ≤ 3 has its own kernel (a chain for k ≤ 3, pairs for k = 4, 5). Exact for k ≤ 6, n ≤ 8 (k = 6 with
/// n = 2, 3 approximate).
pub fn gemv_t(a: &[f64], v: &[f64], n: usize, j: usize) -> f64 {
    let k = a.len();
    if k == 0 {
        return 0.0;
    }
    if k == 1 {
        return a[0] * v[0];
    }
    if n == 1 || (n > 3 && j >= 4 * (n / 4)) {
        return chain(a, v);
    }
    if n > 3 {
        let mut total: Option<f64> = None;
        let mut i = 0;
        while i < k {
            let left = k - i;
            let part = if left >= 4 {
                let t = fma(a[i + 3], v[i + 3], fma(a[i + 2], v[i + 2], fma(a[i], v[i], a[i + 1] * v[i + 1])));
                i += 4;
                t
            } else if left >= 2 {
                let t = fma(a[i], v[i], a[i + 1] * v[i + 1]);
                i += 2;
                t
            } else {
                let t = a[i] * v[i];
                i += 1;
                t
            };
            total = Some(match total {
                None => part,
                Some(s) => s + part,
            });
        }
        return total.unwrap();
    }
    // 2 ≤ n ≤ 3
    match k {
        4 => fma(a[0], v[0], a[1] * v[1]) + fma(a[2], v[2], a[3] * v[3]),
        5 => fma(a[4], v[4], fma(a[0], v[0], a[1] * v[1]) + fma(a[2], v[2], a[3] * v[3])),
        _ => chain(a, v),
    }
}

/// Entry (i, j) of a small `A.dot(B)` (dgemm) with a = row i of A and b = column j of B: an FMA chain.
pub fn gemm_entry(a: &[f64], b: &[f64]) -> f64 {
    chain(a, b)
}

/// One part (re or im) of column j of `A.T.dot(v)` with A real (3, n) and v complex.
#[inline]
pub fn cgemv_t3(a: [f64; 3], v: &[f64; 3], n: usize, j: usize) -> f64 {
    if n == 1 {
        fma(a[2], v[2], fma(a[1], v[1], a[0] * v[0]))
    } else if n > 3 && j < 4 * (n / 4) {
        fma(a[1], v[1], a[0] * v[0]) + a[2] * v[2]
    } else {
        (a[0] * v[0] + a[1] * v[1]) + a[2] * v[2]
    }
}

/// A 3-term dot as in a small dgemm (FMA chain in index order).
#[inline]
pub fn chain3(a: [f64; 3], b: [f64; 3]) -> f64 {
    fma(a[2], b[2], fma(a[1], b[1], a[0] * b[0]))
}

/// `np.dot(Q, p)` for Q (n, 3) and a vector p (3,): one entry.
#[inline]
pub fn qdotp(q: &[f64; 3], p: &[f64; 3]) -> f64 {
    fma(q[2], p[2], fma(q[0], p[0], q[1] * p[1]))
}

/// NumPy's complex multiply of arrays.
#[inline]
pub fn cmul(a: C64, b: C64) -> C64 {
    C64::new(fma(a.re, b.re, -(a.im * b.im)), fma(a.re, b.im, a.im * b.re))
}

/// SciPy's `lu_factor` of a real n×n matrix (row-major): the packed LU and the pivots (row i was swapped with
/// piv[i]); None if a pivot is zero or not finite (LAPACK reports it; the solvers treat it as a failure).
pub fn lu_real(a: &[f64], n: usize) -> (Vec<f64>, Vec<usize>, bool) {
    let mut a = a.to_vec();
    let mut piv = Vec::with_capacity(n);
    let mut singular = false;
    let at = |a: &Vec<f64>, i: usize, j: usize| a[i * n + j];
    for j in 0..n {
        let mut b: Vec<f64> = (0..n).map(|i| at(&a, i, j)).collect();
        for i in 0..j {
            b.swap(i, piv[i]);
        }
        for i in 1..j {
            let mut d = 0.0;
            for k in 0..i {
                d = fma(at(&a, i, k), b[k], d);
            }
            b[i] -= d;
        }
        for i in j..n {
            let mut d = 0.0;
            for k in 0..j {
                d = fma(at(&a, i, k), b[k], d);
            }
            b[i] -= d;
        }
        let mut jp = j;
        for i in j + 1..n {
            if b[i].abs() > b[jp].abs() {
                jp = i;
            }
        }
        piv.push(jp);
        for i in 0..n {
            a[i * n + j] = b[i];
        }
        if b[jp] != 0.0 {
            if jp != j {
                for k in 0..=j {
                    a.swap(j * n + k, jp * n + k);
                }
            }
            let r = 1.0 / a[j * n + j];
            for i in j + 1..n {
                a[i * n + j] *= r;
            }
        } else {
            singular = true;
        }
    }
    (a, piv, singular)
}

/// SciPy's `lu_solve` (dgetrs) with the packed LU and pivots of [`lu_real`].
pub fn lu_solve_real(lu: &[f64], piv: &[usize], n: usize, b: &mut [f64]) {
    for i in 0..n {
        b.swap(i, piv[i]);
    }
    for j in 0..n {
        for i in j + 1..n {
            b[i] = fma(-b[j], lu[i * n + j], b[i]);
        }
    }
    for j in (0..n).rev() {
        b[j] /= lu[j * n + j];
        for i in 0..j {
            b[i] = fma(-b[j], lu[i * n + j], b[i]);
        }
    }
}

/// LAPACK's reciprocal of a complex pivot in zgetf2 (ratio form).
fn crecip(t1: f64, t2: f64) -> C64 {
    if t1.abs() >= t2.abs() {
        let ratio = t2 / t1;
        let den = 1.0 / (t1 * (1.0 + ratio * ratio));
        C64::new(den, -ratio * den)
    } else {
        let ratio = t1 / t2;
        let den = 1.0 / (t2 * (1.0 + ratio * ratio));
        C64::new(ratio * den, -den)
    }
}

/// SciPy's `lu_factor` of a complex matrix (OpenBLAS zgetf2).
pub fn lu_complex(a: &[C64], n: usize) -> (Vec<C64>, Vec<usize>, bool) {
    let mut a = a.to_vec();
    let mut piv = Vec::with_capacity(n);
    let mut singular = false;
    for j in 0..n {
        let mut b: Vec<C64> = (0..n).map(|i| a[i * n + j]).collect();
        for i in 0..j {
            b.swap(i, piv[i]);
        }
        // the triangular part: b[i] -= Σ a[i][k] b[k], one term at a time
        for i in 1..j {
            let (mut re, mut im) = (b[i].re, b[i].im);
            for k in 0..i {
                let (x, y) = (a[i * n + k], b[k]);
                re -= fma(x.re, y.re, -(x.im * y.im));
                im -= fma(x.im, y.re, x.re * y.im);
            }
            b[i] = C64::new(re, im);
        }
        // the gemv part: rows in blocks of 4 (two accumulators, plain first product) and the leftover rows
        let rows = n - j;
        for i in j..n {
            let r = i - j;
            if r < 4 * (rows / 4) {
                let (mut p, mut q, mut s1, mut s2) = (0.0, 0.0, 0.0, 0.0);
                for k in 0..j {
                    let (x, y) = (a[i * n + k], b[k]);
                    if k == 0 {
                        p = x.re * y.re;
                        q = x.im * y.im;
                        s1 = x.re * y.im;
                        s2 = x.im * y.re;
                    } else {
                        p = fma(x.re, y.re, p);
                        q = fma(x.im, y.im, q);
                        s1 = fma(x.re, y.im, s1);
                        s2 = fma(x.im, y.re, s2);
                    }
                }
                b[i] = C64::new(b[i].re - (p - q), b[i].im - (s1 + s2));
            } else {
                let (mut tr, mut ti) = (0.0, 0.0);
                for k in 0..j {
                    let (x, y) = (a[i * n + k], b[k]);
                    tr += fma(x.re, y.re, -(x.im * y.im));
                    ti += fma(x.re, y.im, x.im * y.re);
                }
                b[i] = C64::new(b[i].re - tr, b[i].im - ti);
            }
        }
        let mut jp = j;
        for i in j + 1..n {
            if b[i].abs1() > b[jp].abs1() {
                jp = i;
            }
        }
        piv.push(jp);
        for i in 0..n {
            a[i * n + j] = b[i];
        }
        let t = a[jp * n + j];
        if t.re != 0.0 || t.im != 0.0 {
            if jp != j {
                for k in 0..=j {
                    a.swap(j * n + k, jp * n + k);
                }
            }
            let r = crecip(t.re, t.im);
            for i in j + 1..n {
                let x = a[i * n + j];
                a[i * n + j] = C64::new(r.re * x.re - r.im * x.im, r.im * x.re + r.re * x.im);
            }
        } else {
            singular = true;
        }
    }
    (a, piv, singular)
}

/// SciPy's `lu_solve` for a complex LU (zgetrs).
pub fn lu_solve_complex(lu: &[C64], piv: &[usize], n: usize, b: &mut [C64]) {
    for i in 0..n {
        b.swap(i, piv[i]);
    }
    let sub = |bi: C64, a: C64, x: C64| -> C64 {
        C64::new(bi.re - fma(a.re, x.re, -(a.im * x.im)), bi.im - fma(a.im, x.re, a.re * x.im))
    };
    for j in 0..n {
        for i in j + 1..n {
            b[i] = sub(b[i], lu[i * n + j], b[j]);
        }
    }
    for j in (0..n).rev() {
        let u = lu[j * n + j];
        let r = crecip(u.re, u.im);
        let x = b[j];
        b[j] = C64::new(r.re * x.re - r.im * x.im, r.re * x.im + r.im * x.re);
        for i in 0..j {
            b[i] = sub(b[i], lu[i * n + j], b[j]);
        }
    }
}

/// A real LU as SciPy's `lu_factor` returns it.
#[derive(Debug, Clone)]
pub struct RealLu {
    pub a: Vec<f64>,
    pub piv: Vec<usize>,
    pub n: usize,
}

impl RealLu {
    /// None if the matrix has an infinite or NaN entry (SciPy's check_finite raises ValueError).
    pub fn new(a: &[f64], n: usize) -> Option<Self> {
        if a.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let (a, piv, _) = lu_real(a, n);
        Some(RealLu { a, piv, n })
    }
    pub fn solve(&self, b: &mut [f64]) {
        lu_solve_real(&self.a, &self.piv, self.n, b)
    }
}

/// A complex LU as SciPy's `lu_factor` returns it.
#[derive(Debug, Clone)]
pub struct ComplexLu {
    pub a: Vec<C64>,
    pub piv: Vec<usize>,
    pub n: usize,
}

impl ComplexLu {
    pub fn new(a: &[C64], n: usize) -> Option<Self> {
        if a.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let (a, piv, _) = lu_complex(a, n);
        Some(ComplexLu { a, piv, n })
    }
    pub fn solve(&self, b: &mut [C64]) {
        lu_solve_complex(&self.a, &self.piv, self.n, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddot_matches_numpy() {
        // python3 -c "import numpy as np; x = np.arange(1, 21) / 7; print(repr(float(x.dot(x[::-1]))))"
        let x: Vec<f64> = (1..=20).map(|k| k as f64 / 7.0).collect();
        let y: Vec<f64> = x.iter().rev().copied().collect();
        assert_eq!(ddot(&x, &y), 31.42857142857143);
        let x3 = [0.1, 0.2, 0.3];
        assert_eq!(ddot(&x3, &x3), 0.14);
    }
}

/// NumPy's `np.sum` of a contiguous float64 array: pairwise summation (blocks of 128 with 8 accumulators).
pub fn np_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut r = 0.0;
        for &v in a {
            r += v;
        }
        return r;
    }
    if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - n % 8 {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        return res;
    }
    let mut n2 = n / 2;
    n2 -= n2 % 8;
    np_sum(&a[..n2]) + np_sum(&a[n2..])
}
