// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! A job: one video's folder, and the work that fills it.
//!
//! ```text
//! <project>/generate/<name>/
//!     storyboard.json     the script, scene by scene
//!     prompts.md          every shot's prompt, for making shots by hand
//!     voice/<scene>.*     the narration
//!     shots/<scene>_<n>.* the shots
//!     frames/<scene>_<n>.* the stills clips start from
//!     idea.json           the idea it was written from, to find it again
//! ```
//!
//! Every step skips a file that is already there, so a job stopped half
//! way, or one whose files were replaced by hand, carries on from where it
//! is; and the folder sits in the project's, so it moves with it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use concat_project::commands::NewMedia;
use concat_project::model::MediaKind;

use crate::cost::{Prices, Spend};
use crate::plan::SceneFiles;
use crate::providers::{self, ModelInputs, SHOT_EXTENSIONS, Shots, VOICE_EXTENSIONS, Voice};
use crate::storyboard::{Chapter, Shot, ShotKind, Storyboard, shot_stem};

/// One video's folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// `<project>/generate/<name>`.
    pub dir: PathBuf,
}

/// Where the making is, for the sheet's bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Scene `n` (from 1) of `of` being read aloud.
    Voice {
        /// This one.
        n: usize,
        /// Of how many.
        of: usize,
        /// The words being read.
        text: String,
    },
    /// Shot `n` (from 1) of `of` being made.
    Shot {
        /// This one.
        n: usize,
        /// Of how many.
        of: usize,
        /// What it is to show.
        prompt: String,
    },
    /// Shot `n` is made and saved: a run ended after this keeps it.
    Made {
        /// This one.
        n: usize,
        /// Of how many.
        of: usize,
        /// A still of it to show, when there is one: the shot itself, or
        /// the picture a clip started from.
        picture: Option<PathBuf>,
        /// What the run has paid so far, in dollars.
        spent: f64,
    },
    /// The files being probed.
    Probing,
}

/// The idea a job was written from: kept in its folder, so the sheet opened
/// again - the app closed, another project opened in between - finds the
/// job and carries on with it rather than starting over.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Idea {
    /// The idea, in the person's words: what names the job's folder.
    pub topic: String,
    /// Audience, tone, must-haves.
    #[serde(default)]
    pub notes: String,
}

/// How far a job is: what its folder already holds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    /// Scenes with words to read.
    pub voices: usize,
    /// How many of them are read.
    pub voices_made: usize,
    /// Shots.
    pub shots: usize,
    /// How many of them are made.
    pub shots_made: usize,
}

/// The extensions of a still, to show one of a shot.
const STILL_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// What the making leaves for the cut.
#[derive(Clone, Debug, Default)]
pub struct Made {
    /// Each scene's files and their lengths.
    pub files: HashMap<String, SceneFiles>,
    /// Each file's probe, to import it with.
    pub media: HashMap<String, NewMedia>,
    /// What the run worked around rather than stopped for: a clip that
    /// became a still, a still that became a colour card. Said once the
    /// video is done, so an unattended run finishes and says what it did.
    pub stand_ins: Vec<String>,
    /// About what this run paid the services, in dollars.
    pub spent: f64,
}

/// What a run may spend, and what each thing costs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Costs {
    /// What each thing costs.
    pub prices: Prices,
    /// The most a run may spend; None for no limit.
    pub limit: Option<f64>,
}

impl Job {
    /// The job for `name` in the project folder `project`.
    pub fn new(project: &Path, name: &str) -> Job {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug: String = slug.chars().take(60).collect();
        Job {
            dir: project.join("generate").join(if slug.is_empty() {
                "video".to_owned()
            } else {
                slug
            }),
        }
    }

    /// The idea's file.
    pub fn idea(&self) -> PathBuf {
        self.dir.join("idea.json")
    }

    /// Keeps the idea the job is written from in its folder.
    pub fn remember(&self, idea: &Idea) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("{}: {error}", self.dir.display()))?;
        let text = serde_json::to_string_pretty(idea).map_err(|error| error.to_string())?;
        std::fs::write(self.idea(), text)
            .map_err(|error| format!("{}: {error}", self.idea().display()))
    }

    /// The idea of the job in `project` worked on last: the one whose
    /// folder changed last, among those that keep their idea.
    pub fn latest(project: &Path) -> Option<Idea> {
        std::fs::read_dir(project.join("generate"))
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let job = Job { dir: entry.path() };
                let idea: Idea =
                    serde_json::from_str(&std::fs::read_to_string(job.idea()).ok()?).ok()?;
                let changed = job.last_change();
                Some((changed, idea))
            })
            .filter(|(_, idea)| !idea.topic.trim().is_empty())
            .max_by_key(|(changed, _)| *changed)
            .map(|(_, idea)| idea)
    }

    /// When anything in the folder was last written.
    fn last_change(&self) -> std::time::SystemTime {
        let changed = |path: &Path| path.metadata().and_then(|meta| meta.modified()).ok();
        [
            self.idea(),
            self.storyboard(),
            self.shots_dir(),
            self.voice_dir(),
        ]
        .iter()
        .filter_map(|path| changed(path))
        .max()
        .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// How far the job is with `board`; `voiced` is whether its scenes
    /// are read at all.
    pub fn status(&self, board: &Storyboard, voiced: bool) -> Status {
        let mut status = Status::default();
        if voiced {
            for scene in board
                .scenes()
                .filter(|scene| !scene.narration.trim().is_empty())
            {
                status.voices += 1;
                if providers::existing(&self.voice_dir(), &scene.id, &VOICE_EXTENSIONS).is_some() {
                    status.voices_made += 1;
                }
            }
        }
        let stood_in: Vec<String> = std::fs::read_to_string(self.stand_ins())
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_owned())
            .collect();
        for scene in board.scenes() {
            for index in 0..scene.shots.len() {
                let stem = shot_stem(&scene.id, index);
                status.shots += 1;
                if !stood_in.contains(&stem) && self.current(&stem).is_some() {
                    status.shots_made += 1;
                }
            }
        }
        status
    }

    /// A still of the shot `stem` to show: the shot, or the picture the
    /// clip started from.
    pub fn picture(&self, stem: &str) -> Option<PathBuf> {
        let current = self.current(stem)?;
        let still = current
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|extension| STILL_EXTENSIONS.contains(&extension));
        if still {
            return Some(current);
        }
        let name = current.file_stem()?.to_str()?;
        providers::existing(&self.frames_dir(), name, &STILL_EXTENSIONS)
    }

    /// The storyboard's file.
    pub fn storyboard(&self) -> PathBuf {
        self.dir.join("storyboard.json")
    }

    /// The prompt sheet's file.
    pub fn prompts(&self) -> PathBuf {
        self.dir.join("prompts.md")
    }

    /// The shots the last run stood in for, one name a line: they are
    /// made again on the next run rather than taken for finished.
    pub fn stand_ins(&self) -> PathBuf {
        self.dir.join("stand-ins.txt")
    }

    /// Removes the shots the last run stood in for, so this run makes them.
    fn forget_stand_ins(&self) {
        let Ok(list) = std::fs::read_to_string(self.stand_ins()) else {
            return;
        };
        for stem in list.lines().map(str::trim).filter(|stem| !stem.is_empty()) {
            while let Some(path) = providers::existing(&self.shots_dir(), stem, &SHOT_EXTENSIONS) {
                if std::fs::remove_file(&path).is_err() {
                    break;
                }
            }
        }
        let _ = std::fs::remove_file(self.stand_ins());
    }

    /// Notes the shots this run stood in for; see [`Job::stand_ins`].
    fn remember_stand_ins(&self, stems: &[String]) -> Result<(), String> {
        if stems.is_empty() {
            return Ok(());
        }
        std::fs::write(self.stand_ins(), stems.join("\n") + "\n")
            .map_err(|error| format!("{}: {error}", self.stand_ins().display()))
    }

    /// Where the narration goes.
    pub fn voice_dir(&self) -> PathBuf {
        self.dir.join("voice")
    }

    /// Where the shots go.
    pub fn shots_dir(&self) -> PathBuf {
        self.dir.join("shots")
    }

    /// Where the stills clips start from go: apart from the shots, so a
    /// clip not made yet is not taken for a still that is.
    pub fn frames_dir(&self) -> PathBuf {
        self.dir.join("frames")
    }

    /// Removes what was made for scenes and shots the script has changed
    /// since: the narration of each of `voices` (scene ids) and each of
    /// `shots` (shot stems) - with the still a clip started from - so the
    /// next run makes them from the new words.
    pub fn forget(&self, voices: &[String], shots: &[String]) {
        for (dir, names, extensions) in [
            (self.voice_dir(), voices, &VOICE_EXTENSIONS[..]),
            (self.shots_dir(), shots, &SHOT_EXTENSIONS[..]),
            (self.frames_dir(), shots, &SHOT_EXTENSIONS[..]),
        ] {
            for name in names {
                while let Some(path) = providers::existing(&dir, name, extensions) {
                    if std::fs::remove_file(&path).is_err() {
                        break;
                    }
                }
                // A shot asked for from the old words is not followed.
                let _ = std::fs::remove_file(providers::pending(&dir, name));
            }
        }
        // Nor are the takes made from them, nor the stills those started
        // from.
        for stem in shots {
            for (_, take) in self.takes(stem) {
                if let Some(name) = take.file_stem().and_then(|name| name.to_str()) {
                    if let Some(frame) =
                        providers::existing(&self.frames_dir(), name, &SHOT_EXTENSIONS)
                    {
                        let _ = std::fs::remove_file(frame);
                    }
                    let _ = std::fs::remove_file(providers::pending(&self.takes_dir(), name));
                }
                let _ = std::fs::remove_file(take);
            }
        }
    }

    /// Removes what was made for `old` that `new` asks for differently: a
    /// script written again keeps the narration and the shots whose words
    /// it did not change, and makes the rest from the new words rather
    /// than showing pictures of the old ones.
    pub fn retire(&self, old: &Storyboard, new: &Storyboard) {
        let voices = |board: &Storyboard| -> HashMap<String, (String, String)> {
            board
                .scenes()
                .map(|scene| {
                    (
                        scene.id.clone(),
                        (scene.narration.clone(), scene.voice.clone()),
                    )
                })
                .collect()
        };
        let shots = |board: &Storyboard| -> HashMap<String, (String, String, ShotKind)> {
            board
                .chapters
                .iter()
                .flat_map(|chapter| chapter.scenes.iter().map(move |scene| (chapter, scene)))
                .flat_map(|(chapter, scene)| {
                    scene.shots.iter().enumerate().map(move |(index, shot)| {
                        (
                            shot_stem(&scene.id, index),
                            (
                                board.prompt(chapter, shot),
                                board.video_prompt(chapter, shot),
                                shot.kind,
                            ),
                        )
                    })
                })
                .collect()
        };
        let (old_voices, new_voices) = (voices(old), voices(new));
        let (old_shots, new_shots) = (shots(old), shots(new));
        let changed_voices: Vec<String> = old_voices
            .iter()
            .filter(|(id, words)| new_voices.get(*id) != Some(words))
            .map(|(id, _)| id.clone())
            .collect();
        let changed_shots: Vec<String> = old_shots
            .iter()
            .filter(|(stem, words)| new_shots.get(*stem) != Some(words))
            .map(|(stem, _)| stem.clone())
            .collect();
        self.forget(&changed_voices, &changed_shots);
    }

    /// Writes the storyboard and its prompt sheet.
    pub fn keep(&self, board: &Storyboard) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("{}: {error}", self.dir.display()))?;
        board.write(&self.storyboard())?;
        std::fs::write(self.prompts(), board.prompt_sheet())
            .map_err(|error| format!("{}: {error}", self.prompts().display()))
    }

    /// Makes every narration and shot the job is missing, then probes them
    /// all. `speak` reads one scene's words into a file for
    /// [`Voice::BuiltIn`] and is not called otherwise; `stop` is asked
    /// between files. Every paid call is counted against `costs`' limit
    /// first, and the run stops at a call the limit does not cover.
    #[allow(clippy::too_many_arguments)]
    pub fn make(
        &self,
        board: &Storyboard,
        voice: &Voice,
        shots: &Shots,
        costs: &Costs,
        speak: &mut dyn FnMut(&str) -> Result<PathBuf, String>,
        progress: &mut dyn FnMut(Step),
        stop: &dyn Fn() -> bool,
    ) -> Result<Made, String> {
        let voice_dir = self.voice_dir();
        let shots_dir = self.shots_dir();
        let mut spend = Spend::new(costs.limit);
        let prices = &costs.prices;
        for dir in [&voice_dir, &shots_dir] {
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        self.forget_stand_ins();

        // The narration first: it decides every scene's length.
        let spoken: Vec<_> = board
            .scenes()
            .filter(|scene| !scene.narration.trim().is_empty())
            .collect();
        if *voice != Voice::None {
            for (i, scene) in spoken.iter().enumerate() {
                if stop() {
                    return Err("cancelled".to_owned());
                }
                if providers::existing(&voice_dir, &scene.id, &VOICE_EXTENSIONS).is_some() {
                    continue;
                }
                progress(Step::Voice {
                    n: i + 1,
                    of: spoken.len(),
                    text: scene.narration.trim().to_owned(),
                });
                match voice {
                    Voice::None => {}
                    Voice::BuiltIn => {
                        let made = speak(scene.narration.trim())?;
                        let extension = made.extension().and_then(|e| e.to_str()).unwrap_or("wav");
                        let out = voice_dir.join(format!("{}.{extension}", scene.id));
                        std::fs::copy(&made, &out)
                            .map_err(|error| format!("{}: {error}", out.display()))?;
                    }
                    Voice::ElevenLabs { key, voice_id } => {
                        let text = scene.narration.trim();
                        let cost = text.chars().count() as f64 / 1000.0 * prices.voice_per_1k;
                        spend.take(cost)?;
                        providers::retry(|| {
                            providers::elevenlabs(
                                key,
                                voice_id,
                                text,
                                &voice_dir.join(format!("{}.mp3", scene.id)),
                            )
                        })
                        .inspect_err(|_| spend.refund(cost))?
                    }
                }
            }
        }

        let (width, height) = board.frame();
        let total = board.shot_count();
        let mut missing = Vec::new();
        let mut stand_ins = Vec::new();
        let mut stood_in = Vec::new();
        // What the video model takes, read once, when the first clip is due.
        let mut inputs: Option<Option<ModelInputs>> = None;
        let mut n = 0;
        for (chapter, scene) in board
            .chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().map(move |scene| (chapter, scene)))
        {
            for (index, shot) in scene.shots.iter().enumerate() {
                n += 1;
                let stem = shot_stem(&scene.id, index);
                if providers::existing(&shots_dir, &stem, &SHOT_EXTENSIONS).is_some() {
                    continue;
                }
                if stop() {
                    return Err("cancelled".to_owned());
                }
                let seconds = shot_seconds(scene, shot);
                match shots {
                    Shots::Folder => missing.push(stem.clone()),
                    Shots::Draft => {
                        progress(Step::Shot {
                            n,
                            of: total,
                            prompt: shot.prompt.clone(),
                        });
                        providers::draft(&shots_dir.join(format!("{stem}.png")), width, height, n)?;
                    }
                    Shots::Replicate { .. } => {
                        progress(Step::Shot {
                            n,
                            of: total,
                            prompt: shot.prompt.clone(),
                        });
                        let order = Order::new(board, chapter, shot, seconds, &shots_dir, &stem);
                        match self.replicate_shot(shots, &order, &mut inputs, prices, &mut spend)? {
                            Outcome::Made => {}
                            Outcome::StillForClip(error) => stand_ins
                                .push(format!("{stem}: a still in place of a clip ({error})")),
                            // A still that cannot be made becomes a colour
                            // card: the cut is whole, and the card says
                            // which shot to make again.
                            Outcome::Failed(error) => {
                                stand_ins.push(format!(
                                    "{stem}: a colour card in place of a still ({error})"
                                ));
                                stood_in.push(stem.clone());
                                providers::draft(
                                    &shots_dir.join(format!("{stem}.png")),
                                    width,
                                    height,
                                    n,
                                )?;
                                // Noted as it happens: a run that ends early
                                // still makes this shot again next time.
                                self.remember_stand_ins(&stood_in)?;
                            }
                        }
                    }
                }
                if !matches!(shots, Shots::Folder) {
                    progress(Step::Made {
                        n,
                        of: total,
                        picture: self.picture(&stem),
                        spent: spend.spent,
                    });
                }
            }
        }
        if !missing.is_empty() {
            return Err(format!(
                "{} shots are still to be made by hand: see prompts.md, and save them in {}",
                missing.len(),
                shots_dir.display()
            ));
        }

        progress(Step::Probing);
        let mut made = Made {
            stand_ins,
            spent: spend.spent,
            ..Made::default()
        };
        let mut probe = |path: &Path| -> Result<(String, Option<f64>), String> {
            let path_text = path.to_string_lossy().into_owned();
            let summary = concat_host::media::probe(&path_text)?;
            let item = summary.to_new_media();
            let length = (item.kind != MediaKind::Image).then(|| item.duration.unwrap_or(0.0));
            made.media.insert(path_text.clone(), item);
            Ok((path_text, length))
        };
        let mut files = HashMap::new();
        for scene in board.scenes() {
            let mut scene_files = SceneFiles::default();
            if *voice != Voice::None
                && let Some(path) = providers::existing(&voice_dir, &scene.id, &VOICE_EXTENSIONS)
            {
                let (path, length) = probe(&path)?;
                scene_files.voice = Some((path, length.unwrap_or(0.0)));
            }
            for index in 0..scene.shots.len() {
                let stem = shot_stem(&scene.id, index);
                let path = self
                    .current(&stem)
                    .ok_or_else(|| format!("shot {stem} is missing"))?;
                scene_files.shots.push(probe(&path)?);
            }
            files.insert(scene.id.clone(), scene_files);
        }
        made.files = files;
        Ok(made)
    }
}

/// One shot to make: what [`Job::replicate_shot`] is handed.
struct Order<'a> {
    /// The shot's still prompt, for a still or the picture a clip starts
    /// from.
    prompt: String,
    /// The shot's picture and how it moves.
    video_prompt: String,
    kind: ShotKind,
    seconds: f64,
    aspect: &'a str,
    /// Where the shot goes, and its file's name without the extension.
    dir: &'a Path,
    stem: &'a str,
}

impl<'a> Order<'a> {
    fn new(
        board: &'a Storyboard,
        chapter: &Chapter,
        shot: &Shot,
        seconds: f64,
        dir: &'a Path,
        stem: &'a str,
    ) -> Order<'a> {
        Order {
            prompt: board.prompt(chapter, shot),
            video_prompt: board.video_prompt(chapter, shot),
            kind: shot.kind,
            seconds,
            aspect: &board.aspect,
            dir,
            stem,
        }
    }
}

/// What a shot asked of Replicate came to, short of a failure that ends
/// the run.
enum Outcome {
    /// Made as asked.
    Made,
    /// A clip that could not be made, stood in for by the still it was to
    /// start from; why.
    StillForClip(String),
    /// Nothing could be made; why.
    Failed(String),
}

/// A shot made again: see [`Job::remake`].
#[derive(Clone, Debug, PartialEq)]
pub struct Remade {
    /// The new take.
    pub path: PathBuf,
    /// The take it replaces, which is kept.
    pub previous: Option<PathBuf>,
    /// About what it cost, in dollars.
    pub spent: f64,
}

/// The seconds a shot runs for: its own, or its share of the scene's.
fn shot_seconds(scene: &crate::storyboard::Scene, shot: &Shot) -> f64 {
    if shot.seconds > 0.0 {
        shot.seconds
    } else {
        scene.seconds / scene.shots.len().max(1) as f64
    }
    .clamp(2.0, 10.0)
}

impl Job {
    /// Where the takes of a shot made again go: never over a file the
    /// timeline may be showing, so each take is a file of its own and
    /// Undo goes back to the one before.
    pub fn takes_dir(&self) -> PathBuf {
        self.dir.join("takes")
    }

    /// The takes of `stem`, oldest first, with their numbers.
    fn takes(&self, stem: &str) -> Vec<(u32, PathBuf)> {
        let mut takes: Vec<(u32, PathBuf)> = std::fs::read_dir(self.takes_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_stem()?.to_str()?;
                let (base, number) = name.rsplit_once('-')?;
                let number: u32 = number.parse().ok()?;
                let extension = path.extension()?.to_str()?;
                (base == stem
                    && SHOT_EXTENSIONS.contains(&extension)
                    && path.metadata().is_ok_and(|meta| meta.len() > 0))
                .then_some((number, path))
            })
            .collect();
        takes.sort();
        takes
    }

    /// The file a shot is now: its latest take, or the one first made.
    pub fn current(&self, stem: &str) -> Option<PathBuf> {
        self.takes(stem)
            .pop()
            .map(|(_, path)| path)
            .or_else(|| providers::existing(&self.shots_dir(), stem, &SHOT_EXTENSIONS))
    }

    /// The shot a file of this job's is, by its stem: one first made, or
    /// a take of one. None for a file that is not one of its shots.
    pub fn stem_of(&self, path: &Path) -> Option<String> {
        let parent = path.parent()?;
        let name = path.file_stem()?.to_str()?;
        if parent == self.shots_dir() {
            Some(name.to_owned())
        } else if parent == self.takes_dir() {
            let (base, number) = name.rsplit_once('-')?;
            number.parse::<u32>().ok()?;
            Some(base.to_owned())
        } else {
            None
        }
    }

    /// The job a shot's file belongs to, read from where it is: the folder
    /// above its `shots/` or `takes/`, when that folder holds a storyboard.
    pub fn of_shot(path: &Path) -> Option<Job> {
        let folder = path.parent()?;
        if !matches!(folder.file_name()?.to_str()?, "shots" | "takes") {
            return None;
        }
        let job = Job {
            dir: folder.parent()?.to_owned(),
        };
        job.storyboard().is_file().then_some(job)
    }

    /// Makes the shot `stem` of `board` again, as a new take beside the
    /// ones before: nothing already made is moved or written over, so a
    /// take the person likes less is one Undo away. A take that cannot be
    /// made as asked - a clip that would have become a still, a still a
    /// colour card - is not kept, and says why instead.
    pub fn remake(
        &self,
        board: &Storyboard,
        stem: &str,
        shots: &Shots,
        costs: &Costs,
    ) -> Result<Remade, String> {
        let (chapter, scene, index) = board
            .chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().map(move |scene| (chapter, scene)))
            .find_map(|(chapter, scene)| {
                (0..scene.shots.len())
                    .find(|index| shot_stem(&scene.id, *index) == stem)
                    .map(|index| (chapter, scene, index))
            })
            .ok_or_else(|| format!("the script has no shot {stem}"))?;
        let shot = &scene.shots[index];
        let previous = self.current(stem);
        let number = self
            .takes(stem)
            .last()
            .map(|(number, _)| number + 1)
            .unwrap_or(2);
        let take = format!("{stem}-{number}");
        let dir = self.takes_dir();
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        let mut spend = Spend::new(costs.limit);
        match shots {
            Shots::Folder => {
                return Err(format!(
                    "a shot made by hand is made again by hand: save it in {}",
                    self.shots_dir().display()
                ));
            }
            Shots::Draft => {
                let (width, height) = board.frame();
                providers::draft(
                    &dir.join(format!("{take}.png")),
                    width,
                    height,
                    number as usize + index,
                )?;
            }
            Shots::Replicate { .. } => {
                let order =
                    Order::new(board, chapter, shot, shot_seconds(scene, shot), &dir, &take);
                let refused = match self.replicate_shot(
                    shots,
                    &order,
                    &mut None,
                    &costs.prices,
                    &mut spend,
                )? {
                    Outcome::Made => None,
                    Outcome::StillForClip(error) | Outcome::Failed(error) => Some(error),
                };
                if let Some(error) = refused {
                    while let Some(path) = providers::existing(&dir, &take, &SHOT_EXTENSIONS) {
                        if std::fs::remove_file(&path).is_err() {
                            break;
                        }
                    }
                    return Err(error);
                }
            }
        }
        let path = providers::existing(&dir, &take, &SHOT_EXTENSIONS)
            .ok_or_else(|| format!("{take} was not saved"))?;
        Ok(Remade {
            path,
            previous,
            spent: spend.spent,
        })
    }

    /// Makes one shot with Replicate, as `order` says. A shot written as
    /// a clip moves, when there is a video model; a clip that cannot be
    /// made becomes a still - the one it was to start from, when there is
    /// one - rather than stopping the run. Fails outright only for what
    /// fails every shot alike: the spend limit, or a key that is wrong or
    /// out of credit.
    fn replicate_shot(
        &self,
        shots: &Shots,
        order: &Order,
        inputs: &mut Option<Option<ModelInputs>>,
        prices: &Prices,
        spend: &mut Spend,
    ) -> Result<Outcome, String> {
        let Shots::Replicate {
            token,
            image_model,
            video_model,
            video_input,
        } = shots
        else {
            return Err("these shots are not made with Replicate".to_owned());
        };
        let mut still_for_clip = None;
        if order.kind == ShotKind::Video && !video_model.trim().is_empty() {
            // What the video model takes, read once, when the first clip
            // is due.
            let inputs = inputs.get_or_insert_with(|| {
                providers::replicate_inputs(token, video_model)
                    .inspect_err(|error| log::warn!("{video_model}'s inputs: {error}"))
                    .ok()
            });
            let clip = Clip {
                token,
                image_model,
                video_model,
                video_input,
                order,
            };
            match self.clip(&clip, inputs.as_ref(), prices, spend) {
                Ok(()) => return Ok(Outcome::Made),
                Err(error) if error.starts_with(crate::cost::LIMIT_REACHED) => return Err(error),
                Err(error) => {
                    if let Some(frame) =
                        providers::existing(&self.frames_dir(), order.stem, &SHOT_EXTENSIONS)
                    {
                        let extension = frame.extension().and_then(|e| e.to_str()).unwrap_or("png");
                        let out = order.dir.join(format!("{}.{extension}", order.stem));
                        std::fs::copy(&frame, &out)
                            .map_err(|error| format!("{}: {error}", out.display()))?;
                        return Ok(Outcome::StillForClip(error));
                    }
                    still_for_clip = Some(error);
                }
            }
        }
        spend.take(prices.still)?;
        let image = providers::retry(|| {
            providers::replicate_image(
                token,
                image_model,
                &order.prompt,
                order.aspect,
                order.dir,
                order.stem,
            )
        });
        match image {
            Ok(_) => Ok(match still_for_clip {
                Some(error) => Outcome::StillForClip(error),
                None => Outcome::Made,
            }),
            Err(error) => {
                spend.refund(prices.still);
                if providers::refused_for_good(&error) || error.contains("needs an API token") {
                    // A key that is wrong or out of credit fails every
                    // shot the same way: say so now.
                    return Err(error);
                }
                Ok(Outcome::Failed(error))
            }
        }
    }
}

/// One clip to make: what [`Job::clip`] is handed.
struct Clip<'a> {
    token: &'a str,
    image_model: &'a str,
    video_model: &'a str,
    video_input: &'a str,
    order: &'a Order<'a>,
}

impl Job {
    /// Makes one clip. A model that starts from a picture is sent a still
    /// made first from the shot's own prompt, kept in `frames/` so a clip
    /// tried again does not pay for it twice; one that takes words alone,
    /// or whose inputs could not be read, is sent the words.
    fn clip(
        &self,
        clip: &Clip,
        inputs: Option<&ModelInputs>,
        prices: &Prices,
        spend: &mut Spend,
    ) -> Result<(), String> {
        let order = clip.order;
        let image = match inputs.and_then(ModelInputs::image_field) {
            Some(_) => {
                let frames = self.frames_dir();
                std::fs::create_dir_all(&frames)
                    .map_err(|error| format!("{}: {error}", frames.display()))?;
                let frame = match providers::existing(&frames, order.stem, &SHOT_EXTENSIONS) {
                    Some(frame) => frame,
                    None => {
                        spend.take(prices.still)?;
                        providers::retry(|| {
                            providers::replicate_image(
                                clip.token,
                                clip.image_model,
                                &order.prompt,
                                order.aspect,
                                &frames,
                                order.stem,
                            )
                        })
                        .inspect_err(|_| spend.refund(prices.still))?
                    }
                };
                Some(providers::retry(|| {
                    providers::replicate_upload(clip.token, &frame)
                })?)
            }
            None => None,
        };
        let input = match inputs {
            Some(inputs) => inputs.clip(
                &order.video_prompt,
                order.seconds,
                order.aspect,
                image.as_deref(),
            ),
            None => providers::templated_clip(
                clip.video_input,
                &order.video_prompt,
                order.seconds,
                order.aspect,
            )?,
        };
        spend.take(prices.clip)?;
        providers::retry(|| {
            providers::replicate_clip(
                clip.token,
                clip.video_model,
                input.clone(),
                order.dir,
                order.stem,
            )
        })
        .inspect_err(|_| spend.refund(prices.clip))
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stood_in_shot_is_made_again_next_time() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-standin-{}", std::process::id()));
        let job = Job::new(&root, "stand ins");
        std::fs::create_dir_all(job.shots_dir()).unwrap();
        for stem in ["c01s01_1", "c01s01_2"] {
            std::fs::write(job.shots_dir().join(format!("{stem}.png")), b"x").unwrap();
        }
        job.remember_stand_ins(&["c01s01_2".to_owned()]).unwrap();
        job.forget_stand_ins();
        assert!(job.shots_dir().join("c01s01_1.png").exists());
        assert!(!job.shots_dir().join("c01s01_2.png").exists());
        assert!(!job.stand_ins().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_edited_shot_is_forgotten_with_the_still_it_started_from() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-forget-{}", std::process::id()));
        let job = Job::new(&root, "forget");
        std::fs::create_dir_all(job.shots_dir()).unwrap();
        std::fs::create_dir_all(job.frames_dir()).unwrap();
        for (dir, name) in [
            (job.shots_dir(), "c01s01_1.mp4"),
            (job.frames_dir(), "c01s01_1.png"),
            (job.shots_dir(), "c01s01_2.png"),
            (job.frames_dir(), "c01s01_2.png"),
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        job.forget(&[], &["c01s01_1".to_owned()]);
        assert!(!job.shots_dir().join("c01s01_1.mp4").exists());
        assert!(!job.frames_dir().join("c01s01_1.png").exists());
        assert!(job.shots_dir().join("c01s01_2.png").exists());
        assert!(job.frames_dir().join("c01s01_2.png").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_job_folder_is_named_after_the_video() {
        let job = Job::new(Path::new("/p"), "المدن المفقودة: الجزء 1!");
        assert_eq!(job.dir, Path::new("/p/generate/المدن-المفقودة-الجزء-1"));
        assert_eq!(
            Job::new(Path::new("/p"), "  ").dir,
            Path::new("/p/generate/video")
        );
    }

    #[test]
    fn the_job_worked_on_last_is_found_again() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-latest-{}", std::process::id()));
        assert_eq!(Job::latest(&root), None);
        let older = Job::new(&root, "older idea");
        older
            .remember(&Idea {
                topic: "older idea".into(),
                notes: String::new(),
            })
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let newer = Job::new(&root, "newer idea");
        let idea = Idea {
            topic: "newer idea".into(),
            notes: "for children".into(),
        };
        newer.remember(&idea).unwrap();
        assert_eq!(Job::latest(&root), Some(idea));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_job_says_how_far_it_is() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-status-{}", std::process::id()));
        let job = Job::new(&root, "status");
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","chapters":[{"title":"A","scenes":[
                {"narration":"n","seconds":6,"shots":[{"prompt":"a"},{"prompt":"b"}]},
                {"narration":"","seconds":2,"shots":[{"prompt":"c"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        std::fs::create_dir_all(job.shots_dir()).unwrap();
        std::fs::create_dir_all(job.voice_dir()).unwrap();
        std::fs::write(job.voice_dir().join("c01s01.wav"), b"x").unwrap();
        std::fs::write(job.shots_dir().join("c01s01_1.png"), b"x").unwrap();
        std::fs::write(job.shots_dir().join("c01s01_2.png"), b"x").unwrap();
        // A colour card standing in for a shot is not a shot made.
        job.remember_stand_ins(&["c01s01_2".to_owned()]).unwrap();
        let status = job.status(&board, true);
        assert_eq!(
            status,
            Status {
                voices: 1,
                voices_made: 1,
                shots: 3,
                shots_made: 1,
            }
        );
        assert_eq!(job.status(&board, false).voices, 0);
        assert_eq!(
            job.picture("c01s01_1"),
            Some(job.shots_dir().join("c01s01_1.png"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_script_written_again_keeps_only_what_it_did_not_change() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-retire-{}", std::process::id()));
        let job = Job::new(&root, "retire");
        let board = |second: &str| -> Storyboard {
            let mut board: Storyboard = serde_json::from_str(&format!(
                r#"{{"title":"T","chapters":[{{"title":"A","scenes":[
                    {{"narration":"same words","seconds":6,"shots":[{{"prompt":"a"}},{{"prompt":"{second}"}}]}}]}}]}}"#
            ))
            .unwrap();
            board.tidy().unwrap();
            board
        };
        std::fs::create_dir_all(job.shots_dir()).unwrap();
        std::fs::create_dir_all(job.voice_dir()).unwrap();
        std::fs::write(job.voice_dir().join("c01s01.wav"), b"x").unwrap();
        for stem in ["c01s01_1", "c01s01_2"] {
            std::fs::write(job.shots_dir().join(format!("{stem}.png")), b"x").unwrap();
        }
        job.retire(&board("b"), &board("something else"));
        assert!(job.voice_dir().join("c01s01.wav").exists());
        assert!(job.shots_dir().join("c01s01_1.png").exists());
        assert!(!job.shots_dir().join("c01s01_2.png").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_shot_made_again_is_a_new_take_beside_the_old() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-remake-{}", std::process::id()));
        let job = Job::new(&root, "remake");
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","chapters":[{"title":"A","scenes":[
                {"narration":"n","seconds":6,"shots":[{"prompt":"a"},{"prompt":"b"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        job.keep(&board).unwrap();
        std::fs::create_dir_all(job.shots_dir()).unwrap();
        let first = job.shots_dir().join("c01s01_2.png");
        std::fs::write(&first, b"first").unwrap();

        let costs = Costs::default();
        let second = job
            .remake(&board, "c01s01_2", &Shots::Draft, &costs)
            .unwrap();
        assert_eq!(second.path, job.takes_dir().join("c01s01_2-2.png"));
        assert_eq!(second.previous, Some(first.clone()));
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        let third = job
            .remake(&board, "c01s01_2", &Shots::Draft, &costs)
            .unwrap();
        assert_eq!(third.path, job.takes_dir().join("c01s01_2-3.png"));
        assert_eq!(third.previous, Some(second.path.clone()));
        assert_eq!(job.current("c01s01_2"), Some(third.path.clone()));
        assert_eq!(job.picture("c01s01_2"), Some(third.path.clone()));
        // The other shot is not touched, and the file of a take says
        // which shot and which job it is.
        assert_eq!(job.current("c01s01_1"), None);
        assert_eq!(job.stem_of(&third.path).as_deref(), Some("c01s01_2"));
        assert_eq!(job.stem_of(&first).as_deref(), Some("c01s01_2"));
        assert_eq!(Job::of_shot(&third.path), Some(job.clone()));
        assert_eq!(Job::of_shot(&root.join("elsewhere.png")), None);
        assert!(
            job.remake(&board, "c09s09_1", &Shots::Draft, &costs)
                .is_err()
        );
        // Words changed: the takes go with the shot.
        job.forget(&[], &["c01s01_2".to_owned()]);
        assert_eq!(job.current("c01s01_2"), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
