//! Fermium 2's run-time library: native numerics (this crate replaces v1's SciPy/NumPy calls),
//! and later CSV, plotting and uncertainties.
//!
//! Everything in [`numerics`] works on plain `f64` values, slices and closures: the checker has
//! already verified and erased the units.

pub mod cffi;
pub mod data;
pub mod format;
pub mod numerics;
pub mod plot;
pub mod python;
pub mod vfs;
