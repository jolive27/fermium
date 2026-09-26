//! The evaluator, vectors and matrices: the operations the compiled v1.5 path runs (codegen_llvm.py's
//! matrix_op, e_IVecIndex, e_IVecSet and the vdot/norm/unit/cross built-ins), with the dense linear algebra of
//! fermium-runtime (a port of v1's linalg.py / linalg_big.py, operation for operation).
//! Each function here is called by the dispatch in eval.rs / eval_more.rs; `None` from a builtin_* means
//! "not mine".
use std::rc::Rc;

use fermium_ir::{Expr, ExprKind};
use fermium_runtime::numerics::{err, linalg, Fail};

use crate::eval::{Frame, Interpreter, Printer, RunError, Value};

/// Python format_number(x) (6 significant figures, trimmed).
pub(crate) fn fmt6(x: f64) -> String {
    fermium_units::numfmt::format_number(x, 6, true)
}

fn vals(v: &Value) -> &[f64] {
    match v {
        Value::Vec(x) => x,
        _ => &[],
    }
}

fn vecv(x: Vec<f64>) -> Value {
    Value::Vec(Rc::new(x))
}

fn isqrt(n: usize) -> usize {
    (n as f64).sqrt().round() as usize
}

impl<'m, P: Printer> Interpreter<'m, P> {
    /// A run-time failure with v1's error kind (core.py describe_error).
    pub(crate) fn fail_kind(&self, f: Fail) -> RunError {
        let msg = match f.kind {
            err::SINGULAR => "this matrix is singular (its determinant is 0), so it has no inverse and M x = b has no \
                              unique solution".to_string(),
            err::NOT_SYMMETRIC => "eigenvalues and eigenvectors need a symmetric matrix (M[i, j] = M[j, i]), like a \
                                   stiffness or mass matrix; for K v = ω² M v write eigenvalues(K, M) rather than \
                                   eigenvalues(inverse(M) K)".to_string(),
            err::NOT_POSDEF => "in eigenvalues(K, M) the second matrix M must be positive definite, like a mass matrix \
                                (positive masses on the diagonal)".to_string(),
            5 => format!("these two lists have different lengths ({} and {})", f.a as i64, f.b as i64),
            6 => "this list is empty".to_string(),
            err::SIZE if f.a.is_nan() => "the length of a list must be a number, not NaN".to_string(),
            err::SIZE => format!("not enough memory for a list of {} numbers (the most is 10⁹)", fmt6(f.a)),
            err::INDEX => {
                let (a, n) = (f.a, f.b as i64);
                if a.is_nan() {
                    "a list index must be a whole number (1, 2, 3, ...), not NaN".to_string()
                } else if n < 0 {
                    // a vector's component or a matrix's row/column picked at run time (#54)
                    if a.is_finite() && a != a.trunc() {
                        format!("an index must be a whole number (1, 2, 3, ...), not {}", fmt6(a))
                    } else {
                        format!("index {} is out of range: valid indexes here are 1 to {}{}", fmt6(a), -n,
                                if a == 0.0 { "; Fermium counts from 1" } else { "" })
                    }
                } else if a.is_finite() && a != a.trunc() {
                    format!("a list index must be a whole number (1, 2, 3, ...), not {}", fmt6(a))
                } else if n == 0 {
                    format!("index {} is out of range: the list is empty", fmt6(a))
                } else {
                    format!("index {} is out of range: the list has {n} element{} (valid indexes are 1 to {n}){}",
                            fmt6(a), if n != 1 { "s" } else { "" },
                            if a == 0.0 { "; Fermium counts from 1, so the first element is [1]" } else { "" })
                }
            }
            _ => "runtime error".to_string(),
        };
        RunError { message: msg, line: self.line, hint: None }
    }

    /// The flat offset Σ (i − 1)·stride of checked 1-based run-time indexes (e_IVecIndex / e_IVecSet).
    fn flat_base(&mut self, idxs: &[(Expr, usize, usize)], fr: &mut Frame) -> Result<usize, RunError> {
        let mut base = 0usize;
        for (ie, size, stride) in idxs {
            let idx = self.eval(ie, fr)?;
            let idx = self.plain(&idx)?;
            let inside = idx >= 1.0 && idx <= *size as f64;
            let i = if inside { idx as i64 } else { 1 };
            if !inside || i as f64 != idx {
                return Err(self.fail_kind(Fail::new(err::INDEX, idx, -(*size as f64))));
            }
            base += (i as usize - 1) * stride;
        }
        Ok(base)
    }

    pub(crate) fn builtin_vecmat(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, RunError>> {
        let num = |k: usize| args[k].num();
        Some(Ok(match name {
            "shuffle" => {
                let src = vals(&args[0]);
                let out: Vec<f64> = args[1..].iter().map(|k| src[k.num() as usize]).collect();
                vecv(out)
            }
            "matmul" => {
                let (r, k, c) = (num(2) as usize, num(3) as usize, num(4) as usize);
                let out = linalg::matmul(vals(&args[0]), r, k, vals(&args[1]), c);
                if out.len() == 1 { Value::Num(out[0]) } else { vecv(out) }
            }
            "det" => {
                let a = vals(&args[0]);
                Value::Num(linalg::det(a, isqrt(a.len())))
            }
            "inverse" => {
                let a = vals(&args[0]);
                match linalg::inverse(a, isqrt(a.len())) {
                    Ok(x) => vecv(x),
                    Err(f) => return Some(Err(self.fail_kind(f))),
                }
            }
            "solve_linear" => {
                let (a, b) = (vals(&args[0]), vals(&args[1]));
                match linalg::solve(a, b.len(), b, 1) {
                    Ok(x) => vecv(x),
                    Err(f) => return Some(Err(self.fail_kind(f))),
                }
            }
            "eigenvalues" | "eigenvectors" => {
                let k = vals(&args[0]);
                let n = isqrt(k.len());
                let r = if args.len() == 1 { linalg::eigen_sym(k, n) } else { linalg::eigen_general(k, vals(&args[1]), n) };
                match r {
                    Ok((values, vectors)) => vecv(if name == "eigenvalues" { values } else { vectors }),
                    Err(f) => return Some(Err(self.fail_kind(f))),
                }
            }
            "vdot" => {
                let (a, b) = (vals(&args[0]), vals(&args[1]));
                let mut acc = a[0] * b[0];
                for k in 1..a.len() {
                    acc += a[k] * b[k];
                }
                Value::Num(acc)
            }
            "cross" => {
                let (x, y) = (vals(&args[0]), vals(&args[1]));
                if x.len() == 2 {
                    Value::Num(x[0] * y[1] - x[1] * y[0])
                } else {
                    vecv(vec![x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]])
                }
            }
            "norm" | "unit" => {
                let a = vals(&args[0]);
                if a.is_empty() {
                    return None;
                }
                let mut acc = a[0] * a[0];
                for x in &a[1..] {
                    acc += x * x;
                }
                let nrm = acc.sqrt();
                if name == "norm" { Value::Num(nrm) } else { vecv(a.iter().map(|x| x / nrm).collect()) }
            }
            "approx" => {
                let (atol, rtol) = (num(2), num(3));
                let (a, b): (Vec<f64>, Vec<f64>) = match (&args[0], &args[1]) {
                    (Value::Vec(a), Value::Vec(b)) => (a.to_vec(), b.to_vec()),
                    _ => (vec![num(0)], vec![num(1)]),
                };
                Value::Bool(approx_real(&a, &b, atol, rtol))
            }
            _ => return None,
        }))
    }

    pub(crate) fn eval_vecmat(&mut self, e: &Expr, fr: &mut Frame) -> Result<Value, RunError> {
        match &e.kind {
            ExprKind::VecIndex { v, idxs, offs } => {
                let v = self.eval(v, fr)?;
                let base = self.flat_base(idxs, fr)?;
                if let Value::UVec(src) = &v {
                    let xs: Vec<Value> = offs.iter().map(|o| src[base + o].clone()).collect();
                    return Ok(if xs.len() == 1 { xs[0].clone() } else { crate::eval_unc::make_vec(xs) });
                }
                let src = vals(&v);
                let xs: Vec<f64> = offs.iter().map(|o| src[base + o]).collect();
                Ok(if xs.len() == 1 { Value::Num(xs[0]) } else { vecv(xs) })
            }
            ExprKind::VecSet { v, idxs, value } => {
                let v = self.eval(v, fr)?;
                let x = self.eval(value, fr)?;
                let base = self.flat_base(idxs, fr)?;
                if matches!(v, Value::UVec(_)) || matches!(x, Value::Unc(_)) {
                    // v[:base] + (x,) + v[base + 1:]
                    let mut out = crate::eval_unc::vec_items(&v).unwrap_or_default();
                    out[base] = x;
                    return Ok(crate::eval_unc::make_vec(out));
                }
                let x = x.num();
                let mut out = vals(&v).to_vec();
                out[base] = x;
                Ok(vecv(out))
            }
            _ => self.err("this isn't supported by the Rust back end yet"),
        }
    }
}

/// Julia's isapprox (D260) as cplx.approx_core: a == b, or |a − b| ≤ max(atol, rtol·max(|a|, |b|)) with a finite
/// difference.
pub(crate) fn approx_core(same: bool, diff: f64, sa: f64, sb: f64, atol: f64, rtol: f64) -> bool {
    let tol = atol.max(rtol * sa.max(sb));
    let close = diff <= tol && diff != f64::INFINITY;
    !(!same && !close)
}

/// ≈ for numbers (one-element slices) and vectors (norms summed in order; cplx.k_approx_real).
pub(crate) fn approx_real(a: &[f64], b: &[f64], atol: f64, rtol: f64) -> bool {
    if a.len() == 1 {
        return approx_core(a[0] == b[0], (a[0] - b[0]).abs(), a[0].abs(), b[0].abs(), atol, rtol);
    }
    let norm = |xs: &[f64]| {
        let mut s = xs[0] * xs[0];
        for x in &xs[1..] {
            s += x * x;
        }
        s.sqrt()
    };
    let same = a.iter().zip(b).all(|(x, y)| x == y);
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    approx_core(same, norm(&d), norm(a), norm(b), atol, rtol)
}
