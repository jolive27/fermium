//! Linking an executable for `fermium build` with lld, which is linked into the fermium binary (spec B5.10):
//! the program's object file + the run time (fermium-aotrt, a static library the fermium binary carries) + the
//! C start-up files. No system linker or C compiler is needed.
//!
//! Linux (glibc): the start-up objects (Scrt1.o, crti.o, crtbeginS.o, crtendS.o, crtn.o) and libc_nonshared.a
//! travel inside the fermium binary; the shared libraries every glibc system has (libc.so.6, libm.so.6,
//! libgcc_s.so.1, ld-linux) are linked from the computer the program is built on, so the executable runs there.
//! macOS: libSystem comes from the SDK of Apple's Command Line Tools (`xcode-select --install`), the only place
//! it exists on disk (see rust/BUILD.md).
use std::ffi::CString;
use std::path::{Path, PathBuf};

extern "C" {
    fn fermium_lld_link(argc: i32, argv: *const *const std::ffi::c_char, msg: *mut u8, cap: usize) -> i32;
}

/// What the fermium binary carries for linking executables.
pub struct RuntimeFiles<'a> {
    /// libfermium_aotrt.a
    pub archive: &'a [u8],
    /// (file name, bytes) of the C start-up files (Linux)
    pub crt: Vec<(&'a str, &'a [u8])>,
}

/// Run lld with these arguments (argv[0] picks the flavor); its messages on failure.
pub fn lld(args: &[String]) -> Result<(), String> {
    let cs: Vec<CString> = args.iter().map(|a| CString::new(a.as_str()).unwrap_or_default()).collect();
    let ptrs: Vec<*const std::ffi::c_char> = cs.iter().map(|c| c.as_ptr()).collect();
    let mut msg = vec![0u8; 1 << 16];
    let code = unsafe { fermium_lld_link(ptrs.len() as i32, ptrs.as_ptr(), msg.as_mut_ptr(), msg.len()) };
    let n = msg.iter().position(|&b| b == 0).unwrap_or(msg.len());
    let text = String::from_utf8_lossy(&msg[..n]).trim().to_string();
    if code == 0 {
        Ok(())
    } else {
        Err(if text.is_empty() { format!("lld stopped with code {code}") } else { text })
    }
}

fn find_lib(name: &str, dirs: &[&str]) -> Option<PathBuf> {
    dirs.iter().map(|d| Path::new(d).join(name)).find(|p| p.exists())
}

/// Link `obj` (the program) with the run time into the executable `out`.
pub fn link_executable(obj: &[u8], rt: &RuntimeFiles, out: &str) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("fermium-build-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("can't make a temporary folder: {e}"))?;
    let r = link_in(&tmp, obj, rt, out);
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

fn link_in(tmp: &Path, obj: &[u8], rt: &RuntimeFiles, out: &str) -> Result<(), String> {
    let put = |name: &str, bytes: &[u8]| -> Result<String, String> {
        let p = tmp.join(name);
        std::fs::write(&p, bytes).map_err(|e| format!("can't write {}: {e}", p.display()))?;
        Ok(p.to_string_lossy().into_owned())
    };
    let prog = put("prog.o", obj)?;
    let archive = put("libfermium_aotrt.a", rt.archive)?;
    let mut crt = std::collections::HashMap::new();
    for (name, bytes) in &rt.crt {
        crt.insert(*name, put(name, bytes)?);
    }
    let s = |x: &str| x.to_string();
    if cfg!(target_os = "macos") {
        let sdk = std::process::Command::new("xcrun").args(["--show-sdk-path"]).output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|p| !p.is_empty())
            .or_else(|| {
                let p = "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk";
                Path::new(p).exists().then(|| p.to_string())
            })
            .ok_or("fermium build on macOS needs Apple's Command Line Tools (for libSystem, the C library every Mac \
                    program links with): run  xcode-select --install  once, then try again")?;
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" };
        let args = vec![s("ld64.lld"), s("-arch"), s(arch), s("-platform_version"), s("macos"), s("11.0"), s("11.0"),
                        s("-syslibroot"), sdk, s("-o"), s(out), prog, archive, s("-lSystem"), s("-lc++"), s("-dead_strip")];
        return lld(&args);
    }
    // Linux / glibc
    let (dirs, dl): (&[&str], &str) = if cfg!(target_arch = "aarch64") {
        (&["/lib/aarch64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/lib64", "/usr/lib64", "/lib", "/usr/lib"],
         "/lib/ld-linux-aarch64.so.1")
    } else {
        (&["/lib/x86_64-linux-gnu", "/usr/lib/x86_64-linux-gnu", "/lib64", "/usr/lib64", "/lib", "/usr/lib"],
         "/lib64/ld-linux-x86-64.so.2")
    };
    let need = |name: &str| -> Result<String, String> {
        find_lib(name, dirs).map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| format!("fermium build can't find {name} (the C library every Linux program uses)"))
    };
    let c = |name: &str| -> Result<String, String> {
        crt.get(name).cloned().ok_or_else(|| format!("this fermium binary was built without {name}, so it can't \
                                                      link executables"))
    };
    let mut args = vec![s("ld.lld"), s("--eh-frame-hdr"), s("-pie"), s("-z"), s("relro"), s("-z"), s("now"),
                        s("--hash-style=gnu"), s("--build-id"), s("--gc-sections"), s("-dynamic-linker"), s(dl),
                        s("-o"), s(out), c("Scrt1.o")?, c("crti.o")?, c("crtbeginS.o")?, prog, archive,
                        s("--push-state"), s("--as-needed"), need("libgcc_s.so.1")?, need("libm.so.6")?];
    // before glibc 2.34 these were separate libraries; later they are in libc.so.6
    for old in ["libpthread.so.0", "libdl.so.2", "librt.so.1", "libutil.so.1"] {
        if let Some(p) = find_lib(old, dirs) {
            args.push(p.to_string_lossy().into_owned());
        }
    }
    args.extend([need("libc.so.6")?, s("--pop-state"), c("libc_nonshared.a")?, c("crtendS.o")?, c("crtn.o")?]);
    lld(&args)
}
