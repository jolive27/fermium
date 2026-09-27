//! The LLVM back end (spec §B4, §B5.3): compiles the typed IR to LLVM IR with inkwell, JIT-compiles it with
//! MCJIT (LLVM 18, statically linked) and runs it. The compiled code calls back into Rust (`rt.rs`) for
//! printing, lists, run-time errors and the built-ins it doesn't compile itself, like v1's compiled code called
//! fermium/runtime/core.py.
//!
//! It must print exactly what the tree-walker prints. A module with a construct it can't compile yet is
//! rejected up front (`supports`), before anything runs, and the CLI runs the tree-walker instead.
pub mod cache;
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
        set_llvm_options();
    });
}

/// LLVM's own options, set once per process (D311). `force-ordered-reductions` lets the loop vectorizer vectorize
/// a loop whose floating-point sum must stay in source order (`s += f(j)`): the terms are computed several at a
/// time and added one by one in the same order, so the result is bit for bit the scalar loop's (no reassociation,
/// no fast-math). `FERMIUM_LLVM_ARGS` (space-separated) replaces the list: performance experiments only.
fn set_llvm_options() {
    use std::ffi::CString;
    let args: Vec<String> = match std::env::var("FERMIUM_LLVM_ARGS") {
        Ok(s) => s.split_whitespace().map(str::to_string).collect(),
        Err(_) => LLVM_OPTIONS.iter().map(|s| s.to_string()).collect(),
    };
    let mut all = vec![CString::new("fermium").unwrap()];
    all.extend(args.into_iter().filter_map(|a| CString::new(a).ok()));
    let ptrs: Vec<*const std::os::raw::c_char> = all.iter().map(|c| c.as_ptr()).collect();
    let overview = CString::new("").unwrap();
    // SAFETY: argv points at NUL-terminated strings that outlive the call; LLVM copies what it keeps
    unsafe { inkwell::llvm_sys::support::LLVMParseCommandLineOptions(ptrs.len() as i32, ptrs.as_ptr(), overview.as_ptr()) };
}

/// The LLVM options Fermium runs with (set_llvm_options).
pub const LLVM_OPTIONS: &[&str] = &["-force-ordered-reductions"];

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
    run_module_with(module, printer, None)
}

/// What a compilation for the compile cache gives back to be saved (cache.rs): the machine code and the run
/// time's tables (native::blob, with the program's source and file name).
pub struct Compiled {
    pub object: Vec<u8>,
    pub blob: Vec<u8>,
}

/// Can a module's machine code be saved and loaded by another run (D317)? Not when it calls Python, C or C++,
/// reads data files when checked, or has constructs the tree-walker runs (their code holds addresses of this
/// process).
fn cacheable(module: &Module, t: &blob::GenTables) -> bool {
    let mt = &module.tables;
    mt.pycalls.is_empty() && mt.ccalls.is_empty() && mt.loads.is_empty() && t.interp_sites.is_empty()
}

/// run_module; with `save` = Some((source, file name, save)), the code is compiled so that it can be saved
/// (Gen::new_reloc) and, when the module is cacheable, `save` gets the machine code before the program runs.
pub fn run_module_with(module: &Module, printer: &mut dyn Printer, save: Option<(&str, &str, &mut dyn FnMut(Compiled))>)
                       -> Result<Result<(), RunError>, String> {
    init();
    let mut ctx = rt::Ctx::new(module, printer);
    let addr = &mut *ctx as *mut rt::Ctx as usize;
    let cx = Context::create();
    let mut g = if save.is_some() { compile::Gen::new_reloc(&cx, module) } else { compile::Gen::new(&cx, module, addr) };
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
    } else if std::env::var_os("FERMIUM_LLVM_NOSLP").is_none() {
        // straight-line code on several independent values (x, y, z components) in vector registers (D311)
        po.set_loop_slp_vectorization(true);
    }
    g.lm.run_passes(&pipeline, &tm, po).map_err(|e| e.to_string())?;
    if std::env::var_os("FERMIUM_DUMP_LLVM_OPT").is_some() {
        eprintln!("{}", g.lm.print_to_string().to_string());
    }
    let t1 = std::time::Instant::now();
    let saving = match &save {
        Some((source, file_name, _)) if cacheable(module, &g.tables) => Some(blob::write(module, &g.tables, source, file_name)),
        _ => None,
    };
    ctx.set_tables(std::mem::take(&mut g.tables));
    let cg = if std::env::var_os("FERMIUM_LLVM_CG3").is_some() { OptimizationLevel::Aggressive } else { OptimizationLevel::Default };
    // (declared before the engine, so dropped after it)
    let oc = cache::ObjectCache::new(&[]);
    let slot = Box::new(addr);
    let ee = g.lm.create_jit_execution_engine(cg).map_err(|e| e.to_string())?;
    if saving.is_some() {
        oc.attach(&ee);
    }
    // (the optimizer has removed the declarations of callbacks the program doesn't use)
    for (name, a) in &g.mappings {
        if let Some(f) = g.lm.get_function(name) {
            ee.add_global_mapping(&f, *a);
        }
    }
    if let Some(gv) = g.lm.get_global("fm_ctx") {
        ee.add_global_mapping(&gv, &*slot as *const usize as usize);
    }
    let main = unsafe { ee.get_function::<unsafe extern "C" fn()>("fm_main") }.map_err(|e| e.to_string())?;
    if let (Some(blob), Some((_, _, save))) = (saving, save) {
        let object = oc.saved();
        if !object.is_empty() {
            save(Compiled { object, blob });
        }
    }
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

/// Run a program from the compile cache (D317): its machine code (`object`, saved by run_module_with) and the run
/// time's tables, read back from native::blob. Nothing is parsed, checked, generated or optimized.
pub fn run_object(module: &Module, tables: blob::GenTables, object: &[u8], printer: &mut dyn Printer,
                  before_run: &mut dyn FnMut())
                  -> Result<Result<(), RunError>, String> {
    use inkwell::module::Linkage;
    init();
    let t0 = std::time::Instant::now();
    let mut ctx = rt::Ctx::new(module, printer);
    let addr = &mut *ctx as *mut rt::Ctx as usize;
    let cx = Context::create();
    // the run-time callbacks' declarations (and addresses), and a definition of fm_main for MCJIT to find the
    // program by: its code comes from the object cache, not from this module
    let g = compile::Gen::new_reloc(&cx, module);
    let f = g.lm.add_function("fm_main", cx.void_type().fn_type(&[], false), Some(Linkage::External));
    let b = cx.create_builder();
    b.position_at_end(cx.append_basic_block(f, "entry"));
    b.build_return(None).map_err(|e| e.to_string())?;
    let tm = target_machine()?;
    g.lm.set_triple(&tm.get_triple());
    g.lm.set_data_layout(&tm.get_target_data().get_data_layout());
    let cg = if std::env::var_os("FERMIUM_LLVM_CG3").is_some() { OptimizationLevel::Aggressive } else { OptimizationLevel::Default };
    let oc = cache::ObjectCache::new(object);
    let slot = Box::new(addr);
    let ee = g.lm.create_jit_execution_engine(cg).map_err(|e| e.to_string())?;
    oc.attach(&ee);
    for (name, a) in &g.mappings {
        if let Some(f) = g.lm.get_function(name) {
            ee.add_global_mapping(&f, *a);
        }
    }
    if let Some(gv) = g.lm.get_global("fm_ctx") {
        ee.add_global_mapping(&gv, &*slot as *const usize as usize);
    }
    ctx.set_tables(tables);
    let main = unsafe { ee.get_function::<unsafe extern "C" fn()>("fm_main") }.map_err(|e| e.to_string())?;
    if !oc.saved().is_empty() {
        return Err("the cached machine code wasn't used".into());
    }
    before_run();
    let t1 = std::time::Instant::now();
    unsafe { main.call() };
    ctx.report_gc();
    if std::env::var_os("FERMIUM_LLVM_TIME").is_some() {
        eprintln!("llvm: cached code loaded in {:.2} ms, run {:.2} ms, {} integrand evaluations",
                  (t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3,
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
