//! Fermium's cache folder (compiled C++ wrappers in `cpp/`, compiled programs in `jit/`) and the rule that keeps
//! it private (red team 15 #1, DECISIONS D320): the folders are made owner-only (0700), and a folder owned by
//! another user or writable by group or others is not used (one warning, then Fermium runs without a cache),
//! since whatever is in it gets loaded and run.
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `$FERMIUM_CACHE_DIR`, else `$XDG_CACHE_HOME/fermium`, else `~/.cache/fermium` (macOS
/// `~/Library/Caches/fermium`), else a folder in the system's temporary folder.
pub fn root() -> PathBuf {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(d) = env("FERMIUM_CACHE_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = env("XDG_CACHE_HOME") {
        return PathBuf::from(d).join("fermium");
    }
    if let Some(h) = env("HOME") {
        if cfg!(target_os = "macos") {
            return PathBuf::from(h).join("Library/Caches/fermium");
        }
        return PathBuf::from(h).join(".cache/fermium");
    }
    std::env::temp_dir().join(format!("fermium-cache-{}", euid()))
}

#[cfg(unix)]
fn euid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: geteuid has no preconditions and can't fail
    unsafe { geteuid() }
}

#[cfg(not(unix))]
fn euid() -> u32 {
    0
}

/// Why `p` (which exists) can't be a private cache folder, if it can't.
#[cfg(unix)]
fn not_private(p: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let m = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) => return Some(format!("it can't be read ({e})")),
    };
    if m.file_type().is_symlink() {
        // a link is fine when it is the user's and leads to a private folder
        if m.uid() != euid() {
            return Some("it is a link owned by another user".into());
        }
        return match std::fs::metadata(p) {
            Ok(t) if t.is_dir() => check_meta(&t),
            _ => Some("it isn't a folder".into()),
        };
    }
    if !m.is_dir() {
        return Some("it isn't a folder".into());
    }
    check_meta(&m)
}

#[cfg(unix)]
fn check_meta(m: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    if m.uid() != euid() {
        return Some("it is owned by another user".into());
    }
    if m.mode() & 0o022 != 0 {
        return Some(format!("other users can write to it (permissions {:o})", m.mode() & 0o777));
    }
    None
}

#[cfg(not(unix))]
fn not_private(p: &Path) -> Option<String> {
    if p.is_dir() { None } else { Some("it isn't a folder".into()) }
}

fn make_private(p: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(p)
}

/// The private cache folder `root()/sub`, made if needed; Err(the one-line reason) when it can't be used.
pub fn private_dir(sub: &str) -> Result<PathBuf, (String, String)> {
    let root = root();
    let dir = root.join(sub);
    let fix = format!("make it yours and private (chmod 700 {}), or set FERMIUM_CACHE_DIR to a folder of your own",
                      root.display());
    for p in [&root, &dir] {
        if let Err(e) = make_private(p) {
            return Err((format!("can't make the cache folder {}: {e}", p.display()),
                        "set FERMIUM_CACHE_DIR to a folder you can write".into()));
        }
        if let Some(why) = not_private(p) {
            return Err((format!("Fermium's cache folder {} isn't used: {why}", p.display()), fix));
        }
    }
    Ok(dir)
}

/// Print the warning for a cache folder that can't be used, once per process and folder.
pub fn warn_once(msg: &str, hint: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(vec![]);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.iter().any(|s| s == msg) {
        return;
    }
    seen.push(msg.to_string());
    eprintln!("warning: {msg}; running without a cache\n  hint: {hint}");
}

/// A new private folder of this process's own in the system's temporary folder (0700, made fresh, so no other
/// user can have put anything in it), for when the cache can't be used.
pub fn private_temp_dir(tag: &str) -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir();
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut last = None;
    for i in 0..16u32 {
        let p = base.join(format!("fermium-{tag}-{}-{}-{:x}", euid(), std::process::id(), t.wrapping_add(i as u128)));
        let mut b = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            b.mode(0o700);
        }
        match b.create(&p) {
            Ok(()) => return Ok(p),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no name")))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_folder_others_can_write_is_refused() {
        let d = private_temp_dir("cachetest").unwrap();
        assert_eq!(not_private(&d), None);
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(not_private(&d).unwrap().contains("other users can write"), "{:?}", not_private(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
