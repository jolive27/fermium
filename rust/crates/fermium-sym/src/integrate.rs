//! Native indefinite integrals (replacing v1's SymPy bridge, spec §B6). Filled in below.
use fermium_syntax::ast as A;

use crate::build::*;

/// ∫ integrand d var (without +C), or an error suggesting a definite integral.
pub fn integrate(_integrand: &A::Expr, _var: &str, _positive: &[String]) -> SymResult<A::Expr> {
    Err(ferr0("Fermium couldn't find a formula for this integral",
              Some("give limits (from a to b) to compute it numerically".into())))
}
