//! A small line editor for the REPL: arrow keys, Home/End, history (Up/Down, saved in ~/.fermium_history as
//! v1's readline did) and `\name` + Tab completion to symbols. Written over the terminal directly (termios
//! through the libc crate on Unix) so the binary needs no readline or other C library; see NOTES.md in
//! fermium-cli for the choice. On other systems, and when stdin is not a terminal, lines are read plainly.
use std::io::{self, BufRead, Write};

pub enum Read {
    Line(String),
    /// Ctrl-D on an empty line, or the end of the input
    Eof,
    /// Ctrl-C
    Interrupted,
}

pub struct Editor {
    pub history: Vec<String>,
    path: Option<std::path::PathBuf>,
    /// keep at most this many history entries in the file
    pub max_history: usize,
}

/// The history file: ~/.fermium_history (repl.py `_setup_readline`).
pub fn history_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::Path::new(&home).join(".fermium_history"))
}

impl Editor {
    /// An editor with the history read from `path` (GNU readline's plain format; a libedit file, as macOS's
    /// Python writes it, is read too).
    pub fn new(path: Option<std::path::PathBuf>) -> Editor {
        let mut history = vec![];
        if let Some(text) = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            let libedit = text.starts_with("_HiStOrY_V2_");
            for line in text.lines() {
                if line == "_HiStOrY_V2_" || line.is_empty() {
                    continue;
                }
                history.push(if libedit { unvis(line) } else { line.to_string() });
            }
        }
        Editor { history, path, max_history: 1000 }
    }

    pub fn save(&self) {
        let Some(p) = &self.path else { return };
        let start = self.history.len().saturating_sub(self.max_history);
        let mut s = String::new();
        for h in &self.history[start..] {
            s.push_str(h);
            s.push('\n');
        }
        let _ = std::fs::write(p, s);
    }

    pub fn add(&mut self, line: &str) {
        if !line.trim().is_empty() && self.history.last().map(String::as_str) != Some(line) {
            self.history.push(line.to_string());
        }
    }

    /// Read one line with editing when stdin is a terminal.
    pub fn read_line(&mut self, prompt: &str) -> Read {
        #[cfg(unix)]
        {
            if let Some(raw) = RawMode::enter() {
                let r = self.edit(prompt);
                drop(raw);
                if let Read::Line(l) = &r {
                    let l = l.clone();
                    self.add(&l);
                }
                return r;
            }
        }
        print!("{prompt}");
        let _ = io::stdout().flush();
        let mut s = String::new();
        match io::stdin().lock().read_line(&mut s) {
            Ok(0) | Err(_) => Read::Eof,
            Ok(_) => {
                let l = s.trim_end_matches('\n').trim_end_matches('\r').to_string();
                self.add(&l);
                Read::Line(l)
            }
        }
    }

    #[cfg(unix)]
    fn edit(&mut self, prompt: &str) -> Read {
        let mut st = LineState { buf: vec![], pos: 0 };
        let mut hist_i = self.history.len();
        let mut saved: Vec<char> = vec![];
        let mut out = io::stdout();
        let mut last_tab = false;
        redraw(&mut out, prompt, &st);
        loop {
            let Some(key) = read_key() else { return Read::Eof };
            let was_tab = last_tab;
            last_tab = false;
            match key {
                Key::Char(c) => {
                    st.buf.insert(st.pos, c);
                    st.pos += 1;
                }
                Key::Enter => {
                    let _ = out.write_all(b"\r\n");
                    let _ = out.flush();
                    return Read::Line(st.buf.iter().collect());
                }
                Key::CtrlC => {
                    let _ = out.write_all(b"^C");
                    let _ = out.flush();
                    return Read::Interrupted;
                }
                Key::CtrlD => {
                    if st.buf.is_empty() {
                        return Read::Eof;
                    }
                    if st.pos < st.buf.len() {
                        st.buf.remove(st.pos);
                    }
                }
                Key::Backspace => {
                    if st.pos > 0 {
                        st.pos -= 1;
                        st.buf.remove(st.pos);
                    }
                }
                Key::Delete => {
                    if st.pos < st.buf.len() {
                        st.buf.remove(st.pos);
                    }
                }
                Key::Left => st.pos = st.pos.saturating_sub(1),
                Key::Right => st.pos = (st.pos + 1).min(st.buf.len()),
                Key::Home => st.pos = 0,
                Key::End => st.pos = st.buf.len(),
                Key::KillEnd => st.buf.truncate(st.pos),
                Key::KillStart => {
                    st.buf.drain(..st.pos);
                    st.pos = 0;
                }
                Key::KillWord => {
                    let mut i = st.pos;
                    while i > 0 && st.buf[i - 1].is_whitespace() {
                        i -= 1;
                    }
                    while i > 0 && !st.buf[i - 1].is_whitespace() {
                        i -= 1;
                    }
                    st.buf.drain(i..st.pos);
                    st.pos = i;
                }
                Key::Up | Key::Down => {
                    if hist_i == self.history.len() {
                        saved = st.buf.clone();
                    }
                    let up = matches!(key, Key::Up);
                    if up && hist_i > 0 {
                        hist_i -= 1;
                    } else if !up && hist_i < self.history.len() {
                        hist_i += 1;
                    } else {
                        continue;
                    }
                    st.buf = if hist_i == self.history.len() {
                        saved.clone()
                    } else {
                        self.history[hist_i].chars().collect()
                    };
                    st.pos = st.buf.len();
                }
                Key::Tab => {
                    last_tab = true;
                    match tab(&mut st) {
                        Tab::Done => {}
                        Tab::Indent => {
                            for _ in 0..4 {
                                st.buf.insert(st.pos, ' ');
                            }
                            st.pos += 4;
                        }
                        Tab::Choices(list) => {
                            if was_tab || list.len() <= 12 {
                                let _ = write!(out, "\r\n{}\r\n", list.join("   "));
                            }
                        }
                        Tab::None => {
                            let _ = out.write_all(b"\x07");
                        }
                    }
                }
                Key::ClearScreen => {
                    let _ = out.write_all(b"\x1b[H\x1b[2J");
                }
                Key::Ignore => continue,
            }
            redraw(&mut out, prompt, &st);
        }
    }
}

/// libedit's history file escapes spaces and other characters in vis(3) octal form (`\040`).
fn unvis(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else if b[i] == b'\\' && i + 1 < b.len() && b[i + 1] == b'\\' {
            out.push(b'\\');
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct LineState {
    pub buf: Vec<char>,
    pub pos: usize,
}

pub enum Tab {
    /// completed in place
    Done,
    /// nothing before the cursor on this line: indent
    Indent,
    /// several symbols fit: these (`\name sym`) are shown
    Choices(Vec<String>),
    None,
}

/// Tab at the cursor: complete the `\name` before it (symbols.py `complete`), or indent a blank line.
pub fn tab(st: &mut LineState) -> Tab {
    let before: String = st.buf[..st.pos].iter().collect();
    if before.trim().is_empty() {
        return Tab::Indent;
    }
    let Some(bs) = before.rfind('\\') else { return Tab::None };
    let frag = &before[bs..];
    if frag[1..].chars().any(|c| !(c.is_ascii_alphanumeric() || c == '^' || c == '_' || c == '-')) {
        return Tab::None;
    }
    let name = &frag[1..];
    let fl = frag.chars().count();
    let cands = fermium_syntax::symbols::complete(frag);
    if cands.len() == 1 {
        let rep: Vec<char> = cands[0].chars().collect();
        st.buf.splice(st.pos - fl..st.pos, rep.iter().cloned());
        st.pos = st.pos - fl + rep.len();
        return Tab::Done;
    }
    if cands.is_empty() {
        return Tab::None;
    }
    let mut names: Vec<(&str, &str)> =
        fermium_syntax::symbols::sorted_names().into_iter().filter(|(k, _)| k.starts_with(name)).collect();
    // extend the name as far as every candidate agrees (\vareps → \varepsilon)
    let mut common = names[0].0.to_string();
    for (k, _) in &names[1..] {
        while !k.starts_with(common.as_str()) {
            common.pop();
        }
    }
    if common.len() > name.len() {
        let extra: Vec<char> = common[name.len()..].chars().collect();
        for (i, c) in extra.iter().enumerate() {
            st.buf.insert(st.pos + i, *c);
        }
        st.pos += extra.len();
        return Tab::Done;
    }
    names.dedup();
    Tab::Choices(names.iter().map(|(k, v)| format!("\\{k} {v}")).collect())
}

#[cfg(unix)]
fn redraw(out: &mut io::Stdout, prompt: &str, st: &LineState) {
    let text: String = st.buf.iter().collect();
    let back = st.buf.len() - st.pos;
    let mut s = format!("\r{prompt}{text}\x1b[K");
    if back > 0 {
        s += &format!("\x1b[{back}D");
    }
    let _ = out.write_all(s.as_bytes());
    let _ = out.flush();
}

#[cfg(unix)]
enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    CtrlC,
    CtrlD,
    KillEnd,
    KillStart,
    KillWord,
    ClearScreen,
    Ignore,
}

#[cfg(unix)]
fn read_byte() -> Option<u8> {
    let mut b = [0u8; 1];
    loop {
        let n = unsafe { libc::read(0, b.as_mut_ptr() as *mut libc::c_void, 1) };
        if n == 1 {
            return Some(b[0]);
        }
        if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return None;
    }
}

#[cfg(unix)]
fn read_key() -> Option<Key> {
    let b = read_byte()?;
    Some(match b {
        b'\r' | b'\n' => Key::Enter,
        b'\t' => Key::Tab,
        127 | 8 => Key::Backspace,
        1 => Key::Home,
        5 => Key::End,
        2 => Key::Left,
        6 => Key::Right,
        3 => Key::CtrlC,
        4 => Key::CtrlD,
        11 => Key::KillEnd,
        21 => Key::KillStart,
        23 => Key::KillWord,
        12 => Key::ClearScreen,
        14 => Key::Down,
        16 => Key::Up,
        27 => {
            let Some(b1) = read_byte() else { return Some(Key::Ignore) };
            if b1 != b'[' && b1 != b'O' {
                return Some(Key::Ignore);
            }
            let mut seq = vec![];
            loop {
                let c = read_byte()?;
                seq.push(c);
                if c.is_ascii_alphabetic() || c == b'~' || seq.len() > 8 {
                    break;
                }
            }
            match seq.as_slice() {
                b"A" => Key::Up,
                b"B" => Key::Down,
                b"C" => Key::Right,
                b"D" => Key::Left,
                b"H" | b"1~" | b"7~" => Key::Home,
                b"F" | b"4~" | b"8~" => Key::End,
                b"3~" => Key::Delete,
                _ => Key::Ignore,
            }
        }
        c if c < 32 => Key::Ignore,
        c if c < 128 => Key::Char(c as char),
        c => {
            // a UTF-8 sequence
            let n = if c >= 0xF0 { 3 } else if c >= 0xE0 { 2 } else { 1 };
            let mut bytes = vec![c];
            for _ in 0..n {
                bytes.push(read_byte()?);
            }
            match std::str::from_utf8(&bytes).ok().and_then(|s| s.chars().next()) {
                Some(ch) => Key::Char(ch),
                None => Key::Ignore,
            }
        }
    })
}

/// The terminal in raw mode while a line is edited (restored when dropped).
#[cfg(unix)]
struct RawMode(libc::termios);

#[cfg(unix)]
impl RawMode {
    fn enter() -> Option<RawMode> {
        unsafe {
            if libc::isatty(0) != 1 {
                return None;
            }
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut t) != 0 {
                return None;
            }
            let orig = t;
            t.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
            t.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
            t.c_cflag |= libc::CS8;
            t.c_cc[libc::VMIN] = 1;
            t.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(0, libc::TCSAFLUSH, &t) != 0 {
                return None;
            }
            Some(RawMode(orig))
        }
    }
}

#[cfg(unix)]
impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(0, libc::TCSAFLUSH, &self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(s: &str) -> LineState {
        let buf: Vec<char> = s.chars().collect();
        let pos = buf.len();
        LineState { buf, pos }
    }

    #[test]
    fn tab_completes_a_symbol() {
        let mut s = st("x = \\omega");
        assert!(matches!(tab(&mut s), Tab::Done));
        assert_eq!(s.buf.iter().collect::<String>(), "x = ω");
        let mut s = st("E = \\hb");
        assert!(matches!(tab(&mut s), Tab::Done));
        assert_eq!(s.buf.iter().collect::<String>(), "E = ħ");
        let mut s = st("a\\^2");
        tab(&mut s);
        assert_eq!(s.buf.iter().collect::<String>(), "a²");
    }

    #[test]
    fn tab_extends_or_lists() {
        let mut s = st("\\vare");
        assert!(matches!(tab(&mut s), Tab::Done));
        assert_eq!(s.buf.iter().collect::<String>(), "ε"); // \varepsilon is the only match: completed
        let mut s = st("\\va");
        assert!(matches!(tab(&mut s), Tab::Done));
        assert_eq!(s.buf.iter().collect::<String>(), "\\var");
        match tab(&mut s) {
            Tab::Choices(c) => assert_eq!(c, vec!["\\varepsilon ε", "\\varphi φ", "\\vartheta θ"]),
            _ => panic!(),
        }
        let mut s = st("    ");
        assert!(matches!(tab(&mut s), Tab::Indent));
        let mut s = st("print \\nothing");
        assert!(matches!(tab(&mut s), Tab::None));
    }

    #[test]
    fn history_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("fm_hist_{}", std::process::id()));
        std::fs::write(&dir, "_HiStOrY_V2_\nprint\\0402\\040m\n").unwrap();
        let mut e = Editor::new(Some(dir.clone()));
        assert_eq!(e.history, vec!["print 2 m"]);
        e.add("x = 1");
        e.add("x = 1");
        e.add("   ");
        e.save();
        assert_eq!(std::fs::read_to_string(&dir).unwrap(), "print 2 m\nx = 1\n");
        let _ = std::fs::remove_file(dir);
    }
}
