// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Where the shots and the narration come from.
//!
//! Every provider writes one file to a path it is given and nothing else,
//! so any of them can be swapped for another, or for a file made by hand,
//! without the cut noticing. A file already there is never made twice:
//! generation is resumable, and a shot replaced by hand stays.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::writer::message_of;

/// What a shot file may be.
pub const SHOT_EXTENSIONS: [&str; 8] = ["mp4", "mov", "webm", "mkv", "png", "jpg", "jpeg", "webp"];
/// What a narration file may be.
pub const VOICE_EXTENSIONS: [&str; 5] = ["wav", "mp3", "m4a", "flac", "ogg"];

/// Where shots come from.
#[derive(Clone, Debug, PartialEq)]
pub enum Shots {
    /// Colour cards: a draft cut, to judge the pacing before paying for a
    /// single shot. The cut moves over them slowly, as it does any still.
    Draft,
    /// Made by hand from the prompt sheet and saved in the job's `shots/`.
    Folder,
    /// Any text-to-video model on Replicate.
    Replicate {
        /// The account's API token.
        token: String,
        /// "owner/name".
        model: String,
        /// The model's input as JSON; strings may hold `{prompt}`,
        /// `{seconds}` and `{aspect}`.
        input: String,
    },
}

/// Where the narration comes from, besides the window's own voices.
#[derive(Clone, Debug, PartialEq)]
pub enum Voice {
    /// No narration: each scene lasts its written seconds.
    None,
    /// Read by the window's own text to speech; the caller speaks.
    BuiltIn,
    /// ElevenLabs, which reads most languages, Arabic among them.
    ElevenLabs {
        /// The account's API key.
        key: String,
        /// The voice to read with.
        voice_id: String,
    },
}

/// The file `stem` already has in `dir` under one of `extensions`.
pub fn existing(dir: &Path, stem: &str, extensions: &[&str]) -> Option<PathBuf> {
    extensions
        .iter()
        .map(|extension| dir.join(format!("{stem}.{extension}")))
        .find(|path| path.metadata().is_ok_and(|meta| meta.len() > 0))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .build()
}

fn refused(service: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            format!("{service} answered {code}: {}", message_of(&body))
        }
        error => format!("{service} did not answer: {error}"),
    }
}

/// A colour card for scene `n`: a diagonal wash between two hues, written
/// as a PNG of the frame's size.
pub fn draft(out: &Path, width: u32, height: u32, n: usize) -> Result<(), String> {
    let hue = (n as f32 * 137.5) % 360.0;
    let (a, b) = (rgb(hue, 0.55, 0.35), rgb((hue + 40.0) % 360.0, 0.6, 0.6));
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    let span = (width + height) as f32;
    for y in 0..height {
        for x in 0..width {
            let t = (x + y) as f32 / span;
            for channel in 0..3 {
                pixels.push((a[channel] + (b[channel] - a[channel]) * t).round() as u8);
            }
        }
    }
    let file = std::fs::File::create(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&pixels))
        .map_err(|error| format!("{}: {error}", out.display()))
}

/// HSL to 0..=255 channels.
fn rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// One shot from a Replicate model, saved as `dir`/`stem`.<its extension>.
pub fn replicate(
    token: &str,
    model: &str,
    input: &str,
    prompt: &str,
    seconds: f64,
    aspect: &str,
    dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    if token.trim().is_empty() || model.trim().is_empty() {
        return Err("Replicate needs an API token and a model".to_owned());
    }
    let template: Value = serde_json::from_str(input)
        .map_err(|error| format!("the model's input is not JSON: {error}"))?;
    let body = json!({ "input": fill(&template, prompt, seconds, aspect) });
    let agent = agent();
    let auth = format!("Bearer {}", token.trim());
    let mut prediction: Value = agent
        .post(&format!(
            "https://api.replicate.com/v1/models/{}/predictions",
            model.trim()
        ))
        .set("authorization", &auth)
        .send_json(body)
        .map_err(|error| refused("Replicate", error))?
        .into_json()
        .map_err(|error| error.to_string())?;
    // A video takes minutes; ask every five seconds until it says.
    loop {
        match prediction["status"].as_str().unwrap_or_default() {
            "succeeded" => break,
            "failed" | "canceled" => {
                return Err(format!(
                    "Replicate could not make {stem}: {}",
                    prediction["error"].as_str().unwrap_or("no reason given")
                ));
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_secs(5));
        let url = prediction["urls"]["get"]
            .as_str()
            .ok_or("Replicate gave no address to follow the shot at")?
            .to_owned();
        prediction = agent
            .get(&url)
            .set("authorization", &auth)
            .call()
            .map_err(|error| refused("Replicate", error))?
            .into_json()
            .map_err(|error| error.to_string())?;
    }
    let output = &prediction["output"];
    let url = output
        .as_str()
        .or_else(|| output.get(0).and_then(Value::as_str))
        .ok_or("Replicate made the shot but gave no file")?;
    let extension = Path::new(url.split('?').next().unwrap_or(url))
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("mp4");
    let out = dir.join(format!("{stem}.{extension}"));
    fetch(&agent, url, &out)?;
    Ok(out)
}

/// The template with `{prompt}`, `{seconds}` and `{aspect}` filled in. A
/// string that is `{seconds}` alone becomes the number, which is what
/// most models take.
fn fill(value: &Value, prompt: &str, seconds: f64, aspect: &str) -> Value {
    match value {
        Value::String(text) if text == "{seconds}" => json!(seconds.round() as i64),
        Value::String(text) => Value::String(
            text.replace("{prompt}", prompt)
                .replace("{seconds}", &format!("{}", seconds.round() as i64))
                .replace("{aspect}", aspect),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| fill(v, prompt, seconds, aspect))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill(v, prompt, seconds, aspect)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The narration for one scene from ElevenLabs, as an MP3.
pub fn elevenlabs(key: &str, voice_id: &str, text: &str, out: &Path) -> Result<(), String> {
    if key.trim().is_empty() || voice_id.trim().is_empty() {
        return Err("ElevenLabs needs an API key and a voice id".to_owned());
    }
    let response = agent()
        .post(&format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}",
            voice_id.trim()
        ))
        .set("xi-api-key", key.trim())
        .set("accept", "audio/mpeg")
        .send_json(json!({"text": text, "model_id": "eleven_multilingual_v2"}))
        .map_err(|error| refused("ElevenLabs", error))?;
    save(response, out)
}

fn fetch(agent: &ureq::Agent, url: &str, out: &Path) -> Result<(), String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|error| refused("the download", error))?;
    save(response, out)
}

/// Writes a response's body, through a partial file so an interrupted
/// download is never taken for a finished one.
fn save(response: ureq::Response, out: &Path) -> Result<(), String> {
    let partial = out.with_extension("partial");
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    std::fs::write(&partial, bytes).map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, out).map_err(|error| format!("{}: {error}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_filled_with_types_kept() {
        let template = json!({"prompt": "{prompt}", "duration": "{seconds}", "aspect_ratio": "{aspect}",
                              "note": "{seconds}s", "fps": 24});
        let filled = fill(&template, "a dune", 5.4, "9:16");
        assert_eq!(
            filled,
            json!({"prompt": "a dune", "duration": 5, "aspect_ratio": "9:16",
                                  "note": "5s", "fps": 24})
        );
    }

    #[test]
    fn a_draft_card_is_a_png_of_the_frame() {
        let dir =
            std::env::temp_dir().join(format!("concat-generate-draft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("c01s01_1.png");
        draft(&out, 64, 36, 3).unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(&out).unwrap());
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (64, 36));
        assert_eq!(existing(&dir, "c01s01_1", &SHOT_EXTENSIONS), Some(out));
        assert_eq!(existing(&dir, "c01s01_2", &SHOT_EXTENSIONS), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
