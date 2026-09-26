//! The drawing of a plot as primitives in pixel coordinates (y down), shared by the SVG writer and
//! the rasterizer. The layout is v1's native plotter's (`fm_plot_done` in aot_data.c).

use super::{format_number, superscript, PlotSpec, Style};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

pub const BLACK: Rgb = Rgb(0, 0, 0);
pub const WHITE: Rgb = Rgb(255, 255, 255);
const TEXT: Rgb = Rgb(0x22, 0x22, 0x22);

/// matplotlib's default colour cycle
pub const COLORS: [Rgb; 10] = [
    Rgb(0x1f, 0x77, 0xb4),
    Rgb(0xff, 0x7f, 0x0e),
    Rgb(0x2c, 0xa0, 0x2c),
    Rgb(0xd6, 0x27, 0x28),
    Rgb(0x94, 0x67, 0xbd),
    Rgb(0x8c, 0x56, 0x4b),
    Rgb(0xe3, 0x77, 0xc2),
    Rgb(0x7f, 0x7f, 0x7f),
    Rgb(0xbc, 0xbd, 0x22),
    Rgb(0x17, 0xbe, 0xcf),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Rect { x: f64, y: f64, w: f64, h: f64, fill: Option<(Rgb, f64)>, stroke: Option<(Rgb, f64)> },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, color: Rgb, width: f64, alpha: f64 },
    Polyline { pts: Vec<(f64, f64)>, color: Rgb, width: f64 },
    Polygon { pts: Vec<(f64, f64)>, color: Rgb, alpha: f64 },
    Circle { cx: f64, cy: f64, r: f64, color: Rgb },
    Text { x: f64, y: f64, text: String, size: f64, anchor: Anchor, rotate: bool, color: Rgb },
    ClipBegin { x: f64, y: f64, w: f64, h: f64 },
    ClipEnd,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub w: f64,
    pub h: f64,
    pub prims: Vec<Prim>,
    /// the plot area (left, top, right, bottom)
    pub area: (f64, f64, f64, f64),
}

impl Scene {
    /// A label in the top-left corner of the plot area (the time of an animation frame).
    pub fn add_corner_label(&mut self, text: &str) {
        let (l, t, _, _) = self.area;
        self.prims.push(Prim::Text { x: l + 10.0, y: t + 18.0, text: text.to_string(), size: 12.0, anchor: Anchor::Start, rotate: false, color: TEXT });
    }
}

/// The width of a text in pixels at a font size, from DejaVu Sans' advances.
pub fn text_width(s: &str, size: f64) -> f64 {
    let fd = super::font_data::GLYPHS;
    let mut w = 0.0;
    for ch in s.chars() {
        let adv = match fd.binary_search_by(|g| g.0.cmp(&ch)) {
            Ok(i) => fd[i].1 as f64,
            Err(_) => 1233.0, // like '?'-wide
        };
        w += adv;
    }
    w / super::font_data::UNITS_PER_EM * size
}

struct Axis {
    lo: f64,
    hi: f64,
    log: bool,
}

impl Axis {
    fn t(&self, v: f64) -> f64 {
        if self.log { v.log10() } else { v }
    }
}

fn range_of(mut lo: f64, mut hi: f64, log: bool) -> Axis {
    if !(lo <= hi) {
        lo = 0.0;
        hi = 1.0;
    }
    if lo == hi {
        if log {
            lo -= 1.0;
            hi += 1.0;
        } else if lo == 0.0 {
            lo = -1.0;
            hi = 1.0;
        } else {
            let d = lo.abs() * 0.05;
            lo -= d;
            hi += d;
        }
    }
    let m = (hi - lo) * 0.05;
    Axis { lo: lo - m, hi: hi + m, log }
}

fn make_ticks(a: &Axis) -> Vec<f64> {
    let mut t = Vec::new();
    if a.log {
        let e0 = (a.lo - 1e-9).ceil() as i32;
        let e1 = (a.hi + 1e-9).floor() as i32;
        let mut stride = 1;
        while (e1 - e0) / stride > 8 {
            stride += 1;
        }
        let mut e = e0;
        while e <= e1 && t.len() < 64 {
            t.push(10f64.powi(e));
            e += stride;
        }
        if e1 - e0 < 1 {
            for e in e0 - 1..=e1 {
                for m in [2.0, 5.0] {
                    let v = m * 10f64.powi(e);
                    if v.log10() >= a.lo && v.log10() <= a.hi && t.len() < 64 {
                        t.push(v);
                    }
                }
            }
        }
        return t;
    }
    let span = a.hi - a.lo;
    let raw = span / 6.0;
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let step = (if f < 1.5 { 1.0 } else if f < 2.25 { 2.0 } else if f < 3.5 { 2.5 } else if f < 7.5 { 5.0 } else { 10.0 }) * mag;
    if !(step > 0.0) || !step.is_finite() {
        return t;
    }
    let mut k = (a.lo / step - 1e-9).ceil();
    while k * step <= a.hi + step * 1e-9 && t.len() < 64 {
        let mut v = k * step;
        if v.abs() < step * 1e-9 {
            v = 0.0;
        }
        t.push(v);
        k += 1.0;
    }
    t
}

fn tick_label(v: f64, log: bool) -> String {
    if log {
        let lv = v.log10();
        let e = lv.round() as i32;
        if (lv - e as f64).abs() < 1e-9 {
            if (0..=3).contains(&e) {
                return format!("{}", 10f64.powi(e));
            }
            return format!("10{}", superscript(e));
        }
    }
    format_number(v, 6, true)
}

/// Lay out a plot (v1's `fm_plot_done`, plus error bars and bands).
pub fn build_plot(spec: &PlotSpec) -> Scene {
    let (w, h) = (770.0, 495.0);
    let title = spec.title.as_deref().filter(|t| !t.is_empty());
    let (l, r, t, b) = (90.0, w - 25.0, if title.is_some() { 45.0 } else { 25.0 }, h - 62.0);
    // ranges (in log10 for log axes)
    let (mut xlo, mut xhi, mut ylo, mut yhi) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut see = |x: f64, y: f64| {
        if !x.is_finite() || !y.is_finite() || (spec.logx && x <= 0.0) || (spec.logy && y <= 0.0) {
            return;
        }
        let tx = if spec.logx { x.log10() } else { x };
        let ty = if spec.logy { y.log10() } else { y };
        xlo = xlo.min(tx);
        xhi = xhi.max(tx);
        ylo = ylo.min(ty);
        yhi = yhi.max(ty);
    };
    for s in &spec.series {
        for j in 0..s.x.len().min(s.y.len()) {
            let (x, y) = (s.x[j], s.y[j]);
            match &s.style {
                Style::ErrorBars { xerr, yerr } => {
                    let ex = xerr.as_ref().map(|e| e[j]).unwrap_or(0.0);
                    let ey = yerr.as_ref().map(|e| e[j]).unwrap_or(0.0);
                    see(x - ex, y - ey);
                    see(x + ex, y + ey);
                }
                Style::Band { yerr } => {
                    see(x, y - yerr[j]);
                    see(x, y + yerr[j]);
                }
                _ => see(x, y),
            }
        }
    }
    let mut ax = range_of(xlo, xhi, spec.logx);
    let mut ay = range_of(ylo, yhi, spec.logy);
    if let Some((a, c)) = spec.xlim {
        ax.lo = if spec.logx { a.log10() } else { a };
        ax.hi = if spec.logx { c.log10() } else { c };
    }
    if let Some((a, c)) = spec.ylim {
        ay.lo = if spec.logy { a.log10() } else { a };
        ay.hi = if spec.logy { c.log10() } else { c };
    }
    if spec.equal_aspect && !spec.logx && !spec.logy && spec.xlim.is_none() && spec.ylim.is_none() {
        let sx = (ax.hi - ax.lo) / (r - l);
        let sy = (ay.hi - ay.lo) / (b - t);
        if sx > sy {
            let c = (ay.lo + ay.hi) / 2.0;
            let hh = sx * (b - t) / 2.0;
            ay.lo = c - hh;
            ay.hi = c + hh;
        } else {
            let c = (ax.lo + ax.hi) / 2.0;
            let hh = sy * (r - l) / 2.0;
            ax.lo = c - hh;
            ax.hi = c + hh;
        }
    }
    let fx = |v: f64| (ax.t(v) - ax.lo) / (ax.hi - ax.lo);
    let fy = |v: f64| (ay.t(v) - ay.lo) / (ay.hi - ay.lo);
    let px = |v: f64| if spec.revx { r - fx(v) * (r - l) } else { l + fx(v) * (r - l) };
    let py = |v: f64| if spec.revy { t + fy(v) * (b - t) } else { b - fy(v) * (b - t) };
    let mut prims = vec![Prim::Rect { x: 0.0, y: 0.0, w, h, fill: Some((WHITE, 1.0)), stroke: None }];
    // grid, ticks and their labels
    for v in make_ticks(&ax) {
        let x = px(v);
        if x < l - 0.5 || x > r + 0.5 {
            continue;
        }
        prims.push(Prim::Line { x1: x, y1: t, x2: x, y2: b, color: BLACK, width: 1.0, alpha: 0.12 });
        prims.push(Prim::Line { x1: x, y1: b, x2: x, y2: b + 5.0, color: BLACK, width: 1.0, alpha: 1.0 });
        prims.push(Prim::Text { x, y: b + 18.0, text: tick_label(v, ax.log), size: 11.0, anchor: Anchor::Middle, rotate: false, color: TEXT });
    }
    for v in make_ticks(&ay) {
        let y = py(v);
        if y < t - 0.5 || y > b + 0.5 {
            continue;
        }
        prims.push(Prim::Line { x1: l, y1: y, x2: r, y2: y, color: BLACK, width: 1.0, alpha: 0.12 });
        prims.push(Prim::Line { x1: l - 5.0, y1: y, x2: l, y2: y, color: BLACK, width: 1.0, alpha: 1.0 });
        prims.push(Prim::Text { x: l - 8.0, y: y + 4.0, text: tick_label(v, ay.log), size: 11.0, anchor: Anchor::End, rotate: false, color: TEXT });
    }
    // the data
    prims.push(Prim::ClipBegin { x: l, y: t, w: r - l, h: b - t });
    let good = |x: f64, y: f64| x.is_finite() && y.is_finite() && !(ax.log && x <= 0.0) && !(ay.log && y <= 0.0);
    for (i, s) in spec.series.iter().enumerate() {
        let col = COLORS[i % 10];
        let n = s.x.len().min(s.y.len());
        match &s.style {
            Style::Points | Style::ErrorBars { .. } => {
                if let Style::ErrorBars { xerr, yerr } = &s.style {
                    for j in 0..n {
                        let (x, y) = (s.x[j], s.y[j]);
                        if !good(x, y) {
                            continue;
                        }
                        if let Some(e) = yerr {
                            let (y1, y2) = (py(y - e[j]), py(y + e[j]));
                            if y1.is_finite() && y2.is_finite() {
                                let cx = px(x);
                                prims.push(Prim::Line { x1: cx, y1, x2: cx, y2, color: col, width: 1.5, alpha: 1.0 });
                                for yy in [y1, y2] {
                                    prims.push(Prim::Line { x1: cx - 4.0, y1: yy, x2: cx + 4.0, y2: yy, color: col, width: 1.5, alpha: 1.0 });
                                }
                            }
                        }
                        if let Some(e) = xerr {
                            let (x1, x2) = (px(x - e[j]), px(x + e[j]));
                            if x1.is_finite() && x2.is_finite() {
                                let cy = py(y);
                                prims.push(Prim::Line { x1, y1: cy, x2, y2: cy, color: col, width: 1.5, alpha: 1.0 });
                                for xx in [x1, x2] {
                                    prims.push(Prim::Line { x1: xx, y1: cy - 4.0, x2: xx, y2: cy + 4.0, color: col, width: 1.5, alpha: 1.0 });
                                }
                            }
                        }
                    }
                }
                for j in 0..n {
                    let (x, y) = (s.x[j], s.y[j]);
                    if good(x, y) {
                        prims.push(Prim::Circle { cx: px(x), cy: py(y), r: 3.5, color: col });
                    }
                }
            }
            Style::Line | Style::Band { .. } => {
                if let Style::Band { yerr } = &s.style {
                    let mut up: Vec<(f64, f64)> = Vec::new();
                    let mut down: Vec<(f64, f64)> = Vec::new();
                    for j in 0..n {
                        let (x, y) = (s.x[j], s.y[j]);
                        if good(x, y - yerr[j]) && good(x, y + yerr[j]) {
                            up.push((px(x), py(y + yerr[j])));
                            down.push((px(x), py(y - yerr[j])));
                        }
                    }
                    down.reverse();
                    up.extend(down);
                    if up.len() >= 3 {
                        prims.push(Prim::Polygon { pts: up, color: col, alpha: 0.25 });
                    }
                }
                let mut run: Vec<(f64, f64)> = Vec::new();
                for j in 0..n {
                    let (x, y) = (s.x[j], s.y[j]);
                    if !good(x, y) {
                        if run.len() > 0 {
                            prims.push(Prim::Polyline { pts: std::mem::take(&mut run), color: col, width: 1.8 });
                        }
                        continue;
                    }
                    run.push((px(x), py(y)));
                }
                if !run.is_empty() {
                    prims.push(Prim::Polyline { pts: run, color: col, width: 1.8 });
                }
            }
        }
    }
    prims.push(Prim::ClipEnd);
    // frame, axis labels, title
    prims.push(Prim::Rect { x: l, y: t, w: r - l, h: b - t, fill: None, stroke: Some((BLACK, 1.0)) });
    let xl = spec.series.first().map(|s| s.xlabel.clone()).unwrap_or_default();
    prims.push(Prim::Text { x: (l + r) / 2.0, y: h - 18.0, text: xl, size: 13.0, anchor: Anchor::Middle, rotate: false, color: TEXT });
    let labels: Vec<(String, bool)> = spec.series.iter().map(|s| (s.ylabel.clone(), s.y_is_formula)).collect();
    prims.push(Prim::Text { x: 20.0, y: (t + b) / 2.0, text: super::y_axis_label(&labels), size: 13.0, anchor: Anchor::Middle, rotate: true, color: TEXT });
    if let Some(tt) = title {
        prims.push(Prim::Text { x: (l + r) / 2.0, y: 28.0, text: tt.to_string(), size: 15.0, anchor: Anchor::Middle, rotate: false, color: TEXT });
    }
    let shown: Vec<&super::Series> = spec.series.iter().filter(|s| !s.legend.is_empty()).collect();
    if spec.series.len() > 1 && !shown.is_empty() {
        let lw = 48.0 + spec.series.iter().map(|s| text_width(&s.legend, 11.0)).fold(0.0, f64::max);
        let lh = 8.0 + 18.0 * spec.series.len() as f64;
        // matplotlib's loc="best" (the corners): the corner covering the fewest data points
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for s in &spec.series {
            for j in 0..s.x.len().min(s.y.len()) {
                if good(s.x[j], s.y[j]) {
                    pts.push((px(s.x[j]), py(s.y[j])));
                }
            }
        }
        let corners = [(r - lw - 10.0, t + 10.0), (l + 10.0, t + 10.0), (l + 10.0, b - lh - 10.0), (r - lw - 10.0, b - lh - 10.0)];
        let mut best = (usize::MAX, corners[0]);
        for c in corners {
            let n = pts.iter().filter(|(x, y)| *x >= c.0 - 4.0 && *x <= c.0 + lw + 4.0 && *y >= c.1 - 4.0 && *y <= c.1 + lh + 4.0).count();
            if n < best.0 {
                best = (n, c);
            }
        }
        let (lx, ly) = best.1;
        prims.push(Prim::Rect { x: lx, y: ly, w: lw, h: lh, fill: Some((WHITE, 0.85)), stroke: Some((Rgb(0xcc, 0xcc, 0xcc), 1.0)) });
        for (i, s) in spec.series.iter().enumerate() {
            let col = COLORS[i % 10];
            let y = ly + 16.0 + 18.0 * i as f64;
            match s.style {
                Style::Points => prims.push(Prim::Circle { cx: lx + 18.0, cy: y - 4.0, r: 3.5, color: col }),
                Style::ErrorBars { .. } => {
                    prims.push(Prim::Line { x1: lx + 18.0, y1: y - 10.0, x2: lx + 18.0, y2: y + 2.0, color: col, width: 1.5, alpha: 1.0 });
                    prims.push(Prim::Circle { cx: lx + 18.0, cy: y - 4.0, r: 3.5, color: col });
                }
                Style::Band { .. } => {
                    prims.push(Prim::Rect { x: lx + 8.0, y: y - 9.0, w: 20.0, h: 10.0, fill: Some((col, 0.25)), stroke: None });
                    prims.push(Prim::Line { x1: lx + 8.0, y1: y - 4.0, x2: lx + 28.0, y2: y - 4.0, color: col, width: 1.8, alpha: 1.0 });
                }
                Style::Line => prims.push(Prim::Line { x1: lx + 8.0, y1: y - 4.0, x2: lx + 28.0, y2: y - 4.0, color: col, width: 1.8, alpha: 1.0 }),
            }
            prims.push(Prim::Text { x: lx + 34.0, y, text: s.legend.clone(), size: 11.0, anchor: Anchor::Start, rotate: false, color: TEXT });
        }
    }
    Scene { w, h, prims, area: (l, t, r, b) }
}
