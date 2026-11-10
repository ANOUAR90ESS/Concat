// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Animated explainers: a scene the writer directs as a motion designer
//! would - shapes, icons, arrows, words and paths on a canvas, and the
//! steps that bring them on, move them, draw them and count them up, each
//! when the narration says the word it belongs to - drawn by the app frame
//! by frame and written as a clip. Nothing is asked of a picture model:
//! the writer's JSON is the whole of it.
//!
//! The canvas is in pixels, 1920 by 1080 for a wide video (see [`canvas`]),
//! `y` down, and is scaled to the frame when drawn. Times are seconds from
//! the start of the shot.

mod bumper;
pub mod icons;
mod render;
mod stand_in;
mod svg;

use serde::{Deserialize, Serialize};

pub use bumper::{DEFAULT_COLOUR, INTRO_SECONDS, OUTRO_SECONDS, bumper};
pub use render::{Fonts, RATE, render_clip, render_still, still_png};
pub use stand_in::stand_in;

/// The most elements an explainer may hold: more is clutter, and a frame
/// the phone draws slowly.
pub const MOST_ELEMENTS: usize = 40;
/// The smallest words may be on the canvas, in pixels: smaller is not read
/// on a phone.
pub const SMALLEST_TEXT: f64 = 26.0;
/// Where the captions' band starts, as a share of the canvas's height:
/// nothing the viewer must read goes below it.
pub const CAPTIONS_BAND: f64 = 0.78;

/// The canvas an explainer is drawn on for a video of `aspect`, in pixels.
pub fn canvas(aspect: &str) -> (f64, f64) {
    match aspect {
        "9:16" => (1080.0, 1920.0),
        "1:1" => (1080.0, 1080.0),
        _ => (1920.0, 1080.0),
    }
}

/// One animated explainer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Explainer {
    /// The backdrop: one colour, or two for a wash from the top to the
    /// bottom.
    pub background: Vec<String>,
    /// What is on the canvas, drawn in this order: later over earlier.
    pub elements: Vec<Element>,
    /// What happens, and when.
    pub steps: Vec<Step>,
    /// Soft coloured lights behind everything, drifting slowly: the depth
    /// a modern explainer is lit with.
    pub lights: Vec<Light>,
    /// How many specks float up through the frame, twinkling: dust in a
    /// beam, stars, plankton. 0 for none.
    pub particles: u32,
    /// The specks' colour.
    pub particle_color: String,
    /// A faint grid behind everything: a blueprint, a lab, a screen.
    pub grid: bool,
}

/// A soft light behind the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Light {
    /// Its centre, across.
    pub x: f64,
    /// Its centre, down.
    pub y: f64,
    /// How far it reaches, in pixels.
    pub r: f64,
    /// Its colour.
    pub color: String,
    /// How bright at its centre, 0 to 1.
    pub opacity: f64,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            x: 0.0,
            y: 0.0,
            r: 500.0,
            color: "#3B82F6".to_owned(),
            opacity: 0.45,
        }
    }
}

/// One thing on the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Element {
    /// Its name, for the steps.
    pub id: String,
    /// What it is.
    pub kind: ElementKind,
    /// Where it is - a text's anchor, a circle's or an icon's centre, a
    /// rectangle's top left corner, a group's origin - across.
    pub x: f64,
    /// The same, down.
    pub y: f64,
    /// A rectangle's width.
    pub w: f64,
    /// A rectangle's height.
    pub h: f64,
    /// A circle's radius; a rectangle's corner radius.
    pub r: f64,
    /// A line's or an arrow's points, start first.
    pub points: Vec<[f64; 2]>,
    /// How far an arrow bows to its left (negative: right), as a share of
    /// its length.
    pub curve: f64,
    /// A path's outline, in SVG path data, in canvas pixels.
    pub d: String,
    /// The words of a text; a new line at each `\n`.
    pub text: String,
    /// A text's size or an icon's, in pixels.
    pub size: f64,
    /// A text's weight, 100 to 900.
    pub weight: f64,
    /// Where a text's anchor is along its line.
    pub align: Align,
    /// A text's face.
    pub font: Face,
    /// Which icon, by name (see [`icons::ICONS`]).
    pub icon: String,
    /// The inside's colour, `#rrggbb` or `#rrggbbaa`; empty for none.
    pub fill: String,
    /// The outline's colour; an icon's, a line's and an arrow's colour.
    pub stroke: String,
    /// The outline's thickness, in pixels.
    pub width: f64,
    /// A dashed outline.
    pub dash: bool,
    /// How opaque it is at rest, 0 to 1.
    pub opacity: f64,
    /// Two or three colours the fill (an outline, for a line, an arrow or
    /// an icon) runs through instead of one.
    pub gradient: Vec<String>,
    /// The gradient's direction, in degrees: 0 left to right, 90 top to
    /// bottom.
    pub angle: f64,
    /// The gradient from the middle out rather than across.
    pub radial: bool,
    /// A glow around it in its own colours, 0 to 1: neon, light, heat.
    pub glow: f64,
    /// A soft shadow under it, lifting it off the background.
    pub shadow: bool,
    /// A group's elements, placed from its origin.
    pub children: Vec<Element>,
}

impl Default for Element {
    fn default() -> Self {
        Element {
            id: String::new(),
            kind: ElementKind::Text,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            r: 0.0,
            points: Vec::new(),
            curve: 0.0,
            d: String::new(),
            text: String::new(),
            size: 0.0,
            weight: 0.0,
            align: Align::Middle,
            font: Face::Sans,
            icon: String::new(),
            fill: String::new(),
            stroke: String::new(),
            width: 0.0,
            dash: false,
            opacity: 1.0,
            gradient: Vec::new(),
            angle: 0.0,
            radial: false,
            glow: 0.0,
            shadow: false,
            children: Vec::new(),
        }
    }
}

/// The kinds of element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementKind {
    /// Words.
    #[default]
    Text,
    /// A rectangle, rounded by `r`.
    Rect,
    /// A disc or a ring.
    Circle,
    /// A straight line, or one through several points.
    Line,
    /// A line with a head at its end, bowed by `curve`.
    Arrow,
    /// Any outline, in SVG path data.
    Path,
    /// One of the built-in icons.
    Icon,
    /// Elements that move together.
    Group,
}

/// Where a text's anchor is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// The line starts at the anchor (its left in a left-to-right script).
    Start,
    /// The line is centred on it.
    #[default]
    Middle,
    /// The line ends at it.
    End,
}

/// The faces a text may be set in; each falls back to the bundled Arabic,
/// Hebrew and other scripts' faces for what it does not draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    /// Inter: clean, for labels and body.
    #[default]
    Sans,
    /// Montserrat: heavy and wide, for headings and figures.
    Display,
    /// DM Serif Display: for a quote or a period piece.
    Serif,
    /// A monospace face, for code and data.
    Mono,
}

/// One step of the animation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Step {
    /// The element it acts on, or `camera` for the view itself.
    pub target: String,
    /// What it does.
    pub effect: Effect,
    /// When it starts, in seconds from the shot's start.
    pub at: f64,
    /// A word or a few from the narration: the step starts as they are
    /// said, where they are found, and at `at` where they are not.
    pub cue: String,
    /// How long it takes; 0 is the effect's own length.
    pub duration: f64,
    /// How it eases.
    pub ease: Ease,
    /// Where `move` takes it, as `[x, y]`, or where the camera looks, as
    /// `[x, y, zoom]`.
    pub to: Vec<f64>,
    /// What `scale`, `rotate` (degrees) and `count` go to.
    pub value: f64,
    /// What `count` starts from.
    pub from: f64,
    /// What `color` turns it to.
    pub color: String,
    /// Which edge `grow` grows from: left, right, top, bottom, or centre.
    pub edge: Edge,
}

/// What a step does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Fades in.
    #[default]
    FadeIn,
    /// Fades out.
    FadeOut,
    /// Grows past its size and settles, fading in.
    Pop,
    /// Grows from an edge: a bar from the bottom, a line from the left.
    Grow,
    /// Its outline is drawn on, start to end: a line, an arrow, a path, an
    /// icon, a shape's outline.
    Draw,
    /// Its words come up one at a time.
    Type,
    /// Moves to `to`.
    Move,
    /// Scales to `value`.
    Scale,
    /// Turns to `value` degrees.
    Rotate,
    /// Changes colour to `color`.
    Color,
    /// Its number counts from `from` to `value`.
    Count,
    /// Swells and settles once: look here.
    Pulse,
    /// Bobs up and down from then on, by `value` pixels: a thing afloat,
    /// alive.
    Float,
    /// Turns from then on, `value` degrees a second: a wheel, a planet,
    /// an atom.
    Spin,
    /// Shakes: a warning, a mistake.
    Shake,
    /// The camera moves and zooms to `to` = `[x, y, zoom]`.
    Zoom,
}

impl Effect {
    /// Whether the element is unseen until this step starts.
    pub fn enters(self) -> bool {
        matches!(
            self,
            Effect::FadeIn | Effect::Pop | Effect::Grow | Effect::Draw | Effect::Type
        )
    }

    /// The effect's own length, in seconds.
    fn length(self) -> f64 {
        match self {
            Effect::FadeIn | Effect::FadeOut | Effect::Color => 0.5,
            Effect::Pop | Effect::Scale => 0.55,
            Effect::Grow | Effect::Move | Effect::Rotate => 0.8,
            Effect::Draw => 1.0,
            Effect::Type => 0.9,
            Effect::Count => 1.4,
            Effect::Pulse => 0.6,
            Effect::Shake => 0.5,
            Effect::Float | Effect::Spin => 1.0,
            Effect::Zoom => 1.3,
        }
    }
}

/// How a step eases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    /// Fast, then slowing into place.
    #[default]
    Out,
    /// Slow, then fast.
    In,
    /// Slow at both ends.
    InOut,
    /// At one speed.
    Linear,
    /// A touch past, then back.
    Back,
    /// Past and back, settling like a spring.
    Spring,
}

impl Ease {
    /// `p` (0 to 1) eased.
    pub fn apply(self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Ease::Linear => p,
            Ease::In => p * p * p,
            Ease::Out => 1.0 - (1.0 - p).powi(3),
            Ease::InOut => {
                if p < 0.5 {
                    4.0 * p * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
                }
            }
            Ease::Back => {
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2)
            }
            Ease::Spring => {
                if p >= 1.0 {
                    1.0
                } else {
                    1.0 - (-6.0 * p).exp() * (p * 12.0).cos()
                }
            }
        }
    }
}

/// The edge `grow` grows from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    /// From its middle, both ways.
    #[default]
    Center,
    /// From its left edge, rightwards.
    Left,
    /// From its right edge, leftwards.
    Right,
    /// From its top edge, down.
    Top,
    /// From its bottom edge, up: a bar.
    Bottom,
}

impl Step {
    /// How long the step takes.
    pub fn length(&self) -> f64 {
        if self.duration.is_finite() && self.duration > 0.0 {
            self.duration
        } else {
            self.effect.length()
        }
    }

    /// How far through it is at `t`, 0 to 1, not eased.
    pub fn progress(&self, t: f64) -> f64 {
        ((t - self.at) / self.length()).clamp(0.0, 1.0)
    }
}

impl Explainer {
    /// Every element, groups' children included, in drawing order.
    pub fn all(&self) -> Vec<&Element> {
        fn walk<'a>(elements: &'a [Element], out: &mut Vec<&'a Element>) {
            for element in elements {
                out.push(element);
                walk(&element.children, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.elements, &mut out);
        out
    }

    /// The steps with their cues placed: each step whose cue `when` finds
    /// starts then, the rest at their `at`; then in the order they start.
    pub fn cued(&self, when: &dyn Fn(&str) -> Option<f64>) -> Explainer {
        let mut cued = self.clone();
        for step in &mut cued.steps {
            if !step.cue.trim().is_empty()
                && let Some(at) = when(step.cue.trim())
            {
                step.at = at;
            }
            if !step.at.is_finite() || step.at < 0.0 {
                step.at = 0.0;
            }
        }
        cued.steps
            .sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
        cued
    }

    /// When the last step ends.
    pub fn ends(&self) -> f64 {
        self.steps
            .iter()
            .map(|step| step.at + step.length())
            .fold(0.0, f64::max)
    }

    /// What is wrong with it, in sentences the writer can act on: names
    /// used twice or not at all, steps on nothing, icons there are none
    /// of, colours that are not colours, numbers that are not numbers,
    /// words too small to read or off the canvas. Empty when it is sound.
    pub fn problems(&self, canvas: (f64, f64)) -> Vec<String> {
        let mut problems = Vec::new();
        let all = self.all();
        if all.is_empty() {
            problems.push("there are no elements".to_owned());
        }
        if all.len() > MOST_ELEMENTS {
            problems.push(format!(
                "{} elements: keep to {MOST_ELEMENTS} at most",
                all.len()
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for element in &all {
            let name = if element.id.is_empty() {
                format!("a {:?} element", element.kind).to_lowercase()
            } else {
                format!("\"{}\"", element.id)
            };
            if element.id.trim().is_empty() {
                problems.push(format!("{name} has no id"));
            } else if element.id == "camera" {
                problems.push("\"camera\" is the view, not an element's id".to_owned());
            } else if !seen.insert(element.id.as_str()) {
                problems.push(format!("the id {name} is used twice"));
            }
            for (what, colour) in [("fill", &element.fill), ("stroke", &element.stroke)] {
                if !colour.is_empty() && colour != "none" && parse_colour(colour).is_none() {
                    problems.push(format!("{name}: {what} \"{colour}\" is not #rrggbb"));
                }
            }
            let numbers = [
                element.x,
                element.y,
                element.w,
                element.h,
                element.r,
                element.size,
                element.width,
                element.curve,
                element.opacity,
            ];
            if numbers.iter().any(|n| !n.is_finite()) {
                problems.push(format!("{name} has a number that is not a number"));
            }
            match element.kind {
                ElementKind::Text => {
                    if element.text.trim().is_empty() {
                        problems.push(format!("{name} is a text with no words"));
                    }
                    if element.size > 0.0 && element.size < SMALLEST_TEXT {
                        problems.push(format!(
                            "{name}: size {} is too small to read; {SMALLEST_TEXT} at least",
                            element.size
                        ));
                    }
                    if element.text.chars().count() > 90 {
                        problems.push(format!(
                            "{name} has {} characters: a frame is not a page, say it in fewer",
                            element.text.chars().count()
                        ));
                    }
                }
                ElementKind::Rect => {
                    if element.w <= 0.0 || element.h <= 0.0 {
                        problems.push(format!("{name} is a rect with no w or h"));
                    }
                }
                ElementKind::Circle => {
                    if element.r <= 0.0 {
                        problems.push(format!("{name} is a circle with no r"));
                    }
                }
                ElementKind::Line | ElementKind::Arrow => {
                    if element.points.len() < 2 {
                        problems.push(format!("{name} needs two points or more"));
                    }
                    if element.points.iter().flatten().any(|n| !n.is_finite()) {
                        problems.push(format!("{name} has a point that is not a number"));
                    }
                }
                ElementKind::Path => {
                    if kurbo::BezPath::from_svg(element.d.trim()).is_err()
                        || element.d.trim().is_empty()
                    {
                        problems.push(format!("{name}: d is not SVG path data"));
                    }
                }
                ElementKind::Icon => {
                    if icon(&element.icon).is_none() {
                        problems.push(format!(
                            "{name}: there is no icon \"{}\"; use one of the list",
                            element.icon
                        ));
                    }
                }
                ElementKind::Group => {
                    if element.children.is_empty() {
                        problems.push(format!("{name} is a group with nothing in it"));
                    }
                }
            }
        }
        // Words that begin off the canvas are not seen; a little over the
        // edge is a design's own business.
        for element in &self.elements {
            if element.kind == ElementKind::Text {
                let (w, h) = canvas;
                if element.x < -10.0 || element.x > w + 10.0 || element.y < 0.0 || element.y > h {
                    problems.push(format!("\"{}\" is placed off the canvas", element.id));
                }
            }
        }
        for (n, step) in self.steps.iter().enumerate() {
            let which = format!("step {}", n + 1);
            if step.target == "camera" {
                if step.effect != Effect::Zoom {
                    problems.push(format!("{which}: the camera only zooms"));
                }
            } else if !seen.contains(step.target.as_str()) {
                problems.push(format!(
                    "{which} acts on \"{}\", which is no element's id",
                    step.target
                ));
            }
            if !step.at.is_finite() || step.at < 0.0 {
                problems.push(format!("{which}: at must be 0 or more"));
            }
            match step.effect {
                Effect::Move if step.to.len() < 2 => {
                    problems.push(format!("{which}: move needs to = [x, y]"))
                }
                Effect::Zoom if step.to.len() < 3 => {
                    problems.push(format!("{which}: zoom needs to = [x, y, zoom]"))
                }
                Effect::Zoom if step.to.get(2).is_some_and(|z| !(0.3..=6.0).contains(z)) => {
                    problems.push(format!("{which}: zoom between 0.3 and 6"))
                }
                Effect::Color if parse_colour(&step.color).is_none() => {
                    problems.push(format!("{which}: color needs color = #rrggbb"))
                }
                Effect::Scale if !(step.value > 0.0 && step.value.is_finite()) => {
                    problems.push(format!("{which}: scale needs a value above 0"))
                }
                _ => {}
            }
        }
        if !self.background.is_empty()
            && self
                .background
                .iter()
                .any(|colour| parse_colour(colour).is_none())
        {
            problems.push("the background is not one or two #rrggbb colours".to_owned());
        }
        for element in &all {
            if element
                .gradient
                .iter()
                .any(|colour| parse_colour(colour).is_none())
            {
                problems.push(format!(
                    "\"{}\": a gradient colour is not #rrggbb",
                    element.id
                ));
            }
        }
        let glowing = all.iter().filter(|e| e.glow > 0.0).count();
        if glowing > 8 {
            problems.push(format!(
                "{glowing} elements glow: keep it to the few that matter, 8 at most"
            ));
        }
        if self.lights.len() > 5 {
            problems.push("more than 5 lights: 2 or 3 are enough".to_owned());
        }
        if self.lights.iter().any(|l| parse_colour(&l.color).is_none()) {
            problems.push("a light's color is not #rrggbb".to_owned());
        }
        if self.particles > 120 {
            problems.push("more than 120 particles".to_owned());
        }
        if !self.particle_color.is_empty() && parse_colour(&self.particle_color).is_none() {
            problems.push("particleColor is not #rrggbb".to_owned());
        }
        // The captions run along the bottom of the frame: words there are
        // covered by them.
        let band = canvas.1 * CAPTIONS_BAND;
        for element in &self.elements {
            let low = match element.kind {
                ElementKind::Text => element.y > band,
                ElementKind::Rect => element.y + element.h > band + canvas.1 * 0.04,
                ElementKind::Circle => element.y + element.r > band + canvas.1 * 0.04,
                ElementKind::Icon => element.y + element.size / 2.0 > band + canvas.1 * 0.04,
                _ => false,
            };
            if low {
                problems.push(format!(
                    "\"{}\" reaches into the lowest fifth (below y {band:.0}), where the \
                     captions go: move it up",
                    element.id
                ));
            }
        }
        problems
    }

    /// Fills in what was left out with what looks right: sizes, weights
    /// and widths at their defaults, colours that read.
    pub fn settle(&mut self) {
        fn walk(elements: &mut [Element]) {
            for element in elements {
                if !element.opacity.is_finite() {
                    element.opacity = 1.0;
                }
                element.opacity = element.opacity.clamp(0.0, 1.0);
                if element.size <= 0.0 {
                    element.size = match element.kind {
                        ElementKind::Icon => 120.0,
                        _ => 48.0,
                    };
                }
                if element.weight <= 0.0 {
                    element.weight = 600.0;
                }
                if element.width <= 0.0 {
                    element.width = match element.kind {
                        ElementKind::Icon => 2.0,
                        _ => 6.0,
                    };
                }
                if element.kind == ElementKind::Text && element.fill.is_empty() {
                    element.fill = "#FFFFFF".to_owned();
                }
                if matches!(
                    element.kind,
                    ElementKind::Line | ElementKind::Arrow | ElementKind::Icon
                ) && element.stroke.is_empty()
                {
                    element.stroke = "#FFFFFF".to_owned();
                }
                walk(&mut element.children);
            }
        }
        walk(&mut self.elements);
        if self.background.is_empty() {
            self.background = vec!["#0F172A".to_owned()];
        }
        if self.particles > 0 && self.particle_color.is_empty() {
            self.particle_color = "#FFFFFF".to_owned();
        }
        for light in &mut self.lights {
            if light.r.is_nan() || light.r <= 0.0 {
                light.r = 500.0;
            }
            light.opacity = light.opacity.clamp(0.0, 1.0);
        }
    }
}

/// An icon's paths by its name.
pub fn icon(name: &str) -> Option<&'static [&'static str]> {
    let name = name.trim().to_lowercase();
    icons::ICONS
        .iter()
        .find(|(icon, _)| *icon == name)
        .map(|(_, paths)| *paths)
}

/// Every icon's name, for the writer.
pub fn icon_names() -> String {
    icons::ICONS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A colour as channels and alpha: `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub fn parse_colour(colour: &str) -> Option<[u8; 4]> {
    let hex = colour.trim().strip_prefix('#')?;
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        3 => {
            let short = |i: usize| u8::from_str_radix(hex.get(i..=i)?, 16).ok().map(|v| v * 17);
            Some([short(0)?, short(1)?, short(2)?, 255])
        }
        6 => Some([channel(0)?, channel(2)?, channel(4)?, 255]),
        8 => Some([channel(0)?, channel(2)?, channel(4)?, channel(6)?]),
        _ => None,
    }
}

/// What an element looks like at one moment, from the steps so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// How far it has moved from where it was put, across.
    pub dx: f64,
    /// The same, down.
    pub dy: f64,
    /// Its size, 1 as put.
    pub scale: f64,
    /// Its turn, in degrees.
    pub rotate: f64,
    /// How far it has grown from `edge`, 0 to 1.
    pub grow: f64,
    /// The edge it grows from.
    pub edge: Edge,
    /// How opaque, times its own opacity.
    pub opacity: f64,
    /// How much of its outline is drawn, 0 to 1.
    pub drawn: f64,
    /// How much of its words are up, 0 to 1.
    pub typed: f64,
    /// A colour it has been turned to, mixed in by `tint`.
    pub tint: Option<([u8; 4], [u8; 4], f64)>,
    /// The number it shows while it counts.
    pub count: Option<f64>,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotate: 0.0,
            grow: 1.0,
            edge: Edge::Center,
            opacity: 1.0,
            drawn: 1.0,
            typed: 1.0,
            tint: None,
            count: None,
        }
    }
}

/// Where the camera looks, and how near.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The canvas point at the middle of the frame, across.
    pub x: f64,
    /// The same, down.
    pub y: f64,
    /// How near: 1 shows the whole canvas.
    pub zoom: f64,
}

impl Explainer {
    /// How `element` looks at `t`, from the steps (cued already) that act
    /// on it.
    pub fn look(&self, element: &Element, t: f64) -> Look {
        let mut look = Look::default();
        let steps: Vec<&Step> = self
            .steps
            .iter()
            .filter(|step| step.target == element.id)
            .collect();
        if let Some(first) = steps
            .iter()
            .filter(|s| s.effect.enters())
            .map(|s| s.at)
            .reduce(f64::min)
            && t < first
        {
            look.opacity = 0.0;
            return look;
        }
        let mut pop = 1.0;
        let mut pulse = 1.0;
        let mut shake = 0.0;
        for step in steps {
            if t < step.at {
                continue;
            }
            let raw = step.progress(t);
            let p = step.ease.apply(raw);
            match step.effect {
                Effect::FadeIn => look.opacity = p,
                Effect::FadeOut => look.opacity *= 1.0 - p,
                Effect::Pop => {
                    pop = pop_curve(raw);
                    look.opacity = (raw * 3.0).min(1.0);
                }
                Effect::Grow => {
                    look.grow = p.max(0.001);
                    look.edge = step.edge;
                }
                Effect::Draw => look.drawn = p,
                Effect::Type => look.typed = raw,
                Effect::Move => {
                    if let [x, y, ..] = step.to[..] {
                        let (tx, ty) = (x - element.x, y - element.y);
                        look.dx += (tx - look.dx) * p;
                        look.dy += (ty - look.dy) * p;
                    }
                }
                Effect::Scale => look.scale += (step.value - look.scale) * p,
                Effect::Rotate => look.rotate += (step.value - look.rotate) * p,
                Effect::Color => {
                    if let Some(to) = parse_colour(&step.color) {
                        let from = match look.tint {
                            Some((a, b, k)) => mix(a, b, k),
                            None => parse_colour(base_colour(element)).unwrap_or([255; 4]),
                        };
                        look.tint = Some((from, to, p));
                    }
                }
                Effect::Count => look.count = Some(step.from + (step.value - step.from) * p),
                Effect::Pulse => pulse = 1.0 + 0.14 * (std::f64::consts::PI * raw).sin(),
                Effect::Shake => {
                    shake = 14.0 * (raw * std::f64::consts::PI * 8.0).sin() * (1.0 - raw)
                }
                Effect::Float => {
                    let since = t - step.at;
                    let amplitude = if step.value > 0.0 { step.value } else { 12.0 };
                    look.dy += amplitude
                        * (since * std::f64::consts::TAU / 3.2).sin()
                        * (since / 0.6).min(1.0);
                }
                Effect::Spin => {
                    let speed = if step.value != 0.0 { step.value } else { 40.0 };
                    look.rotate += speed * (t - step.at);
                }
                Effect::Zoom => {}
            }
        }
        look.scale *= pop * pulse;
        look.dx += shake;
        look
    }

    /// Where the camera is at `t`: the canvas whole until a zoom moves it.
    /// With no zoom at all it drifts in, slowly, so a still frame is never
    /// quite still.
    pub fn camera(&self, canvas: (f64, f64), t: f64) -> Camera {
        let mut camera = Camera {
            x: canvas.0 / 2.0,
            y: canvas.1 / 2.0,
            zoom: 1.0,
        };
        if !self.steps.iter().any(|s| s.target == "camera") {
            camera.zoom = 1.0 + t.clamp(0.0, 16.0) * 0.0025;
            return camera;
        }
        for step in self.steps.iter().filter(|s| s.target == "camera") {
            if t < step.at {
                continue;
            }
            if let [x, y, zoom, ..] = step.to[..] {
                let p = step.ease.apply(step.progress(t));
                camera.x += (x - camera.x) * p;
                camera.y += (y - camera.y) * p;
                camera.zoom += (zoom.clamp(0.3, 6.0) - camera.zoom) * p;
            }
        }
        camera
    }
}

/// The colour a `color` step starts from: an element's fill, or its
/// stroke where it has no fill.
fn base_colour(element: &Element) -> &str {
    match element.kind {
        ElementKind::Line | ElementKind::Arrow | ElementKind::Icon => &element.stroke,
        _ if element.fill.is_empty() => &element.stroke,
        _ => &element.fill,
    }
}

/// `a` to `b` by `k`.
pub fn mix(a: [u8; 4], b: [u8; 4], k: f64) -> [u8; 4] {
    let k = k.clamp(0.0, 1.0);
    std::array::from_fn(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * k).round() as u8)
}

/// A pop's size over its course: small, a touch past full, then full.
fn pop_curve(raw: f64) -> f64 {
    if raw < 0.6 {
        0.55 + (1.08 - 0.55) * Ease::Out.apply(raw / 0.6)
    } else {
        1.08 - 0.08 * Ease::InOut.apply((raw - 0.6) / 0.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Explainer {
        serde_json::from_value(serde_json::json!({
            "background": ["#0B1220", "#13233F"],
            "elements": [
                {"id": "sun", "kind": "icon", "icon": "sun", "x": 400, "y": 500, "size": 200, "stroke": "#FFD54A"},
                {"id": "ray", "kind": "arrow", "points": [[520, 500], [1300, 500]], "curve": 0.15, "stroke": "#FFD54A"},
                {"id": "earth", "kind": "circle", "x": 1450, "y": 500, "r": 110, "fill": "#2E86DE"},
                {"id": "label", "kind": "text", "text": "8 minutes", "x": 960, "y": 380, "size": 56},
                {"id": "bar", "kind": "rect", "x": 200, "y": 800, "w": 600, "h": 60, "r": 12, "fill": "#22C55E"}
            ],
            "steps": [
                {"target": "sun", "effect": "pop", "at": 0.2},
                {"target": "ray", "effect": "draw", "at": 1.0, "cue": "light"},
                {"target": "earth", "effect": "fade_in", "at": 1.5},
                {"target": "label", "effect": "type", "at": 2.0},
                {"target": "bar", "effect": "grow", "edge": "left", "at": 2.5},
                {"target": "earth", "effect": "move", "to": [1500, 520], "at": 3.0},
                {"target": "camera", "effect": "zoom", "to": [1450, 500, 1.6], "at": 4.0}
            ]
        }))
        .unwrap()
    }

    /// A sound explainer has nothing to say against it; each fault is
    /// named so the writer can mend it.
    #[test]
    fn faults_are_named_for_the_writer() {
        let mut good = sample();
        good.settle();
        assert_eq!(good.problems(canvas("16:9")), Vec::<String>::new());

        let mut bad = good.clone();
        bad.elements[0].icon = "unicorn".into();
        bad.elements[2].id = "sun".into();
        bad.elements[3].size = 12.0;
        bad.steps[1].target = "beam".into();
        bad.steps[5].to = vec![1.0];
        let problems = bad.problems(canvas("16:9")).join("\n");
        for expected in [
            "no icon \"unicorn\"",
            "\"sun\" is used twice",
            "too small to read",
            "\"beam\", which is no element",
            "move needs to",
        ] {
            assert!(problems.contains(expected), "{expected} in {problems}");
        }
    }

    /// A step starts when its cue is said; an element with an entrance is
    /// unseen until it, then comes on; a move gets where it goes.
    #[test]
    fn steps_follow_the_words_and_the_clock() {
        let cued = sample().cued(&|cue| (cue == "light").then_some(2.4));
        let ray = cued.all()[1].clone();
        assert_eq!(cued.look(&ray, 2.3).opacity, 0.0);
        assert!(cued.look(&ray, 2.9).drawn > 0.3);
        assert_eq!(cued.look(&ray, 9.0).drawn, 1.0);
        let earth = cued.all()[2].clone();
        assert_eq!(cued.look(&earth, 1.0).opacity, 0.0);
        let moved = cued.look(&earth, 10.0);
        assert!((moved.dx - 50.0).abs() < 1e-9 && (moved.dy - 20.0).abs() < 1e-9);
        let bar = cued.all()[4].clone();
        let growing = cued.look(&bar, 2.9);
        assert!(growing.grow > 0.0 && growing.grow < 1.0 && growing.edge == Edge::Left);
        let camera = cued.camera(canvas("16:9"), 10.0);
        assert!((camera.zoom - 1.6).abs() < 1e-9 && (camera.x - 1450.0).abs() < 1e-9);
        assert!(cued.ends() > 5.0);
    }

    /// The easings start at 0 and end at 1; a pop overshoots and settles.
    #[test]
    fn easings_start_and_end_where_they_should() {
        for ease in [
            Ease::Out,
            Ease::In,
            Ease::InOut,
            Ease::Linear,
            Ease::Back,
            Ease::Spring,
        ] {
            assert!(ease.apply(0.0).abs() < 1e-9, "{ease:?}");
            assert!((ease.apply(1.0) - 1.0).abs() < 1e-9, "{ease:?}");
        }
        assert!(Ease::Back.apply(0.8) > 1.0);
        assert!(pop_curve(0.6) > 1.05 && (pop_curve(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn colours_are_read_in_their_three_spellings() {
        assert_eq!(parse_colour("#fff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_colour("#2E86DE"), Some([0x2E, 0x86, 0xDE, 255]));
        assert_eq!(parse_colour("#00000080"), Some([0, 0, 0, 0x80]));
        assert_eq!(parse_colour("blue"), None);
        assert!(icon("Sun").is_some() && icon("unicorn").is_none());
    }
}
