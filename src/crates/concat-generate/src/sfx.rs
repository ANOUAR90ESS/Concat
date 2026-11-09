// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The cut's own sounds, made here from nothing: a whoosh into a new
//! scene, a pop under a sticker, a chime under a figure. A few hundred
//! milliseconds each, computed sample by sample and written as WAV - no
//! library of recordings to ship or to license, and the same on every
//! phone and desk.

use std::f64::consts::TAU;
use std::path::{Path, PathBuf};

/// Samples a second.
const RATE: u32 = 44_100;

/// One of the cut's sounds: its file and how long it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The WAV file.
    pub path: String,
    /// Seconds.
    pub seconds: f64,
}

/// The sounds a cut is laid with.
#[derive(Clone, Debug, PartialEq)]
pub struct Sounds {
    /// Air rushing by: into a new scene.
    pub whoosh: Sound,
    /// A short blip dropping in pitch: under a sticker.
    pub pop: Sound,
    /// A small bell: under a figure or a card.
    pub chime: Sound,
}

impl Sounds {
    /// Each sound, for probing and for telling one from another.
    pub fn all(&self) -> [&Sound; 3] {
        [&self.whoosh, &self.pop, &self.chime]
    }

    /// How loud the sound at `path` is played: under the voice, never
    /// over it.
    pub fn gain(&self, path: &str) -> f64 {
        if path == self.whoosh.path {
            0.32
        } else if path == self.pop.path {
            0.5
        } else {
            0.38
        }
    }
}

/// The sounds, written into `dir` where they are not there already.
pub fn sounds(dir: &Path) -> Result<Sounds, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let make = |name: &str, samples: Vec<f64>| -> Result<Sound, String> {
        let path = dir.join(format!("{name}.wav"));
        let seconds = samples.len() as f64 / f64::from(RATE);
        if !path.metadata().is_ok_and(|meta| meta.len() > 44) {
            write_wav(&path, &samples)?;
        }
        Ok(Sound {
            path: path.to_string_lossy().into_owned(),
            seconds,
        })
    };
    Ok(Sounds {
        whoosh: make("whoosh", level(whoosh()))?,
        pop: make("pop", level(pop()))?,
        chime: make("chime", level(chime()))?,
    })
}

/// Where the sounds of a job in `dir` are kept.
pub fn dir_in(dir: &Path) -> PathBuf {
    dir.join("sounds")
}

/// `samples` brought to one peak, well inside what a file holds: each
/// sound as loud as the next, and none clipped.
fn level(mut samples: Vec<f64>) -> Vec<f64> {
    let peak = samples.iter().fold(0.0_f64, |peak, s| peak.max(s.abs()));
    if peak > 0.0 {
        for sample in &mut samples {
            *sample *= 0.8 / peak;
        }
    }
    samples
}

fn count(seconds: f64) -> usize {
    (seconds * f64::from(RATE)).round() as usize
}

/// Air rushing by: noise through a band that sweeps up and back, swelling
/// and dying away.
fn whoosh() -> Vec<f64> {
    let n = count(0.5);
    // A small generator of its own: the same noise on every machine.
    let mut seed: u32 = 0x9E37_79B9;
    let mut noise = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(seed >> 8) / f64::from(1u32 << 23) - 1.0
    };
    // A state-variable filter, its band moving with the swell.
    let (mut low, mut band) = (0.0_f64, 0.0_f64);
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let swell = (t * std::f64::consts::PI).sin().powi(2);
            let cutoff = 350.0 + 4200.0 * swell;
            let f = 2.0 * (std::f64::consts::PI * cutoff / f64::from(RATE)).sin();
            let high = noise() - low - 0.6 * band;
            band += f * high;
            low += f * band;
            band * swell * 0.9
        })
        .collect()
}

/// A blip falling from a high note to a low one in a tenth of a second.
fn pop() -> Vec<f64> {
    let n = count(0.16);
    let mut phase = 0.0_f64;
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let pitch = 660.0 * (170.0_f64 / 660.0).powf(t);
            phase += TAU * pitch / f64::from(RATE);
            let attack = (t * 60.0).min(1.0);
            phase.sin() * attack * (-5.0 * t).exp() * 0.9
        })
        .collect()
}

/// A small bell: a note and two partials over it, struck and left to ring.
fn chime() -> Vec<f64> {
    let n = count(0.8);
    (0..n)
        .map(|i| {
            let s = i as f64 / f64::from(RATE);
            let attack = (s * 400.0).min(1.0);
            let ring = |pitch: f64, level: f64, decay: f64| {
                (TAU * pitch * s).sin() * level * (-decay * s).exp()
            };
            (ring(1318.5, 0.55, 5.0) + ring(2637.0, 0.25, 8.0) + ring(3955.5, 0.12, 12.0))
                * attack
                * 0.9
        })
        .collect()
}

/// `samples`, each within -1..=1, as a 16-bit mono WAV at `out`, faded
/// over its last few milliseconds so it never ends on a click.
fn write_wav(out: &Path, samples: &[f64]) -> Result<(), String> {
    let tail = count(0.01).min(samples.len());
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for (i, sample) in samples.iter().enumerate() {
        let left = samples.len() - i;
        let fade = if left <= tail {
            (left - 1) as f64 / tail as f64
        } else {
            1.0
        };
        let value = (sample * fade).clamp(-1.0, 1.0) * f64::from(i16::MAX);
        bytes.extend_from_slice(&(value.round() as i16).to_le_bytes());
    }
    let partial = out.with_extension("partial");
    std::fs::write(&partial, bytes).map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, out).map_err(|error| format!("{}: {error}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each sound is short, is heard, never clips, and ends in silence;
    /// made twice, it is the same file.
    #[test]
    fn the_sounds_are_short_heard_and_whole() {
        for (name, samples) in [
            ("whoosh", level(whoosh())),
            ("pop", level(pop())),
            ("chime", level(chime())),
        ] {
            let seconds = samples.len() as f64 / f64::from(RATE);
            assert!((0.1..=1.0).contains(&seconds), "{name}: {seconds}");
            let peak = samples.iter().fold(0.0_f64, |peak, s| peak.max(s.abs()));
            assert!((peak - 0.8).abs() < 1e-9, "{name}: peak {peak}");
            let loud = (samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64).sqrt();
            assert!(loud > 0.03, "{name}: rms {loud}");
        }
        let dir = std::env::temp_dir().join(format!("concat-generate-sfx-{}", std::process::id()));
        let made = sounds(&dir).unwrap();
        for sound in made.all() {
            let bytes = std::fs::read(&sound.path).unwrap();
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(bytes.len(), 44 + count(sound.seconds) * 2);
            // The last sample is silence.
            assert_eq!(&bytes[bytes.len() - 2..], &[0, 0]);
        }
        let first = std::fs::read(&made.pop.path).unwrap();
        assert_eq!(sounds(&dir).unwrap(), made);
        assert_eq!(std::fs::read(&made.pop.path).unwrap(), first);
        assert!(made.gain(&made.pop.path) > made.gain(&made.whoosh.path));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
