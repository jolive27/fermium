//! fermium-jupyter: the Jupyter kernel written in Rust (spec §B5.13), a port of fermium/jupyter/kernel.py.
//! Jupyter's wire protocol (ZeroMQ's ZMTP 3.1 over TCP, HMAC-SHA256 signatures) is implemented here
//! (`zmtp.rs`, `crypto.rs`), so the kernel needs no libzmq, Python or ipykernel. `fermium jupyter install`
//! registers it (kernel.json); Jupyter then starts `fermium jupyter kernel -f CONNECTION_FILE`.
pub mod crypto;
pub mod kernel;
pub mod zmtp;

use std::path::{Path, PathBuf};

use fermium_lsp::json::Json;

/// Jupyter's per-user data folder (jupyter_core.paths.jupyter_data_dir).
pub fn user_data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("JUPYTER_DATA_DIR") {
        return Some(PathBuf::from(d));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library").join("Jupyter"));
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("jupyter"));
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(x) if !x.is_empty() => Some(PathBuf::from(x).join("jupyter")),
        _ => home.map(|h| h.join(".local").join("share").join("jupyter")),
    }
}

/// The Python environment `--sys-prefix` installs into: the active conda or virtual environment, else the
/// prefix of `python3` (Jupyter is a Python program; Fermium itself doesn't need Python).
fn sys_prefix() -> Option<PathBuf> {
    for v in ["CONDA_PREFIX", "VIRTUAL_ENV"] {
        if let Some(p) = std::env::var_os(v).filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(p));
        }
    }
    let o = std::process::Command::new("python3").args(["-c", "import sys; print(sys.prefix)"]).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if o.status.success() && !s.is_empty() {
        Some(PathBuf::from(s))
    } else {
        None
    }
}

/// kernel.json, written like Python's `json.dump(spec, fh, indent=1)` (v1's install).
pub fn kernel_json(exe: &str) -> String {
    let q = |s: &str| Json::from(s).dump();
    format!("{{\n \"argv\": [\n  {},\n  \"jupyter\",\n  \"kernel\",\n  \"-f\",\n  \"{{connection_file}}\"\n ],\n \
             \"display_name\": \"Fermium\",\n \"language\": \"fermium\",\n \"interrupt_mode\": \"message\"\n}}",
            q(exe))
}

/// Register the kernel (kernel.py `install`): into Jupyter's user folder, or `PREFIX/share/jupyter/kernels`.
/// Returns the folder written.
pub fn install(prefix: Option<&Path>) -> Result<PathBuf, String> {
    let base = match prefix {
        Some(p) => p.join("share").join("jupyter"),
        None => user_data_dir().ok_or("can't find your home folder (HOME isn't set)")?,
    };
    let dir = base.join("kernels").join("fermium");
    let exe = std::env::current_exe().map_err(|e| format!("can't find the fermium program: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    std::fs::write(dir.join("kernel.json"), kernel_json(&exe.to_string_lossy()))
        .map_err(|e| format!("can't write {}: {e}", dir.join("kernel.json").display()))?;
    Ok(dir)
}

/// `fermium jupyter install [--sys-prefix] [--prefix DIR]`: v1's messages. Returns the exit code.
pub fn install_command(sys_prefix_flag: bool, prefix: Option<&str>) -> i32 {
    let prefix = match (prefix, sys_prefix_flag) {
        (Some(p), _) => Some(PathBuf::from(p)),
        (None, true) => match sys_prefix() {
            Some(p) => Some(p),
            None => {
                eprintln!("--sys-prefix installs into a Python environment, and none was found (no conda or \
                           virtual environment is active, and python3 isn't there)\n  hint: leave out --sys-prefix \
                           to install for your user");
                return 1;
            }
        },
        (None, false) => None,
    };
    match install(prefix.as_deref()) {
        Ok(dir) => {
            println!("installed the Fermium kernel in {}\nstart Jupyter (jupyter lab) and pick 'Fermium' as the kernel",
                     dir.display());
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `fermium jupyter kernel -f CONNECTION_FILE`: run the kernel (Jupyter starts this). Returns the exit code.
pub fn kernel_command(connection_file: &str) -> i32 {
    let text = match std::fs::read_to_string(connection_file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("can't read the connection file '{connection_file}': {e}");
            return 1;
        }
    };
    let conn = match Json::parse(&text).map_err(|e| e.to_string()).and_then(|j| kernel::Connection::from_json(&j)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("the connection file '{connection_file}' can't be used: {e}");
            return 1;
        }
    };
    // Jupyter interrupts a kernel with SIGINT unless kernel.json says "message"; never die of it
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
    fermium_codegen::eval::STACK_LIMIT.store(400 << 20, std::sync::atomic::Ordering::Relaxed);
    let run = move || match kernel::Kernel::bind(&conn) {
        Ok(mut k) => {
            k.run();
            0
        }
        Err(e) => {
            eprintln!("the kernel can't open its ports: {e}");
            1
        }
    };
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).ok().and_then(|h| h.join().ok()).unwrap_or(3)
}

#[cfg(test)]
mod tests {
    #[test]
    fn kernel_json_is_valid() {
        let j = fermium_lsp::json::Json::parse(&super::kernel_json("/opt/fermium \"x\"")).unwrap();
        assert_eq!(j.get("argv").arr()[0].str(), Some("/opt/fermium \"x\""));
        assert_eq!(j.get("argv").arr()[4].str(), Some("{connection_file}"));
        assert_eq!(j.get("display_name").str(), Some("Fermium"));
    }
}
