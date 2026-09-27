//! Fermium in the browser playground (spec §B5.12): parse → check → run with the tree-walking back end, compiled
//! to `wasm32-unknown-unknown` with no JS glue generator. The page (web/worker.js) talks to it through a tiny C
//! ABI of numbers and byte buffers in the module's memory:
//!
//! | export                                   | what it does                                                      |
//! |------------------------------------------|-------------------------------------------------------------------|
//! | `alloc(len) -> ptr`                      | a buffer of `len` bytes for the page to write into                |
//! | `dealloc(ptr, len)`                      | give it back                                                      |
//! | `put_file(path, path_len, data, len)`    | add a file programs can `load` (the example data files)           |
//! | `run_program(src, len, dir, len) -> ptr` | run a program; `ptr` → u32 length (little-endian) + JSON result   |
//! | `panic_message() -> ptr`                 | after a trap: the Rust panic message, as u32 length + UTF-8       |
//! | `partial_stdout() -> ptr`                | after a trap: what the program printed before it (same format)    |
//!
//! It imports one function, `env.fermium_now_ms() -> f64` (performance.now(), for the `clock()` builtin).
//!
//! The JSON result: `{"stdout": "...", "warnings": ["..."], "error": "..." | null,
//! "plots": [{"name": "fig.png", "mime": "image/png", "base64": "..."}]}`. Warnings and errors are formatted as
//! `fermium run` formats them, without the file name ("line 5: …", as v1's playground showed them).
//!
//! Each run should use a fresh instance of the module (the page instantiates the compiled module per run): the
//! runtime keeps per-run state in thread-locals (warnings shown once, the recursion check's stack base), as a
//! `fermium run` process does.
use std::cell::RefCell;
use std::fmt::Write as _;

/// The result of one run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Output {
    pub stdout: String,
    /// compile-time warnings, then run-time warnings, one entry each (a warning may span lines)
    pub warnings: Vec<String>,
    /// the error that stopped the program, formatted as `fermium run` prints it (without the file name)
    pub error: Option<String>,
    /// files the program wrote (plots): path relative to the program's folder, bytes
    pub files: Vec<(String, Vec<u8>)>,
}

/// The shadow stack of the wasm module is 32 MB (build.rs); the recursion check stops a runaway recursion
/// before it is used up. (The browser's own call stack for wasm frames is usually the tighter limit: the page
/// turns its overflow into the same kind of message.)
#[cfg(target_arch = "wasm32")]
const STACK_LIMIT: usize = 28 << 20;

/// The run-time warnings as `fermium run` writes them on stderr, one entry per warning (a line starting with
/// "warning:" starts a new one), like v1's playground.py `_split_warnings`.
fn split_warnings(text: &str) -> Vec<String> {
    let mut items: Vec<String> = vec![];
    for ln in text.lines() {
        if ln.starts_with("warning:") || items.is_empty() {
            items.push(ln.to_string());
        } else {
            let last = items.last_mut().unwrap();
            last.push('\n');
            last.push_str(ln);
        }
    }
    items.retain(|w| !w.trim().is_empty());
    items
}

/// Run a program whose data files (and plots) are in the folder `base_dir`, as `fermium run` does.
pub fn run(src: &str, base_dir: &str) -> Output {
    #[cfg(target_arch = "wasm32")]
    fermium_codegen::eval::STACK_LIMIT.store(STACK_LIMIT, std::sync::atomic::Ordering::Relaxed);
    fermium_runtime::vfs::capture_stderr();
    let _ = fermium_runtime::vfs::take_written();
    let mut out = Output::default();
    run_here(src, base_dir, &mut out);
    out.warnings.extend(split_warnings(&fermium_runtime::vfs::take_stderr()));
    for path in fermium_runtime::vfs::take_written() {
        if let Ok(bytes) = fermium_runtime::vfs::read(&path) {
            let name = path.strip_prefix(base_dir).map(|p| p.trim_start_matches('/')).unwrap_or(&path).to_string();
            out.files.push((name, bytes));
        }
    }
    out
}

thread_local! {
    /// what the program printed so far: also readable after a trap (`partial_stdout`)
    static STDOUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The program's stdout, appended to [`STDOUT`].
struct SharedOut;

impl std::io::Write for SharedOut {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        STDOUT.with(|o| o.borrow_mut().extend_from_slice(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run_here(src: &str, base_dir: &str, out: &mut Output) {
    use fermium_syntax::diag::Diagnostic;
    let fmt = |d: &Diagnostic| d.format(Some(src), None);
    let (prog, pdiags) = match fermium_syntax::parse(src, &[]) {
        Ok(x) => x,
        Err(e) => {
            out.error = Some(fmt(&e));
            return;
        }
    };
    let opts = fermium_check::CheckOptions { base_dir: base_dir.to_string(), repl: false, no_load: false,
                                             source_name: "program.fm".into() };
    let (module, cdiags) = match fermium_check::check(&prog, opts) {
        Ok(x) => x,
        Err((e, d)) => {
            // the warnings collected before the error often explain it
            out.warnings.extend(pdiags.warnings.iter().chain(d.warnings.iter()).map(fmt));
            out.error = Some(fmt(&e));
            return;
        }
    };
    out.warnings.extend(pdiags.warnings.iter().chain(cdiags.warnings.iter()).map(fmt));
    STDOUT.with(|o| o.borrow_mut().clear());
    let mut printer = fermium_codegen::printer::StdPrinter::new(&module, SharedOut);
    let r = {
        use fermium_codegen::Backend;
        fermium_codegen::InterpBackend.run(&module, &mut printer)
    };
    drop(printer);
    out.stdout = STDOUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned());
    if let Err(e) = r {
        let d = Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                             length: 1, hint: e.hint, severity: fermium_syntax::Severity::Error, fix: vec![] };
        out.error = Some(fmt(&d));
    }
}

// ---------------------------------------------------------------------------------------------- JSON

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for ch in bytes.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

fn mime(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.ends_with(".png") {
        "image/png"
    } else if n.ends_with(".svg") {
        "image/svg+xml"
    } else if n.ends_with(".gif") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

impl Output {
    /// The result as the page reads it (see the crate docs).
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\"stdout\":");
        json_str(&mut s, &self.stdout);
        s.push_str(",\"warnings\":[");
        for (i, w) in self.warnings.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            json_str(&mut s, w);
        }
        s.push_str("],\"error\":");
        match &self.error {
            Some(e) => json_str(&mut s, e),
            None => s.push_str("null"),
        }
        s.push_str(",\"plots\":[");
        for (i, (name, bytes)) in self.files.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"name\":");
            json_str(&mut s, name);
            s.push_str(",\"mime\":");
            json_str(&mut s, mime(name));
            s.push_str(",\"base64\":\"");
            s.push_str(&base64(bytes));
            s.push_str("\"}");
        }
        s.push_str("]}");
        s
    }
}

// ---------------------------------------------------------------------------------------------- the C ABI

thread_local! {
    /// the last result handed to the page (kept alive until the next run)
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// the last panic's message, for the page to show after the trap
    static PANIC: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn with_len(body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 4);
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// # Safety
/// `ptr..ptr+len` must be memory the caller owns and has initialised as UTF-8 (lossy otherwise).
unsafe fn text<'a>(ptr: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    if len == 0 {
        return "".into();
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len))
}

/// A buffer of `len` bytes for the page to write into; free it with `dealloc(ptr, len)`.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` and `len` must come from one `alloc(len)` call.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let msg = info.to_string();
            PANIC.with(|p| *p.borrow_mut() = with_len(msg.as_bytes()));
        }));
    });
}

/// Add a file programs can read (`load "data/pendulum.csv"` from the folder `/bootcamp` reads
/// `/bootcamp/data/pendulum.csv`).
///
/// # Safety
/// Both buffers must be valid (see `alloc`).
#[no_mangle]
pub unsafe extern "C" fn put_file(path: *const u8, path_len: usize, data: *const u8, len: usize) {
    let path = text(path, path_len);
    let bytes = if len == 0 { &[][..] } else { std::slice::from_raw_parts(data, len) };
    let _ = fermium_runtime::vfs::put(&path, bytes);
}

/// Run the program `src` from the folder `dir`; the result: a pointer to a u32 length (little-endian) and the
/// JSON text, valid until the next call.
///
/// # Safety
/// Both buffers must be valid (see `alloc`).
#[no_mangle]
pub unsafe extern "C" fn run_program(src: *const u8, src_len: usize, dir: *const u8, dir_len: usize) -> *const u8 {
    install_panic_hook();
    let src = text(src, src_len).into_owned();
    let dir = text(dir, dir_len).into_owned();
    let json = run(&src, if dir.is_empty() { "/" } else { &dir }).to_json();
    RESULT.with(|r| {
        *r.borrow_mut() = with_len(json.as_bytes());
        r.borrow().as_ptr()
    })
}

/// The message of the panic that trapped the last call (empty if none), as u32 length + UTF-8.
#[no_mangle]
pub extern "C" fn panic_message() -> *const u8 {
    PANIC.with(|p| {
        if p.borrow().is_empty() {
            *p.borrow_mut() = with_len(b"");
        }
        p.borrow().as_ptr()
    })
}

/// After a trap: what the program printed before it, as u32 length + UTF-8.
#[no_mangle]
pub extern "C" fn partial_stdout() -> *const u8 {
    let v = STDOUT.with(|o| with_len(&o.borrow()));
    RESULT.with(|r| {
        *r.borrow_mut() = v;
        r.borrow().as_ptr()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escapes_and_base64() {
        let mut s = String::new();
        json_str(&mut s, "a\"b\\c\nd\u{1}é²");
        assert_eq!(s, "\"a\\\"b\\\\c\\nd\\u0001é²\"");
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn warnings_split_like_v1() {
        assert_eq!(split_warnings("warning: a\n  hint: b\nwarning: c\n"), vec!["warning: a\n  hint: b", "warning: c"]);
        assert!(split_warnings("").is_empty());
    }
}
