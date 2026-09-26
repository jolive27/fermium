//! Embeds what `fermium build` needs to link executables (spec B5.10): the run time of executables
//! (fermium-aotrt, a static library built here by a nested cargo, in its own target folder so it is compiled
//! without LLVM) and, on Linux, the C start-up files of this computer's C library (Scrt1.o, crti.o, crtn.o,
//! crtbeginS.o, crtendS.o, libc_nonshared.a). FERMIUM_NO_AOTRT=1 skips all this (a faster build for working on
//! the compiler; `fermium build` then says it is unavailable).
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.parent().unwrap().parent().unwrap().to_path_buf();
    println!("cargo:rerun-if-env-changed=FERMIUM_NO_AOTRT");
    let mut code = String::new();
    let skip = std::env::var_os("FERMIUM_NO_AOTRT").is_some();
    let lib = if skip { None } else { build_aotrt(&workspace, &out) };
    match &lib {
        Some(p) => code += &format!("pub static AOTRT: &[u8] = include_bytes!({:?});\n", p.to_string_lossy()),
        None => code += "pub static AOTRT: &[u8] = &[];\n",
    }
    code += "pub static CRT: &[(&str, &[u8])] = &[\n";
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("linux") && lib.is_some() {
        for name in ["Scrt1.o", "crti.o", "crtn.o", "crtbeginS.o", "crtendS.o", "libc_nonshared.a"] {
            if let Some(p) = find_crt(name) {
                let dst = out.join(name);
                if std::fs::copy(&p, &dst).is_ok() {
                    code += &format!("    ({name:?}, include_bytes!({:?})),\n", dst.to_string_lossy());
                }
            } else {
                println!("cargo:warning=no {name} found: fermium build won't be able to link executables");
            }
        }
    }
    code += "];\n";
    std::fs::write(out.join("aot_files.rs"), code).unwrap();
}

/// `cc -print-file-name=NAME`, if it names an existing file.
fn find_crt(name: &str) -> Option<PathBuf> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let o = Command::new(cc).arg(format!("-print-file-name={name}")).output().ok()?;
    let p = PathBuf::from(String::from_utf8_lossy(&o.stdout).trim());
    (p.is_absolute() && p.exists()).then_some(p)
}

/// Build libfermium_aotrt.a with a nested cargo (profile aotrt), in OUT_DIR/aotrt.
fn build_aotrt(workspace: &Path, out: &Path) -> Option<PathBuf> {
    for dir in ["crates/fermium-aotrt", "crates/fermium-codegen/src", "crates/fermium-ir/src", "crates/fermium-units",
                "crates/fermium-runtime/src", "crates/fermium-syntax/src", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={}", workspace.join(dir).display());
    }
    let tdir = out.join("aotrt");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--offline", "-p", "fermium-aotrt", "--profile", "aotrt", "--target-dir"])
        .arg(&tdir)
        .current_dir(workspace)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => {
            println!("cargo:warning=building the run time of executables (fermium-aotrt) failed: fermium build \
                      will be unavailable");
            return None;
        }
    }
    let lib = tdir.join("aotrt").join(if cfg!(windows) { "fermium_aotrt.lib" } else { "libfermium_aotrt.a" });
    lib.exists().then_some(lib)
}
