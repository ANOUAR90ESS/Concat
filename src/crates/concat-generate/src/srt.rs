// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The video's words as a captions file, SubRip (`.srt`), to upload beside
//! the video rather than burn into it: Facebook and YouTube show a file's
//! captions in the viewer's own player, at the size and language the viewer
//! picked, and read them for search; burnt-in words are only pixels to
//! them. The cards are the ones the cut would show - the same words, cut
//! the same way, at the same times - whether or not they are on the
//! picture.

use crate::plan::{Kind, Placement};

/// The caption cards among `placements`, in order of time, as SubRip: a
/// numbered cue each, `HH:MM:SS,mmm --> HH:MM:SS,mmm`, its words, and a
/// blank line. Nothing when the cut has no captions.
pub fn from_placements(placements: &[Placement]) -> String {
    let mut cues: Vec<(f64, f64, String)> = placements
        .iter()
        .filter(|placement| placement.kind == Kind::Caption)
        .filter_map(|placement| {
            let text = placement.text.as_deref()?.trim();
            (!text.is_empty() && placement.length > 0.0).then(|| {
                (
                    placement.start.max(0.0),
                    placement.start.max(0.0) + placement.length,
                    text.to_owned(),
                )
            })
        })
        .collect();
    cues.sort_by(|a, b| a.0.total_cmp(&b.0));
    // A cue ends where the next begins at the latest: players stack
    // overlapping cues, and a caption over the next reads as two lines.
    for index in 1..cues.len() {
        let next = cues[index].0;
        let previous = &mut cues[index - 1];
        if previous.1 > next {
            previous.1 = next.max(previous.0 + 0.001);
        }
    }
    let mut out = String::new();
    for (number, (start, end, text)) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            number + 1,
            stamp(*start),
            stamp(*end),
            // A blank line ends a cue: one inside the words would cut it.
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    out
}

/// Seconds as SubRip writes them: `HH:MM:SS,mmm`.
fn stamp(seconds: f64) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caption(start: f64, length: f64, text: &str) -> Placement {
        Placement {
            kind: Kind::Caption,
            start,
            length,
            file: None,
            source: None,
            from: 0.0,
            text: Some(text.to_owned()),
            graphic: None,
            low: false,
            emphasis: String::new(),
        }
    }

    /// The captions, in order, numbered from one, timed to the millisecond;
    /// everything that is not a caption is left out.
    #[test]
    fn captions_become_numbered_cues() {
        let mut title = caption(0.0, 3.0, "The title");
        title.kind = Kind::Title;
        let placements = vec![
            caption(3725.5, 2.25, "and after an hour"),
            title,
            caption(1.0, 2.0, "Rome was not\n\nbuilt in a day"),
        ];
        assert_eq!(
            from_placements(&placements),
            "1\n00:00:01,000 --> 00:00:03,000\nRome was not\nbuilt in a day\n\n\
             2\n01:02:05,500 --> 01:02:07,750\nand after an hour\n\n"
        );
    }

    /// A cue that runs into the next ends where the next begins; an empty
    /// caption is no cue; no captions, no file.
    #[test]
    fn cues_do_not_overlap() {
        let placements = vec![
            caption(0.0, 3.0, "one"),
            caption(2.0, 2.0, "two"),
            caption(5.0, 1.0, "  "),
        ];
        assert_eq!(
            from_placements(&placements),
            "1\n00:00:00,000 --> 00:00:02,000\none\n\n\
             2\n00:00:02,000 --> 00:00:04,000\ntwo\n\n"
        );
        assert_eq!(from_placements(&[]), "");
    }
}
