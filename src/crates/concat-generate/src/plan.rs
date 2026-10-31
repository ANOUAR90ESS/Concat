// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The cut: where every file lands, and the commands that put it there.
//!
//! Planning is arithmetic over lengths, kept apart from the edit so it can
//! be read and tested on its own; [`cut`] then lays the plan down as edit
//! commands on a new timeline, through whatever applies them - the window's
//! session, or a bare [`concat_project::Editor`] in a test.
//!
//! Lanes, bottom to top:
//!
//! ```text
//! 1  the shots, back to back, a cross-fade on each cut
//! 2  the narration, one file a scene
//! 3  the music bed, looped under everything
//! 4  chapter titles
//! +  captions, on a lane of their own minted on top
//! ```

use std::collections::HashMap;

use concat_project::Command;
use concat_project::commands::{ClipMove, ClipPatch, NewMedia, TrimEdge};
use concat_project::model::{KeyEase, KeyProperty, TextStyle, Transition, VideoSettings};

use crate::storyboard::Storyboard;

/// The house style of the cut.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The transition on every cut between shots; None for straight cuts.
    pub transition: Option<String>,
    /// Its length in seconds.
    pub transition_seconds: f64,
    /// The pause after each scene's narration before the next one's:
    /// a breath between paragraphs, not a gap.
    pub scene_pad: f64,
    /// The shortest a scene may be.
    pub min_scene: f64,
    /// Captions timed to the narration.
    pub captions: bool,
    /// At most this many words a caption.
    pub caption_words: usize,
    /// How the captions look.
    pub caption_look: CaptionLook,
    /// Each scene's on-screen text, over its start.
    pub titles: bool,
    /// How long a title stays.
    pub title_seconds: f64,
    /// A slow push in or pull out on every still.
    pub ken_burns: bool,
    /// The music bed's gain.
    pub music_volume: f64,
    /// The fade up from black and down to it, in seconds.
    pub fade_seconds: f64,
    /// Every picture enlarged to fill the frame, cropping what spills over,
    /// rather than fitted inside it: a wide shot on a tall Short.
    pub fill_frame: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            transition: Some("cross-fade".to_owned()),
            transition_seconds: 0.5,
            scene_pad: 0.35,
            min_scene: 3.0,
            captions: true,
            caption_words: 7,
            caption_look: CaptionLook::Classic,
            titles: true,
            title_seconds: 3.0,
            ken_burns: true,
            music_volume: 0.12,
            fade_seconds: 1.0,
            fill_frame: false,
        }
    }
}

/// How the captions look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionLook {
    /// A sentence at a time, small, on a dark plate, low in the frame.
    #[default]
    Classic,
    /// A few words at a time, big and bold with a heavy outline, each card
    /// popping in, every other one in yellow: the look of short-form video.
    Animated,
}

impl CaptionLook {
    /// The most words a card holds in this look; `classic` is the sheet's
    /// own number for the classic one.
    pub fn words(self, classic: usize) -> usize {
        match self {
            CaptionLook::Classic => classic,
            CaptionLook::Animated => 3,
        }
    }
}

/// How slow a shot shorter than its slot may play before it loops instead.
const SLOWEST: f64 = 0.5;

/// What a placement is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A shot on the picture lane.
    Shot,
    /// A scene's narration.
    Voice,
    /// A stretch of the music bed.
    Music,
    /// A scene's on-screen text.
    Title,
    /// A caption card.
    Caption,
    /// A data card: a figure the scene turns on, drawn large.
    Card,
}

/// One clip to lay down.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// What it is.
    pub kind: Kind,
    /// Timeline seconds.
    pub start: f64,
    /// Timeline seconds.
    pub length: f64,
    /// The file, for everything but words.
    pub file: Option<String>,
    /// The file's own length; None for a still.
    pub source: Option<f64>,
    /// Where in the file it starts: a narration's silence before its first
    /// word is left out.
    pub from: f64,
    /// The words, for a title or a caption.
    pub text: Option<String>,
}

/// A word of a narration, and when it is said in its file.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Word {
    /// As written, with its punctuation.
    pub text: String,
    /// Seconds into the file.
    pub start: f64,
    /// Seconds into the file.
    pub end: f64,
}

/// One scene's files, with their lengths: what generation leaves behind.
#[derive(Clone, Debug, Default)]
pub struct SceneFiles {
    /// The narration and its length.
    pub voice: Option<(String, f64)>,
    /// When each of the narration's words is said, when the voice says:
    /// ElevenLabs does. Empty otherwise.
    pub words: Vec<Word>,
    /// Where in the narration's file there is sound, first to last, in
    /// seconds: read from the file, for a voice whose words are not timed.
    pub sounding: Option<(f64, f64)>,
    /// The shots in order, each with its length (None for a still).
    pub shots: Vec<(String, Option<f64>)>,
}

/// The narration as caption cards: sentences, then at most `words` words a
/// card, shared out evenly, so a card is read in a glance.
pub fn caption_cards(text: &str, words: usize) -> Vec<String> {
    let mut cards = Vec::new();
    let mut sentence = Vec::new();
    for word in text.split_whitespace() {
        sentence.push(word);
        if word.ends_with(['.', '!', '?', '؟', '。']) {
            cards.extend(split_evenly(&sentence, words));
            sentence.clear();
        }
    }
    cards.extend(split_evenly(&sentence, words));
    cards
}

fn split_evenly(words: &[&str], most: usize) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    let cards = words.len().div_ceil(most.max(1));
    let per = words.len().div_ceil(cards);
    words.chunks(per).map(|chunk| chunk.join(" ")).collect()
}

/// How long before its first word a narration is started, so the word's
/// first sound is not lost.
const LEAD_IN: f64 = 0.05;
/// How long after its last word a narration runs on: the word's fall.
const TAIL: f64 = 0.15;

/// Each card's start and length over `over` seconds, shared out by its
/// length in letters: for a narration whose words are not timed.
fn shared(cards: &[String], over: f64) -> Vec<(f64, f64)> {
    let chars: usize = cards.iter().map(|card| card.chars().count()).sum();
    let mut at = 0.0;
    cards
        .iter()
        .map(|card| {
            let span = over * card.chars().count() as f64 / chars.max(1) as f64;
            let this = (at, span);
            at += span;
            this
        })
        .collect()
}

/// Each card's start and length from when its words are said: a card
/// comes up as its first word is said and stays until the next one's
/// does, the last until the narration ends at `over`. `from` is where in
/// the file the narration starts. None when the words do not match the
/// cards one for one - a script edited after its narration was made.
fn timed(cards: &[String], words: &[Word], from: f64, over: f64) -> Option<Vec<(f64, f64)>> {
    let counts: Vec<usize> = cards
        .iter()
        .map(|card| card.split_whitespace().count())
        .collect();
    if counts.iter().sum::<usize>() != words.len() || words.is_empty() {
        return None;
    }
    let mut starts = Vec::with_capacity(cards.len());
    let mut n = 0;
    for count in &counts {
        let at = if n == 0 {
            0.0
        } else {
            (words[n].start - from).clamp(0.0, over)
        };
        starts.push(at);
        n += count;
    }
    let mut spans = Vec::with_capacity(cards.len());
    for (i, at) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(over);
        if end <= *at {
            return None;
        }
        spans.push((*at, end - at));
    }
    Some(spans)
}

/// Every placement, in timeline order. `files` maps a scene id to what was
/// made for it; `music` is a bed and its length.
pub fn plan(
    board: &Storyboard,
    files: &HashMap<String, SceneFiles>,
    options: &Options,
    music: Option<(&str, f64)>,
) -> Result<Vec<Placement>, String> {
    let mut out = Vec::new();
    let mut t = 0.0;
    for scene in board.scenes() {
        let made = files.get(&scene.id).cloned().unwrap_or_default();
        if made.shots.is_empty() {
            return Err(format!("scene {} has no shots", scene.id));
        }
        let file_length = made
            .voice
            .as_ref()
            .map(|(_, seconds)| *seconds)
            .filter(|s| *s > 0.0);
        // The silence a voice file opens and closes with is left out: the
        // pause between paragraphs is the pad alone, the same every time.
        let heard = match (made.words.first(), made.words.last()) {
            (Some(first), Some(last)) => Some((first.start, last.end)),
            _ => made.sounding,
        };
        let (from, spoken) = match (file_length, heard) {
            (Some(file), Some((first, last))) if last > first => {
                let from = (first - LEAD_IN).clamp(0.0, file);
                let to = (last + TAIL).min(file);
                (from, Some(to - from).filter(|s| *s > 0.0).or(Some(file)))
            }
            (file, _) => (0.0, file),
        };
        let length = match spoken {
            Some(spoken) => (spoken + options.scene_pad).max(options.min_scene),
            None => scene.seconds.max(options.min_scene),
        };

        // The scene's time shared over its shots in proportion to their own
        // lengths, so a ten-second clip is not squeezed to match a four.
        let even = length / made.shots.len() as f64;
        // A length the probe could not read - none, nought, not a number
        // - counts as an even share, so one odd file cannot leave every
        // later shot without a place (#16's phone run: a clip probed at no
        // length made the shares 0/0).
        let weights: Vec<f64> = made
            .shots
            .iter()
            .map(|(_, source)| source.filter(|s| s.is_finite() && *s > 0.1).unwrap_or(even))
            .collect();
        let total: f64 = weights.iter().sum();
        let mut at = t;
        for (i, (path, source)) in made.shots.iter().enumerate() {
            let slot = if i + 1 == made.shots.len() {
                t + length - at
            } else {
                length * weights[i] / total
            };
            out.push(Placement {
                kind: Kind::Shot,
                start: at,
                length: slot,
                file: Some(path.clone()),
                source: *source,
                from: 0.0,
                text: None,
            });
            at += slot;
        }

        if let (Some((path, _)), Some(spoken)) = (&made.voice, spoken) {
            out.push(Placement {
                kind: Kind::Voice,
                start: t,
                length: spoken,
                file: Some(path.clone()),
                source: file_length,
                from,
                text: None,
            });
        }
        // The captions run with the voice when there is one, and over the
        // scene when there is not: a cut with no narration still says what
        // the script says, as subtitles.
        if options.captions {
            let cards = caption_cards(
                &scene.narration,
                options.caption_look.words(options.caption_words),
            );
            let over = spoken.unwrap_or(length);
            let spans = match spoken.and_then(|_| timed(&cards, &made.words, from, over)) {
                Some(spans) => spans,
                None => shared(&cards, over),
            };
            for (card, (at, span)) in cards.into_iter().zip(spans) {
                out.push(Placement {
                    kind: Kind::Caption,
                    start: t + at,
                    length: span,
                    file: None,
                    source: None,
                    from: 0.0,
                    text: Some(card),
                });
            }
        }
        if options.titles
            && let Some(text) = scene
                .on_screen_text
                .as_ref()
                .filter(|text| !text.trim().is_empty())
        {
            out.push(Placement {
                kind: Kind::Title,
                start: t,
                length: options.title_seconds.min(length),
                file: None,
                source: None,
                from: 0.0,
                text: Some(text.trim().to_owned()),
            });
        }
        // A data card a moment into the scene, held while the point is
        // made: four seconds and a half at most, never past the scene.
        if let Some(card) = scene
            .card
            .as_ref()
            .filter(|card| !card.figure.trim().is_empty())
        {
            let at = CARD_LEAD.min(length / 4.0);
            let text = match card.label.trim() {
                "" => card.figure.trim().to_owned(),
                label => format!("{}\n{label}", card.figure.trim()),
            };
            out.push(Placement {
                kind: Kind::Card,
                start: t + at,
                length: CARD_SECONDS.min(length - at),
                file: None,
                source: None,
                from: 0.0,
                text: Some(text),
            });
        }
        t += length;
    }

    if let Some((path, source)) = music.filter(|(_, source)| *source > 0.1) {
        let mut at = 0.0;
        while at < t - 0.05 {
            out.push(Placement {
                kind: Kind::Music,
                start: at,
                length: source.min(t - at),
                file: Some(path.to_owned()),
                source: Some(source),
                from: 0.0,
                text: None,
            });
            at += source;
        }
    }
    Ok(out)
}

/// How long into its scene a data card comes up, and how long it stays.
const CARD_LEAD: f64 = 0.4;
const CARD_SECONDS: f64 = 4.5;

/// The end of the last placement.
pub fn total_length(placements: &[Placement]) -> f64 {
    placements
        .iter()
        .map(|p| p.start + p.length)
        .fold(0.0, f64::max)
}

/// What the cut is laid down through.
pub trait Editing {
    /// Applies one command; the id it minted, if any.
    fn apply(&mut self, command: Command) -> Option<String>;
    /// The active timeline's lanes, bottom first.
    fn lanes(&self) -> Vec<String>;
    /// The bin item already holding `path`, if one does.
    fn media_of(&self, path: &str) -> Option<String>;
}

/// When each chapter starts in the cut: its first scene's first shot.
/// The plan lays every scene's shots down in order, one placement a shot.
pub fn chapter_starts(board: &Storyboard, placements: &[Placement]) -> Vec<(String, f64)> {
    let mut shots = placements.iter().filter(|p| p.kind == Kind::Shot);
    let mut starts = Vec::new();
    for chapter in &board.chapters {
        let mut first = None;
        for scene in &chapter.scenes {
            for _ in &scene.shots {
                if let Some(shot) = shots.next() {
                    first.get_or_insert(shot.start);
                }
            }
        }
        if let Some(at) = first {
            starts.push((chapter.title.clone(), at));
        }
    }
    starts
}

/// The thumbnail, on a timeline of its own beside the video's, 1280x720 as
/// YouTube takes one: `picture` filling the frame and `words` large over
/// it, yellow outlined in black, to move and change like any title; Save
/// Video Frame As makes it a picture. Returns the timeline's id.
pub fn thumbnail(
    editor: &mut dyn Editing,
    name: &str,
    picture: &NewMedia,
    words: &str,
) -> Result<String, String> {
    let timeline = editor
        .apply(Command::AddTimeline)
        .ok_or("the thumbnail's timeline could not be made")?;
    editor.apply(Command::RenameTimeline {
        timeline_id: timeline.clone(),
        name: name.to_owned(),
    });
    let frame = (1280, 720);
    editor.apply(Command::SetTimelineVideo {
        timeline_id: timeline.clone(),
        video: VideoSettings {
            width: frame.0,
            height: frame.1,
            rate_num: 30,
            rate_den: 1,
            ..VideoSettings::default()
        },
    });
    let lanes = editor.lanes();
    let lane = lanes
        .first()
        .cloned()
        .ok_or("the thumbnail's timeline has no lane")?;
    let media = editor
        .apply(Command::AddMedia {
            item: picture.clone(),
        })
        .or_else(|| editor.media_of(&picture.path))
        .ok_or_else(|| format!("{} could not be imported", picture.path))?;
    let clip = editor
        .apply(Command::AddClip {
            media_id: media.clone(),
            track_id: lane.clone(),
            start: 0.0,
            ripple: false,
        })
        .ok_or("the thumbnail's picture could not be placed")?;
    let place = Placement {
        kind: Kind::Shot,
        start: 0.0,
        length: THUMBNAIL_SECONDS,
        file: Some(picture.path.clone()),
        source: None,
        from: 0.0,
        text: None,
    };
    fit(editor, &clip, &media, &lane, &place, cover(picture, frame));
    if !words.trim().is_empty() {
        editor.apply(Command::AddTextClip {
            track_id: lanes.get(3).cloned(),
            above: false,
            start: 0.0,
            style: Some(TextStyle {
                content: words.trim().to_owned(),
                font_size: 0.14,
                font_weight: 900.0,
                color: "#FFD400".to_owned(),
                stroke_width: 0.012,
                stroke_color: "#000000".to_owned(),
                shadow: true,
                max_width: 0.6,
                ..TextStyle::default()
            }),
            duration: Some(THUMBNAIL_SECONDS),
            offset_y: Some(0.18),
        });
    }
    Ok(timeline)
}

/// How long the thumbnail's timeline runs: long enough to see and work on.
const THUMBNAIL_SECONDS: f64 = 5.0;

/// Lays the plan down on a new timeline named after the video, framed for
/// its aspect: every file imported (`media` maps a path to its probe), then
/// each placement as clips. Returns the new timeline's id, and the files
/// the timeline would not take - left out, and said, rather than losing the
/// whole cut over one of them.
pub fn cut(
    editor: &mut dyn Editing,
    board: &Storyboard,
    placements: &[Placement],
    media: &HashMap<String, NewMedia>,
    options: &Options,
) -> Result<(String, Vec<String>), String> {
    let timeline = editor
        .apply(Command::AddTimeline)
        .ok_or("the new timeline could not be made")?;
    if !board.title.trim().is_empty() {
        editor.apply(Command::RenameTimeline {
            timeline_id: timeline.clone(),
            name: board.title.trim().to_owned(),
        });
    }
    let (width, height) = board.frame();
    editor.apply(Command::SetTimelineVideo {
        timeline_id: timeline.clone(),
        video: VideoSettings {
            width,
            height,
            rate_num: 30,
            rate_den: 1,
            ..VideoSettings::default()
        },
    });
    let lanes = editor.lanes();
    if lanes.len() < 4 {
        return Err("the new timeline has too few lanes".to_owned());
    }
    let lane = |kind: Kind| -> String {
        match kind {
            Kind::Shot => lanes[0].clone(),
            Kind::Voice => lanes[1].clone(),
            Kind::Music => lanes[2].clone(),
            Kind::Title | Kind::Caption | Kind::Card => lanes[3].clone(),
        }
    };

    let mut ids: HashMap<String, String> = HashMap::new();
    let mut media_id = |editor: &mut dyn Editing, path: &str| -> Result<String, String> {
        if let Some(id) = ids.get(path) {
            return Ok(id.clone());
        }
        let item = media
            .get(path)
            .ok_or_else(|| format!("{path} was not probed"))?;
        let id = editor
            .apply(Command::AddMedia { item: item.clone() })
            .or_else(|| editor.media_of(path))
            .ok_or_else(|| format!("{path} could not be imported"))?;
        ids.insert(path.to_owned(), id.clone());
        Ok(id)
    };

    let media_of_path = |path: &str| media.get(path);

    // Media first, words last, captions on a lane of their own on top so
    // they sit over everything and hide in one click.
    let order = [
        Kind::Shot,
        Kind::Voice,
        Kind::Music,
        Kind::Title,
        Kind::Card,
        Kind::Caption,
    ];
    let shots: Vec<&Placement> = placements.iter().filter(|p| p.kind == Kind::Shot).collect();
    let last_music = placements.iter().rposition(|p| p.kind == Kind::Music);
    let mut captions: Option<String> = None;
    let mut figures: Option<String> = None;
    let mut cards = 0usize;
    let mut left_out = Vec::new();
    for kind in order {
        for (index, p) in placements
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == kind)
        {
            match kind {
                Kind::Shot | Kind::Voice | Kind::Music => {
                    let file = p.file.as_deref().unwrap_or_default();
                    let placed = media_id(editor, file).and_then(|media| {
                        if !p.start.is_finite() || !p.length.is_finite() {
                            return Err(format!("{file} has no time to go at"));
                        }
                        editor
                            .apply(Command::AddClip {
                                media_id: media.clone(),
                                track_id: lane(kind),
                                start: p.start,
                                ripple: false,
                            })
                            .map(|clip| (media, clip))
                            .ok_or_else(|| format!("{file} could not be placed"))
                    });
                    let (media, clip) = match placed {
                        Ok(placed) => placed,
                        Err(error) => {
                            log::warn!("generate: {error}");
                            left_out.push(file.to_owned());
                            continue;
                        }
                    };
                    // How much larger than fitted a picture is drawn to
                    // fill the frame, when the cut asks it to.
                    let fill = if kind == Kind::Shot && options.fill_frame {
                        media_of_path(file).map_or(1.0, |item| cover(item, (width, height)))
                    } else {
                        1.0
                    };
                    fit(editor, &clip, &media, &lane(kind), p, fill);
                    let mut patch = ClipPatch::default();
                    if kind == Kind::Shot {
                        let n = shots
                            .iter()
                            .position(|shot| std::ptr::eq(*shot, p))
                            .unwrap_or(0);
                        if n > 0
                            && let Some(id) = &options.transition
                        {
                            patch.transition_in = Some(Some(Transition {
                                id: id.clone(),
                                duration: options.transition_seconds,
                            }));
                        }
                        if p.source.is_none() && options.ken_burns {
                            let (from, to) = if n % 2 == 0 { (1.0, 1.12) } else { (1.12, 1.0) };
                            key(
                                editor,
                                &clip,
                                KeyProperty::Scale,
                                &[(0.0, from * fill), (1.0, to * fill)],
                            );
                        }
                        fade(
                            editor,
                            &clip,
                            p.length,
                            options.fade_seconds,
                            n == 0,
                            n + 1 == shots.len(),
                        );
                    } else if kind == Kind::Music {
                        patch.volume = Some(options.music_volume);
                        if Some(index) == last_music {
                            patch.fade_out = Some(p.length.min(3.0));
                        }
                    }
                    if patch != ClipPatch::default() {
                        editor.apply(Command::UpdateClip {
                            clip_id: clip,
                            patch,
                        });
                    }
                }
                Kind::Title => {
                    editor.apply(Command::AddTextClip {
                        track_id: Some(lane(kind)),
                        above: false,
                        start: p.start,
                        style: Some(title_style(p.text.clone().unwrap_or_default())),
                        duration: Some(p.length),
                        offset_y: Some(-0.3),
                    });
                }
                // On a lane of their own, under the captions: a card and
                // the words being said can be on screen together.
                Kind::Card => {
                    if figures.is_none() {
                        figures = editor.apply(Command::AddTrack { bottom: false });
                    }
                    let card = editor.apply(Command::AddTextClip {
                        track_id: figures.clone(),
                        above: true,
                        start: p.start,
                        style: Some(card_style(p.text.clone().unwrap_or_default())),
                        duration: Some(p.length),
                        offset_y: Some(-0.04),
                    });
                    if let Some(card) = card {
                        pop(editor, &card, p.length);
                    }
                }
                Kind::Caption => {
                    if captions.is_none() {
                        captions = editor.apply(Command::AddTrack { bottom: false });
                    }
                    let text = p.text.clone().unwrap_or_default();
                    let (style, offset_y) = match options.caption_look {
                        CaptionLook::Classic => (caption_style(text), 0.38),
                        CaptionLook::Animated => (animated_style(text, cards % 2 == 1), 0.24),
                    };
                    let card = editor.apply(Command::AddTextClip {
                        track_id: captions.clone(),
                        above: true,
                        start: p.start,
                        style: Some(style),
                        duration: Some(p.length),
                        offset_y: Some(offset_y),
                    });
                    if options.caption_look == CaptionLook::Animated
                        && let Some(card) = card
                    {
                        pop(editor, &card, p.length);
                    }
                    cards += 1;
                }
            }
        }
    }
    // A lane the plan put nothing on is taken away: a cut with no
    // narration or music would otherwise show its pictures under empty
    // rows, out of sight on a phone's short stack of lanes.
    for (kind, index) in [(Kind::Voice, 1), (Kind::Music, 2), (Kind::Title, 3)] {
        if !placements.iter().any(|p| p.kind == kind) {
            editor.apply(Command::RemoveTrack {
                track_id: lanes[index].clone(),
            });
        }
    }
    Ok((timeline, left_out))
}

/// Makes a clip exactly as long as its slot: trimmed when the file is
/// longer, stretched when it is a still, and slowed (down to [`SLOWEST`])
/// when it is a shorter video - which then plays again from its top for
/// whatever is still left, the way an editor loops b-roll.
fn fit(editor: &mut dyn Editing, clip: &str, media: &str, lane: &str, p: &Placement, fill: f64) {
    if fill > 1.001 {
        editor.apply(Command::SetClipTransform {
            clip_id: clip.to_owned(),
            scale: Some(fill),
            offset_x: None,
            offset_y: None,
            rotation: None,
            stretch_x: None,
            stretch_y: None,
        });
    }
    // A clip whose length the probe could not read is fitted as a still
    // is: trimmed to its place, never slowed by a speed of nought.
    let source = p.source.filter(|s| s.is_finite() && *s > 0.1);
    let still = source.is_none();
    let mut natural = source.unwrap_or(5.0);
    // A narration starting into its file: the head trimmed off, which
    // moves the clip on by as much, and the clip moved back to its place.
    if p.from > 1e-3 && !still && p.from < natural {
        editor.apply(Command::TrimClip {
            clip_id: clip.to_owned(),
            edge: TrimEdge::Start,
            delta: p.from,
            ripple: false,
        });
        editor.apply(Command::MoveClips {
            moves: vec![ClipMove {
                clip_id: clip.to_owned(),
                start: p.start,
                track_id: lane.to_owned(),
            }],
        });
        natural -= p.from;
    }
    if p.kind == Kind::Shot && !still && natural < p.length {
        let speed = (natural / p.length).max(SLOWEST);
        editor.apply(Command::SetClipSpeed {
            clip_id: clip.to_owned(),
            speed,
        });
        natural /= speed;
    }
    let delta = p.length - natural;
    if delta < -1e-3 || (still && delta.abs() > 1e-3) {
        editor.apply(Command::TrimClip {
            clip_id: clip.to_owned(),
            edge: TrimEdge::End,
            delta,
            ripple: false,
        });
    } else if delta > 0.05 && p.kind == Kind::Shot {
        let start = p.start + natural;
        if let Some(again) = editor.apply(Command::AddClip {
            media_id: media.to_owned(),
            track_id: lane.to_owned(),
            start,
            ripple: false,
        }) {
            let rest = Placement {
                start,
                length: delta,
                ..p.clone()
            };
            fit(editor, &again, media, lane, &rest, fill);
        }
    }
}

fn key(editor: &mut dyn Editing, clip: &str, property: KeyProperty, keys: &[(f64, f64)]) {
    for &(at, value) in keys {
        editor.apply(Command::SetClipKey {
            clip_id: clip.to_owned(),
            property,
            at,
            value,
            ease: KeyEase::LINEAR,
        });
    }
}

/// The film's fade up from black and down to it, as opacity keys.
fn fade(editor: &mut dyn Editing, clip: &str, length: f64, seconds: f64, first: bool, last: bool) {
    if length <= 0.0 || seconds <= 0.0 {
        return;
    }
    let edge = (seconds / length).min(0.45);
    let mut keys = Vec::new();
    if first {
        keys.extend([(0.0, 0.0), (edge, 1.0)]);
    }
    if last {
        keys.extend([(1.0 - edge, 1.0), (1.0, 0.0)]);
    }
    key(editor, clip, KeyProperty::Opacity, &keys);
}

/// A chapter title: large, bold, shadowed.
fn title_style(content: String) -> TextStyle {
    TextStyle {
        content,
        font_size: 0.075,
        font_weight: 800.0,
        shadow: true,
        max_width: 0.86,
        ..TextStyle::default()
    }
}

/// A data card: the figure large and heavy in yellow, its label under
/// it, on a dark plate that reads over any picture.
fn card_style(content: String) -> TextStyle {
    TextStyle {
        content,
        font_size: 0.085,
        font_weight: 900.0,
        color: "#FFD400".to_owned(),
        shadow: false,
        background: "#000000bf".to_owned(),
        background_radius: 0.018,
        background_padding_x: 0.04,
        background_padding_y: 0.024,
        max_width: 0.8,
        ..TextStyle::default()
    }
}

/// A caption: small, on a dark plate. Sizes are fractions of the frame's
/// height, as titles measure them.
fn caption_style(content: String) -> TextStyle {
    TextStyle {
        content,
        font_size: 0.048,
        font_weight: 700.0,
        shadow: false,
        background: "#000000a6".to_owned(),
        background_radius: 0.01,
        background_padding_x: 0.022,
        background_padding_y: 0.012,
        ..TextStyle::default()
    }
}

/// How much larger than fitted a picture of `item`'s shape is drawn to fill
/// a `frame` - the ratio of the two shapes, either way round; 1 for the
/// same shape, or a size the probe did not read.
fn cover(item: &NewMedia, frame: (u32, u32)) -> f64 {
    match (item.width, item.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 && frame.0 > 0 && frame.1 > 0 => {
            let ratio = (w as f64 / h as f64) / (frame.0 as f64 / frame.1 as f64);
            ratio.max(1.0 / ratio).min(8.0)
        }
        _ => 1.0,
    }
}

/// An animated caption: big, heavy, outlined so it reads over any picture
/// without a plate; `accent` cards in yellow, the rest in white.
fn animated_style(content: String, accent: bool) -> TextStyle {
    TextStyle {
        content,
        font_size: 0.075,
        font_weight: 900.0,
        color: if accent { "#FFD400" } else { "#FFFFFF" }.to_owned(),
        stroke_width: 0.008,
        stroke_color: "#000000".to_owned(),
        shadow: true,
        max_width: 0.86,
        ..TextStyle::default()
    }
}

/// A caption card's entrance: it pops - in from small and faint, a touch
/// past full size, then settles - over its first quarter second whatever
/// the card's length, so short cards and long ones land alike.
fn pop(editor: &mut dyn Editing, card: &str, length: f64) {
    if length <= 0.0 {
        return;
    }
    let at = |seconds: f64| (seconds / length).min(0.45);
    let keys = [
        (KeyProperty::Scale, 0.0, 0.6, KeyEase::LINEAR),
        (KeyProperty::Scale, at(0.12), 1.08, KeyEase::OUT),
        (KeyProperty::Scale, at(0.24), 1.0, KeyEase::IN_OUT),
        (KeyProperty::Opacity, 0.0, 0.0, KeyEase::LINEAR),
        (KeyProperty::Opacity, at(0.1), 1.0, KeyEase::OUT),
    ];
    for (property, at, value, ease) in keys {
        editor.apply(Command::SetClipKey {
            clip_id: card.to_owned(),
            property,
            at,
            value,
            ease,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::Editor;
    use concat_project::model::{ClipKind, MediaKind};

    fn board() -> Storyboard {
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"Desert","aspect":"9:16","chapters":[{"title":"One","scenes":[
                {"narration":"First sentence here. Second one.","onScreenText":"Chapter one",
                 "seconds":6,"shots":[{"prompt":"a"},{"prompt":"b"}]},
                {"narration":"","seconds":2,"shots":[{"prompt":"c"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        board
    }

    fn files() -> HashMap<String, SceneFiles> {
        HashMap::from([
            (
                "c01s01".to_owned(),
                SceneFiles {
                    voice: Some(("/v1.wav".into(), 5.0)),
                    shots: vec![("/a.mp4".into(), Some(4.0)), ("/b.mp4".into(), Some(6.0))],
                    ..SceneFiles::default()
                },
            ),
            (
                "c01s02".to_owned(),
                SceneFiles {
                    voice: None,
                    shots: vec![("/c.png".into(), None)],
                    ..SceneFiles::default()
                },
            ),
        ])
    }

    fn of(placements: &[Placement], kind: Kind) -> Vec<&Placement> {
        placements.iter().filter(|p| p.kind == kind).collect()
    }

    #[test]
    fn shots_fill_each_scene_back_to_back() {
        let placements = plan(
            &board(),
            &files(),
            &Options::default(),
            Some(("/m.mp3", 4.0)),
        )
        .unwrap();
        let shots = of(&placements, Kind::Shot);
        // Scene one is its narration and the pad, shared 4:6.
        assert!((shots[0].length - 5.35 * 0.4).abs() < 1e-9);
        assert!((shots[1].start + shots[1].length - 5.35).abs() < 1e-9);
        // Scene two has no narration: its seconds, floored.
        assert!((shots[2].start - 5.35).abs() < 1e-9);
        assert!((shots[2].length - 3.0).abs() < 1e-9);
        let captions = of(&placements, Kind::Caption);
        let words: Vec<_> = captions
            .iter()
            .map(|c| c.text.as_deref().unwrap())
            .collect();
        assert_eq!(words, ["First sentence here.", "Second one."]);
        let end = captions.last().map(|c| c.start + c.length).unwrap();
        assert!((end - 5.0).abs() < 1e-9);
        let music: Vec<_> = of(&placements, Kind::Music)
            .iter()
            .map(|m| m.start)
            .collect();
        assert_eq!(music, [0.0, 4.0, 8.0]);
        assert!((total_length(&placements) - 8.35).abs() < 1e-9);
    }

    #[test]
    fn without_narration_the_captions_run_over_the_scene() {
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let placements = plan(&board(), &files, &Options::default(), None).unwrap();
        assert!(of(&placements, Kind::Voice).is_empty());
        let captions = of(&placements, Kind::Caption);
        let words: Vec<_> = captions
            .iter()
            .map(|c| c.text.as_deref().unwrap())
            .collect();
        assert_eq!(words, ["First sentence here.", "Second one."]);
        // The scene's written six seconds, shared by the cards' lengths.
        assert_eq!(captions[0].start, 0.0);
        let end = captions.last().map(|c| c.start + c.length).unwrap();
        assert!((end - 6.0).abs() < 1e-9);
        let off = plan(
            &board(),
            &files,
            &Options {
                captions: false,
                ..Options::default()
            },
            None,
        )
        .unwrap();
        assert!(of(&off, Kind::Caption).is_empty());
    }

    /// A scene that turns on a figure gets its card a moment in, the
    /// figure over its label, held a few seconds and never past the scene.
    #[test]
    fn a_figure_is_carded_over_its_scene() {
        let mut board = board();
        let scene = board.chapters[0].scenes.first_mut().unwrap();
        scene.card = Some(crate::storyboard::Card {
            figure: "$1.2 trillion".into(),
            label: "spent in 2025".into(),
        });
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let placements = plan(&board, &files, &Options::default(), None).unwrap();
        let cards = of(&placements, Kind::Card);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].text.as_deref(), Some("$1.2 trillion\nspent in 2025"));
        assert!((cards[0].start - CARD_LEAD).abs() < 1e-9);
        assert!(cards[0].start + cards[0].length <= 6.0 + 1e-9);
        assert!(of(&plan(&self::board(), &files, &Options::default(), None).unwrap(), Kind::Card).is_empty());
    }

    #[test]
    fn arabic_question_marks_end_a_caption() {
        assert_eq!(caption_cards("ما هذا؟ نجم.", 7), ["ما هذا؟", "نجم."]);
        let long: String = (0..10).map(|i| format!("w{i} ")).collect();
        let sizes: Vec<_> = caption_cards(&long, 7)
            .iter()
            .map(|c| c.split(' ').count())
            .collect();
        assert_eq!(sizes, [5, 5]);
    }

    /// The window's session, as far as the cut needs it.
    struct Bare(Editor);

    impl Editing for Bare {
        fn apply(&mut self, command: Command) -> Option<String> {
            self.0
                .apply(command)
                .ok()
                .and_then(|outcome| outcome.created_id)
        }
        fn lanes(&self) -> Vec<String> {
            self.0
                .project()
                .active()
                .tracks
                .iter()
                .map(|t| t.id.clone())
                .collect()
        }
        fn media_of(&self, path: &str) -> Option<String> {
            self.0
                .project()
                .media
                .iter()
                .find(|m| m.path == path)
                .map(|m| m.id.clone())
        }
    }

    fn probed(path: &str, kind: MediaKind, duration: Option<f64>) -> NewMedia {
        NewMedia {
            path: path.to_owned(),
            name: path.trim_start_matches('/').to_owned(),
            duration,
            kind,
            width: (kind != MediaKind::Audio).then_some(1080),
            height: (kind != MediaKind::Audio).then_some(1920),
            frame_rate: (kind == MediaKind::Video).then_some(30.0),
            frame_rate_fraction: None,
            video_codec: None,
            audio_codec: None,
            has_audio: kind == MediaKind::Audio,
            audio_tracks: Vec::new(),
            origin: None,
            color_space: Default::default(),
        }
    }

    #[test]
    fn lanes_left_empty_are_taken_away() {
        let board = board();
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let options = Options {
            titles: false,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let media = HashMap::from([
            (
                "/a.mp4".to_owned(),
                probed("/a.mp4", MediaKind::Video, Some(4.0)),
            ),
            (
                "/b.mp4".to_owned(),
                probed("/b.mp4", MediaKind::Video, Some(6.0)),
            ),
            (
                "/c.png".to_owned(),
                probed("/c.png", MediaKind::Image, None),
            ),
        ]);
        let mut editor = Bare(Editor::default());
        let (_, left_out) = cut(&mut editor, &board, &placements, &media, &options).unwrap();
        assert!(left_out.is_empty());
        let active = editor.0.project().active();
        // The pictures and the captions, and no lane between them.
        assert_eq!(active.tracks.len(), 2, "{:?}", active.tracks);
        for track in &active.tracks {
            assert!(active.clips.iter().any(|c| c.track_id == track.id));
        }
    }

    #[test]
    fn the_cut_lands_on_a_new_timeline_of_its_own() {
        let board = board();
        let placements =
            plan(&board, &files(), &Options::default(), Some(("/m.mp3", 4.0))).unwrap();
        let media = HashMap::from([
            (
                "/v1.wav".to_owned(),
                probed("/v1.wav", MediaKind::Audio, Some(5.0)),
            ),
            (
                "/a.mp4".to_owned(),
                probed("/a.mp4", MediaKind::Video, Some(4.0)),
            ),
            (
                "/b.mp4".to_owned(),
                probed("/b.mp4", MediaKind::Video, Some(6.0)),
            ),
            (
                "/c.png".to_owned(),
                probed("/c.png", MediaKind::Image, None),
            ),
            (
                "/m.mp3".to_owned(),
                probed("/m.mp3", MediaKind::Audio, Some(4.0)),
            ),
        ]);
        let mut editor = Bare(Editor::default());
        let before = editor.0.project().timelines.len();
        let (timeline, left_out) = cut(
            &mut editor,
            &board,
            &placements,
            &media,
            &Options::default(),
        )
        .unwrap();

        assert!(left_out.is_empty());
        let project = editor.0.project();
        assert_eq!(project.timelines.len(), before + 1);
        let active = project.active();
        assert_eq!(active.id, timeline);
        assert_eq!(active.name, "Desert");
        assert_eq!((active.video.width, active.video.height), (1080, 1920));
        assert_eq!(project.media.len(), 5);

        let lanes: Vec<_> = active.tracks.iter().map(|t| t.id.clone()).collect();
        let on = |lane: usize| -> Vec<_> {
            let mut clips: Vec<_> = active
                .clips
                .iter()
                .filter(|c| c.track_id == lanes[lane])
                .collect();
            clips.sort_by(|a, b| a.start.total_cmp(&b.start));
            clips
        };
        // The shots tile the picture lane exactly, with a transition on
        // every cut but the first.
        let pictures = on(0);
        assert_eq!(pictures.len(), 3);
        let mut at = 0.0;
        for clip in &pictures {
            assert!((clip.start - at).abs() < 1e-6, "{} at {at}", clip.start);
            at = clip.start + clip.duration;
        }
        assert!((at - 8.35).abs() < 1e-6);
        assert!(pictures[0].transition_in.is_none());
        assert!(pictures[1].transition_in.is_some());
        // The music is ducked and fades out at the very end.
        let music = on(2);
        assert_eq!(music.len(), 3);
        assert!(music.iter().all(|m| (m.volume - 0.12).abs() < 1e-9));
        assert!((music[2].start + music[2].duration - 8.35).abs() < 1e-6);
        // A title on lane four, captions on a lane minted on top.
        assert_eq!(on(3).len(), 1);
        assert_eq!(on(4).len(), 2);
        assert!(on(4).iter().all(|c| c.kind == ClipKind::Text));
    }

    #[test]
    fn a_clip_probed_at_no_length_still_leaves_every_shot_a_place() {
        let mut files = files();
        for (_, source) in &mut files.get_mut("c01s01").unwrap().shots {
            *source = Some(0.0);
        }
        let placements = plan(&board(), &files, &Options::default(), None).unwrap();
        let shots = of(&placements, Kind::Shot);
        assert!(!shots.is_empty());
        for shot in shots {
            assert!(
                shot.start.is_finite() && shot.length.is_finite(),
                "{shot:?}"
            );
            assert!(shot.length > 0.0, "{shot:?}");
        }
    }

    #[test]
    fn animated_captions_pop_in_a_few_words_at_a_time() {
        let board = board();
        let options = Options {
            caption_look: CaptionLook::Animated,
            ..Options::default()
        };
        let placements = plan(&board, &files(), &options, None).unwrap();
        let captions = of(&placements, Kind::Caption);
        let words: Vec<_> = captions
            .iter()
            .map(|c| c.text.as_deref().unwrap())
            .collect();
        // Three words at most a card, sentences kept apart.
        assert_eq!(words, ["First sentence here.", "Second one."]);
        assert!(words.iter().all(|w| w.split_whitespace().count() <= 3));

        let media = HashMap::from([
            (
                "/v1.wav".to_owned(),
                probed("/v1.wav", MediaKind::Audio, Some(5.0)),
            ),
            (
                "/a.mp4".to_owned(),
                probed("/a.mp4", MediaKind::Video, Some(4.0)),
            ),
            (
                "/b.mp4".to_owned(),
                probed("/b.mp4", MediaKind::Video, Some(6.0)),
            ),
            (
                "/c.png".to_owned(),
                probed("/c.png", MediaKind::Image, None),
            ),
        ]);
        let mut editor = Bare(Editor::default());
        let (_, left_out) = cut(&mut editor, &board, &placements, &media, &options).unwrap();
        assert!(left_out.is_empty());
        let active = editor.0.project().active();
        let mut cards: Vec<_> = active
            .clips
            .iter()
            .filter(|c| {
                c.text
                    .as_ref()
                    .is_some_and(|t| words.contains(&t.content.as_str()))
            })
            .collect();
        cards.sort_by(|a, b| a.start.total_cmp(&b.start));
        assert_eq!(cards.len(), 2);
        // Every other card in yellow; each pops: small, past full, settled.
        let colours: Vec<_> = cards
            .iter()
            .map(|c| c.text.as_ref().unwrap().color.clone())
            .collect();
        assert_eq!(colours, ["#FFFFFF", "#FFD400"]);
        for card in cards {
            let scale: Vec<f64> = card
                .keys
                .iter()
                .filter(|k| k.property == KeyProperty::Scale)
                .map(|k| k.value)
                .collect();
            assert_eq!(scale, [0.6, 1.08, 1.0]);
            assert!(card.text.as_ref().unwrap().stroke_width > 0.0);
        }
    }

    #[test]
    fn a_wide_picture_fills_a_tall_frame_when_asked() {
        let wide = NewMedia {
            width: Some(1920),
            height: Some(1080),
            ..probed("/w.png", MediaKind::Image, None)
        };
        let tall = (1080, 1920);
        let expected = (1920.0 / 1080.0) / (1080.0 / 1920.0);
        assert!((cover(&wide, tall) - expected).abs() < 1e-9);
        assert_eq!(cover(&wide, (1920, 1080)), 1.0);
        let unknown = NewMedia {
            width: None,
            ..wide.clone()
        };
        assert_eq!(cover(&unknown, tall), 1.0);

        // On the cut: the still's push-in keys are scaled by the fill.
        let board = board();
        let mut files = HashMap::new();
        files.insert(
            "c01s01".to_owned(),
            SceneFiles {
                voice: None,
                shots: vec![("/w.png".to_owned(), None)],
                ..SceneFiles::default()
            },
        );
        files.insert(
            "c01s02".to_owned(),
            SceneFiles {
                voice: None,
                shots: vec![("/w.png".to_owned(), None)],
                ..SceneFiles::default()
            },
        );
        let options = Options {
            fill_frame: true,
            captions: false,
            titles: false,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let media = HashMap::from([("/w.png".to_owned(), wide)]);
        let mut editor = Bare(Editor::default());
        cut(&mut editor, &board, &placements, &media, &options).unwrap();
        let active = editor.0.project().active();
        let first = active
            .clips
            .iter()
            .min_by(|a, b| a.start.total_cmp(&b.start))
            .unwrap();
        let scale: Vec<f64> = first
            .keys
            .iter()
            .filter(|k| k.property == KeyProperty::Scale)
            .map(|k| k.value)
            .collect();
        assert!((scale[0] - expected).abs() < 1e-6, "{scale:?}");
        assert!((first.scale - expected).abs() < 1e-6, "{}", first.scale);
    }

    /// The test board's first narration as ElevenLabs times it: half a
    /// second of silence before it, and nearly a second after.
    fn timed_words() -> Vec<Word> {
        [
            ("First", 0.5, 0.9),
            ("sentence", 0.9, 1.5),
            ("here.", 1.5, 2.0),
            ("Second", 2.6, 3.1),
            ("one.", 3.1, 4.1),
        ]
        .into_iter()
        .map(|(text, start, end)| Word {
            text: text.to_owned(),
            start,
            end,
        })
        .collect()
    }

    #[test]
    fn timed_words_leave_out_the_voices_silence_and_time_the_captions() {
        let mut files = files();
        files.get_mut("c01s01").unwrap().words = timed_words();
        let placements = plan(&board(), &files, &Options::default(), None).unwrap();
        let voice = of(&placements, Kind::Voice)[0];
        // From just before the first word to just after the last.
        assert!((voice.from - 0.45).abs() < 1e-9);
        assert!((voice.length - (4.25 - 0.45)).abs() < 1e-9);
        // The scene is the speech and the pause, no more.
        let shots = of(&placements, Kind::Shot);
        assert!((shots[2].start - (3.8 + 0.35)).abs() < 1e-9);
        // A card comes up as its first word is said.
        let captions = of(&placements, Kind::Caption);
        assert_eq!(captions[0].start, 0.0);
        assert!((captions[1].start - (2.6 - 0.45)).abs() < 1e-9);
        assert!((captions[1].start + captions[1].length - 3.8).abs() < 1e-9);
    }

    #[test]
    fn words_that_no_longer_match_the_script_fall_back_to_sharing() {
        let mut files = files();
        let mut words = timed_words();
        words.pop();
        files.get_mut("c01s01").unwrap().words = words;
        let placements = plan(&board(), &files, &Options::default(), None).unwrap();
        let captions = of(&placements, Kind::Caption);
        let voice = of(&placements, Kind::Voice)[0];
        // Shared by letters over the speech: 20 of 31 letters, then 11.
        assert!((captions[1].start - voice.length * 20.0 / 31.0).abs() < 1e-9);
    }

    #[test]
    fn a_narration_is_laid_down_from_its_first_word() {
        let board = board();
        let mut files = files();
        files.get_mut("c01s01").unwrap().words = timed_words();
        let placements = plan(&board, &files, &Options::default(), None).unwrap();
        let media = HashMap::from([
            (
                "/v1.wav".to_owned(),
                probed("/v1.wav", MediaKind::Audio, Some(5.0)),
            ),
            (
                "/a.mp4".to_owned(),
                probed("/a.mp4", MediaKind::Video, Some(4.0)),
            ),
            (
                "/b.mp4".to_owned(),
                probed("/b.mp4", MediaKind::Video, Some(6.0)),
            ),
            (
                "/c.png".to_owned(),
                probed("/c.png", MediaKind::Image, None),
            ),
        ]);
        let mut editor = Bare(Editor::default());
        let (_, left_out) = cut(
            &mut editor,
            &board,
            &placements,
            &media,
            &Options::default(),
        )
        .unwrap();
        assert!(left_out.is_empty());
        let active = editor.0.project().active();
        let voice = active
            .clips
            .iter()
            .find(|c| c.kind == ClipKind::Audio)
            .expect("the narration is on the timeline");
        assert!(voice.start.abs() < 1e-6, "{}", voice.start);
        assert!((voice.source_start - 0.45).abs() < 1e-6);
        assert!((voice.duration - 3.8).abs() < 1e-6);
    }

    #[test]
    fn an_untimed_voice_is_trimmed_to_where_it_sounds() {
        let mut files = files();
        files.get_mut("c01s01").unwrap().sounding = Some((0.3, 4.6));
        let placements = plan(&board(), &files, &Options::default(), None).unwrap();
        let voice = of(&placements, Kind::Voice)[0];
        assert!((voice.from - 0.25).abs() < 1e-9);
        assert!((voice.length - (4.75 - 0.25)).abs() < 1e-9);
    }

    #[test]
    fn each_chapter_starts_with_its_first_shot() {
        let mut board = board();
        let mut second = board.chapters[0].clone();
        second.title = "Two".to_owned();
        second.scenes.truncate(1);
        second.scenes[0].id = "c02s01".to_owned();
        board.chapters.push(second);
        let mut files = files();
        files.insert(
            "c02s01".to_owned(),
            SceneFiles {
                shots: vec![("/d.png".into(), None), ("/e.png".into(), None)],
                ..SceneFiles::default()
            },
        );
        let placements = plan(&board, &files, &Options::default(), None).unwrap();
        let starts = chapter_starts(&board, &placements);
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0], ("One".to_owned(), 0.0));
        assert_eq!(starts[1].0, "Two");
        assert!((starts[1].1 - 8.35).abs() < 1e-9, "{}", starts[1].1);
    }

    #[test]
    fn the_thumbnail_is_a_timeline_of_its_own() {
        let mut editor = Bare(Editor::default());
        let before = editor.0.project().timelines.len();
        let picture = probed("/t.png", MediaKind::Image, None);
        let timeline = thumbnail(&mut editor, "Desert · thumbnail", &picture, "Lost city").unwrap();
        let project = editor.0.project();
        assert_eq!(project.timelines.len(), before + 1);
        let active = project.active();
        assert_eq!(active.id, timeline);
        assert_eq!((active.video.width, active.video.height), (1280, 720));
        assert_eq!(
            active
                .clips
                .iter()
                .filter(|c| c.kind == ClipKind::Image)
                .count(),
            1
        );
        assert_eq!(
            active
                .clips
                .iter()
                .filter(|c| c.kind == ClipKind::Text)
                .count(),
            1
        );
    }
}
