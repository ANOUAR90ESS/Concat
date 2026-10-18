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

use serde_json::{Value, json};

use crate::storyboard::{Chapter, Storyboard, Style};

/// The Messages API.
const API: &str = "https://api.anthropic.com/v1/messages";
/// The model that writes unless the caller names another.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Narration speed in words a second, for sizing a chapter's words from
/// its seconds.
const WORDS_PER_SECOND: f64 = 2.4;
/// About how long a chapter runs; a ten-minute video is four of them.
const CHAPTER_SECONDS: f64 = 150.0;

const SYSTEM: &str = "You write long-form videos for YouTube: documentaries, explainers, stories.
Narration is written to be heard, not read: short sentences, concrete images, a hook in the
first ten seconds, a question or a turn that pulls the viewer into the next chapter. Never pad.

Shot prompts are for a text-to-video model and are always in English, whatever the narration's
language. Each prompt is one shot of 4 to 8 seconds: subject, action, setting, camera move,
framing, light. No text, logos or captions in the picture, no real people's names. Describe what
is there; do not tell the model what to avoid.";

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

    let mut brief_text = format!(
        "Topic: {}\nLength: about {} minutes\nNarration language: {}\nAspect ratio: {}\n",
        brief.topic.trim(),
        brief.minutes,
        brief.language,
        brief.aspect
    );
    if !brief.notes.trim().is_empty() {
        brief_text += &format!("Notes from the producer: {}\n", brief.notes.trim());
    }
    let wanted = ((brief.minutes * 60.0 / CHAPTER_SECONDS).round() as usize).max(3);

    progress(Writing::Outline);
    let outline = ask(
        &outline_tool(),
        format!(
            "{brief_text}\nOutline the video in about {wanted} chapters whose seconds add up to the length."
        ),
    )?;
    let chapters = outline["chapters"].as_array().cloned().unwrap_or_default();
    if chapters.is_empty() {
        return Err("the writer returned an outline with no chapters".to_owned());
    }
    let visual = text(&outline["visual_style"]);
    let voice = text(&outline["voice_style"]);
    let summary: String = chapters
        .iter()
        .enumerate()
        .map(|(i, chapter)| {
            format!(
                "{}. {} ({} s): {}\n",
                i + 1,
                text(&chapter["title"]),
                seconds(&chapter["seconds"]),
                text(&chapter["summary"])
            )
        })
        .collect();

    let mut board = Storyboard {
        title: text(&outline["title"]),
        language: brief.language.clone(),
        aspect: brief.aspect.clone(),
        style: Style {
            visual: visual.clone(),
            voice: voice.clone(),
        },
        chapters: Vec::new(),
    };
    let mut ending = "This is the first chapter: open with the hook.".to_owned();
    for (i, chapter) in chapters.iter().enumerate() {
        progress(Writing::Chapter {
            n: i + 1,
            of: chapters.len(),
        });
        let length = seconds(&chapter["seconds"]);
        let words = (length * WORDS_PER_SECOND).round();
        let got = ask(
            &scenes_tool(),
            format!(
                "{brief_text}\nThe outline:\n{summary}\nVisual style: {visual}\nNarrator: {voice}\n\n\
                 Write chapter {n}, \"{title}\": about {length} seconds, so about {words} words of \
                 narration in {language}, split into scenes of 6 to 20 seconds. Give each scene one \
                 shot per 4 to 8 seconds of its length. Where the previous chapter left off: {ending}",
                n = i + 1,
                title = text(&chapter["title"]),
                language = brief.language,
            ),
        )?;
        let scenes = serde_json::from_value(got["scenes"].clone())
            .map_err(|error| format!("chapter {}: {error}", i + 1))?;
        board.chapters.push(Chapter {
            title: text(&chapter["title"]),
            scenes,
        });
        ending = text(&got["ending"]);
    }
    board.tidy()?;
    Ok(board)
}

/// One Messages API call answered through `tool`: the tool's input.
fn call(key: &str, model: &str, tool: &Value, prompt: &str) -> Result<Value, String> {
    let name = tool["name"].as_str().unwrap_or_default();
    let body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": SYSTEM,
        "tools": [tool],
        "tool_choice": {"type": "tool", "name": name},
        "messages": [{"role": "user", "content": prompt}],
    });
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(600))
        .build();
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
    reply["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|block| block["type"] == "tool_use"))
        .map(|block| block["input"].clone())
        .ok_or_else(|| format!("Claude did not answer with the {name}"))
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

fn seconds(value: &Value) -> f64 {
    value
        .as_f64()
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(CHAPTER_SECONDS)
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
                            "summary": {"type": "string"},
                            "seconds": {"type": "number"}
                        },
                        "required": ["title", "summary", "seconds"]
                    }
                }
            },
            "required": ["title", "visual_style", "voice_style", "chapters"]
        }
    })
}

fn scenes_tool() -> Value {
    json!({
        "name": "scenes",
        "description": "One chapter's scenes, in order.",
        "input_schema": {
            "type": "object",
            "properties": {
                "scenes": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "narration": {"type": "string"},
                            "onScreenText": {
                                "type": ["string", "null"],
                                "description": "A short title shown over the scene's start, or null."
                            },
                            "seconds": {"type": "number"},
                            "shots": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {"prompt": {"type": "string"}},
                                    "required": ["prompt"]
                                }
                            }
                        },
                        "required": ["narration", "seconds", "shots"]
                    }
                },
                "ending": {"type": "string", "description": "One sentence: where this chapter leaves the viewer."}
            },
            "required": ["scenes", "ending"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
