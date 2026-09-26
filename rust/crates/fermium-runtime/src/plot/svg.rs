//! A scene as SVG text (the same markup style as v1's `fermium build` plots).

use super::scene::{Anchor, Prim, Scene};
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn render(sc: &Scene) -> String {
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        o,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\" font-family=\"DejaVu Sans, Helvetica, Arial, sans-serif\">",
        w = sc.w,
        h = sc.h
    );
    let mut clip_id = 0;
    for p in &sc.prims {
        match p {
            Prim::Rect { x, y, w, h, fill, stroke } => {
                let f = match fill {
                    Some((c, a)) if *a < 1.0 => format!("fill=\"{}\" fill-opacity=\"{a}\"", c.hex()),
                    Some((c, _)) => format!("fill=\"{}\"", c.hex()),
                    None => "fill=\"none\"".into(),
                };
                let s = match stroke {
                    Some((c, wd)) => format!(" stroke=\"{}\" stroke-width=\"{wd}\"", c.hex()),
                    None => String::new(),
                };
                let _ = writeln!(o, "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" {f}{s}/>");
            }
            Prim::Line { x1, y1, x2, y2, color, width, alpha } => {
                let a = if *alpha < 1.0 { format!(" stroke-opacity=\"{alpha}\"") } else { String::new() };
                let wd = if *width != 1.0 { format!(" stroke-width=\"{width}\"") } else { String::new() };
                let _ = writeln!(o, "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{}\"{wd}{a}/>", color.hex());
            }
            Prim::Polyline { pts, color, width } => {
                let _ = write!(o, "<polyline class=\"series\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linejoin=\"round\" points=\"", color.hex());
                for (x, y) in pts {
                    let _ = write!(o, "{x:.2},{y:.2} ");
                }
                o.push_str("\"/>\n");
            }
            Prim::Polygon { pts, color, alpha } => {
                let _ = write!(o, "<polygon fill=\"{}\" fill-opacity=\"{alpha}\" stroke=\"none\" points=\"", color.hex());
                for (x, y) in pts {
                    let _ = write!(o, "{x:.2},{y:.2} ");
                }
                o.push_str("\"/>\n");
            }
            Prim::Circle { cx, cy, r, color } => {
                let _ = writeln!(o, "<circle class=\"series\" cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r}\" fill=\"{}\"/>", color.hex());
            }
            Prim::Text { x, y, text, size, anchor, rotate, color } => {
                let an = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let pos = if *rotate {
                    format!("transform=\"translate({x:.2} {y:.2}) rotate(-90)\"")
                } else {
                    format!("x=\"{x:.2}\" y=\"{y:.2}\"")
                };
                let _ = writeln!(o, "<text {pos} font-size=\"{size}\" fill=\"{}\" text-anchor=\"{an}\">{}</text>", color.hex(), esc(text));
            }
            Prim::ClipBegin { x, y, w, h } => {
                clip_id += 1;
                let _ = writeln!(o, "<defs><clipPath id=\"area{clip_id}\"><rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\"/></clipPath></defs>");
                let _ = writeln!(o, "<g clip-path=\"url(#area{clip_id})\">");
            }
            Prim::ClipEnd => o.push_str("</g>\n"),
        }
    }
    o.push_str("</svg>\n");
    o
}
