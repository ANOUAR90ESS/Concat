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

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use concat_project::commands::NewMedia;
use concat_project::model::MediaKind;

use crate::cost::{Prices, Spend};
use crate::plan::{SceneFiles, Word};
use crate::providers::{self, ModelInputs, SHOT_EXTENSIONS, Shots, VOICE_EXTENSIONS, Voice};
use crate::storyboard::{Chapter, Shot, ShotKind, Storyboard, shot_stem};
use crate::writer::Publishing;

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
    /// The music bed being made.
    Music,
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

/// What a music bed may be saved as.
const MUSIC_EXTENSIONS: [&str; 5] = ["mp3", "wav", "m4a", "flac", "ogg"];

/// How long a music bed is made: the most MusicGen plays at once. The cut
/// loops it under a longer video.
const MUSIC_SECONDS: u32 = 30;

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
    /// The music bed made for the video, and its length, when there is one.
    pub music: Option<(String, f64)>,
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
        let stale = self.stale();
        let stood_in: Vec<String> = std::fs::read_to_string(self.stand_ins())
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_owned())
            .collect();
        for scene in board.scenes() {
            for index in 0..scene.shots.len() {
                let stem = shot_stem(&scene.id, index);
                status.shots += 1;
                if !stood_in.contains(&stem)
                    && !stale.contains(&stem)
                    && self.current(&stem).is_some()
                {
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

    /// Marks the shots the last run stood in for as stale, so this run
    /// makes them again as takes. Their colour cards are kept: a cut
    /// already on the timeline uses them, and an export of it failed with
    /// the file gone when they were removed here.
    fn forget_stand_ins(&self) -> Result<(), String> {
        let Ok(list) = std::fs::read_to_string(self.stand_ins()) else {
            return Ok(());
        };
        let mut stale = self.stale();
        stale.extend(
            list.lines()
                .map(str::trim)
                .filter(|stem| !stem.is_empty())
                .map(str::to_owned),
        );
        self.set_stale(&stale)?;
        let _ = std::fs::remove_file(self.stand_ins());
        Ok(())
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

    /// When each word of scene `id`'s narration is said, beside it.
    pub fn words_file(&self, id: &str) -> PathBuf {
        self.voice_dir().join(format!("{id}.words.json"))
    }

    /// Writes when each word of scene `id`'s narration is said.
    fn keep_words(&self, id: &str, words: &[Word]) -> Result<(), String> {
        let out = self.words_file(id);
        let text = serde_json::to_string(words).map_err(|error| error.to_string())?;
        std::fs::write(&out, text).map_err(|error| format!("{}: {error}", out.display()))
    }

    /// When each word of scene `id`'s narration is said, if that is known.
    pub fn words(&self, id: &str) -> Vec<Word> {
        std::fs::read_to_string(self.words_file(id))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
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

    /// The shots whose words changed since they were made, one stem a
    /// line: made again, as new takes, on the next run.
    pub fn stale_list(&self) -> PathBuf {
        self.dir.join("stale.txt")
    }

    /// The stems in [`Job::stale_list`].
    fn stale(&self) -> BTreeSet<String> {
        std::fs::read_to_string(self.stale_list())
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|stem| !stem.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Writes `stems` as the stale list; none removes it.
    fn set_stale(&self, stems: &BTreeSet<String>) -> Result<(), String> {
        if stems.is_empty() {
            let _ = std::fs::remove_file(self.stale_list());
            return Ok(());
        }
        let text: String = stems.iter().map(|stem| format!("{stem}\n")).collect();
        std::fs::write(self.stale_list(), text)
            .map_err(|error| format!("{}: {error}", self.stale_list().display()))
    }

    /// Lets go of what was made for scenes and shots the script has changed
    /// since, so the next run makes them from the new words: the narration
    /// of each of `voices` (scene ids) is removed, and each of `shots`
    /// (shot stems) is marked to be made again - as a new take, leaving the
    /// picture already paid for, and on the timeline, where it is until
    /// then.
    pub fn forget(&self, voices: &[String], shots: &[String]) {
        for name in voices {
            let _ = std::fs::remove_file(self.words_file(name));
            while let Some(path) = providers::existing(&self.voice_dir(), name, &VOICE_EXTENSIONS) {
                if std::fs::remove_file(&path).is_err() {
                    break;
                }
            }
        }
        if shots.is_empty() {
            return;
        }
        let mut stale = self.stale();
        for stem in shots {
            // A shot asked for from the old words is not followed.
            let _ = std::fs::remove_file(providers::pending(&self.shots_dir(), stem));
            for (number, _) in self.takes(stem) {
                let name = format!("{stem}-{}", number + 1);
                let _ = std::fs::remove_file(providers::pending(&self.takes_dir(), &name));
            }
            let _ =
                std::fs::remove_file(providers::pending(&self.takes_dir(), &format!("{stem}-2")));
            if self.current(stem).is_some() {
                stale.insert(stem.clone());
            }
        }
        if let Err(error) = self.set_stale(&stale) {
            log::warn!("{error}");
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
        music: Option<&str>,
        speak: &mut dyn FnMut(&str) -> Result<PathBuf, String>,
        progress: &mut dyn FnMut(Step),
        stop: &dyn Fn() -> bool,
    ) -> Result<Made, String> {
        let voice_dir = self.voice_dir();
        let shots_dir = self.shots_dir();
        let spend = Spend::new(costs.limit);
        let prices = &costs.prices;
        for dir in [&voice_dir, &shots_dir] {
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        self.forget_stand_ins()?;

        // The narration first: it decides every scene's length.
        let spoken: Vec<_> = board
            .scenes()
            .filter(|scene| !scene.narration.trim().is_empty())
            .collect();
        if let Voice::ElevenLabs { key, voice_id } = voice {
            // A few lines at a time: each is a wait on the service, and
            // each is read in the light of its neighbours' words, not of
            // their sound, so none waits on another.
            let todo: Vec<usize> = (0..spoken.len())
                .filter(|&i| {
                    providers::existing(&voice_dir, &spoken[i].id, &VOICE_EXTENSIONS).is_none()
                })
                .collect();
            let around = |n: Option<usize>| {
                n.and_then(|n| spoken.get(n))
                    .map_or("", |scene| scene.narration.trim())
            };
            for lines in todo.chunks(VOICES_AT_ONCE) {
                if stop() {
                    return Err("cancelled".to_owned());
                }
                for &i in lines {
                    progress(Step::Voice {
                        n: i + 1,
                        of: spoken.len(),
                        text: spoken[i].narration.trim().to_owned(),
                    });
                }
                let read: Vec<(usize, Result<Vec<Word>, String>)> = std::thread::scope(|scope| {
                    let readers: Vec<_> = lines
                        .iter()
                        .map(|&i| {
                            let (spend, voice_dir, around, spoken) =
                                (&spend, &voice_dir, &around, &spoken);
                            scope.spawn(move || {
                                let scene = spoken[i];
                                let text = scene.narration.trim();
                                let cost =
                                    text.chars().count() as f64 / 1000.0 * prices.voice_per_1k;
                                let words = spend.take(cost).and_then(|()| {
                                    providers::retry(|| {
                                        providers::elevenlabs(
                                            key,
                                            voice_id,
                                            text,
                                            around(i.checked_sub(1)),
                                            around(Some(i + 1)),
                                            &voice_dir.join(format!("{}.mp3", scene.id)),
                                        )
                                    })
                                    .inspect_err(|_| spend.refund(cost))
                                });
                                (i, words)
                            })
                        })
                        .collect();
                    readers
                        .into_iter()
                        .map(|reader| {
                            reader.join().unwrap_or_else(|_| {
                                (usize::MAX, Err("a reader stopped".to_owned()))
                            })
                        })
                        .collect()
                });
                for (i, words) in read {
                    let words = words?;
                    self.keep_words(&spoken[i].id, &words)?;
                }
            }
        } else if *voice == Voice::BuiltIn {
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
                let made = speak(scene.narration.trim())?;
                let extension = made.extension().and_then(|e| e.to_str()).unwrap_or("wav");
                let out = voice_dir.join(format!("{}.{extension}", scene.id));
                std::fs::copy(&made, &out)
                    .map_err(|error| format!("{}: {error}", out.display()))?;
                // Untimed: the cut shares its captions out.
                let _ = std::fs::remove_file(self.words_file(&scene.id));
            }
        }

        let (width, height) = board.frame();
        let total = board.shot_count();
        let mut missing = Vec::new();
        let mut stand_ins = Vec::new();
        let mut stood_in = Vec::new();
        // What the video model takes, read once, when the first clip is due.
        let mut inputs: Option<Option<ModelInputs>> = None;
        let mut stale = self.stale();
        let mut n = 0;
        // The shots asked of Replicate, made a few at a time once the
        // rest are seen to: a minute of waiting a shot, one after the
        // other, was most of a long video's making.
        let mut queue: Vec<Queued> = Vec::new();
        for (chapter, scene) in board
            .chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().map(move |scene| (chapter, scene)))
        {
            for (index, shot) in scene.shots.iter().enumerate() {
                n += 1;
                let stem = shot_stem(&scene.id, index);
                let again = stale.contains(&stem) && !matches!(shots, Shots::Folder);
                if self.current(&stem).is_some() && !again {
                    continue;
                }
                if stop() {
                    return Err("cancelled".to_owned());
                }
                // Made before from words since changed: made again as a new
                // take, the old one kept - and kept in the cut, said, when
                // the new one cannot be made.
                if again {
                    progress(Step::Shot {
                        n,
                        of: total,
                        prompt: shot.prompt.clone(),
                    });
                    let take = self.take(
                        board,
                        chapter,
                        scene,
                        index,
                        shots,
                        prices,
                        &spend,
                        &mut inputs,
                    )?;
                    match take {
                        Ok(_) => {
                            stale.remove(&stem);
                            self.set_stale(&stale)?;
                        }
                        Err(error) => stand_ins.push(format!(
                            "{stem}: the picture made from the old words is kept ({error})"
                        )),
                    }
                    progress(Step::Made {
                        n,
                        of: total,
                        picture: self.picture(&stem),
                        spent: spend.spent(),
                    });
                    continue;
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
                        queue.push(Queued {
                            n,
                            chapter,
                            shot,
                            seconds,
                            stem,
                        });
                        continue;
                    }
                }
                if !matches!(shots, Shots::Folder) {
                    progress(Step::Made {
                        n,
                        of: total,
                        picture: self.picture(&stem),
                        spent: spend.spent(),
                    });
                }
            }
        }
        if !queue.is_empty() {
            let batch = Batch {
                board,
                shots,
                prices,
                spend: &spend,
                dir: &shots_dir,
                total,
                frame: (width, height),
            };
            self.make_at_once(
                &batch,
                &queue,
                &mut inputs,
                &mut stand_ins,
                &mut stood_in,
                progress,
                stop,
            )?;
        }
        if !missing.is_empty() {
            return Err(format!(
                "{} shots are still to be made by hand: see prompts.md, and save them in {}",
                missing.len(),
                shots_dir.display()
            ));
        }

        // The music last, once the pictures are paid for: a run cut short
        // has spent nothing on a bed for a video it did not finish.
        if let (Some(prompt), Shots::Replicate { token, .. }) = (music, shots)
            && self.music().is_none()
        {
            if stop() {
                return Err("cancelled".to_owned());
            }
            progress(Step::Music);
            spend.take(prices.music)?;
            if let Err(error) = providers::retry(|| {
                providers::replicate_music(token, prompt, MUSIC_SECONDS, &self.dir, "music")
            }) {
                spend.refund(prices.music);
                stand_ins.push(format!("the music could not be made ({error})"));
            }
        }

        progress(Step::Probing);
        let mut made = self.gather(board, *voice != Voice::None)?;
        made.stand_ins = stand_ins;
        made.spent = spend.spent();
        Ok(made)
    }

    /// The music bed made for this video, when there is one.
    pub fn music(&self) -> Option<PathBuf> {
        providers::existing(&self.dir, "music", &MUSIC_EXTENSIONS)
    }

    /// Where what goes up with the video on YouTube is kept.
    pub fn publishing_file(&self) -> PathBuf {
        self.dir.join("publishing.json")
    }

    /// What goes up with the video on YouTube, once written.
    pub fn publishing(&self) -> Option<Publishing> {
        let text = std::fs::read_to_string(self.publishing_file()).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Keeps what goes up with the video, and the same as text beside it:
    /// `publishing.txt`, to copy from on any device.
    pub fn keep_publishing(&self, publishing: &Publishing) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("{}: {error}", self.dir.display()))?;
        let json = serde_json::to_string_pretty(publishing).map_err(|error| error.to_string())?;
        let file = self.publishing_file();
        std::fs::write(&file, json).map_err(|error| format!("{}: {error}", file.display()))?;
        let text = self.dir.join("publishing.txt");
        std::fs::write(&text, publishing.sheet())
            .map_err(|error| format!("{}: {error}", text.display()))
    }

    /// The thumbnail's picture: one made for it and kept, else - with
    /// shots on Replicate - one made now from `prompt`, a still's price,
    /// else the first picture the video has.
    pub fn thumbnail_picture(
        &self,
        board: &Storyboard,
        shots: &Shots,
        prompt: &str,
    ) -> Result<PathBuf, String> {
        if let Some(kept) = providers::existing(&self.dir, "thumbnail", &STILL_EXTENSIONS) {
            return Ok(kept);
        }
        if let Shots::Replicate {
            token, image_model, ..
        } = shots
            && !prompt.trim().is_empty()
        {
            let prompt = format!("{}, {}", prompt.trim(), board.style.visual.trim());
            let made = providers::retry(|| {
                providers::replicate_image(
                    token,
                    image_model,
                    &prompt,
                    "16:9",
                    &self.dir,
                    "thumbnail",
                )
            })?;
            return Ok(providers::true_name(&made));
        }
        board
            .scenes()
            .flat_map(|scene| (0..scene.shots.len()).map(|n| shot_stem(&scene.id, n)))
            .find_map(|stem| self.picture(&stem))
            .ok_or_else(|| "the video has no picture for a thumbnail yet".to_owned())
    }

    /// Probes what the job holds for `board` - each scene's narration, when
    /// `voiced`, and the take each shot is now - for a cut: what a run ends
    /// with, and what a Short is cut from without making anything again.
    pub fn gather(&self, board: &Storyboard, voiced: bool) -> Result<Made, String> {
        let voice_dir = self.voice_dir();
        let mut made = Made::default();
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
            if voiced
                && let Some(path) = providers::existing(&voice_dir, &scene.id, &VOICE_EXTENSIONS)
            {
                let (path, length) = probe(&path)?;
                scene_files.words = self.words(&scene.id);
                if scene_files.words.is_empty() {
                    scene_files.sounding = sounding(&path);
                }
                scene_files.voice = Some((path, length.unwrap_or(0.0)));
            }
            for index in 0..scene.shots.len() {
                let stem = shot_stem(&scene.id, index);
                let path = self
                    .current(&stem)
                    .ok_or_else(|| format!("shot {stem} is missing"))?;
                // Named for what it holds before it is probed: a picture
                // an earlier build saved as `.mp4` went in as a clip.
                scene_files.shots.push(probe(&providers::true_name(&path))?);
            }
            files.insert(scene.id.clone(), scene_files);
        }
        made.files = files;
        if let Some(path) = self.music() {
            let (path, length) = probe(&path)?;
            made.music = Some((path, length.unwrap_or(0.0)));
        }
        Ok(made)
    }
}

/// Where in a narration's file there is sound, first to last, in seconds:
/// read from its waveform, a millisecond at a time. None when it cannot be
/// read, or is silent.
fn sounding(path: &str) -> Option<(f64, f64)> {
    let peaks = concat_host::media::peaks(path, None, None).ok()?;
    let loud = |(min, max): (&f32, &f32)| max.max(-min) > SOUNDING;
    let buckets: Vec<bool> = peaks.min.iter().zip(&peaks.max).map(loud).collect();
    let first = buckets.iter().position(|&b| b)?;
    let last = buckets.iter().rposition(|&b| b)?;
    let per = f64::from(peaks.buckets_per_second.max(1.0));
    Some((first as f64 / per, (last + 1) as f64 / per))
}

/// The level, of full scale, under which a narration is silent: -42 dB,
/// under a breath, over a quiet file's hiss.
const SOUNDING: f32 = 0.008;

/// How many narration lines are asked of ElevenLabs at once: under the
/// fewest requests its smaller plans run side by side.
const VOICES_AT_ONCE: usize = 2;

/// How many shots are asked of Replicate at once: four waits overlap
/// where one followed another, well inside what an account may run.
const AT_ONCE: usize = 4;

/// A shot waiting to be made on Replicate: its number in the run, and
/// where it is in the storyboard.
struct Queued<'a> {
    n: usize,
    chapter: &'a Chapter,
    shot: &'a Shot,
    seconds: f64,
    stem: String,
}

/// What every shot of a batch shares.
struct Batch<'a> {
    board: &'a Storyboard,
    shots: &'a Shots,
    prices: &'a Prices,
    spend: &'a Spend,
    dir: &'a Path,
    total: usize,
    frame: (u32, u32),
}

/// What a worker of [`Job::make_at_once`] says.
enum Said {
    /// It has started on the queue's shot `.0`.
    Started(usize),
    /// It is done with it.
    Done(usize, Result<Outcome, String>),
}

impl Job {
    /// Makes `queue`'s shots on Replicate, [`AT_ONCE`] at a time. Each
    /// takes its price from the run's purse before its call; what each
    /// came to is seen to here, on the caller's thread, in the order they
    /// finish. A failure that ends the run - the limit, a key refused -
    /// or `stop` lets the shots under way finish and starts no more.
    #[allow(clippy::too_many_arguments)]
    fn make_at_once(
        &self,
        batch: &Batch,
        queue: &[Queued],
        inputs: &mut Option<Option<ModelInputs>>,
        stand_ins: &mut Vec<String>,
        stood_in: &mut Vec<String>,
        progress: &mut dyn FnMut(Step),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        // What the video model takes, read once before the workers start.
        if let Shots::Replicate {
            token, video_model, ..
        } = batch.shots
            && inputs.is_none()
            && !video_model.trim().is_empty()
            && queue.iter().any(|q| q.shot.kind == ShotKind::Video)
        {
            *inputs = Some(
                providers::replicate_inputs(token, video_model)
                    .inspect_err(|error| log::warn!("{video_model}'s inputs: {error}"))
                    .ok(),
            );
        }
        let ready = inputs.clone();
        let next = AtomicUsize::new(0);
        let halt = AtomicBool::new(false);
        let (tell, heard) = std::sync::mpsc::channel::<Said>();
        std::thread::scope(|scope| {
            for _ in 0..AT_ONCE.min(queue.len()) {
                let tell = tell.clone();
                let (next, halt, ready) = (&next, &halt, &ready);
                scope.spawn(move || {
                    while !halt.load(Ordering::Acquire) {
                        let i = next.fetch_add(1, Ordering::AcqRel);
                        let Some(queued) = queue.get(i) else {
                            break;
                        };
                        let _ = tell.send(Said::Started(i));
                        let order = Order::new(
                            batch.board,
                            queued.chapter,
                            queued.shot,
                            queued.seconds,
                            batch.dir,
                            &queued.stem,
                        );
                        let mut inputs = ready.clone();
                        let outcome = self.replicate_shot(
                            batch.shots,
                            &order,
                            &mut inputs,
                            batch.prices,
                            batch.spend,
                        );
                        // A failure that ends the run starts no more
                        // shots from here, before it is even heard.
                        if outcome.is_err() {
                            halt.store(true, Ordering::Release);
                        }
                        let _ = tell.send(Said::Done(i, outcome));
                    }
                });
            }
            drop(tell);
            let mut failure: Option<String> = None;
            loop {
                let said = match heard.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(said) => said,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if failure.is_none() && stop() {
                            halt.store(true, Ordering::Release);
                            failure = Some("cancelled".to_owned());
                        }
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };
                match said {
                    Said::Started(i) => progress(Step::Shot {
                        n: queue[i].n,
                        of: batch.total,
                        prompt: queue[i].shot.prompt.clone(),
                    }),
                    Said::Done(i, outcome) => {
                        let queued = &queue[i];
                        let stem = &queued.stem;
                        match outcome {
                            Ok(Outcome::Made) => {}
                            Ok(Outcome::StillForClip(error)) => stand_ins
                                .push(format!("{stem}: a still in place of a clip ({error})")),
                            // A still that cannot be made becomes a colour
                            // card: the cut is whole, and the card says
                            // which shot to make again.
                            Ok(Outcome::Failed(error)) => {
                                stand_ins.push(format!(
                                    "{stem}: a colour card in place of a still ({error})"
                                ));
                                stood_in.push(stem.clone());
                                let (width, height) = batch.frame;
                                // Noted as it happens: a run that ends
                                // early still makes this shot again next
                                // time.
                                let noted = providers::draft(
                                    &batch.dir.join(format!("{stem}.png")),
                                    width,
                                    height,
                                    queued.n,
                                )
                                .and_then(|()| self.remember_stand_ins(stood_in));
                                if let Err(error) = noted {
                                    halt.store(true, Ordering::Release);
                                    failure.get_or_insert(error);
                                }
                            }
                            Err(error) => {
                                halt.store(true, Ordering::Release);
                                failure.get_or_insert(error);
                                continue;
                            }
                        }
                        progress(Step::Made {
                            n: queued.n,
                            of: batch.total,
                            picture: self.picture(stem),
                            spent: batch.spend.spent(),
                        });
                    }
                }
            }
            failure.map_or(Ok(()), Err)
        })
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
        let previous = self.current(stem);
        let spend = Spend::new(costs.limit);
        let path = self.take(
            board,
            chapter,
            scene,
            index,
            shots,
            &costs.prices,
            &spend,
            &mut None,
        )??;
        let mut stale = self.stale();
        if stale.remove(stem) {
            self.set_stale(&stale)?;
        }
        Ok(Remade {
            path,
            previous,
            spent: spend.spent(),
        })
    }

    /// Makes shot `index` of `scene` as a new take beside the ones before.
    /// Fails outright for what fails every shot alike - see
    /// [`Job::replicate_shot`]; a take that could not be made as asked is
    /// not kept, and the inner error says why.
    #[allow(clippy::too_many_arguments)]
    fn take(
        &self,
        board: &Storyboard,
        chapter: &Chapter,
        scene: &crate::storyboard::Scene,
        index: usize,
        shots: &Shots,
        prices: &Prices,
        spend: &Spend,
        inputs: &mut Option<Option<ModelInputs>>,
    ) -> Result<Result<PathBuf, String>, String> {
        let stem = shot_stem(&scene.id, index);
        let shot = &scene.shots[index];
        let number = self
            .takes(&stem)
            .last()
            .map(|(number, _)| number + 1)
            .unwrap_or(2);
        let take = format!("{stem}-{number}");
        let dir = self.takes_dir();
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        match shots {
            Shots::Folder => {
                return Ok(Err(format!(
                    "a shot made by hand is made again by hand: save it in {}",
                    self.shots_dir().display()
                )));
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
                let refused = match self.replicate_shot(shots, &order, inputs, prices, spend)? {
                    Outcome::Made => None,
                    Outcome::StillForClip(error) | Outcome::Failed(error) => Some(error),
                };
                if let Some(error) = refused {
                    while let Some(path) = providers::existing(&dir, &take, &SHOT_EXTENSIONS) {
                        if std::fs::remove_file(&path).is_err() {
                            break;
                        }
                    }
                    return Ok(Err(error));
                }
            }
        }
        Ok(providers::existing(&dir, &take, &SHOT_EXTENSIONS)
            .ok_or_else(|| format!("{take} was not saved")))
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
        spend: &Spend,
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
        spend: &Spend,
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
    fn the_music_bed_is_found_in_the_jobs_folder() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-music-{}", std::process::id()));
        let job = Job::new(&root, "music");
        std::fs::create_dir_all(&job.dir).unwrap();
        assert_eq!(job.music(), None);
        std::fs::write(job.dir.join("music.mp3"), b"ID3").unwrap();
        assert_eq!(job.music(), Some(job.dir.join("music.mp3")));
        std::fs::remove_dir_all(root).unwrap();
    }

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
        job.forget_stand_ins().unwrap();
        // The card is kept for the cut that uses it, and made again as a
        // take.
        assert!(job.shots_dir().join("c01s01_1.png").exists());
        assert!(job.shots_dir().join("c01s01_2.png").exists());
        assert_eq!(job.stale(), BTreeSet::from(["c01s01_2".to_owned()]));
        assert!(!job.stand_ins().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_edited_shot_is_kept_and_marked_to_be_made_again() {
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
        // Paid for, and maybe on the timeline: nothing is removed, the shot
        // is only marked, and the next run makes it again as a take.
        for (dir, name) in [
            (job.shots_dir(), "c01s01_1.mp4"),
            (job.frames_dir(), "c01s01_1.png"),
            (job.shots_dir(), "c01s01_2.png"),
        ] {
            assert!(dir.join(name).exists(), "{name}");
        }
        assert_eq!(job.stale(), BTreeSet::from(["c01s01_1".to_owned()]));
        // A shot never made is not marked: the next run makes it anyway.
        job.forget(&[], &["c09s09_1".to_owned()]);
        assert_eq!(job.stale().len(), 1);
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
        assert!(job.shots_dir().join("c01s01_2.png").exists());
        assert_eq!(job.stale(), BTreeSet::from(["c01s01_2".to_owned()]));
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
        // Words changed: the shot keeps its takes, counts as not made, and
        // a take made from the new words clears the mark.
        job.forget(&[], &["c01s01_2".to_owned()]);
        assert_eq!(job.current("c01s01_2"), Some(third.path.clone()));
        assert_eq!(job.status(&board, false).shots_made, 0);
        let fourth = job
            .remake(&board, "c01s01_2", &Shots::Draft, &costs)
            .unwrap();
        assert_eq!(fourth.path, job.takes_dir().join("c01s01_2-4.png"));
        assert!(job.stale().is_empty());
        assert_eq!(job.status(&board, false).shots_made, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_narrations_silence_is_read_from_its_waveform() {
        // Half a second of silence, a second of tone, 0.8 s of silence:
        // 16-bit mono at 8 kHz, written by hand.
        let rate = 8000u32;
        let mut samples = vec![0i16; 4000];
        samples.extend((0..8000).map(|n| {
            (8000.0 * (n as f64 * 440.0 * std::f64::consts::TAU / f64::from(rate)).sin()) as i16
        }));
        samples.extend(vec![0i16; 6400]);
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 2).to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(data);
        let path = std::env::temp_dir().join(format!("concat-sounding-{}.wav", std::process::id()));
        std::fs::write(&path, wav).unwrap();
        let (first, last) = sounding(&path.to_string_lossy()).expect("it sounds");
        assert!((first - 0.5).abs() < 0.01, "{first}");
        assert!((last - 1.5).abs() < 0.01, "{last}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn what_goes_up_on_youtube_is_kept_as_json_and_text() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-publish-{}", std::process::id()));
        let job = Job::new(&root, "publish");
        assert_eq!(job.publishing(), None);
        let publishing = Publishing {
            title: "Title".to_owned(),
            description: "About.".to_owned(),
            tags: vec!["a".to_owned()],
            ..Publishing::default()
        };
        job.keep_publishing(&publishing).unwrap();
        assert_eq!(job.publishing(), Some(publishing));
        let text = std::fs::read_to_string(job.dir.join("publishing.txt")).unwrap();
        assert_eq!(text, "Title\n\nAbout.\n\na\n");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_batch_stops_at_a_failure_that_ends_the_run() {
        let root =
            std::env::temp_dir().join(format!("concat-generate-batch-{}", std::process::id()));
        let job = Job::new(&root, "batch");
        let mut board: Storyboard = serde_json::from_str(
            r#"{"title":"T","chapters":[{"title":"A","scenes":[
                {"narration":"","seconds":6,"shots":[{"prompt":"a"},{"prompt":"b"},
                 {"prompt":"c"},{"prompt":"d"},{"prompt":"e"},{"prompt":"f"}]}]}]}"#,
        )
        .unwrap();
        board.tidy().unwrap();
        // No token: every shot is refused alike, and the run says so
        // rather than standing in a colour card for each.
        let shots = Shots::Replicate {
            token: String::new(),
            image_model: String::new(),
            video_model: String::new(),
            video_input: String::new(),
        };
        let mut started = 0;
        let error = job
            .make(
                &board,
                &Voice::None,
                &shots,
                &Costs::default(),
                None,
                &mut |_| Err("no voice".to_owned()),
                &mut |step| {
                    if matches!(step, Step::Shot { .. }) {
                        started += 1;
                    }
                },
                &|| false,
            )
            .unwrap_err();
        assert!(error.contains("API token"), "{error}");
        assert!((1..=AT_ONCE).contains(&started), "{started} started");
        assert!(job.current("c01s01_1").is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
