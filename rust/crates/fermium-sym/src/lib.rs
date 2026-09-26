//! fermium-sym: symbolic calculus on the AST (spec §B4): derivatives, simplification, the stable evaluation
//! form of a derivative, printing formulas, isolating the highest derivative of an ODE, and native indefinite
//! integrals. A port of `fermium/calculus.py` from Fermium 1.5 (the oracle); the SymPy bridge of v1 is replaced
//! by native rules (spec §B6, rust/DIVERGENCES.md).
//!
//! The API, by Python name:
//!
//! | Rust                                   | calculus.py            |
//! |----------------------------------------|------------------------|
//! | [`diff`]`(e, var, ctx)`                | `diff`                 |
//! | [`d`]`(e, var, ctx)` (unsimplified)    | `_d`                   |
//! | [`DiffContext`], [`Plain`]             | `DiffContext`, `Plain` |
//! | [`simplify`], [`factor_common`]        | same                   |
//! | [`stabilize`]                          | same                   |
//! | [`to_source`], [`to_source_p`], [`key`]| `to_source`, `key`     |
//! | [`isolate`], [`linear_coeffs`]         | same                   |
//! | [`subst`], [`subst1`], [`map_children`], [`inline_where`], [`depends_on`], [`free_names`] | same |
//! | [`integrate`]                          | `integrate_symbolic`   |
//!
//! Errors are `fermium_syntax::diag::Diagnostic`s (Python FermiumError): a node made here has no position,
//! so errors about it have `line: None`, and callers fill the position in, as the Python checker does.
pub mod ad;
pub mod build;
pub mod diff;
pub mod integrate;
pub mod numeval;
pub mod ode;
pub mod simplify;
pub mod source;
pub mod tidy;
pub mod walk;

pub use ad::ad_body;
pub use build::SymResult;
pub use diff::{builtin_deriv, d, diff, tidy, DiffContext, Plain, BUILTIN_DERIV_NAMES};
pub use integrate::integrate;
pub use ode::{isolate, isolate_with, linear_coeffs};
pub use simplify::{factor_common, simplify, stabilize};
pub use source::{canonical_unit_name, key, to_source, to_source_p};
pub use walk::{depends_on, free_names, inline_where, map_children, subst, subst1};

#[cfg(test)]
mod tests;
