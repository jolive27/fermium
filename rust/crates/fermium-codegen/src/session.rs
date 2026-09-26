//! Hooks for the interactive tools (the REPL and the Jupyter kernel, fermium-repl): run one input after
//! another with the main program's variables and ODE solutions kept between inputs (v1's ReplSession keeps them
//! in an arena of globals), and print a module's LLVM IR (`fermium run --emit-llvm`).

use fermium_ir::{Module, SymId};

use crate::eval::{Interpreter, Printer, RunError, Value};

/// What the main program has computed so far: the value of each top-level variable, and the ODE, eigenvalue and
/// PDE solutions they refer to. The module grows input by input (symbol ids stay valid), so the state is kept
/// by symbol id.
#[derive(Default)]
pub struct ReplState {
    globals: crate::eval::SymMap<Value>,
    solve: crate::eval_solve::SolveState,
}

impl ReplState {
    pub fn new() -> ReplState {
        ReplState::default()
    }

    /// Run the module's main program (the latest input) with the tree-walker, starting from the values computed
    /// by earlier inputs; what it assigns is kept for the next input (also when it stops with an error part way,
    /// as v1's arena keeps it).
    pub fn run(&mut self, module: &Module, printer: &mut dyn Printer) -> Result<(), RunError> {
        let mut it = Interpreter::new(module, printer);
        it.globals = std::mem::take(&mut self.globals);
        it.solve = std::mem::take(&mut self.solve);
        let r = it.run();
        self.globals = std::mem::take(&mut it.globals);
        self.solve = std::mem::take(&mut it.solve);
        r
    }

    /// The value of a top-level variable, if it has one.
    pub fn value(&self, sym: SymId) -> Option<&Value> {
        self.globals.get(&sym)
    }
}

/// The LLVM IR of a module (before optimization), or why the LLVM back end can't compile it yet.
#[cfg(feature = "llvm")]
pub fn emit_llvm(module: &Module) -> Result<String, String> {
    crate::llvm::supports(module)?; // (also initializes LLVM)
    let cx = inkwell::context::Context::create();
    let mut g = crate::llvm::compile::Gen::new(&cx, module, 0);
    g.compile_module()?;
    Ok(g.lm.print_to_string().to_string())
}
