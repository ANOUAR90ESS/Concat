// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Checking the pictures against the words: once the shots are made, each
//! picture a model drew is shown to a model that can see, with the words
//! the narrator says over it and what the shot was meant to show, and
//! judged - does it show what is said, are the things it needs there, is
//! anything in it misleading or beside the point. A picture that fails is
//! drawn again with what the check says is missing, a set number of times,
//! and the best take is kept. What could not be checked - no key, a file
//! too large, a model that cannot see - is said so, never taken as passed.
//!
//! The app's own drawings - animated cards and explainers - are drawn from
//! the narration's own words and figures, and are not checked.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::storyboard::{Scene, Shot, ShotKind, Storyboard, shot_stem};

/// The score, out of 10, a picture needs to pass, unless the person sets
/// another.
pub const DEFAULT_THRESHOLD: u8 = 7;

/// How many times a failed picture is drawn again, unless the person sets
/// another: each is a picture paid for.
pub const DEFAULT_ATTEMPTS: u8 = 1;

/// The largest picture sent to be looked at: what one request takes of an
/// image once it is written out as text.
const LARGEST: u64 = 3_700_000;

/// What the check found in one picture.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Check {
    /// The shot, as its file is named.
    pub stem: String,
    /// The picture looked at.
    pub file: String,
    /// It was looked at; false when it could not be, with why in `note`.
    pub checked: bool,
    /// How well it shows what is said, 0 to 10.
    pub semantic_alignment_score: u8,
    /// What it needed that it has.
    pub required_elements_present: Vec<String>,
    /// What it needed that it lacks.
    pub missing_elements: Vec<String>,
    /// Where it shows something other than what is said, or misleads.
    pub visual_mismatches: Vec<String>,
    /// It passes.
    pub is_acceptable: bool,
    /// How to draw it so it would.
    pub improvement_instructions: String,
    /// Why it could not be checked, or what was done after.
    pub note: String,
}

impl Check {
    /// A picture not looked at, and why: never a pass.
    pub fn unchecked(stem: &str, file: &Path, why: &str) -> Self {
        Check {
            stem: stem.to_owned(),
            file: file.to_string_lossy().into_owned(),
            note: why.to_owned(),
            ..Check::default()
        }
    }

    /// The check as the seeing model's answer gives it, held to
    /// `threshold`: a picture scored under it does not pass, whatever the
    /// answer says.
    pub fn from_value(stem: &str, file: &Path, got: &Value, threshold: u8) -> Self {
        let list = |key: &str| -> Vec<String> {
            got[key]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(|item| item.trim().to_owned())
                        .filter(|item| !item.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let score = got["semantic_alignment_score"]
            .as_f64()
            .unwrap_or(0.0)
            .clamp(0.0, 10.0)
            .round() as u8;
        Check {
            stem: stem.to_owned(),
            file: file.to_string_lossy().into_owned(),
            checked: true,
            semantic_alignment_score: score,
            required_elements_present: list("required_elements_present"),
            missing_elements: list("missing_elements"),
            visual_mismatches: list("visual_mismatches"),
            is_acceptable: got["is_acceptable"].as_bool().unwrap_or(false) && score >= threshold,
            improvement_instructions: got["improvement_instructions"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            note: String::new(),
        }
    }

    /// What a picture drawn again is told, after the shot's own prompt.
    pub fn amend(&self, prompt: &str) -> String {
        let mut asked = Vec::new();
        if !self.missing_elements.is_empty() {
            asked.push(format!(
                "It must show: {}.",
                self.missing_elements.join(", ")
            ));
        }
        if !self.visual_mismatches.is_empty() {
            asked.push(format!("Avoid: {}.", self.visual_mismatches.join("; ")));
        }
        if !self.improvement_instructions.is_empty() {
            asked.push(self.improvement_instructions.clone());
        }
        if asked.is_empty() {
            return prompt.to_owned();
        }
        format!("{} {}", prompt.trim(), asked.join(" "))
    }
}

/// The shots a model drew - stills and clips, by the picture each was
/// made from - with the scene each sits in, in order.
pub fn drawn(board: &Storyboard) -> Vec<(&Scene, usize, &Shot)> {
    board
        .scenes()
        .flat_map(|scene| {
            scene
                .shots
                .iter()
                .enumerate()
                .filter(|(_, shot)| matches!(shot.kind, ShotKind::Image | ShotKind::Video))
                .map(move |(index, shot)| (scene, index, shot))
        })
        .collect()
}

/// The ask: the words said over the picture, what it was meant to show,
/// and what to judge.
pub fn check_prompt(scene: &Scene, shot: &Shot, style: &str) -> String {
    let mut prompt = format!(
        "You are checking that a picture in a video shows what the narrator says over it.\n\
         The narration over this picture: \"{}\"\n",
        scene.narration.trim()
    );
    if !shot.idea.trim().is_empty() {
        prompt += &format!(
            "What the picture is meant to make the viewer see: {}\n",
            shot.idea.trim()
        );
    }
    if !shot.elements.is_empty() {
        prompt += &format!("What it must show: {}\n", shot.elements.join(", "));
    }
    prompt += &format!("The picture was asked for as: \"{}\"\n", shot.prompt.trim());
    if !style.trim().is_empty() {
        prompt += &format!("The film's look, which is not a fault: {}\n", style.trim());
    }
    prompt
        + "Judge the attached picture. Does it represent what the narration says? Are the \
           things it must show there? Does it show the action meant? Does it show something \
           else, or mislead - a wrong object, a wrong era, an unrelated person, a decorative \
           scene that says nothing? Score how well it shows what is said, 0 to 10: 10 a viewer \
           would understand the narration from it, 5 related but vague, 0 unrelated. Accept it \
           only if a viewer would see the link without guessing. If not, say exactly how to \
           draw it so it would."
}

/// The check, as a tool whose input is the answer.
pub fn check_tool() -> Value {
    let list = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "name": "picture_check",
        "description": "Whether the picture shows what the narration says.",
        "input_schema": {
            "type": "object",
            "properties": {
                "semantic_alignment_score": {"type": "number"},
                "required_elements_present": list,
                "missing_elements": list,
                "visual_mismatches": list,
                "is_acceptable": {"type": "boolean"},
                "improvement_instructions": {"type": "string"}
            },
            "required": [
                "semantic_alignment_score", "required_elements_present", "missing_elements",
                "visual_mismatches", "is_acceptable", "improvement_instructions"
            ]
        }
    })
}

/// The picture as a content block a seeing model takes, or why it cannot
/// be sent.
pub fn picture_block(file: &Path) -> Result<Value, String> {
    use base64::Engine as _;
    let media = match file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => return Err("not a picture a model can look at".to_owned()),
    };
    let size = std::fs::metadata(file)
        .map_err(|error| format!("{}: {error}", file.display()))?
        .len();
    if size > LARGEST {
        return Err(format!(
            "too large to send to be looked at ({} MB)",
            size / 1_000_000
        ));
    }
    let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": media,
            "data": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }
    }))
}

/// The picture a shot's check looks at: a still as it is, a clip by the
/// still it was made from - a clip made from words alone has none, and is
/// not checked.
pub fn picture_of(job: &crate::job::Job, scene: &Scene, index: usize) -> Option<PathBuf> {
    let stem = shot_stem(&scene.id, index);
    match scene.shots[index].kind {
        ShotKind::Video => {
            crate::providers::existing(&job.frames_dir(), &stem, &crate::providers::SHOT_EXTENSIONS)
        }
        _ => job.current(&stem),
    }
}

/// What a whole check found, kept in the job's folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    /// The score a picture needed.
    pub threshold: u8,
    /// Every picture, as last checked.
    pub checks: Vec<Check>,
    /// The shots drawn again.
    pub redrawn: Vec<String>,
}

impl Report {
    /// How many pictures passed, failed, and were not looked at.
    pub fn counts(&self) -> (usize, usize, usize) {
        let passed = self
            .checks
            .iter()
            .filter(|check| check.is_acceptable)
            .count();
        let unchecked = self.checks.iter().filter(|check| !check.checked).count();
        (passed, self.checks.len() - passed - unchecked, unchecked)
    }
}

/// The file a whole check is kept in, in the job's folder.
pub fn report_file(job: &crate::job::Job) -> PathBuf {
    job.dir.join("picture-checks.json")
}

/// The last check kept with the job, if one ran.
pub fn read_report(job: &crate::job::Job) -> Option<Report> {
    let bytes = std::fs::read(report_file(job)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Looks at every picture a model drew for `board` against its narration
/// and draws again, with what was missing, each still that fails - up to
/// `attempts` times, each a picture paid for at the run's prices. A clip
/// that fails is said, not made again: a clip costs many stills. The
/// prompts drawn again are kept in `board`, so a later remake starts from
/// them. Stops between pictures when `stop` says to. The report is kept in
/// the job's folder, and what was spent on redrawing is in it.
#[allow(clippy::too_many_arguments)]
pub fn check_and_mend(
    job: &crate::job::Job,
    board: &mut Storyboard,
    shots: &crate::providers::Shots,
    costs: &crate::job::Costs,
    key: &str,
    model: &str,
    threshold: u8,
    attempts: u8,
    progress: &mut dyn FnMut(usize, usize),
    stop: &dyn Fn() -> bool,
) -> (Report, f64) {
    let mut report = Report {
        threshold,
        ..Report::default()
    };
    let mut spent = 0.0;
    let todo: Vec<(String, usize)> = drawn(board)
        .iter()
        .map(|(scene, index, _)| (scene.id.clone(), *index))
        .collect();
    let of = todo.len();
    for (n, (id, index)) in todo.into_iter().enumerate() {
        if stop() {
            break;
        }
        progress(n + 1, of);
        let stem = shot_stem(&id, index);
        let Some((c, s)) = board.position_of(&id) else {
            continue;
        };
        let mut check = look(job, board, c, s, index, key, model, threshold);
        let mut tries = 0;
        while check.checked
            && !check.is_acceptable
            && tries < attempts
            && board.chapters[c].scenes[s].shots[index].kind == ShotKind::Image
            && !stop()
        {
            tries += 1;
            let shot = &mut board.chapters[c].scenes[s].shots[index];
            shot.prompt = check.amend(&shot.prompt);
            if let Err(error) = job.keep(board) {
                log::warn!("{stem}: could not keep the amended prompt: {error}");
            }
            match job.remake(board, &stem, shots, costs) {
                Ok(remade) => {
                    spent += remade.spent;
                    if !report.redrawn.contains(&stem) {
                        report.redrawn.push(stem.clone());
                    }
                    check = look(job, board, c, s, index, key, model, threshold);
                }
                Err(error) => {
                    check.note = format!("drawn again, but: {error}");
                    break;
                }
            }
        }
        if !check.is_acceptable
            && check.checked
            && board.chapters[c].scenes[s].shots[index].kind == ShotKind::Video
        {
            check.note = "a clip is not made again on its own: remake it from the script".into();
        }
        report.checks.push(check);
    }
    match serde_json::to_vec_pretty(&report) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(report_file(job), bytes) {
                log::warn!("the picture checks could not be kept: {error}");
            }
        }
        Err(error) => log::warn!("the picture checks: {error}"),
    }
    (report, spent)
}

/// One picture of shot `index` of scene `s` of chapter `c`, looked at.
#[allow(clippy::too_many_arguments)]
fn look(
    job: &crate::job::Job,
    board: &Storyboard,
    c: usize,
    s: usize,
    index: usize,
    key: &str,
    model: &str,
    threshold: u8,
) -> Check {
    let scene = &board.chapters[c].scenes[s];
    let shot = &scene.shots[index];
    let stem = shot_stem(&scene.id, index);
    let Some(file) = picture_of(job, scene, index) else {
        return Check::unchecked(&stem, Path::new(""), "no picture to look at");
    };
    let block = match picture_block(&file) {
        Ok(block) => block,
        Err(why) => return Check::unchecked(&stem, &file, &why),
    };
    match crate::writer::look_at(
        key,
        model,
        &check_tool(),
        &check_prompt(scene, shot, &board.style.visual),
        block,
    ) {
        Ok(got) => Check::from_value(&stem, &file, &got, threshold),
        Err(error) => Check::unchecked(&stem, &file, &format!("could not be looked at: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeing model's answer, held to the threshold: a low score never
    /// passes, whatever it says; lists keep their words, blanks dropped.
    #[test]
    fn a_check_is_held_to_its_threshold() {
        let got = json!({
            "semantic_alignment_score": 6.4,
            "required_elements_present": ["a meter"],
            "missing_elements": ["an electricity bill", " "],
            "visual_mismatches": ["a city skyline unrelated to the narration"],
            "is_acceptable": true,
            "improvement_instructions": " Show the bill on a kitchen table. "
        });
        let check = Check::from_value("s1_0", Path::new("/shots/s1_0.png"), &got, 7);
        assert!(check.checked);
        assert_eq!(check.semantic_alignment_score, 6);
        assert!(!check.is_acceptable);
        assert_eq!(check.missing_elements, ["an electricity bill"]);
        assert_eq!(
            check.improvement_instructions,
            "Show the bill on a kitchen table."
        );
        assert!(Check::from_value("s1_0", Path::new("x.png"), &got, 6).is_acceptable);
        // Nonsense is no pass.
        let nothing = Check::from_value("s1_0", Path::new("x.png"), &json!(null), 0);
        assert!(!nothing.is_acceptable);
        // Not looked at is never a pass.
        let unchecked = Check::unchecked("s1_0", Path::new("x.png"), "no key");
        assert!(!unchecked.checked && !unchecked.is_acceptable);
    }

    /// A picture drawn again is told what was missing, what to avoid and
    /// how, after its own prompt.
    #[test]
    fn a_redrawn_picture_is_told_what_was_wrong() {
        let check = Check {
            missing_elements: vec!["a bill".into(), "a meter".into()],
            visual_mismatches: vec!["a skyline".into()],
            improvement_instructions: "A kitchen table, close.".into(),
            ..Check::default()
        };
        assert_eq!(
            check.amend("A family at home. "),
            "A family at home. It must show: a bill, a meter. Avoid: a skyline. A kitchen table, close."
        );
        assert_eq!(Check::default().amend("As is"), "As is");
    }

    /// Only what a model drew is checked: cards and explainers are the
    /// app's own drawing of the narration's words.
    #[test]
    fn only_drawn_pictures_are_checked() {
        let board: Storyboard = serde_json::from_value(json!({
            "chapters": [{"scenes": [{"id": "s1", "narration": "Prices rose.", "shots": [
                {"kind": "image", "prompt": "a meter"},
                {"kind": "motion", "prompt": ""},
                {"kind": "explainer", "prompt": "a chart"},
                {"kind": "video", "prompt": "a bill"}
            ]}]}]
        }))
        .unwrap();
        let shots: Vec<usize> = drawn(&board).iter().map(|(_, index, _)| *index).collect();
        assert_eq!(shots, [0, 3]);
        let prompt = check_prompt(
            &board.chapters[0].scenes[0],
            &board.chapters[0].scenes[0].shots[0],
            "",
        );
        assert!(prompt.contains("\"Prices rose.\""));
        assert!(prompt.contains("asked for as: \"a meter\""));
    }

    /// A report kept with the job is read back as it was written; a job
    /// with none has none.
    #[test]
    fn a_report_is_read_back() {
        let root = std::env::temp_dir().join(format!("concat-visual-{}", std::process::id()));
        let job = crate::job::Job::new(&root, "checked");
        std::fs::create_dir_all(&job.dir).unwrap();
        assert!(read_report(&job).is_none());
        let report = Report {
            threshold: 7,
            checks: vec![Check::unchecked("s1_0", Path::new("x.png"), "no key")],
            redrawn: vec!["s2_0".into()],
        };
        std::fs::write(report_file(&job), serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(read_report(&job), Some(report));
        std::fs::remove_dir_all(&root).ok();
    }

    /// Only a picture a model can take is sent.
    #[test]
    fn only_a_picture_is_sent() {
        assert!(picture_block(Path::new("/clip.mp4")).is_err());
        assert!(picture_block(Path::new("/missing.png")).is_err());
    }
}
