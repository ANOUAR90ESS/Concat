// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! What to look at before a video goes up: the things a platform or a
//! viewer holds against a channel, not holes in the cut (that is
//! `review.rs`). Each is a warning, never a block: the person knows things
//! the app does not - that the music is theirs, that a figure is common
//! knowledge - and decides.

use crate::storyboard::Storyboard;

/// The longest an upright video may run and still be taken as a Short on
/// YouTube or a Reel on Instagram: three minutes.
pub const UPRIGHT_LIMIT: f64 = 180.0;

/// Where the music under the video comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Music {
    /// None.
    None,
    /// Made for the video by a model (MusicGen).
    Made,
    /// A recording from the bin, by its file name.
    Own(String),
}

/// One video of the project as it will be exported.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// Its timeline's name.
    pub name: String,
    /// How long it runs.
    pub seconds: f64,
    /// Taller than wide: a Short, a Reel, a TikTok.
    pub upright: bool,
}

/// What the check reads.
pub struct Before<'a> {
    /// The long video's script.
    pub board: &'a Storyboard,
    /// The research notes kept with the video, if any.
    pub notes: Option<&'a str>,
    /// The titles the channel made before, as remembered.
    pub made_before: &'a [String],
    /// The music under the video.
    pub music: Music,
    /// Pictures or clips made by a model that could pass for real ones:
    /// photographs or footage, not illustration or cards.
    pub realistic: bool,
    /// The videos that will be exported.
    pub cuts: &'a [Cut],
}

/// One thing to look at before publishing.
#[derive(Clone, Debug, PartialEq)]
pub enum Warning {
    /// So many scenes state figures, and the video lists no source.
    Unsourced(usize),
    /// The research found so many disputed points: the script should say
    /// they are disputed.
    Disputed(usize),
    /// The title is close to one the channel already made.
    MadeBefore(String),
    /// The music is a recording from the bin: whether it may be used.
    OwnMusic(String),
    /// The music was made by MusicGen, whose licence is non-commercial.
    MadeMusic,
    /// An upright video runs past what Shorts and Reels take.
    TooLong {
        /// Its name.
        name: String,
        /// How long it runs.
        seconds: f64,
    },
    /// Realistic AI pictures or clips: the platforms ask for a label.
    AiLabel,
}

/// Everything worth a look before `before`'s videos go up, most serious
/// first.
pub fn check(before: &Before) -> Vec<Warning> {
    let mut warnings = Vec::new();
    if before.board.sources.is_empty() {
        let stated = before
            .board
            .scenes()
            .filter(|scene| states_figures(&scene.narration))
            .count();
        if stated > 0 {
            warnings.push(Warning::Unsourced(stated));
        }
    }
    let disputed = before.notes.map(disputed).unwrap_or(0);
    if disputed > 0 {
        warnings.push(Warning::Disputed(disputed));
    }
    if let Some(title) = closest(&before.board.title, before.made_before) {
        warnings.push(Warning::MadeBefore(title));
    }
    match &before.music {
        Music::None => {}
        Music::Made => warnings.push(Warning::MadeMusic),
        Music::Own(name) => warnings.push(Warning::OwnMusic(name.clone())),
    }
    for cut in before.cuts {
        if cut.upright && cut.seconds > UPRIGHT_LIMIT {
            warnings.push(Warning::TooLong {
                name: cut.name.clone(),
                seconds: cut.seconds,
            });
        }
    }
    if before.realistic {
        warnings.push(Warning::AiLabel);
    }
    warnings
}

/// Whether `words` state a figure: a number in any of the digits a
/// script is written in - a count, a year, a share, a price.
fn states_figures(words: &str) -> bool {
    words
        .chars()
        .any(|c| c.is_ascii_digit() || ('٠'..='٩').contains(&c) || ('۰'..='۹').contains(&c))
}

/// How many points the research notes list under DISPUTED: each line of
/// that section with words on it, up to the next section's heading. "None"
/// alone counts as none.
fn disputed(notes: &str) -> usize {
    let mut inside = false;
    let mut count = 0;
    for line in notes.lines() {
        let line = line.trim();
        let heading = line.trim_end_matches(':').trim_start_matches('#').trim();
        if line.ends_with(':') && heading.chars().all(|c| !c.is_lowercase()) && !heading.is_empty()
        {
            inside = heading.eq_ignore_ascii_case("DISPUTED");
            continue;
        }
        if let Some(rest) = line.strip_prefix("DISPUTED:") {
            inside = true;
            if !is_none(rest) {
                count += 1;
            }
            continue;
        }
        if inside && !line.is_empty() && !is_none(line) {
            count += 1;
        }
    }
    count
}

/// Whether a line of notes says there is nothing: "none", "- None." and
/// the like.
fn is_none(line: &str) -> bool {
    let words = line
        .trim()
        .trim_start_matches(['-', '*', '•'])
        .trim()
        .trim_end_matches('.')
        .to_lowercase();
    words.is_empty() || words == "none" || words == "n/a"
}

/// The title among `made` that `title` is closest to, when it is near
/// enough to read as the same video: most of the words of the shorter
/// shared. The title itself, remembered once it was written, is passed
/// over.
fn closest(title: &str, made: &[String]) -> Option<String> {
    let mine = words(title);
    if mine.is_empty() {
        return None;
    }
    made.iter()
        .filter(|other| words(other) != mine)
        .map(|other| (overlap(&mine, &words(other)), other))
        .filter(|(share, _)| *share >= 0.75)
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, other)| other.clone())
}

/// The words of a title that carry it: lower case, without punctuation,
/// three letters or more.
fn words(title: &str) -> Vec<String> {
    let mut words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    words.sort();
    words.dedup();
    words
}

/// The share of the shorter list's words the two have in common.
fn overlap(a: &[String], b: &[String]) -> f64 {
    let shorter = a.len().min(b.len());
    if shorter < 2 {
        return 0.0;
    }
    let shared = a.iter().filter(|word| b.contains(word)).count();
    shared as f64 / shorter as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storyboard::Source;

    fn board(title: &str, narration: &[&str]) -> Storyboard {
        let mut board: Storyboard = serde_json::from_value(serde_json::json!({
            "title": title,
            "chapters": [{
                "title": "One",
                "scenes": narration
                    .iter()
                    .enumerate()
                    .map(|(n, words)| serde_json::json!({"id": format!("s{n}"), "narration": words}))
                    .collect::<Vec<_>>(),
            }],
        }))
        .unwrap();
        board.sources.clear();
        board
    }

    fn before<'a>(board: &'a Storyboard, cuts: &'a [Cut]) -> Before<'a> {
        Before {
            board,
            notes: None,
            made_before: &[],
            music: Music::None,
            realistic: false,
            cuts,
        }
    }

    /// Figures in a video with no sources are flagged, in Arabic digits as
    /// in Western ones; with a source listed they are not.
    #[test]
    fn figures_without_a_source_are_flagged() {
        let mut video = board(
            "Rome",
            &["Rome had a million people in 100 AD.", "قُتل ٢٣ شخصًا", "It fell."],
        );
        assert_eq!(check(&before(&video, &[])), vec![Warning::Unsourced(2)]);
        video.sources.push(Source {
            title: "Britannica".into(),
            url: "https://britannica.com/rome".into(),
        });
        assert!(check(&before(&video, &[])).is_empty());
    }

    /// The disputed points are counted from their section alone; "None"
    /// is none.
    #[test]
    fn disputed_points_are_counted() {
        let notes = "FACTS:\n- Rome was founded [1]\n\nOPINIONS:\n- A says B [2]\n\n\
                     DISPUTED:\n- The founding date [1][3]\n- Remus's fate\n";
        assert_eq!(disputed(notes), 2);
        assert_eq!(disputed("FACTS:\n- one\nDISPUTED:\n- None.\n"), 0);
        assert_eq!(disputed("DISPUTED: the date\nFACTS:\n- one\n"), 1);
        assert_eq!(disputed("Just notes, no sections."), 0);
    }

    /// A title close to one already made is named; the video's own title,
    /// remembered once written, and unrelated titles are not.
    #[test]
    fn a_title_made_before_is_named() {
        let made = vec![
            "Why Rome Fell".to_owned(),
            "How the Roman Empire Fell Apart".to_owned(),
            "The secret life of octopuses".to_owned(),
        ];
        let video = board("Why Rome fell", &[]);
        assert_eq!(closest(&video.title, &made), None);
        let video = board("How the Roman Empire fell", &[]);
        assert_eq!(
            closest(&video.title, &made),
            Some("How the Roman Empire Fell Apart".to_owned())
        );
        assert_eq!(closest("Octopus intelligence", &made), None);
    }

    /// Music, an upright video past three minutes and realistic AI
    /// pictures are each said; a short upright video and a long wide one
    /// are not.
    #[test]
    fn music_length_and_ai_are_said() {
        let video = board("Rome", &["It fell."]);
        let cuts = vec![
            Cut {
                name: "Rome".into(),
                seconds: 600.0,
                upright: false,
            },
            Cut {
                name: "Rome - Short".into(),
                seconds: 58.0,
                upright: true,
            },
            Cut {
                name: "Rome - Short 2".into(),
                seconds: 200.0,
                upright: true,
            },
        ];
        let mut asked = before(&video, &cuts);
        asked.music = Music::Own("theme.mp3".into());
        asked.realistic = true;
        assert_eq!(
            check(&asked),
            vec![
                Warning::OwnMusic("theme.mp3".into()),
                Warning::TooLong {
                    name: "Rome - Short 2".into(),
                    seconds: 200.0
                },
                Warning::AiLabel,
            ]
        );
        asked.music = Music::Made;
        asked.realistic = false;
        assert_eq!(check(&asked)[0], Warning::MadeMusic);
    }
}
