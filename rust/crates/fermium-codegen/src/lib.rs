//! fermium-codegen: the back ends, behind one trait (spec §B4). `eval` is the tree-walking back end (a port of
//! interp.py, the reference); the LLVM back end (inkwell, LLVM 18, statically linked) implements the same trait
//! and must print identically.
pub mod eval;
mod eval_calc;
mod eval_complex;
mod eval_core;
mod eval_data;
mod eval_m3;
mod eval_more;
mod eval_par;
mod eval_solve;
mod eval_unc;
mod eval_vecmat;
pub mod printer;

use fermium_ir::Module;

/// A back end runs a checked module.
pub trait Backend {
    type Error;
    fn run(&mut self, module: &Module) -> Result<(), Self::Error>;
}
