//! Root finding: v1's algebraic `solve lhs = rhs for x from a to b` (D32: scan 200 sub-intervals
//! for the first sign change, refine with Illinois, detect poles and rounding noise; port of
//! `fm_root` / `interp.root`), and Brent's method as SciPy's `brentq` (used by v1's eigenvalue
//! shooting), both bracketing.

use super::{err, pymax, pymin, Fail};

/// |lhs − rhs| at most this times |lhs| + |rhs| near the root: rounding noise (#36)
pub const NOISE: f64 = 1e-12;

/// The result of [`root`]: the root, and where v1 warns (kind 1) that the two sides agree only to
/// rounding there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Root {
    pub x: f64,
    pub noise_warning: Option<f64>,
}

/// v1's `root` (`fm_root`): the FIRST root of f after a0 towards b0. `scale(x)` = |lhs| + |rhs|
/// enables the rounding-noise warning.
pub fn root<F: FnMut(f64) -> f64>(
    mut f: F,
    a0: f64,
    b0: f64,
    scan: usize,
    mut scale: Option<&mut dyn FnMut(f64) -> f64>,
) -> Result<Root, Fail> {
    let mut warn = None;
    let mut fa = f(a0);
    if fa == 0.0 {
        return Ok(Root { x: a0, noise_warning: None });
    }
    let h = (b0 - a0) / scan as f64;
    let mut fprev = fa;
    let mut found = false;
    let (mut a, mut c, mut fc) = (0.0f64, 0.0f64, 0.0f64);
    let (mut fjump, mut xjump, mut pole) = (f64::NAN, 0.0f64, f64::NAN);
    for i in 1..=scan {
        let xi = if i == scan { b0 } else { a0 + i as f64 * h };
        let fi = f(xi);
        if fi == 0.0 {
            if let Some(s) = scale.as_mut() {
                if fprev.abs() <= NOISE * s(xi - h) {
                    warn = Some(xi);
                }
            }
            return Ok(Root { x: xi, noise_warning: warn });
        }
        if fi.abs() == f64::INFINITY {
            // on a pole: not a crossing; skip past it
            fjump = fprev;
            xjump = xi;
            fprev = f64::NAN;
            continue;
        }
        if fjump * fi < 0.0 && pole != pole {
            pole = xjump;
        }
        fjump = f64::NAN;
        if fprev * fi < 0.0 {
            a = xi - h;
            fa = fprev;
            c = xi;
            fc = fi;
            found = true;
            break;
        }
        fprev = fi;
    }
    if !found {
        if pole == pole {
            return Err(Fail::new(err::POLE, pole, 0.0));
        }
        return Err(Fail::new(err::ROOT, a0, b0));
    }
    if let Some(s) = scale.as_mut() {
        if fa.abs() <= NOISE * s(a) && fc.abs() <= NOISE * s(c) {
            warn = Some(c);
        }
    }
    let mut side = 0;
    let m0 = pymax(fa.abs(), fc.abs());
    for _ in 0..300 {
        if (c - a).abs() <= 4e-16 * pymax(a.abs(), c.abs()) {
            let best = if fa.abs() < fc.abs() { a } else { c };
            if f(best).abs() > m0 {
                return Err(Fail::new(err::POLE, best, 0.0));
            }
            return Ok(Root { x: best, noise_warning: warn });
        }
        let mut x = c - fc * (c - a) / (fc - fa);
        if !(pymin(a, c) < x && x < pymax(a, c)) {
            x = 0.5 * (a + c);
        }
        let fx = f(x);
        if fx == 0.0 {
            return Ok(Root { x, noise_warning: warn });
        }
        if fx != fx {
            // undefined inside the bracket: not a verified root
            return Err(Fail::new(err::POLE, x, 0.0));
        }
        if fx * fc < 0.0 {
            a = c;
            fa = fc;
            side = 0;
        } else {
            if side == 1 {
                fa *= 0.5;
            }
            side = 1;
        }
        c = x;
        fc = fx;
    }
    Ok(Root { x: c, noise_warning: warn })
}

/// SciPy's `brentq` (scipy/optimize/Zeros/brentq.c): a root of f in [xa, xb], where f(xa) and
/// f(xb) have opposite signs. Defaults in SciPy: xtol = 2e-12, rtol = 4ε, maxiter = 100.
/// Err(()) if the signs don't differ or it does not converge.
pub fn brentq<F: FnMut(f64) -> f64>(mut f: F, xa: f64, xb: f64, xtol: f64, rtol: f64, maxiter: usize) -> Result<f64, ()> {
    let (mut xpre, mut xcur) = (xa, xb);
    let (mut xblk, mut fblk, mut spre, mut scur) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut fpre = f(xpre);
    let mut fcur = f(xcur);
    if fpre == 0.0 {
        return Ok(xpre);
    }
    if fcur == 0.0 {
        return Ok(xcur);
    }
    if fpre.is_sign_negative() == fcur.is_sign_negative() {
        return Err(());
    }
    for _ in 0..maxiter {
        if fpre != 0.0 && fcur != 0.0 && fpre.is_sign_negative() != fcur.is_sign_negative() {
            xblk = xpre;
            fblk = fpre;
            spre = xcur - xpre;
            scur = spre;
        }
        if fblk.abs() < fcur.abs() {
            xpre = xcur;
            xcur = xblk;
            xblk = xpre;
            fpre = fcur;
            fcur = fblk;
            fblk = fpre;
        }
        let delta = (xtol + rtol * xcur.abs()) / 2.0;
        let sbis = (xblk - xcur) / 2.0;
        if fcur == 0.0 || sbis.abs() < delta {
            return Ok(xcur);
        }
        if spre.abs() > delta && fcur.abs() < fpre.abs() {
            let stry = if xpre == xblk {
                -fcur * (xcur - xpre) / (fcur - fpre)
            } else {
                let dpre = (fpre - fcur) / (xpre - xcur);
                let dblk = (fblk - fcur) / (xblk - xcur);
                -fcur * (fblk * dblk - fpre * dpre) / (dblk * dpre * (fblk - fpre))
            };
            if 2.0 * stry.abs() < spre.abs().min(3.0 * sbis.abs() - delta) {
                spre = scur;
                scur = stry;
            } else {
                spre = sbis;
                scur = sbis;
            }
        } else {
            spre = sbis;
            scur = sbis;
        }
        xpre = xcur;
        fpre = fcur;
        if scur.abs() > delta {
            xcur += scur;
        } else {
            xcur += if sbis > 0.0 { delta } else { -delta };
        }
        fcur = f(xcur);
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_root_and_brent() {
        let r = root(|x: f64| x.cos() - x, 0.0, 2.0, 200, None).unwrap();
        assert!((r.x - 0.7390851332151607).abs() < 1e-15);
        let b = brentq(|x: f64| x.cos() - x, 0.0, 2.0, 2e-12, 4.0 * f64::EPSILON, 100).unwrap();
        assert!((b - 0.7390851332151607).abs() < 1e-11);
        assert_eq!(root(|x: f64| x * x + 1.0, -1.0, 1.0, 200, None).unwrap_err().kind, err::ROOT);
        assert_eq!(root(|x: f64| x.tan(), 1.0, 2.0, 200, None).unwrap_err().kind, err::POLE);
    }
}
