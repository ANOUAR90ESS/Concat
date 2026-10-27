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
use concat_project::commands::{ClipPatch, NewMedia, TrimEdge};
use concat_project::model::{KeyEase, KeyProperty, TextStyle, Transition, VideoSettings};

use crate::storyboard::Storyboard;

/// The house style of the cut.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The transition on every cut between shots; None for straight cuts.
    pub transition: Option<String>,
    /// Its length in seconds.
    pub transition_seconds: f64,
    /// Silence after each scene's narration, so a cut never clips a word.
    pub scene_pad: f64,
    /// The shortest a scene may be.
    pub min_scene: f64,
    /// Captions timed to the narration.
    pub captions: bool,
    /// At most this many words a caption.
    pub caption_words: usize,
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
}

impl Default for Options {
    fn default() -> Self {
        Options {
            transition: Some("cross-fade".to_owned()),
            transition_seconds: 0.5,
            scene_pad: 0.6,
            min_scene: 3.0,
            captions: true,
            caption_words: 7,
            titles: true,
            title_seconds: 3.0,
            ken_burns: true,
            music_volume: 0.12,
            fade_seconds: 1.0,
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
    /// The words, for a title or a caption.
    pub text: Option<String>,
}

/// One scene's files, with their lengths: what generation leaves behind.
#[derive(Clone, Debug, Default)]
pub struct SceneFiles {
    /// The narration and its length.
    pub voice: Option<(String, f64)>,
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
        let spoken = made
            .voice
            .as_ref()
            .map(|(_, seconds)| *seconds)
            .filter(|s| *s > 0.0);
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
                source: Some(spoken),
                text: None,
            });
        }
        // The captions run with the voice when there is one, and over the
        // scene when there is not: a cut with no narration still says what
        // the script says, as subtitles.
        if options.captions {
            let cards = caption_cards(&scene.narration, options.caption_words);
            let chars: usize = cards.iter().map(|card| card.chars().count()).sum();
            let over = spoken.unwrap_or(length);
            let mut at = t;
            for card in cards {
                let span = over * card.chars().count() as f64 / chars.max(1) as f64;
                out.push(Placement {
                    kind: Kind::Caption,
                    start: at,
                    length: span,
                    file: None,
                    source: None,
                    text: Some(card),
                });
                at += span;
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
                text: Some(text.trim().to_owned()),
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
                text: None,
            });
            at += source;
        }
    }
    Ok(out)
}

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
            Kind::Title | Kind::Caption => lanes[3].clone(),
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

    // Media first, words last, captions on a lane of their own on top so
    // they sit over everything and hide in one click.
    let order = [
        Kind::Shot,
        Kind::Voice,
        Kind::Music,
        Kind::Title,
        Kind::Caption,
    ];
    let shots: Vec<&Placement> = placements.iter().filter(|p| p.kind == Kind::Shot).collect();
    let last_music = placements.iter().rposition(|p| p.kind == Kind::Music);
    let mut captions: Option<String> = None;
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
                    fit(editor, &clip, &media, &lane(kind), p);
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
                            key(editor, &clip, KeyProperty::Scale, &[(0.0, from), (1.0, to)]);
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
                Kind::Caption => {
                    if captions.is_none() {
                        captions = editor.apply(Command::AddTrack { bottom: false });
                    }
                    editor.apply(Command::AddTextClip {
                        track_id: captions.clone(),
                        above: true,
                        start: p.start,
                        style: Some(caption_style(p.text.clone().unwrap_or_default())),
                        duration: Some(p.length),
                        offset_y: Some(0.38),
                    });
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
fn fit(editor: &mut dyn Editing, clip: &str, media: &str, lane: &str, p: &Placement) {
    // A clip whose length the probe could not read is fitted as a still
    // is: trimmed to its place, never slowed by a speed of nought.
    let source = p.source.filter(|s| s.is_finite() && *s > 0.1);
    let still = source.is_none();
    let mut natural = source.unwrap_or(5.0);
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
            fit(editor, &again, media, lane, &rest);
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
                },
            ),
            (
                "c01s02".to_owned(),
                SceneFiles {
                    voice: None,
                    shots: vec![("/c.png".into(), None)],
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
        assert!((shots[0].length - 5.6 * 0.4).abs() < 1e-9);
        assert!((shots[1].start + shots[1].length - 5.6).abs() < 1e-9);
        // Scene two has no narration: its seconds, floored.
        assert!((shots[2].start - 5.6).abs() < 1e-9);
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
        assert!((total_length(&placements) - 8.6).abs() < 1e-9);
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
        assert!((at - 8.6).abs() < 1e-6);
        assert!(pictures[0].transition_in.is_none());
        assert!(pictures[1].transition_in.is_some());
        // The music is ducked and fades out at the very end.
        let music = on(2);
        assert_eq!(music.len(), 3);
        assert!(music.iter().all(|m| (m.volume - 0.12).abs() < 1e-9));
        assert!((music[2].start + music[2].duration - 8.6).abs() < 1e-6);
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
}
