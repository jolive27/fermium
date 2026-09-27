//! Checked arithmetic on unit exponents (red team 13 #1, #2).
//!
//! Exponents are `Rational64`. Plain `Ratio<i64>` arithmetic wraps silently in a release build (so
//! `(x^4294967296)^4294967296` became a plain number and got past the unit check) or panics ("denominator ==
//! 0"). Every exponent computed from user code goes through these functions instead: the exact result is worked
//! out in i128, reduced, and kept only if it fits in 64 bits.
//!
//! The dimension algebra (`Dim`, `DExpr`, the unifier) is infallible by signature and used in hundreds of places,
//! so an exponent that doesn't fit is *recorded* in a per-thread flag rather than returned as an error: the
//! operation returns a placeholder, and the checker turns the flag into a one-line error at the innermost
//! expression that produced it (and, as a backstop, at the statement, and at the end of checking). Nothing
//! can print or run while the flag is set.
use num_rational::Rational64;
use std::cell::RefCell;

thread_local! {
    static OVERFLOW: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// True if an exponent overflowed since the flag was last taken or cleared.
pub fn overflow_pending() -> bool {
    OVERFLOW.with(|o| o.borrow().is_some())
}

/// Take the recorded overflow: the too-large unit as text ("m^(1/18446744073709551616)"), or None.
pub fn overflow_take() -> Option<String> {
    OVERFLOW.with(|o| o.borrow_mut().take())
}

/// Forget any recorded overflow (at the start of checking a program, a REPL line or a cell).
pub fn overflow_clear() {
    OVERFLOW.with(|o| *o.borrow_mut() = None);
}

/// Record an overflow; the first one recorded is kept (it is the innermost).
pub fn overflow_record(what: String) {
    OVERFLOW.with(|o| {
        let mut o = o.borrow_mut();
        if o.is_none() {
            *o = Some(what);
        }
    });
}

/// The one-line message and hint for a recorded overflow.
pub fn overflow_message(what: &str) -> (String, String) {
    (format!("this unit's power is too large to track exactly ({what})"),
     "Fermium keeps a unit's power as an exact fraction of 64-bit whole numbers; raise a plain number to the \
      power instead, and attach the unit afterwards"
         .to_string())
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i128
}

/// n/d reduced, with a positive denominator (None if d == 0 or a value is i128::MIN).
fn reduce(n: i128, d: i128) -> Option<(i128, i128)> {
    if d == 0 || n == i128::MIN || d == i128::MIN {
        return None;
    }
    let g = gcd(n, d).max(1);
    let (mut n, mut d) = (n / g, d / g);
    if d < 0 {
        n = -n;
        d = -d;
    }
    Some((n, d))
}

/// n/d as a Rational64 if it fits. i64::MIN is refused too, so that negating any exponent is safe.
fn fit(n: i128, d: i128) -> Option<Rational64> {
    let (n, d) = reduce(n, d)?;
    let lim = i64::MAX as i128;
    if n.abs() > lim || d > lim {
        return None;
    }
    Some(Rational64::new_raw(n as i64, d as i64))
}

/// The exact result as text, for the message: "1/18446744073709551616", "18446744073709551616", or "…" when even
/// i128 can't hold it.
fn text(nd: Option<(i128, i128)>) -> String {
    match nd {
        Some((n, 1)) => n.to_string(),
        Some((n, d)) => format!("{n}/{d}"),
        None => "…".into(),
    }
}

fn parts(a: Rational64) -> (i128, i128) {
    (*a.numer() as i128, *a.denom() as i128)
}

fn mul_exact(a: Rational64, b: Rational64) -> Option<(i128, i128)> {
    let ((an, ad), (bn, bd)) = (parts(a), parts(b));
    reduce(an.checked_mul(bn)?, ad.checked_mul(bd)?)
}

fn div_exact(a: Rational64, b: Rational64) -> Option<(i128, i128)> {
    let ((an, ad), (bn, bd)) = (parts(a), parts(b));
    reduce(an.checked_mul(bd)?, ad.checked_mul(bn)?)
}

fn add_exact(a: Rational64, b: Rational64) -> Option<(i128, i128)> {
    let ((an, ad), (bn, bd)) = (parts(a), parts(b));
    reduce(an.checked_mul(bd)?.checked_add(bn.checked_mul(ad)?)?, ad.checked_mul(bd)?)
}

/// a · b, None if it doesn't fit.
pub fn checked_mul(a: Rational64, b: Rational64) -> Option<Rational64> {
    mul_exact(a, b).and_then(|(n, d)| fit(n, d))
}

/// a / b, None if b is 0 or it doesn't fit.
pub fn checked_div(a: Rational64, b: Rational64) -> Option<Rational64> {
    div_exact(a, b).and_then(|(n, d)| fit(n, d))
}

/// a + b, None if it doesn't fit.
pub fn checked_add(a: Rational64, b: Rational64) -> Option<Rational64> {
    add_exact(a, b).and_then(|(n, d)| fit(n, d))
}

/// a - b, None if it doesn't fit.
pub fn checked_sub(a: Rational64, b: Rational64) -> Option<Rational64> {
    checked_add(a, -b)
}

/// How a failed result is named in the message: `name^(n/d)` for a base unit, `a power of n/d` otherwise.
fn describe(name: Option<&str>, t: String) -> String {
    match name {
        Some(n) if t.contains('/') || t.starts_with('-') => format!("{n}^({t})"),
        Some(n) => format!("{n}^{t}"),
        None => format!("a power of {t}"),
    }
}

/// a · b; on overflow records it (naming the unit `name`) and returns a.
pub fn mul_or_record(a: Rational64, b: Rational64, name: Option<&str>) -> Rational64 {
    match mul_exact(a, b) {
        Some((n, d)) => fit(n, d).unwrap_or_else(|| {
            overflow_record(describe(name, text(Some((n, d)))));
            a
        }),
        None => {
            overflow_record(describe(name, text(None)));
            a
        }
    }
}

/// a / b; on overflow (or b == 0) records it and returns a.
pub fn div_or_record(a: Rational64, b: Rational64, name: Option<&str>) -> Rational64 {
    match div_exact(a, b) {
        Some((n, d)) => fit(n, d).unwrap_or_else(|| {
            overflow_record(describe(name, text(Some((n, d)))));
            a
        }),
        None => {
            overflow_record(describe(name, text(None)));
            a
        }
    }
}

/// a + b; on overflow records it and returns a.
pub fn add_or_record(a: Rational64, b: Rational64, name: Option<&str>) -> Rational64 {
    match add_exact(a, b) {
        Some((n, d)) => fit(n, d).unwrap_or_else(|| {
            overflow_record(describe(name, text(Some((n, d)))));
            a
        }),
        None => {
            overflow_record(describe(name, text(None)));
            a
        }
    }
}

/// a - b; on overflow records it and returns a.
pub fn sub_or_record(a: Rational64, b: Rational64, name: Option<&str>) -> Rational64 {
    add_or_record(a, -b, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Rational64 {
        Rational64::new(n, d)
    }

    #[test]
    fn exact_when_it_fits() {
        assert_eq!(checked_mul(q(1, 2), q(2, 3)), Some(q(1, 3)));
        assert_eq!(checked_add(q(1, 2), q(1, 3)), Some(q(5, 6)));
        assert_eq!(checked_sub(q(1, 2), q(1, 2)), Some(q(0, 1)));
        assert_eq!(checked_div(q(1, 2), q(0, 1)), None);
        // large intermediates that reduce back into range
        let big = q(i64::MAX, 1);
        assert_eq!(checked_mul(big, q(1, i64::MAX)), Some(q(1, 1)));
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let two32 = q(1 << 32, 1);
        assert_eq!(checked_mul(two32, two32), None); // 2^64
        assert_eq!(checked_mul(q(1, 1 << 32), q(1, 1 << 32)), None); // 1/2^64
        assert_eq!(checked_mul(q(i64::MAX, 1), q(-1, 1)).map(|r| *r.numer()), Some(-i64::MAX));
        assert_eq!(checked_add(q(i64::MAX, 1), q(1, 1)), None);
        assert_eq!(checked_sub(q(-i64::MAX, 1), q(1, 1)), None); // i64::MIN is refused
    }

    #[test]
    fn records_the_first_overflow_with_its_exact_value() {
        overflow_clear();
        let r = mul_or_record(q(1, 1 << 32), q(1, 1 << 32), Some("m"));
        assert_eq!(r, q(1, 1 << 32));
        let _ = mul_or_record(q(1 << 40, 1), q(1 << 40, 1), Some("kg"));
        assert!(overflow_pending());
        assert_eq!(overflow_take().as_deref(), Some("m^(1/18446744073709551616)"));
        assert!(!overflow_pending());
        let _ = mul_or_record(q(1 << 32, 1), q(1 << 32, 1), None);
        assert_eq!(overflow_take().as_deref(), Some("a power of 18446744073709551616"));
        let _ = div_or_record(q(1, 1), q(0, 1), Some("s"));
        assert_eq!(overflow_take().as_deref(), Some("s^…"));
    }
}
