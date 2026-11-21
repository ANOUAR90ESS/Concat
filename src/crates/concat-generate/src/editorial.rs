// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Deeper analysis: what the writer works out before it writes and checks
//! after, when the person asks for a video that explains rather than
//! describes. Two calls around the writing `writer::write` already does:
//!
//! - the **analysis**, after the research and before the outline: first
//!   what kind of subject it is and so what kind of explanation it needs -
//!   historical, scientific, causal, philosophical, a tutorial, a story -
//!   then, of the questions that matter, those that apply to it: the
//!   central idea, why it matters, how it works and why, the assumptions,
//!   evidence, distinctions, limits and other readings, and what the
//!   viewer should understand after. Then the opening and the beats that
//!   suit that kind of subject, and the kind of pictures that explain it.
//!   Nothing in it is about any one topic: the writer judges what applies,
//!   and leaves the rest empty. Every call after it is told it.
//! - the **review**, once the script is written: an editor's read of the
//!   whole for depth, accuracy against the research, logic, unsupported
//!   claims, repetition and generic lines, and whether the ending follows.
//!   It names the scenes; the ones it holds against are written again with
//!   its note, once, before anything is made.
//!
//! Both are kept with the script, so the person can read why it says what
//! it says.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::storyboard::Storyboard;

/// What the video sets out to explain, worked out before it is written.
/// Every field but the approach and the question may be empty: not every
/// question applies to every subject.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
// Named as the writer's tool names them, which is how they are kept.
#[serde(default)]
pub struct Analysis {
    /// What kind of explanation the subject needs - historical,
    /// scientific, causal, political, economic, philosophical,
    /// comparative, investigative, a story, a tutorial, a documentary -
    /// and why.
    pub approach: String,
    /// The question the video answers, as a viewer would ask it.
    pub central_question: String,
    /// The answer it argues for.
    pub thesis: String,
    /// Why it matters to the viewer.
    pub why_it_matters: String,
    /// How it works and what causes it: why, not only what.
    pub mechanisms: Vec<String>,
    /// The assumptions it rests on.
    pub assumptions: Vec<String>,
    /// Distinctions people commonly miss, and what they get wrong.
    pub misunderstood: Vec<String>,
    /// The evidence and examples that carry the argument.
    pub evidence: Vec<String>,
    /// The strongest objections or other readings, fairly put.
    pub counterarguments: Vec<String>,
    /// Where the evidence or the idea stops.
    pub limitations: Vec<String>,
    /// What the viewer should understand after watching.
    pub takeaway: String,
    /// The conclusion the evidence supports, and no further.
    pub conclusion: String,
    /// How it opens, as suits the kind of subject: a paradox, a
    /// surprising observation, a decisive event, a contradiction, a
    /// problem, a character.
    pub opening: String,
    /// The beats that tell it, in order, each with what it is for.
    pub beats: Vec<String>,
    /// The kind of pictures that explain it - cinematic scenes,
    /// illustration, characters, charts, maps, reconstructions, diagrams,
    /// metaphors, demonstrations, documentary - and what they must show.
    pub visuals: String,
}

/// One thing the review holds against the script.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
// Named as the writer's tool names them, which is how they are kept.
#[serde(default)]
pub struct Issue {
    /// The scene's id, as the script has it; empty for the whole.
    pub scene: String,
    /// One of [`KINDS`].
    pub kind: String,
    /// What is wrong, specifically.
    pub problem: String,
    /// What would put it right.
    pub fix: String,
}

/// What the review can hold against a scene.
pub const KINDS: [&str; 13] = [
    "accuracy",
    "unsupported",
    "logic",
    "depth",
    "missing",
    "conclusion",
    "precision",
    "clarity",
    "purpose",
    "narrative",
    "visuals",
    "repetition",
    "generic",
];

/// The kinds that send a scene back to be written again even in a script
/// the review passes: wrong, unsupported, incoherent, shallow or
/// imprecise is not made. The rest are mended when the script fails.
const SERIOUS: [&str; 8] = [
    "accuracy",
    "unsupported",
    "logic",
    "depth",
    "missing",
    "conclusion",
    "precision",
    "purpose",
];

/// The most scenes one review writes again: the worst first.
pub const MOST_FIXES: usize = 6;

/// The review of the written script.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
// Named as the writer's tool names them, which is how they are kept.
#[serde(default)]
pub struct Review {
    /// The script holds up: deep, accurate and coherent enough to make.
    pub passed: bool,
    /// The ending follows from the evidence the video gives.
    pub conclusion_follows: bool,
    /// What it holds against which scenes.
    pub issues: Vec<Issue>,
    /// The editor's note on the whole, in a few lines.
    pub summary: String,
    /// The scenes written again from it.
    pub revised: Vec<String>,
}

/// The analysis and the review, kept with the script.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Editorial {
    /// Worked out before writing.
    pub analysis: Analysis,
    /// Read after writing; none when it could not be had.
    pub review: Option<Review>,
}

impl Analysis {
    /// The analysis as an answer to [`analysis_tool`] gives it.
    pub fn from_value(got: &Value) -> Self {
        serde_json::from_value(got.clone()).unwrap_or_default()
    }

    /// Nothing worked out: no approach, no question, no thesis.
    pub fn is_empty(&self) -> bool {
        self.approach.trim().is_empty()
            && self.central_question.trim().is_empty()
            && self.thesis.trim().is_empty()
    }

    /// The analysis as every call after it is told it, with how the
    /// script is to use it.
    pub fn brief(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let list = |items: &[String]| -> String {
            items
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .map(|item| format!("\n  - {item}"))
                .collect()
        };
        let mut out = String::from(
            "\nThe editorial analysis this video is built on - follow its approach, its \
             opening and its beats. Explain, do not merely describe: say why and how, not \
             only what. Open with genuine curiosity, never exaggeration. Carry the idea with \
             the evidence, put the strongest objection fairly, and end where the evidence \
             ends. Say a fact as a fact only where the research holds it; say an \
             interpretation, a hypothesis or an unverified claim as what it is. Let every \
             picture serve the idea its scene explains, in the kind of visuals named here.",
        );
        for (label, value) in [
            ("Approach", &self.approach),
            ("Central question", &self.central_question),
            ("Thesis", &self.thesis),
            ("Why it matters", &self.why_it_matters),
            ("Conclusion", &self.conclusion),
            ("What the viewer should understand after", &self.takeaway),
            ("Opening", &self.opening),
            ("Visuals", &self.visuals),
        ] {
            if !value.trim().is_empty() {
                out += &format!("\n{label}: {}", value.trim());
            }
        }
        for (label, items) in [
            ("How it works and why", &self.mechanisms),
            ("Assumptions", &self.assumptions),
            ("Commonly misunderstood", &self.misunderstood),
            ("Evidence and examples", &self.evidence),
            ("Objections and other readings", &self.counterarguments),
            ("Limits", &self.limitations),
            ("The beats, in order", &self.beats),
        ] {
            let listed = list(items);
            if !listed.is_empty() {
                out += &format!("\n{label}:{listed}");
            }
        }
        out + "\n"
    }
}

impl Review {
    /// The review as an answer to [`review_tool`] gives it, its kinds held
    /// to [`KINDS`].
    pub fn from_value(got: &Value) -> Self {
        let mut review: Review = serde_json::from_value(got.clone()).unwrap_or_default();
        for issue in &mut review.issues {
            issue.kind = issue.kind.trim().to_ascii_lowercase();
            if !KINDS.contains(&issue.kind.as_str()) {
                issue.kind = "generic".to_owned();
            }
        }
        review
            .issues
            .retain(|issue| !issue.problem.trim().is_empty());
        review.revised.clear();
        review
    }

    /// The scenes to write again, worst first, each once, at most
    /// [`MOST_FIXES`], with the notes that send them back: serious issues
    /// always, the rest only when the review did not pass the script.
    /// Scenes the script does not have are passed over.
    pub fn to_fix(&self, board: &Storyboard) -> Vec<(String, String)> {
        let known: std::collections::HashSet<&str> =
            board.scenes().map(|scene| scene.id.as_str()).collect();
        let mut order: Vec<&Issue> = self
            .issues
            .iter()
            .filter(|issue| known.contains(issue.scene.as_str()))
            .filter(|issue| !self.passed || SERIOUS.contains(&issue.kind.as_str()))
            .collect();
        // Serious first, in the review's order within each.
        order.sort_by_key(|issue| !SERIOUS.contains(&issue.kind.as_str()));
        let mut fixes: Vec<(String, String)> = Vec::new();
        for issue in order {
            let note = format!(
                "An editor's review found this scene {}: {}{}",
                match issue.kind.as_str() {
                    "depth" => "shallow",
                    "accuracy" => "inaccurate",
                    "unsupported" => "claiming what the evidence does not support",
                    "logic" => "breaking the argument's logic",
                    "missing" => "leaving out an explanation it needs",
                    "conclusion" => "concluding beyond the evidence",
                    "precision" => "imprecise with its concepts",
                    "clarity" => "unclear",
                    "purpose" => "off the video's purpose",
                    "narrative" => "weak as storytelling",
                    "visuals" => "showing pictures that do not serve its idea",
                    "repetition" => "repeating what another scene says",
                    _ => "generic",
                },
                issue.problem.trim(),
                if issue.fix.trim().is_empty() {
                    String::new()
                } else {
                    format!(" Fix: {}", issue.fix.trim())
                }
            );
            if let Some((_, notes)) = fixes.iter_mut().find(|(id, _)| *id == issue.scene) {
                notes.push(' ');
                notes.push_str(&note);
            } else if fixes.len() < MOST_FIXES {
                fixes.push((issue.scene.clone(), note));
            }
        }
        fixes
    }
}

/// The ask for the analysis: the brief, with its research, and what to
/// work out. Nothing in it names a subject: the writer judges which
/// questions apply.
pub fn analysis_prompt(brief_text: &str) -> String {
    format!(
        "{brief_text}\nBefore the video is outlined, work out what it is actually trying to \
         explain. Write no narration yet.\n\
         First decide what kind of subject this is and so what kind of explanation it needs: \
         historical, scientific, causal, political or ideological, economic, philosophical, \
         comparative, investigative, a narrative, a practical tutorial, a documentary - or a \
         mix. Do not force it into another kind's frame.\n\
         Then answer, of these, the questions that genuinely matter for this subject, and \
         leave empty those that do not apply: the central question a curious viewer would \
         ask; the answer the evidence supports; why it matters; how it works and what causes \
         it; the assumptions involved; the distinctions people commonly miss; the evidence \
         and concrete examples; the strongest objections or other readings; the limits of \
         the evidence or the idea; the conclusion it supports and no more; and what the \
         viewer should understand after watching. Use only what the research notes hold or \
         what is certain; where the evidence is thin, say so rather than overstate.\n\
         Then choose how to tell it, as suits this kind of subject - a paradox for an idea, \
         a surprising observation for science, a decisive event for history, a \
         contradiction between claims and consequences for policy, a problem and its \
         solution for a tutorial, a character and a conflict for a human story - and give \
         the beats in order, each with what it is for. Last, the kind of pictures that \
         explain this subject - cinematic scenes, illustration, characters, charts, maps, \
         historical reconstructions, scientific diagrams, metaphors, demonstrations, \
         documentary - and what they must show. Write in the video's language."
    )
}

/// The ask for the review: the script whole, scene by scene with its ids
/// and what its pictures show, against the research and the analysis,
/// judged by what this subject needs.
pub fn review_prompt(board: &Storyboard, research: &str, analysis: &Analysis) -> String {
    let script: String = board
        .chapters
        .iter()
        .map(|chapter| {
            let scenes: String = chapter
                .scenes
                .iter()
                .map(|scene| {
                    let shown: Vec<String> = scene
                        .shots
                        .iter()
                        .map(|shot| shot.prompt.trim().chars().take(160).collect())
                        .filter(|prompt: &String| !prompt.is_empty())
                        .collect();
                    let shown = if shown.is_empty() {
                        String::new()
                    } else {
                        format!("\n   Pictures: {}", shown.join(" | "))
                    };
                    format!("[{}] {}{shown}\n", scene.id, scene.narration.trim())
                })
                .collect();
            format!("## {}\n{scenes}", chapter.title.trim())
        })
        .collect();
    let research = if research.trim().is_empty() {
        "No research was done: hold every figure, date, name and quote to what is certain."
            .to_owned()
    } else {
        format!(
            "The research notes, the only evidence there is:\n{}",
            research.trim()
        )
    };
    format!(
        "You are the editor reviewing a video script before anything is made. Read it as a \
         demanding viewer who wants to understand the subject, not just hear about it.\n\n\
         {research}\n{}\nThe script, each scene with its id in brackets and what its \
         pictures show:\n{script}\n\
         Judge it by what this subject and this kind of video need, not by one template: \
         accuracy; depth (does it say why and how, or only what?); clarity; logical \
         consistency; the quality of the evidence where it states facts; precision with its \
         concepts; whether it tells the story well; whether it speaks to its audience; \
         whether each scene's pictures serve the idea it explains; and whether the video \
         does what it set out to. Also: claims the evidence does not hold, explanations it \
         leaves out, repetition, generic lines, and whether the conclusion follows. Name \
         each problem at the scene it is in, by its id, with what is wrong and how to put it \
         right. Pass the script only when nothing serious is wrong; do not pass a weak \
         script to be kind. Keep to the problems that matter - a handful, the worst first.",
        analysis.brief()
    )
}

/// The analysis, as a tool whose input is the answer.
pub fn analysis_tool() -> Value {
    let list = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "name": "analysis",
        "description": "What the video sets out to explain, worked out before it is outlined.",
        "input_schema": {
            "type": "object",
            "properties": {
                "approach": {"type": "string"},
                "central_question": {"type": "string"},
                "thesis": {"type": "string"},
                "why_it_matters": {"type": "string"},
                "mechanisms": list,
                "assumptions": list,
                "misunderstood": list,
                "evidence": list,
                "counterarguments": list,
                "limitations": list,
                "conclusion": {"type": "string"},
                "takeaway": {"type": "string"},
                "opening": {"type": "string"},
                "beats": list,
                "visuals": {"type": "string"}
            },
            "required": ["approach", "central_question", "opening", "beats", "visuals"]
        }
    })
}

/// The review, as a tool whose input is the answer.
pub fn review_tool() -> Value {
    json!({
        "name": "review",
        "description": "The editor's review of the whole script.",
        "input_schema": {
            "type": "object",
            "properties": {
                "passed": {"type": "boolean"},
                "conclusion_follows": {"type": "boolean"},
                "issues": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "scene": {"type": "string"},
                            "kind": {"type": "string", "enum": KINDS},
                            "problem": {"type": "string"},
                            "fix": {"type": "string"}
                        },
                        "required": ["scene", "kind", "problem", "fix"]
                    }
                },
                "summary": {"type": "string"}
            },
            "required": ["passed", "conclusion_follows", "issues", "summary"]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Storyboard {
        serde_json::from_value(json!({
            "title": "Empty homes",
            "chapters": [
                {"title": "One", "scenes": [
                    {"id": "s1", "narration": "Spain has millions of empty homes."},
                    {"id": "s2", "narration": "Yet rents keep rising."}
                ]},
                {"title": "Two", "scenes": [
                    {"id": "s3", "narration": "So the empty homes must be the cause."}
                ]}
            ]
        }))
        .unwrap()
    }

    /// The analysis is told to every call after it with what each part
    /// is; an empty one tells nothing.
    #[test]
    fn the_analysis_is_told_with_its_parts() {
        let analysis = Analysis::from_value(&json!({
            "approach": "Scientific explanation, opened on a surprising observation.",
            "central_question": "Why is the sky blue and not violet?",
            "mechanisms": ["Shorter wavelengths scatter more", " "],
            "counterarguments": ["Sunlight holds less violet"],
            "visuals": "Diagrams of light scattering."
        }));
        let told = analysis.brief();
        assert!(told.contains("Approach: Scientific explanation"));
        assert!(told.contains("Central question: Why is the sky blue"));
        assert!(told.contains("How it works and why:\n  - Shorter wavelengths scatter more"));
        assert!(!told.contains("  - \n"));
        assert!(told.contains("Objections and other readings:\n  - Sunlight holds less violet"));
        assert!(told.contains("Visuals: Diagrams of light scattering."));
        // What does not apply is left out, not filled.
        assert!(!told.contains("Commonly misunderstood"));
        assert!(!told.contains("Thesis"));
        assert_eq!(Analysis::default().brief(), "");
        assert!(Analysis::from_value(&json!("not an object")).is_empty());
    }

    /// The scenes the review sends back: serious issues first, one entry a
    /// scene with its notes joined, unknown scenes passed over; a passed
    /// script sends back only what is serious.
    #[test]
    fn the_review_names_the_scenes_to_write_again() {
        let review = Review::from_value(&json!({
            "passed": false,
            "conclusion_follows": false,
            "issues": [
                {"scene": "s2", "kind": "repetition", "problem": "Says s1 again.", "fix": ""},
                {"scene": "s3", "kind": "logic", "problem": "Count is not cause.", "fix": "Ask where the homes are."},
                {"scene": "s9", "kind": "accuracy", "problem": "No such scene.", "fix": ""},
                {"scene": "s3", "kind": "Conclusion", "problem": "Overreaches.", "fix": ""},
                {"scene": "s1", "kind": "made-up", "problem": "Vague.", "fix": ""},
                {"scene": "s1", "kind": "depth", "problem": "", "fix": "dropped: no problem"}
            ],
            "summary": "Shallow."
        }));
        assert_eq!(review.issues[3].kind, "conclusion");
        assert_eq!(review.issues[4].kind, "generic");
        assert_eq!(review.issues.len(), 5);
        let fixes = review.to_fix(&board());
        let ids: Vec<&str> = fixes.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["s3", "s2", "s1"]);
        assert!(
            fixes[0]
                .1
                .contains("breaking the argument's logic: Count is not cause.")
        );
        assert!(fixes[0].1.contains("Fix: Ask where the homes are."));
        assert!(
            fixes[0]
                .1
                .contains("concluding beyond the evidence: Overreaches.")
        );

        let passed = Review {
            passed: true,
            ..review.clone()
        };
        let ids: Vec<String> = passed
            .to_fix(&board())
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, ["s3"]);
    }

    /// The review reads the script whole with every scene's id, and the
    /// research it is held to.
    #[test]
    fn the_review_reads_the_script_with_its_ids() {
        let prompt = review_prompt(
            &board(),
            "FACTS:\n- 3.8 million empty homes [1]",
            &Analysis::default(),
        );
        assert!(prompt.contains("[s1] Spain has millions of empty homes."));
        assert!(prompt.contains("## Two\n[s3]"));
        assert!(prompt.contains("3.8 million empty homes [1]"));
        let unresearched = review_prompt(&board(), " ", &Analysis::default());
        assert!(unresearched.contains("No research was done"));
    }
}
