// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The Long video sheet: an idea in, a cut timeline out.
//!
//! Three pieces of work, two on a worker and one here. Writing the script
//! asks Claude for the storyboard; making asks the chosen providers for the
//! narration and the shots, and probes what they wrote. Both only read and
//! write the job's folder inside the project folder, so a run stopped half
//! way carries on where it was, and a storyboard edited by hand is the one
//! used. The cut is laid on the window's session, as one undo step, on a
//! timeline of its own: Undo takes it all away, and the timeline the person
//! was cutting is not touched.
//!
//! See `concat_generate` for the steps themselves.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use concat_generate::budget::{self, Totals};
use concat_generate::cost::{self, Estimate, Prices};
use concat_generate::job::{Costs, Job, Made, Step};
use concat_generate::plan::{self, Editing, Options};
use concat_generate::providers::{DEFAULT_VIDEO_MODEL, Shots, Voice};
use concat_generate::storyboard::{Scene, ShotKind, Storyboard, shot_stem};
use concat_generate::writer::{self, Brief, Writing};
use concat_project::Command;
use concat_project::model::MediaKind;
use slint::SharedString;

use crate::host::{on_ui, on_ui_in_project, spawn_in_project};
use crate::i18n::{t, tf};
use crate::panes::Msg;
use crate::panes::export::ExportMsg;
use crate::panes::settings::installed;
use crate::studio::Studio;
use crate::ui::{GenerateSheetData, ScriptRow, ScriptSheetData};

/// The lengths offered, in minutes, row for row with the sheet's list.
const MINUTES: [f64; 8] = [1.0, 3.0, 5.0, 10.0, 15.0, 20.0, 30.0, 60.0];
/// The row a first opening starts on: ten minutes.
const DEFAULT_MINUTES: usize = 3;
/// The languages offered, by the English name the writer is told, row for
/// row with the sheet's list.
const LANGUAGES: [&str; 10] = [
    "Arabic",
    "English",
    "French",
    "Spanish",
    "German",
    "Turkish",
    "Portuguese",
    "Indonesian",
    "Hindi",
    "Urdu",
];
/// The frames offered.
const ASPECTS: [&str; 3] = ["16:9", "9:16", "1:1"];
/// The shares of shots made as clips, row for row with the sheet's list.
const CLIP_SHARES: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// The row a first opening starts on: a quarter of the shots clips, the
/// rest stills - clips cost several times what a still does.
const DEFAULT_CLIPS: usize = 1;
/// The Kokoro speaker a read starts with when none has been chosen.
const DEFAULT_SPEAKER: i32 = 3;
/// The input a Replicate model is sent when nothing else is known about it.
const REPLICATE_INPUT: &str = r#"{"prompt": "{prompt}", "aspect_ratio": "{aspect}"}"#;

/// Everything that can happen to the Long video sheet.
#[derive(Debug)]
pub enum GenerateMsg {
    /// The menu's Long video.
    Open,
    Close,
    /// See the sheet's `text-edited` for the fields.
    TextEdited(i32, String),
    /// See the sheet's `choice-changed` for the fields.
    ChoiceChanged(i32, i32),
    /// See the sheet's `toggled` for the fields.
    Toggled(i32, bool),
    /// Write the storyboard, and stop there.
    WriteScript,
    /// Make whatever is missing, the script included, and cut it.
    Create,
    /// Show the job's folder in the file manager.
    OpenFolder,
    /// Open the script editor on the storyboard.
    EditScript,
    /// Close it, writing the storyboard back.
    CloseScript,
    /// A text of the script changed: chapter, scene and shot (-1 where the
    /// row is none) and which text; see the editor's `edited`.
    ScriptEdited(i32, i32, i32, i32, String),
    /// A shot made a clip, or a still.
    ShotKindChanged(i32, i32, i32, bool),
    /// Write one scene again.
    RewriteScene(i32, i32),
    /// The writer is done with a scene.
    Rewritten(usize, usize, Box<Result<Scene, String>>),
    Cancel,
    /// A worker saying where it is: `0..=1`, and in words.
    Progress(f32, String),
    /// The writer is done; and whether making follows.
    Written(Box<Result<Storyboard, String>>, bool),
    /// The providers are done, and the files probed.
    Made(Box<Result<(Storyboard, Made), String>>),
}

/// The Long video sheet's state.
pub struct GeneratePane {
    pub open: bool,
    pub topic: String,
    pub notes: String,
    pub minutes: usize,
    pub captions: bool,
    pub titles: bool,
    /// Row of [`GeneratePane::music_rows`]; 0 is none.
    pub music: usize,
    pub running: bool,
    pub progress: f32,
    pub stage: String,
    pub message: String,
    /// The storyboard already in the job's folder, read when the sheet
    /// opens and whenever the idea changes.
    board: Option<Storyboard>,
    /// The shots whose pictures read as ones already used, by stem:
    /// worked out when the storyboard changes, not on every publish.
    repeats: HashSet<String>,
    /// The script editor is showing.
    pub script_open: bool,
    /// The scene being written again, if one is.
    rewriting: Option<(usize, usize)>,
    /// Why the last rewrite failed.
    script_message: String,
    /// The storyboard has edits the file does not have yet.
    dirty: bool,
    /// What the edits made stale: scenes whose narration changed and shots
    /// whose picture did, by id and stem, removed from the job's folder
    /// when the storyboard is written so the next run makes them again.
    stale_voices: BTreeSet<String>,
    stale_shots: BTreeSet<String>,
    /// Asked between files by the running worker.
    stop: Arc<AtomicBool>,
}

impl Default for GeneratePane {
    fn default() -> Self {
        GeneratePane {
            open: false,
            topic: String::new(),
            notes: String::new(),
            minutes: DEFAULT_MINUTES,
            captions: true,
            titles: true,
            music: 0,
            running: false,
            progress: 0.0,
            stage: String::new(),
            message: String::new(),
            board: None,
            repeats: HashSet::new(),
            script_open: false,
            rewriting: None,
            script_message: String::new(),
            dirty: false,
            stale_voices: BTreeSet::new(),
            stale_shots: BTreeSet::new(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl GeneratePane {
    /// Applies one message. The studio is the rest of the window; while
    /// this runs the studio's copy of the pane is a blank it must not read.
    pub fn update(&mut self, msg: GenerateMsg, studio: &mut Studio) {
        match msg {
            GenerateMsg::Open => {
                self.open = true;
                self.message.clear();
                if let Some(minutes) = studio.prefs.generate.minutes {
                    self.minutes = minutes.min(MINUTES.len() - 1);
                }
                self.look(studio);
            }
            GenerateMsg::Close => {
                self.keep(studio);
                self.open = false;
            }
            GenerateMsg::TextEdited(field, text) => {
                let prefs = &mut studio.prefs.generate;
                match field {
                    0 => {
                        self.topic = text;
                        self.look(studio);
                        return;
                    }
                    1 => {
                        self.notes = text;
                        return;
                    }
                    2 => prefs.anthropic_key = text,
                    3 => prefs.replicate_token = text,
                    4 => prefs.replicate_model = text,
                    5 => prefs.elevenlabs_key = text,
                    6 => prefs.elevenlabs_voice = text,
                    7 => prefs.replicate_image_model = text,
                    8 => prefs.spend_limit = text,
                    9 => prefs.clip_price = text,
                    10 => prefs.image_price = text,
                    _ => return,
                }
                studio.prefs.save(&studio.host.dirs);
            }
            GenerateMsg::ChoiceChanged(field, index) => {
                let index = index.max(0) as usize;
                let prefs = &mut studio.prefs.generate;
                match field {
                    0 => {
                        self.minutes = index.min(MINUTES.len() - 1);
                        prefs.minutes = Some(self.minutes);
                    }
                    1 => prefs.language = index.min(LANGUAGES.len() - 1),
                    2 => prefs.aspect = index.min(ASPECTS.len() - 1),
                    3 => prefs.voice = Some(index.min(2)),
                    4 => prefs.shots = index.min(2),
                    5 => {
                        self.music = index;
                        return;
                    }
                    6 => prefs.clips = Some(index.min(CLIP_SHARES.len() - 1)),
                    _ => return,
                }
                studio.prefs.save(&studio.host.dirs);
            }
            GenerateMsg::Toggled(field, on) => match field {
                0 => self.captions = on,
                1 => self.titles = on,
                2 => {
                    studio.prefs.generate.auto_export = Some(on);
                    studio.prefs.save(&studio.host.dirs);
                }
                _ => {}
            },
            GenerateMsg::WriteScript => self.write(studio, false),
            GenerateMsg::Create => {
                self.keep(studio);
                match self.board.clone() {
                    Some(board) => self.make(studio, board),
                    None => self.write(studio, true),
                }
            }
            GenerateMsg::EditScript => {
                if self.board.is_some() {
                    self.script_open = true;
                    self.script_message.clear();
                }
            }
            GenerateMsg::CloseScript => {
                self.keep(studio);
                self.script_open = false;
            }
            GenerateMsg::ScriptEdited(chapter, scene, shot, field, text) => {
                self.edit(chapter, scene, shot, field, text);
            }
            GenerateMsg::ShotKindChanged(chapter, scene, shot, video) => {
                if let Some((stem, found)) = self.shot_mut(chapter, scene, shot) {
                    found.kind = if video {
                        ShotKind::Video
                    } else {
                        ShotKind::Image
                    };
                    self.stale_shots.insert(stem);
                    self.dirty = true;
                }
            }
            GenerateMsg::RewriteScene(chapter, scene) => self.rewrite(studio, chapter, scene),
            GenerateMsg::Rewritten(chapter, scene, result) => {
                self.rewriting = None;
                match *result {
                    Ok(new) => {
                        if let Some(board) = self.board.as_mut()
                            && let Some(old) = board
                                .chapters
                                .get_mut(chapter)
                                .and_then(|c| c.scenes.get_mut(scene))
                        {
                            self.stale_voices.insert(old.id.clone());
                            for n in 0..old.shots.len().max(new.shots.len()) {
                                self.stale_shots.insert(shot_stem(&old.id, n));
                            }
                            *old = new;
                            self.dirty = true;
                            self.changed();
                        }
                    }
                    Err(error) => self.script_message = error,
                }
            }
            GenerateMsg::OpenFolder => {
                if let Some(job) = self.job(studio)
                    && let Err(error) = crate::platform::reveal(&job.storyboard().to_string_lossy())
                {
                    studio.notify(&error, true);
                }
            }
            GenerateMsg::Cancel => {
                self.stop.store(true, Ordering::Release);
                studio.host.speech.cancel();
                self.stage = t("generate.stopping");
            }
            GenerateMsg::Progress(progress, stage) => {
                if self.running {
                    self.progress = progress;
                    self.stage = stage;
                }
            }
            GenerateMsg::Written(result, then_make) => {
                self.running = false;
                match *result {
                    Ok(board) => {
                        self.board = Some(board.clone());
                        self.dirty = false;
                        self.changed();
                        if then_make {
                            self.make(studio, board);
                        }
                    }
                    Err(error) => self.failed(error),
                }
            }
            GenerateMsg::Made(result) => {
                self.running = false;
                match *result {
                    Ok((board, made)) => self.cut(studio, &board, &made),
                    Err(error) => self.failed(error),
                }
            }
        }
    }

    /// A worker's failure: said on the sheet, unless it was asked for.
    fn failed(&mut self, error: String) {
        if !error.contains("cancel") {
            self.message = error;
        }
    }

    /// The job the idea names in the open project, if one is open.
    fn job(&self, studio: &Studio) -> Option<Job> {
        let project = studio.session.as_ref()?.path().to_owned();
        Some(Job::new(Path::new(&project), self.topic.trim()))
    }

    /// Reads the storyboard already written for the idea, if there is one.
    fn look(&mut self, studio: &Studio) {
        self.board = self
            .job(studio)
            .filter(|_| !self.topic.trim().is_empty())
            .and_then(|job| Storyboard::read(&job.storyboard()).ok());
        self.dirty = false;
        self.stale_voices.clear();
        self.stale_shots.clear();
        self.changed();
    }

    /// Works out again what follows from the storyboard: which shots repeat.
    fn changed(&mut self) {
        self.repeats = self
            .board
            .as_ref()
            .map(|board| {
                board
                    .repeats()
                    .into_iter()
                    .map(|(_, later)| later)
                    .collect()
            })
            .unwrap_or_default();
    }

    /// Writes the edited storyboard back to the job's folder, and removes
    /// what the edits made stale, so the next run makes it again.
    fn keep(&mut self, studio: &mut Studio) {
        if !self.dirty {
            return;
        }
        let Some(job) = self.job(studio) else {
            return;
        };
        let Some(board) = self.board.as_mut() else {
            return;
        };
        match board.tidy().and_then(|()| job.keep(board)) {
            Ok(()) => {
                let voices: Vec<String> =
                    std::mem::take(&mut self.stale_voices).into_iter().collect();
                let shots: Vec<String> =
                    std::mem::take(&mut self.stale_shots).into_iter().collect();
                job.forget(&voices, &shots);
                self.dirty = false;
            }
            Err(error) => studio.notify(&error, true),
        }
        self.changed();
    }

    /// The shot `shot` of `scene` of `chapter`, with its stem.
    fn shot_mut(
        &mut self,
        chapter: i32,
        scene: i32,
        shot: i32,
    ) -> Option<(String, &mut concat_generate::storyboard::Shot)> {
        let scene = self
            .board
            .as_mut()?
            .chapters
            .get_mut(usize::try_from(chapter).ok()?)?
            .scenes
            .get_mut(usize::try_from(scene).ok()?)?;
        let n = usize::try_from(shot).ok()?;
        let stem = shot_stem(&scene.id, n);
        scene.shots.get_mut(n).map(|found| (stem, found))
    }

    /// One text of the script changed; see the editor's `edited`.
    fn edit(&mut self, chapter: i32, scene: i32, shot: i32, field: i32, text: String) {
        if shot >= 0 {
            let Some((stem, found)) = self.shot_mut(chapter, scene, shot) else {
                return;
            };
            match field {
                0 => found.prompt = text,
                1 => found.motion = text,
                _ => return,
            }
            self.stale_shots.insert(stem);
            self.dirty = true;
            if field == 0 {
                self.changed();
            }
            return;
        }
        let Some(board) = self.board.as_mut() else {
            return;
        };
        let Some(found) = usize::try_from(chapter)
            .ok()
            .and_then(|c| board.chapters.get_mut(c))
        else {
            return;
        };
        if scene < 0 {
            match field {
                0 => found.title = text,
                1 => {
                    // A new mood is a new look for every shot of the chapter.
                    found.mood = text;
                    for scene in &found.scenes {
                        for n in 0..scene.shots.len() {
                            self.stale_shots.insert(shot_stem(&scene.id, n));
                        }
                    }
                }
                2 => found.summary = text,
                _ => return,
            }
            self.dirty = true;
            return;
        }
        let Some(found) = usize::try_from(scene)
            .ok()
            .and_then(|s| found.scenes.get_mut(s))
        else {
            return;
        };
        match field {
            0 => {
                found.narration = text;
                self.stale_voices.insert(found.id.clone());
            }
            1 => {
                found.voice = text;
                self.stale_voices.insert(found.id.clone());
            }
            2 => {
                found.on_screen_text = Some(text).filter(|text| !text.trim().is_empty());
            }
            _ => return,
        }
        self.dirty = true;
    }

    /// The brief the sheet's choices make.
    fn brief(&self, studio: &Studio) -> Brief {
        let prefs = &studio.prefs.generate;
        Brief {
            topic: self.topic.trim().to_owned(),
            minutes: MINUTES[self.minutes.min(MINUTES.len() - 1)],
            language: LANGUAGES[prefs.language.min(LANGUAGES.len() - 1)].to_owned(),
            aspect: ASPECTS[prefs.aspect.min(ASPECTS.len() - 1)].to_owned(),
            notes: self.notes.clone(),
            clip_share: CLIP_SHARES[self.clips_row(studio)],
        }
    }

    /// What the video will cost: the storyboard's own counts once it is
    /// written, the budget's before.
    fn estimate(&self, studio: &Studio) -> Estimate {
        let prefs = &studio.prefs.generate;
        let totals = match &self.board {
            Some(board) => {
                let (clips, stills) = board.kinds();
                Totals {
                    seconds: board.seconds(),
                    chapters: board.chapters.len(),
                    scenes: board.scenes().count(),
                    clips,
                    stills,
                    words: board.words(),
                }
            }
            None => budget::budget(&self.brief(studio).pacing()).totals(),
        };
        let mut estimate = cost::estimate(
            &totals,
            &prices(studio),
            true,
            // A model's inputs are not known before the run reads them:
            // count the still a clip may start from, which costs little.
            true,
            self.voice_row(studio) == 2,
        );
        // Shots from Replicate are paid for; draft cards and shots made by
        // hand are not.
        if prefs.shots != 1 {
            estimate.stills = 0.0;
            estimate.clips = 0.0;
        }
        estimate
    }

    /// The estimate in a line, and over the limit when it is.
    fn cost_line(&self, studio: &Studio) -> String {
        let estimate = self.estimate(studio);
        let mut line = tf(
            "generate.cost",
            &[
                &dollars(estimate.total()),
                &dollars(estimate.script),
                &dollars(estimate.stills),
                &dollars(estimate.clips),
                &dollars(estimate.voice),
            ],
        );
        if let Some(limit) = spend_limit(studio)
            && estimate.total() > limit
        {
            line.push(' ');
            line.push_str(&tf("generate.overLimit", &[&dollars(limit)]));
        }
        line
    }

    /// Row of the clips list.
    fn clips_row(&self, studio: &Studio) -> usize {
        studio
            .prefs
            .generate
            .clips
            .unwrap_or(DEFAULT_CLIPS)
            .min(CLIP_SHARES.len() - 1)
    }

    /// Asks Claude for one scene again, on a worker.
    fn rewrite(&mut self, studio: &mut Studio, chapter: i32, scene: i32) {
        let (Ok(chapter), Ok(scene)) = (usize::try_from(chapter), usize::try_from(scene)) else {
            return;
        };
        let Some(board) = self.board.clone() else {
            return;
        };
        if self.rewriting.is_some() {
            return;
        }
        let key = studio.prefs.generate.anthropic_key.clone();
        if key.trim().is_empty() {
            self.script_message = t("generate.needAnthropicKey");
            return;
        }
        self.script_message.clear();
        self.rewriting = Some((chapter, scene));
        let brief = self.brief(studio);
        spawn_in_project(
            move || {
                writer::rewrite_scene(
                    &board,
                    &brief,
                    chapter,
                    scene,
                    "",
                    false,
                    &key,
                    writer::DEFAULT_MODEL,
                )
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Generate(GenerateMsg::Rewritten(
                    chapter,
                    scene,
                    Box::new(result),
                )))
            },
        );
    }

    /// Asks Claude for the storyboard on a worker; `then_make` goes on to
    /// make and cut it once it is written.
    fn write(&mut self, studio: &mut Studio, then_make: bool) {
        let prefs = studio.prefs.generate.clone();
        if self.topic.trim().is_empty() {
            self.message = t("generate.describeIdeaFirst");
            return;
        }
        if prefs.anthropic_key.trim().is_empty() {
            self.message = t("generate.needAnthropicKey");
            return;
        }
        let Some(job) = self.job(studio) else { return };
        let brief = self.brief(studio);
        self.begin(t("generate.writingOutline"));
        let epoch = crate::host::project_epoch();
        spawn_in_project(
            move || {
                let board =
                    writer::write(&brief, &prefs.anthropic_key, writer::DEFAULT_MODEL, |at| {
                        let (fraction, stage) = match at {
                            Writing::Outline => (0.02, t("generate.writingOutline")),
                            Writing::Chapter { n, of } => (
                                n as f32 / (of + 1) as f32,
                                tf("generate.writingChapter", &[&n, &of]),
                            ),
                        };
                        on_ui_in_project(epoch, move |studio, _, _| {
                            studio.handle(Msg::Generate(GenerateMsg::Progress(fraction, stage)));
                        });
                    })?;
                job.keep(&board)?;
                Ok(board)
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Generate(GenerateMsg::Written(
                    Box::new(result),
                    then_make,
                )))
            },
        );
    }

    /// Makes the narration and the shots on a worker, then cuts.
    fn make(&mut self, studio: &mut Studio, board: Storyboard) {
        let prefs = studio.prefs.generate.clone();
        let Some(job) = self.job(studio) else { return };
        let Some(project) = studio
            .session
            .as_ref()
            .map(|session| session.path().to_owned())
        else {
            return;
        };
        let voice = match self.voice_row(studio) {
            1 => Voice::BuiltIn,
            2 => Voice::ElevenLabs {
                key: prefs.elevenlabs_key.clone(),
                voice_id: prefs.elevenlabs_voice.clone(),
            },
            _ => Voice::None,
        };
        let shots = match prefs.shots {
            1 => Shots::Replicate {
                token: prefs.replicate_token.clone(),
                image_model: prefs.replicate_image_model.clone(),
                video_model: video_model(&prefs.replicate_model).to_owned(),
                video_input: REPLICATE_INPUT.to_owned(),
            },
            2 => Shots::Folder,
            _ => Shots::Draft,
        };
        // The window's own voice: the speech model in use and the speaker
        // chosen last in the speech sheet.
        let model = installed(&studio.settings.voices)
            .iter()
            .find(|model| model.active)
            .or(installed(&studio.settings.voices).first())
            .map(|model| model.id.clone());
        if voice == Voice::BuiltIn && model.is_none() {
            self.message = t("speech.downloadVoiceModelFirst");
            return;
        }
        let costs = Costs {
            prices: prices(studio),
            limit: spend_limit(studio),
        };
        let speaker = studio.prefs.tts_voice.unwrap_or(DEFAULT_SPEAKER);
        let speech = Arc::clone(&studio.host.speech);
        let dirs = studio.host.dirs.clone();

        self.begin(t("generate.makingShots"));
        let stop = Arc::clone(&self.stop);
        let epoch = crate::host::project_epoch();
        spawn_in_project(
            move || {
                let mut speak = |text: &str| -> Result<PathBuf, String> {
                    let request = concat_speech::tts::SpeakRequest {
                        model_id: model.clone().unwrap_or_default(),
                        voice: speaker,
                        reference: None,
                        text: text.to_owned(),
                        speed: 1.0,
                        pauses: Some(0.2),
                        steps: None,
                        project: project.clone(),
                    };
                    speech
                        .speak(&dirs, &request, |_| {})
                        .map(|spoken| PathBuf::from(spoken.path))
                };
                let mut progress = |step: Step| {
                    let (fraction, stage) = match step {
                        Step::Voice { n, of } => (
                            0.4 * n as f32 / of as f32,
                            tf("generate.readingScene", &[&n, &of]),
                        ),
                        Step::Shot { n, of } => (
                            0.4 + 0.55 * n as f32 / of as f32,
                            tf("generate.makingShot", &[&n, &of]),
                        ),
                        Step::Probing => (0.97, t("generate.readingFiles")),
                    };
                    on_ui_in_project(epoch, move |studio, _, _| {
                        studio.handle(Msg::Generate(GenerateMsg::Progress(fraction, stage)));
                    });
                };
                let made = job.make(
                    &board,
                    &voice,
                    &shots,
                    &costs,
                    &mut speak,
                    &mut progress,
                    &|| stop.load(Ordering::Acquire),
                )?;
                Ok((board, made))
            },
            |studio, _, _, result| {
                studio.handle(Msg::Generate(GenerateMsg::Made(Box::new(result))))
            },
        );
    }

    /// Lays the cut on a new timeline, as one undo step, and closes.
    fn cut(&mut self, studio: &mut Studio, board: &Storyboard, made: &Made) {
        let music = self.music.checked_sub(1).and_then(|row| {
            music_items(studio)
                .get(row)
                .map(|(path, length)| (path.clone(), *length))
        });
        let mut media = made.media.clone();
        if let Some((path, _)) = &music
            && let Ok(summary) = concat_host::media::probe(path)
        {
            media.insert(path.clone(), summary.to_new_media());
        }
        let options = Options {
            captions: self.captions,
            titles: self.titles,
            ..Options::default()
        };
        let placements = match plan::plan(
            board,
            &made.files,
            &options,
            music
                .as_ref()
                .map(|(path, length)| (path.as_str(), *length)),
        ) {
            Ok(placements) => placements,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let mut editing = Session(studio);
        match plan::cut(&mut editing, board, &placements, &media, &options) {
            Ok(_) => {
                self.open = false;
                let minutes = plan::total_length(&placements) / 60.0;
                studio.notify(
                    &tf("generate.videoReady", &[&format!("{minutes:.1}")]),
                    false,
                );
                if made.spent > 0.0 {
                    studio.notify(&tf("generate.spent", &[&dollars(made.spent)]), false);
                }
                for stand_in in &made.stand_ins {
                    log::warn!("generate: {stand_in}");
                }
                if !made.stand_ins.is_empty() {
                    studio.notify(&tf("generate.standIns", &[&made.stand_ins.len()]), true);
                }
                if studio.prefs.generate.auto_export.unwrap_or(true)
                    && let Some(job) = self.job(studio)
                {
                    export(job.dir, &board.title);
                }
            }
            Err(error) => self.message = error,
        }
    }

    /// Clears the last run's words and starts the bar.
    fn begin(&mut self, stage: String) {
        self.stop.store(false, Ordering::Release);
        self.running = true;
        self.message.clear();
        self.progress = 0.0;
        self.stage = stage;
    }

    /// Who reads: the remembered choice, or the window's own voices when a
    /// speech model is installed and nothing has been chosen.
    fn voice_row(&self, studio: &Studio) -> usize {
        studio.prefs.generate.voice.unwrap_or_else(|| {
            if installed(&studio.settings.voices).is_empty() {
                0
            } else {
                1
            }
        })
    }

    /// The music list's rows: none, then every recording in the bin.
    pub fn music_rows(&self, studio: &Studio) -> Vec<SharedString> {
        std::iter::once(t("generate.noMusic"))
            .chain(music_items(studio).into_iter().map(|(path, _)| {
                Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or(path)
            }))
            .map(SharedString::from)
            .collect()
    }

    /// The sheet as Slint shows it.
    pub fn data(&self, studio: &Studio) -> GenerateSheetData {
        let prefs = &studio.prefs.generate;
        let summary = self.board.as_ref().map(|board| {
            // A tenth of a minute while that is a difference worth seeing:
            // a short draft would otherwise read as "about 0 min".
            let minutes = board.seconds() / 60.0;
            let minutes = if minutes < 10.0 {
                format!("{minutes:.1}")
            } else {
                format!("{minutes:.0}")
            };
            let (clips, stills) = board.kinds();
            tf(
                "generate.scriptReady",
                &[
                    &board.title,
                    &board.scenes().count(),
                    &clips,
                    &stills,
                    &minutes,
                ],
            )
        });
        let totals = budget::budget(&self.brief(studio).pacing()).totals();
        GenerateSheetData {
            open: self.open,
            topic: self.topic.as_str().into(),
            notes: self.notes.as_str().into(),
            minutes: self.minutes as i32,
            clips: self.clips_row(studio) as i32,
            budget: tf(
                "generate.budget",
                &[
                    &totals.chapters,
                    &totals.scenes,
                    &totals.clips,
                    &totals.stills,
                    &totals.words,
                ],
            )
            .into(),
            cost: self.cost_line(studio).into(),
            spend_limit: prefs.spend_limit.as_str().into(),
            clip_price: prefs.clip_price.as_str().into(),
            image_price: prefs.image_price.as_str().into(),
            language: prefs.language as i32,
            aspect: prefs.aspect as i32,
            voice: self.voice_row(studio) as i32,
            shots: prefs.shots as i32,
            music: self.music as i32,
            captions: self.captions,
            titles: self.titles,
            anthropic_key: prefs.anthropic_key.as_str().into(),
            replicate_token: prefs.replicate_token.as_str().into(),
            replicate_model: prefs.replicate_model.as_str().into(),
            replicate_image_model: prefs.replicate_image_model.as_str().into(),
            auto_export: prefs.auto_export.unwrap_or(true),
            elevenlabs_key: prefs.elevenlabs_key.as_str().into(),
            elevenlabs_voice: prefs.elevenlabs_voice.as_str().into(),
            has_script: self.board.is_some(),
            script_summary: summary.unwrap_or_default().into(),
            running: self.running,
            progress: self.progress,
            stage: self.stage.as_str().into(),
            message: self.message.as_str().into(),
        }
    }
}

impl GeneratePane {
    /// The script editor as Slint shows it.
    pub fn script_data(&self, studio: &Studio) -> ScriptSheetData {
        let Some(board) = self.board.as_ref().filter(|_| self.script_open) else {
            return ScriptSheetData::default();
        };
        let (clips, stills) = board.kinds();
        let minutes = board.seconds() / 60.0;
        let planned = budget::budget(&self.brief(studio).pacing()).totals();
        ScriptSheetData {
            open: true,
            summary: tf(
                "script.summary",
                &[
                    &board.title,
                    &format!("{minutes:.1}"),
                    &board.chapters.len(),
                    &board.scenes().count(),
                    &clips,
                    &stills,
                    &board.words(),
                ],
            )
            .into(),
            planned: tf(
                "script.planned",
                &[
                    &MINUTES[self.minutes.min(MINUTES.len() - 1)],
                    &planned.scenes,
                    &planned.clips,
                    &planned.stills,
                ],
            )
            .into(),
            warning: if self.repeats.is_empty() {
                SharedString::default()
            } else {
                tf("script.repeats", &[&self.repeats.len()]).into()
            },
            message: self.script_message.as_str().into(),
        }
    }

    /// The script editor's rows: each chapter, then its scenes, each scene
    /// then its shots.
    pub fn script_rows(&self) -> Vec<ScriptRow> {
        let Some(board) = self.board.as_ref().filter(|_| self.script_open) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for (c, chapter) in board.chapters.iter().enumerate() {
            rows.push(ScriptRow {
                kind: 0,
                chapter: c as i32,
                scene: -1,
                shot: -1,
                label: tf("script.chapter", &[&(c + 1)]).into(),
                text: chapter.title.as_str().into(),
                detail: chapter.mood.as_str().into(),
                extra: chapter.summary.as_str().into(),
                ..ScriptRow::default()
            });
            for (s, scene) in chapter.scenes.iter().enumerate() {
                rows.push(ScriptRow {
                    kind: 1,
                    chapter: c as i32,
                    scene: s as i32,
                    shot: -1,
                    label: format!("{} · {:.0} s", scene.id, scene.seconds).into(),
                    text: scene.narration.as_str().into(),
                    detail: scene.voice.as_str().into(),
                    extra: scene.on_screen_text.as_deref().unwrap_or_default().into(),
                    busy: self.rewriting == Some((c, s)),
                    ..ScriptRow::default()
                });
                for (n, shot) in scene.shots.iter().enumerate() {
                    let stem = shot_stem(&scene.id, n);
                    rows.push(ScriptRow {
                        kind: 2,
                        chapter: c as i32,
                        scene: s as i32,
                        shot: n as i32,
                        label: format!(
                            "{} · {:.1} s",
                            tf("script.shot", &[&(n + 1)]),
                            shot.seconds
                        )
                        .into(),
                        text: shot.prompt.as_str().into(),
                        detail: shot.motion.as_str().into(),
                        video: shot.kind == ShotKind::Video,
                        repeat: self.repeats.contains(&stem),
                        ..ScriptRow::default()
                    });
                }
            }
        }
        rows
    }
}

/// Renders the cut through the export sheet, exactly as pressing Export
/// would, into the job's folder under the video's title: the last step of
/// a run nobody has to watch. Posted for the next turn, so it starts once
/// the cut is published and the sheet's own state is its to change.
fn export(folder: PathBuf, title: &str) {
    let name: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned();
    let name = if name.is_empty() {
        "video".to_owned()
    } else {
        name
    };
    let folder = folder.to_string_lossy().into_owned();
    on_ui(move |studio, _, _| {
        studio.handle(Msg::Export(ExportMsg::Open));
        studio.handle(Msg::Export(ExportMsg::NameEdited(name)));
        studio.handle(Msg::Export(ExportMsg::FolderChosen(folder)));
        // Overwrite: a run made again replaces its own file, not asks.
        studio.handle(Msg::Export(ExportMsg::Overwrite));
    });
}

/// Every recording in the bin, with its length: what a music bed can be.
fn music_items(studio: &Studio) -> Vec<(String, f64)> {
    studio
        .project()
        .media
        .iter()
        .filter(|item| item.kind == MediaKind::Audio)
        .map(|item| (item.path.clone(), item.duration.unwrap_or(0.0)))
        .collect()
}

/// The window's session as the cut lays itself down through it: every
/// command folded into one undo step.
struct Session<'a>(&'a mut Studio);

impl Editing for Session<'_> {
    fn apply(&mut self, command: Command) -> Option<String> {
        self.0.apply_gesture("generate", command)
    }

    fn lanes(&self) -> Vec<String> {
        self.0
            .timeline()
            .tracks
            .iter()
            .map(|track| track.id.clone())
            .collect()
    }

    fn media_of(&self, path: &str) -> Option<String> {
        self.0
            .project()
            .media
            .iter()
            .find(|item| item.path == path)
            .map(|item| item.id.clone())
    }
}

/// The video model to use: the one named, or the default.
fn video_model(named: &str) -> &str {
    if named.trim().is_empty() {
        DEFAULT_VIDEO_MODEL
    } else {
        named.trim()
    }
}

/// A price or a limit typed in the sheet, in dollars: None when it is
/// empty or not a number of them.
fn amount(text: &str) -> Option<f64> {
    text.trim()
        .trim_start_matches('$')
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

/// What each thing costs: the prices typed in the sheet, the defaults'
/// where none is.
fn prices(studio: &Studio) -> Prices {
    let prefs = &studio.prefs.generate;
    let defaults = Prices::default();
    Prices {
        still: amount(&prefs.image_price).unwrap_or(defaults.still),
        clip: amount(&prefs.clip_price).unwrap_or(defaults.clip),
        ..defaults
    }
}

/// The most a run may spend; None for no limit.
fn spend_limit(studio: &Studio) -> Option<f64> {
    amount(&studio.prefs.generate.spend_limit)
}

/// Dollars as the sheet shows them: cents, or a tenth of one below a cent.
fn dollars(value: f64) -> String {
    if value > 0.0 && value < 0.01 {
        format!("{value:.3}")
    } else {
        format!("{value:.2}")
    }
}
