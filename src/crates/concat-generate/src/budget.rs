// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The budget: how much a video of a given length needs, worked out before
//! a word is written.
//!
//! A length in minutes becomes chapters, the chapters scenes, the scenes
//! shots, and the shots clips and stills, by fixed paces: a chapter runs
//! about two and a half minutes, a scene about twelve seconds, a clip as
//! long as a video model makes one and a still as long as it holds the eye.
//! Every scene is one line of narration, so the voice lines are the scenes.
//!
//! The writer is then told the exact shape - "chapter 2: 13 scenes; scene
//! 4: 11 s, 26 words, a clip then two stills" - and the storyboard it gives
//! back is held to it, so the counts the sheet shows before writing are the
//! counts that get made, and paid for.

use crate::storyboard::ShotKind;

/// How a video is paced.
#[derive(Clone, Debug, PartialEq)]
pub struct Pacing {
    /// The length asked for.
    pub minutes: f64,
    /// The share of shots that are clips, `0..=1`; the rest are stills.
    pub clip_share: f64,
    /// How long a clip runs: what a video model makes in one go.
    pub clip_seconds: f64,
    /// How long a still holds.
    pub still_seconds: f64,
    /// About how long a scene runs: one breath of narration.
    pub scene_seconds: f64,
    /// About how long a chapter runs.
    pub chapter_seconds: f64,
    /// Narration speed, in words a second.
    pub words_per_second: f64,
    /// How many chapters, when that is settled rather than worked out from
    /// the length: the outline the writer gave back.
    pub chapters: Option<usize>,
}

impl Pacing {
    /// The paces a long video is made at, for `minutes` with `clip_share`
    /// of its shots clips.
    pub fn new(minutes: f64, clip_share: f64) -> Pacing {
        Pacing {
            minutes,
            clip_share: clip_share.clamp(0.0, 1.0),
            clip_seconds: 5.0,
            still_seconds: 5.0,
            scene_seconds: 12.0,
            chapter_seconds: 150.0,
            words_per_second: 2.4,
            chapters: None,
        }
    }
}

/// The shape of a whole video.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Budget {
    /// Its chapters, in order.
    pub chapters: Vec<ChapterPlan>,
}

/// One chapter's shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChapterPlan {
    /// How long it runs.
    pub seconds: f64,
    /// Its scenes, in order.
    pub scenes: Vec<ScenePlan>,
}

/// One scene's shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScenePlan {
    /// How long it runs.
    pub seconds: f64,
    /// About how many words its narration has.
    pub words: usize,
    /// Its shots: what each is made as, and how long it holds.
    pub shots: Vec<(ShotKind, f64)>,
}

/// What a budget adds up to: what the sheet shows before writing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// The length, in seconds.
    pub seconds: f64,
    /// Chapters.
    pub chapters: usize,
    /// Scenes, which are the voice lines: one narration each.
    pub scenes: usize,
    /// Clips: video prompts.
    pub clips: usize,
    /// Stills: image prompts.
    pub stills: usize,
    /// Words of narration.
    pub words: usize,
}

/// Works out the shape of a video paced by `pacing`.
pub fn budget(pacing: &Pacing) -> Budget {
    let total = (pacing.minutes * 60.0).max(10.0);
    let chapter_count = pacing
        .chapters
        .unwrap_or_else(|| (total / pacing.chapter_seconds).round() as usize)
        .clamp(1, 60);
    let chapter_seconds = total / chapter_count as f64;
    let scene_count = ((chapter_seconds / pacing.scene_seconds).round() as usize).max(1);
    let scene_seconds = chapter_seconds / scene_count as f64;
    // The length the average shot holds, given the mix.
    let shot_seconds =
        pacing.clip_share * pacing.clip_seconds + (1.0 - pacing.clip_share) * pacing.still_seconds;
    let shot_count = ((scene_seconds / shot_seconds).round() as usize).max(1);

    // Clips spread evenly over the whole film rather than bunched at the
    // start: the nth shot is a clip when it takes the running count of
    // clips over the next whole number.
    let mut n = 0usize;
    let mut next_kind = || {
        let before = (n as f64 * pacing.clip_share + 0.5).floor();
        let after = ((n + 1) as f64 * pacing.clip_share + 0.5).floor();
        n += 1;
        if after > before {
            ShotKind::Video
        } else {
            ShotKind::Image
        }
    };

    let chapters = (0..chapter_count)
        .map(|_| ChapterPlan {
            seconds: chapter_seconds,
            scenes: (0..scene_count)
                .map(|_| {
                    let kinds: Vec<ShotKind> = (0..shot_count).map(|_| next_kind()).collect();
                    // The scene's time shared in proportion to what each shot
                    // is: a clip its clip's length, a still its hold.
                    let weight = |kind: &ShotKind| match kind {
                        ShotKind::Video => pacing.clip_seconds,
                        ShotKind::Image => pacing.still_seconds,
                    };
                    let weights: f64 = kinds.iter().map(weight).sum();
                    ScenePlan {
                        seconds: scene_seconds,
                        words: (scene_seconds * pacing.words_per_second).round() as usize,
                        shots: kinds
                            .iter()
                            .map(|kind| (*kind, scene_seconds * weight(kind) / weights))
                            .collect(),
                    }
                })
                .collect(),
        })
        .collect();
    Budget { chapters }
}

impl Budget {
    /// What it adds up to.
    pub fn totals(&self) -> Totals {
        let scenes = self
            .chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter());
        let mut totals = Totals {
            chapters: self.chapters.len(),
            ..Totals::default()
        };
        for scene in scenes {
            totals.seconds += scene.seconds;
            totals.scenes += 1;
            totals.words += scene.words;
            for (kind, _) in &scene.shots {
                match kind {
                    ShotKind::Video => totals.clips += 1,
                    ShotKind::Image => totals.stills += 1,
                }
            }
        }
        totals
    }
}

impl ScenePlan {
    /// The scene's shape in a line, for the writer: "11 s, about 26 words;
    /// shots: video 4.6 s, image 3.2 s".
    pub fn describe(&self) -> String {
        let shots: Vec<String> = self
            .shots
            .iter()
            .map(|(kind, seconds)| {
                let kind = match kind {
                    ShotKind::Video => "video",
                    ShotKind::Image => "image",
                };
                format!("{kind} {seconds:.1} s")
            })
            .collect();
        format!(
            "{:.0} s, about {} words; shots: {}",
            self.seconds,
            self.words,
            shots.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_minutes_adds_up_to_ten_minutes() {
        let budget = budget(&Pacing::new(10.0, 0.3));
        let totals = budget.totals();
        assert!((totals.seconds - 600.0).abs() < 1e-6, "{totals:?}");
        assert_eq!(totals.chapters, 4);
        assert!(totals.scenes >= 40 && totals.scenes <= 60, "{totals:?}");
        let shots = totals.clips + totals.stills;
        assert_eq!(totals.clips, (shots as f64 * 0.3).round() as usize);
        // A voice line a scene, at about two and a half words a second.
        assert!((totals.words as f64 - 600.0 * 2.4).abs() < totals.scenes as f64);
    }

    #[test]
    fn every_scene_has_a_shot_and_its_shots_fill_it() {
        for minutes in [1.0, 3.0, 60.0] {
            for share in [0.0, 0.5, 1.0] {
                let budget = budget(&Pacing::new(minutes, share));
                for scene in budget.chapters.iter().flat_map(|c| c.scenes.iter()) {
                    assert!(!scene.shots.is_empty());
                    let filled: f64 = scene.shots.iter().map(|(_, s)| s).sum();
                    assert!((filled - scene.seconds).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn the_share_decides_the_mix() {
        let none = budget(&Pacing::new(5.0, 0.0)).totals();
        assert_eq!(none.clips, 0);
        let all = budget(&Pacing::new(5.0, 1.0)).totals();
        assert_eq!(all.stills, 0);
        // Clips spread out: no run of a quarter-share film is all clips.
        let quarter = budget(&Pacing::new(10.0, 0.25));
        let kinds: Vec<ShotKind> = quarter
            .chapters
            .iter()
            .flat_map(|c| c.scenes.iter())
            .flat_map(|s| s.shots.iter().map(|(k, _)| *k))
            .collect();
        assert!(kinds.windows(4).all(|w| w.contains(&ShotKind::Image)));
    }

    #[test]
    fn a_scene_reads_as_a_line() {
        let scene = ScenePlan {
            seconds: 11.0,
            words: 26,
            shots: vec![(ShotKind::Video, 6.0), (ShotKind::Image, 5.0)],
        };
        assert_eq!(
            scene.describe(),
            "11 s, about 26 words; shots: video 6.0 s, image 5.0 s"
        );
    }
}
