// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Animated cards, laid on the timeline as the edit's own text clips with
//! keyframes: a heading, a figure counting up, bars to their values, dates
//! along a line, points one by one, a quote. Each is words and plates over
//! the backdrop the job drew in the card's colour, so it is drawn by the
//! app, in the narration's language and script, and costs nothing to make.
//!
//! Positions are fractions of the frame from its centre, `y` down; sizes
//! are fractions of its height, widths of its width, as the editor keeps
//! them.

use concat_project::Command;
use concat_project::model::{KeyEase, KeyProperty, TextAlign, TextStyle};

use crate::plan::Editing;
use crate::storyboard::{Graphic, GraphicKind};

/// How long a figure takes to count up to its value.
const COUNT_SECONDS: f64 = 1.1;
/// The steps it counts in.
const COUNT_STEPS: usize = 14;
/// How long after the one before each line, bar or date comes up.
const STAGGER: f64 = 0.35;
/// The fade every element leaves on at the card's end.
const FADE_OUT: f64 = 0.25;
/// What every element stops short of its card's end by.
const EDGE: f64 = 0.002;
/// The most lines a card shows.
const MOST: usize = 5;

/// Lays `graphic` over `start..start + length` of the timeline, on lanes of
/// its own above whatever is there. `frame` is the timeline's size, for
/// the few shapes that must be as wide as they are tall.
pub fn lay(
    editor: &mut dyn Editing,
    lanes: &mut Lanes,
    graphic: &Graphic,
    start: f64,
    length: f64,
    frame: (u32, u32),
) {
    if length <= 0.2 || graphic.is_empty() {
        return;
    }
    let mut card = Card {
        editor,
        lanes,
        start,
        length,
        accent: bright(graphic.rgb()),
        rtl: graphic
            .items
            .iter()
            .map(|item| item.label.as_str())
            .chain([graphic.title.as_str()])
            .any(is_rtl),
        aspect: frame.0.max(1) as f64 / frame.1.max(1) as f64,
    };
    let items: Vec<_> = graphic
        .items
        .iter()
        .filter(|item| !item.label.trim().is_empty() || !item.value.trim().is_empty())
        .take(MOST)
        .collect();
    match graphic.kind {
        GraphicKind::Title => {
            card.text(
                0.0,
                graphic.title.trim(),
                heading(0.10, 900.0),
                (0.0, -0.04),
                Enter::Pop,
            );
            card.rule(0.25, 0.18, (0.0, 0.07));
            if let Some(item) = items.first() {
                card.text(
                    0.45,
                    item.label.trim(),
                    body(0.05, &card.white()),
                    (0.0, 0.16),
                    Enter::Rise,
                );
            }
        }
        GraphicKind::Number => {
            card.title(graphic.title.trim());
            let value = items.first().map_or("", |item| item.value.trim());
            let figure = if value.is_empty() {
                graphic.title.trim()
            } else {
                value
            };
            let style = heading(0.2, 900.0);
            card.count(figure, style, (0.0, -0.01));
            if let Some(item) = items.first() {
                card.text(
                    0.5,
                    item.label.trim(),
                    body(0.055, &card.white()),
                    (0.0, 0.16),
                    Enter::Rise,
                );
            }
        }
        GraphicKind::Compare => {
            card.title(graphic.title.trim());
            let values: Vec<f64> = items
                .iter()
                .map(|item| number_in(&item.value).map_or(0.0, |n| n.value.abs()))
                .collect();
            let most = values.iter().cloned().fold(0.0, f64::max).max(1e-9);
            let rows = items.len().min(4);
            let gap = 0.13;
            let top = 0.04 - gap * (rows as f64 - 1.0) / 2.0;
            for (n, (item, value)) in items.iter().zip(&values).take(rows).enumerate() {
                let y = top + gap * n as f64;
                let delay = 0.2 + STAGGER * n as f64;
                let width = (0.42 * value / most).max(0.02);
                // The labels on the reading side, the bars growing away
                // from them.
                let side = if card.rtl { -1.0 } else { 1.0 };
                let edge = -0.15 * side;
                let label = TextStyle {
                    align: if card.rtl {
                        TextAlign::Left
                    } else {
                        TextAlign::Right
                    },
                    ..body(0.045, &card.white())
                };
                card.text(
                    delay,
                    item.label.trim(),
                    label,
                    (edge - 0.02 * side, y),
                    Enter::Fade,
                );
                let bar = plate(
                    &card.accent,
                    if *value >= most { "ff" } else { "b3" },
                    width,
                    0.055,
                );
                card.text(
                    delay,
                    "\u{a0}",
                    bar,
                    (edge + side * width / 2.0, y),
                    Enter::Grow,
                );
                let figure = TextStyle {
                    align: if card.rtl {
                        TextAlign::Right
                    } else {
                        TextAlign::Left
                    },
                    ..heading(0.045, 800.0)
                };
                card.text(
                    delay + 0.3,
                    item.value.trim(),
                    TextStyle {
                        color: card.white(),
                        ..figure
                    },
                    (edge + side * (width + 0.015), y),
                    Enter::Fade,
                );
            }
        }
        GraphicKind::Timeline => {
            card.title(graphic.title.trim());
            let line = plate("#ffffff", "66", 0.8, 0.006);
            card.text(0.1, "\u{a0}", line, (0.0, 0.02), Enter::Grow);
            let count = items.len().max(1);
            for (n, item) in items.iter().enumerate() {
                let along = if count == 1 {
                    0.0
                } else {
                    -0.32 + 0.64 * n as f64 / (count - 1) as f64
                };
                let x = if card.rtl { -along } else { along };
                let delay = 0.3 + STAGGER * n as f64;
                let dot = 0.032;
                let mut disc = plate(&card.accent, "ff", dot / card.aspect, dot);
                disc.background_radius = dot / 2.0;
                card.text(delay, "\u{a0}", disc, (x, 0.02), Enter::Pop);
                card.text(
                    delay + 0.1,
                    item.value.trim(),
                    TextStyle {
                        color: card.accent.clone(),
                        ..heading(0.06, 900.0)
                    },
                    (x, -0.07),
                    Enter::Rise,
                );
                card.text(
                    delay + 0.2,
                    item.label.trim(),
                    TextStyle {
                        max_width: 0.17,
                        ..body(0.036, &card.white())
                    },
                    (x, 0.12),
                    Enter::Fade,
                );
            }
        }
        GraphicKind::List => {
            card.title(graphic.title.trim());
            let gap = 0.11;
            let top = 0.05 - gap * (items.len() as f64 - 1.0) / 2.0;
            let (align, x) = if card.rtl {
                (TextAlign::Right, 0.32)
            } else {
                (TextAlign::Left, -0.32)
            };
            for (n, item) in items.iter().enumerate() {
                let text = if card.rtl {
                    format!("{}  ●", item.label.trim())
                } else {
                    format!("●  {}", item.label.trim())
                };
                card.text(
                    0.3 + 0.45 * n as f64,
                    &text,
                    TextStyle {
                        align,
                        ..body(0.05, &card.white())
                    },
                    (x, top + gap * n as f64),
                    Enter::Rise,
                );
            }
        }
        GraphicKind::Quote => {
            card.text(
                0.0,
                "\u{201C}",
                TextStyle {
                    color: card.accent.clone(),
                    ..heading(0.22, 900.0)
                },
                (0.0, -0.24),
                Enter::Pop,
            );
            card.text(
                0.25,
                graphic.title.trim(),
                TextStyle {
                    max_width: 0.74,
                    line_height: 1.3,
                    ..body(0.058, &card.white())
                },
                (0.0, -0.01),
                Enter::Rise,
            );
            if let Some(item) = items.first() {
                card.text(
                    0.9,
                    &format!("— {}", item.label.trim()),
                    TextStyle {
                        color: card.accent.clone(),
                        ..body(0.042, "")
                    },
                    (0.0, 0.2),
                    Enter::Fade,
                );
            }
        }
    }
}

/// The lanes the cards are laid on, made as they are needed, and what each
/// already holds: every element of a card is on screen with the others, so
/// each needs a lane of its own while it is.
#[derive(Default)]
pub struct Lanes(Vec<(String, Vec<(f64, f64)>)>);

impl Lanes {
    /// A lane free over `start..end`, made when none is, and marked taken.
    fn free(&mut self, editor: &mut dyn Editing, start: f64, end: f64) -> Option<String> {
        let open = self.0.iter().position(|(_, taken)| {
            taken
                .iter()
                .all(|(from, to)| end <= *from + 1e-6 || start >= *to - 1e-6)
        });
        let n = match open {
            Some(n) => n,
            None => {
                let id = editor.apply(Command::AddTrack { bottom: false })?;
                self.0.push((id, Vec::new()));
                self.0.len() - 1
            }
        };
        self.0[n].1.push((start, end));
        Some(self.0[n].0.clone())
    }
}

/// How an element comes on.
#[derive(Clone, Copy)]
enum Enter {
    /// Fades in.
    Fade,
    /// Fades in, settling into its size.
    Rise,
    /// Grows past its size and settles.
    Pop,
    /// Grows from a sliver: a bar, a line.
    Grow,
}

/// One card being laid.
struct Card<'a> {
    editor: &'a mut dyn Editing,
    lanes: &'a mut Lanes,
    start: f64,
    length: f64,
    accent: String,
    rtl: bool,
    aspect: f64,
}

impl Card<'_> {
    /// Words sized for the frame: sizes are fractions of its height, so in
    /// a frame narrower than wide they are made smaller to keep to its
    /// width. A plate keeps the size it was given.
    fn fitted(&self, style: TextStyle) -> TextStyle {
        if !style.background.is_empty() {
            return style;
        }
        let k = (self.aspect / (16.0 / 9.0)).clamp(0.5, 1.0).sqrt();
        TextStyle {
            font_size: style.font_size * k,
            ..style
        }
    }

    fn white(&self) -> String {
        "#FFFFFF".to_owned()
    }

    /// The card's heading, near the top, with a short rule under it.
    fn title(&mut self, title: &str) {
        if title.is_empty() {
            return;
        }
        self.text(0.0, title, heading(0.065, 800.0), (0.0, -0.31), Enter::Rise);
        self.rule(0.2, 0.1, (0.0, -0.24));
    }

    /// A short rule in the accent.
    fn rule(&mut self, delay: f64, width: f64, at: (f64, f64)) {
        let rule = plate(&self.accent, "ff", width, 0.008);
        self.text(delay, "\u{a0}", rule, at, Enter::Grow);
    }

    /// Words `delay` seconds into the card, held to its end.
    fn text(&mut self, delay: f64, content: &str, style: TextStyle, at: (f64, f64), enter: Enter) {
        if content.is_empty() || delay >= self.length - 0.3 {
            return;
        }
        let style = self.fitted(style);
        // A hair short of the card's end, so the next card can take the
        // lane at its start without the two touching.
        let length = self.length - delay - EDGE;
        let begin = self.start + delay;
        let lane = self.lanes.free(self.editor, begin, begin + length);
        let Some(clip) = self.editor.apply(Command::AddTextClip {
            track_id: lane,
            above: true,
            start: begin,
            style: Some(TextStyle {
                content: content.to_owned(),
                ..style
            }),
            duration: Some(length),
            offset_y: Some(at.1),
        }) else {
            return;
        };
        if at.0.abs() > 1e-6 {
            self.editor.apply(Command::SetClipTransform {
                clip_id: clip.clone(),
                scale: None,
                offset_x: Some(at.0),
                offset_y: None,
                rotation: None,
                stretch_x: None,
                stretch_y: None,
            });
        }
        let t = |seconds: f64| (seconds / length).clamp(0.0, 1.0);
        let out = t(length - FADE_OUT);
        let mut keys: Vec<(KeyProperty, f64, f64, KeyEase)> = vec![
            (KeyProperty::Opacity, 0.0, 0.0, KeyEase::LINEAR),
            (KeyProperty::Opacity, t(0.3), 1.0, KeyEase::OUT),
            (KeyProperty::Opacity, out, 1.0, KeyEase::LINEAR),
            (KeyProperty::Opacity, 1.0, 0.0, KeyEase::IN),
        ];
        match enter {
            Enter::Fade => {}
            // Grown a touch into place rather than moved: a text clip's
            // keyed position is drawn on top of its own.
            Enter::Rise => keys.extend([
                (KeyProperty::Scale, 0.0, 0.92, KeyEase::LINEAR),
                (KeyProperty::Scale, t(0.45), 1.0, KeyEase::OUT),
            ]),
            Enter::Pop => keys.extend([
                (KeyProperty::Scale, 0.0, 0.6, KeyEase::LINEAR),
                (KeyProperty::Scale, t(0.18), 1.08, KeyEase::OUT),
                (KeyProperty::Scale, t(0.32), 1.0, KeyEase::IN_OUT),
            ]),
            Enter::Grow => keys.extend([
                (KeyProperty::Scale, 0.0, 0.05, KeyEase::LINEAR),
                (KeyProperty::Scale, t(0.5), 1.0, KeyEase::OUT),
            ]),
        }
        for (property, at, value, ease) in keys {
            self.editor.apply(Command::SetClipKey {
                clip_id: clip.clone(),
                property,
                at,
                value,
                ease,
            });
        }
    }

    /// A figure counting up to `figure` in quick steps, then held. Words
    /// that hold no number pop in as they are.
    fn count(&mut self, figure: &str, style: TextStyle, at: (f64, f64)) {
        let style = TextStyle {
            color: self.accent.clone(),
            ..style
        };
        let Some(number) = number_in(figure) else {
            self.text(0.1, figure, style, at, Enter::Pop);
            return;
        };
        // Words that are no figure are fitted by `text`; these here.
        let style = self.fitted(style);
        let counting = COUNT_SECONDS.min(self.length / 3.0);
        let step = counting / COUNT_STEPS as f64;
        for n in 1..COUNT_STEPS {
            let share = n as f64 / COUNT_STEPS as f64;
            // Fast, then slowing into the figure.
            let eased = 1.0 - (1.0 - share).powi(3);
            let shown = number.with(number.value * eased);
            let begin = self.start + step * (n - 1) as f64;
            let lane = self.lanes.free(self.editor, begin, begin + step);
            self.editor.apply(Command::AddTextClip {
                track_id: lane,
                above: true,
                start: begin,
                style: Some(TextStyle {
                    content: shown,
                    ..style.clone()
                }),
                duration: Some(step),
                offset_y: Some(at.1),
            });
        }
        let delay = step * (COUNT_STEPS - 1) as f64;
        let held = self.length - delay - EDGE;
        let begin = self.start + delay;
        let lane = self.lanes.free(self.editor, begin, begin + held);
        let Some(clip) = self.editor.apply(Command::AddTextClip {
            track_id: lane,
            above: true,
            start: begin,
            style: Some(TextStyle {
                content: figure.to_owned(),
                ..style
            }),
            duration: Some(held),
            offset_y: Some(at.1),
        }) else {
            return;
        };
        let t = |seconds: f64| (seconds / held).clamp(0.0, 1.0);
        for (property, at, value, ease) in [
            (KeyProperty::Scale, 0.0, 1.0, KeyEase::LINEAR),
            (KeyProperty::Scale, t(0.12), 1.06, KeyEase::OUT),
            (KeyProperty::Scale, t(0.3), 1.0, KeyEase::IN_OUT),
            (
                KeyProperty::Opacity,
                t(held - FADE_OUT),
                1.0,
                KeyEase::LINEAR,
            ),
            (KeyProperty::Opacity, 1.0, 0.0, KeyEase::IN),
        ] {
            self.editor.apply(Command::SetClipKey {
                clip_id: clip.clone(),
                property,
                at,
                value,
                ease,
            });
        }
    }
}

/// Large, heavy words in white with a soft shadow.
fn heading(size: f64, weight: f64) -> TextStyle {
    TextStyle {
        font_size: size,
        font_weight: weight,
        color: "#FFFFFF".to_owned(),
        shadow: true,
        background: String::new(),
        max_width: 0.86,
        ..TextStyle::default()
    }
}

/// Plain words; `color` empty is white.
fn body(size: f64, color: &str) -> TextStyle {
    TextStyle {
        font_size: size,
        font_weight: 600.0,
        color: if color.is_empty() { "#FFFFFF" } else { color }.to_owned(),
        shadow: true,
        background: String::new(),
        max_width: 0.8,
        ..TextStyle::default()
    }
}

/// A plain box of colour `hex` at `alpha` (two hex digits), `width` of the
/// frame's width and `height` of its height: a bar, a rule, a dot.
fn plate(hex: &str, alpha: &str, width: f64, height: f64) -> TextStyle {
    TextStyle {
        font_size: 0.02,
        shadow: false,
        background: format!("{}{alpha}", &hex[..7.min(hex.len())]),
        background_radius: height / 3.0,
        background_padding_x: 0.0,
        background_padding_y: 0.0,
        max_width: width,
        max_height: height,
        ..TextStyle::default()
    }
}

/// The accent made light enough to read on its own deep shade.
fn bright(rgb: [u8; 3]) -> String {
    let [r, g, b] = rgb.map(f64::from);
    let light = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
    let lift = if light < 0.55 {
        (0.55 - light) / (1.0 - light).max(0.01)
    } else {
        0.0
    };
    let up = |c: f64| (c + (255.0 - c) * lift).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", up(r), up(g), up(b))
}

/// Whether `text` is written right to left: Arabic, Hebrew, Persian, Urdu.
fn is_rtl(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FEFC}'))
}

/// A figure found in words: what comes before it, its value, how it is
/// written - decimals and thousands marks - and what comes after.
#[derive(Clone, Debug, PartialEq)]
struct Figure {
    before: String,
    value: f64,
    decimals: usize,
    grouped: bool,
    after: String,
}

impl Figure {
    /// `value` written the way the figure is.
    fn with(&self, value: f64) -> String {
        let text = format!("{value:.*}", self.decimals);
        let text = if self.grouped {
            let (whole, rest) = text.split_once('.').unwrap_or((&text, ""));
            let digits: Vec<char> = whole.chars().collect();
            let mut grouped = String::new();
            for (i, c) in digits.iter().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    grouped.push(',');
                }
                grouped.push(*c);
            }
            if rest.is_empty() {
                grouped
            } else {
                format!("{grouped}.{rest}")
            }
        } else {
            text
        };
        format!("{}{text}{}", self.before, self.after)
    }
}

/// `text` with its number written as `value`, the way the number is
/// written; `text` as it is where it holds none.
pub(crate) fn count_in(text: &str, value: f64) -> String {
    number_in(text).map_or_else(|| text.to_owned(), |figure| figure.with(value))
}

/// The first number in `text`, in Western or Arabic-Indic digits:
/// "$1.2" , "2,300,000", "38%", "٤٥٠٠".
fn number_in(text: &str) -> Option<Figure> {
    let western: String = text
        .chars()
        .map(|c| match c {
            '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
            '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
            '\u{066B}' => '.',
            '\u{066C}' => ',',
            c => c,
        })
        .collect();
    let start = western.find(|c: char| c.is_ascii_digit())?;
    let rest = &western[start..];
    let end = rest
        .char_indices()
        .find(|(i, c)| {
            !(c.is_ascii_digit()
                || ((*c == ',' || *c == '.')
                    && rest[i + 1..].starts_with(|n: char| n.is_ascii_digit())))
        })
        .map_or(rest.len(), |(i, _)| i);
    let written = &rest[..end];
    let grouped = written.contains(',');
    let plain: String = written.chars().filter(|c| *c != ',').collect();
    let value: f64 = plain.parse().ok()?;
    let decimals = plain.split_once('.').map_or(0, |(_, d)| d.len());
    // Only a number written in Western digits is counted in them; one in
    // Arabic-Indic digits is shown as written.
    if western != text {
        return None;
    }
    Some(Figure {
        before: western[..start].to_owned(),
        value,
        decimals,
        grouped,
        after: rest[end..].to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A figure is found in its words and written back the same way at any
    /// value along its count.
    #[test]
    fn a_figure_counts_up_written_as_it_is() {
        let figure = number_in("2,300,000 stones").unwrap();
        assert_eq!(figure.value, 2_300_000.0);
        assert_eq!(figure.with(1_234_567.0), "1,234,567 stones");
        let money = number_in("$1.25bn").unwrap();
        assert_eq!(money.with(0.5), "$0.50bn");
        assert_eq!(number_in("38%").unwrap().with(12.0), "12%");
        assert_eq!(number_in("in 1969.").unwrap().with(1000.0), "in 1000.");
        assert!(number_in("no figure").is_none());
        // Arabic-Indic digits are shown as written, not counted.
        assert!(number_in("٤٥٠٠ سنة").is_none());
    }

    /// The accent is lifted to read on its own shade; Arabic is laid out
    /// from the right.
    #[test]
    fn a_card_reads_on_its_backdrop_and_in_its_direction() {
        assert_eq!(bright([255, 255, 255]), "#FFFFFF");
        let lifted = bright([20, 30, 90]);
        assert_ne!(lifted, "#141E5A");
        assert!(is_rtl("أهرامات الجيزة"));
        assert!(!is_rtl("Pyramids"));
    }

    struct Laid(Vec<Command>);

    impl Editing for Laid {
        fn apply(&mut self, command: Command) -> Option<String> {
            self.0.push(command);
            Some(format!("c{}", self.0.len()))
        }
        fn lanes(&self) -> Vec<String> {
            Vec::new()
        }
        fn media_of(&self, _: &str) -> Option<String> {
            None
        }
    }

    /// Every card lays its words inside its span, and a comparison's
    /// largest bar is the widest.
    #[test]
    fn every_card_lays_its_words_inside_its_span() {
        use crate::storyboard::GraphicItem;
        let item = |label: &str, value: &str| GraphicItem {
            label: label.into(),
            value: value.into(),
        };
        for kind in [
            GraphicKind::Title,
            GraphicKind::Number,
            GraphicKind::Compare,
            GraphicKind::Timeline,
            GraphicKind::List,
            GraphicKind::Quote,
        ] {
            let graphic = Graphic {
                kind,
                title: "Heading".into(),
                items: vec![item("Egypt", "4,500"), item("Rome", "2,000")],
                accent: "#C08A2E".into(),
            };
            let mut laid = Laid(Vec::new());
            lay(
                &mut laid,
                &mut Lanes::default(),
                &graphic,
                10.0,
                6.0,
                (1920, 1080),
            );
            let texts: Vec<(f64, f64, TextStyle)> = laid
                .0
                .iter()
                .filter_map(|command| match command {
                    Command::AddTextClip {
                        start,
                        duration,
                        style,
                        ..
                    } => Some((*start, duration.unwrap(), style.clone().unwrap())),
                    _ => None,
                })
                .collect();
            assert!(texts.len() >= 2, "{kind:?} laid {}", texts.len());
            for (start, duration, _) in &texts {
                assert!(
                    *start >= 10.0 && start + duration <= 16.0 + 1e-9,
                    "{kind:?}"
                );
            }
            if kind == GraphicKind::Compare {
                let bars: Vec<f64> = texts
                    .iter()
                    .filter(|(_, _, style)| !style.background.is_empty() && style.max_height > 0.05)
                    .map(|(_, _, style)| style.max_width)
                    .collect();
                assert_eq!(bars.len(), 2);
                assert!(bars[0] > bars[1]);
            }
        }
    }
}
