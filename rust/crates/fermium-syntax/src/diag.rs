//! Errors and warnings in plain physics language: a port of `fermium/errors.py`.
//!
//! Every user-facing problem has a line and column, one plain sentence, a caret under the problem, and usually
//! a hint. `format` prints exactly what the Python `FermiumError.format` prints.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// An edit that fixes a problem: (start offset, end offset, replacement), in code points of the source (D235).
pub type Fix = (usize, usize, String);

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub message: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub length: u32,
    pub hint: Option<String>,
    pub severity: Severity,
    pub fix: Vec<Fix>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, line: u32, col: u32, length: u32, hint: Option<String>) -> Self {
        Diagnostic { message: message.into(), line: Some(line), col: Some(col), length: length.max(1), hint,
                     severity: Severity::Error, fix: vec![] }
    }

    pub fn warning(message: impl Into<String>, line: u32, col: u32, length: u32, hint: Option<String>) -> Self {
        Diagnostic { severity: Severity::Warning, ..Self::error(message, line, col, length, hint) }
    }

    /// The message with its location, the source line, a caret, and the hint (like `FermiumError.format`).
    pub fn format(&self, source: Option<&str>, filename: Option<&str>) -> String {
        let line = self.line.filter(|l| *l != 0);
        let where_ = match (filename, line) {
            (Some(f), Some(l)) => format!("{f}, line {l}"),
            (None, Some(l)) => format!("line {l}"),
            _ => String::new(),
        };
        let prefix = if self.severity == Severity::Warning { "warning: " } else { "" };
        let head = if where_.is_empty() { self.message.clone() } else { format!("{where_}: {}", self.message) };
        let mut out = vec![format!("{prefix}{head}")];
        if let (Some(src), Some(line)) = (source, line) {
            let lines: Vec<&str> = src.split('\n').collect();
            if line >= 1 && (line as usize) <= lines.len() {
                let text = lines[line as usize - 1];
                out.push(format!("    {text}"));
                if let Some(col) = self.col {
                    if col >= 1 {
                        let width = text.chars().count() as i64 - col as i64 + 1;
                        let n = (self.length.max(1) as i64).min(width.max(1)).max(0) as usize;
                        out.push(format!("    {}{}", " ".repeat(col as usize - 1), "^".repeat(n)));
                    }
                }
            }
        }
        if let Some(h) = &self.hint {
            out.push(format!("  hint: {h}"));
        }
        out.join("\n")
    }
}

/// Warnings collected while compiling, in the order they were found (errors.Diagnostics): a warning with the same
/// message at the same place is given once.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub warnings: Vec<Diagnostic>,
    seen: std::collections::HashSet<(String, Option<u32>, Option<u32>)>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn warn(&mut self, d: Diagnostic) {
        let key = (d.message.clone(), d.line, d.col);
        if self.seen.contains(&key) {
            return;
        }
        self.seen.insert(key);
        self.warnings.push(Diagnostic { severity: Severity::Warning, ..d });
    }

    /// `warn(message, line=…, col=…, length=…, hint=…)`.
    pub fn warn_at(&mut self, message: impl Into<String>, line: u32, col: u32, length: u32, hint: Option<String>) {
        self.warn(Diagnostic { message: message.into(), line: Some(line), col: Some(col), length, hint,
                               severity: Severity::Warning, fix: vec![] });
    }
}
