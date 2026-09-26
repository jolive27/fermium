//! The expression half of the parser (`fermium/parser.py`, "expressions" and "calculus syntax").

use std::collections::HashSet;

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer::{canonical_name, is_vulgar_str, ExtraVal, Kind};
use crate::parser::{vec_calc_word, Parser, CMP_OPS, R};
use crate::units::{is_affine, is_constant, is_unit_name};

fn bx(e: Expr) -> Box<Expr> {
    Box::new(e)
}

/// `isinstance(n, BinOp) and n.implicit and not n.paren`.
pub fn is_juxt(n: &Expr) -> bool {
    matches!(&n.kind, ExprKind::BinOp { implicit: true, .. }) && !n.paren
}

pub fn binop_left(n: &Expr) -> Option<&Expr> {
    if let ExprKind::BinOp { left, .. } = &n.kind {
        Some(left)
    } else {
        None
    }
}

pub fn binop_right(n: &Expr) -> Option<&Expr> {
    if let ExprKind::BinOp { right, .. } = &n.kind {
        Some(right)
    } else {
        None
    }
}

impl Parser {
    pub fn expr_where(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.expr_full()?;
        if self.at_kw("where") {
            let binds = self.where_bindings()?;
            let names: HashSet<String> = binds.iter().map(|(b, _)| b.clone()).collect();
            self.check_where_collisions(std::slice::from_ref(&e), &names)?;
            e = self.mks(ExprKind::Where { value: bx(e), bindings: binds }, t);
        }
        Ok(e)
    }

    pub fn where_bindings(&mut self) -> R<Vec<(String, Expr)>> {
        self.next();
        let mut binds = vec![];
        let saved = self.known.clone();
        let r = (|| -> R<()> {
            loop {
                let nt = self.expect_name("a name after 'where'")?;
                self.expect_op("=", None)?;
                let e = self.expr()?;
                let nv = self.toks[nt].s().to_string();
                binds.push((nv.clone(), e));
                self.known.insert(nv);
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                break;
            }
            Ok(())
        })();
        self.known = saved;
        r?;
        Ok(binds)
    }

    pub fn expr_full(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.expr()?;
        if self.at_kw("in") {
            self.next();
            let u = self.unit_expr(true, false)?;
            e = self.mks(ExprKind::Convert { value: bx(e), unit: u }, t);
        }
        Ok(e)
    }

    pub fn expr(&mut self) -> R<Expr> {
        if self.at_kw("if") {
            let t = self.next();
            let c = self.expr()?;
            self.expect_kw("then", Some("(write: if condition then a else b)"))?;
            let a = self.expr()?;
            if !self.at_kw("else") && self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Dedent]) {
                return Err(self.err_h(
                    "expected 'else' (an if-expression needs an else part) but the line ended",
                    "to continue on the next line, indent the line that starts with else, or put the whole right \
                     side in brackets ( … )",
                ));
            }
            self.expect_kw("else", Some("(an if-expression needs an else part)"))?;
            let b = self.expr()?;
            return Ok(self.mks(ExprKind::IfExpr { cond: bx(c), then: bx(a), other: bx(b) }, t));
        }
        self.or_expr()
    }

    fn or_expr(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.and_expr()?;
        while self.at_kw("or") {
            self.next();
            let r = self.and_expr()?;
            e = self.mks(ExprKind::Logic { op: "or".into(), left: bx(e), right: bx(r) }, t);
        }
        Ok(e)
    }

    fn and_expr(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.not_expr()?;
        while self.at_kw("and") && !self.and_is_separator() {
            self.next();
            let r = self.not_expr()?;
            e = self.mks(ExprKind::Logic { op: "and".into(), left: bx(e), right: bx(r) }, t);
        }
        Ok(e)
    }

    /// In `solve a = b and c = d` / `plot y vs x and z vs x`, 'and' separates items.
    fn and_is_separator(&self) -> bool {
        let mut depth = 0;
        let mut j = self.i + 1;
        while j < self.toks.len() {
            let t = &self.toks[j];
            if matches!(t.kind, Kind::Newline | Kind::Eof) {
                return false;
            }
            if t.kind == Kind::Op && "([{".contains(t.s()) {
                depth += 1;
            } else if t.kind == Kind::Op && ")]}".contains(t.s()) {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            } else if depth == 0 && (t.is_op("=") || t.is_kw("vs")) {
                return true;
            } else if depth == 0 && t.is_op(",") {
                return false;
            }
            j += 1;
        }
        false
    }

    fn not_expr(&mut self) -> R<Expr> {
        if self.at_kw("not") {
            let t = self.next();
            let e = self.not_expr()?;
            return Ok(self.mks(ExprKind::Not { operand: bx(e) }, t));
        }
        self.compare()
    }

    fn compare(&mut self) -> R<Expr> {
        let t = self.i;
        let e = self.sum()?;
        if !(self.kind() == Kind::Op && CMP_OPS.contains(&self.tok().s())) {
            return Ok(e);
        }
        let mut operands = vec![e];
        let mut ops: Vec<String> = vec![];
        let within = !self.known.contains("within");
        let mut op_tok = self.i;
        while self.kind() == Kind::Op && CMP_OPS.contains(&self.tok().s()) {
            op_tok = self.next();
            let ov = self.toks[op_tok].s().to_string();
            ops.push(ov.clone());
            let saved = self.no_juxt_names.clone();
            if ov == "~=" && within {
                self.no_juxt_names.insert("within".into());
            }
            let r = self.sum();
            self.no_juxt_names = saved;
            operands.push(r?);
        }
        if ops.len() == 1 {
            let right = operands.pop().unwrap();
            let left = operands.pop().unwrap();
            if ops[0] == "~=" {
                return self.approx(left, right, t, op_tok, within);
            }
            return Ok(self.mks(ExprKind::Compare { op: ops[0].clone(), left: bx(left), right: bx(right), tol: None }, t));
        }
        if ops.iter().any(|o| ["==", "!=", "~="].contains(&o.as_str())) && !ops.iter().all(|o| o == "==") {
            return Err(self.err_h(
                "a chain of comparisons can only use <, <=, > and >= (like a < x < b), or only ==",
                "write the comparisons separately, joined with and",
            ));
        }
        let mut binds = vec![];
        for k in 1..operands.len() - 1 {
            let x = &operands[k];
            if !matches!(x.kind, ExprKind::Name { .. } | ExprKind::Num { .. }) {
                self.chain_count += 1;
                let nm = format!("·chain{}", self.chain_count);
                let sp = x.span;
                let nn = self.name_node(&nm, sp);
                let old = std::mem::replace(&mut operands[k], nn);
                binds.push((nm, old));
            }
        }
        let mut e: Option<Expr> = None;
        for (k, op) in ops.iter().enumerate() {
            let lo = operands[k].clone();
            let hi = operands[k + 1].clone();
            let mut sp = lo.span;
            if hi.span.line == lo.span.line && hi.span.length != 0 {
                sp.length = (hi.span.col as i64 + hi.span.length as i64 - lo.span.col as i64).max(1) as u32;
            }
            let c = self.mk(ExprKind::Compare { op: op.clone(), left: bx(lo), right: bx(hi), tol: None }, sp);
            e = Some(match e {
                None => c,
                Some(prev) => {
                    let sp = prev.span;
                    self.mk(ExprKind::Logic { op: "and".into(), left: bx(prev), right: bx(c) }, sp)
                }
            });
        }
        let mut e = e.unwrap();
        e.span = self.span_from(t);
        if !binds.is_empty() {
            e = self.mks(ExprKind::Where { value: bx(e), bindings: binds }, t);
        }
        Ok(e)
    }

    /// A literal zero: `0`, `0.0 m/s`, `0 [m]`, `-0 J`, `<0, 0> m/s` (not `0 °C`). Returns the unit text to suggest
    /// for the tolerance ('' for a pure number), or None (D260).
    pub fn zero_literal(n: &Expr) -> Option<String> {
        let mut n = n;
        while let ExprKind::Neg { operand } = &n.kind {
            n = operand;
        }
        let mut unit = String::new();
        if let ExprKind::Quantity { value, unit: u, bracket } = &n.kind {
            if u.factors.iter().any(|f| is_affine(&f.name)) {
                return None;
            }
            let txt = u.text.trim();
            unit = format!(" {}", if *bracket { format!("[{txt}]") } else { txt.to_string() });
            n = value;
        }
        if let ExprKind::VecLit { items } = &n.kind {
            let zeros: Vec<Option<String>> = items.iter().map(Self::zero_literal).collect();
            if zeros.is_empty() || zeros.iter().any(|z| z.is_none()) {
                return None;
            }
            let mut inner: Vec<String> = vec![];
            for z in zeros.into_iter().flatten() {
                if !z.is_empty() && !inner.contains(&z) {
                    inner.push(z);
                }
            }
            return Some(if !unit.is_empty() {
                unit
            } else if inner.len() == 1 {
                inner[0].clone()
            } else {
                String::new()
            });
        }
        if n.num_value() == Some(0.0) {
            return Some(unit);
        }
        None
    }

    /// `a ≈ b [within tol]` (D260).
    fn approx(&mut self, left: Expr, right: Expr, t: usize, op_tok: usize, within: bool) -> R<Expr> {
        let mut tol = None;
        if within && self.tok().is_name("within") {
            let wt = self.next();
            if self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Dedent]) || self.at_op(")") || self.at_op(",") {
                return Err(self.error("'within' needs a tolerance after it, like  x ≈ 0 m/s within 1e-9 m/s", Some(wt),
                                      None));
            }
            let tl = self.sum()?;
            let v = match &tl.kind {
                ExprKind::Quantity { value, .. } => &**value,
                _ => &tl,
            };
            if matches!(v.kind, ExprKind::Neg { .. }) || v.num_value().is_some_and(|x| x < 0.0) {
                let src = self.src_of(&tl);
                let stripped = src.trim_start_matches(['-', '−', ' ']);
                return Err(self.error("a tolerance can't be negative", Some(wt),
                                      Some(format!("write the size of the allowed difference, like within {stripped}"))));
            }
            tol = Some(tl);
        }
        let mut zero_unit = Self::zero_literal(&right);
        let mut other = &left;
        if zero_unit.is_none() {
            zero_unit = Self::zero_literal(&left);
            other = &right;
        }
        let relative = matches!(&tol, Some(Expr { kind: ExprKind::Quantity { unit, .. }, .. })
                                if ["%", "percent"].contains(&unit.text.trim()));
        if let Some(zu) = &zero_unit {
            if Self::zero_literal(other).is_none() && (tol.is_none() || relative) {
                let ot = &self.toks[op_tok];
                let op = if ot.raw == "≈" || ot.raw == "~=" { ot.raw.clone() } else { "≈".into() };
                let written = format!("{} {op} {}", self.src_of(&left), self.src_of(&right));
                let fix = format!("{written} within 1e-9{zu}");
                let msg = if tol.is_none() {
                    format!("'{written}' is true only when {} is exactly 0: ≈ allows a difference of 10⁻⁶ × the \
                             larger size, which is 0 here", self.src_of(other))
                } else {
                    format!("'{written} within {}' is true only when {} is exactly 0: a percentage of 0 is 0",
                            self.src_of(tol.as_ref().unwrap()), self.src_of(other))
                };
                return Err(Diagnostic::error(msg, ot.line, ot.col, (ot.rawlen as u32).max(1),
                                             Some(format!("give an absolute tolerance: write {fix} (the difference \
                                                           you can accept)"))));
            }
        }
        Ok(self.mks(ExprKind::Compare { op: "~=".into(), left: bx(left), right: bx(right), tol: tol.map(bx) }, t))
    }

    pub fn sum(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.pm_term()?;
        while self.kind() == Kind::Op && ["+", "-"].contains(&self.tok().s()) {
            let op = self.next();
            let r = self.pm_term()?;
            let ov = self.toks[op].s().to_string();
            e = self.mks(ExprKind::BinOp { op: ov, left: bx(e), right: bx(r), implicit: false }, t);
        }
        Ok(e)
    }

    /// a ± b binds tighter than + and - and looser than * and / (D120): 2 x ± 0.1 is (2 x) ± 0.1.
    fn pm_term(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.product()?;
        while self.at_op("+-") {
            let op = self.next();
            if self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Dedent]) || self.at_op(")") || self.at_op(",") {
                return Err(self.error("± needs the uncertainty after it, like  L = 1.20 ± 0.01 m", Some(op), None));
            }
            let r = self.product()?;
            if matches!(e.kind, ExprKind::Uncertain { .. }) && !e.paren {
                return Err(self.error("a value can only have one ±; to add a second (independent) uncertainty, \
                                       write  (a ± b) ± c", Some(op), None));
            }
            // `5.0 ± 0.2 m`: the unit written after the uncertainty belongs to both numbers
            let e_is_neg = matches!(e.kind, ExprKind::Neg { .. }) && !e.paren;
            let ev: &Expr = if e_is_neg {
                if let ExprKind::Neg { operand } = &e.kind { operand } else { unreachable!() }
            } else {
                &e
            };
            let rq: &Expr = match &r.kind {
                ExprKind::Neg { operand } if !r.paren => operand,
                _ => &r,
            };
            let mut new_q = None;
            if matches!(ev.kind, ExprKind::Num { .. }) && !ev.paren {
                if let ExprKind::Quantity { value, unit, bracket } = &rq.kind {
                    if !rq.paren && value.is_num() && !*bracket && unit.text.trim() != "%" {
                        new_q = Some((ev.clone(), unit.clone()));
                    }
                }
            }
            if let Some((evc, unit)) = new_q {
                let sp = evc.span;
                let q = self.mk(ExprKind::Quantity { value: bx(evc), unit, bracket: false }, sp);
                e = if e_is_neg {
                    let sp = e.span;
                    self.mk(ExprKind::Neg { operand: bx(q) }, sp)
                } else {
                    q
                };
            }
            e = self.mks(ExprKind::Uncertain { value: bx(e), err: bx(r) }, t);
        }
        Ok(e)
    }

    fn product(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.unary()?;
        while self.kind() == Kind::Op && ["*", "/", "×"].contains(&self.tok().s()) {
            if self.tok().s() == "/" && self.ends_upper_limit() {
                break;
            }
            let op = self.next();
            let den_start = self.i;
            let tight = !self.toks[op].ws_before && !self.tok().ws_before;
            let mut r = self.unary()?;
            let mut info = None;
            let ov = self.toks[op].s().to_string();
            if ov == "/" {
                // `∫ … to E / (2 P0)`: for the checker (D205)
                let div_text = self.text(den_start, self.i);
                let rref = r.noderef();
                if let Some(last_ref) = Self::mark_limit_divisor(&mut e, op, &rref, &div_text) {
                    r.attrs.limit_div_of = Some(last_ref);
                }
                if let Some(coef) = self.fraction_coefficient(&e, &r, op)? {
                    e = coef;
                    continue;
                }
                let warned = self.check_ambiguous_division(&e, &r, op);
                info = self.juxt_denominator(&r, op, den_start, tight, warned)?;
            }
            e = self.mks(ExprKind::BinOp { op: ov, left: bx(e), right: bx(r), implicit: false }, t);
            if let Some(info) = info {
                e.attrs.div_info = Some(Box::new(info));
            }
        }
        Ok(e)
    }

    /// The last factor of `e` through implicit products and minus signs: if it is an integral whose upper limit
    /// ended at this '/', record the divisor on it. Returns a reference to the integral.
    fn mark_limit_divisor(e: &mut Expr, op: usize, r: &NodeRef, div_text: &str) -> Option<NodeRef> {
        let mut last = e;
        loop {
            let go = !last.paren
                && (matches!(last.kind, ExprKind::Neg { .. }) || matches!(last.kind, ExprKind::BinOp { implicit: true, .. }));
            if !go {
                break;
            }
            last = match &mut last.kind {
                ExprKind::Neg { operand } => operand,
                ExprKind::BinOp { right, .. } => right,
                _ => unreachable!(),
            };
        }
        if !matches!(last.kind, ExprKind::Integral { .. }) {
            return None;
        }
        let lref = last.noderef();
        let d = last.attrs.div_info.as_mut()?;
        if d.tok_i == Some(op) && d.divisor.is_none() {
            d.divisor = Some(r.clone());
            d.div_text = Some(div_text.to_string());
            return Some(lref);
        }
        None
    }

    /// `c²/g (√(1 + x) − 1)` is c²/(g (…)): implicit multiplication binds tighter than '/' (D8). Warn when the way
    /// it's written suggests (a/b) c (FRICTION #9, D34). Returns the denominator's factors.
    fn juxt_denominator(&mut self, den: &Expr, op: usize, den_start: usize, tight: bool, warned: bool)
                        -> R<Option<DivInfo>> {
        if !is_juxt(den) {
            return Ok(None);
        }
        let mut factors: Vec<DivFactor> = vec![];
        let mut n = den;
        while is_juxt(n) {
            let right = binop_right(n).unwrap();
            factors.push(DivFactor {
                node: right.noderef(),
                expr: Box::new(right.clone()),
                start: n.attrs.juxt_i.unwrap_or(0),
                ws: n.attrs.juxt_ws.unwrap_or(false),
            });
            n = binop_left(n).unwrap();
        }
        factors.push(DivFactor { node: n.noderef(), expr: Box::new(n.clone()), start: den_start, ws: false });
        factors.reverse();
        let mut end = self.i;
        if self.limit_start.is_none() && self.in_integrand > 0 {
            // `∫ 1/u du`: the trailing differential isn't part of the denominator
            while factors.len() > 1 {
                let last = factors.last().unwrap();
                let is_d = last.expr.name().is_some_and(|nm| nm.starts_with('d') && nm.chars().count() > 1);
                if !is_d {
                    break;
                }
                end = factors.pop().unwrap().start;
            }
            if factors.len() == 1 {
                return Ok(None);
            }
        }
        let tk = &self.toks[op];
        let mut info = DivInfo {
            op: Some((tk.line, tk.col)),
            op_i: Some(op),
            start: Some(den_start),
            end: Some(end),
            factors: Some(factors),
            warned: Some(warned),
            ..Default::default()
        };
        if warned {
            return Ok(Some(info));
        }
        let factors = info.factors.as_ref().unwrap();
        let spaced: Vec<usize> = (1..factors.len()).filter(|&k| factors[k].ws).collect();
        let bracketed: Vec<usize> =
            spaced.iter().copied().filter(|&k| factors[k].expr.paren || factors[k - 1].expr.paren).collect();
        if factors[0].expr.paren && spaced.contains(&1) {
            let mut bounds: Vec<usize> = factors.iter().map(|f| f.start).collect();
            bounds.push(end);
            let first = self.text(bounds[0], bounds[1]);
            let rest = self.text(bounds[1], *bounds.last().unwrap()).trim().to_string();
            return Err(self.error(
                format!("'/{first} {rest}' is ambiguous: implicit multiplication binds tighter than '/', so this would \
                         divide by all of '{first} {rest}'"),
                Some(bounds[0]),
                Some(format!("write  /{first} * {rest}  to multiply by {rest}, or  /({first} {rest})  to divide by both")),
            ));
        }
        if (tight && !spaced.is_empty()) || !bracketed.is_empty() {
            let k = if tight && !spaced.is_empty() { spaced[0] } else { bracketed[0] };
            self.warn_juxt_denominator(&info, k, "");
            info.warned = Some(true);
        }
        Ok(Some(info))
    }

    pub fn warn_juxt_denominator(&mut self, info: &DivInfo, k: usize, why: &str) {
        let factors = info.factors.as_ref().unwrap();
        let mut bounds: Vec<usize> = factors.iter().map(|f| f.start).collect();
        bounds.push(info.end.unwrap());
        let mut texts = vec![];
        for (j, f) in factors.iter().enumerate() {
            let mut tx = self.text(bounds[j], bounds[j + 1]);
            if f.expr.paren && tx.chars().count() > 12 {
                tx = "(…)".into();
            }
            texts.push(format!("{}{}", if f.ws && j > 0 { " " } else { "" }, tx));
        }
        let head = texts[..k].concat();
        let rest = texts[k..].concat().trim_start().to_string();
        let den = texts.concat();
        let op = info.op_i.unwrap();
        let ot = &self.toks[op];
        let d = Diagnostic::warning(
            format!("this divides by all of '{den}'{why}: implicit multiplication binds tighter than '/'"),
            ot.line, ot.col, ot.rawlen as u32,
            Some(format!("write …/{head} * {rest} if only {head} is below the line, or …/({den}) if all of it is")),
        );
        self.diags.warn(Diagnostic { length: ot.rawlen as u32, ..d });
    }

    /// A pure number as written: digits, π, √ and powers of them, and products of those (A2, D236).
    pub fn pure(n: &Expr) -> bool {
        match &n.kind {
            ExprKind::Num { .. } => true,
            ExprKind::Name { name } => name == "π" && n.attrs.imag_literal != Some(true),
            ExprKind::Neg { operand } | ExprKind::Sqrt { operand, .. } => Self::pure(operand),
            ExprKind::BinOp { op, left, right, implicit } => {
                if op == "^" {
                    return Self::pure(left) && Self::pure(right);
                }
                if *implicit || n.paren {
                    return Self::pure(left) && Self::pure(right);
                }
                false
            }
            _ => false,
        }
    }

    /// A2: a fraction of pure numbers is one coefficient (D236). Returns the rewritten product, or None.
    fn fraction_coefficient(&mut self, left: &Expr, right: &Expr, op: usize) -> R<Option<Expr>> {
        if !Self::pure(left) {
            return Ok(None);
        }
        let mut path_len = 0;
        let mut n = right;
        let mut last_path: Option<&Expr> = None;
        while is_juxt(n) {
            last_path = Some(n);
            path_len += 1;
            n = binop_left(n).unwrap();
        }
        let mut quantity: Option<&Expr> = None;
        let leaf: &Expr;
        if let ExprKind::Quantity { value, .. } = &n.kind {
            if !n.paren && value.is_num() {
                if !(Self::whole_literal(left) && Self::whole_literal(value)) {
                    return Ok(None);
                }
                quantity = Some(n);
                leaf = value;
            } else {
                leaf = n;
            }
        } else {
            leaf = n;
        }
        let leaf_ok = matches!(leaf.kind, ExprKind::Num { .. } | ExprKind::BinOp { .. } | ExprKind::Sqrt { .. }
                                          | ExprKind::Name { .. });
        let binop_not_pow = matches!(&leaf.kind, ExprKind::BinOp { op, .. } if op != "^") && !leaf.paren;
        if !leaf_ok || !Self::pure(leaf) || binop_not_pow {
            return Ok(None);
        }
        if path_len == 0 && quantity.is_none() {
            return Ok(None);
        }
        if leaf.num_value() == Some(1.0) {
            return Ok(None);
        }
        if let Some(lp) = last_path {
            if quantity.is_none() && Self::pure(binop_right(lp).unwrap()) && !leaf.paren {
                let (a, b, c) = (self.src_of(left), self.src_of(leaf), self.src_of(binop_right(lp).unwrap()));
                return Err(self.error(format!("'{a}/{b} {c}' is ambiguous: is it ({a}/{b})·{c} or {a}/({b} {c})?"),
                                      Some(op), Some(format!("write  ({a}/{b}) {c}  or  {a}/({b} {c})"))));
            }
        }
        let length = if leaf.span.line == left.span.line {
            (leaf.span.col as i64 + leaf.span.length as i64 - left.span.col as i64).max(1) as u32
        } else {
            1
        };
        let fsp = Span { line: left.span.line, col: left.span.col, length };
        let mut frac = self.mk(
            ExprKind::BinOp { op: "/".into(), left: bx(left.clone()), right: bx(leaf.clone()), implicit: false },
            fsp,
        );
        frac.paren = true;
        frac.attrs.coefficient = Some(true);
        let new_leaf = if let Some(q) = quantity {
            let (unit, bracket) = match &q.kind {
                ExprKind::Quantity { unit, bracket, .. } => (unit.clone(), *bracket),
                _ => unreachable!(),
            };
            let sp = Span { line: fsp.line, col: fsp.col, length: q.span.length };
            self.mk(ExprKind::Quantity { value: bx(frac), unit, bracket }, sp)
        } else {
            frac
        };
        if path_len == 0 {
            return Ok(Some(new_leaf));
        }
        let mut r = right.clone();
        let (lline, lcol) = (left.span.line, left.span.col);
        let mut cur = &mut r;
        let mut new_leaf = Some(new_leaf);
        for depth in 0..path_len {
            if cur.span.line == lline {
                cur.span.col = lcol;
            }
            let ExprKind::BinOp { left: l, .. } = &mut cur.kind else { unreachable!() };
            if depth == path_len - 1 {
                *l = bx(new_leaf.take().unwrap());
                break;
            }
            cur = l;
        }
        Ok(Some(r))
    }

    /// A whole number written with digits only: `1`, `24`, not `0.5`, `2.2`, `1e3`.
    pub fn whole_literal(n: &Expr) -> bool {
        match (&n.kind, &n.attrs.raw) {
            (ExprKind::Num { digit: true, .. }, Some(raw)) => {
                !n.paren && !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit())
            }
            _ => false,
        }
    }

    /// The source text of a node on one line (for messages).
    pub fn src_of(&self, n: &Expr) -> String {
        if n.is_num() {
            return num_text(n);
        }
        if let Some(nm) = n.name() {
            return nm.to_string();
        }
        let Some(k) = self.toks.iter().position(|tk| tk.line == n.span.line && tk.col == n.span.col) else {
            return "…".into();
        };
        let end = n.span.col + n.span.length;
        let mut j = k;
        while j + 1 < self.toks.len() && self.toks[j + 1].line == n.span.line && self.toks[j + 1].col < end {
            j += 1;
        }
        self.text(k, j + 1)
    }

    /// Warn about `1/2 m v²` which Fermium reads as 1/(2 m v²).
    fn check_ambiguous_division(&mut self, left: &Expr, right: &Expr, op: usize) -> bool {
        if is_juxt(right) {
            let mut first = right;
            while is_juxt(first) {
                first = binop_left(first).unwrap();
            }
            if let ExprKind::Quantity { value, bracket: false, .. } = &first.kind {
                if value.is_num() {
                    first = value;
                }
            }
            if first.is_num() && left.is_num() && !left.paren {
                let (a, b) = (num_text(left), num_text(first));
                let ot = &self.toks[op];
                let d = Diagnostic {
                    message: format!("this is read as a/(b c), i.e. {a}/({b} ...): implicit multiplication binds \
                                      tighter than '/'"),
                    line: Some(ot.line),
                    col: Some(ot.col),
                    length: ot.rawlen as u32,
                    hint: Some(format!("if you meant ({a}/{b}) times the rest, write ({a}/{b}) with parentheses (or ½ \
                                        for one half)")),
                    severity: crate::diag::Severity::Warning,
                    fix: vec![],
                };
                self.diags.warn(d);
                return true;
            }
        }
        false
    }

    fn unary(&mut self) -> R<Expr> {
        if self.at_op("-") {
            let t = self.next();
            let e = self.unary()?;
            return Ok(self.mks(ExprKind::Neg { operand: bx(e) }, t));
        }
        if self.at_op("+") {
            self.next();
            return self.unary();
        }
        self.juxt()
    }

    /// At '[': is this a unit in brackets ([m/s]) rather than a list ([1, 2])?
    pub fn bracket_is_unit(&self) -> bool {
        let j = self.i + 1;
        let t = self.tk(j as i64);
        if t.kind == Kind::Name && self.unit_tok(j.min(self.toks.len() - 1)) {
            let Some(k) = self.match_(self.i) else { return true };
            return !(self.i..k).any(|m| self.toks[m].is_op(","));
        }
        let nx = self.tk(j as i64 + 1);
        if t.kind == Kind::Num && t.f() == 1.0 && nx.kind == Kind::Op && ["/", "]"].contains(&nx.s()) {
            return true;
        }
        if t.kind == Kind::Name
            && t.raw == "h"
            && !self.known.contains("h")
            && (nx.kind == Kind::Sup || (nx.kind == Kind::Op && ["]", "/", "^"].contains(&nx.s())))
        {
            return true;
        }
        false
    }

    fn starts_term(&self) -> bool {
        let t = self.tok();
        if t.is_op("[") && t.ws_before && !self.bracket_is_unit() {
            return true;
        }
        match t.kind {
            Kind::Num | Kind::Imag => true,
            Kind::Name => !self.no_juxt_names.contains(t.s()),
            Kind::Kw => ["sqrt", "cbrt", "integral", "partial", "nabla"].contains(&t.s()),
            Kind::Op => {
                (t.s() == "(") || (t.s() == "|" && self.abs_depth == 0) || (t.s() == "<" && self.vector_after_space())
            }
            _ => false,
        }
    }

    /// At '<': is this `R <cos φ, sin φ, 0>`, a vector literal multiplied by what came before? (FRICTION #7)
    fn vector_after_space(&self) -> bool {
        let t = self.tok();
        if !t.ws_before || self.peek(1).ws_before {
            return false;
        }
        let (mut depth, mut comma) = (0, false);
        let mut j = self.i + 1;
        while j < self.toks.len() {
            let tk = &self.toks[j];
            if matches!(tk.kind, Kind::Newline | Kind::Eof | Kind::Indent | Kind::Dedent)
                || (tk.kind == Kind::Op && (tk.s() == "=" || tk.s() == "<") && depth == 0)
            {
                return false;
            }
            if tk.kind == Kind::Op && "([{".contains(tk.s()) {
                depth += 1;
            } else if tk.kind == Kind::Op && ")]}".contains(tk.s()) {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            } else if depth == 0 && tk.is_op(",") {
                comma = true;
            } else if depth == 0 && tk.is_op(">") {
                return comma && !tk.ws_before;
            }
            j += 1;
        }
        false
    }

    /// Every name the program assigns, loops over or defines as a function or parameter, wherever it is (D215).
    pub fn named_anywhere(&mut self) -> &HashSet<String> {
        if self.named.is_none() {
            let mut out = HashSet::new();
            let n = self.toks.len();
            for j in 0..n {
                let tk = &self.toks[j];
                if tk.kind != Kind::Name || j + 1 >= n {
                    continue;
                }
                let nx = &self.toks[j + 1];
                let prev = if j > 0 { Some(&self.toks[j - 1]) } else { None };
                if (nx.kind == Kind::Op && ["=", "+=", "-=", "*=", "/=", "^="].contains(&nx.s()))
                    || prev.is_some_and(|p| matches!(p.kind, Kind::Kw | Kind::Name) && ["for", "as", "import"].contains(&p.s()))
                {
                    out.insert(tk.s().to_string());
                } else if nx.is_op("(")
                    && !nx.ws_before
                    && (prev.is_none()
                        || prev.is_some_and(|p| {
                            matches!(p.kind, Kind::Newline | Kind::Indent | Kind::Dedent)
                                || (p.kind == Kind::Kw && ["def", "function"].contains(&p.s()))
                        }))
                {
                    if let Some(k) = self.match_(j + 1) {
                        if k + 1 < n && self.toks[k + 1].is_op("=") {
                            out.insert(tk.s().to_string());
                            for p in &self.toks[j + 2..k] {
                                if p.kind == Kind::Name {
                                    out.insert(p.s().to_string());
                                }
                            }
                        }
                    }
                }
            }
            self.named = Some(out);
        }
        self.named.as_ref().unwrap()
    }

    fn is_named_anywhere(&mut self, name: &str) -> bool {
        self.named_anywhere().contains(name)
    }

    /// `(51 - 33 (N - Z)/A) MeV`: a unit right after a bracketed expression multiplies it by 1 unit (D7, D215).
    fn unit_after_paren(&mut self, e: &Expr, t: usize) -> R<Option<Expr>> {
        let tk = self.tok().clone();
        let vulgar = e.is_num()
            && matches!(e.kind, ExprKind::Num { digit: false, .. })
            && e.attrs.raw.as_deref().is_some_and(is_vulgar_str);
        if !((e.paren || vulgar)
            && !matches!(e.kind, ExprKind::Uncertain { .. })
            && tk.kind == Kind::Name
            && self.unit_tok(self.i)
            && !self.no_juxt_names.contains(tk.s())
            && !is_constant(tk.s())
            && !self.is_call_like())
        {
            return Ok(None);
        }
        if self.known.contains(tk.s()) || self.is_named_anywhere(tk.s()) {
            return Ok(None);
        }
        if self.in_integrand > 0
            && tk.raw.starts_with('d')
            && tk.raw.chars().count() > 1
            && self.known.contains(&canonical_name(&tk.raw[1..]))
        {
            return Ok(None);
        }
        let j0 = self.i;
        let u = self.unit_expr(false, false)?;
        let mut q = self.mks(ExprKind::Quantity { value: bx(e.clone()), unit: u, bracket: false }, t);
        q.attrs.times_unit = Some(true);
        if !Self::pure(e) {
            self.bracket_needed(j0, "a bracket")?;
        }
        Ok(Some(q))
    }

    /// A unit name that doesn't come right after a number is a variable (the A1 rule) (D215).
    pub fn bracket_needed(&mut self, j0: usize, after: &str) -> R<()> {
        let (first, last) = (j0, self.i - 1);
        let fix = self.bracket_fix(first, last);
        if self.fix_mode {
            self.add_fix(fix);
            return Ok(());
        }
        let unit = self.unit_span_text(j0, self.i - 1);
        let extra = if unit == "g" { "; for standard gravity use g_n, or define g = 9.81 m/s²" } else { "" };
        let mut e = self.error(
            format!("'{unit}' after {after} is read as a variable, and you haven't defined {}", self.toks[first].raw),
            Some(first),
            Some(format!("for the unit write it in brackets: [{unit}]{extra}")),
        );
        e.fix = vec![fix];
        Err(e)
    }

    /// Is e a product written after a number, like `100 h`?
    fn number_first(e: &Expr) -> bool {
        let mut e = e;
        while is_juxt(e) {
            e = binop_left(e).unwrap();
        }
        matches!(e.kind, ExprKind::Num { digit: true, .. }) && !e.paren
    }

    /// A unit name here that no variable, function, parameter or constant of the program shares (D215).
    fn free_unit_here(&mut self) -> bool {
        let tk = self.tok().clone();
        tk.kind == Kind::Name
            && self.unit_tok(self.i)
            && !self.known.contains(tk.s())
            && !self.is_named_anywhere(tk.s())
            && !self.no_juxt_names.contains(tk.s())
            && !is_constant(tk.s())
            && !self.is_call_like()
    }

    fn juxt(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.power()?;
        if matches!(e.kind, ExprKind::Uncertain { .. })
            && e.paren
            && self.kind() == Kind::Name
            && self.unit_tok(self.i)
            && !self.known.contains(self.tok().s())
            && !self.is_call_like()
        {
            let u = self.unit_expr(false, false)?;
            e = self.mks(ExprKind::Quantity { value: bx(e), unit: u, bracket: false }, t);
        }
        if let Some(q) = self.unit_after_paren(&e, t)? {
            e = q;
        }
        loop {
            if self.at_op("[") && self.tok().ws_before && self.bracket_is_unit() {
                let u = self.bracket_unit()?;
                e = self.mks(ExprKind::Quantity { value: bx(e), unit: u, bracket: true }, t);
                continue;
            }
            if !self.starts_term() {
                break;
            }
            let (ws, ri) = (self.tok().ws_before, self.i);
            let mut r = self.power()?;
            if r.is_name() && !e.is_num() {
                r.attrs.unit_left = Some(e.noderef());
            }
            let r_paren = r.paren;
            let r_known_name = r.name().is_some_and(|nm| self.known.contains(nm));
            let r_clone = if r_paren { Some(r.clone()) } else { None };
            e = self.mks(ExprKind::BinOp { op: "*".into(), left: bx(e), right: bx(r), implicit: true }, t);
            e.attrs.juxt_ws = Some(ws);
            e.attrs.juxt_i = Some(ri);
            if r_paren {
                if let Some(q) = self.unit_after_paren(r_clone.as_ref().unwrap(), t)? {
                    let ExprKind::Quantity { unit, .. } = q.kind else { unreachable!() };
                    e = self.mks(ExprKind::Quantity { value: bx(e), unit, bracket: false }, t);
                    e.attrs.times_unit = Some(true);
                }
            } else if r_known_name && Self::number_first(&e) && self.free_unit_here() {
                // `100 h km/s/Mpc` with your h: a compound unit after `number variable` is a unit (D215)
                let j0 = self.i;
                let hi = (j0 + 16).min(self.toks.len());
                let roles: Vec<String> = self.toks[j0..hi].iter().map(|tk| tk.role.clone()).collect();
                let u = self.unit_expr(false, false)?;
                if u.factors.len() >= 2 {
                    e = self.mks(ExprKind::Quantity { value: bx(e), unit: u, bracket: false }, t);
                    e.attrs.times_unit = Some(true);
                    self.bracket_needed(j0, "your variable")?;
                } else {
                    for (k, rl) in roles.into_iter().enumerate() {
                        self.toks[j0 + k].role = rl;
                    }
                    self.i = j0;
                }
            }
        }
        Ok(e)
    }

    /// The number of a quantity as it was written (`8.5e28`, not `8.5e+28`), for messages (round 4 #9).
    pub fn num_text_of(q: &Expr, default: &str) -> String {
        let n = match &q.kind {
            ExprKind::Quantity { value, .. } => &**value,
            _ => q,
        };
        if n.is_num() {
            num_text(n)
        } else {
            default.to_string()
        }
    }

    fn sup_num(&mut self, s: usize) -> Expr {
        let sp = self.tok_span(s);
        let v = self.toks[s].int();
        self.mk(ExprKind::Num { value: v as f64, sigfigs: None, digit: false }, sp)
    }

    fn power(&mut self) -> R<Expr> {
        let t = self.i;
        let base = self.postfix()?;
        if self.kind() == Kind::Sup {
            let s = self.next();
            let mut e = self.sup_num(s);
            e.attrs.extra_int = Some(self.toks[s].int());
            if self.kind() == Kind::Sup {
                return Err(self.err("two exponents in a row"));
            }
            let base_digit_num = matches!(base.kind, ExprKind::Num { digit: true, .. });
            let mut r = self.mks(ExprKind::BinOp { op: "^".into(), left: bx(base), right: bx(e), implicit: false }, t);
            if base_digit_num && self.kind() == Kind::Name && self.unit_tok(self.i) && !self.is_call_like() {
                let u = self.unit_expr(false, false)?;
                let single_known = u.factors.len() == 1 && self.known.contains(&u.factors[0].name);
                let ufirst = self.u_first(&u);
                r = self.mks(ExprKind::Quantity { value: bx(r), unit: u, bracket: false }, t);
                if single_known {
                    let num = self.text(t, ufirst);
                    self.single_collision(&r, None, false, Some(num))?;
                }
            }
            return Ok(r);
        }
        if self.at_op("^") {
            self.next();
            let ex = self.exponent()?;
            let base_digit_num = matches!(base.kind, ExprKind::Num { digit: true, .. });
            let mut r = self.mks(ExprKind::BinOp { op: "^".into(), left: bx(base), right: bx(ex), implicit: false }, t);
            if base_digit_num && self.kind() == Kind::Name && self.unit_tok(self.i) && !self.is_call_like() {
                let u = self.unit_expr(false, false)?;
                let single_known = u.factors.len() == 1 && self.known.contains(&u.factors[0].name);
                let ufirst = self.u_first(&u);
                r = self.mks(ExprKind::Quantity { value: bx(r), unit: u, bracket: false }, t);
                if single_known {
                    let num = self.text(t, ufirst);
                    self.single_collision(&r, None, false, Some(num))?;
                }
            }
            return Ok(r);
        }
        Ok(base)
    }

    /// The token where the unit expression u starts.
    pub fn u_first(&self, u: &UnitExpr) -> usize {
        let f = &u.factors[0];
        self.toks.iter().position(|tk| tk.line == f.span.line && tk.col == f.span.col).unwrap_or(0)
    }

    /// The thing after ^ : a signed atom, possibly itself raised (right-assoc).
    fn exponent(&mut self) -> R<Expr> {
        let t = self.i;
        if self.at_op("-") {
            self.next();
            let e = self.exponent()?;
            return Ok(self.mks(ExprKind::Neg { operand: bx(e) }, t));
        }
        if self.at_op("+") {
            self.next();
            return self.exponent();
        }
        let base = if matches!(self.kind(), Kind::Num | Kind::Imag) {
            let nt = self.next();
            let ntt = self.toks[nt].clone();
            let sp = Span { line: ntt.line, col: ntt.col, length: ntt.rawlen as u32 };
            let base = self.mk(ExprKind::Num { value: ntt.f(), sigfigs: ntt.sigfigs, digit: ntt.digit }, sp);
            if ntt.kind == Kind::Imag {
                let bsp = base.span;
                if ntt.f() == 1.0 && ntt.sigfigs.is_none() {
                    self.name_node("𝑖", bsp)
                } else {
                    let i = self.name_node("𝑖", bsp);
                    self.mk(ExprKind::BinOp { op: "*".into(), left: bx(base), right: bx(i), implicit: false }, bsp)
                }
            } else {
                base
            }
        } else {
            self.postfix()?
        };
        if self.at_op("^") {
            self.next();
            let ex = self.exponent()?;
            return Ok(self.mks(ExprKind::BinOp { op: "^".into(), left: bx(base), right: bx(ex), implicit: false }, t));
        }
        if self.kind() == Kind::Sup {
            let s = self.next();
            let e = self.sup_num(s);
            return Ok(self.mks(ExprKind::BinOp { op: "^".into(), left: bx(base), right: bx(e), implicit: false }, t));
        }
        Ok(base)
    }

    /// ∇f, ∇·f, ∇×f, ∇²f (ASCII: nabla f, nabla*F, nabla×F, nabla^2 f).
    fn nabla_op(&mut self) -> R<Expr> {
        let t = self.next();
        let mut kind = "grad";
        if self.kind() == Kind::Sup && self.tok().int() == 2 {
            self.next();
            kind = "lap";
        } else if self.at_op("^") && self.peek(1).kind == Kind::Num && self.peek(1).f() == 2.0 {
            self.next();
            self.next();
            kind = "lap";
        } else if self.at_op("*") {
            self.next();
            kind = "div";
        } else if self.at_op("×") {
            self.next();
            kind = "curl";
        }
        if self.kind() != Kind::Name {
            return Err(self.err(format!("∇ needs the name of a function after it, like ∇φ, ∇·E, ∇×B or ∇²φ{}",
                                        self.found())));
        }
        let n = self.next();
        let nt = self.toks[n].clone();
        let name = self.name_node(nt.s(), Span { line: nt.line, col: nt.col, length: nt.rawlen as u32 });
        Ok(self.mks(ExprKind::VecCalc { kind: kind.into(), func: bx(name) }, t))
    }

    fn postfix(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.atom()?;
        loop {
            let callable = !matches!(e.kind, ExprKind::Num { .. } | ExprKind::Quantity { .. })
                && (!e.paren || matches!(e.kind, ExprKind::Deriv { .. } | ExprKind::Prime { .. }));
            if self.at_op("(") && !self.tok().ws_before && callable {
                if e.name() == Some("table")
                    && !self.known.contains("table")
                    && self.peek(1).kind == Kind::Name
                    && self.at_op_at(self.i + 2, "=")
                {
                    e = self.table_args(t)?;
                    continue;
                }
                self.next();
                let mut args = vec![];
                self.skip_newlines();
                while !self.at_op(")") {
                    args.push(self.expr_full()?);
                    if self.at_op(",") {
                        self.next();
                    } else if !self.at_op(")") {
                        return Err(self.err(format!("expected ',' or ')' in the list of arguments{}", self.found())));
                    }
                }
                self.next();
                let fname = e.name().map(|s| s.to_string());
                e = self.mks(ExprKind::Call { func: bx(e), args }, t);
                if let Some(fname) = &fname {
                    if let Some(kind) = vec_calc_word(fname) {
                        if !self.known.contains(fname) {
                            if let ExprKind::Call { args, .. } = &e.kind {
                                if args.len() == 1 && args[0].is_name() {
                                    let a = args[0].clone();
                                    e = self.mks(ExprKind::VecCalc { kind: kind.into(), func: bx(a) }, t);
                                }
                            }
                        }
                    }
                    if fname == "vec"
                        && matches!(e.kind, ExprKind::Call { .. })
                        && self.kind() == Kind::Name
                        && self.unit_tok(self.i)
                        && !self.known.contains(self.tok().s())
                    {
                        let u = self.unit_expr(false, false)?;
                        e = self.mks(ExprKind::Quantity { value: bx(e), unit: u, bracket: false }, t);
                    }
                }
            } else if self.at_op("[") && !self.tok().ws_before {
                let st = self.next();
                let mut idx = if self.at_op(":") { None } else { Some(self.expr()?) };
                if self.at_op(":") {
                    self.next();
                    let hi = if self.at_op("]") { None } else { Some(self.expr()?) };
                    idx = Some(self.mks(ExprKind::Slice { lo: idx.map(bx), hi: hi.map(bx) }, st));
                }
                e = self.mks(ExprKind::Index { target: bx(e), index: idx.map(bx) }, t);
                if self.at_op(",") {
                    self.next();
                    let i2 = self.expr()?;
                    e = self.mks(ExprKind::Index { target: bx(e), index: Some(bx(i2)) }, t);
                }
                self.expect_op("]", None)?;
            } else if self.at_op("ᵀ") {
                self.next();
                let sp = e.span;
                let f = self.name_node("transpose", sp);
                e = self.mks(ExprKind::Call { func: bx(f), args: vec![e] }, t);
            } else if self.at_op("[") && self.tok().ws_before && !e.is_num() && self.bracket_is_unit() {
                let u = self.bracket_unit()?;
                e = self.mks(ExprKind::Quantity { value: bx(e), unit: u, bracket: true }, t);
            } else if self.at_op(".")
                && !self.tok().ws_before
                && (self.peek(1).kind == Kind::Name
                    || (self.peek(1).kind == Kind::Kw && self.peek(2).is_op("(") && !self.peek(2).ws_before))
            {
                self.next();
                let name = self.next();
                let (nv, nr) = (self.toks[name].s().to_string(), self.toks[name].raw.clone());
                e = self.mks(ExprKind::Field { target: bx(e), name: nv }, t);
                e.attrs.raw = Some(nr);
            } else if self.kind() == Kind::Prime {
                let p = self.next();
                let order = self.toks[p].int();
                e = self.mks(ExprKind::Prime { target: bx(e), order }, t);
            } else {
                return Ok(e);
            }
        }
    }

    /// table(x = xs, y = ys): named columns (D193).
    fn table_args(&mut self, t: usize) -> R<Expr> {
        self.next();
        let (mut names, mut items): (Vec<String>, Vec<Expr>) = (vec![], vec![]);
        self.skip_newlines();
        while !self.at_op(")") {
            let nt = self.expect_name("a column name, like  table(x = xs, y = ys)")?;
            let nv = self.toks[nt].s().to_string();
            if names.contains(&nv) {
                return Err(self.error(format!("the column {nv} appears twice in this table"), Some(nt), None));
            }
            self.expect_op("=", Some("after the column name (write: table(x = xs, y = ys))"))?;
            names.push(nv);
            items.push(self.expr_full()?);
            self.skip_newlines();
            if self.at_op(",") {
                self.next();
                self.skip_newlines();
            } else if !self.at_op(")") {
                return Err(self.err(format!("expected ',' or ')' in this table{}", self.found())));
            }
        }
        self.next();
        Ok(self.mks(ExprKind::Table { names, items }, t))
    }

    fn atom(&mut self) -> R<Expr> {
        let t = self.i;
        let tt = self.tok().clone();
        if matches!(tt.kind, Kind::Num | Kind::Imag) {
            self.next();
            let sp = Span { line: tt.line, col: tt.col, length: tt.rawlen as u32 };
            let mut n = self.mk(ExprKind::Num { value: tt.f(), sigfigs: tt.sigfigs, digit: tt.digit }, sp);
            if tt.kind == Kind::Num {
                n.attrs.raw = Some(tt.raw.clone());
            }
            if tt.kind == Kind::Imag {
                let nsp = n.span;
                n = if tt.f() == 1.0 && tt.sigfigs.is_none() {
                    self.name_node("𝑖", nsp)
                } else {
                    let i = self.name_node("𝑖", nsp);
                    self.mk(ExprKind::BinOp { op: "*".into(), left: bx(n), right: bx(i), implicit: false }, nsp)
                };
                n.attrs.imag_literal = Some(true);
            }
            if tt.digit {
                if self.at_op("[") && self.bracket_is_unit() {
                    let u = self.bracket_unit()?;
                    return Ok(self.mks(ExprKind::Quantity { value: bx(n), unit: u, bracket: true }, t));
                }
                if self.kind() == Kind::Name && self.unit_tok(self.i) && !self.is_call_like() {
                    let ustart = self.i;
                    let u = self.unit_expr(false, false)?;
                    let single_known = u.factors.len() == 1 && self.known.contains(&u.factors[0].name);
                    let q = self.mks(ExprKind::Quantity { value: bx(n), unit: u, bracket: false }, t);
                    if single_known {
                        self.single_collision(&q, Some(ustart), false, None)?;
                    }
                    return Ok(q);
                }
                let denominator = t > 0 && self.toks[t - 1].is_op("/");
                if !denominator {
                    self.check_mixed_reciprocal(&tt.raw)?;
                }
                if !denominator && (self.unit_reciprocal_follows() || self.bracketed_reciprocal_unit()) {
                    let u = self.unit_expr(true, true)?;
                    return Ok(self.mks(ExprKind::Quantity { value: bx(n), unit: u, bracket: false }, t));
                }
            }
            return Ok(n);
        }
        if tt.kind == Kind::Str {
            self.next();
            return Ok(self.mks(ExprKind::Str { value: tt.s().to_string() }, t));
        }
        if tt.kind == Kind::Name {
            if tt.s() == "d" && self.is_deriv_op() {
                return self.deriv_op();
            }
            if tt.s() == "d" {
                if let Some(lz) = self.leibniz_higher()? {
                    return Ok(lz);
                }
            }
            if tt.s() == "∞"
                && self.peek(1).kind == Kind::Name
                && self.unit_tok((self.i + 1).min(self.toks.len() - 1))
                && !self.known.contains(self.peek(1).s())
            {
                self.next();
                let n = self.mks(ExprKind::Name { name: "∞".into() }, t);
                let u = self.unit_expr(false, false)?;
                return Ok(self.mks(ExprKind::Quantity { value: bx(n), unit: u, bracket: false }, t));
            }
            if (tt.s() == "Σ" || tt.s() == "sum") && self.peek(1).is_op("(") {
                if let Some(j) = self.sum_for_index() {
                    return self.sum_expr(j);
                }
            }
            self.next();
            if tt.s() == "end" && self.in_index() {
                return Ok(self.mks(ExprKind::End, t));
            }
            return Ok(self.mks(ExprKind::Name { name: tt.s().to_string() }, t));
        }
        if tt.kind == Kind::Kw {
            let kw = tt.s();
            if kw == "to" && self.peek(1).is_op("(") && !self.peek(1).ws_before {
                self.next();
                return Ok(self.mks(ExprKind::Name { name: "to".into() }, t));
            }
            if kw == "true" || kw == "false" {
                self.next();
                return Ok(self.mks(ExprKind::Bool { value: kw == "true" }, t));
            }
            if kw == "sqrt" || kw == "cbrt" {
                self.next();
                let start_i = self.i;
                let operand = self.power()?;
                let end = self.i as i64 - 1;
                self.toks[t].set_extra("operand_end", ExtraVal::Int(end));
                let st_paren = self.toks[start_i].is_op("(") && self.match_(start_i).map(|m| m as i64) == Some(end);
                self.toks[t].set_extra("operand_paren", ExtraVal::Bool(st_paren));
                return Ok(self.mks(ExprKind::Sqrt { operand: bx(operand), root: if kw == "sqrt" { 2 } else { 3 } }, t));
            }
            if kw == "integral" {
                if tt.raw == "integral" && self.at_op_at(self.i + 1, "(") && !self.peek(1).ws_before {
                    let k = self.match_(self.i + 1);
                    let mut depth = 0;
                    let hi = k.unwrap_or(self.i + 2);
                    for m in self.i + 2..hi {
                        let tk = &self.toks[m];
                        if tk.kind == Kind::Op && "([{".contains(tk.s()) {
                            depth += 1;
                        } else if tk.kind == Kind::Op && ")]}".contains(tk.s()) {
                            depth -= 1;
                        } else if depth == 0 && tk.is_op(",") {
                            return Err(self.keyword_as_name(t));
                        }
                    }
                }
                return self.integral();
            }
            if kw == "partial" {
                return self.partial_op();
            }
            if kw == "nabla" {
                return self.nabla_op();
            }
            if kw == "load" {
                self.next();
                if self.kind() != Kind::Str {
                    return Err(self.err("expected a file name in quotes after load, like load \"data.csv\""));
                }
                let p = self.next();
                let path = self.toks[p].s().to_string();
                return Ok(self.mks(ExprKind::Load { path }, t));
            }
        }
        if tt.kind == Kind::Op {
            match tt.s() {
                "(" => {
                    self.next();
                    let saved_abs = self.abs_depth;
                    self.abs_depth = 0;
                    self.skip_newlines();
                    let mut e = self.expr_full()?;
                    self.skip_newlines();
                    self.abs_depth = saved_abs;
                    if !self.at_op(")") {
                        if self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Indent, Kind::Dedent]) {
                            return Err(self.error("this '(' is never closed", Some(t), Some("add the missing ')'".into())));
                        }
                        return Err(self.err(format!("expected ')' to close '('{}", self.found())));
                    }
                    self.next();
                    e.paren = true;
                    return Ok(e);
                }
                "[" => {
                    self.next();
                    let mut items = vec![];
                    while !self.at_op("]") {
                        items.push(self.expr()?);
                        if self.at_op(",") {
                            self.next();
                        } else if !self.at_op("]") {
                            return Err(self.err(format!("expected ',' or ']' in this list{}", self.found())));
                        }
                    }
                    self.next();
                    let has = !items.is_empty();
                    let mut lst = self.mks(ExprKind::ListLit { items }, t);
                    let name_unit = has && self.kind() == Kind::Name && self.unit_tok(self.i) && !self.is_call_like();
                    if name_unit && !self.known.contains(self.tok().s()) {
                        let u = self.unit_expr(false, false)?;
                        lst = self.mks(ExprKind::Quantity { value: bx(lst), unit: u, bracket: false }, t);
                    } else if name_unit
                        && ((self.peek(1).is_op("/")
                            && self.peek(2).kind == Kind::Name
                            && self.unit_tok((self.i + 2).min(self.toks.len() - 1)))
                            || (self.peek(1).kind == Kind::Name
                                && self.unit_tok((self.i + 1).min(self.toks.len() - 1))
                                && !self.known.contains(self.peek(1).s())))
                    {
                        let u = self.unit_expr(false, false)?;
                        lst = self.mks(ExprKind::Quantity { value: bx(lst), unit: u, bracket: false }, t);
                    } else if name_unit {
                        self.list_collision(t)?;
                    }
                    return Ok(lst);
                }
                "<" => return self.vector_literal(),
                "|" => {
                    self.next();
                    self.abs_depth += 1;
                    let e = self.expr()?;
                    self.abs_depth -= 1;
                    self.expect_op("|", Some("to close the absolute value |x|"))?;
                    return Ok(self.mks(ExprKind::Abs { operand: bx(e) }, t));
                }
                "+-" => return Err(self.err("± needs a value on its left, like  L = 1.20 ± 0.01 m")),
                _ => {}
            }
        }
        if matches!(tt.kind, Kind::Newline | Kind::Eof) {
            let prev = if self.i > 0 { Some(self.toks[self.i - 1].clone()) } else { None };
            let pp = if self.i > 1 { Some(self.toks[self.i - 2].clone()) } else { None };
            if let (Some(prev), Some(pp)) = (prev, pp) {
                if prev.kind == Kind::Op && pp.kind == Kind::Op && prev.s() == pp.s() && ["+", "-"].contains(&prev.s()) {
                    return Err(self.err_h("this line ended before the expression was complete",
                                          format!("Fermium has no {0}{0}; write  x {0}= 1", prev.s())));
                }
            }
            return Err(self.err("this line ended before the expression was complete"));
        }
        if tt.is_op("=") {
            return Err(self.err_h("unexpected '='", "use == to compare two values"));
        }
        Err(self.err(format!("didn't expect '{}' here", tt.raw)))
    }

    /// At `Σ (` or `sum (`: the index of a `for` at the top level inside the brackets, or None.
    fn sum_for_index(&self) -> Option<usize> {
        let mut depth = 0;
        let mut j = self.i + 1;
        while j < self.toks.len() {
            let tk = &self.toks[j];
            if tk.kind == Kind::Eof {
                return None;
            }
            if tk.kind == Kind::Op && "([{".contains(tk.s()) {
                depth += 1;
            } else if tk.kind == Kind::Op && ")]}".contains(tk.s()) {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
            } else if depth == 1 && tk.is_kw("for") {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Σ(k² for k from 1 to 10) or sum(f(k) for k from 1 to N step 2): a one-line sum (#49, D51).
    fn sum_expr(&mut self, j: usize) -> R<Expr> {
        let t = self.next();
        self.next();
        if self.tk(j as i64 + 1).kind == Kind::Name {
            let v = self.tk(j as i64 + 1).s().to_string();
            self.known.insert(v);
        }
        let body = self.expr()?;
        if !self.at_kw("for") {
            return Err(self.err(format!("expected 'for' in this sum (write: Σ(k² for k from 1 to 10)){}", self.found())));
        }
        self.next();
        let vt = self.expect_name("the summation variable (write: Σ(k² for k from 1 to 10))")?;
        self.expect_kw("from", Some("(write: Σ(k² for k from 1 to 10))"))?;
        let lo = self.expr()?;
        self.expect_kw("to", Some("(write: Σ(k² for k from 1 to 10))"))?;
        let hi = self.expr()?;
        let mut step = None;
        if self.at_kw("step") {
            self.next();
            step = Some(self.expr()?);
        }
        if !self.at_op(")") {
            return Err(self.err(format!("expected ')' to close this sum{}", self.found())));
        }
        self.next();
        let var = self.toks[vt].s().to_string();
        Ok(self.mks(ExprKind::Sum { body: bx(body), var, lo: bx(lo), hi: bx(hi), step: step.map(bx) }, t))
    }

    /// `0.300 /(m s²)` right after a number, with your own m: ask (D235).
    fn check_mixed_reciprocal(&self, num: &str) -> R<()> {
        if !(self.at_op("/") && self.at_op_at(self.i + 1, "(") && self.bracket_all_units(self.i + 1)) {
            return Ok(());
        }
        let Some(k) = self.match_(self.i + 1) else { return Ok(()) };
        let names: Vec<usize> = (self.i + 2..k).filter(|&m| self.toks[m].kind == Kind::Name).collect();
        let yours: Vec<usize> = names.iter().copied().filter(|&m| self.known.contains(self.toks[m].s())).collect();
        if yours.is_empty() || yours.len() == names.len() {
            return Ok(());
        }
        let text = self.unit_span_text(self.i, k);
        let v = self.toks[yours[0]].raw.clone();
        Err(self.error(
            format!("'{num} {text}' is ambiguous: right after a number, {text} is the unit 1{text}, but {v} is also your \
                     variable {v}"),
            Some(yours[0]),
            Some(format!("write  {num} [1{text}]  for the unit, or give the units their own number to divide by your {v}")),
        ))
    }

    /// `0.300 /(m s²)` right after a number: '/' then brackets holding only unit names and exponents, none of them
    /// your variable (gauntlet #72; D235).
    fn bracketed_reciprocal_unit(&self) -> bool {
        if !(self.at_op("/") && self.at_op_at(self.i + 1, "(")) {
            return false;
        }
        let Some(k) = self.match_(self.i + 1) else { return false };
        if k == self.i + 2 {
            return false;
        }
        if self.toks[self.i + 2].kind != Kind::Name {
            return false;
        }
        let mut prev: Option<usize> = None;
        for m in self.i + 2..k {
            let tk = &self.toks[m];
            match tk.kind {
                Kind::Name => {
                    if !self.unit_tok(m) || self.known.contains(tk.s()) {
                        return false;
                    }
                }
                Kind::Sup => {}
                Kind::Num => {
                    let ok = prev.is_some_and(|p| {
                        let pt = &self.toks[p];
                        pt.kind == Kind::Op && ["^", "-", "(", "/"].contains(&pt.s())
                    });
                    if !ok {
                        return false;
                    }
                }
                Kind::Op if ["^", "/", "*", "(", ")", "-"].contains(&tk.s()) => {}
                _ => return false,
            }
            prev = Some(m);
        }
        let nx = self.tk(k as i64 + 1);
        if k + 1 < self.toks.len() && nx.is_op("(") && !nx.ws_before {
            return false;
        }
        true
    }

    /// `0.1 1/s`, `0.1 /s` or `0.1/s` right after a number: a unit name after '/' that isn't your variable (D235).
    fn unit_reciprocal_follows(&self) -> bool {
        let t = self.tok();
        if t.kind == Kind::Num && t.f() == 1.0 && t.digit && self.peek(1).is_op("/") {
            let nx = self.peek(2);
            return nx.kind == Kind::Name
                && self.unit_tok((self.i + 2).min(self.toks.len() - 1))
                && !self.known.contains(nx.s());
        }
        if t.is_op("/")
            && self.peek(1).kind == Kind::Name
            && self.unit_tok((self.i + 1).min(self.toks.len() - 1))
            && !self.known.contains(self.peek(1).s())
            && !(self.peek(2).is_op("(") && !self.peek(2).ws_before)
        {
            return true;
        }
        false
    }

    /// <a, b>, <a, b, c> or <a, b, c, d>, optionally followed by a unit: <3, 4> m/s.
    fn vector_literal(&mut self) -> R<Expr> {
        let t = self.next();
        let mut items = vec![];
        loop {
            items.push(self.sum()?);
            if self.at_op(",") {
                self.next();
                continue;
            }
            break;
        }
        if !self.at_op(">") {
            return Err(self.err(format!("expected '>' to close this vector (written <x, y> or <x, y, z>){}",
                                        self.found())));
        }
        self.next();
        if !(2..=16).contains(&items.len()) {
            return Err(self.error(format!("a vector needs 2 to 16 components, not {}", items.len()), Some(t), None));
        }
        let mut v = self.mks(ExprKind::VecLit { items }, t);
        if self.kind() == Kind::Name && self.unit_tok(self.i) && !self.is_call_like() {
            let u = self.unit_expr(false, false)?;
            let single_known = u.factors.len() == 1 && self.known.contains(&u.factors[0].name);
            v = self.mks(ExprKind::Quantity { value: bx(v), unit: u, bracket: false }, t);
            if single_known {
                self.single_collision(&v, None, false, Some("<…>".into()))?;
            }
        } else if self.unit_reciprocal_follows() {
            let u = self.unit_expr(true, true)?;
            v = self.mks(ExprKind::Quantity { value: bx(v), unit: u, bracket: false }, t);
        }
        Ok(v)
    }

    fn in_index(&self) -> bool {
        let mut depth = 0;
        let mut j = self.i as i64 - 1;
        while j >= 0 {
            let tk = &self.toks[j as usize];
            if tk.is_op("]") {
                depth += 1;
            } else if tk.is_op("[") {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            } else if tk.kind == Kind::Newline {
                return false;
            }
            j -= 1;
        }
        false
    }

    pub fn is_call_like(&self) -> bool {
        let nx = self.peek(1);
        nx.is_op("(") && !nx.ws_before
    }

    // ------------------------------------------------------------ calculus syntax
    fn is_deriv_op(&self) -> bool {
        let mut j = self.i as i64 + 1;
        if self.tk(j).kind == Kind::Sup {
            j += 1;
        } else if self.tk(j).is_op("^") && self.tk(j + 1).kind == Kind::Num {
            j += 2;
        }
        if !self.tk(j).is_op("/") {
            return false;
        }
        let v = self.tk(j + 1);
        if v.kind != Kind::Name || !v.raw.starts_with('d') || v.raw.chars().count() < 2 {
            return false;
        }
        let mut k = j + 2;
        if self.tk(k).kind == Kind::Sup || self.tk(k).is_op("^") {
            k += if self.tk(k).kind == Kind::Sup { 1 } else { 2 };
        }
        let nxt = self.tk(k);
        matches!(nxt.kind, Kind::Name | Kind::Num)
            || (nxt.kind == Kind::Op && ["(", "|"].contains(&nxt.s()))
            || (nxt.kind == Kind::Kw && ["sqrt", "cbrt"].contains(&nxt.s()))
    }

    /// d²x/dt² or d^2x/dt^2 -> Deriv(t, 2, x); returns None if the tokens don't match.
    fn leibniz_higher(&mut self) -> R<Option<Expr>> {
        let mut j = self.i as i64 + 1;
        let order;
        if self.tk(j).kind == Kind::Sup {
            order = self.tk(j).int();
            j += 1;
        } else if self.tk(j).is_op("^")
            && self.tk(j + 1).kind == Kind::Num
            && self.tk(j + 1).f() == self.tk(j + 1).f().trunc()
            && !self.tk(j + 2).ws_before
        {
            order = self.tk(j + 1).f() as i64;
            j += 2;
        } else {
            return Ok(None);
        }
        if self.tk(j).kind != Kind::Name || self.tk(j).ws_before {
            return Ok(None);
        }
        let xname = j;
        if !self.tk(j + 1).is_op("/") {
            return Ok(None);
        }
        let v = self.tk(j + 2).clone();
        if v.kind != Kind::Name || !v.raw.starts_with('d') || v.raw.chars().count() < 2 {
            return Ok(None);
        }
        let mut k = j + 3;
        let o2;
        if self.tk(k).kind == Kind::Sup {
            o2 = self.tk(k).int();
            k += 1;
        } else if self.tk(k).is_op("^") && self.tk(k + 1).kind == Kind::Num {
            o2 = self.tk(k + 1).f() as i64;
            k += 2;
        } else {
            return Ok(None);
        }
        if o2 != order {
            return Err(self.error(format!("the orders don't match in this derivative (d{order}.../d...{o2})"),
                                  Some((j + 2) as usize), None));
        }
        let start = self.i;
        self.i = k as usize;
        let xt = self.tk(xname).clone();
        let x = self.name_node(xt.s(), Span { line: xt.line, col: xt.col, length: xt.rawlen as u32 });
        let var = canonical_name(&v.raw[1..]);
        Ok(Some(self.mks(ExprKind::Deriv { var, order, operand: bx(x), partial: false }, start)))
    }

    fn deriv_order(&mut self) -> R<i64> {
        if self.kind() == Kind::Sup {
            let s = self.next();
            return Ok(self.toks[s].int());
        }
        if self.at_op("^") {
            self.next();
            let n = self.next();
            let nt = &self.toks[n];
            let v = nt.f();
            if nt.kind != Kind::Num || v != v.trunc() || !(1.0..=10.0).contains(&v) {
                return Err(self.error("the order of a derivative must be a whole number like d²/dt²", Some(n), None));
            }
            return Ok(v as i64);
        }
        Ok(1)
    }

    fn deriv_op(&mut self) -> R<Expr> {
        let t = self.next();
        let order = self.deriv_order()?;
        self.expect_op("/", None)?;
        let v = self.next();
        let var = canonical_name(&self.toks[v].raw.chars().skip(1).collect::<String>());
        let o2 = self.deriv_order()?;
        if o2 != order {
            return Err(self.error(format!("the orders don't match: d{order}/d{var}{o2}"), Some(v), None));
        }
        let operand = self.deriv_operand(&var)?;
        Ok(self.mks(ExprKind::Deriv { var, order, operand: bx(operand), partial: false }, t))
    }

    /// The operand of d/ds or ∂/∂s: its variable counts as your variable for D7 (D231).
    fn deriv_operand(&mut self, var: &str) -> R<Expr> {
        let saved_known = self.known.clone();
        let saved_dv = self.deriv_vars.clone();
        self.known.insert(var.to_string());
        self.deriv_vars.insert(var.to_string());
        let r = self.power();
        self.known = saved_known;
        self.deriv_vars = saved_dv;
        r
    }

    fn partial_op(&mut self) -> R<Expr> {
        let t = self.next();
        let order = self.deriv_order()?;
        if self.kind() == Kind::Name && self.peek(1).is_op("/") && self.peek(2).is_kw("partial") {
            let fnt = self.next();
            self.next();
            self.next();
            let v = self.expect_name("the variable to differentiate by")?;
            let o2 = self.deriv_order()?;
            let (fv, vv) = (self.toks[fnt].s().to_string(), self.toks[v].s().to_string());
            if o2 != order && o2 != 1 {
                return Err(self.error(format!("the orders don't match: ∂{order}{fv}/∂{vv}{o2}"), Some(v), None));
            }
            let fname = self.mks(ExprKind::Name { name: fv }, fnt);
            return Ok(self.mks(ExprKind::Deriv { var: vv, order, operand: bx(fname), partial: true }, t));
        }
        self.expect_op("/", Some("(write ∂/∂x f)"))?;
        self.expect_kw("partial", Some("(write ∂/∂x f)"))?;
        let v = self.expect_name("the variable to differentiate by")?;
        let o2 = self.deriv_order()?;
        let vv = self.toks[v].s().to_string();
        if o2 != order && o2 != 1 {
            return Err(self.error(format!("the orders don't match: ∂{order}/∂{vv}{o2}"), Some(v), None));
        }
        let operand = self.deriv_operand(&vv)?;
        Ok(self.mks(ExprKind::Deriv { var: vv, order, operand: bx(operand), partial: true }, t))
    }

    fn integral(&mut self) -> R<Expr> {
        let t = self.next();
        let new: HashSet<String> = self.integration_vars().difference(&self.known).cloned().collect();
        self.known.extend(new.iter().cloned());
        let saved_limit = self.limit_start.take();
        self.in_integrand += 1;
        let r = (|| -> R<(Expr, Option<String>)> {
            let body = self.sum()?;
            Ok(self.split_dvar(body))
        })();
        for n in &new {
            self.known.remove(n);
        }
        self.limit_start = saved_limit;
        self.in_integrand -= 1;
        let (integrand, var) = r?;
        let Some(var) = var else {
            return Err(self.error("this integral is missing its 'dx' (the variable to integrate over)", Some(t),
                                  Some("write e.g.  ∫ F(x) dx from 0 m to 1 m".into())));
        };
        if self.at_kw("from") {
            self.next();
            let lo = self.sum()?;
            self.expect_kw("to", None)?;
            let saved = self.limit_start.replace(self.i);
            let hi_start = self.i;
            let hi = self.sum();
            self.limit_start = saved;
            let hi = hi?;
            let sum_info = self.sum_in_limit(hi_start, &hi);
            let mut div_tok = None;
            if self.at_op("/") && self.tok().ws_before && self.divisor_follows() && !self.limit_is_infinite(hi_start) {
                div_tok = Some(self.i);
            }
            let mut node = self.mks(
                ExprKind::Integral { integrand: bx(integrand), var, lo: Some(bx(lo)), hi: Some(bx(hi)) },
                t,
            );
            node.attrs.sum_info = Some(sum_info.map(Box::new));
            if let Some(dt) = div_tok {
                let tk = &self.toks[dt];
                node.attrs.div_info = Some(Box::new(DivInfo {
                    tok: Some((tk.line, tk.col)),
                    tok_i: Some(dt),
                    hi_text: Some(self.text(hi_start, self.i)),
                    ..Default::default()
                }));
            }
            return Ok(node);
        }
        Ok(self.mks(ExprKind::Integral { integrand: bx(integrand), var, lo: None, hi: None }, t))
    }

    /// `2 ∫ x dx from 0 to 1 - π`: a spaced binary + or - at the top level of the upper limit (D173, D205).
    fn sum_in_limit(&self, start: usize, hi: &Expr) -> Option<SumInfo> {
        let (mut depth, mut bars) = (0i64, 0i64);
        for j in start..self.i {
            let tk = &self.toks[j];
            if tk.kind == Kind::Op && "([{".contains(tk.s()) {
                depth += 1;
            } else if tk.kind == Kind::Op && ")]}".contains(tk.s()) {
                depth -= 1;
            } else if tk.is_op("|") {
                bars += 1;
            } else if tk.kind == Kind::Op
                && (tk.s() == "+" || tk.s() == "-")
                && j > start
                && depth == 0
                && bars % 2 == 0
                && tk.ws_before
                && j + 1 < self.i
                && self.toks[j + 1].ws_before
            {
                let pv = &self.toks[j - 1];
                if pv.kind == Kind::Op && ![")", "]", "}", "|"].contains(&pv.s()) {
                    continue;
                }
                let mut split: Option<&Expr> = None;
                let mut n = hi;
                while let ExprKind::BinOp { op, left, implicit: false, .. } = &n.kind {
                    if !((op == "+" || op == "-") && !n.paren) {
                        break;
                    }
                    split = Some(n);
                    n = left;
                }
                return Some(SumInfo {
                    tok: (tk.line, tk.col),
                    limit: self.text(start, self.i),
                    rest: self.text(j, self.i),
                    head: self.text(start, j),
                    op: tk.s().to_string(),
                    head_node: split.map(|s| binop_left(s).unwrap().noderef()),
                    rest_node: split.map(|s| binop_right(s).unwrap().noderef()),
                });
            }
        }
        None
    }

    /// After a spaced '/' that ended an upper limit: is a divisor next? (FRICTION #8, #61)
    fn divisor_follows(&self) -> bool {
        let nx = self.peek(1);
        matches!(nx.kind, Kind::Num | Kind::Name)
            || (nx.kind == Kind::Op && ["(", "[", "|"].contains(&nx.s()))
            || (nx.kind == Kind::Kw && ["sqrt", "cbrt"].contains(&nx.s()))
    }

    /// Is the upper limit starting at token `start` ±∞ (possibly with a unit)?
    fn limit_is_infinite(&self, start: usize) -> bool {
        let mut j = start;
        while j < self.i && self.toks[j].kind == Kind::Op && "+-".contains(self.toks[j].s()) {
            j += 1;
        }
        j < self.i && self.toks[j].is_name("∞")
    }

    /// At '/': does it end an integral's upper limit? (FRICTION #8, D34)
    fn ends_upper_limit(&self) -> bool {
        let Some(ls) = self.limit_start else { return false };
        if !self.tok().ws_before {
            return false;
        }
        let (mut depth, mut bars) = (0i64, 0i64);
        for j in ls..self.i {
            let tk = &self.toks[j];
            if tk.kind == Kind::Op && "([{".contains(tk.s()) {
                depth += 1;
            } else if tk.kind == Kind::Op && ")]}".contains(tk.s()) {
                depth -= 1;
            } else if tk.is_op("|") {
                bars += 1;
            }
        }
        depth == 0 && bars % 2 == 0
    }

    /// Names v of the `dv` tokens that end the integrand starting at the current token.
    fn integration_vars(&self) -> HashSet<String> {
        let (mut depth, mut j) = (0i64, self.i);
        let mut last: Vec<String> = vec![];
        while j < self.toks.len() {
            let t = &self.toks[j];
            if matches!(t.kind, Kind::Newline | Kind::Eof)
                || (depth == 0 && t.kind == Kind::Kw)
                || (depth == 0 && t.kind == Kind::Op && [",", "="].contains(&t.s()))
            {
                break;
            }
            if t.kind == Kind::Op && "([{".contains(t.s()) {
                depth += 1;
            } else if t.kind == Kind::Op && ")]}".contains(t.s()) {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            if depth == 0 && t.kind == Kind::Name && t.s().starts_with('d') && t.s().chars().count() > 1 {
                last.push(canonical_name(&t.s()[1..]));
            } else if !(t.kind == Kind::Op && ")]}".contains(t.s())) {
                last.clear();
            }
            j += 1;
        }
        last.into_iter().collect()
    }

    /// Pull a trailing `dx` out of the integrand: `F(x) dx` -> (F(x), 'x').
    fn split_dvar(&mut self, e: Expr) -> (Expr, Option<String>) {
        let dname = |n: &Expr| -> Option<String> {
            n.name().filter(|nm| nm.starts_with('d') && nm.chars().count() > 1).map(|nm| canonical_name(&nm[1..]))
        };
        if !e.paren {
            if let Some(v) = dname(&e) {
                let sp = e.span;
                return (self.mk(ExprKind::Num { value: 1.0, sigfigs: None, digit: false }, sp), Some(v));
            }
        }
        if let ExprKind::Quantity { value, unit, bracket } = &e.kind {
            if !*bracket && !e.paren && !unit.factors.is_empty() {
                let f = unit.factors.last().unwrap();
                if *f.exp.numer() == 1 && *f.exp.denom() == 1
                    && f.name.chars().count() > 1
                    && f.name.starts_with('d')
                    && is_unit_name(&f.name[1..])
                    && unit.text.ends_with(&f.name)
                {
                    let var = canonical_name(&f.name[1..]);
                    if unit.factors.len() == 1 {
                        return ((**value).clone(), Some(var));
                    }
                    let text = unit.text[..unit.text.len() - f.name.len()].trim_end().to_string();
                    let mut uspan = unit.span;
                    if uspan.length == 0 {
                        uspan.length = 1;
                    }
                    let u = UnitExpr { factors: unit.factors[..unit.factors.len() - 1].to_vec(), text, span: uspan,
                                       juxt_join: None };
                    let sp = e.span;
                    let q = self.mk(ExprKind::Quantity { value: value.clone(), unit: u, bracket: *bracket }, sp);
                    return (q, Some(var));
                }
            }
        }
        if !e.paren {
            if let ExprKind::Neg { .. } = &e.kind {
                let sp = e.span;
                let ExprKind::Neg { operand } = e.kind else { unreachable!() };
                let op_clone = (*operand).clone();
                let (inner, var) = self.split_dvar(*operand);
                if var.is_some() {
                    let n = self.mk(ExprKind::Neg { operand: bx(inner) }, sp);
                    return (n, var);
                }
                // unchanged: rebuild the original node (same identity)
                let orig = Expr { id: e.id, kind: ExprKind::Neg { operand: bx(op_clone) }, span: sp, paren: e.paren,
                                  attrs: e.attrs };
                return (orig, None);
            }
        }
        if !e.paren {
            if let ExprKind::BinOp { op, left, right, implicit } = &e.kind {
                if *implicit {
                    if let Some(v) = dname(right) {
                        return ((**left).clone(), Some(v));
                    }
                }
                if ["+", "-", "*", "/"].contains(&op.as_str()) || *implicit {
                    let (inner, var) = self.split_dvar((**right).clone());
                    if var.is_some() {
                        let sp = e.span;
                        let n = self.mk(ExprKind::BinOp { op: op.clone(), left: left.clone(), right: bx(inner),
                                                          implicit: *implicit }, sp);
                        return (n, var);
                    }
                }
            }
        }
        (e, None)
    }
}
