//! The run time of natively compiled code: the LLVM JIT's and that of executables made by `fermium build`
//! (which carry it as a static library, fermium-aotrt). No LLVM here. The files live next to the back end.
#[path = "llvm/blob.rs"]
pub mod blob;
#[path = "llvm/rt.rs"]
pub mod rt;
#[path = "llvm/solve_rt.rs"]
pub mod solve_rt;
