//! A small antialiasing rasterizer for scenes: every primitive becomes filled polygons whose exact
//! area coverage is accumulated per pixel (the signed-area accumulation method of font-rs /
//! stb_truetype 2), then blended onto an RGB image. Text uses the embedded DejaVu Sans outlines.

use super::font_data::{GLYPHS, UNITS_PER_EM};
use super::scene::{text_width, Anchor, Prim, Rgb, Scene};

/// An RGB image, row-major, 3 bytes per pixel.
#[derive(Debug, Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

type Poly = Vec<(f64, f64)>;

fn signed_area(p: &Poly) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = p[i];
        let (x1, y1) = p[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    0.5 * a
}

fn positive(mut p: Poly) -> Poly {
    if signed_area(&p) < 0.0 {
        p.reverse();
    }
    p
}

fn circle(cx: f64, cy: f64, r: f64) -> Poly {
    let n = ((r * 4.0).ceil() as usize).clamp(12, 64);
    (0..n)
        .map(|k| {
            let a = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn stroke(pts: &[(f64, f64)], width: f64, round_joins: bool) -> Vec<Poly> {
    let hw = 0.5 * width;
    let mut out = Vec::new();
    for s in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (s[0], s[1]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len > 0.0) {
            continue;
        }
        let (nx, ny) = (-dy / len * hw, dx / len * hw);
        out.push(positive(vec![(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)]));
    }
    if round_joins && pts.len() > 2 {
        for &(x, y) in &pts[1..pts.len() - 1] {
            out.push(circle(x, y, hw));
        }
    }
    out
}

/// Glyph outlines of a text as polygons (orientation kept: holes cancel).
fn text_polys(text: &str, x: f64, y: f64, size: f64, anchor: Anchor, rotate: bool) -> Vec<Poly> {
    let s = size / UNITS_PER_EM;
    let w = text_width(text, size);
    let mut pen = match anchor {
        Anchor::Start => 0.0,
        Anchor::Middle => -0.5 * w,
        Anchor::End => -w,
    };
    let place = |u: f64, v: f64| -> (f64, f64) {
        // u along the text, v down from the baseline
        if rotate { (x + v, y - u) } else { (x + u, y + v) }
    };
    let mut polys = Vec::new();
    for ch in text.chars() {
        let (adv, cmds): (f64, &[i16]) = match GLYPHS.binary_search_by(|g| g.0.cmp(&ch)) {
            Ok(i) => (GLYPHS[i].1 as f64, GLYPHS[i].2),
            Err(_) => match GLYPHS.binary_search_by(|g| g.0.cmp(&'?')) {
                Ok(i) => (GLYPHS[i].1 as f64, GLYPHS[i].2),
                Err(_) => (1233.0, &[]),
            },
        };
        let mut cur: Poly = Vec::new();
        let mut last = (0.0f64, 0.0f64);
        let pt = |gx: f64, gy: f64| place(pen + gx * s, -gy * s);
        let mut i = 0;
        while i < cmds.len() {
            match cmds[i] {
                1 => {
                    if cur.len() > 2 {
                        polys.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    last = (cmds[i + 1] as f64, cmds[i + 2] as f64);
                    cur.push(pt(last.0, last.1));
                    i += 3;
                }
                2 => {
                    last = (cmds[i + 1] as f64, cmds[i + 2] as f64);
                    cur.push(pt(last.0, last.1));
                    i += 3;
                }
                3 => {
                    let (cx, cy) = (cmds[i + 1] as f64, cmds[i + 2] as f64);
                    let (ex, ey) = (cmds[i + 3] as f64, cmds[i + 4] as f64);
                    let steps = ((size / 4.0).ceil() as usize).clamp(3, 12);
                    for k in 1..=steps {
                        let t = k as f64 / steps as f64;
                        let mt = 1.0 - t;
                        let bx = mt * mt * last.0 + 2.0 * mt * t * cx + t * t * ex;
                        let by = mt * mt * last.1 + 2.0 * mt * t * cy + t * t * ey;
                        cur.push(pt(bx, by));
                    }
                    last = (ex, ey);
                    i += 5;
                }
                _ => {
                    if cur.len() > 2 {
                        polys.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    i += 1;
                }
            }
        }
        if cur.len() > 2 {
            polys.push(cur);
        }
        pen += adv * s;
    }
    polys
}

struct Canvas {
    img: Image,
    clip: (f64, f64, f64, f64),
}

impl Canvas {
    /// Fill the union (non-zero-ish: |accumulated winding| clamped to 1) of polygons with a colour.
    fn fill(&mut self, polys: &[Poly], color: Rgb, alpha: f64) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in polys {
            for &(x, y) in p {
                if x.is_finite() && y.is_finite() {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let bx0 = x0.max(cx0).floor().max(0.0) as i64;
        let by0 = y0.max(cy0).floor().max(0.0) as i64;
        let bx1 = x1.min(cx1).ceil().min(self.img.w as f64) as i64;
        let by1 = y1.min(cy1).ceil().min(self.img.h as f64) as i64;
        if bx1 <= bx0 || by1 <= by0 {
            return;
        }
        let bw = (bx1 - bx0) as usize;
        let bh = (by1 - by0) as usize;
        let stride = bw + 3;
        let mut acc = vec![0.0f32; stride * bh + 3];
        for p in polys {
            let n = p.len();
            for i in 0..n {
                let (ax, ay) = p[i];
                let (bx, by) = p[(i + 1) % n];
                line(&mut acc, stride, bw, bh, (ax - bx0 as f64, ay - by0 as f64), (bx - bx0 as f64, by - by0 as f64));
            }
        }
        // clip rectangle in buffer coordinates (fractional edges ignored: the clip is pixel aligned)
        let ccx0 = (cx0 - bx0 as f64).max(0.0);
        let ccy0 = (cy0 - by0 as f64).max(0.0);
        let ccx1 = (cx1 - bx0 as f64).min(bw as f64);
        let ccy1 = (cy1 - by0 as f64).min(bh as f64);
        for yy in 0..bh {
            let mut a = 0.0f32;
            for xx in 0..stride {
                a += acc[yy * stride + xx];
                if xx >= bw {
                    continue;
                }
                let cov = a.abs().min(1.0) as f64;
                if cov <= 0.0 {
                    continue;
                }
                let (fxx, fyy) = (xx as f64 + 0.5, yy as f64 + 0.5);
                if fxx < ccx0 || fxx > ccx1 || fyy < ccy0 || fyy > ccy1 {
                    continue;
                }
                let k = cov * alpha;
                let px = ((by0 as usize + yy) * self.img.w + bx0 as usize + xx) * 3;
                let c = [color.0, color.1, color.2];
                for ch in 0..3 {
                    let old = self.img.rgb[px + ch] as f64;
                    self.img.rgb[px + ch] = (old + (c[ch] as f64 - old) * k).round().clamp(0.0, 255.0) as u8;
                }
            }
            // carry the residue of this row into the next (it is ~0 for closed outlines)
        }
    }
}

/// Accumulate the signed area of the segment p0 → p1 (font-rs's `draw_line`).
fn line(a: &mut [f32], stride: usize, bw: usize, bh: usize, p0: (f64, f64), p1: (f64, f64)) {
    if !(p0.0.is_finite() && p0.1.is_finite() && p1.0.is_finite() && p1.1.is_finite()) || p0.1 == p1.1 {
        return;
    }
    let (dir, p0, p1) = if p0.1 < p1.1 { (1.0, p0, p1) } else { (-1.0, p1, p0) };
    let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
    let mut x = p0.0;
    if p0.1 < 0.0 {
        x -= p0.1 * dxdy;
    }
    let ystart = p0.1.max(0.0).floor() as usize;
    let yend = (p1.1.ceil().max(0.0) as usize).min(bh);
    let clampx = |v: f64| v.clamp(0.0, bw as f64 + 1.0);
    for y in ystart..yend {
        let ls = y * stride;
        let dy = (y as f64 + 1.0).min(p1.1) - (y as f64).max(p0.1);
        if dy <= 0.0 {
            continue;
        }
        let xnext = x + dxdy * dy;
        let d = (dy * dir) as f32;
        let (xa, xb) = if x < xnext { (clampx(x), clampx(xnext)) } else { (clampx(xnext), clampx(x)) };
        let x0floor = xa.floor();
        let x0i = x0floor as usize;
        let x1ceil = xb.ceil();
        let x1i = x1ceil as usize;
        if x1i <= x0i + 1 {
            let xmf = (0.5 * (xa + xb) - x0floor) as f32;
            a[ls + x0i] += d - d * xmf;
            a[ls + x0i + 1] += d * xmf;
        } else {
            let s = (1.0 / (xb - xa)) as f32;
            let x0f = (xa - x0floor) as f32;
            let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
            let x1f = (xb - x1ceil + 1.0) as f32;
            let am = 0.5 * s * x1f * x1f;
            a[ls + x0i] += d * a0;
            if x1i == x0i + 2 {
                a[ls + x0i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - x0f);
                a[ls + x0i + 1] += d * (a1 - a0);
                for xi in x0i + 2..x1i - 1 {
                    a[ls + xi] += d * s;
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                a[ls + x1i - 1] += d * (1.0 - a2 - am);
            }
            a[ls + x1i] += d * am;
        }
        x = xnext;
    }
}

/// Render a scene to an RGB image.
pub fn render(sc: &Scene) -> Image {
    let (w, h) = (sc.w.round() as usize, sc.h.round() as usize);
    let mut cv = Canvas { img: Image { w, h, rgb: vec![255; w * h * 3] }, clip: (0.0, 0.0, w as f64, h as f64) };
    let full = cv.clip;
    for p in &sc.prims {
        match p {
            Prim::Rect { x, y, w, h, fill, stroke: st } => {
                if let Some((c, a)) = fill {
                    cv.fill(&[positive(vec![(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)])], *c, *a);
                }
                if let Some((c, wd)) = st {
                    let pts = [(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h), (*x, *y)];
                    cv.fill(&stroke(&pts, *wd, false), *c, 1.0);
                }
            }
            Prim::Line { x1, y1, x2, y2, color, width, alpha } => {
                cv.fill(&stroke(&[(*x1, *y1), (*x2, *y2)], *width, false), *color, *alpha);
            }
            Prim::Polyline { pts, color, width } => {
                // long polylines in chunks (bounded buffers), joined by round joins
                for chunk in pts.chunks(200).enumerate().map(|(i, c)| {
                    let start = i * 200;
                    let end = (start + c.len() + 1).min(pts.len());
                    &pts[start..end]
                }) {
                    if chunk.len() >= 2 {
                        cv.fill(&stroke(chunk, *width, true), *color, 1.0);
                    }
                }
            }
            Prim::Polygon { pts, color, alpha } => cv.fill(&[positive(pts.clone())], *color, *alpha),
            Prim::Circle { cx, cy, r, color } => cv.fill(&[circle(*cx, *cy, *r)], *color, 1.0),
            Prim::Text { x, y, text, size, anchor, rotate, color } => {
                let polys = text_polys(text, *x, *y, *size, *anchor, *rotate);
                cv.fill(&polys, *color, 1.0);
            }
            Prim::ClipBegin { x, y, w, h } => cv.clip = (*x, *y, x + w, y + h),
            Prim::ClipEnd => cv.clip = full,
        }
    }
    cv.img
}
