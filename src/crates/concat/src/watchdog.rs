// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Says in the log when the event loop stops answering, and what it was
//! doing: a phone offers to close an app whose window is stuck for five
//! seconds, and a log that only shows the minute before is no help in
//! finding why.
//!
//! The event loop beats on a timer; a thread of its own looks at the beat
//! and, when it has stopped for [`STUCK`], writes the last message the
//! studio was handling, then how long the stall lasted once it ends.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How often the event loop beats.
const BEAT: Duration = Duration::from_millis(250);
/// How long without a beat is a stall worth a line.
const STUCK: Duration = Duration::from_secs(2);

/// When the event loop last beat, in milliseconds from `START`.
static LAST_BEAT: AtomicU64 = AtomicU64::new(0);
/// What the studio was last handling, in a few words.
static DOING: Mutex<String> = Mutex::new(String::new());
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn now() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Notes what the studio is about to handle, at most a line of it.
pub fn doing(what: &dyn std::fmt::Debug) {
    if let Ok(mut doing) = DOING.lock() {
        doing.clear();
        let _ = std::fmt::write(
            &mut Capped {
                out: &mut doing,
                left: 100,
            },
            format_args!("{what:?}"),
        );
    }
}

/// Starts the beat and the thread that watches it. Call once, on the event
/// loop, with the timer kept for as long as the window runs.
pub fn start() -> slint::Timer {
    LAST_BEAT.store(now(), Ordering::Relaxed);
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, BEAT, || {
        LAST_BEAT.store(now(), Ordering::Relaxed);
    });
    let _ = std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(|| {
            let mut stalled: Option<u64> = None;
            loop {
                std::thread::sleep(BEAT);
                let beat = LAST_BEAT.load(Ordering::Relaxed);
                let quiet = now().saturating_sub(beat);
                match stalled {
                    None if quiet >= STUCK.as_millis() as u64 => {
                        let doing = DOING.lock().map(|d| d.clone()).unwrap_or_default();
                        log::warn!(
                            "the window has not answered for {:.1}s; last handling: {doing}",
                            quiet as f64 / 1000.0
                        );
                        stalled = Some(beat);
                    }
                    Some(from) if beat != from => {
                        // With the memory: a phone short of it stalls
                        // every thread alike, and the stall is all it says.
                        log::warn!(
                            "the window answers again after {:.1}s{}",
                            beat.saturating_sub(from) as f64 / 1000.0,
                            concat_export::memory_said()
                        );
                        stalled = None;
                    }
                    _ => {}
                }
            }
        });
    timer
}

/// A writer that keeps the first `left` characters and then stops the
/// formatting, so a message carrying a whole script costs no more than a
/// line.
struct Capped<'a> {
    out: &'a mut String,
    left: usize,
}

impl std::fmt::Write for Capped<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        for c in text.chars() {
            if self.left == 0 {
                self.out.push('…');
                return Err(std::fmt::Error);
            }
            self.out.push(c);
            self.left -= 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// A long message is cut to its first characters.
    #[test]
    fn what_is_being_done_is_kept_to_a_line() {
        super::doing(&"x".repeat(1000));
        let doing = super::DOING.lock().unwrap().clone();
        assert_eq!(doing.chars().count(), 101);
        assert!(doing.ends_with('…'));
    }
}
