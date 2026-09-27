//! The kernel (a port of fermium/jupyter/kernel.py, which ran on ipykernel): each cell is run by one REPL
//! session, so variables and functions carry over from cell to cell. Printed output is streamed back, plots
//! are shown inline (and still saved as files), errors come back in the usual one-line form, warnings go to
//! stderr, and Tab completes `\name` symbols, the names defined so far, keywords, and a module's members.
use std::sync::mpsc::{channel, Receiver};

use fermium_lsp::json::Json;
use fermium_repl::Session;

use crate::crypto::{base64, hex, hmac_sha256};
use crate::zmtp::{bind, Bound, Event, Kind};

pub const PROTOCOL_VERSION: &str = "5.3";
const DELIM: &[u8] = b"<IDS|MSG>";

/// The connection file Jupyter writes for a kernel it starts.
#[derive(Clone, Debug)]
pub struct Connection {
    pub ip: String,
    pub key: Vec<u8>,
    pub shell_port: u16,
    pub iopub_port: u16,
    pub stdin_port: u16,
    pub control_port: u16,
    pub hb_port: u16,
}

impl Connection {
    pub fn from_json(j: &Json) -> Result<Connection, String> {
        if let Some(t) = j.get("transport").str() {
            if t != "tcp" {
                return Err(format!("the transport '{t}' isn't supported (only tcp)"));
            }
        }
        if let Some(s) = j.get("signature_scheme").str() {
            if s != "hmac-sha256" && !s.is_empty() {
                return Err(format!("the signature scheme '{s}' isn't supported (only hmac-sha256)"));
            }
        }
        let port = |k: &str| j.get(k).int().map(|p| p as u16).ok_or_else(|| format!("the connection file has no {k}"));
        Ok(Connection {
            ip: j.get("ip").str().unwrap_or("127.0.0.1").to_string(),
            key: j.get("key").str().unwrap_or("").as_bytes().to_vec(),
            shell_port: port("shell_port")?,
            iopub_port: port("iopub_port")?,
            stdin_port: port("stdin_port")?,
            control_port: port("control_port")?,
            hb_port: port("hb_port")?,
        })
    }
}

/// A message of the Jupyter protocol.
#[derive(Clone, Debug)]
pub struct Message {
    pub ids: Vec<Vec<u8>>,
    pub header: Json,
    pub parent: Json,
    pub metadata: Json,
    pub content: Json,
}

impl Message {
    pub fn msg_type(&self) -> &str {
        self.header.get("msg_type").str().unwrap_or("")
    }
}

pub fn sign(key: &[u8], parts: &[&[u8]]) -> String {
    if key.is_empty() {
        return String::new();
    }
    hex(&hmac_sha256(key, parts))
}

/// Decode the frames of a message; None if it is malformed or its signature is wrong.
pub fn decode(frames: &[Vec<u8>], key: &[u8]) -> Option<Message> {
    let d = frames.iter().position(|f| f == DELIM)?;
    let rest = &frames[d + 1..];
    if rest.len() < 5 {
        return None;
    }
    let want = sign(key, &[&rest[1], &rest[2], &rest[3], &rest[4]]);
    if !key.is_empty() && want.as_bytes() != rest[0].as_slice() {
        return None;
    }
    let j = |b: &Vec<u8>| Json::parse(&String::from_utf8_lossy(b)).ok();
    Some(Message { ids: frames[..d].to_vec(), header: j(&rest[1])?, parent: j(&rest[2])?, metadata: j(&rest[3])?,
                   content: j(&rest[4])? })
}

/// The frames of a message.
pub fn encode(ids: &[Vec<u8>], header: &Json, parent: &Json, metadata: &Json, content: &Json, key: &[u8])
              -> Vec<Vec<u8>> {
    let (h, p, m, c) = (header.dump().into_bytes(), parent.dump().into_bytes(), metadata.dump().into_bytes(),
                        content.dump().into_bytes());
    let sig = sign(key, &[&h, &p, &m, &c]).into_bytes();
    let mut frames = ids.to_vec();
    frames.extend([DELIM.to_vec(), sig, h, p, m, c]);
    frames
}

/// 32 random hex digits (from the system's random source; the time and a counter if there is none).
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b)).is_ok();
    if !ok {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        b = crate::crypto::sha256(&[&t.to_le_bytes(), &n.to_le_bytes(), &std::process::id().to_le_bytes()])[..16]
            .try_into()
            .unwrap();
    }
    hex(&b)
}

/// The current time as ISO 8601 in UTC, with microseconds (as Jupyter writes dates).
pub fn now_iso() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}Z", rem / 3600, rem % 3600 / 60, rem % 60,
            d.subsec_micros())
}

pub fn language_info() -> Json {
    Json::obj(vec![
        ("name", Json::from("fermium")),
        ("version", Json::from(fermium_repl::VERSION)),
        ("mimetype", Json::from("text/x-fermium")),
        ("file_extension", Json::from(".fm")),
        ("codemirror_mode", Json::from("python")),
        ("pygments_lexer", Json::from("python")),
    ])
}

/// What one cell printed, in order: (stream name, text).
pub type Parts = Vec<(&'static str, String)>;

/// A cell's output for the notebook: "plot saved to …" lines are replaced by the pictures (v1 `_flush`).
/// Returns the streams (neighbours of the same stream joined) and the plot files, in order.
pub fn split_plots(parts: &Parts) -> (Parts, Vec<String>) {
    let mut out: Parts = vec![];
    let mut plots = vec![];
    for (name, text) in parts {
        let mut t = String::new();
        if *name == "stdout" {
            for ln in text.split_inclusive('\n') {
                match ln.strip_prefix("plot saved to ") {
                    Some(p) => plots.push(p.trim_end_matches('\n').to_string()),
                    None => t.push_str(ln),
                }
            }
        } else {
            t = text.clone();
        }
        if t.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some((n, prev)) if n == name => prev.push_str(&t),
            _ => out.push((name, t)),
        }
    }
    (out, plots)
}

/// The notebook's rendering of a plot file: PNG and GIF as base64, SVG as text.
pub fn plot_data(path: &str) -> Option<Json> {
    let name = std::path::Path::new(path).file_name()?.to_string_lossy().into_owned();
    let plain = ("text/plain", Json::from(format!("<plot {name}>")));
    let lower = path.to_lowercase();
    if lower.ends_with(".svg") {
        let svg = std::fs::read_to_string(path).ok()?;
        return Some(Json::obj(vec![("image/svg+xml", Json::from(svg)), plain]));
    }
    if lower.ends_with(".png") {
        let png = std::fs::read(path).ok()?;
        return Some(Json::obj(vec![("image/png", Json::from(base64(&png))), plain]));
    }
    if lower.ends_with(".gif") {
        let gif = std::fs::read(path).ok()?;
        return Some(Json::obj(vec![("image/gif", Json::from(base64(&gif))), plain]));
    }
    None
}

/// Run `f` with the process's stderr (file descriptor 2) captured: the run-time warnings the evaluator writes
/// there go to the notebook's stderr stream.
#[cfg(unix)]
pub fn capture_stderr<R>(f: impl FnOnce() -> R) -> (R, String) {
    use std::io::{Read, Seek, Write};
    use std::os::unix::io::AsRawFd;
    let tmp = std::env::temp_dir().join(format!("fermium-kernel-{}-{}.err", std::process::id(), new_id()));
    let Ok(mut file) = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&tmp)
    else {
        return (f(), String::new());
    };
    let _ = std::fs::remove_file(&tmp);
    let _ = std::io::stderr().flush();
    let saved = unsafe { libc::dup(2) };
    if saved < 0 || unsafe { libc::dup2(file.as_raw_fd(), 2) } < 0 {
        return (f(), String::new());
    }
    let r = f();
    let _ = std::io::stderr().flush();
    unsafe {
        libc::dup2(saved, 2);
        libc::close(saved);
    }
    let mut s = String::new();
    let _ = file.seek(std::io::SeekFrom::Start(0));
    let _ = file.read_to_string(&mut s);
    (r, s)
}

#[cfg(not(unix))]
pub fn capture_stderr<R>(f: impl FnOnce() -> R) -> (R, String) {
    (f(), String::new())
}

pub struct Kernel {
    key: Vec<u8>,
    session_id: String,
    shell: Bound,
    control: Bound,
    iopub: Bound,
    _stdin: Bound,
    _hb: Bound,
    rx: Receiver<Event>,
    pub fm: Session,
    execution_count: i64,
}

impl Kernel {
    /// Bind the five sockets of the connection.
    pub fn bind(c: &Connection) -> std::io::Result<Kernel> {
        let (tx, rx) = channel();
        let shell = bind(&c.ip, c.shell_port, Kind::Router, "shell", Some(tx.clone()))?;
        let control = bind(&c.ip, c.control_port, Kind::Router, "control", Some(tx.clone()))?;
        let stdin = bind(&c.ip, c.stdin_port, Kind::Router, "stdin", Some(tx))?;
        let iopub = bind(&c.ip, c.iopub_port, Kind::Pub, "iopub", None)?;
        let hb = bind(&c.ip, c.hb_port, Kind::Rep, "hb", None)?;
        let base = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or(".".into());
        Ok(Kernel { key: c.key.clone(), session_id: new_id(), shell, control, iopub, _stdin: stdin, _hb: hb, rx,
                    fm: Session::new(&base), execution_count: 0 })
    }

    /// The ports actually bound (a port 0 in the connection picks a free one; the tests use that).
    pub fn ports(&self) -> (u16, u16, u16, u16, u16) {
        (self.shell.port, self.iopub.port, self._stdin.port, self.control.port, self._hb.port)
    }

    fn header(&self, msg_type: &str) -> Json {
        Json::obj(vec![
            ("msg_id", Json::from(new_id())),
            ("session", Json::from(self.session_id.clone())),
            ("username", Json::from("kernel")),
            ("date", Json::from(now_iso())),
            ("msg_type", Json::from(msg_type)),
            ("version", Json::from(PROTOCOL_VERSION)),
        ])
    }

    fn publish(&self, parent: &Json, msg_type: &str, content: Json) {
        let topic = format!("kernel.{}.{msg_type}", self.session_id).into_bytes();
        let frames = encode(&[topic], &self.header(msg_type), parent, &Json::obj::<&str>(vec![]), &content, &self.key);
        self.iopub.broadcast(&frames);
    }

    fn status(&self, parent: &Json, state: &str) {
        self.publish(parent, "status", Json::obj(vec![("execution_state", Json::from(state))]));
    }

    fn reply(&self, channel: &str, conn: u64, req: &Message, msg_type: &str, content: Json) {
        let status = content.get("status").clone();
        let md = if status.is_null() { Json::obj::<&str>(vec![]) } else { Json::obj(vec![("status", status)]) };
        let frames = encode(&req.ids, &self.header(msg_type), &req.header, &md, &content, &self.key);
        match channel {
            "control" => self.control.send(conn, &frames),
            _ => self.shell.send(conn, &frames),
        }
    }

    /// Serve until a shutdown request.
    pub fn run(&mut self) {
        self.status(&Json::obj::<&str>(vec![]), "starting");
        while let Ok(ev) = self.rx.recv() {
            let Some(msg) = decode(&ev.frames, &self.key) else { continue };
            if ev.channel == "stdin" {
                continue;
            }
            self.status(&msg.header, "busy");
            let stop = self.handle(ev.channel, ev.conn, &msg);
            self.status(&msg.header, "idle");
            if stop {
                // give the sockets a moment to send the reply
                std::thread::sleep(std::time::Duration::from_millis(100));
                return;
            }
        }
    }

    fn ok(pairs: Vec<(&str, Json)>) -> Json {
        let mut v = vec![("status", Json::from("ok"))];
        v.extend(pairs);
        Json::obj(v)
    }

    /// Handle one request; true to stop the kernel.
    pub fn handle(&mut self, channel: &str, conn: u64, msg: &Message) -> bool {
        let t = msg.msg_type().to_string();
        let reply_type = t.replace("_request", "_reply");
        match t.as_str() {
            "kernel_info_request" => {
                let c = Self::ok(vec![
                    ("protocol_version", Json::from(PROTOCOL_VERSION)),
                    ("implementation", Json::from("fermium")),
                    ("implementation_version", Json::from(fermium_repl::VERSION)),
                    ("language_info", language_info()),
                    ("banner", Json::from(format!("Fermium {}: physics code that reads like physics on paper",
                                                  fermium_repl::VERSION))),
                    ("help_links", Json::Arr(vec![])),
                ]);
                self.reply(channel, conn, msg, &reply_type, c);
            }
            "execute_request" => {
                let c = self.execute(msg);
                self.reply(channel, conn, msg, &reply_type, c);
            }
            "complete_request" => {
                let code = msg.content.get("code").str().unwrap_or("").to_string();
                let pos = msg.content.get("cursor_pos").int().unwrap_or(code.chars().count() as i64).max(0) as usize;
                let c = self.complete(&code, pos);
                self.reply(channel, conn, msg, &reply_type, c);
            }
            "is_complete_request" => {
                let code = msg.content.get("code").str().unwrap_or("");
                let c = if self.fm.needs_more(code) {
                    Json::obj(vec![("status", Json::from("incomplete")), ("indent", Json::from("    "))])
                } else {
                    Json::obj(vec![("status", Json::from("complete"))])
                };
                self.reply(channel, conn, msg, &reply_type, c);
            }
            "inspect_request" => {
                let c = Self::ok(vec![("found", Json::from(false)), ("data", Json::obj::<&str>(vec![])),
                                      ("metadata", Json::obj::<&str>(vec![]))]);
                self.reply(channel, conn, msg, &reply_type, c);
            }
            "history_request" => self.reply(channel, conn, msg, &reply_type, Self::ok(vec![("history", Json::Arr(vec![]))])),
            "comm_info_request" => {
                self.reply(channel, conn, msg, &reply_type, Self::ok(vec![("comms", Json::obj::<&str>(vec![]))]))
            }
            "interrupt_request" => self.reply(channel, conn, msg, &reply_type, Self::ok(vec![])),
            "shutdown_request" => {
                let restart = msg.content.get("restart").bool().unwrap_or(false);
                self.reply(channel, conn, msg, &reply_type, Self::ok(vec![("restart", Json::from(restart))]));
                return true;
            }
            _ => {}
        }
        false
    }

    fn stream(&self, parent: &Json, name: &str, text: &str) {
        self.publish(parent, "stream", Json::obj(vec![("name", Json::from(name)), ("text", Json::from(text))]));
    }

    fn execute(&mut self, msg: &Message) -> Json {
        let code = msg.content.get("code").str().unwrap_or("").to_string();
        let silent = msg.content.get("silent").bool().unwrap_or(false);
        if !silent {
            self.execution_count += 1;
            self.publish(&msg.header, "execute_input", Json::obj(vec![("code", Json::from(code.clone())),
                                                                      ("execution_count", Json::from(self.execution_count))]));
        }
        let count = Json::from(self.execution_count);
        let ok = || Self::ok(vec![("execution_count", count.clone()), ("payload", Json::Arr(vec![])),
                                  ("user_expressions", Json::obj::<&str>(vec![]))]);
        if code.trim().is_empty() {
            return ok();
        }
        let (mut out, mut warn): (Vec<u8>, Vec<u8>) = (vec![], vec![]);
        let fm = &mut self.fm;
        let run = std::panic::AssertUnwindSafe(|| fm.execute(&code, &mut out, Some(&mut warn)));
        let (r, runtime_err) = capture_stderr(|| std::panic::catch_unwind(run));
        let parts: Parts = vec![("stderr", String::from_utf8_lossy(&warn).into_owned()),
                                ("stdout", String::from_utf8_lossy(&out).into_owned()), ("stderr", runtime_err)];
        let (streams, plots) = split_plots(&parts);
        if !silent {
            for (name, text) in &streams {
                self.stream(&msg.header, name, text);
            }
            for p in &plots {
                if let Some(data) = plot_data(p) {
                    self.publish(&msg.header, "display_data", Json::obj(vec![("data", data),
                                                                             ("metadata", Json::obj::<&str>(vec![]))]));
                }
            }
        }
        let err = match r {
            Ok(Ok(())) => return ok(),
            Ok(Err(e)) => e.format(Some(&code), None),
            Err(p) => {
                let what = p.downcast_ref::<String>().cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                format!("internal error in Fermium: panic: {what}\n  (this is a bug in Fermium, not in your program)")
            }
        };
        if !silent {
            self.stream(&msg.header, "stderr", &format!("{err}\n"));
        }
        Json::obj(vec![("status", Json::from("error")), ("execution_count", count),
                       ("ename", Json::from("FermiumError")), ("evalue", Json::from(err.clone())),
                       ("traceback", Json::Arr(vec![Json::from(err)]))])
    }

    /// Tab completion (kernel.py `do_complete`): `\name` symbols, else names, keywords and module members.
    pub fn complete(&mut self, code: &str, cursor_pos: usize) -> Json {
        let text: Vec<char> = code.chars().take(cursor_pos).collect();
        let reply = |matches: Vec<String>, start: usize| {
            Json::obj(vec![("status", Json::from("ok")),
                           ("matches", Json::Arr(matches.into_iter().map(Json::from).collect())),
                           ("cursor_start", Json::from(start)), ("cursor_end", Json::from(cursor_pos)),
                           ("metadata", Json::obj::<&str>(vec![]))])
        };
        if let Some(i) = text.iter().rposition(|&c| c == '\\') {
            if !text[i..].iter().any(|c| c.is_whitespace()) {
                let name: String = text[i + 1..].iter().collect();
                let matches: Vec<String> = match fermium_syntax::symbols::lookup(&name) {
                    Some(s) => vec![s.to_string()],
                    None => {
                        let mut v: Vec<String> = fermium_syntax::symbols::LATEX
                            .iter()
                            .filter(|(k, _)| k.starts_with(name.as_str()))
                            .map(|(_, v)| v.to_string())
                            .collect();
                        v.sort();
                        v.dedup();
                        v
                    }
                };
                return reply(matches, i);
            }
        }
        let t: String = text.iter().collect();
        let line = t.rsplit('\n').next().unwrap_or("").to_string();
        let n = line.chars().count();
        let mut items = fermium_lsp::analysis::completions_in(Some(&mut self.fm.checker), &line, 0, n);
        let start = items.first().map(|i| i.2).unwrap_or(n);
        let lc: Vec<char> = line.chars().collect();
        if start >= lc.len() && !lc[..start.min(lc.len())].ends_with(&['.']) {
            items.clear(); // nothing typed yet: don't list every name
        }
        reply(items.into_iter().map(|i| i.0).collect(), cursor_pos - (n - start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_messages_round_trip() {
        let h = Json::obj(vec![("msg_type", Json::from("kernel_info_request"))]);
        let e = Json::obj::<&str>(vec![]);
        let frames = encode(&[b"id".to_vec()], &h, &e, &e, &e, b"secret");
        let m = decode(&frames, b"secret").unwrap();
        assert_eq!(m.msg_type(), "kernel_info_request");
        assert_eq!(m.ids, vec![b"id".to_vec()]);
        assert!(decode(&frames, b"other").is_none());
    }

    #[test]
    fn dates_and_ids() {
        let d = now_iso();
        assert_eq!(d.len(), 27);
        assert!(d.starts_with("20") && d.ends_with('Z'));
        assert_eq!(new_id().len(), 32);
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn plot_lines_become_pictures() {
        let parts: Parts = vec![("stderr", "".into()), ("stdout", "1 m\nplot saved to /tmp/x.png\n2 m\n".into()),
                                ("stderr", "warn\n".into())];
        let (s, p) = split_plots(&parts);
        assert_eq!(s, vec![("stdout", "1 m\n2 m\n".to_string()), ("stderr", "warn\n".to_string())]);
        assert_eq!(p, vec!["/tmp/x.png"]);
    }
}
