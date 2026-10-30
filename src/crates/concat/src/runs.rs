// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The Long video runs under way, wherever they were started.
//!
//! A run belongs to a job's folder, not to the project open in the window:
//! it goes on when the person leaves for the list of projects or opens
//! another, and its files are there when they come back. This is what
//! knows a run is still going - so the sheet, opened again on that job,
//! shows it rather than starting a second one that would pay for every
//! shot twice - and what the floating pill reads how far it is from.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One run under way.
#[derive(Clone, Debug)]
pub struct Run {
    /// The job's folder.
    pub dir: PathBuf,
    /// The video's title, or its idea while it has none.
    pub title: String,
    /// Asked between two files: the run stops there.
    pub stop: Arc<AtomicBool>,
    /// How far, `0..=1`.
    pub progress: f32,
    /// What it is doing, in words.
    pub stage: String,
}

static RUNS: Mutex<Vec<Run>> = Mutex::new(Vec::new());

fn runs() -> std::sync::MutexGuard<'static, Vec<Run>> {
    RUNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A run starts on the job in `dir`.
pub fn begin(dir: &Path, title: &str, stop: Arc<AtomicBool>) {
    let mut runs = runs();
    runs.retain(|run| run.dir != dir);
    runs.push(Run {
        dir: dir.to_owned(),
        title: title.to_owned(),
        stop,
        progress: 0.0,
        stage: String::new(),
    });
}

/// Where the run on `dir` is.
pub fn update(dir: &Path, progress: f32, stage: &str) {
    if let Some(run) = runs().iter_mut().find(|run| run.dir == dir) {
        run.progress = progress;
        run.stage = stage.to_owned();
    }
}

/// The run on `dir` is over, whichever way.
pub fn end(dir: &Path) {
    runs().retain(|run| run.dir != dir);
}

/// The run on `dir`, while there is one.
pub fn of(dir: &Path) -> Option<Run> {
    runs().iter().find(|run| run.dir == dir).cloned()
}

/// The run started last, while any is going: what the pill shows.
pub fn latest() -> Option<Run> {
    runs().last().cloned()
}

/// Asks the run on `dir` to stop between two files.
pub fn stop(dir: &Path) {
    if let Some(run) = runs().iter().find(|run| run.dir == dir) {
        run.stop.store(true, Ordering::Release);
    }
}

/// Ends the run on `dir` when dropped: held by the worker, so a run that
/// fails or panics is not left looking busy.
pub struct Held(pub PathBuf);

impl Drop for Held {
    fn drop(&mut self) {
        end(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_known_by_its_folder_until_it_ends() {
        let dir = PathBuf::from("/tmp/concat-runs-test/job");
        let stop = Arc::new(AtomicBool::new(false));
        begin(&dir, "Video", Arc::clone(&stop));
        update(&dir, 0.5, "Making shot 3 of 6");
        let run = of(&dir).expect("under way");
        assert_eq!(run.progress, 0.5);
        assert_eq!(run.stage, "Making shot 3 of 6");
        super::stop(&dir);
        assert!(stop.load(Ordering::Acquire));
        drop(Held(dir.clone()));
        assert!(of(&dir).is_none());
    }
}
