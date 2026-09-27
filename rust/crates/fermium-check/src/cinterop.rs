//! The checker's side of `import c` and `import fortran` (spec C3, DECISIONS D275): calling C and Fortran
//! functions through the C ABI with the units checked at every call.
//!
//! ```text
//! import c "libphys.so":
//!     kinetic_energy(m [kg], v [km/s]) -> [J]
//!     twice(n: int) -> int
//!     sum_sq(x: list [m], n: len(x)) -> [m²]
//! import fortran "libnuclear.so":
//!     binding_energy(Z: int, A: int) -> [MeV]                      # the symbol binding_energy_
//!     neutron_separation(Z: int, A: int) -> [MeV] bind(C, name="semf_sn")
//! ```
//!
//! The signatures are those of `use python` (pyinterop.rs), plus `: list [unit]` (an array of doubles) and
//! `: len(x)` (its length, filled in by Fermium). The library is opened when the program is checked, relative to
//! the program's folder, and every symbol is looked up then, so a missing library or function is a compile error.
//! Each argument must have the declared unit's dimension; it is passed as SI ÷ the unit's factor, and the result
//! is multiplied back. Fortran passes everything by reference; its default symbol is the lowercase name with a
//! trailing underscore (gfortran, flang), `bind(C)` keeps the lowercase name, `bind(C, name="…")` is exact.
//!
//! A call is the IR built-in `ccall`, whose first argument is the index of its entry in `tables.ccalls`; both back
//! ends end up in fermium-runtime's `cffi` (the LLVM back end calls the function directly when every argument and
//! the result are doubles).
use std::path::Path;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_runtime::cffi;
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::*;
use crate::exprs::hint_of;
use crate::units::Unit;

/// C++'s keywords (and alternative tokens): none can be a function's name (`phys::new`).
const CPP_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor", "bool", "break", "case", "catch", "char",
    "char8_t", "char16_t", "char32_t", "class", "compl", "concept", "const", "consteval", "constexpr", "constinit",
    "const_cast", "continue", "co_await", "co_return", "co_yield", "decltype", "default", "delete", "do", "double",
    "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto", "if",
    "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "not", "not_eq", "nullptr", "operator", "or",
    "or_eq", "private", "protected", "public", "register", "reinterpret_cast", "requires", "return", "short",
    "signed", "sizeof", "static", "static_assert", "static_cast", "struct", "switch", "template", "this",
    "thread_local", "throw", "true", "try", "typedef", "typeid", "typename", "union", "unsigned", "using", "virtual",
    "void", "volatile", "wchar_t", "while", "xor", "xor_eq",
];

/// A declared C or Fortran function (what its name is bound to).
#[derive(Clone, Debug)]
pub struct CFuncRef {
    /// the name as written in the program
    pub display: String,
    /// "C" or "Fortran"
    pub lang: &'static str,
    /// the library as written, and the path it was opened with
    pub lib: String,
    pub path: String,
    pub symbol: String,
    /// (name, kind, unit, index of the list for a len)
    pub params: Vec<(String, I::CParamKind, Option<Unit>, usize)>,
    pub rint: bool,
    pub runit: Option<Unit>,
    /// the signature's line
    pub line: u32,
}

/// The ASCII name of a Fermium name for the linker: Greek letters spelled out (`δ_e` → `delta_e`, as `fmt
/// --ascii` writes it) and subscript digits as `_0` (`E₀` → `E_0`).
pub fn c_symbol_name(name: &str) -> String {
    let mut out = String::new();
    let mut in_sub = false;
    for c in crate::pyinterop::ascii_name(name).chars() {
        if ('₀'..='₉').contains(&c) {
            if !in_sub {
                out.push('_');
            }
            in_sub = true;
            out.push(char::from_u32('0' as u32 + (c as u32 - '₀' as u32)).unwrap());
        } else {
            in_sub = false;
            out.push(c);
        }
    }
    out
}

impl Checker {
    /// `import c "lib.so":` / `import fortran "lib.so":` with signatures.
    pub fn s_import_c(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::ImportC { lang, lib, header, sigs } = &s.kind else { unreachable!() };
        let fortran = lang == "fortran";
        let cpp = lang == "cpp";
        let what: &'static str = if fortran { "Fortran" } else if cpp { "C++" } else { "C" };
        let kind = self.scopes[ctx.scope].kind;
        if !ctx.is_main || ctx.lam.is_some() || ctx.branch != 0 || ctx.loop_depth != 0
            || !(kind == "global" || kind == "module")
        {
            return Err(self.err(format!("import {lang} must be at the top level of the program (not inside a block or \
                                         function)"), s.span, None));
        }
        if self.mods.scope_module.contains_key(&ctx.scope) {
            return Err(self.err(format!("a Fermium module can't import a {what} library yet; put the  import {lang}  \
                                         lines in the program"), s.span, None));
        }
        if !cffi::supported() {
            return Err(self.err(format!("calling {what} functions isn't supported on this platform yet (only x86-64 \
                                         and AArch64 Linux and macOS)"), s.span, None));
        }
        if cpp {
            return self.s_import_cpp(s, ctx, lib, header.as_deref().unwrap_or(""), sigs);
        }
        // the library: relative to the program's folder; a bare name the program's folder doesn't have is left
        // to the system's search (libm.so.6)
        let path = self.c_library_path(lib);
        self.load_c_library(lang, what, lib, &path, s.span)?;
        let mut refs = vec![];
        let mut names: Vec<String> = vec![];
        for sig in sigs {
            if names.contains(&sig.name) {
                return Err(self.err(format!("{} has two signatures in this import", sig.name), sig.span, None));
            }
            names.push(sig.name.clone());
            let ascii = c_symbol_name(&sig.name);
            if !ascii.is_ascii() {
                return Err(self.err(format!("{} can't be the name of a {what} function: a symbol must be ASCII", sig.name),
                                    sig.span, Some("use an ASCII name, or  bind(C, name=\"…\")  in Fortran".into())));
            }
            let (bind_c, bind_name) = match &sig.bind {
                None => (false, None),
                Some(n) => (true, n.as_deref()),
            };
            let symbol = if fortran { cffi::fortran_symbol(&ascii, bind_c, bind_name) } else { ascii.clone() };
            if !self.c_has_symbol(&path, &symbol) {
                return Err(self.missing_symbol(what, lib, &path, &sig.name, &ascii, &symbol, fortran, bind_c, sig.span));
            }
            refs.push(self.c_signature(sig, what, lib, &path, symbol, sig.span.line)?);
        }
        self.bind_c_functions(refs, s, ctx, lib, what, fortran)
    }

    /// The library's path as dlopen'ed: relative to the program's folder, or a bare name the system finds.
    fn c_library_path(&self, lib: &str) -> String {
        let local = Path::new(&self.opts.base_dir).join(lib);
        if Path::new(lib).is_absolute() {
            lib.to_string()
        } else if local.exists() || lib.contains('/') {
            let p = std::fs::canonicalize(&local).unwrap_or(local);
            p.to_string_lossy().into_owned()
        } else {
            lib.to_string()
        }
    }

    /// Does the library define the symbol? Only checking (no_load): read from the file, and when the file can't
    /// be read that way, assume so (the run says otherwise).
    fn c_has_symbol(&self, path: &str, symbol: &str) -> bool {
        if self.opts.no_load {
            return cffi::file_has_symbol(path, symbol).unwrap_or(true);
        }
        cffi::symbol(path, symbol).ok().flatten().is_some()
    }

    /// Open the library now, so a missing one is a compile error with a hint. Only checking (no_load), the file
    /// must exist when it is named by a path; it isn't opened.
    fn load_c_library(&self, lang: &str, what: &str, lib: &str, path: &str, span: A::Span) -> CResult<()> {
        let base = self.opts.base_dir.clone();
        let fortran = lang == "fortran";
        let opened = if self.opts.no_load {
            if Path::new(path).is_absolute() && !Path::new(path).exists() || lib.contains('/') && !Path::new(path).exists() {
                Err("no such file".to_string())
            } else {
                Ok(0)
            }
        } else {
            cffi::open_library(path)
        };
        if let Err(why) = opened {
            let stem = lib.trim_start_matches("lib").split('.').next().unwrap_or("phys").to_string();
            let build = if fortran {
                format!("gfortran -shared -fPIC -o {lib} {stem}.f90")
            } else if lang == "cpp" {
                format!("c++ -shared -fPIC -o {lib} {stem}.cpp")
            } else {
                format!("cc -shared -fPIC -o {lib} {stem}.c")
            };
            let hint = if Path::new(&path).exists() || !lib.contains('/') && Path::new(&base).join(lib).exists() {
                format!("the file is there but isn't a shared library this machine can load: {why}")
            } else {
                format!("build it next to the program first, like  {build}  (the path is relative to the program's \
                         folder)")
            };
            return Err(self.err(format!("can't load the {what} library {lib}"), span, Some(hint)));
        }
        Ok(())
    }

    /// `import cpp "libphys.so" header "phys.hpp":` (C4, D290): make (or find in the cache) the wrapper, then bind
    /// each signature to its C function there.
    fn s_import_cpp(&mut self, s: &A::Stmt, ctx: &mut Ctx, lib: &str, header: &str, sigs: &[A::CSig])
                    -> CResult<Vec<I::Stmt>> {
        let what = "C++";
        let path = if lib.is_empty() { String::new() } else { self.c_library_path(lib) };
        if !lib.is_empty() {
            self.load_c_library("cpp", what, lib, &path, s.span)?;
        }
        // absolute, so the compiler's dependency list is too (the cache compares its files' times)
        let base_dir = if self.opts.base_dir.is_empty() { "." } else { self.opts.base_dir.as_str() };
        let base = std::fs::canonicalize(base_dir).map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| base_dir.to_string());
        if header.is_empty() || header.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | '"')) {
            return Err(self.err(format!("{header:?} can't be the name of a header"), s.span, None));
        }
        // the program's own header (a path relative to its folder, or absolute) is included by its absolute path,
        // with the program's and the header's folders on the include path; any other name is left to the
        // compiler's own include path, with no folder of the program's on it (D320: a file in the program's
        // folder can't stand in for <cstdio>)
        let local = Path::new(&base).join(header);
        let (include, system) = if Path::new(header).is_absolute() {
            (header.to_string(), false)
        } else if local.is_file() {
            (std::fs::canonicalize(&local).unwrap_or(local).to_string_lossy().into_owned(), false)
        } else {
            (header.to_string(), true) // a system or installed header, like cmath or Eigen/Dense
        };
        let mut names: Vec<String> = vec![];
        let mut csigs = vec![];
        for sig in sigs {
            if names.contains(&sig.name) {
                let hint = format!("give each overload its own name after its result, like  … -> [J] as {}_2", sig.name);
                return Err(self.err(format!("{} has two signatures in this import", sig.name), sig.span, Some(hint)));
            }
            names.push(sig.name.clone());
            let written = sig.cpp_name.clone().unwrap_or_else(|| sig.name.clone());
            let qual: Vec<String> = written.split("::").map(c_symbol_name).collect();
            if qual.iter().any(|q| !q.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
                return Err(self.err(format!("{written} can't be the name of a C++ function: a C++ name must be ASCII"),
                                    sig.span, None));
            }
            if let Some(kw) = qual.iter().find(|q| CPP_KEYWORDS.contains(&q.as_str())) {
                return Err(self.err(format!("{kw} is a C++ keyword, so {written} can't be imported"), sig.span,
                                    Some("operators and keywords can't be called from Fermium: add a function with \
                                          an ordinary name to the library that calls it".into())));
            }
            let params = sig.params.iter().map(|p| p.kind.clone()).collect();
            csigs.push(crate::cppinterop::CppSig { qual: qual.join("::"), params,
                                                   rint: matches!(sig.ret, A::CRetDecl::Int) });
        }
        let imp = crate::cppinterop::CppImport { lib, lib_path: &path, header, include, system, base: &base,
                                                 sigs: csigs };
        let wrapper = match crate::cppinterop::build_wrapper(&imp) {
            Ok(w) => w,
            Err(e) => {
                let span = e.sig.map(|k| sigs[k].span).unwrap_or(s.span);
                return Err(self.err(e.msg, span, e.hint));
            }
        };
        if !self.opts.no_load {
            if let Err(why) = cffi::open_library(&wrapper) {
                return Err(self.err(format!("can't load the compiled C++ wrapper {wrapper}"), s.span,
                                    Some(format!("delete it so that it is made again ({why})"))));
            }
        }
        let shown = if lib.is_empty() { header } else { lib };
        let mut refs = vec![];
        for (k, sig) in sigs.iter().enumerate() {
            let symbol = crate::cppinterop::wrapper_symbol(k);
            if !self.c_has_symbol(&wrapper, &symbol) {
                return Err(self.err(format!("the compiled C++ wrapper {wrapper} has no {symbol}"), sig.span,
                                    Some("delete it so that it is made again".into())));
            }
            refs.push(self.c_signature(sig, what, shown, &wrapper, symbol, sig.span.line)?);
        }
        self.bind_c_functions(refs, s, ctx, shown, what, false)
    }

    /// Bind each declared function's name in the program.
    fn bind_c_functions(&mut self, refs: Vec<CFuncRef>, s: &A::Stmt, ctx: &mut Ctx, lib: &str, what: &'static str,
                        fortran: bool) -> CResult<Vec<I::Stmt>> {
        for r in refs {
            let name = r.display.clone();
            if let Some(ex) = self.scopes[ctx.scope].names.get(&name).cloned() {
                let redo = matches!(ex, Binding::CFunc(_)) && self.opts.repl;
                if !redo {
                    let hint = if fortran {
                        Some(format!("rename your {name}, or use  bind(C, name=\"{}\")  with another name", r.symbol))
                    } else if what == "C++" {
                        Some(format!("call it by another name: add  as my_{name}  after its result"))
                    } else {
                        None
                    };
                    return Err(self.err(format!("{name} already means something in this program, so it can't also be \
                                                 the {what} function {name}"), s.span, hint));
                }
            }
            if let Some(later) = self.mods.top_defs.get(&ctx.scope).and_then(|t| t.get(&name)).copied() {
                if later.line > s.span.line && !self.opts.repl {
                    return Err(self.err(format!("{name} is the {what} function from {lib} (line {}) and is defined \
                                                 again on line {}", s.span.line, later.line), later,
                                        Some(format!("rename your {name}"))));
                }
            }
            self.mods.cfuncs.push(r);
            let idx = self.mods.cfuncs.len() - 1;
            self.scopes[ctx.scope].names.insert(name, Binding::CFunc(idx));
        }
        Ok(vec![])
    }

    #[allow(clippy::too_many_arguments)]
    fn missing_symbol(&self, what: &str, lib: &str, path: &str, name: &str, ascii: &str, symbol: &str, fortran: bool,
                      bind_c: bool, span: A::Span) -> Diagnostic {
        let has = |s: &str| {
            if self.opts.no_load { cffi::file_has_symbol(path, s) == Some(true) } else { cffi::symbol(path, s).ok().flatten().is_some() }
        };
        let lower = ascii.to_lowercase();
        let under = format!("{lower}_");
        let hint = if fortran && !bind_c && has(&lower) {
            format!("the library has {lower} without the trailing underscore (a bind(C) function): add  bind(C)  after \
                     the result")
        } else if fortran && bind_c && symbol != under && has(&under) {
            format!("the library has {under}, Fortran's default spelling: remove  bind(C…)")
        } else if !fortran && has(&format!("{ascii}_")) {
            format!("the library has {ascii}_, a Fortran compiler's spelling: use  import fortran \"{lib}\"")
        } else if !fortran && ascii != lower && has(&lower) {
            format!("the library has {lower}: C names are case-sensitive")
        } else {
            format!("check the spelling (nm -D {lib} lists the functions it defines)")
        };
        let shown = if symbol == name { String::new() } else { format!(" (symbol {symbol})") };
        self.err(format!("the {what} library {lib} has no function {name}{shown}"), span, Some(hint))
    }

    fn c_signature(&self, sig: &A::CSig, what: &'static str, lib: &str, path: &str, symbol: String, line: u32)
                   -> CResult<CFuncRef> {
        if sig.params.len() > cffi::MAX_ARGS {
            return Err(self.err(format!("{} has {} parameters; a {what} function can take at most {} here", sig.name,
                                        sig.params.len(), cffi::MAX_ARGS), sig.span, None));
        }
        let mut params = vec![];
        for p in &sig.params {
            if params.iter().any(|(n, ..): &(String, I::CParamKind, Option<Unit>, usize)| *n == p.name) {
                return Err(self.err(format!("{} has two parameters named {}", sig.name, p.name), p.span, None));
            }
            let (kind, unit, len_of) = match &p.kind {
                A::CParamKind::Num(u) => (I::CParamKind::Num, u.as_ref().map(|u| self.c_unit(u, p.span)).transpose()?, 0),
                A::CParamKind::Int => (I::CParamKind::Int, None, 0),
                A::CParamKind::List(u) => {
                    (I::CParamKind::List, u.as_ref().map(|u| self.c_unit(u, p.span)).transpose()?, 0)
                }
                A::CParamKind::Len(x) => {
                    let Some(k) = sig.params.iter().position(|q| q.name == *x) else {
                        return Err(self.err(format!("{}: len({x}) needs a list parameter named {x}", sig.name), p.span,
                                            Some(format!("declare it, like  {x}: list [m]"))));
                    };
                    if !matches!(sig.params[k].kind, A::CParamKind::List(_)) {
                        return Err(self.err(format!("{}: {x} isn't a list, so len({x}) can't be passed", sig.name),
                                            p.span, Some(format!("declare it as a list:  {x}: list [m]"))));
                    }
                    (I::CParamKind::Len, None, k)
                }
            };
            params.push((p.name.clone(), kind, unit, len_of));
        }
        let lens = params.iter().filter(|p| p.1 == I::CParamKind::Len).count();
        if let Some(l) = params.iter().find(|p| p.1 == I::CParamKind::List) {
            if lens == 0 {
                return Err(self.err(format!("{} takes the list {}, so it needs its length too", sig.name, l.0),
                                    sig.span, Some(format!("add a parameter like  n: len({})", l.0))));
            }
        }
        let (rint, runit) = match &sig.ret {
            A::CRetDecl::Int => (true, None),
            A::CRetDecl::Number => (false, None),
            A::CRetDecl::Unit(u) => (false, Some(self.c_unit(u, sig.span)?)),
        };
        Ok(CFuncRef { display: sig.name.clone(), lang: what, lib: lib.to_string(), path: path.to_string(), symbol,
                      params, rint, runit, line })
    }

    fn c_unit(&self, uexpr: &A::UnitExpr, span: A::Span) -> CResult<Unit> {
        let u = self.resolve_unit(uexpr)?;
        if u.offset != 0.0 {
            return Err(self.err(format!("a C function's unit can't be {} (a scale with an offset); use K", u.name),
                                span, None));
        }
        Ok(u)
    }

    /// The C function a name stands for, if it is one.
    pub fn c_ref_of(&mut self, name: &str, ctx: &Ctx) -> Option<usize> {
        match self.lookup(ctx.scope, name) {
            Some((Binding::CFunc(c), _)) => Some(c),
            _ => None,
        }
    }

    /// kinetic_energy(2 kg, 3 km/s): a call of a C or Fortran function.
    pub fn c_call(&mut self, cref: usize, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Call { args: aargs, .. } = &e.kind else { unreachable!() };
        let r = self.mods.cfuncs[cref].clone();
        let name = r.display.clone();
        if self.nat.natural() {
            return Err(self.err(format!("{name}: {} functions can't be called inside  {}  yet", r.lang,
                                        self.nat.label()), e.span,
                                Some("call it outside the region and bring the value in".into())));
        }
        let visible: Vec<usize> = (0..r.params.len()).filter(|&k| r.params[k].1 != I::CParamKind::Len).collect();
        if aargs.len() != visible.len() {
            let n = visible.len();
            return Err(self.err(format!("{name} takes {n} argument{} (as declared in the import on line {}), but got {}",
                                        if n != 1 { "s" } else { "" }, r.line, aargs.len()), e.span, None));
        }
        let mut vals = Vec::with_capacity(aargs.len() + 1);
        let mut map = false;
        for (&k, node) in visible.iter().zip(aargs.iter()) {
            let (pname, kind, unit, _) = r.params[k].clone();
            let a = self.expr_any(node, ctx)?;
            let a = match a {
                Checked::Val(v) if matches!(v.ty, Ty::Num(_) | Ty::List(_)) => v,
                other => {
                    let what = match &other {
                        Checked::Val(v) => match v.ty {
                            Ty::Complex(_) => "a complex number",
                            Ty::Vec { .. } => "a vector",
                            Ty::Mat { .. } => "a matrix",
                            _ => "not a number",
                        },
                        _ => "a function",
                    };
                    let hint = match what {
                        "a vector" => Some("pass the components one at a time, like v.x".to_string()),
                        "a complex number" => {
                            Some("pass the real and imaginary parts one at a time, like re(z) and im(z)".to_string())
                        }
                        _ => None,
                    };
                    return Err(self.err(format!("{name}: a {} function takes numbers and lists of numbers, but {pname} \
                                                 is {what}", r.lang), node.span, hint));
                }
            };
            let is_list = matches!(a.ty, Ty::List(_));
            if kind == I::CParamKind::List && !is_list {
                return Err(self.err(format!("{name} expects {pname} to be a list (declared on line {}), but got a \
                                             single number", r.line), node.span,
                                    Some("put it in brackets to pass a list of one, like [x]".into())));
            }
            if kind != I::CParamKind::List && is_list {
                map = true; // a list for a number: one call per element
            }
            let dim = match &a.ty {
                Ty::Num(d) | Ty::List(d) => d.clone(),
                _ => unreachable!(),
            };
            let line = r.line;
            match (&unit, kind) {
                (_, I::CParamKind::Int) => {
                    self.unify_or(&dim, &DExpr::of(DIMLESS), |c| {
                        format!("{name} expects {pname} to be a whole number (declared as int on line {line}), but got \
                                 {}", c.desc(&dim))
                    }, node.span, Some("divide by a unit to get a plain number, like  x / (1 m)".into()))?;
                }
                (None, _) => {
                    self.unify_or(&dim, &DExpr::of(DIMLESS), |c| {
                        format!("{name} expects {pname} to be a plain number (declared on line {line}), but got {}",
                                c.desc(&dim))
                    }, node.span, Some(format!("divide by a unit, like  {pname} / (1 m),  or declare the unit in the \
                                                import:  {pname} [m]")))?;
                }
                (Some(u), _) => {
                    let want = self.desc(&DExpr::of(u.dim));
                    let want_name = want.split(" [").next().unwrap_or(&want).to_string();
                    let un = u.name.clone();
                    self.unify_or(&dim, &DExpr::of(u.dim), |c| {
                        format!("{name} expects {pname} in {un} (declared on line {line}), but got {}", c.desc(&dim))
                    }, node.span, Some(format!("pass a {want_name}, like  1 {un}")))?;
                    if a.hint.is_some() {
                        // the C function receives the number in the declared unit: 60 rpm as [Hz] is 2π, not 1
                        self.warn_angle_in_hz(&a, &hint_of(u), node,
                                              &format!("passing {pname} to {name} as [{}]: ", u.name));
                    }
                }
            }
            vals.push(a);
        }
        let params = r.params.iter().map(|(n, k, u, l)| I::CParam {
            name: n.clone(),
            kind: *k,
            fac: u.as_ref().map(|u| u.factor).unwrap_or(1.0),
            len_of: *l,
        }).collect();
        let tables = &mut self.module.tables;
        tables.ccalls.push(I::CCallSite {
            lib: r.path.clone(),
            symbol: r.symbol.clone(),
            display: name.clone(),
            by_ref: r.lang == "Fortran",
            params,
            rint: r.rint,
            rfac: r.runit.as_ref().map(|u| u.factor).unwrap_or(1.0),
            map,
            cpp: r.lang == "C++",
        });
        let id = tables.ccalls.len() - 1;
        let line = e.span.line;
        let mut all = Vec::with_capacity(vals.len() + 1);
        all.push(ir(I::ExprKind::Const(id as f64), Ty::Num(DExpr::of(DIMLESS)), line));
        all.extend(vals);
        let rdim = r.runit.as_ref().map(|u| u.dim).unwrap_or(DIMLESS);
        let ty = if map { Ty::List(DExpr::of(rdim)) } else { Ty::Num(DExpr::of(rdim)) };
        let mut out = ir(I::ExprKind::Builtin("ccall".into(), all), ty, line);
        if let Some(u) = &r.runit {
            if u.name != "1" && !u.name.is_empty() {
                out.hint = Some(hint_of(u));
            }
        }
        Ok(Checked::Val(out))
    }

    /// `kinetic_energy` used without ( ).
    pub fn c_func_as_value(&self, c: usize, name: &str, e: &A::Expr) -> Diagnostic {
        let r = &self.mods.cfuncs[c];
        let ps: Vec<&str> = r.params.iter().filter(|p| p.1 != I::CParamKind::Len).map(|p| p.0.as_str()).collect();
        self.err(format!("{name} is a {} function from {}; it can only be called, like {name}({})", r.lang, r.lib,
                         ps.join(", ")), e.span, None)
    }
}
