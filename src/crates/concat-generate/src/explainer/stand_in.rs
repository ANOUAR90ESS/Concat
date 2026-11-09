// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! A shot that could not be made, stood in for by a designed card: the
//! scene's idea in large words on a glass plate, over coloured lights and
//! drifting specks - drawn here, for nothing, rather than a bare wash of
//! colour that says nothing.

use super::{Align, Effect, Element, ElementKind, Explainer, Face, Light, Step};

/// The palettes a stand-in is lit with, one after another down the video:
/// the backdrop's two shades, then three lights.
const PALETTES: [([&str; 2], [&str; 3]); 7] = [
    // Neon night.
    (["#0B0620", "#1A0B3D"], ["#FF2E97", "#00E5FF", "#7C4DFF"]),
    // Aurora.
    (["#03141A", "#06283A"], ["#00F5A0", "#00C2FF", "#9D4EDD"]),
    // Sunset.
    (["#1A0710", "#3A0F1E"], ["#FF6B35", "#FF2E63", "#FFC145"]),
    // Ocean.
    (["#020C1F", "#06244A"], ["#00B4D8", "#48CAE4", "#3A86FF"]),
    // Ember.
    (["#140704", "#2E0F05"], ["#FF4D00", "#FFB703", "#E63946"]),
    // Forest.
    (["#04130A", "#0A2A17"], ["#2DC653", "#B5E48C", "#00BBF9"]),
    // Cosmos.
    (["#07041A", "#160B33"], ["#B517FF", "#4CC9F0", "#F72585"]),
];

/// A card for shot `n` saying `idea`, on a `canvas` the frame's shape.
pub fn stand_in(idea: &str, n: usize, canvas: (f64, f64)) -> Explainer {
    let (w, h) = canvas;
    let (background, lights) = PALETTES[n % PALETTES.len()];
    let tall = h > w;
    let per_line = if tall { 14 } else { 22 };
    let lines = wrap(idea, per_line, 3);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    // As large as fits the plate: about 0.6 of a size a character.
    let plate_w = w * if tall { 0.84 } else { 0.72 };
    let size = (plate_w * 0.86 / (longest.max(6.0) * 0.6)).min(h * 0.11);
    let leading = size * 1.25;
    let plate_h = leading * lines.len() as f64 + size * 1.6;
    // Above the captions' fifth, a little over the middle.
    let cy = h * 0.42;
    let accent = lights[0];
    let plate = Element {
        id: "plate".into(),
        kind: ElementKind::Rect,
        x: (w - plate_w) / 2.0,
        y: cy - plate_h / 2.0,
        w: plate_w,
        h: plate_h,
        r: size * 0.5,
        gradient: vec!["#FFFFFF2E".into(), "#FFFFFF0A".into()],
        angle: 120.0,
        stroke: "#FFFFFF40".into(),
        width: 2.0,
        shadow: true,
        ..Element::default()
    };
    let bar = Element {
        id: "bar".into(),
        kind: ElementKind::Rect,
        x: w / 2.0 - plate_w * 0.12,
        y: cy + plate_h / 2.0 - size * 0.45,
        w: plate_w * 0.24,
        h: size * 0.12,
        r: size * 0.06,
        gradient: vec![lights[0].into(), lights[1].into()],
        glow: 0.7,
        ..Element::default()
    };
    let words = Element {
        id: "idea".into(),
        kind: ElementKind::Text,
        x: w / 2.0,
        y: cy - leading * (lines.len() as f64 - 1.0) / 2.0 + size * 0.2,
        text: lines.join("\n"),
        size,
        weight: 800.0,
        align: Align::Middle,
        font: Face::Display,
        fill: "#FFFFFF".into(),
        shadow: true,
        ..Element::default()
    };
    let spots = [(0.18, 0.22), (0.85, 0.3), (0.5, 0.85)];
    Explainer {
        background: background.iter().map(|c| (*c).to_owned()).collect(),
        elements: vec![plate, bar, words],
        steps: vec![
            Step {
                target: "plate".into(),
                effect: Effect::Pop,
                at: 0.0,
                duration: 0.6,
                ..Step::default()
            },
            Step {
                target: "idea".into(),
                effect: Effect::FadeIn,
                at: 0.2,
                duration: 0.6,
                ..Step::default()
            },
            Step {
                target: "bar".into(),
                effect: Effect::Grow,
                at: 0.5,
                duration: 0.6,
                ..Step::default()
            },
        ],
        lights: lights
            .iter()
            .zip(spots)
            .map(|(color, (x, y))| Light {
                x: w * x,
                y: h * y,
                r: w.max(h) * 0.38,
                color: (*color).to_owned(),
                opacity: 0.42,
            })
            .collect(),
        particles: 50,
        particle_color: accent.to_owned(),
        grid: false,
    }
}

/// `text` in lines of at most about `width` characters, at most `most`
/// lines, the last cut short with an ellipsis when it runs on.
fn wrap(text: &str, width: usize, most: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => {
                if lines.len() == most {
                    if let Some(last) = lines.last_mut() {
                        last.push('…');
                    }
                    break;
                }
                lines.push(word.to_owned());
            }
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stand_in_is_a_valid_explainer_clear_of_the_captions() {
        for canvas in [(1920.0, 1080.0), (1080.0, 1920.0)] {
            for n in 0..7 {
                let mut card = stand_in(
                    "أقوى قنبلة فجّرها الإنسان على جزيرة نائية في المحيط الهادئ",
                    n,
                    canvas,
                );
                card.settle();
                let problems = card.problems(canvas);
                assert!(problems.is_empty(), "{canvas:?} {n}: {problems:?}");
            }
        }
    }

    #[test]
    fn long_ideas_wrap_and_stop_at_three_lines() {
        let lines = wrap("one two three four five six seven eight nine ten", 9, 3);
        assert_eq!(lines.len(), 3);
        assert!(lines.last().unwrap().ends_with('…'));
        assert_eq!(wrap("", 9, 3), [""]);
    }
}
