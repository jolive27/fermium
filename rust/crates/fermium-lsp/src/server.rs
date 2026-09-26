//! The Language Server Protocol over stdin/stdout (lsp.py `serve`, which used pygls): JSON-RPC messages with a
//! Content-Length header. Documents are kept as the editor sends them (full or incremental changes); each change
//! is analyzed again and its problems published.
use std::collections::HashMap;
use std::io::{BufRead, Write};

use crate::analysis::{analyze, completions, hover_text, quick_fixes, Analysis, Problem};
use crate::json::Json;

/// The UTF-16 column of the 0-based code-point column `ch` in `line` (the protocol counts UTF-16 units).
pub fn to_utf16(line: &str, ch: usize) -> usize {
    ch + line.chars().take(ch).filter(|c| (*c as u32) > 0xFFFF).count()
}

/// The 0-based code-point column of the UTF-16 column `unit` in `line` (lsp.py `from_utf16`).
pub fn from_utf16(line: &str, unit: usize) -> usize {
    let mut n = 0;
    let mut count = 0;
    for (i, c) in line.chars().enumerate() {
        if n >= unit {
            return i;
        }
        n += if (c as u32) > 0xFFFF { 2 } else { 1 };
        count = i + 1;
    }
    count + unit.saturating_sub(n)
}

fn line_of(source: &str, line: usize) -> &str {
    source.split('\n').nth(line).unwrap_or("")
}

fn pos(line: usize, ch: usize) -> Json {
    Json::obj(vec![("line", Json::from(line)), ("character", Json::from(ch))])
}

fn range(l0: usize, c0: usize, l1: usize, c1: usize) -> Json {
    Json::obj(vec![("start", pos(l0, c0)), ("end", pos(l1, c1))])
}

/// The folder of a file:// document (imports and data files are found there).
fn doc_dir(uri: &str) -> String {
    let path = uri.strip_prefix("file://").map(percent_decode).unwrap_or_default();
    match std::path::Path::new(&path).parent().map(|p| p.to_string_lossy().into_owned()) {
        Some(p) if !p.is_empty() => p,
        _ => ".".into(),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Analyze, never letting a bug in the compiler take the editor down.
fn safe_analyze(source: &str, dir: &str) -> Analysis {
    let (src, d) = (source.to_string(), dir.to_string());
    match std::panic::catch_unwind(move || analyze(&src, &d)) {
        Ok(an) => an,
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            Analysis {
                problems: vec![Problem { line: 1, col: 1, length: 1, error: true, hint: None, fix: vec![],
                                         message: format!("internal error in Fermium: panic: {msg}") }],
                checker: None,
            }
        }
    }
}

pub struct Server<W: Write> {
    out: W,
    docs: HashMap<String, String>,
    cache: HashMap<String, Analysis>,
    shutdown: bool,
}

impl<W: Write> Server<W> {
    pub fn new(out: W) -> Self {
        Server { out, docs: HashMap::new(), cache: HashMap::new(), shutdown: false }
    }

    /// What has been written so far.
    pub fn output(&self) -> &W {
        &self.out
    }

    fn send(&mut self, msg: Json) {
        let body = msg.dump();
        let _ = write!(self.out, "Content-Length: {}\r\n\r\n{body}", body.len());
        let _ = self.out.flush();
    }

    fn reply(&mut self, id: &Json, result: Json) {
        self.send(Json::obj(vec![("jsonrpc", Json::from("2.0")), ("id", id.clone()), ("result", result)]));
    }

    fn analysis(&mut self, uri: &str) -> &mut Analysis {
        if !self.cache.contains_key(uri) {
            let src = self.docs.get(uri).cloned().unwrap_or_default();
            let an = safe_analyze(&src, &doc_dir(uri));
            self.cache.insert(uri.to_string(), an);
        }
        self.cache.get_mut(uri).unwrap()
    }

    fn refresh(&mut self, uri: &str) {
        let src = self.docs.get(uri).cloned().unwrap_or_default();
        let an = safe_analyze(&src, &doc_dir(uri));
        let mut diags = vec![];
        for p in &an.problems {
            let (ln, c) = ((p.line as usize).saturating_sub(1), (p.col as usize).saturating_sub(1));
            let text = line_of(&src, ln);
            let msg = match &p.hint {
                Some(h) => format!("{}\nhint: {h}", p.message),
                None => p.message.clone(),
            };
            diags.push(Json::obj(vec![
                ("range", range(ln, to_utf16(text, c), ln, to_utf16(text, c + (p.length as usize).max(1)))),
                ("message", Json::from(msg)),
                ("severity", Json::from(if p.error { 1usize } else { 2 })),
                ("source", Json::from("fermium")),
            ]));
        }
        self.cache.insert(uri.to_string(), an);
        self.send(Json::obj(vec![
            ("jsonrpc", Json::from("2.0")),
            ("method", Json::from("textDocument/publishDiagnostics")),
            ("params", Json::obj(vec![("uri", Json::from(uri)), ("diagnostics", Json::Arr(diags))])),
        ]));
    }

    /// Apply one content change (full text, or a range in UTF-16 positions).
    fn apply_change(&mut self, uri: &str, change: &Json) {
        let text = change.get("text").str().unwrap_or("").to_string();
        let r = change.get("range");
        if r.is_null() {
            self.docs.insert(uri.to_string(), text);
            return;
        }
        let doc = self.docs.entry(uri.to_string()).or_default();
        let offset = |doc: &str, p: &Json| -> usize {
            let (line, unit) = (p.get("line").int().unwrap_or(0).max(0) as usize,
                                p.get("character").int().unwrap_or(0).max(0) as usize);
            let mut start = 0;
            for (i, l) in doc.split('\n').enumerate() {
                if i == line {
                    let cp = from_utf16(l, unit).min(l.chars().count());
                    return start + l.char_indices().nth(cp).map(|(b, _)| b).unwrap_or(l.len());
                }
                start += l.len() + 1;
            }
            doc.len()
        };
        let (a, b) = (offset(doc, r.get("start")), offset(doc, r.get("end")));
        let (a, b) = (a.min(b), a.max(b));
        doc.replace_range(a..b, &text);
    }

    /// Handle one message. Returns false after `exit`.
    pub fn handle(&mut self, msg: &Json) -> bool {
        let method = msg.get("method").str().unwrap_or("").to_string();
        let id = msg.get("id").clone();
        let params = msg.get("params");
        let uri = params.at(&["textDocument", "uri"]).str().unwrap_or("").to_string();
        match method.as_str() {
            "initialize" => {
                let caps = Json::obj(vec![
                    ("textDocumentSync", Json::obj(vec![("openClose", Json::from(true)), ("change", Json::from(2usize)),
                                                        ("save", Json::obj(vec![("includeText", Json::from(false))]))])),
                    ("hoverProvider", Json::from(true)),
                    ("completionProvider", Json::obj(vec![("triggerCharacters", Json::Arr(vec![Json::from("\\")]))])),
                    ("codeActionProvider", Json::obj(vec![("codeActionKinds", Json::Arr(vec![Json::from("quickfix")]))])),
                ]);
                let info = Json::obj(vec![("name", Json::from("fermium")),
                                          ("version", Json::from(env!("CARGO_PKG_VERSION")))]);
                self.reply(&id, Json::obj(vec![("capabilities", caps), ("serverInfo", info)]));
            }
            "initialized" | "$/setTrace" | "$/cancelRequest" | "workspace/didChangeConfiguration" => {}
            "textDocument/didOpen" => {
                let text = params.at(&["textDocument", "text"]).str().unwrap_or("").to_string();
                self.docs.insert(uri.clone(), text);
                self.refresh(&uri);
            }
            "textDocument/didChange" => {
                for ch in params.get("contentChanges").arr() {
                    self.apply_change(&uri, ch);
                }
                self.refresh(&uri);
            }
            "textDocument/didSave" => {
                if let Some(t) = params.get("text").str() {
                    self.docs.insert(uri.clone(), t.to_string());
                }
                self.refresh(&uri);
            }
            "textDocument/didClose" => {
                self.docs.remove(&uri);
                self.cache.remove(&uri);
            }
            "textDocument/hover" => {
                let src = self.docs.get(&uri).cloned().unwrap_or_default();
                let ln = params.at(&["position", "line"]).int().unwrap_or(0).max(0) as usize;
                let unit = params.at(&["position", "character"]).int().unwrap_or(0).max(0) as usize;
                let ch = from_utf16(line_of(&src, ln), unit);
                let an = self.analysis(&uri);
                let text = hover_text(an, &src, ln, ch);
                let result = match text {
                    Some(t) => Json::obj(vec![("contents", Json::obj(vec![("kind", Json::from("markdown")),
                                                                          ("value", Json::from(t))]))]),
                    None => Json::Null,
                };
                self.reply(&id, result);
            }
            "textDocument/completion" => {
                let src = self.docs.get(&uri).cloned().unwrap_or_default();
                let ln = params.at(&["position", "line"]).int().unwrap_or(0).max(0) as usize;
                let unit = params.at(&["position", "character"]).int().unwrap_or(0).max(0) as usize;
                let text = line_of(&src, ln).to_string();
                let ch = from_utf16(&text, unit);
                let an = self.analysis(&uri);
                let items: Vec<Json> = completions(an, &src, ln, ch)
                    .into_iter()
                    .map(|(label, insert, start, detail)| {
                        Json::obj(vec![
                            ("label", Json::from(label.clone())),
                            ("detail", Json::from(detail)),
                            ("filterText", Json::from(label)),
                            ("textEdit", Json::obj(vec![("range", range(ln, to_utf16(&text, start), ln, unit)),
                                                        ("newText", Json::from(insert))])),
                        ])
                    })
                    .collect();
                self.reply(&id, Json::obj(vec![("isIncomplete", Json::from(false)), ("items", Json::Arr(items))]));
            }
            "textDocument/codeAction" => {
                let src = self.docs.get(&uri).cloned().unwrap_or_default();
                let (r0, r1) = (params.at(&["range", "start", "line"]).int().unwrap_or(0),
                                params.at(&["range", "end", "line"]).int().unwrap_or(0));
                let an = self.analysis(&uri);
                let mut actions = vec![];
                for (title, p, edits) in quick_fixes(an, &src) {
                    let ln = p.line as i64 - 1;
                    if !(r0 <= ln && ln <= r1) {
                        continue;
                    }
                    let tes: Vec<Json> = edits
                        .iter()
                        .map(|(l0, c0, l1, c1, t)| {
                            let (l0, c0, l1, c1) = (*l0 as usize, *c0 as usize, *l1 as usize, *c1 as usize);
                            Json::obj(vec![("range", range(l0, to_utf16(line_of(&src, l0), c0), l1,
                                                           to_utf16(line_of(&src, l1), c1))),
                                           ("newText", Json::from(t.clone()))])
                        })
                        .collect();
                    actions.push(Json::obj(vec![
                        ("title", Json::from(title)),
                        ("kind", Json::from("quickfix")),
                        ("isPreferred", Json::from(true)),
                        ("edit", Json::obj(vec![("changes", Json::obj(vec![(uri.clone(), Json::Arr(tes))]))])),
                    ]));
                }
                self.reply(&id, Json::Arr(actions));
            }
            "shutdown" => {
                self.shutdown = true;
                self.reply(&id, Json::Null);
            }
            "exit" => return false,
            _ => {
                if !id.is_null() {
                    let err = Json::obj(vec![("code", Json::from(-32601i64)),
                                             ("message", Json::from(format!("Method Not Found: {method}")))]);
                    self.send(Json::obj(vec![("jsonrpc", Json::from("2.0")), ("id", id), ("error", err)]));
                }
            }
        }
        true
    }
}

/// Read one message (None at the end of the input).
pub fn read_message(r: &mut dyn BufRead) -> Option<Result<Json, String>> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse::<usize>().ok();
            }
        }
    }
    let mut body = vec![0u8; len?];
    r.read_exact(&mut body).ok()?;
    Some(Json::parse(&String::from_utf8_lossy(&body)))
}

/// `fermium lsp`: serve on stdin/stdout until `exit` (exit code 0 after `shutdown`, 1 without, as the protocol
/// says) or the end of the input.
pub fn serve() -> i32 {
    let run = || {
        let stdin = std::io::stdin();
        let mut r = stdin.lock();
        let mut s = Server::new(std::io::stdout());
        while let Some(m) = read_message(&mut r) {
            match m {
                Ok(msg) => {
                    if !s.handle(&msg) {
                        return if s.shutdown { 0 } else { 1 };
                    }
                }
                Err(e) => {
                    let err = Json::obj(vec![("code", Json::from(-32700i64)),
                                             ("message", Json::from(format!("Parse error: {e}")))]);
                    s.send(Json::obj(vec![("jsonrpc", Json::from("2.0")), ("id", Json::Null), ("error", err)]));
                }
            }
        }
        0
    };
    // the checker recurses on deeply nested programs: a big stack, as `fermium run` has
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).ok().and_then(|h| h.join().ok()).unwrap_or(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_columns() {
        assert_eq!(to_utf16("z = 2𝑖 + x", 7), 8);
        assert_eq!(from_utf16("z = 2𝑖 + x", 8), 7);
        assert_eq!(from_utf16("abc", 5), 5);
    }

    #[test]
    fn incremental_change() {
        let mut s = Server::new(Vec::new());
        s.docs.insert("u".into(), "a = 1\nz = 2𝑖 + 3 m\n".into());
        let ch = Json::parse(r#"{"range":{"start":{"line":1,"character":10},"end":{"line":1,"character":13}},"text":"1"}"#)
            .unwrap();
        s.apply_change("u", &ch);
        assert_eq!(s.docs["u"], "a = 1\nz = 2𝑖 + 1\n");
    }
}
