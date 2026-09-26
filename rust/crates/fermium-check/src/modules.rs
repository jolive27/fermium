//! Modules (M7, D100–D102) and the checker's side of `use python` (D140). A port of `fermium/importer.py`
//! (loading a module, binding its names, pointing errors inside modules back at the user's program),
//! `fermium/modules.py` (the search path, the standard library and `fermium.toml`) and the parts of
//! `fermium/pyinterop.py` that don't need Python.
//!
//! A module is checked once per compilation, in its own scope (whose parent is the built-in constants), so its
//! functions see its own names and never the importing program's. Its constants become globals of the program's
//! main function (computed where the import is); its functions are ordinary generic functions, instantiated at
//! each call with the caller's units.
//!
//! The standard library (`stdlib/*.fm`, Fermium source) is embedded in the binary at build time; it is searched
//! last, as the folder `<stdlib>`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use fermium_ir as I;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::names::get_close_matches;

include!(concat!(env!("OUT_DIR"), "/stdlib.rs"));

/// The standard library's folder as the search path shows it (its files are embedded in the binary).
pub const STDLIB_DIR: &str = "<stdlib>";
pub const PROJECT_FILE: &str = "fermium.toml";

/// Run-time line codes of module code (errors.py, D185): `(k << MODLINE_SHIFT) | line`, where k − 1 is the text
/// id of the module's file name.
pub const MODLINE_SHIFT: u32 = 20;
pub const MODLINE_MAX: u32 = (1 << MODLINE_SHIFT) - 1;

pub fn encode_module_line(k: usize, line: u32) -> u32 {
    if line == 0 || line > MODLINE_MAX || !(0 < k && k < (1 << 12)) {
        return line;
    }
    ((k as u32) << MODLINE_SHIFT) | line
}

/// (program line, suffix) for a run-time line code: the suffix is "" for program code, else
/// " (in stdlib/stats.fm, line 6)"; `call` is the program line of the call into the module.
pub fn decode_line(code: u32, call: u32, texts: &[String]) -> (u32, String) {
    if code <= MODLINE_MAX {
        return (code, String::new());
    }
    let (k, ml) = ((code >> MODLINE_SHIFT) as usize, code & MODLINE_MAX);
    let name = if 0 < k && k <= texts.len() { texts[k - 1].as_str() } else { "a module" };
    (call, format!(" (in {name}, line {ml})"))
}

#[derive(Clone, Debug)]
pub struct ModuleInfo {
    /// the module's own name (file stem)
    pub name: String,
    pub path: String,
    pub scope: ScopeId,
    /// short path for messages: springs.fm, stdlib/mechanics.fm
    pub display: String,
}

/// The checker's module bookkeeping.
#[derive(Clone, Debug, Default)]
pub struct ModState {
    pub modules: Vec<ModuleInfo>,
    /// path → module, loaded in this compilation
    pub by_path: HashMap<String, usize>,
    /// (name, path) being loaded now (cycle detection)
    pub loading: Vec<(String, String)>,
    /// module scope → its module
    pub scope_module: HashMap<ScopeId, usize>,
    /// names a program or module defines at its top level: scope → name → the defining statement's span
    pub top_defs: HashMap<ScopeId, HashMap<String, A::Span>>,
    /// names imported into a scope: scope → name → (module, line)
    pub imported: HashMap<ScopeId, HashMap<String, (String, u32)>>,
    /// warnings already moved to the importing line (by index)
    pub warn_noted: HashSet<usize>,
    /// errors that already say where in the module they happened
    pub err_noted: HashSet<String>,
    /// `use python` modules (name as imported)
    pub py: Vec<String>,
    /// the last AST node id given to a module's nodes (ids are unique across the program and its modules)
    pub last_id: u32,
}

fn err_key(e: &Diagnostic) -> String {
    format!("{}|{:?}|{:?}", e.message, e.line, e.col)
}

fn top_defs(body: &[A::Stmt]) -> HashMap<String, A::Span> {
    let mut out = HashMap::new();
    for s in body {
        match &s.kind {
            A::StmtKind::FuncDef { name, .. } => {
                out.entry(name.clone()).or_insert(s.span);
            }
            A::StmtKind::Assign { name, op, .. } if op == "=" => {
                out.entry(name.clone()).or_insert(s.span);
            }
            A::StmtKind::Analyze { title: Some(t), .. } => {
                out.entry(t.clone()).or_insert(s.span);
            }
            _ => {}
        }
    }
    out
}

fn stmt_word(k: &A::StmtKind) -> &'static str {
    use A::StmtKind as K;
    match k {
        K::Print { .. } => "a print",
        K::Plot { .. } => "a plot",
        K::Solve(_) => "a solve",
        K::Fit { .. } => "a fit",
        K::If { .. } => "an if",
        K::For { .. } | K::ForIn { .. } => "a for loop",
        K::While { .. } => "a while loop",
        K::ExprStmt { .. } => "a bare expression",
        K::Assert { .. } => "an assert",
        K::Analyze { .. } => "an analyze",
        K::IndexAssign { .. } => "an element assignment",
        _ => "a statement",
    }
}

fn is_identifier(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_alphanumeric() || c == '_')
}

fn safe(stem: &str) -> String {
    let out: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if !out.is_empty() && !out.chars().next().unwrap().is_ascii_digit() { out } else { format!("m_{out}") }
}

fn stdlib_source(name: &str) -> Option<&'static str> {
    STDLIB.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

fn stdlib_path(name: &str) -> String {
    format!("{STDLIB_DIR}/{name}.fm")
}

fn abspath(p: &str) -> PathBuf {
    let p = if p.is_empty() { "." } else { p };
    let pb = Path::new(p);
    let abs = if pb.is_absolute() { pb.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(pb) };
    normpath(&abs)
}

/// os.path.normpath for absolute paths: drop `.`, resolve `..` lexically.
fn normpath(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn dirname(p: &str) -> String {
    Path::new(p).parent().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

fn basename(p: &str) -> String {
    Path::new(p).file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

// ------------------------------------------------------------ fermium.toml (modules.py)
/// A project (the folder holding fermium.toml).
#[derive(Clone, Debug, Default)]
pub struct Project {
    pub root: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub module_paths: Vec<String>,
}

#[derive(Clone, Debug)]
enum TomlVal {
    Str(String),
    List(Vec<TomlVal>),
    Other,
}

fn parse_toml_value(v: &str) -> Option<TomlVal> {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let mut items = vec![];
        let mut rest = inner.trim();
        while !rest.is_empty() {
            let q = rest.chars().next()?;
            if q != '"' && q != '\'' {
                return None;
            }
            let end = rest[1..].find(q)? + 1;
            items.push(TomlVal::Str(rest[1..end].to_string()));
            rest = rest[end + 1..].trim_start();
            if let Some(r) = rest.strip_prefix(',') {
                rest = r.trim_start();
            } else if !rest.is_empty() {
                return None;
            }
        }
        return Some(TomlVal::List(items));
    }
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return Some(TomlVal::Str(v[1..v.len() - 1].to_string()));
        }
    }
    if v.parse::<f64>().is_ok() || v == "true" || v == "false" {
        return Some(TomlVal::Other);
    }
    None
}

/// Enough TOML for fermium.toml: [tables], key = "str" or ["list", "of", "str"] (modules.py _mini_toml).
fn mini_toml(path: &str, src: &str) -> Result<Vec<(String, Vec<(String, TomlVal)>)>, Diagnostic> {
    let mut out: Vec<(String, Vec<(String, TomlVal)>)> = vec![];
    let bad = |n: usize, raw: &str| {
        let mut d = Diagnostic::error(format!("{path} isn't a valid fermium.toml (line {n}: {})", raw.trim()), 0, 0, 1,
                                      None);
        d.line = None;
        d.col = None;
        d
    };
    for (i, raw) in src.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_string();
            if !out.iter().any(|(t, _)| *t == name) {
                out.push((name, vec![]));
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { return Err(bad(i + 1, raw)) };
        if out.is_empty() {
            return Err(bad(i + 1, raw));
        }
        let Some(val) = parse_toml_value(v) else { return Err(bad(i + 1, raw)) };
        out.last_mut().unwrap().1.push((k.trim().to_string(), val));
    }
    Ok(out)
}

fn toml_err(msg: String, hint: Option<&str>) -> Diagnostic {
    let mut d = Diagnostic::error(msg, 0, 0, 1, hint.map(str::to_string));
    d.line = None;
    d.col = None;
    d
}

pub fn read_project(path: &str) -> Result<Project, Diagnostic> {
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let data = mini_toml(path, &src)?;
    let root = dirname(path);
    let mut proj = Project { root: root.clone(), ..Default::default() };
    let table = |name: &str| data.iter().find(|(t, _)| t == name).map(|(_, kv)| kv);
    if let Some(info) = table("project") {
        for (k, v) in info {
            if let TomlVal::Str(s) = v {
                match k.as_str() {
                    "name" => proj.name = Some(s.clone()),
                    "version" => proj.version = Some(s.clone()),
                    _ => {}
                }
            }
        }
    }
    if let Some(paths) = table("paths") {
        if let Some((_, v)) = paths.iter().find(|(k, _)| k == "modules") {
            let TomlVal::List(items) = v else {
                return Err(toml_err(format!("{path}: modules under [paths] must be a list of folder names"),
                                    Some("write  modules = [\"lib\"]")));
            };
            for m in items {
                let TomlVal::Str(m) = m else {
                    return Err(toml_err(format!("{path}: modules under [paths] must be a list of folder names"),
                                        Some("write  modules = [\"lib\"]")));
                };
                proj.module_paths.push(normpath(&Path::new(&root).join(m)).to_string_lossy().into_owned());
            }
        }
    }
    Ok(proj)
}

/// The nearest fermium.toml in start_dir or a parent folder, read; None when there is none.
pub fn find_project(start_dir: &str) -> Result<Option<Project>, Diagnostic> {
    let mut d = abspath(start_dir);
    loop {
        let p = d.join(PROJECT_FILE);
        if p.is_file() {
            return read_project(&p.to_string_lossy()).map(Some);
        }
        if !d.pop() {
            return Ok(None);
        }
    }
}

/// Folders searched for `import name`, in order: the importing file's folder, the program's folder, the folders
/// of fermium.toml's `[paths] modules`, the standard library.
pub fn module_search_path(program_dir: &str, importer_dir: Option<&str>) -> Result<Vec<String>, Diagnostic> {
    let mut out: Vec<String> = vec![];
    let prog = abspath(program_dir).to_string_lossy().into_owned();
    for d in [importer_dir.map(str::to_string), Some(prog)].into_iter().flatten() {
        if !d.is_empty() && !out.contains(&d) {
            out.push(d);
        }
    }
    if let Some(proj) = find_project(program_dir)? {
        for d in proj.module_paths {
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out.push(STDLIB_DIR.into());
    Ok(out)
}

/// (file path, searched folders); the path is None when not found.
pub fn resolve_module(name: &str, is_path: bool, program_dir: &str, importer_dir: Option<&str>)
                      -> Result<(Option<String>, Vec<String>), Diagnostic> {
    if is_path {
        let base = match importer_dir {
            Some(d) if !d.starts_with(STDLIB_DIR) => d.to_string(),
            _ => abspath(program_dir).to_string_lossy().into_owned(),
        };
        let expanded = match (name.strip_prefix("~/"), std::env::var("HOME")) {
            (Some(rest), Ok(h)) => format!("{h}/{rest}"),
            _ => name.to_string(),
        };
        let p = normpath(&Path::new(&base).join(expanded)).to_string_lossy().into_owned();
        let found = Path::new(&p).is_file();
        return Ok((if found { Some(p) } else { None }, vec![base]));
    }
    let folders = module_search_path(program_dir, importer_dir)?;
    for d in &folders {
        if d == STDLIB_DIR {
            if stdlib_source(name).is_some() {
                return Ok((Some(stdlib_path(name)), folders.clone()));
            }
            continue;
        }
        let p = Path::new(d).join(format!("{name}.fm"));
        if p.is_file() {
            return Ok((Some(normpath(&p).to_string_lossy().into_owned()), folders.clone()));
        }
    }
    Ok((None, folders))
}

pub fn available_modules(folders: &[String]) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = Default::default();
    for d in folders {
        if d == STDLIB_DIR {
            out.extend(STDLIB.iter().map(|(n, _)| n.to_string()));
            continue;
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if let Some(stem) = n.strip_suffix(".fm") {
                    out.insert(stem.to_string());
                }
            }
        }
    }
    out.into_iter().collect()
}

fn same_binding(a: &Binding, b: &Binding) -> bool {
    match (a, b) {
        (Binding::Sym(x), Binding::Sym(y)) => x == y,
        (Binding::Func(x), Binding::Func(y)) => x == y,
        (Binding::Sol(x), Binding::Sol(y)) => x == y,
        (Binding::Local(x), Binding::Local(y)) => x == y,
        (Binding::Module(x), Binding::Module(y)) => x == y,
        (Binding::PyModule(x), Binding::PyModule(y)) => x == y,
        (Binding::Const(x), Binding::Const(y)) => x.name == y.name,
        _ => false,
    }
}

/// Give the IR of module code line codes (D185): lines of its statements and expressions (not called functions).
fn encode_stmts(stmts: &mut [I::Stmt], k: usize) {
    for s in stmts {
        encode_stmt(s, k);
    }
}

fn enc(line: &mut u32, k: usize) {
    if *line > 0 && *line <= MODLINE_MAX {
        *line = encode_module_line(k, *line);
    }
}

fn encode_stmt(s: &mut I::Stmt, k: usize) {
    use I::StmtKind as K;
    enc(&mut s.line, k);
    match &mut s.kind {
        K::Assign(_, e) | K::Push(_, e) | K::Expr(e) | K::Assert(e, _) => encode_expr(e, k),
        K::IndexAssign(_, a, b) => {
            encode_expr(a, k);
            encode_expr(b, k);
        }
        K::If(c, a, b) => {
            encode_expr(c, k);
            encode_stmts(a, k);
            encode_stmts(b, k);
        }
        K::While(c, b) => {
            encode_expr(c, k);
            encode_stmts(b, k);
        }
        K::For { lo, hi, step, body, .. } => {
            encode_expr(lo, k);
            encode_expr(hi, k);
            if let Some(st) = step {
                encode_expr(st, k);
            }
            encode_stmts(body, k);
        }
        K::ForIn(_, e, b) => {
            encode_expr(e, k);
            encode_stmts(b, k);
        }
        K::Print(items) => {
            for it in items {
                match it {
                    I::PrintItem::Num(e, _) | I::PrintItem::List(e, _) | I::PrintItem::Complex(e, _)
                    | I::PrintItem::Vec(e, _) | I::PrintItem::MixedVec(e, _) | I::PrintItem::Mat(e, _)
                    | I::PrintItem::ComplexList(e, _) | I::PrintItem::TextList(e) | I::PrintItem::Bool(e)
                    | I::PrintItem::TextVar(e) | I::PrintItem::Data(e, _) => encode_expr(e, k),
                    I::PrintItem::Text(_) => {}
                }
            }
        }
        K::Return(Some(e)) => encode_expr(e, k),
        K::Propagate { body, .. } => encode_stmts(body, k),
        _ => {}
    }
}

fn encode_expr(e: &mut I::Expr, k: usize) {
    enc(&mut e.line, k);
    use I::ExprKind as K;
    match &mut e.kind {
        K::Bin(_, a, b) | K::Pow(a, b) | K::Cmp(_, a, b) | K::Index(a, b) | K::Uncertain(a, b) => {
            encode_expr(a, k);
            encode_expr(b, k);
        }
        K::Logic { a, b, .. } => {
            encode_expr(a, k);
            encode_expr(b, k);
        }
        K::Approx { a, b, atol, .. } => {
            encode_expr(a, k);
            encode_expr(b, k);
            if let Some(t) = atol {
                encode_expr(t, k);
            }
        }
        K::PowC(a, _) | K::Neg(a) | K::Not(a) | K::VecElem(a, _) | K::Column(a, _) => encode_expr(a, k),
        K::Call(_, args) | K::Map { args, .. } | K::Builtin(_, args) | K::List(args) | K::Vec(args) | K::Table(args) => {
            for a in args {
                encode_expr(a, k);
            }
        }
        K::VecSet { v, idxs, value } => {
            encode_expr(v, k);
            for (i, _, _) in idxs {
                encode_expr(i, k);
            }
            encode_expr(value, k);
        }
        K::VecIndex { v, idxs, .. } => {
            encode_expr(v, k);
            for (i, _, _) in idxs {
                encode_expr(i, k);
            }
        }
        K::If(a, b, c) => {
            encode_expr(a, k);
            encode_expr(b, k);
            encode_expr(c, k);
        }
        K::Let(binds, body) => {
            for (_, b) in binds {
                encode_expr(b, k);
            }
            encode_expr(body, k);
        }
        K::Integral { lo, hi, .. } | K::Root { lo, hi, .. } => {
            encode_expr(lo, k);
            encode_expr(hi, k);
        }
        K::Sum { lo, hi, step, .. } => {
            encode_expr(lo, k);
            encode_expr(hi, k);
            if let Some(s) = step {
                encode_expr(s, k);
            }
        }
        K::SolEval { t, .. } => encode_expr(t, k),
        K::PdeEval { x, t, .. } => {
            encode_expr(x, k);
            encode_expr(t, k);
        }
        _ => {}
    }
}

/// The lambdas an expression tree refers to (integrands, sums, roots), for encoding their bodies too.
fn lambdas_in_stmts(stmts: &[I::Stmt], out: &mut Vec<I::LambdaId>) {
    fn ex(e: &I::Expr, out: &mut Vec<I::LambdaId>) {
        match &e.kind {
            I::ExprKind::Integral { lam, .. } | I::ExprKind::Sum { lam, .. } | I::ExprKind::Root { lam, .. }
            | I::ExprKind::Sample { lam, .. } => {
                out.push(*lam)
            }
            _ => {}
        }
        for c in expr_children(e) {
            ex(c, out);
        }
    }
    for s in stmts {
        crate::modules::for_each_stmt_expr(s, &mut |e| ex(e, out));
        if let I::StmtKind::Solve { rhs, .. } = &s.kind {
            out.push(*rhs);
        }
        if let I::StmtKind::Fit { model, .. } = &s.kind {
            out.push(*model);
        }
    }
}

fn expr_children(e: &I::Expr) -> Vec<&I::Expr> {
    use I::ExprKind as K;
    match &e.kind {
        K::Bin(_, a, b) | K::Pow(a, b) | K::Cmp(_, a, b) | K::Index(a, b) | K::Uncertain(a, b) => vec![a, b],
        K::Logic { a, b, .. } => vec![a, b],
        K::Approx { a, b, atol, .. } => {
            let mut v: Vec<&I::Expr> = vec![a, b];
            if let Some(t) = atol {
                v.push(t);
            }
            v
        }
        K::PowC(a, _) | K::Neg(a) | K::Not(a) | K::VecElem(a, _) | K::Column(a, _) => vec![a],
        K::Call(_, args) | K::Map { args, .. } | K::Builtin(_, args) | K::List(args) | K::Vec(args) | K::Table(args) => {
            args.iter().collect()
        }
        K::VecSet { v, idxs, value } => {
            let mut out: Vec<&I::Expr> = vec![v];
            out.extend(idxs.iter().map(|x| &x.0));
            out.push(value);
            out
        }
        K::VecIndex { v, idxs, .. } => {
            let mut out: Vec<&I::Expr> = vec![v];
            out.extend(idxs.iter().map(|x| &x.0));
            out
        }
        K::If(a, b, c) => vec![a, b, c],
        K::Let(binds, body) => {
            let mut out: Vec<&I::Expr> = binds.iter().map(|x| &x.1).collect();
            out.push(body);
            out
        }
        K::Integral { lo, hi, .. } | K::Root { lo, hi, .. } => vec![lo, hi],
        K::Sum { lo, hi, step, .. } => {
            let mut out: Vec<&I::Expr> = vec![lo, hi];
            if let Some(s) = step {
                out.push(s);
            }
            out
        }
        K::SolEval { t, .. } => vec![t],
        K::PdeEval { x, t, .. } => vec![x, t],
        _ => vec![],
    }
}

fn for_each_stmt_expr(s: &I::Stmt, f: &mut dyn FnMut(&I::Expr)) {
    use I::StmtKind as K;
    match &s.kind {
        K::Assign(_, e) | K::Push(_, e) | K::Expr(e) | K::Assert(e, _) | K::Return(Some(e)) => f(e),
        K::IndexAssign(_, a, b) => {
            f(a);
            f(b)
        }
        K::If(c, a, b) => {
            f(c);
            a.iter().chain(b.iter()).for_each(|s| for_each_stmt_expr(s, f));
        }
        K::While(c, b) | K::ForIn(_, c, b) => {
            f(c);
            b.iter().for_each(|s| for_each_stmt_expr(s, f));
        }
        K::For { lo, hi, step, body, .. } => {
            f(lo);
            f(hi);
            if let Some(st) = step {
                f(st);
            }
            body.iter().for_each(|s| for_each_stmt_expr(s, f));
        }
        K::Print(items) => {
            for it in items {
                match it {
                    I::PrintItem::Num(e, _) | I::PrintItem::List(e, _) | I::PrintItem::Complex(e, _)
                    | I::PrintItem::Vec(e, _) | I::PrintItem::MixedVec(e, _) | I::PrintItem::Mat(e, _)
                    | I::PrintItem::ComplexList(e, _) | I::PrintItem::TextList(e) | I::PrintItem::Bool(e)
                    | I::PrintItem::TextVar(e) | I::PrintItem::Data(e, _) => f(e),
                    I::PrintItem::Text(_) => {}
                }
            }
        }
        K::Propagate { body, .. } => body.iter().for_each(|s| for_each_stmt_expr(s, f)),
        _ => {}
    }
}

impl Checker {
    // ------------------------------------------------------------ bookkeeping
    /// Remember a program's (or module's) top-level definitions, for clash errors (Python note_program).
    pub fn note_program(&mut self, prog: &A::Program, scope: ScopeId) {
        self.mods.top_defs.insert(scope, top_defs(&prog.body));
    }

    fn mod_display(&self, path: &str) -> String {
        if dirname(path) == STDLIB_DIR {
            return format!("stdlib/{}", basename(path));
        }
        let base = abspath(&self.opts.base_dir);
        let p = normpath(Path::new(path));
        match p.strip_prefix(&base) {
            Ok(rel) if rel.as_os_str().is_empty() => "the program's folder".into(),
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => path.to_string(),
        }
    }

    /// Mark the module code's IR with line codes, so run-time errors in it say where in the module (D185).
    fn encode_module_code(&mut self, stmts: &mut [I::Stmt], display: &str) {
        let k = self.text(display) + 1;
        let mut lams = vec![];
        lambdas_in_stmts(stmts, &mut lams);
        encode_stmts(stmts, k);
        self.encode_lambdas(lams, k);
    }

    fn encode_lambdas(&mut self, mut lams: Vec<I::LambdaId>, k: usize) {
        let mut seen = HashSet::new();
        while let Some(l) = lams.pop() {
            if !seen.insert(l) {
                continue;
            }
            let mut body = std::mem::take(&mut self.module.lambdas[l].body);
            for e in &mut body {
                let wrapped = vec![I::Stmt { kind: I::StmtKind::Expr(e.clone()), line: 0 }];
                lambdas_in_stmts(&wrapped, &mut lams);
                encode_expr(e, k);
            }
            self.module.lambdas[l].body = body;
        }
    }

    // ------------------------------------------------------------ the statement
    pub fn s_import(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::Import { module, is_path, alias, names } = &s.kind else { unreachable!() };
        let kind = self.scopes[ctx.scope].kind;
        if !ctx.is_main || ctx.lam.is_some() || ctx.branch != 0 || ctx.loop_depth != 0
            || !(kind == "global" || kind == "module")
        {
            return Err(self.err("import must be at the top level of the program (not inside a block or function)",
                                s.span, None));
        }
        let importer = self.mods.scope_module.get(&ctx.scope).copied();
        let importer_dir = importer.map(|m| dirname(&self.mods.modules[m].path));
        let (path, folders) = match resolve_module(module, *is_path, &self.opts.base_dir, importer_dir.as_deref()) {
            Ok(x) => x,
            Err(mut e) => {
                // a broken fermium.toml
                if e.line.is_none() {
                    e.line = Some(s.span.line);
                    e.col = Some(s.span.col);
                    e.length = s.span.length.max(1);
                }
                return Err(e);
            }
        };
        let Some(path) = path else { return Err(self.not_found(s, module, *is_path, &folders)) };
        let base = basename(&path);
        let stem = base.strip_suffix(".fm").map(str::to_string).unwrap_or_else(|| {
            Path::new(&base).file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
        });
        if names.is_none() && alias.is_none() && !is_identifier(&stem) {
            return Err(self.err(format!("the module file {base} has a name that can't be used in a program"), s.span,
                                Some(format!("give it a name with as:  import \"{module}\" as {}", safe(&stem)))));
        }
        let mut stmts = vec![];
        let info = match self.mods.by_path.get(&path) {
            Some(&m) => m,
            None => {
                let (m, st) = self.load_module(&path, &stem, s, ctx)?;
                stmts = st;
                m
            }
        };
        match names {
            None => {
                let n = alias.clone().unwrap_or(stem);
                self.bind_import(ctx.scope, &n, Binding::Module(info), s, info, None)?;
            }
            Some(list) => {
                for (name, al) in list {
                    let mname = self.mods.modules[info].name.clone();
                    if name.starts_with('_') {
                        return Err(self.err(format!("{name} is private to the module {mname} (names starting with _ \
                                                     aren't exported)"), s.span, None));
                    }
                    let mscope = self.mods.modules[info].scope;
                    let Some(b) = self.scopes[mscope].names.get(name).cloned() else {
                        return Err(self.no_member(info, name, s.span));
                    };
                    self.bind_import(ctx.scope, al.as_ref().unwrap_or(name), b, s, info, Some(name))?;
                }
            }
        }
        Ok(stmts)
    }

    fn not_found(&self, s: &A::Stmt, module: &str, is_path: bool, folders: &[String]) -> Diagnostic {
        if is_path {
            return self.err(format!("can't find the module file \"{module}\""), s.span,
                            Some(format!("the path is relative to the folder of the file with the import ({})",
                                         folders[0])));
        }
        let known = available_modules(folders);
        let close = get_close_matches(module, &known, 1, 0.6);
        let place = folders
            .iter()
            .map(|f| if f == STDLIB_DIR { "the standard library".to_string() } else { self.mod_display(f) })
            .collect::<Vec<_>>()
            .join(", ");
        let mut hint = close.first().map(|c| format!("did you mean {c}? ")).unwrap_or_default();
        hint += &format!("looked for {module}.fm in: {place}");
        self.err(format!("can't find a module called {module}"), s.span, Some(hint))
    }

    /// The names a module exports (not starting with _), sorted.
    pub fn exported(&self, m: usize) -> Vec<String> {
        let mut v: Vec<String> =
            self.scopes[self.mods.modules[m].scope].names.keys().filter(|n| !n.starts_with('_')).cloned().collect();
        v.sort();
        v
    }

    fn no_member(&self, m: usize, name: &str, span: A::Span) -> Diagnostic {
        let ex = self.exported(m);
        let close = get_close_matches(name, &ex, 1, 0.6);
        let mname = &self.mods.modules[m].name;
        let hint = match close.first() {
            Some(c) => format!("did you mean {c}?"),
            None => format!("{mname} defines: {}{}", ex.iter().take(12).cloned().collect::<Vec<_>>().join(", "),
                            if ex.len() > 12 { " …" } else { "" }),
        };
        self.err(format!("{mname} has no {name}"), span, Some(hint))
    }

    fn bind_import(&mut self, scope: ScopeId, name: &str, obj: Binding, s: &A::Stmt, m: usize, member: Option<&str>)
                   -> CResult<()> {
        let mname = self.mods.modules[m].name.clone();
        let existing = self.scopes[scope].names.get(name).cloned();
        let what = match member {
            Some(mb) => format!("{mname}.{mb}"),
            None => format!("the module {mname}"),
        };
        if let Some(ex) = &existing {
            if !same_binding(ex, &obj) {
                if let Some((src, ln)) = self.mods.imported.get(&scope).and_then(|i| i.get(name)).cloned() {
                    return Err(self.err(format!("{name} is already imported from {src} (line {ln}); importing it from \
                                                 {mname} too would be ambiguous"), s.span,
                                        Some(format!("give one a new name, like  from {mname} import {} as {name}_{mname}, \
                                                      or  import {mname} as ... and write the full name",
                                                     member.unwrap_or(name)))));
                }
                let hint = match member {
                    Some(mb) => format!("import it under another name:  from {mname} import {mb} as {mb}_{mname}"),
                    None => format!("import it under another name:  import {mname} as {mname}_mod"),
                };
                return Err(self.err(format!("{name} already means something in this program, so it can't also be \
                                             {what}"), s.span, Some(hint)));
            }
        }
        if let Some(later) = self.mods.top_defs.get(&scope).and_then(|t| t.get(name)).copied() {
            if later.line > s.span.line && !self.opts.repl {
                return Err(self.err(format!("{name} is {what} (imported on line {}) and is defined again on line {}",
                                            s.span.line, later.line), later,
                                    Some(format!("rename your {name}, or import it under another name with as"))));
            }
        }
        self.scopes[scope].names.insert(name.to_string(), obj);
        self.mods.imported.entry(scope).or_default().entry(name.to_string()).or_insert((mname, s.span.line));
        Ok(())
    }

    // ------------------------------------------------------------ loading
    fn load_module(&mut self, path: &str, stem: &str, s: &A::Stmt, ctx: &mut Ctx) -> CResult<(usize, Vec<I::Stmt>)> {
        if let Some(i) = self.mods.loading.iter().position(|(_, p)| p == path) {
            let mut chain: Vec<String> = self.mods.loading[i..].iter().map(|(n, _)| n.clone()).collect();
            chain.push(stem.to_string());
            return Err(self.err(format!("circular import: {}", chain.join(" → ")), s.span,
                                Some("modules can't import each other in a circle; move the shared functions into a \
                                      third module that both import".into())));
        }
        let display = self.mod_display(path);
        let src = if dirname(path) == STDLIB_DIR {
            stdlib_source(stem).unwrap_or("").to_string()
        } else {
            match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return Err(self.err(format!("can't read the module {display}: {e}"), s.span, None)),
            }
        };
        let mut info = ModuleInfo { name: stem.to_string(), path: path.to_string(), scope: 0, display: display.clone() };
        let after = self.nodes.keys().copied().max().unwrap_or(0).max(self.mods.last_id);
        let (prog, pdiags, last) = match fermium_syntax::parse_module(&src, after) {
            Ok(x) => x,
            Err(e) => return Err(self.wrap(e, &info, s)),
        };
        self.mods.last_id = last;
        // the checker keeps references into the module's AST for the rest of the compilation
        let prog: &'static A::Program = Box::leak(Box::new(prog));
        let mut tops = vec![];
        crate::walk::all_exprs_in_stmts(&prog.body, &mut tops);
        for t in tops {
            for n in t.walk() {
                self.nodes.entry(n.id).or_insert(n as *const A::Expr);
            }
        }
        for st in &prog.body {
            let ok = match &st.kind {
                A::StmtKind::FuncDef { .. } | A::StmtKind::Import { .. } => true,
                A::StmtKind::Assign { op, .. } => op == "=",
                _ => false,
            };
            if !ok {
                let what = match &st.kind {
                    A::StmtKind::Assign { name, .. } => format!("a change to {name}"),
                    k => stmt_word(k).to_string(),
                };
                let e = Diagnostic::error(format!("a module can only define functions and constants, but this line has \
                                                   {what}"), st.span.line, st.span.col, st.span.length,
                                          Some("modules can't print or compute things when imported; put that in your \
                                                program".into()));
                return Err(self.wrap(e, &info, s));
            }
        }
        let scope = self.new_scope(Some(self.root), "module");
        info.scope = scope;
        let m = self.mods.modules.len();
        self.mods.modules.push(info.clone());
        self.mods.scope_module.insert(scope, m);
        self.note_program(prog, scope);
        let mut mctx = Ctx { func: ctx.func, scope, is_main: true, lam: None, loop_depth: 0, branch: 0,
                             ret_types: self.new_ret_types(), regions: vec![], lam_parents: vec![] };
        let saved_future = std::mem::take(&mut self.future_funcs);
        let saved_pos = std::mem::take(&mut self.positive_names);
        self.future_funcs = prog
            .body
            .iter()
            .filter_map(|st| match &st.kind {
                A::StmtKind::FuncDef { name, .. } => Some((name.clone(), st.span.line)),
                _ => None,
            })
            .collect();
        let n0 = self.diags.warnings.len();
        self.mods.loading.push((stem.to_string(), path.to_string()));
        let r = self.block(&prog.body, &mut mctx);
        self.mods.loading.pop();
        self.future_funcs = saved_future;
        self.positive_names = saved_pos;
        let mut stmts = match r {
            Ok(st) => st,
            Err(e) => return Err(self.wrap(e, &info, s)),
        };
        // run-time errors in its constants name the module (D185)
        self.encode_module_code(&mut stmts, &display);
        for w in pdiags.warnings {
            self.diags.warnings.push(w);
        }
        self.relocate_warnings(n0, m, s.span, None);
        let names: Vec<(String, Binding)> =
            self.scopes[scope].names.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (name, b) in names {
            if let Binding::Func(fi) = b {
                if self.funcs[fi].module.is_none() {
                    self.funcs[fi].module = Some(m);
                    if self.funcs[fi].display_name == name {
                        self.funcs[fi].display_name = format!("{stem}.{name}");
                    }
                }
            }
        }
        self.mods.by_path.insert(path.to_string(), m);
        Ok((m, stmts))
    }

    fn wrap(&self, e: Diagnostic, info: &ModuleInfo, s: &A::Stmt) -> Diagnostic {
        let ln = e.line.filter(|l| *l != 0).map(|l| format!(", line {l}")).unwrap_or_default();
        Diagnostic::error(format!("in the module {} ({}{ln}): {}", info.name, info.display, e.message), s.span.line,
                          s.span.col, s.span.length, e.hint)
    }

    fn relocate_warnings(&mut self, n0: usize, m: usize, at: A::Span, fname: Option<&str>) {
        let info = self.mods.modules[m].clone();
        for i in n0..self.diags.warnings.len() {
            let w = &mut self.diags.warnings[i];
            if !self.mods.warn_noted.insert(i) {
                w.line = Some(at.line);
                w.col = Some(at.col);
                w.length = at.length.max(1);
                continue;
            }
            let place = match w.line.filter(|l| *l != 0) {
                Some(l) => format!("{}, line {l}", info.display),
                None => info.display.clone(),
            };
            let owner = fname.map(str::to_string).unwrap_or_else(|| format!("the module {}", info.name));
            w.message = format!("{} (in {owner}, {place})", w.message);
            w.line = Some(at.line);
            w.col = Some(at.col);
            w.length = at.length.max(1);
        }
    }

    // ------------------------------------------------------------ calls into a module
    /// Instantiate a module's function: warnings from its body are shown at the call.
    pub fn module_call(&mut self, info: FuncInfoId, args: Vec<Checked>, node: &A::Expr, cache: bool)
                       -> CResult<I::Expr> {
        let n0 = self.diags.warnings.len();
        let added = self.in_module_call.insert(info);
        let r = self.instantiate(info, args, node, cache);
        if added {
            self.in_module_call.remove(&info);
        }
        let m = self.funcs[info].module.unwrap();
        let r = match r {
            Ok(r) => {
                if let I::ExprKind::Call(inst, _) | I::ExprKind::Map { func: inst, .. } = &r.kind {
                    let inst = *inst;
                    if self.mods_encoded_insert(inst) {
                        // run-time errors inside say where in the module (D185)
                        let display = self.mods.modules[m].display.clone();
                        let mut body = std::mem::take(&mut self.module.funcs[inst].body);
                        self.encode_module_code(&mut body, &display);
                        self.module.funcs[inst].body = body;
                    }
                }
                Ok(r)
            }
            Err(mut e) => {
                let fspan = self.funcs[info].fdef.as_ref().map(|f| f.span).unwrap_or_default();
                if (e.line, e.col) == (Some(fspan.line), Some(fspan.col)) {
                    // about the definition (e.g. never returns)
                    self.module_body_error(&mut e, info, node);
                }
                Err(e)
            }
        };
        if node.span.line != 0 {
            let dn = self.funcs[info].display_name.clone();
            self.relocate_warnings(n0, m, node.span, Some(&dn));
        }
        r
    }

    fn mods_encoded_insert(&mut self, inst: I::FuncId) -> bool {
        let key = format!("inst:{inst}");
        self.mods.err_noted.insert(key)
    }

    /// An error inside a module function's body: say where in the module, and point at the call.
    pub fn module_body_error(&mut self, e: &mut Diagnostic, info: FuncInfoId, node: &A::Expr) {
        if node.span.line == 0 {
            return;
        }
        let Some(m) = self.funcs[info].module else { return };
        if !self.mods.err_noted.contains(&err_key(e)) {
            let display = &self.mods.modules[m].display;
            let place = match e.line.filter(|l| *l != 0) {
                Some(l) => format!("{display}, line {l}"),
                None => display.clone(),
            };
            e.message = format!("{} (in {}, {place})", e.message, self.funcs[info].display_name);
        }
        e.line = Some(node.span.line);
        e.col = Some(node.span.col).filter(|c| *c != 0);
        e.length = node.span.length.max(1);
        self.mods.err_noted.insert(err_key(e));
    }

    // ------------------------------------------------------------ using a module's names
    /// The module a Name / Field node refers to, or None.
    pub fn module_of(&self, node: &A::Expr, ctx: &Ctx) -> Option<usize> {
        self.module_of_in(node, ctx.scope)
    }

    /// The module a Name / Field node refers to from a scope, or None.
    pub fn module_of_in(&self, node: &A::Expr, scope: ScopeId) -> Option<usize> {
        match &node.kind {
            A::ExprKind::Name { name } => match self.lookup(scope, name) {
                Some((Binding::Module(m), _)) => Some(m),
                _ => None,
            },
            A::ExprKind::Field { target, name } => {
                let m = self.module_of_in(target, scope)?;
                match self.scopes[self.mods.modules[m].scope].names.get(name) {
                    Some(Binding::Module(x)) => Some(*x),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `mechanics.pendulum_period`: a module's name used from the program.
    pub fn module_member(&mut self, m: usize, e: &A::Expr, name: &str, ctx: &mut Ctx) -> CResult<Checked> {
        let mname = self.mods.modules[m].name.clone();
        if name.starts_with('_') {
            return Err(self.err(format!("{name} is private to the module {mname} (names starting with _ aren't \
                                         exported)"), e.span, None));
        }
        let Some(b) = self.scopes[self.mods.modules[m].scope].names.get(name).cloned() else {
            return Err(self.no_member(m, name, e.span));
        };
        self.use_binding(b, &format!("{mname}.{name}"), e, ctx)
    }

    pub fn module_as_value(&self, m: usize, name: &str, e: &A::Expr) -> Diagnostic {
        let ex = self.exported(m);
        let eg = match ex.first() {
            Some(x) => format!("{name}.{x}"),
            None => format!("{name}.name"),
        };
        self.err(format!("{name} is a module, not a value; use the names it defines, like {eg}"), e.span, None)
    }

    /// `hooke isn't defined` after `import springs`: point to springs.hooke.
    pub fn module_hint(&self, name: &str, ctx: &Ctx) -> Option<String> {
        let mut s = Some(ctx.scope);
        while let Some(id) = s {
            let mut entries: Vec<(&String, &Binding)> = self.scopes[id].names.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (alias, b) in entries {
                if let Binding::Module(m) = b {
                    let info = &self.mods.modules[*m];
                    if self.scopes[info.scope].names.contains_key(name) && !name.starts_with('_') {
                        return Some(format!("{name} is in the module {}: write {alias}.{name}, or  from {} import {name}",
                                            info.name, info.name));
                    }
                }
            }
            s = self.scopes[id].parent;
        }
        None
    }

    // ------------------------------------------------------------ use python (D140)
    /// `use python numpy as np`: Python interop is loaded at run time and only when imported (spec §B5.14), the
    /// last milestone; until then a program that uses it stops here with one clear error (rust/DIVERGENCES.md).
    pub fn s_use_python(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let kind = self.scopes[ctx.scope].kind;
        if !ctx.is_main || ctx.lam.is_some() || ctx.branch != 0 || ctx.loop_depth != 0
            || !(kind == "global" || kind == "module")
        {
            return Err(self.err("use python must be at the top level of the program (not inside a block or function)",
                                s.span, None));
        }
        if self.mods.scope_module.contains_key(&ctx.scope) {
            return Err(self.err("a Fermium module can't use Python yet; put the  use python  line in the program",
                                s.span, None));
        }
        Err(self.err("use python needs Python interop, which this build doesn't have yet", s.span,
                     Some("run it with the Python implementation (legacy/) for now".into())))
    }

    pub fn py_module_name(&self, m: usize) -> String {
        self.mods.py.get(m).cloned().unwrap_or_else(|| "?".into())
    }
}
