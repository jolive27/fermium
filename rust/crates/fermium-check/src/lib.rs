//! fermium-check: names, types, dimensions and diagnostics (spec §B4). A port of `fermium/checker.py`,
//! `fermium/types.py` and friends from Fermium 1.5, producing the typed IR of fermium-ir.
//! See `checker.rs` for how the Python class is split into modules.
pub use fermium_ir::{types, Dim, DIMLESS};

pub mod analyze;
pub mod arith;
pub mod ast_ext;
pub mod builtin;
pub mod builtins;
pub mod calculus;
pub mod calls;
pub mod eigen;
pub mod checker;
pub mod clist;
pub mod convert;
pub mod cplx;
pub mod exprs;
pub mod lists;
pub mod modules;
pub mod names;
mod pending;
pub mod pde;
pub mod parallel;
pub mod print;
pub mod solve;
pub mod source;
pub mod stmts;
pub mod systems;
pub mod units;
pub mod vecmat;
pub mod walk;

pub use checker::{check, CheckOptions, Checker};

#[cfg(test)]
mod tests {
    use fermium_ir::types::*;
    use fermium_ir::{Dim, DIMLESS};

    #[test]
    fn energy_from_a_zero_start() {
        // E = 0 (unknown dimension) then E += ½ m v²: E learns it is an energy
        let mut u = Unifier::default();
        let e = DExpr::fresh();
        let kg = Dim::base(1);
        let m_per_s = Dim::base(0) / Dim::base(2);
        let energy = DExpr::of(kg * m_per_s * m_per_s);
        assert!(u.unify(&e, &energy));
        assert_eq!(u.resolve(&e), kg * m_per_s * m_per_s);
        assert!(!u.unify(&DExpr::of(kg), &DExpr::of(DIMLESS)));
    }
}
