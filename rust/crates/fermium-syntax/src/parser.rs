//! Parser: tokens -> AST. A port of `fermium/parser.py` (Fermium 1.5, the oracle), method by method with the
//! same names (without the leading underscore), so the two can be compared side by side.
//!
//! Precedence, lowest to highest (see DECISIONS.md, "Implicit multiplication"):
//!
//! ```text
//! expr_full  := expr ['in' unit]                 conversion for display
//! expr       := 'if' expr 'then' expr 'else' expr | or
//! or         := and ('or' and)*
//! and        := not ('and' not)*
//! not        := 'not' not | compare
//! compare    := sum [cmp_op sum]
//! sum        := product (('+' | '-') product)*
//! product    := unary (('*' | '/') unary)*         explicit * and /
//! unary      := '-' unary | '+' unary | juxt
//! juxt       := power power*                       implicit multiplication binds TIGHTER than / and *
//! power      := postfix ['^' exponent | superscript]   (right-assoc)
//! postfix    := atom ( '(' args ')' | '[' index ']' | '.' name | ' )*   (no space before ( and [)
//! atom       := number [unit] | name | string | '(' expr_full ')' | '[' list ']' | '|' expr '|'
//!             | √ power | ∫ ... | d/dt power | ∂/∂x power | load "file" | true | false
//! ```
//!
//! So `h c / λ k_B T` = (h c)/(λ k_B T), and `1/2 m v²` = 1/(2 m v²) (with a warning).
//!
//! Tokens are referred to by their index in `toks` (the Python parser holds the token objects).

use std::collections::HashSet;

use crate::ast::*;
use crate::diag::{Diagnostic, Diagnostics, Fix};
use crate::lexer::{Kind, Token};

pub type R<T> = Result<T, Diagnostic>;

pub const CMP_OPS: &[&str] = &["==", "!=", "<", ">", "<=", ">=", "~="];
pub const AUG_OPS: &[&str] = &["+=", "-=", "*=", "/="];

/// keywords that are also the ASCII spelling of a symbol: `integral(b, T) = …` gets a clear error (#78)
pub fn keyword_spelled(kw: &str) -> Option<&'static str> {
    Some(match kw {
        "integral" => "∫",
        "partial" => "∂",
        "sqrt" => "√",
        "cbrt" => "∛",
        "nabla" => "∇",
        _ => return None,
    })
}

pub fn vec_calc_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "grad" => "grad",
        "div" => "div",
        "curl" => "curl",
        "laplacian" => "lap",
        _ => return None,
    })
}

pub fn unit_words(name: &str) -> Option<&'static str> {
    Some(match name {
        "g" => "grams",
        "m" => "metres",
        "s" => "seconds",
        "L" => "litres",
        "l" => "litres",
        "V" => "volts",
        "T" => "tesla",
        "b" => "barns",
        "A" => "amperes",
        "K" => "kelvin",
        "N" => "newtons",
        "J" => "joules",
        "W" => "watts",
        "C" => "coulombs",
        "F" => "farads",
        "H" => "henries",
        "Pa" => "pascals",
        "u" => "atomic mass units",
        "c" => "the speed of light",
        "d" => "days",
        "min" => "minutes",
        "yr" => "years",
        "h" => "hours",
        "t" => "tonnes",
        "au" => "AU",
        "pc" => "parsecs",
        _ => return None,
    })
}

const NATURAL_CONSTANTS: &[&str] = &["ħ", "hbar", "c", "k_B", "kB", "G", "ε₀", "ε_0", "epsilon_0", "e", "μ₀", "μ_0"];

pub fn set_of(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub struct Parser {
    pub toks: Vec<Token>,
    pub i: usize,
    pub diags: Diagnostics,
    /// names assigned so far (for unit/variable collisions)
    pub known: HashSet<String>,
    /// names bound by `import m [as a]` (for the hint on `m.x = …`)
    pub module_names: HashSet<String>,
    pub abs_depth: i64,
    pub no_juxt_names: HashSet<String>,
    pub warned_units: HashSet<String>,
    pub in_integrand: i64,
    /// token index where an integral's upper limit starts (FRICTION #8)
    pub limit_start: Option<usize>,
    /// every name the program assigns, defines or loops over (D215)
    pub named: Option<HashSet<String>>,
    /// variables of the d/ds, ∂/∂s whose operand is being parsed (D231)
    pub deriv_vars: HashSet<String>,
    /// the unknowns of the solve being parsed: names written with a prime (D211)
    pub solve_unknowns: HashSet<String>,
    pub solve_independents: HashSet<String>,
    /// `fmt --fix`: rewrite unit/variable collisions instead of stopping (D235)
    pub fix_mode: bool,
    /// fix mode: (start, end, replacement) edits in the source
    pub fixes: Vec<Fix>,
    /// fix mode: units to bracket whole once read (v1 read on through a collision)
    pub fix_whole: Vec<usize>,
    pub stmt_done: bool,
    pub chain_count: u32,
    pub(crate) next_id: u32,
    /// Nesting depth of the recursive descent: deeper than MAX_DEPTH is an error, not a stack overflow.
    pub depth: u32,
}

/// Fermium 1.5 stops at Python's recursion limit (20000 frames: about 1300 nested brackets); the port stops at
/// a similar depth (PARITY.md). The parser runs on a thread with a large stack (`with_big_stack`).
pub const MAX_DEPTH: u32 = 5000;

impl Parser {
    pub fn new(tokens: Vec<Token>, diags: Diagnostics, known: &[String]) -> Self {
        Parser {
            toks: tokens,
            i: 0,
            diags,
            known: known.iter().cloned().collect(),
            module_names: HashSet::new(),
            abs_depth: 0,
            no_juxt_names: HashSet::new(),
            warned_units: HashSet::new(),
            in_integrand: 0,
            limit_start: None,
            named: None,
            deriv_vars: HashSet::new(),
            solve_unknowns: HashSet::new(),
            solve_independents: HashSet::new(),
            fix_mode: false,
            fixes: vec![],
            fix_whole: vec![],
            stmt_done: false,
            chain_count: 0,
            next_id: 1,
            depth: 0,
        }
    }

    // ------------------------------------------------------------ helpers
    /// Run a recursive step with the nesting depth checked.
    pub fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.depth >= MAX_DEPTH {
            // v1's message and hint (driver.run_source; red team 13 #10)
            return Err(Diagnostic { message: "this program is nested too deeply for Fermium to compile (very long or \
                                              deeply nested expressions)".into(),
                                    line: None, col: None, length: 1,
                                    hint: Some("split the expression into several lines with names".into()),
                                    severity: crate::diag::Severity::Error, fix: vec![] });
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    pub fn tok(&self) -> &Token {
        &self.toks[self.i]
    }

    /// The token at index j, Python style: negative indexes count from the end; past the end gives the last (EOF).
    pub fn tk(&self, j: i64) -> &Token {
        let n = self.toks.len() as i64;
        let k = if j < 0 { (j + n).max(0) } else { j.min(n - 1) };
        &self.toks[k as usize]
    }

    pub fn peek(&self, k: usize) -> &Token {
        let j = (self.i + k).min(self.toks.len() - 1);
        &self.toks[j]
    }

    pub fn next(&mut self) -> usize {
        let t = self.i;
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }

    pub fn at_op(&self, op: &str) -> bool {
        self.tok().is_op(op)
    }

    pub fn at_ops(&self, ops: &[&str]) -> bool {
        self.tok().kind == Kind::Op && ops.contains(&self.tok().s())
    }

    pub fn at_kw(&self, kw: &str) -> bool {
        self.tok().is_kw(kw)
    }

    pub fn kind(&self) -> Kind {
        self.tok().kind
    }

    pub fn at_kind(&self, kinds: &[Kind]) -> bool {
        kinds.contains(&self.tok().kind)
    }

    pub fn expect_op(&mut self, op: &str, what: Option<&str>) -> R<usize> {
        if !self.at_op(op) {
            let w = what.map(|w| format!(" {w}")).unwrap_or_default();
            return Err(self.err(format!("expected '{op}'{w}{}", self.found())));
        }
        Ok(self.next())
    }

    pub fn expect_kw(&mut self, kw: &str, what: Option<&str>) -> R<usize> {
        if !self.at_kw(kw) {
            let w = what.map(|w| format!(" {w}")).unwrap_or_default();
            return Err(self.err(format!("expected '{kw}'{w}{}", self.found())));
        }
        Ok(self.next())
    }

    pub fn expect_name(&mut self, what: &str) -> R<usize> {
        if self.kind() != Kind::Name {
            return Err(self.err(format!("expected {what}{}", self.found())));
        }
        Ok(self.next())
    }

    pub fn found(&self) -> String {
        let t = self.tok();
        match t.kind {
            Kind::Newline => " but the line ended".into(),
            Kind::Eof => " but the program ended".into(),
            Kind::Indent | Kind::Dedent => " but the indentation changed".into(),
            _ => format!(" but found '{}'", t.raw),
        }
    }

    /// `self.error(msg, tok=None, hint=None)`.
    pub fn error(&self, msg: impl Into<String>, tok: Option<usize>, hint: Option<String>) -> Diagnostic {
        let msg = msg.into();
        if tok.is_none() && matches!(self.tok().kind, Kind::Eof | Kind::Newline | Kind::Dedent) {
            let mut j = self.i as i64 - 1;
            while j > 0 && matches!(self.tk(j).kind, Kind::Eof | Kind::Newline | Kind::Dedent | Kind::Indent) {
                j -= 1;
            }
            let prev = self.tk(j);
            return Diagnostic::error(msg, prev.line, prev.col + prev.rawlen as u32, 1, hint);
        }
        let t = &self.toks[tok.unwrap_or(self.i)];
        Diagnostic::error(msg, t.line, t.col, (t.rawlen as u32).max(1), hint)
    }

    pub fn err(&self, msg: impl Into<String>) -> Diagnostic {
        self.error(msg, None, None)
    }

    pub fn err_h(&self, msg: impl Into<String>, hint: impl Into<String>) -> Diagnostic {
        self.error(msg, None, Some(hint.into()))
    }

    /// The span from token t to the previous token (`self.span(n, tok)`).
    pub fn span_from(&self, t: usize) -> Span {
        let tok = &self.toks[t];
        let prev = self.tk(self.i as i64 - 1);
        let length = if prev.line == tok.line {
            (prev.end as i64 - tok.start as i64).max(1)
        } else {
            (tok.rawlen as i64).max(1)
        };
        Span { line: tok.line, col: tok.col, length: length as u32 }
    }

    /// `node.at(tok)`: the token's position and length.
    pub fn tok_span(&self, t: usize) -> Span {
        let tok = &self.toks[t];
        Span { line: tok.line, col: tok.col, length: if tok.rawlen == 0 { 1 } else { tok.rawlen as u32 } }
    }

    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// A new node.
    pub fn mk(&mut self, kind: ExprKind, span: Span) -> Expr {
        Expr { id: self.new_id(), kind, span, paren: false, attrs: Attrs::default() }
    }

    /// A new node spanning from token t to the previous token.
    pub fn mks(&mut self, kind: ExprKind, t: usize) -> Expr {
        let sp = self.span_from(t);
        self.mk(kind, sp)
    }

    pub fn name_node(&mut self, name: &str, span: Span) -> Expr {
        self.mk(ExprKind::Name { name: name.to_string() }, span)
    }

    pub fn skip_newlines(&mut self) {
        while self.kind() == Kind::Newline {
            self.next();
        }
    }

    pub fn at_op_at(&self, j: usize, value: &str) -> bool {
        j < self.toks.len() && self.toks[j].is_op(value)
    }

    /// Source text of tokens i..j-1.
    pub fn text(&self, i: usize, j: usize) -> String {
        let mut parts = String::new();
        for k in i..j.min(self.toks.len()) {
            let tk = &self.toks[k];
            if !parts.is_empty() && tk.ws_before {
                parts.push(' ');
            }
            parts.push_str(&tk.raw);
        }
        parts
    }

    /// Index of the bracket matching the one at j.
    pub fn match_(&self, j: usize) -> Option<usize> {
        let mut depth = 0;
        let opening = self.toks[j].s().to_string();
        let closing = match opening.as_str() {
            "(" => ")",
            "[" => "]",
            "{" => "}",
            _ => return None,
        };
        let mut j = j;
        while j < self.toks.len() {
            let t = &self.toks[j];
            if t.is_op(&opening) {
                depth += 1;
            } else if t.is_op(closing) {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            } else if t.kind == Kind::Eof {
                return None;
            }
            j += 1;
        }
        None
    }

    // ------------------------------------------------------------ program
    pub fn parse_program(&mut self) -> R<Program> {
        let mut body = vec![];
        self.skip_newlines();
        while self.kind() != Kind::Eof {
            if self.kind() == Kind::Indent {
                return Err(self.err_h("this line is indented but isn't inside a block",
                                      "remove the spaces at the start of the line"));
            }
            if self.kind() == Kind::Dedent {
                self.next();
                continue;
            }
            body.push(self.statement(true)?);
            self.skip_newlines();
        }
        Ok(Program { body })
    }

    /// NEWLINE INDENT stmt* DEDENT, or a single statement on the same line after ':'.
    pub fn block(&mut self) -> R<Vec<Stmt>> {
        if self.at_op(":") {
            self.next();
        }
        if self.kind() != Kind::Newline {
            return Ok(vec![self.statement(false)?]);
        }
        self.next();
        self.skip_newlines();
        if self.kind() != Kind::Indent {
            return Err(self.err_h("expected an indented block here",
                                  "indent the lines that belong to this block (e.g. 4 spaces)"));
        }
        self.next();
        let mut stmts = vec![];
        while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
            stmts.push(self.statement(true)?);
            self.skip_newlines();
        }
        if self.kind() == Kind::Dedent {
            self.next();
        }
        Ok(stmts)
    }

    pub fn end_statement(&mut self) -> R<()> {
        if self.stmt_done {
            self.stmt_done = false;
            return Ok(());
        }
        match self.kind() {
            Kind::Newline => {
                self.next();
            }
            Kind::Eof | Kind::Dedent => {}
            _ if self.at_op(";") => {
                self.next();
            }
            _ => {
                let t = self.i;
                let tt = self.tok().clone();
                let mut hint = None;
                if tt.is_op("+-") {
                    return Err(self.err("± needs a value on its left, like  L = 1.20 ± 0.01 m"));
                }
                if tt.is_op("=") {
                    hint = Some("use == to compare two values; = stores a value in a variable".to_string());
                    if let Some(chain) = self.chained_assignment(t) {
                        return Err(chain);
                    }
                }
                if tt.kind == Kind::Op && (tt.s() == "+" || tt.s() == "-") && self.peek(1).kind == Kind::Op
                    && self.peek(1).s() == tt.s()
                {
                    hint = Some(format!("Fermium has no {0}{0}; write  x {0}= 1", tt.s()));
                }
                return Err(self.error(format!("didn't expect '{}' here", tt.raw), None, hint));
            }
        }
        Ok(())
    }

    /// `ħ = c = 1` (spec A3.1): setting constants to 1 is natural units; `a = b = 1` assigns one at a time.
    fn chained_assignment(&self, t: usize) -> Option<Diagnostic> {
        let tline = self.toks[t].line;
        let line: Vec<usize> = (0..self.toks.len())
            .filter(|&k| {
                let tk = &self.toks[k];
                tk.line == tline && !matches!(tk.kind, Kind::Newline | Kind::Indent | Kind::Dedent | Kind::Eof)
            })
            .collect();
        let mut names = vec![];
        let mut j = 0;
        while j + 1 < line.len() && self.toks[line[j]].kind == Kind::Name && self.toks[line[j + 1]].is_op("=") {
            names.push(line[j]);
            j += 2;
        }
        if names.len() < 2 || !line[..j].contains(&t) {
            return None;
        }
        let mut text = names.iter().map(|&n| self.toks[n].raw.clone()).collect::<Vec<_>>().join(" = ") + " = ";
        for (k, &tk) in line[j..].iter().enumerate() {
            let tk = &self.toks[tk];
            if tk.ws_before && k > 0 {
                text.push(' ');
            }
            text.push_str(&tk.raw);
        }
        if names.iter().all(|&n| {
            NATURAL_CONSTANTS.contains(&self.toks[n].raw.as_str()) || NATURAL_CONSTANTS.contains(&self.toks[n].s())
        }) {
            return Some(self.error(
                format!("'{text}' sets physical constants to 1: that's natural units"),
                Some(t),
                Some(format!("write  units natural({text})  (or just  units natural  for ħ = c = 1)")),
            ));
        }
        let ex: Vec<String> = names[..2].iter().map(|&n| format!("{} = …", self.toks[n].raw)).collect();
        Some(self.error(
            format!("'{text}': Fermium gives one variable a value at a time"),
            Some(t),
            Some(format!("write each on its own line, like  {}  (== compares two values)", ex.join("  and  "))),
        ))
    }

    fn stmt(&self, kind: StmtKind, t: usize) -> Stmt {
        Stmt { kind, span: self.span_from(t) }
    }

    // ------------------------------------------------------------ statements
    pub fn statement(&mut self, end_line: bool) -> R<Stmt> {
        self.nested(|p| p.statement_(end_line))
    }

    fn statement_(&mut self, end_line: bool) -> R<Stmt> {
        let t = self.i;
        let tt = self.tok().clone();
        let nx = self.peek(1).clone();
        if tt.kind == Kind::Name && ["def", "function", "fn"].contains(&tt.s()) && nx.kind == Kind::Name {
            let nm = nx.raw.clone();
            return Err(self.err_h(
                format!("Fermium doesn't use '{}': a function is written like a formula", tt.raw),
                format!("write  {nm}(x) = 2 x   (or put the body on the indented lines after {nm}(x) =)"),
            ));
        }
        if tt.kind == Kind::Kw && nx.kind == Kind::Op && (nx.s() == "=" || AUG_OPS.contains(&nx.s()))
            && tt.s() != "print"
        {
            return Err(self.err_h(
                format!("'{}' is a reserved word in Fermium, so it can't be a variable name", tt.raw),
                format!("pick another name, e.g. {0}_ or my_{0}", tt.raw),
            ));
        }
        if tt.kind == Kind::Kw {
            let kw = tt.s();
            if kw == "break" || kw == "continue" {
                self.next();
                let s = self.stmt(if kw == "break" { StmtKind::Break } else { StmtKind::Continue }, t);
                if end_line {
                    self.end_statement()?;
                }
                return Ok(s);
            }
            let s = match kw {
                "print" => Some(self.print_stmt()?),
                "plot" => Some(self.plot_stmt()?),
                "solve" => Some(self.solve_stmt()?),
                "fit" => Some(self.fit_stmt()?),
                "if" => Some(self.if_stmt()?),
                "for" => Some(self.for_stmt()?),
                "while" => Some(self.while_stmt()?),
                "return" => Some(self.return_stmt()?),
                "assert" => Some(self.assert_stmt()?),
                _ => None,
            };
            if let Some(s) = s {
                let compound = matches!(
                    s.kind,
                    StmtKind::If { .. } | StmtKind::For { .. } | StmtKind::ForIn { .. } | StmtKind::While { .. }
                        | StmtKind::Solve(_)
                );
                if end_line && !compound {
                    self.end_statement()?;
                }
                return Ok(s);
            }
        }
        if tt.is_name("import") && (nx.is_name("c") || nx.is_name("fortran")) && self.peek(2).kind == Kind::Str {
            return self.import_c_stmt(end_line); // import c "libphys.so": … (C3, D275)
        }
        if (tt.is_name("import") || tt.is_kw("from")) && matches!(nx.kind, Kind::Name | Kind::Str) {
            let s = self.import_stmt()?;
            if end_line {
                self.end_statement()?;
            }
            return Ok(s);
        }
        if tt.is_name("use") && nx.is_name("python") {
            return self.use_python_stmt(end_line);
        }
        if tt.is_name("parallel") && nx.is_kw("for") {
            self.next();
            let mut s = self.for_stmt()?;
            if let StmtKind::ForIn { .. } = s.kind {
                return Err(self.error(
                    "parallel for works with a range of numbers: write  parallel for i from 1 to n",
                    Some(t),
                    Some("loop over the indexes: parallel for i from 1 to len(xs), then use xs[i]".into()),
                ));
            }
            if let StmtKind::For { parallel, .. } = &mut s.kind {
                *parallel = true;
            }
            s.span.length = if s.span.line == tt.line { (s.span.col - tt.col) + 3 } else { 8 };
            s.span.col = tt.col;
            return Ok(s);
        }
        if tt.is_name("analyze") && self.is_analyze() {
            let s = self.analyze_stmt()?;
            if end_line {
                self.end_statement()?;
            }
            return Ok(s);
        }
        if tt.is_name("propagate") && nx.kind == Kind::Name && ["montecarlo", "monte_carlo", "MonteCarlo"].contains(&nx.s())
        {
            return self.propagate_stmt();
        }
        if tt.is_name("units") && nx.kind == Kind::Name && ["natural", "nuclear", "astro", "SI"].contains(&nx.s()) {
            return self.units_stmt(end_line);
        }
        if tt.kind == Kind::Name {
            if nx.is_op("=") {
                let s = self.assign_stmt()?;
                if end_line {
                    self.end_statement()?;
                }
                return Ok(s);
            }
            if nx.kind == Kind::Op && AUG_OPS.contains(&nx.s()) {
                let name = self.next();
                let op = self.next();
                let op = self.toks[op].s().to_string();
                let val = self.expr_where()?;
                let s = self.stmt(StmtKind::Assign { name: self.toks[name].s().to_string(), value: val, op }, name);
                if end_line {
                    self.end_statement()?;
                }
                return Ok(s);
            }
            if nx.is_op("(") && !nx.ws_before && self.is_funcdef() {
                return self.funcdef();
            }
            if nx.is_op("[") && !nx.ws_before {
                let j = self.match_(self.i + 1);
                if let Some(j) = j {
                    let after = self.tk(j as i64 + 1);
                    if after.kind == Kind::Op && (after.s() == "=" || AUG_OPS.contains(&after.s())) {
                        let name = self.next();
                        self.next();
                        let idx = self.expr()?;
                        let mut idx2 = None;
                        if self.at_op(",") {
                            self.next();
                            idx2 = Some(self.expr()?);
                        }
                        if self.at_op(":") {
                            return Err(self.err("a slice xs[a:b] can be read but not assigned to; set the elements \
                                                 one at a time, like  for i from a to b  then  xs[i] = ..."));
                        }
                        self.expect_op("]", None)?;
                        let op = self.next();
                        let op = self.toks[op].s().to_string();
                        let val = self.expr_where()?;
                        let s = self.stmt(
                            StmtKind::IndexAssign {
                                target: self.toks[name].s().to_string(),
                                index: idx,
                                value: val,
                                op,
                                index2: idx2,
                            },
                            name,
                        );
                        if end_line {
                            self.end_statement()?;
                        }
                        return Ok(s);
                    }
                }
            }
        }
        if tt.kind == Kind::Kw
            && keyword_spelled(tt.s()).is_some()
            && tt.raw == tt.s()
            && (nx.is_op("=") || (self.at_op_at(self.i + 1, "(") && !nx.ws_before && self.is_funcdef()))
        {
            return Err(self.keyword_as_name(t));
        }
        let e = self.expr_where()?;
        if self.at_op("=") {
            return Err(self.assign_to_non_name(t, &e));
        }
        let s = self.stmt(StmtKind::ExprStmt { value: e }, t);
        if end_line {
            self.end_statement()?;
        }
        Ok(s)
    }

    /// propagate montecarlo [N [samples]] + an indented block (or ':' and one formula) (D123).
    fn propagate_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        self.next();
        let mut n = None;
        if !(self.at_op(":") || self.at_kind(&[Kind::Newline, Kind::Eof])) {
            if self.kind() == Kind::Num || (self.kind() == Kind::Name && self.peek(1).kind == Kind::Name) {
                let k = self.next();
                let kt = self.toks[k].clone();
                let sp = self.tok_span(k);
                n = Some(if kt.kind == Kind::Num {
                    self.mk(ExprKind::Num { value: kt.f(), sigfigs: kt.sigfigs, digit: true }, sp)
                } else {
                    self.name_node(kt.s(), sp)
                });
            } else {
                n = Some(self.sum()?);
            }
            if self.kind() == Kind::Name && ["samples", "sample"].contains(&self.tok().s()) {
                self.next();
            }
        }
        if !(self.at_op(":") || self.kind() == Kind::Newline) {
            return Err(self.err("write  propagate montecarlo 100000 samples  and put the formulas on the indented \
                                 lines below it"));
        }
        let body = self.block()?;
        Ok(self.stmt(StmtKind::Propagate { samples: n, body }, t))
    }

    /// units natural(ħ = c = 1) | units nuclear | units astro | units SI, optionally with ':' + a block (D60).
    fn units_stmt(&mut self, end_line: bool) -> R<Stmt> {
        let t = self.next();
        let sys = self.next();
        let system = self.toks[sys].s().to_string();
        let mut consts = vec![];
        if self.at_op("(") {
            self.next();
            loop {
                let first = self.expect_name("a constant, like ħ or c")?;
                let mut names = vec![self.toks[first].s().to_string()];
                loop {
                    self.expect_op("=", Some("(write it like  units natural(ħ = c = 1))"))?;
                    if self.kind() == Kind::Num {
                        break;
                    }
                    let nm = self.expect_name("a constant, like ħ or c")?;
                    names.push(self.toks[nm].s().to_string());
                }
                let num = self.next();
                if self.toks[num].f() != 1.0 {
                    return Err(self.error("natural units set constants to 1, like  units natural(ħ = c = 1)",
                                          Some(num), None));
                }
                consts.extend(names);
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                break;
            }
            self.expect_op(")", None)?;
        }
        let mut s = self.stmt(StmtKind::Units { system, consts, body: None }, t);
        if self.at_op(":") {
            let b = self.block()?;
            if let StmtKind::Units { body, .. } = &mut s.kind {
                *body = Some(b);
            }
        } else if end_line {
            self.end_statement()?;
        }
        Ok(s)
    }

    /// `integral(b, T) = …`: integral is the ASCII spelling of ∫ (gauntlet #78).
    pub fn keyword_as_name(&self, t: usize) -> Diagnostic {
        let tt = &self.toks[t];
        let sym = keyword_spelled(tt.s()).unwrap_or("");
        let what = if !sym.is_empty() { format!("the ASCII spelling of {sym}") } else { "a Fermium keyword".into() };
        let hint = if tt.s() == "integral" {
            let short: String = tt.raw.chars().take(3).collect();
            format!("for example {}_ or I_{}", tt.raw, short)
        } else {
            format!("for example {}_", tt.raw)
        };
        self.error(format!("{} is {what}, so it can't be the name of a function or variable; pick another name", tt.raw),
                   Some(t), Some(hint))
    }

    /// `h² = GM a (1 − e²)`: only a name can be stored to; point to `solve` (FRICTION #28).
    fn assign_to_non_name(&self, start: usize, lhs: &Expr) -> Diagnostic {
        let text = self.text(start, self.i);
        let run: Vec<&Token> = self.toks[start..self.i].iter().collect();
        if run.len() > 1
            && run.iter().all(|tk| tk.kind == Kind::Name)
            && !run[1..].iter().any(|tk| tk.ws_before)
            && run.iter().any(|tk| tk.s() == "π")
        {
            let prod = run.iter().map(|tk| tk.raw.as_str()).collect::<Vec<_>>().join(" × ");
            let under = run.iter().map(|tk| tk.raw.as_str()).collect::<Vec<_>>().join("_");
            return self.err_h(
                format!("can't store a value in {text}: it is read as {prod} (π is always the number π, even written \
                         next to a letter)"),
                format!("name it with an underscore instead, like  {under} = …"),
            );
        }
        let pow_name = match &lhs.kind {
            ExprKind::BinOp { op, left, .. } if op == "^" => left.name().map(|s| s.to_string()),
            _ => None,
        };
        let var = if let Some(v) = &pow_name {
            v.clone()
        } else {
            let names: Vec<String> = lhs.walk().iter().filter_map(|n| n.name().map(|s| s.to_string())).collect();
            let unknown: Vec<&String> = names.iter().filter(|n| !self.known.contains(*n)).collect();
            unknown.first().map(|s| (*s).clone()).or_else(|| names.first().cloned()).unwrap_or_else(|| "x".into())
        };
        if let ExprKind::Field { target, name } = &lhs.kind {
            if let Some(owner) = target.name() {
                if self.module_names.contains(owner) {
                    return self.err_h(
                        format!("can't change {text}: a module's names can't be changed from outside it"),
                        format!("make your own copy and use it instead:  {name} = …  (the module's own functions keep \
                                 using {text})"),
                    );
                }
                return self.err_h(
                    format!("can't store a value in {text}: the left side of = must be a variable name"),
                    format!("{text} is a part of {owner}, which can be read but not changed; store it in a name of its \
                             own:  {name} = …"),
                );
            }
        }
        let hint = if lhs.walk().iter().any(|n| matches!(n.kind, ExprKind::Prime { .. } | ExprKind::Deriv { .. })) {
            format!("a differential equation is solved with solve:  solve {text} = … with (starting values) for t from \
                     0 s to 10 s")
        } else {
            let mut h = format!("you can only assign to a name; to solve {text} = … for {var}, write  solve {text} = … \
                                 for {var} from {var}_min to {var}_max");
            if pow_name.is_some() {
                h += &format!(", or store {var} = √(…) and write {text} where you need it");
            } else {
                h += "; to compare two values use ==";
            }
            h
        };
        self.err_h(format!("can't store a value in {text}: the left side of = must be a variable name"), hint)
    }

    pub fn is_funcdef(&self) -> bool {
        match self.match_(self.i + 1) {
            None => false,
            Some(j) => self.tk(j as i64 + 1).is_op("="),
        }
    }

    fn assign_stmt(&mut self) -> R<Stmt> {
        let name = self.next();
        self.next();
        let val = self.expr_where()?;
        let nm = self.toks[name].s().to_string();
        self.known.insert(nm.clone());
        Ok(self.stmt(StmtKind::Assign { name: nm, value: val, op: "=".into() }, name))
    }

    fn funcdef(&mut self) -> R<Stmt> {
        let name = self.next();
        self.expect_op("(", None)?;
        let mut params = vec![];
        let saved = self.known.clone();
        while !self.at_op(")") {
            let pt = self.expect_name("a parameter name")?;
            let mut unit = None;
            let mut kind = None;
            if self.at_op(":") {
                // r: vector [m] — the kind of argument this version takes (C5, multiple dispatch)
                self.next();
                let kt = self.expect_name("number, vector, list or complex after ':'")?;
                let k = self.toks[kt].s().to_string();
                if !matches!(k.as_str(), "number" | "vector" | "list" | "complex") {
                    let raw = self.toks[kt].raw.clone();
                    return Err(self.error(
                        format!("a parameter can be marked  : number,  : vector,  : list  or  : complex, not : {raw}"),
                        Some(kt),
                        Some(format!("give a unit in brackets instead:  {} [{raw}]", self.toks[pt].raw)),
                    ));
                }
                kind = Some(k);
            }
            if self.at_op("[") {
                unit = Some(self.bracket_unit()?);
            }
            let pn = self.toks[pt].s().to_string();
            params.push(Param { name: pn.clone(), unit, kind, span: self.span_from(pt) });
            self.known.insert(pn);
            if self.at_op(",") {
                self.next();
            } else if !self.at_op(")") {
                return Err(self.err(format!("expected ',' or ')' in the list of parameters{}", self.found())));
            }
        }
        self.expect_op(")", None)?;
        self.expect_op("=", None)?;
        let nm = self.toks[name].s().to_string();
        self.known.insert(nm.clone());
        let body = if self.kind() == Kind::Newline {
            FuncBody::Block(self.block()?)
        } else {
            let e = self.expr_where()?;
            self.end_statement()?;
            FuncBody::Expr(e)
        };
        self.known = saved;
        self.known.insert(nm.clone());
        let nt = &self.toks[name];
        let span = Span { line: nt.line, col: nt.col, length: nt.rawlen as u32 };
        Ok(Stmt { kind: StmtKind::FuncDef { name: nm, params, body, where_: vec![] }, span })
    }

    fn print_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let mut items = vec![];
        if !self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Dedent]) {
            items.push(self.print_item()?);
            while self.at_op(",") {
                self.next();
                items.push(self.print_item()?);
            }
        }
        if self.at_kw("where") {
            let binds = self.where_bindings()?;
            let names: HashSet<String> = binds.iter().map(|(b, _)| b.clone()).collect();
            self.check_where_collisions(&items, &names)?;
            let mut out = vec![];
            for it in items {
                if matches!(it.kind, ExprKind::Str { .. }) {
                    out.push(it);
                } else {
                    let sp = it.span;
                    out.push(self.mk(ExprKind::Where { value: Box::new(it), bindings: binds.clone() }, sp));
                }
            }
            items = out;
        }
        Ok(self.stmt(StmtKind::Print { items }, t))
    }

    fn print_item(&mut self) -> R<Expr> {
        let t = self.i;
        let mut e = self.expr_full()?;
        if self.at_kw("to") && self.peek(1).kind == Kind::Num && self.peek(2).kind == Kind::Name
            && ["digits", "digit"].contains(&self.peek(2).s())
        {
            self.next();
            let n = self.next();
            self.next();
            let d = self.toks[n].f() as i64;
            e = self.mks(ExprKind::Digits { value: Box::new(e), digits: d }, t);
        }
        Ok(e)
    }

    fn animate_next(&self) -> bool {
        self.tok().is_name("animate") && !self.known.contains("animate")
    }

    fn set_opt(opts: &mut Vec<(String, PlotOpt)>, k: &str, v: PlotOpt) {
        if let Some(e) = opts.iter_mut().find(|(key, _)| key == k) {
            e.1 = v;
        } else {
            opts.push((k.to_string(), v));
        }
    }

    fn plot_nj(&self, saved: &HashSet<String>) -> HashSet<String> {
        let mut s = saved.clone();
        for w in ["title", "animate"] {
            if !self.known.contains(w) {
                s.insert(w.to_string());
            }
        }
        s
    }

    fn plot_options(&mut self, mut bare_opts: bool, out: &mut Option<String>, opts: &mut Vec<(String, PlotOpt)>)
                    -> R<()> {
        while bare_opts || self.at_kw("to") || self.at_kw("with") || self.animate_next() {
            if !bare_opts && self.at_kw("to") {
                self.next();
                if self.kind() != Kind::Str {
                    return Err(self.err("expected a file name in quotes after 'to', like \"orbit.png\""));
                }
                let f = self.next();
                *out = Some(self.toks[f].s().to_string());
                if self.at_op(",") {
                    self.next();
                    bare_opts = true;
                } else if self.plot_option_ahead() {
                    bare_opts = true;
                }
                continue;
            }
            if !bare_opts && !self.animate_next() {
                self.next();
            }
            bare_opts = false;
            loop {
                let w = self.tok().clone();
                let wv = if w.kind == Kind::Name { w.s() } else { "" };
                if wv == "log" {
                    self.next();
                    let mut axes = "xy".to_string();
                    if self.kind() == Kind::Name && ["x", "y"].contains(&self.tok().s()) {
                        let a = self.next();
                        axes = self.toks[a].s().to_string();
                    }
                    for a in axes.chars() {
                        Self::set_opt(opts, &format!("log{a}"), PlotOpt::Bool(true));
                    }
                } else if ["points", "dots", "markers"].contains(&wv) {
                    self.next();
                    Self::set_opt(opts, "points", PlotOpt::Bool(true));
                } else if wv == "animate" {
                    self.next();
                    if !self.tok().is_name("over") {
                        return Err(self.err("write  with animate over t  (the variable that changes from frame to frame)"));
                    }
                    self.next();
                    let v = self.expect_name("the variable to animate over, like t")?;
                    let v = self.toks[v].s().to_string();
                    Self::set_opt(opts, "animate", PlotOpt::Str(v));
                    if self.tok().is_name("frames") && self.peek(1).kind == Kind::Num {
                        self.next();
                        let f = self.next();
                        let fv = self.toks[f].f();
                        Self::set_opt(opts, "frames", PlotOpt::Num(fv));
                    }
                } else if wv == "title" {
                    self.next();
                    if self.kind() != Kind::Str {
                        return Err(self.err("expected the title in quotes, like title \"Decay of Ba-137m\""));
                    }
                    let s = self.next();
                    let s = self.toks[s].s().to_string();
                    Self::set_opt(opts, "title", PlotOpt::Str(s));
                } else if wv == "xlabel" || wv == "ylabel" {
                    self.next();
                    if self.kind() != Kind::Str {
                        return Err(self.err(format!("expected the axis label in quotes, like  {wv} \"mass fraction\"")));
                    }
                    let s = self.next();
                    let s = self.toks[s].s().to_string();
                    Self::set_opt(opts, wv, PlotOpt::Str(s));
                } else if (wv == "x" || wv == "y") && self.peek(1).is_kw("from") {
                    self.next();
                    self.next();
                    let lo = self.expr()?;
                    self.expect_kw("to", Some(&format!("(write:  {wv} from 1e-12 to 1)")))?;
                    let hi = self.expr()?;
                    Self::set_opt(opts, &format!("{wv}range"), PlotOpt::Range(Box::new(lo), Box::new(hi)));
                } else if wv == "reversed" {
                    self.next();
                    if !(self.kind() == Kind::Name && ["x", "y"].contains(&self.tok().s())) {
                        return Err(self.err("write  with reversed x  (or  reversed y)"));
                    }
                    let a = self.next();
                    let a = self.toks[a].s().to_string();
                    Self::set_opt(opts, &format!("rev{a}"), PlotOpt::Bool(true));
                } else {
                    return Err(self.err(
                        "plot options are:  with log y,  with log x,  with log,  with points,  with title \"...\",  \
                         with xlabel \"...\",  with ylabel \"...\",  with y from a to b,  with x from a to b,  \
                         with reversed x,  with animate over t",
                    ));
                }
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                break;
            }
        }
        Ok(())
    }

    fn plot_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let mut series = vec![];
        let saved_nj = self.no_juxt_names.clone();
        self.no_juxt_names = self.plot_nj(&saved_nj);
        let r = self.plot_series(&mut series);
        self.no_juxt_names = saved_nj.clone();
        let bare_opts = r?;
        let mut out = None;
        let mut opts = vec![];
        self.plot_options(bare_opts, &mut out, &mut opts)?;
        if self.kind() == Kind::Newline && self.peek(1).kind == Kind::Indent {
            // plot … continued on indented lines: more series, `with …` options, `to "f.png"` (D216)
            self.next();
            self.next();
            while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                if self.at_op(",") || self.at_kw("and") {
                    self.next();
                }
                if self.at_kw("with") || self.at_kw("to") || self.animate_next() {
                    self.plot_options(false, &mut out, &mut opts)?;
                } else if self.plot_option_ahead() {
                    self.plot_options(true, &mut out, &mut opts)?;
                } else {
                    self.no_juxt_names = self.plot_nj(&saved_nj);
                    let r = self.plot_series(&mut series);
                    self.no_juxt_names = saved_nj.clone();
                    let b = r?;
                    self.plot_options(b, &mut out, &mut opts)?;
                }
                if !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                    self.end_statement()?;
                }
                self.skip_newlines();
            }
            if self.kind() == Kind::Dedent {
                self.next();
            }
            self.stmt_done = true;
        }
        Ok(self.stmt(StmtKind::Plot { series, out, options: opts }, t))
    }

    /// The `y vs x [from a to b]` series of a plot, separated by ',' or 'and'. True when plot options follow
    /// without `with`: `plot y vs x, title "…"` or `plot y vs x title "…"` (#65).
    fn plot_series(&mut self, series: &mut Vec<PlotSeries>) -> R<bool> {
        loop {
            let st = self.i;
            let y = self.expr_full()?;
            if !self.at_kw("vs") && (self.kind() == Kind::Str || self.tok().is_name("title")) {
                return Err(self.err("expected 'vs' after the quantity to plot; a title goes after the series, like  \
                                     plot y vs x, title \"Orbit\"  (or  with title \"Orbit\")"));
            }
            self.expect_kw("vs", Some("(write: plot y vs x)"))?;
            let x = self.expr_full()?;
            let (mut lo, mut hi) = (None, None);
            if self.at_kw("from") {
                self.next();
                lo = Some(self.expr()?);
                self.expect_kw("to", None)?;
                hi = Some(self.expr()?);
            }
            series.push(PlotSeries { y, x, lo, hi, span: self.span_from(st) });
            if self.at_op(",") || self.at_kw("and") {
                self.next();
                if self.plot_option_ahead() {
                    return Ok(true);
                }
                continue;
            }
            return Ok(self.tok().is_name("title") && self.plot_option_ahead());
        }
    }

    /// After a ',' in a plot: does a plot option (title "…", log [x|y], points) follow rather than another series?
    fn plot_option_ahead(&self) -> bool {
        let (w, nx) = (self.tok(), self.peek(1));
        if w.kind == Kind::Name && ["x", "y"].contains(&w.s()) && nx.is_kw("from") {
            return true;
        }
        if w.kind != Kind::Name || self.known.contains(w.s()) {
            return false;
        }
        let ends = matches!(nx.kind, Kind::Newline | Kind::Eof)
            || nx.is_op(",")
            || (nx.kind == Kind::Kw && ["to", "with"].contains(&nx.s()));
        match w.s() {
            "title" | "xlabel" | "ylabel" => nx.kind == Kind::Str,
            "reversed" => nx.kind == Kind::Name && ["x", "y"].contains(&nx.s()),
            "log" => ends || (nx.kind == Kind::Name && ["x", "y"].contains(&nx.s())),
            "points" | "dots" | "markers" => ends,
            _ => false,
        }
    }

    pub fn equation(&mut self) -> R<Equation> {
        let st = self.i;
        let lhs = self.expr()?;
        if !self.at_op("=") {
            return Err(self.err(format!("expected '=' in this equation{}", self.found())));
        }
        self.next();
        let rhs = self.expr()?;
        Ok(Equation { lhs, rhs, span: self.span_from(st) })
    }

    /// Is this token a unit name? In a solve, the unknown written with a prime (`u''`) never is (D211).
    pub fn unit_tok(&self, j: usize) -> bool {
        let tk = &self.toks[j];
        crate::units::is_unit_name(&tk.raw) && !tk.unknown_prime
    }

    /// Before parsing a solve: the names written with a prime in it (`u''`, `x'`), its unknowns (D211).
    fn solve_unknowns_scan(&mut self) -> HashSet<String> {
        let mut names = HashSet::new();
        let mut indep = HashSet::new();
        let mut depth = 0i64;
        let mut j = self.i;
        while j < self.toks.len() {
            let kind = self.toks[j].kind;
            if kind == Kind::Eof {
                break;
            }
            let nx = self.tk(j as i64 + 1).clone();
            if kind == Kind::Indent {
                depth += 1;
            } else if kind == Kind::Dedent {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            } else if kind == Kind::Newline && depth == 0 && nx.kind != Kind::Indent {
                break;
            } else if kind == Kind::Name && nx.kind == Kind::Prime && !nx.ws_before {
                names.insert(self.toks[j].s().to_string());
                if crate::units::is_unit_name(&self.toks[j].raw) {
                    self.toks[j].unknown_prime = true;
                }
            } else if kind == Kind::Name && j > 0 && nx.is_kw("from")
                && (self.toks[j - 1].is_kw("for") || self.toks[j - 1].is_op(","))
            {
                indep.insert(self.toks[j].s().to_string());
            }
            j += 1;
        }
        self.solve_independents = indep;
        names
    }

    /// An equation of a solve: its unknowns count as your variables while it is parsed (D211).
    fn solve_equation(&mut self, unknowns: &HashSet<String>) -> R<Equation> {
        let saved_known = self.known.clone();
        let saved_unk = self.solve_unknowns.clone();
        let mut k = saved_known.clone();
        k.extend(unknowns.iter().cloned());
        k.extend(self.solve_independents.iter().cloned());
        self.known = k;
        self.solve_unknowns = unknowns.clone();
        let r = self.equation();
        self.known = saved_known;
        self.solve_unknowns = saved_unk;
        r
    }

    fn solve_opts_nj(saved: &HashSet<String>) -> HashSet<String> {
        let mut s = saved.clone();
        for w in ["tolerance", "absolute", "using", "method", "until", "lowest", "grid"] {
            s.insert(w.to_string());
        }
        s
    }

    fn solve_clause(&mut self, sv: &mut SolveState) -> R<bool> {
        if self.at_kw("with") {
            self.next();
            let e = self.equation()?;
            sv.initial.push(e);
            while self.at_op(",") || self.at_kw("and") {
                self.next();
                let e = self.equation()?;
                sv.initial.push(e);
            }
            return Ok(true);
        }
        if self.at_kw("for") {
            self.next();
            let vt = self.expect_name("the time variable, e.g. 'for t from 0 s to 5 s'")?;
            sv.var = Some(self.toks[vt].s().to_string());
            self.expect_kw("from", None)?;
            let saved = self.no_juxt_names.clone();
            self.no_juxt_names = Self::solve_opts_nj(&saved);
            sv.lo = Some(self.expr()?);
            self.expect_kw("to", None)?;
            sv.hi = Some(self.expr()?);
            if self.at_kw("step") {
                self.next();
                sv.step = Some(self.expr()?);
            }
            let mut seen: HashSet<&'static str> = HashSet::new();
            while self.kind() == Kind::Name {
                let word = self.tok().s().to_string();
                let key = if word == "tolerance" {
                    "tolerance"
                } else if word == "absolute" && !self.known.contains("absolute") {
                    "absolute"
                } else if word == "using" || word == "method" {
                    "using"
                } else if word == "until" && !self.known.contains("until") {
                    "until"
                } else {
                    break;
                };
                if seen.contains(key) {
                    return Err(self.err(format!("'{word}' is given twice in this solve")));
                }
                seen.insert(key);
                self.next();
                match key {
                    "tolerance" => sv.tol = Some(self.expr()?),
                    "absolute" => {
                        let mut v = vec![self.expr()?];
                        while self.at_op(",") && self.peek(1).kind == Kind::Num {
                            self.next();
                            v.push(self.expr()?);
                        }
                        sv.abs = Some(v);
                    }
                    "using" => {
                        let m = self.expect_name("a method name (rk4, rk45, radau or bdf)")?;
                        sv.method = Some(self.toks[m].s().to_string());
                    }
                    _ => sv.until = Some(self.equation()?),
                }
            }
            self.no_juxt_names = saved.clone();
            if self.at_op(",") && self.peek(1).kind == Kind::Name && self.peek(2).is_kw("from") {
                self.next();
                let v2 = self.next();
                sv.var2 = Some(self.toks[v2].s().to_string());
                self.expect_kw("from", None)?;
                self.no_juxt_names = Self::solve_opts_nj(&saved);
                sv.lo2 = Some(self.expr()?);
                self.expect_kw("to", None)?;
                sv.hi2 = Some(self.expr()?);
                if self.at_kw("step") {
                    self.next();
                    sv.step2 = Some(self.expr()?);
                }
                self.no_juxt_names = saved.clone();
            }
            if self.tok().is_name("tolerance") {
                self.next();
                sv.tol = Some(self.expr()?);
            }
            if self.kind() == Kind::Name && ["using", "method"].contains(&self.tok().s()) {
                self.next();
                let m = self.expect_name("a method name (rk4, rk45, radau or bdf)")?;
                sv.method = Some(self.toks[m].s().to_string());
            }
            if self.tok().is_name("until") && !self.known.contains("until") {
                self.next();
                sv.until = Some(self.equation()?);
            }
            return Ok(true);
        }
        if self.kind() == Kind::Name
            && ["lowest", "grid"].contains(&self.tok().s())
            && !self.known.contains(self.tok().s())
            && self.peek(1).kind == Kind::Num
        {
            let w = self.next();
            let word = self.toks[w].s().to_string();
            let saved = self.no_juxt_names.clone();
            let mut nj = saved.clone();
            for x in ["states", "levels", "state", "grid", "lowest", "using", "method"] {
                nj.insert(x.to_string());
            }
            self.no_juxt_names = nj;
            let e = self.expr()?;
            if word == "lowest" {
                sv.lowest = Some(e);
            } else {
                sv.grid = Some(e);
            }
            self.no_juxt_names = saved;
            if self.kind() == Kind::Name && ["states", "levels", "state"].contains(&self.tok().s()) && word == "lowest"
            {
                self.next();
            }
            if self.kind() == Kind::Name && ["using", "method"].contains(&self.tok().s()) && sv.method.is_none() {
                self.next();
                let m = self.expect_name("a method name (matrix or shooting)")?;
                sv.method = Some(self.toks[m].s().to_string());
            }
            return Ok(true);
        }
        if self.kind() == Kind::Name
            && ["tolerance", "absolute"].contains(&self.tok().s())
            && !self.known.contains(self.tok().s())
            && !self.peek(1).is_op("=")
        {
            let word = self.tok().s().to_string();
            let given = if word == "tolerance" { sv.tol.is_some() } else { sv.abs.is_some() };
            if given {
                return Err(self.err(format!("'{word}' is given twice in this solve")));
            }
            self.next();
            let saved = self.no_juxt_names.clone();
            self.no_juxt_names = Self::solve_opts_nj(&saved);
            if word == "tolerance" {
                sv.tol = Some(self.expr()?);
            } else {
                let mut v = vec![self.expr()?];
                while self.at_op(",") && self.peek(1).kind == Kind::Num {
                    self.next();
                    v.push(self.expr()?);
                }
                sv.abs = Some(v);
            }
            self.no_juxt_names = saved;
            return Ok(true);
        }
        if self.kind() == Kind::Name
            && ["using", "method"].contains(&self.tok().s())
            && !self.known.contains(self.tok().s())
            && self.peek(1).kind == Kind::Name
        {
            if sv.method.is_some() {
                return Err(self.err(format!("'{}' is given twice in this solve", self.tok().s())));
            }
            self.next();
            let m = self.expect_name(
                "a method name (rk4, rk45, radau, bdf, matrix, shooting, crank_nicolson, implicit or explicit)",
            )?;
            sv.method = Some(self.toks[m].s().to_string());
            return Ok(true);
        }
        Ok(false)
    }

    fn solve_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let unknowns = self.solve_unknowns_scan();
        let mut sv = SolveState::default();
        let mut eqs = vec![];
        if self.kind() != Kind::Newline {
            eqs.push(self.solve_equation(&unknowns)?);
            while self.at_op(",") || self.at_kw("and") {
                self.next();
                eqs.push(self.solve_equation(&unknowns)?);
            }
        }
        while self.solve_clause(&mut sv)? {}
        if self.kind() == Kind::Newline && self.peek(1).kind == Kind::Indent {
            self.next();
            self.next();
            while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                if !self.solve_clause(&mut sv)? {
                    eqs.push(self.solve_equation(&unknowns)?);
                    while self.at_op(",") || self.at_kw("and") {
                        self.next();
                        eqs.push(self.solve_equation(&unknowns)?);
                    }
                }
                while self.solve_clause(&mut sv)? {}
                self.end_statement()?;
                self.skip_newlines();
            }
            if self.kind() == Kind::Dedent {
                self.next();
            }
            if self.kind() == Kind::Indent && self.peek(1).kind == Kind::Kw && ["with", "for"].contains(&self.peek(1).s())
            {
                self.next();
                while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                    if !self.solve_clause(&mut sv)? {
                        return Err(self.err(format!("expected 'with ...' or 'for ...' here{}", self.found())));
                    }
                    while self.solve_clause(&mut sv)? {}
                    self.end_statement()?;
                    self.skip_newlines();
                }
                if self.kind() == Kind::Dedent {
                    self.next();
                }
            }
        } else {
            self.end_statement()?;
        }
        if eqs.is_empty() {
            return Err(self.error("this solve has no equation", Some(t),
                                  Some("write e.g. solve x' = -x with x(0) = 1 for t from 0 to 5".into())));
        }
        let Some(var) = sv.var.clone() else {
            let tt = &self.toks[t];
            return Err(Diagnostic::error("solve needs a range for the independent variable", tt.line, tt.col, 5,
                                         Some("add e.g.  for t from 0 s to 10 s".into())));
        };
        self.warn_divide_by_unknown(&mut eqs);
        let tt = &self.toks[t];
        let span = Span { line: tt.line, col: tt.col, length: 5 };
        for eq in &eqs {
            for n in eq.lhs.walk() {
                if let Some(nm) = n.name() {
                    self.known.insert(nm.to_string());
                }
            }
        }
        let s = Solve {
            equations: eqs,
            initial: sv.initial,
            var,
            lo: sv.lo.unwrap(),
            hi: sv.hi.unwrap(),
            step: sv.step,
            method: sv.method,
            tolerance: sv.tol,
            until: sv.until,
            absolute: sv.abs,
            lowest: sv.lowest,
            grid: sv.grid,
            var2: sv.var2,
            lo2: sv.lo2,
            hi2: sv.hi2,
            step2: sv.step2,
        };
        Ok(Stmt { kind: StmtKind::Solve(Box::new(s)), span })
    }

    /// `ψ'' = -2 m_e E / ħ² ψ` divides by ψ (D8): warn (FRICTION #9).
    fn warn_divide_by_unknown(&mut self, eqs: &mut [Equation]) {
        let mut unknowns = HashSet::new();
        for eq in eqs.iter() {
            for n in eq.lhs.walk() {
                let base = match &n.kind {
                    ExprKind::Prime { target, .. } => Some(&**target),
                    ExprKind::Deriv { operand, .. } => Some(&**operand),
                    _ => None,
                };
                if let Some(mut base) = base {
                    if let ExprKind::Call { func, .. } = &base.kind {
                        base = func;
                    }
                    if let Some(nm) = base.name() {
                        unknowns.insert(nm.to_string());
                    }
                }
            }
        }
        // first find what to warn about (walk order), then warn and mark the infos
        let mut todo: Vec<(u32, DivInfo, usize, String)> = vec![];
        for eq in eqs.iter() {
            for n in eq.rhs.walk() {
                let Some(info) = &n.attrs.div_info else { continue };
                if info.warned != Some(false) || todo.iter().any(|(id, ..)| *id == n.id) {
                    continue;
                }
                let Some(factors) = &info.factors else { continue };
                for (k, f) in factors.iter().enumerate() {
                    let g = match &f.expr.kind {
                        ExprKind::Call { func, .. } => &**func,
                        _ => &*f.expr,
                    };
                    if k > 0 {
                        if let Some(nm) = g.name() {
                            if unknowns.contains(nm) {
                                todo.push((n.id, (**info).clone(), k, nm.to_string()));
                                break;
                            }
                        }
                    }
                }
            }
        }
        for (id, info, k, nm) in todo {
            self.warn_juxt_denominator(&info, k, &format!(", including the unknown {nm}"));
            for eq in eqs.iter_mut() {
                eq.rhs.walk_mut(&mut |n: &mut Expr| {
                    if n.id == id {
                        if let Some(d) = &mut n.attrs.div_info {
                            d.warned = Some(true);
                        }
                    }
                });
            }
        }
    }

    /// import mechanics [as m] | import "path/file.fm" [as m] | from mechanics import a [as b], c  (D100)
    fn import_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let frm = self.toks[t].s() == "from";
        let mt = self.next();
        let mtt = self.toks[mt].clone();
        let (module, is_path) =
            if mtt.kind == Kind::Str { (mtt.s().to_string(), true) } else { (mtt.raw.clone(), false) };
        if !frm {
            let mut alias = None;
            if self.tok().is_name("as") {
                self.next();
                let a = self.expect_name("a name after 'as' (like  import astro as a)")?;
                alias = Some(self.toks[a].s().to_string());
            }
            if self.at_op(",") {
                return Err(self.err_h("import one module per line", "write each on its own line:  import mechanics"));
            }
            let bound = alias.clone().unwrap_or_else(|| mtt.s().to_string());
            self.known.insert(bound.clone());
            self.module_names.insert(bound);
            return Ok(self.stmt(StmtKind::Import { module, is_path, alias, names: None }, t));
        }
        if !self.tok().is_name("import") {
            return Err(self.err_h(format!("expected 'import' after 'from {}'{}", mtt.raw, self.found()),
                                  format!("write  from {} import name1, name2", mtt.raw)));
        }
        self.next();
        let mut names = vec![];
        loop {
            let nt = self.expect_name(&format!("a name to import from {} (like  from nuclear import semf_binding)",
                                               mtt.raw))?;
            let mut alias = None;
            if self.tok().is_name("as") {
                self.next();
                let a = self.expect_name("a name after 'as'")?;
                alias = Some(self.toks[a].s().to_string());
            }
            let nv = self.toks[nt].s().to_string();
            self.known.insert(alias.clone().unwrap_or_else(|| nv.clone()));
            names.push((nv, alias));
            if !self.at_op(",") {
                break;
            }
            self.next();
        }
        Ok(self.stmt(StmtKind::Import { module, is_path, alias: None, names: Some(names) }, t))
    }

    /// use python numpy [as np] [: signatures]   (D140).
    fn use_python_stmt(&mut self, end_line: bool) -> R<Stmt> {
        let t = self.next();
        self.next();
        let first = self.expect_name("the name of a Python module (like  use python numpy as np)")?;
        let mut parts = vec![self.toks[first].raw.clone()];
        while self.at_op(".") && !self.tok().ws_before {
            self.next();
            let p = self.expect_name("the rest of the Python module's name (like scipy.special)")?;
            parts.push(self.toks[p].raw.clone());
        }
        let module = parts.join(".");
        let mut alias = None;
        if self.tok().is_name("as") {
            self.next();
            let a = self.expect_name("a name after 'as' (like  use python numpy as np)")?;
            alias = Some(self.toks[a].s().to_string());
        } else if parts.len() > 1 {
            let short: String = parts.last().unwrap().chars().take(2).collect();
            return Err(self.err_h(format!("give the Python module {module} a short name with as"),
                                  format!("write  use python {module} as {short}")));
        }
        self.known.insert(alias.clone().unwrap_or_else(|| module.clone()));
        let mut sigs = vec![];
        if self.at_op(":") {
            self.next();
            if self.kind() != Kind::Newline {
                sigs.push(self.py_signature()?);
                while self.at_op(";") {
                    self.next();
                    sigs.push(self.py_signature()?);
                }
            } else {
                self.next();
                self.skip_newlines();
                if self.kind() != Kind::Indent {
                    return Err(self.err_h(
                        "expected the signatures of the Python functions, indented on the next lines",
                        "like\n    use python mylib as ml:\n        energy(m [kg], v [m/s]) -> [J]",
                    ));
                }
                self.next();
                while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                    sigs.push(self.py_signature()?);
                    if self.kind() == Kind::Newline {
                        self.next();
                    } else if !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                        return Err(self.err(format!("expected one signature per line{}", self.found())));
                    }
                    self.skip_newlines();
                }
                if self.kind() == Kind::Dedent {
                    self.next();
                }
                return Ok(self.stmt(StmtKind::UsePython { module, alias, sigs }, t));
            }
        }
        if end_line {
            self.end_statement()?;
        }
        Ok(self.stmt(StmtKind::UsePython { module, alias, sigs }, t))
    }

    /// import c "libphys.so": / import fortran "libnuclear.so":  then one signature per line (C3, D275).
    fn import_c_stmt(&mut self, end_line: bool) -> R<Stmt> {
        let t = self.next();
        let lt = self.next();
        let lang = self.toks[lt].s().to_string();
        let st = self.next();
        let lib = self.toks[st].s().to_string();
        let what = if lang == "c" { "C" } else { "Fortran" };
        let example = if lang == "c" {
            "like\n    import c \"libphys.so\":\n        kinetic_energy(m [kg], v [m/s]) -> [J]"
        } else {
            "like\n    import fortran \"libnuclear.so\":\n        binding_energy(Z: int, A: int) -> [MeV]"
        };
        if !self.at_op(":") {
            return Err(self.err_h(format!("expected ':' and the signatures of the {what} functions after the \
                                           library's name{}", self.found()), example));
        }
        self.next();
        let mut sigs = vec![];
        if self.kind() != Kind::Newline {
            sigs.push(self.c_signature(&lang)?);
            while self.at_op(";") {
                self.next();
                sigs.push(self.c_signature(&lang)?);
            }
            if end_line {
                self.end_statement()?;
            }
        } else {
            self.next();
            self.skip_newlines();
            if self.kind() != Kind::Indent {
                return Err(self.err_h(format!("expected an indented block: the signatures of the {what} functions, \
                                               one per line"), example));
            }
            self.next();
            while !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                sigs.push(self.c_signature(&lang)?);
                if self.kind() == Kind::Newline {
                    self.next();
                } else if !self.at_kind(&[Kind::Dedent, Kind::Eof]) {
                    return Err(self.err(format!("expected one signature per line{}", self.found())));
                }
                self.skip_newlines();
            }
            if self.kind() == Kind::Dedent {
                self.next();
            }
        }
        for g in &sigs {
            self.known.insert(g.name.clone());
        }
        Ok(self.stmt(StmtKind::ImportC { lang, lib, sigs }, t))
    }

    /// kinetic_energy(m [kg], v [km/s]) -> [J];  sum_sq(x: list [m], n: len(x)) -> [m²];
    /// neutron_separation(Z: int, A: int) -> [MeV] bind(C, name="semf_sn")
    fn c_signature(&mut self, lang: &str) -> R<CSig> {
        let what = if lang == "c" { "C" } else { "Fortran" };
        let nt = self.i;
        let ntt = self.tok().clone();
        if ntt.kind == Kind::Kw && self.peek(1).is_op("(") {
            let hint = if lang == "c" {
                format!("give it another name in a small C wrapper, like  double my_{0}(double x) {{ return {0}(x); }}",
                        ntt.raw)
            } else {
                format!("declare it under another name with its symbol:  my_{0}(…) -> [J] bind(C, name=\"{0}_\")",
                        ntt.raw)
            };
            return Err(self.error(format!("{} is a Fermium keyword, so it can't be the name of a {what} function",
                                          ntt.raw), None, Some(hint)));
        }
        if ntt.kind != Kind::Name {
            return Err(self.err(format!(
                "expected a {what} function's signature, like  energy(m [kg], v [m/s]) -> [J]{}", self.found())));
        }
        self.next();
        let fname = ntt.s().to_string();
        self.expect_op("(", Some(&format!("after {0} (write the signature like  {0}(x [m]) -> [J])", ntt.raw)))?;
        let mut params = vec![];
        while !self.at_op(")") {
            let p0 = self.i;
            let pt = self.expect_name("a parameter name")?;
            let pname = self.toks[pt].s().to_string();
            let kind = if self.at_op("[") {
                CParamKind::Num(Some(self.bracket_unit()?))
            } else if self.at_op(":") {
                self.next();
                let kt = self.expect_name("int, list or len(…) after ':'")?;
                match self.toks[kt].s() {
                    "int" => CParamKind::Int,
                    "list" => CParamKind::List(if self.at_op("[") { Some(self.bracket_unit()?) } else { None }),
                    "len" => {
                        self.expect_op("(", Some("after len (like  n: len(x))"))?;
                        let lt = self.expect_name("the name of a list parameter inside len( )")?;
                        self.expect_op(")", None)?;
                        CParamKind::Len(self.toks[lt].s().to_string())
                    }
                    _ => {
                        let unit = self.toks[kt].raw.clone();
                        return Err(self.error(
                            format!("a parameter can be marked  : int  (a whole number), : list  or  : len(x), not : \
                                     {unit}"),
                            Some(kt),
                            Some(format!("give a unit in brackets instead:  {} [{unit}]", self.toks[pt].raw)),
                        ));
                    }
                }
            } else {
                CParamKind::Num(None)
            };
            params.push(CParam { name: pname, kind, span: self.span_from(p0) });
            if self.at_op(",") {
                self.next();
            } else if !self.at_op(")") {
                return Err(self.err(format!("expected ',' or ')' in the list of parameters{}", self.found())));
            }
        }
        self.expect_op(")", None)?;
        if !(self.at_op("-") && self.peek(1).is_op(">")) {
            return Err(self.err_h(
                format!("a {what} function's signature needs its result after ->{}", self.found()),
                format!("like  {}(…) -> [J]   (-> number or -> int for a plain number; functions that return \
                         nothing aren't supported yet)", ntt.raw),
            ));
        }
        self.next();
        self.next();
        let ret = if self.at_op("[") {
            CRetDecl::Unit(self.bracket_unit()?)
        } else if self.tok().is_name("number") {
            self.next();
            CRetDecl::Number
        } else if self.tok().is_name("int") {
            self.next();
            CRetDecl::Int
        } else {
            return Err(self.err_h(
                format!("expected the result's unit in brackets, or number / int, after ->{}", self.found()),
                format!("like  {}(x [m]) -> [J]   or  -> int", ntt.raw),
            ));
        };
        let mut bind = None;
        if self.tok().is_name("bind") {
            let bt = self.next();
            if lang != "fortran" {
                return Err(self.error("bind(C) is for Fortran functions; a C function is found by its own name",
                                      Some(bt), Some("remove bind(…)".into())));
            }
            self.expect_op("(", Some("after bind (like  bind(C)  or  bind(C, name=\"f\"))"))?;
            let ct = self.i;
            if !(self.kind() == Kind::Name && self.tok().raw == "C") {
                return Err(self.error(format!("expected C in bind( ){}", self.found()), Some(ct),
                                      Some("write  bind(C)  or  bind(C, name=\"f\")".into())));
            }
            self.next();
            let mut name = None;
            if self.at_op(",") {
                self.next();
                if !self.tok().is_name("name") {
                    return Err(self.err_h(format!("expected name=\"…\" after bind(C,{}", self.found()),
                                          "write  bind(C, name=\"f\")"));
                }
                self.next();
                self.expect_op("=", Some("after name (like  bind(C, name=\"f\"))"))?;
                if self.kind() != Kind::Str {
                    return Err(self.err_h(format!("expected the symbol's name in quotes{}", self.found()),
                                          "write  bind(C, name=\"f\")"));
                }
                let q = self.next();
                name = Some(self.toks[q].s().to_string());
            }
            self.expect_op(")", None)?;
            bind = Some(name);
        }
        Ok(CSig { name: fname, params, ret, bind, span: self.span_from(nt) })
    }

    /// f(x [m], xs [s], n: int) -> list [J]   (the result part is optional).
    fn py_signature(&mut self) -> R<PySig> {
        let nt = self.i;
        let ntt = self.tok().clone();
        if !matches!(ntt.kind, Kind::Name | Kind::Kw) {
            return Err(self.err(format!(
                "expected a Python function's signature, like  energy(m [kg], v [m/s]) -> [J]{}", self.found())));
        }
        self.next();
        self.expect_op("(", Some(&format!("after {0} (write the signature like  {0}(x [m]) -> [J])", ntt.raw)))?;
        let mut params = vec![];
        while !self.at_op(")") {
            let pt = self.expect_name("a parameter name")?;
            let (mut unit, mut is_int) = (None, false);
            if self.at_op("[") {
                unit = Some(self.bracket_unit()?);
            } else if self.at_op(":") {
                self.next();
                let kt = self.expect_name("int after ':' (a whole number passed to Python as an int)")?;
                if self.toks[kt].s() != "int" {
                    return Err(self.error(
                        format!("a parameter can be marked  : int  (a whole number), not : {}", self.toks[kt].raw),
                        Some(kt),
                        Some("give a unit in brackets instead, like  x [m]".into()),
                    ));
                }
                is_int = true;
            }
            params.push((self.toks[pt].raw.clone(), unit, is_int));
            if self.at_op(",") {
                self.next();
            } else if !self.at_op(")") {
                return Err(self.err(format!("expected ',' or ')' in the list of parameters{}", self.found())));
            }
        }
        self.expect_op(")", None)?;
        let (mut shape, mut runit) = (None, None);
        if self.at_op("-") && self.peek(1).is_op(">") {
            self.next();
            self.next();
            if self.kind() == Kind::Name && ["list", "number"].contains(&self.tok().s()) {
                let s = self.next();
                shape = Some(self.toks[s].s().to_string());
            }
            if self.at_op("[") {
                runit = Some(self.bracket_unit()?);
            } else if shape.is_none() {
                return Err(self.err_h(
                    format!("expected the result's unit in brackets, or list / number, after ->{}", self.found()),
                    format!("like  {}(x [m]) -> [J]   or  -> list [m]", ntt.raw),
                ));
            }
        }
        Ok(PySig { name: ntt.raw.clone(), params, ret_shape: shape, ret_unit: runit, span: self.span_from(nt) })
    }

    /// `analyze [title:] T depends on ...`: 'depends' follows on the same line (so `analyze` stays a name).
    fn is_analyze(&self) -> bool {
        let mut j = self.i + 1;
        while j < self.toks.len() && !matches!(self.toks[j].kind, Kind::Newline | Kind::Eof) {
            if self.toks[j].is_name("depends") {
                return true;
            }
            j += 1;
        }
        false
    }

    fn analyze_quantity(&mut self, what: &str, raw: &mut Vec<(String, String)>) -> R<Param> {
        let nt = self.i;
        if self.kind() != Kind::Name {
            return Err(self.err_h(format!("expected {what}{}", self.found()),
                                  "write  analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]"));
        }
        self.next();
        let unit = if self.at_op("[") { Some(self.bracket_unit()?) } else { None };
        let (v, r) = (self.toks[nt].s().to_string(), self.toks[nt].raw.clone());
        if let Some(e) = raw.iter_mut().find(|(k, _)| *k == v) {
            e.1 = r;
        } else {
            raw.push((v.clone(), r));
        }
        Ok(Param { name: v, unit, kind: None, span: self.span_from(nt) })
    }

    /// analyze [title:] T [unit] depends on a [unit], b, c  (D70)
    fn analyze_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let mut title = None;
        let mut raw = vec![];
        if self.kind() == Kind::Name && self.peek(1).is_op(":") {
            let tt = self.next();
            title = Some(self.toks[tt].s().to_string());
            self.next();
        }
        let target = self.analyze_quantity("the quantity to analyze (like T)", &mut raw)?;
        if !self.tok().is_name("depends") {
            return Err(self.err_h(format!("expected 'depends on' after the quantity to analyze{}", self.found()),
                                  "write  analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]"));
        }
        self.next();
        if !self.tok().is_name("on") {
            return Err(self.err(format!("expected 'on' after 'depends'{}", self.found())));
        }
        self.next();
        let mut inputs = vec![self.analyze_quantity("a quantity after 'depends on'", &mut raw)?];
        while self.at_op(",") {
            self.next();
            inputs.push(self.analyze_quantity("a quantity after ','", &mut raw)?);
        }
        if let Some(tl) = &title {
            if !tl.is_empty() {
                self.known.insert(tl.clone());
            }
        }
        Ok(self.stmt(StmtKind::Analyze { title, target, inputs, raw }, t))
    }

    fn fit_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let model = self.equation()?;
        self.expect_kw("to", Some("(write: fit y = model to data)"))?;
        let data = self.expr()?;
        let mut guesses = vec![];
        let mut indented = false;
        if self.kind() == Kind::Newline && self.peek(1).kind == Kind::Indent && self.peek(2).is_kw("with") {
            self.next();
            self.next();
            indented = true;
        }
        if self.at_kw("with") || self.at_kw("starting") {
            self.next();
            loop {
                let nt = self.expect_name("a parameter name")?;
                self.expect_op("=", None)?;
                let e = self.expr()?;
                guesses.push((self.toks[nt].s().to_string(), e));
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                break;
            }
        }
        let f = self.stmt(StmtKind::Fit { model, data, guesses }, t);
        if indented {
            self.skip_newlines();
            if self.kind() != Kind::Dedent {
                return Err(self.err(format!("expected the end of the indented 'with' line{}", self.found())));
            }
            self.next();
            self.stmt_done = true;
        }
        Ok(f)
    }

    fn if_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let cond = self.expr()?;
        if self.at_kw("then") {
            self.next();
        }
        let then = self.block()?;
        let mut other = None;
        self.skip_newlines_if_else();
        if self.at_kw("else") {
            self.next();
            if self.at_kw("if") {
                other = Some(vec![self.if_stmt()?]);
            } else {
                other = Some(self.block()?);
            }
        } else if self.at_kw("elif") {
            other = Some(vec![self.if_stmt()?]);
        }
        Ok(self.stmt(StmtKind::If { cond, then, other }, t))
    }

    fn skip_newlines_if_else(&mut self) {
        let mut j = self.i;
        while self.toks[j].kind == Kind::Newline {
            j += 1;
        }
        if self.toks[j].kind == Kind::Kw && ["else", "elif"].contains(&self.toks[j].s()) {
            self.i = j;
        }
    }

    fn for_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let vt = self.expect_name("a loop variable name")?;
        let var = self.toks[vt].s().to_string();
        self.known.insert(var.clone());
        if self.at_kw("in") {
            self.next();
            let it = self.expr()?;
            let body = self.block()?;
            return Ok(self.stmt(StmtKind::ForIn { var, iterable: it, body }, t));
        }
        self.expect_kw("from", Some("(write: for i from 1 to 10)"))?;
        let lo = self.expr()?;
        self.expect_kw("to", None)?;
        let hi = self.expr()?;
        let mut step = None;
        if self.at_kw("step") {
            self.next();
            step = Some(self.expr()?);
        }
        let body = self.block()?;
        let tt = &self.toks[t];
        Ok(Stmt {
            kind: StmtKind::For { var, lo, hi, step, body, parallel: false },
            span: Span { line: tt.line, col: tt.col, length: 3 },
        })
    }

    fn while_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let cond = self.expr()?;
        let body = self.block()?;
        let tt = &self.toks[t];
        Ok(Stmt { kind: StmtKind::While { cond, body }, span: Span { line: tt.line, col: tt.col, length: 5 } })
    }

    fn return_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let mut val = None;
        if !self.at_kind(&[Kind::Newline, Kind::Eof, Kind::Dedent]) {
            val = Some(self.expr_where()?);
        }
        Ok(self.stmt(StmtKind::Return { value: val }, t))
    }

    fn assert_stmt(&mut self) -> R<Stmt> {
        let t = self.next();
        let cond = self.expr()?;
        let mut msg = None;
        if self.at_op(",") {
            self.next();
            if self.kind() != Kind::Str {
                return Err(self.err("expected a message in quotes after the comma"));
            }
            let m = self.next();
            msg = Some(self.toks[m].s().to_string());
        }
        Ok(self.stmt(StmtKind::Assert { cond, message: msg }, t))
    }
}

/// The clauses of a solve as they are read (the Python code's nonlocal variables).
#[derive(Default)]
pub struct SolveState {
    pub initial: Vec<Equation>,
    pub var: Option<String>,
    pub lo: Option<Expr>,
    pub hi: Option<Expr>,
    pub step: Option<Expr>,
    pub method: Option<String>,
    pub tol: Option<Expr>,
    pub abs: Option<Vec<Expr>>,
    pub until: Option<Equation>,
    pub lowest: Option<Expr>,
    pub grid: Option<Expr>,
    pub var2: Option<String>,
    pub lo2: Option<Expr>,
    pub hi2: Option<Expr>,
    pub step2: Option<Expr>,
}
