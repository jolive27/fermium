//! The checker's side of `use python` (M6, DECISIONS D140–D141): calling Python functions from Fermium with the
//! units checked at the boundary. A port of `fermium/pyinterop.py` (PythonMixin), method by method.
//!
//! ```text
//! use python numpy as np
//! use python scipy.special as sp
//! use python mylib as ml:
//!     energy(m [kg], v [m/s]) -> [J]
//!     positions(t [s]) -> list [m]
//! ```
//!
//! Python functions take and return plain numbers. Without a declared signature every argument must be
//! dimensionless (a unit error at compile time says how to fix it: divide by a unit, like r / (1 m)) and the
//! result is a plain number (a list when an argument is a list). With a signature, each argument must have the
//! declared unit's dimension and is passed as a number in that unit; the result gets the declared unit.
//! The module and the function are looked up when the program is checked (libpython is loaded then, and only
//! then: fermium-runtime's `python` module), so a misspelt name is a compile error.
//!
//! A call is the IR built-in `pycall` whose first argument is the index of its entry in `tables.pycalls`; both
//! back ends end up in `fermium_runtime::python::call` (the LLVM back end through its built-in callback).
use std::collections::HashMap;

use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_ir::DIMLESS;
use fermium_runtime::python as py;
use fermium_syntax::ast as A;

use crate::checker::*;
use crate::exprs::hint_of;
use crate::units::Unit;

/// A declared signature: (parameter name, unit, is_int) for each parameter, the result's shape and unit.
#[derive(Clone, Debug)]
pub struct PySigInfo {
    pub params: Vec<(String, Option<Unit>, bool)>,
    pub shape: Option<String>,
    pub unit: Option<Unit>,
}

/// What `np` is bound to after `use python numpy as np` (pyinterop.py PyModRef).
#[derive(Clone, Debug)]
pub struct PyModRef {
    /// "scipy.special"
    pub module: String,
    /// "sp"
    pub alias: String,
    /// by the function's ASCII name
    pub sigs: HashMap<String, PySigInfo>,
    pub line: u32,
}

/// `sp.gamma` may be written sp.γ; Python wants the spelling with the Greek letters spelled out (lexer.py
/// GREEK_TO_ASCII).
pub fn ascii_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            let s = c.to_string();
            fermium_syntax::lexer::GREEK.iter().rev().filter(|(k, _)| *k != "inf").find(|(_, v)| *v == s)
                .map(|(k, _)| k.to_string()).unwrap_or(s)
        })
        .collect()
}

impl Checker {
    // ------------------------------------------------------------ the statement
    /// `use python numpy as np [: signatures]` (Python s_UsePython).
    pub fn s_use_python(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::UsePython { module, alias, sigs } = &s.kind else { unreachable!() };
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
        let base_dir = self.opts.base_dir.clone();
        match py::import_module(module, &base_dir) {
            Err(why) => {
                return Err(self.err(format!("use python needs Python 3, but it couldn't be loaded: {why}"), s.span,
                                    Some("install Python 3 (with its shared library, libpython), or name the \
                                          interpreter to use in the environment variable FERMIUM_PYTHON".into())));
            }
            Ok(py::Import::Missing(missing)) => {
                let top = missing.split('.').next().unwrap_or(&missing).to_string();
                let hint = if missing == *module || module.starts_with(&missing) {
                    format!("install it with  pip install {top}  (or put {missing}.py in the program's folder)")
                } else {
                    format!("{module} needs {missing}: install it with  pip install {top}")
                };
                return Err(self.err(format!("can't find the Python module {module}"), s.span, Some(hint)));
            }
            Ok(py::Import::Failed(why)) => {
                return Err(self.err(format!("importing the Python module {module} failed: {why}"), s.span, None));
            }
            Ok(py::Import::Ok) => {}
        }
        let name = alias.clone().unwrap_or_else(|| module.clone());
        let mut table: HashMap<String, PySigInfo> = HashMap::new();
        for sig in sigs {
            let key = ascii_name(&sig.name);
            if table.contains_key(&key) {
                return Err(self.err(format!("{} has two signatures in this use line", sig.name), sig.span, None));
            }
            self.py_attr(module, &name, &sig.name, sig.span, true)?;
            let mut params = vec![];
            for (pname, u, is_int) in &sig.params {
                let unit = match u {
                    Some(u) => Some(self.py_unit(u, sig.span)?),
                    None => None,
                };
                params.push((pname.clone(), unit, *is_int));
            }
            let unit = match &sig.ret_unit {
                Some(u) => Some(self.py_unit(u, sig.span)?),
                None => None,
            };
            table.insert(key, PySigInfo { params, shape: sig.ret_shape.clone(), unit });
        }
        let existing = self.scopes[ctx.scope].names.get(&name).cloned();
        if let Some(ex) = existing {
            let redo = matches!(ex, Binding::PyModule(_)) && self.opts.repl; // the REPL may redo it
            if !redo {
                if let Binding::PyModule(m) = ex {
                    if self.mods.py_refs[m].module == *module && sigs.is_empty() {
                        return Ok(vec![]);
                    }
                }
                return Err(self.err(format!("{name} already means something in this program, so it can't also be \
                                             the Python module {module}"), s.span,
                                    Some(format!("give the module another name:  use python {module} as {name}_py"))));
            }
        }
        if let Some(later) = self.mods.top_defs.get(&ctx.scope).and_then(|t| t.get(&name)).copied() {
            if later.line > s.span.line && !self.opts.repl {
                return Err(self.err(format!("{name} is the Python module {module} (line {}) and is defined again on \
                                             line {}", s.span.line, later.line), later,
                                    Some(format!("rename your {name}, or use another name after as"))));
            }
        }
        self.module.tables.py_base_dir = base_dir;
        self.mods.py.push(module.clone());
        self.mods.py_refs.push(PyModRef { module: module.clone(), alias: name.clone(), sigs: table,
                                          line: s.span.line });
        let idx = self.mods.py.len() - 1;
        self.scopes[ctx.scope].names.insert(name, Binding::PyModule(idx));
        Ok(vec![])
    }

    fn py_unit(&self, uexpr: &A::UnitExpr, span: A::Span) -> CResult<Unit> {
        let u = self.resolve_unit(uexpr)?;
        if u.offset != 0.0 {
            return Err(self.err(format!("a Python function's unit can't be {} (a scale with an offset); use K", u.name),
                                span, None));
        }
        Ok(u)
    }

    /// Look up module.attr (Python _py_attr): an error when it is missing, or not callable when it must be.
    fn py_attr(&self, module: &str, alias: &str, attr: &str, span: A::Span, want_callable: bool) -> CResult<py::Attr> {
        let a = py::attribute(module, attr, &ascii_name(attr))
            .map_err(|why| self.err(format!("looking up {alias}.{attr} in Python failed: {why}"), span, None))?;
        match &a {
            py::Attr::Missing(close) => {
                return Err(self.err(format!("the Python module {module} has no {attr}"), span,
                                    close.as_ref().map(|c| format!("did you mean {alias}.{c}?"))));
            }
            py::Attr::Number(_) if want_callable => {
                return Err(self.err(format!("{alias}.{attr} is a number in Python, so it can't be called"), span,
                                    Some(format!("write {alias}.{attr} without ( )"))));
            }
            py::Attr::Other(_) if want_callable => {
                return Err(self.err(format!("{alias}.{attr} is not a function in Python, so it can't be called"),
                                    span, None));
            }
            _ => {}
        }
        Ok(a)
    }

    // ------------------------------------------------------------ using it
    /// The Python module a name stands for, if it is one (index into `mods.py_refs`).
    pub fn py_ref_of(&mut self, target: &A::Expr, ctx: &mut Ctx) -> Option<usize> {
        if let A::ExprKind::Name { name } = &target.kind {
            if let Some((Binding::PyModule(m), _)) = self.lookup(ctx.scope, name) {
                return Some(m);
            }
        }
        None
    }

    /// np.pi: a number attribute of a Python module, read when the program is checked (a plain number).
    pub fn python_value(&mut self, pref: usize, e: &A::Expr, name: &str) -> CResult<Checked> {
        let r = self.mods.py_refs[pref].clone();
        let attr = e.attrs.raw.clone().unwrap_or_else(|| name.to_string());
        let a = self.py_attr(&r.module, &r.alias, &attr, e.span, false)?;
        match a {
            py::Attr::Callable(_) => Err(self.err(format!("{}.{attr} is a Python function; call it, like {}.{attr}(x)",
                                                         r.alias, r.alias), e.span, None)),
            py::Attr::Other(t) => Err(self.err(format!("{}.{attr} is a {t} in Python; Fermium can only use Python \
                                                        numbers and functions", r.alias), e.span, None)),
            py::Attr::Number(x) => Ok(Checked::Val(ir(I::ExprKind::Const(x), Ty::Num(DExpr::of(DIMLESS)), e.span.line))),
            py::Attr::Missing(_) => unreachable!(),
        }
    }

    /// np.sinc(x): a call of a Python function (Python python_call).
    pub fn python_call(&mut self, pref: usize, e: &A::Expr, ctx: &mut Ctx) -> CResult<Checked> {
        let A::ExprKind::Call { func: f, args: aargs } = &e.kind else { unreachable!() };
        let A::ExprKind::Field { name: fname, .. } = &f.kind else { unreachable!() };
        let r = self.mods.py_refs[pref].clone();
        let mut attr = f.attrs.raw.clone().unwrap_or_else(|| fname.clone());
        let found = self.py_attr(&r.module, &r.alias, &attr, f.span, true)?;
        if self.nat.natural() {
            return Err(self.err(format!("{}.{attr}: Python functions can't be called inside  {}  yet", r.alias,
                                        self.nat.label()), e.span,
                                Some("call it outside the region and bring the value in".into())));
        }
        let display = format!("{}.{attr}", r.alias);
        let sig = r.sigs.get(&ascii_name(&attr)).cloned();
        if let py::Attr::Callable(used) = found {
            attr = used; // sp.γ after fmt --pretty is sp.gamma
        }
        let mut args = Vec::with_capacity(aargs.len());
        for a in aargs {
            args.push(self.expr_any(a, ctx)?);
        }
        if let Some(sig) = &sig {
            if args.len() != sig.params.len() {
                let n = sig.params.len();
                return Err(self.err(format!("{display} takes {n} argument{} (as declared in the use line on line {}), \
                                             but got {}", if n != 1 { "s" } else { "" }, r.line, args.len()),
                                    e.span, None));
            }
        }
        let (mut facs, mut ints, mut pnames) = (vec![], vec![], vec![]);
        let mut any_list = false;
        let mut vals = Vec::with_capacity(args.len());
        for (k, (a, node)) in args.into_iter().zip(aargs.iter()).enumerate() {
            let (pname, unit, is_int) = match &sig {
                Some(s) => s.params[k].clone(),
                None => (format!("argument {}", k + 1), None, false),
            };
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
                    return Err(self.err(format!("{display}: a Python function takes numbers and lists of numbers, but \
                                                 {pname} is {what}"), node.span, hint));
                }
            };
            let dim = match &a.ty {
                Ty::Num(d) | Ty::List(d) => d.clone(),
                _ => unreachable!(),
            };
            any_list = any_list || matches!(a.ty, Ty::List(_));
            match &unit {
                None => {
                    let hint = self.py_unit_hint(&dim, node, &attr, aargs.len(), k);
                    let declared = sig.is_some();
                    let (disp, pn) = (display.clone(), pname.clone());
                    self.unify_or(&dim, &DExpr::of(DIMLESS), |c| {
                        let why = if declared {
                            " (declare a unit for it in the use line if the function expects one)"
                        } else {
                            ""
                        };
                        format!("{disp} is a Python function, which takes plain numbers, but {pn} is {}{why}",
                                c.desc(&dim))
                    }, node.span, Some(hint))?;
                    facs.push(1.0);
                }
                Some(u) => {
                    let (disp, pn, line) = (display.clone(), pname.clone(), r.line);
                    self.unify_or(&dim, &DExpr::of(u.dim), |c| {
                        format!("{disp} expects {pn} in {} (declared on line {line}), but got {}", u.name, c.desc(&dim))
                    }, node.span, None)?;
                    facs.push(u.factor);
                    if a.hint.is_some() {
                        // Python receives the number in the declared unit: 60 rpm as [Hz] is 2π, not 1 (D95, D202)
                        self.warn_angle_in_hz(&a, &hint_of(u), node,
                                              &format!("passing {pname} to {display} as [{}]: ", u.name));
                    }
                }
            }
            ints.push(is_int);
            pnames.push(pname);
            vals.push(a);
        }
        let runit = sig.as_ref().and_then(|s| s.unit.clone());
        let shape = sig.as_ref().and_then(|s| s.shape.clone());
        let rlist = match &shape {
            Some(s) => s == "list",
            None => any_list,
        };
        let rdim = runit.as_ref().map(|u| u.dim).unwrap_or(DIMLESS);
        let tables = &mut self.module.tables;
        tables.pycalls.push(I::PyCallSite {
            module: r.module.clone(),
            func: attr,
            display,
            facs,
            ints,
            pnames,
            rlist,
            rfac: runit.as_ref().map(|u| u.factor).unwrap_or(1.0),
            declared: shape.is_some(),
        });
        let id = tables.pycalls.len() - 1;
        let line = e.span.line;
        let mut all = Vec::with_capacity(vals.len() + 1);
        all.push(ir(I::ExprKind::Const(id as f64), Ty::Num(DExpr::of(DIMLESS)), line));
        all.extend(vals);
        let ty = if rlist { Ty::List(DExpr::of(rdim)) } else { Ty::Num(DExpr::of(rdim)) };
        let mut out = ir(I::ExprKind::Builtin("pycall".into(), all), ty, line);
        if let Some(u) = &runit {
            if u.name != "1" && !u.name.is_empty() {
                out.hint = Some(hint_of(u));
            }
        }
        Ok(Checked::Val(out))
    }

    /// "divide by a unit, like  r / (1 m),  or declare the unit in the use line:  jv(x1, x2 [m])".
    fn py_unit_hint(&self, dim: &DExpr, node: &A::Expr, attr: &str, nargs: usize, k: usize) -> String {
        let d = self.u.resolve(dim);
        let u = if d.is_dimensionless() { "m".to_string() } else { fermium_units::display::preferred_unit(&d).name };
        let src = match &node.kind {
            A::ExprKind::Name { name } => name.clone(),
            _ => "…".to_string(),
        };
        let mut params: Vec<String> = (0..nargs).map(|i| format!("x{}", i + 1)).collect();
        params[k] = format!("x{} [{u}]", k + 1);
        format!("divide by a unit, like  {src} / (1 {u}),  or declare the unit in the use line:  {attr}({})",
                params.join(", "))
    }
}
