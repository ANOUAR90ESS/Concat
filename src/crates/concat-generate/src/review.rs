// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The cut looked over before it is exported, the way an editor watches a
//! video through before it goes out: a shot that could not be made and is
//! a card for now, a scene nobody reads, a stretch of silence, a picture
//! shown twice. An unattended run ends in a file; this is what keeps a
//! file with a hole in it from being taken for a finished video.

use std::collections::HashMap;

use crate::plan::{Kind, Placement, SceneFiles};
use crate::storyboard::{ShotKind, Storyboard};

/// One thing the review found.
#[derive(Clone, Debug, PartialEq)]
pub enum Finding {
    /// So many shots could not be made, and cards stand in for them.
    StoodIn(usize),
    /// So many scenes have words and no voice reading them.
    Unread(usize),
    /// So many files the timeline would not take, and the cut is without.
    LeftOut(usize),
    /// So many explainers could not be designed, and are pictures instead.
    NotDrawn(usize),
    /// So many pairs of pictures are near enough alike to read as one
    /// shown twice.
    Repeats(usize),
    /// Nobody speaks for so many seconds, from so many seconds in.
    Silence {
        /// How long.
        seconds: f64,
        /// From where in the video.
        at: f64,
    },
}

impl Finding {
    /// Whether the video should not go out as it is: something is missing
    /// from it, rather than merely less good than it could be.
    pub fn holds(&self) -> bool {
        matches!(
            self,
            Finding::StoodIn(_) | Finding::Unread(_) | Finding::LeftOut(_)
        )
    }
}

/// The longest nobody may speak before it is worth saying.
const QUIET: f64 = 4.0;

/// How many shots of `stand_ins` - a run's notes of what it worked around -
/// are cards standing in for pictures that could not be made.
pub fn stood_in(stand_ins: &[String]) -> usize {
    stand_ins
        .iter()
        .filter(|note| note.contains("in place of a still"))
        .count()
}

/// What an editor would stop at in the cut `placements` of `board`: the
/// files made for it are `files`, what the run worked around `stand_ins`,
/// and `left_out` the files the timeline would not take. `voiced` when the
/// video is narrated.
pub fn review(
    board: &Storyboard,
    files: &HashMap<String, SceneFiles>,
    stand_ins: &[String],
    placements: &[Placement],
    left_out: usize,
    voiced: bool,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let cards = stood_in(stand_ins);
    if cards > 0 {
        findings.push(Finding::StoodIn(cards));
    }
    if voiced {
        let unread = board
            .scenes()
            .filter(|scene| !scene.narration.trim().is_empty())
            .filter(|scene| files.get(&scene.id).is_none_or(|made| made.voice.is_none()))
            .count();
        if unread > 0 {
            findings.push(Finding::Unread(unread));
        }
    }
    if left_out > 0 {
        findings.push(Finding::LeftOut(left_out));
    }
    let not_drawn = board
        .scenes()
        .flat_map(|scene| &scene.shots)
        .filter(|shot| shot.kind == ShotKind::Explainer && shot.explainer.is_none())
        .count();
    if not_drawn > 0 {
        findings.push(Finding::NotDrawn(not_drawn));
    }
    let repeats = board.repeats().len();
    if repeats > 0 {
        findings.push(Finding::Repeats(repeats));
    }
    if voiced && let Some((seconds, at)) = quietest(placements) {
        findings.push(Finding::Silence { seconds, at });
    }
    findings
}

/// The longest stretch of the cut with no voice, and where it starts, when
/// it is long enough to be heard as a hole: before the first word, between
/// two readings, or after the last.
fn quietest(placements: &[Placement]) -> Option<(f64, f64)> {
    let end = crate::plan::total_length(placements);
    let mut voiced: Vec<(f64, f64)> = placements
        .iter()
        .filter(|p| p.kind == Kind::Voice)
        .map(|p| (p.start, p.start + p.length))
        .collect();
    if voiced.is_empty() {
        return None;
    }
    voiced.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut longest: Option<(f64, f64)> = None;
    let mut at = 0.0;
    for (from, to) in voiced.into_iter().chain(std::iter::once((end, end))) {
        let gap = from - at;
        if gap > QUIET && longest.is_none_or(|(seconds, _)| gap > seconds) {
            longest = Some((gap, at));
        }
        at = at.max(to);
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Options, plan};

    fn board() -> Storyboard {
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","chapters":[{"title":"One","scenes":[
                {"narration":"First sentence here.","seconds":5,
                 "shots":[{"prompt":"a red kite over a grey harbour at dawn"}]},
                {"narration":"Second sentence here.","seconds":5,
                 "shots":[{"prompt":"an old clockmaker bent over brass wheels","kind":"explainer"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        board
    }

    fn files(second_voice: bool) -> HashMap<String, SceneFiles> {
        HashMap::from([
            (
                "c01s01".to_owned(),
                SceneFiles {
                    voice: Some(("/v1.mp3".into(), 4.0)),
                    shots: vec![("/a.png".into(), None)],
                    ..SceneFiles::default()
                },
            ),
            (
                "c01s02".to_owned(),
                SceneFiles {
                    voice: second_voice.then(|| ("/v2.mp3".into(), 4.0)),
                    shots: vec![("/b.png".into(), None)],
                    ..SceneFiles::default()
                },
            ),
        ])
    }

    /// A whole cut has nothing to stop at but what is merely less good; a
    /// card for a shot, a scene unread and a file left out each hold it.
    #[test]
    fn a_cut_with_a_hole_in_it_is_held() {
        let board = board();
        let whole = files(true);
        let placements = plan(&board, &whole, &Options::default(), None).unwrap();
        let found = review(&board, &whole, &[], &placements, 0, true);
        // The explainer with no design is a picture: said, not held for.
        assert_eq!(found, [Finding::NotDrawn(1)]);
        assert!(!found.iter().any(Finding::holds));

        let notes = vec![
            "c01s01_1: a colour card in place of a still (NSFW)".to_owned(),
            "c01s02_1: a still in place of a clip (too long)".to_owned(),
        ];
        assert_eq!(stood_in(&notes), 1);
        let holed = files(false);
        let placements = plan(&board, &holed, &Options::default(), None).unwrap();
        let found = review(&board, &holed, &notes, &placements, 2, true);
        assert!(found.contains(&Finding::StoodIn(1)));
        assert!(found.contains(&Finding::Unread(1)));
        assert!(found.contains(&Finding::LeftOut(2)));
        assert!(found.iter().filter(|finding| finding.holds()).count() == 3);
        // The unread scene is a silence too: five seconds at the end.
        assert!(found.iter().any(
            |finding| matches!(finding, Finding::Silence { seconds, at } if *seconds > 4.0 && *at > 3.0)
        ));
        // A video with no narration is not listened to.
        let silent = review(&board, &holed, &[], &placements, 0, false);
        assert_eq!(silent, [Finding::NotDrawn(1)]);
    }
}
