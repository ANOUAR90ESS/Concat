// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The storyboard: what the video says and shows, scene by scene.
//!
//! It is plain JSON in the job's folder, `storyboard.json`, so it can be
//! written by the writer, by hand, or by the writer and then by hand:
//!
//! ```json
//! {
//!   "title": "...", "language": "ar", "aspect": "16:9",
//!   "style": {"visual": "cinematic, 35mm, warm light", "voice": "calm"},
//!   "chapters": [{"title": "...", "summary": "...", "mood": "cold dawn blues",
//!     "scenes": [
//!     {"id": "c01s01", "narration": "...", "voice": "hushed, curious",
//!      "onScreenText": null, "seconds": 8, "shots": [
//!        {"kind": "video", "prompt": "what the camera sees",
//!         "motion": "slow push in", "seconds": 5},
//!        {"kind": "image", "prompt": "...", "seconds": 3}]}
//!   ]}]
//! }
//! ```
//!
//! Every scene carries the three things a generator is asked for: its
//! narration and how it is said (the voice), and its shots, each a still
//! (an image prompt) or a clip (the same picture, and how it moves). A
//! scene lasts as long as its narration takes to say, and its shots share
//! that time. The film's visual style is appended to every shot's prompt
//! and the chapter's mood after it, so a hundred shots generated one at a
//! time look like one film without every chapter looking the same.

use serde::{Deserialize, Serialize};

/// The frames a storyboard can be made for, and their size.
pub const ASPECTS: [(&str, u32, u32); 4] = [
    ("16:9", 1920, 1080),
    ("9:16", 1080, 1920),
    ("1:1", 1080, 1080),
    ("4:5", 1080, 1350),
];

/// A whole video.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Storyboard {
    /// The video's title; the timeline is named after it.
    pub title: String,
    /// The narration's language, as a code or a name.
    pub language: String,
    /// One of [`ASPECTS`].
    pub aspect: String,
    /// The look and the voice shared by every scene.
    pub style: Style,
    /// The video, chapter by chapter.
    pub chapters: Vec<Chapter>,
}

/// What every scene shares.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Style {
    /// One line appended to every shot prompt: medium, lens, light, grade.
    pub visual: String,
    /// How the narrator sounds.
    pub voice: String,
}

/// One chapter: a title over its first scene, and its scenes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chapter {
    /// The chapter's name.
    pub title: String,
    /// What it covers, in a sentence: what the chapters after it are told,
    /// so none of them says it again.
    pub summary: String,
    /// Its own light and palette inside the film's style - "cold dawn
    /// blues", "dusty noon gold" - so a long video moves between looks
    /// rather than holding one for an hour. Empty is the film's look alone.
    pub mood: String,
    /// Its scenes, in order.
    pub scenes: Vec<Scene>,
}

/// One scene: a stretch of narration and the shots under it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scene {
    /// "c01s01": chapter and scene, filled in when absent. Every file made
    /// for the scene is named from it.
    pub id: String,
    /// What the voice says over the scene.
    pub narration: String,
    /// How it is said: pace, feeling, emphasis. A direction for the reader,
    /// never read aloud.
    pub voice: String,
    /// A title over the scene's start, if any.
    pub on_screen_text: Option<String>,
    /// How long the scene was written to last; its narration decides in
    /// the end, and this is the length of a scene with none.
    pub seconds: f64,
    /// What the camera sees, one shot after another.
    pub shots: Vec<Shot>,
}

/// One shot: a picture, still or moving.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shot {
    /// A still from an image model, or a clip from a video model.
    pub kind: ShotKind,
    /// What the camera sees, in English: the image prompt, and for a clip
    /// the picture it starts from.
    pub prompt: String,
    /// For a clip, how it moves: the camera and what moves in the frame.
    /// Empty on a still.
    pub motion: String,
    /// How long the shot was planned to hold; 0 shares the scene evenly.
    pub seconds: f64,
}

/// What a shot is made as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShotKind {
    /// A still, moved over by the cut.
    #[default]
    Image,
    /// A clip.
    Video,
}

impl Storyboard {
    /// Reads `storyboard.json`, filling in what may be left out.
    pub fn read(path: &std::path::Path) -> Result<Storyboard, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut board: Storyboard =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        board.tidy()?;
        Ok(board)
    }

    /// Writes it, pretty, for a person to read and edit.
    pub fn write(&self, path: &std::path::Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, text + "\n").map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Fills in scene ids and the aspect, and refuses a storyboard the later
    /// steps could not use: no scenes, a scene without a shot, an id twice.
    pub fn tidy(&mut self) -> Result<(), String> {
        if self.aspect.is_empty() {
            self.aspect = "16:9".to_owned();
        }
        if !ASPECTS.iter().any(|(name, ..)| *name == self.aspect) {
            return Err(format!("unknown aspect {}", self.aspect));
        }
        let mut seen = std::collections::HashSet::new();
        for (c, chapter) in self.chapters.iter_mut().enumerate() {
            for (s, scene) in chapter.scenes.iter_mut().enumerate() {
                if scene.id.is_empty() {
                    scene.id = format!("c{:02}s{:02}", c + 1, s + 1);
                }
                if !seen.insert(scene.id.clone()) {
                    return Err(format!("scene {} is named twice", scene.id));
                }
                scene.shots.retain(|shot| !shot.prompt.trim().is_empty());
                if scene.shots.is_empty() {
                    return Err(format!("scene {} has no shots", scene.id));
                }
                if !scene.seconds.is_finite() || scene.seconds < 0.0 {
                    scene.seconds = 0.0;
                }
            }
        }
        if seen.is_empty() {
            return Err("the storyboard has no scenes".to_owned());
        }
        Ok(())
    }

    /// Every scene in order.
    pub fn scenes(&self) -> impl Iterator<Item = &Scene> {
        self.chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter())
    }

    /// How many shots the whole video has.
    pub fn shot_count(&self) -> usize {
        self.scenes().map(|scene| scene.shots.len()).sum()
    }

    /// The length the writer aimed for, in seconds.
    pub fn seconds(&self) -> f64 {
        self.scenes().map(|scene| scene.seconds).sum()
    }

    /// The frame, width by height.
    pub fn frame(&self) -> (u32, u32) {
        ASPECTS
            .iter()
            .find(|(name, ..)| *name == self.aspect)
            .map(|(_, width, height)| (*width, *height))
            .unwrap_or((1920, 1080))
    }

    /// How many shots are clips, and how many stills.
    pub fn kinds(&self) -> (usize, usize) {
        let clips = self
            .scenes()
            .flat_map(|scene| scene.shots.iter())
            .filter(|shot| shot.kind == ShotKind::Video)
            .count();
        (clips, self.shot_count() - clips)
    }

    /// How many words the narration has, all told.
    pub fn words(&self) -> usize {
        self.scenes()
            .map(|scene| scene.narration.split_whitespace().count())
            .sum()
    }

    /// The image prompt: the shot, then the film's look, then the
    /// chapter's mood.
    pub fn prompt(&self, chapter: &Chapter, shot: &Shot) -> String {
        let mut parts = vec![shot.prompt.trim().trim_end_matches('.').to_owned()];
        for extra in [self.style.visual.trim(), chapter.mood.trim()] {
            if !extra.is_empty() {
                parts.push(extra.trim_end_matches('.').to_owned());
            }
        }
        parts.join(". ")
    }

    /// The video prompt: the picture, how it moves, then the look.
    pub fn video_prompt(&self, chapter: &Chapter, shot: &Shot) -> String {
        let motion = shot.motion.trim().trim_end_matches('.');
        if motion.is_empty() {
            return self.prompt(chapter, shot);
        }
        let mut with_motion = shot.clone();
        with_motion.prompt = format!("{}. {motion}", shot.prompt.trim().trim_end_matches('.'));
        self.prompt(chapter, &with_motion)
    }

    /// Pairs of shots whose pictures are near enough alike to read as the
    /// same shot twice, by their ids ("c01s02_1"). A long video written a
    /// chapter at a time drifts back to the shots it has already used; this
    /// is how that is caught.
    pub fn repeats(&self) -> Vec<(String, String)> {
        let shots: Vec<(String, std::collections::HashSet<String>)> = self
            .scenes()
            .flat_map(|scene| {
                scene
                    .shots
                    .iter()
                    .enumerate()
                    .map(|(n, shot)| (shot_stem(&scene.id, n), content_words(&shot.prompt)))
            })
            .collect();
        let mut pairs = Vec::new();
        for (i, (a, words_a)) in shots.iter().enumerate() {
            for (b, words_b) in &shots[i + 1..] {
                if alike(words_a, words_b) {
                    pairs.push((a.clone(), b.clone()));
                }
            }
        }
        pairs
    }

    /// Markdown: every shot's prompt under its scene, with the name its file
    /// is looked for under, for making shots by hand in any tool.
    pub fn prompt_sheet(&self) -> String {
        let mut out = format!(
            "# {}\n\nAspect {}. Save each shot under the name given, in this folder's \
             `shots/`, as .mp4, .mov, .webm, .png or .jpg.\n",
            self.title, self.aspect
        );
        for chapter in &self.chapters {
            out += &format!("\n## {}\n", chapter.title);
            for scene in &chapter.scenes {
                out += &format!("\n### {} ({} s)\n\n", scene.id, scene.seconds);
                if !scene.narration.is_empty() {
                    out += &format!("> {}\n\n", scene.narration);
                }
                if !scene.voice.trim().is_empty() {
                    out += &format!("Voice: {}\n\n", scene.voice.trim());
                }
                for (n, shot) in scene.shots.iter().enumerate() {
                    let stem = shot_stem(&scene.id, n);
                    out += &match shot.kind {
                        ShotKind::Image => {
                            format!("- `{stem}` image: {}\n", self.prompt(chapter, shot))
                        }
                        ShotKind::Video => {
                            format!("- `{stem}` video: {}\n", self.video_prompt(chapter, shot))
                        }
                    };
                }
            }
        }
        out
    }
}

/// Words too common to tell two pictures apart.
const COMMON: &[&str] = &[
    "a", "an", "the", "of", "in", "on", "at", "to", "and", "with", "from", "by", "over", "under",
    "into", "its", "his", "her", "their", "is", "are", "shot", "camera", "frame", "framing",
    "light", "lit", "wide", "medium", "close", "closeup", "up", "view", "angle", "slow",
];

/// A prompt's telling words: lower case, no punctuation, no [`COMMON`].
fn content_words(prompt: &str) -> std::collections::HashSet<String> {
    prompt
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| word.len() > 2 && !COMMON.contains(&word.as_str()))
        .collect()
}

/// Two pictures alike: most of the smaller one's telling words are the
/// other's too. Prompts too short to judge never match.
fn alike(a: &std::collections::HashSet<String>, b: &std::collections::HashSet<String>) -> bool {
    let smaller = a.len().min(b.len());
    if smaller < 4 {
        return false;
    }
    let shared = a.intersection(b).count();
    shared as f64 / smaller as f64 >= 0.75
}

/// The name, without extension, a scene's `n`th shot (from 0) is saved as.
pub fn shot_stem(scene: &str, n: usize) -> String {
    format!("{scene}_{}", n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Storyboard {
        serde_json::from_str(
            r#"{"title":"T","style":{"visual":"warm light"},"chapters":[{"title":"A","scenes":[
                {"narration":"n","seconds":6,"shots":[{"prompt":"a dune."},{"prompt":"b"}]},
                {"narration":"","seconds":2,"shots":[{"prompt":"c"}]}]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn ids_and_the_aspect_are_filled_in() {
        let mut board = board();
        board.tidy().unwrap();
        let ids: Vec<_> = board.scenes().map(|scene| scene.id.as_str()).collect();
        assert_eq!(ids, ["c01s01", "c01s02"]);
        assert_eq!(board.frame(), (1920, 1080));
        assert_eq!(board.shot_count(), 3);
    }

    #[test]
    fn the_style_and_the_mood_ride_on_every_prompt() {
        let mut board = board();
        let chapter = board.chapters[0].clone();
        let shot = &chapter.scenes[0].shots[0];
        assert_eq!(board.prompt(&chapter, shot), "a dune. warm light");
        board.chapters[0].mood = "dusk violet".into();
        board.chapters[0].scenes[0].shots[0].kind = ShotKind::Video;
        board.chapters[0].scenes[0].shots[0].motion = "slow push in.".into();
        let chapter = board.chapters[0].clone();
        let shot = &chapter.scenes[0].shots[0];
        assert_eq!(
            board.video_prompt(&chapter, shot),
            "a dune. slow push in. warm light. dusk violet"
        );
        board.tidy().unwrap();
        let sheet = board.prompt_sheet();
        assert!(
            sheet.contains("`c01s01_1` video: a dune. slow push in."),
            "{sheet}"
        );
        assert!(
            sheet.contains("`c01s01_2` image: b. warm light. dusk violet"),
            "{sheet}"
        );
        assert_eq!(board.kinds(), (1, 2));
    }

    #[test]
    fn an_old_storyboard_reads_as_stills() {
        let board = board();
        let shot = &board.chapters[0].scenes[0].shots[0];
        assert_eq!(shot.kind, ShotKind::Image);
        assert!(shot.motion.is_empty());
        assert!(board.chapters[0].scenes[0].voice.is_empty());
    }

    #[test]
    fn a_shot_written_twice_is_caught() {
        let mut board = board();
        board.chapters[0].scenes[0].shots[0].prompt =
            "aerial view of red sandstone ruins at the edge of a dry riverbed".into();
        board.chapters[0].scenes[1].shots[0].prompt =
            "wide aerial shot over red sandstone ruins beside a dry riverbed".into();
        board.chapters[0].scenes[0].shots[1].prompt =
            "a merchant's hands weighing saffron on a brass scale".into();
        board.tidy().unwrap();
        assert_eq!(
            board.repeats(),
            vec![("c01s01_1".to_owned(), "c01s02_1".to_owned())]
        );
    }

    #[test]
    fn a_scene_without_shots_is_refused() {
        let mut board = board();
        board.chapters[0].scenes[1].shots[0].prompt = "  ".to_owned();
        assert!(board.tidy().is_err());
        let mut empty = Storyboard::default();
        assert!(empty.tidy().is_err());
    }
}
