//! The evaluator, data: `load`, `table(…)`, columns, `fit` and `plot` (v1's compiled path: e_ILoad, e_ITable,
//! e_IColumn, s_SFit, s_SPlot in codegen_llvm.py, and Runtime.load / table / fit / make_plot and m3rt.animate in
//! fermium/runtime), over fermium-runtime's `data` and `plot` modules. The checker has already chosen every
//! display unit (module.tables.loads / fits / plots, written by fermium-check's data.rs).
use std::cell::RefCell;
use std::rc::Rc;

use fermium_ir::serde_like::Json;
use fermium_ir::{Expr, ExprKind, Stmt, StmtKind};
use fermium_runtime::data::{self as rtdata, ColUnit, Dataset, FitInfo, ParamInfo};
use fermium_runtime::plot::{self, PdePlot, PlotSpec, Series, Style};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

/// The data sets made so far (a Value::Handle of a data-table type indexes here).
#[derive(Default)]
pub struct DataState {
    pub(crate) sets: Vec<Rc<Dataset>>,
}

fn get<'a>(j: &'a Json, k: &str) -> &'a Json {
    static NULL: Json = Json::Null;
    match j {
        Json::Obj(kv) => kv.iter().find(|(n, _)| n == k).map(|(_, v)| v).unwrap_or(&NULL),
        _ => &NULL,
    }
}

fn num(j: &Json) -> f64 {
    match j {
        Json::Num(x) => *x,
        _ => f64::NAN,
    }
}

fn text(j: &Json) -> String {
    match j {
        Json::Str(s) => s.clone(),
        _ => String::new(),
    }
}

fn opt_text(j: &Json) -> Option<String> {
    match j {
        Json::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn flag(j: &Json) -> bool {
    matches!(j, Json::Bool(true))
}

fn list(j: &Json) -> &[Json] {
    match j {
        Json::List(v) => v,
        _ => &[],
    }
}

/// A display unit: (name, factor, offset).
fn unit(j: &Json) -> (String, f64, f64) {
    (text(get(j, "name")), num(get(j, "factor")), num(get(j, "offset")))
}

fn lim(j: &Json, u: &(String, f64, f64)) -> Option<(f64, f64)> {
    match list(j) {
        [a, b] => Some(((num(a) - u.2) / u.1, (num(b) - u.2) / u.1)),
        _ => None,
    }
}

impl<'m, P: Printer> Interpreter<'m, P> {
    pub(crate) fn builtin_data(&mut self, _name: &str, _args: &[Value]) -> Option<Result<Value, RunError>> {
        None
    }

    fn dataset(&self, v: &Value) -> Rc<Dataset> {
        match v {
            Value::Handle(h) => self.data.sets.get(*h).cloned().unwrap_or_default(),
            _ => Rc::new(Dataset::default()),
        }
    }

    fn new_set(&mut self, d: Dataset) -> Value {
        self.data.sets.push(Rc::new(d));
        Value::Handle(self.data.sets.len() - 1)
    }

    fn print_line(&mut self, s: &str) {
        self.printer.text(s);
        self.printer.end();
    }

    pub(crate) fn eval_data(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::Load(i) => {
                let info = &self.module.tables.loads[*i];
                let units: Vec<ColUnit> = list(get(info, "units")).iter().map(|u| {
                    let (_, factor, offset) = unit(u);
                    ColUnit { factor, offset }
                }).collect();
                match rtdata::load(&text(get(info, "full")), &text(get(info, "path")), &units) {
                    Ok(d) => Ok(self.new_set(d)),
                    Err(m) => self.err(m),
                }
            }
            ExprKind::Table(items) => {
                let mut cols = vec![];
                for it in items {
                    match self.eval(it, fr)? {
                        Value::List(l) => cols.push(l.borrow().clone()),
                        _ => cols.push(vec![]),
                    }
                }
                match rtdata::table(cols) {
                    Ok(d) => Ok(self.new_set(d)),
                    Err(m) => self.err(m),
                }
            }
            ExprKind::Column(d, k) => {
                let d = self.eval(d, fr)?;
                let ds = self.dataset(&d);
                let col = ds.cols.get(*k).cloned().unwrap_or_default();
                Ok(Value::List(Rc::new(RefCell::new(col))))
            }
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }

    /// fit … to data (s_SFit + Runtime.fit): the model's residuals row by row, v1's report, the parameters and
    /// their standard errors (NaN where there is none) into their variables.
    pub(crate) fn stmt_fit(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Fit { fit_id, data, params, guesses, model, errs } = &s.kind else { unreachable!() };
        let module = self.module;
        let info = &module.tables.fits[*fit_id];
        let mut guess = vec![];
        for g in guesses {
            let v = self.eval(g, fr)?.num();
            guess.push(if v.is_finite() { Some(v) } else { None });
        }
        let d = self.eval(data, fr)?;
        let ds = self.dataset(&d);
        self.line = s.line;
        let cols: Vec<usize> = list(get(info, "cols")).iter().map(|c| num(c) as usize).collect();
        let n = ds.len();
        let lam = &module.lambdas[*model];
        let mut failure: Option<RunError> = None;
        let mut resid = |p: &[f64], out: &mut [f64]| {
            if failure.is_some() {
                out.fill(f64::NAN);
                return;
            }
            for (sym, v) in lam.param_syms.iter().zip(p) {
                fr.vars.insert(*sym, Value::Num(*v));
            }
            for row in 0..n {
                for (sym, c) in lam.col_syms.iter().zip(&cols) {
                    fr.vars.insert(*sym, Value::Num(ds.cols[*c][row]));
                }
                match self.eval(&lam.body[0], fr) {
                    Ok(Value::Unc(_)) => {
                        failure = Some(RunError {
                            message: "a fit model can't use uncertain values (±) other than the parameters being \
                                      fitted; write value(x) in the model".into(),
                            line: self.line,
                            hint: None,
                        });
                        out.fill(f64::NAN);
                        return;
                    }
                    Ok(v) => out[row] = v.num(),
                    Err(ex) => {
                        failure = Some(ex);
                        out.fill(f64::NAN);
                        return;
                    }
                }
            }
        };
        let fi = FitInfo {
            text: text(get(info, "text")),
            path: text(get(info, "path")),
            params: list(get(info, "params")).iter().map(|p| {
                let (unit, factor, offset) = unit(get(p, "unit"));
                ParamInfo { name: text(get(p, "name")), unit, factor, offset }
            }).collect(),
            y_unit: unit(get(info, "yunit")).0,
            y_factor: unit(get(info, "yunit")).1,
        };
        let r = rtdata::run_fit(&mut resid, n, &guess, &fi);
        for sym in lam.param_syms.iter().chain(&lam.col_syms) {
            fr.vars.remove(sym);
        }
        if let Some(ex) = failure {
            return Err(ex);
        }
        self.line = s.line;
        let out = match r {
            Ok(o) => o,
            Err(m) => return self.err(m),
        };
        for l in &out.lines {
            self.print_line(l);
        }
        let k = params.len();
        let vals: Vec<Value> = if module.uses_uncertainty {
            // the fitted parameters carry their standard errors and correlations (s_SFit, D124)
            use fermium_runtime::numerics::uncertain::{correlated, UFloat};
            let ps = &out.result.params[..k];
            let ok = out.errors_or_nan[..k].iter().all(|e| e.is_finite());
            let us = match &out.result.cov {
                Some(cov) if ok => correlated(ps, cov),
                _ => None,
            };
            match us {
                Some(us) => us.into_iter().map(|u| Value::Unc(Rc::new(u))).collect(),
                None => ps.iter().zip(&out.errors_or_nan).map(|(v, e)| {
                    if e.is_finite() { Value::Unc(Rc::new(UFloat::measured(*v, *e))) } else { Value::Num(*v) }
                }).collect(),
            }
        } else {
            out.result.params[..k].iter().map(|v| Value::Num(*v)).collect()
        };
        for (sym, v) in params.iter().zip(vals) {
            self.set(*sym, v, fr);
        }
        for (i, sym) in errs.iter().enumerate() {
            self.set(*sym, Value::Num(out.errors_or_nan[i]), fr);
        }
        Ok(())
    }

    /// The (x, y) samples of one plotted series, in SI.
    fn plot_samples(&mut self, sj: &Json, exprs: &[Expr], fr: &mut Frame) -> Result<(Vec<f64>, Vec<f64>), RunError> {
        let idx: Vec<usize> = list(get(sj, "exprs")).iter().map(|k| num(k) as usize).collect();
        let lst = |v: Value| match v {
            Value::List(l) => l.borrow().clone(),
            _ => vec![],
        };
        match text(get(sj, "kind")).as_str() {
            "lists" => {
                let y = lst(self.eval(&exprs[idx[0]], fr)?);
                let x = lst(self.eval(&exprs[idx[1]], fr)?);
                if x.len() != y.len() {
                    return self.err(format!("plot: the two lists have different lengths ({} and {} values)", y.len(),
                                            x.len()));
                }
                Ok((x, y))
            }
            "func" => {
                let npts = 400;
                let lo = self.eval(&exprs[idx[0]], fr)?.num();
                let hi = self.eval(&exprs[idx[1]], fr)?.num();
                let lam = num(get(sj, "lam")) as usize;
                let dx = (hi - lo) / (npts - 1) as f64;
                let (mut xs, mut ys) = (vec![], vec![]);
                for i in 0..npts {
                    let x = lo + i as f64 * dx;
                    xs.push(x);
                    ys.push(self.call_scalar_lambda(lam, x, fr)?);
                }
                Ok((xs, ys))
            }
            _ => {
                let h = match self.eval(&exprs[idx[0]], fr)? {
                    Value::Handle(h) => h,
                    _ => return self.err("plot: this solution has no values yet"),
                };
                let sd = self.solve.sols[h].clone();
                let sol = &sd.sol;
                let (comp, dy) = (num(get(sj, "comp")) as usize, flag(get(sj, "dy")));
                let (ts, ys) = plot::sample_solution(&sol.t, &sol.y, &sol.dy, sol.dim, comp, dy);
                let xs = match get(sj, "comp2") {
                    Json::Num(c2) => {
                        plot::sample_solution(&sol.t, &sol.y, &sol.dy, sol.dim, *c2 as usize, flag(get(sj, "dy2"))).1
                    }
                    _ => ts,
                };
                Ok((xs, ys))
            }
        }
    }

    /// plot … (s_SPlot + Runtime.make_plot): the series in display units, the axis labels, the options.
    pub(crate) fn stmt_plot(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Plot(pid, exprs) = &s.kind else { unreachable!() };
        let module = self.module;
        let info = &module.tables.plots[*pid];
        let sjs = list(get(info, "series"));
        let mut series = vec![];
        let mut ylabels = vec![];
        for sj in sjs {
            let (xs, ys) = self.plot_samples(sj, exprs, fr)?;
            let (yu, xu) = (unit(get(sj, "yunit")), unit(get(sj, "xunit")));
            let x: Vec<f64> = xs.iter().map(|v| (v - xu.2) / xu.1).collect();
            let y: Vec<f64> = ys.iter().map(|v| (v - yu.2) / yu.1).collect();
            let yl = text(get(sj, "ylabel"));
            let ylabel = plot::axis_label(&yl, &yu.0, opt_text(get(info, "ylabel")).as_deref());
            let xlabel = plot::axis_label(&text(get(sj, "xlabel")), &xu.0, opt_text(get(info, "xlabel")).as_deref());
            let formula = plot::is_formula_label(&yl);
            ylabels.push((ylabel.clone(), formula));
            series.push(Series { x, y, style: if flag(get(sj, "points")) { Style::Points } else { Style::Line },
                                 legend: yl, xlabel, ylabel, y_is_formula: formula });
        }
        let (yu0, xu0) = sjs.first().map(|s| (unit(get(s, "yunit")), unit(get(s, "xunit"))))
            .unwrap_or_else(|| ((String::new(), 1.0, 0.0), (String::new(), 1.0, 0.0)));
        let spec = PlotSpec {
            path: text(get(info, "full")),
            series,
            title: opt_text(get(info, "title")),
            logx: flag(get(info, "logx")),
            logy: flag(get(info, "logy")),
            xlim: lim(get(info, "xlim"), &xu0),
            ylim: lim(get(info, "ylim"), &yu0),
            revx: flag(get(info, "revx")),
            revy: flag(get(info, "revy")),
            equal_aspect: flag(get(info, "equal")),
        };
        let _ = ylabels;
        match plot::save_plot(&spec) {
            Ok(line) => self.print_line(&line),
            Err(ex) => self.print_line(&format!("(plot not saved: {ex})")),
        }
        Ok(())
    }

    /// plot u vs x [animate over t] of a PDE solution (m3rt.animate): six times in one image, or an animation.
    pub(crate) fn stmt_animate(&mut self, s: &Stmt, fr: &mut Frame) -> Result<(), RunError> {
        let StmtKind::Animate { anim_id, sol, .. } = &s.kind else { unreachable!() };
        let module = self.module;
        let anims: Vec<&Json> = module.tables.plots.iter().filter(|p| flag(get(p, "anim"))).collect();
        let info = anims[*anim_id];
        let h = match self.get(*sol, fr)? {
            Value::Handle(h) => h,
            _ => return self.err("animate: the solution has no values yet"),
        };
        let xa = self.get(num(get(info, "xa")) as usize, fr)?.num();
        let xb = self.get(num(get(info, "xb")) as usize, fr)?.num();
        let sd = self.solve.sols[h].clone();
        let sol = &sd.sol;
        let (m, ncomp) = (num(get(info, "m")) as usize, num(get(info, "ncomp")) as usize);
        let (xu, tu, uu) = (unit(get(info, "xunit")), unit(get(info, "tunit")), unit(get(info, "uunit")));
        let xs: Vec<f64> = fermium_runtime::numerics::eigen::linspace(xa, xb, m + 1).iter()
            .map(|v| (v - xu.2) / xu.1).collect();
        let name = text(get(info, "name"));
        let w = ncomp * (m + 1);
        let rows: Vec<Vec<f64>> = (0..sol.n()).map(|k| {
            let r = &sol.y[k * sol.dim..k * sol.dim + w.min(sol.dim)];
            let vals: Vec<f64> = if ncomp == 2 {
                (0..=m).map(|j| r[j].powi(2) + r[m + 1 + j].powi(2)).collect()
            } else {
                r[..m + 1].to_vec()
            };
            vals.iter().map(|v| (v - uu.2) / uu.1).collect()
        }).collect();
        let mut ylabel = if ncomp == 2 { format!("|{name}|²") } else { name.clone() };
        if uu.0 != "" && uu.0 != "1" {
            ylabel += &format!(" [{}]", uu.0);
        }
        let xname = text(get(info, "xname"));
        let xlabel = if xu.0 != "" && xu.0 != "1" { format!("{xname} [{}]", xu.0) } else { xname };
        let tname = text(get(info, "tname"));
        let labels = sol.t.iter().map(|t| {
            let v = (t - tu.2) / tu.1;
            let unit = if tu.0 != "" && tu.0 != "1" { format!(" {}", tu.0) } else { String::new() };
            format!("{tname} = {}{unit}", fermium_units::format_number(v, 4, true))
        }).collect();
        let p = PdePlot {
            path: text(get(info, "full")), xs, labels, rows, xlabel, ylabel, title: opt_text(get(info, "title")),
            animate: if flag(get(info, "animate")) { Some(num(get(info, "frames")) as usize) } else { None },
        };
        self.line = s.line;
        match plot::save_pde_plot(&p) {
            Ok(line) => self.print_line(&line),
            Err(ex) => return self.err(format!("the animation failed: {ex}")),
        }
        Ok(())
    }
}
