//! Lists of complex numbers (D243), what `fft(xs)` returns: the checker's rules from `fermium/clist.py`, and
//! the older FFT built-ins (fft_re, fft_im, ifft(re, im), amplitude_spectrum, power_spectrum, frequencies,
//! argmax, argmin: Checker.m3_fourier in `fermium/checker.py`, D81).
use fermium_ir as I;
use fermium_ir::types::{DExpr, Ty};
use fermium_syntax::ast as A;

use crate::checker::*;
use crate::stmts::ty_dim;
use crate::units;
use crate::vecmat::{arg_span, builtin_ir, hint_power};

fn dim(v: &I::Expr) -> DExpr {
    ty_dim(&v.ty).unwrap_or_else(DExpr::dimless)
}

fn mk(name: &str, args: Vec<I::Expr>, ty: Ty, hint: Option<I::Hint>, line: u32) -> I::Expr {
    let mut r = builtin_ir(name, args, ty, line);
    r.hint = hint;
    r.sf = None;
    r
}

fn affine(h: &Option<I::Hint>) -> bool {
    h.as_ref().is_some_and(|h| h.offset != 0.0)
}

impl Checker {
    /// zs[k] of a list of complex numbers: a complex number (clist.index).
    pub fn clist_index(&mut self, t: I::Expr, idx: I::Expr, e: &A::Expr) -> CResult<I::Expr> {
        let (d, hint) = (dim(&t), t.hint.clone());
        let mut r = builtin_ir("cl.get", vec![t, idx], Ty::Complex(d), e.span.line);
        r.hint = hint;
        r.sf = None;
        Ok(r)
    }

    /// fft(xs), ifft(X), complex(re_list, im_list), len/re/im/abs/arg/conj of a complex list.
    pub fn clist_call(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let n = args.len();
        let line = e.span.line;
        let need = |c: &Checker, k: usize, usage: &str| -> CResult<()> {
            if n != k {
                return Err(c.err(format!("{name} takes {k} argument{}: {usage}", if k != 1 { "s" } else { "" }), e.span,
                                 None));
            }
            Ok(())
        };
        if name == "fft" || name == "ifft" {
            need(self, 1, &format!("{name}(xs), with xs a list of numbers or of complex numbers"))?;
            let a = &args[0];
            if !matches!(a.ty, Ty::List(_) | Ty::ComplexList(_)) {
                return Err(self.err(format!("{name}(xs): xs must be a list, not {}", self.type_desc(&a.ty)),
                                    arg_span(e, 0), None));
            }
            let h = if affine(&a.hint) { None } else { a.hint.clone() };
            let d = dim(a);
            return Ok(mk(&format!("cl.{name}"), args, Ty::ComplexList(d), h, line));
        }
        if name == "complex" {
            need(self, 2, "complex(re, im) with two lists of the same length and units")?;
            for (i, a) in args.iter().enumerate() {
                if !matches!(a.ty, Ty::List(_)) {
                    return Err(self.err("complex(re, im) takes two real numbers, or two lists of real numbers",
                                        arg_span(e, i), None));
                }
            }
            let (d0, d1) = (dim(&args[0]), dim(&args[1]));
            if !self.u.unify(&d0, &d1) {
                return Err(self.err(format!("complex(re, im) needs both parts in the same units, but they are {} and {}",
                                            self.desc(&d0), self.desc(&d1)), e.span, None));
            }
            let h = args[0].hint.clone().or_else(|| args[1].hint.clone());
            return Ok(mk("cl.make", args, Ty::ComplexList(d0), h, line));
        }
        if name == "len" {
            need(self, 1, "len(X)")?;
            return Ok(mk("len", args, Ty::Num(DExpr::dimless()), None, line));
        }
        if ["re", "im", "abs", "arg", "conj"].contains(&name) {
            need(self, 1, &format!("{name}(X)"))?;
            let a = &args[0];
            let (d, h) = (dim(a), a.hint.clone());
            return Ok(match name {
                "arg" => mk("cl.arg", args, Ty::List(DExpr::dimless()), None, line),
                "conj" => mk("cl.conj", args, Ty::ComplexList(d), h, line),
                _ => mk(&format!("cl.{name}"), args, Ty::List(d), h, line),
            });
        }
        Err(self.err(format!("{name} doesn't work on a list of complex numbers"), e.span,
                     Some("a list of complex numbers works with X[k], len, for z in X, print, fft, ifft, re, im, abs, arg \
                           and conj; take one element X[k] for other operations".into())))
    }

    /// fft_re, fft_im, ifft(re, im), amplitude_spectrum, power_spectrum, frequencies, argmax, argmin (D81).
    pub fn m3_fourier(&mut self, name: &str, args: Vec<I::Expr>, e: &A::Expr) -> CResult<I::Expr> {
        let n = args.len();
        let line = e.span.line;
        let usage = match name {
            "fft_re" => "fft_re(xs)",
            "fft_im" => "fft_im(xs)",
            "ifft" => "ifft(re, im)",
            "amplitude_spectrum" => "amplitude_spectrum(xs)",
            "power_spectrum" => "power_spectrum(xs, dt)",
            "frequencies" => "frequencies(xs, dt) or frequencies(n, dt)",
            "argmax" => "argmax(xs)",
            _ => "argmin(xs)",
        };
        let want = if matches!(name, "ifft" | "power_spectrum" | "frequencies") { 2 } else { 1 };
        if n != want {
            return Err(self.err(format!("{name} takes {want} argument{}: {usage}", if want > 1 { "s" } else { "" }),
                                e.span, None));
        }
        if matches!(name, "fft_re" | "fft_im" | "ifft") {
            // from before complex numbers (D81); one more version (D243)
            let new = match name {
                "fft_re" => "re(fft(xs))",
                "fft_im" => "im(fft(xs))",
                _ => "ifft(complex(re, im))",
            };
            let d = fermium_syntax::diag::Diagnostic::warning(
                format!("{usage} is deprecated and will be removed after Fermium 1.5: write {new}"), e.span.line,
                e.span.col, 1,
                Some("fft(xs) gives the transform as a list of complex numbers; ifft(X) transforms one back".into()));
            self.diags.warn(d);
        }
        let need_list = |c: &Checker, i: usize| -> CResult<()> {
            if !matches!(args[i].ty, Ty::List(_)) {
                return Err(c.err(format!("{usage}: {} must be a list, not {}", if i == 0 { "xs" } else { "the second argument" },
                                         c.type_desc(&args[i].ty)), arg_span(e, i), None));
            }
            Ok(())
        };
        let arg_node = |i: usize| crate::vecmat::arg_nodes(e)[i].clone();
        if name == "argmax" || name == "argmin" {
            need_list(self, 0)?;
            return Ok(mk(name, args, Ty::Num(DExpr::dimless()), None, line));
        }
        if name == "frequencies" {
            self.need_num(&args[1], &arg_node(1), "the time step (spacing) between samples")?;
            let mut args = args;
            let dt = args.pop().unwrap();
            let a0 = args.pop().unwrap();
            let cnt = if matches!(a0.ty, Ty::List(_)) {
                builtin_ir("len", vec![a0], Ty::Num(DExpr::dimless()), line)
            } else {
                self.need_num(&a0, &arg_node(0), "the number of samples")?;
                let d0 = dim(&a0);
                if !self.u.unify(&d0, &DExpr::dimless()) {
                    return Err(self.err("frequencies(n, dt): n must be a plain number", arg_span(e, 0), None));
                }
                a0
            };
            let d = DExpr::dimless().div(&dim(&dt));
            let hint = if self.u.is_concrete(&d) && self.u.resolve(&d) == fermium_ir::DIMLESS / time_dim() {
                units::lookup_unit("Hz").map(|u| crate::exprs::hint_of(&u)) // shown in Hz, not 1/s
            } else {
                None
            };
            return Ok(mk("frequencies", vec![cnt, dt], Ty::List(d), hint, line));
        }
        need_list(self, 0)?;
        let d0 = dim(&args[0]);
        if name == "ifft" {
            need_list(self, 1)?;
            let d1 = dim(&args[1]);
            if !self.u.unify(&d0, &d1) {
                return Err(self.err("ifft(re, im): the real and imaginary parts need the same units", e.span, None));
            }
            return Ok(mk(name, args, Ty::List(d0), None, line));
        }
        if name == "power_spectrum" {
            self.need_num(&args[1], &arg_node(1), "the time step (spacing) between samples")?;
            let d1 = dim(&args[1]);
            let rd = d0.mul(&d0).mul(&d1);
            let hp = hint_power(&args[0].hint, 2); // V -> V²/Hz, the usual unit of a spectral density
            let mut hint = None;
            if let Some(hp) = hp {
                if self.u.is_concrete(&d1) && self.u.resolve(&d1) == time_dim() {
                    if let Ok(u) = units::parse_unit_string(&format!("{}/Hz", hp.name)) {
                        if self.u.is_concrete(&rd) && u.dim == self.u.resolve(&rd) {
                            hint = Some(crate::exprs::hint_of(&u));
                        }
                    }
                }
            }
            return Ok(mk(name, args, Ty::List(rd), hint, line));
        }
        let h = if affine(&args[0].hint) { None } else { args[0].hint.clone() };
        Ok(mk(name, args, Ty::List(d0), h, line))
    }
}

/// The dimension of time.
pub fn time_dim() -> fermium_ir::Dim {
    units::lookup_unit("s").unwrap().dim
}
