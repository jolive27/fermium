//! The standard printer: formats values with fermium-units exactly as the v1 runtime does
//! (fermium/runtime/core.py print_* callbacks) and writes one line per print statement.
use std::io::Write;
use std::rc::Rc;

use fermium_ir::{Fmt, Module};
use fermium_units::quantity::{format_clist, format_complex, format_list, format_mat, format_mvec, format_value,
                              format_vec, PrintFmt};
use fermium_units::Unit;

use crate::eval::Printer;

pub struct StdPrinter<W: Write> {
    fmts: Vec<PrintFmt>,
    line: Vec<String>,
    pub out: W,
}

fn print_fmt(f: &Fmt) -> PrintFmt {
    PrintFmt {
        rdim: f.dim,
        hint: f.hint.as_ref().map(|h| Unit { name: h.name.clone(), dim: h.dim, factor: h.factor, offset: h.offset }),
        sf: f.sf.map(|x| x as i64),
        direct: f.direct,
        echo: f.echo,
    }
}

impl<W: Write> StdPrinter<W> {
    pub fn new(module: &Module, out: W) -> Self {
        StdPrinter { fmts: module.tables.fmts.iter().map(print_fmt).collect(), line: vec![], out }
    }
}

impl<W: Write> Printer for StdPrinter<W> {
    fn num(&mut self, fmt: usize, v: f64) {
        let s = format_value(v, &self.fmts[fmt]);
        self.line.push(s);
    }
    fn list(&mut self, fmt: usize, v: &[f64]) {
        let s = format_list(v, &self.fmts[fmt]);
        self.line.push(s);
    }
    fn vec(&mut self, fmt: usize, v: &[f64]) {
        let s = format_vec(v, &self.fmts[fmt]);
        self.line.push(s);
    }
    fn mixed_vec(&mut self, fmts: &[usize], v: &[f64]) {
        let fs: Vec<PrintFmt> = fmts.iter().map(|&i| self.fmts[i].clone()).collect();
        self.line.push(format_mvec(v, &fs));
    }
    fn mat(&mut self, fmt: usize, v: &[f64], r: usize, c: usize) {
        let s = format_mat(v, r, c, &self.fmts[fmt]);
        self.line.push(s);
    }
    fn complex(&mut self, fmt: usize, re: f64, im: f64) {
        let f = &self.fmts[fmt];
        let s = format_complex(re, im, &f.rdim, f.hint.as_ref(), f.sf, f.direct);
        self.line.push(s);
    }
    fn clist(&mut self, fmt: usize, v: &[(f64, f64)]) {
        let f = &self.fmts[fmt];
        let s = format_clist(v, &f.rdim, f.hint.as_ref(), f.sf, f.direct, Some(v.len()));
        self.line.push(s);
    }
    fn boolean(&mut self, b: bool) {
        self.line.push(if b { "true" } else { "false" }.into());
    }
    fn text(&mut self, s: &str) {
        self.line.push(s.to_string());
    }
    fn textlist(&mut self, v: &[Rc<str>]) {
        self.line.push(format!("[{}]", v.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(", ")));
    }
    fn end(&mut self) {
        let _ = writeln!(self.out, "{}", self.line.join(" "));
        self.line.clear();
    }
}
