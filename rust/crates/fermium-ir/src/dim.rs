//! A stand-in for `fermium_units::Dim` (same shape, agreed with the units crate) until that crate lands; then
//! this module becomes `pub use fermium_units::{Dim, DIMLESS};`.
use num_rational::Rational64;
use std::ops::{Div, Mul};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Dim(pub [Rational64; 7]);

pub const DIMLESS: Dim = Dim([Rational64::new_raw(0, 1); 7]);

impl Dim {
    pub fn base(i: usize) -> Dim {
        let mut e = DIMLESS.0;
        e[i] = Rational64::from_integer(1);
        Dim(e)
    }
    pub fn pow(self, p: Rational64) -> Dim {
        let mut e = self.0;
        for x in e.iter_mut() {
            *x *= p;
        }
        Dim(e)
    }
    pub fn is_dimensionless(&self) -> bool {
        self.0.iter().all(|x| *x == Rational64::from_integer(0))
    }
}

impl Mul for Dim {
    type Output = Dim;
    fn mul(self, o: Dim) -> Dim {
        let mut e = self.0;
        for (a, b) in e.iter_mut().zip(o.0) {
            *a += b;
        }
        Dim(e)
    }
}

impl Div for Dim {
    type Output = Dim;
    fn div(self, o: Dim) -> Dim {
        let mut e = self.0;
        for (a, b) in e.iter_mut().zip(o.0) {
            *a -= b;
        }
        Dim(e)
    }
}
