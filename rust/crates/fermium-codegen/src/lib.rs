//! fermium-codegen: the back ends, behind one trait (spec §B4). `eval` is the tree-walking back end (a port of
//! interp.py, the reference); the LLVM back end (`llvm`, inkwell over LLVM 18, statically linked) implements the
//! same trait and must print identically.
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
pub mod session;
#[cfg(feature = "llvm")]
pub mod llvm;

use eval::{Printer, RunError};
use fermium_ir::Module;

/// A back end runs a checked module, printing through the runtime's printer (the formats are in
/// `module.tables.fmts`). Everything else a back end needs at run time (lists, texts, errors with their line,
/// numerics) is its own business: the tree-walker has it in Rust values, the LLVM back end in a context its
/// compiled code calls back into (`llvm::rt`).
pub trait Backend {
    fn name(&self) -> &'static str;
    /// Ok if this back end can run every construct of the module (else what it can't run yet): the CLI picks
    /// the LLVM back end only when it supports the whole module, so a missing construct never changes output.
    fn supports(&self, module: &Module) -> Result<(), String>;
    /// Run the module's main program; a run-time error stops it.
    fn run(&mut self, module: &Module, printer: &mut dyn Printer) -> Result<(), RunError>;
}

impl<P: Printer + ?Sized> Printer for &mut P {
    fn num(&mut self, fmt: usize, v: f64) {
        (**self).num(fmt, v)
    }
    fn num_capped(&mut self, fmt: usize, v: f64, max_sf: u32) {
        (**self).num_capped(fmt, v, max_sf)
    }
    fn num_sf(&mut self, fmt: usize, v: f64, sf: u32) {
        (**self).num_sf(fmt, v, sf)
    }
    fn list(&mut self, fmt: usize, v: &[f64]) {
        (**self).list(fmt, v)
    }
    fn vec(&mut self, fmt: usize, v: &[f64]) {
        (**self).vec(fmt, v)
    }
    fn mixed_vec(&mut self, fmts: &[usize], v: &[f64]) {
        (**self).mixed_vec(fmts, v)
    }
    fn mat(&mut self, fmt: usize, v: &[f64], r: usize, c: usize) {
        (**self).mat(fmt, v, r, c)
    }
    fn complex(&mut self, fmt: usize, re: f64, im: f64) {
        (**self).complex(fmt, re, im)
    }
    fn clist(&mut self, fmt: usize, v: &[(f64, f64)]) {
        (**self).clist(fmt, v)
    }
    fn boolean(&mut self, b: bool) {
        (**self).boolean(b)
    }
    fn text(&mut self, s: &str) {
        (**self).text(s)
    }
    fn textlist(&mut self, v: &[std::rc::Rc<str>]) {
        (**self).textlist(v)
    }
    fn end(&mut self) {
        (**self).end()
    }
}

/// The tree-walking back end (eval.rs) behind the `Backend` trait: it runs everything the checker produces.
pub struct InterpBackend;

impl Backend for InterpBackend {
    fn name(&self) -> &'static str {
        "interp"
    }
    fn supports(&self, _module: &Module) -> Result<(), String> {
        Ok(())
    }
    fn run(&mut self, module: &Module, printer: &mut dyn Printer) -> Result<(), RunError> {
        eval::Interpreter::new(module, printer).run()
    }
}
