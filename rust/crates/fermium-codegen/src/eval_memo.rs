//! Remembering pure function calls while one ODE right-hand side is evaluated (D270).
//!
//! A network of rate equations calls the same expensive functions of the same arguments many times per
//! evaluation: in research/bbn_network every one of 42 rate terms calls n_b(T), which integrates the e± plasma
//! twice, so one right-hand side ran ~100 quadratures where 5 are distinct. v1 compiles it (native code, 0.35 ms
//! per evaluation); the tree-walker took ~20 ms, and the program 25 minutes instead of 45 s.
//!
//! While [`Interpreter::ode_call`] evaluates the equations, a call to a *memoizable* function with plain-number
//! arguments that it has already made in this evaluation returns the remembered number. The cache is emptied at
//! the start of every evaluation (the state, t and any global can only change between evaluations), so a hit
//! returns exactly what the call would compute: the same bits, and no output or warning is lost (a run-time
//! warning is shown once per text anyway; an error stops the program on the first call).
//!
//! Memoizable: numbers in, a number out, and a body that can only compute: assignments to its own locals, if,
//! loops, return; arithmetic, comparisons, `where`, pure math built-ins, integrals and sums of such expressions,
//! and calls of memoizable functions. Nothing that prints, plots, pushes, draws random numbers, reads data or
//! solutions, or assigns a global. Only functions that integrate or sum (directly or through a call) are
//! remembered: for a cheap function the lookup costs about what the call does.

use std::collections::HashMap;

use fermium_ir::{Expr, ExprKind, FuncId, Module, Stmt, StmtKind, Ty};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

/// Built-ins that are plain functions of their number arguments.
const PURE_BUILTINS: &[&str] = &[
    "sin", "cos", "exp", "ln", "log", "log10", "log2", "abs", "floor", "ceil", "round", "tan", "asin", "acos",
    "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "erf", "erfc", "gamma", "lgamma", "expm1", "log1p",
    "cot", "sec", "csc", "sign", "sqrt", "min", "max", "atan2", "hypot", "mod", "isnan", "besselj", "bessely",
    "besseli", "besselk", "ellipk", "ellipe",
];

#[derive(Default)]
pub(crate) struct Memo {
    /// per function (computed on the first ODE evaluation): IMPURE, PURE (not worth remembering) or REMEMBER
    ok: Option<Vec<u8>>,
    /// remembering (inside an ODE right-hand side)
    active: bool,
    map: HashMap<(FuncId, Vec<u64>), f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Unknown,
    Busy,
    Pure { heavy: bool },
    Impure,
}

struct Analysis<'a> {
    m: &'a Module,
    st: Vec<State>,
}

impl Analysis<'_> {
    /// Some(heavy) when function f is pure
    fn func(&mut self, f: FuncId) -> Option<bool> {
        match self.st[f] {
            State::Pure { heavy } => return Some(heavy),
            State::Impure | State::Busy => return None, // recursion: not remembered
            State::Unknown => {}
        }
        self.st[f] = State::Busy;
        let func = &self.m.funcs[f];
        let mut heavy = false;
        let ok = self.stmts(&func.body, f, &mut heavy);
        self.st[f] = if ok { State::Pure { heavy } } else { State::Impure };
        ok.then_some(heavy)
    }

    fn local(&self, s: fermium_ir::SymId, f: FuncId) -> bool {
        self.m.syms[s].func == Some(f)
    }

    fn stmts(&mut self, ss: &[Stmt], f: FuncId, heavy: &mut bool) -> bool {
        ss.iter().all(|s| self.stmt(s, f, heavy))
    }

    fn stmt(&mut self, s: &Stmt, f: FuncId, heavy: &mut bool) -> bool {
        match &s.kind {
            StmtKind::Assign(sym, e) => self.local(*sym, f) && self.expr(e, f, heavy),
            StmtKind::If(c, a, b) => self.expr(c, f, heavy) && self.stmts(a, f, heavy) && self.stmts(b, f, heavy),
            StmtKind::While(c, body) => self.expr(c, f, heavy) && self.stmts(body, f, heavy),
            StmtKind::For { sym, lo, hi, step, body, parallel: false, .. } => {
                self.local(*sym, f)
                    && self.expr(lo, f, heavy)
                    && self.expr(hi, f, heavy)
                    && step.as_ref().is_none_or(|e| self.expr(e, f, heavy))
                    && self.stmts(body, f, heavy)
            }
            StmtKind::Return(e) => e.as_ref().is_none_or(|e| self.expr(e, f, heavy)),
            StmtKind::Break | StmtKind::Continue => true,
            StmtKind::Expr(e) => self.expr(e, f, heavy),
            _ => false,
        }
    }

    fn lambda(&mut self, l: usize, f: FuncId, heavy: &mut bool) -> bool {
        let m = self.m;
        m.lambdas[l].body.iter().all(|e| self.expr(e, f, heavy))
    }

    fn expr(&mut self, e: &Expr, f: FuncId, heavy: &mut bool) -> bool {
        match &e.kind {
            ExprKind::Const(_) | ExprKind::Bool(_) | ExprKind::Var(_) => true,
            ExprKind::Bin(_, a, b) | ExprKind::Pow(a, b) | ExprKind::Cmp(_, a, b) | ExprKind::Logic { a, b, .. } => {
                self.expr(a, f, heavy) && self.expr(b, f, heavy)
            }
            ExprKind::PowC(a, _) | ExprKind::Neg(a) | ExprKind::Not(a) => self.expr(a, f, heavy),
            ExprKind::Approx { a, b, atol, .. } => {
                self.expr(a, f, heavy) && self.expr(b, f, heavy) && atol.as_ref().is_none_or(|t| self.expr(t, f, heavy))
            }
            ExprKind::If(c, a, b) => self.expr(c, f, heavy) && self.expr(a, f, heavy) && self.expr(b, f, heavy),
            ExprKind::Let(binds, body) => {
                binds.iter().all(|(_, b)| self.expr(b, f, heavy)) && self.expr(body, f, heavy)
            }
            ExprKind::Call(g, args) => {
                args.iter().all(|a| self.expr(a, f, heavy))
                    && match self.func(*g) {
                        Some(h) => {
                            *heavy |= h;
                            true
                        }
                        None => false,
                    }
            }
            ExprKind::Builtin(name, args) => {
                PURE_BUILTINS.contains(&name.as_str()) && args.iter().all(|a| self.expr(a, f, heavy))
            }
            ExprKind::Integral { lam, lo, hi, atol, .. } => {
                *heavy = true;
                self.expr(lo, f, heavy)
                    && self.expr(hi, f, heavy)
                    && atol.as_ref().is_none_or(|t| self.expr(t, f, heavy))
                    && self.lambda(*lam, f, heavy)
            }
            ExprKind::Sum { lam, lo, hi, step } => {
                *heavy = true;
                self.expr(lo, f, heavy)
                    && self.expr(hi, f, heavy)
                    && step.as_ref().is_none_or(|t| self.expr(t, f, heavy))
                    && self.lambda(*lam, f, heavy)
            }
            _ => false,
        }
    }
}

const IMPURE: u8 = 0;
const PURE: u8 = 1;
const REMEMBER: u8 = 2;

/// Which functions are pure, and which may be remembered: pure, numbers in and out, and integrating or summing
/// somewhere.
fn memoizable(m: &Module) -> Vec<u8> {
    let mut a = Analysis { m, st: vec![State::Unknown; m.funcs.len()] };
    (0..m.funcs.len())
        .map(|f| {
            let func = &m.funcs[f];
            let scalar = matches!(func.ret_ty, Ty::Num(_)) && func.params.iter().all(|&p| matches!(m.syms[p].ty, Ty::Num(_)));
            match a.func(f) {
                None => IMPURE,
                Some(heavy) if heavy && scalar => REMEMBER,
                Some(_) => PURE,
            }
        })
        .collect()
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// Start remembering for one ODE right-hand side evaluation; returns the state to give to [`Self::memo_end`].
    pub(crate) fn memo_begin(&mut self) -> (bool, HashMap<(FuncId, Vec<u64>), f64>) {
        if self.memo.ok.is_none() {
            self.memo.ok = Some(memoizable(self.module));
        }
        let was = self.memo.active;
        self.memo.active = !crate::eval_unc::mc_active(); // Monte Carlo samples each read afresh
        (was, std::mem::take(&mut self.memo.map))
    }

    /// Stop remembering (restore an enclosing evaluation's cache).
    pub(crate) fn memo_end(&mut self, saved: (bool, HashMap<(FuncId, Vec<u64>), f64>)) {
        self.memo.active = saved.0;
        self.memo.map = saved.1;
    }

    /// A call, remembered while an ODE right-hand side is evaluated when the function is memoizable.
    pub(crate) fn call_expr(&mut self, f: FuncId, args: &[Expr], fr: &mut Frame) -> Result<Value, RunError> {
        let vals = self.eval_args(args, fr)?;
        if !self.memo.active {
            return self.call(f, vals);
        }
        let kind = self.memo.ok.as_ref().map_or(IMPURE, |ok| ok[f]);
        if kind != REMEMBER {
            let r = self.call(f, vals);
            if kind == IMPURE {
                self.memo.map.clear(); // it may do more than compute: forget what was remembered
            }
            return r;
        }
        let mut key = Vec::with_capacity(vals.len());
        for v in &vals {
            match v {
                Value::Num(x) => key.push(x.to_bits()),
                _ => return self.call(f, vals),
            }
        }
        let key = (f, key);
        if let Some(&r) = self.memo.map.get(&key) {
            crate::varmap::give_args(vals);
            return Ok(Value::Num(r));
        }
        let r = self.call(f, vals)?;
        if let Value::Num(x) = r {
            if self.memo.active {
                self.memo.map.insert(key, x);
            }
        }
        Ok(r)
    }
}
