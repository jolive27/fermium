//! Native plots: SVG and PNG files, their messages, and a valid image structure.
use fermium_runtime::plot::*;

fn out_dir() -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("plots");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn demo(path: &str) -> PlotSpec {
    let xs: Vec<f64> = (0..50).map(|i| 0.2 * i as f64 + 0.1).collect();
    let data_x: Vec<f64> = (0..10).map(|i| i as f64 + 0.5).collect();
    PlotSpec {
        path: path.into(),
        series: vec![
            Series {
                x: data_x.clone(),
                y: data_x.iter().map(|x| 2.0 * x.sqrt() + 0.1 * (3.0 * x).sin()).collect(),
                style: Style::ErrorBars { xerr: None, yerr: Some(vec![0.2; 10]) },
                legend: "T".into(),
                xlabel: axis_label("L", "cm", None),
                ylabel: axis_label("T", "s", None),
                y_is_formula: false,
            },
            Series {
                x: xs.clone(),
                y: xs.iter().map(|x| 2.0 * x.sqrt()).collect(),
                style: Style::Band { yerr: xs.iter().map(|x| 0.05 * x).collect() },
                legend: "2π √(L/g)".into(),
                xlabel: axis_label("L", "cm", None),
                ylabel: axis_label("2π √(L/g)", "s", None),
                y_is_formula: true,
            },
            Series {
                x: xs.clone(),
                y: xs.iter().map(|x| 1.5 * x.sqrt()).collect(),
                style: Style::Line,
                legend: "T₀ (μ = 0.5)".into(),
                xlabel: axis_label("L", "cm", None),
                ylabel: axis_label("T₀", "s", None),
                y_is_formula: false,
            },
        ],
        title: Some("Pendulum period vs length".into()),
        ..Default::default()
    }
}

#[test]
fn svg_and_png_plots() {
    let d = out_dir();
    let svg = d.join("demo.svg").to_string_lossy().into_owned();
    let msg = save_plot(&demo(&svg)).unwrap();
    assert_eq!(msg, format!("plot saved to {svg}"));
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.starts_with("<?xml") && text.contains("T [s], T₀ [s]") && text.contains("L [cm]"));
    assert!(text.contains("Pendulum period vs length") && text.trim_end().ends_with("</svg>"));
    let png = d.join("sub/demo.png").to_string_lossy().into_owned();
    save_plot(&demo(&png)).unwrap();
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 770);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 495);
    assert!(bytes.len() < 200_000, "PNG is {} bytes", bytes.len());
    // log axes
    let mut spec = demo(&d.join("log.png").to_string_lossy());
    spec.logx = true;
    spec.logy = true;
    save_plot(&spec).unwrap();
}

#[test]
fn pde_plot_and_animation() {
    let d = out_dir();
    let xs: Vec<f64> = (0..=100).map(|i| i as f64 / 100.0).collect();
    let rows: Vec<Vec<f64>> = (0..40).map(|k| xs.iter().map(|x| (-(k as f64) * 0.05).exp() * (std::f64::consts::PI * x).sin()).collect()).collect();
    let labels: Vec<String> = (0..40).map(|k| format!("t = {} s", format_number(k as f64 * 0.01, 4, true))).collect();
    let base = PdePlot {
        path: d.join("heat.png").to_string_lossy().into_owned(),
        xs,
        labels,
        rows,
        xlabel: "x [m]".into(),
        ylabel: "u [K]".into(),
        title: None,
        animate: None,
    };
    let m = save_pde_plot(&base).unwrap();
    assert!(m.starts_with("plot saved to "));
    let mut anim = base.clone();
    anim.path = d.join("heat.gif").to_string_lossy().into_owned();
    anim.animate = Some(20);
    let m = save_pde_plot(&anim).unwrap();
    assert!(m.ends_with("(20 frames)"), "{m}");
    let g = std::fs::read(&anim.path).unwrap();
    assert_eq!(&g[..6], b"GIF89a");
    assert_eq!(*g.last().unwrap(), 0x3B);
    let mut frames = base.clone();
    frames.path = d.join("heat_anim.png").to_string_lossy().into_owned();
    frames.animate = Some(5);
    let m = save_pde_plot(&frames).unwrap();
    assert!(m.starts_with("animation saved as 5 PNG frames in ") && m.ends_with("heat_anim_frames/"), "{m}");
    assert!(d.join("heat_anim_frames/frame_0004.png").exists());
}
