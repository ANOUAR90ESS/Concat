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
use crate::storyboard::{
    Chapter, Character, Graphic, GraphicKind, Reading, Scene, ShotKind, Storyboard, Style,
    shot_stem, without_marks,
};

/// The Messages API.
const API: &str = "https://api.anthropic.com/v1/messages";
/// The model that writes unless the caller names another.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// How many earlier pictures a chapter is shown to keep clear of: the most
/// recent, which are the ones it is likeliest to fall back on.
const AVOID: usize = 80;

const SYSTEM: &str = "You are the head writer of a channel that competes with the best on YouTube \
and TikTok. You write like a person with a point of view, not like an assistant: every line \
earns its place, and a viewer who leaves is a viewer lost.

Narration is written to be heard, not read. Short sentences, one idea each. Concrete over \
abstract: a number, a name, a place, a moment, never a generality. Show the thing, then say \
what it means. Use the second person and real questions where they pull the viewer in.

Make the hard things simple without making them false. Write for a smart viewer who knows \
nothing of the subject: never assume what you know, they know. Every term the viewer may not \
know is said in plain words first and named after, or not used at all. Give each difficult \
idea one concrete analogy from everyday life and keep using the same one as the idea grows, \
rather than a new image every time. Say the bottom line before the detail. Teach one idea at \
a time, in at most three steps a chapter. Prefer one vivid example to three general ones, and \
a comparison the viewer can picture - 'as tall as a ten-storey building' - to a bare number.

Clear is the floor, not the ceiling: a video that only explains is one of a hundred on the \
topic, and nobody shares it. Write it as no one else would. Find the angle - the way into the \
subject a dozen other videos on it did not take - and hold it to the end. Tell it through \
moments, not summaries: a particular person, a place, an hour, what was seen and heard, then \
what it meant. Surprise the viewer at least once a chapter: the detail nobody mentions, the \
consequence nobody expects, the fact that turns the obvious around. Let the narrator have a \
voice - curiosity, wonder, dry wit or irony where the subject allows it, gravity where it does \
not - and the same voice from the first line to the last. Vary the rhythm: a run of short \
sentences, then one that opens out; a line of three words where it should land. Give every \
chapter one sentence worth quoting, and let the end call back to an image from the beginning. \
Never open two scenes the same way, and never lean on the stock moves - 'imagine...', 'have \
you ever wondered', 'but here is the thing', 'the answer may surprise you', 'let's break it \
down', a list counted out 'one... two... three...' - when a scene, a figure or a question of \
your own would do.

Join scenes by cause, not by sequence: the story moves on 'but' and 'so', never on 'and \
then'. Each scene gives a reason for the next.

Structure for retention. Open cold, inside the story or on the most surprising fact - never \
with a greeting, a definition or 'in this video'. State the question the film answers within \
the first twenty seconds. Every minute or so open a loop - a promise, a mystery, a 'but' - and \
close it later; between them, remind the viewer what they are about to find out. End each chapter on a turn that makes the next one necessary. Build to a \
payoff: the last chapter answers the opening question and leaves one thought worth repeating.

Never write these, in any language, or their like: 'in today's fast-paced world', 'let's dive \
in', 'in conclusion', 'it is worth noting', 'without a doubt', 'a testament to', 'in the realm \
of', 'unlock', 'delve', 'journey', 'game-changer', 'buckle up', 'stay tuned', 'smash that like \
button'; in Arabic: 'في عالمنا المتسارع', 'دعونا نغوص', 'في الختام', 'لا شك أن', 'تجدر الإشارة', \
'رحلة مثيرة', 'عالم مليء بالأسرار'. No filler, no lists of adjectives, no moralising, and never \
say again what an earlier chapter said. Facts are exact or not given: no invented numbers, \
quotes or names.

In Arabic, write clear Modern Standard Arabic as a good narrator speaks it - natural, never \
stiff - unless the producer asks for a dialect; write it without vowel marks, which are added \
for the voice afterwards. In every language, write numbers the way they are said, and keep a \
sentence under twenty words so it can be said in one breath.

Every scene has three parts. The narration, in the video's language. The voice: one short line \
in English on how the narrator says it - pace, feeling, emphasis - never read aloud. And its \
shots, each one of two kinds:
- image: a still. Its prompt, in English, is one natural sentence or two of 40 to 70 words \
  describing one photograph in this order: the subject and what it is doing, the setting, the \
  framing and the lens with its aperture ('35mm, f/2.8'), the light, the mood. At most two \
  style words of its own; the film's look is added after it. No text, logos or captions, no \
  real people's names.
- video: a clip of a few seconds. Its prompt describes the picture it starts from, as for an \
  image. Its motion, under twenty words, does not describe that picture again: first what \
  moves in the frame and how, then one camera move at the end (slow push-in, pull-back, pan, \
  orbit, crane, handheld drift). Simple and explicit: one action, one move.
Describe what is there; do not tell the model what to avoid. An abstract idea is shown \
through a real, physical thing that stands for it.

Pictures follow the words. Most shots show what the narration says, literally; some give it a \
visual metaphor; none is decoration. Vary them: change the shot size from one shot to the \
next (aerial, wide, medium, close-up, macro detail), and the subject and the place from one \
scene to the next. Within one scene each shot shows something the others do not. A picture \
already used, or one that would read as the same picture, is never used again. Do not give \
every picture the same colours: the film has one look, each chapter its own palette, and \
teal-and-amber is one palette among many, not a default.";

/// Who the video is explained to: how much the viewer already knows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    /// Knows nothing of the subject: every step and term explained.
    Beginner,
    /// Curious and bright, new to this subject.
    #[default]
    General,
    /// Knows the basics: deeper, the common terms taken as known.
    Informed,
}

impl Level {
    /// In the sheet's order.
    pub const ALL: [Level; 3] = [Level::Beginner, Level::General, Level::Informed];

    /// What the writer is told of the viewer.
    pub(crate) fn brief(self) -> &'static str {
        match self {
            Level::Beginner => {
                "complete beginners: no step skipped, every term in plain words, an everyday \
                 example for each idea, slower and warmer"
            }
            Level::General => "bright and curious viewers new to this subject",
            Level::Informed => {
                "viewers who know the basics: common terms taken as known, more depth, \
                 nuance and the latest findings, no explaining what they already know"
            }
        }
    }
}

/// What the pictures look like, the whole video through. Auto lets the
/// writer choose for the topic; any other is held to, word for word, at
/// the head of every picture's prompt, so a video never drifts from one
/// medium to another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PictureStyle {
    /// The writer picks the medium for the topic.
    #[default]
    Auto,
    /// Cinematic photographs.
    Photo,
    /// Flat 2D vector illustration with simple characters: the look of
    /// motion-graphics explainers.
    Flat,
    /// A stylised 3D cartoon, like an animated feature.
    Cartoon,
}

impl PictureStyle {
    /// In the sheet's order.
    pub const ALL: [PictureStyle; 4] = [
        PictureStyle::Auto,
        PictureStyle::Photo,
        PictureStyle::Flat,
        PictureStyle::Cartoon,
    ];

    /// The line every picture's prompt starts its look with; None to let
    /// the writer choose.
    pub fn medium(self) -> Option<&'static str> {
        match self {
            PictureStyle::Auto => None,
            PictureStyle::Photo => Some(
                "Cinematic photograph, full-frame camera, natural light, shallow depth of field, \
                 filmic colour grade, photorealistic",
            ),
            PictureStyle::Flat => Some(
                "Flat 2D vector illustration, motion-graphics explainer style, clean bold \
                 shapes, solid colours with soft gradients, simple rounded characters with big \
                 expressive eyes, clean outlines, uncluttered background, no photographic \
                 detail, no text",
            ),
            PictureStyle::Cartoon => Some(
                "Stylised 3D cartoon render, animated feature film look, soft global \
                 illumination, smooth rounded characters with expressive faces, vivid colours, \
                 no text",
            ),
        }
    }
}

/// A kind of video made whole: a shape the writer holds to on top of what
/// the video is for - where the hook goes, how it is paced, who is in its
/// pictures - tuned until what comes out can go up as it is. The sheet
/// sets the frame, the length and the look that go with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoFormat {
    /// No format: the sheet's choices alone.
    #[default]
    Custom,
    /// One idea in a minute, upright, with the same two characters in
    /// every picture: for Shorts, Reels and TikTok.
    Minute,
    /// A true story told as a film, in acts: for a long video.
    Story,
    /// One concept taught from an everyday question, drawn as it is
    /// explained.
    Lesson,
}

impl VideoFormat {
    /// In the sheet's order.
    pub const ALL: [VideoFormat; 4] = [
        VideoFormat::Custom,
        VideoFormat::Minute,
        VideoFormat::Story,
        VideoFormat::Lesson,
    ];

    /// What the writer is told of the format; nothing for none.
    fn brief(self) -> &'static str {
        match self {
            VideoFormat::Custom => "",
            VideoFormat::Minute => {
                "FORMAT - one idea in a minute, upright, for Shorts, Reels and TikTok.\n\
                 - The first sentence puts the viewer in a situation or asks them a question \
                 they cannot scroll past: 'A policeman stops you and asks for your phone. Do \
                 you have to hand it over?' Never a greeting, never 'in this video'.\n\
                 - Then the answer in three or four beats, one idea a beat, each one or two \
                 sentences of at most ten words. Speak to the viewer as 'you'.\n\
                 - One beat is the turn: what most people get wrong, or the exception. Give \
                 that scene a sticker.\n\
                 - End on the one line to remember, then a question to the viewer that \
                 invites a comment. No summary, no goodbye.\n\
                 - Give the video a cast of two: one who stands for the viewer, one for the \
                 other side of the situation. Every picture is a moment of the situation with \
                 them in it - doing, not posing - or the object the beat is about, large.\n\
                 - Every scene has keywords. Figures are said and shown: where a number \
                 matters, an animated card."
            }
            VideoFormat::Story => {
                "FORMAT - a true story told as a film.\n\
                 - Open cold on the story's most dramatic moment, in the present tense, \
                 thirty seconds at most, and stop before it resolves; then go back: 'To \
                 understand how it came to this, we start in...'.\n\
                 - Each chapter is an act with its own question, and ends on a turn that \
                 makes the next one necessary. The last returns to the opening moment and \
                 resolves it.\n\
                 - People before events: name who did what, where they stood, what they \
                 said or wrote where it is on record. Dates and figures exactly as the \
                 research has them.\n\
                 - One line in each chapter says why this still matters to the viewer \
                 today.\n\
                 - Pictures are places, objects, documents and people at the moment the \
                 narration names them, shot like a film; maps and timelines are drawn, not \
                 described."
            }
            VideoFormat::Lesson => {
                "FORMAT - one concept, taught.\n\
                 - Start from a question the viewer has asked themselves in everyday life, \
                 never from a definition.\n\
                 - Give the concept one analogy from everyday life and carry it through the \
                 whole video: every new part of the concept is a new part of the same \
                 analogy.\n\
                 - Three steps at most, each built on the one before; say each new term \
                 once, plainly, the moment it appears.\n\
                 - Wherever the narration says how something works, the shot is an \
                 explainer that draws it as it is said.\n\
                 - Half way, ask the viewer to predict what happens next, then show it.\n\
                 - End by returning to the everyday question of the opening, now answered, \
                 and the one misconception this video has just corrected; the points to \
                 remember go on a list card, said in a fresh line rather than counted out.\n\
                 - Give the video a guide: one character who asks the questions the viewer \
                 would, and is in the pictures that are not drawings."
            }
        }
    }
}

/// How the video is written: what it is for, and the shape that serves
/// it. Chosen in the Long video sheet; each comes with its own brief to
/// the writer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentStyle {
    /// A story told about the real world, with tension and a reveal.
    #[default]
    Documentary,
    /// Teaches one idea, step by step, to someone new to it.
    Educational,
    /// Rigorous: the evidence, its sources, where it is unsure.
    Academic,
    /// A narrative with a character, a conflict and a turn.
    Story,
    /// What happened, why it matters, what comes next.
    News,
}

impl ContentStyle {
    /// In the sheet's order.
    pub const ALL: [ContentStyle; 5] = [
        ContentStyle::Documentary,
        ContentStyle::Educational,
        ContentStyle::Academic,
        ContentStyle::Story,
        ContentStyle::News,
    ];

    /// What the writer is told the video is, and how it is built.
    fn brief(self) -> &'static str {
        match self {
            ContentStyle::Documentary => {
                "A documentary. Tell the real story as a story: people, places, dates, the \
                 stakes, a tension that builds and a reveal. Authoritative and cinematic, never \
                 dry. Pictures: real places and objects, shot like a feature film."
            }
            ContentStyle::Educational => {
                "An explainer that teaches. Start from what the viewer already knows, build one \
                 idea on the last, explain every new term the moment it appears, and give each \
                 idea an everyday example or analogy. Check understanding with a question, then \
                 answer it. End with the three things to remember. Pictures: clear, literal, \
                 one idea each - diagrams-as-photographs, close-ups of the thing explained."
            }
            ContentStyle::Academic => {
                "An academic deep dive. Precise terms, the evidence behind every claim and \
                 where it comes from (study, institution, year) in the narration, the debate \
                 where there is one, and what is still unknown. Calm and exact, never \
                 sensational. Pictures: documentary realism - laboratories, archives, \
                 instruments, data on screens, the places the research happened."
            }
            ContentStyle::Story => {
                "A story. One main character, real or composite and named as such, who wants \
                 something; obstacles; a turning point; a resolution that carries the lesson \
                 without stating it. Scenes, dialogue in a line or two, sensory detail. \
                 Pictures: the character's world, the same character described the same way \
                 in every shot they are in."
            }
            ContentStyle::News => {
                "A news analysis. What happened, in one line, first; then why it matters, \
                 who wins and who loses, the context that explains it, and what to watch \
                 next. Neutral and sourced, fast-paced. Pictures: photojournalism - places, \
                 crowds, objects and screens tied to the events, no staged look."
            }
        }
    }
}

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
    /// How it is written.
    pub style: ContentStyle,
    /// Who it is explained to.
    pub level: Level,
    /// The channel it is for, as the person described it once: name,
    /// audience, voice, opening and closing lines, scripts to write like.
    /// Empty when they have not.
    pub channel: String,
    /// Read up on the web before writing, and list the sources.
    pub research: bool,
    /// Every shot an animated card the app draws, none made by a model.
    pub cards_only: bool,
    /// What the pictures look like.
    pub pictures: PictureStyle,
    /// What the video is made from, when it is made from something: a
    /// page's address, or a PDF or text file's path. Empty for a video
    /// written from the topic alone.
    pub source: String,
    /// The kind of video it is made as, when it is one of those the app
    /// makes whole.
    pub format: VideoFormat,
    /// The video's shape in the person's own words - how it opens, what
    /// it is made of, how it ends: a format of their own, for any kind of
    /// video the app has no format for. Empty for none.
    pub structure: String,
    /// Open on a hook: a few seconds written last, from the whole script,
    /// with a picture of its own, before anything else.
    pub hook: bool,
}

impl Brief {
    /// How the video is paced.
    pub fn pacing(&self) -> Pacing {
        Pacing::new(self.minutes, self.clip_share)
    }

    /// The brief as every call starts with it.
    fn text(&self) -> String {
        let mut text = format!(
            "Topic: {}\nLength: about {} minutes\nNarration language: {}\nAspect ratio: {}\n\
             What it is: {}\nWho it is for: {}\n",
            self.topic.trim(),
            self.minutes,
            self.language,
            self.aspect,
            self.style.brief(),
            self.level.brief()
        );
        if !self.channel.trim().is_empty() {
            text += &format!(
                "The channel, as its owner describes it - write as this channel, in its voice, \
                 with its opening and closing lines:\n{}\n",
                self.channel.trim()
            );
        }
        if !self.notes.trim().is_empty() {
            text += &format!("Notes from the producer: {}\n", self.notes.trim());
        }
        if !self.format.brief().is_empty() {
            text += &format!("{}\n", self.format.brief());
        }
        // The person's own shape last: where it and a format disagree, it
        // is theirs that holds.
        if !self.structure.trim().is_empty() {
            text += &format!(
                "THE PRODUCER'S OWN SHAPE for this video, in their words - hold to it in every \
                 chapter, and where it differs from anything above, it wins:\n{}\n",
                self.structure.trim()
            );
        }
        if let Some(medium) = self.pictures.medium() {
            text += &format!(
                "Picture style, fixed for every picture: {medium}. Write every picture's prompt \
                 for this medium.\n"
            );
        }
        text
    }
}

/// Where the writing is: the outline, then chapter `n` of `of`.
#[derive(Clone, Debug, PartialEq)]
pub enum Writing {
    /// The topic is being read up on, on the web.
    Research,
    /// The outline is being written.
    Outline,
    /// Chapter `n` (from 1) of `of` is being written.
    Chapter {
        /// This chapter.
        n: usize,
        /// Of how many.
        of: usize,
        /// Its title, as the outline has it.
        title: String,
    },
    /// Explainer `n` (from 1) of `of` in chapter `chapter` (from 1) of
    /// `chapters` is being designed.
    Explainer {
        /// The chapter.
        chapter: usize,
        /// Of how many.
        chapters: usize,
        /// This explainer.
        n: usize,
        /// Of how many in the chapter.
        of: usize,
    },
    /// The opening hook is being written, from the finished script.
    Hook,
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
    let mut brief_text = brief.text();
    let mut sources = Vec::new();
    if brief.research || !brief.source.trim().is_empty() {
        progress(Writing::Research);
        match research(brief, key, model) {
            Ok(found) => {
                brief_text += &format!(
                    "\nResearch notes, read on the web for this video. Build on these facts; \
                     give no number, date, name or quote that is not here or certain:\n{}\n",
                    found.notes
                );
                sources = found.sources;
            }
            // Written from what the writer knows, as before: the research
            // is a help, not a condition.
            Err(error) => log::warn!("research: {error}; writing without it"),
        }
    }

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
    // The angle the outline chose, told to every chapter after it, so
    // the video is told one way from its first line to its last.
    let angle = text(&outline["angle"]);
    if !angle.trim().is_empty() {
        brief_text += &format!(
            "The video's angle, chosen with its outline - tell every chapter through it:\n{}\n",
            angle.trim()
        );
    }
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
            visual: match brief.pictures.medium() {
                // The medium held to word for word, whatever the writer
                // added to it.
                Some(medium) => format!(
                    "{medium}. {}",
                    text(&outline["visual_style"]).trim_end_matches('.')
                )
                .trim_end_matches(". ")
                .to_owned(),
                None => text(&outline["visual_style"]),
            },
            voice: text(&outline["voice_style"]),
            cast: cast_of(&outline["cast"]),
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
        sources,
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
        progress(Writing::Chapter {
            n: i + 1,
            of,
            title: board.chapters[i].title.trim().to_owned(),
        });
        let prompt = chapter_prompt(&board, &brief_text, brief, i, plan, points, &ending);
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
        board.chapters[i].scenes = conform(scenes, plan, brief.cards_only);
        board.tidy()?;
        ending = text(&got["ending"]);

        // An editor's pass over the chapter's words: what a first draft
        // leaves - a soft opening, a phrase every script uses, a line that
        // says again what another said - taken out, at the same length.
        match ask(&edit_tool(), edit_prompt(&board, &brief_text, i)) {
            Ok(edited) => apply_edits(&mut board.chapters[i].scenes, &edited),
            Err(error) => log::warn!("chapter {}: the editor's pass failed: {error}", i + 1),
        }

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

        // The words as the voice is to read them, last, once they are
        // final: a voice reading Arabic without its vowels guesses, and
        // guesses some words wrong.
        if needs_marks(&brief.language) {
            match ask(&reading_tool(), reading_prompt(&board.chapters[i].scenes)) {
                Ok(read) => apply_readings(&mut board.chapters[i].scenes, &read),
                Err(error) => log::warn!("chapter {}: the vowel marks failed: {error}", i + 1),
            }
        }

        // The explainers designed, each in a call of its own, now that
        // the words they move with are final.
        crate::director::direct_chapter(&mut board, brief, i, key, model, &mut |n, of_these| {
            progress(Writing::Explainer {
                chapter: i + 1,
                chapters: of,
                n,
                of: of_these,
            })
        });
    }
    board.tidy()?;

    // The hook last, from the script as it came out: an opening written
    // first promises what the plan was, and this one what the video has.
    // A video of cards alone has no picture to open on, and none is made.
    if brief.hook && !brief.cards_only {
        progress(Writing::Hook);
        match ask(&hook_tool(), hook_prompt(&board, &brief_text)) {
            Ok(got) => match hook_scene(&got) {
                Some(mut scene) => {
                    if needs_marks(&brief.language) {
                        let scenes = std::slice::from_mut(&mut scene);
                        match ask(&reading_tool(), reading_prompt(scenes)) {
                            Ok(read) => apply_readings(scenes, &read),
                            Err(error) => log::warn!("the hook's vowel marks failed: {error}"),
                        }
                    }
                    board.chapters[0].scenes.insert(0, scene);
                    board.tidy()?;
                }
                None => log::warn!("the hook came back without a line or a picture; none is used"),
            },
            // The video is whole without it.
            Err(error) => log::warn!("the hook could not be written: {error}"),
        }
    }
    Ok(board)
}

/// How long a hook is planned to hold; its voice decides in the end.
const HOOK_SECONDS: f64 = 4.0;

/// What the hook's call may read of the script, in characters: past it
/// each scene is given by its opening words.
const HOOK_READS: usize = 60_000;

/// The ask for the hook: the whole script, and what a hook is.
fn hook_prompt(board: &Storyboard, brief_text: &str) -> String {
    let whole = |words: usize| -> String {
        board
            .chapters
            .iter()
            .map(|chapter| {
                let scenes: String = chapter
                    .scenes
                    .iter()
                    .map(|scene| format!("{}\n", short(scene.narration.trim(), words)))
                    .collect();
                format!("## {}\n{scenes}", chapter.title.trim())
            })
            .collect()
    };
    let mut script = whole(usize::MAX);
    if script.len() > HOOK_READS {
        script = whole(30);
    }
    format!(
        "{brief_text}\nThe video \"{}\" is written. This is all of its narration, in order:\n\n\
         {script}\n\
         Now write its HOOK: the three to five seconds a viewer sees before anything else, \
         which decide whether they stay. Read the whole script and take the strongest thing \
         it really holds - the number nobody expects, the contradiction, the question it \
         answers, what is at stake - and open on that.\n\
         - line: one spoken sentence in {}, at most 16 words, said to the viewer. It promises \
         only what the video delivers, gives away no answer, and uses none of the first \
         scene's own words. No greeting, no channel name, no \"in this video\", no \"did you \
         know\".\n\
         - words: two to four words in {} shown very large over the picture while the line \
         is said - the hook at a glance with the sound off, not the line again.\n\
         - keywords: one or two words of the line, exactly as written in it, for the caption \
         to colour.\n\
         - picture: in English, 50 to 80 words, the one image the video opens on. {}\n\
         - voice: how the line is said, in a few words.",
        board.title.trim(),
        board.language,
        board.language,
        striking(&board.style.visual),
    )
}

/// How the hook's picture and the thumbnail's are to look, after what the
/// owner showed as the quality they want: an image a top channel would
/// open on, not a plain photograph of the subject.
fn striking(visual: &str) -> String {
    format!(
        "Make it a bold composite in the manner of a top YouTube thumbnail: one hero subject \
         large in the foreground, and one or two objects or figures from the video around it \
         that tell the story at a glance - the contrast or the danger the video is about. \
         Dramatic coloured light: a hot colour against a cold one (red against blue, orange \
         against teal), a rim light on every edge, glow, smoke or sparks where they belong; \
         deep shadows, rich saturated colour, razor-sharp focus, high contrast, readable on a \
         phone at a glance. Leave one side clear for large words. No text, letters or logos \
         in the picture. Keep to the film's medium: {}",
        visual.trim()
    )
}

/// The hook as the video's first scene, or None when it came back
/// without a line or without a picture.
fn hook_scene(got: &Value) -> Option<Scene> {
    let line = text(&got["line"]).trim().to_owned();
    let picture = text(&got["picture"]).trim().to_owned();
    if line.is_empty() || picture.is_empty() {
        return None;
    }
    let words = text(&got["words"]).trim().to_owned();
    Some(Scene {
        id: crate::storyboard::HOOK.to_owned(),
        narration: line,
        voice: text(&got["voice"]).trim().to_owned(),
        seconds: HOOK_SECONDS,
        shots: vec![crate::storyboard::Shot {
            kind: ShotKind::Image,
            prompt: picture,
            ..Default::default()
        }],
        keywords: got["keywords"]
            .as_array()
            .map(|words| {
                words
                    .iter()
                    .map(|word| text(word).trim().to_owned())
                    .filter(|word| !word.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        sticker: (!words.is_empty()).then_some(words),
        ..Scene::default()
    })
}

/// Whether a voice reads `language` better with its words marked: Arabic,
/// whose short vowels are not written.
pub fn needs_marks(language: &str) -> bool {
    let language = language.trim().to_lowercase();
    language == "ar" || language.starts_with("ar-") || language.contains("arab")
}

/// Asks for each scene's narration back, word for word, with its marks.
fn reading_prompt(scenes: &[Scene]) -> String {
    let lines: String = scenes
        .iter()
        .enumerate()
        .map(|(n, scene)| format!("{}. {}\n", n + 1, scene.narration.trim()))
        .collect();
    format!(
        "This is narration a voice will read aloud, scene by scene:\n{lines}\n\
         Write each scene back with full Arabic vowel marks (tashkeel) on every word, as a \
         careful narrator of Modern Standard Arabic says it: the right short vowels, shadda, \
         sukun, tanwin and hamza, and the case endings a narrator voices, with a sukun where \
         the voice pauses at the end of a sentence. Mark names and borrowed words the way \
         they are said in Arabic. Keep every word and every punctuation mark as it is, in \
         order: change no letter, add, drop, join or split no word, and leave numbers in \
         digits as they are. Return all {} scenes, in order.",
        scenes.len()
    )
}

/// The marked narration put in place, scene for scene, where it is the
/// same words: a reading that changed them would say what the captions
/// do not, and they are timed to it word for word.
fn apply_readings(scenes: &mut [Scene], read: &Value) {
    let Some(lines) = read["readings"].as_array() else {
        return;
    };
    if lines.len() != scenes.len() {
        log::warn!(
            "the vowel marks came back for {} scenes of {}; the words are read as written",
            lines.len(),
            scenes.len()
        );
        return;
    }
    for (scene, line) in scenes.iter_mut().zip(lines) {
        let line = text(line);
        if same_words(&scene.narration, &line) {
            scene.reading = Some(Reading {
                text: line.trim().to_owned(),
                of: scene.narration.trim().to_owned(),
            });
        } else {
            log::warn!(
                "scene {}: its marked words differ; read as written",
                scene.id
            );
        }
    }
}

/// Whether `marked` is `plain` with marks: as many words, nearly all the
/// same letters once the marks are taken off (a hamza seated differently
/// is not worth losing a scene's marks over).
fn same_words(plain: &str, marked: &str) -> bool {
    let plain: Vec<&str> = plain.split_whitespace().collect();
    let marked: Vec<String> = marked.split_whitespace().map(without_marks).collect();
    if plain.is_empty() || plain.len() != marked.len() {
        return false;
    }
    let same = plain
        .iter()
        .zip(&marked)
        .filter(|(a, b)| without_marks(a) == **b)
        .count();
    same * 10 >= plain.len() * 9
}

/// Strings in a list, joined by a semicolon.
pub(crate) fn list_text(value: &Value) -> String {
    value
        .as_array()
        .map(|items| items.iter().map(text).collect::<Vec<_>>().join("; "))
        .unwrap_or_default()
}

/// What the writer read up on before writing: its notes, and the pages
/// they came from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Research {
    /// The facts, in English, each with the number of its source.
    pub notes: String,
    /// The pages, numbered from 1 in that order.
    pub sources: Vec<crate::storyboard::Source>,
}

/// How many searches one research may run: about ten cents of them.
const SEARCHES: u32 = 8;
/// How many pages it may read in full: the few that matter most.
const FETCHES: u32 = 4;

/// Reads up on `brief`'s topic on the web, with Anthropic's web search:
/// the facts, the numbers and their dates, the people and places, the
/// debate and the latest, for the kind of video it is.
pub fn research(brief: &Brief, key: &str, model: &str) -> Result<Research, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to read up on the topic".to_owned());
    }
    let (from_source, attached) = source_of(&brief.source)?;
    let prompt = format!(
        "{}\n{from_source}Before anything is written, research this topic on the web for the video \
         described. Find what a specialist would put in it: the key facts and how they connect, \
         numbers with their dates and units, the people, places and events, what is surprising \
         or little known, where experts disagree, and the latest developments; and how the \
         best teachers explain it - the plain-language explanation, the analogies they use \
         and the misconceptions people commonly have. Prefer primary and reputable sources, \
         and read the two or three most important pages in full rather than their snippets. Write the notes in English as short factual lines, each ending \
         with the number of its source in brackets, like [2]; list the sources in that order. \
         End the notes with a line 'Explain it like this:' giving the clearest analogy you \
         found, and a line 'Common misconception:'.",
        brief.text()
    );
    let search = |kind: &str| json!({"type": kind, "name": "web_search", "max_uses": SEARCHES});
    let fetch = json!({"type": "web_fetch_20260209", "name": "web_fetch", "max_uses": FETCHES});
    // The newer search, with pages read in full, on the models that take
    // them; the first search alone on the others - a model the person
    // named themselves may be older.
    let ask = |tools: &[Value]| {
        call_blocks(
            key,
            model,
            SYSTEM,
            tools,
            &research_tool(),
            &prompt,
            attached.clone(),
        )
    };
    let got = crate::providers::retry(|| ask(&[search("web_search_20260209"), fetch.clone()]))
        .or_else(|error| {
            if error.contains("web_search") || error.contains("web_fetch") {
                ask(&[search("web_search_20250305")])
            } else {
                Err(error)
            }
        })?;
    let sources = got["sources"]
        .as_array()
        .map(|sources| {
            sources
                .iter()
                .map(|source| crate::storyboard::Source {
                    title: text(&source["title"]),
                    url: text(&source["url"]),
                })
                .filter(|source| source.url.starts_with("http"))
                .collect()
        })
        .unwrap_or_default();
    let notes = text(&got["notes"]);
    if notes.is_empty() {
        return Err("the research came back empty".to_owned());
    }
    Ok(Research { notes, sources })
}

/// The most of a text file a video is made from: more is refused, not cut
/// short without saying.
const SOURCE_CHARS: usize = 400_000;

/// What the research is told of the video's `source`, and what goes with
/// the request: a page is named, to be fetched and read in full; a PDF is
/// attached; a text file is quoted whole. Nothing for no source.
fn source_of(source: &str) -> Result<(String, Vec<Value>), String> {
    use base64::Engine as _;
    let source = source.trim();
    let made_from = "This video is made from one source, given here. Read it in full first: \
                     the video says what the source says, in its order of importance, and adds \
                     nothing the source does not hold. Search the web only to check a figure or \
                     a name, or to explain a term the source takes as known. List the source \
                     first among the sources.";
    if source.is_empty() {
        return Ok((String::new(), Vec::new()));
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        return Ok((
            format!("{made_from}\nThe source's address - fetch it: {source}\n\n"),
            Vec::new(),
        ));
    }
    let path = std::path::Path::new(source);
    let bytes = std::fs::read(path).map_err(|error| format!("{source}: {error}"))?;
    if bytes.starts_with(b"%PDF") {
        // What one request takes of a PDF.
        if bytes.len() > 30 * 1024 * 1024 {
            return Err(format!(
                "{source} is too large a PDF to read at once (30 MB at most)"
            ));
        }
        return Ok((
            format!("{made_from}\nThe source is the document attached.\n\n"),
            vec![json!({
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": base64::engine::general_purpose::STANDARD.encode(&bytes),
                }
            })],
        ));
    }
    let words = String::from_utf8(bytes)
        .map_err(|_| format!("{source} is neither a PDF nor a text file"))?;
    if words.chars().count() > SOURCE_CHARS {
        return Err(format!(
            "{source} is too long to make one video from: split it, or give the part to use"
        ));
    }
    Ok((
        format!(
            "{made_from}\nThe source, whole:\n<source>\n{}\n</source>\n\n",
            words.trim()
        ),
        Vec::new(),
    ))
}

/// The sources, as a YouTube description lists them.
pub fn sources_text(sources: &[crate::storyboard::Source]) -> String {
    if sources.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = sources
        .iter()
        .map(|source| {
            if source.title.trim().is_empty() {
                source.url.clone()
            } else {
                format!("{} - {}", source.title.trim(), source.url)
            }
        })
        .collect();
    format!("Sources:\n{}", lines.join("\n"))
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
    new.reading = None;
    if keep_narration {
        new.narration = old.narration.clone();
        new.reading = old.reading.clone();
    } else if needs_marks(&brief.language) {
        let scenes = std::slice::from_mut(&mut new);
        match crate::providers::retry(|| call(key, model, &reading_tool(), &reading_prompt(scenes)))
        {
            Ok(read) => apply_readings(scenes, &read),
            Err(error) => log::warn!("the vowel marks failed: {error}"),
        }
    }
    let mut new = conform(
        vec![new],
        &ChapterPlan {
            seconds: plan.seconds,
            scenes: vec![plan],
        },
        brief.cards_only,
    )
    .pop()
    .ok_or_else(|| "the rewritten scene came back without a picture".to_owned())?;
    // Its explainers designed, in the scene's place in the video.
    if new
        .shots
        .iter()
        .any(|shot| shot.kind == ShotKind::Explainer && shot.explainer.is_none())
    {
        let mut placed = board.clone();
        placed.chapters[chapter].scenes[scene] = new.clone();
        crate::director::direct_chapter(&mut placed, brief, chapter, key, model, &mut |_, _| {});
        new = placed.chapters[chapter].scenes[scene].clone();
    }
    Ok(new)
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

/// Up to `count` stretches of the video each worth a Short, none sharing
/// a scene with another: several Shorts from one long video. A pick that
/// is not a run of scenes that fits is passed over; none at all is an
/// error, and the caller falls back to [`crate::shorts::opening`].
pub fn pick_shorts(
    board: &Storyboard,
    brief: &Brief,
    count: usize,
    key: &str,
    model: &str,
) -> Result<Vec<crate::shorts::Pick>, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let count = count.clamp(1, 5);
    let prompt = format!(
        "{}\nPick {count} such stretches, the {count} best, no scene in more than one of them, \
         each with its own hook; fewer where the video does not hold that many.",
        short_prompt(board, brief)
    );
    let got = crate::providers::retry(|| call(key, model, &shorts_tool(), &prompt))?;
    let picks = picks_of(board, &got["shorts"], count);
    if picks.is_empty() {
        return Err("the writer picked no stretch that is a minute in a row".to_owned());
    }
    Ok(picks)
}

/// The picks a `shorts` answer holds that are runs of `board`'s scenes
/// that fit, none sharing a scene with one before it, `count` at most.
fn picks_of(board: &Storyboard, shorts: &Value, count: usize) -> Vec<crate::shorts::Pick> {
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut picks = Vec::new();
    for short in shorts.as_array().into_iter().flatten() {
        let scenes: Vec<String> = short["scenes"]
            .as_array()
            .map(|ids| ids.iter().map(text).filter(|id| !id.is_empty()).collect())
            .unwrap_or_default();
        if picks.len() == count
            || !crate::shorts::fits(board, &scenes)
            || scenes.iter().any(|id| taken.contains(id))
        {
            continue;
        }
        taken.extend(scenes.iter().cloned());
        picks.push(crate::shorts::Pick {
            scenes,
            hook: text(&short["hook"]),
        });
    }
    picks
}

/// A video worth making, as [`ideas`] proposes it.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VideoIdea {
    /// The video's working title, in the narration's language: what goes
    /// in the topic box.
    pub title: String,
    /// Why a viewer would click, in a sentence, in the same language.
    pub angle: String,
}

/// `count` videos worth making for the channel the brief describes, about
/// `field` - a subject, or nothing for whatever suits the channel - none
/// of them one of `made`, the titles made already.
pub fn ideas(
    field: &str,
    brief: &Brief,
    made: &[String],
    count: usize,
    key: &str,
    model: &str,
) -> Result<Vec<VideoIdea>, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to suggest ideas".to_owned());
    }
    let count = count.clamp(1, 10);
    let mut prompt = format!(
        "Narration language: {}\nWhat the videos are: {}\nWho they are for: {}\n",
        brief.language,
        brief.style.brief(),
        brief.level.brief()
    );
    if !brief.channel.trim().is_empty() {
        prompt += &format!(
            "The channel, as its owner describes it:\n{}\n",
            brief.channel.trim()
        );
    }
    if !field.trim().is_empty() {
        prompt += &format!("The subject to stay within: {}\n", field.trim());
    }
    if !made.is_empty() {
        prompt += &format!("Already made - nothing like these: {}\n", made.join("; "));
    }
    prompt += &format!(
        "\nPropose {count} videos worth making next, each one a viewer would click on and watch \
         to the end: a specific question, story or claim, never a broad subject ('The history of \
         Rome' is a subject; 'The day Rome ran out of water' is a video). Each unlike the others \
         in angle. Give each a title of at most ten words and, in one sentence, why a viewer \
         clicks. Both in the narration's language."
    );
    let got = crate::providers::retry(|| call(key, model, &ideas_tool(), &prompt))?;
    let ideas: Vec<VideoIdea> = got["ideas"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|idea| VideoIdea {
            title: text(&idea["title"]),
            angle: text(&idea["angle"]),
        })
        .filter(|idea| !idea.title.is_empty())
        .take(count)
        .collect();
    if ideas.is_empty() {
        return Err("the writer proposed no ideas".to_owned());
    }
    Ok(ideas)
}

/// `board` again in `language`: every word a viewer hears or reads - the
/// narration, titles, stickers, key words, cards, and the words drawn in
/// its animated cards and explainers - translated a chapter at a time,
/// its shape untouched, so the pictures made for it serve the new video
/// as they are. `progress` is told each chapter, from 1, and how many.
pub fn translate(
    board: &Storyboard,
    language: &str,
    key: &str,
    model: &str,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Storyboard, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to translate the script".to_owned());
    }
    let mut out = board.clone();
    out.language = language.trim().to_owned();
    let of = board.chapters.len();
    for (n, chapter) in board.chapters.iter().enumerate() {
        progress(n + 1, of);
        let words = chapter_words(&board.title, chapter);
        let prompt = format!(
            "This is one chapter of a narrated video, as the words a viewer hears and reads, \
             written in {}:\n{}\n\nTranslate it into {language} for a voice to read aloud: \
             natural, idiomatic, spoken {language} - not word for word - saying the same things \
             in about the same time. Keep the structure exactly: the same scenes in order, and \
             in each the same number of shots, texts, cues and keywords, an empty string where \
             one is empty. Names stay names; figures stay figures, written as {language} writes \
             them. 'keywords' and 'cues' are words of the scene's narration: give each as the \
             word or words of your translated narration that say the same thing, copied exactly \
             from it. 'texts' are words drawn on screen: short, as short as the originals.",
            board.language,
            serde_json::to_string_pretty(&words).unwrap_or_default()
        );
        let got = crate::providers::retry(|| {
            let got = call(key, model, &translation_tool(), &prompt)?;
            // Shaped as it was sent, or asked for again: a scene dropped
            // would leave pictures with the wrong words under them.
            let mut trial = out.chapters[n].clone();
            apply_words(&mut trial, &got).map(|()| (got, trial))
        })?;
        if n == 0 {
            let title = text(&got.0["title"]);
            if !title.is_empty() {
                out.title = title;
            }
        }
        out.chapters[n] = got.1;
        if needs_marks(language) {
            match crate::providers::retry(|| {
                call(
                    key,
                    model,
                    &reading_tool(),
                    &reading_prompt(&out.chapters[n].scenes),
                )
            }) {
                Ok(read) => apply_readings(&mut out.chapters[n].scenes, &read),
                Err(error) => log::warn!("the vowel marks: {error}; read as written"),
            }
        }
    }
    Ok(out)
}

/// The words of `chapter` a viewer hears or reads, as the translator is
/// shown them.
fn chapter_words(title: &str, chapter: &Chapter) -> Value {
    let scenes: Vec<Value> = chapter
        .scenes
        .iter()
        .map(|scene| {
            let shots: Vec<Value> = scene
                .shots
                .iter()
                .map(|shot| {
                    let (texts, cues) = shot_words(shot);
                    json!({"texts": texts, "cues": cues})
                })
                .collect();
            json!({
                "narration": scene.narration,
                "onScreenText": scene.on_screen_text.clone().unwrap_or_default(),
                "sticker": scene.sticker.clone().unwrap_or_default(),
                "keywords": scene.keywords,
                "cardFigure": scene.card.as_ref().map(|card| card.figure.clone()).unwrap_or_default(),
                "cardLabel": scene.card.as_ref().map(|card| card.label.clone()).unwrap_or_default(),
                "shots": shots,
            })
        })
        .collect();
    json!({
        "title": title,
        "chapterTitle": chapter.title,
        "scenes": scenes,
    })
}

/// The words drawn in a shot - an animated card's heading and lines, an
/// explainer's texts - and the words of the narration its steps wait for.
fn shot_words(shot: &crate::storyboard::Shot) -> (Vec<String>, Vec<String>) {
    let mut texts = Vec::new();
    let mut cues = Vec::new();
    if let Some(graphic) = &shot.graphic {
        texts.push(graphic.title.clone());
        for item in &graphic.items {
            texts.push(item.label.clone());
            texts.push(item.value.clone());
        }
    }
    if let Some(explainer) = &shot.explainer {
        for element in explainer.all() {
            if element.kind == crate::explainer::ElementKind::Text {
                texts.push(element.text.clone());
            }
        }
        cues.extend(explainer.steps.iter().map(|step| step.cue.clone()));
    }
    (texts, cues)
}

/// Puts `texts` and `cues` where [`shot_words`] read them from; false
/// when there are not as many as the shot holds.
fn set_shot_words(shot: &mut crate::storyboard::Shot, texts: &[String], cues: &[String]) -> bool {
    let (had_texts, had_cues) = shot_words(shot);
    if had_texts.len() != texts.len() || had_cues.len() != cues.len() {
        return false;
    }
    let mut texts = texts.iter();
    let mut next = || texts.next().cloned().unwrap_or_default();
    if let Some(graphic) = &mut shot.graphic {
        graphic.title = next();
        for item in &mut graphic.items {
            item.label = next();
            item.value = next();
        }
    }
    if let Some(explainer) = &mut shot.explainer {
        fn set(elements: &mut [crate::explainer::Element], next: &mut dyn FnMut() -> String) {
            for element in elements {
                if element.kind == crate::explainer::ElementKind::Text {
                    element.text = next();
                }
                set(&mut element.children, next);
            }
        }
        set(&mut explainer.elements, &mut next);
        for (step, cue) in explainer.steps.iter_mut().zip(cues) {
            step.cue = cue.clone();
        }
    }
    true
}

/// A chapter's translated words put in place, or why they cannot be: a
/// scene, shot, text or cue too many or too few.
fn apply_words(chapter: &mut Chapter, got: &Value) -> Result<(), String> {
    let list = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .map(|items| items.iter().map(text).collect())
            .unwrap_or_default()
    };
    let scenes = got["scenes"].as_array().cloned().unwrap_or_default();
    if scenes.len() != chapter.scenes.len() {
        return Err(format!(
            "the translation came back with {} scenes of {}",
            scenes.len(),
            chapter.scenes.len()
        ));
    }
    let title = text(&got["chapterTitle"]);
    if !title.is_empty() {
        chapter.title = title;
    }
    for (scene, words) in chapter.scenes.iter_mut().zip(&scenes) {
        let narration = text(&words["narration"]);
        if narration.is_empty() != scene.narration.trim().is_empty() {
            return Err(format!(
                "scene {} came back without its narration",
                scene.id
            ));
        }
        scene.narration = narration;
        // Marked for the old words: the new ones are read as written until
        // they are marked again.
        scene.reading = None;
        if scene.on_screen_text.is_some() {
            scene.on_screen_text = Some(text(&words["onScreenText"])).filter(|t| !t.is_empty());
        }
        if scene.sticker.is_some() {
            scene.sticker = Some(text(&words["sticker"])).filter(|t| !t.is_empty());
        }
        if !scene.keywords.is_empty() {
            scene.keywords = list(&words["keywords"])
                .into_iter()
                .filter(|word| !word.is_empty())
                .collect();
        }
        if let Some(card) = &mut scene.card {
            let (figure, label) = (text(&words["cardFigure"]), text(&words["cardLabel"]));
            if !figure.is_empty() {
                card.figure = figure;
            }
            card.label = label;
        }
        let shots = words["shots"].as_array().cloned().unwrap_or_default();
        if shots.len() != scene.shots.len() {
            return Err(format!(
                "scene {} came back with {} shots of {}",
                scene.id,
                shots.len(),
                scene.shots.len()
            ));
        }
        for (n, (shot, words)) in scene.shots.iter_mut().zip(&shots).enumerate() {
            if !set_shot_words(shot, &list(&words["texts"]), &list(&words["cues"])) {
                return Err(format!(
                    "scene {} shot {} came back with other texts than it has",
                    scene.id,
                    n + 1
                ));
            }
        }
    }
    Ok(())
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

/// What goes up with the video on YouTube, written for it: the words a
/// person would otherwise write last and in a hurry.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Publishing {
    /// The video's title on YouTube, in the video's language.
    pub title: String,
    /// What it is about, in a few short paragraphs, without the chapters:
    /// [`Publishing::description_with`] adds them from the cut.
    pub description: String,
    /// Search tags, in the video's language and in English.
    pub tags: Vec<String>,
    /// The few words written large on the thumbnail.
    pub thumbnail_text: String,
    /// The thumbnail's picture, in English, as an image prompt.
    pub thumbnail_prompt: String,
    /// When each chapter starts in the cut, by its title: set from the
    /// cut, not by the writer.
    #[serde(default)]
    pub chapters: Vec<(String, f64)>,
}

impl Publishing {
    /// The description with the chapters' start times under it, as YouTube
    /// reads them: when there are three or more, the first at 0:00, and
    /// none shorter than ten seconds - otherwise without them.
    pub fn full_description(&self) -> String {
        let chapters = &self.chapters;
        let usable = chapters.len() >= 3
            && chapters.first().is_some_and(|(_, at)| *at < 0.5)
            && chapters
                .windows(2)
                .all(|pair| pair[1].1 - pair[0].1 >= 10.0);
        let mut out = self.description.trim().to_owned();
        if usable {
            out.push_str("\n\n");
            for (n, (title, at)) in chapters.iter().enumerate() {
                let at = if n == 0 { 0.0 } else { *at };
                out.push_str(&format!("{} {}\n", timestamp(at), title.trim()));
            }
            out = out.trim_end().to_owned();
        }
        out
    }

    /// The tags as YouTube takes them: comma-separated.
    pub fn tag_line(&self) -> String {
        self.tags
            .iter()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Everything, to keep beside the video as a text file.
    pub fn sheet(&self) -> String {
        format!(
            "{}\n\n{}\n\n{}\n",
            self.title.trim(),
            self.full_description(),
            self.tag_line()
        )
    }
}

/// A time as YouTube writes one: 0:00, 12:34, 1:02:03.
pub fn timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Asks the writer for what goes up with the video: a title, a
/// description, tags, and a thumbnail's words and picture.
pub fn publishing(board: &Storyboard, key: &str, model: &str) -> Result<Publishing, String> {
    if key.trim().is_empty() {
        return Err("an Anthropic API key is needed to write the script".to_owned());
    }
    let got =
        crate::providers::retry(|| call(key, model, &publishing_tool(), &publishing_ask(board)))?;
    let tags = got["tags"]
        .as_array()
        .map(|tags| {
            tags.iter()
                .map(text)
                .filter(|tag| !tag.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut description = text(&got["description"]);
    let sources = sources_text(&board.sources);
    if !sources.is_empty() {
        description = format!("{}\n\n{sources}", description.trim_end());
    }
    let publishing = Publishing {
        title: text(&got["title"]),
        description,
        tags,
        thumbnail_text: text(&got["thumbnail_text"]),
        thumbnail_prompt: text(&got["thumbnail_prompt"]),
        chapters: Vec::new(),
    };
    if publishing.title.is_empty() {
        return Err("the writer wrote no title".to_owned());
    }
    Ok(publishing)
}

/// What [`publishing`] asks.
fn publishing_ask(board: &Storyboard) -> String {
    let chapters: String = board
        .chapters
        .iter()
        .map(|chapter| format!("- {}: {}\n", chapter.title.trim(), chapter.summary.trim()))
        .collect();
    let opening: String = board
        .scenes()
        .take(4)
        .map(|scene| scene.narration.trim())
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "The video \"{}\", in {}. Its chapters:\n{chapters}It opens: {opening}\n\n\
         Write what goes up with it on YouTube, in {}:\n\
         - a title of at most 70 characters that makes a person curious without lying \
         about the video, no clickbait capitals, no emoji;\n\
         - a description: two or three short paragraphs on what the viewer will learn \
         and why it matters, the first line able to stand alone in search results; no \
         chapter list, no links, no hashtags;\n\
         - 10 to 15 search tags, in {} and in English, each a few words;\n\
         - the thumbnail's words: at most four, large and bold, that the title does not \
         already say;\n\
         - the thumbnail's picture, in English, as an image prompt. {}",
        board.title.trim(),
        board.language,
        board.language,
        board.language,
        striking(&board.style.visual),
    )
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
        ShotKind::Motion | ShotKind::Explainer => {
            return Err("a drawn shot has no picture to suggest".to_owned());
        }
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
#[allow(clippy::too_many_arguments)]
fn chapter_prompt(
    board: &Storyboard,
    brief_text: &str,
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
        brief_text,
        outline_summary(board),
        board.style.visual,
        board.style.voice,
        i + 1,
        board.chapters.len(),
        chapter.title,
        chapter.summary,
    );
    if !board.style.cast.is_empty() {
        let cast: String = board
            .style
            .cast
            .iter()
            .map(|one| format!("- {}: {}\n", one.name, one.look))
            .collect();
        prompt += &format!(
            "The recurring characters - where one is in a picture, name them in its prompt by \
             this name and they are drawn the same every time; show them doing what the \
             narration says, and bring them back often, so the viewer follows someone:\n{cast}"
        );
    }
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
    let explainers = match brief.style {
        ContentStyle::Educational | ContentStyle::Academic => "up to one shot in two",
        ContentStyle::Documentary | ContentStyle::News => "up to one shot in four",
        ContentStyle::Story => "up to one shot in six",
    };
    prompt += &if brief.cards_only {
        "\nThis video has no pictures: every shot is drawn by the app, whatever kind is listed. \
         Where the narration explains how something works, a process, a cause and its effect, a \
         comparison of sizes or amounts, a route, a structure or a cycle, make it an animated \
         explainer (kind \"explainer\"): its prompt says, in English, what the diagram shows \
         and how it builds, beat by beat, in step with the words. Elsewhere make it an animated \
         card (kind \"motion\") with its graphic: number for a figure, compare for two to four \
         things measured against each other, timeline for dates, list for steps or reasons, quote \
         for a definition or something someone said, title for anything else - a scene's idea in \
         a few strong words. Vary them. Leave the scene's data card null.\n"
            .to_owned()
    } else {
        format!(
            "\nA shot listed as image or video may instead be drawn by the app. Where the narration \
             explains how something works, a process, a cause and its effect, a comparison of sizes \
             or amounts, a route, a structure or a cycle - where a viewer understands by seeing it \
             drawn - make it an animated explainer (kind \"explainer\"), {explainers}: its \
             prompt says, in English, what the diagram shows and how it builds, beat by beat, in \
             step with the words. Where what is said is a single figure, a list of points, a date \
             or a quote, it may be an animated card (kind \"motion\", with its graphic and no \
             prompt), at most one shot in four. Keep pictures for places, people, moments and \
             atmosphere, and never draw the first shot of the video.\n"
        )
    };
    prompt + &avoid_list(board, None)
}

/// What the editor is asked of chapter `i`: its narration, scene by
/// scene, to be tightened without changing what it says or how long it
/// takes to say.
fn edit_prompt(board: &Storyboard, brief_text: &str, i: usize) -> String {
    let chapter = &board.chapters[i];
    let scenes: String = chapter
        .scenes
        .iter()
        .enumerate()
        .map(|(n, scene)| {
            format!(
                "{}. ({} words) {}\n",
                n + 1,
                scene.narration.split_whitespace().count(),
                scene.narration.trim()
            )
        })
        .collect();
    format!(
        "{}\n{}\nYou are now the channel's editor. This is the narration of chapter {} of {}, \
         \"{}\", scene by scene:\n{scenes}\n\
         Edit it the way a senior editor edits a first draft for a channel that lives on \
         retention: a stronger first line, sharper verbs, concrete detail where it is vague, \
         every phrase from the list you never write taken out, nothing said twice, the turn at \
         the end kept. Make it less ordinary, not only clearer: a line any video on the topic \
         could say is rewritten as only this one would; a stock move ('imagine', 'have you \
         ever wondered', a list counted out) is replaced by a scene or a fact; two scenes that \
         open the same way are made to open differently; the narrator's voice is kept, and \
         its one quotable line in the chapter made sharper. Check it as a viewer new to the subject would hear it: a term left \
         unexplained gets its plain words, a step that is skipped is put back, a sentence too \
         long for one breath is split. Check every number, date, name and quote against the \
         research notes above, where there are any: one the notes do not support is corrected \
         from them or taken out. Keep the order, the language and the meaning of each scene; \
         keep each scene within a tenth of its word count, so it still fits its pictures. \
         Return all {} scenes, in order.",
        brief_text,
        outline_summary(board),
        i + 1,
        board.chapters.len(),
        chapter.title,
        chapter.scenes.len(),
    )
}

/// The editor's narration put in place, scene for scene, where it kept
/// to the shape: the same number of scenes, each near its length. A
/// scene that drifted keeps the writer's words, which fit its pictures.
fn apply_edits(scenes: &mut [Scene], edited: &Value) {
    let Some(lines) = edited["narrations"].as_array() else {
        return;
    };
    if lines.len() != scenes.len() {
        log::warn!(
            "the editor returned {} scenes for {}; the draft stands",
            lines.len(),
            scenes.len()
        );
        return;
    }
    for (scene, line) in scenes.iter_mut().zip(lines) {
        let line = text(line);
        let (was, now) = (
            scene.narration.split_whitespace().count() as f64,
            line.split_whitespace().count() as f64,
        );
        if !line.trim().is_empty() && (now - was).abs() <= (was * 0.2).max(3.0) {
            scene.narration = line.trim().to_owned();
        }
    }
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
        // A card may stand where any picture was planned.
        let fits = kinds.len() == wanted.len()
            && kinds
                .iter()
                .zip(&wanted)
                .all(|(kind, want)| kind == want || drawn(*kind));
        if !fits {
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

/// Whether shots of `kind` are drawn by the app rather than made by a
/// model: a card, an explainer.
fn drawn(kind: ShotKind) -> bool {
    matches!(kind, ShotKind::Motion | ShotKind::Explainer)
}

/// Holds `scenes` to `plan` as near as they come: each scene its planned
/// length, each shot its planned kind and hold, a shot too many dropped, a
/// scene's missing shots made from its last, a scene too many - or one with
/// no picture at all - dropped. What still differs is the count the sheet
/// shows against the budget.
fn conform(scenes: Vec<Scene>, plan: &ChapterPlan, cards_only: bool) -> Vec<Scene> {
    scenes
        .into_iter()
        .zip(&plan.scenes)
        .filter_map(|(mut scene, want)| {
            scene.seconds = want.seconds;
            if cards_only {
                // A video of cards alone: a shot that came back as a
                // picture is shown as its scene's idea in words.
                let idea = scene
                    .on_screen_text
                    .clone()
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or_else(|| short(&scene.narration, 6));
                for shot in &mut scene.shots {
                    if shot.kind == ShotKind::Explainer && !shot.prompt.trim().is_empty() {
                        continue;
                    }
                    shot.kind = ShotKind::Motion;
                    if shot.graphic.as_ref().is_none_or(Graphic::is_empty) {
                        shot.graphic = Some(Graphic {
                            kind: GraphicKind::Title,
                            title: idea.clone(),
                            ..Graphic::default()
                        });
                    }
                }
                scene.card = None;
            }
            for shot in &mut scene.shots {
                shot.settle();
            }
            scene.shots.retain(|shot| shot.is_whole());
            if scene.shots.is_empty() {
                return None;
            }
            scene.shots.truncate(want.shots.len());
            while scene.shots.len() < want.shots.len() {
                // Made from the last picture rather than a card, where
                // there is one: the same card twice says nothing new.
                let from = scene
                    .shots
                    .iter()
                    .rposition(|shot| !drawn(shot.kind))
                    .filter(|_| !cards_only)
                    .unwrap_or(scene.shots.len() - 1);
                let mut again = scene.shots[from].clone();
                again.prompt = format!("{}, a different angle and framing", again.prompt.trim());
                scene.shots.push(again);
            }
            for (shot, (kind, seconds)) in scene.shots.iter_mut().zip(&want.shots) {
                shot.seconds = *seconds;
                // A drawn shot stays what it is where it stands; any other
                // shot is the kind planned.
                if !drawn(shot.kind) && !drawn(*kind) {
                    shot.kind = *kind;
                }
                if shot.kind != ShotKind::Video {
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
    call_with(key, model, &[], tool, prompt)
}

/// [`call`], with Anthropic's own tools - web search - offered beside the
/// answer's: the server runs them in the same turn, and a turn it pauses
/// for running long is sent back to carry on.
fn call_with(
    key: &str,
    model: &str,
    server_tools: &[Value],
    tool: &Value,
    prompt: &str,
) -> Result<Value, String> {
    call_as(key, model, SYSTEM, server_tools, tool, prompt, &[])
}

/// [`call_with`] under another standing brief, with PNG pictures shown
/// before the words.
pub(crate) fn call_as(
    key: &str,
    model: &str,
    system: &str,
    server_tools: &[Value],
    tool: &Value,
    prompt: &str,
    pictures: &[Vec<u8>],
) -> Result<Value, String> {
    use base64::Engine as _;
    let blocks = pictures
        .iter()
        .map(|png| {
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": base64::engine::general_purpose::STANDARD.encode(png),
                }
            })
        })
        .collect();
    call_blocks(key, model, system, server_tools, tool, prompt, blocks)
}

/// [`call_as`], with whatever content blocks go before the words: pictures,
/// or a document to read.
fn call_blocks(
    key: &str,
    model: &str,
    system: &str,
    server_tools: &[Value],
    tool: &Value,
    prompt: &str,
    before: Vec<Value>,
) -> Result<Value, String> {
    let name = tool["name"].as_str().unwrap_or_default();
    let mut tools: Vec<Value> = server_tools.to_vec();
    tools.push(tool.clone());
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(600))
        .build();
    let mut content: Vec<Value> = before;
    content.push(json!({
        "type": "text",
        "text": format!("{prompt}\n\nAnswer by calling the {name} tool, once."),
    }));
    let mut messages = vec![json!({"role": "user", "content": content})];
    let mut last = format!("Claude did not answer with the {name}");
    let mut paused = 0;
    let mut tries = 0;
    while tries < 3 {
        tries += 1;
        let body = json!({
            "model": model,
            "max_tokens": 32000,
            "system": system,
            "tools": tools,
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
            // The server's searching ran long: its turn is sent back as it
            // is, and it carries on where it stopped.
            "pause_turn" if paused < 5 => {
                paused += 1;
                tries -= 1;
                messages.push(json!({"role": "assistant", "content": reply["content"].clone()}));
                continue;
            }
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
            if let Some(mut decoded) = json_in(text) {
                decode_json_text(&mut decoded);
                *value = decoded;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(decode_json_text),
        Value::Object(map) => map.values_mut().for_each(decode_json_text),
        _ => {}
    }
}

/// The list or object a text holds, where a tool answer put one in a
/// string: as it is, inside a code fence, or after a line of words - the
/// owner's outlines came back with their chapters so, and read as none.
fn json_in(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let fenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.trim_end().strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let span = |open: char, close: char| {
        let start = fenced.find(open)?;
        let end = fenced.rfind(close)?;
        (start < end).then(|| &fenced[start..=end])
    };
    [Some(fenced), span('[', ']'), span('{', '}')]
        .into_iter()
        .flatten()
        .filter(|candidate| candidate.starts_with('[') || candidate.starts_with('{'))
        .find_map(|candidate| serde_json::from_str::<Value>(candidate).ok())
}

/// The required fields of `tool` that `input` leaves out or empty, or
/// gives as something other than the list the schema asks for: such an
/// answer is asked again rather than taken as having nothing in it.
fn missing(tool: &Value, input: &Value) -> Vec<String> {
    let properties = &tool["input_schema"]["properties"];
    tool["input_schema"]["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|field| match &input[*field] {
            Value::Null => true,
            _ if properties[*field]["type"] == "array" && !input[*field].is_array() => true,
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

/// The recurring characters an outline names: at most four, each with a
/// name and a look.
fn cast_of(value: &Value) -> Vec<Character> {
    value
        .as_array()
        .map(|cast| {
            cast.iter()
                .map(|one| Character {
                    name: text(&one["name"]),
                    look: text(&one["look"]),
                })
                .filter(|one| !one.name.is_empty() && !one.look.is_empty())
                .take(4)
                .collect()
        })
        .unwrap_or_default()
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
                    "description": "One line in English appended to every shot prompt so all shots look like one film, chosen for this topic and this kind of video: the medium (cinematic photograph, photojournalism, archival film, 3D render, illustration), the lens and film or sensor look, the light and the grade. No palette: each chapter brings its own."
                },
                "voice_style": {"type": "string", "description": "How the narrator sounds."},
                "angle": {
                    "type": "string",
                    "description": "In the video's language, two or three sentences: the way into this topic that other videos on it do not take - the person, the moment, the question or the contradiction the whole video is told through - and the image it opens and closes on."
                },
                "cast": {
                    "type": "array",
                    "description": "The recurring characters the viewer follows from picture to picture, when the video has any - a guide, a family, a scientist, a soldier, an everyman who stands for the viewer. Each is drawn the same in every picture that names them. Up to four; none for a video of places and objects.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string", "description": "A short name in English, one word, used in shot prompts to bring them in: \"Omar\", \"Grandma\"."},
                            "look": {"type": "string", "description": "In English, what they look like, fixed: age, build, face, hair, clothes and their colours, one thing they carry or wear that marks them."}
                        },
                        "required": ["name", "look"]
                    }
                },
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
                                "description": "In English, this chapter's own light and palette - name its two or three colours and the time of day - unlike every other chapter's: warm against cold, bright against dark, as the story turns."
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
            "keywords": {
                "type": "array",
                "items": {"type": "string"},
                "description": "One to three single words of this scene's narration, copied exactly as written there, that carry its meaning - the name, the object, the surprising word - shown in colour in the captions so the eye lands on them. Nouns and strong verbs, never 'the' or 'and'."
            },
            "sticker": {
                "type": ["string", "null"],
                "description": "At most one scene in three: two to four words of the narration's language, punchy, with ! or ? - the turn of the scene ('Fined on the spot!', 'Lost forever?') - stuck over the pictures as a tilted label. Null otherwise."
            },
            "card": {
                "type": ["object", "null"],
                "description": "When the scene turns on a number, a date or a comparison, the card the app draws over its pictures: the figure in large type ('$1.2 trillion', '1999', '3x') and a label of a few words in the narration's language. Null otherwise; at most one scene in four.",
                "properties": {
                    "figure": {"type": "string"},
                    "label": {"type": "string"}
                },
                "required": ["figure", "label"]
            },
            "shots": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": {"type": "string", "enum": ["image", "video", "motion", "explainer"]},
                        "prompt": {"type": "string", "description": "In English: the picture; for an explainer, what the diagram shows and how it builds, beat by beat. Empty for a motion card."},
                        "motion": {
                            "type": "string",
                            "description": "For a video shot, in English, how the camera and the subject move. Empty for an image."
                        },
                        "graphic": {
                            "type": ["object", "null"],
                            "description": "For a motion shot only, the animated card the app draws; null otherwise. Words in the narration's language, short: a title under eight words, labels under five.",
                            "properties": {
                                "kind": {
                                    "type": "string",
                                    "enum": ["title", "number", "compare", "timeline", "list", "quote"],
                                    "description": "title: a heading. number: one figure counting up (one item: label and value). compare: two to four items measured against each other, values as numbers. timeline: two to five items, value the date. list: two to five points (labels only). quote: the quote as title, who said it as one item's label."
                                },
                                "title": {"type": "string"},
                                "items": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": {"type": "string"},
                                            "value": {"type": "string", "description": "'4,500', '38%', '1969'; empty for a point."}
                                        },
                                        "required": ["label"]
                                    }
                                },
                                "accent": {"type": "string", "description": "The card's colour as #rrggbb, from the chapter's palette."}
                            },
                            "required": ["kind", "title", "items", "accent"]
                        }
                    },
                    "required": ["kind", "prompt"]
                }
            }
        },
        "required": ["narration", "voice", "shots"]
    })
}

fn research_tool() -> Value {
    json!({
        "name": "research",
        "description": "What the research found: notes citing sources by number, and the sources.",
        "input_schema": {
            "type": "object",
            "properties": {
                "notes": {"type": "string"},
                "sources": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "url": {"type": "string"}
                        },
                        "required": ["title", "url"]
                    }
                }
            },
            "required": ["notes", "sources"]
        }
    })
}

fn edit_tool() -> Value {
    json!({
        "name": "edit",
        "description": "The chapter's narration, edited, one entry per scene in order.",
        "input_schema": {
            "type": "object",
            "properties": {
                "narrations": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["narrations"]
        }
    })
}

fn reading_tool() -> Value {
    json!({
        "name": "reading",
        "description": "Each scene's narration with its vowel marks, one entry per scene in order.",
        "input_schema": {
            "type": "object",
            "properties": {
                "readings": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["readings"]
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

fn publishing_tool() -> Value {
    json!({
        "name": "publishing",
        "description": "What goes up with the video on YouTube.",
        "input_schema": {
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "At most 70 characters."},
                "description": {"type": "string", "description": "Two or three short paragraphs."},
                "tags": {"type": "array", "items": {"type": "string"}},
                "thumbnail_text": {"type": "string", "description": "At most four words."},
                "thumbnail_prompt": {"type": "string", "description": "An image prompt, in English."}
            },
            "required": ["title", "description", "tags", "thumbnail_text", "thumbnail_prompt"]
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

fn hook_tool() -> Value {
    json!({
        "name": "hook",
        "description": "The video's opening hook.",
        "input_schema": {
            "type": "object",
            "properties": {
                "line": {"type": "string", "description": "The spoken sentence, at most 16 words."},
                "words": {"type": "string", "description": "Two to four words shown large."},
                "keywords": {"type": "array", "items": {"type": "string"}},
                "picture": {"type": "string", "description": "The opening image's prompt, in English."},
                "voice": {"type": "string", "description": "How the line is said."}
            },
            "required": ["line", "words", "picture"]
        }
    })
}

fn translation_tool() -> Value {
    let strings = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "name": "translation",
        "description": "The chapter's words, translated, in the shape they were given.",
        "input_schema": {
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "The video's title, translated."},
                "chapterTitle": {"type": "string"},
                "scenes": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "narration": {"type": "string"},
                            "onScreenText": {"type": "string"},
                            "sticker": {"type": "string"},
                            "keywords": strings,
                            "cardFigure": {"type": "string"},
                            "cardLabel": {"type": "string"},
                            "shots": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {"texts": strings, "cues": strings},
                                    "required": ["texts", "cues"]
                                }
                            }
                        },
                        "required": ["narration", "shots"]
                    }
                }
            },
            "required": ["title", "chapterTitle", "scenes"]
        }
    })
}

fn shorts_tool() -> Value {
    json!({
        "name": "shorts",
        "description": "The stretches picked for Shorts, best first.",
        "input_schema": {
            "type": "object",
            "properties": {
                "shorts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "scenes": {"type": "array", "items": {"type": "string"},
                                       "description": "The scenes' ids, in order, in a row."},
                            "hook": {"type": "string", "description": "At most six words."}
                        },
                        "required": ["scenes", "hook"]
                    }
                }
            },
            "required": ["shorts"]
        }
    })
}

fn ideas_tool() -> Value {
    json!({
        "name": "ideas",
        "description": "Videos worth making next.",
        "input_schema": {
            "type": "object",
            "properties": {
                "ideas": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "angle": {"type": "string"}
                        },
                        "required": ["title", "angle"]
                    }
                }
            },
            "required": ["ideas"]
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

    /// A hook is the video's first scene under its own id: its line read,
    /// its words stuck large, one picture; without a line or a picture
    /// there is none.
    #[test]
    fn a_hook_is_a_scene_of_its_own() {
        let got = json!({
            "line": " Una bomba del tamaño de una piña borró una ciudad. ",
            "words": "UNA SOLA BOMBA",
            "keywords": ["piña", " "],
            "picture": "a pineapple-sized metal sphere on a desert floor at dawn",
            "voice": "low, urgent"
        });
        let scene = hook_scene(&got).expect("a hook");
        assert_eq!(scene.id, crate::storyboard::HOOK);
        assert_eq!(
            scene.narration,
            "Una bomba del tamaño de una piña borró una ciudad."
        );
        assert_eq!(scene.sticker.as_deref(), Some("UNA SOLA BOMBA"));
        assert_eq!(scene.keywords, ["piña"]);
        assert_eq!(scene.shots.len(), 1);
        assert_eq!(scene.shots[0].made_as(), ShotKind::Image);
        assert!(hook_scene(&json!({"line": "x", "words": "y", "picture": " "})).is_none());
        assert!(hook_scene(&json!({"words": "y", "picture": "z"})).is_none());

        // It goes in first, and the storyboard still tidies with it there.
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","language":"Spanish","chapters":[{"title":"One","scenes":[
                {"narration":"First.","seconds":5,"shots":[{"prompt":"a red kite"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        board.chapters[0].scenes.insert(0, scene);
        board.tidy().unwrap();
        assert!(board.hooked());
        assert_eq!(board.chapters[0].scenes[1].id, "c01s01");
        let ask = hook_prompt(&board, "Topic: bombs\n");
        assert!(ask.contains("First.") && ask.contains("in Spanish"));
    }

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
                    graphic: None,
                    explainer: None,
                })
                .collect(),
            ..Scene::default()
        }
    }

    /// The editor's words go in where they keep to the scene's length;
    /// a scene that drifted, or an answer of the wrong size, keeps the
    /// draft.
    #[test]
    fn an_editor_keeps_each_scene_to_its_length() {
        let mut scenes = vec![scene(&[]), scene(&[])];
        scenes[0].narration = "one two three four five six seven eight nine ten".into();
        scenes[1].narration = "one two three four five six seven eight nine ten".into();
        apply_edits(
            &mut scenes,
            &json!({"narrations": [
                "un deux trois quatre cinq six sept huit neuf dix onze",
                "far too short"
            ]}),
        );
        assert!(scenes[0].narration.starts_with("un deux"));
        assert!(
            scenes[1].narration.starts_with("one two"),
            "drifted: the draft stands"
        );

        let before = scenes.clone();
        apply_edits(&mut scenes, &json!({"narrations": ["just one"]}));
        assert_eq!(scenes, before, "one scene for two: the draft stands");
    }

    /// The sources go under the description as YouTube shows them; none,
    /// nothing.
    #[test]
    fn the_sources_are_listed_for_the_description() {
        use crate::storyboard::Source;
        assert_eq!(sources_text(&[]), "");
        let listed = sources_text(&[
            Source {
                title: "IMF outlook".into(),
                url: "https://imf.org/a".into(),
            },
            Source {
                title: String::new(),
                url: "https://example.org/b".into(),
            },
        ]);
        assert_eq!(
            listed,
            "Sources:\nIMF outlook - https://imf.org/a\nhttps://example.org/b"
        );
    }

    /// The channel the person described is in every call's brief.
    #[test]
    fn the_brief_carries_the_channel() {
        let brief = Brief {
            topic: "Rome".into(),
            channel: "Name: Tarikh. Opens with: Ahlan.".into(),
            ..Brief::default()
        };
        assert!(brief.text().contains("Name: Tarikh. Opens with: Ahlan."));
        assert!(!Brief::default().text().contains("The channel"));
    }

    /// Several Shorts are picked from one video, each a run that fits and
    /// none sharing a scene; a pick that is neither is passed over.
    #[test]
    fn several_shorts_share_no_scene() {
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","chapters":[{"title":"A","scenes":[
                {"narration":"a","seconds":20,"shots":[{"prompt":"p1"}]},
                {"narration":"b","seconds":20,"shots":[{"prompt":"p2"}]},
                {"narration":"c","seconds":20,"shots":[{"prompt":"p3"}]},
                {"narration":"d","seconds":20,"shots":[{"prompt":"p4"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        let answer = json!([
            {"scenes": ["c01s01", "c01s02"], "hook": "One"},
            {"scenes": ["c01s02", "c01s03"], "hook": "Shares a scene"},
            {"scenes": ["c01s04", "c01s01"], "hook": "Not in a row"},
            {"scenes": ["c01s03", "c01s04"], "hook": "Two"},
            {"scenes": ["c01s04"], "hook": "Taken"}
        ]);
        let picks = picks_of(&board, &answer, 3);
        let hooks: Vec<&str> = picks.iter().map(|pick| pick.hook.as_str()).collect();
        assert_eq!(hooks, ["One", "Two"]);
        assert_eq!(picks_of(&board, &answer, 1).len(), 1);
        assert!(picks_of(&board, &Value::Null, 3).is_empty());
    }

    /// A chapter's words go out and come back in one shape: the narration,
    /// what is written on screen, a card's lines and an explainer's texts
    /// and cues are each put back where they were read; an answer of
    /// another shape is refused and changes nothing it should not.
    #[test]
    fn a_translation_is_put_back_where_its_words_were() {
        let mut board: Storyboard = serde_json::from_str(
            r##"{"title":"Stars","language":"English","chapters":[{"title":"One","scenes":[
                {"narration":"The sun is a star.","onScreenText":"Our star","sticker":"So close!",
                 "keywords":["star"],"card":{"figure":"150 million km","label":"from Earth"},
                 "seconds":6,"shots":[
                   {"prompt":"the sun"},
                   {"kind":"motion","graphic":{"kind":"number","title":"Distance",
                     "items":[{"label":"kilometres","value":"150,000,000"}]}},
                   {"kind":"explainer","prompt":"orbit","explainer":{"elements":[
                     {"id":"g","kind":"group","children":[{"id":"t","kind":"text","text":"Sun","size":60,"x":900,"y":400,"fill":"#FFFFFF"}]},
                     {"id":"c","kind":"circle","x":900,"y":500,"r":50,"fill":"#FFCC00"}],
                     "steps":[{"target":"t","effect":"fade_in","cue":"star"}]}}]}]}]}"##,
        )
        .unwrap();
        board.tidy().unwrap();
        let chapter = &board.chapters[0];
        let words = chapter_words(&board.title, chapter);
        assert_eq!(
            words["scenes"][0]["shots"][1]["texts"],
            json!(["Distance", "kilometres", "150,000,000"])
        );
        assert_eq!(words["scenes"][0]["shots"][2]["texts"], json!(["Sun"]));
        assert_eq!(words["scenes"][0]["shots"][2]["cues"], json!(["star"]));

        let answer = json!({
            "title": "النجوم", "chapterTitle": "واحد",
            "scenes": [{
                "narration": "الشمس نجم.", "onScreenText": "نجمنا", "sticker": "قريبة جدًا!",
                "keywords": ["نجم"], "cardFigure": "150 مليون كم", "cardLabel": "عن الأرض",
                "shots": [
                    {"texts": [], "cues": []},
                    {"texts": ["المسافة", "كيلومتر", "150,000,000"], "cues": []},
                    {"texts": ["الشمس"], "cues": ["نجم"]}
                ]
            }]
        });
        let mut translated = chapter.clone();
        apply_words(&mut translated, &answer).unwrap();
        let scene = &translated.scenes[0];
        assert_eq!(translated.title, "واحد");
        assert_eq!(scene.narration, "الشمس نجم.");
        assert_eq!(scene.on_screen_text.as_deref(), Some("نجمنا"));
        assert_eq!(scene.sticker.as_deref(), Some("قريبة جدًا!"));
        assert_eq!(scene.keywords, ["نجم"]);
        assert_eq!(scene.card.as_ref().unwrap().label, "عن الأرض");
        assert_eq!(
            scene.shots[1].graphic.as_ref().unwrap().items[0].label,
            "كيلومتر"
        );
        let explainer = scene.shots[2].explainer.as_ref().unwrap();
        assert_eq!(explainer.elements[0].children[0].text, "الشمس");
        assert_eq!(explainer.steps[0].cue, "نجم");
        // The pictures are the ones already made.
        assert_eq!(scene.shots[0].prompt, chapter.scenes[0].shots[0].prompt);

        let mut short = answer.clone();
        short["scenes"][0]["shots"][2]["texts"] = json!([]);
        assert!(apply_words(&mut chapter.clone(), &short).is_err());
        short["scenes"] = json!([]);
        assert!(apply_words(&mut chapter.clone(), &short).is_err());
    }

    /// A format is in every call's brief, and none adds nothing.
    #[test]
    fn a_format_is_briefed_in_every_call() {
        for format in [VideoFormat::Minute, VideoFormat::Story, VideoFormat::Lesson] {
            let brief = Brief {
                format,
                ..Brief::default()
            };
            assert!(brief.text().contains("FORMAT - "), "{format:?}");
        }
        assert!(!Brief::default().text().contains("FORMAT"));
        // A shape of the person's own is briefed after any format, and
        // alone when there is none.
        let own = Brief {
            format: VideoFormat::Minute,
            structure: " Open on a joke, then three facts. ".into(),
            ..Brief::default()
        };
        let text = own.text();
        assert!(text.find("FORMAT - ").unwrap() < text.find("Open on a joke").unwrap());
        assert!(text.contains("it wins:\nOpen on a joke, then three facts.\n"));
        assert!(!Brief::default().text().contains("PRODUCER'S OWN SHAPE"));
        assert_eq!(VideoFormat::ALL[0], VideoFormat::Custom);
    }

    /// A fixed picture style is in every call's brief; Auto leaves the
    /// medium to the writer. The outline's cast is read, at most four.
    #[test]
    fn a_picture_style_is_briefed_and_a_cast_read() {
        let flat = Brief {
            pictures: PictureStyle::Flat,
            ..Brief::default()
        };
        assert!(flat.text().contains("Flat 2D vector illustration"));
        assert!(!Brief::default().text().contains("Picture style"));
        let cast = cast_of(&json!([
            {"name": "Omar", "look": "a boy of ten in a red cap"},
            {"name": "", "look": "nobody"},
            {"name": "Grandma", "look": "white scarf, round glasses"}
        ]));
        assert_eq!(cast.len(), 2);
        assert_eq!(cast[1].name, "Grandma");
        assert!(cast_of(&Value::Null).is_empty());
    }

    /// A list put in a string comes out as the list, fenced or after a
    /// line of words; text that holds none stays text.
    #[test]
    fn a_list_written_as_text_is_read_as_the_list() {
        let mut fenced = json!({"chapters": "```json\n[{\"title\": \"One\"}]\n```"});
        decode_json_text(&mut fenced);
        assert_eq!(fenced["chapters"][0]["title"], "One");
        let mut prefaced = json!({"chapters": "Here are the chapters: [{\"title\": \"Two\"}]"});
        decode_json_text(&mut prefaced);
        assert_eq!(prefaced["chapters"][0]["title"], "Two");
        let mut plain = json!({"title": "Rome [the city]"});
        decode_json_text(&mut plain);
        assert_eq!(plain["title"], "Rome [the city]");
    }

    /// A list the schema asks for, given as anything else, is asked for
    /// again.
    #[test]
    fn a_list_given_as_text_is_missing() {
        let tool = outline_tool();
        let answer = json!({
            "title": "Rome",
            "visual_style": "film",
            "voice_style": "calm",
            "chapters": "One, two and three"
        });
        assert_eq!(missing(&tool, &answer), ["chapters"]);
        let good = json!({
            "title": "Rome",
            "visual_style": "film",
            "voice_style": "calm",
            "chapters": [{"title": "One"}]
        });
        assert!(missing(&tool, &good).is_empty());
    }

    /// Arabic narration is given its marks for the voice where the marked
    /// words are the same words; a reading that changed them is not used.
    #[test]
    fn arabic_is_read_with_its_marks_when_the_words_are_kept() {
        assert!(needs_marks("Arabic"));
        assert!(needs_marks("ar"));
        assert!(!needs_marks("English"));
        let mut scenes = vec![
            Scene {
                id: "c01s01".into(),
                narration: "ذهب الولد إلى المدرسة".into(),
                ..Scene::default()
            },
            Scene {
                id: "c01s02".into(),
                narration: "كتب الدرس".into(),
                ..Scene::default()
            },
        ];
        let read = json!({"readings": [
            "ذَهَبَ الوَلَدُ إِلَى المَدْرَسَةِ",
            "كَتَبَ الطالبُ الدَّرْسَ"
        ]});
        apply_readings(&mut scenes, &read);
        assert_eq!(scenes[0].spoken(), "ذَهَبَ الوَلَدُ إِلَى المَدْرَسَةِ");
        // A word added: read as written.
        assert_eq!(scenes[1].reading, None);
        assert_eq!(scenes[1].spoken(), "كتب الدرس");
        // Edited after: the old marks are not read.
        scenes[0].narration = "ذهب الولد".into();
        assert_eq!(scenes[0].spoken(), "ذهب الولد");
    }

    /// Every call is told what kind of video it writes.
    #[test]
    fn the_brief_says_what_the_video_is() {
        let brief = Brief {
            topic: "Rome".into(),
            style: ContentStyle::Educational,
            ..Brief::default()
        };
        assert!(brief.text().contains("An explainer that teaches"));
        assert!(brief.text().contains("bright and curious viewers"));
        let beginners = Brief {
            level: Level::Beginner,
            ..Brief::default()
        };
        assert!(beginners.text().contains("complete beginners"));
        assert_eq!(ContentStyle::ALL[0], ContentStyle::default());
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
        let kept = conform(came, &plan(), false);
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

        let thin = conform(vec![scene(&[("video", "a")])], &plan(), false);
        assert_eq!(thin[0].shots.len(), 2);
        assert!(thin[0].shots[1].prompt.starts_with("a, a different angle"));
    }

    /// An explainer stands where a picture was planned and stays one, with
    /// pictures and in a video drawn alone; a pending one is made as a
    /// still until it is designed.
    #[test]
    fn an_explainer_keeps_its_place() {
        let mut came = vec![
            scene(&[("video", "a"), ("image", "b")]),
            scene(&[("image", "c")]),
        ];
        came[0].shots[0].kind = ShotKind::Explainer;
        came[0].shots[0].prompt = "salt dissolving, ions drifting apart".into();
        assert_eq!(misfit(&came, &plan()), None);
        let kept = conform(came.clone(), &plan(), false);
        assert_eq!(kept[0].shots[0].kind, ShotKind::Explainer);
        assert_eq!(kept[0].shots[0].made_as(), ShotKind::Image);
        assert!(kept[0].shots[0].motion.is_empty());
        let drawn_only = conform(came, &plan(), true);
        assert_eq!(drawn_only[0].shots[0].kind, ShotKind::Explainer);
        assert_eq!(drawn_only[0].shots[1].kind, ShotKind::Motion);
    }

    /// A card may stand where a picture was planned, and is kept there; a
    /// card with nothing on it falls back to its prompt; with cards only,
    /// every shot is a card, the scene's idea where it gave none.
    #[test]
    fn an_animated_card_stands_where_a_picture_was_planned() {
        let card = Graphic {
            kind: GraphicKind::Number,
            title: "Stones".into(),
            items: vec![crate::storyboard::GraphicItem {
                label: "blocks".into(),
                value: "2,300,000".into(),
            }],
            accent: "#C08A2E".into(),
        };
        let mut came = vec![
            scene(&[("video", "a"), ("image", "b")]),
            scene(&[("image", "c")]),
        ];
        came[0].shots[1].kind = ShotKind::Motion;
        came[0].shots[1].graphic = Some(card.clone());
        // A card with nothing to draw: a still of its prompt.
        came[1].shots[0].kind = ShotKind::Motion;
        came[1].shots[0].graphic = Some(Graphic::default());
        assert_eq!(misfit(&came, &plan()), None);
        let kept = conform(came.clone(), &plan(), false);
        assert_eq!(kept[0].shots[1].kind, ShotKind::Motion);
        assert_eq!(kept[0].shots[1].graphic, Some(card));
        assert_eq!(kept[1].shots[0].kind, ShotKind::Image);
        assert_eq!(kept[1].shots[0].graphic, None);

        let mut cards = came;
        cards[1].narration = "The pyramid rose over twenty years of work".into();
        let kept = conform(cards, &plan(), true);
        assert!(
            kept.iter()
                .flat_map(|scene| &scene.shots)
                .all(|shot| shot.kind == ShotKind::Motion && shot.is_whole())
        );
        let idea = kept[1].shots[0].graphic.as_ref().unwrap();
        assert_eq!(idea.kind, GraphicKind::Title);
        assert!(idea.title.starts_with("The pyramid rose"));
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
        let prompt = chapter_prompt(
            &board,
            &brief.text(),
            &brief,
            1,
            &plan(),
            "the siege; the fire",
            "x",
        );
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

    #[test]
    fn chapters_go_under_the_description_only_as_youtube_reads_them() {
        let mut publishing = Publishing {
            title: "T".to_owned(),
            description: "About it.".to_owned(),
            tags: vec!["a".to_owned(), " b ".to_owned(), "".to_owned()],
            ..Publishing::default()
        };
        let chapters = vec![
            ("One".to_owned(), 0.2),
            ("Two".to_owned(), 65.0),
            ("Three".to_owned(), 3725.0),
        ];
        publishing.chapters = chapters.clone();
        assert_eq!(
            publishing.full_description(),
            "About it.\n\n0:00 One\n1:05 Two\n1:02:05 Three"
        );
        // Two chapters, or one under ten seconds: none listed.
        publishing.chapters = chapters[..2].to_vec();
        assert_eq!(publishing.full_description(), "About it.");
        publishing.chapters = vec![
            ("One".to_owned(), 0.0),
            ("Two".to_owned(), 5.0),
            ("Three".to_owned(), 30.0),
        ];
        assert_eq!(publishing.full_description(), "About it.");
        assert_eq!(publishing.tag_line(), "a, b");
    }

    #[test]
    fn the_publishing_ask_is_in_the_videos_language() {
        let board: Storyboard = serde_json::from_str(
            r#"{"title":"Butterfly","language":"Arabic","chapters":[{"title":"One",
                "summary":"chaos","scenes":[{"narration":"Hello.","shots":[]}]}]}"#,
        )
        .unwrap();
        let ask = publishing_ask(&board);
        assert!(ask.contains("in Arabic:"));
        assert!(ask.contains("- One: chaos"));
        assert!(ask.contains("It opens: Hello."));
    }
}
