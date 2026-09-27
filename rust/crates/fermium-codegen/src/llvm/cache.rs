//! The compile cache (spec C6, DECISIONS D317): `fermium run` keeps the machine code the JIT generated for a
//! program, with what the run time needs besides (native::blob: the tables of the checked module and of the code
//! generator), in `<cache>/jit/` (the folder of C4's C++ wrappers: `$FERMIUM_CACHE_DIR`, else
//! `$XDG_CACHE_HOME/fermium`, else `~/.cache/fermium`). A later run of the same program loads it and skips
//! parsing, checking, code generation, optimization and the JIT's code generation.
//!
//! An entry is found by a key made of everything the machine code depends on: this `fermium` binary (its
//! version, size and modification time), LLVM's version, the computer's CPU and its features, the code
//! generator's switches (environment variables), the program's file name, folder and text. It is used only if
//! it is intact (magic, length and checksum), holds exactly the program's text, and every module file and
//! fermium.toml the compilation read is unchanged (same contents) and every one it looked for and didn't find is
//! still missing (a module file added where an import looked first would change what it finds).
//!
//! Not cached (compiled every time, as before): programs that use Python, C or C++ functions or read data files
//! when checked (their meaning depends on files outside the program), programs with constructs the
//! tree-walker runs (the code holds this process's addresses), and runs with an LLVM dump switch. Writes are
//! atomic (a temporary file renamed into place), so a concurrent or interrupted run never leaves a partial entry;
//! a damaged or unreadable entry is ignored (and replaced). `FERMIUM_NO_CACHE=1` switches the cache off. The
//! folder keeps at most MAX_ENTRIES programs (the least recently written are removed).
use std::hash::{Hash, Hasher};
use std::path::Path;

const MAGIC: &[u8; 8] = b"FMJIT001";
/// The most programs the cache keeps.
pub const MAX_ENTRIES: usize = 400;

/// The code generator's switches that change the machine code (their values are part of the key).
const CODEGEN_ENV: &[&str] = &["FERMIUM_LLVM_ARGS", "FERMIUM_LLVM_PASSES", "FERMIUM_LLVM_NOVEC", "FERMIUM_LLVM_NOSLP",
                               "FERMIUM_LLVM_CG3", "FERMIUM_NO_IFCONV", "FERMIUM_NO_MATH_INTRINSICS", "FERMIUM_C2"];

/// A cache entry read back: the warnings the compilation printed, the run time's tables (native::blob) and the
/// machine code.
pub struct Entry {
    pub warnings: String,
    pub blob: Vec<u8>,
    pub object: Vec<u8>,
}

/// Is the cache off for this run (FERMIUM_NO_CACHE=1, or an LLVM dump switch, which needs the compilation)?
pub fn off() -> bool {
    let on = |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0");
    on("FERMIUM_NO_CACHE") || on("FERMIUM_DUMP_LLVM") || on("FERMIUM_DUMP_LLVM_OPT")
}

fn hash_bytes(parts: &[&[u8]]) -> u64 {
    // SipHash with fixed keys: stable within one binary, which is part of every key
    #[allow(deprecated)]
    let mut h = std::hash::SipHasher::new_with_keys(0x6665_726d_6975_6d32, 0x6a69_7463_6163_6865);
    for p in parts {
        p.len().hash(&mut h);
        h.write(p);
    }
    h.finish()
}

/// This binary: its version, size and modification time (a rebuilt `fermium` never reads an older one's code).
fn binary_id() -> String {
    let exe = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok());
    let (len, mtime) = exe.map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos()))).unwrap_or((0, 0));
    format!("{} {} {len} {mtime}", env!("CARGO_PKG_VERSION"), super::llvm_version())
}

/// The key of a program: a file name in the cache folder.
pub fn key(source: &str, file_name: &str, base_dir: &str) -> String {
    use inkwell::targets::TargetMachine;
    let mut env = String::new();
    for k in CODEGEN_ENV {
        env += &format!("{k}={:?};", std::env::var(k).ok());
    }
    let cpu = TargetMachine::get_host_cpu_name().to_string();
    let features = TargetMachine::get_host_cpu_features().to_string();
    let id = binary_id();
    let a = hash_bytes(&[id.as_bytes(), cpu.as_bytes(), features.as_bytes(), env.as_bytes(), file_name.as_bytes(),
                         base_dir.as_bytes(), source.as_bytes()]);
    let b = hash_bytes(&[source.as_bytes(), base_dir.as_bytes(), file_name.as_bytes(), id.as_bytes(), env.as_bytes(),
                         features.as_bytes(), cpu.as_bytes(), b"second"]);
    format!("{a:016x}{b:016x}")
}

fn file_hash(p: &str) -> String {
    match std::fs::read(p) {
        Ok(b) => format!("{:016x}", hash_bytes(&[&b])),
        Err(_) => "absent".into(),
    }
}

/// The dependency lines of an entry: `F <hash> <path>` per file the compilation read or looked for (`absent`
/// for one it didn't find).
fn dep_lines(files: &[String]) -> String {
    files.iter().map(|f| format!("F {} {f}\n", file_hash(f))).collect()
}

fn deps_current(lines: &str) -> bool {
    lines.lines().all(|l| {
        let mut it = l.splitn(3, ' ');
        match (it.next(), it.next(), it.next()) {
            (Some("F"), Some(h), Some(p)) => file_hash(p) == h,
            _ => false,
        }
    })
}

fn put(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u64).to_le_bytes());
    out.extend_from_slice(b);
}

fn take<'a>(b: &mut &'a [u8]) -> Option<&'a [u8]> {
    if b.len() < 8 {
        return None;
    }
    let n = u64::from_le_bytes(b[..8].try_into().ok()?) as usize;
    let rest = &b[8..];
    if rest.len() < n {
        return None;
    }
    let (x, r) = rest.split_at(n);
    *b = r;
    Some(x)
}

/// The entry for this key, if it is intact, holds this very program and its dependencies are unchanged.
pub fn lookup(dir: &Path, key: &str, source: &str) -> Option<Entry> {
    let bytes = std::fs::read(dir.join(format!("{key}.fmc"))).ok()?;
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return None;
    }
    let sum = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    let body = &bytes[16..];
    if hash_bytes(&[body]) != sum {
        return None;
    }
    let mut b = body;
    let src = take(&mut b)?;
    let deps = std::str::from_utf8(take(&mut b)?).ok()?;
    let warnings = String::from_utf8(take(&mut b)?.to_vec()).ok()?;
    let blob = take(&mut b)?.to_vec();
    let object = take(&mut b)?.to_vec();
    if src != source.as_bytes() || !b.is_empty() || object.is_empty() || !deps_current(deps) {
        return None;
    }
    Some(Entry { warnings, blob, object })
}

/// Save a compiled program (errors are ignored: the cache is only a shortcut).
pub fn store(dir: &Path, key: &str, source: &str, files: &[String], warnings: &str, blob: &[u8], object: &[u8]) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let mut body = vec![];
    put(&mut body, source.as_bytes());
    put(&mut body, dep_lines(files).as_bytes());
    put(&mut body, warnings.as_bytes());
    put(&mut body, blob);
    put(&mut body, object);
    let mut all = MAGIC.to_vec();
    all.extend_from_slice(&hash_bytes(&[&body]).to_le_bytes());
    all.extend_from_slice(&body);
    let tmp = dir.join(format!("{key}.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, &all).is_ok() && std::fs::rename(&tmp, dir.join(format!("{key}.fmc"))).is_ok() {
        prune(dir);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Keep at most MAX_ENTRIES entries: remove the least recently written (and stale temporary files).
fn prune(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries = vec![];
    let now = std::time::SystemTime::now();
    for e in rd.flatten() {
        let p = e.path();
        let Ok(m) = e.metadata() else { continue };
        let t = m.modified().unwrap_or(std::time::UNIX_EPOCH);
        match p.extension().and_then(|x| x.to_str()) {
            Some("fmc") => entries.push((t, p)),
            // a temporary file of a run that was killed while writing
            Some("tmp") if now.duration_since(t).is_ok_and(|d| d.as_secs() > 3600) => {
                let _ = std::fs::remove_file(&p);
            }
            _ => {}
        }
    }
    if entries.len() <= MAX_ENTRIES {
        return;
    }
    entries.sort();
    for (_, p) in &entries[..entries.len() - MAX_ENTRIES] {
        let _ = std::fs::remove_file(p);
    }
}

// ------------------------------------------------------------------ MCJIT's object cache (jit_cache.cpp)
extern "C" {
    fn fermium_jit_cache_new(obj: *const u8, len: usize) -> *mut std::ffi::c_void;
    fn fermium_jit_cache_attach(ee: inkwell::llvm_sys::execution_engine::LLVMExecutionEngineRef,
                                c: *mut std::ffi::c_void);
    fn fermium_jit_cache_saved(c: *mut std::ffi::c_void, data: *mut *const u8) -> usize;
    fn fermium_jit_cache_free(c: *mut std::ffi::c_void);
}

/// An llvm::ObjectCache for one execution engine: gives MCJIT `hit` instead of generating code, or keeps the
/// code it generates. Must outlive the engine it is attached to (drop it after the engine).
pub struct ObjectCache(*mut std::ffi::c_void);

impl ObjectCache {
    pub fn new(hit: &[u8]) -> ObjectCache {
        // SAFETY: the C++ side copies the bytes
        ObjectCache(unsafe { fermium_jit_cache_new(hit.as_ptr(), hit.len()) })
    }

    pub fn attach(&self, ee: &inkwell::execution_engine::ExecutionEngine) {
        // SAFETY: ee is a live MCJIT engine; self outlives it (the caller drops the engine first)
        unsafe { fermium_jit_cache_attach(ee.as_mut_ptr(), self.0) }
    }

    /// The machine code MCJIT generated (empty when it used the cached code).
    pub fn saved(&self) -> Vec<u8> {
        let mut p: *const u8 = std::ptr::null();
        // SAFETY: the returned pointer and length describe a buffer owned by the cache, copied at once
        let n = unsafe { fermium_jit_cache_saved(self.0, &mut p) };
        if n == 0 || p.is_null() {
            return vec![];
        }
        unsafe { std::slice::from_raw_parts(p, n) }.to_vec()
    }
}

impl Drop for ObjectCache {
    fn drop(&mut self) {
        // SAFETY: made by fermium_jit_cache_new, freed once
        unsafe { fermium_jit_cache_free(self.0) }
    }
}
