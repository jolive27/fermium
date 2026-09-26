//! Linear algebra on matrices of uncertain values (B-U1). v1 runs a program with uncertainties in its
//! interpreter, whose matrix_op calls linalg.py (up to 4×4) or linalg_big.py (larger) with FloatOps on UFloat
//! values; the same operations in the same order here, on Values that are plain or uncertain numbers.
//! Comparisons (pivot choice, the zero-pivot test, the symmetry test) are by nominal value, as UFloat's are.
use std::rc::Rc;

use fermium_ir::BinOp;
use fermium_runtime::numerics::linalg;

use crate::eval::Value;
use crate::eval_unc::{num_op, vec_fail};

fn add(a: &Value, b: &Value) -> Value {
    num_op(BinOp::Add, a, b)
}

fn sub(a: &Value, b: &Value) -> Value {
    num_op(BinOp::Sub, a, b)
}

fn mul(a: &Value, b: &Value) -> Value {
    num_op(BinOp::Mul, a, b)
}

fn div(a: &Value, b: &Value) -> Value {
    num_op(BinOp::Div, a, b)
}

fn neg(a: &Value) -> Value {
    match a {
        Value::Unc(u) => Value::Unc(Rc::new(u.neg())),
        Value::Arr(x) => Value::Arr(Rc::new(x.iter().map(|y| -y).collect())),
        v => Value::Num(-v.num()),
    }
}

pub(crate) fn abs(a: &Value) -> Value {
    match a {
        Value::Unc(u) => Value::Unc(Rc::new(u.abs())),
        Value::Arr(x) => Value::Arr(Rc::new(x.iter().map(|y| y.abs()).collect())),
        v => Value::Num(v.num().abs()),
    }
}

/// The nominal value, which UFloat's comparisons use (NumPy samples can't be compared: one sample at a time).
pub(crate) fn nom(v: &Value) -> f64 {
    match v {
        Value::Unc(u) => u.v,
        Value::Arr(_) => {
            vec_fail();
            f64::NAN
        }
        v => v.num(),
    }
}

/// ops.gt_abs: |x| > |y|
fn gt_abs(x: &Value, y: &Value) -> bool {
    nom(&abs(x)) > nom(&abs(y))
}

/// linalg.det: cofactor expansion along the first row (n <= 4).
fn det_small(a: &[Value], n: usize) -> Value {
    if n == 1 {
        return a[0].clone();
    }
    if n == 2 {
        return sub(&mul(&a[0], &a[3]), &mul(&a[1], &a[2]));
    }
    let mut acc: Option<Value> = None;
    for j in 0..n {
        let minor: Vec<Value> =
            (1..n).flat_map(|i| (0..n).filter(move |&m| m != j).map(move |m| (i, m))).map(|(i, m)| a[i * n + m].clone())
                  .collect();
        let term = mul(&a[j], &det_small(&minor, n - 1));
        acc = Some(match acc {
            None => term,
            Some(s) if j % 2 == 1 => sub(&s, &term),
            Some(s) => add(&s, &term),
        });
    }
    acc.unwrap()
}

/// linalg.solve: Gaussian elimination with partial pivoting; (X flat n×m, pivots).
fn solve_small(a: &[Value], n: usize, b: &[Value], m: usize) -> (Vec<Value>, Vec<Value>) {
    let w = n + m;
    let mut rows: Vec<Vec<Value>> =
        (0..n).map(|i| (0..n).map(|j| a[i * n + j].clone()).chain((0..m).map(|j| b[i * m + j].clone())).collect())
              .collect();
    let mut pivots = Vec::with_capacity(n);
    for k in 0..n {
        for i in k + 1..n {
            if gt_abs(&rows[i][k], &rows[k][k]) {
                for j in k..w {
                    let x = std::mem::replace(&mut rows[k][j], Value::Num(0.0));
                    let y = std::mem::replace(&mut rows[i][j], x);
                    rows[k][j] = y;
                }
            }
        }
        let p = rows[k][k].clone();
        for i in k + 1..n {
            let f = div(&rows[i][k], &p);
            for j in k + 1..w {
                rows[i][j] = sub(&rows[i][j], &mul(&f, &rows[k][j]));
            }
        }
        pivots.push(p);
    }
    let mut x: Vec<Vec<Value>> = vec![vec![Value::Num(0.0); m]; n];
    for i in (0..n).rev() {
        for j in 0..m {
            let mut acc = rows[i][n + j].clone();
            for q in i + 1..n {
                acc = sub(&acc, &mul(&rows[i][q], &x[q][j]));
            }
            x[i][j] = div(&acc, &rows[i][i]);
        }
    }
    (x.into_iter().flatten().collect(), pivots)
}

/// linalg_big.solve: (X flat n×m, [the smallest |pivot|], det A).
fn solve_big(a: &[Value], n: usize, b: &[Value], m: usize) -> (Vec<Value>, Vec<Value>, Value) {
    let w = n + m;
    let mut wm: Vec<Value> = Vec::with_capacity(n * w);
    for i in 0..n {
        wm.extend(a[i * n..i * n + n].iter().cloned());
        wm.extend(b[i * m..i * m + m].iter().cloned());
    }
    let mut sign = Value::Num(1.0);
    let mut piv: Vec<Value> = Vec::with_capacity(n);
    for k in 0..n {
        let mut best = k;
        for i in k + 1..n {
            if gt_abs(&wm[i * w + k], &wm[best * w + k]) {
                best = i;
            }
        }
        let p = best;
        for j in k..w {
            wm.swap(k * w + j, p * w + j);
        }
        sign = if p == k { sign } else { neg(&sign) };
        let pk = wm[k * w + k].clone();
        for i in k + 1..n {
            let f = div(&wm[i * w + k], &pk);
            for j in k + 1..w {
                wm[i * w + j] = sub(&wm[i * w + j], &mul(&f, &wm[k * w + j]));
            }
        }
        piv.push(pk);
    }
    let mut x = vec![Value::Num(0.0); n * m];
    for t in 0..n {
        let i = n - 1 - t;
        for j in 0..m {
            let mut acc = wm[i * w + n + j].clone();
            for q in i + 1..n {
                acc = sub(&acc, &mul(&wm[i * w + q], &x[q * m + j]));
            }
            x[i * m + j] = div(&acc, &wm[i * w + i]);
        }
    }
    let mut acc = sign;
    let mut small = abs(&piv[0]);
    for pk in &piv {
        acc = mul(&acc, pk);
        let a = abs(pk);
        if nom(&a) < nom(&small) {
            small = a;
        }
    }
    (x, vec![small], acc)
}

/// What a matrix operation on uncertain values gives.
pub(crate) enum LaOut {
    Val(Value),
    Singular,
    NotSymmetric,
    NotPosDef,
    /// v1 needs a plain number here (math.sqrt of a UFloat in the Jacobi rotations or the Cholesky factor)
    Generic,
}

fn isqrt(n: usize) -> usize {
    (n as f64).sqrt().round() as usize
}

/// det, inverse, solve_linear, eigenvalues and eigenvectors (interp.matrix_op) on matrices given as flat
/// row-major components; `None` for other names.
pub(crate) fn unc_matrix_op(name: &str, mats: &[Vec<Value>]) -> Option<LaOut> {
    let a = &mats[0];
    let n = if name == "solve_linear" { mats[1].len() } else { isqrt(a.len()) };
    let big = n > 4;
    let singular = |piv: &[Value]| piv.iter().any(|p| nom(p) == 0.0);
    let vecv = |x: Vec<Value>| crate::eval_unc::make_vec(x);
    Some(match name {
        "det" => LaOut::Val(if big { solve_big(a, n, &vec![Value::Num(0.0); n], 1).2 } else { det_small(a, n) }),
        "inverse" | "solve_linear" => {
            let (b, m) = if name == "inverse" {
                ((0..n * n).map(|k| Value::Num(if k / n == k % n { 1.0 } else { 0.0 })).collect(), n)
            } else {
                (mats[1].clone(), 1)
            };
            let (x, piv) = if big {
                let (x, piv, _) = solve_big(a, n, &b, m);
                (x, piv)
            } else {
                solve_small(a, n, &b, m)
            };
            if singular(&piv) { LaOut::Singular } else { LaOut::Val(vecv(x)) }
        }
        "eigenvalues" | "eigenvectors" => {
            // the symmetry test compares nominal values, computed exactly as on plain numbers
            if mats.iter().any(|m| !linalg::is_symmetric(&m.iter().map(nom).collect::<Vec<_>>(), n)) {
                return Some(LaOut::NotSymmetric);
            }
            let r = if mats.len() == 1 {
                jacobi_eigen(a, n).map(|(v, x)| (v, x, vec![]))
            } else if big {
                generalized_big(a, &mats[1], n)
            } else {
                generalized_small(a, &mats[1], n)
            };
            match r {
                // math.sqrt of a UFloat with a value >= 0 needs a plain number
                Err(()) => LaOut::Generic,
                Ok((_, _, piv)) if piv.iter().any(|p| nom(p) <= 0.0) => LaOut::NotPosDef,
                Ok((vals, vecs, _)) => LaOut::Val(vecv(if name == "eigenvalues" { vals } else { vecs })),
            }
        }
        _ => return None,
    })
}

/// FloatOps.sqrt: `math.sqrt(x) if x >= 0 else (x if x != x else nan)`; math.sqrt of a UFloat raises (Err).
fn sqrt(x: &Value) -> Result<Value, ()> {
    match x {
        Value::Arr(_) => {
            vec_fail();
            Ok(Value::Num(f64::NAN))
        }
        Value::Unc(u) if u.v >= 0.0 => Err(()),
        Value::Unc(u) if u.v.is_nan() => Ok(x.clone()),
        Value::Unc(_) => Ok(Value::Num(f64::NAN)),
        v => {
            let y = v.num();
            Ok(Value::Num(if y >= 0.0 { y.sqrt() } else if y.is_nan() { y } else { f64::NAN }))
        }
    }
}

const ZERO: Value = Value::Num(0.0);
const ONE: Value = Value::Num(1.0);

/// Cyclic Jacobi rotations on the flat symmetric n×n array `am` (its diagonal ends up holding the eigenvalues);
/// returns V (columns: eigenvectors). linalg.jacobi_eigen's loop and linalg_big._jacobi's (the same operations).
fn jacobi(am: &mut [Value], n: usize, sweeps: usize) -> Result<Vec<Value>, ()> {
    let mut v: Vec<Value> = (0..n * n).map(|k| if k / n == k % n { ONE } else { ZERO }).collect();
    for _ in 0..sweeps {
        for p in 0..n {
            for q in p + 1..n {
                let (apq, app, aqq) = (am[p * n + q].clone(), am[p * n + p].clone(), am[q * n + q].clone());
                let theta = div(&sub(&aqq, &app), &add(&apq, &apq));
                let sgn = if nom(&theta) < 0.0 { neg(&ONE) } else { ONE };
                let t = div(&sgn, &add(&abs(&theta), &sqrt(&add(&mul(&theta, &theta), &ONE))?));
                let t = if nom(&apq) == 0.0 { ZERO } else { t };
                let c = div(&ONE, &sqrt(&add(&mul(&t, &t), &ONE))?);
                let s = mul(&t, &c);
                am[p * n + p] = sub(&app, &mul(&t, &apq));
                am[q * n + q] = add(&aqq, &mul(&t, &apq));
                am[p * n + q] = ZERO;
                am[q * n + p] = ZERO;
                for r in 0..n {
                    if r != p && r != q {
                        let (arp, arq) = (am[r * n + p].clone(), am[r * n + q].clone());
                        let nrp = sub(&mul(&c, &arp), &mul(&s, &arq));
                        let nrq = add(&mul(&s, &arp), &mul(&c, &arq));
                        am[r * n + p] = nrp.clone();
                        am[p * n + r] = nrp;
                        am[r * n + q] = nrq.clone();
                        am[q * n + r] = nrq;
                    }
                    let (vrp, vrq) = (v[r * n + p].clone(), v[r * n + q].clone());
                    v[r * n + p] = sub(&mul(&c, &vrp), &mul(&s, &vrq));
                    v[r * n + q] = add(&mul(&s, &vrp), &mul(&c, &vrq));
                }
            }
        }
    }
    Ok(v)
}

/// _sort_and_fix_signs (linalg and linalg_big alike): ascending values (a bubble-sort network that swaps the
/// columns of the flat `v` along), then each column's largest-magnitude entry made positive.
fn sort_and_fix_signs(mut vals: Vec<Value>, mut v: Vec<Value>, n: usize) -> (Vec<Value>, Vec<Value>) {
    for i in 0..n {
        for j in 0..n - 1 - i {
            if nom(&vals[j + 1]) < nom(&vals[j]) {
                vals.swap(j, j + 1);
                for r in 0..n {
                    v.swap(r * n + j, r * n + j + 1);
                }
            }
        }
    }
    for j in 0..n {
        let mut big = v[j].clone();
        for r in 1..n {
            if gt_abs(&v[r * n + j], &big) {
                big = v[r * n + j].clone();
            }
        }
        if nom(&big) < 0.0 {
            for r in 0..n {
                v[r * n + j] = neg(&v[r * n + j]);
            }
        }
    }
    (vals, v)
}

/// The symmetrised matrix (a + aᵀ)/2, the diagonal kept.
fn symmetrised(a: &[Value], n: usize) -> Vec<Value> {
    (0..n * n)
        .map(|k| {
            let (i, j) = (k / n, k % n);
            if i == j { a[k].clone() } else { mul(&Value::Num(0.5), &add(&a[i * n + j], &a[j * n + i])) }
        })
        .collect()
}

type Eig = (Vec<Value>, Vec<Value>);

/// jacobi_eigen: 10 sweeps up to 4×4 (linalg), 16 beyond (linalg_big).
fn jacobi_eigen(a: &[Value], n: usize) -> Result<Eig, ()> {
    let mut am = symmetrised(a, n);
    let v = jacobi(&mut am, n, if n > 4 { 16 } else { 10 })?;
    let vals = (0..n).map(|i| am[i * n + i].clone()).collect();
    Ok(sort_and_fix_signs(vals, v, n))
}

/// Cholesky M = L Lᵀ (flat L) and the pivots d_j before their square roots.
fn cholesky(m: &[Value], n: usize) -> Result<(Vec<Value>, Vec<Value>), ()> {
    let mut l = vec![ZERO; n * n];
    let mut piv = Vec::with_capacity(n);
    for j in 0..n {
        let mut d = m[j * n + j].clone();
        for k in 0..j {
            d = sub(&d, &mul(&l[j * n + k], &l[j * n + k]));
        }
        l[j * n + j] = sqrt(&d)?;
        piv.push(d);
        for i in j + 1..n {
            let mut x = mul(&Value::Num(0.5), &add(&m[i * n + j], &m[j * n + i]));
            for k in 0..j {
                x = sub(&x, &mul(&l[i * n + k], &l[j * n + k]));
            }
            l[i * n + j] = div(&x, &l[j * n + j]);
        }
    }
    Ok((l, piv))
}

/// L y = b for lower-triangular L.
fn forward(l: &[Value], b: &[Value], n: usize) -> Vec<Value> {
    let mut y: Vec<Value> = Vec::with_capacity(n);
    for i in 0..n {
        let mut acc = b[i].clone();
        for k in 0..i {
            acc = sub(&acc, &mul(&l[i * n + k], &y[k]));
        }
        y.push(div(&acc, &l[i * n + i]));
    }
    y
}

/// Lᵀ x = b for lower-triangular L.
fn backward_t(l: &[Value], b: &[Value], n: usize) -> Vec<Value> {
    let mut x = vec![ZERO; n];
    for i in (0..n).rev() {
        let mut acc = b[i].clone();
        for k in i + 1..n {
            acc = sub(&acc, &mul(&l[k * n + i], &x[k]));
        }
        x[i] = div(&acc, &l[i * n + i]);
    }
    x
}

/// The unit vector v / √(Σ v²), the sum in v's order.
fn unit_len(v: Vec<Value>) -> Result<Vec<Value>, ()> {
    let mut s = mul(&v[0], &v[0]);
    for x in &v[1..] {
        s = add(&s, &mul(x, x));
    }
    let norm = sqrt(&s)?;
    Ok(v.iter().map(|x| div(x, &norm)).collect())
}

type GenEig = (Vec<Value>, Vec<Value>, Vec<Value>);

/// linalg.generalized_eigen: (values, vectors, the Cholesky pivots).
fn generalized_small(k: &[Value], m: &[Value], n: usize) -> Result<GenEig, ()> {
    let (l, piv) = cholesky(m, n)?;
    let col = |src: &dyn Fn(usize) -> Value| (0..n).map(src).collect::<Vec<_>>();
    let cols: Vec<Vec<Value>> = (0..n).map(|j| forward(&l, &col(&|i| k[i * n + j].clone()), n)).collect();
    let acols: Vec<Vec<Value>> = (0..n).map(|j| forward(&l, &col(&|i| cols[i][j].clone()), n)).collect();
    let a: Vec<Value> = (0..n * n).map(|q| acols[q % n][q / n].clone()).collect();
    let (vals, y) = jacobi_eigen(&a, n)?;
    let mut out: Vec<Vec<Value>> = Vec::with_capacity(n);
    for j in 0..n {
        out.push(unit_len(backward_t(&l, &col(&|i| y[i * n + j].clone()), n))?);
    }
    let flat: Vec<Value> = (0..n * n).map(|q| out[q % n][q / n].clone()).collect();
    let (vals, vecs) = sort_and_fix_signs(vals, flat, n);
    Ok((vals, vecs, piv))
}

/// Column j of dst = L⁻¹ (column j of src, or row j when transposed).
fn fwd_big(l: &[Value], src: &[Value], n: usize, transposed: bool) -> Vec<Value> {
    let mut dst = vec![ZERO; n * n];
    for j in 0..n {
        for i in 0..n {
            let mut acc = if transposed { src[j * n + i].clone() } else { src[i * n + j].clone() };
            for q in 0..i {
                acc = sub(&acc, &mul(&l[i * n + q], &dst[q * n + j]));
            }
            dst[i * n + j] = div(&acc, &l[i * n + i]);
        }
    }
    dst
}

/// linalg_big.generalized_eigen: (values, vectors, [the smallest Cholesky pivot]).
fn generalized_big(k: &[Value], m: &[Value], n: usize) -> Result<GenEig, ()> {
    let (l, piv) = cholesky(m, n)?;
    let mut small = m[0].clone();
    for d in &piv {
        if nom(d) < nom(&small) {
            small = d.clone();
        }
    }
    let y = fwd_big(&l, k, n, false); // Y = L⁻¹ K
    let mut am = fwd_big(&l, &y, n, true); // A = L⁻¹ Yᵀ
    let v = jacobi(&mut am, n, 16)?;
    let vals: Vec<Value> = (0..n).map(|i| am[i * n + i].clone()).collect();
    let mut x = vec![ZERO; n * n];
    for j in 0..n {
        for t in 0..n {
            let i = n - 1 - t;
            let mut acc = v[i * n + j].clone();
            for q in i + 1..n {
                acc = sub(&acc, &mul(&l[q * n + i], &x[q * n + j]));
            }
            x[i * n + j] = div(&acc, &l[i * n + i]);
        }
        let unit = unit_len((0..n).map(|r| x[r * n + j].clone()).collect())?;
        for (r, c) in unit.into_iter().enumerate() {
            x[r * n + j] = c;
        }
    }
    let (vals, vecs) = sort_and_fix_signs(vals, x, n);
    Ok((vals, vecs, vec![small]))
}
