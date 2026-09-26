//! Numeric evaluation of a formula at given values of its names, for checking antiderivatives (the native
//! counterpart of calculus._antiderivative_ok, which evaluated SymPy expressions). Units are ignored: a
//! quantity counts as its number, which is consistent within one check.
use std::collections::HashMap;

use fermium_syntax::ast as A;

type K = A::ExprKind;

/// The value of `e`, or None where it can't be evaluated as a real number (unknown names, 𝑖, lists, …).
pub fn eval(e: &A::Expr, env: &HashMap<String, f64>) -> Option<f64> {
    let ev = |x: &A::Expr| eval(x, env);
    Some(match &e.kind {
        K::Num { value, .. } => *value,
        K::Name { name } => match env.get(name) {
            Some(v) => *v,
            None if name == "π" => std::f64::consts::PI,
            None => return None,
        },
        K::Quantity { value, .. } => ev(value)?,
        K::Neg { operand } => -ev(operand)?,
        K::BinOp { op, left, right, .. } => {
            let (a, b) = (ev(left)?, ev(right)?);
            match op.as_str() {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                "^" => {
                    if a < 0.0 && b != b.trunc() {
                        return None;
                    }
                    a.powf(b)
                }
                _ => return None,
            }
        }
        K::Sqrt { operand, root } => {
            let x = ev(operand)?;
            if *root == 2 {
                x.sqrt()
            } else {
                x.cbrt()
            }
        }
        K::Abs { operand } => ev(operand)?.abs(),
        K::IfExpr { cond, then, other } => {
            if eval_cond(cond, env)? {
                ev(then)?
            } else {
                ev(other)?
            }
        }
        K::Call { func, args } => {
            let f = func.name()?;
            if args.len() != 1 {
                return None;
            }
            let x = ev(&args[0])?;
            match f {
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "cot" => 1.0 / x.tan(),
                "sec" => 1.0 / x.cos(),
                "csc" => 1.0 / x.sin(),
                "exp" => x.exp(),
                "ln" | "log" => x.ln(),
                "log10" => x.log10(),
                "log2" => x.log2(),
                "sqrt" => x.sqrt(),
                "cbrt" => x.cbrt(),
                "abs" => x.abs(),
                "sign" => {
                    if x > 0.0 {
                        1.0
                    } else if x < 0.0 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                "sinh" => x.sinh(),
                "cosh" => x.cosh(),
                "tanh" => x.tanh(),
                "asin" => x.asin(),
                "acos" => x.acos(),
                "atan" => x.atan(),
                "asinh" => x.asinh(),
                "acosh" => x.acosh(),
                "atanh" => x.atanh(),
                "erf" => erf(x),
                _ => return None,
            }
        }
        _ => return None,
    })
}

fn eval_cond(c: &A::Expr, env: &HashMap<String, f64>) -> Option<bool> {
    match &c.kind {
        K::Compare { op, left, right, .. } => {
            let (a, b) = (eval(left, env)?, eval(right, env)?);
            Some(match op.as_str() {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                ">=" => a >= b,
                "==" => a == b,
                "!=" => a != b,
                _ => return None,
            })
        }
        K::Logic { op, left, right } => {
            let (a, b) = (eval_cond(left, env)?, eval_cond(right, env)?);
            Some(if op == "and" { a && b } else { a || b })
        }
        _ => None,
    }
}

/// erf to about 1e-15 (series / continued fraction), enough for a consistency check.
fn erf(x: f64) -> f64 {
    if x.abs() < 2.5 {
        // Maclaurin series
        let mut sum = x;
        let mut term = x;
        let x2 = x * x;
        for n in 1..200 {
            term *= -x2 / n as f64;
            let t = term / (2 * n + 1) as f64;
            sum += t;
            if t.abs() < 1e-17 * sum.abs() {
                break;
            }
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else {
        // continued fraction for erfc
        let a = x.abs();
        let mut f = 0.0;
        for k in (1..60).rev() {
            f = (k as f64 / 2.0) / (a + f);
        }
        let erfc = (-a * a).exp() / std::f64::consts::PI.sqrt() / (a + f);
        if x > 0.0 { 1.0 - erfc } else { erfc - 1.0 }
    }
}
