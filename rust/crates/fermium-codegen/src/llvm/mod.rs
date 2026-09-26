//! The LLVM back end (spec §B4, §B5.3): compiles the typed IR to LLVM IR with inkwell, JIT-compiles it with
//! MCJIT (LLVM 18, statically linked) and runs it. The compiled code calls back into Rust (`rt.rs`) for
//! printing, lists, run-time errors and the built-ins it doesn't compile itself, like v1's compiled code called
//! fermium/runtime/core.py.
//!
//! It must print exactly what the tree-walker prints. A module with a construct it can't compile yet is
//! rejected up front (`supports`), before anything runs, and the CLI runs the tree-walker instead.
pub mod compile;
pub mod rt;

use std::sync::Once;

use inkwell::context::Context;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::OptimizationLevel;

use fermium_ir::Module;

use crate::eval::{Printer, RunError};
use crate::Backend;

pub fn llvm_version() -> &'static str {
    env!("FERMIUM_LLVM_VERSION")
}

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        Target::initialize_native(&InitializationConfig::default()).expect("LLVM native target");
        inkwell::execution_engine::ExecutionEngine::link_in_mc_jit();
    });
}

/// Ok if the LLVM back end compiles every construct of the module; else what it can't compile yet.
pub fn supports(module: &Module) -> Result<(), String> {
    init();
    let cx = Context::create();
    let mut g = compile::Gen::new(&cx, module, 0);
    g.compile_module()
}

fn target_machine() -> Result<TargetMachine, String> {
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let cpu = TargetMachine::get_host_cpu_name();
    let features = TargetMachine::get_host_cpu_features();
    target.create_target_machine(&triple, cpu.to_str().unwrap_or(""), features.to_str().unwrap_or(""),
                                 OptimizationLevel::Default, RelocMode::Default, CodeModel::JITDefault)
        .ok_or_else(|| "no LLVM target machine for this computer".to_string())
}

/// Compile and run a module. Err(reason) when the back end can't compile it (nothing has run then);
/// Ok(result of the run) otherwise.
pub fn run_module(module: &Module, printer: &mut dyn Printer) -> Result<Result<(), RunError>, String> {
    init();
    let mut ctx = rt::Ctx::new(module, printer);
    let addr = &mut *ctx as *mut rt::Ctx as usize;
    let cx = Context::create();
    let mut g = compile::Gen::new(&cx, module, addr);
    g.compile_module()?;
    let timing = std::env::var_os("FERMIUM_LLVM_TIME").is_some();
    let t0 = std::time::Instant::now();
    if std::env::var_os("FERMIUM_DUMP_LLVM").is_some() {
        eprintln!("{}", g.lm.print_to_string().to_string());
    }
    g.lm.verify().map_err(|e| format!("LLVM verifier: {}", e.to_string()))?;
    let tm = target_machine()?;
    g.lm.set_triple(&tm.get_triple());
    g.lm.set_data_layout(&tm.get_target_data().get_data_layout());
    g.lm.run_passes("default<O2>", &tm, PassBuilderOptions::create()).map_err(|e| e.to_string())?;
    if std::env::var_os("FERMIUM_DUMP_LLVM_OPT").is_some() {
        eprintln!("{}", g.lm.print_to_string().to_string());
    }
    let t1 = std::time::Instant::now();
    let tables = std::mem::take(&mut g.tables);
    ctx.texts = tables.texts;
    ctx.builtins = tables.builtins;
    ctx.mvec_fmts = tables.mvec_fmts;
    let ee = g.lm.create_jit_execution_engine(OptimizationLevel::Default).map_err(|e| e.to_string())?;
    // (the optimizer has removed the declarations of callbacks the program doesn't use)
    for (name, a) in &g.mappings {
        if let Some(f) = g.lm.get_function(name) {
            ee.add_global_mapping(&f, *a);
        }
    }
    let main = unsafe { ee.get_function::<unsafe extern "C" fn()>("fm_main") }.map_err(|e| e.to_string())?;
    let t2 = std::time::Instant::now();
    unsafe { main.call() };
    if timing {
        eprintln!("llvm: codegen+opt {:.2} ms, jit {:.2} ms, run {:.2} ms", (t1 - t0).as_secs_f64() * 1e3,
                  (t2 - t1).as_secs_f64() * 1e3, t2.elapsed().as_secs_f64() * 1e3);
    }
    Ok(match ctx.error.take() {
        Some(e) => Err(ctx.locate(e)),
        None => Ok(()),
    })
}

/// The LLVM back end behind the `Backend` trait.
pub struct LlvmBackend;

impl Backend for LlvmBackend {
    fn name(&self) -> &'static str {
        "llvm"
    }
    fn supports(&self, module: &Module) -> Result<(), String> {
        supports(module)
    }
    fn run(&mut self, module: &Module, printer: &mut dyn Printer) -> Result<(), RunError> {
        match run_module(module, printer) {
            Ok(r) => r,
            Err(why) => Err(RunError { message: format!("the LLVM back end can't compile this program yet: {why}"),
                                       line: 0, hint: None }),
        }
    }
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use inkwell::OptimizationLevel;

    #[test]
    fn jit_smoke() {
        super::init();
        let ctx = Context::create();
        let m = ctx.create_module("t");
        let f64t = ctx.f64_type();
        let f = m.add_function("add", f64t.fn_type(&[f64t.into(), f64t.into()], false), None);
        let b = ctx.create_builder();
        b.position_at_end(ctx.append_basic_block(f, "e"));
        let s = b.build_float_add(f.get_nth_param(0).unwrap().into_float_value(),
                                  f.get_nth_param(1).unwrap().into_float_value(), "s").unwrap();
        b.build_return(Some(&s)).unwrap();
        let ee = m.create_jit_execution_engine(OptimizationLevel::Default).unwrap();
        let add = unsafe { ee.get_function::<unsafe extern "C" fn(f64, f64) -> f64>("add").unwrap() };
        assert_eq!(unsafe { add.call(1.5, 2.25) }, 3.75);
    }
}
