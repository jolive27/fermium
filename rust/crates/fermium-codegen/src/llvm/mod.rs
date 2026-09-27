//! The LLVM back end (spec §B4, §B5.3): compiles the typed IR to LLVM IR with inkwell, JIT-compiles it with
//! MCJIT (LLVM 18, statically linked) and runs it. The compiled code calls back into Rust (`rt.rs`) for
//! printing, lists, run-time errors and the built-ins it doesn't compile itself, like v1's compiled code called
//! fermium/runtime/core.py.
//!
//! It must print exactly what the tree-walker prints. A module with a construct it can't compile yet is
//! rejected up front (`supports`), before anything runs, and the CLI runs the tree-walker instead.
pub mod compile;
pub mod link;
pub use crate::native::{blob, rt, solve_rt};

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
    guarded(|| g.compile_module())
}

/// Run a compile step; a panic in it (a bug in the back end) is reported as a construct it can't compile, so
/// the program still runs, in the tree-walker, instead of crashing (quietly: the panic message would change the
/// program's output).
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    match r {
        Ok(r) => r,
        Err(p) => {
            let what = p.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
            Err(format!("internal error in the LLVM back end ({what})"))
        }
    }
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
    guarded(|| g.compile_module())?;
    let timing = std::env::var_os("FERMIUM_LLVM_TIME").is_some();
    let t0 = std::time::Instant::now();
    if std::env::var_os("FERMIUM_DUMP_LLVM").is_some() {
        eprintln!("{}", g.lm.print_to_string().to_string());
    }
    g.lm.verify().map_err(|e| format!("LLVM verifier: {}", e.to_string()))?;
    let tm = target_machine()?;
    // this computer's CPU (the JIT's code runs only here): MCJIT's code generator follows these attributes
    {
        use inkwell::attributes::AttributeLoc;
        let cpu = TargetMachine::get_host_cpu_name().to_string();
        let features = TargetMachine::get_host_cpu_features().to_string();
        let mut f = g.lm.get_first_function();
        while let Some(fv) = f {
            if fv.count_basic_blocks() > 0 {
                fv.add_attribute(AttributeLoc::Function, cx.create_string_attribute("target-cpu", &cpu));
                fv.add_attribute(AttributeLoc::Function, cx.create_string_attribute("target-features", &features));
            }
            f = fv.get_next_function();
        }
    }
    g.lm.set_triple(&tm.get_triple());
    g.lm.set_data_layout(&tm.get_target_data().get_data_layout());
    let pipeline = std::env::var("FERMIUM_LLVM_PASSES").unwrap_or_else(|_| "default<O2>".into());
    let po = PassBuilderOptions::create();
    if std::env::var_os("FERMIUM_LLVM_NOVEC").is_some() {
        po.set_loop_vectorization(false);
    }
    g.lm.run_passes(&pipeline, &tm, po).map_err(|e| e.to_string())?;
    if std::env::var_os("FERMIUM_DUMP_LLVM_OPT").is_some() {
        eprintln!("{}", g.lm.print_to_string().to_string());
    }
    let t1 = std::time::Instant::now();
    ctx.set_tables(std::mem::take(&mut g.tables));
    let cg = if std::env::var_os("FERMIUM_LLVM_CG3").is_some() { OptimizationLevel::Aggressive } else { OptimizationLevel::Default };
    let ee = g.lm.create_jit_execution_engine(cg).map_err(|e| e.to_string())?;
    // (the optimizer has removed the declarations of callbacks the program doesn't use)
    for (name, a) in &g.mappings {
        if let Some(f) = g.lm.get_function(name) {
            ee.add_global_mapping(&f, *a);
        }
    }
    let main = unsafe { ee.get_function::<unsafe extern "C" fn()>("fm_main") }.map_err(|e| e.to_string())?;
    let t2 = std::time::Instant::now();
    unsafe { main.call() };
    ctx.report_gc();
    if timing {
        eprintln!("llvm: codegen+opt {:.2} ms, jit {:.2} ms, run {:.2} ms, {} integrand evaluations",
                  (t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3, t2.elapsed().as_secs_f64() * 1e3,
                  rt::QUAD_EVALS.load(std::sync::atomic::Ordering::Relaxed));
    }
    Ok(match ctx.error.take() {
        Some(e) => Err(ctx.locate(e)),
        None => Ok(()),
    })
}

/// Compile a module for an executable (`fermium build`): an object file (for this computer's target) that
/// defines `fm_main`, `fm_blob` and `fm_blob_len`, and reads the context from `fm_ctx`; the run time
/// (fermium-aotrt) provides the rest. Err(reason) when the back end can't compile the module.
pub fn build_object(module: &Module, source: &str, file_name: &str) -> Result<Vec<u8>, String> {
    use inkwell::targets::FileType;
    init();
    let cx = Context::create();
    let mut g = compile::Gen::new_aot(&cx, module);
    guarded(|| g.compile_module())?;
    let data = blob::write(module, &g.tables, source, file_name);
    let arr = cx.const_string(&data, false);
    let gb = g.lm.add_global(arr.get_type(), None, "fm_blob");
    gb.set_initializer(&arr);
    gb.set_constant(true);
    let gl = g.lm.add_global(cx.i64_type(), None, "fm_blob_len");
    gl.set_initializer(&cx.i64_type().const_int(data.len() as u64, false));
    gl.set_constant(true);
    g.lm.verify().map_err(|e| format!("LLVM verifier: {}", e.to_string()))?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    // the generic CPU of the target: an executable may run on another computer than the one it was built on
    let tm = target.create_target_machine(&triple, "", "", OptimizationLevel::Default, RelocMode::PIC,
                                          CodeModel::Default)
        .ok_or_else(|| "no LLVM target machine for this computer".to_string())?;
    g.lm.set_triple(&triple);
    g.lm.set_data_layout(&tm.get_target_data().get_data_layout());
    g.lm.run_passes("default<O2>", &tm, PassBuilderOptions::create()).map_err(|e| e.to_string())?;
    let buf = tm.write_to_memory_buffer(&g.lm, FileType::Object).map_err(|e| e.to_string())?;
    Ok(buf.as_slice().to_vec())
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
