// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! What a video costs, before it is made and while it is.
//!
//! The services bill by the call: Claude by the token, Replicate by the
//! picture or the clip, ElevenLabs by the character. The budget already
//! says how many of each a length needs, so the bill can be worked out
//! before anything is paid for - and, with a limit set, a run stops at the
//! limit instead of past it.
//!
//! The prices are estimates: what the default models cost when written,
//! and the person can set their own for the models they choose.

use crate::budget::Totals;

/// What one of each thing costs, in US dollars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prices {
    /// One still from the image model.
    pub still: f64,
    /// One clip from the video model.
    pub clip: f64,
    /// A thousand characters read by ElevenLabs.
    pub voice_per_1k: f64,
    /// One music bed from MusicGen: half a minute of the A100's time.
    pub music: f64,
    /// A million tokens the writer reads.
    pub script_in_per_m: f64,
    /// A million tokens the writer writes.
    pub script_out_per_m: f64,
}

impl Default for Prices {
    /// The default models' prices: FLUX schnell's still, a short
    /// image-to-video clip at the cheap end, ElevenLabs' API rate, and the
    /// writer's model.
    fn default() -> Prices {
        Prices {
            still: 0.003,
            clip: 0.10,
            voice_per_1k: 0.30,
            music: 0.08,
            script_in_per_m: 2.0,
            script_out_per_m: 10.0,
        }
    }
}

/// About how many characters a word of narration is, its space included.
const CHARS_PER_WORD: f64 = 6.0;

/// A video's bill, item by item.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Estimate {
    /// Claude writing the script.
    pub script: f64,
    /// The stills, the key frames clips start from among them.
    pub stills: f64,
    /// The clips.
    pub clips: f64,
    /// The narration, when ElevenLabs reads it.
    pub voice: f64,
}

impl Estimate {
    /// The whole bill.
    pub fn total(&self) -> f64 {
        self.script + self.stills + self.clips + self.voice
    }
}

/// What a video of `totals` costs at `prices`. `clips` is whether clips
/// are made at all (a video model is set), `key_frames` whether each starts
/// from a still of its own, and `paid_voice` whether ElevenLabs reads.
pub fn estimate(
    totals: &Totals,
    prices: &Prices,
    clips: bool,
    key_frames: bool,
    paid_voice: bool,
) -> Estimate {
    // The writer reads its brief, the outline and the pictures already used
    // once a chapter, and writes the narration and every shot's prompt and
    // motion; a revision pass rewrites about a third again.
    let tokens_in = 2_000.0 + totals.chapters as f64 * 9_000.0;
    let shots = (totals.clips + totals.stills) as f64;
    let tokens_out = (totals.words as f64 * 1.6 + shots * 90.0) * 1.35;
    let (clip_count, still_count) = if clips {
        (
            totals.clips as f64,
            totals.stills as f64 + if key_frames { totals.clips as f64 } else { 0.0 },
        )
    } else {
        (0.0, shots)
    };
    Estimate {
        script: tokens_in / 1e6 * prices.script_in_per_m
            + tokens_out / 1e6 * prices.script_out_per_m,
        stills: still_count * prices.still,
        clips: clip_count * prices.clip,
        voice: if paid_voice {
            totals.words as f64 * CHARS_PER_WORD / 1000.0 * prices.voice_per_1k
        } else {
            0.0
        },
    }
}

/// How the error a run stops at its limit with begins, so the run tells
/// it from a shot that merely failed.
pub const LIMIT_REACHED: &str = "stopped at the spend limit";

/// What a run has paid for so far, held against a limit. Shared by the
/// shots a run makes at once: each takes its price before its call.
#[derive(Debug, Default)]
pub struct Spend {
    /// Spent this run, in dollars.
    spent: std::sync::Mutex<f64>,
    /// The most this run may spend; None for no limit.
    pub limit: Option<f64>,
}

impl Spend {
    /// A run that may spend up to `limit`.
    pub fn new(limit: Option<f64>) -> Spend {
        Spend {
            spent: std::sync::Mutex::new(0.0),
            limit,
        }
    }

    /// Spent this run, in dollars.
    pub fn spent(&self) -> f64 {
        *self
            .spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes `cost` from what is left, or says why not: the run stops
    /// before a call the limit does not cover, never after it.
    pub fn take(&self, cost: f64) -> Result<(), String> {
        let mut spent = self
            .spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(limit) = self.limit
            && *spent + cost > limit + 1e-9
        {
            return Err(format!(
                "{LIMIT_REACHED} of ${limit:.2} (${:.2} spent): what is made is kept; \
                 raise the limit and press Create video to carry on",
                *spent
            ));
        }
        *spent += cost;
        Ok(())
    }

    /// Gives back what a failed call was charged: a call that fails is
    /// mostly not billed, and a limit should not end a run for one.
    pub fn refund(&self, cost: f64) {
        let mut spent = self
            .spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *spent = (*spent - cost).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{Pacing, budget};

    #[test]
    fn ten_minutes_is_counted_item_by_item() {
        let totals = budget(&Pacing::new(10.0, 0.25)).totals();
        let prices = Prices::default();
        let bill = estimate(&totals, &prices, true, true, true);
        assert!((bill.clips - totals.clips as f64 * 0.10).abs() < 1e-9);
        // Every clip starts from a still of its own.
        let stills = (totals.stills + totals.clips) as f64 * 0.003;
        assert!((bill.stills - stills).abs() < 1e-9);
        assert!(bill.voice > 2.0 && bill.voice < 4.0, "{bill:?}");
        assert!(bill.script > 0.05 && bill.script < 1.0, "{bill:?}");
        assert!(
            (bill.total() - (bill.script + bill.stills + bill.clips + bill.voice)).abs() < 1e-9
        );
    }

    #[test]
    fn without_a_video_model_every_shot_is_a_still() {
        let totals = budget(&Pacing::new(5.0, 0.5)).totals();
        let bill = estimate(&totals, &Prices::default(), false, true, false);
        assert_eq!(bill.clips, 0.0);
        assert_eq!(bill.voice, 0.0);
        let shots = (totals.clips + totals.stills) as f64;
        assert!((bill.stills - shots * 0.003).abs() < 1e-9);
    }

    #[test]
    fn a_limit_stops_before_the_call_it_does_not_cover() {
        let spend = Spend::new(Some(0.25));
        assert!(spend.take(0.10).is_ok());
        assert!(spend.take(0.10).is_ok());
        let refused = spend.take(0.10).unwrap_err();
        assert!(refused.contains("$0.25"), "{refused}");
        assert!((spend.spent() - 0.20).abs() < 1e-9);
        spend.refund(0.10);
        assert!(spend.take(0.05).is_ok());
        let free = Spend::new(None);
        assert!(free.take(1e6).is_ok());
    }
}
