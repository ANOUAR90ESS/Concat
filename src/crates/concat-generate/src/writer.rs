// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The script: an idea becomes a storyboard, written by Claude.
//!
//! A long video is written the way a writers' room would: an outline
//! first, then each chapter on its own, every call carrying the outline and
//! how the chapter before it ended. One call could not hold an hour of
//! narration in its answer, and chapter by chapter keeps each call short
//! enough to be good. Each call answers through a forced tool, so what
//! comes back is the schema's JSON and never prose to be picked apart.
//!
//! The shape is not the writer's to choose. [`crate::budget`] works out
//! from the length how many chapters, scenes and shots the video has and
//! which shots are clips; every chapter call is handed its scenes one line
//! each - length, words, shots - and what comes back is held to them.
//!
//! Nor is repeating itself. A chapter is told what the chapters before it
//! covered and the pictures they already used, and asked for pictures of
//! its own; the shots it gives back are compared with every earlier one,
//! and any that read as the same picture are sent back to be replaced.

use serde_json::{Value, json};

use crate::budget::{self, ChapterPlan, Pacing, ScenePlan};
use crate::storyboard::{Chapter, Scene, ShotKind, Storyboard, Style, shot_stem};

/// The Messages API.
const API: &str = "https://api.anthropic.com/v1/messages";
/// The model that writes unless the caller names another.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// How many earlier pictures a chapter is shown to keep clear of: the most
/// recent, which are the ones it is likeliest to fall back on.
const AVOID: usize = 80;

const SYSTEM: &str = "You write long-form videos for YouTube: documentaries, explainers, stories.
Narration is written to be heard, not read: short sentences, concrete images, a hook in the
first ten seconds, a question or a turn that pulls the viewer into the next chapter. Never pad,
and never say again what an earlier chapter said.

Every scene has three parts. The narration, in the video's language. The voice: one short line
in English on how the narrator says it - pace, feeling, emphasis - never read aloud. And its
shots, each one of two kinds:
- image: a still. Its prompt, in English, describes one photograph: subject, setting, framing,
  lens, light. No text, logos or captions, no real people's names.
- video: a clip of a few seconds. Its prompt describes the picture it starts from, as for an
  image; its motion says in a short English phrase how the camera moves and what moves in the
  frame.
Describe what is there; do not tell the model what to avoid.

Vary the pictures. Change the shot size from one shot to the next (aerial, wide, medium,
close-up, macro detail), and the subject and the place from one scene to the next. Within one
scene each shot shows something the others do not: another subject, or the same subject from
another side, distance and height - never the same composition in other words. A picture
already used, or one that would read as the same picture, is never used again.";

/// What the video is to be.
#[derive(Clone, Debug, Default)]
pub struct Brief {
    /// The idea, in the person's words.
    pub topic: String,
    /// About how long, in minutes.
    pub minutes: f64,
    /// The narration's language, by name: "Arabic", "English".
    pub language: String,
    /// One of [`crate::storyboard::ASPECTS`].
    pub aspect: String,
    /// Anything else the writer should know: audience, tone, must-haves.
    pub notes: String,
    /// The share of shots that are clips, `0..=1`.
    pub clip_share: f64,
}

impl Brief {
    /// How the video is paced.
    pub fn pacing(&self) -> Pacing {
        Pacing::new(self.minutes, self.clip_share)
    }

    /// The brief as every call starts with it.
    fn text(&self) -> String {
        let mut text = format!(
            "Topic: {}\nLength: about {} minutes\nNarration language: {}\nAspect ratio: {}\n",
            self.topic.trim(),
            self.minutes,
            self.language,
            self.aspect
        );
        if !self.notes.trim().is_empty() {
            text += &format!("Notes from the producer: {}\n", self.notes.trim());
        }
        text
    }
}

/// Where the writing is: the outline, then chapter `n` of `of`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Writing {
    /// The outline is being written.
    Outline,
    /// Chapter `n` (from 1) of `of` is being written.
    Chapter {
        /// This chapter.
        n: usize,
        /// Of how many.
        of: usize,
    },
}

/// Writes a storyboard for `brief`, reporting each call before it is made.
pub fn write(
    brief: &Brief,
    key: &str,
    model: &str,
    mut progress: impl FnMut(Writing),
) -> Result<Storyboard, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let ask =
        |tool: &Value, prompt: String| crate::providers::retry(|| call(key, model, tool, &prompt));
    let brief_text = brief.text();

    progress(Writing::Outline);
    let mut shape = budget::budget(&brief.pacing());
    let wanted = shape.chapters.len();
    let outline = ask(
        &outline_tool(),
        format!(
            "{brief_text}\nOutline the video in exactly {wanted} chapters of about {} seconds \
             each. Give every chapter ground of its own: facts, places and turns no other \
             chapter covers, and a mood - its own light and palette inside the film's style - \
             that sets it apart from the chapters beside it.",
            shape.chapters[0].seconds.round()
        ),
    )?;
    let outlined = outline["chapters"].as_array().cloned().unwrap_or_default();
    if outlined.is_empty() {
        return Err("the writer returned an outline with no chapters".to_owned());
    }
    if outlined.len() != wanted {
        // Fewer or more chapters than asked: the length still holds, shared
        // over the chapters there are.
        let mut pacing = brief.pacing();
        pacing.chapters = Some(outlined.len());
        shape = budget::budget(&pacing);
    }

    let mut board = Storyboard {
        title: text(&outline["title"]),
        language: brief.language.clone(),
        aspect: brief.aspect.clone(),
        style: Style {
            visual: text(&outline["visual_style"]),
            voice: text(&outline["voice_style"]),
        },
        chapters: outlined
            .iter()
            .map(|chapter| Chapter {
                title: text(&chapter["title"]),
                summary: text(&chapter["summary"]),
                mood: text(&chapter["mood"]),
                scenes: Vec::new(),
            })
            .collect(),
    };
    let points: Vec<String> = outlined
        .iter()
        .map(|chapter| {
            chapter["points"]
                .as_array()
                .map(|points| points.iter().map(text).collect::<Vec<_>>().join("; "))
                .unwrap_or_default()
        })
        .collect();

    let mut ending = "This is the first chapter: open with the hook.".to_owned();
    let of = board.chapters.len();
    for (i, (plan, points)) in shape.chapters.iter().zip(&points).enumerate() {
        progress(Writing::Chapter { n: i + 1, of });
        let prompt = chapter_prompt(&board, brief, i, plan, points, &ending);
        let mut got = ask(&scenes_tool(), prompt.clone())?;
        let mut scenes = scenes_of(&got)?;
        if let Some(problem) = misfit(&scenes, plan) {
            // Held to its shape: asked once more, then taken as near as it
            // comes.
            log::warn!("chapter {} came back off its shape: {problem}", i + 1);
            got = ask(
                &scenes_tool(),
                format!(
                    "{prompt}\n\nYour last answer did not keep to the shape: {problem}. Keep to it exactly."
                ),
            )?;
            scenes = scenes_of(&got)?;
        }
        board.chapters[i].scenes = conform(scenes, plan);
        board.tidy()?;
        ending = text(&got["ending"]);

        // The pictures that came back the same as earlier ones, replaced.
        let repeated = repeats_in(&board, i);
        if !repeated.is_empty() {
            log::info!(
                "chapter {}: {} shots repeat earlier ones",
                i + 1,
                repeated.len()
            );
            if let Ok(revised) = ask(&revise_tool(), revise_prompt(&board, brief, &repeated)) {
                apply_revisions(&mut board, &revised);
            }
        }
    }
    board.tidy()?;
    Ok(board)
}

/// Writes one scene again in place - the chapter's `chapter` (from 0),
/// scene `scene` - to the same shape: its length, its words and its shots'
/// kinds. `note` is what the person wants different; `keep_narration`
/// keeps the words and writes only the voice and the pictures.
pub fn rewrite_scene(
    board: &Storyboard,
    brief: &Brief,
    chapter: usize,
    scene: usize,
    note: &str,
    keep_narration: bool,
    key: &str,
    model: &str,
) -> Result<Scene, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let old = board
        .chapters
        .get(chapter)
        .and_then(|c| c.scenes.get(scene))
        .ok_or_else(|| "no such scene".to_owned())?;
    let plan = ScenePlan {
        seconds: old.seconds,
        words: old.narration.split_whitespace().count().max(1),
        shots: old
            .shots
            .iter()
            .map(|shot| (shot.kind, shot.seconds))
            .collect(),
    };
    let around: String = board.chapters[chapter]
        .scenes
        .iter()
        .enumerate()
        .map(|(n, other)| {
            let mark = if n == scene { " <- this scene" } else { "" };
            format!("{}. {}{mark}\n", n + 1, other.narration.trim())
        })
        .collect();
    let mut prompt = format!(
        "{}\n{}\nChapter \"{}\": {}\nIts narration, scene by scene:\n{around}\n\
         Write scene {} again: {}.",
        brief.text(),
        outline_summary(board),
        board.chapters[chapter].title,
        board.chapters[chapter].summary,
        scene + 1,
        plan.describe(),
    );
    if keep_narration {
        prompt += &format!(
            "\nKeep its narration word for word: \"{}\". Write its voice and its shots anew.",
            old.narration.trim()
        );
    }
    if !note.trim().is_empty() {
        prompt += &format!("\nWhat the producer wants different: {}", note.trim());
    }
    prompt += &avoid_list(board, Some(&old.id));
    let got = crate::providers::retry(|| call(key, model, &scene_tool(), &prompt))?;
    let mut new: Scene = serde_json::from_value(got.clone())
        .map_err(|error| format!("the rewritten scene: {error}"))?;
    new.id = old.id.clone();
    if keep_narration {
        new.narration = old.narration.clone();
    }
    conform(
        vec![new],
        &ChapterPlan {
            seconds: plan.seconds,
            scenes: vec![plan],
        },
    )
    .pop()
    .ok_or_else(|| "the rewritten scene came back without a picture".to_owned())
}

/// A new picture for one shot, from the writer: what it shows, and for a
/// clip how it moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// What the shot shows.
    pub prompt: String,
    /// How a clip moves; empty for a still.
    pub motion: String,
}

/// Asks the writer for another picture for shot `shot` of scene `scene` of
/// chapter `chapter` (each from 0): one that says what the narration says
/// as well or better than the picture it has, in the film's look and the
/// chapter's light, and shows nothing another shot already does. One short
/// call - the cost of a few hundred words - and nothing is made: the
/// person reads it, and makes the shot again if they like it.
pub fn suggest_shot(
    board: &Storyboard,
    brief: &Brief,
    chapter: usize,
    scene: usize,
    shot: usize,
    key: &str,
    model: &str,
) -> Result<Suggestion, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let prompt = suggest_prompt(board, brief, chapter, scene, shot)?;
    let video = board.chapters[chapter].scenes[scene].shots[shot].kind == ShotKind::Video;
    let got = crate::providers::retry(|| call(key, model, &suggest_tool(), &prompt))?;
    let suggestion = Suggestion {
        prompt: text(&got["prompt"]),
        motion: if video {
            text(&got["motion"])
        } else {
            String::new()
        },
    };
    if suggestion.prompt.is_empty() {
        return Err("the writer suggested no picture".to_owned());
    }
    Ok(suggestion)
}

/// Asks the writer for the minute of the video most worth a Short: a run
/// of scenes in a row, a minute at most, that holds on its own, and a hook
/// of a few words for its first second. A pick that is not such a run is
/// refused; the caller falls back to [`crate::shorts::opening`].
pub fn pick_short(
    board: &Storyboard,
    brief: &Brief,
    key: &str,
    model: &str,
) -> Result<crate::shorts::Pick, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let got =
        crate::providers::retry(|| call(key, model, &short_tool(), &short_prompt(board, brief)))?;
    let scenes: Vec<String> = got["scenes"]
        .as_array()
        .map(|ids| ids.iter().map(text).filter(|id| !id.is_empty()).collect())
        .unwrap_or_default();
    if !crate::shorts::fits(board, &scenes) {
        return Err(format!(
            "the writer picked scenes that are not a minute in a row: {}",
            scenes.join(", ")
        ));
    }
    Ok(crate::shorts::Pick {
        scenes,
        hook: text(&got["hook"]),
    })
}

/// A music bed's description for MusicGen, in English, the words it plays
/// best from: the writer's, from the video's title, moods and narration.
pub fn music_prompt(board: &Storyboard, key: &str, model: &str) -> Result<String, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let got = crate::providers::retry(|| call(key, model, &music_tool(), &music_ask(board)))?;
    let prompt = text(&got["prompt"]);
    if prompt.is_empty() {
        return Err("the writer described no music".to_owned());
    }
    Ok(prompt)
}

/// The music bed's description where the writer cannot be asked: a calm
/// score that sits under any narration.
pub fn plain_music_prompt() -> String {
    "Instrumental cinematic background score for a documentary, calm and steady, \
     soft piano and strings, no vocals, no drums, smooth and loopable"
        .to_owned()
}

/// What [`music_prompt`] asks.
fn music_ask(board: &Storyboard) -> String {
    let moods: Vec<&str> = board
        .chapters
        .iter()
        .map(|chapter| chapter.mood.trim())
        .filter(|mood| !mood.is_empty())
        .collect();
    let opening: String = board
        .scenes()
        .take(3)
        .map(|scene| scene.narration.trim())
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "The video \"{}\". Its chapters' moods: {}. It opens: {opening}\n\
         Describe, in English and in at most 30 words, the instrumental music to play \
         under the whole of it, quietly, beneath the narration: the genre, the instruments, \
         the tempo and the feeling. No vocals, no lyrics, nothing that fights a voice; \
         it is looped, so it should have no clear ending.",
        board.title,
        if moods.is_empty() {
            "none given".to_owned()
        } else {
            moods.join("; ")
        },
    )
}

/// What [`pick_short`] asks.
fn short_prompt(board: &Storyboard, brief: &Brief) -> String {
    let scenes: String = board
        .scenes()
        .map(|scene| {
            format!(
                "- {} ({:.0} s): {}\n",
                scene.id,
                scene.seconds,
                scene.narration.trim()
            )
        })
        .collect();
    format!(
        "{}\nThe video \"{}\", scene by scene, with each scene's length:\n{scenes}\n\
         Pick the stretch of it most worth a Short for TikTok, Reels and YouTube Shorts: \
         scenes in a row, together {:.0} seconds at most, that make sense on their own and \
         grip from the first line - a surprise, a question, a turn. Give their ids in order, \
         and a hook: at most six words over the first second that make a scroller stop, in \
         the narration's language.",
        brief.text(),
        board.title,
        crate::shorts::LONGEST,
    )
}

/// What [`suggest_shot`] asks.
fn suggest_prompt(
    board: &Storyboard,
    brief: &Brief,
    chapter: usize,
    scene: usize,
    shot: usize,
) -> Result<String, String> {
    let found = board
        .chapters
        .get(chapter)
        .and_then(|c| c.scenes.get(scene))
        .and_then(|s| s.shots.get(shot).map(|found| (s, found)));
    let Some((the_scene, the_shot)) = found else {
        return Err("no such shot".to_owned());
    };
    let the_chapter = &board.chapters[chapter];
    let others: String = the_scene
        .shots
        .iter()
        .enumerate()
        .filter(|(n, _)| *n != shot)
        .map(|(n, other)| format!("- shot {}: {}\n", n + 1, other.prompt.trim()))
        .collect();
    let kind = match the_shot.kind {
        ShotKind::Video => {
            "a video clip: give the picture and, as motion, how the camera and \
                            what is in the picture move"
        }
        ShotKind::Image => "a still image, moved over slowly by the cut",
    };
    let mut prompt = format!(
        "{}\nVisual style: {}\nChapter \"{}\", its light and palette: {}\n\
         The scene's narration: \"{}\"\n",
        brief.text(),
        board.style.visual,
        the_chapter.title,
        the_chapter.mood,
        the_scene.narration.trim(),
    );
    if !others.is_empty() {
        prompt += &format!("The scene's other shots:\n{others}");
    }
    prompt += &format!(
        "\nShot {} of this scene is {kind}, {:.0} seconds. Its picture now: \"{}\"\n\
         The producer wants another picture for it. Write one that shows what the narration \
         says at that moment as well or better, in the film's style and the chapter's light: a \
         different subject, place, angle or shot size from the one it has, written as an image \
         prompt (subject, setting, light, lens, composition), in English.",
        shot + 1,
        the_shot.seconds,
        the_shot.prompt.trim(),
    );
    prompt += &avoid_list(board, Some(&the_scene.id));
    Ok(prompt)
}

/// The prompt for chapter `i`: the brief, the outline, the shape scene by
/// scene, and the pictures to keep clear of.
fn chapter_prompt(
    board: &Storyboard,
    brief: &Brief,
    i: usize,
    plan: &ChapterPlan,
    points: &str,
    ending: &str,
) -> String {
    let chapter = &board.chapters[i];
    let shape: String = plan
        .scenes
        .iter()
        .enumerate()
        .map(|(n, scene)| format!("Scene {}: {}\n", n + 1, scene.describe()))
        .collect();
    let mut prompt = format!(
        "{}\n{}\nVisual style of the whole film: {}\nNarrator: {}\n\n\
         Write chapter {} of {}, \"{}\": {}\n",
        brief.text(),
        outline_summary(board),
        board.style.visual,
        board.style.voice,
        i + 1,
        board.chapters.len(),
        chapter.title,
        chapter.summary,
    );
    if !points.is_empty() {
        prompt += &format!("It covers: {points}\n");
    }
    if !chapter.mood.is_empty() {
        prompt += &format!("Its mood: {}\n", chapter.mood);
    }
    prompt += &format!(
        "Narration in {}. Where the previous chapter left off: {ending}\n\n\
         Exactly {} scenes, each with exactly the shots listed, in that order and of those kinds:\n{shape}",
        brief.language,
        plan.scenes.len(),
    );
    prompt + &avoid_list(board, None)
}

/// Every chapter in a line each: what the next call is told the video is.
fn outline_summary(board: &Storyboard) -> String {
    let lines: String = board
        .chapters
        .iter()
        .enumerate()
        .map(|(n, chapter)| format!("{}. {}: {}\n", n + 1, chapter.title, chapter.summary))
        .collect();
    format!("The outline:\n{lines}")
}

/// The pictures already used, but `skip`'s own, as a list to keep clear of.
fn avoid_list(board: &Storyboard, skip: Option<&str>) -> String {
    let used: Vec<String> = board
        .scenes()
        .filter(|scene| Some(scene.id.as_str()) != skip)
        .flat_map(|scene| scene.shots.iter())
        .map(|shot| short(&shot.prompt, 14))
        .collect();
    if used.is_empty() {
        return String::new();
    }
    let recent = &used[used.len().saturating_sub(AVOID)..];
    format!(
        "\n\nPictures earlier scenes already use; give none of these again, nor anything that \
         would read as the same picture:\n- {}",
        recent.join("\n- ")
    )
}

/// The first `words` words of `text`.
fn short(text: &str, words: usize) -> String {
    text.split_whitespace()
        .take(words)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A chapter call's scenes, read from the tool's answer.
fn scenes_of(got: &Value) -> Result<Vec<Scene>, String> {
    serde_json::from_value(got["scenes"].clone()).map_err(|error| format!("the scenes: {error}"))
}

/// What is off about `scenes` against `plan`, if anything, in words the
/// writer is told when it is asked again.
fn misfit(scenes: &[Scene], plan: &ChapterPlan) -> Option<String> {
    if scenes.len() != plan.scenes.len() {
        return Some(format!(
            "{} scenes where {} were asked for",
            scenes.len(),
            plan.scenes.len()
        ));
    }
    for (n, (scene, want)) in scenes.iter().zip(&plan.scenes).enumerate() {
        let kinds: Vec<ShotKind> = scene.shots.iter().map(|shot| shot.kind).collect();
        let wanted: Vec<ShotKind> = want.shots.iter().map(|(kind, _)| *kind).collect();
        if kinds != wanted {
            return Some(format!(
                "scene {} has {} shots where {} were asked for, or not of the kinds listed",
                n + 1,
                kinds.len(),
                wanted.len()
            ));
        }
    }
    None
}

/// Holds `scenes` to `plan` as near as they come: each scene its planned
/// length, each shot its planned kind and hold, a shot too many dropped, a
/// scene's missing shots made from its last, a scene too many - or one with
/// no picture at all - dropped. What still differs is the count the sheet
/// shows against the budget.
fn conform(scenes: Vec<Scene>, plan: &ChapterPlan) -> Vec<Scene> {
    scenes
        .into_iter()
        .zip(&plan.scenes)
        .filter_map(|(mut scene, want)| {
            scene.seconds = want.seconds;
            scene.shots.retain(|shot| !shot.prompt.trim().is_empty());
            if scene.shots.is_empty() {
                return None;
            }
            scene.shots.truncate(want.shots.len());
            while scene.shots.len() < want.shots.len() {
                let mut again = scene.shots[scene.shots.len() - 1].clone();
                again.prompt = format!("{}, a different angle and framing", again.prompt.trim());
                scene.shots.push(again);
            }
            for (shot, (kind, seconds)) in scene.shots.iter_mut().zip(&want.shots) {
                shot.kind = *kind;
                shot.seconds = *seconds;
                if *kind == ShotKind::Image {
                    shot.motion.clear();
                }
            }
            Some(scene)
        })
        .collect()
}

/// The shots of chapter `i` that read as a picture already used, there or
/// in an earlier chapter, by id with their prompts.
fn repeats_in(board: &Storyboard, i: usize) -> Vec<(String, String)> {
    let prefix = format!("c{:02}", i + 1);
    let mut ids: Vec<String> = board
        .repeats()
        .into_iter()
        // The later of each pair is the copy.
        .map(|(_, later)| later)
        .filter(|id| id.starts_with(&prefix))
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| prompt_of(board, &id).map(|prompt| (id, prompt)))
        .collect()
}

/// A shot's prompt by its id ("c01s02_1").
fn prompt_of(board: &Storyboard, id: &str) -> Option<String> {
    board.scenes().find_map(|scene| {
        scene
            .shots
            .iter()
            .enumerate()
            .find(|(n, _)| shot_stem(&scene.id, *n) == id)
            .map(|(_, shot)| shot.prompt.clone())
    })
}

/// The prompt that sends repeated shots back.
fn revise_prompt(board: &Storyboard, brief: &Brief, repeated: &[(String, String)]) -> String {
    let list: String = repeated
        .iter()
        .map(|(id, prompt)| format!("- {id}: {prompt}\n"))
        .collect();
    format!(
        "{}\nVisual style: {}\n\nThese shots repeat pictures the video already uses:\n{list}\n\
         Give each a new picture: a different subject, place or shot size that still fits its \
         scene's narration.{}",
        brief.text(),
        board.style.visual,
        avoid_list(board, None)
    )
}

/// Puts the revised prompts in place, by id; an id it does not know is
/// passed over.
fn apply_revisions(board: &mut Storyboard, revised: &Value) {
    let Some(shots) = revised["shots"].as_array() else {
        return;
    };
    for revision in shots {
        let id = text(&revision["id"]);
        let prompt = text(&revision["prompt"]);
        if prompt.is_empty() {
            continue;
        }
        for scene in board.chapters.iter_mut().flat_map(|c| c.scenes.iter_mut()) {
            let scene_id = scene.id.clone();
            for (n, shot) in scene.shots.iter_mut().enumerate() {
                if shot_stem(&scene_id, n) == id {
                    shot.prompt = prompt.clone();
                    let motion = text(&revision["motion"]);
                    if shot.kind == ShotKind::Video && !motion.is_empty() {
                        shot.motion = motion;
                    }
                }
            }
        }
    }
}

/// One Messages API call answered through `tool`: the tool's input.
///
/// The model is asked, not forced, to answer with the tool: the current
/// models refuse a `tool_choice` that names one. Without the schema held
/// to, an answer can come back with a list written out as JSON text, a
/// required field left empty, or in words alone: text that is JSON is read
/// as what it says, and an answer still missing what the tool requires is
/// sent back, with what is missing, twice at most.
fn call(key: &str, model: &str, tool: &Value, prompt: &str) -> Result<Value, String> {
    let name = tool["name"].as_str().unwrap_or_default();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(600))
        .build();
    let mut messages = vec![json!({
        "role": "user",
        "content": format!("{prompt}\n\nAnswer by calling the {name} tool, once."),
    })];
    let mut last = format!("Claude did not answer with the {name}");
    for _ in 0..3 {
        let body = json!({
            "model": model,
            "max_tokens": 32000,
            "system": SYSTEM,
            "tools": [tool],
            "tool_choice": {"type": "auto"},
            "messages": messages,
        });
        let reply: Value = match agent
            .post(API)
            .set("x-api-key", key.trim())
            .set("anthropic-version", "2023-06-01")
            .send_json(body)
        {
            Ok(response) => response.into_json().map_err(|error| error.to_string())?,
            Err(ureq::Error::Status(code, response)) => {
                let detail = response.into_string().unwrap_or_default();
                return Err(format!("Claude answered {code}: {}", message_of(&detail)));
            }
            Err(error) => return Err(format!("Claude did not answer: {error}")),
        };
        match reply["stop_reason"].as_str().unwrap_or_default() {
            // A tool call cut off is missing its end, whatever it holds.
            "max_tokens" => {
                return Err(format!(
                    "Claude ran out of room writing the {name}: try a shorter video"
                ));
            }
            "refusal" => return Err(format!("Claude declined to write the {name}")),
            _ => {}
        }
        let content = reply["content"].clone();
        let answered = content.as_array().is_some_and(|blocks| !blocks.is_empty());
        let note = match tool_call(&reply) {
            Some((id, mut input)) => {
                decode_json_text(&mut input);
                let missing = missing(tool, &input);
                if missing.is_empty() {
                    return Ok(input);
                }
                last = format!(
                    "the writer returned the {name} without {} (it gave: {})",
                    missing.join(", "),
                    given(&input)
                );
                json!([{
                    "type": "tool_result",
                    "tool_use_id": id,
                    "is_error": true,
                    "content": format!(
                        "{} missing or empty. Call the {name} tool again with every \
                         required field filled, lists as JSON arrays.",
                        missing.join(", ")
                    ),
                }])
            }
            None => json!(format!("Call the {name} tool now with your answer.")),
        };
        if answered {
            messages.push(json!({"role": "assistant", "content": content}));
            messages.push(json!({"role": "user", "content": note}));
        }
    }
    Err(last)
}

/// The id and the input of the first tool call in a reply, if it made one.
fn tool_call(reply: &Value) -> Option<(String, Value)> {
    reply["content"]
        .as_array()?
        .iter()
        .find(|block| block["type"] == "tool_use")
        .map(|block| {
            (
                block["id"].as_str().unwrap_or_default().to_owned(),
                block["input"].clone(),
            )
        })
}

/// Reads a field written as JSON text - `"[{\"title\": ...}]"` where a list
/// belongs - as the list or the object it says, at any depth.
fn decode_json_text(value: &mut Value) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if ((trimmed.starts_with('[') && trimmed.ends_with(']'))
                || (trimmed.starts_with('{') && trimmed.ends_with('}')))
                && let Ok(mut decoded) = serde_json::from_str::<Value>(trimmed)
            {
                decode_json_text(&mut decoded);
                *value = decoded;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(decode_json_text),
        Value::Object(map) => map.values_mut().for_each(decode_json_text),
        _ => {}
    }
}

/// The required fields of `tool` that `input` leaves out or empty.
fn missing(tool: &Value, input: &Value) -> Vec<String> {
    tool["input_schema"]["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|field| match &input[*field] {
            Value::Null => true,
            Value::String(text) => text.trim().is_empty(),
            Value::Array(items) => items.is_empty(),
            Value::Object(map) => map.is_empty(),
            _ => false,
        })
        .map(str::to_owned)
        .collect()
}

/// What an answer holds, field by field, for the error that says it fell
/// short: each field's name and kind.
fn given(input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return "no fields".to_owned();
    };
    if map.is_empty() {
        return "no fields".to_owned();
    }
    map.iter()
        .map(|(field, value)| {
            let kind = match value {
                Value::Array(items) => format!("a list of {}", items.len()),
                Value::String(_) => "text".to_owned(),
                Value::Object(_) => "an object".to_owned(),
                Value::Null => "null".to_owned(),
                _ => "a value".to_owned(),
            };
            format!("{field} as {kind}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The error's own sentence out of an API error body, or the body.
pub(crate) fn message_of(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or(value["detail"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim().to_owned()
}

fn outline_tool() -> Value {
    json!({
        "name": "outline",
        "description": "The video's outline: its look, its voice, and its chapters.",
        "input_schema": {
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "visual_style": {
                    "type": "string",
                    "description": "One line in English appended to every shot prompt so all shots look like one film: medium, lens, light, palette, grade."
                },
                "voice_style": {"type": "string", "description": "How the narrator sounds."},
                "chapters": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "summary": {"type": "string", "description": "What this chapter covers, in one sentence."},
                            "points": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "The facts, places and turns only this chapter covers."
                            },
                            "mood": {
                                "type": "string",
                                "description": "In English, this chapter's own light and palette within the film's style, unlike the chapters beside it."
                            }
                        },
                        "required": ["title", "summary", "points", "mood"]
                    }
                }
            },
            "required": ["title", "visual_style", "voice_style", "chapters"]
        }
    })
}

/// One scene, as the chapter and the rewrite tools both give it.
fn scene_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "narration": {"type": "string"},
            "voice": {
                "type": "string",
                "description": "In English, how the narrator says this scene: pace, feeling, emphasis."
            },
            "onScreenText": {
                "type": ["string", "null"],
                "description": "A short title shown over the scene's start, or null."
            },
            "shots": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": {"type": "string", "enum": ["image", "video"]},
                        "prompt": {"type": "string", "description": "In English, the picture."},
                        "motion": {
                            "type": "string",
                            "description": "For a video shot, in English, how the camera and the subject move. Empty for an image."
                        }
                    },
                    "required": ["kind", "prompt"]
                }
            }
        },
        "required": ["narration", "voice", "shots"]
    })
}

fn scenes_tool() -> Value {
    json!({
        "name": "scenes",
        "description": "One chapter's scenes, in order.",
        "input_schema": {
            "type": "object",
            "properties": {
                "scenes": {"type": "array", "items": scene_schema()},
                "ending": {"type": "string", "description": "One sentence: where this chapter leaves the viewer."}
            },
            "required": ["scenes", "ending"]
        }
    })
}

fn scene_tool() -> Value {
    json!({
        "name": "scene",
        "description": "The scene, written again.",
        "input_schema": scene_schema()
    })
}

fn music_tool() -> Value {
    json!({
        "name": "music",
        "description": "The music bed's description.",
        "input_schema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "At most 30 words, in English."}
            },
            "required": ["prompt"]
        }
    })
}

fn short_tool() -> Value {
    json!({
        "name": "short",
        "description": "The minute picked for a Short.",
        "input_schema": {
            "type": "object",
            "properties": {
                "scenes": {"type": "array", "items": {"type": "string"},
                           "description": "The scenes' ids, in order, in a row."},
                "hook": {"type": "string", "description": "At most six words."}
            },
            "required": ["scenes", "hook"]
        }
    })
}

fn suggest_tool() -> Value {
    json!({
        "name": "suggested_shot",
        "description": "Another picture for the shot.",
        "input_schema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "The image prompt."},
                "motion": {"type": "string", "description": "For a video clip: how it moves."}
            },
            "required": ["prompt"]
        }
    })
}

fn revise_tool() -> Value {
    json!({
        "name": "revised_shots",
        "description": "New pictures for the shots that repeated earlier ones.",
        "input_schema": {
            "type": "object",
            "properties": {
                "shots": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "prompt": {"type": "string"},
                            "motion": {"type": "string"}
                        },
                        "required": ["id", "prompt"]
                    }
                }
            },
            "required": ["shots"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_music_is_asked_for_in_english_from_the_moods() {
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"جريمة وعقاب","chapters":[
                {"title":"Uno","mood":"cold dawn blues","scenes":[{"narration":"Una noche.","seconds":6,"shots":[{"prompt":"x"}]}]},
                {"title":"Dos","mood":"warm amber dusk","scenes":[{"narration":"Otra.","seconds":6,"shots":[{"prompt":"y"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        let ask = music_ask(&board);
        assert!(ask.contains("cold dawn blues; warm amber dusk"), "{ask}");
        assert!(ask.contains("Una noche."));
        assert!(ask.contains("in English"));
        assert!(plain_music_prompt().contains("no vocals"));
    }
    use crate::storyboard::Shot;

    fn plan() -> ChapterPlan {
        ChapterPlan {
            seconds: 20.0,
            scenes: vec![
                ScenePlan {
                    seconds: 12.0,
                    words: 29,
                    shots: vec![(ShotKind::Video, 6.0), (ShotKind::Image, 6.0)],
                },
                ScenePlan {
                    seconds: 8.0,
                    words: 19,
                    shots: vec![(ShotKind::Image, 8.0)],
                },
            ],
        }
    }

    fn scene(shots: &[(&str, &str)]) -> Scene {
        Scene {
            narration: "words".into(),
            shots: shots
                .iter()
                .map(|(kind, prompt)| Shot {
                    kind: if *kind == "video" {
                        ShotKind::Video
                    } else {
                        ShotKind::Image
                    },
                    prompt: (*prompt).into(),
                    motion: "push in".into(),
                    seconds: 0.0,
                })
                .collect(),
            ..Scene::default()
        }
    }

    #[test]
    fn the_answer_is_the_tool_call_after_any_thinking_or_words() {
        let reply = json!({"content": [
            {"type": "thinking", "thinking": ""},
            {"type": "text", "text": "Here is the outline."},
            {"type": "tool_use", "id": "toolu_1", "name": "outline", "input": {"title": "x"}}
        ], "stop_reason": "tool_use"});
        assert_eq!(
            tool_call(&reply),
            Some(("toolu_1".to_owned(), json!({"title": "x"})))
        );
        let words = json!({"content": [{"type": "text", "text": "No tool."}]});
        assert_eq!(tool_call(&words), None);
    }

    #[test]
    fn a_list_written_as_json_text_is_read_as_the_list() {
        let mut input = json!({
            "title": "The fall",
            "chapters": "[{\"title\": \"One\", \"points\": \"[\\\"a\\\"]\"}]",
            "note": "[not json"
        });
        decode_json_text(&mut input);
        assert_eq!(input["chapters"][0]["title"], "One");
        assert_eq!(input["chapters"][0]["points"], json!(["a"]));
        assert_eq!(input["note"], "[not json");
        assert_eq!(input["title"], "The fall");
    }

    #[test]
    fn an_answer_short_of_what_the_tool_requires_says_what_is_missing() {
        let tool = outline_tool();
        let short =
            json!({"title": "T", "visual_style": "film", "voice_style": "", "chapters": []});
        assert_eq!(missing(&tool, &short), vec!["voice_style", "chapters"]);
        assert_eq!(
            given(&short),
            "chapters as a list of 0, title as text, visual_style as text, voice_style as text"
        );
        let whole = json!({"title": "T", "visual_style": "film", "voice_style": "calm",
                           "chapters": [{"title": "One"}]});
        assert!(missing(&tool, &whole).is_empty());
    }

    #[test]
    fn an_api_error_reads_as_its_sentence() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(message_of(body), "invalid x-api-key");
        assert_eq!(message_of("Bad gateway"), "Bad gateway");
    }

    #[test]
    fn no_key_is_said_before_any_call() {
        let brief = Brief {
            topic: "t".into(),
            minutes: 1.0,
            ..Brief::default()
        };
        let error = write(&brief, " ", DEFAULT_MODEL, |_| {}).unwrap_err();
        assert!(error.contains("API key"));
    }

    #[test]
    fn a_chapter_off_its_shape_is_told_how() {
        let fits = vec![
            scene(&[("video", "a"), ("image", "b")]),
            scene(&[("image", "c")]),
        ];
        assert_eq!(misfit(&fits, &plan()), None);
        let short = vec![scene(&[("video", "a"), ("image", "b")])];
        assert_eq!(
            misfit(&short, &plan()).unwrap(),
            "1 scenes where 2 were asked for"
        );
        let wrong = vec![scene(&[("image", "a")]), scene(&[("image", "c")])];
        assert!(
            misfit(&wrong, &plan())
                .unwrap()
                .starts_with("scene 1 has 1 shots")
        );
    }

    #[test]
    fn a_chapter_is_held_to_its_shape() {
        let came = vec![
            scene(&[("image", "a"), ("image", "b"), ("video", "extra")]),
            scene(&[]),
            scene(&[("image", "spare scene")]),
        ];
        let kept = conform(came, &plan());
        // The second came back without a picture and the third was not
        // asked for.
        assert_eq!(kept.len(), 1);
        let first = &kept[0];
        assert_eq!(first.seconds, 12.0);
        assert_eq!(first.shots.len(), 2);
        assert_eq!(first.shots[0].kind, ShotKind::Video);
        assert_eq!(first.shots[1].kind, ShotKind::Image);
        assert!(first.shots[1].motion.is_empty(), "a still has no motion");
        assert_eq!(first.shots[0].seconds, 6.0);

        let thin = conform(vec![scene(&[("video", "a")])], &plan());
        assert_eq!(thin[0].shots.len(), 2);
        assert!(thin[0].shots[1].prompt.starts_with("a, a different angle"));
    }

    #[test]
    fn a_chapter_is_shown_its_shape_and_what_to_keep_clear_of() {
        let mut board = Storyboard {
            title: "T".into(),
            aspect: "16:9".into(),
            chapters: vec![
                Chapter {
                    title: "One".into(),
                    summary: "the founding".into(),
                    scenes: vec![scene(&[("image", "caravan crossing red dunes at dawn")])],
                    ..Chapter::default()
                },
                Chapter {
                    title: "Two".into(),
                    summary: "the fall".into(),
                    mood: "ash grey".into(),
                    ..Chapter::default()
                },
            ],
            ..Storyboard::default()
        };
        board.tidy().ok();
        let brief = Brief {
            topic: "lost cities".into(),
            minutes: 3.0,
            language: "Arabic".into(),
            ..Brief::default()
        };
        let prompt = chapter_prompt(&board, &brief, 1, &plan(), "the siege; the fire", "x");
        assert!(prompt.contains("Write chapter 2 of 2, \"Two\": the fall"));
        assert!(prompt.contains("Exactly 2 scenes"));
        assert!(prompt.contains("Scene 1: 12 s, about 29 words; shots: video 6.0 s, image 6.0 s"));
        assert!(prompt.contains("It covers: the siege; the fire"));
        assert!(prompt.contains("Its mood: ash grey"));
        assert!(prompt.contains("- caravan crossing red dunes at dawn"));
        assert!(prompt.contains("1. One: the founding"));
    }

    #[test]
    fn a_shot_is_asked_for_anew_in_its_scene_and_light() {
        let mut board = Storyboard {
            title: "T".into(),
            aspect: "16:9".into(),
            chapters: vec![Chapter {
                title: "One".into(),
                mood: "dusk violet".into(),
                scenes: vec![
                    scene(&[("image", "caravan crossing red dunes"), ("video", "a gate")]),
                    scene(&[("image", "a market at noon")]),
                ],
                ..Chapter::default()
            }],
            ..Storyboard::default()
        };
        board.tidy().ok();
        let brief = Brief {
            topic: "lost cities".into(),
            minutes: 3.0,
            ..Brief::default()
        };
        let prompt = suggest_prompt(&board, &brief, 0, 0, 1).unwrap();
        assert!(prompt.contains("its light and palette: dusk violet"));
        assert!(prompt.contains("- shot 1: caravan crossing red dunes"));
        assert!(prompt.contains("Shot 2 of this scene is a video clip"));
        assert!(prompt.contains("Its picture now: \"a gate\""));
        // Other scenes' pictures are kept clear of, this scene's are not
        // listed twice.
        assert!(prompt.contains("- a market at noon"));
        assert!(suggest_prompt(&board, &brief, 0, 0, 9).is_err());
        let error = suggest_shot(&board, &brief, 0, 0, 1, " ", DEFAULT_MODEL).unwrap_err();
        assert!(error.contains("API key"));
    }

    #[test]
    fn the_writer_is_shown_every_scene_to_pick_a_short_from() {
        let mut board = Storyboard {
            title: "Lost cities".into(),
            chapters: vec![Chapter {
                title: "One".into(),
                scenes: vec![scene(&[("image", "a")]), scene(&[("image", "b")])],
                ..Chapter::default()
            }],
            ..Storyboard::default()
        };
        board.tidy().ok();
        let brief = Brief {
            topic: "lost cities".into(),
            minutes: 3.0,
            ..Brief::default()
        };
        let prompt = short_prompt(&board, &brief);
        assert!(prompt.contains("- c01s01 ("));
        assert!(prompt.contains("- c01s02 ("));
        assert!(prompt.contains("60 seconds at most"));
        let error = pick_short(&board, &brief, "", DEFAULT_MODEL).unwrap_err();
        assert!(error.contains("API key"));
    }

    #[test]
    fn repeated_shots_are_sent_back_and_replaced() {
        let mut board = Storyboard {
            aspect: "16:9".into(),
            chapters: vec![
                Chapter {
                    scenes: vec![scene(&[(
                        "image",
                        "aerial view of red sandstone ruins beside a dry riverbed",
                    )])],
                    ..Chapter::default()
                },
                Chapter {
                    scenes: vec![scene(&[
                        (
                            "video",
                            "wide aerial over red sandstone ruins near a dry riverbed",
                        ),
                        ("image", "a potter's hands shaping wet clay on a wheel"),
                    ])],
                    ..Chapter::default()
                },
            ],
            ..Storyboard::default()
        };
        board.tidy().unwrap();
        let repeated = repeats_in(&board, 1);
        assert_eq!(repeated.len(), 1);
        assert_eq!(repeated[0].0, "c02s01_1");
        assert!(
            repeats_in(&board, 0).is_empty(),
            "the first use is not the copy"
        );

        apply_revisions(
            &mut board,
            &json!({"shots": [
                {"id": "c02s01_1", "prompt": "torchlit tunnel under the citadel", "motion": "slow dolly forward"},
                {"id": "c09s09_9", "prompt": "unknown"}
            ]}),
        );
        let shot = &board.chapters[1].scenes[0].shots[0];
        assert_eq!(shot.prompt, "torchlit tunnel under the citadel");
        assert_eq!(shot.motion, "slow dolly forward");
        assert!(board.repeats().is_empty());
    }
}
