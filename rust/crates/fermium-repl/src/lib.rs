//! fermium-repl: the interactive Fermium prompt (a port of `fermium/repl.py`) and the session it shares with the
//! Jupyter kernel (`session.rs`, v1's `driver.ReplSession`).
pub mod editor;
pub mod session;

use std::io::{BufRead, Write};

pub use session::{needs_more, opens_block, shell_command, Session};

use editor::Read;
use fermium_syntax::symbols::expand_all;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn banner() -> String {
    format!("Fermium {VERSION} -- physics that reads like physics.  Type :help for help, :quit to leave.\n\
             Tip: type \\theta then press Tab to get θ (also \\hbar, \\int, \\sqrt, \\^2 ...).")
}

pub const HELP: &str = "Examples:
    L = 1.20 m
    T = 2.21 s
    g = 4π² L / T²          (or ASCII: g = 4 pi^2 L / T^2)
    print g in ft/s²
    x(t) = 0.1 m cos(10 t / 1 s)
    print d/dt x
Commands:  :help   :quit   :vars
Symbols:   type \\name then Tab, e.g. \\omega -> ω, \\^2 -> ², \\int -> ∫";

/// Where the prompt's lines come from: the line editor (a terminal) or a file or pipe.
pub trait Lines {
    fn read(&mut self, prompt: &str) -> Read;
}

impl Lines for editor::Editor {
    fn read(&mut self, prompt: &str) -> Read {
        self.read_line(prompt)
    }
}

/// Lines from a file or pipe (no prompt is shown).
pub struct Piped<R: BufRead>(pub R);

impl<R: BufRead> Lines for Piped<R> {
    fn read(&mut self, _prompt: &str) -> Read {
        let mut buf = vec![];
        match self.0.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => Read::Eof,
            Ok(_) => {
                let s = String::from_utf8_lossy(&buf).into_owned();
                Read::Line(s.strip_suffix('\n').unwrap_or(&s).to_string())
            }
        }
    }
}

/// The prompt loop (repl.py `main`). `interactive`: lines come from a person at a terminal (a blank line ends a
/// block); otherwise from a file or pipe (a block goes on while lines are indented). Returns the exit code.
pub fn run(lines: &mut dyn Lines, interactive: bool, out: &mut dyn Write, base_dir: &str) -> i32 {
    if interactive {
        let _ = writeln!(out, "{}", banner());
    }
    let mut session = Session::new(base_dir);
    let mut pending: Vec<String> = vec![];
    let read = |lines: &mut dyn Lines, pending: &mut Vec<String>, prompt: &str| -> Read {
        match pending.pop() {
            Some(l) => Read::Line(l),
            None => lines.read(prompt),
        }
    };
    loop {
        let _ = out.flush();
        let line = match read(lines, &mut pending, "fm> ") {
            Read::Line(l) => l,
            Read::Eof => {
                if interactive {
                    let _ = writeln!(out);
                }
                return 0;
            }
            Read::Interrupted => {
                let _ = writeln!(out);
                continue;
            }
        };
        let line = expand_all(&line);
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        if matches!(s, ":quit" | ":q" | "quit" | "exit" | ":exit") {
            return 0;
        }
        if s == ":help" {
            let _ = writeln!(out, "{HELP}");
            continue;
        }
        if s == ":vars" {
            let _ = writeln!(out, "{}", session.describe_vars());
            continue;
        }
        if let Some(cmd) = shell_command(s, &session.names()) {
            let _ = writeln!(out, "'{cmd}' looks like a terminal command. This is the Fermium prompt; type :quit to \
                                   go back to the terminal first.");
            continue;
        }
        let mut text = format!("{line}\n");
        let block = opens_block(&line) || session.needs_more(&text);
        while block {
            if interactive {
                match read(lines, &mut pending, "... ") {
                    Read::Line(more) => {
                        if more.trim().is_empty() {
                            break; // an empty line ends the block (like Python)
                        }
                        text += &expand_all(&more);
                        text.push('\n');
                        continue;
                    }
                    Read::Eof => break,
                    Read::Interrupted => {
                        text.clear();
                        let _ = writeln!(out);
                        break;
                    }
                }
            }
            // reading a file/pipe: go on while lines are indented, or are 'else', or the input is unfinished
            let nxt = match read(lines, &mut pending, "... ") {
                Read::Line(l) => l,
                _ => break,
            };
            let st = nxt.trim();
            if nxt.starts_with([' ', '\t']) || st.starts_with("else") || st.starts_with("elif") || session.needs_more(&text)
            {
                if !st.is_empty() {
                    text += &expand_all(&nxt);
                    text.push('\n');
                }
                continue;
            }
            pending.push(nxt);
            break;
        }
        if text.is_empty() {
            continue;
        }
        let r = session.execute(&text, &mut *out, None);
        if let Err(e) = r {
            let _ = writeln!(out, "{}", e.format(Some(&text), None));
        }
        let _ = out.flush();
    }
}

/// Ctrl+C while a program runs: say so and stop with exit code 130 (v1's `driver._CtrlC`, which the REPL and
/// `fermium run` both use; while the REPL edits a line, Ctrl+C arrives as a key instead and drops the line).
pub fn stop_on_ctrl_c() {
    #[cfg(unix)]
    {
        extern "C" fn stop(_: libc::c_int) {
            let msg = b"\nstopped by Ctrl+C\n";
            unsafe {
                libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                libc::_exit(130);
            }
        }
        unsafe {
            libc::signal(libc::SIGINT, stop as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }
}

/// The prompt with the line editor when stdin is a terminal, else reading stdin as a script (repl.py `main`).
pub fn main() -> i32 {
    let base = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or(".".into());
    let interactive = unsafe { is_tty() };
    stop_on_ctrl_c();
    let stdout = std::io::stdout();
    if interactive {
        let mut ed = editor::Editor::new(editor::history_path());
        let code = run(&mut ed, true, &mut stdout.lock(), &base);
        ed.save();
        code
    } else {
        let stdin = std::io::stdin();
        run(&mut Piped(stdin.lock()), false, &mut stdout.lock(), &base)
    }
}

#[cfg(unix)]
unsafe fn is_tty() -> bool {
    libc::isatty(0) == 1
}

#[cfg(not(unix))]
unsafe fn is_tty() -> bool {
    false
}
