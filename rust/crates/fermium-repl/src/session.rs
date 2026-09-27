//! One interactive session (v1's `driver.ReplSession`): inputs are checked one after another by a single
//! checker in REPL mode (names persist, a variable may be redefined with other units, D13) and run by the
//! tree-walker with the variables kept between inputs. A failed input leaves no names behind (D220): the checker
//! is rolled back to a copy taken before the input.
use std::io::Write;

use fermium_check::checker::{Binding, CheckOptions, Checker};
use fermium_codegen::eval::Value;
use fermium_codegen::printer::StdPrinter;
use fermium_codegen::session::ReplState;
use fermium_ir::types::Ty;
use fermium_syntax::ast as A;
use fermium_syntax::diag::{Diagnostic, Severity};

pub struct Session {
    pub checker: Checker,
    pub state: ReplState,
    /// names assigned or defined so far (the parser's unit/variable collision rule needs them)
    pub known: Vec<String>,
    /// every input's tree, kept alive: the checker keeps pointers to nodes it has seen
    programs: Vec<Box<A::Program>>,
    pub count: usize,
}

impl Session {
    pub fn new(base_dir: &str) -> Session {
        let opts = CheckOptions { base_dir: base_dir.to_string(), repl: true, source_name: String::new(), no_load: false };
        Session { checker: Checker::new(opts), state: ReplState::new(), known: vec![], programs: vec![], count: 0 }
    }

    /// Check and run one input. Printed output goes to `out`, warnings to `warn` (None: `out`, as the REPL shows
    /// them; Jupyter: stderr). Err: the error, to be shown with `e.format(Some(text), None)`.
    pub fn execute(&mut self, text: &str, out: &mut dyn Write, warn: Option<&mut dyn Write>)
                   -> Result<(), Diagnostic> {
        let snap = (self.checker.clone(), self.known.clone());
        let r = self.execute_inner(text, out, warn);
        if r.is_err() {
            self.checker = snap.0;
            self.known = snap.1;
        }
        let _ = out.flush();
        r
    }

    fn execute_inner(&mut self, text: &str, out: &mut dyn Write, warn: Option<&mut dyn Write>) -> Result<(), Diagnostic> {
        self.count += 1;
        self.checker.diags = Default::default();
        let (prog, pdiags) = fermium_syntax::parse(text, &self.known)?;
        let prog = Box::new(prog);
        let module = self.checker.check_program(&prog);
        self.programs.push(prog);
        let module = module?;
        // the next input builds on this module: symbol and function ids stay valid
        self.checker.module = module.clone();
        for s in &self.programs.last().unwrap().body {
            if let A::StmtKind::Assign { name, .. } | A::StmtKind::FuncDef { name, .. } = &s.kind {
                if !self.known.contains(name) {
                    self.known.push(name.clone());
                }
            }
        }
        {
            let warn: &mut dyn Write = match warn {
                Some(w) => w,
                None => &mut *out,
            };
            for w in pdiags.warnings.iter().chain(self.checker.diags.warnings.iter()) {
                let _ = writeln!(warn, "{}", w.format(Some(text), None));
            }
            let _ = warn.flush();
        }
        let mut printer = StdPrinter::new(&module, &mut *out);
        let r = self.state.run(&module, &mut printer);
        drop(printer);
        r.map_err(|e| Diagnostic { message: e.message, line: if e.line > 0 { Some(e.line) } else { None }, col: None,
                                   length: 1, hint: e.hint, severity: Severity::Error, fix: vec![] })
    }

    /// The names defined so far (a variable called `cd` or `ls` is not a terminal command).
    pub fn names(&self) -> Vec<String> {
        self.checker.global_names().into_iter().map(|(n, _)| n).collect()
    }

    /// `:vars`: one line per name (repl.py `describe_vars`).
    pub fn describe_vars(&self) -> String {
        let ck = &self.checker;
        let mut lines = vec![];
        for (name, b) in ck.global_names() {
            if name.starts_with("__") || name.contains('\'') || name.contains("_∂") {
                continue;
            }
            match b {
                Binding::Sym(id) => {
                    let Some(sym) = ck.module.syms.get(id) else { continue };
                    if let (Ty::Num(d), Some(Value::Num(v))) = (&sym.ty, self.state.value(id)) {
                        let dim = ck.u.resolve(d);
                        let hint = sym.hint.as_ref().map(|h| fermium_units::Unit {
                            name: h.name.clone(),
                            dim: h.dim,
                            factor: h.factor,
                            offset: h.offset,
                        });
                        let s = fermium_units::quantity::format_quantity(*v, &dim, hint.as_ref(),
                                                                         sym.sf.map(|x| x as i64), sym.direct, true,
                                                                         true);
                        lines.push(format!("{name} = {s}"));
                    } else {
                        lines.push(format!("{name}: {}", ck.type_text(&sym.ty, sym.hint.as_ref())));
                    }
                }
                Binding::Func(f) if ck.funcs[f].versions.len() > 1 => {
                    for v in ck.versions_of(f) {
                        lines.push(format!("{}: function", ck.version_sig(v))); // one line per version (C5)
                    }
                }
                Binding::Func(f) => {
                    let params = match &ck.funcs[f].fdef {
                        Some(A::Stmt { kind: A::StmtKind::FuncDef { params, .. }, .. }) => {
                            params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                        }
                        _ => "...".into(),
                    };
                    lines.push(format!("{name}({params}): function"));
                }
                Binding::Sol(_) => lines.push(format!("{name}: solution of an ODE")),
                Binding::Module(m) => {
                    if let Some((mname, _, _)) = ck.module_names(m) {
                        lines.push(format!("{name}: the module {mname}"));
                    }
                }
                _ => {}
            }
        }
        if lines.is_empty() {
            "(no variables yet)".into()
        } else {
            lines.join("\n")
        }
    }

    /// Is this input an unfinished block, e.g. an `if` without its body (repl.py `needs_more`)?
    pub fn needs_more(&self, text: &str) -> bool {
        needs_more(text, &self.known)
    }
}

const CONTINUE_MARKERS: &[&str] =
    &["expected an indented block", "ended before", "program ended", "the line ended", "solve needs a range"];

/// Is this input an unfinished block (repl.py `needs_more`)?
pub fn needs_more(text: &str, known: &[String]) -> bool {
    match fermium_syntax::parse(text, known) {
        Ok(_) => false,
        Err(e) => {
            if e.message.contains("solve needs a range") {
                return true;
            }
            if CONTINUE_MARKERS.iter().any(|m| e.message.contains(m)) {
                let n = text.trim_end_matches('\n').split('\n').count() as u32;
                return match e.line {
                    None | Some(0) => true,
                    Some(l) => l >= n,
                };
            }
            false
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Does this line start an indented block: if/for/while/else/elif/solve, or `f(x) =` with nothing after
/// (repl.py `opens_block`)?
pub fn opens_block(line: &str) -> bool {
    let st = line.trim();
    for kw in ["if", "for", "while", "else", "elif", "solve"] {
        if let Some(rest) = st.strip_prefix(kw) {
            if !rest.chars().next().is_some_and(is_word_char) {
                return true;
            }
        }
    }
    // [^\s=]+\([^)]*\)\s*=\s*(#.*)?$
    let Some(open) = st.find('(') else { return false };
    let head = &st[..open];
    if head.is_empty() || head.chars().any(|c| c.is_whitespace() || c == '=') {
        return false;
    }
    let after = &st[open + 1..];
    let Some(close) = after.find(')') else { return false };
    let rest = after[close + 1..].trim_start();
    let Some(rest) = rest.strip_prefix('=') else { return false };
    let rest = rest.trim_start();
    rest.is_empty() || rest.starts_with('#')
}

const SHELL_WORDS: &[&str] = &["fermium", "python", "python3", "pip", "pip3", "cd", "ls", "dir", "cat", "open",
                               "clear", "git", "brew", "sudo", "mkdir", "rm", "cp", "mv"];

/// A terminal command typed at the Fermium prompt (`fm> fermium run ke.fm`, spec A3.5): repl.py's SHELL_LINE,
/// not an assignment, and not a name the session defined. Returns the command word.
pub fn shell_command<'a>(s: &'a str, names: &[String]) -> Option<&'a str> {
    let mut words = s.split_whitespace();
    let first = words.next()?;
    if !SHELL_WORDS.contains(&first) || s.starts_with(char::is_whitespace) {
        return None;
    }
    // (\s+[-\w./~\\"'=:]+)*$
    let ok = |c: char| is_word_char(c) || "-./~\\\"'=:".contains(c);
    if !words.all(|w| w.chars().all(ok)) {
        return None;
    }
    // not ^\S+\s*[-+*/^]?=
    let tok_end = s.find(char::is_whitespace).unwrap_or(s.len());
    if s[..tok_end].char_indices().skip(1).any(|(_, c)| c == '=') {
        return None;
    }
    let rest = s[tok_end..].trim_start();
    let rest = rest.strip_prefix(['-', '+', '*', '/', '^']).unwrap_or(rest);
    if rest.starts_with('=') {
        return None;
    }
    if names.iter().any(|n| n == first) {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks() {
        assert!(opens_block("if x > 2 m"));
        assert!(opens_block("  else"));
        assert!(!opens_block("iffy = 2"));
        assert!(!opens_block("ifθ = 2"));
        assert!(opens_block("f(x) ="));
        assert!(opens_block("speed(h) =   # the speed"));
        assert!(!opens_block("f(x) = x^2"));
        assert!(!opens_block("x = (1 +"));
    }

    #[test]
    fn shell_lines() {
        assert_eq!(shell_command("fermium run ke.fm", &[]), Some("fermium"));
        assert_eq!(shell_command("ls -la", &[]), Some("ls"));
        assert_eq!(shell_command("cd = 3", &[]), None);
        assert_eq!(shell_command("cd += 3", &[]), None);
        assert_eq!(shell_command("ls", &["ls".into()]), None);
        assert_eq!(shell_command("cd (x)", &[]), None);
        assert_eq!(shell_command("print 1", &[]), None);
    }

    #[test]
    fn unfinished_blocks() {
        assert!(needs_more("if 1 > 0\n", &[]));
        assert!(needs_more("x = (1 +\n", &[]));
        assert!(!needs_more("x = 1\n", &[]));
    }
}
