// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The writer as a motion designer: for each shot the script marks as an
//! explainer, a call of its own designs the animation - what is on the
//! canvas, how each thing comes on and moves, on which word of the
//! narration - to a design system that keeps every video's explainers of
//! one hand and quality. What comes back is checked; what is wrong is
//! sent back to be mended; then two of its frames are drawn and shown to
//! the designer, who mends what it sees - words over words, a label cut
//! by the edge, a crowded or a lopsided frame - before it is kept.

use serde_json::{Value, json};

use crate::explainer::{self, Explainer};
use crate::storyboard::{ShotKind, Storyboard};
use crate::writer::{Brief, call_as};

/// The designer's standing brief.
const DIRECTOR: &str = "You are the motion designer of an educational channel that competes with \
the best explainer channels on YouTube - Kurzgesagt, Vox, 3Blue1Brown, TED-Ed. You design \
animated explainers: a diagram that builds itself in step with the narration, so the viewer \
sees the idea while they hear it. You never decorate: every element shows something the \
narration says, and the last frame tells the whole idea at a glance.

You answer in JSON the app draws exactly as you write it, on a canvas in pixels, origin at \
the top left, y down. You think like a designer who codes: a grid, a hierarchy, a palette, \
whitespace, one movement at a time.";

/// The design system every explainer keeps to.
const DESIGN: &str = "DESIGN SYSTEM
- Safe area: keep everything 80 px inside the edges. Keep the lowest fifth of the canvas free \
of words: captions sit there.
- One idea per explainer, built up in three to six beats. Begin with one or two elements and \
add as the narration goes; never show everything at once.
- Hierarchy: at most one heading (52 to 72 px, weight 800, face display) near the top; labels \
34 to 44 px (weight 600); figures 56 to 110 px (face display, weight 800). Nothing under 30 \
px. At most 12 elements and about 25 words on screen.
- Palette: a dark two-tone background from the chapter's palette (two colours a few shades \
apart); one accent for the thing that matters, one second colour for its counterpart, \
#FFFFFF for words and #CBD5E1 for quieter ones. Fills under 30% opacity (#rrggbb4d) for \
plates and glows. Strong contrast always.
- Shapes: rounded rects (r 16 to 28) as plates and boxes; strokes 5 to 8 px; arrows 6 to 8 px; \
icons 110 to 260 px with width 1.5 to 2. Align things on a grid; equal gaps; balance the \
frame around its middle.
- Diagram patterns: a process - boxes or icons joined by arrows in reading order; a \
comparison - bars or circles sized to the true ratio, labelled with their figures; a cycle - \
icons on a ring with curved arrows; a route or a flow - a path drawn on, or a dot moved along; \
cause and effect - the cause, an arrow, the effect; parts of a thing - the thing and labels \
with short lines to them; a timeline - a line with dots and dates; scale - a small thing \
beside a big one, the camera zooming from one to the other.
- Motion: every element enters - pop for objects and plates, draw for lines, arrows, paths, \
icons and outlines, type for headings and labels, grow from the bottom for bars and from the \
start for a line of progress, fade_in for what supports. Then show change: move, scale, \
color, count, pulse to point. One movement at a time, 0.2 to 0.6 s apart; ease out for \
entrances, in_out for moves. Use the camera seldom (zoom at most 1.5) and never so that words \
leave the frame.
- Timing: put each entrance on the word of the narration that names it - its cue, a word or \
two copied exactly as written in the narration - and give its at as well, in seconds, for \
when the cue is not found. The first element enters within 0.3 s; everything has entered by \
four fifths of the shot, so the full picture holds before the cut.
- Right-to-left narration (Arabic, Hebrew): words read from the right, so lay a sequence out \
from right to left - the first step on the right, time running leftwards; a text's align \
start is its right end - prefer middle. Write the words in the narration's language and \
script, numbers as digits.";

/// What the JSON holds, for the designer.
const VOCABULARY: &str = "ELEMENTS (each needs a unique id)
- text: text (\\n for a new line), x, y (the first line's baseline), size, weight, align \
(start|middle|end), font (sans|display|serif|mono), fill.
- rect: x, y (top left), w, h, r (corners), fill, stroke, width.
- circle: x, y (centre), r, fill, stroke, width.
- line: points [[x,y],...], stroke, width, dash (true for a dashed line).
- arrow: points [[x,y],[x,y]] start to end, curve (-0.5..0.5, how far it bows), stroke, width.
- path: d (SVG path data in canvas pixels), fill, stroke, width.
- icon: icon (a name from the list), x, y (centre), size, stroke, width.
- group: x, y and children placed from that origin; steps on the group move them together.
Any element may have opacity (0..1 at rest).

STEPS: target (an element's id, or camera), effect, at, cue, duration (0 = the effect's own), \
ease (out|in|in_out|linear|back|spring) and what the effect needs:
- fade_in, fade_out, pop, draw, type, pulse, shake.
- grow: edge (left|right|top|bottom|center).
- move: to [x, y] (where its x, y go).
- scale: value. rotate: value (degrees). color: color (#rrggbb).
- count: from, value (the text's number counts from one to the other, written as the text \
writes it).
- zoom (camera only): to [x, y, zoom] - the canvas point to centre and how near (1 = whole).";

/// Directs every explainer of chapter `i` not directed yet. An explainer
/// that cannot be directed is left to be made as a still of its prompt.
pub fn direct_chapter(
    board: &mut Storyboard,
    brief: &Brief,
    i: usize,
    key: &str,
    model: &str,
    progress: &mut dyn FnMut(usize, usize),
) {
    let pending: Vec<(usize, usize)> = board.chapters[i]
        .scenes
        .iter()
        .enumerate()
        .flat_map(|(s, scene)| {
            scene
                .shots
                .iter()
                .enumerate()
                .filter(|(_, shot)| shot.kind == ShotKind::Explainer && shot.explainer.is_none())
                .map(move |(n, _)| (s, n))
        })
        .collect();
    for (k, (s, n)) in pending.iter().enumerate() {
        progress(k + 1, pending.len());
        match direct(board, brief, i, *s, *n, "", key, model) {
            Ok(directed) => board.chapters[i].scenes[*s].shots[*n].explainer = Some(directed),
            Err(error) => log::warn!(
                "{}: the explainer could not be directed ({error}); made as a still",
                crate::storyboard::shot_stem(&board.chapters[i].scenes[*s].id, *n)
            ),
        }
    }
}

/// Designs shot `shot` of scene `scene` of chapter `chapter` as an
/// explainer, its prompt saying what it is to show; `note` is what the
/// producer wants different. Checked, mended, drawn and looked at.
#[allow(clippy::too_many_arguments)]
pub fn direct(
    board: &Storyboard,
    brief: &Brief,
    chapter: usize,
    scene: usize,
    shot: usize,
    note: &str,
    key: &str,
    model: &str,
) -> Result<Explainer, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to direct an explainer".to_owned());
    }
    let prompt = director_prompt(board, brief, chapter, scene, shot, note)?;
    let canvas = explainer::canvas(&board.aspect);
    let ask = |prompt: &str, pictures: &[Vec<u8>], tool: &Value| {
        crate::providers::retry(|| call_as(key, model, DIRECTOR, &[], tool, prompt, pictures))
    };
    let mut directed = read(&ask(&prompt, &[], &explainer_tool())?)?;
    // What is wrong, sent back once to be mended; what is still wrong
    // after that is not drawn.
    let problems = directed.problems(canvas);
    if !problems.is_empty() {
        log::info!("explainer: mending {} problems", problems.len());
        let mended = ask(
            &format!(
                "{prompt}\n\nYour explainer:\n{}\n\nIt has these problems:\n- {}\nCall the tool \
                 again with the whole explainer, mended.",
                serde_json::to_string(&directed).unwrap_or_default(),
                problems.join("\n- ")
            ),
            &[],
            &explainer_tool(),
        )?;
        directed = read(&mended)?;
        let problems = directed.problems(canvas);
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
    }
    // Two frames drawn and looked at: the middle of the build and the
    // finished picture.
    let seconds = shot_seconds(board, chapter, scene, shot);
    let timed = directed.cued(&|_| None);
    let size = match board.aspect.as_str() {
        "9:16" => (540, 960),
        "1:1" => (720, 720),
        _ => (960, 540),
    };
    let frames: Result<Vec<Vec<u8>>, String> = [seconds * 0.45, (seconds - 0.2).max(0.5)]
        .iter()
        .map(|t| explainer::still_png(&timed, canvas, size, *t))
        .collect();
    match frames {
        Ok(frames) => {
            let review = ask(
                &format!(
                    "{prompt}\n\nYou designed this explainer:\n{}\n\nThe two pictures are it drawn, \
                     at {:.1} s and at {:.1} s (the end). Look at them as the channel's art \
                     director would. Check: words over words or over shapes; anything cut by the \
                     edge or by the camera; words too small or low in contrast; a crowded, empty \
                     or lopsided frame; a diagram that does not show what the narration says; \
                     anything in the lowest fifth where the captions go. If it is right, answer \
                     ok. If not, list what is wrong and give the whole explainer mended.",
                    serde_json::to_string(&directed).unwrap_or_default(),
                    seconds * 0.45,
                    (seconds - 0.2).max(0.5)
                ),
                &frames,
                &review_tool(),
            );
            match review {
                Ok(answer) if !answer["ok"].as_bool().unwrap_or(true) => {
                    let issues = crate::writer::list_text(&answer["issues"]);
                    log::info!("explainer: the review mends: {issues}");
                    match read(&answer["explainer"]) {
                        Ok(mended) if mended.problems(canvas).is_empty() => directed = mended,
                        Ok(_) | Err(_) => {
                            log::warn!("explainer: the mended design did not hold; the first kept")
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => log::warn!("explainer: the review failed: {error}"),
            }
        }
        Err(error) => log::warn!("explainer: its frames could not be drawn to look at: {error}"),
    }
    directed.settle();
    Ok(directed)
}

/// An explainer read from a tool's answer.
fn read(value: &Value) -> Result<Explainer, String> {
    let mut value = value.clone();
    if let Some(inner) = value.get("explainer").filter(|v| v.is_object()) {
        value = inner.clone();
    }
    let mut directed: Explainer =
        serde_json::from_value(value).map_err(|error| format!("the explainer: {error}"))?;
    directed.settle();
    Ok(directed)
}

/// How long the shot is planned to hold, in seconds.
fn shot_seconds(board: &Storyboard, chapter: usize, scene: usize, shot: usize) -> f64 {
    let scene = &board.chapters[chapter].scenes[scene];
    let planned = scene.shots.get(shot).map_or(0.0, |s| s.seconds);
    if planned > 0.5 {
        planned
    } else {
        (scene.seconds / scene.shots.len().max(1) as f64).max(3.0)
    }
}

/// The designer's brief for one explainer.
fn director_prompt(
    board: &Storyboard,
    brief: &Brief,
    chapter: usize,
    scene: usize,
    shot: usize,
    note: &str,
) -> Result<String, String> {
    let the_chapter = board
        .chapters
        .get(chapter)
        .ok_or_else(|| "no such chapter".to_owned())?;
    let the_scene = the_chapter
        .scenes
        .get(scene)
        .ok_or_else(|| "no such scene".to_owned())?;
    let the_shot = the_scene
        .shots
        .get(shot)
        .ok_or_else(|| "no such shot".to_owned())?;
    let (w, h) = explainer::canvas(&board.aspect);
    let seconds = shot_seconds(board, chapter, scene, shot);
    let before: String = the_scene
        .shots
        .iter()
        .take(shot)
        .map(|other| format!("- {:?}: {}\n", other.kind, other.prompt.trim()).to_lowercase())
        .collect();
    let mut prompt = format!(
        "The video: \"{}\", on {}. Narration in {}. {}\n\
         Chapter \"{}\": {}\nIts light and palette: {}\n\n\
         The scene's narration, said while this shot is on screen:\n\"{}\"\n\n\
         What this explainer is to show: {}\n\
         The shot lasts about {seconds:.1} seconds.\n",
        board.title.trim(),
        brief.topic.trim(),
        brief.language,
        brief.level.brief(),
        the_chapter.title.trim(),
        the_chapter.summary.trim(),
        if the_chapter.mood.trim().is_empty() {
            board.style.visual.trim()
        } else {
            the_chapter.mood.trim()
        },
        the_scene.narration.trim(),
        the_shot.prompt.trim(),
    );
    if !before.is_empty() {
        prompt += &format!("The shots before it in the scene:\n{before}");
    }
    if !note.trim().is_empty() {
        prompt += &format!("What the producer wants different: {}\n", note.trim());
    }
    prompt += &format!(
        "\nThe canvas is {w} by {h} pixels.\n\n{DESIGN}\n\n{VOCABULARY}\n\nICONS: {}\n\n\
         Design the explainer.",
        explainer::icon_names()
    );
    Ok(prompt)
}

/// The JSON schema of an explainer, for the tool.
fn explainer_schema() -> Value {
    let point = json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2});
    let mut element = json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "kind": {"type": "string", "enum": ["text", "rect", "circle", "line", "arrow", "path", "icon", "group"]},
            "x": {"type": "number"}, "y": {"type": "number"},
            "w": {"type": "number"}, "h": {"type": "number"}, "r": {"type": "number"},
            "points": {"type": "array", "items": point},
            "curve": {"type": "number"},
            "d": {"type": "string"},
            "text": {"type": "string"},
            "size": {"type": "number"},
            "weight": {"type": "number"},
            "align": {"type": "string", "enum": ["start", "middle", "end"]},
            "font": {"type": "string", "enum": ["sans", "display", "serif", "mono"]},
            "icon": {"type": "string"},
            "fill": {"type": "string"},
            "stroke": {"type": "string"},
            "width": {"type": "number"},
            "dash": {"type": "boolean"},
            "opacity": {"type": "number"}
        },
        "required": ["id", "kind"]
    });
    element["properties"]["children"] = json!({"type": "array", "items": element.clone()});
    json!({
        "type": "object",
        "properties": {
            "background": {"type": "array", "items": {"type": "string"}, "description": "One or two #rrggbb colours, top to bottom."},
            "elements": {"type": "array", "items": element},
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "target": {"type": "string"},
                        "effect": {"type": "string", "enum": ["fade_in", "fade_out", "pop", "grow", "draw", "type", "move", "scale", "rotate", "color", "count", "pulse", "shake", "zoom"]},
                        "at": {"type": "number"},
                        "cue": {"type": "string"},
                        "duration": {"type": "number"},
                        "ease": {"type": "string", "enum": ["out", "in", "in_out", "linear", "back", "spring"]},
                        "to": {"type": "array", "items": {"type": "number"}},
                        "value": {"type": "number"},
                        "from": {"type": "number"},
                        "color": {"type": "string"},
                        "edge": {"type": "string", "enum": ["left", "right", "top", "bottom", "center"]}
                    },
                    "required": ["target", "effect", "at"]
                }
            }
        },
        "required": ["background", "elements", "steps"]
    })
}

fn explainer_tool() -> Value {
    json!({
        "name": "explainer",
        "description": "The animated explainer: its background, its elements and its steps.",
        "input_schema": explainer_schema()
    })
}

fn review_tool() -> Value {
    json!({
        "name": "review",
        "description": "The art director's verdict on the drawn explainer.",
        "input_schema": {
            "type": "object",
            "properties": {
                "ok": {"type": "boolean", "description": "True when nothing needs mending."},
                "issues": {"type": "array", "items": {"type": "string"}},
                "explainer": {
                    "type": ["object", "null"],
                    "description": "When not ok, the whole explainer mended; null when ok.",
                    "properties": explainer_schema()["properties"].clone()
                }
            },
            "required": ["ok", "issues"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The designer is told the narration, the idea, the shot's length,
    /// the canvas, the design system and every icon.
    #[test]
    fn the_designer_is_briefed_on_the_shot() {
        let board: Storyboard = serde_json::from_value(json!({
            "title": "Light", "aspect": "16:9",
            "chapters": [{"title": "Speed", "summary": "how fast light is", "mood": "cold night blues",
                "scenes": [{"narration": "Light takes eight minutes to reach us.", "seconds": 8,
                    "shots": [{"prompt": "the sun"}, {"kind": "explainer", "prompt": "the sun, an arrow to the earth, 8 min", "seconds": 5}]}]}]
        }))
        .unwrap();
        let brief = Brief {
            topic: "light".into(),
            language: "Arabic".into(),
            ..Brief::default()
        };
        let prompt = director_prompt(&board, &brief, 0, 0, 1, "bigger earth").unwrap();
        for expected in [
            "Light takes eight minutes",
            "an arrow to the earth",
            "about 5.0 seconds",
            "1920 by 1080",
            "cold night blues",
            "DESIGN SYSTEM",
            "bigger earth",
            "Right-to-left",
            "telescope",
        ] {
            assert!(prompt.contains(expected), "{expected}");
        }
    }

    /// An explainer is read from the tool's answer, or from inside a
    /// review's.
    #[test]
    fn an_explainer_is_read_from_an_answer() {
        let answer = json!({"background": ["#000000"], "elements": [{"id": "a", "kind": "circle", "x": 1, "y": 1, "r": 5}], "steps": []});
        assert_eq!(read(&answer).unwrap().elements.len(), 1);
        let review = json!({"ok": false, "issues": [], "explainer": answer});
        assert_eq!(read(&review).unwrap().elements[0].id, "a");
        assert!(read(&json!({"elements": "none"})).is_err());
    }
}
