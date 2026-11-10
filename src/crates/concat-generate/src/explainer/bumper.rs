// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! A channel's opening and closing, drawn here in its own colour: its name
//! arriving over coloured lights at the head of a video, and its closing
//! line under its name at the end. A few seconds each, the same on every
//! video of the channel, so a viewer knows whose video it is.

use super::{Align, Effect, Element, ElementKind, Explainer, Face, Light, Step};

/// How long the opening plays, and the closing.
pub const INTRO_SECONDS: f64 = 2.4;
/// How long the closing plays.
pub const OUTRO_SECONDS: f64 = 3.6;

/// The colour a channel with none of its own is drawn in.
pub const DEFAULT_COLOUR: &str = "#FFD400";

/// A channel's opening (`closing` false) or closing: `name` large, `line`
/// under it - its opening words, or what it asks of the viewer at the end
/// - lit in `colour`, on a `canvas` the frame's shape.
pub fn bumper(
    name: &str,
    line: &str,
    colour: &str,
    closing: bool,
    canvas: (f64, f64),
) -> Explainer {
    let (w, h) = canvas;
    let colour = if super::parse_colour(colour).is_some() {
        colour
    } else {
        DEFAULT_COLOUR
    };
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let line: String = line
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ");
    let fits = |text: &str, most: f64| {
        let wide = text.chars().count().max(4) as f64 * 0.6;
        (w * 0.8 / wide).min(most)
    };
    let name_size = fits(&name, h * 0.13);
    let line_size = fits(&line, h * 0.055);
    let cy = h * 0.45;
    let bar_w = (name.chars().count().max(4) as f64 * name_size * 0.3).min(w * 0.5);
    let mut elements = vec![
        Element {
            id: "name".into(),
            kind: ElementKind::Text,
            x: w / 2.0,
            y: cy,
            text: name,
            size: name_size,
            weight: 900.0,
            align: Align::Middle,
            font: Face::Display,
            fill: "#FFFFFF".into(),
            shadow: true,
            ..Element::default()
        },
        Element {
            id: "bar".into(),
            kind: ElementKind::Rect,
            x: w / 2.0 - bar_w / 2.0,
            y: cy + name_size * 0.42,
            w: bar_w,
            h: name_size * 0.09,
            r: name_size * 0.045,
            fill: colour.to_owned(),
            glow: 0.8,
            ..Element::default()
        },
    ];
    // The closing holds its words longer; the opening is over quickly.
    let (pop, bar, words) = if closing {
        (0.15, 0.55, 0.9)
    } else {
        (0.05, 0.35, 0.6)
    };
    let mut steps = vec![
        Step {
            target: "name".into(),
            effect: Effect::Pop,
            at: pop,
            duration: 0.5,
            ..Step::default()
        },
        Step {
            target: "bar".into(),
            effect: Effect::Grow,
            at: bar,
            duration: 0.5,
            ..Step::default()
        },
    ];
    if !line.is_empty() {
        elements.push(Element {
            id: "line".into(),
            kind: ElementKind::Text,
            x: w / 2.0,
            y: cy + name_size * 0.42 + line_size * 1.9,
            text: line,
            size: line_size,
            weight: 600.0,
            align: Align::Middle,
            fill: "#FFFFFFE0".into(),
            ..Element::default()
        });
        steps.push(Step {
            target: "line".into(),
            effect: Effect::FadeIn,
            at: words,
            duration: 0.5,
            ..Step::default()
        });
    }
    let reach = w.max(h);
    Explainer {
        background: vec!["#07070C".into(), "#12121C".into()],
        elements,
        steps,
        lights: [(0.2, 0.25, 0.5), (0.82, 0.7, 0.36)]
            .iter()
            .map(|&(x, y, opacity)| Light {
                x: w * x,
                y: h * y,
                r: reach * 0.42,
                color: colour.to_owned(),
                opacity,
            })
            .collect(),
        particles: 40,
        particle_color: colour.to_owned(),
        grid: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An opening and a closing are valid explainers in either frame, in
    /// any script, with or without a line, and whatever colour is typed.
    #[test]
    fn a_bumper_is_drawn_for_any_channel() {
        for canvas in [(1920.0, 1080.0), (1080.0, 1920.0)] {
            for (name, line, colour) in [
                ("فصل وقانون", "اشترك ليصلك كل جديد", "#00C2FF"),
                ("A Channel With A Fairly Long Name", "", "not a colour"),
                ("X", "Subscribe for more", "#E63946"),
            ] {
                for closing in [false, true] {
                    let mut card = bumper(name, line, colour, closing, canvas);
                    card.settle();
                    let problems = card.problems(canvas);
                    assert!(problems.is_empty(), "{canvas:?} {name}: {problems:?}");
                }
            }
        }
        let fallback = bumper("X", "", "nope", false, (1920.0, 1080.0));
        assert_eq!(fallback.particle_color, DEFAULT_COLOUR);
    }
}
