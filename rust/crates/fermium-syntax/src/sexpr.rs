//! The tree, the tokens and the diagnostics as text, in exactly the format of `rust/tools/parse_oracle.py`
//! (which prints Fermium 1.5's Python AST), so the two parsers can be compared program by program.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer::{ExtraVal, TokValue, Token};
use crate::pyfmt::{json_str as q, repr_float};

/// A Python value as the oracle prints it.
enum PV {
    /// Already rendered (None, numbers, strings, references).
    S(String),
    Node(Box<NodeRepr>),
    List(Vec<PV>),
    Tuple(Vec<PV>),
    Dict(Vec<(String, PV)>),
}

struct NodeRepr {
    class: &'static str,
    span: Span,
    paren: bool,
    fields: Vec<(&'static str, PV)>,
    extras: Vec<(&'static str, PV)>,
}

fn none() -> PV {
    PV::S("None".into())
}

fn b(v: bool) -> PV {
    PV::S(if v { "True" } else { "False" }.into())
}

fn s(v: &str) -> PV {
    PV::S(q(v))
}

fn int(v: i64) -> PV {
    PV::S(v.to_string())
}

fn opt_s(v: &Option<String>) -> PV {
    v.as_ref().map(|x| s(x)).unwrap_or_else(none)
}

fn frac(r: &num_rational::Rational64) -> String {
    if *r.denom() == 1 {
        r.numer().to_string()
    } else {
        format!("{}/{}", r.numer(), r.denom())
    }
}

struct Printer {
    /// id -> (class, span) of every node in the tree, for references (the Python parser stores the object).
    live: HashMap<u32, (&'static str, Span)>,
}

impl Printer {
    fn noderef(&self, r: &NodeRef) -> String {
        let (class, sp) = self.live.get(&r.id).copied().unwrap_or((r.class, r.span));
        format!("<{}@{}:{}+{}>", class, sp.line, sp.col, sp.length)
    }

    fn opt_ref(&self, r: &Option<NodeRef>) -> String {
        r.as_ref().map(|x| self.noderef(x)).unwrap_or_else(|| "None".into())
    }

    fn expr(&self, e: &Expr) -> PV {
        use ExprKind::*;
        let mut f: Vec<(&'static str, PV)> = vec![];
        match &e.kind {
            Num { value, sigfigs, digit } => {
                f.push(("value", PV::S(repr_float(*value))));
                f.push(("sigfigs", sigfigs.map(|x| int(x as i64)).unwrap_or_else(none)));
                f.push(("digit", b(*digit)));
            }
            Quantity { value, unit, bracket } => {
                f.push(("value", self.expr(value)));
                f.push(("unit", self.unit(unit)));
                f.push(("bracket", b(*bracket)));
            }
            Str { value } => f.push(("value", s(value))),
            Bool { value } => f.push(("value", b(*value))),
            Name { name } => f.push(("name", s(name))),
            BinOp { op, left, right, implicit } => {
                f.push(("op", s(op)));
                f.push(("left", self.expr(left)));
                f.push(("right", self.expr(right)));
                f.push(("implicit", b(*implicit)));
            }
            Neg { operand } | Not { operand } | Abs { operand } => f.push(("operand", self.expr(operand))),
            Compare { op, left, right, tol } => {
                f.push(("op", s(op)));
                f.push(("left", self.expr(left)));
                f.push(("right", self.expr(right)));
                f.push(("tol", self.opt(tol.as_deref())));
            }
            Logic { op, left, right } => {
                f.push(("op", s(op)));
                f.push(("left", self.expr(left)));
                f.push(("right", self.expr(right)));
            }
            Call { func, args } => {
                f.push(("func", self.expr(func)));
                f.push(("args", PV::List(args.iter().map(|a| self.expr(a)).collect())));
            }
            Index { target, index } => {
                f.push(("target", self.expr(target)));
                f.push(("index", self.opt(index.as_deref())));
            }
            Slice { lo, hi } => {
                f.push(("lo", self.opt(lo.as_deref())));
                f.push(("hi", self.opt(hi.as_deref())));
            }
            End => {}
            Field { target, name } => {
                f.push(("target", self.expr(target)));
                f.push(("name", s(name)));
            }
            Prime { target, order } => {
                f.push(("target", self.expr(target)));
                f.push(("order", int(*order)));
            }
            Deriv { var, order, operand, partial } => {
                f.push(("var", s(var)));
                f.push(("order", int(*order)));
                f.push(("operand", self.expr(operand)));
                f.push(("partial", b(*partial)));
            }
            Integral { integrand, var, lo, hi } => {
                f.push(("integrand", self.expr(integrand)));
                f.push(("var", s(var)));
                f.push(("lo", self.opt(lo.as_deref())));
                f.push(("hi", self.opt(hi.as_deref())));
            }
            Sum { body, var, lo, hi, step } => {
                f.push(("body", self.expr(body)));
                f.push(("var", s(var)));
                f.push(("lo", self.expr(lo)));
                f.push(("hi", self.expr(hi)));
                f.push(("step", self.opt(step.as_deref())));
            }
            Sqrt { operand, root } => {
                f.push(("operand", self.expr(operand)));
                f.push(("root", int(*root)));
            }
            ListLit { items } | VecLit { items } => f.push(("items", self.list(items))),
            Table { names, items } => {
                f.push(("names", PV::List(names.iter().map(|n| s(n)).collect())));
                f.push(("items", self.list(items)));
            }
            VecCalc { kind, func } => {
                f.push(("kind", s(kind)));
                f.push(("func", self.expr(func)));
            }
            IfExpr { cond, then, other } => {
                f.push(("cond", self.expr(cond)));
                f.push(("then", self.expr(then)));
                f.push(("other", self.expr(other)));
            }
            Convert { value, unit } => {
                f.push(("value", self.expr(value)));
                f.push(("unit", self.unit(unit)));
            }
            Digits { value, digits } => {
                f.push(("value", self.expr(value)));
                f.push(("digits", int(*digits)));
            }
            Load { path } => f.push(("path", s(path))),
            Where { value, bindings } => {
                f.push(("value", self.expr(value)));
                f.push(("bindings", self.bindings(bindings)));
            }
            Uncertain { value, err } => {
                f.push(("value", self.expr(value)));
                f.push(("err", self.expr(err)));
            }
        }
        let a = &e.attrs;
        let mut x: Vec<(&'static str, PV)> = vec![];
        if let Some(v) = a.coefficient {
            x.push(("coefficient", b(v)));
        }
        if let Some(d) = &a.div_info {
            x.push(("div_info", PV::S(self.div_info(d))));
        }
        if let Some(v) = a.extra_int {
            x.push(("extra_int", int(v)));
        }
        if let Some(v) = a.imag_literal {
            x.push(("imag_literal", b(v)));
        }
        if let Some(v) = a.juxt_i {
            x.push(("juxt_i", int(v as i64)));
        }
        if let Some(v) = a.juxt_ws {
            x.push(("juxt_ws", b(v)));
        }
        if let Some(r) = &a.limit_div_of {
            x.push(("limit_div_of", PV::S(self.noderef(r))));
        }
        if let Some(r) = &a.raw {
            x.push(("raw", s(r)));
        }
        if let Some(si) = &a.sum_info {
            x.push(("sum_info", PV::S(self.sum_info(si.as_deref()))));
        }
        if let Some(v) = a.times_unit {
            x.push(("times_unit", b(v)));
        }
        if let Some(r) = &a.unit_left {
            x.push(("unit_left", PV::S(self.noderef(r))));
        }
        PV::Node(Box::new(NodeRepr { class: e.class(), span: e.span, paren: e.paren, fields: f, extras: x }))
    }

    fn div_info(&self, d: &DivInfo) -> String {
        let mut out: Vec<String> = vec![];
        // keys sorted: div_text divisor end factors hi_text op start tok warned
        if let Some(t) = &d.div_text {
            out.push(format!("div_text:{}", q(t)));
        }
        if let Some(r) = &d.divisor {
            out.push(format!("divisor:{}", self.noderef(r)));
        }
        if let Some(v) = d.end {
            out.push(format!("end:{v}"));
        }
        if let Some(fs) = &d.factors {
            let items: Vec<String> = fs
                .iter()
                .map(|f| format!("({} {} {})", self.noderef(&f.node), f.start, if f.ws { "True" } else { "False" }))
                .collect();
            out.push(format!("factors:[{}]", items.join(" ")));
        }
        if let Some(t) = &d.hi_text {
            out.push(format!("hi_text:{}", q(t)));
        }
        if let Some((l, c)) = d.op {
            out.push(format!("op:tok@{l}:{c}"));
        }
        if let Some(v) = d.start {
            out.push(format!("start:{v}"));
        }
        if let Some((l, c)) = d.tok {
            out.push(format!("tok:tok@{l}:{c}"));
        }
        if let Some(w) = d.warned {
            out.push(format!("warned:{}", if w { "True" } else { "False" }));
        }
        format!("{{{}}}", out.join(" "))
    }

    fn sum_info(&self, si: Option<&SumInfo>) -> String {
        let Some(si) = si else { return "None".into() };
        // keys sorted: head head_node limit op rest rest_node tok
        format!(
            "{{head:{} head_node:{} limit:{} op:{} rest:{} rest_node:{} tok:tok@{}:{}}}",
            q(&si.head),
            self.opt_ref(&si.head_node),
            q(&si.limit),
            q(&si.op),
            q(&si.rest),
            self.opt_ref(&si.rest_node),
            si.tok.0,
            si.tok.1
        )
    }

    fn opt(&self, e: Option<&Expr>) -> PV {
        e.map(|x| self.expr(x)).unwrap_or_else(none)
    }

    fn list(&self, items: &[Expr]) -> PV {
        PV::List(items.iter().map(|x| self.expr(x)).collect())
    }

    fn bindings(&self, bs: &[(String, Expr)]) -> PV {
        PV::List(bs.iter().map(|(n, e)| PV::Tuple(vec![s(n), self.expr(e)])).collect())
    }

    fn unit(&self, u: &UnitExpr) -> PV {
        let factors = u
            .factors
            .iter()
            .map(|f| {
                PV::Node(Box::new(NodeRepr {
                    class: "UnitFactor",
                    span: f.span,
                    paren: false,
                    fields: vec![("name", s(&f.name)), ("exp", PV::S(frac(&f.exp)))],
                    extras: vec![],
                }))
            })
            .collect();
        let mut extras = vec![];
        if let Some(j) = u.juxt_join {
            extras.push(("juxt_join", b(j)));
        }
        PV::Node(Box::new(NodeRepr {
            class: "UnitExpr",
            span: u.span,
            paren: false,
            fields: vec![("factors", PV::List(factors)), ("text", s(&u.text))],
            extras,
        }))
    }

    fn opt_unit(&self, u: &Option<UnitExpr>) -> PV {
        u.as_ref().map(|x| self.unit(x)).unwrap_or_else(none)
    }

    fn node(class: &'static str, span: Span, fields: Vec<(&'static str, PV)>) -> PV {
        PV::Node(Box::new(NodeRepr { class, span, paren: false, fields, extras: vec![] }))
    }

    fn param(&self, p: &Param) -> PV {
        Self::node("Param", p.span, vec![("name", s(&p.name)), ("unit", self.opt_unit(&p.unit))])
    }

    fn equation(&self, e: &Equation) -> PV {
        Self::node("Equation", e.span, vec![("lhs", self.expr(&e.lhs)), ("rhs", self.expr(&e.rhs))])
    }

    fn block(&self, b: &[Stmt]) -> PV {
        PV::List(b.iter().map(|x| self.stmt(x)).collect())
    }

    fn opt_block(&self, b: &Option<Vec<Stmt>>) -> PV {
        b.as_ref().map(|x| self.block(x)).unwrap_or_else(none)
    }

    fn stmt(&self, st: &Stmt) -> PV {
        use StmtKind::*;
        let mut f: Vec<(&'static str, PV)> = vec![];
        let mut x: Vec<(&'static str, PV)> = vec![];
        match &st.kind {
            Propagate { samples, body } => {
                f.push(("samples", self.opt(samples.as_ref())));
                f.push(("body", self.block(body)));
            }
            Assign { name, value, op } => {
                f.push(("name", s(name)));
                f.push(("value", self.expr(value)));
                f.push(("op", s(op)));
            }
            IndexAssign { target, index, value, op, index2, rest } => {
                f.push(("target", s(target)));
                f.push(("index", self.expr(index)));
                f.push(("value", self.expr(value)));
                f.push(("op", s(op)));
                f.push(("index2", self.opt(index2.as_ref())));
                if !rest.is_empty() {
                    f.push(("rest", PV::List(rest.iter().map(|e| self.expr(e)).collect())));
                }
            }
            FuncDef { name, params, body, where_ } => {
                f.push(("name", s(name)));
                f.push(("params", PV::List(params.iter().map(|p| self.param(p)).collect())));
                f.push((
                    "body",
                    match body {
                        FuncBody::Expr(e) => self.expr(e),
                        FuncBody::Block(b) => self.block(b),
                    },
                ));
                f.push(("where", self.bindings(where_)));
            }
            Print { items } => f.push(("items", self.list(items))),
            Plot { series, out, options } => {
                let ser = series
                    .iter()
                    .map(|p| {
                        Self::node(
                            "PlotSeries",
                            p.span,
                            vec![
                                ("y", self.expr(&p.y)),
                                ("x", self.expr(&p.x)),
                                ("lo", self.opt(p.lo.as_ref())),
                                ("hi", self.opt(p.hi.as_ref())),
                            ],
                        )
                    })
                    .collect();
                f.push(("series", PV::List(ser)));
                f.push(("out", opt_s(out)));
                let opts = options
                    .iter()
                    .map(|(k, v)| {
                        let pv = match v {
                            PlotOpt::Bool(v) => b(*v),
                            PlotOpt::Str(v) => s(v),
                            PlotOpt::Num(v) => PV::S(repr_float(*v)),
                            PlotOpt::Range(lo, hi) => PV::Tuple(vec![self.expr(lo), self.expr(hi)]),
                        };
                        (k.clone(), pv)
                    })
                    .collect();
                x.push(("options", PV::Dict(opts)));
            }
            Solve(sv) => {
                f.push(("equations", PV::List(sv.equations.iter().map(|e| self.equation(e)).collect())));
                f.push(("initial", PV::List(sv.initial.iter().map(|e| self.equation(e)).collect())));
                f.push(("var", s(&sv.var)));
                f.push(("lo", self.expr(&sv.lo)));
                f.push(("hi", self.expr(&sv.hi)));
                f.push(("step", self.opt(sv.step.as_ref())));
                f.push(("method", opt_s(&sv.method)));
                f.push(("tolerance", self.opt(sv.tolerance.as_ref())));
                f.push(("until", sv.until.as_ref().map(|e| self.equation(e)).unwrap_or_else(none)));
                f.push(("absolute", sv.absolute.as_ref().map(|v| self.list(v)).unwrap_or_else(none)));
                x.push(("grid", self.opt(sv.grid.as_ref())));
                x.push(("hi2", self.opt(sv.hi2.as_ref())));
                x.push(("lo2", self.opt(sv.lo2.as_ref())));
                x.push(("lowest", self.opt(sv.lowest.as_ref())));
                x.push(("step2", self.opt(sv.step2.as_ref())));
                x.push(("var2", opt_s(&sv.var2)));
            }
            Fit { model, data, guesses } => {
                f.push(("model", self.equation(model)));
                f.push(("data", self.expr(data)));
                f.push(("guesses", self.bindings(guesses)));
            }
            Analyze { title, target, inputs, raw } => {
                f.push(("title", opt_s(title)));
                f.push(("target", self.param(target)));
                f.push(("inputs", PV::List(inputs.iter().map(|p| self.param(p)).collect())));
                f.push(("raw", PV::Dict(raw.iter().map(|(k, v)| (k.clone(), s(v))).collect())));
            }
            If { cond, then, other } => {
                f.push(("cond", self.expr(cond)));
                f.push(("then", self.block(then)));
                f.push(("other", self.opt_block(other)));
            }
            For { var, lo, hi, step, body, parallel } => {
                f.push(("var", s(var)));
                f.push(("lo", self.expr(lo)));
                f.push(("hi", self.expr(hi)));
                f.push(("step", self.opt(step.as_ref())));
                f.push(("body", self.block(body)));
                f.push(("parallel", b(*parallel)));
            }
            ForIn { var, iterable, body } => {
                f.push(("var", s(var)));
                f.push(("iterable", self.expr(iterable)));
                f.push(("body", self.block(body)));
            }
            While { cond, body } => {
                f.push(("cond", self.expr(cond)));
                f.push(("body", self.block(body)));
            }
            Return { value } => f.push(("value", self.opt(value.as_ref()))),
            Break | Continue => {}
            ExprStmt { value } => f.push(("value", self.expr(value))),
            Assert { cond, message } => {
                f.push(("cond", self.expr(cond)));
                f.push(("message", opt_s(message)));
            }
            Units { system, consts, body } => {
                f.push(("system", s(system)));
                f.push(("consts", PV::List(consts.iter().map(|c| s(c)).collect())));
                f.push(("body", self.opt_block(body)));
            }
            Import { module, is_path, alias, names } => {
                f.push(("module", s(module)));
                f.push(("is_path", b(*is_path)));
                f.push(("alias", opt_s(alias)));
                f.push((
                    "names",
                    names
                        .as_ref()
                        .map(|ns| PV::List(ns.iter().map(|(n, a)| PV::Tuple(vec![s(n), opt_s(a)])).collect()))
                        .unwrap_or_else(none),
                ));
            }
            UsePython { module, alias, sigs } => {
                f.push(("module", s(module)));
                f.push(("alias", opt_s(alias)));
                let ss = sigs
                    .iter()
                    .map(|g| {
                        let ps = g
                            .params
                            .iter()
                            .map(|(n, u, i)| PV::Tuple(vec![s(n), self.opt_unit(u), b(*i)]))
                            .collect();
                        Self::node(
                            "PySig",
                            g.span,
                            vec![
                                ("name", s(&g.name)),
                                ("params", PV::List(ps)),
                                ("ret_shape", opt_s(&g.ret_shape)),
                                ("ret_unit", self.opt_unit(&g.ret_unit)),
                            ],
                        )
                    })
                    .collect();
                f.push(("sigs", PV::List(ss)));
            }
        }
        PV::Node(Box::new(NodeRepr { class: st.kind.class(), span: st.span, paren: false, fields: f, extras: x }))
    }
}

fn is_scalar(v: &PV) -> bool {
    matches!(v, PV::S(_))
}

fn render(v: &PV, ind: usize) -> String {
    match v {
        PV::S(x) => x.clone(),
        PV::Node(n) => render_node(n, ind),
        PV::List(items) => {
            if items.is_empty() {
                return "[]".into();
            }
            let mut out = String::from("[");
            for it in items {
                out.push('\n');
                out.push_str(&" ".repeat(ind + 2));
                out.push_str(&render(it, ind + 2));
            }
            out.push(']');
            out
        }
        PV::Tuple(items) => format!("({})", items.iter().map(|x| render(x, ind)).collect::<Vec<_>>().join(" ")),
        PV::Dict(items) => {
            format!("{{{}}}", items.iter().map(|(k, x)| format!("{k}:{}", render(x, ind))).collect::<Vec<_>>().join(" "))
        }
    }
}

fn render_node(n: &NodeRepr, ind: usize) -> String {
    let mut out = format!("({}@{}:{}+{}", n.class, n.span.line, n.span.col, n.span.length);
    if n.paren {
        out.push_str(" paren");
    }
    for (k, v) in &n.fields {
        let inline = is_scalar(v) || matches!(v, PV::Dict(items) if items.iter().all(|(_, x)| is_scalar(x)));
        if inline {
            out.push_str(&format!(" {k}={}", render(v, ind)));
        } else {
            out.push_str(&format!("\n{}{k}={}", " ".repeat(ind + 2), render(v, ind + 2)));
        }
    }
    for (k, v) in &n.extras {
        if is_scalar(v) {
            out.push_str(&format!(" #{k}={}", render(v, ind)));
        } else {
            out.push_str(&format!("\n{}#{k}={}", " ".repeat(ind + 2), render(v, ind + 2)));
        }
    }
    out.push(')');
    out
}

fn collect_live(p: &Program) -> HashMap<u32, (&'static str, Span)> {
    let mut live = HashMap::new();
    fn ex(e: &Expr, live: &mut HashMap<u32, (&'static str, Span)>) {
        live.entry(e.id).or_insert((e.class(), e.span));
        for c in e.children() {
            ex(c, live);
        }
        if let ExprKind::VecCalc { func, .. } = &e.kind {
            ex(func, live);
        }
    }
    fn eq(e: &Equation, live: &mut HashMap<u32, (&'static str, Span)>) {
        ex(&e.lhs, live);
        ex(&e.rhs, live);
    }
    fn st(s: &Stmt, live: &mut HashMap<u32, (&'static str, Span)>) {
        use StmtKind::*;
        match &s.kind {
            Propagate { samples, body } => {
                samples.iter().for_each(|e| ex(e, live));
                body.iter().for_each(|x| st(x, live));
            }
            Assign { value, .. } => ex(value, live),
            IndexAssign { index, value, index2, rest, .. } => {
                ex(index, live);
                ex(value, live);
                index2.iter().for_each(|e| ex(e, live));
                rest.iter().for_each(|e| ex(e, live));
            }
            FuncDef { body, where_, .. } => {
                match body {
                    FuncBody::Expr(e) => ex(e, live),
                    FuncBody::Block(b) => b.iter().for_each(|x| st(x, live)),
                }
                where_.iter().for_each(|(_, e)| ex(e, live));
            }
            Print { items } => items.iter().for_each(|e| ex(e, live)),
            Plot { series, options, .. } => {
                for p in series {
                    ex(&p.y, live);
                    ex(&p.x, live);
                    p.lo.iter().for_each(|e| ex(e, live));
                    p.hi.iter().for_each(|e| ex(e, live));
                }
                for (_, o) in options {
                    if let PlotOpt::Range(a, b) = o {
                        ex(a, live);
                        ex(b, live);
                    }
                }
            }
            Solve(sv) => {
                sv.equations.iter().for_each(|e| eq(e, live));
                sv.initial.iter().for_each(|e| eq(e, live));
                ex(&sv.lo, live);
                ex(&sv.hi, live);
                for e in [&sv.step, &sv.tolerance, &sv.lowest, &sv.grid, &sv.lo2, &sv.hi2, &sv.step2].into_iter().flatten() {
                    ex(e, live);
                }
                sv.until.iter().for_each(|e| eq(e, live));
                sv.absolute.iter().flatten().for_each(|e| ex(e, live));
            }
            Fit { model, data, guesses } => {
                eq(model, live);
                ex(data, live);
                guesses.iter().for_each(|(_, e)| ex(e, live));
            }
            If { cond, then, other } => {
                ex(cond, live);
                then.iter().for_each(|x| st(x, live));
                other.iter().flatten().for_each(|x| st(x, live));
            }
            For { lo, hi, step, body, .. } => {
                ex(lo, live);
                ex(hi, live);
                step.iter().for_each(|e| ex(e, live));
                body.iter().for_each(|x| st(x, live));
            }
            ForIn { iterable, body, .. } => {
                ex(iterable, live);
                body.iter().for_each(|x| st(x, live));
            }
            While { cond, body } => {
                ex(cond, live);
                body.iter().for_each(|x| st(x, live));
            }
            Return { value } => value.iter().for_each(|e| ex(e, live)),
            ExprStmt { value } => ex(value, live),
            Assert { cond, .. } => ex(cond, live),
            Units { body, .. } => body.iter().flatten().for_each(|x| st(x, live)),
            Analyze { .. } | Break | Continue | Import { .. } | UsePython { .. } => {}
        }
    }
    p.body.iter().for_each(|s| st(s, &mut live));
    live
}

/// The program as S-expressions (the oracle's TREE section).
pub fn program(p: &Program) -> String {
    let pr = Printer { live: collect_live(p) };
    let root = NodeRepr {
        class: "Program",
        span: Span { line: 0, col: 0, length: 1 },
        paren: false,
        fields: vec![("body", pr.block(&p.body))],
        extras: vec![],
    };
    render_node(&root, 0)
}

pub fn token(t: &Token) -> String {
    let vs = match &t.value {
        TokValue::None => "None".to_string(),
        TokValue::Num(x) => repr_float(*x),
        TokValue::Int(i) => i.to_string(),
        TokValue::Str(x) => q(x),
    };
    let mut extra: Vec<(String, String)> = t
        .extra
        .iter()
        .map(|(k, v)| {
            (k.clone(), match v {
                ExtraVal::Int(i) => i.to_string(),
                ExtraVal::Bool(x) => (if *x { "True" } else { "False" }).to_string(),
            })
        })
        .collect();
    extra.sort();
    let extra: Vec<String> = extra.into_iter().map(|(k, v)| format!("{k}:{v}")).collect();
    let mut out = format!(
        "{} {} {} {}:{} {}-{} ws={} sig={} digit={} role={} extra={{{}}}",
        t.kind.as_str(),
        vs,
        q(&t.raw),
        t.line,
        t.col,
        t.start,
        t.end,
        t.ws_before as u8,
        t.sigfigs.map(|x| x.to_string()).unwrap_or_else(|| "None".into()),
        t.digit as u8,
        q(&t.role),
        extra.join(" ")
    );
    if t.unknown_prime {
        out.push_str(" unknown_prime");
    }
    out
}

fn opt_num(v: Option<u32>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "None".into())
}

pub fn warning(w: &Diagnostic) -> String {
    let mut out = format!("W {}:{}+{} {}", opt_num(w.line), opt_num(w.col), w.length, q(&w.message));
    if let Some(h) = &w.hint {
        if !h.is_empty() {
            out.push_str(&format!(" hint={}", q(h)));
        }
    }
    out
}

pub fn error(e: &Diagnostic) -> String {
    let mut out = format!("ERROR {}:{}+{} {}", opt_num(e.line), opt_num(e.col), e.length.max(1), q(&e.message));
    if let Some(h) = &e.hint {
        if !h.is_empty() {
            out.push_str(&format!(" hint={}", q(h)));
        }
    }
    if !e.fix.is_empty() {
        let items: Vec<String> = e.fix.iter().map(|(a, b, r)| format!("({a} {b} {})", q(r))).collect();
        out.push_str(&format!(" fix=[{}]", items.join(" ")));
    }
    out
}
