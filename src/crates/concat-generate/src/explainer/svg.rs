// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! One frame of an explainer as an SVG document: every element as it looks
//! at that moment, under the camera.

use std::fmt::Write as _;

use kurbo::{ParamCurveArclen, Shape};

use super::{Align, Edge, Element, ElementKind, Explainer, Face, Look, icon, mix, parse_colour};

/// The families a face asks for, with the bundled script faces after it
/// for the letters it has none of.
fn families(face: Face) -> &'static str {
    match face {
        Face::Sans => "Inter, 'Noto Sans Arabic UI', 'Noto Sans Hebrew', sans-serif",
        Face::Display => "Montserrat, 'Noto Sans Arabic UI', 'Noto Sans Hebrew', sans-serif",
        Face::Serif => "'DM Serif Display', 'Noto Sans Arabic UI', 'Noto Sans Hebrew', serif",
        Face::Mono => "'Google Sans Code', 'Noto Sans Arabic UI', monospace",
    }
}

/// The frame at `t` (seconds), for an explainer whose cues are placed.
pub fn frame(explainer: &Explainer, canvas: (f64, f64), t: f64) -> String {
    let (w, h) = canvas;
    let mut out = String::with_capacity(4096);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"#
    );
    let colours: Vec<&str> = explainer
        .background
        .iter()
        .map(String::as_str)
        .filter(|c| parse_colour(c).is_some())
        .collect();
    match colours.as_slice() {
        [top, bottom, ..] => {
            let _ = write!(
                out,
                r#"<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs><rect width="{w}" height="{h}" fill="url(#bg)"/>"#,
                hex(top),
                hex(bottom)
            );
        }
        [one] => {
            let _ = write!(
                out,
                r#"<rect width="{w}" height="{h}" fill="{}"/>"#,
                hex(one)
            );
        }
        [] => {
            let _ = write!(out, r##"<rect width="{w}" height="{h}" fill="#0F172A"/>"##);
        }
    }
    let camera = explainer.camera(canvas, t);
    let _ = write!(
        out,
        r#"<g transform="translate({} {}) scale({}) translate({} {})">"#,
        w / 2.0,
        h / 2.0,
        camera.zoom,
        -camera.x,
        -camera.y
    );
    for element in &explainer.elements {
        draw(&mut out, explainer, element, t);
    }
    out.push_str("</g></svg>");
    out
}

/// One element, and a group's children inside it.
fn draw(out: &mut String, explainer: &Explainer, element: &Element, t: f64) {
    let look = explainer.look(element, t);
    let opacity = look.opacity * element.opacity;
    if opacity <= 0.002 {
        return;
    }
    let (ox, oy) = origin(element, look.edge);
    let mut transform = format!("translate({} {})", ox + look.dx, oy + look.dy);
    if look.rotate.abs() > 1e-6 {
        let _ = write!(transform, " rotate({})", look.rotate);
    }
    if (look.scale - 1.0).abs() > 1e-6 {
        let _ = write!(transform, " scale({})", look.scale.max(0.0001));
    }
    if look.grow < 0.9999 {
        let g = look.grow.max(0.0001);
        let (sx, sy) = match look.edge {
            Edge::Left | Edge::Right => (g, 1.0),
            Edge::Top | Edge::Bottom => (1.0, g),
            Edge::Center => (g, g),
        };
        let _ = write!(transform, " scale({sx} {sy})");
    }
    let _ = write!(transform, " translate({} {})", -ox, -oy);
    let _ = write!(
        out,
        r#"<g transform="{transform}" opacity="{}">"#,
        round(opacity)
    );
    let fill = paint(&element.fill, &look);
    let stroke = paint(&element.stroke, &look);
    let dash = if element.dash {
        format!(
            r#" stroke-dasharray="{} {}""#,
            element.width * 2.5,
            element.width * 2.0
        )
    } else {
        String::new()
    };
    match element.kind {
        ElementKind::Text => text(out, element, &look, &fill),
        ElementKind::Rect => {
            let outline = element.w * 2.0 + element.h * 2.0;
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}"{}/>"#,
                element.x,
                element.y,
                element.w.max(0.0),
                element.h.max(0.0),
                element.r.max(0.0),
                drawn_fill(&fill, &look),
                outline_attrs(&stroke, element.width, &look, outline, &dash)
            );
        }
        ElementKind::Circle => {
            let outline = std::f64::consts::TAU * element.r;
            let _ = write!(
                out,
                r#"<circle cx="{}" cy="{}" r="{}" fill="{}"{}/>"#,
                element.x,
                element.y,
                element.r.max(0.0),
                drawn_fill(&fill, &look),
                outline_attrs(&stroke, element.width, &look, outline, &dash)
            );
        }
        ElementKind::Line => {
            let d = polyline(&element.points);
            let length = path_length(&d);
            let _ = write!(
                out,
                r#"<path d="{d}" fill="none" stroke-linecap="round" stroke-linejoin="round"{}/>"#,
                outline_attrs(&stroke, element.width, &look, length, &dash)
            );
        }
        ElementKind::Arrow => arrow(out, element, &look, &stroke, &dash),
        ElementKind::Path => {
            let length = path_length(&element.d);
            let _ = write!(
                out,
                r#"<path d="{}" fill="{}" stroke-linecap="round" stroke-linejoin="round"{}/>"#,
                escape(element.d.trim()),
                drawn_fill(&fill, &look),
                outline_attrs(&stroke, element.width, &look, length, &dash)
            );
        }
        ElementKind::Icon => {
            if let Some(paths) = icon(&element.icon) {
                let size = element.size.max(1.0);
                let _ = write!(
                    out,
                    r#"<g transform="translate({} {}) scale({})" fill="none" stroke-linecap="round" stroke-linejoin="round">"#,
                    element.x - size / 2.0,
                    element.y - size / 2.0,
                    size / 24.0
                );
                let colour = if stroke == "none" {
                    "#FFFFFF".to_owned()
                } else {
                    stroke.clone()
                };
                for d in paths {
                    let length = path_length(d);
                    let _ = write!(
                        out,
                        r#"<path d="{d}"{}/>"#,
                        outline_attrs(&colour, element.width, &look, length, "")
                    );
                }
                out.push_str("</g>");
            }
        }
        ElementKind::Group => {
            let _ = write!(
                out,
                r#"<g transform="translate({} {})">"#,
                element.x, element.y
            );
            for child in &element.children {
                draw(out, explainer, child, t);
            }
            out.push_str("</g>");
        }
    }
    out.push_str("</g>");
}

/// The point an element turns, scales and grows about.
fn origin(element: &Element, edge: Edge) -> (f64, f64) {
    match element.kind {
        ElementKind::Rect => {
            let (x, y, w, h) = (element.x, element.y, element.w, element.h);
            match edge {
                Edge::Left => (x, y + h / 2.0),
                Edge::Right => (x + w, y + h / 2.0),
                Edge::Top => (x + w / 2.0, y),
                Edge::Bottom => (x + w / 2.0, y + h),
                Edge::Center => (x + w / 2.0, y + h / 2.0),
            }
        }
        ElementKind::Line | ElementKind::Arrow => {
            let points = &element.points;
            match (points.first(), points.last()) {
                (Some(a), Some(b)) => match edge {
                    Edge::Left | Edge::Top => (a[0], a[1]),
                    Edge::Right | Edge::Bottom => (b[0], b[1]),
                    Edge::Center => ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0),
                },
                _ => (element.x, element.y),
            }
        }
        ElementKind::Path => match kurbo::BezPath::from_svg(element.d.trim()) {
            Ok(path) => {
                let b = path.bounding_box();
                match edge {
                    Edge::Left => (b.x0, b.center().y),
                    Edge::Right => (b.x1, b.center().y),
                    Edge::Top => (b.center().x, b.y0),
                    Edge::Bottom => (b.center().x, b.y1),
                    Edge::Center => (b.center().x, b.center().y),
                }
            }
            Err(_) => (element.x, element.y),
        },
        _ => (element.x, element.y),
    }
}

/// A colour as the frame paints it: turned by a `color` step, or as given;
/// `none` where there is none.
fn paint(colour: &str, look: &Look) -> String {
    let given = parse_colour(colour);
    match (given, look.tint) {
        (Some(_), Some((from, to, k))) => hex_of(mix(from, to, k)),
        (Some(rgba), None) => hex_of(rgba),
        (None, _) => "none".to_owned(),
    }
}

/// A shape's inside while its outline is drawn on: none until the outline
/// is nearly round, then fading up.
fn drawn_fill(fill: &str, look: &Look) -> String {
    if fill == "none" || look.drawn >= 0.999 {
        return fill.to_owned();
    }
    let k = ((look.drawn - 0.7) / 0.3).clamp(0.0, 1.0);
    if k <= 0.0 {
        return "none".to_owned();
    }
    match parse_colour(fill) {
        Some([r, g, b, a]) => hex_of([r, g, b, (a as f64 * k).round() as u8]),
        None => fill.to_owned(),
    }
}

/// An outline's paint, thickness, and the dash that draws it on.
fn outline_attrs(stroke: &str, width: f64, look: &Look, length: f64, dash: &str) -> String {
    if stroke == "none" || width <= 0.0 {
        return String::new();
    }
    let mut attrs = format!(r#" stroke="{stroke}" stroke-width="{}""#, width.max(0.0));
    if look.drawn < 0.999 && length > 0.0 {
        let _ = write!(
            attrs,
            r#" stroke-dasharray="{} {}" stroke-dashoffset="{}""#,
            round(length),
            round(length + 1.0),
            round(length * (1.0 - look.drawn.max(0.0)))
        );
    } else {
        attrs.push_str(dash);
    }
    attrs
}

/// A text, line by line, its words up to how many are typed.
fn text(out: &mut String, element: &Element, look: &Look, fill: &str) {
    let content = match look.count {
        Some(n) => crate::motion::count_in(&element.text, n),
        None => element.text.clone(),
    };
    let lines: Vec<&str> = content.split('\n').collect();
    let words: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
    let mut shown = if look.typed >= 0.999 {
        usize::MAX
    } else {
        (look.typed * words as f64).ceil() as usize
    };
    let anchor = match element.align {
        Align::Start => "start",
        Align::Middle => "middle",
        Align::End => "end",
    };
    let size = element.size.max(1.0);
    let lead = size * 1.25;
    let weight = element.weight.clamp(100.0, 900.0).round();
    let rtl = content.chars().any(is_rtl);
    // Right to left where the words are Arabic or Hebrew, so a line that
    // opens on a figure keeps it at its start; and those words thickened
    // for a heavy weight, the bundled faces of those scripts having one
    // weight alone.
    let mut extra = String::new();
    if rtl {
        extra.push_str(r#" direction="rtl""#);
        if weight >= 650.0 && fill != "none" {
            let _ = write!(
                extra,
                r#" stroke="{fill}" stroke-width="{}" stroke-linejoin="round""#,
                round(size * 0.022 * (weight - 500.0) / 400.0)
            );
        }
    }
    // The block is placed by its first line's baseline, the rest under it.
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" font-family="{}" font-size="{size}" font-weight="{weight}" text-anchor="{anchor}" fill="{fill}"{extra}>"#,
        element.x,
        element.y,
        families(element.font),
    );
    for (n, line) in lines.iter().enumerate() {
        let mut kept = Vec::new();
        for word in line.split_whitespace() {
            if shown == 0 {
                break;
            }
            kept.push(word);
            shown = shown.saturating_sub(1);
        }
        let dy = if n == 0 { 0.0 } else { lead };
        // An Arabic line embedded right to left: the renderer orders a
        // line by its marks, not by `direction`, and a line opening on a
        // figure otherwise read backwards.
        let words = kept.join(" ");
        let words = if rtl && !words.is_empty() {
            format!("\u{202B}{words}\u{202C}")
        } else {
            words
        };
        let _ = write!(
            out,
            r#"<tspan x="{}" dy="{dy}">{}</tspan>"#,
            element.x,
            escape(&words)
        );
    }
    out.push_str("</text>");
}

/// An arrow: its line, bowed by `curve`, drawn on; its head once the line
/// reaches it.
fn arrow(out: &mut String, element: &Element, look: &Look, stroke: &str, dash: &str) {
    let points = &element.points;
    let (Some(a), Some(b)) = (points.first(), points.last()) else {
        return;
    };
    let d = if points.len() > 2 {
        polyline(points)
    } else {
        let (mx, my) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        // The bow's control point, to the left of the way it goes.
        let (cx, cy) = (mx + dy * element.curve, my - dx * element.curve);
        format!("M{} {}Q{} {} {} {}", a[0], a[1], cx, cy, b[0], b[1])
    };
    let length = path_length(&d);
    let _ = write!(
        out,
        r#"<path d="{d}" fill="none" stroke-linecap="round" stroke-linejoin="round"{}/>"#,
        outline_attrs(stroke, element.width, look, length, dash)
    );
    let shown = ((look.drawn - 0.85) / 0.15).clamp(0.0, 1.0);
    if shown <= 0.0 || stroke == "none" {
        return;
    }
    // The head points along the line's last stretch.
    let before = if points.len() > 2 {
        points[points.len() - 2]
    } else {
        let (mx, my) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        [mx + dy * element.curve, my - dx * element.curve]
    };
    let angle = (b[1] - before[1]).atan2(b[0] - before[0]);
    let size = (element.width * 4.0).max(16.0);
    let corner = |turn: f64| {
        (
            b[0] - size * (angle + turn).cos(),
            b[1] - size * (angle + turn).sin(),
        )
    };
    let (l, r) = (corner(0.45), corner(-0.45));
    let _ = write!(
        out,
        r#"<path d="M{} {}L{} {}L{} {}Z" fill="{stroke}" opacity="{}"/>"#,
        b[0],
        b[1],
        l.0,
        l.1,
        r.0,
        r.1,
        round(shown)
    );
}

/// Whether `c` is written right to left: Arabic, Hebrew, Syriac, Thaana.
fn is_rtl(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

/// Path data through `points`.
fn polyline(points: &[[f64; 2]]) -> String {
    let mut d = String::new();
    for (n, [x, y]) in points.iter().enumerate() {
        let _ = write!(d, "{}{x} {y}", if n == 0 { "M" } else { "L" });
    }
    d
}

/// How long a path is, in its own units; 0 for data that does not parse.
pub fn path_length(d: &str) -> f64 {
    kurbo::BezPath::from_svg(d.trim())
        .map(|path| path.segments().map(|segment| segment.arclen(0.5)).sum())
        .unwrap_or(0.0)
}

/// A colour as `#rrggbb` or `#rrggbbaa`.
fn hex(colour: &str) -> String {
    parse_colour(colour).map_or_else(|| "#000000".to_owned(), hex_of)
}

fn hex_of([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Words made safe inside XML.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line drawn half way is dashed to half its length; text is escaped;
    /// typing shows the first words.
    #[test]
    fn a_frame_draws_what_the_steps_say() {
        let explainer: Explainer = serde_json::from_value(serde_json::json!({
            "elements": [
                {"id": "l", "kind": "line", "points": [[0, 0], [100, 0]], "stroke": "#FFFFFF", "width": 4},
                {"id": "t", "kind": "text", "text": "Salt & water <in> the sea", "x": 50, "y": 50, "size": 40, "fill": "#FFFFFF"}
            ],
            "steps": [
                {"target": "l", "effect": "draw", "at": 0, "duration": 1, "ease": "linear"},
                {"target": "t", "effect": "type", "at": 0, "duration": 1}
            ]
        }))
        .unwrap();
        let svg = frame(&explainer, (1920.0, 1080.0), 0.5);
        assert!(svg.contains(r#"stroke-dashoffset="50""#), "{svg}");
        // Half way, three of the six words.
        assert!(svg.contains(">Salt &amp; water</tspan>"), "{svg}");
        assert!(!svg.contains("sea"), "{svg}");
        let done = frame(&explainer, (1920.0, 1080.0), 2.0);
        assert!(!done.contains("stroke-dashoffset"));
        assert!(done.contains("&lt;in&gt; the sea"));
        assert!((path_length("M0 0L30 40") - 50.0).abs() < 1e-6);
    }
}
