//! Small dense linear algebra used by the solvers: LU with partial pivoting (LAPACK getrf's
//! pivot choice: the first largest |a|, or |re| + |im| for complex) and a minimal complex type.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A complex number (just what the solvers need).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    pub const fn new(re: f64, im: f64) -> Self {
        C64 { re, im }
    }
    pub fn abs1(self) -> f64 {
        self.re.abs() + self.im.abs()
    }
    pub fn scale(self, s: f64) -> Self {
        C64::new(self.re * s, self.im * s)
    }
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn conj(self) -> Self {
        C64::new(self.re, -self.im)
    }
}

impl Add for C64 {
    type Output = C64;
    fn add(self, o: C64) -> C64 {
        C64::new(self.re + o.re, self.im + o.im)
    }
}
impl Sub for C64 {
    type Output = C64;
    fn sub(self, o: C64) -> C64 {
        C64::new(self.re - o.re, self.im - o.im)
    }
}
impl Mul for C64 {
    type Output = C64;
    fn mul(self, o: C64) -> C64 {
        C64::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}
impl Neg for C64 {
    type Output = C64;
    fn neg(self) -> C64 {
        C64::new(-self.re, -self.im)
    }
}
impl Div for C64 {
    type Output = C64;
    /// Smith's algorithm (as LAPACK's zladiv / gfortran's complex division, up to rounding)
    fn div(self, o: C64) -> C64 {
        let (a, b, c, d) = (self.re, self.im, o.re, o.im);
        if d.abs() <= c.abs() {
            let r = d / c;
            let den = c + d * r;
            C64::new((a + b * r) / den, (b - a * r) / den)
        } else {
            let r = c / d;
            let den = c * r + d;
            C64::new((a * r + b) / den, (b * r - a) / den)
        }
    }
}

/// Scalars the LU works on.
pub trait Scalar: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self> + PartialEq {
    fn zero() -> Self;
    /// the size used to choose pivots (LAPACK's idamax / izamax)
    fn pivot_size(self) -> f64;
    fn finite(self) -> bool;
}

impl Scalar for f64 {
    fn zero() -> Self {
        0.0
    }
    fn pivot_size(self) -> f64 {
        self.abs()
    }
    fn finite(self) -> bool {
        self.is_finite()
    }
}

impl Scalar for C64 {
    fn zero() -> Self {
        C64::default()
    }
    fn pivot_size(self) -> f64 {
        self.abs1()
    }
    fn finite(self) -> bool {
        self.is_finite()
    }
}

/// An LU factorisation P A = L U of an n×n matrix (row-major), as SciPy's `lu_factor`.
#[derive(Debug, Clone)]
pub struct Lu<T> {
    pub n: usize,
    pub lu: Vec<T>,
    pub piv: Vec<usize>,
    /// a zero pivot was met (the matrix is singular; solving divides by zero, as LAPACK)
    pub singular: bool,
}

impl<T: Scalar> Lu<T> {
    /// Factor `a` (row-major n×n). None if it contains NaN or ∞ (SciPy's `check_finite` raises).
    pub fn new(mut a: Vec<T>, n: usize) -> Option<Self> {
        if a.iter().any(|v| !v.finite()) {
            return None;
        }
        let mut piv = vec![0usize; n];
        let mut singular = false;
        for k in 0..n {
            let mut p = k;
            let mut best = a[k * n + k].pivot_size();
            for i in k + 1..n {
                let s = a[i * n + k].pivot_size();
                if s > best {
                    best = s;
                    p = i;
                }
            }
            piv[k] = p;
            if p != k {
                for j in 0..n {
                    a.swap(k * n + j, p * n + j);
                }
            }
            let d = a[k * n + k];
            if d == T::zero() {
                singular = true;
                continue;
            }
            for i in k + 1..n {
                let l = a[i * n + k] / d;
                a[i * n + k] = l;
                if l != T::zero() {
                    for j in k + 1..n {
                        let v = a[i * n + j] - l * a[k * n + j];
                        a[i * n + j] = v;
                    }
                }
            }
        }
        Some(Lu { n, lu: a, piv, singular })
    }

    /// Solve A x = b in place.
    pub fn solve(&self, b: &mut [T]) {
        let n = self.n;
        for k in 0..n {
            let p = self.piv[k];
            if p != k {
                b.swap(k, p);
            }
        }
        for i in 0..n {
            let mut s = b[i];
            for j in 0..i {
                s = s - self.lu[i * n + j] * b[j];
            }
            b[i] = s;
        }
        for i in (0..n).rev() {
            let mut s = b[i];
            for j in i + 1..n {
                s = s - self.lu[i * n + j] * b[j];
            }
            b[i] = s / self.lu[i * n + i];
        }
    }

    /// The determinant (product of the pivots, with the permutation's sign).
    pub fn det(&self) -> T
    where
        T: Neg<Output = T>,
    {
        let n = self.n;
        let mut d = self.lu[0];
        for i in 1..n {
            d = d * self.lu[i * n + i];
        }
        let swaps = self.piv.iter().enumerate().filter(|(k, p)| *k != **p).count();
        if swaps % 2 == 1 { -d } else { d }
    }
}
