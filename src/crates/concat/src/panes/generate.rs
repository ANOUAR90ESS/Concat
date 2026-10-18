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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use concat_generate::job::{Job, Made, Step};
use concat_generate::plan::{self, Editing, Options};
use concat_generate::providers::{Shots, Voice};
use concat_generate::storyboard::Storyboard;
use concat_generate::writer::{self, Brief, Writing};
use concat_project::Command;
use concat_project::model::MediaKind;
use slint::SharedString;

use crate::host::{on_ui_in_project, spawn_in_project};
use crate::i18n::{t, tf};
use crate::panes::Msg;
use crate::panes::settings::installed;
use crate::studio::Studio;
use crate::ui::GenerateSheetData;

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
            GenerateMsg::Close => self.open = false,
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
                    _ => return,
                }
                studio.prefs.save(&studio.host.dirs);
            }
            GenerateMsg::Toggled(field, on) => match field {
                0 => self.captions = on,
                1 => self.titles = on,
                _ => {}
            },
            GenerateMsg::WriteScript => self.write(studio, false),
            GenerateMsg::Create => match self.board.clone() {
                Some(board) => self.make(studio, board),
                None => self.write(studio, true),
            },
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
        let brief = Brief {
            topic: self.topic.trim().to_owned(),
            minutes: MINUTES[self.minutes.min(MINUTES.len() - 1)],
            language: LANGUAGES[prefs.language.min(LANGUAGES.len() - 1)].to_owned(),
            aspect: ASPECTS[prefs.aspect.min(ASPECTS.len() - 1)].to_owned(),
            notes: self.notes.clone(),
        };
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
                model: prefs.replicate_model.clone(),
                input: REPLICATE_INPUT.to_owned(),
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
                let made = job.make(&board, &voice, &shots, &mut speak, &mut progress, &|| {
                    stop.load(Ordering::Acquire)
                })?;
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
            tf(
                "generate.scriptReady",
                &[
                    &board.title,
                    &board.scenes().count(),
                    &board.shot_count(),
                    &format!("{:.0}", board.seconds() / 60.0),
                ],
            )
        });
        GenerateSheetData {
            open: self.open,
            topic: self.topic.as_str().into(),
            notes: self.notes.as_str().into(),
            minutes: self.minutes as i32,
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
