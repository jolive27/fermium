//! Native plots (spec §B6: replaces matplotlib): SVG and PNG line/marker plots with unit-labelled
//! axes, log axes, several series with a legend, error bars and ±1σ bands (D124), and PDE plots
//! (`plot u vs x [animate over t]`, D83) as a PNG of six times or an animated GIF.
//!
//! # API (for the evaluator)
//!
//! ```text
//! use fermium_runtime::plot::*;
//! let spec = PlotSpec {
//!     path: "out/fig.png".into(),            // .svg, .png (anything else: PNG)
//!     series: vec![Series {
//!         x: xs_in_display_units, y: ys_in_display_units,
//!         style: Style::Line,                // Line | Points | ErrorBars { xerr, yerr } | Band { yerr }
//!         legend: "T".into(),                // the plotted name (v1: s["ylabel"])
//!         xlabel: axis_label("L", "cm", None),       // "L [cm]"
//!         ylabel: axis_label("T", "s", None),        // "T [s]"
//!         y_is_formula: is_formula_label("T"),
//!     }],
//!     title: None, logx: false, logy: false, xlim: None, ylim: None, revx: false, revy: false,
//!     equal_aspect: false,                   // v1: orbits (solxy, or x and y of the same dimension)
//! };
//! let line = save_plot(&spec)?;              // "plot saved to /abs/out/fig.png"
//! ```
//!
//! Values are in display units already (the caller divides SI values by the unit's factor after
//! subtracting its offset, as v1's `make_plot`); errors likewise (divided by |factor|).
//! [`sample_solution`] gives the 600-point Hermite samples v1 plots for an ODE solution.
//! [`save_pde_plot`] is v1's `m3rt.animate`.
//!
//! Layout follows v1's own native plotter (`aot_data.c`, `fermium build`: 770×495, matplotlib's
//! colours, 5 % margins, 1-2-2.5-5 ticks, the legend top right), which mirrors v1's matplotlib
//! output; images are not pixel-identical to matplotlib's (see NOTES.md).

mod font_data;
mod gif;
mod png;
mod raster;
mod scene;
mod svg;

pub use scene::{Anchor, Prim, Rgb, Scene};

use std::path::Path;

/// How a series is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Style {
    /// a line (v1: computed values, solutions)
    Line,
    /// markers only (v1: measured data, `s["points"]`)
    Points,
    /// markers with error bars (uncertain values, D124); None where there are no errors
    ErrorBars { xerr: Option<Vec<f64>>, yerr: Option<Vec<f64>> },
    /// a line with a ±1σ band (an uncertain curve, D124)
    Band { yerr: Vec<f64> },
}

/// One plotted series (values in display units).
#[derive(Debug, Clone)]
pub struct Series {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub style: Style,
    /// the legend text (v1: the plotted expression as written)
    pub legend: String,
    /// this series' x axis label (the first series' is used)
    pub xlabel: String,
    /// this series' y axis label (combined by [`y_axis_label`])
    pub ylabel: String,
    /// the plotted thing is a formula, not a name (see [`is_formula_label`])
    pub y_is_formula: bool,
}

/// A whole plot.
#[derive(Debug, Clone, Default)]
pub struct PlotSpec {
    pub path: String,
    pub series: Vec<Series>,
    pub title: Option<String>,
    pub logx: bool,
    pub logy: bool,
    /// axis ranges (display units), from `xlim`/`ylim` (D161)
    pub xlim: Option<(f64, f64)>,
    pub ylim: Option<(f64, f64)>,
    pub revx: bool,
    pub revy: bool,
    /// the same scale on both axes (v1: orbits, when no range is given)
    pub equal_aspect: bool,
}

/// v1's `axis_label`: the name (or the given label) then the unit in brackets, unless the unit is
/// plain or the given label already names its own unit ("T [MeV]").
pub fn axis_label(name: &str, unit_name: &str, given: Option<&str>) -> String {
    let unit = unit_name != "" && unit_name != "1";
    match given {
        Some(g) => {
            if unit && !g.contains('[') {
                format!("{g} [{unit_name}]")
            } else {
                g.to_string()
            }
        }
        None => {
            if unit {
                format!("{name} [{unit_name}]")
            } else {
                name.to_string()
            }
        }
    }
}

/// v1's `is_formula_label`: a plotted formula (`2π √(L/g)`), not a name (`T`, `N_Mo`, `sol.x`).
pub fn is_formula_label(label: &str) -> bool {
    label.is_empty() || !label.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '′' || c == '\'')
}

/// v1's `y_axis_label`: the distinct labels, leaving out formulas when a named series is there too
/// (D253).
pub fn y_axis_label(labels: &[(String, bool)]) -> String {
    let named: Vec<&String> = labels.iter().filter(|(_, f)| !f).map(|(l, _)| l).collect();
    let pick: Vec<&String> = if named.is_empty() { labels.iter().map(|(l, _)| l).collect() } else { named };
    let mut out: Vec<&String> = Vec::new();
    for l in pick {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}

/// v1's `format_number(x, sig, trim)` (units.py; aot_rt.c `fmt_num`): fixed notation unless
/// rounding would leave non-significant zeros, else `m×10ⁿ`.
pub fn format_number(x: f64, sig: usize, trim: bool) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    if trim && x == x.trunc() && x.abs() < 1e7 && sig >= 6 {
        return format!("{}", x as i64);
    }
    let sig = sig.clamp(1, 17);
    let s = format!("{:.*e}", sig - 1, x);
    let (m, e) = s.split_once('e').unwrap();
    let exp: i32 = e.parse().unwrap();
    let trim_zeros = |t: &str| -> String {
        if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t.to_string() }
    };
    if (-4..6).contains(&exp) && (trim || exp <= sig as i32) {
        let decimals = (sig as i32 - 1 - exp).max(0) as usize;
        let r: f64 = format!("{m}e{e}").parse().unwrap();
        let s = format!("{:.*}", decimals, r);
        return if trim { trim_zeros(&s) } else { s };
    }
    let m = if trim { trim_zeros(m) } else { m.to_string() };
    format!("{m}×10{}", superscript(exp))
}

/// "−12" as "⁻¹²"
pub fn superscript(n: i32) -> String {
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string().chars().map(|c| if c == '-' { '⁻' } else { SUP[c.to_digit(10).unwrap() as usize] }).collect()
}

/// v1's `sample_solution`: 600 samples of an ODE solution component (cubic Hermite between steps),
/// or the steps themselves when there are 600 or more. `t`, `y`, `dy` as in `numerics::ode::Sol`.
pub fn sample_solution(t: &[f64], y: &[f64], dy: &[f64], dim: usize, comp: usize, use_dy: bool) -> (Vec<f64>, Vec<f64>) {
    let n = t.len();
    let npts = 600;
    let get = |v: &[f64], i: usize| v[i * dim + comp];
    if n >= npts || n < 2 {
        return (t.to_vec(), (0..n).map(|i| if use_dy { get(dy, i) } else { get(y, i) }).collect());
    }
    let (t0, t1) = (t[0], t[n - 1]);
    let sg = if t1 >= t0 { 1.0 } else { -1.0 };
    let tt = crate::numerics::eigen::linspace(t0, t1, npts);
    let mut out = Vec::with_capacity(npts);
    for &x in &tt {
        // searchsorted(sg t, sg x, side = right) − 1, clipped to [0, n − 2]
        let mut i = t.partition_point(|&v| sg * v <= sg * x);
        i = i.saturating_sub(1).min(n - 2);
        let h = t[i + 1] - t[i];
        let u = (x - t[i]) / h;
        let (y0, y1, d0, d1) = (get(y, i), get(y, i + 1), get(dy, i), get(dy, i + 1));
        let v = if use_dy {
            let (a00, a10) = (6.0 * u * u - 6.0 * u, 3.0 * u * u - 4.0 * u + 1.0);
            let (a01, a11) = (6.0 * u - 6.0 * u * u, 3.0 * u * u - 2.0 * u);
            (a00 * y0 + a10 * h * d0 + a01 * y1 + a11 * h * d1) / h
        } else {
            let (u2, u3) = (u * u, u * u * u);
            let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
            let h10 = u3 - 2.0 * u2 + u;
            let h01 = -2.0 * u3 + 3.0 * u2;
            let h11 = u3 - u2;
            h00 * y0 + h10 * h * d0 + h01 * y1 + h11 * h * d1
        };
        out.push(v);
    }
    (tt, out)
}

/// The kind of image a path asks for.
fn is_svg(path: &str) -> bool {
    path.to_lowercase().ends_with(".svg")
}

fn absolute(path: &str) -> String {
    let p = Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf())
    };
    abs.to_string_lossy().into_owned()
}

fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = Path::new(path).parent() {
        if !d.as_os_str().is_empty() {
            std::fs::create_dir_all(d).map_err(|e| format!("can't write {path}: {e}"))?;
        }
    }
    std::fs::write(path, bytes).map_err(|e| format!("can't write {path}: {e}"))
}

/// Draw and save a plot. Ok: v1's line "plot saved to <absolute path>"; Err: the reason, for v1's
/// "(plot not saved: …)".
pub fn save_plot(spec: &PlotSpec) -> Result<String, String> {
    let scene = scene::build_plot(spec);
    let bytes = if is_svg(&spec.path) { svg::render(&scene).into_bytes() } else { png::encode_rgb(&raster::render(&scene)) };
    write_file(&spec.path, &bytes)?;
    Ok(format!("plot saved to {}", absolute(&spec.path)))
}

/// Render a plot to an image in memory (SVG text or PNG bytes) without writing it.
pub fn render_plot(spec: &PlotSpec, svg: bool) -> Vec<u8> {
    let scene = scene::build_plot(spec);
    if svg { svg::render(&scene).into_bytes() } else { png::encode_rgb(&raster::render(&scene)) }
}

/// A PDE solution to draw (v1's `m3rt.animate` inputs, values already in display units).
#[derive(Debug, Clone)]
pub struct PdePlot {
    pub path: String,
    /// grid x values (display units)
    pub xs: Vec<f64>,
    /// snapshot times (for the labels) and one row of values per snapshot (|ψ|² for a complex ψ)
    pub labels: Vec<String>,
    pub rows: Vec<Vec<f64>>,
    pub xlabel: String,
    pub ylabel: String,
    pub title: Option<String>,
    /// `animate`: a GIF with this many frames at most; None: one image with 6 times
    pub animate: Option<usize>,
}

/// v1's `animate`: without `animate`, one image with the solution at 6 times ("plot saved to …");
/// with it, an animated GIF ("animation saved to … (N frames)") when the path ends in .gif, else
/// PNG frames in a folder ("animation saved as N PNG frames in …_frames/").
pub fn save_pde_plot(p: &PdePlot) -> Result<String, String> {
    let n = p.rows.len();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in &p.rows {
        for &v in r {
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
    }
    let pad = 0.05 * if hi > lo { hi - lo } else { 1.0 };
    let ylim = (lo - pad, hi + pad);
    let xlim = (p.xs[0], p.xs[p.xs.len() - 1]);
    let base = |series: Vec<Series>| PlotSpec {
        path: p.path.clone(),
        series,
        title: p.title.clone(),
        xlim: Some(xlim),
        ylim: Some(ylim),
        ..Default::default()
    };
    let line = |k: usize, legend: String| Series {
        x: p.xs.clone(),
        y: p.rows[k].clone(),
        style: Style::Line,
        legend,
        xlabel: p.xlabel.clone(),
        ylabel: p.ylabel.clone(),
        y_is_formula: false,
    };
    let abs = absolute(&p.path);
    match p.animate {
        None => {
            let m = n.min(6);
            let idx: Vec<usize> = crate::numerics::eigen::linspace(0.0, (n - 1) as f64, m).iter().map(|v| v.round_ties_even() as usize).collect();
            let spec = base(idx.iter().map(|&k| line(k, p.labels[k].clone())).collect());
            let mut spec = spec;
            spec.series.iter_mut().for_each(|s| s.legend = s.legend.clone());
            save_plot(&spec)?;
            Ok(format!("plot saved to {abs}"))
        }
        Some(frames) => {
            let f = frames.min(n).max(1);
            let idx: Vec<usize> = crate::numerics::eigen::linspace(0.0, (n - 1) as f64, f).iter().map(|v| v.round_ties_even() as usize).collect();
            let images: Vec<raster::Image> = idx
                .iter()
                .map(|&k| {
                    let mut sc = scene::build_plot(&base(vec![line(k, String::new())]));
                    sc.add_corner_label(&p.labels[k]);
                    raster::render(&sc)
                })
                .collect();
            if p.path.to_lowercase().ends_with(".gif") {
                write_file(&p.path, &gif::encode_animation(&images, 15))?;
                Ok(format!("animation saved to {abs} ({} frames)", idx.len()))
            } else {
                let stem = Path::new(&p.path).with_extension("");
                let folder = format!("{}_frames", stem.to_string_lossy());
                for (i, im) in images.iter().enumerate() {
                    write_file(&format!("{folder}/frame_{i:04}.png"), &png::encode_rgb(im))?;
                }
                let astem = Path::new(&abs).with_extension("");
                Ok(format!("animation saved as {} PNG frames in {}_frames{}", idx.len(), astem.to_string_lossy(), std::path::MAIN_SEPARATOR))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_numbers() {
        assert_eq!(axis_label("T", "s", None), "T [s]");
        assert_eq!(axis_label("n", "1", None), "n");
        assert_eq!(axis_label("T", "MeV", Some("T [MeV]")), "T [MeV]");
        assert!(is_formula_label("2π √(L/g)"));
        assert!(!is_formula_label("sol.x"));
        let l = vec![("T [s]".to_string(), false), ("2π √(L/g) [s]".to_string(), true), ("T [s]".to_string(), false)];
        assert_eq!(y_axis_label(&l), "T [s]");
        assert_eq!(format_number(333333.0, 3, false), "3.33×10⁵");
        assert_eq!(format_number(0.5, 3, false), "0.500");
        assert_eq!(format_number(2.5, 6, true), "2.5");
        assert_eq!(format_number(1e-7, 6, true), "1×10⁻⁷");
    }
}
