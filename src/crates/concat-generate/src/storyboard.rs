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
//!   "chapters": [{"title": "...", "scenes": [
//!     {"id": "c01s01", "narration": "...", "onScreenText": null,
//!      "seconds": 8, "shots": [{"prompt": "what the camera sees"}]}
//!   ]}]
//! }
//! ```
//!
//! A scene lasts as long as its narration takes to say, and its shots share
//! that time. The visual style is appended to every shot's prompt, so a
//! hundred shots generated one at a time still look like one film.

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
    /// A title over the scene's start, if any.
    pub on_screen_text: Option<String>,
    /// How long the scene was written to last; its narration decides in
    /// the end, and this is the length of a scene with none.
    pub seconds: f64,
    /// What the camera sees, one shot after another.
    pub shots: Vec<Shot>,
}

/// One shot: a prompt for a text-to-video model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shot {
    /// What the camera sees, in English.
    pub prompt: String,
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

    /// The prompt a generator gets: the shot, then the film's look.
    pub fn prompt(&self, shot: &Shot) -> String {
        let prompt = shot.prompt.trim().trim_end_matches('.');
        let style = self.style.visual.trim();
        if style.is_empty() {
            prompt.to_owned()
        } else {
            format!("{prompt}. {style}")
        }
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
                for (n, shot) in scene.shots.iter().enumerate() {
                    out += &format!("- `{}`: {}\n", shot_stem(&scene.id, n), self.prompt(shot));
                }
            }
        }
        out
    }
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
    fn the_style_rides_on_every_prompt() {
        let board = board();
        let shot = &board.chapters[0].scenes[0].shots[0];
        assert_eq!(board.prompt(shot), "a dune. warm light");
        let mut tidy = board.clone();
        tidy.tidy().unwrap();
        assert!(tidy.prompt_sheet().contains("`c01s01_2`: b. warm light"));
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
