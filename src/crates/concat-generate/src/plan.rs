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

use crate::storyboard::{Graphic, Scene, ShotKind, Storyboard};

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
    /// The longest a still is held before the cut punches in on it - the
    /// same picture again, framed tighter or wider - so the eye gets
    /// something new every few seconds, as short-form video does; 0 holds
    /// a still for its whole slot.
    pub hold: f64,
    /// The cut's own sounds - into a scene, under a sticker, under a
    /// figure - and the music lifted where nobody speaks; None for a cut
    /// with neither.
    pub sounds: Option<crate::sfx::Sounds>,
    /// The channel's opening, a clip and its length, played before the
    /// first scene.
    pub intro: Option<(String, f64)>,
    /// The channel's closing, played after the last.
    pub outro: Option<(String, f64)>,
    /// The channel's logo, a picture kept in a corner over the scenes.
    pub logo: Option<String>,
    /// The channel's colour, `#rrggbb`: a caption's emphasised word and a
    /// sticker's plate take it.
    pub accent: Option<String>,
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
            hold: 0.0,
            sounds: None,
            intro: None,
            outro: None,
            logo: None,
            accent: None,
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
    /// An animated card's words and plates, over its backdrop.
    Graphic,
    /// A punchy phrase stuck over the pictures as a tilted label.
    Sticker,
    /// One of the cut's own sounds: a whoosh, a pop, a chime.
    Sound,
    /// The channel's opening or closing, before the first scene or after
    /// the last.
    Bumper,
    /// The channel's logo, in a corner over the scenes.
    Logo,
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
    /// What an animated card shows.
    pub graphic: Option<Graphic>,
    /// A caption over a drawn shot - an explainer or an animated card -
    /// kept low, under the drawing, rather than over what it draws.
    pub low: bool,
    /// For a caption, the word of it shown in colour; empty for none.
    pub emphasis: String,
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

/// How a scene's time falls: where its narration starts in its file, how
/// long it is heard, how long the scene lasts, and each shot's place in
/// it, as seconds from the scene's start and a length. The cut lays the
/// scene by it, and the job draws an explainer to its slot by the same
/// sums, so the clip fits its place exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    /// Where in the voice's file the scene's narration starts.
    pub from: f64,
    /// How long the narration is heard, when there is one.
    pub spoken: Option<f64>,
    /// How long the scene lasts.
    pub length: f64,
    /// Each shot's start in the scene and its length.
    pub slots: Vec<(f64, f64)>,
}

/// The scene's [`Timing`], from what was made for it.
pub fn timing(scene: &Scene, made: &SceneFiles, options: &Options) -> Timing {
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
    let count = made.shots.len().max(1);
    // The scene's time shared over its shots in proportion to their own
    // lengths, so a ten-second clip is not squeezed to match a four.
    let even = length / count as f64;
    // A length the probe could not read - none, nought, not a number -
    // counts as an even share, so one odd file cannot leave every later
    // shot without a place (#16's phone run: a clip probed at no length
    // made the shares 0/0). A drawn shot - a card, an explainer - is made
    // to its share, so it counts as one too, whatever its file's length.
    let weights: Vec<f64> =
        made.shots
            .iter()
            .enumerate()
            .map(|(i, (_, source))| {
                let drawn = scene.shots.get(i).is_some_and(|shot| {
                    matches!(shot.kind, ShotKind::Motion | ShotKind::Explainer)
                });
                if drawn {
                    even
                } else {
                    source.filter(|s| s.is_finite() && *s > 0.1).unwrap_or(even)
                }
            })
            .collect();
    let total: f64 = weights.iter().sum::<f64>().max(1e-9);
    let mut slots = Vec::with_capacity(made.shots.len());
    let mut at = 0.0;
    for (i, weight) in weights.iter().enumerate() {
        let slot = if i + 1 == weights.len() {
            length - at
        } else {
            length * weight / total
        };
        slots.push((at, slot));
        at += slot;
    }
    Timing {
        from,
        spoken,
        length,
        slots,
    }
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
    // One of the cut's sounds at `at`, never past `end`.
    let sound = |sound: &crate::sfx::Sound, at: f64, end: f64| Placement {
        kind: Kind::Sound,
        start: at.max(0.0),
        length: sound.seconds.min(end - at.max(0.0)),
        file: Some(sound.path.clone()),
        source: Some(sound.seconds),
        from: 0.0,
        text: None,
        graphic: None,
        low: false,
        emphasis: String::new(),
    };
    let bumper = |clip: &(String, f64), at: f64| Placement {
        kind: Kind::Bumper,
        start: at,
        length: clip.1,
        file: Some(clip.0.clone()),
        source: Some(clip.1),
        from: 0.0,
        text: None,
        graphic: None,
        low: false,
        emphasis: String::new(),
    };
    // The channel's opening first: every scene starts after it. A video
    // that opens on a hook shows the hook before anything - a viewer
    // decides in its first seconds - and the channel's opening after it.
    let hooked = board.hooked();
    let intro = options.intro.as_ref().filter(|clip| clip.1 > 0.1);
    if let Some(intro) = intro.filter(|_| !hooked) {
        out.push(bumper(intro, 0.0));
        t = intro.1;
    }
    let mut opened = t;
    for (n, scene) in board.scenes().enumerate() {
        let made = files.get(&scene.id).cloned().unwrap_or_default();
        if made.shots.is_empty() {
            return Err(format!("scene {} has no shots", scene.id));
        }
        let timing = timing(scene, &made, options);
        let (from, spoken, length) = (timing.from, timing.spoken, timing.length);
        let file_length = made
            .voice
            .as_ref()
            .map(|(_, seconds)| *seconds)
            .filter(|s| *s > 0.0);
        // Into every scene but the first, a whoosh that starts just before
        // the cut and carries over it.
        if let Some(sounds) = options.sounds.as_ref().filter(|_| t > 0.0) {
            out.push(sound(&sounds.whoosh, t - WHOOSH_LEAD, t + length));
        }
        let mut at = t;
        // Where the scene's drawn shots are: a caption over one goes low.
        let mut drawings: Vec<(f64, f64)> = Vec::new();
        for (i, (path, source)) in made.shots.iter().enumerate() {
            let slot = timing.slots[i].1;
            if scene.shots.get(i).is_some_and(|shot| {
                matches!(shot.made_as(), ShotKind::Motion | ShotKind::Explainer)
            }) {
                drawings.push((at, at + slot));
            }
            // A picture held longer than the cut's hold is shown in pieces,
            // each a punch in on the last; a drawn shot is whole.
            let still = source.is_none()
                && scene
                    .shots
                    .get(i)
                    .is_some_and(|shot| shot.made_as() == ShotKind::Image);
            let pieces = if still && options.hold > 0.0 && slot > options.hold * 1.4 {
                (slot / options.hold).ceil() as usize
            } else {
                1
            };
            for piece in 0..pieces {
                let length = slot / pieces as f64;
                out.push(Placement {
                    kind: Kind::Shot,
                    start: at + length * piece as f64,
                    length,
                    file: Some(path.clone()),
                    source: *source,
                    from: 0.0,
                    text: None,
                    graphic: None,
                    low: false,
                    emphasis: String::new(),
                });
            }
            // An animated card's words over its backdrop, for the shot's
            // whole slot.
            if let Some(graphic) = scene
                .shots
                .get(i)
                .filter(|shot| shot.kind == ShotKind::Motion)
                .and_then(|shot| shot.graphic.clone())
            {
                out.push(Placement {
                    kind: Kind::Graphic,
                    start: at,
                    length: slot,
                    file: None,
                    source: None,
                    from: 0.0,
                    text: None,
                    graphic: Some(graphic),
                    low: false,
                    emphasis: String::new(),
                });
                // A chime as the card's figure lands, a beat in.
                if let Some(sounds) = &options.sounds
                    && slot > CHIME_LEAD + 0.2
                {
                    out.push(sound(&sounds.chime, at + CHIME_LEAD, at + slot));
                }
            }
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
                graphic: None,
                low: false,
                emphasis: String::new(),
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
                let (from, to) = (t + at, t + at + span);
                let emphasis = emphasis_of(&card, &scene.keywords);
                let drawn: f64 = drawings
                    .iter()
                    .map(|&(a, b)| (to.min(b) - from.max(a)).max(0.0))
                    .sum();
                out.push(Placement {
                    kind: Kind::Caption,
                    start: from,
                    length: span,
                    file: None,
                    source: None,
                    from: 0.0,
                    text: Some(card),
                    graphic: None,
                    low: span > 0.0 && drawn * 2.0 >= span,
                    emphasis,
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
                graphic: None,
                low: false,
                emphasis: String::new(),
            });
        }
        // A sticker a moment into the scene, for as long as it takes to
        // read twice.
        if let Some(text) = scene
            .sticker
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            let at = STICKER_LEAD.min(length / 4.0);
            out.push(Placement {
                kind: Kind::Sticker,
                start: t + at,
                length: STICKER_SECONDS.min(length - at),
                file: None,
                source: None,
                from: 0.0,
                text: Some(text.to_owned()),
                graphic: None,
                low: false,
                emphasis: String::new(),
            });
            if let Some(sounds) = &options.sounds {
                out.push(sound(&sounds.pop, t + at, t + length));
            }
        }
        // A data card a moment into the scene, held while the point is
        // made: four seconds and a half at most, never past the scene.
        // Not over a scene that has animated cards: they say it already.
        let carded = scene.shots.iter().any(|shot| shot.kind == ShotKind::Motion);
        if let Some(card) = scene
            .card
            .as_ref()
            .filter(|card| !carded && !card.figure.trim().is_empty())
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
                graphic: None,
                low: false,
                emphasis: String::new(),
            });
            if let Some(sounds) = &options.sounds {
                out.push(sound(&sounds.chime, t + at, t + length));
            }
        }
        t += length;
        if n == 0 && hooked {
            if let Some(intro) = intro {
                out.push(bumper(intro, t));
                t += intro.1;
            }
            // The logo is the channel's, over its video: not over the hook.
            opened = t;
        }
    }

    // A sound with no room left to ring is left out.
    out.retain(|p| p.kind != Kind::Sound || p.length > 0.05);
    // The logo over the scenes, not over the channel's own cards.
    if let Some(logo) = options.logo.as_ref().filter(|_| t > opened) {
        out.push(Placement {
            kind: Kind::Logo,
            start: opened,
            length: t - opened,
            file: Some(logo.clone()),
            source: None,
            from: 0.0,
            text: None,
            graphic: None,
            low: false,
            emphasis: String::new(),
        });
    }
    if let Some(outro) = options.outro.as_ref().filter(|clip| clip.1 > 0.1) {
        out.push(bumper(outro, t));
        t += outro.1;
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
                graphic: None,
                low: false,
                emphasis: String::new(),
            });
            at += source;
        }
    }
    Ok(out)
}

/// Where an animated caption over a drawn shot sits, down from the middle
/// as a fraction of the frame: under the explainer's captions band.
const LOW_CAPTION: f64 = 0.37;

/// How large the channel's logo is drawn, of the frame it is fitted to,
/// and where: right and up from the middle, as fractions of the frame.
const LOGO_SCALE: f64 = 0.13;
const LOGO_OFFSET: (f64, f64) = (0.4, -0.39);

/// How long before a scene's first frame its whoosh starts, and how long
/// into an animated card its chime rings.
const WHOOSH_LEAD: f64 = 0.15;
const CHIME_LEAD: f64 = 0.35;

/// The shortest stretch with nobody speaking the music is lifted in, how
/// long it takes to rise and fall, and how far it rises: so many times the
/// level it keeps under the voice, and never over this.
const MUSIC_GAP: f64 = 1.2;
const MUSIC_RAMP: f64 = 0.4;
const MUSIC_LIFT: f64 = 2.6;
const MUSIC_MOST: f64 = 0.45;

/// The keys that lift a music clip `from..to` where nobody speaks: the
/// voice's stretches are `voiced`, and `low` is the music's level under
/// them. Each key is where in the clip, `0..=1`, and the level there;
/// none when the voice leaves no gap worth the lift.
fn music_keys(from: f64, to: f64, voiced: &[(f64, f64)], low: f64) -> Vec<(f64, f64)> {
    let length = to - from;
    if length <= 0.0 {
        return Vec::new();
    }
    let high = (low * MUSIC_LIFT).min(MUSIC_MOST).max(low);
    let mut spoken: Vec<(f64, f64)> = voiced
        .iter()
        .map(|&(a, b)| (a.max(from), b.min(to)))
        .filter(|(a, b)| b > a)
        .collect();
    spoken.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // The gaps between them, the clip's two ends included.
    let mut gaps = Vec::new();
    let mut at = from;
    for (a, b) in spoken {
        if a - at >= MUSIC_GAP {
            gaps.push((at, a));
        }
        at = at.max(b);
    }
    if to - at >= MUSIC_GAP {
        gaps.push((at, to));
    }
    let mut keys: Vec<(f64, f64)> = Vec::new();
    let where_in = |t: f64| ((t - from) / length).clamp(0.0, 1.0);
    for (a, b) in gaps {
        // Up already where the gap opens the clip, and still up where it
        // closes it: the cut's own fades see to its two ends.
        if a > from {
            keys.push((where_in(a), low));
            keys.push((where_in(a + MUSIC_RAMP), high));
        } else {
            keys.push((0.0, high));
        }
        if b < to {
            keys.push((where_in(b - MUSIC_RAMP), high));
            keys.push((where_in(b), low));
        } else {
            keys.push((1.0, high));
        }
    }
    if !keys.is_empty() {
        if keys.first().is_some_and(|key| key.0 > 0.0) {
            keys.insert(0, (0.0, low));
        }
        if keys.last().is_some_and(|key| key.0 < 1.0) {
            keys.push((1.0, low));
        }
    }
    keys
}

/// How long into its scene a sticker comes up, and how long it stays.
const STICKER_LEAD: f64 = 0.6;
const STICKER_SECONDS: f64 = 2.4;

/// The colour a caption's emphasised word is shown in.
const EMPHASIS: &str = "#FFD400";

/// The first of `keywords` a caption `card` holds, as the card has it;
/// empty when it holds none.
fn emphasis_of(card: &str, keywords: &[String]) -> String {
    let plain = |word: &str| {
        crate::storyboard::without_marks(word)
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    let wanted: Vec<String> = keywords
        .iter()
        .map(|word| plain(word))
        .filter(|word| !word.is_empty())
        .collect();
    card.split_whitespace()
        .find(|word| wanted.contains(&plain(word)))
        .map(plain)
        .unwrap_or_default()
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
    // A still punched in on is one shot in several pieces: the pieces
    // after the first go on from it with the same file, and are no shot
    // of their own.
    let all: Vec<&Placement> = placements.iter().filter(|p| p.kind == Kind::Shot).collect();
    let mut shots = all.iter().enumerate().filter_map(|(n, p)| {
        let goes_on = n > 0
            && all[n - 1].file == p.file
            && all[n - 1].source.is_none()
            && (all[n - 1].start + all[n - 1].length - p.start).abs() < 1e-6;
        (!goes_on).then_some(*p)
    });
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
        graphic: None,
        low: false,
        emphasis: String::new(),
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
            Kind::Shot | Kind::Bumper => lanes[0].clone(),
            Kind::Voice => lanes[1].clone(),
            Kind::Music => lanes[2].clone(),
            Kind::Title
            | Kind::Caption
            | Kind::Card
            | Kind::Graphic
            | Kind::Sticker
            | Kind::Sound
            | Kind::Logo => lanes[3].clone(),
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
        Kind::Bumper,
        Kind::Voice,
        Kind::Music,
        Kind::Sound,
        Kind::Title,
        Kind::Graphic,
        Kind::Card,
        Kind::Sticker,
        Kind::Caption,
        Kind::Logo,
    ];
    let shots: Vec<&Placement> = placements.iter().filter(|p| p.kind == Kind::Shot).collect();
    let last_music = placements.iter().rposition(|p| p.kind == Kind::Music);
    // The channel's own cards take the cut's first fade and its last.
    let opens = placements
        .iter()
        .any(|p| p.kind == Kind::Bumper && p.start < 1e-6);
    // The closing is the card after the last shot; one between two
    // shots is the opening of a video that starts on its hook.
    let ended = shots.last().map_or(0.0, |shot| shot.start + shot.length);
    let closes = placements
        .iter()
        .any(|p| p.kind == Kind::Bumper && p.start > ended - 1e-6);
    // A hook is there at once: it does not come up from black.
    let hooked = board.hooked();
    let voiced: Vec<(f64, f64)> = placements
        .iter()
        .filter(|p| p.kind == Kind::Voice)
        .map(|p| (p.start, p.start + p.length))
        .collect();
    // The sounds' lanes, minted under everything as they are needed: two
    // sounds may ring at once, and a lane holds one clip at a time.
    let mut sound_lanes: Vec<(String, f64)> = Vec::new();
    let mut captions: Option<String> = None;
    let mut figures: Option<String> = None;
    let mut graphic_lanes = crate::motion::Lanes::default();
    let mut cards = 0usize;
    let mut left_out = Vec::new();
    for kind in order {
        for (index, p) in placements
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == kind)
        {
            match kind {
                Kind::Shot
                | Kind::Voice
                | Kind::Music
                | Kind::Sound
                | Kind::Bumper
                | Kind::Logo => {
                    let file = p.file.as_deref().unwrap_or_default();
                    let track = if kind == Kind::Sound {
                        // The first of the sounds' lanes free by now.
                        let free = sound_lanes
                            .iter_mut()
                            .find(|(_, busy_until)| *busy_until <= p.start + 1e-6);
                        match free {
                            Some((id, busy_until)) => {
                                *busy_until = p.start + p.length;
                                id.clone()
                            }
                            None => match editor.apply(Command::AddTrack { bottom: true }) {
                                Some(id) => {
                                    sound_lanes.push((id.clone(), p.start + p.length));
                                    id
                                }
                                None => continue,
                            },
                        }
                    } else if kind == Kind::Logo {
                        // On a lane of its own over everything.
                        match editor.apply(Command::AddTrack { bottom: false }) {
                            Some(id) => id,
                            None => continue,
                        }
                    } else {
                        lane(kind)
                    };
                    let placed = media_id(editor, file).and_then(|media| {
                        if !p.start.is_finite() || !p.length.is_finite() {
                            return Err(format!("{file} has no time to go at"));
                        }
                        editor
                            .apply(Command::AddClip {
                                media_id: media.clone(),
                                track_id: track.clone(),
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
                    fit(editor, &clip, &media, &track, p, fill);
                    let mut patch = ClipPatch::default();
                    if kind == Kind::Shot {
                        let n = shots
                            .iter()
                            .position(|shot| std::ptr::eq(*shot, p))
                            .unwrap_or(0);
                        // The same picture going on from the shot before
                        // it: a punch in, cut straight.
                        let punch = n > 0
                            && shots[n - 1].file == p.file
                            && (shots[n - 1].start + shots[n - 1].length - p.start).abs() < 1e-6;
                        if n > 0
                            && !punch
                            && let Some(id) = &options.transition
                        {
                            patch.transition_in = Some(Some(Transition {
                                id: id.clone(),
                                duration: options.transition_seconds,
                            }));
                        }
                        if p.source.is_none() && options.ken_burns {
                            let (from, to) = match (punch, n % 2 == 0) {
                                // Tighter than the piece before ended, then
                                // wide again, by turns: each jump reads as
                                // a cut.
                                (true, false) => (1.3, 1.2),
                                (true, true) => (1.0, 1.08),
                                (false, true) => (1.0, 1.12),
                                (false, false) => (1.12, 1.0),
                            };
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
                            n == 0 && !opens && !hooked,
                            n + 1 == shots.len() && !closes,
                        );
                    } else if kind == Kind::Music {
                        patch.volume = Some(options.music_volume);
                        if Some(index) == last_music {
                            patch.fade_out = Some(p.length.min(3.0));
                        }
                        // Lifted where nobody speaks, back down under the
                        // voice.
                        if options.sounds.is_some() {
                            for (at, value) in music_keys(
                                p.start,
                                p.start + p.length,
                                &voiced,
                                options.music_volume,
                            ) {
                                editor.apply(Command::SetClipKey {
                                    clip_id: clip.clone(),
                                    property: KeyProperty::Volume,
                                    at,
                                    value,
                                    ease: KeyEase::IN_OUT,
                                });
                            }
                        }
                    } else if kind == Kind::Sound
                        && let Some(sounds) = &options.sounds
                    {
                        patch.volume = Some(sounds.gain(file));
                    }
                    if kind == Kind::Bumper {
                        // The opening comes up from black, the closing
                        // goes down to it.
                        let first = p.start < 1e-6;
                        let last = p.start > ended - 1e-6;
                        fade(editor, &clip, p.length, options.fade_seconds, first, last);
                    } else if kind == Kind::Logo {
                        // Small, in the top corner at the line's end side.
                        editor.apply(Command::SetClipTransform {
                            clip_id: clip.clone(),
                            scale: Some(LOGO_SCALE),
                            offset_x: Some(LOGO_OFFSET.0),
                            offset_y: Some(LOGO_OFFSET.1),
                            rotation: None,
                            stretch_x: None,
                            stretch_y: None,
                        });
                        patch.opacity = Some(0.9);
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
                // On lanes of their own over the pictures, under the
                // captions, which are laid after.
                Kind::Graphic => {
                    if let Some(graphic) = &p.graphic {
                        crate::motion::lay(
                            editor,
                            &mut graphic_lanes,
                            graphic,
                            p.start,
                            p.length,
                            (width, height),
                        );
                    }
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
                // Over the pictures and under the captions, on the cards'
                // lane: tilted, on a red plate, popping in.
                Kind::Sticker => {
                    if figures.is_none() {
                        figures = editor.apply(Command::AddTrack { bottom: false });
                    }
                    let sticker = editor.apply(Command::AddTextClip {
                        track_id: figures.clone(),
                        above: true,
                        start: p.start,
                        style: Some(sticker_style(
                            p.text.clone().unwrap_or_default(),
                            options.accent.as_deref(),
                        )),
                        duration: Some(p.length),
                        offset_y: Some(-0.2),
                    });
                    if let Some(sticker) = sticker {
                        key(
                            editor,
                            &sticker,
                            KeyProperty::Rotation,
                            &[(0.0, -6.0), (1.0, -6.0)],
                        );
                        pop(editor, &sticker, p.length);
                    }
                }
                Kind::Caption => {
                    if captions.is_none() {
                        captions = editor.apply(Command::AddTrack { bottom: false });
                    }
                    let text = p.text.clone().unwrap_or_default();
                    let (mut style, offset_y) = match options.caption_look {
                        CaptionLook::Classic => (caption_style(text), 0.38),
                        // Over a drawing, under it: the drawing keeps the
                        // frame above its captions band clear for them.
                        CaptionLook::Animated if p.low => {
                            (animated_style(text, cards % 2 == 1), LOW_CAPTION)
                        }
                        CaptionLook::Animated => (animated_style(text, cards % 2 == 1), 0.24),
                    };
                    // The word to land on in colour; the rest of the card
                    // then in white, so the colour means something.
                    if !p.emphasis.is_empty() {
                        style.color = "#FFFFFF".to_owned();
                        style.emphasis = p.emphasis.clone();
                        style.emphasis_color = accent_of(options).to_owned();
                    }
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

/// The colour a caption's emphasised word takes: the channel's, or the
/// cut's own yellow.
fn accent_of(options: &Options) -> &str {
    options
        .accent
        .as_deref()
        .filter(|colour| rgb_of(colour).is_some())
        .unwrap_or(EMPHASIS)
}

/// `#rrggbb` as its three channels.
fn rgb_of(colour: &str) -> Option<[u8; 3]> {
    let hex = colour.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// A sticker: a few heavy words on a plate - the channel's colour, or
/// red - tilted by the cut; dark words on a light plate.
fn sticker_style(content: String, accent: Option<&str>) -> TextStyle {
    let plate = accent
        .filter(|colour| rgb_of(colour).is_some())
        .unwrap_or("#E63946");
    let light = rgb_of(plate).is_some_and(|[r, g, b]| {
        0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b) > 160.0
    });
    TextStyle {
        content,
        font_size: 0.058,
        font_weight: 900.0,
        color: if light { "#111111" } else { "#FFFFFF" }.to_owned(),
        shadow: true,
        background: plate.trim().to_owned(),
        background_radius: 0.012,
        background_padding_x: 0.026,
        background_padding_y: 0.014,
        max_width: 0.7,
        ..TextStyle::default()
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
        assert_eq!(
            cards[0].text.as_deref(),
            Some("$1.2 trillion\nspent in 2025")
        );
        assert!((cards[0].start - CARD_LEAD).abs() < 1e-9);
        assert!(cards[0].start + cards[0].length <= 6.0 + 1e-9);
        assert!(
            of(
                &plan(&self::board(), &files, &Options::default(), None).unwrap(),
                Kind::Card
            )
            .is_empty()
        );
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

    /// A video that opens on a hook shows the hook first: the channel's
    /// opening comes after it, the logo starts after that, and the rest of
    /// the video is as long as it was.
    #[test]
    fn a_hook_goes_before_the_channels_opening() {
        let mut board = board();
        board.chapters[0].scenes[0].id = crate::storyboard::HOOK.to_owned();
        assert!(board.hooked());
        let mut files = files();
        let first = files.remove("c01s01").unwrap();
        files.insert(crate::storyboard::HOOK.to_owned(), first);
        let plain = Options {
            titles: false,
            ..Options::default()
        };
        let branded = Options {
            intro: Some(("/intro.mp4".into(), 2.4)),
            logo: Some("/logo.png".into()),
            ..plain.clone()
        };
        let before = plan(&board, &files, &plain, None).unwrap();
        let placements = plan(&board, &files, &branded, None).unwrap();
        let shots = of(&placements, Kind::Shot);
        assert_eq!(shots[0].start, 0.0);
        // The hook's own length: where its last shot ends without the
        // opening, which is where the opening now starts.
        let hook = of(&before, Kind::Shot)[2].start;
        let bumpers = of(&placements, Kind::Bumper);
        assert_eq!(bumpers.len(), 1);
        assert!((bumpers[0].start - hook).abs() < 1e-9);
        assert!((shots[2].start - (hook + 2.4)).abs() < 1e-9);
        let logo = of(&placements, Kind::Logo)[0];
        assert!((logo.start - (hook + 2.4)).abs() < 1e-9);
        assert!((total_length(&placements) - (total_length(&before) + 2.4)).abs() < 1e-9);
        // Without an opening the hook is the first scene and nothing more.
        assert_eq!(of(&before, Kind::Bumper).len(), 0);
    }

    /// A channel's opening goes before the first scene and its closing
    /// after the last, everything between moved on by the opening; its
    /// logo sits over the scenes alone, small and in a corner; its colour
    /// is the captions' and the stickers'.
    #[test]
    fn a_channel_opens_closes_and_marks_its_video() {
        let mut board = board();
        board.chapters[0].scenes[0].keywords = vec!["Second".into()];
        board.chapters[0].scenes[0].sticker = Some("Look!".into());
        let plain = Options {
            titles: false,
            ..Options::default()
        };
        let branded = Options {
            intro: Some(("/intro.mp4".into(), 2.4)),
            outro: Some(("/outro.mp4".into(), 3.6)),
            logo: Some("/logo.png".into()),
            accent: Some("#00C2FF".into()),
            ..plain.clone()
        };
        let before = plan(&board, &files(), &plain, None).unwrap();
        let placements = plan(&board, &files(), &branded, None).unwrap();
        let bumpers = of(&placements, Kind::Bumper);
        assert_eq!(bumpers.len(), 2);
        assert_eq!(bumpers[0].start, 0.0);
        let body = total_length(&before);
        assert!((bumpers[1].start - (2.4 + body)).abs() < 1e-9);
        assert!((total_length(&placements) - (body + 6.0)).abs() < 1e-9);
        assert!((of(&placements, Kind::Shot)[0].start - 2.4).abs() < 1e-9);
        let logo = of(&placements, Kind::Logo)[0];
        assert!((logo.start - 2.4).abs() < 1e-9 && (logo.length - body).abs() < 1e-9);
        // The chapters are timed as the video now runs.
        assert!((chapter_starts(&board, &placements)[0].1 - 2.4).abs() < 1e-9);

        let mut media = HashMap::from([
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
                "/logo.png".to_owned(),
                probed("/logo.png", MediaKind::Image, None),
            ),
        ]);
        for (path, length) in [("/intro.mp4", 2.4), ("/outro.mp4", 3.6)] {
            media.insert(
                path.to_owned(),
                probed(path, MediaKind::Video, Some(length)),
            );
        }
        let mut editor = Bare(Editor::default());
        cut(&mut editor, &board, &placements, &media, &branded).unwrap();
        let logo_media = editor.media_of("/logo.png").expect("imported");
        let active = editor.0.project().active();
        let mark = active
            .clips
            .iter()
            .find(|c| c.media_id == logo_media)
            .expect("laid");
        assert!((mark.scale - LOGO_SCALE).abs() < 1e-9);
        assert!(mark.offset_x > 0.3 && mark.offset_y < -0.3);
        // On the stack's top lane.
        assert_eq!(active.tracks.last().map(|t| &t.id), Some(&mark.track_id));
        let texts: Vec<_> = active
            .clips
            .iter()
            .filter_map(|c| c.text.as_ref())
            .collect();
        assert!(texts.iter().any(|t| t.emphasis_color == "#00C2FF"));
        let sticker = texts.iter().find(|t| t.content == "Look!").unwrap();
        assert_eq!(sticker.background, "#00C2FF");
        // A light plate takes dark words; a colour that is none is passed over.
        assert_eq!(sticker_style("x".into(), Some("#FFFF00")).color, "#111111");
        assert_eq!(
            sticker_style("x".into(), Some("blue")).background,
            "#E63946"
        );
    }

    /// The music is lifted where nobody speaks for long enough and brought
    /// back under the voice; a voice with no gaps leaves it alone.
    #[test]
    fn the_music_rises_where_nobody_speaks() {
        // Voice from 2 to 10 of a 14 s clip: up at the start, down by 2,
        // up again after 10.
        let keys = music_keys(0.0, 14.0, &[(2.0, 10.0)], 0.12);
        let high = 0.12 * MUSIC_LIFT;
        assert_eq!(keys.first(), Some(&(0.0, high)));
        assert_eq!(keys.last(), Some(&(1.0, high)));
        assert!(keys.contains(&(2.0 / 14.0, 0.12)));
        assert!(keys.contains(&(10.0 / 14.0, 0.12)));
        for pair in keys.windows(2) {
            assert!(pair[0].0 <= pair[1].0, "{keys:?}");
        }
        // Breaths between paragraphs are no gap.
        assert!(music_keys(0.0, 10.0, &[(0.0, 4.8), (5.2, 10.0)], 0.12).is_empty());
        // A gap in the middle: down, up, up, down, and low at both ends.
        let middle = music_keys(0.0, 20.0, &[(0.0, 5.0), (9.0, 20.0)], 0.12);
        assert_eq!(middle.first(), Some(&(0.0, 0.12)));
        assert_eq!(middle.last(), Some(&(1.0, 0.12)));
        assert_eq!(middle.iter().filter(|key| key.1 > 0.12).count(), 2);
        // Never louder than the most.
        assert!(
            music_keys(0.0, 9.0, &[], 0.3)
                .iter()
                .all(|key| key.1 <= MUSIC_MOST)
        );
    }

    /// With sounds, a whoosh leads into every scene but the first, a pop
    /// goes under a sticker and a chime under a card; they are laid under
    /// everything, each at its own level, two at once on two lanes.
    #[test]
    fn the_cut_is_laid_with_its_own_sounds() {
        let dir =
            std::env::temp_dir().join(format!("concat-generate-plan-sfx-{}", std::process::id()));
        let sounds = crate::sfx::sounds(&dir).unwrap();
        let mut board = board();
        let scene = &mut board.chapters[0].scenes[0];
        scene.sticker = Some("Look!".into());
        scene.card = Some(crate::storyboard::Card {
            figure: "1969".into(),
            label: "the landing".into(),
        });
        let options = Options {
            titles: false,
            captions: false,
            sounds: Some(sounds.clone()),
            ..Options::default()
        };
        let silent = plan(
            &board,
            &files(),
            &Options {
                sounds: None,
                ..options.clone()
            },
            None,
        )
        .unwrap();
        assert!(of(&silent, Kind::Sound).is_empty());
        let placements = plan(&board, &files(), &options, None).unwrap();
        let heard = of(&placements, Kind::Sound);
        let count = |sound: &crate::sfx::Sound| {
            heard
                .iter()
                .filter(|p| p.file.as_deref() == Some(sound.path.as_str()))
                .count()
        };
        assert_eq!(
            (
                count(&sounds.whoosh),
                count(&sounds.pop),
                count(&sounds.chime)
            ),
            (1, 1, 1)
        );
        let second = of(&placements, Kind::Shot)[2].start;
        let whoosh = heard
            .iter()
            .find(|p| p.file.as_deref() == Some(sounds.whoosh.path.as_str()))
            .unwrap();
        assert!((whoosh.start - (second - WHOOSH_LEAD)).abs() < 1e-9);
        assert_eq!(total_length(&placements), total_length(&silent));

        let mut media = HashMap::from([
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
        for sound in sounds.all() {
            media.insert(
                sound.path.clone(),
                probed(&sound.path, MediaKind::Audio, Some(sound.seconds)),
            );
        }
        let mut editor = Bare(Editor::default());
        cut(&mut editor, &board, &placements, &media, &options).unwrap();
        let pop = editor.media_of(&sounds.pop.path).expect("imported");
        let active = editor.0.project().active();
        let clip = active
            .clips
            .iter()
            .find(|clip| clip.media_id == pop)
            .expect("laid");
        assert!((clip.volume - sounds.gain(&sounds.pop.path)).abs() < 1e-9);
        // Under the pictures: the sounds' lanes are the stack's first.
        let lane = active
            .tracks
            .iter()
            .position(|t| t.id == clip.track_id)
            .unwrap();
        let pictures = editor.media_of("/a.mp4").unwrap();
        let picture = active
            .clips
            .iter()
            .find(|c| c.media_id == pictures)
            .unwrap();
        assert!(
            lane < active
                .tracks
                .iter()
                .position(|t| t.id == picture.track_id)
                .unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With a hold, a still longer than it is shown in pieces cut straight
    /// one into the next, each framed anew; a clip is whole.
    #[test]
    fn a_long_still_is_punched_in_on() {
        let board = board();
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let options = Options {
            titles: false,
            captions: false,
            hold: 1.0,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let shots = of(&placements, Kind::Shot);
        let stills: Vec<_> = shots
            .iter()
            .filter(|p| p.file.as_deref() == Some("/c.png"))
            .collect();
        assert!(stills.len() >= 2, "{stills:?}");
        // The chapters still start where their first shots do.
        assert_eq!(
            chapter_starts(&board, &placements),
            chapter_starts(
                &board,
                &plan(
                    &board,
                    &files,
                    &Options {
                        hold: 0.0,
                        ..options.clone()
                    },
                    None
                )
                .unwrap()
            )
        );
        let whole: f64 = stills.iter().map(|p| p.length).sum();
        assert!(
            (stills[0].start + whole
                - (stills.last().unwrap().start + stills.last().unwrap().length))
                .abs()
                < 1e-9
        );
        assert_eq!(
            shots
                .iter()
                .filter(|p| p.file.as_deref() == Some("/a.mp4"))
                .count(),
            1
        );

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
        cut(&mut editor, &board, &placements, &media, &options).unwrap();
        let still = editor.media_of("/c.png").expect("imported");
        let active = editor.0.project().active();
        let mut pieces: Vec<_> = active
            .clips
            .iter()
            .filter(|clip| clip.media_id == still)
            .collect();
        pieces.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        assert_eq!(pieces.len(), stills.len());
        // The pieces after the first come in on a straight cut.
        for piece in &pieces[1..] {
            assert!(piece.transition_in.is_none(), "{piece:?}");
        }
    }

    /// A caption that holds one of its scene's keywords shows it in colour,
    /// whatever its case or punctuation; a sticker is laid tilted on the
    /// cards' lane, a moment into its scene.
    #[test]
    fn keywords_colour_their_captions_and_a_sticker_is_tilted() {
        assert_eq!(
            emphasis_of("جيب التيليفون!", &["التيليفون".into()]),
            "التيليفون"
        );
        assert_eq!(
            emphasis_of("Show me the Phone.", &["phone".into()]),
            "phone"
        );
        assert_eq!(emphasis_of("nothing here", &["phone".into()]), "");
        assert_eq!(emphasis_of("a phone", &[]), "");
        assert_eq!(emphasis_of("سلّم هاتفك", &["هاتِفَك".into()]), "هاتفك");

        let mut board = board();
        let scene = &mut board.chapters[0].scenes[0];
        scene.keywords = vec!["Second".into()];
        scene.sticker = Some("Fined on the spot!".into());
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let options = Options {
            titles: false,
            caption_look: CaptionLook::Animated,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let captions = of(&placements, Kind::Caption);
        assert!(captions.iter().any(|c| c.emphasis == "second"));
        assert!(captions.iter().any(|c| c.emphasis.is_empty()));
        let stickers = of(&placements, Kind::Sticker);
        assert_eq!(stickers.len(), 1);
        assert!((stickers[0].start - STICKER_LEAD).abs() < 1e-9);

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
        cut(&mut editor, &board, &placements, &media, &options).unwrap();
        let active = editor.0.project().active();
        let sticker = active
            .clips
            .iter()
            .find(|clip| {
                clip.text
                    .as_ref()
                    .is_some_and(|t| t.content == "Fined on the spot!")
            })
            .expect("the sticker is laid");
        assert!(
            sticker
                .keys
                .iter()
                .any(|k| k.property == KeyProperty::Rotation && k.value == -6.0)
        );
        let coloured = active
            .clips
            .iter()
            .filter_map(|clip| clip.text.as_ref())
            .find(|text| text.emphasis == "second")
            .expect("a caption in colour");
        assert_eq!(coloured.emphasis_color, EMPHASIS);
        assert_eq!(coloured.color, "#FFFFFF");
    }

    /// A caption over a drawn shot is kept low, under the drawing; one
    /// over a picture stays where captions go.
    #[test]
    fn captions_over_a_drawing_go_low() {
        let mut board = board();
        let scene = &mut board.chapters[0].scenes[0];
        scene.narration = "One two three four five six seven eight nine ten eleven twelve.".into();
        scene.shots[1].kind = ShotKind::Motion;
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let options = Options {
            titles: false,
            caption_look: CaptionLook::Animated,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let drawn = of(&placements, Kind::Shot)[1];
        let captions = of(&placements, Kind::Caption);
        assert!(captions.len() > 2, "{captions:?}");
        for caption in captions {
            let middle = caption.start + caption.length / 2.0;
            let over = middle > drawn.start && middle < drawn.start + drawn.length;
            assert_eq!(caption.low, over, "{caption:?}");
        }
    }

    /// An animated card is laid over its backdrop on lanes of its own,
    /// which survive the lanes the cut takes away; a scene with one has no
    /// data card over it.
    #[test]
    fn an_animated_card_is_laid_over_its_backdrop() {
        use crate::storyboard::{Graphic, GraphicItem, GraphicKind};
        let mut board = board();
        let scene = &mut board.chapters[0].scenes[0];
        scene.card = Some(crate::storyboard::Card {
            figure: "1969".into(),
            label: "the landing".into(),
        });
        scene.shots[1].kind = ShotKind::Motion;
        scene.shots[1].graphic = Some(Graphic {
            kind: GraphicKind::Number,
            title: "Stones".into(),
            items: vec![GraphicItem {
                label: "blocks".into(),
                value: "2,300,000".into(),
            }],
            accent: "#C08A2E".into(),
        });
        let mut files = files();
        files.get_mut("c01s01").unwrap().voice = None;
        let options = Options {
            titles: false,
            captions: false,
            ..Options::default()
        };
        let placements = plan(&board, &files, &options, None).unwrap();
        let graphics = of(&placements, Kind::Graphic);
        assert_eq!(graphics.len(), 1);
        let backdrop = of(&placements, Kind::Shot)[1];
        assert_eq!(
            (graphics[0].start, graphics[0].length),
            (backdrop.start, backdrop.length)
        );
        assert!(of(&placements, Kind::Card).is_empty());

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
        cut(&mut editor, &board, &placements, &media, &options).unwrap();
        let active = editor.0.project().active();
        let words: Vec<_> = active
            .clips
            .iter()
            .filter_map(|clip| clip.text.as_ref().map(|text| text.content.clone()))
            .collect();
        assert!(words.iter().any(|w| w == "2,300,000"), "{words:?}");
        assert!(words.iter().any(|w| w == "blocks"));
        // Every lane holds something: none the cut emptied had a card on it.
        for track in &active.tracks {
            assert!(active.clips.iter().any(|c| c.track_id == track.id));
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
