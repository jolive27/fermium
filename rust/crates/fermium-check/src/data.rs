//! Data: `load "file.csv"` (read_csv_header), `table(x = xs, …)`, columns of a data set (`data.T`), `fit … to
//! data` (with err(g)), and `plot` (with its options and `plot u vs x animate over t`). A port of Checker.e_Load,
//! e_Table, read_csv_header, the data branch of e_Field, err(x) in e_Call, s_Fit / s_Plot and solve.py's
//! check_fit, check_plot, _plot_options, _plot_series, _finish_plot, _axis_name and m3solve.check_animate.
//!
//! The run-time tables (module.tables.loads / fits / plots) are written when checking ends (resolve_data_tables),
//! once every dimension is known, with the display units already chosen (v1 did that at run time with
//! display_unit on the resolved dimensions; the result is the same).
use fermium_ir as I;
use fermium_ir::serde_like::Json;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;

use crate::arith::minsf;
use crate::checker::*;
use crate::stmts::ty_dim;
use crate::units::{self, Unit};

/// A data set's description (Python DataTy.info): where it came from and its columns with their units.
#[derive(Clone, Debug)]
pub struct DataInfo {
    pub path: String,
    pub columns: Vec<(String, Unit)>,
    pub table: bool,
}

#[derive(Clone, Debug)]
pub struct PlotSeriesInfo {
    pub ylabel: String,
    pub xlabel: String,
    /// "lists", "func", "sol", "solxy"
    pub kind: &'static str,
    pub ydim: DExpr,
    pub xdim: DExpr,
    pub yhint: Option<I::Hint>,
    pub xhint: Option<I::Hint>,
    pub points: bool,
    /// indexes into the Plot statement's expressions: lists [y, x]; func [lo, hi]; sol / solxy [the solution]
    pub exprs: Vec<usize>,
    pub lam: Option<I::LambdaId>,
    pub comp: usize,
    pub dy: bool,
    pub comp2: Option<usize>,
    pub dy2: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PlotOptions {
    pub logx: bool,
    pub logy: bool,
    pub revx: bool,
    pub revy: bool,
    pub title: Option<String>,
    pub xlabel: Option<String>,
    pub ylabel: Option<String>,
    pub xlim: Option<(f64, f64)>,
    pub ylim: Option<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct PlotInfo {
    pub full: String,
    pub options: PlotOptions,
    pub series: Vec<PlotSeriesInfo>,
}

#[derive(Clone, Debug)]
pub struct FitTableInfo {
    pub params: Vec<String>,
    pub dims: Vec<DExpr>,
    pub text: String,
    pub ydim: DExpr,
    pub path: String,
    pub columns: Vec<(String, Unit)>,
    pub cols: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct AnimInfo {
    pub full: String,
    pub animate: bool,
    pub frames: usize,
    pub m: usize,
    pub ncomp: usize,
    pub name: String,
    pub xname: String,
    pub tname: String,
    pub udim: DExpr,
    pub xdim: DExpr,
    pub tdim: DExpr,
    pub uhint: Option<I::Hint>,
    pub xhint: Option<I::Hint>,
    pub thint: Option<I::Hint>,
    pub title: Option<String>,
    pub sol: I::SymId,
    pub xa: I::SymId,
    pub xb: I::SymId,
}

/// The checker's data tables.
#[derive(Clone, Debug, Default)]
pub struct DataTables {
    pub infos: Vec<DataInfo>,
    pub plots: Vec<PlotInfo>,
    pub fits: Vec<FitTableInfo>,
    pub anims: Vec<AnimInfo>,
    /// the load statements' entries, in order: (path as written, absolute path, the column units)
    pub loads: Vec<(String, String, Vec<Unit>)>,
}

fn hint_unit(h: &I::Hint) -> Unit {
    Unit { name: h.name.clone(), dim: h.dim, factor: h.factor, offset: h.offset }
}

fn unit_json(u: &Unit) -> Json {
    Json::Obj(vec![("name".into(), Json::Str(u.name.clone())), ("factor".into(), Json::Num(u.factor)),
                   ("offset".into(), Json::Num(u.offset))])
}

fn opt_str(s: &Option<String>) -> Json {
    match s {
        Some(t) => Json::Str(t.clone()),
        None => Json::Null,
    }
}

fn lim_json(l: &Option<(f64, f64)>) -> Json {
    match l {
        Some((a, b)) => Json::List(vec![Json::Num(*a), Json::Num(*b)]),
        None => Json::Null,
    }
}

fn abspath(p: &str) -> String {
    let path = std::path::Path::new(p);
    let full = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    // os.path.abspath normalises . and ..
    let mut out = std::path::PathBuf::new();
    for c in full.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

fn join(base: &str, p: &str) -> String {
    if std::path::Path::new(p).is_absolute() || base.is_empty() {
        p.to_string()
    } else {
        std::path::Path::new(base).join(p).to_string_lossy().into_owned()
    }
}

fn basename(p: &str) -> String {
    std::path::Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

impl Checker {
    pub fn data_info(&self, t: &Ty) -> Option<&DataInfo> {
        match t {
            Ty::Data(i) => self.data.infos.get(*i),
            _ => None,
        }
    }

    /// Read column names and units from a CSV header like  L [m], T [s]  (Python read_csv_header).
    pub fn read_csv_header(&self, full: &str) -> CResult<Vec<(String, Unit)>> {
        let cols = fermium_runtime_header(full).map_err(|m| self.err(m, A::Span::default(), None))?;
        let mut out = vec![];
        for c in cols {
            let unit = match &c.unit {
                None => Unit::new("1", fermium_ir::DIMLESS, 1.0),
                Some(ut) => units::parse_unit_string(ut).map_err(|ex| {
                    self.err(format!("in {}, column '{}': {ex}", basename(full), c.header), A::Span::default(), None)
                })?,
            };
            out.push((crate::names::canonical_name(&c.name), unit));
        }
        Ok(out)
    }

    pub fn e_load(&mut self, e: &A::Expr, path: &str) -> CResult<I::Expr> {
        if self.natural() {
            return Err(self.err(format!("data files can't be loaded inside a {} region (their columns are in SI units)",
                                        self.nat_label()), e.span,
                                Some("load the file before the  units  line; its columns then convert into natural \
                                      units when you use them".into())));
        }
        let full = join(&self.opts.base_dir, path);
        if !std::path::Path::new(&full).exists() {
            let dir = std::path::Path::new(&full).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            let dir = if dir.is_empty() { "." } else { &dir };
            return Err(self.err(format!("can't find the file '{path}'"), e.span,
                                Some(format!("looked in {}", abspath(dir)))));
        }
        let cols = self.read_csv_header(&full)?;
        let absfull = abspath(&full);
        self.data.loads.push((path.to_string(), absfull, cols.iter().map(|(_, u)| u.clone()).collect()));
        let load_id = self.data.loads.len() - 1;
        self.data.infos.push(DataInfo { path: path.to_string(), columns: cols, table: false });
        let id = self.data.infos.len() - 1;
        Ok(ir(I::ExprKind::Load(load_id), Ty::Data(id), e.span.line))
    }

    /// table(x = xs, y = ys): lists of the same length as the named columns of a data set (D193).
    pub fn e_table(&mut self, e: &A::Expr, names: &[String], items: &[A::Expr], ctx: &mut Ctx) -> CResult<I::Expr> {
        if items.is_empty() {
            return Err(self.err("a table needs at least one column, like  table(x = xs, y = ys)", e.span, None));
        }
        let mut vals = vec![];
        let mut cols = vec![];
        for (nm, node) in names.iter().zip(items) {
            let v = self.expr(node, ctx)?;
            let Ty::List(d) = &v.ty else {
                return Err(self.err(format!("the column {nm} of a table must be a list of numbers, like  {nm} = [1, 2, 3] m"),
                                    node.span, None));
            };
            let d = self.u.norm(d);
            if !d.is_concrete() {
                return Err(self.err(format!("the units of the column {nm} aren't known here"), node.span,
                                    Some(format!("give the list its unit, like  {nm} = [1, 2, 3] m"))));
            }
            let u = match &v.hint {
                Some(h) if h.dim == d.konst && h.offset == 0.0 => hint_unit(h),
                _ if d.konst.is_dimensionless() => Unit::new("1", d.konst, 1.0),
                _ => fermium_units::preferred_unit(&d.konst),
            };
            cols.push((nm.clone(), u));
            vals.push(v);
        }
        self.data.infos.push(DataInfo { path: crate::source::to_source(e), columns: cols, table: true });
        let id = self.data.infos.len() - 1;
        Ok(ir(I::ExprKind::Table(vals), Ty::Data(id), e.span.line))
    }

    /// data.T: a column of a data set (the data branch of Python e_Field); other fields of other things are errors.
    pub fn field_other(&mut self, e: &A::Expr, _target: &A::Expr, name: &str, t: Checked, _ctx: &mut Ctx)
                       -> CResult<Checked> {
        if let Checked::Val(v) = &t {
            if let Some(info) = self.data_info(&v.ty) {
                for (i, (cn, u)) in info.columns.iter().enumerate() {
                    if cn == name {
                        let hint = if u.name != "1" { Some(crate::exprs::hint_of(u)) } else { None };
                        let mut r = ir(I::ExprKind::Column(Box::new(v.clone()), i), Ty::List(DExpr::of(u.dim)), e.span.line);
                        r.hint = hint;
                        return Ok(Checked::Val(r));
                    }
                }
                let names = info.columns.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                return Err(self.err(format!("the data has no column called {name} (columns: {names})"), e.span, None));
            }
        }
        Err(self.err(format!("'.{name}' only works on vectors (v.x) and data loaded from a file (data.{name})"), e.span,
                     None))
    }

    /// print data: "data from pendulum.csv: columns L [m], T [s]".
    pub fn data_description(&self, v: &I::Expr) -> String {
        let Some(info) = self.data_info(&v.ty) else { return "data".into() };
        let cols = info.columns.iter()
            .map(|(n, u)| if u.name != "1" { format!("{n} [{}]", u.name) } else { n.clone() })
            .collect::<Vec<_>>()
            .join(", ");
        format!("data from {}: columns {cols}", info.path)
    }

    /// err(g): the standard error of a fitted parameter (or of any uncertain value, D121).
    pub fn err_call(&mut self, e: &A::Expr, args: &[A::Expr], ctx: &mut Ctx) -> CResult<Checked> {
        let a0 = if args.len() == 1 { Some(&args[0]) } else { None };
        let mut es = None;
        if let Some(A::Expr { kind: A::ExprKind::Name { name }, .. }) = a0 {
            if let Some((Binding::Sym(s), _)) = self.lookup(ctx.scope, name) {
                es = self.extra[s].err_sym;
            }
        }
        if es.is_none() && a0.is_some() && self.uses_unc {
            return self.builtin("uncertainty", e, ctx); // err(x) of any uncertain value (D121)
        }
        let Some(es) = es else {
            return Err(self.err("err(x) gives the standard error of a parameter found by fit, like err(g) after fit T \
                                 = 2π √(L/g) to data", e.span, None));
        };
        self.var_ref(es, ctx, e).map(Checked::Val)
    }

    // ============================================================ fit
    pub fn s_fit(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::Fit { model, data: data_ast, guesses: guess_asts } = &s.kind else { unreachable!() };
        if !ctx.is_main || ctx.lam.is_some() {
            return Err(self.err("fit can only be used at the top level of a program", s.span, None));
        }
        let data = self.expr(data_ast, ctx)?;
        let Some(info) = self.data_info(&data.ty).cloned() else {
            return Err(self.err("fit ... to <data>: the data must come from load \"file.csv\" or table(x = xs, y = ys)",
                                data_ast.span, None));
        };
        let cols = info.columns.clone();
        let colnames: Vec<&str> = cols.iter().map(|(n, _)| n.as_str()).collect();
        let (lhs, rhs) = (&model.lhs, &model.rhs);
        let lhs_names = crate::walk::free_names(lhs);
        if !lhs_names.iter().any(|n| colnames.contains(&n.as_str())) {
            return Err(self.err(format!("the left side of a fit must use a column of the data ({})", colnames.join(", ")),
                                lhs.span, None));
        }
        let mut used: Vec<String> = vec![];
        for n in crate::walk::free_names(rhs).into_iter()
            .chain(lhs_names.iter().filter(|x| !colnames.contains(&x.as_str())).cloned()) {
            if !used.contains(&n) {
                used.push(n);
            }
        }
        let guess_names: Vec<&str> = guess_asts.iter().map(|(g, _)| g.as_str()).collect();
        let (mut params, mut user_vars): (Vec<String>, Vec<String>) = (vec![], vec![]);
        for n in &used {
            if colnames.contains(&n.as_str()) {
                continue;
            }
            let b = self.lookup(ctx.scope, n).map(|(b, _)| b);
            match &b {
                Some(Binding::Const(_)) | Some(Binding::Func(_)) => continue,
                None if crate::builtins::is_builtin(n) => continue,
                Some(Binding::Sym(_)) if !guess_names.contains(&n.as_str()) => {
                    user_vars.push(n.clone());
                    continue;
                }
                _ => {}
            }
            params.push(n.clone());
        }
        if params.is_empty() && !user_vars.is_empty() {
            params = std::mem::take(&mut user_vars);
        }
        if params.is_empty() {
            return Err(self.err("this fit has no unknown parameters to adjust", s.span,
                                Some("parameters are the names in the model that aren't data columns or known values".into())));
        }
        // the model lambda: columns and parameters are its inputs
        let lname = self.fresh_name("model");
        self.module.lambdas.push(I::Lambda { kind: I::LambdaKind::Model, name: lname, params: vec![], captures: vec![],
                                             locals: vec![], body: vec![], state: vec![], col_syms: vec![],
                                             param_syms: vec![] });
        let lam = self.module.lambdas.len() - 1;
        let scope = self.new_scope(Some(ctx.scope), "block");
        let mut lctx = Ctx { func: ctx.func, scope, is_main: false, lam: Some(lam), loop_depth: 0, branch: 0,
                             ret_types: ctx.ret_types, regions: vec![], lam_parents: vec![] };
        let mut pdims = vec![];
        for n in &params {
            let d = match self.lookup(ctx.scope, n) {
                Some((Binding::Sym(b), _)) => match &self.module.syms[b].ty {
                    Ty::Num(d) => d.clone(),
                    _ => DExpr::fresh(),
                },
                _ => DExpr::fresh(),
            };
            pdims.push(d.clone());
            let sym = self.new_sym(n, Ty::Num(d), &lctx);
            self.module.lambdas[lam].locals.retain(|x| *x != sym);
            self.module.lambdas[lam].param_syms.push(sym);
            self.extra[sym].assigned = true;
            self.bind(scope, n, Binding::Sym(sym));
        }
        let mut col_idx = vec![];
        for (i, (cn, u)) in cols.iter().enumerate() {
            if used.contains(cn) || lhs_names.contains(cn) {
                let sym = self.new_sym(cn, Ty::Num(DExpr::of(u.dim)), &lctx);
                self.module.lambdas[lam].locals.retain(|x| *x != sym);
                self.module.lambdas[lam].col_syms.push(sym);
                self.extra[sym].assigned = true;
                self.bind(scope, cn, Binding::Sym(sym));
                col_idx.push(i);
            }
        }
        let body = self.expr(rhs, &mut lctx)?;
        self.need_num(&body, rhs, "the model")?;
        let lv = self.expr(lhs, &mut lctx)?;
        self.need_num(&lv, lhs, "the left side of the fit")?;
        let ydim = ty_dim(&lv.ty).unwrap();
        let bd = ty_dim(&body.ty).unwrap();
        let lname = crate::source::to_source(lhs);
        if !self.u.unify(&bd, &ydim) {
            return Err(self.err(format!("the model gives {} but {lname} is {}", self.desc(&bd), self.desc(&ydim)),
                                model.span,
                                Some("check the formula: both sides of the fit equation need the same units".into())));
        }
        // residual = model − left side; the fit drives it to zero
        let res = ir(I::ExprKind::Bin(I::BinOp::Sub, Box::new(body), Box::new(lv)), Ty::Num(ydim.clone()), s.span.line);
        self.module.lambdas[lam].body = vec![res];
        // initial guesses (NaN: none)
        let mut guesses = vec![];
        for (k, n) in params.iter().enumerate() {
            let mut g = None;
            for (gn, gv) in guess_asts {
                if gn == n {
                    let v = self.expr(gv, ctx)?;
                    self.need_num(&v, gv, "a starting guess")?;
                    let vd = ty_dim(&v.ty).unwrap();
                    if !self.u.unify(&vd, &pdims[k]) {
                        return Err(self.err(format!("the starting guess for {n} is {} but {n} must be {}", self.desc(&vd),
                                                    self.desc(&pdims[k])), gv.span, None));
                    }
                    g = Some(v);
                }
            }
            if g.is_none() {
                if let Some((Binding::Sym(b), _)) = self.lookup(ctx.scope, n) {
                    if matches!(self.module.syms[b].ty, Ty::Num(_)) {
                        let node = crate::ast_ext::name(n, s.span);
                        g = Some(self.var_ref(b, ctx, &node)?);
                    }
                }
            }
            guesses.push(g.unwrap_or_else(|| ir(I::ExprKind::Const(f64::NAN), dimless_num(), s.span.line)));
        }
        // the result variables
        let mut out_syms = vec![];
        for (k, n) in params.iter().enumerate() {
            let reuse = match self.lookup(ctx.scope, n) {
                Some((Binding::Sym(b), _)) if matches!(self.module.syms[b].ty, Ty::Num(_))
                    && (self.module.syms[b].func == self.owner_func(ctx)
                        || self.module.syms[b].storage == I::Storage::Arena) => Some(b),
                _ => None,
            };
            let sym = match reuse {
                Some(b) => b,
                None => {
                    let sym = self.new_sym(n, Ty::Num(pdims[k].clone()), ctx);
                    self.bind(ctx.scope, n, Binding::Sym(sym));
                    sym
                }
            };
            self.extra[sym].assigned = true;
            self.module.syms[sym].sf = Some(3);
            self.module.syms[sym].direct = 0;
            let d = self.u.norm(&pdims[k]);
            if d.is_concrete() {
                // show it in the data's unit if a column has the same dimension (τ in min)
                for (_, u) in &cols {
                    if u.dim == d.konst && u.name != "1" && self.module.syms[sym].hint.is_none() {
                        self.module.syms[sym].hint = Some(crate::exprs::hint_of(u));
                    }
                }
            }
            out_syms.push(sym);
        }
        let fit_id = self.data.fits.len();
        self.data.fits.push(FitTableInfo {
            params: params.clone(), dims: pdims.clone(),
            text: format!("{} = {}", crate::source::to_source(lhs), crate::source::to_source(rhs)), ydim,
            path: info.path.clone(), columns: cols.clone(), cols: col_idx,
        });
        // standard errors, for err(x) (gauntlet friction #26): hidden variables written by the fit
        let mut err_syms = vec![];
        for (k, (n, &sym)) in params.iter().zip(&out_syms).enumerate() {
            let es = match self.extra[sym].err_sym {
                Some(es) => es,
                None => {
                    let nm = format!("__err_{n}");
                    let es = self.new_sym(&nm, Ty::Num(pdims[k].clone()), ctx);
                    self.bind(ctx.scope, &nm, Binding::Sym(es));
                    self.extra[sym].err_sym = Some(es);
                    es
                }
            };
            self.extra[es].assigned = true;
            let h = self.module.syms[sym].hint.clone();
            let ms = &mut self.module.syms[es];
            ms.hint = h;
            ms.sf = Some(2);
            ms.direct = 0;
            err_syms.push(es);
        }
        Ok(vec![I::Stmt { kind: I::StmtKind::Fit { fit_id, data, params: out_syms, guesses, model: lam, errs: err_syms },
                          line: s.span.line }])
    }

    // ============================================================ plot
    /// What an axis and the legend call a plotted thing: its source, with a data set's name dropped before its
    /// columns (`T`, not `data.T`) (spec A6.4, D253).
    fn axis_name(&self, node: &A::Expr, ctx: &Ctx) -> String {
        let mut src = crate::source::to_source(node);
        let mut names: Vec<String> = vec![];
        for n in node.walk() {
            if let A::ExprKind::Field { target, .. } = &n.kind {
                if let A::ExprKind::Name { name } = &target.kind {
                    if let Some((Binding::Sym(b), _)) = self.lookup(ctx.scope, name) {
                        if matches!(self.module.syms[b].ty, Ty::Data(_)) && !names.contains(name) {
                            names.push(name.clone());
                        }
                    }
                }
            }
        }
        for nm in names {
            // re.sub(rf"(?<![\w.]){nm}\.(?=\w)", "", src)
            let pat = format!("{nm}.");
            let chars: Vec<char> = src.chars().collect();
            let pc: Vec<char> = pat.chars().collect();
            let mut out = String::new();
            let mut i = 0;
            while i < chars.len() {
                let m = i + pc.len() <= chars.len() && chars[i..i + pc.len()] == pc[..]
                    && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '.'))
                    && chars.get(i + pc.len()).is_some_and(|c| c.is_alphanumeric() || *c == '_');
                if m {
                    i += pc.len();
                } else {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            src = out;
        }
        src
    }

    pub fn s_plot(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::Plot { series: series_ast, out, options } = &s.kind else { unreachable!() };
        let opt = |k: &str| options.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v);
        let plots_pde = series_ast.iter().any(|sr| matches!(&sr.y.kind, A::ExprKind::Name { name } if self.is_pde_name(name, ctx)));
        if opt("animate").is_some() || plots_pde {
            return self.check_animate(s, ctx);
        }
        let mut exprs: Vec<I::Expr> = vec![];
        let mut series: Vec<PlotSeriesInfo> = vec![];
        for sr in series_ast {
            let (mut yunit, mut xunit) = (None, None);
            let mut sr = sr.clone();
            if let A::ExprKind::Convert { value, unit } = &sr.y.kind {
                yunit = Some(self.resolve_unit(unit)?);
                sr.y = (**value).clone();
            }
            if let A::ExprKind::Convert { value, unit } = &sr.x.kind {
                xunit = Some(self.resolve_unit(unit)?);
                sr.x = (**value).clone();
            }
            let mut entry = self.plot_series(&sr, ctx, s, &mut exprs)?;
            for (which, u) in [("y", &yunit), ("x", &xunit)] {
                if let Some(u) = u {
                    let d = if which == "y" { entry.ydim.clone() } else { entry.xdim.clone() };
                    if !self.u.unify(&d, &DExpr::of(u.dim)) {
                        return Err(self.err(format!("can't show {} in {}", self.desc(&d), u.name), sr.span, None));
                    }
                    let h = Some(crate::exprs::hint_of(u));
                    if which == "y" { entry.yhint = h } else { entry.xhint = h }
                }
            }
            if let Some(first) = series.first() {
                // one pair of axes: every series in the same units (A23)
                for which in ["x", "y"] {
                    let (a, b) = if which == "x" { (first.xdim.clone(), entry.xdim.clone()) } else { (first.ydim.clone(), entry.ydim.clone()) };
                    self.unify_or(&a, &b, |c| format!("all series in one plot need the same {which} units (here {} and {})",
                                                     c.desc(&a), c.desc(&b)), sr.span, None)?;
                }
            }
            series.push(entry);
        }
        // _finish_plot
        let out = match out {
            Some(o) => o.clone(),
            None => {
                let clean = |t: &str| {
                    let c: String = t.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
                    let c = c.trim_matches('_').to_string();
                    if c.is_empty() { "plot".to_string() } else { c }
                };
                let first = &series_ast[0];
                format!("{}_vs_{}.png", clean(&crate::source::to_source(&first.y)), clean(&crate::source::to_source(&first.x)))
            }
        };
        let full = join(&self.opts.base_dir, &out);
        let options = self.plot_options(options, &series, ctx)?;
        self.data.plots.push(PlotInfo { full, options, series });
        let pid = self.data.plots.len() - 1;
        Ok(vec![I::Stmt { kind: I::StmtKind::Plot(pid, exprs), line: s.span.line }])
    }

    fn plot_options(&mut self, options: &[(String, A::PlotOpt)], series: &[PlotSeriesInfo], ctx: &mut Ctx)
                    -> CResult<PlotOptions> {
        let get = |k: &str| options.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v);
        let flag = |k: &str| matches!(get(k), Some(A::PlotOpt::Bool(true)));
        let text = |k: &str| match get(k) {
            Some(A::PlotOpt::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let mut o = PlotOptions { logx: flag("logx"), logy: flag("logy"), revx: flag("revx"), revy: flag("revy"),
                                  title: text("title"), xlabel: text("xlabel"), ylabel: text("ylabel"), xlim: None,
                                  ylim: None };
        for which in ["x", "y"] {
            let Some(A::PlotOpt::Range(lo, hi)) = get(&format!("{which}range")) else { continue };
            let mut vals = vec![];
            for node in [&**lo, &**hi] {
                let v = self.expr(node, ctx)?;
                self.need_num(&v, node, &format!("the {which} range"))?;
                let I::ExprKind::Const(c) = v.kind else {
                    return Err(self.err(format!("the {which} range must be constants, like  {which} from 1e-12 to 1  or  \
                                                 {which} from 0 s to 10 s"), node.span, None));
                };
                let want = if which == "x" { series[0].xdim.clone() } else { series[0].ydim.clone() };
                let vd = ty_dim(&v.ty).unwrap();
                self.unify_or(&vd, &want, |c| format!("the {which} axis is {}, but this end of its range is {}", c.desc(&want),
                                                     c.desc(&vd)), node.span, None)?;
                vals.push(c);
            }
            if !(vals[0] < vals[1]) {
                return Err(self.err(format!("the {which} range must go from the smaller value to the larger one; to have \
                                             {which} decrease along the axis, add  reversed {which}"), lo.span, None));
            }
            if (if which == "x" { o.logx } else { o.logy }) && vals[0] <= 0.0 {
                return Err(self.err(format!("a log {which} axis can't start at 0 or below"), lo.span, None));
            }
            if which == "x" { o.xlim = Some((vals[0], vals[1])) } else { o.ylim = Some((vals[0], vals[1])) }
        }
        Ok(o)
    }

    fn plot_series(&mut self, sr: &A::PlotSeries, ctx: &mut Ctx, s: &A::Stmt, exprs: &mut Vec<I::Expr>)
                   -> CResult<PlotSeriesInfo> {
        let x_name = match &sr.x.kind {
            A::ExprKind::Name { name } => Some(name.clone()),
            _ => None,
        };
        let (yv, xv): (Option<Checked>, Option<Checked>);
        if sr.lo.is_some() && x_name.is_some() {
            let mut y = None;
            if let A::ExprKind::Name { name } = &sr.y.kind {
                if let Some((Binding::Func(info), _)) = self.lookup(ctx.scope, name) {
                    let dn = self.funcs[info].display_name.clone();
                    y = Some(Checked::Func { info, name: dn, param: false });
                }
            }
            yv = y;
            xv = None;
        } else {
            yv = Some(self.expr_any(&sr.y, ctx)?);
            xv = match &x_name {
                Some(n) => {
                    let b = self.lookup(ctx.scope, n).map(|(b, _)| b);
                    if b.is_none() || (matches!(b, Some(Binding::Const(_))) && n != "π") {
                        None
                    } else {
                        Some(self.expr_any(&sr.x, ctx)?)
                    }
                }
                None => Some(self.expr_any(&sr.x, ctx)?),
            };
        }
        let mut entry = PlotSeriesInfo { ylabel: self.axis_name(&sr.y, ctx), xlabel: self.axis_name(&sr.x, ctx),
                                         kind: "lists", ydim: DExpr::dimless(), xdim: DExpr::dimless(), yhint: None,
                                         xhint: None, points: false, exprs: vec![], lam: None, comp: 0, dy: false,
                                         comp2: None, dy2: false };
        for (side, node) in [(&yv, &sr.y), (&xv, &sr.x)] {
            if let Some(Checked::Sol(v)) = side {
                if self.sols[*v].n > 1 {
                    let nm = self.sols[*v].name.clone();
                    return Err(self.err(format!("{nm} is a vector; plot its components, e.g.  plot {nm}.y vs {nm}.x"),
                                        node.span, None));
                }
            }
        }
        let line = s.span.line;
        match (&yv, &xv) {
            (Some(Checked::Sol(vy)), None) => {
                let v = self.sols[*vy].clone();
                if x_name.as_deref() != Some(v.tname.as_str()) {
                    return Err(self.err(format!("{} is a function of {}; plot it  vs {}", v.name, v.tname, v.tname),
                                        sr.x.span, None));
                }
                let sol = self.var_ref(v.sol_sym, ctx, &sr.y)?;
                let (comp, dy) = if v.comp <= v.top { (v.comp, false) } else { (v.top, true) };
                entry.kind = "sol";
                entry.comp = comp;
                entry.dy = dy;
                entry.ydim = v.dim.clone();
                entry.xdim = v.tdim.clone();
                entry.yhint = v.hint.clone();
                entry.xhint = v.thint.clone();
                entry.exprs = vec![exprs.len()];
                exprs.push(sol);
                if sr.lo.is_some() {
                    return Err(self.err("a solution is plotted over the range it was solved for (no 'from ... to' needed)",
                                        sr.span, None));
                }
            }
            (Some(Checked::Sol(va)), Some(Checked::Sol(vb))) => {
                let (a, b) = (self.sols[*va].clone(), self.sols[*vb].clone());
                if a.sol_sym != b.sol_sym {
                    return Err(self.err("can only plot two solutions against each other if they come from the same solve",
                                        sr.span, None));
                }
                let sol = self.var_ref(a.sol_sym, ctx, &sr.y)?;
                entry.kind = "solxy";
                entry.comp = if a.comp <= a.top { a.comp } else { a.top };
                entry.dy = a.comp > a.top;
                entry.comp2 = Some(if b.comp <= b.top { b.comp } else { b.top });
                entry.dy2 = b.comp > b.top;
                entry.ydim = a.dim.clone();
                entry.xdim = b.dim.clone();
                entry.yhint = a.hint.clone();
                entry.xhint = b.hint.clone();
                entry.exprs = vec![exprs.len()];
                exprs.push(sol);
            }
            _ if xv.is_none() || sr.lo.is_some() => {
                // y is a formula in the (undefined) x variable, sampled over a range
                let (Some(lo_a), Some(hi_a)) = (&sr.lo, &sr.hi) else {
                    let xs = crate::source::to_source(&sr.x);
                    return Err(self.err(format!("{xs} isn't defined; to plot a formula give a range, like plot y vs {xs} \
                                                 from 0 to 10"), sr.x.span, None));
                };
                let Some(xn) = x_name.clone() else {
                    return Err(self.err("to plot a formula, the thing after 'vs' must be a variable name", sr.x.span, None));
                };
                let lo = self.expr(lo_a, ctx)?;
                let hi = self.expr(hi_a, ctx)?;
                self.need_num(&lo, lo_a, "this value")?;
                self.need_num(&hi, hi_a, "this value")?;
                let (ld, hd) = (ty_dim(&lo.ty).unwrap(), ty_dim(&hi.ty).unwrap());
                self.unify_or(&ld, &hd, |_| "the two ends of the plot range need the same units".into(), sr.span, None)?;
                let (lam, mut lctx, _) = self.scalar_lambda("plotfn", &xn, ld.clone(), ctx);
                let yexpr = if matches!(yv, Some(Checked::Func { .. })) {
                    crate::ast_ext::mk(A::ExprKind::Call { func: Box::new(sr.y.clone()),
                                                           args: vec![crate::ast_ext::name(&xn, sr.y.span)] }, sr.y.span)
                } else {
                    sr.y.clone()
                };
                let body = self.expr(&yexpr, &mut lctx)?;
                self.need_num(&body, &sr.y, "the thing to plot")?;
                entry.kind = "func";
                entry.ydim = ty_dim(&body.ty).unwrap();
                entry.xdim = ld;
                entry.yhint = body.hint.clone();
                entry.xhint = lo.hint.clone().or_else(|| hi.hint.clone());
                self.module.lambdas[lam].body = vec![body];
                entry.lam = Some(lam);
                entry.exprs = vec![exprs.len(), exprs.len() + 1];
                exprs.push(lo);
                exprs.push(hi);
            }
            _ => {
                let mut sides = vec![];
                for (side, node) in [(yv.clone().unwrap(), &sr.y), (xv.clone().unwrap(), &sr.x)] {
                    let v = match side {
                        Checked::Sol(view) => Checked::Val(self.sol_values(view, node)?),
                        other => other,
                    };
                    match v {
                        Checked::Val(v) if matches!(v.ty, Ty::List(_)) => sides.push(v),
                        other => {
                            let what = if matches!(other, Checked::Func { .. }) { "a function" } else { "a single value" };
                            return Err(self.err(format!("can't plot {what} here; plot needs lists of values (or a solution, \
                                                         or a formula with a range)"), node.span,
                                                Some("e.g.  plot v vs t from 0 s to 5 s   or   plot ys vs xs".into())));
                        }
                    }
                }
                let (y, x) = (sides.remove(0), sides.remove(0));
                entry.kind = "lists";
                entry.ydim = ty_dim(&y.ty).unwrap();
                entry.xdim = ty_dim(&x.ty).unwrap();
                entry.yhint = y.hint.clone();
                entry.xhint = x.hint.clone();
                let has_points = matches!(s.kind, A::StmtKind::Plot { ref options, .. }
                                          if options.iter().any(|(k, v)| k == "points" && matches!(v, A::PlotOpt::Bool(true))));
                entry.points = matches!(y.kind, I::ExprKind::Column(..)) || matches!(x.kind, I::ExprKind::Column(..))
                    || has_points;
                entry.exprs = vec![exprs.len(), exprs.len() + 1];
                exprs.push(y);
                exprs.push(x);
            }
        }
        let _ = line;
        Ok(entry)
    }

    /// plot u vs x animate over t [frames N] [to "file.gif"]: frames of u(x, t); without `animate`, one plot with the
    /// solution at 6 times (D83).
    fn check_animate(&mut self, s: &A::Stmt, ctx: &mut Ctx) -> CResult<Vec<I::Stmt>> {
        let A::StmtKind::Plot { series, out, options } = &s.kind else { unreachable!() };
        let get = |k: &str| options.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v);
        if series.len() != 1 {
            return Err(self.err("an animation shows one PDE solution:  plot u vs x animate over t", s.span, None));
        }
        let set = |v: Option<&A::PlotOpt>| match v {
            None => false,
            Some(A::PlotOpt::Bool(b)) => *b,
            Some(A::PlotOpt::Str(t)) => !t.is_empty(),
            Some(_) => true,
        };
        if ["xrange", "yrange", "xlabel", "ylabel", "revx", "revy", "logx", "logy", "points"].iter().any(|k| set(get(k))) {
            return Err(self.err("a PDE solution's plot takes only  title  and  animate over t [frames N]  (not axis \
                                 ranges, labels, log or reversed axes)", s.span, None));
        }
        let sr = &series[0];
        let view = match &sr.y.kind {
            A::ExprKind::Name { name } => match self.lookup(ctx.scope, name) {
                Some((Binding::Pde(p), _)) => Some(self.solve.pdes[p].clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(view) = view else {
            return Err(self.err("animate over t works for the solution of a PDE:  plot u vs x animate over t", sr.y.span,
                                None));
        };
        if !matches!(&sr.x.kind, A::ExprKind::Name { name } if *name == view.xname) || sr.lo.is_some() {
            return Err(self.err(format!("plot a PDE solution against its space variable:  plot {} vs {}", view.name,
                                        view.xname), sr.x.span, None));
        }
        let anim = match get("animate") {
            Some(A::PlotOpt::Str(t)) => Some(t.clone()),
            _ => None,
        };
        if let Some(t) = &anim {
            if *t != view.tname {
                return Err(self.err(format!("{} changes with {}: write  animate over {}", view.name, view.tname,
                                            view.tname), s.span, None));
            }
        }
        let frames = match get("frames") {
            Some(A::PlotOpt::Num(f)) => *f as i64,
            _ => 60,
        };
        if !(2..=1000).contains(&frames) {
            return Err(self.err("frames must be from 2 to 1000", s.span, None));
        }
        let animate = anim.is_some();
        let out = out.clone().unwrap_or_else(|| {
            format!("{}_vs_{}.{}", view.name, view.xname, if animate { "gif" } else { "png" })
        });
        let full = join(&self.opts.base_dir, &out);
        let title = match get("title") {
            Some(A::PlotOpt::Str(t)) => Some(t.clone()),
            _ => None,
        };
        let node = &sr.y;
        let sol = self.var_ref(view.sol_sym, ctx, node)?;
        self.var_ref(view.xa_sym, ctx, node)?;
        self.var_ref(view.xb_sym, ctx, node)?;
        let I::ExprKind::Var(sol_sym) = sol.kind else { unreachable!() };
        self.data.anims.push(AnimInfo {
            full, animate, frames: frames as usize, m: view.m, ncomp: view.ncomp, name: view.name.clone(),
            xname: view.xname.clone(), tname: view.tname.clone(), udim: view.udim.clone(), xdim: view.xdim.clone(),
            tdim: view.tdim.clone(), uhint: view.uhint.clone(), xhint: view.xhint.clone(), thint: view.thint.clone(),
            title, sol: sol_sym, xa: view.xa_sym, xb: view.xb_sym,
        });
        let anim_id = self.data.anims.len() - 1;
        Ok(vec![I::Stmt { kind: I::StmtKind::Animate { anim_id, sol: sol_sym, xa: 0.0, xb: 0.0 }, line: s.span.line }])
    }

    // ============================================================ tables for the run time
    /// Write module.tables.loads / fits / plots (and the animations, as plots entries with "anim") now that every
    /// dimension is known (Python tables.finalize_tables), with the display units chosen.
    pub fn resolve_data_tables(&mut self) {
        let t = std::mem::take(&mut self.data);
        let du = |c: &Checker, d: &DExpr, h: &Option<I::Hint>| {
            let r = c.u.resolve(d);
            let hu = h.as_ref().map(hint_unit);
            fermium_units::display_unit(&r, hu.as_ref())
        };
        self.module.tables.loads = t.loads.iter().map(|(path, full, us)| {
            Json::Obj(vec![("path".into(), Json::Str(path.clone())), ("full".into(), Json::Str(full.clone())),
                           ("units".into(), Json::List(us.iter().map(unit_json).collect()))])
        }).collect();
        self.module.tables.fits = t.fits.iter().map(|f| {
            // col_units: the last column of each dimension, not plain numbers
            let col_unit = |d: &fermium_ir::Dim| f.columns.iter().rev().find(|(_, u)| u.name != "1" && u.dim == *d)
                .map(|(_, u)| u.clone());
            let params = f.params.iter().zip(&f.dims).map(|(n, d)| {
                let r = self.u.resolve(d);
                let u = fermium_units::display_unit(&r, col_unit(&r).as_ref());
                Json::Obj(vec![("name".into(), Json::Str(n.clone())), ("unit".into(), unit_json(&u))])
            }).collect();
            let ry = self.u.resolve(&f.ydim);
            let yu = fermium_units::display_unit(&ry, col_unit(&ry).as_ref());
            Json::Obj(vec![("text".into(), Json::Str(f.text.clone())), ("path".into(), Json::Str(f.path.clone())),
                           ("params".into(), Json::List(params)), ("yunit".into(), unit_json(&yu)),
                           ("cols".into(), Json::List(f.cols.iter().map(|c| Json::Num(*c as f64)).collect()))])
        }).collect();
        let mut plots: Vec<Json> = t.plots.iter().map(|p| {
            let equal = p.options.xlim.is_none() && p.options.ylim.is_none() && p.series.iter().all(|s| {
                let (rx, ry) = (self.u.resolve(&s.xdim), self.u.resolve(&s.ydim));
                s.kind == "solxy" || (rx == ry && !rx.is_dimensionless())
            });
            let series = p.series.iter().map(|s| {
                let yu = du(self, &s.ydim, &s.yhint);
                let xu = du(self, &s.xdim, &s.xhint);
                Json::Obj(vec![
                    ("kind".into(), Json::Str(s.kind.into())), ("ylabel".into(), Json::Str(s.ylabel.clone())),
                    ("xlabel".into(), Json::Str(s.xlabel.clone())), ("points".into(), Json::Bool(s.points)),
                    ("yunit".into(), unit_json(&yu)), ("xunit".into(), unit_json(&xu)),
                    ("exprs".into(), Json::List(s.exprs.iter().map(|k| Json::Num(*k as f64)).collect())),
                    ("lam".into(), s.lam.map(|l| Json::Num(l as f64)).unwrap_or(Json::Null)),
                    ("comp".into(), Json::Num(s.comp as f64)), ("dy".into(), Json::Bool(s.dy)),
                    ("comp2".into(), s.comp2.map(|c| Json::Num(c as f64)).unwrap_or(Json::Null)),
                    ("dy2".into(), Json::Bool(s.dy2)),
                ])
            }).collect();
            let o = &p.options;
            Json::Obj(vec![
                ("full".into(), Json::Str(p.full.clone())), ("series".into(), Json::List(series)),
                ("logx".into(), Json::Bool(o.logx)), ("logy".into(), Json::Bool(o.logy)),
                ("revx".into(), Json::Bool(o.revx)), ("revy".into(), Json::Bool(o.revy)),
                ("title".into(), opt_str(&o.title)), ("xlabel".into(), opt_str(&o.xlabel)),
                ("ylabel".into(), opt_str(&o.ylabel)), ("xlim".into(), lim_json(&o.xlim)),
                ("ylim".into(), lim_json(&o.ylim)), ("equal".into(), Json::Bool(equal)),
            ])
        }).collect();
        // animations follow the plots in the same table, marked "anim" (the Animate statement's id counts from 0)
        for a in &t.anims {
            let rx = self.u.resolve(&a.xdim);
            let ru = self.u.resolve(&a.udim);
            let rt = self.u.resolve(&a.tdim);
            let xu = fermium_units::display_unit(&rx, a.xhint.as_ref().map(hint_unit).as_ref());
            let tu = fermium_units::display_unit(&rt, a.thint.as_ref().map(hint_unit).as_ref());
            let uu = if a.ncomp == 2 {
                // a complex solution: the probability density |ψ|², per nm when x is in nm
                let d2 = ru * ru;
                match units::parse_unit_string(&format!("1/{}", xu.name)) {
                    Ok(per) if per.dim == d2 => per,
                    _ => fermium_units::display_unit(&d2, None),
                }
            } else {
                fermium_units::display_unit(&ru, a.uhint.as_ref().map(hint_unit).as_ref())
            };
            plots.push(Json::Obj(vec![
                ("anim".into(), Json::Bool(true)), ("full".into(), Json::Str(a.full.clone())),
                ("animate".into(), Json::Bool(a.animate)), ("frames".into(), Json::Num(a.frames as f64)),
                ("m".into(), Json::Num(a.m as f64)), ("ncomp".into(), Json::Num(a.ncomp as f64)),
                ("name".into(), Json::Str(a.name.clone())), ("xname".into(), Json::Str(a.xname.clone())),
                ("tname".into(), Json::Str(a.tname.clone())), ("xunit".into(), unit_json(&xu)),
                ("tunit".into(), unit_json(&tu)), ("uunit".into(), unit_json(&uu)), ("title".into(), opt_str(&a.title)),
                ("xa".into(), Json::Num(a.xa as f64)), ("xb".into(), Json::Num(a.xb as f64)),
            ]));
        }
        self.module.tables.plots = plots;
        self.data = t;
        let _ = minsf(&[]);
    }
}

/// fermium-runtime's header reader, without making fermium-check depend on the runtime crate: the same rules
/// (Python csv + read_csv_header): the first record, each field trimmed, `name [unit]`.
fn fermium_runtime_header(full: &str) -> Result<Vec<HeaderColC>, String> {
    let text = match std::fs::read(full) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return Err(format!("can't read {full}: {e}")),
    };
    let t = text.strip_prefix('\u{feff}').unwrap_or(&text);
    // the first CSV record (quotes may contain commas and newlines)
    let mut fields = vec![];
    let mut buf = String::new();
    let mut quoted = false;
    let mut chars = t.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    buf.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                buf.push(c);
            }
            continue;
        }
        match c {
            '"' if buf.is_empty() => quoted = true,
            ',' => fields.push(std::mem::take(&mut buf)),
            '\r' | '\n' => break,
            _ => buf.push(c),
        }
    }
    if !any {
        return Err(format!("the file {} is empty", basename(full)));
    }
    fields.push(buf);
    Ok(fields.into_iter().map(|h| {
        let h = h.trim().to_string();
        let (mut name, mut unit) = (h.clone(), None);
        if h.contains('[') && h.ends_with(']') {
            let (n, rest) = h.split_once('[').unwrap();
            name = n.trim().to_string();
            let ut = rest[..rest.len() - 1].trim();
            if !matches!(ut, "" | "1" | "-") {
                unit = Some(ut.to_string());
            }
        }
        HeaderColC { header: h, name: name.replace(' ', "_"), unit }
    }).collect())
}

struct HeaderColC {
    header: String,
    name: String,
    unit: Option<String>,
}
