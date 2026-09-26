//! Native numerics for Fermium 2 (spec §B2, §B6).
//!
//! Each module ports the algorithm Fermium 1.5 uses (the oracle) where v1 implements it itself
//! (`fermium/interp.py` mirrors the compiled kernels operation for operation), and replaces SciPy /
//! NumPy with a native method that agrees to high accuracy where v1 called a library.
//! Accuracy per method is documented in `NUMERICS.md`; intentional differences in `NOTES.md`.

pub mod dense;
pub mod eigen;
pub mod fft;
pub mod fit;
pub mod linalg;
pub mod ode;
pub mod pde;
pub mod quad;
pub mod rng;
pub mod roots;
pub mod special;
pub mod stiff;
pub mod uncertain;

/// A run-time failure with v1's error kind and its two numbers (see `describe_error` in
/// `fermium/runtime/core.py`, which turns them into the message).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fail {
    pub kind: i64,
    pub a: f64,
    pub b: f64,
}

impl Fail {
    pub fn new(kind: i64, a: f64, b: f64) -> Self {
        Fail { kind, a, b }
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "run-time error kind {} ({}, {})", self.kind, self.a, self.b)
    }
}

impl std::error::Error for Fail {}

/// v1's run-time error kinds (`fermium/interp.py`, `fermium/runtime/stiff.py`).
pub mod err {
    pub const INDEX: i64 = 1;
    pub const SOLRANGE: i64 = 2;
    pub const ODE_STEPS: i64 = 3;
    pub const STEP: i64 = 7;
    pub const ODE_H: i64 = 8;
    pub const QUAD: i64 = 9;
    pub const SIZE: i64 = 11;
    pub const ROOT: i64 = 13;
    pub const POLE: i64 = 14;
    pub const SINGULAR: i64 = 15;
    pub const ODE_NAN: i64 = 16;
    pub const ODE_RANGE: i64 = 17;
    pub const NO_EVENT: i64 = 18;
    pub const NOT_SYMMETRIC: i64 = 21;
    pub const NOT_POSDEF: i64 = 22;
    pub const STIFF_STEPS: i64 = 23;
    pub const QUAD_NAN: i64 = 31;
    pub const QUAD_INF: i64 = 32;
    pub const ODE_SINGULAR: i64 = 33;
    pub const ODE_H_FLAT: i64 = 34;
    /// + 1 + the variable's text id: "too many steps" (RK45); a = reached, b = start (D214)
    pub const ODE_STEPS_FROM: i64 = 1_000_000;
    /// + 1 + the variable's text id: "too many steps" (stiff solvers)
    pub const STIFF_STEPS_FROM: i64 = 2_000_000;
}

/// Python's `max(a, b)` for floats: `a` unless `b > a` (so a NaN in `a` is kept, and one in `b` is
/// ignored). Unlike `f64::max`; v1's kernels rely on this order.
#[inline]
pub(crate) fn pymax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python's `min(a, b)` for floats: `a` unless `b < a`.
#[inline]
pub(crate) fn pymin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}
