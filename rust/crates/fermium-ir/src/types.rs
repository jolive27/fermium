//! Types and dimension inference: a port of `fermium/types.py`.
//!
//! Dimensions during checking are affine expressions over unknowns, `D = D0 · x1^a1 · x2^a2 …` (written
//! additively on exponents). Checking `a + b` unifies dim(a) and dim(b), a linear equation on the exponents,
//! solved by substitution (Kennedy-style inference): this is how `E = 0` followed by `E += ½ m v²` learns that E
//! is an energy, and how `fit T = 2π √(L/g)` works out that g is an acceleration.
use crate::dim::{Dim, DIMLESS};
use fermium_units::exact::{add_or_record, div_or_record, mul_or_record, sub_or_record};
use num_rational::Rational64;
use num_traits::{One, Signed, Zero};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

static IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DimVar(pub u64);

impl DimVar {
    pub fn fresh() -> DimVar {
        DimVar(IDS.fetch_add(1, Ordering::Relaxed))
    }
}

/// const Dim + Σ coeff·var (a product of powers, written on exponents).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DExpr {
    pub konst: Dim,
    pub terms: BTreeMap<DimVar, Rational64>,
}

impl DExpr {
    pub fn of(d: Dim) -> DExpr {
        DExpr { konst: d, terms: BTreeMap::new() }
    }
    pub fn var(v: DimVar) -> DExpr {
        let mut t = BTreeMap::new();
        t.insert(v, Rational64::one());
        DExpr { konst: DIMLESS, terms: t }
    }
    pub fn fresh() -> DExpr {
        DExpr::var(DimVar::fresh())
    }
    pub fn dimless() -> DExpr {
        DExpr::of(DIMLESS)
    }
    fn clean(mut self) -> DExpr {
        self.terms.retain(|_, v| !v.is_zero());
        self
    }
    pub fn mul(&self, o: &DExpr) -> DExpr {
        let mut t = self.terms.clone();
        for (k, v) in &o.terms {
            let e = t.entry(*k).or_insert_with(Rational64::zero);
            *e = add_or_record(*e, *v, None);
        }
        DExpr { konst: self.konst * o.konst, terms: t }.clean()
    }
    pub fn div(&self, o: &DExpr) -> DExpr {
        let mut t = self.terms.clone();
        for (k, v) in &o.terms {
            let e = t.entry(*k).or_insert_with(Rational64::zero);
            *e = sub_or_record(*e, *v, None);
        }
        DExpr { konst: self.konst / o.konst, terms: t }.clean()
    }
    pub fn pow(&self, p: Rational64) -> DExpr {
        DExpr { konst: self.konst.pow(p), terms: self.terms.iter().map(|(k, v)| (*k, mul_or_record(*v, p, None))).collect() }.clean()
    }
    pub fn is_concrete(&self) -> bool {
        self.terms.is_empty()
    }
}

/// Holds the substitution for dimension variables.
#[derive(Clone, Debug, Default)]
pub struct Unifier {
    pub subst: BTreeMap<DimVar, DExpr>,
}

impl Unifier {
    pub fn norm(&self, d: &DExpr) -> DExpr {
        if d.terms.is_empty() {
            return d.clone();
        }
        let mut out = DExpr::of(d.konst);
        for (var, c) in &d.terms {
            if let Some(s) = self.subst.get(var) {
                out = out.mul(&self.norm(s).pow(*c));
            } else {
                out = out.mul(&DExpr::var(*var).pow(*c));
            }
        }
        out
    }

    /// Make a == b; false if impossible.
    pub fn unify(&mut self, a: &DExpr, b: &DExpr) -> bool {
        let diff = self.norm(&a.div(b));
        if diff.terms.is_empty() {
            return diff.konst.is_dimensionless();
        }
        // the variable with the "simplest" coefficient: |c| == 1 first, then the oldest
        let (&var, &c) = diff.terms.iter().min_by_key(|(k, v)| (v.abs() != Rational64::one(), k.0)).unwrap();
        let mut rest = diff.clone();
        rest.terms.remove(&var);
        // c·var + rest = 0  →  var = -rest / c
        self.subst.insert(var, rest.pow(div_or_record(Rational64::new(-1, 1), c, None)));
        true
    }

    /// The concrete Dim, any still-unknown variable defaulting to dimensionless.
    pub fn resolve(&self, d: &DExpr) -> Dim {
        self.norm(d).konst
    }

    pub fn is_concrete(&self, d: &DExpr) -> bool {
        self.norm(d).is_concrete()
    }
}

/// A value type (the Python `Ty` classes).
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Num(DExpr),
    Bool,
    Str,
    List(DExpr),
    /// A small vector: one shared dimension, or one per component (mixed, D29).
    Vec { n: usize, dim: Option<DExpr>, dims: Option<Vec<DExpr>> },
    /// r × c entries sharing one dimension (D29, D195).
    Mat { r: usize, c: usize, dim: DExpr },
    /// A complex quantity, stored like a 2-vector (D90).
    Complex(DExpr),
    /// A list of complex numbers sharing one unit (D243).
    ComplexList(DExpr),
    TextList,
    /// Handle to an ODE solution; the index is into the checker's solution table.
    Sol(usize),
    /// A data table (load, table(...)); index into the data table registry.
    Data(usize),
    Void,
}

impl Ty {
    pub fn num(d: Dim) -> Ty {
        Ty::Num(DExpr::of(d))
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Ty::Num(_) => "num",
            Ty::Bool => "bool",
            Ty::Str => "str",
            Ty::List(_) => "list",
            Ty::Vec { .. } => "vec",
            Ty::Mat { .. } => "mat",
            Ty::Complex(_) => "cplx",
            Ty::ComplexList(_) => "clist",
            Ty::TextList => "textlist",
            Ty::Sol(_) => "sol",
            Ty::Data(_) => "data",
            Ty::Void => "void",
        }
    }
}
