//! Adaptive quadrature: v1's globally adaptive Gauss–Kronrod 7-15 (`fm_quadcore` / `fm_gk15` in
//! `fermium/codegen_llvm.py`, mirrored by `_quadcore` in `fermium/interp.py`; DECISIONS D44, D45,
//! D110), ported operation for operation, plus the B2 improvements.
//!
//! v1's method, in short:
//! * a finite range [p, q] is mapped to u ∈ [0, 1] by the smoothstep x = p + L·u²(3 − 2u), which
//!   weakens endpoint singularities; a half-infinite range [p, ∞) is split at p + L, where L is the
//!   scale at which |f(p + s)|·s peaks (a scan over s = 10⁻⁴⁰ … 10⁴⁰ in quarter decades), and the
//!   tail uses x = p + L·u/(1 − u);
//! * 8 initial panels, then the panel with the largest |K15 − G7| error is bisected, up to 2000
//!   panels; convergence when the summed error is below max(atol, rtol·|I|), or 10⁻¹⁴·|I|, or at the
//!   rounding level 50ε·∫|f| (D44);
//! * NaN/∞ nodes count as 0 with an infinite error; a run of such panels that is one point wide
//!   (8 ulps) is dropped (D45); an interior singularity that stops the refinement splits the range
//!   there (`fm_quadfin`).
//!
//! B2 improvements (on by default; [`quad_v1`] gives v1's exact behaviour). Each one only acts where
//! v1 would have returned a wrong answer or an error, so every result v1 got right is unchanged,
//! bit for bit:
//! 1. **Panel-end sentinels** (narrow peaks and half peaks, OPEN_ITEMS RT1-1, BL-1, BL-2): when the
//!    adaptive loop has converged, the integrand is also sampled at every panel's two ends (points
//!    Gauss–Kronrod never uses). A panel whose end value exceeds 10× the largest value at its 15
//!    nodes, and would matter at the requested accuracy, has missed a feature at that end: its error
//!    is raised to |g(end)|·width and the loop continues. This catches peaks sitting exactly on a
//!    subdivision point, e.g. `∫ exp(-((x-1)/1e-6)²) dx from 0 to ∞` (v1: half the value, no
//!    warning) and `∫ exp(-x²) dx from -1e6 to 1e6` (v1: 0).
//! 2. **Located splits** (strong singularities away from 0, BL-3): when v1's split at the middle of
//!    the worst panel still fails, the point where |f| is largest near it is located (golden-section
//!    search to rounding level) and the range is split exactly there, so the singularity sits at an
//!    endpoint, where the smoothstep map tames it (|x − c|^(−0.8) becomes u^(−0.6)).

/// Gauss–Kronrod 7-15 nodes/weights (from QUADPACK qk15), as `fermium/numerics.py`.
const XGK: [f64; 8] = [
    0.991455371120812639206854697526329,
    0.949107912342758524526189684047851,
    0.864864423359769072789712788640926,
    0.741531185599394439863864773280788,
    0.586087235467691130294144845693013,
    0.405845151377397166906606412076961,
    0.207784955007898467600689403773245,
    0.000000000000000000000000000000000,
];
const WGK: [f64; 8] = [
    0.022935322010529224963732008058970,
    0.063092092629978553290700663189204,
    0.104790010322250183839876322541518,
    0.140653259715525918745189590510238,
    0.169004726639267902826583426598550,
    0.190350578064785409913256402421014,
    0.204432940075298892414161999234649,
    0.209482141084727828012999174891714,
];
const WG: [f64; 4] = [
    0.129484966168869693270611432679082,
    0.279705391489276667901467771423780,
    0.381830050505118944950369775488975,
    0.417959183673469387755102040816327,
];

use super::{err, Fail};

const EPS: f64 = 2.220446049250313e-16;
/// a NaN panel this narrow (relative) is one point (D45)
const QUAD_ULPS: f64 = 8.0 * EPS;
/// an error below this × ∫|f| is rounding (D44)
const QUAD_ROUND: f64 = 50.0 * EPS;
/// initial panels and the most panels
const P0: usize = 8;
const MAX_PANELS: usize = 2000;
/// QUAD_SOFT: the panel limit of the quiet first try of a vector component (D44)
const QUAD_SOFT: usize = 250;
/// sentinel test: an end value this many times the panel's largest node value is a missed feature
const SENTINEL_RATIO: f64 = 10.0;
/// sentinels stop firing on panels narrower than this (in u): an isolated point value is not a peak
const SENTINEL_MIN_WIDTH: f64 = 1e-12;

/// The result of [`quad`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadResult {
    /// the integral
    pub value: f64,
    /// the summed error estimate of the accepted panels
    pub error: f64,
    /// the summed ∫|g| estimates of the accepted panels (v1's `fm.qabs`, D110)
    pub abs_sum: f64,
    /// the result is exactly 0 because the integrand was 0 at every node, and the range is not
    /// empty (v1 warns, kind 3, D110; for the quiet first try, atol < 0, v1 counts it in `fm.qzero`)
    pub all_zero: bool,
}

/// ∫ f(x) dx from a to b with v1's defaults of the caller (rtol 1e-10, atol 0 in v1's `quad`).
///
/// `atol < 0` is v1's quiet first try for a vector component (D44): a soft panel limit, and NaN
/// instead of an error. `name` is the text id of the variable, for the error message (−1: none).
pub fn quad<F: FnMut(f64) -> f64>(f: F, a: f64, b: f64, rtol: f64, atol: f64, name: f64) -> Result<QuadResult, Fail> {
    quad_opts(f, a, b, rtol, atol, name, true)
}

/// v1's quadrature exactly, without the B2 improvements (for comparison and tests).
pub fn quad_v1<F: FnMut(f64) -> f64>(f: F, a: f64, b: f64, rtol: f64, atol: f64, name: f64) -> Result<QuadResult, Fail> {
    quad_opts(f, a, b, rtol, atol, name, false)
}

fn quad_opts<F: FnMut(f64) -> f64>(
    mut f: F,
    a: f64,
    b: f64,
    rtol: f64,
    atol: f64,
    name: f64,
    b2: bool,
) -> Result<QuadResult, Fail> {
    let mut acc = Acc { abs_sum: 0.0, error: 0.0 };
    let value = quad_in(&mut f, a, b, rtol, atol, name, b2, &mut acc)?;
    let all_zero = value == 0.0 && acc.abs_sum == 0.0 && a != b;
    Ok(QuadResult { value, error: acc.error, abs_sum: acc.abs_sum, all_zero })
}

struct Acc {
    abs_sum: f64,
    error: f64,
}

#[allow(clippy::too_many_arguments)]
fn quad_in<F: FnMut(f64) -> f64>(
    f: &mut F,
    a: f64,
    b: f64,
    rtol: f64,
    atol: f64,
    name: f64,
    b2: bool,
    acc: &mut Acc,
) -> Result<f64, Fail> {
    if a > b {
        return Ok(-quad_in(f, b, a, rtol, atol, name, b2, acc)?);
    }
    if a == b {
        return Ok(0.0);
    }
    let a_inf = a.abs() == f64::INFINITY;
    let b_inf = b.abs() == f64::INFINITY;
    if !(a_inf || b_inf) {
        return quadfin(f, a, b, rtol, atol, name, b2, acc);
    }
    if !a_inf {
        let l = qscan(f, a, 1.0);
        let r1 = quadfin(f, a, a + l, rtol, atol, name, b2, acc)?;
        let r2 = core_plain(f, 1, a + l, l, rtol, atol, name, b2, acc)?;
        return Ok(r1 + r2);
    }
    if !b_inf {
        let l = qscan(f, b, -1.0);
        let r1 = core_plain(f, 2, b - l, l, rtol, atol, name, b2, acc)?;
        let r2 = quadfin(f, b - l, b, rtol, atol, name, b2, acc)?;
        return Ok(r1 + r2);
    }
    let lr = qscan(f, 0.0, 1.0);
    let ll = qscan(f, 0.0, -1.0);
    let vr = f(lr).abs() * lr;
    let vl = f(-ll).abs() * ll;
    let c = if !(vr < vl) { lr } else { -ll };
    let r1 = core_plain(f, 2, c, c.abs(), rtol, atol, name, b2, acc)?;
    let r2 = core_plain(f, 1, c, c.abs(), rtol, atol, name, b2, acc)?;
    Ok(r1 + r2)
}

/// v1's `_qscan`: the scale s (quarter decades from 1e-40) where |f(base + sign·s)|·s is largest.
fn qscan<F: FnMut(f64) -> f64>(f: &mut F, base: f64, sign: f64) -> f64 {
    let step = 10f64.powf(0.25);
    let (mut best, mut bs, mut sc) = (0.0f64, 1.0f64, 1e-40f64);
    for _ in 0..321 {
        let v = f(base + sign * sc).abs() * sc;
        if v > best && v < f64::INFINITY {
            best = v;
            bs = sc;
        }
        sc *= step;
    }
    bs
}

#[allow(clippy::too_many_arguments)]
fn core_plain<F: FnMut(f64) -> f64>(
    f: &mut F,
    mode: u8,
    p: f64,
    q: f64,
    rtol: f64,
    atol: f64,
    name: f64,
    b2: bool,
    acc: &mut Acc,
) -> Result<f64, Fail> {
    match quadcore(f, mode, p, q, rtol, atol, false, name, b2, acc)? {
        Core::Value(v) => Ok(v),
        Core::Split { .. } => unreachable!("no split without split=true"),
    }
}

/// v1's `fm_quadfin`: split a finite range at an interior singularity.
#[allow(clippy::too_many_arguments)]
fn quadfin<F: FnMut(f64) -> f64>(
    f: &mut F,
    a: f64,
    b: f64,
    rtol: f64,
    atol: f64,
    name: f64,
    b2: bool,
    acc: &mut Acc,
) -> Result<f64, Fail> {
    match quadcore(f, 0, a, b, rtol, atol, true, name, b2, acc)? {
        Core::Value(v) => Ok(v),
        Core::Split { c, xa, xb } => {
            let saved = (acc.abs_sum, acc.error);
            let first = core_plain(f, 0, c, b, rtol, atol, name, b2, acc)
                .and_then(|r1| Ok(r1 - core_plain(f, 0, c, a, rtol, atol, name, b2, acc)?));
            match first {
                Ok(v) => Ok(v),
                Err(e) if b2 => {
                    // B2: split exactly where |f| is largest near the stuck panel
                    acc.abs_sum = saved.0;
                    acc.error = saved.1;
                    match locate_peak(f, a, b, xa, xb) {
                        Some(c2) => {
                            let r1 = side_integral(f, c2, b, rtol, name, acc).ok_or(e)?;
                            let r2 = side_integral(f, c2, a, rtol, name, acc).ok_or(e)?;
                            Ok(r1 - r2)
                        }
                        None => Err(e),
                    }
                }
                Err(e) => Err(e),
            }
        }
    }
}

/// B2: ∫ f from c to e (either order) where f has an integrable singularity at c. First directly
/// (the smoothstep map puts c at an endpoint); if that fails, because near c the integrand is lost
/// to rounding (x − c for x ≈ c ≠ 0 is quantised at ulp(c)), then as the limit of F(d) = ∫ from c + d
/// to e for d = d₀, d₀/2, d₀/4 by Aitken's Δ² (exact for a power law |x − c|^α, −1 < α < 0; this is
/// the extrapolation step of QUADPACK's qags). None if neither works.
fn side_integral<F: FnMut(f64) -> f64>(f: &mut F, c: f64, e: f64, rtol: f64, name: f64, acc: &mut Acc) -> Option<f64> {
    if e == c {
        return Some(0.0);
    }
    let s = if e > c { 1.0 } else { -1.0 };
    let ordered = |f: &mut F, lo: f64, hi: f64, acc: &mut Acc| core_plain(f, 0, lo, hi, rtol, 0.0, name, true, acc).ok();
    let (lo, hi) = if s > 0.0 { (c, e) } else { (e, c) };
    if let Some(v) = ordered(f, lo, hi, acc) {
        return Some(s * v);
    }
    let span = (e - c).abs();
    let ulp = if c == 0.0 { f64::MIN_POSITIVE } else { c.abs() * EPS };
    let want = (1e-9 * c.abs().max(span)).max(ulp * 1048576.0);
    let d0 = 2f64.powi(want.log2().ceil() as i32);
    if !(d0 < 0.25 * span) {
        return None;
    }
    let mut fd = [0.0f64; 3];
    for (k, slot) in fd.iter_mut().enumerate() {
        let d = d0 / (1u64 << k) as f64;
        let (lo, hi) = if s > 0.0 { (c + d, e) } else { (e, c - d) };
        *slot = ordered(f, lo, hi, acc)?;
    }
    let (d1, d2) = (fd[1] - fd[0], fd[2] - fd[1]);
    let r = d2 / d1;
    if !(r > 0.0 && r < 0.999) {
        return None;
    }
    let lim = fd[2] + d2 * r / (1.0 - r);
    lim.is_finite().then_some(s * lim)
}

/// Where |f| is largest near [xa, xb] (inside (a, b)): golden-section search to rounding level.
/// None unless it is a real spike (|f| there far above its values a panel width away).
fn locate_peak<F: FnMut(f64) -> f64>(f: &mut F, a: f64, b: f64, xa: f64, xb: f64) -> Option<f64> {
    let w = (xb - xa).abs().max(1e-12 * (b - a));
    let lo0 = (xa.min(xb) - w).max(a);
    let hi0 = (xa.max(xb) + w).min(b);
    let mut val = |x: f64| -> f64 {
        let v = f(x).abs();
        if v != v { f64::INFINITY } else { v }
    };
    // coarse scan first (the spike may be anywhere in the widened panel)
    let n = 64;
    let (mut bx, mut bv) = (lo0, -1.0);
    for i in 0..=n {
        let x = lo0 + (hi0 - lo0) * (i as f64) / (n as f64);
        let v = val(x);
        if v == f64::INFINITY && x > a && x < b {
            return Some(x);
        }
        if v > bv {
            bv = v;
            bx = x;
        }
    }
    let h = (hi0 - lo0) / n as f64;
    let (mut lo, mut hi) = ((bx - h).max(lo0), (bx + h).min(hi0));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let mut x1 = hi - g * (hi - lo);
    let mut x2 = lo + g * (hi - lo);
    let mut v1 = val(x1);
    let mut v2 = val(x2);
    for _ in 0..200 {
        if v1 == f64::INFINITY {
            return (x1 > a && x1 < b).then_some(x1);
        }
        if v2 == f64::INFINITY {
            return (x2 > a && x2 < b).then_some(x2);
        }
        if hi - lo <= 4.0 * EPS * lo.abs().max(hi.abs()) {
            break;
        }
        if v1 >= v2 {
            hi = x2;
            x2 = x1;
            v2 = v1;
            x1 = hi - g * (hi - lo);
            v1 = val(x1);
        } else {
            lo = x1;
            x1 = x2;
            v1 = v2;
            x2 = lo + g * (hi - lo);
            v2 = val(x2);
        }
    }
    let c = if v1 >= v2 { x1 } else { x2 };
    let vc = v1.max(v2);
    let edge = val(lo0).min(val(hi0));
    (c > a && c < b && vc > 1e3 * edge).then_some(c)
}

enum Core {
    Value(f64),
    /// v1's `_Split(c)`: an interior singularity stopped the refinement; split at c (the x of the
    /// worst panel's middle). xa, xb: that panel's ends in x.
    Split { c: f64, xa: f64, xb: f64 },
}

#[derive(Clone, Copy)]
struct Panel {
    u0: f64,
    u1: f64,
    r: f64,
    e: f64,
    ra: f64,
    /// u of a NaN node (+u) or ±∞ node (−u), NaN when all nodes were finite
    badu: f64,
    /// the largest finite |g| at the nodes (−1 once dropped as a point)
    gmax: f64,
    /// B2: the panel-end sentinels have been checked
    checked: bool,
}

/// v1's `_qx`: x for the quadrature variable u.
#[inline]
fn qx(mode: u8, p: f64, q: f64, u: f64) -> f64 {
    if mode == 0 {
        let l = q - p;
        let v = 1.0 - u;
        if u <= 0.5 { p + l * (u * u * (3.0 - 2.0 * u)) } else { q - l * (v * v * (1.0 + 2.0 * u)) }
    } else {
        let sx = q * (u / (1.0 - u));
        if mode == 1 { p + sx } else { p - sx }
    }
}

struct Integrand<'a, F> {
    f: &'a mut F,
    mode: u8,
    p: f64,
    q: f64,
}

impl<F: FnMut(f64) -> f64> Integrand<'_, F> {
    /// the transformed integrand g(u)
    #[inline]
    fn g(&mut self, u: f64) -> f64 {
        let (mode, p, q) = (self.mode, self.p, self.q);
        if mode == 0 {
            let l = q - p;
            let v = 1.0 - u;
            let x = qx(0, p, q, u);
            if x == p || x == q {
                return 0.0;
            }
            return (self.f)(x) * (6.0 * (u * v) * l);
        }
        let om = 1.0 - u;
        (self.f)(qx(mode, p, q, u)) * q / (om * om)
    }

    /// (result, error, ∫|g|, u of a NaN/∞ node or NaN, largest finite |g|)
    fn gk(&mut self, lo: f64, hi: f64) -> (f64, f64, f64, f64, f64) {
        let c = 0.5 * (lo + hi);
        let h = 0.5 * (hi - lo);
        let mut bad = f64::NAN;
        let mut gmax = 0.0f64;
        let mut node = |this: &mut Self, u: f64| -> f64 {
            let g = this.g(u);
            if g.abs() < f64::INFINITY {
                gmax = gmax.max(g.abs());
                return g;
            }
            if g != g {
                bad = u; // a NaN node: +u, wins over a ±∞ node: -u
            } else if !(bad > 0.0) {
                bad = -u;
            }
            gmax = gmax.max(0.0);
            0.0
        };
        let fc = node(self, c);
        let mut resk = fc * WGK[7];
        let mut resg = fc * WG[3];
        let mut rabs = fc.abs() * WGK[7];
        for j in 0..7 {
            let dx = h * XGK[j];
            let f1 = node(self, c - dx);
            let f2 = node(self, c + dx);
            let s = f1 + f2;
            resk += s * WGK[j];
            rabs += (f1.abs() + f2.abs()) * WGK[j];
            if j % 2 == 1 {
                resg += s * WG[j / 2];
            }
        }
        (resk * h, ((resk - resg) * h).abs(), (rabs * h).abs(), bad, gmax)
    }

    fn panel(&mut self, u0: f64, u1: f64) -> Panel {
        let (r, e, ra, badu, gmax) = self.gk(u0, u1);
        if badu == badu {
            // a NaN/∞ node: 0 with an infinite error (D45)
            return Panel { u0, u1, r: 0.0, e: f64::INFINITY, ra: 0.0, badu, gmax, checked: false };
        }
        Panel { u0, u1, r, e, ra, badu, gmax, checked: false }
    }
}

/// v1's `_quadcore` (`fm_quadcore` / `fm_gk15`, D44, D45), plus the B2 sentinels.
#[allow(clippy::too_many_arguments)]
fn quadcore<F: FnMut(f64) -> f64>(
    f: &mut F,
    mode: u8,
    p: f64,
    q: f64,
    rtol: f64,
    atol: f64,
    split: bool,
    name: f64,
    b2: bool,
    acc: &mut Acc,
) -> Result<Core, Fail> {
    let mut ig = Integrand { f, mode, p, q };
    let tiny_x = |u0: f64, u1: f64| -> bool {
        let xa = qx(mode, p, q, u0);
        let xb = qx(mode, p, q, u1);
        let ext = (xb - xa).abs();
        let scale = if mode == 0 { (q - p).abs() } else { q.abs() };
        ext <= QUAD_ULPS * scale.max(xa.abs().max(xb.abs())) && ext < f64::INFINITY
    };
    let limit = if atol < 0.0 { QUAD_SOFT } else { MAX_PANELS - 1 };
    let width = 1.0 / P0 as f64;
    let mut panels: Vec<Panel> = Vec::with_capacity(64);
    for i in 0..P0 {
        let x0 = i as f64 * width;
        let x1 = if i == P0 - 1 { 1.0 } else { x0 + width };
        panels.push(ig.panel(x0, x1));
    }
    // B2 sentinel values g(u) at panel ends, keyed by the bits of u
    let mut ends: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();
    loop {
        let mut total = 0.0;
        let mut toterr = 0.0;
        let mut totabs = 0.0;
        let mut w = 0usize;
        for (i, pn) in panels.iter().enumerate() {
            let e = pn.e;
            total += pn.r;
            toterr += e;
            totabs += pn.ra;
            if e > panels[w].e || e != e {
                w = i;
            }
        }
        let finite = total.abs() < f64::INFINITY;
        let goal = if atol == atol { atol.max(rtol * total.abs()) } else { rtol * total.abs() };
        if finite && (toterr <= goal || toterr <= 1e-14 * total.abs() || toterr <= QUAD_ROUND * totabs) {
            if b2 && sentinels(&mut ig, &mut panels, &mut ends, goal) {
                continue;
            }
            acc.abs_sum += totabs;
            acc.error += toterr;
            return Ok(Core::Value(total));
        }
        let (wl, wh, wbad) = (panels[w].u0, panels[w].u1, panels[w].badu);
        if wbad == wbad && panels[w].gmax >= 0.0 && tiny_x(wl, wh) && exclude_run(&mut panels, wl, wh, &tiny_x) {
            continue;
        }
        let mid = 0.5 * (wl + wh);
        let stuck = (wh - wl) <= 1e-13 * wl.abs().max(wh.abs());
        if stuck && finite && toterr <= 1e-7 * total.abs() {
            acc.abs_sum += totabs;
            acc.error += toterr;
            return Ok(Core::Value(total));
        }
        let n = panels.len();
        if split && (n >= limit || stuck) {
            let um = 1.0 - mid;
            let mut c = if mid <= 0.5 {
                p + (q - p) * (mid * mid * (3.0 - 2.0 * mid))
            } else {
                q - (q - p) * (um * um * (1.0 + 2.0 * mid))
            };
            if c.abs() <= 1e-9 * (q - p) {
                c = 0.0;
            }
            if p < c && c < q {
                return Ok(Core::Split { c, xa: qx(mode, p, q, wl), xb: qx(mode, p, q, wh) });
            }
        }
        if atol < 0.0 && (n >= limit || stuck || toterr != toterr || total.abs() == f64::INFINITY) {
            return Ok(Core::Value(f64::NAN)); // the quiet first try for a component of a vector integral (D44)
        }
        if (n >= limit || stuck) && wbad == wbad {
            let kind = if wbad > 0.0 { err::QUAD_NAN } else { err::QUAD_INF };
            return Err(Fail::new(kind, qx(mode, p, q, wbad.abs()), name));
        }
        if n >= limit || stuck || toterr != toterr || total.abs() == f64::INFINITY {
            return Err(Fail::new(err::QUAD, total, toterr));
        }
        let p1 = ig.panel(wl, mid);
        let p2 = ig.panel(mid, wh);
        panels[w] = p1;
        panels.push(p2);
    }
}

/// v1's run search in `fm_quadcore`: true if the NaN/∞ panels around [wl, wh] are one point
/// (then they are dropped: 0 with an error of their width × the neighbours' largest |g|).
fn exclude_run(panels: &mut [Panel], wl: f64, wh: f64, tiny_x: &dyn Fn(f64, f64) -> bool) -> bool {
    use std::collections::HashMap;
    let mut by_start: HashMap<u64, usize> = HashMap::new();
    let mut by_end: HashMap<u64, usize> = HashMap::new();
    for (i, pn) in panels.iter().enumerate() {
        by_start.insert(pn.u0.to_bits(), i);
        by_end.insert(pn.u1.to_bits(), i);
    }
    let mut ends = [0.0f64; 2];
    let mut gs = [0.0f64; 2];
    for (side, start) in [(0usize, wl), (1usize, wh)] {
        let mut cur = start;
        let mut j: Option<usize>;
        loop {
            // left: the panel that ends at cur; right: the panel that starts at cur
            j = if side == 0 { by_end.get(&cur.to_bits()).copied() } else { by_start.get(&cur.to_bits()).copied() };
            match j {
                None => break,
                Some(jj) => {
                    let pn = &panels[jj];
                    if pn.badu == pn.badu {
                        cur = if side == 0 { pn.u0 } else { pn.u1 };
                        continue;
                    }
                    break;
                }
            }
        }
        ends[side] = cur;
        gs[side] = j.map(|jj| panels[jj].gmax).unwrap_or(-1.0);
    }
    let g = gs[0].max(gs[1]);
    if !(g >= 0.0 && tiny_x(ends[0], ends[1])) {
        return false;
    }
    for pn in panels.iter_mut() {
        if pn.u0 >= ends[0] && pn.u1 <= ends[1] {
            pn.r = 0.0;
            pn.e = (pn.u1 - pn.u0) * g;
            pn.ra = 0.0;
            pn.gmax = -1.0;
        }
    }
    true
}

/// B2 panel-end sentinels: check the panels not yet checked; true if one missed a feature at an
/// end (its error has then been raised so the loop refines it).
fn sentinels<F: FnMut(f64) -> f64>(
    ig: &mut Integrand<'_, F>,
    panels: &mut [Panel],
    ends: &mut std::collections::HashMap<u64, f64>,
    goal: f64,
) -> bool {
    let mut fired = false;
    for pn in panels.iter_mut() {
        if pn.checked {
            continue;
        }
        pn.checked = true;
        let wdt = pn.u1 - pn.u0;
        if pn.badu == pn.badu || pn.gmax < 0.0 || wdt < SENTINEL_MIN_WIDTH {
            continue;
        }
        let mut m = 0.0f64;
        for u in [pn.u0, pn.u1] {
            let g = *ends.entry(u.to_bits()).or_insert_with(|| ig.g(u));
            if g.abs() < f64::INFINITY {
                m = m.max(g.abs());
            }
        }
        if m > SENTINEL_RATIO * pn.gmax && m * wdt > goal {
            pn.e = pn.e.max(m * wdt);
            fired = true;
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(f: impl FnMut(f64) -> f64, a: f64, b: f64) -> f64 {
        quad(f, a, b, 1e-10, 0.0, -1.0).unwrap().value
    }

    #[test]
    fn basics() {
        assert!((q(|x| x * x, 0.0, 3.0) - 9.0).abs() < 1e-13);
        assert!((q(|x| (-x * x).exp(), f64::NEG_INFINITY, f64::INFINITY) - std::f64::consts::PI.sqrt()).abs() < 1e-12);
        assert!((q(|x| 1.0 / x.sqrt(), 0.0, 1.0) - 2.0).abs() < 1e-9);
        assert_eq!(q(|x| x, 1.0, 1.0), 0.0);
    }

    #[test]
    fn rt1_1_half_peak_on_infinite_range() {
        let f = |x: f64| (-((x - 1.0) / 1e-6).powi(2)).exp();
        let truth = 1e-6 * std::f64::consts::PI.sqrt();
        let v1 = quad_v1(f, 0.0, f64::INFINITY, 1e-10, 0.0, -1.0).unwrap().value;
        assert!((v1 / truth - 0.5).abs() < 1e-3, "v1 gave {v1}"); // v1's documented bug
        let v2 = q(f, 0.0, f64::INFINITY);
        assert!((v2 / truth - 1.0).abs() < 1e-8, "B2 gave {v2}");
    }

    #[test]
    fn bl1_bl2_peaks_on_subdivision_points() {
        let truth = (std::f64::consts::PI / 100.0).sqrt();
        let v = q(|x| (-(x - 1000.0).powi(2) * 100.0).exp(), 0.0, 2000.0);
        assert!((v / truth - 1.0).abs() < 1e-8, "{v}");
        let v = q(|x| (-x * x).exp(), -1e6, 1e6);
        assert!((v / std::f64::consts::PI.sqrt() - 1.0).abs() < 1e-8, "{v}");
    }

    #[test]
    fn bl3_strong_singularity_away_from_zero() {
        for (pw, truth) in [(-0.6f64, 0.0), (-0.8, 0.0)] {
            let _ = truth;
            let f = |x: f64| (x - 0.3).abs().powf(pw);
            let exact = (0.3f64.powf(pw + 1.0) + 0.7f64.powf(pw + 1.0)) / (pw + 1.0);
            let r = quad(f, 0.0, 1.0, 1e-10, 0.0, -1.0);
            let v = r.unwrap_or_else(|e| panic!("^{pw}: {e}")).value;
            assert!((v / exact - 1.0).abs() < 1e-6, "^{pw}: {v} vs {exact}");
        }
    }

    #[test]
    fn isolated_point_is_not_a_peak() {
        // a value at one point only: v1 gives 0 (with the D110 warning); B2 must stop refining
        let r = quad(|x| if x == 0.5 { 1.0 } else { 0.0 }, 0.0, 1.0, 1e-10, 0.0, -1.0).unwrap();
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn errors() {
        let e = quad(|x| 1.0 / x, 0.0, 1.0, 1e-10, 0.0, -1.0).unwrap_err();
        assert!(e.kind == err::QUAD || e.kind == err::QUAD_INF, "{e:?}");
    }
}
