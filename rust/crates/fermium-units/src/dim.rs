//! Dimensions: vectors of 7 rational exponents over the SI base quantities (port of `Dim` in
//! fermium/units.py).

use num_rational::Rational64;
use num_traits::{One, Zero};
use std::fmt;
use std::ops::{Div, Mul};

/// The SI base unit symbols, in the order of `Dim`'s exponents.
pub const BASE_SYMBOLS: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];
/// The SI base quantities, in the order of `Dim`'s exponents.
pub const BASE_NAMES: [&str; 7] = ["length", "mass", "time", "current", "temperature", "amount", "luminosity"];

/// A dimension: exponents of (length, mass, time, current, temperature, amount, luminous intensity),
/// i.e. of (m, kg, s, A, K, mol, cd) -- the same order as Python's `Dim.e`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Dim(pub [Rational64; 7]);

const fn r(n: i64) -> Rational64 {
    Rational64::new_raw(n, 1)
}

/// The dimension of a plain number.
pub const DIMLESS: Dim = Dim([r(0), r(0), r(0), r(0), r(0), r(0), r(0)]);
pub const LENGTH: Dim = Dim([r(1), r(0), r(0), r(0), r(0), r(0), r(0)]);
pub const MASS: Dim = Dim([r(0), r(1), r(0), r(0), r(0), r(0), r(0)]);
pub const TIME: Dim = Dim([r(0), r(0), r(1), r(0), r(0), r(0), r(0)]);
pub const CURRENT: Dim = Dim([r(0), r(0), r(0), r(1), r(0), r(0), r(0)]);
pub const TEMPERATURE: Dim = Dim([r(0), r(0), r(0), r(0), r(1), r(0), r(0)]);
pub const AMOUNT: Dim = Dim([r(0), r(0), r(0), r(0), r(0), r(1), r(0)]);
pub const LUMINOSITY: Dim = Dim([r(0), r(0), r(0), r(0), r(0), r(0), r(1)]);

impl Dim {
    /// The i-th base dimension (0 = length ... 6 = luminous intensity).
    pub fn base(i: usize) -> Dim {
        let mut d = DIMLESS;
        d.0[i] = Rational64::one();
        d
    }

    /// A dimension from integer exponents.
    pub fn from_ints(e: [i64; 7]) -> Dim {
        Dim(e.map(Rational64::from_integer))
    }

    /// Raise to a rational power (multiply every exponent).
    pub fn pow(self, p: Rational64) -> Dim {
        Dim(self.0.map(|a| a * p))
    }

    /// Raise to an integer power.
    pub fn powi(self, p: i64) -> Dim {
        self.pow(Rational64::from_integer(p))
    }

    pub fn is_dimensionless(&self) -> bool {
        self.0.iter().all(|x| x.is_zero())
    }

    /// Number of base quantities with a non-zero exponent.
    pub fn n_bases(&self) -> usize {
        self.0.iter().filter(|x| !x.is_zero()).count()
    }
}

impl Mul for Dim {
    type Output = Dim;
    fn mul(self, o: Dim) -> Dim {
        let mut e = self.0;
        for i in 0..7 {
            e[i] += o.0[i];
        }
        Dim(e)
    }
}

impl Div for Dim {
    type Output = Dim;
    fn div(self, o: Dim) -> Dim {
        let mut e = self.0;
        for i in 0..7 {
            e[i] -= o.0[i];
        }
        Dim(e)
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = format_dim(self);
        write!(f, "{}", if s.is_empty() { "1" } else { &s })
    }
}

/// Translate an integer's decimal text to superscript digits (`-12` -> `⁻¹²`).
pub fn superscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '-' => '⁻',
            '/' => 'ᐟ',
            c => c,
        })
        .collect()
}

/// Python `_fmt_exp`: the exponent suffix of a unit name ("" for 1, "²", "⁻¹", "^(1/2)"; ASCII "^2").
pub fn fmt_exp(p: Rational64, pretty: bool) -> String {
    if p == Rational64::one() {
        return String::new();
    }
    if !pretty {
        if *p.denom() == 1 {
            return format!("^{}", p.numer());
        }
        return format!("^({}/{})", p.numer(), p.denom());
    }
    if *p.denom() == 1 {
        return superscript(&p.numer().to_string());
    }
    format!("^({}/{})", p.numer(), p.denom())
}

/// Python `join_units`: "a b/c", "a/(b c)", "1/s".
pub fn join_units<S: AsRef<str>>(num: &[S], den: &[S]) -> String {
    if num.is_empty() && den.is_empty() {
        return String::new();
    }
    let mut s = if num.is_empty() {
        "1".to_string()
    } else {
        num.iter().map(|x| x.as_ref()).collect::<Vec<_>>().join(" ")
    };
    if !den.is_empty() {
        if den.len() == 1 {
            s.push('/');
            s.push_str(den[0].as_ref());
        } else {
            s.push_str("/(");
            s.push_str(&den.iter().map(|x| x.as_ref()).collect::<Vec<_>>().join(" "));
            s.push(')');
        }
    }
    s
}

/// A dimension as SI base units, kg first: "kg m/s²" ("" for a plain number).
pub fn format_dim(d: &Dim) -> String {
    format_dim_style(d, true)
}

/// `format_dim` with ASCII exponents when `pretty` is false ("kg m/s^2").
pub fn format_dim_style(d: &Dim, pretty: bool) -> String {
    const ORDER: [usize; 7] = [1, 0, 2, 3, 4, 5, 6];
    let mut num = Vec::new();
    let mut den = Vec::new();
    for i in ORDER {
        let p = d.0[i];
        if p > Rational64::zero() {
            num.push(format!("{}{}", BASE_SYMBOLS[i], fmt_exp(p, pretty)));
        } else if p < Rational64::zero() {
            den.push(format!("{}{}", BASE_SYMBOLS[i], fmt_exp(-p, pretty)));
        }
    }
    join_units(&num, &den)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algebra_and_format() {
        let force = MASS * LENGTH / TIME.powi(2);
        assert_eq!(format_dim(&force), "kg m/s²");
        assert_eq!(format_dim(&(DIMLESS / TIME)), "1/s");
        assert_eq!(format_dim(&LENGTH.pow(Rational64::new(1, 2))), "m^(1/2)");
        assert_eq!(format_dim(&(MASS / (LENGTH * TIME.powi(2)))), "kg/(m s²)");
        assert!(DIMLESS.is_dimensionless());
        assert_eq!(Dim::default(), DIMLESS);
    }
}
