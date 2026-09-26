//! The run time of an executable made by `fermium build` (spec B5.10): the C `main` of the executable. The
//! program's object file defines `fm_main` (the compiled program), `fm_blob` / `fm_blob_len` (its tables and
//! source, fermium_codegen::native::blob) and reads the context from `fm_ctx`; everything the compiled code
//! calls (printing, lists, errors, numerics) is fermium_codegen::native::rt, linked in from this library.
//!
//! What the executable prints is exactly what `fermium run` prints with the LLVM back end: the same compiled
//! code, the same run time, the same printer.
use std::io::Write;

use fermium_codegen::native::{blob, rt};
use fermium_syntax::diag::Diagnostic;

extern "C" {
    static fm_blob: u8;
    static fm_blob_len: u64;
    fn fm_main();
}

/// The run-time context the compiled code uses (set before fm_main runs).
#[no_mangle]
pub static mut fm_ctx: *mut u8 = std::ptr::null_mut();

/// Stack of the thread the program runs on (as `fermium run`: runaway recursion stops with an error).
const STACK: usize = 512 << 20;

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    match std::thread::Builder::new().stack_size(STACK).spawn(run) {
        Ok(h) => h.join().unwrap_or(101),
        Err(_) => run(),
    }
}

fn run() -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(fm_blob), fm_blob_len as usize) };
    let b = match blob::read(bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let stdout = std::io::stdout();
    let mut printer = fermium_codegen::printer::StdPrinter::new(&b.module, std::io::BufWriter::new(stdout.lock()));
    let r = {
        let mut ctx = rt::Ctx::new(&b.module, &mut printer);
        ctx.set_tables(b.tables);
        ctx.run(|p| unsafe { fm_ctx = p }, fm_main)
    };
    drop(printer);
    let _ = std::io::stdout().flush();
    match r {
        Ok(()) => 0,
        Err(e) => {
            let d = Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                                 length: 1, hint: e.hint, severity: fermium_syntax::Severity::Error, fix: vec![] };
            eprintln!("{}", d.format(Some(&b.source), Some(&b.file_name)));
            1
        }
    }
}
