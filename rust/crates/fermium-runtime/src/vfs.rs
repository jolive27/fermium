//! The files a program reads (`load`) and writes (plots), and its run-time warnings (stderr).
//!
//! Natively these are the file system and stderr. In the browser playground (wasm32, fermium-wasm) there is
//! neither: files live in an in-memory map the page fills with the example data files before a run and reads
//! the plots back from after it, and warnings are collected for the page to show.
//!
//! Both builds remember the paths written during a run ([`take_written`]), so a driver can show the plots.
use std::cell::RefCell;
use std::io;

thread_local! {
    static WRITTEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Some: stderr lines are collected here instead of written (always, in the browser, which has no stderr)
    static STDERR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// From now on, collect the lines written to stderr (see [`take_stderr`]) instead of writing them.
pub fn capture_stderr() {
    STDERR.with(|s| *s.borrow_mut() = Some(String::new()));
}

/// The stderr lines collected since [`capture_stderr`] (or the last call), each ending with a newline.
pub fn take_stderr() -> String {
    STDERR.with(|s| s.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// One line on stderr: a run-time warning.
pub fn stderr_line(text: &str) {
    STDERR.with(|s| match s.borrow_mut().as_mut() {
        Some(buf) => {
            buf.push_str(text);
            buf.push('\n');
        }
        None => imp::stderr_line(text),
    })
}

fn note_written(path: &str) {
    WRITTEN.with(|w| {
        let mut w = w.borrow_mut();
        if !w.iter().any(|p| p == path) {
            w.push(path.to_string());
        }
    });
}

/// The paths written since the last call, in the order first written.
pub fn take_written() -> Vec<String> {
    WRITTEN.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io;

    pub fn read(path: &str) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    pub fn write(path: &str, bytes: &[u8]) -> io::Result<()> {
        if let Some(d) = std::path::Path::new(path).parent() {
            if !d.as_os_str().is_empty() {
                std::fs::create_dir_all(d)?;
            }
        }
        std::fs::write(path, bytes)
    }
    pub fn stderr_line(text: &str) {
        eprintln!("{text}");
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;

    thread_local! {
        static FILES: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    }

    /// `/a/./b/../c` → `/a/c`, so a path spelled differently finds the same file.
    pub fn normalize(path: &str) -> String {
        let mut parts: Vec<&str> = vec![];
        for c in path.split('/') {
            match c {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                c => parts.push(c),
            }
        }
        format!("/{}", parts.join("/"))
    }

    pub fn read(path: &str) -> io::Result<Vec<u8>> {
        FILES.with(|f| f.borrow().get(&normalize(path)).cloned())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No such file or directory"))
    }
    pub fn write(path: &str, bytes: &[u8]) -> io::Result<()> {
        FILES.with(|f| f.borrow_mut().insert(normalize(path), bytes.to_vec()));
        Ok(())
    }
    /// nowhere to write it: the driver captures stderr ([`super::capture_stderr`])
    pub fn stderr_line(_text: &str) {}
}

#[cfg(target_arch = "wasm32")]
pub use imp::normalize;

/// Read a whole file.
pub fn read(path: &str) -> io::Result<Vec<u8>> {
    imp::read(path)
}

/// Read a whole file as text (invalid UTF-8 is an error, as with `std::fs::read_to_string`).
pub fn read_to_string(path: &str) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write a file, creating its folder if needed.
pub fn write(path: &str, bytes: &[u8]) -> io::Result<()> {
    imp::write(path, bytes)?;
    note_written(path);
    Ok(())
}

/// Add a file for programs to read (the playground's example data files). Natively: a plain write.
pub fn put(path: &str, bytes: &[u8]) -> io::Result<()> {
    imp::write(path, bytes)
}
