//! What the interactive tools need from the checker (the REPL's `:vars`, the language server's hover and
//! completion, the Jupyter kernel): check a program and keep the checker, with its module, for questions about
//! the names it defined; and describe a type in physics words (lsp.py `_type_text`).
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;
use fermium_syntax::diag::Diagnostic;

use crate::checker::{Binding, CheckOptions, Checker};

/// Check a program and keep the checker for questions about its names. On success the checker's module is the
/// checked module (check_program takes it out; it is put back). On an error the checker is as the error left
/// it.
pub fn check_keep(prog: &A::Program, opts: CheckOptions) -> Result<Checker, (Diagnostic, Checker)> {
    let mut c = Checker::new(opts);
    match c.check_program(prog) {
        Ok(m) => {
            c.module = m;
            Ok(c)
        }
        Err(e) => Err((e, c)),
    }
}

impl Checker {
    /// The names defined at the top level (the global scope), sorted, with what they are bound to.
    pub fn global_names(&self) -> Vec<(String, Binding)> {
        let mut v: Vec<(String, Binding)> =
            self.scopes[self.globals].names.iter().map(|(k, b)| (k.clone(), b.clone())).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// What a top-level name is bound to.
    pub fn global(&self, name: &str) -> Option<Binding> {
        self.scopes[self.globals].names.get(name).cloned()
    }

    /// A module bound by `import` (index into the module table): its name, its short path, its exported names.
    pub fn module_names(&self, m: usize) -> Option<(String, String, Vec<String>)> {
        let info = self.mods.modules.get(m)?;
        Some((info.name.clone(), info.display.clone(), self.exported(m)))
    }

    /// What a name inside an imported module is bound to.
    pub fn module_binding(&self, m: usize, name: &str) -> Option<Binding> {
        let info = self.mods.modules.get(m)?;
        self.scopes[info.scope].names.get(name).cloned()
    }

    fn num_text(&self, d: &DExpr, hint: Option<&I::Hint>) -> String {
        let r = self.u.resolve(d);
        let mut s = if !r.is_dimensionless() { self.desc(d) } else { "a plain number (no units)".to_string() };
        let hname = hint.map(|h| h.name.as_str()).unwrap_or("");
        if !hname.is_empty() && hname != "1" && hname != fermium_units::preferred_unit(&r).name {
            s += &format!(", shown in {hname}");
        }
        s
    }

    /// A type in physics words: 'length [m], shown in cm', 'a list of energy [J]', 'a 3-D vector of …'
    /// (lsp.py `_type_text`, used by hover and `:vars`).
    pub fn type_text(&self, ty: &Ty, hint: Option<&I::Hint>) -> String {
        match ty {
            Ty::Num(d) => self.num_text(d, hint),
            Ty::List(d) => format!("a list of {}", self.num_text(d, hint).replace("a plain number", "plain numbers")),
            Ty::Complex(d) => format!("a complex number of {}",
                                      self.num_text(d, hint).replace("a plain number (no units)", "plain numbers")),
            Ty::Vec { n, dims: Some(ds), .. } => {
                let parts: Vec<String> = ds.iter().map(|d| self.desc(d)).collect();
                format!("a {n}-D vector of ({})", parts.join(", "))
            }
            Ty::Vec { n, dim: Some(d), .. } => {
                format!("a {n}-D vector of {}", self.num_text(d, hint).replace("a plain number", "plain numbers"))
            }
            Ty::Vec { n, .. } => format!("a {n}-D vector"),
            Ty::Mat { r, c, dim } => {
                format!("a {r}×{c} matrix of {}", self.num_text(dim, hint).replace("a plain number", "plain numbers"))
            }
            Ty::TextList => "a list of text".into(),
            Ty::ComplexList(d) => format!("a list of complex numbers of {}",
                                          self.num_text(d, hint).replace("a plain number (no units)", "plain numbers")),
            Ty::Str => "text".into(),
            Ty::Bool => "true or false".into(),
            Ty::Sol(_) => "the solution of an ODE".into(),
            Ty::Data(_) => "a data table".into(),
            Ty::Void => "void".into(),
        }
    }
}
