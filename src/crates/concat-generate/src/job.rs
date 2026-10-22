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
use crate::storyboard::{ShotKind, Storyboard, shot_stem};

/// One video's folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// `<project>/generate/<name>`.
    pub dir: PathBuf,
}

/// Where the making is, for the sheet's bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Scene `n` (from 1) of `of` being read aloud.
    Voice {
        /// This one.
        n: usize,
        /// Of how many.
        of: usize,
    },
    /// Shot `n` (from 1) of `of` being made.
    Shot {
        /// This one.
        n: usize,
        /// Of how many.
        of: usize,
    },
    /// The files being probed.
    Probing,
}

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
            }
        }
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
                let seconds = if shot.seconds > 0.0 {
                    shot.seconds
                } else {
                    scene.seconds / scene.shots.len() as f64
                }
                .clamp(2.0, 10.0);
                match shots {
                    Shots::Folder => missing.push(stem),
                    Shots::Draft => {
                        progress(Step::Shot { n, of: total });
                        providers::draft(&shots_dir.join(format!("{stem}.png")), width, height, n)?;
                    }
                    Shots::Replicate {
                        token,
                        image_model,
                        video_model,
                        video_input,
                    } => {
                        progress(Step::Shot { n, of: total });
                        let prompt = board.prompt(chapter, shot);
                        // A shot written as a clip moves, when there is a
                        // video model; a clip that cannot be made becomes a
                        // still - the one it was to start from, when there
                        // is one - rather than stopping the run.
                        let mut made = false;
                        if shot.kind == ShotKind::Video && !video_model.trim().is_empty() {
                            let inputs = inputs.get_or_insert_with(|| {
                                providers::replicate_inputs(token, video_model)
                                    .inspect_err(|error| {
                                        log::warn!("{video_model}'s inputs: {error}");
                                    })
                                    .ok()
                            });
                            let clip = Clip {
                                token,
                                image_model,
                                video_model,
                                video_input,
                                prompt: &prompt,
                                video_prompt: &board.video_prompt(chapter, shot),
                                seconds,
                                aspect: &board.aspect,
                                stem: &stem,
                            };
                            match self.clip(&clip, inputs.as_ref(), prices, &mut spend) {
                                Ok(()) => made = true,
                                Err(error) if error.starts_with(crate::cost::LIMIT_REACHED) => {
                                    return Err(error);
                                }
                                Err(error) => {
                                    let frame = providers::existing(
                                        &self.frames_dir(),
                                        &stem,
                                        &SHOT_EXTENSIONS,
                                    );
                                    if let Some(frame) = frame {
                                        let extension = frame
                                            .extension()
                                            .and_then(|e| e.to_str())
                                            .unwrap_or("png");
                                        let out = shots_dir.join(format!("{stem}.{extension}"));
                                        std::fs::copy(&frame, &out).map_err(|error| {
                                            format!("{}: {error}", out.display())
                                        })?;
                                        made = true;
                                    }
                                    stand_ins.push(format!(
                                        "{stem}: a still in place of a clip ({error})"
                                    ));
                                }
                            }
                        }
                        // And a still that cannot be made becomes a colour
                        // card: the cut is whole, and the card says which
                        // shot to make again.
                        if !made {
                            spend.take(prices.still)?;
                            let image = providers::retry(|| {
                                providers::replicate_image(
                                    token,
                                    image_model,
                                    &prompt,
                                    &board.aspect,
                                    &shots_dir,
                                    &stem,
                                )
                            });
                            if let Err(error) = image {
                                spend.refund(prices.still);
                                if providers::refused_for_good(&error)
                                    || error.contains("needs an API token")
                                {
                                    // A key that is wrong or out of credit fails
                                    // every shot the same way: say so now.
                                    return Err(error);
                                }
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
                            }
                        }
                    }
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

        self.remember_stand_ins(&stood_in)?;
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
                let path = providers::existing(&shots_dir, &stem, &SHOT_EXTENSIONS)
                    .ok_or_else(|| format!("shot {stem} is missing"))?;
                scene_files.shots.push(probe(&path)?);
            }
            files.insert(scene.id.clone(), scene_files);
        }
        made.files = files;
        Ok(made)
    }
}

/// One clip to make: what [`Job::clip`] is handed.
struct Clip<'a> {
    token: &'a str,
    image_model: &'a str,
    video_model: &'a str,
    video_input: &'a str,
    /// The shot's still prompt, for the picture it starts from.
    prompt: &'a str,
    /// The shot's picture and how it moves.
    video_prompt: &'a str,
    seconds: f64,
    aspect: &'a str,
    stem: &'a str,
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
        let image = match inputs.and_then(ModelInputs::image_field) {
            Some(_) => {
                let frames = self.frames_dir();
                std::fs::create_dir_all(&frames)
                    .map_err(|error| format!("{}: {error}", frames.display()))?;
                let frame = match providers::existing(&frames, clip.stem, &SHOT_EXTENSIONS) {
                    Some(frame) => frame,
                    None => {
                        spend.take(prices.still)?;
                        providers::retry(|| {
                            providers::replicate_image(
                                clip.token,
                                clip.image_model,
                                clip.prompt,
                                clip.aspect,
                                &frames,
                                clip.stem,
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
                clip.video_prompt,
                clip.seconds,
                clip.aspect,
                image.as_deref(),
            ),
            None => providers::templated_clip(
                clip.video_input,
                clip.video_prompt,
                clip.seconds,
                clip.aspect,
            )?,
        };
        spend.take(prices.clip)?;
        providers::retry(|| {
            providers::replicate_clip(
                clip.token,
                clip.video_model,
                input.clone(),
                &self.shots_dir(),
                clip.stem,
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
}
