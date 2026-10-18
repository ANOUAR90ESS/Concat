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
//! ```
//!
//! Every step skips a file that is already there, so a job stopped half
//! way, or one whose files were replaced by hand, carries on from where it
//! is; and the folder sits in the project's, so it moves with it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use concat_project::commands::NewMedia;
use concat_project::model::MediaKind;

use crate::plan::SceneFiles;
use crate::providers::{self, SHOT_EXTENSIONS, Shots, VOICE_EXTENSIONS, Voice};
use crate::storyboard::{Storyboard, shot_stem};

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
    /// between files.
    pub fn make(
        &self,
        board: &Storyboard,
        voice: &Voice,
        shots: &Shots,
        speak: &mut dyn FnMut(&str) -> Result<PathBuf, String>,
        progress: &mut dyn FnMut(Step),
        stop: &dyn Fn() -> bool,
    ) -> Result<Made, String> {
        let voice_dir = self.voice_dir();
        let shots_dir = self.shots_dir();
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
                    Voice::ElevenLabs { key, voice_id } => providers::retry(|| {
                        providers::elevenlabs(
                            key,
                            voice_id,
                            scene.narration.trim(),
                            &voice_dir.join(format!("{}.mp3", scene.id)),
                        )
                    })?,
                }
            }
        }

        let (width, height) = board.frame();
        let total = board.shot_count();
        let mut missing = Vec::new();
        let mut stand_ins = Vec::new();
        let mut stood_in = Vec::new();
        let mut n = 0;
        for scene in board.scenes() {
            for (index, shot) in scene.shots.iter().enumerate() {
                n += 1;
                let stem = shot_stem(&scene.id, index);
                if providers::existing(&shots_dir, &stem, &SHOT_EXTENSIONS).is_some() {
                    continue;
                }
                if stop() {
                    return Err("cancelled".to_owned());
                }
                let seconds = (scene.seconds / scene.shots.len() as f64).clamp(2.0, 10.0);
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
                        let prompt = board.prompt(shot);
                        // The scene's opening shot moves, when there is a
                        // video model; a clip that cannot be made becomes a
                        // still rather than stopping the run.
                        let mut made = false;
                        if index == 0 && !video_model.trim().is_empty() {
                            match providers::retry(|| {
                                providers::replicate_video(
                                    token,
                                    video_model,
                                    video_input,
                                    &prompt,
                                    seconds,
                                    &board.aspect,
                                    &shots_dir,
                                    &stem,
                                )
                            }) {
                                Ok(_) => made = true,
                                Err(error) => stand_ins
                                    .push(format!("{stem}: a still in place of a clip ({error})")),
                            }
                        }
                        // And a still that cannot be made becomes a colour
                        // card: the cut is whole, and the card says which
                        // shot to make again.
                        if !made {
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
    fn a_job_folder_is_named_after_the_video() {
        let job = Job::new(Path::new("/p"), "المدن المفقودة: الجزء 1!");
        assert_eq!(job.dir, Path::new("/p/generate/المدن-المفقودة-الجزء-1"));
        assert_eq!(
            Job::new(Path::new("/p"), "  ").dir,
            Path::new("/p/generate/video")
        );
    }
}
